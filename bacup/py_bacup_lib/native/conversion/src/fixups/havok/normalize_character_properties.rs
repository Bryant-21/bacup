//! Declare FO4's shared character properties on converted `character.hkx`.
//!
//! Behavior graphs address bone-weight blends and bone indices by name through
//! the character file's `characterPropertyNames`. FO76 humanoid creatures
//! declare far fewer than FO4's shared Character graphs expect (mole miner 27,
//! FO4 raider 59), so those blends resolve to nothing and the actor holds bind
//! pose.
//!
//! A missing pointer property is backed by a populated bone-weight array from
//! the same mask family (see `mask_family`); with no relative it stays
//! undeclared (see `family_mask_slot`). Arrays are never imported or appended:
//! FO4's are sized for FO4's skeleton, and new objects renumber the file's
//! pointers. `records_changed` counts updated `character.hkx` files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use havok_native::hkx::model::HkxObject;
use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxFile, read_packfile};

use crate::fixups::havok::normalize_weapon_behavior_contracts::{
    directory_named, file_named, member_array, mesh_roots_for_mod_path, pointer_member, push_array,
    push_string, string_array, variable_type,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

/// FO4's fullest humanoid character table; the shared Character graphs are
/// authored against it.
const TARGET_CHARACTER_FILE: &str = "RaiderCharacter.hkx";

/// Present only in FO76 humanoid creature graphs — marks the actors that mount
/// FO4's shared Character behaviors after conversion.
const HUMANOID_MARKER_BEHAVIOR: &str = "GunBehavior.hkx";

pub struct NormalizeCharacterPropertiesFixup;

impl Fixup for NormalizeCharacterPropertiesFixup {
    fn name(&self) -> &'static str {
        "normalize_character_properties"
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
        config.mod_path.is_some() && config.target_extracted_dir.is_some()
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
        normalize_character_properties_in_mod_path(mod_path, config.target_extracted_dir.as_deref())
    }
}

