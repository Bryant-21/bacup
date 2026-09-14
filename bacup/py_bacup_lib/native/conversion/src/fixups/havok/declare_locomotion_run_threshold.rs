//! Give a converted root behavior graph the run half of FO4's gait thresholds.
//!
//! FO4 buckets the requested `Speed` into a gait against the root graph's
//! `fSpeedWalk` and `fSpeedRun` (on 1.11.221 the player's root has 85 and 300;
//! crossing 300 moves `iLocomotionSpeedState` into the run branch). FO76's mole
//! miner root declares neither, blending gait off `Speed` and the
//! `fLocomotion*PlaybackSpeed` scalars. The recompiled root still gets
//! `fSpeedWalk` from merged child graphs (`weaponbehavior.hkx`, the injured
//! wrappers), but `fSpeedRun` exists only in the sibling
//! `raiderrootbehavior.hkx`, so it never comes along. Root graphs only: vanilla
//! non-root graphs carry `fSpeedWalk` alone.
//!
//! This does not make an actor run whose `Speed` never passes `fSpeedWalk`: the
//! converted mole miner requests ~46.8 against a melee Movement Type of walk
//! 87.55 / run 331.84, so the request pins it to a walk.

use std::path::Path;

use havok_native::hkx::HkxFile;
use havok_native::hkx::read_packfile;

use crate::fixups::havok::normalize_weapon_behavior_contracts::{
    member_array, mesh_roots_for_mod_path, pointer_member, push_array, push_string, set_word_value,
    string_array, walk_behavior_dirs,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

pub struct DeclareLocomotionRunThresholdFixup;

const SPEED_WALK: &str = "fSpeedWalk";
const SPEED_RUN: &str = "fSpeedRun";
/// Root behavior graphs are the only ones vanilla gives the full pair to.
const ROOT_SUFFIX: &str = "rootbehavior.hkx";
/// Measured on the live FO4 player root, paired with the `fSpeedWalk` of 85 that
/// the converted graphs already carry.
const RUN_THRESHOLD: f32 = 300.0;

impl Fixup for DeclareLocomotionRunThresholdFixup {
    fn name(&self) -> &'static str {
        "declare_locomotion_run_threshold"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.havok
    }

    /// Selects the `run_with_session` entry point. This is an API-shape flag, NOT a statement
    /// about needing session data — returning false makes the dispatcher look for the legacy
    /// `run()` and abort the whole Translate Records phase with "does not implement the legacy
    /// API". Asset-only fixups still answer true.
    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        config.mod_path.is_some() && !crate::fo76_behaviors::enabled(session, config)
    }

    fn run_with_session(
        &self,
        _session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let Some(mod_path) = config.mod_path.as_ref() else {
            return Ok(FixupReport::empty());
        };
        declare_run_threshold_in_mod_path(mod_path)
    }
}

