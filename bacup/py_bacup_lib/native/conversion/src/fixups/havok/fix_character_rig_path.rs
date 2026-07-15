//! Rewrite FO76-style skeleton paths in converted character HKX files.
//!

//!
//! # What this does
//! FO76 ships `SingleBoneSkeleton.hkt` at
//! `Meshes\UniqueBehaviors\zSingleBoneSkeleton\` — one level up from the
//! `UniqueBehaviors\<name>\Characters\` directory — so FO76 `character.hkx`
//! files use `"..\zSingleBoneSkeleton\SingleBoneSkeleton.hkt"` as their
//! `rigName`.
//!
//! FO4 ships the same skeleton at `Meshes\GenericBehaviors\zSingleBoneSkeleton\`
//! — two levels up from the same directory — so the FO4 path should be
//! `"..\..\GenericBehaviors\zSingleBoneSkeleton\SingleBoneSkeleton.hkt"`.
//!
//! Without this rewrite, weapon FX characters fail to locate the skeleton and
//! the behavior graph never drives the NIF's controller sequences.
//!
//! This fixup walks every `character.hkx` under `ctx.mod_path/meshes/`,
//! loads it, checks `hkbCharacterStringData.rigName`, and rewrites if needed.
//!
//! # FixupReport mapping
//! `records_changed` = number of `character.hkx` files whose `rigName` was rewritten.

use std::path::{Path, PathBuf};

use havok_native::hkx::read_packfile;
use havok_native::hkx::types::HkxValue;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Lower-cased, backslash-normalised prefix that identifies FO76-style rig paths.
const FO76_RIG_PREFIX_LC: &str = "..\\zsingleboneskeleton\\";
const SINGLE_BONE_SKELETON_FILE: &str = "SingleBoneSkeleton.hkt";

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FixCharacterRigPathFixup;

impl Fixup for FixCharacterRigPathFixup {
    fn name(&self) -> &'static str {
        "fix_character_rig_path"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.havok
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
        fix_character_rig_path_in_mod_path(mod_path)
    }
}

// ---------------------------------------------------------------------------
// Mod-path entry point (used by postprocess wave)
// ---------------------------------------------------------------------------

pub fn fix_character_rig_path_in_mod_path(mod_path: &Path) -> Result<FixupReport, FixupError> {
    let mut rewritten = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_character_hkx_files(&meshes_root, &mut |hkx_path| match fix_rig_path(hkx_path) {
            Ok(true) => rewritten += 1,
            Ok(false) | Err(_) => {}
        });
    }
    Ok(FixupReport {
        records_changed: rewritten,
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

// ---------------------------------------------------------------------------
// Core algorithm
// ---------------------------------------------------------------------------

/// Returns `true` if the file's `rigName` was rewritten.
fn fix_rig_path(hkx_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(hkx_path)?;
    let mut hkx = read_packfile(&data)?;

    let mut changed = false;
    let fallback_rig = generic_single_bone_rig_path(hkx_path, SINGLE_BONE_SKELETON_FILE);

    'outer: for obj in hkx.objects_mut() {
        if obj.class_name != "hkbCharacterStringData" {
            continue;
        }
        for m in &mut obj.members {
            if m.name != "rigName" {
                continue;
            }
            if let HkxValue::String { value, .. } = &mut m.value {
                let low = value.to_lowercase().replace('/', "\\");
                if low.trim().is_empty() {
                    *value = fallback_rig;
                    changed = true;
                } else if low.starts_with(FO76_RIG_PREFIX_LC) {
                    // Extract the filename after the prefix (preserving original case).
                    let normalised = value.replace('/', "\\");
                    // Split on the first two backslash-delimited components:
                    // "..", "zSingleBoneSkeleton", <remainder>
                    let remainder = normalised
                        .splitn(3, '\\')
                        .nth(2)
                        .unwrap_or("SingleBoneSkeleton.hkt");
                    *value = generic_single_bone_rig_path(hkx_path, remainder);
                    changed = true;
                }
            }
            break 'outer;
        }
    }

    if changed {
        let out = hkx.save();
        std::fs::write(hkx_path, out)?;
    }

    Ok(changed)
}

fn generic_single_bone_rig_path(hkx_path: &Path, filename: &str) -> String {
    let project_rel_components = project_rel_components_from_meshes(hkx_path);
    if project_rel_components
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("GenericBehaviors"))
    {
        let up_count = project_rel_components.len().saturating_sub(1).max(1);
        let mut parts = vec!["..".to_string(); up_count];
        parts.push("zSingleBoneSkeleton".to_string());
        parts.push(filename.to_string());
        return parts.join("\\");
    }

    let up_count = project_rel_components.len().max(2);
    let mut parts = vec!["..".to_string(); up_count];
    parts.push("GenericBehaviors".to_string());
    parts.push("zSingleBoneSkeleton".to_string());
    parts.push(filename.to_string());
    parts.join("\\")
}

