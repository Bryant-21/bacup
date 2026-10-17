pub(crate) fn contains_ascii_case_insensitive(data: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && data.windows(needle.len()).any(|window| {
            window
                .iter()
                .zip(needle)
                .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
        })
}

pub(crate) fn contains_animation_class(data: &[u8]) -> bool {
    contains_ascii_case_insensitive(data, b"hkaSplineCompressedAnimation")
        || contains_ascii_case_insensitive(data, b"hkaInterleavedUncompressedAnimation")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_only_generated_precombine_geometry_at_mesh_root() {
        let temp = tempfile::tempdir().unwrap();
        let meshes = temp.path().join("MeShEs");
        std::fs::create_dir_all(meshes.join("pReCoMbInEd")).unwrap();
        std::fs::create_dir_all(meshes.join("Actors/Meshes/PreCombined")).unwrap();
        std::fs::write(meshes.join("other.hkx"), []).unwrap();
        let entries: Vec<_> = read_asset_dir(&meshes, &meshes)
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .any(|entry| entry.file_name() == "Actors" && entry_is_dir(entry))
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.file_name() == "other.hkx" && !entry_is_dir(entry))
        );
        assert_eq!(
            read_asset_dir(&meshes.join("Actors/Meshes"), &meshes)
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn finds_mixed_case_ascii_token() {
        assert!(contains_ascii_case_insensitive(
            b"prefix Fo76_WeaponAttackStart suffix",
            b"fo76_"
        ));
    }

    #[test]
    fn rejects_partial_and_empty_tokens() {
        assert!(!contains_ascii_case_insensitive(b"fo76", b"fo76_"));
        assert!(!contains_ascii_case_insensitive(b"anything", b""));
    }

    #[test]
    fn recognizes_both_mutable_animation_classes() {
        assert!(contains_animation_class(b"hkaSplineCompressedAnimation"));
        assert!(contains_animation_class(
            b"hkaInterleavedUncompressedAnimation"
        ));
        assert!(!contains_animation_class(b"hkbBehaviorGraph"));
    }
}
use std::fs::DirEntry;
use std::path::Path;

#[derive(Default)]
pub(crate) struct BehaviorNamesCache {
    entries: std::collections::HashMap<std::path::PathBuf, havok_native::behavior_clip_names::BehaviorNames>,
    pub hits: usize,
}

impl BehaviorNamesCache {
    pub fn get(&mut self, path: &Path) -> &havok_native::behavior_clip_names::BehaviorNames {
        let key = if cfg!(windows) {
            std::path::PathBuf::from(path.to_string_lossy().replace('\\', "/").to_lowercase())
        } else {
            path.to_path_buf()
        };
        match self.entries.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                self.hits += 1;
                entry.into_mut()
            }
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(havok_native::behavior_clip_names::read_behavior_names(path)),
        }
    }

    pub fn clips_in(&mut self, directory: &Path) -> std::collections::HashSet<String> {
        let mut names = std::collections::HashSet::new();
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("hkx")) {
                    names.extend(self.get(&path).clips.iter().cloned());
                }
            }
        }
        names
    }

    pub fn files(&self) -> usize {
        self.entries.len()
    }
}

pub(crate) fn read_asset_dir(
    dir: &Path,
    scan_root: &Path,
) -> std::io::Result<impl Iterator<Item = std::io::Result<DirEntry>>> {
    let mesh_root = dir == scan_root
        && dir
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("meshes"));
    Ok(std::fs::read_dir(dir)?.filter(move |entry| {
        // Engine-generated precombine geometry contains no animation assets.
        !mesh_root
            || !entry.as_ref().is_ok_and(|entry| {
                entry.file_name().eq_ignore_ascii_case("precombined") && entry_is_dir(entry)
            })
    }))
}

pub(crate) fn entry_is_dir(entry: &DirEntry) -> bool {
    entry
        .file_type()
        .map(|kind| {
            if kind.is_symlink() {
                entry.path().is_dir()
            } else {
                kind.is_dir()
            }
        })
        .unwrap_or_else(|_| entry.path().is_dir())
}
