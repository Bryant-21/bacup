//! Data-relative asset lookup across loose roots, then BA2/BSA archives.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bsarchive_native::python::ArchiveReader;
use rustc_hash::FxHashMap;

use crate::error::{PrevisError, Result};

pub struct AssetResolver {
    loose_roots: Vec<PathBuf>,
    archives: Vec<Mutex<ArchiveReader>>,
    archive_index: FxHashMap<String, (usize, String)>,
}

pub fn normalize(relative: &str) -> String {
    relative.replace('/', "\\").trim_start_matches('\\').to_ascii_lowercase()
}

/// Model paths from records are relative to `meshes\`; reject escapes.
pub fn mesh_path(model: &str) -> Result<String> {
    let normalized = model.replace('/', "\\");
    let parts: Vec<&str> = normalized.split('\\').filter(|p| !p.is_empty()).collect();
    if normalized.contains(':') || normalized.starts_with('\\') || parts.iter().any(|p| *p == "..") {
        return Err(PrevisError::invalid(format!("unsafe model path: {model}")));
    }
    if parts.first().is_some_and(|p| p.eq_ignore_ascii_case("meshes")) {
        Ok(parts.join("\\"))
    } else {
        Ok(format!("meshes\\{}", parts.join("\\")))
    }
}

impl AssetResolver {
    pub fn new(loose_roots: Vec<PathBuf>, archive_paths: &[PathBuf]) -> Result<Self> {
        let mut archives = Vec::with_capacity(archive_paths.len());
        let mut archive_index = FxHashMap::default();
        for (index, path) in archive_paths.iter().enumerate() {
            let reader = ArchiveReader::open(path)
                .map_err(|e| PrevisError::invalid(format!("{}: {e}", path.display())))?;
            for name in reader.list_files() {
                // First archive in the list wins, matching the caller's priority order.
                archive_index.entry(normalize(&name)).or_insert((index, name));
            }
            archives.push(Mutex::new(reader));
        }
        Ok(Self {
            loose_roots,
            archives,
            archive_index,
        })
    }

    pub fn loose_path(&self, relative: &str) -> Option<PathBuf> {
        let relative = normalize(relative);
        self.loose_roots.iter().find_map(|root| {
            let candidate = root.join(Path::new(&relative.replace('\\', std::path::MAIN_SEPARATOR_STR)));
            candidate.is_file().then_some(candidate)
        })
    }

    /// Returns the asset bytes, or `None` when no root or archive has it.
    pub fn read(&self, relative: &str) -> Result<Option<Vec<u8>>> {
        if let Some(path) = self.loose_path(relative) {
            return std::fs::read(&path)
                .map(Some)
                .map_err(|e| PrevisError::io(path.display().to_string(), e));
        }
        let Some((index, name)) = self.archive_index.get(&normalize(relative)) else {
            return Ok(None);
        };
        let reader = self.archives[*index].lock().unwrap_or_else(|p| p.into_inner());
        reader
            .read_file(name)
            .map(Some)
            .map_err(|e| PrevisError::invalid(format!("{relative}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_paths_gain_the_meshes_prefix_once_and_reject_escapes() {
        {
            assert_eq!(mesh_path("Architecture\\Wall.nif").unwrap(), "meshes\\Architecture\\Wall.nif");
            assert_eq!(mesh_path("Meshes/Architecture/Wall.nif").unwrap(), "Meshes\\Architecture\\Wall.nif");
        }
        {
            assert!(mesh_path("..\\secret.nif").is_err());
            assert!(mesh_path("C:\\x.nif").is_err());
        }
    }
}