pub fn declare_run_threshold_in_mod_path(mod_path: &Path) -> Result<FixupReport, FixupError> {
    let mut changed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_behavior_dirs(&meshes_root, &mut |behavior_dir| {
            let Ok(entries) = std::fs::read_dir(behavior_dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let is_root = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.to_ascii_lowercase().ends_with(ROOT_SUFFIX));
                if is_root && declare_in_file(&path).unwrap_or(false) {
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

fn declare_in_file(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let mut hkx = read_packfile(&std::fs::read(path)?)?;
    if !declare_run_threshold(&mut hkx).map_err(std::io::Error::other)? {
        return Ok(false);
    }
    std::fs::write(path, hkx.save())?;
    Ok(true)
}

/// Append `fSpeedRun` to a root graph that declares `fSpeedWalk` without it.
///
/// The new entry is appended at the tail — keeping every existing variable index
/// valid — with its info and initial word cloned from `fSpeedWalk` itself. Using
/// the walk threshold as the template is what makes the appended entries match
/// the file's own member layout and real variable type without hardcoding the
/// Havok type enum. A graph that declares neither is left alone: it is not
/// threshold-driven, and inventing half a contract for it would be a guess.
fn declare_run_threshold(hkx: &mut HkxFile) -> Result<bool, String> {
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

    if names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(SPEED_RUN))
    {
        return Ok(false);
    }
    let Some(template) = names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(SPEED_WALK))
    else {
        return Ok(false);
    };

    let info = infos[template].clone();
    let mut word = words[template].clone();
    // A real variable stores its initial value as the float's bit pattern in the
    // word slot, so the threshold has to be bit-cast rather than rounded.
    set_word_value(&mut word, RUN_THRESHOLD.to_bits() as usize)?;

    let objects = hkx.objects_mut();
    push_string(
        &mut objects[strings],
        "variableNames",
        SPEED_RUN.to_string(),
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
    use crate::fixups::havok::normalize_weapon_behavior_contracts::word_value;
    use havok_native::hkx::types::HkxValue;
    use havok_native::hkx::{HkxMember, HkxObject};

    /// `hkbVariableInfo::VariableType::VARIABLE_TYPE_REAL`.
    const VARIABLE_TYPE_REAL: i32 = 4;

    fn member(name: &str, value: HkxValue) -> HkxMember {
        HkxMember {
            name: name.to_string(),
            value,
        }
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

    /// A graph declaring `names` as real variables, each holding 85.0.
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
                            .map(|_| {
                                HkxValue::Object(vec![member(
                                    "type",
                                    HkxValue::I32(VARIABLE_TYPE_REAL),
                                )])
                            })
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
                HkxValue::Array(
                    names
                        .iter()
                        .map(|_| {
                            HkxValue::Object(vec![member(
                                "value",
                                HkxValue::I32(85.0f32.to_bits() as i32),
                            )])
                        })
                        .collect(),
                ),
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

    /// Registration contract. `uses_session` picks which entry point the dispatcher calls, and
    /// only `run_with_session` is implemented here — answering false aborts Translate Records
    /// at runtime with "does not implement the legacy API", which no unit test of the graph
    /// edit itself would catch.
    #[test]
    fn fixup_declares_the_api_it_actually_implements() {
        let fixup = DeclareLocomotionRunThresholdFixup;
        assert!(fixup.uses_session());
        assert_eq!(fixup.name(), "declare_locomotion_run_threshold");
    }

    #[test]
    fn walk_only_graph_gains_the_run_threshold_at_the_tail() {
        let mut hkx = graph(&["bIsSynced", SPEED_WALK, "Direction"]);
        assert!(declare_run_threshold(&mut hkx).unwrap());

        let names = names_of(&hkx);
        assert_eq!(names.len(), 4);
        // Appended at the tail so every pre-existing variable index stays valid.
        assert_eq!(names[3], SPEED_RUN);
        assert_eq!(&names[..3], &["bIsSynced", SPEED_WALK, "Direction"]);

        let words = member_array(&hkx.objects()[2], "wordVariableValues").unwrap();
        let bits = word_value(&words[3]).unwrap() as u32;
        assert_eq!(f32::from_bits(bits), RUN_THRESHOLD);
    }

    #[test]
    fn graph_that_already_declares_the_run_threshold_is_untouched() {
        let mut hkx = graph(&[SPEED_WALK, SPEED_RUN]);
        assert!(!declare_run_threshold(&mut hkx).unwrap());
        assert_eq!(names_of(&hkx).len(), 2);
    }

    /// A graph with no walk threshold is not gait-threshold driven, so half a
    /// contract must not be invented for it.
    #[test]
    fn graph_without_the_walk_threshold_is_left_alone() {
        let mut hkx = graph(&["bIsSynced", "Direction"]);
        assert!(!declare_run_threshold(&mut hkx).unwrap());
        assert_eq!(names_of(&hkx).len(), 2);
    }
}
