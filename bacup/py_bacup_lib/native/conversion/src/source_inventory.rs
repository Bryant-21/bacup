use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

struct Directory {
    entries: Vec<(OsString, fs::FileType)>,
    by_name: HashMap<OsString, usize>,
    snapshot_compatible: bool,
}

#[derive(Default)]
struct Inventory {
    directories: HashMap<PathBuf, Option<Arc<Directory>>>,
    probes: HashMap<PathBuf, (bool, bool)>,
    directory_requests: usize,
    directory_cache_hits: usize,
    directory_reads: usize,
    snapshot_requests: usize,
    snapshot_hits: usize,
    snapshot_fallbacks: usize,
}

impl Inventory {
    fn directory(&mut self, path: &Path) -> Option<Arc<Directory>> {
        self.directory_requests += 1;
        if self.directories.contains_key(path) {
            self.directory_cache_hits += 1;
        } else {
            self.directory_reads += 1;
        }
        self.directories
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let read_dir = fs::read_dir(path).ok()?;
                let mut entries = Vec::new();
                let mut snapshot_compatible = true;
                for entry in read_dir {
                    let Ok(entry) = entry else {
                        snapshot_compatible = false;
                        continue;
                    };
                    let Ok(file_type) = entry.file_type() else {
                        snapshot_compatible = false;
                        continue;
                    };
                    entries.push((entry.file_name(), file_type));
                }
                let by_name = entries
                    .iter()
                    .enumerate()
                    .map(|(index, (name, _))| (name.clone(), index))
                    .collect();
                Some(Arc::new(Directory {
                    entries,
                    by_name,
                    snapshot_compatible,
                }))
            })
            .clone()
    }

    fn exists(&mut self, path: &Path, allow_directories: bool) -> bool {
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            if let Some(directory) = self.directory(parent) {
                if let Some(&index) = directory.by_name.get(name) {
                    let kind = directory.entries[index].1;
                    if !kind.is_symlink() {
                        return kind.is_file() || (allow_directories && kind.is_dir());
                    }
                }
            }
        }
        // Let the filesystem decide case aliases, links and unusual path forms.
        let (file, directory) = *self.probes.entry(path.to_path_buf()).or_insert_with(|| {
            fs::metadata(path)
                .map(|metadata| (metadata.is_file(), metadata.is_dir()))
                .unwrap_or_default()
        });
        file || (allow_directories && directory)
    }

    fn files(
        &mut self,
        root: &Path,
        suffixes: &[String],
        follow_file_links: bool,
        recursive: bool,
        include_non_files: bool,
    ) -> Vec<String> {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(path) = stack.pop() {
            let Some(directory) = self.directory(&path) else {
                continue;
            };
            let mut children = Vec::new();
            for (name, kind) in &directory.entries {
                let child = path.join(name);
                if kind.is_dir() {
                    if recursive {
                        children.push(child);
                    }
                } else if (kind.is_file()
                    || (follow_file_links && kind.is_symlink() && child.is_file())
                    || (include_non_files && !child.is_dir()))
                    && suffixes
                        .iter()
                        .any(|suffix| name.to_string_lossy().to_lowercase().ends_with(suffix))
                {
                    files.push(child.to_string_lossy().into_owned());
                }
            }
            stack.extend(children.into_iter().rev());
        }
        files
    }

    fn files_if_compatible(&mut self, root: &Path, suffixes: &[&str]) -> Option<Vec<PathBuf>> {
        self.files_if_compatible_from(root, root, suffixes)
    }

    fn files_if_compatible_from(
        &mut self,
        scan_root: &Path,
        emitted_root: &Path,
        suffixes: &[&str],
    ) -> Option<Vec<PathBuf>> {
        self.snapshot_requests += 1;
        let mut files = Vec::new();
        let result = self
            .collect_files_if_compatible(scan_root, emitted_root, suffixes, &mut files)
            .map(|()| files);
        if result.is_some() {
            self.snapshot_hits += 1;
        } else {
            self.snapshot_fallbacks += 1;
        }
        result
    }

    fn collect_files_if_compatible(
        &mut self,
        scan_path: &Path,
        emitted_path: &Path,
        suffixes: &[&str],
        files: &mut Vec<PathBuf>,
    ) -> Option<()> {
        let directory = self.directory(scan_path)?;
        if !directory.snapshot_compatible {
            return None;
        }
        for (name, kind) in &directory.entries {
            let scan_child = scan_path.join(name);
            let emitted_child = emitted_path.join(name);
            if kind.is_dir() {
                self.collect_files_if_compatible(&scan_child, &emitted_child, suffixes, files)?;
                continue;
            }
            if kind.is_symlink() && scan_child.is_dir() {
                return None;
            }
            if scan_child
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    suffixes.iter().any(|suffix| {
                        extension.eq_ignore_ascii_case(suffix.trim_start_matches('.'))
                    })
                })
            {
                files.push(emitted_child);
            }
        }
        Some(())
    }
}

