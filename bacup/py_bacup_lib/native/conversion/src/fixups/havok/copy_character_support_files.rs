//! Copy creature support files (`*.ssf`, `bonelodsetting.txt`) from the
//! extracted source tree into the converted mod.
//!
//! The pre-native Python walker ported these alongside character-asset NIFs
//! (`_CHARACTER_ASSETS_EXTENSIONS = {".nif", ".txt", ".ssf"}`); the native
//! pipeline lost them — FO76 ships 217 `.ssf` under actors/ and we emitted
//! zero. `.ssf` carries bone-delta/morph data, `bonelodsetting.txt` bone LOD.
//! Working fan ports ship both.
//!
//! Every support file under a source creature dir is mirrored (subdirectories
//! created as needed) for each creature present in the output — variant
//! subtrees like `floater/overgrown/` included.
//!
//! # FixupReport mapping
//! `records_changed` = files copied.

use std::path::{Path, PathBuf};

use crate::fixups::{FixupError, FixupReport};

pub fn copy_character_support_files_in_mod_path(
    mod_path: &Path,
    source_extracted_dir: &Path,
) -> Result<FixupReport, FixupError> {
    let mut copied = 0u32;
    let Some(source_actors) = child_dir_ci(&source_extracted_dir.join("Meshes"), "actors") else {
        return Ok(FixupReport::empty());
    };
    for actors_root in actor_roots_for_mod_path(mod_path) {
        let Ok(entries) = std::fs::read_dir(&actors_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let out_creature = entry.path();
            if !out_creature.is_dir() {
                continue;
            }
            let Some(name) = out_creature.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(src_creature) = child_dir_ci(&source_actors, name) else {
                continue;
            };
            copied += copy_support_files(&src_creature, &out_creature);
        }
    }
    Ok(FixupReport {
        records_changed: copied,
        ..FixupReport::empty()
    })
}

fn actor_roots_for_mod_path(mod_path: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for meshes in [
        mod_path.join("data").join("Meshes"),
        mod_path.join("meshes"),
    ] {
        if let Some(actors) = child_dir_ci(&meshes, "actors") {
            roots.push(actors);
        }
    }
    roots
}

fn child_dir_ci(parent: &Path, name: &str) -> Option<PathBuf> {
    if !parent.is_dir() {
        return None;
    }
    let direct = parent.join(name);
    if direct.is_dir() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        {
            return Some(path);
        }
    }
    None
}

fn is_support_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".ssf") || lower == "bonelodsetting.txt"
}

/// Recursively mirror support files from `src_dir` into `out_dir`, reusing
/// existing output subdirectories case-insensitively and creating the rest.
fn copy_support_files(src_dir: &Path, out_dir: &Path) -> u32 {
    let mut copied = 0u32;
    let Ok(entries) = std::fs::read_dir(src_dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let src = entry.path();
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if src.is_dir() {
            let out_sub = child_dir_ci(out_dir, name).unwrap_or_else(|| out_dir.join(name));
            copied += copy_support_files(&src, &out_sub);
            continue;
        }
        if !is_support_file(name) {
            continue;
        }
        if out_dir.is_dir() && child_file_ci(out_dir, name).is_some() {
            continue;
        }
        if std::fs::create_dir_all(out_dir).is_err() {
            continue;
        }
        if std::fs::copy(&src, out_dir.join(name)).is_ok() {
            copied += 1;
        }
    }
    copied
}

fn child_file_ci(parent: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        {
            return Some(path);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn copies_ssf_and_bonelod_for_every_output_creature_including_variants() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("extracted");
        let mod_path = tmp.path().join("mod");

        let src_ca = source.join("Meshes/actors/snallygaster/characterassets");
        write(&src_ca.join("snallygaster.ssf"), b"ssf");
        write(&src_ca.join("bonelodsetting.txt"), b"lod");
        write(&src_ca.join("notes.txt"), b"junk");
        write(
            &source.join("Meshes/actors/floater/overgrown/overgrownfloater.ssf"),
            b"ssf",
        );
        write(
            &source.join("Meshes/actors/floater/characterassets/floater.ssf"),
            b"ssf",
        );
        write(
            &source.join("Meshes/actors/wendigo/characterassets/wendigo.ssf"),
            b"ssf",
        );

        let out_snally = mod_path.join("data/Meshes/actors/snallygaster/CharacterAssets");
        std::fs::create_dir_all(&out_snally).unwrap();
        let out_floater_ca = mod_path.join("data/Meshes/actors/floater/characterassets");
        std::fs::create_dir_all(&out_floater_ca).unwrap();
        // No floater/overgrown output dir — the variant ssf must still ship.
        // No wendigo output creature dir at all — wendigo must be skipped.

        let report = copy_character_support_files_in_mod_path(&mod_path, &source).unwrap();
        assert_eq!(report.records_changed, 4);
        assert!(out_snally.join("snallygaster.ssf").is_file());
        assert!(out_snally.join("bonelodsetting.txt").is_file());
        assert!(
            !out_snally.join("notes.txt").exists(),
            "arbitrary .txt files are not support files"
        );
        assert!(out_floater_ca.join("floater.ssf").is_file());
        assert!(
            mod_path
                .join("data/Meshes/actors/floater/overgrown/overgrownfloater.ssf")
                .is_file(),
            "variant subdirs are created so all variant support files ship"
        );
        assert!(
            !mod_path.join("data/Meshes/actors/wendigo").exists(),
            "creatures absent from the output stay absent"
        );
    }

    #[test]
    fn existing_output_files_are_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("extracted");
        let mod_path = tmp.path().join("mod");

        write(
            &source.join("Meshes/actors/mothman/characterassets/mothman.ssf"),
            b"source",
        );
        let out_ca = mod_path.join("data/Meshes/actors/mothman/characterassets");
        write(&out_ca.join("Mothman.ssf"), b"already-there");

        let report = copy_character_support_files_in_mod_path(&mod_path, &source).unwrap();
        assert_eq!(report.records_changed, 0);
        assert_eq!(
            std::fs::read(out_ca.join("Mothman.ssf")).unwrap(),
            b"already-there"
        );
    }

    #[test]
    fn missing_source_or_output_is_a_no_op() {
        let tmp = tempfile::tempdir().unwrap();
        let report = copy_character_support_files_in_mod_path(
            &tmp.path().join("mod"),
            &tmp.path().join("extracted"),
        )
        .unwrap();
        assert!(report.is_no_op());
    }
}
