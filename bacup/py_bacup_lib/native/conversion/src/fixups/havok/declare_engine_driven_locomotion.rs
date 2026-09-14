//! Mark converted creatures whose locomotion clips are in-place as engine-driven.
//!
//! FO4 graph-driven movement (`bGraphDriven` = 1, the default for most
//! creatures) takes translation and speed from root motion baked into the
//! walk/run clips; engine-driven (`bGraphDriven` = 0) moves the actor at its
//! Movement Type speeds while the graph plays the matching clip. FO76 authors
//! some creatures engine-driven: RadHog's `WalkFwd`, `WalkBwd`, `TrotFwd` and
//! `RunFwd` have no root motion, though its 34 other clips do. Left
//! graph-driven in FO4, such an actor never moves, `Speed` stays 0, the
//! idle-exit events never fire, and a melee creature never closes to attack
//! range. Creatures whose locomotion clips carry root motion are left alone.

use std::path::{Path, PathBuf};

use havok_native::hkx::HkxFile;
use havok_native::hkx::read_packfile;
use havok_native::hkx::types::HkxValue;

use crate::fixups::havok::normalize_weapon_behavior_contracts::{
    directory_named, member_array, mesh_roots_for_mod_path, pointer_member, push_array,
    push_string, set_word_value, string_array, variable_type, walk_behavior_dirs, word_value,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

pub struct DeclareEngineDrivenLocomotionFixup;

const GRAPH_DRIVEN: &str = "bGraphDriven";
/// `hkbVariableInfo::VariableType::VARIABLE_TYPE_BOOL`.
const VARIABLE_TYPE_BOOL: i64 = 0;
/// Clip name prefixes that identify a creature's ground locomotion set.
const LOCOMOTION_PREFIXES: [&str; 6] = ["walk", "run", "trot", "jog", "sprint", "gallop"];
/// Additive clips are deltas layered onto a base pose; their extracted motion is
/// legitimately zero, so they say nothing about how the creature is driven.
const ADDITIVE: &str = "additive";
/// Reference-frame samples below this are the exporter's zero.
const MOTION_EPSILON: f32 = 1e-6;

impl Fixup for DeclareEngineDrivenLocomotionFixup {
    fn name(&self) -> &'static str {
        "declare_engine_driven_locomotion"
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
        let Some(mod_path) = config.mod_path.as_deref() else {
            return Ok(FixupReport::empty());
        };
        declare_engine_driven_locomotion_in_mod_path(mod_path)
    }
}

pub fn declare_engine_driven_locomotion_in_mod_path(
    mod_path: &Path,
) -> Result<FixupReport, FixupError> {
    let mut changed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_behavior_dirs(&meshes_root, &mut |behavior_dir| {
            let Some(actor_dir) = behavior_dir.parent() else {
                return;
            };
            if locomotion_drive(actor_dir) != Drive::Engine {
                return;
            }
            let Ok(entries) = std::fs::read_dir(behavior_dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && declare_in_file(&path).unwrap_or(false) {
                    changed += 1;
                }
            }
        });
    }
    Ok(FixupReport {
        records_changed: changed,
        ..FixupReport::empty()
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Drive {
    /// Locomotion clips carry root motion — FO4's default handling is correct.
    Graph,
    /// Locomotion clips animate in place — the engine must supply the movement.
    Engine,
    /// No locomotion clips to judge by; leave the graph alone.
    Unknown,
}

/// Classify an actor directory by whether its locomotion clips translate.
///
/// Only clips that actually carry an extracted-motion frame get a vote, so a
/// creature whose locomotion lives in a shared graph — or whose clips we cannot
/// read — is left to FO4's default rather than guessed at.
fn locomotion_drive(actor_dir: &Path) -> Drive {
    let clips = match directory_named(actor_dir, "Animations") {
        Some(animations) => locomotion_clips(&animations),
        // Ported FO76 projects keep their clips outside the behavior project.
        None => referenced_locomotion_clips(actor_dir),
    };
    let votes: Vec<bool> = clips
        .iter()
        .filter_map(|clip| clip_root_motion(clip))
        .collect();
    match votes.iter().any(|translates| *translates) {
        true => Drive::Graph,
        false if votes.is_empty() => Drive::Unknown,
        false => Drive::Engine,
    }
}

fn referenced_locomotion_clips(actor_dir: &Path) -> Vec<PathBuf> {
    let Some(behaviors) = directory_named(actor_dir, "Behaviors") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(behaviors) else {
        return Vec::new();
    };
    let mut clips = std::collections::BTreeSet::new();
    for entry in entries.flatten() {
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        let Ok(graph) = read_packfile(&bytes) else {
            continue;
        };
        for object in graph.objects() {
            if object.class_name != "hkbClipGenerator" {
                continue;
            }
            let Some(HkxValue::String { value, .. }) = object
                .members
                .iter()
                .find(|member| member.name == "animationName")
                .map(|member| &member.value)
            else {
                continue;
            };
            let mut path = actor_dir.join(value.replace('\\', "/"));
            path.set_extension("hkx");
            if is_locomotion_clip(&path) {
                clips.insert(path);
            }
        }
    }
    clips.into_iter().collect()
}

fn locomotion_clips(animations: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(animations) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_locomotion_clip(path))
        .collect()
}

fn is_locomotion_clip(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("hkx"))
        && path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| {
                !stem.to_ascii_lowercase().contains(ADDITIVE)
                    && LOCOMOTION_PREFIXES.iter().any(|prefix| {
                        stem.len() >= prefix.len()
                            && stem[..prefix.len()].eq_ignore_ascii_case(prefix)
                    })
            })
}

