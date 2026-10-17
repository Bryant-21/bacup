use crate::target_assets::TargetAssetStore;
use papyrus_core::headers;
use pyo3::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const ANCHORS: [&str; 3] = ["scriptobject", "form", "objectreference"];

fn pex_paths(root: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut pending = vec![root.to_owned()];
    let mut paths = Vec::new();
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if recursive && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path.clone());
            }
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pex"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort_by_cached_key(|path| path.to_string_lossy().to_lowercase());
    paths
}

pub fn has_anchors(paths: &[String]) -> bool {
    let names: HashSet<_> = paths
        .iter()
        .filter_map(|path| Path::new(path).file_stem())
        .map(|name| name.to_string_lossy().to_lowercase())
        .collect();
    ANCHORS.iter().all(|anchor| names.contains(*anchor))
}

pub fn find_pex_corpus(roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .find(|root| {
            has_anchors(
                &pex_paths(root, false)
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
            )
        })
        .cloned()
}

pub fn materialize_pex_corpus(store: &TargetAssetStore) -> Result<Option<PathBuf>, String> {
    let assets = store.list_assets("scripts/", ".pex");
    if !has_anchors(&assets) {
        return Ok(None);
    }
    let Some(sample) = store.materialize(&assets[0])? else {
        return Ok(None);
    };
    store.materialize_many(&assets)?;
    let mut root = sample;
    for _ in Path::new(&assets[0]).components().skip(1) {
        root.pop();
    }
    Ok(Some(root))
}

#[derive(Default, Serialize)]
pub struct HeaderPreparation {
    pub import_roots: Vec<PathBuf>,
    pub flags_file: Option<PathBuf>,
    pub logs: Vec<(String, String)>,
}

pub fn ensure_headers(pex_root: &Path, out_root: &Path) -> Result<Vec<(String, String)>, String> {
    fs::create_dir_all(out_root).map_err(|e| e.to_string())?;
    let paths = pex_paths(pex_root, true);
    let mut digest = Sha256::new();
    digest.update(headers::EMITTER_SOURCE);
    digest.update(headers::DEFAULT_MANIFEST);
    digest.update(headers::FLAGS);
    let stamp = format!("{:x}", digest.finalize());
    let mut fingerprint = HashMap::new();
    for path in &paths {
        let Ok(stat) = path.metadata() else { continue };
        let Ok(modified) = stat.modified().and_then(|time| {
            time.duration_since(UNIX_EPOCH)
                .map_err(std::io::Error::other)
        }) else {
            continue;
        };
        fingerprint.insert(
            path.file_name().unwrap().to_string_lossy().to_lowercase(),
            [stat.len(), modified.as_secs()],
        );
    }
    let expected = json!({"emitter": &stamp[..16], "sources": fingerprint});
    let manifest_path = out_root.join(".header_cache.json");
    let cached: Option<Value> = fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if cached.as_ref() == Some(&expected) {
        return Ok(vec![(
            "INFO".into(),
            format!("[Scripts] type universe up to date ({} pex)", paths.len()),
        )]);
    }
    fs::write(out_root.join(headers::FLAGS_FILE_NAME), headers::FLAGS)
        .map_err(|e| e.to_string())?;
    let api_manifest =
        serde_json::from_str(headers::DEFAULT_MANIFEST).map_err(|e| e.to_string())?;
    let mut written = 0;
    let mut failed = 0;
    let mut logs = Vec::new();
    for path in &paths {
        let result = (|| -> Result<(), String> {
            for (name, text) in headers::emit_headers_for_file(path, &api_manifest)? {
                let dest = name
                    .split(':')
                    .fold(out_root.to_owned(), |path, part| path.join(part))
                    .with_extension("psc");
                fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
                #[cfg(windows)]
                let text = text.replace('\n', "\r\n");
                fs::write(dest, text).map_err(|e| e.to_string())?;
                written += 1;
            }
            Ok(())
        })();
        if let Err(error) = result {
            failed += 1;
            if failed <= 10 {
                logs.push((
                    "WARN".into(),
                    format!(
                        "[Scripts] could not read {}: {error}",
                        path.file_name().unwrap().to_string_lossy()
                    ),
                ));
            }
        }
    }
    fs::write(manifest_path, expected.to_string()).map_err(|e| e.to_string())?;
    logs.push((
        "INFO".into(),
        format!(
            "[Scripts] built type universe from {} pex: {written} header(s), {failed} unreadable",
            paths.len()
        ),
    ));
    Ok(logs)
}

