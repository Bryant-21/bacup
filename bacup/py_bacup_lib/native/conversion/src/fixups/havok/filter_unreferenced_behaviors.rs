//! Remove known FO76-only generic behavior files from the output.
//!

//!
//! # What this does
//! FO4 loads ALL `.hkx` files in a creature's `Behaviors/` directory via
//! `BSBehaviorGraphSwapGenerator` at runtime (the swap generator's
//! `pDefaultGenerator` pointer is null in packfiles and resolved at runtime).
//! There is no explicit reference chain to follow.
//!
//! Instead, remove a known set of FO76-only generic behavior files that have
//! no FO4 equivalent.  Creature-specific behaviors (named after the creature)
//! are always kept.
//!
//! # FixupReport mapping
//! `records_dropped` = number of `.hkx` files removed from disk.

use std::path::{Path, PathBuf};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Known FO76-only generic behavior filenames (all lower-case)

// ---------------------------------------------------------------------------

const FO76_ONLY_BEHAVIORS: &[&str] = &[
    "ambushbehavior.hkx",
    "dialoguebehavior.hkx",
    "furniturebed.hkx",
    "furniturebehavior.hkx",
    "furniturefishingbehavior.hkx",
    "furniturenomirrorbehavior.hkx",
    "sharedcorebehavior.hkx",
    "sharedrootbehavior.hkx",
    "sharedcorewrappingbehavior.hkx",
];

const PRESERVED_BEHAVIOR_PATHS: &[&str] =
    &["actors/atx/sharedcollectron/behaviors/sharedcorebehavior.hkx"];

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FilterUnreferencedBehaviorsFixup;

impl Fixup for FilterUnreferencedBehaviorsFixup {
    fn name(&self) -> &'static str {
        "filter_unreferenced_behaviors"
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
        filter_unreferenced_behaviors_in_mod_path(mod_path)
    }
}

// ---------------------------------------------------------------------------
// Mod-path entry point (used by postprocess wave)
// ---------------------------------------------------------------------------

pub fn filter_unreferenced_behaviors_in_mod_path(
    mod_path: &Path,
) -> Result<FixupReport, FixupError> {
    let mut removed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_behavior_dirs(&meshes_root, &mut |behavior_dir| {
            removed += remove_fo76_only_behaviors(behavior_dir);
        });
    }
    Ok(FixupReport {
        records_dropped: removed,
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

/// Remove known FO76-only behaviors from `behavior_dir`.
/// Returns the count of files removed.
fn remove_fo76_only_behaviors(behavior_dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(behavior_dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let fname = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if FO76_ONLY_BEHAVIORS.contains(&fname.as_str()) && !is_preserved_behavior_path(&path) {
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

fn is_preserved_behavior_path(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    PRESERVED_BEHAVIOR_PATHS
        .iter()
        .any(|candidate| normalized.ends_with(candidate))
}

/// Walk `dir` recursively, calling `f` on every directory named `behaviors`
/// (case-insensitive).
fn walk_behavior_dirs(dir: &Path, f: &mut impl FnMut(&Path)) {
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
            if dname == "behaviors" {
                f(&path);
            }
            walk_behavior_dirs(&path, f);
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
    fn fo76_only_behaviors_list_not_empty() {
        assert!(!FO76_ONLY_BEHAVIORS.is_empty());
    }

    #[test]
    fn all_entries_are_lowercase() {
        for name in FO76_ONLY_BEHAVIORS {
            assert_eq!(*name, name.to_lowercase(), "{name} is not lowercase");
        }
    }

    #[test]
    fn no_mod_path_returns_empty() {
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
        use crate::session::open_session;
        use crate::sym::StringInterner;

        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "FilterBehaviorsTest.esp",
            Some("fo4"),
        )
        .expect("test plugin handle");
        let config = FixupConfig::default();
        let mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let fixup = FilterUnreferencedBehaviorsFixup;
        assert!(!fixup.applies_to_session(&session, &config));
        let report = fixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert!(report.is_no_op());
    }

    #[test]
    fn remove_fo76_only_behaviors_in_temp_dir() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let behavior_dir = dir.path();

        // Create some files: one FO76-only, one creature-specific.
        fs::write(behavior_dir.join("ambushbehavior.hkx"), b"dummy").unwrap();
        fs::write(behavior_dir.join("deathclaw.hkx"), b"dummy").unwrap();

        let removed = remove_fo76_only_behaviors(behavior_dir);
        assert_eq!(removed, 1, "should remove exactly ambushbehavior.hkx");
        assert!(!behavior_dir.join("ambushbehavior.hkx").exists());
        assert!(behavior_dir.join("deathclaw.hkx").exists());
    }

    #[test]
    fn preserves_collectron_shared_core_but_filters_unrelated_shared_core() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let meshes = dir.path().join("data/Meshes/Actors");
        let collectron = meshes.join("aTx/sHaReDcOlLeCtRoN/bEhAvIoRs");
        let unrelated = meshes.join("OtherCreature/Behaviors");
        fs::create_dir_all(&collectron).unwrap();
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(collectron.join("sHaReDcOrEbEhAvIoR.HkX"), b"collectron").unwrap();
        fs::write(unrelated.join("SharedCoreBehavior.hkx"), b"unrelated").unwrap();

        let report = filter_unreferenced_behaviors_in_mod_path(dir.path()).unwrap();

        assert_eq!(report.records_dropped, 1);
        assert!(collectron.join("sHaReDcOrEbEhAvIoR.HkX").exists());
        assert!(!unrelated.join("SharedCoreBehavior.hkx").exists());
    }
}
