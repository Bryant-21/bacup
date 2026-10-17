//! Remove known FO76-only generic behavior files from the output.
//!
//! FO4 loads every `.hkx` in a creature's `Behaviors/` directory through
//! `BSBehaviorGraphSwapGenerator` (its `pDefaultGenerator` is null in packfiles
//! and resolved at runtime), so unused FO76-only graphs are dead weight. Files a
//! surviving graph mounts through `hkbBehaviorReferenceGenerator::behaviorName`
//! stay: deleting one leaves a null subgraph that faults in
//! `hkbBehaviorReferenceGenerator::updateSync` (Mole Miner and Scorched T-pose
//! without the `Behaviors\DialogueBehavior.hkx` their `MTBehavior.hkx` mounts).
//! Creature-specific behaviors are always kept. `records_dropped` counts `.hkx`
//! files removed.

use std::collections::HashSet;
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

const FO76_ONLY_BEHAVIOR_PATHS: &[&str] = &[
    "actors/powerarmor/behaviors/raiderrootbehavior.hkx",
    "actors/powerarmor/_1stperson/behaviors/raiderrootbehavior.hkx",
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
    filter_unreferenced_behaviors_preserving(mod_path, &[])
}

pub(crate) fn filter_unreferenced_behaviors_preserving(
    mod_path: &Path,
    preserved_files: &[PathBuf],
) -> Result<FixupReport, FixupError> {
    filter_unreferenced_behaviors_with_cache(mod_path, preserved_files, &mut Default::default())
}