#[derive(Default)]
pub struct SourceAssetInventory {
    inner: Mutex<Inventory>,
}

impl SourceAssetInventory {
    pub(crate) fn files_if_compatible(
        &self,
        root: &Path,
        suffixes: &[&str],
    ) -> Option<Vec<PathBuf>> {
        let scan_root = self.filesystem_spelling(root);
        self.inner
            .lock()
            .unwrap()
            .files_if_compatible_from(&scan_root, root, suffixes)
    }

    pub(crate) fn filesystem_spelling(&self, path: &Path) -> PathBuf {
        if !path.is_dir() {
            return path.to_path_buf();
        }
        let (Some(parent), Some(requested_name)) = (path.parent(), path.file_name()) else {
            return path.to_path_buf();
        };
        let Some(directory) = self.inner.lock().unwrap().directory(parent) else {
            return path.to_path_buf();
        };
        if directory.by_name.contains_key(requested_name) {
            return path.to_path_buf();
        }
        #[cfg(windows)]
        if let Some((actual_name, _)) = directory
            .entries
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(requested_name))
        {
            return parent.join(actual_name);
        }
        path.to_path_buf()
    }
}

#[pyclass(name = "SourceAssetInventory")]
#[derive(Default)]
pub struct PySourceAssetInventory {
    inner: Arc<SourceAssetInventory>,
}

#[pymethods]
impl PySourceAssetInventory {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    #[pyo3(signature = (path, allow_directories=false))]
    fn exists(&self, path: &str, allow_directories: bool) -> bool {
        self.inner
            .inner
            .lock()
            .unwrap()
            .exists(Path::new(path), allow_directories)
    }

    #[pyo3(signature = (root, suffixes, follow_file_links=false, recursive=true, include_non_files=false))]
    fn files(
        &self,
        py: Python<'_>,
        root: &str,
        suffixes: Vec<String>,
        follow_file_links: bool,
        recursive: bool,
        include_non_files: bool,
    ) -> Vec<String> {
        py.detach(|| {
            self.inner.inner.lock().unwrap().files(
                Path::new(root),
                &suffixes,
                follow_file_links,
                recursive,
                include_non_files,
            )
        })
    }

    fn attach_run(&self, run_id: u64) -> PyResult<()> {
        crate::run::with_run(run_id, |run| {
            run.source_asset_inventory = Some(Arc::clone(&self.inner));
            Ok::<_, crate::run::RunError>(())
        })
        .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }

    fn stats(&self) -> HashMap<String, usize> {
        let inner = self.inner.inner.lock().unwrap();
        HashMap::from([
            ("directories".into(), inner.directories.len()),
            (
                "entries".into(),
                inner
                    .directories
                    .values()
                    .flatten()
                    .map(|dir| dir.entries.len())
                    .sum(),
            ),
            ("fallback_probes".into(), inner.probes.len()),
            ("directory_requests".into(), inner.directory_requests),
            ("directory_cache_hits".into(), inner.directory_cache_hits),
            ("directory_reads".into(), inner.directory_reads),
            ("snapshot_requests".into(), inner.snapshot_requests),
            ("snapshot_hits".into(), inner.snapshot_hits),
            ("snapshot_fallbacks".into(), inner.snapshot_fallbacks),
            (
                "retained_path_bytes_lower_bound".into(),
                inner
                    .directories
                    .iter()
                    .map(|(path, directory)| {
                        path.to_string_lossy().len()
                            + directory
                                .iter()
                                .flat_map(|directory| &directory.entries)
                                .map(|(name, _)| name.to_string_lossy().len())
                                .sum::<usize>()
                    })
                    .sum(),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_preserves_files_directories_and_fresh_scopes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let nested = root.join("nested");
        fs::create_dir(&nested).unwrap();
        let mesh = nested.join("Mesh.NIF");
        fs::write(&mesh, b"first").unwrap();
        fs::write(root.join("ignore.txt"), b"other").unwrap();
        let mut inventory = Inventory::default();
        assert!(!inventory.exists(&nested, false));
        assert!(inventory.exists(&nested, true));
        assert!(inventory.exists(&mesh, false));
        assert_eq!(
            inventory.files(root, &[".nif".into()], false, true, false),
            vec![mesh.to_string_lossy().into_owned()]
        );
        assert!(
            inventory
                .files(root, &[".nif".into()], false, false, false)
                .is_empty()
        );
        let missing = root.join("later.nif");
        assert!(!inventory.exists(&missing, false));
        fs::write(&missing, b"new").unwrap();
        fs::write(&mesh, b"replaced").unwrap();
        let mut fresh = Inventory::default();
        assert!(fresh.exists(&missing, false));
        assert_eq!(
            fresh
                .files(root, &[".nif".into()], false, true, false)
                .len(),
            2
        );
        assert_eq!(fs::read(&mesh).unwrap(), b"replaced");
    }

    #[test]
    fn compatible_snapshot_reuses_directory_reads_and_matches_recursive_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Textures");
        fs::create_dir_all(root.join("nested/deeper")).unwrap();
        fs::write(root.join("root.DDS"), b"root").unwrap();
        fs::write(root.join("nested/deeper/child.dds"), b"child").unwrap();
        fs::write(root.join("nested/ignore.txt"), b"ignore").unwrap();

        let mut inventory = Inventory::default();
        let first = inventory.files_if_compatible(&root, &[".dds"]).unwrap();
        let reads = inventory.directory_reads;
        let mut second = inventory.files_if_compatible(&root, &[".dds"]).unwrap();
        assert_eq!(inventory.directory_reads, reads);
        assert!(inventory.directory_cache_hits >= 3);
        assert_eq!(inventory.snapshot_requests, 2);
        assert_eq!(inventory.snapshot_hits, 2);
        assert_eq!(inventory.snapshot_fallbacks, 0);

        let mut first = first;
        first.sort();
        second.sort();
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
    }

    #[test]
    fn cached_directory_failure_requests_direct_walker_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("created-later");
        let mut inventory = Inventory::default();
        assert_eq!(inventory.files_if_compatible(&root, &[".dds"]), None);

        fs::create_dir(&root).unwrap();
        fs::write(root.join("late.dds"), b"late").unwrap();
        assert_eq!(inventory.files_if_compatible(&root, &[".dds"]), None);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert_eq!(inventory.snapshot_requests, 2);
        assert_eq!(inventory.snapshot_hits, 0);
        assert_eq!(inventory.snapshot_fallbacks, 2);
    }

    #[cfg(windows)]
    #[test]
    fn filesystem_spelling_reuses_actual_windows_directory_case() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("meshes");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("asset.nif"), b"mesh").unwrap();
        let uppercase_alias = temp.path().join("Meshes");
        let inventory = SourceAssetInventory::default();

        assert_eq!(inventory.filesystem_spelling(&uppercase_alias), root);
        let uppercase_files = inventory
            .files_if_compatible(&uppercase_alias, &[".nif"])
            .unwrap();
        let reads = inventory.inner.lock().unwrap().directory_reads;
        let actual_files = inventory.files_if_compatible(&root, &[".nif"]).unwrap();
        assert_eq!(inventory.inner.lock().unwrap().directory_reads, reads);
        assert_eq!(uppercase_files, vec![uppercase_alias.join("asset.nif")]);
        assert_eq!(actual_files, vec![root.join("asset.nif")]);
    }

    #[test]
    fn native_probes_match_filesystem_case_and_missing_paths() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Mixed.NIF");
        fs::write(&path, b"mesh").unwrap();
        let mut inventory = Inventory::default();
        for candidate in [
            path,
            temp.path().join("mixed.nif"),
            temp.path().join("missing.nif"),
        ] {
            assert_eq!(inventory.exists(&candidate, false), candidate.is_file());
            assert_eq!(
                inventory.exists(&candidate, true),
                candidate.is_file() || candidate.is_dir()
            );
        }
    }
}