pub fn normalize_character_properties_in_mod_path(
    mod_path: &Path,
    target_extracted_dir: Option<&Path>,
) -> Result<FixupReport, FixupError> {
    let Some(target) = target_extracted_dir
        .and_then(target_character_file)
        .and_then(|path| read_properties_file(&path))
    else {
        return Ok(FixupReport::empty());
    };
    let mut changed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_humanoid_character_dirs(&meshes_root, &mut |character_dir| {
            for path in character_files_in_dir(character_dir) {
                if adopt_properties(&path, &target).unwrap_or(false) {
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

fn target_character_file(target_extracted_dir: &Path) -> Option<PathBuf> {
    let dir = ["Meshes", "Actors", "Character", "Characters"]
        .into_iter()
        .try_fold(target_extracted_dir.to_path_buf(), |dir, name| {
            directory_named(&dir, name)
        })?;
    file_named(&dir, TARGET_CHARACTER_FILE)
}

/// Visit `Characters/` directories belonging to a converted FO76 humanoid.
///
/// The sibling `Behaviors/` dir must carry the FO76-only gun graph, which is
/// the same signal the behavior-contract fixup keys on. Creature ports that
/// ship a self-consistent graph stack are left alone.
fn walk_humanoid_character_dirs(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let is_characters = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Characters"));
        if !is_characters {
            walk_humanoid_character_dirs(&path, visit);
            continue;
        }
        let humanoid = path
            .parent()
            .and_then(|parent| directory_named(parent, "Behaviors"))
            .and_then(|behaviors| file_named(&behaviors, HUMANOID_MARKER_BEHAVIOR))
            .is_some();
        if humanoid {
            visit(&path);
        }
    }
}

fn character_files_in_dir(dir: &Path) -> Vec<PathBuf> {
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
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("hkx"))
        })
        .collect()
}

#[derive(Clone)]
struct CharacterProperty {
    name: String,
    info: HkxValue,
    property_type: i64,
    /// The raw `wordVariableValues` entry. For non-pointer properties this is
    /// the value itself (a bone index, or a flag), which the creature adopts
    /// verbatim.
    word: HkxValue,
}

#[derive(Clone, Copy)]
struct PropertyLayout {
    data: usize,
    strings: usize,
    values: usize,
}

fn read_properties_file(path: &Path) -> Option<Vec<CharacterProperty>> {
    let hkx = read_packfile(&std::fs::read(path).ok()?).ok()?;
    read_properties(&hkx).ok()
}

fn property_layout(hkx: &HkxFile) -> Result<PropertyLayout, String> {
    let data = hkx
        .objects()
        .iter()
        .position(|object| object.class_name == "hkbCharacterData")
        .ok_or_else(|| "missing hkbCharacterData".to_string())?;
    let strings = pointer_member(&hkx.objects()[data], "stringData")
        .ok_or_else(|| "missing character stringData".to_string())?;
    let values = pointer_member(&hkx.objects()[data], "characterPropertyValues")
        .ok_or_else(|| "missing characterPropertyValues".to_string())?;
    if strings >= hkx.objects().len() || values >= hkx.objects().len() {
        return Err("character property pointer is out of range".to_string());
    }
    Ok(PropertyLayout {
        data,
        strings,
        values,
    })
}

fn read_properties(hkx: &HkxFile) -> Result<Vec<CharacterProperty>, String> {
    let layout = property_layout(hkx)?;
    let names = string_array(&hkx.objects()[layout.strings], "characterPropertyNames")?;
    let infos = member_array(&hkx.objects()[layout.data], "characterPropertyInfos")?;
    let words = member_array(&hkx.objects()[layout.values], "wordVariableValues")?;
    if names.len() != infos.len() || names.len() != words.len() {
        return Err("character property arrays are not aligned".to_string());
    }
    names
        .into_iter()
        .zip(infos.iter())
        .zip(words.iter())
        .map(|((name, info), word)| {
            Ok(CharacterProperty {
                name,
                info: info.clone(),
                property_type: variable_type(info)
                    .ok_or_else(|| "character property info has no numeric type".to_string())?,
                word: word.clone(),
            })
        })
        .collect()
}

/// Length of the bone-weight array a variant slot points at, if any.
fn bone_weight_len(hkx: &HkxFile, variants: &[HkxValue], slot: usize) -> Option<usize> {
    let index = match variants.get(slot)? {
        HkxValue::Pointer(Some(index)) => *index,
        _ => return None,
    };
    let object = hkx.objects().get(index)?;
    if object.class_name != "hkbBoneWeightArray" {
        return None;
    }
    Some(member_array(object, "boneWeights").ok()?.len())
}

/// Reduce a blend-mask name to the family it belongs to, so a creature's own
/// populated mask can stand in for a variant it never declared.
///
/// FO4 splits one mask into weapon and per-index variants
/// (`UpperBodyFeatheredSpine` -> `...WPN`, `...00` .. `...10`); they cover the
/// same bones, so the base mask is the right stand-in.
fn mask_family(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let trimmed = lower.trim_end_matches(|c: char| c.is_ascii_digit());
    trimmed.strip_suffix("wpn").unwrap_or(trimmed).to_string()
}

/// Variant slot holding a populated mask from the same family as `name`.
///
/// A mask must never be backed by an empty array: the graphs blend through it,
/// and an empty mask yields undefined per-bone weights — which reads in game as
/// the weapon and upper body flying apart. When no relative exists, the property
/// is left undeclared rather than declared wrong.
fn family_mask_slot(
    hkx: &HkxFile,
    layout: PropertyLayout,
    existing: &[CharacterProperty],
    name: &str,
) -> Option<usize> {
    let variants = member_array(&hkx.objects()[layout.values], "variantVariableValues").ok()?;
    let family = mask_family(name);
    existing
        .iter()
        .filter(|property| property.property_type == POINTER_PROPERTY)
        .filter(|property| mask_family(&property.name) == family)
        .find_map(|property| {
            let slot = word_value(&property.word)?;
            bone_weight_len(hkx, variants, slot)
                .filter(|len| *len > 0)
                .map(|_| slot)
        })
}

fn adopt_properties(
    path: &Path,
    target: &[CharacterProperty],
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut hkx = read_packfile(&std::fs::read(path)?)?;
    if !merge_properties(&mut hkx, target).map_err(std::io::Error::other)? {
        return Ok(false);
    }
    std::fs::write(path, hkx.save())?;
    Ok(true)
}

fn merge_properties(hkx: &mut HkxFile, target: &[CharacterProperty]) -> Result<bool, String> {
    let layout = property_layout(hkx)?;
    let existing = read_properties(hkx)?;
    let mut seen: HashSet<String> = existing
        .iter()
        .map(|property| property.name.to_ascii_lowercase())
        .collect();
    let missing: Vec<CharacterProperty> = target
        .iter()
        .filter(|property| seen.insert(property.name.to_ascii_lowercase()))
        .cloned()
        .collect();
    if missing.is_empty() {
        return Ok(false);
    }

    let word_template = word_template(hkx, layout)?;
    let pointer_slots: Vec<Option<usize>> = missing
        .iter()
        .map(|property| family_mask_slot(hkx, layout, &existing, &property.name))
        .collect();

    let objects = hkx.objects_mut();
    let mut added = false;
    for (property, slot) in missing.into_iter().zip(pointer_slots) {
        let word = match property.property_type {
            // A pointer's value indexes the *target's* variant list, so it
            // cannot be copied; point at a populated mask of the same family
            // this character already owns. With no relative to borrow, leave the
            // property undeclared — a wrong mask blends the weapon through
            // undefined per-bone weights and tears the pose apart.
            POINTER_PROPERTY => match slot {
                Some(slot) => set_word(&word_template, slot as i64)?,
                None => continue,
            },
            // Everything else is the value itself — a bone index or a flag.
            // Adopt the target's verbatim: these creatures share FO4's humanoid
            // bone ordering (their own DirectAtWeapon/WeaponGrip indices already
            // match it exactly), and a placeholder here reads as "no such bone",
            // which detaches the weapon from the hand.
            _ => property.word.clone(),
        };
        push_string(
            &mut objects[layout.strings],
            "characterPropertyNames",
            property.name,
        )?;
        push_array(
            &mut objects[layout.data],
            "characterPropertyInfos",
            property.info,
        )?;
        push_array(&mut objects[layout.values], "wordVariableValues", word)?;
        added = true;
    }
    Ok(added)
}

const POINTER_PROPERTY: i64 = 5;

/// A `wordVariableValues` entry cloned from the file's own, so the member
/// layout and numeric width match whatever this packfile version uses.
fn word_template(hkx: &HkxFile, layout: PropertyLayout) -> Result<HkxValue, String> {
    member_array(&hkx.objects()[layout.values], "wordVariableValues")?
        .first()
        .cloned()
        .ok_or_else(|| "character has no word values to copy".to_string())
}

fn word_value(word: &HkxValue) -> Option<usize> {
    let HkxValue::Object(members) = word else {
        return None;
    };
    let member = members.iter().find(|member| member.name == "value")?;
    let value = match member.value {
        HkxValue::I8(value) => value as i64,
        HkxValue::I16(value) => value as i64,
        HkxValue::I32(value) => value as i64,
        HkxValue::I64(value) => value,
        _ => return None,
    };
    usize::try_from(value).ok()
}

fn set_word(template: &HkxValue, value: i64) -> Result<HkxValue, String> {
    let mut word = template.clone();
    let HkxValue::Object(members) = &mut word else {
        return Err("word value is not an object".to_string());
    };
    let member = members
        .iter_mut()
        .find(|member| member.name == "value")
        .ok_or_else(|| "word value has no value member".to_string())?;
    member.value = match member.value {
        HkxValue::I8(_) => HkxValue::I8(value as i8),
        HkxValue::I16(_) => HkxValue::I16(value as i16),
        HkxValue::I64(_) => HkxValue::I64(value),
        _ => HkxValue::I32(value as i32),
    };
    Ok(word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use havok_native::hkx::HkxMember;

    fn member(name: &str, value: HkxValue) -> HkxMember {
        HkxMember {
            name: name.to_string(),
            value,
        }
    }

    fn info(property_type: i32) -> HkxValue {
        HkxValue::Object(vec![member("type", HkxValue::I32(property_type))])
    }

    fn word(value: i32) -> HkxValue {
        HkxValue::Object(vec![member("value", HkxValue::I32(value))])
    }

    fn strings(values: &[&str]) -> HkxValue {
        HkxValue::Array(
            values
                .iter()
                .map(|value| HkxValue::String {
                    value: (*value).to_string(),
                    is_null: false,
                })
                .collect(),
        )
    }

    fn bone_weight_array(name: &str, len: usize) -> HkxObject {
        HkxObject {
            name: Some(name.to_string()),
            offset: 0,
            signature: 0,
            class_name: "hkbBoneWeightArray".to_string(),
            members: vec![member(
                "boneWeights",
                HkxValue::Array(vec![HkxValue::I32(1); len]),
            )],
        }
    }

    /// `#0001` data, `#0002` strings, `#0003` values, `#0004` populated array,
    /// `#0005` empty array.
    fn character_file(properties: &[(&str, i32, i32)]) -> HkxFile {
        let data = HkxObject {
            name: Some("#0001".to_string()),
            offset: 0,
            signature: 0,
            class_name: "hkbCharacterData".to_string(),
            members: vec![
                member(
                    "characterPropertyInfos",
                    HkxValue::Array(properties.iter().map(|(_, t, _)| info(*t)).collect()),
                ),
                member("characterPropertyValues", HkxValue::Pointer(Some(2))),
                member("stringData", HkxValue::Pointer(Some(1))),
            ],
        };
        let string_data = HkxObject {
            name: Some("#0002".to_string()),
            offset: 0,
            signature: 0,
            class_name: "hkbCharacterStringData".to_string(),
            members: vec![member(
                "characterPropertyNames",
                strings(&properties.iter().map(|(n, _, _)| *n).collect::<Vec<_>>()),
            )],
        };
        let values = HkxObject {
            name: Some("#0003".to_string()),
            offset: 0,
            signature: 0,
            class_name: "hkbVariableValueSet".to_string(),
            members: vec![
                member(
                    "wordVariableValues",
                    HkxValue::Array(properties.iter().map(|(_, _, w)| word(*w)).collect()),
                ),
                member(
                    "variantVariableValues",
                    HkxValue::Array(vec![HkxValue::Pointer(Some(3)), HkxValue::Pointer(Some(4))]),
                ),
            ],
        };
        HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![
                data,
                string_data,
                values,
                bone_weight_array("#0004", 93),
                bone_weight_array("#0005", 0),
            ],
        )
    }

    #[test]
    fn missing_properties_borrow_a_populated_mask_of_their_own_family() {
        let target = read_properties(&character_file(&[
            ("UpperBodyOnly", 5, 0),
            ("UpperBodyOnlyWPN", 5, 0),
            ("WeaponBonesOnly", 5, 0),
            ("WeaponBoneIndex", 2, 7),
        ]))
        .unwrap();
        let mut creature = character_file(&[("UpperBodyOnly", 5, 0)]);

        assert!(merge_properties(&mut creature, &target).unwrap());

        let merged = read_properties(&creature).unwrap();
        assert_eq!(
            merged
                .iter()
                .map(|property| property.name.as_str())
                .collect::<Vec<_>>(),
            // `WeaponBonesOnly` has no relative to borrow, so it stays
            // undeclared rather than pointing at an empty mask.
            ["UpperBodyOnly", "UpperBodyOnlyWPN", "WeaponBoneIndex"]
        );

        let layout = property_layout(&creature).unwrap();
        let words = member_array(&creature.objects()[layout.values], "wordVariableValues").unwrap();
        // The weapon variant blends through the populated mask at slot 0, not
        // the empty array at slot 1 — an empty mask tears the pose apart.
        assert_eq!(words[1], word(0));
        // A bone index is adopted verbatim from the target. Writing a
        // placeholder here would read as "no such bone" and detach the weapon
        // from the hand.
        assert_eq!(words[2], word(7));
        // No objects were appended, so every existing pointer still resolves.
        assert_eq!(creature.objects().len(), 5);
        assert!(!merge_properties(&mut creature, &target).unwrap());
    }

    #[test]
    fn a_mask_with_no_populated_relative_is_left_undeclared() {
        let target = read_properties(&character_file(&[
            ("LowerBodyOnly", 5, 0),
            ("LowerBodyOnlyWPN", 5, 0),
        ]))
        .unwrap();
        // This creature's own `LowerBodyOnly` points at the empty array, so it
        // is not a usable stand-in for the weapon variant.
        let mut creature = character_file(&[("LowerBodyOnly", 5, 1)]);

        assert!(!merge_properties(&mut creature, &target).unwrap());
    }

    /// `target_extracted_dir` must stay the extracted root. A run once replaced
    /// it with a single-asset-class membership tree, which silently disabled
    /// every fixup that resolves real base-game paths under it — they reported
    /// `changed=0` and looked like they had nothing to do.
    #[test]
    fn a_membership_tree_is_not_a_usable_extracted_root() {
        let tmp = tempfile::tempdir().unwrap();
        let membership_root = tmp.path().join("membership/textures/effects/gobos");
        std::fs::create_dir_all(&membership_root).unwrap();

        assert!(target_character_file(&tmp.path().join("membership")).is_none());

        let extracted = tmp.path().join("extracted");
        let characters = extracted.join("Meshes/Actors/Character/Characters");
        std::fs::create_dir_all(&characters).unwrap();
        std::fs::write(characters.join(TARGET_CHARACTER_FILE), b"").unwrap();
        assert!(target_character_file(&extracted).is_some());
    }

    #[test]
    fn existing_properties_keep_their_index_and_value() {
        let target = read_properties(&character_file(&[
            ("UpperBodyOnly00", 5, 0),
            ("UpperBodyOnly", 5, 0),
        ]))
        .unwrap();
        let mut creature = character_file(&[("UpperBodyOnly", 5, 0)]);

        assert!(merge_properties(&mut creature, &target).unwrap());

        let merged = read_properties(&creature).unwrap();
        assert_eq!(merged[0].name, "UpperBodyOnly");
        let layout = property_layout(&creature).unwrap();
        let words = member_array(&creature.objects()[layout.values], "wordVariableValues").unwrap();
        assert_eq!(words[0], word(0));
    }
}
