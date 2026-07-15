//! Populate `animationBundleNameData[0].assetNames` in character.hkx.
//!

//!
//! # What this does
//! FO76 `character.hkx` files have an empty `assetNames` array because FO76
//! resolves animations differently.  FO4 needs the `character.hkx` to list
//! every animation file the behavior graph references, otherwise the creature
//! T-poses.
//!
//! Algorithm:
//! 1. Collect behavior-referenced clip names from adjacent `Behaviors/` dir
//!    (via `collect_behavior_clip_names`). Actor-local names are intersected
//!    with the emitted `Animations/` tree; `..\` shared-actor references are
//!    preserved. Fall back to the disk inventory if no clip names are found.
//! 2. Read `character.hkx`, find `hkbCharacterStringData`, inject assetNames.
//! 3. Clean up orphan `hkbBoneIndexArray` objects (empty `boneIndices`,
//!    unreferenced) — TAG0 reader artifact.
//! 4. Remove trailing `variantVariableValues` entries pointing at orphans.
//!
//! Existing Havok string references are preserved. Some working FO4 creature
//! ports keep internal animation names as `.hkt` even when the converted loose
//! files are emitted as `.hkx`.
//!
//! # FixupReport mapping
//! `records_changed` = number of `character.hkx` files updated.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub use havok_native::behavior_clip_names::{
    collect_behavior_clip_names_from_dir, collect_behavior_clip_names_from_file,
};
use havok_native::hkx::read_packfile;
use havok_native::hkx::types::HkxValue;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct InjectAnimationNamesFixup;

impl Fixup for InjectAnimationNamesFixup {
    fn name(&self) -> &'static str {
        "inject_animation_names"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.havok && phases.animations
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

        inject_animation_names_in_mod_path(mod_path)
    }
}

pub fn inject_animation_names_in_mod_path(mod_path: &Path) -> Result<FixupReport, FixupError> {
    let mut changed_files: HashSet<PathBuf> = HashSet::new();

    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        find_character_hkx_dirs(&meshes_root, &mut |char_dir| {
            if char_dir.parent().is_none() {
                return;
            }

            // FO76 names the character file per-creature (radhog.hkx,
            // deathclaw.hkx, <creature>character.hkx, ...) and some actors put
            // it under Characters/<variant>/.  Process every .hkx under the
            // Characters tree; inject_anim_names is a no-op for files that lack
            // an empty hkbCharacterStringData assetNames array.
            let character_files = character_hkx_files_in_dir(char_dir);
            if character_files.is_empty() {
                return;
            }

            // Determine animation names from the sibling Behaviors/ dir first,
            // then fall back to scanning the Animations/ dir.  These are shared
            // by every character file in this directory, so resolve them once.
            let behavior_dir = char_dir.parent().and_then(|p| {
                let b = p.join("Behaviors");
                if b.is_dir() { Some(b) } else { None }
            });

            let anim_dir = char_dir.parent().and_then(|p| {
                let a = p.join("Animations");
                if a.is_dir() { Some(a) } else { None }
            });

            let clip_names: HashSet<String> = behavior_dir
                .as_deref()
                .map(collect_behavior_clip_names_from_dir)
                .unwrap_or_default();

            let anim_files = select_animation_names(&clip_names, anim_dir.as_deref());
            if anim_files.is_empty() {
                return;
            }

            for character_hkx in character_files {
                match inject_anim_names(&character_hkx, &anim_files) {
                    Ok(true) => {
                        changed_files.insert(character_hkx);
                    }
                    Ok(false) => {}
                    Err(_) => {} // tolerate errors
                }
            }
        });
    }

    Ok(FixupReport {
        records_changed: changed_files.len() as u32,
        ..FixupReport::empty()
    })
}

fn mesh_roots_for_mod_path(mod_path: &Path) -> Vec<std::path::PathBuf> {
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

// ---------------------------------------------------------------------------
// Disk-scan fallback
// ---------------------------------------------------------------------------

/// Collect animation file paths relative to `anim_dir`, matching the emitted
/// converted files.
fn collect_anim_files_from_disk(anim_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_anim_files_recursive(anim_dir, anim_dir, &mut files);
    files.sort();
    files
}

fn collect_anim_files_recursive(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_anim_files_recursive(root, &path, files);
            continue;
        }
        if !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("hkx"))
        {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            files.push(format!(
                "Animations\\{}",
                relative.to_string_lossy().replace('/', "\\")
            ));
        }
    }
}