fn project_rel_components_from_meshes(hkx_path: &Path) -> Vec<String> {
    let Some(characters_dir) = hkx_path.parent() else {
        return Vec::new();
    };
    let Some(project_dir) = characters_dir.parent() else {
        return Vec::new();
    };

    let components: Vec<String> = project_dir
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
        .collect();
    let Some(meshes_index) = components
        .iter()
        .rposition(|component| component.eq_ignore_ascii_case("meshes"))
    else {
        return Vec::new();
    };
    components[meshes_index + 1..].to_vec()
}

// ---------------------------------------------------------------------------
// Directory walker — only "character.hkx" files
// ---------------------------------------------------------------------------

fn walk_character_hkx_files(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_character_hkx_files(&path, f);
        } else {
            let fname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            if fname == "character.hkx" {
                f(&path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use havok_native::hkx::descriptors::DescriptorRegistry;
    use havok_native::hkx::{HkxFile, HkxMember, HkxObject, write_hkx};

    #[test]
    fn fo76_prefix_constant_is_lowercase_backslash() {
        assert!(FO76_RIG_PREFIX_LC.starts_with("..\\"));
        assert_eq!(FO76_RIG_PREFIX_LC, FO76_RIG_PREFIX_LC.to_lowercase());
    }

    #[test]
    fn no_mod_path_returns_empty() {
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
        use crate::session::open_session;
        use crate::sym::StringInterner;

        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "FixRigPathTest.esp",
            Some("fo4"),
        )
        .expect("test plugin handle");
        let config = FixupConfig::default();
        let mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let fixup = FixCharacterRigPathFixup;
        assert!(!fixup.applies_to_session(&session, &config));
        let report = fixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert!(report.is_no_op());
    }

    fn write_character_hkx(path: &Path, rig_name: &str) {
        let hkx = HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![HkxObject {
                name: Some("#0001".to_string()),
                offset: 0,
                signature: 0,
                class_name: "hkbCharacterStringData".to_string(),
                members: vec![HkxMember {
                    name: "rigName".to_string(),
                    value: HkxValue::String {
                        value: rig_name.to_string(),
                        is_null: false,
                    },
                }],
            }],
        );
        let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
        let data = write_hkx(&hkx, &mut registry);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }

    fn read_rig_name(path: &Path) -> String {
        let data = std::fs::read(path).unwrap();
        let hkx = read_packfile(&data).unwrap();
        for obj in hkx.objects() {
            if obj.class_name != "hkbCharacterStringData" {
                continue;
            }
            for member in &obj.members {
                if member.name == "rigName"
                    && let HkxValue::String { value, .. } = &member.value
                {
                    return value.clone();
                }
            }
        }
        panic!("rigName not found")
    }

    #[test]
    fn empty_rig_name_under_furniture_uses_relative_generic_single_bone_skeleton() {
        let tmp = tempfile::tempdir().unwrap();
        let character = tmp
            .path()
            .join("data/Meshes/Furniture/Instruments/InstrumentBehavior/Characters/character.hkx");
        write_character_hkx(&character, "");

        assert!(fix_rig_path(&character).unwrap());

        assert_eq!(
            read_rig_name(&character),
            "..\\..\\..\\GenericBehaviors\\zSingleBoneSkeleton\\SingleBoneSkeleton.hkt"
        );
    }

    #[test]
    fn fo76_sibling_rig_path_under_unique_behaviors_still_maps_to_fo4_generic() {
        let tmp = tempfile::tempdir().unwrap();
        let character = tmp
            .path()
            .join("data/Meshes/UniqueBehaviors/MeltdownFX/Characters/character.hkx");
        write_character_hkx(
            &character,
            "..\\zSingleBoneSkeleton\\SingleBoneSkeleton.hkt",
        );

        assert!(fix_rig_path(&character).unwrap());

        assert_eq!(
            read_rig_name(&character),
            "..\\..\\GenericBehaviors\\zSingleBoneSkeleton\\SingleBoneSkeleton.hkt"
        );
    }
}
