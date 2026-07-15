//! Rename converted asset files whose stem ends in whitespace and rewrite the
//! matching `*project.hkx` string references.
//!
//! FO76 ships `Actors\Floater\Characters\floatercharacter .hkx` (trailing space
//! in the stem) and its project references `Characters\FloaterCharacter .hkx`.
//! FO4's resource lookup cannot resolve the spaced name — the working fan port
//! renames both — so the actor loads no character and T-poses.
//!
//! # FixupReport mapping
//! `records_changed` = files renamed + project files patched.

use std::path::{Path, PathBuf};

use havok_native::hkx::types::HkxValue;
use havok_native::hkx::read_packfile;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

pub struct NormalizeSpacedAssetNamesFixup;

impl Fixup for NormalizeSpacedAssetNamesFixup {
    fn name(&self) -> &'static str {
        "normalize_spaced_asset_names"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.animations
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.mod_path.is_some()
    }

    fn run_with_session(
        &self,
        _session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mod_path = match config.mod_path.as_deref() {
            Some(path) => path,
            None => return Ok(FixupReport::empty()),
        };
        normalize_spaced_asset_names_in_mod_path(mod_path)
    }
}

pub fn normalize_spaced_asset_names_in_mod_path(mod_path: &Path) -> Result<FixupReport, FixupError> {
    let mut changed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        changed += rename_spaced_files(&meshes_root);
        changed += patch_project_references(&meshes_root);
    }
    Ok(FixupReport {
        records_changed: changed,
        ..FixupReport::empty()
    })
}

fn mesh_roots_for_mod_path(mod_path: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let data_meshes = mod_path.join("data").join("Meshes");
    if data_meshes.is_dir() {
        roots.push(data_meshes);
    }
    let legacy_meshes = mod_path.join("meshes");
    if legacy_meshes.is_dir() {
        roots.push(legacy_meshes);
    }
    roots
}

/// `"floatercharacter .hkx"` → `Some("floatercharacter.hkx")`; `None` when the
/// stem carries no trailing whitespace (or trimming would empty it).
///
/// Havok files only: spaced-stem NIFs (FO76 ships a dozen) are referenced by
/// record MODL paths with the same spaced string, so renaming them would break
/// the record→mesh lookup instead of fixing one.
fn trimmed_spaced_file_name(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if !ext.eq_ignore_ascii_case("hkx") {
        return None;
    }
    let trimmed = stem.trim_end();
    (trimmed != stem && !trimmed.is_empty()).then(|| format!("{trimmed}.{ext}"))
}

/// Normalize the final path component of an hkx string reference.
fn normalized_reference(value: &str) -> Option<String> {
    let split = value.rfind(['\\', '/']).map(|i| i + 1).unwrap_or(0);
    let fixed = trimmed_spaced_file_name(&value[split..])?;
    Some(format!("{}{}", &value[..split], fixed))
}

fn rename_spaced_files(dir: &Path) -> u32 {
    let mut renamed = 0u32;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            renamed += rename_spaced_files(&path);
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(fixed) = trimmed_spaced_file_name(name) {
            let target = path.with_file_name(&fixed);
            if !target.exists() && std::fs::rename(&path, &target).is_ok() {
                renamed += 1;
                if let Some((stem, _)) = fixed.rsplit_once('.') {
                    if strip_internal_spaced_name(&target, stem).unwrap_or(false) {
                        renamed += 1;
                    }
                }
            }
        }
    }
    renamed
}

/// The FO76 typo also lives INSIDE the renamed file: hkbCharacterData `name`
/// is `"FloaterCharacter "` with the same trailing space. Strip any internal
/// string that trims to the renamed file's stem.
fn strip_internal_spaced_name(
    path: &Path,
    trimmed_stem: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let mut hkx = read_packfile(&data)?;
    let mut modified = false;
    for obj in hkx.objects_mut() {
        for member in &mut obj.members {
            if strip_spaced_stem_value(&mut member.value, trimmed_stem) {
                modified = true;
            }
        }
    }
    if modified {
        std::fs::write(path, hkx.save())?;
    }
    Ok(modified)
}

fn strip_spaced_stem_value(value: &mut HkxValue, trimmed_stem: &str) -> bool {
    match value {
        HkxValue::String { value, .. } => {
            let trimmed = value.trim_end();
            if trimmed != value && trimmed.eq_ignore_ascii_case(trimmed_stem) {
                *value = trimmed.to_string();
                true
            } else {
                false
            }
        }
        HkxValue::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= strip_spaced_stem_value(item, trimmed_stem);
            }
            changed
        }
        HkxValue::Object(members) => {
            let mut changed = false;
            for member in members {
                changed |= strip_spaced_stem_value(&mut member.value, trimmed_stem);
            }
            changed
        }
        _ => false,
    }
}

/// Rewrite spaced file references inside every `*project.hkx` under `dir`.
fn patch_project_references(dir: &Path) -> u32 {
    let mut patched = 0u32;
    walk_project_hkx(dir, &mut |path| {
        if patch_one_project(path).unwrap_or(false) {
            patched += 1;
        }
    });
    patched
}

fn walk_project_hkx(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_project_hkx(&path, f);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.to_ascii_lowercase().ends_with("project.hkx"))
        {
            f(&path);
        }
    }
}

fn patch_one_project(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let mut hkx = read_packfile(&data)?;
    let mut modified = false;
    for obj in hkx.objects_mut() {
        for member in &mut obj.members {
            if patch_value(&mut member.value) {
                modified = true;
            }
        }
    }
    if modified {
        std::fs::write(path, hkx.save())?;
    }
    Ok(modified)
}