fn animation_asset_key(path: &str) -> String {
    let normalized = path
        .trim()
        .trim_matches('\0')
        .replace('/', "\\")
        .to_ascii_lowercase();
    normalized
        .strip_suffix(".hkt")
        .or_else(|| normalized.strip_suffix(".hkx"))
        .unwrap_or(&normalized)
        .to_string()
}

fn select_animation_names(clip_names: &HashSet<String>, anim_dir: Option<&Path>) -> Vec<String> {
    let mut referenced: Vec<String> = clip_names.iter().cloned().collect();
    referenced.sort();
    if referenced.is_empty() {
        return anim_dir
            .map(collect_anim_files_from_disk)
            .unwrap_or_default();
    }

    let Some(anim_dir) = anim_dir else {
        return referenced;
    };
    let on_disk = collect_anim_files_from_disk(anim_dir);
    if on_disk.is_empty() {
        return referenced;
    }
    let available: HashSet<String> = on_disk
        .iter()
        .map(|path| animation_asset_key(path))
        .collect();
    let filtered: Vec<String> = referenced
        .into_iter()
        .filter(|path| {
            let key = animation_asset_key(path);
            !key.starts_with("animations\\") || available.contains(&key)
        })
        .collect();
    if filtered.is_empty() {
        on_disk
    } else {
        filtered
    }
}

// ---------------------------------------------------------------------------
// Inject animation names into character.hkx
// ---------------------------------------------------------------------------