pub(crate) fn filter_unreferenced_behaviors_with_cache(
    mod_path: &Path,
    preserved_files: &[PathBuf],
    cache: &mut super::postprocess_scan::BehaviorNamesCache,
) -> Result<FixupReport, FixupError> {
    let preserved: HashSet<_> = preserved_files.iter().map(|path| path_key(path)).collect();
    let mut removed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        let mut behavior_dirs = Vec::new();
        walk_behavior_dirs(&meshes_root, &mut |behavior_dir| {
            behavior_dirs.push(behavior_dir.to_path_buf());
        });
        let mounted =
            behaviors_mounted_by_surviving_graphs(&meshes_root, &behavior_dirs, &preserved, cache);
        for behavior_dir in &behavior_dirs {
            removed += remove_fo76_only_behaviors(behavior_dir, &mounted);
        }
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

/// Remove known FO76-only behaviors from `behavior_dir`, skipping any that a
/// surviving graph mounts (`mounted`, keyed by [`path_key`]).
/// Returns the count of files removed.
fn remove_fo76_only_behaviors(behavior_dir: &Path, mounted: &HashSet<String>) -> u32 {
    let mut removed = 0;
    for path in hkx_files_in(behavior_dir) {
        if !is_filter_candidate(&path) || mounted.contains(&path_key(&path)) {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn is_filter_candidate(path: &Path) -> bool {
    let fname = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    (FO76_ONLY_BEHAVIORS.contains(&fname.as_str()) || is_fo76_only_behavior_path(path))
        && !is_preserved_behavior_path(path)
}

fn is_fo76_only_behavior_path(path: &Path) -> bool {
    FO76_ONLY_BEHAVIOR_PATHS
        .iter()
        .any(|candidate| path_key(path).ends_with(candidate))
}

fn is_preserved_behavior_path(path: &Path) -> bool {
    PRESERVED_BEHAVIOR_PATHS
        .iter()
        .any(|candidate| path_key(path).ends_with(candidate))
}

/// Case-insensitive, separator-insensitive key for comparing two paths.
fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn hkx_files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("hkx"))
                    .unwrap_or(false)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Reference reachability
// ---------------------------------------------------------------------------

/// Filter candidates that a surviving graph mounts as a subgraph, transitively.
///
/// Seeded with every behavior the filter would never touch, then closed over
/// `behaviorName` edges: a candidate reached from a survivor becomes a survivor
/// itself, so a chain like `MTBehavior -> DialogueBehavior -> …` is kept whole.
/// Candidates that only ever reference each other stay unreachable and are removed.
fn behaviors_mounted_by_surviving_graphs(
    meshes_root: &Path,
    behavior_dirs: &[PathBuf],
    preserved: &HashSet<String>,
    cache: &mut super::postprocess_scan::BehaviorNamesCache,
) -> HashSet<String> {
    let mut candidates: HashSet<String> = HashSet::new();
    let mut frontier: Vec<PathBuf> = Vec::new();
    for dir in behavior_dirs {
        for path in hkx_files_in(dir) {
            if is_filter_candidate(&path) && !preserved.contains(&path_key(&path)) {
                candidates.insert(path_key(&path));
            } else {
                frontier.push(path);
            }
        }
    }

    let mut mounted = preserved.clone();
    while let Some(path) = frontier.pop() {
        for reference in &cache.get(&path).mounted {
            for target in resolve_behavior_reference(meshes_root, &path, &reference) {
                let key = path_key(&target);
                if candidates.contains(&key) && mounted.insert(key) {
                    frontier.push(target);
                }
            }
        }
    }
    mounted
}

/// Candidate on-disk targets for a `behaviorName`. FO76 authors these relative to
/// the actor root (`Behaviors\X.hkx`), but a fully-qualified form
/// (`Actors\Shared\Behaviors\X.hkx`) also appears, so try both plus a same-directory
/// fallback. Non-existent candidates are harmless — the caller only keeps hits.
fn resolve_behavior_reference(
    meshes_root: &Path,
    referrer: &Path,
    reference: &str,
) -> Vec<PathBuf> {
    let relative = reference.replace('\\', "/");
    let mut targets = Vec::new();
    let Some(behavior_dir) = referrer.parent() else {
        return targets;
    };
    if let Some(basename) = Path::new(&relative).file_name() {
        targets.push(behavior_dir.join(basename));
    }
    if let Some(actor_root) = behavior_dir.parent() {
        targets.push(actor_root.join(&relative));
    }
    targets.push(meshes_root.join(&relative));
    targets
}

/// Walk `dir` recursively, calling `f` on every directory named `behaviors`
/// (case-insensitive).
fn walk_behavior_dirs(dir: &Path, f: &mut impl FnMut(&Path)) {
    walk_behavior_dirs_in_root(dir, f, dir);
}

fn walk_behavior_dirs_in_root(dir: &Path, f: &mut impl FnMut(&Path), scan_root: &Path) {
    let Ok(entries) = super::postprocess_scan::read_asset_dir(dir, scan_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if super::postprocess_scan::entry_is_dir(&entry) {
            let dname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            if dname == "behaviors" {
                f(&path);
            }
            walk_behavior_dirs_in_root(&path, f, scan_root);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use havok_native::hkx::types::HkxValue;

    #[test]
    fn remove_fo76_only_behaviors_in_temp_dir() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let behavior_dir = dir.path();

        // Create some files: one FO76-only, one creature-specific.
        fs::write(behavior_dir.join("ambushbehavior.hkx"), b"dummy").unwrap();
        fs::write(behavior_dir.join("deathclaw.hkx"), b"dummy").unwrap();

        let removed = remove_fo76_only_behaviors(behavior_dir, &HashSet::new());
        assert_eq!(removed, 1, "should remove exactly ambushbehavior.hkx");
        assert!(!behavior_dir.join("ambushbehavior.hkx").exists());
        assert!(behavior_dir.join("deathclaw.hkx").exists());
    }

    #[test]
    fn removes_power_armor_raider_roots_without_filtering_creature_roots() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let meshes = dir.path().join("data/Meshes/Actors");
        let power_armor = meshes.join("PowerArmor/Behaviors");
        let first_person = meshes.join("PowerArmor/_1stPerson/Behaviors");
        let scorched = meshes.join("Scorched/Behaviors");
        let mole_miner = meshes.join("MoleMiner/Behaviors");
        for behavior_dir in [&power_armor, &first_person, &scorched, &mole_miner] {
            fs::create_dir_all(behavior_dir).unwrap();
            fs::write(behavior_dir.join("RaiderRootBehavior.hkx"), b"dummy").unwrap();
        }

        let report = filter_unreferenced_behaviors_in_mod_path(dir.path()).unwrap();

        assert_eq!(report.records_dropped, 2);
        assert!(!power_armor.join("RaiderRootBehavior.hkx").exists());
        assert!(!first_person.join("RaiderRootBehavior.hkx").exists());
        assert!(scorched.join("RaiderRootBehavior.hkx").exists());
        assert!(mole_miner.join("RaiderRootBehavior.hkx").exists());
    }

    #[test]
    fn generated_roots_and_their_dependencies_survive_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let behaviors = root.path().join("data/Meshes/Actors/Generated/Behaviors");
        std::fs::create_dir_all(&behaviors).unwrap();
        let generated = behaviors.join("SharedCoreBehavior.hkx");
        let dependency = behaviors.join("DialogueBehavior.hkx");
        let unused = behaviors.join("AmbushBehavior.hkx");
        write_graph_mounting(&generated, &["Behaviors\\DialogueBehavior.hkx"]);
        write_graph_mounting(&dependency, &[]);
        write_graph_mounting(&unused, &[]);

        let report =
            filter_unreferenced_behaviors_preserving(root.path(), &[generated.clone()]).unwrap();
        assert_eq!(report.records_dropped, 1);
        assert!(generated.is_file());
        assert!(dependency.is_file());
        assert!(!unused.exists());
    }

    #[test]
    fn animation_name_scan_retains_mounts_for_the_following_filter() {
        let root = tempfile::tempdir().unwrap();
        let behaviors = root.path().join("data/Meshes/Actors/Generated/Behaviors");
        std::fs::create_dir_all(&behaviors).unwrap();
        let generated = behaviors.join("SharedCoreBehavior.hkx");
        let dependency = behaviors.join("DialogueBehavior.hkx");
        let unused = behaviors.join("AmbushBehavior.hkx");
        write_graph_mounting(&generated, &["Behaviors/DialogueBehavior"]);
        write_graph_mounting(&dependency, &[]);
        write_graph_mounting(&unused, &[]);
        let mut cache = super::super::postprocess_scan::BehaviorNamesCache::default();
        cache.clips_in(&behaviors);
        let report = filter_unreferenced_behaviors_with_cache(root.path(), &[generated.clone()], &mut cache).unwrap();
        assert_eq!(report.records_dropped, 1);
        assert_eq!(cache.files(), 3);
        assert!(cache.hits >= 2);
        assert!(generated.is_file() && dependency.is_file());
        assert!(!unused.exists());
    }

    /// Write a behavior packfile that mounts each of `subgraphs`.
    fn write_graph_mounting(path: &Path, subgraphs: &[&str]) {
        use havok_native::hkx::descriptors::DescriptorRegistry;
        use havok_native::hkx::{HkxFile, HkxMember, HkxObject, write_hkx};

        let objects = subgraphs
            .iter()
            .enumerate()
            .map(|(index, name)| HkxObject {
                name: Some(format!("#{:04}", index + 1)),
                offset: 0,
                signature: 0,
                class_name: "hkbBehaviorReferenceGenerator".to_string(),
                members: vec![HkxMember {
                    name: "behaviorName".to_string(),
                    value: HkxValue::String {
                        value: (*name).to_string(),
                        is_null: false,
                    },
                }],
            })
            .collect();
        let hkx = HkxFile::from_tagxml(11, "hk_2014.1.0-r1", objects);
        let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
        std::fs::write(path, write_hkx(&hkx, &mut registry)).unwrap();
    }

    #[test]
    fn keeps_a_filtered_behavior_that_a_surviving_graph_mounts() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let behaviors = dir.path().join("data/Meshes/Actors/MoleMiner/Behaviors");
        fs::create_dir_all(&behaviors).unwrap();

        // Survivor mounts DialogueBehavior — exactly the Mole Miner T-pose shape.
        write_graph_mounting(
            &behaviors.join("mtbehavior.hkx"),
            &["Behaviors\\DialogueBehavior.hkx"],
        );
        fs::write(behaviors.join("dialoguebehavior.hkx"), b"dummy").unwrap();
        // Same filtered basename, mounted by nobody.
        fs::write(behaviors.join("furniturefishingbehavior.hkx"), b"dummy").unwrap();

        let report = filter_unreferenced_behaviors_in_mod_path(dir.path()).unwrap();

        assert!(
            behaviors.join("dialoguebehavior.hkx").exists(),
            "mounted subgraph must survive or the graph faults on a null subgraph"
        );
        assert!(!behaviors.join("furniturefishingbehavior.hkx").exists());
        assert_eq!(report.records_dropped, 1);
    }

    #[test]
    fn mount_chains_through_a_filtered_behavior_are_kept_whole() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let behaviors = dir.path().join("data/Meshes/Actors/MoleMiner/Behaviors");
        fs::create_dir_all(&behaviors).unwrap();

        write_graph_mounting(
            &behaviors.join("mtbehavior.hkx"),
            &["Behaviors\\FurnitureBehavior.hkx"],
        );
        write_graph_mounting(
            &behaviors.join("furniturebehavior.hkx"),
            &["Behaviors\\DialogueBehavior.hkx"],
        );
        fs::write(behaviors.join("dialoguebehavior.hkx"), b"dummy").unwrap();

        let report = filter_unreferenced_behaviors_in_mod_path(dir.path()).unwrap();

        assert!(behaviors.join("furniturebehavior.hkx").exists());
        assert!(
            behaviors.join("dialoguebehavior.hkx").exists(),
            "second hop of the mount chain must survive too"
        );
        assert_eq!(report.records_dropped, 0);
    }

    #[test]
    fn filtered_behaviors_that_only_reference_each_other_are_still_removed() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let behaviors = dir.path().join("data/Meshes/Actors/Ghoul/Behaviors");
        fs::create_dir_all(&behaviors).unwrap();

        // A cycle among candidates, unreachable from any survivor.
        write_graph_mounting(
            &behaviors.join("furniturebehavior.hkx"),
            &["Behaviors\\DialogueBehavior.hkx"],
        );
        write_graph_mounting(
            &behaviors.join("dialoguebehavior.hkx"),
            &["Behaviors\\FurnitureBehavior.hkx"],
        );

        let report = filter_unreferenced_behaviors_in_mod_path(dir.path()).unwrap();

        assert_eq!(report.records_dropped, 2);
        assert!(!behaviors.join("furniturebehavior.hkx").exists());
        assert!(!behaviors.join("dialoguebehavior.hkx").exists());
    }

    #[test]
    fn extensionless_mount_names_still_resolve() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let behaviors = dir.path().join("data/Meshes/Actors/MoleMiner/Behaviors");
        fs::create_dir_all(&behaviors).unwrap();

        // Unconverted FO76 graphs author behaviorName bare.
        write_graph_mounting(
            &behaviors.join("weaponbehavior.hkx"),
            &["Behaviors\\DialogueBehavior"],
        );
        fs::write(behaviors.join("dialoguebehavior.hkx"), b"dummy").unwrap();

        let report = filter_unreferenced_behaviors_in_mod_path(dir.path()).unwrap();

        assert!(behaviors.join("dialoguebehavior.hkx").exists());
        assert_eq!(report.records_dropped, 0);
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