/// Whether a clip's extracted motion moves the actor in world space.
///
/// `None` when the clip declares no extracted-motion samples at all — that is a
/// clip with nothing to say, not a clip that stands still.
fn clip_root_motion(path: &Path) -> Option<bool> {
    let hkx = read_packfile(&std::fs::read(path).ok()?).ok()?;
    let samples: Vec<&HkxValue> = hkx
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkaDefaultAnimatedReferenceFrame")
        .filter_map(|frame| member_array(frame, "referenceFrameSamples").ok())
        .flatten()
        .collect();
    if samples.is_empty() {
        return None;
    }
    Some(samples.iter().any(|sample| match sample {
        HkxValue::F32List(values) => values.iter().any(|value| value.abs() > MOTION_EPSILON),
        _ => false,
    }))
}

fn declare_in_file(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("hkx"))
    {
        return Ok(false);
    }
    let mut hkx = read_packfile(&std::fs::read(path)?)?;
    if !declare_engine_driven(&mut hkx).map_err(std::io::Error::other)? {
        return Ok(false);
    }
    std::fs::write(path, hkx.save())?;
    Ok(true)
}

/// Ensure the graph declares `bGraphDriven` with initial value 0.
///
/// An existing declaration is set to 0 in place. A missing one is appended at
/// the tail — keeping every existing variable index valid — with its info and
/// initial word cloned from a boolean the graph already declares, so the
/// appended entries match the file's own member layout.
fn declare_engine_driven(hkx: &mut HkxFile) -> Result<bool, String> {
    let data = hkx
        .objects()
        .iter()
        .position(|object| object.class_name == "hkbBehaviorGraphData")
        .ok_or_else(|| "missing hkbBehaviorGraphData".to_string())?;
    let strings = pointer_member(&hkx.objects()[data], "stringData")
        .ok_or_else(|| "missing behavior stringData".to_string())?;
    let initial_values = pointer_member(&hkx.objects()[data], "variableInitialValues")
        .ok_or_else(|| "missing behavior variableInitialValues".to_string())?;
    if strings >= hkx.objects().len() || initial_values >= hkx.objects().len() {
        return Err("behavior contract pointer is out of range".to_string());
    }

    let names = string_array(&hkx.objects()[strings], "variableNames")?;
    let infos = member_array(&hkx.objects()[data], "variableInfos")?;
    let words = member_array(&hkx.objects()[initial_values], "wordVariableValues")?;
    if names.len() != infos.len() || names.len() != words.len() {
        return Err("behavior variable arrays are not aligned".to_string());
    }

    if let Some(index) = names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(GRAPH_DRIVEN))
    {
        if word_value(&words[index]) == Some(0) {
            return Ok(false);
        }
        let objects = hkx.objects_mut();
        let words = objects[initial_values]
            .members
            .iter_mut()
            .find(|member| member.name == "wordVariableValues")
            .and_then(|member| match &mut member.value {
                HkxValue::Array(values) => Some(values),
                _ => None,
            })
            .ok_or_else(|| "wordVariableValues is not an array".to_string())?;
        set_word_value(&mut words[index], 0)?;
        return Ok(true);
    }

    let template = infos
        .iter()
        .position(|info| variable_type(info) == Some(VARIABLE_TYPE_BOOL))
        .ok_or_else(|| "graph declares no boolean variable to model".to_string())?;
    let info = infos[template].clone();
    let mut word = words[template].clone();
    set_word_value(&mut word, 0)?;

    let objects = hkx.objects_mut();
    push_string(
        &mut objects[strings],
        "variableNames",
        GRAPH_DRIVEN.to_string(),
    )?;
    push_array(&mut objects[data], "variableInfos", info)?;
    push_array(&mut objects[initial_values], "wordVariableValues", word)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use havok_native::hkx::{HkxMember, HkxObject};

    fn member(name: &str, value: HkxValue) -> HkxMember {
        HkxMember {
            name: name.to_string(),
            value,
        }
    }

    fn info(variable_type: i32) -> HkxValue {
        HkxValue::Object(vec![member("type", HkxValue::I32(variable_type))])
    }

    fn word(value: i32) -> HkxValue {
        HkxValue::Object(vec![member("value", HkxValue::I32(value))])
    }

    fn object(index: usize, class_name: &str, members: Vec<HkxMember>) -> HkxObject {
        HkxObject {
            name: Some(format!("#{index:04}")),
            offset: 0,
            signature: 0,
            class_name: class_name.to_string(),
            members,
        }
    }

    /// A graph declaring `names` as boolean variables, each with initial value 1.
    fn graph(names: &[&str]) -> HkxFile {
        let data = object(
            1,
            "hkbBehaviorGraphData",
            vec![
                member(
                    "variableInfos",
                    HkxValue::Array(
                        names
                            .iter()
                            .map(|_| info(VARIABLE_TYPE_BOOL as i32))
                            .collect(),
                    ),
                ),
                member("stringData", HkxValue::Pointer(Some(1))),
                member("variableInitialValues", HkxValue::Pointer(Some(2))),
            ],
        );
        let string_data = object(
            2,
            "hkbBehaviorGraphStringData",
            vec![member(
                "variableNames",
                HkxValue::Array(
                    names
                        .iter()
                        .map(|name| HkxValue::String {
                            value: (*name).to_string(),
                            is_null: false,
                        })
                        .collect(),
                ),
            )],
        );
        let initial_values = object(
            3,
            "hkbVariableValueSet",
            vec![member(
                "wordVariableValues",
                HkxValue::Array(names.iter().map(|_| word(1)).collect()),
            )],
        );
        HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![data, string_data, initial_values],
        )
    }

    fn names_of(hkx: &HkxFile) -> Vec<String> {
        string_array(&hkx.objects()[1], "variableNames").unwrap()
    }

    fn words_of(hkx: &HkxFile) -> Vec<Option<usize>> {
        member_array(&hkx.objects()[2], "wordVariableValues")
            .unwrap()
            .iter()
            .map(word_value)
            .collect()
    }

    #[test]
    fn appends_graph_driven_as_zero_when_absent() {
        let mut hkx = graph(&["bAnimationDriven", "bEquipOk"]);
        assert!(declare_engine_driven(&mut hkx).unwrap());

        assert_eq!(
            names_of(&hkx),
            vec!["bAnimationDriven", "bEquipOk", "bGraphDriven"],
            "must append at the tail so existing indices survive"
        );
        assert_eq!(
            words_of(&hkx),
            vec![Some(1), Some(1), Some(0)],
            "the appended variable must default to 0 and leave the others alone"
        );
        let infos = member_array(&hkx.objects()[0], "variableInfos").unwrap();
        assert_eq!(infos.len(), 3, "variableInfos must stay aligned");
        assert_eq!(
            variable_type(&infos[2]),
            Some(VARIABLE_TYPE_BOOL),
            "appended variable must be a boolean"
        );
    }

    #[test]
    fn clears_an_existing_graph_driven_declaration() {
        let mut hkx = graph(&["bGraphDriven", "bEquipOk"]);
        assert!(declare_engine_driven(&mut hkx).unwrap());

        assert_eq!(
            names_of(&hkx).len(),
            2,
            "must not duplicate the declaration"
        );
        assert_eq!(
            words_of(&hkx),
            vec![Some(0), Some(1)],
            "only bGraphDriven may be cleared"
        );
    }

    #[test]
    fn no_op_when_already_engine_driven() {
        let mut hkx = graph(&["bGraphDriven"]);
        declare_engine_driven(&mut hkx).unwrap();
        assert!(
            !declare_engine_driven(&mut hkx).unwrap(),
            "a graph already declaring 0 must round-trip untouched"
        );
    }

    #[test]
    fn match_is_case_insensitive() {
        let mut hkx = graph(&["bgraphdriven"]);
        declare_engine_driven(&mut hkx).unwrap();
        assert_eq!(names_of(&hkx).len(), 1, "must not append a cased duplicate");
        assert_eq!(words_of(&hkx), vec![Some(0)]);
    }

    #[test]
    fn rejects_graph_with_no_boolean_to_model() {
        let mut hkx = graph(&[]);
        assert!(
            declare_engine_driven(&mut hkx).is_err(),
            "must refuse rather than synthesize an info of unknown layout"
        );
    }

    #[test]
    fn rejects_misaligned_arrays() {
        let mut hkx = graph(&["bAnimationDriven"]);
        push_string(&mut hkx.objects_mut()[1], "variableNames", "extra".into()).unwrap();
        assert!(
            declare_engine_driven(&mut hkx).is_err(),
            "must not append onto arrays that are already out of lockstep"
        );
    }

    // --- drive classification -------------------------------------------

    /// Write a clip whose extracted motion is `samples`.
    fn write_clip(dir: &Path, name: &str, samples: &[[f32; 4]]) {
        let frame = object(
            1,
            "hkaDefaultAnimatedReferenceFrame",
            vec![member(
                "referenceFrameSamples",
                HkxValue::Array(
                    samples
                        .iter()
                        .map(|s| HkxValue::F32List(s.to_vec()))
                        .collect(),
                ),
            )],
        );
        let hkx = HkxFile::from_tagxml(11, "hk_2014.1.0-r1", vec![frame]);
        std::fs::write(dir.join(name), hkx.save()).unwrap();
    }

    fn actor_with_clips(clips: &[(&str, &[[f32; 4]])]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let animations = tmp.path().join("Animations");
        std::fs::create_dir_all(&animations).unwrap();
        for (name, samples) in clips {
            write_clip(&animations, name, samples);
        }
        tmp
    }

    #[test]
    fn in_place_locomotion_is_engine_driven() {
        let zeros = [[0.0f32; 4]; 3];
        let actor = actor_with_clips(&[
            ("WalkFwd.hkx", &zeros),
            ("TrotFwd.hkx", &zeros),
            ("RunFwd.hkx", &zeros),
        ]);
        assert_eq!(locomotion_drive(actor.path()), Drive::Engine);
    }

    #[test]
    fn translating_locomotion_stays_graph_driven() {
        let zeros = [[0.0f32; 4]; 3];
        let moving = [[0.0, 0.0, 0.0, 0.0], [0.0, 173.0, 0.0, 0.0]];
        let actor = actor_with_clips(&[("WalkBwd.hkx", &zeros), ("WalkForward.hkx", &moving)]);
        assert_eq!(
            locomotion_drive(actor.path()),
            Drive::Graph,
            "one translating clip is enough to prove the creature is graph-driven"
        );
    }

    #[test]
    fn non_locomotion_clips_do_not_decide_the_drive() {
        let zeros = [[0.0f32; 4]; 3];
        let actor = actor_with_clips(&[("Idle.hkx", &zeros), ("Death01.hkx", &zeros)]);
        assert_eq!(
            locomotion_drive(actor.path()),
            Drive::Unknown,
            "idles and deaths animate in place on every creature"
        );
    }

    #[test]
    fn additive_locomotion_clips_do_not_decide_the_drive() {
        // A humanoid whose locomotion lives in the shared character graph keeps
        // only additive deltas of its own; those are zero by construction.
        let zeros = [[0.0f32; 4]; 2];
        let actor = actor_with_clips(&[
            ("run_additive.hkx", &zeros),
            ("sprint_additive.hkx", &zeros),
        ]);
        assert_eq!(locomotion_drive(actor.path()), Drive::Unknown);
    }

    #[test]
    fn clips_without_extracted_motion_do_not_decide_the_drive() {
        let tmp = tempfile::tempdir().unwrap();
        let animations = tmp.path().join("Animations");
        std::fs::create_dir_all(&animations).unwrap();
        let bare = HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![object(1, "hkaAnimationContainer", vec![])],
        );
        std::fs::write(animations.join("WalkForward.hkx"), bare.save()).unwrap();
        assert_eq!(
            locomotion_drive(tmp.path()),
            Drive::Unknown,
            "a clip with no extracted-motion frame has nothing to say"
        );
    }

    #[test]
    fn actor_without_animations_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(locomotion_drive(tmp.path()), Drive::Unknown);
    }

    #[test]
    fn empty_mod_path_is_a_no_op() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            declare_engine_driven_locomotion_in_mod_path(tmp.path())
                .unwrap()
                .is_no_op()
        );
    }

    #[test]
    fn graph_driven_creatures_are_not_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let actor = tmp.path().join("data/Meshes/Actors/Snallygaster");
        let behaviors = actor.join("Behaviors");
        let animations = actor.join("Animations");
        std::fs::create_dir_all(&behaviors).unwrap();
        std::fs::create_dir_all(&animations).unwrap();
        write_clip(
            &animations,
            "WalkForward.hkx",
            &[[0.0, 0.0, 0.0, 0.0], [0.0, 173.0, 0.0, 0.0]],
        );
        std::fs::write(
            behaviors.join("Core.hkx"),
            graph(&["bAnimationDriven"]).save(),
        )
        .unwrap();

        let report = declare_engine_driven_locomotion_in_mod_path(tmp.path()).unwrap();
        assert_eq!(
            report.records_changed, 0,
            "a creature whose walk translates must be left byte-identical"
        );
    }

    #[test]
    fn engine_driven_creature_behaviors_are_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let actor = tmp.path().join("data/Meshes/Actors/RadHog");
        let behaviors = actor.join("Behaviors");
        let animations = actor.join("Animations");
        std::fs::create_dir_all(&behaviors).unwrap();
        std::fs::create_dir_all(&animations).unwrap();
        write_clip(&animations, "WalkFwd.hkx", &[[0.0f32; 4]; 3]);
        std::fs::write(
            behaviors.join("RadHogCoreBehavior.hkx"),
            graph(&["bAnimationDriven"]).save(),
        )
        .unwrap();
        std::fs::write(behaviors.join("notes.txt"), b"not hkx").unwrap();
        std::fs::write(behaviors.join("broken.hkx"), b"not a packfile").unwrap();

        let report = declare_engine_driven_locomotion_in_mod_path(tmp.path()).unwrap();
        assert_eq!(report.records_changed, 1);

        let written =
            read_packfile(&std::fs::read(behaviors.join("RadHogCoreBehavior.hkx")).unwrap())
                .unwrap();
        assert_eq!(names_of(&written), vec!["bAnimationDriven", "bGraphDriven"]);
        assert_eq!(words_of(&written), vec![Some(1), Some(0)]);
    }

    #[test]
    fn relocated_locomotion_clips_determine_drive_for_the_whole_project() {
        for translates in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let actor = tmp.path().join("data/Meshes/Actors/B21_FO76/RadHog");
            let behaviors = actor.join("Behaviors");
            let animations = actor.join("../Source/source_rig/Actors/RadHog/Animations");
            std::fs::create_dir_all(&behaviors).unwrap();
            std::fs::create_dir_all(&animations).unwrap();
            let distance = if translates { 173.0 } else { 0.0 };
            write_clip(
                &animations,
                "WalkFoward.hkx",
                &[[0.0; 4], [0.0, distance, 0.0, 0.0]],
            );
            let mut core = graph(&["bAnimationDriven"]);
            core.push_object(object(
                4,
                "hkbClipGenerator",
                vec![member(
                    "animationName",
                    HkxValue::String {
                        value: r"..\Source\source_rig\Actors\RadHog\Animations\WalkFoward.hkx"
                            .into(),
                        is_null: false,
                    },
                )],
            ));
            std::fs::write(behaviors.join("PrivateCore.hkx"), core.save()).unwrap();
            std::fs::write(
                behaviors.join("Root.hkx"),
                graph(&["bAnimationDriven"]).save(),
            )
            .unwrap();
            let report = declare_engine_driven_locomotion_in_mod_path(tmp.path()).unwrap();
            assert_eq!(report.records_changed, if translates { 0 } else { 2 });
            for name in ["PrivateCore.hkx", "Root.hkx"] {
                let written = read_packfile(&std::fs::read(behaviors.join(name)).unwrap()).unwrap();
                assert_eq!(
                    names_of(&written).contains(&"bGraphDriven".into()),
                    !translates
                );
                if !translates {
                    assert_eq!(words_of(&written).last(), Some(&Some(0)));
                }
            }
            assert!(
                declare_engine_driven_locomotion_in_mod_path(tmp.path())
                    .unwrap()
                    .is_no_op()
            );
        }
    }
}