fn patch_value(value: &mut HkxValue) -> bool {
    match value {
        HkxValue::String { value, .. } => {
            if let Some(fixed) = normalized_reference(value) {
                *value = fixed;
                true
            } else {
                false
            }
        }
        HkxValue::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= patch_value(item);
            }
            changed
        }
        HkxValue::Object(members) => {
            let mut changed = false;
            for member in members {
                changed |= patch_value(&mut member.value);
            }
            changed
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_spaced_stems_only() {
        assert_eq!(
            trimmed_spaced_file_name("floatercharacter .hkx").as_deref(),
            Some("floatercharacter.hkx")
        );
        assert_eq!(
            trimmed_spaced_file_name("multi  .hkx").as_deref(),
            Some("multi.hkx")
        );
        assert_eq!(trimmed_spaced_file_name("clean.hkx"), None);
        assert_eq!(trimmed_spaced_file_name("noext"), None);
        assert_eq!(trimmed_spaced_file_name(" .hkx"), None);
        assert_eq!(
            trimmed_spaced_file_name("Storm_DKWD_Interior_TallWallWithDoor .nif"),
            None,
            "spaced NIFs stay untouched — records reference them with the space"
        );
    }

    #[test]
    fn normalizes_only_the_final_component() {
        assert_eq!(
            normalized_reference(r"Characters\FloaterCharacter .hkx").as_deref(),
            Some(r"Characters\FloaterCharacter.hkx")
        );
        assert_eq!(normalized_reference(r"Characters\FloaterCharacter.hkx"), None);
        assert_eq!(
            normalized_reference("FloaterCharacter .hkx").as_deref(),
            Some("FloaterCharacter.hkx")
        );
    }

    #[test]
    fn renames_spaced_files_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let chars = tmp.path().join("data/Meshes/Actors/Floater/Characters");
        std::fs::create_dir_all(&chars).unwrap();
        std::fs::write(chars.join("floatercharacter .hkx"), b"dummy").unwrap();
        std::fs::write(chars.join("clean.hkx"), b"dummy").unwrap();

        let report = normalize_spaced_asset_names_in_mod_path(tmp.path()).unwrap();
        assert_eq!(report.records_changed, 1);
        assert!(chars.join("floatercharacter.hkx").is_file());
        assert!(!chars.join("floatercharacter .hkx").exists());
        assert!(chars.join("clean.hkx").is_file());
    }

    /// End-to-end on the converted floater character fixture: renaming
    /// `floatercharacter .hkx` must also strip the trailing space from the
    /// internal hkbCharacterData `name` ("FloaterCharacter ").
    #[test]
    fn strips_internal_character_name_when_renaming() {
        let data = include_bytes!("../../test_fixtures/havok_postprocess/floatercharacter.hkx")
            .to_vec();
        let tmp = tempfile::tempdir().unwrap();
        let chars = tmp.path().join("data/Meshes/Actors/Floater/Characters");
        std::fs::create_dir_all(&chars).unwrap();
        std::fs::write(chars.join("floatercharacter .hkx"), &data).unwrap();

        let report = normalize_spaced_asset_names_in_mod_path(tmp.path()).unwrap();
        assert!(report.records_changed >= 2, "rename + internal name strip");

        let renamed = chars.join("floatercharacter.hkx");
        assert!(renamed.is_file());
        let patched = read_packfile(&std::fs::read(&renamed).unwrap()).unwrap();
        let mut strings = Vec::new();
        for obj in patched.objects() {
            collect_strings(&obj.members, &mut strings);
        }
        assert!(
            strings.iter().any(|s| s == "FloaterCharacter"),
            "expected de-spaced internal character name"
        );
        assert!(
            !strings
                .iter()
                .any(|s| s.trim_end() != s && s.trim_end().eq_ignore_ascii_case("FloaterCharacter")),
            "no spaced internal character name may remain"
        );
    }

    /// End-to-end on the converted floater project fixture: the
    /// characterFilenames reference `Characters\FloaterCharacter .hkx` must
    /// come out de-spaced.
    #[test]
    fn patches_real_floater_project_reference() {
        let data = include_bytes!("../../test_fixtures/havok_postprocess/floaterproject.hkx").to_vec();
        let tmp = tempfile::tempdir().unwrap();
        let actor = tmp.path().join("data/Meshes/Actors/Floater");
        std::fs::create_dir_all(&actor).unwrap();
        let project = actor.join("floaterproject.hkx");
        std::fs::write(&project, &data).unwrap();

        let report = normalize_spaced_asset_names_in_mod_path(tmp.path()).unwrap();
        assert!(report.records_changed >= 1, "project must be patched");

        let patched = read_packfile(&std::fs::read(&project).unwrap()).unwrap();
        let mut strings = Vec::new();
        for obj in patched.objects() {
            collect_strings(&obj.members, &mut strings);
        }
        assert!(
            strings.iter().any(|s| s.eq_ignore_ascii_case(r"Characters\FloaterCharacter.hkx")),
            "expected de-spaced character reference, got: {:?}",
            strings.iter().filter(|s| s.to_ascii_lowercase().contains("character")).collect::<Vec<_>>()
        );
        assert!(
            !strings.iter().any(|s| s.contains(" .hkx")),
            "no spaced references may remain"
        );
    }

    #[cfg(test)]
    fn collect_strings(members: &[havok_native::hkx::HkxMember], out: &mut Vec<String>) {
        for m in members {
            collect_value_strings(&m.value, out);
        }
    }

    fn collect_value_strings(v: &HkxValue, out: &mut Vec<String>) {
        match v {
            HkxValue::String { value, .. } => out.push(value.clone()),
            HkxValue::Array(items) => items.iter().for_each(|i| collect_value_strings(i, out)),
            HkxValue::Object(members) => collect_strings(members, out),
            _ => {}
        }
    }
}
