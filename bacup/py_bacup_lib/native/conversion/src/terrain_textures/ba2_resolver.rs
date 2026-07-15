use bsarchive_native::python::{extract_one_impl, list_archive_files};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub struct Ba2Resolver {
    /// Sorted list of archive paths (sorted order is used to build index).
    archives: Vec<PathBuf>,
    /// normalized rel_path → index into `archives`
    index: OnceLock<Result<HashMap<String, usize>, String>>,
    /// Extracted source-game asset root, if available.
    source_extracted_dir: Option<PathBuf>,
}

impl Ba2Resolver {
    pub fn open(fo76_data_dir: &Path) -> Result<Self, String> {
        Self::open_with_extracted_dir(fo76_data_dir, None)
    }

    pub fn open_with_extracted_dir(
        fo76_data_dir: &Path,
        source_extracted_dir: Option<&Path>,
    ) -> Result<Self, String> {
        if !fo76_data_dir.is_dir() {
            return Err(format!(
                "fo76_data_dir is not a directory: {}",
                fo76_data_dir.display()
            ));
        }

        // Collect all *.ba2 files (case-insensitive extension match).
        let read_dir =
            fs::read_dir(fo76_data_dir).map_err(|e| format!("failed to read directory: {e}"))?;
        let mut archives: Vec<PathBuf> = read_dir
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_file() {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_ascii_lowercase());
                    if ext.as_deref() == Some("ba2") {
                        return Some(path);
                    }
                }
                None
            })
            .collect();

        // Sort for deterministic first-archive-wins behavior.
        archives.sort();

        let source_extracted_dir =
            source_extracted_dir.and_then(|path| path.is_dir().then(|| path.to_path_buf()));

        Ok(Self {
            archives,
            index: OnceLock::new(),
            source_extracted_dir,
        })
    }

    fn archive_index(&self) -> Result<&HashMap<String, usize>, String> {
        let result = self
            .index
            .get_or_init(|| build_archive_index(&self.archives));
        match result {
            Ok(index) => Ok(index),
            Err(error) => Err(error.clone()),
        }
    }

    fn extracted_path(&self, normalized: &str) -> Option<PathBuf> {
        let root = self.source_extracted_dir.as_ref()?;
        let path = root.join(normalized.replace('/', std::path::MAIN_SEPARATOR_STR));
        path.is_file().then_some(path)
    }

    pub fn find(&self, rel_path: &str) -> Option<Vec<u8>> {
        let key = normalize_rel_path(rel_path);
        if let Ok(strict_key) = normalize_rel_path_strict(rel_path) {
            if let Some(path) = self.extracted_path(&strict_key) {
                return fs::read(path).ok();
            }
        }
        let index = self.archive_index().ok()?;
        let &archive_idx = index.get(&key)?;
        let archive_path = &self.archives[archive_idx];
        extract_one_impl(archive_path, &key).ok()
    }

    pub fn extract_to(&self, rel_path: &str, dest_root: &Path) -> Result<PathBuf, String> {
        let normalized = normalize_rel_path_strict(rel_path)?;
        if let Some(path) = self.extracted_path(&normalized) {
            return Ok(path);
        }
        let bytes = self
            .find(rel_path)
            .ok_or_else(|| format!("not found in FO76 archives or extracted assets: {rel_path}"))?;
        let out_path = dest_root.join(normalized.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create dirs for {}: {e}", out_path.display()))?;
        }
        fs::write(&out_path, &bytes)
            .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
        Ok(out_path)
    }
}

fn build_archive_index(archives: &[PathBuf]) -> Result<HashMap<String, usize>, String> {
    // Build index: first-archive-wins on collision.
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, archive_path) in archives.iter().enumerate() {
        let files = list_archive_files(archive_path)
            .map_err(|e| format!("failed to list {}: {e}", archive_path.display()))?;
        for rel in files {
            let key = normalize_rel_path(&rel);
            index.entry(key).or_insert(i);
        }
    }

    Ok(index)
}

pub(crate) fn normalize_rel_path(value: &str) -> String {
    value
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0')
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
        .to_ascii_lowercase()
}

/// Like `normalize_rel_path`, but rejects absolute paths and any `..` segment.
/// Used by write-paths (e.g. `extract_to`) as defense-in-depth against
/// path-traversal when constructing on-disk destinations.
pub(crate) fn normalize_rel_path_strict(value: &str) -> Result<String, String> {
    let normalized = normalize_rel_path(value);
    if normalized.starts_with('/') || normalized.split('/').any(|seg| seg == "..") {
        return Err(format!(
            "invalid rel_path (must be relative, no .. segments): {value}"
        ));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_case_and_slashes() {
        assert_eq!(
            normalize_rel_path("Textures\\Shared\\X.DDS"),
            "textures/shared/x.dds"
        );
        assert_eq!(normalize_rel_path("./Foo/Bar.dds"), "foo/bar.dds");
    }

    #[test]
    fn strict_rejects_parent_traversal_and_absolute() {
        assert!(normalize_rel_path_strict("../etc/passwd").is_err());
        assert!(normalize_rel_path_strict("foo/../../bar.dds").is_err());
        assert!(normalize_rel_path_strict("/abs/path.dds").is_err());
        assert!(normalize_rel_path_strict("\\abs\\path.dds").is_err());
        assert_eq!(
            normalize_rel_path_strict("Textures/Shared/X.DDS").unwrap(),
            "textures/shared/x.dds"
        );
    }

    #[test]
    fn open_errors_on_non_directory() {
        let result = Ba2Resolver::open(std::path::Path::new("/definitely/not/a/dir"));
        assert!(result.is_err());
    }

    #[test]
    fn extracted_assets_are_preferred_without_ba2_index() {
        let data_dir = tempfile::tempdir().unwrap();
        let extracted_dir = tempfile::tempdir().unwrap();
        let source_path = extracted_dir
            .path()
            .join("textures")
            .join("shared")
            .join("x.dds");
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        fs::write(&source_path, b"from extracted").unwrap();

        let resolver =
            Ba2Resolver::open_with_extracted_dir(data_dir.path(), Some(extracted_dir.path()))
                .expect("open");
        assert_eq!(
            resolver.find("Textures\\Shared\\X.DDS").unwrap(),
            b"from extracted"
        );
        assert!(resolver.index.get().is_none());

        let dest = tempfile::tempdir().unwrap();
        let resolved = resolver
            .extract_to("textures/shared/x.dds", dest.path())
            .expect("resolve");
        assert_eq!(resolved, source_path);
        assert!(resolver.index.get().is_none());
        assert!(
            !dest
                .path()
                .join("textures")
                .join("shared")
                .join("x.dds")
                .exists()
        );
    }

    #[test]
    fn open_finds_known_fo76_texture_when_install_available() {
        let fo76_data = std::env::var("FO76_DATA_DIR").unwrap_or_default();
        if fo76_data.is_empty() {
            return;
        } // skip on CI
        let resolver = Ba2Resolver::open(std::path::Path::new(&fo76_data)).expect("open");
        let bytes = resolver
            .find("textures/shared/cubemaps/mipblur_defaultoutside1.dds")
            .or_else(|| resolver.find("textures/shared/cubemaps/eyecubemap.dds"));
        assert!(bytes.is_some(), "expected to resolve a known FO76 texture");
    }
}