pub fn prepare_headers(
    target_extracted_dir: Option<&Path>,
    target_data_dir: Option<&Path>,
    output_root: &Path,
    bundled_root: Option<&Path>,
    store: Option<&TargetAssetStore>,
    target_game: &str,
) -> Result<HeaderPreparation, String> {
    let mut result = HeaderPreparation::default();
    let roots: Vec<_> = [target_extracted_dir, target_data_dir]
        .into_iter()
        .flatten()
        .map(|path| path.join("scripts"))
        .collect();
    let mut corpus = find_pex_corpus(&roots);
    if corpus.is_none() {
        if let Some(store) = store {
            corpus = materialize_pex_corpus(store)?;
            if let Some(path) = &corpus {
                result.logs.push(("INFO".into(), format!("[Scripts] building the Papyrus type universe from the game's archives ({})", path.display())));
            }
        }
    }
    if let Some(corpus) = corpus {
        let root = target_extracted_dir
            .unwrap_or(output_root)
            .join("Scripts/GeneratedHeaders");
        result.logs.extend(ensure_headers(&corpus, &root)?);
        result.import_roots.push(root);
    } else if let Some(root) = bundled_root {
        result.logs.push((
            "INFO".into(),
            format!(
                "[Scripts] using the bundled {target_game} Papyrus type universe ({})",
                root.display()
            ),
        ));
        result.import_roots.push(root.to_owned());
    }
    if let Some(root) = bundled_root {
        if !result.import_roots.is_empty()
            && !result.import_roots.iter().any(|existing| {
                existing.to_string_lossy().to_lowercase() == root.to_string_lossy().to_lowercase()
            })
        {
            result.import_roots.push(root.to_owned());
        }
    }
    result.flags_file = result
        .import_roots
        .first()
        .map(|root| root.join(headers::FLAGS_FILE_NAME))
        .filter(|path| path.is_file());
    Ok(result)
}

#[pyfunction(name = "script_headers_find_corpus")]
fn find_corpus_py(py: Python<'_>, roots: Vec<String>) -> Option<String> {
    py.detach(|| {
        find_pex_corpus(&roots.into_iter().map(PathBuf::from).collect::<Vec<_>>())
            .map(|path| path.to_string_lossy().into_owned())
    })
}

#[pyfunction(name = "script_headers_has_anchors")]
fn has_anchors_py(py: Python<'_>, paths: Vec<String>) -> bool {
    py.detach(|| has_anchors(&paths))
}

#[pyfunction(name = "script_headers_ensure")]
fn ensure_headers_py(
    py: Python<'_>,
    source: String,
    output: String,
) -> PyResult<Vec<(String, String)>> {
    py.detach(|| ensure_headers(Path::new(&source), Path::new(&output)))
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(find_corpus_py, m)?)?;
    m.add_function(wrap_pyfunction!(has_anchors_py, m)?)?;
    m.add_function(wrap_pyfunction!(ensure_headers_py, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_requires_all_anchors_and_uses_first_viable_root() {
        let temp = tempfile::tempdir().unwrap();
        let partial = temp.path().join("partial");
        let full = temp.path().join("full");
        fs::create_dir_all(&partial).unwrap();
        fs::create_dir_all(&full).unwrap();
        fs::write(partial.join("Form.pex"), b"pex").unwrap();
        for name in ANCHORS {
            fs::write(full.join(format!("{name}.pex")), b"pex").unwrap();
        }
        assert_eq!(find_pex_corpus(&[partial, full.clone()]), Some(full));
    }

    #[test]
    fn unreadable_pex_is_reported_and_fingerprint_invalidates_on_change() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("scripts");
        let output = temp.path().join("headers");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("broken.pex"), b"broken").unwrap();
        let initial = ensure_headers(&source, &output).unwrap();
        assert!(initial.iter().any(|(level, _)| level == "WARN"));
        assert!(
            ensure_headers(&source, &output).unwrap()[0]
                .1
                .contains("up to date")
        );
        fs::write(source.join("broken.pex"), b"changed and still broken").unwrap();
        assert!(
            ensure_headers(&source, &output)
                .unwrap()
                .iter()
                .any(|(level, _)| level == "WARN")
        );
    }
}