/// Returns `true` if the file was modified.
fn inject_anim_names(
    character_hkx: &Path,
    anim_files: &[String],
) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(character_hkx)?;
    let mut hkx = read_packfile(&data)?;

    let mut modified = false;

    // Find hkbCharacterStringData and inject animation names.
    for obj in hkx.objects_mut() {
        if obj.class_name != "hkbCharacterStringData" {
            continue;
        }
        for member in &mut obj.members {
            if member.name != "animationBundleNameData" {
                continue;
            }
            if let HkxValue::Array(bundles) = &mut member.value {
                if bundles.is_empty() {
                    continue;
                }
                if let HkxValue::Object(bundle_members) = &mut bundles[0] {
                    for bm in bundle_members.iter_mut() {
                        match bm.name.as_str() {
                            "bundleName" => {
                                if let HkxValue::String { value, is_null } = &mut bm.value {
                                    if !*is_null && (value.is_empty() || value == "0") {
                                        value.clear();
                                        *is_null = true;
                                        modified = true;
                                    }
                                }
                            }
                            "assetNames" => {
                                if let HkxValue::Array(existing) = &bm.value {
                                    if existing.is_empty() {
                                        let new_names: Vec<HkxValue> = anim_files
                                            .iter()
                                            .map(|s| HkxValue::String {
                                                value: s.clone(),
                                                is_null: false,
                                            })
                                            .collect();
                                        bm.value = HkxValue::Array(new_names);
                                        modified = true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                break;
            }
            break;
        }
        break;
    }

    if !modified {
        return Ok(false);
    }

    // Clean up orphan hkbBoneIndexArray objects.
    // Collect all pointer targets first.
    let mut referenced: HashSet<usize> = HashSet::new();
    for obj in hkx.objects() {
        for m in &obj.members {
            collect_pointer_refs(&m.value, &mut referenced);
        }
    }

    // Find orphan bone index arrays: hkbBoneIndexArray with empty boneIndices
    // and no incoming pointer references.
    let mut orphan_indices: HashSet<usize> = HashSet::new();
    for (idx, obj) in hkx.objects().iter().enumerate() {
        if obj.class_name != "hkbBoneIndexArray" {
            continue;
        }
        if referenced.contains(&idx) {
            continue;
        }
        let has_data = obj.members.iter().any(|m| {
            m.name == "boneIndices" && matches!(&m.value, HkxValue::Array(v) if !v.is_empty())
        });
        if !has_data {
            orphan_indices.insert(idx);
        }
    }

    if !orphan_indices.is_empty() {
        hkx.retain_objects_remap_pointers(|idx, _obj| !orphan_indices.contains(&idx));

        // Clean up variantVariableValues entries pointing at orphans (now removed).
        // After retain, the orphan objects no longer exist; their pointer slots
        // in arrays will have been set to None by retain_objects_remap_pointers.
        // We need to remove Pointer(None) entries from variantVariableValues arrays.
        for obj in hkx.objects_mut() {
            if obj.class_name != "hkbVariableValueSet" {
                continue;
            }
            for m in &mut obj.members {
                if m.name != "variantVariableValues" {
                    continue;
                }
                if let HkxValue::Array(vals) = &mut m.value {
                    vals.retain(|v| !matches!(v, HkxValue::Pointer(None)));
                }
            }
        }
    }

    let out = hkx.save();
    std::fs::write(character_hkx, out)?;
    Ok(true)
}

/// Recursively collect all `Pointer(Some(idx))` references into `out`.
fn collect_pointer_refs(value: &HkxValue, out: &mut HashSet<usize>) {
    match value {
        HkxValue::Pointer(Some(idx)) => {
            out.insert(*idx);
        }
        HkxValue::Array(vals) => {
            for v in vals {
                collect_pointer_refs(v, out);
            }
        }
        HkxValue::Object(members) | HkxValue::TypedObject { members, .. } => {
            for m in members {
                collect_pointer_refs(&m.value, out);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Walk for Characters/ directories
// ---------------------------------------------------------------------------

/// Return every `*.hkx` file under `char_dir`, sorted for determinism.
///
/// FO76 stores the character file under a per-creature name (`radhog.hkx`,
/// `deathclaw.hkx`, `<creature>character.hkx`); only a handful use the literal
/// name `character.hkx`.  Matching every `.hkx` keeps creature animations from
/// being dropped.
fn character_hkx_files_in_dir(char_dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_character_hkx_files(char_dir, &mut files);
    files.sort();
    files
}

fn collect_character_hkx_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_character_hkx_files(&path, files);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("hkx"))
            .unwrap_or(false)
        {
            files.push(path);
        }
    }
}

/// Walk `dir` recursively, calling `f` on every directory named `characters`
/// (case-insensitive).
fn find_character_hkx_dirs(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let dname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            if dname == "characters" {
                f(&path);
            }
            find_character_hkx_dirs(&path, f);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_anim_files_empty_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let files = collect_anim_files_from_disk(dir.path());
        assert!(files.is_empty());
    }

    #[test]
    fn collect_anim_files_picks_up_hkx() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("idle.hkx"), b"dummy").unwrap();
        fs::write(dir.path().join("readme.txt"), b"txt").unwrap();
        let files = collect_anim_files_from_disk(dir.path());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "Animations\\idle.hkx");
    }

    #[test]
    fn filters_missing_behavior_clips_against_recursive_emitted_animations() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("Weapon").join("Injured").join("Left");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("Walk.hkx"), b"dummy").unwrap();
        let clip_names = HashSet::from([
            "Animations\\Weapon\\Injured\\Left\\Walk.hkt".to_string(),
            "Animations\\Weapon\\Injured\\Left\\Missing.hkt".to_string(),
        ]);

        let selected = select_animation_names(&clip_names, Some(dir.path()));

        assert_eq!(
            selected,
            vec!["Animations\\Weapon\\Injured\\Left\\Walk.hkt".to_string()]
        );
    }

    #[test]
    fn preserves_behavior_clips_when_no_animation_inventory_exists() {
        let dir = tempfile::tempdir().unwrap();
        let clip_names = HashSet::from(["Animations\\Idle.hkt".to_string()]);

        let selected = select_animation_names(&clip_names, Some(dir.path()));

        assert_eq!(selected, vec!["Animations\\Idle.hkt".to_string()]);
    }

    #[test]
    fn preserves_nonlocal_behavior_clips() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Idle.hkx"), b"dummy").unwrap();
        let clip_names = HashSet::from([
            "Animations\\Idle.hkt".to_string(),
            "..\\Character\\Animations\\SharedDeath.hkt".to_string(),
        ]);

        let selected = select_animation_names(&clip_names, Some(dir.path()));

        assert_eq!(
            selected,
            vec![
                "..\\Character\\Animations\\SharedDeath.hkt".to_string(),
                "Animations\\Idle.hkt".to_string(),
            ]
        );
    }

    #[test]
    fn mesh_roots_prefer_unified_data_meshes() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("data").join("Meshes")).unwrap();
        fs::create_dir_all(dir.path().join("meshes")).unwrap();

        let roots = mesh_roots_for_mod_path(dir.path());
        assert_eq!(roots.len(), 2);
        assert!(roots[0].ends_with("data\\Meshes") || roots[0].ends_with("data/Meshes"));
        assert!(roots[1].ends_with("meshes"));
    }

    #[test]
    fn finds_per_creature_character_files_not_just_character_hkx() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let chars = dir.path();
        // Real FO76 layout: per-creature name + a stray non-character file.
        fs::write(chars.join("radhog.hkx"), b"x").unwrap();
        fs::write(chars.join("character.hkx"), b"x").unwrap();
        fs::write(chars.join("notes.txt"), b"x").unwrap();

        let names: Vec<String> = character_hkx_files_in_dir(chars)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        // Both .hkx files are returned (sorted).
        assert_eq!(
            names,
            vec!["character.hkx".to_string(), "radhog.hkx".to_string()]
        );
    }

    fn write_hkx_file(path: &Path, objects: Vec<havok_native::hkx::HkxObject>) {
        use havok_native::hkx::descriptors::DescriptorRegistry;
        use havok_native::hkx::{HkxFile, write_hkx};

        let hkx = HkxFile::from_tagxml(11, "hk_2014.1.0-r1", objects);
        let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
        let data = write_hkx(&hkx, &mut registry);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }

    fn string_member(name: &str, value: &str) -> havok_native::hkx::HkxMember {
        havok_native::hkx::HkxMember {
            name: name.to_string(),
            value: HkxValue::String {
                value: value.to_string(),
                is_null: false,
            },
        }
    }

    fn write_character_fixture(path: &Path, asset_names: &[&str]) {
        use havok_native::hkx::{HkxMember, HkxObject};

        let asset_names = asset_names
            .iter()
            .map(|name| HkxValue::String {
                value: (*name).to_string(),
                is_null: false,
            })
            .collect();

        write_hkx_file(
            path,
            vec![HkxObject {
                name: Some("#0001".to_string()),
                offset: 0,
                signature: 0,
                class_name: "hkbCharacterStringData".to_string(),
                members: vec![
                    HkxMember {
                        name: "animationBundleNameData".to_string(),
                        value: HkxValue::Array(vec![HkxValue::Object(vec![
                            string_member("bundleName", ""),
                            HkxMember {
                                name: "assetNames".to_string(),
                                value: HkxValue::Array(asset_names),
                            },
                        ])]),
                    },
                    string_member("rigName", "CharacterAssets\\skeleton.hkt"),
                    string_member("ragdollName", "CharacterAssets\\skeleton.hkt"),
                ],
            }],
        );
    }

    fn write_behavior_fixture(path: &Path, animation_name: &str) {
        use havok_native::hkx::HkxObject;

        write_hkx_file(
            path,
            vec![HkxObject {
                name: Some("#0001".to_string()),
                offset: 0,
                signature: 0,
                class_name: "hkbClipGenerator".to_string(),
                members: vec![string_member("animationName", animation_name)],
            }],
        );
    }

    fn all_hkx_strings(path: &Path) -> Vec<String> {
        fn collect(value: &HkxValue, strings: &mut Vec<String>) {
            match value {
                HkxValue::String { value, .. } if !value.is_empty() => strings.push(value.clone()),
                HkxValue::Array(values) => {
                    for value in values {
                        collect(value, strings);
                    }
                }
                HkxValue::Object(members) | HkxValue::TypedObject { members, .. } => {
                    for member in members {
                        collect(&member.value, strings);
                    }
                }
                _ => {}
            }
        }

        let data = std::fs::read(path).unwrap();
        let hkx = read_packfile(&data).unwrap();
        let mut strings = Vec::new();
        for obj in hkx.objects() {
            for member in &obj.members {
                collect(&member.value, &mut strings);
            }
        }
        strings
    }

    fn character_bundle_name_is_null(path: &Path) -> bool {
        let data = std::fs::read(path).unwrap();
        let hkx = read_packfile(&data).unwrap();

        hkx.objects()
            .iter()
            .find(|obj| obj.class_name == "hkbCharacterStringData")
            .and_then(|obj| {
                obj.members
                    .iter()
                    .find(|member| member.name == "animationBundleNameData")
            })
            .and_then(|member| match &member.value {
                HkxValue::Array(bundles) => bundles.first(),
                _ => None,
            })
            .and_then(|bundle| match bundle {
                HkxValue::Object(members) | HkxValue::TypedObject { members, .. } => {
                    members.iter().find(|member| member.name == "bundleName")
                }
                _ => None,
            })
            .is_some_and(|member| matches!(&member.value, HkxValue::String { is_null: true, .. }))
    }

    #[test]
    fn injects_behavior_clip_names_without_rewriting_hkt_extensions() {
        let dir = tempfile::tempdir().unwrap();
        let project_dir = dir.path().join("data/Meshes/Actors/MegaSloth");
        let character = project_dir
            .join("Characters")
            .join("megaslothcharacter.hkx");
        let behavior = project_dir
            .join("Behaviors")
            .join("megaslothrootbehavior.hkx");

        write_character_fixture(&character, &[]);
        write_behavior_fixture(&behavior, "Animations\\Idle.hkt");
        std::fs::create_dir_all(project_dir.join("Animations")).unwrap();
        std::fs::write(project_dir.join("Animations").join("Idle.hkx"), b"dummy").unwrap();

        let report = inject_animation_names_in_mod_path(dir.path()).unwrap();

        assert_eq!(report.records_changed, 1);
        let character_strings = all_hkx_strings(&character);
        assert!(character_strings.contains(&"Animations\\Idle.hkt".to_string()));
        assert!(character_strings.contains(&"CharacterAssets\\skeleton.hkt".to_string()));
        assert!(character_bundle_name_is_null(&character));

        let behavior_strings = all_hkx_strings(&behavior);
        assert_eq!(behavior_strings, vec!["Animations\\Idle.hkt".to_string()]);
    }

    #[test]
    fn injects_nested_character_variant_from_actor_animation_dir() {
        let dir = tempfile::tempdir().unwrap();
        let project_dir = dir.path().join("data/Meshes/Actors/Turret");
        let character = project_dir
            .join("Characters")
            .join("Military")
            .join("turretmilitarycharacter.hkx");

        write_character_fixture(&character, &[]);
        std::fs::create_dir_all(project_dir.join("Animations").join("Military")).unwrap();
        std::fs::write(
            project_dir
                .join("Animations")
                .join("Military")
                .join("idle.hkx"),
            b"dummy",
        )
        .unwrap();

        let report = inject_animation_names_in_mod_path(dir.path()).unwrap();

        assert_eq!(report.records_changed, 1);
        let character_strings = all_hkx_strings(&character);
        assert!(character_strings.contains(&"Animations\\Military\\idle.hkx".to_string()));
    }

    #[test]
    fn normalizes_default_bundle_name_when_assets_are_already_present() {
        let dir = tempfile::tempdir().unwrap();
        let character = dir.path().join("character.hkx");
        let animation_names = vec!["Animations\\Idle.hkt".to_string()];
        write_character_fixture(&character, &["Animations\\Idle.hkt"]);

        assert!(!character_bundle_name_is_null(&character));
        assert!(inject_anim_names(&character, &animation_names).unwrap());
        assert!(character_bundle_name_is_null(&character));
        assert!(!inject_anim_names(&character, &animation_names).unwrap());
    }

    #[test]
    fn no_mod_path_returns_empty() {
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
        use crate::session::open_session;
        use crate::sym::StringInterner;

        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "InjectAnimationNamesTest.esp",
            Some("fo4"),
        )
        .expect("test plugin handle");
        let config = FixupConfig::default();
        let mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let fixup = InjectAnimationNamesFixup;
        assert!(!fixup.applies_to_session(&session, &config));
        let report = fixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert!(report.is_no_op());
    }
}
