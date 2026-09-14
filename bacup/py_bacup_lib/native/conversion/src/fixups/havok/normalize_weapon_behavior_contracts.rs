//! Extend converted actor roots with FO4 shared combat-graph name contracts.
//!
//! FO76 humanoid races select `GunBehavior`; FO4 selects `WeaponBehavior`.
//! After the RACE subgraph paths are normalized, a converted custom root must
//! also declare every event, variable, and character property used by FO4's
//! shared combat and locomotion graphs. Havok does not merge
//! those tables at runtime.
//!
//! The same is true of every wrapping behavior the root mounts, so any
//! converted graph that shares a name with an FO4 shared Character graph
//! adopts that graph's contract as well.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use havok_native::hkx::model::HkxObject;
use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxFile, read_packfile};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

pub struct NormalizeWeaponBehaviorContractsFixup;

pub(crate) fn merge_behavior_contracts(
    root: &mut HkxFile,
    dependency: &HkxFile,
) -> Result<bool, String> {
    merge_contract(root, &read_contract(dependency)?)
}

const WEAPON_CONTRACT_PAIRS: &[(&str, &str)] = &[
    ("GunBehavior.hkx", "WeaponBehavior.hkx"),
    ("MeleeBehavior.hkx", "MeleeBehavior.hkx"),
    ("MTBehavior.hkx", "MTBehavior.hkx"),
    ("BigGunWrappingBehavior.hkx", "WeaponBehavior.hkx"),
    (
        "ShoulderMountedGunWrappingBehavior.hkx",
        "WeaponBehavior.hkx",
    ),
    ("BOSLauncherWrappingBehavior.hkx", "WeaponBehavior.hkx"),
    ("BinocularBehavior.hkx", "WeaponBehavior.hkx"),
    ("BinocularInjuredWrappingBehavior.hkx", "WeaponBehavior.hkx"),
    (
        "NoHandIKGunWrappingBehavior.hkx",
        "NoHandIKWeaponWrappingBehavior.hkx",
    ),
    (
        "NoHandIKRelaxedGunWrappingBehavior.hkx",
        "NoHandIKRelaxedWeaponWrappingBehavior.hkx",
    ),
    (
        "ChargeUpWrappingGunBehavior.hkx",
        "ChargeUpWrappingWeaponBehavior.hkx",
    ),
    (
        "LegInjuredGunWrappingBehavior.hkx",
        "RightArmInjuredWeaponWrappingBehavior.hkx",
    ),
    (
        "LegInjuredGunWrappingBehavior.hkx",
        "LeftArmInjuredWeaponWrappingBehavior.hkx",
    ),
    (
        "LegInjuredGunWrappingBehavior.hkx",
        "BothArmInjuredWeaponWrappingBehavior.hkx",
    ),
];

impl Fixup for NormalizeWeaponBehaviorContractsFixup {
    fn name(&self) -> &'static str {
        "normalize_weapon_behavior_contracts"
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
        normalize_weapon_behavior_contracts_in_mod_path(
            mod_path,
            config.target_extracted_dir.as_deref(),
        )
    }
}

pub fn normalize_weapon_behavior_contracts_in_mod_path(
    mod_path: &Path,
    target_extracted_dir: Option<&Path>,
) -> Result<FixupReport, FixupError> {
    let Some(target_behavior_dir) = target_extracted_dir.and_then(target_character_behavior_dir)
    else {
        return Ok(FixupReport::empty());
    };
    let mut changed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_behavior_dirs(&meshes_root, &mut |behavior_dir| {
            let contract_pairs = contract_pairs_in_dir(behavior_dir, &target_behavior_dir);
            if contract_pairs.is_empty() {
                return;
            }
            for root_path in root_behavior_files(behavior_dir) {
                if normalize_root_file(&root_path, &contract_pairs).unwrap_or(false) {
                    changed += 1;
                }
            }
            changed += adopt_target_contracts(behavior_dir, &target_behavior_dir);
        });
    }
    Ok(FixupReport {
        records_changed: changed,
        ..FixupReport::empty()
    })
}

fn contract_pairs_in_dir(
    behavior_dir: &Path,
    target_behavior_dir: &Path,
) -> Vec<(BehaviorContract, BehaviorContract)> {
    WEAPON_CONTRACT_PAIRS
        .iter()
        .filter_map(|(source_name, target_name)| {
            let source = read_contract_file(&file_named(behavior_dir, source_name)?)?;
            let target = read_contract_file(&file_named(target_behavior_dir, target_name)?)?;
            (!contract_is_empty(&source)).then_some((source, target))
        })
        .collect()
}

fn read_contract_file(path: &Path) -> Option<BehaviorContract> {
    let hkx = read_packfile(&std::fs::read(path).ok()?).ok()?;
    read_contract(&hkx).ok()
}

fn normalize_root_file(
    root_path: &Path,
    contract_pairs: &[(BehaviorContract, BehaviorContract)],
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut root = read_packfile(&std::fs::read(root_path)?)?;
    let original_contract = read_contract(&root).map_err(std::io::Error::other)?;
    let mut changed = false;
    for (source, target) in contract_pairs {
        if contains_contract(&original_contract, source) {
            changed |= merge_contract(&mut root, target).map_err(std::io::Error::other)?;
        }
    }
    if !changed {
        return Ok(false);
    }

    std::fs::write(root_path, root.save())?;
    Ok(true)
}

/// Conform each converted wrapping behavior to its FO4 counterpart's contract.
///
/// FO76 wrapping behaviors declare empty event/variable/character-property
/// tables because FO76 lets a mounted subgraph inherit its parent's tables.
/// FO4 resolves each graph's names against its own tables only, so a wrapper
/// that declares nothing forwards nothing and the wrapped graph never receives
/// locomotion variables or combat events.
///
/// Only wrappers qualify: a core graph such as `MTBehavior.hkx` authors its own
/// tables in both games, so adopting FO4's would bloat it with names it never
/// resolves.
fn adopt_target_contracts(behavior_dir: &Path, target_behavior_dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(behavior_dir) else {
        return 0;
    };
    let mut changed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with("wrappingbehavior.hkx") {
            continue;
        }
        let mut adopted = false;
        for target_path in wrapper_target_files(&name, target_behavior_dir) {
            adopted |= adopt_target_contract(&path, &target_path).unwrap_or(false);
        }
        if adopted {
            changed += 1;
        }
    }
    changed
}

/// FO4's counterparts for a converted wrapper.
///
/// Most wrappers keep their name across games. The FO76-only gun wrappers have
/// no same-named FO4 file, so they fall back to the renamed targets already
/// declared in [`WEAPON_CONTRACT_PAIRS`].
fn wrapper_target_files(name: &str, target_behavior_dir: &Path) -> Vec<PathBuf> {
    if let Some(same_name) = file_named(target_behavior_dir, name) {
        return vec![same_name];
    }
    WEAPON_CONTRACT_PAIRS
        .iter()
        .filter(|(source, _)| source.eq_ignore_ascii_case(name))
        .filter_map(|(_, target)| file_named(target_behavior_dir, target))
        .collect()
}

fn adopt_target_contract(
    path: &Path,
    target_path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(target) = read_contract_file(target_path) else {
        return Ok(false);
    };
    if contract_is_empty(&target) {
        return Ok(false);
    }
    let mut behavior = read_packfile(&std::fs::read(path)?)?;
    if !merge_contract(&mut behavior, &target).map_err(std::io::Error::other)? {
        return Ok(false);
    }
    std::fs::write(path, behavior.save())?;
    Ok(true)
}

#[derive(Clone)]
struct NamedInfo {
    name: String,
    info: HkxValue,
}

#[derive(Clone)]
struct VariableDefinition {
    name: String,
    info: HkxValue,
    initial_word: HkxValue,
    variable_type: i64,
}

struct BehaviorContract {
    events: Vec<NamedInfo>,
    variables: Vec<VariableDefinition>,
    character_properties: Vec<NamedInfo>,
    quad_values: Vec<HkxValue>,
    variant_values: Vec<HkxValue>,
}

fn contract_is_empty(contract: &BehaviorContract) -> bool {
    contract.events.is_empty()
        && contract.variables.is_empty()
        && contract.character_properties.is_empty()
}

#[derive(Clone, Copy)]
struct ContractLayout {
    data: usize,
    strings: usize,
    initial_values: usize,
}

fn read_contract(hkx: &HkxFile) -> Result<BehaviorContract, String> {
    let layout = contract_layout(hkx)?;
    let data = &hkx.objects()[layout.data];
    let strings = &hkx.objects()[layout.strings];
    let initial_values = &hkx.objects()[layout.initial_values];

    let event_names = string_array(strings, "eventNames")?;
    let variable_names = string_array(strings, "variableNames")?;
    let property_names = string_array(strings, "characterPropertyNames")?;
    let event_infos = member_array(data, "eventInfos")?;
    let variable_infos = member_array(data, "variableInfos")?;
    let property_infos = member_array(data, "characterPropertyInfos")?;
    let initial_words = member_array(initial_values, "wordVariableValues")?;

    if event_names.len() != event_infos.len()
        || variable_names.len() != variable_infos.len()
        || variable_names.len() != initial_words.len()
        || property_names.len() != property_infos.len()
    {
        return Err("behavior contract arrays are not aligned".to_string());
    }

    let events = event_names
        .into_iter()
        .zip(event_infos.iter().cloned())
        .map(|(name, info)| NamedInfo { name, info })
        .collect();
    let variables = variable_names
        .into_iter()
        .zip(variable_infos.iter())
        .zip(initial_words.iter())
        .map(|((name, info), initial_word)| {
            Ok(VariableDefinition {
                name,
                info: info.clone(),
                initial_word: initial_word.clone(),
                variable_type: variable_type(info)
                    .ok_or_else(|| "variable info has no numeric type".to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let character_properties = property_names
        .into_iter()
        .zip(property_infos.iter().cloned())
        .map(|(name, info)| NamedInfo { name, info })
        .collect();

    Ok(BehaviorContract {
        events,
        variables,
        character_properties,
        quad_values: member_array(initial_values, "quadVariableValues")?.clone(),
        variant_values: member_array(initial_values, "variantVariableValues")?.clone(),
    })
}

fn contract_layout(hkx: &HkxFile) -> Result<ContractLayout, String> {
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
    Ok(ContractLayout {
        data,
        strings,
        initial_values,
    })
}

fn contains_contract(root: &BehaviorContract, child: &BehaviorContract) -> bool {
    names_contain(
        root.events.iter().map(|entry| entry.name.as_str()),
        child.events.iter().map(|entry| entry.name.as_str()),
    ) && names_contain(
        root.variables.iter().map(|entry| entry.name.as_str()),
        child.variables.iter().map(|entry| entry.name.as_str()),
    ) && names_contain(
        root.character_properties
            .iter()
            .map(|entry| entry.name.as_str()),
        child
            .character_properties
            .iter()
            .map(|entry| entry.name.as_str()),
    )
}

fn names_contain<'a>(
    root: impl Iterator<Item = &'a str>,
    mut child: impl Iterator<Item = &'a str>,
) -> bool {
    let root: HashSet<String> = root.map(str::to_ascii_lowercase).collect();
    child.all(|name| root.contains(&name.to_ascii_lowercase()))
}

fn merge_contract(root: &mut HkxFile, weapon: &BehaviorContract) -> Result<bool, String> {
    let layout = contract_layout(root)?;
    let root_contract = read_contract(root)?;

    let mut event_names: HashSet<String> = root_contract
        .events
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect();
    let events: Vec<NamedInfo> = weapon
        .events
        .iter()
        .filter(|entry| event_names.insert(entry.name.to_ascii_lowercase()))
        .cloned()
        .collect();

    let mut property_names: HashSet<String> = root_contract
        .character_properties
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect();
    let properties: Vec<NamedInfo> = weapon
        .character_properties
        .iter()
        .filter(|entry| property_names.insert(entry.name.to_ascii_lowercase()))
        .cloned()
        .collect();

    let mut variable_names: HashSet<String> = root_contract
        .variables
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect();
    let mut variables = Vec::new();
    let mut appended_quads = Vec::new();
    let mut appended_variants = Vec::new();
    for variable in weapon
        .variables
        .iter()
        .filter(|entry| variable_names.insert(entry.name.to_ascii_lowercase()))
    {
        let mut variable = variable.clone();
        match variable.variable_type {
            5 => {
                let source_index = word_value(&variable.initial_word)
                    .ok_or_else(|| "pointer variable has no initial-value index".to_string())?;
                let source = weapon
                    .variant_values
                    .get(source_index)
                    .ok_or_else(|| "pointer variable initial value is out of range".to_string())?
                    .clone();
                if matches!(source, HkxValue::Pointer(Some(_)) | HkxValue::PendingPtr(_)) {
                    return Err("cannot import a non-null child behavior pointer".to_string());
                }
                let target_index = root_contract.variant_values.len() + appended_variants.len();
                set_word_value(&mut variable.initial_word, target_index)?;
                appended_variants.push(source);
            }
            6..=8 => {
                let source_index = word_value(&variable.initial_word)
                    .ok_or_else(|| "quad variable has no initial-value index".to_string())?;
                let source = weapon
                    .quad_values
                    .get(source_index)
                    .ok_or_else(|| "quad variable initial value is out of range".to_string())?
                    .clone();
                let target_index = root_contract.quad_values.len() + appended_quads.len();
                set_word_value(&mut variable.initial_word, target_index)?;
                appended_quads.push(source);
            }
            _ => {}
        }
        variables.push(variable);
    }

    if events.is_empty() && variables.is_empty() && properties.is_empty() {
        return Ok(false);
    }

    let objects = root.objects_mut();
    for event in events {
        push_string(&mut objects[layout.strings], "eventNames", event.name)?;
        push_array(&mut objects[layout.data], "eventInfos", event.info)?;
    }
    for variable in variables {
        push_string(&mut objects[layout.strings], "variableNames", variable.name)?;
        push_array(&mut objects[layout.data], "variableInfos", variable.info)?;
        push_array(
            &mut objects[layout.initial_values],
            "wordVariableValues",
            variable.initial_word,
        )?;
    }
    for value in appended_quads {
        push_array(
            &mut objects[layout.initial_values],
            "quadVariableValues",
            value,
        )?;
    }
    for value in appended_variants {
        push_array(
            &mut objects[layout.initial_values],
            "variantVariableValues",
            value,
        )?;
    }
    for property in properties {
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
    }
    Ok(true)
}

pub(super) fn pointer_member(object: &HkxObject, name: &str) -> Option<usize> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| match member.value {
            HkxValue::Pointer(index) => index,
            _ => None,
        })
}

pub(super) fn member_array<'a>(
    object: &'a HkxObject,
    name: &str,
) -> Result<&'a Vec<HkxValue>, String> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| match &member.value {
            HkxValue::Array(values) => Some(values),
            _ => None,
        })
        .ok_or_else(|| format!("missing array member {name}"))
}

pub(super) fn string_array(object: &HkxObject, name: &str) -> Result<Vec<String>, String> {
    member_array(object, name)?
        .iter()
        .map(|value| match value {
            HkxValue::String {
                value,
                is_null: false,
            } => Ok(value.clone()),
            _ => Err(format!("{name} contains a non-string value")),
        })
        .collect()
}

pub(super) fn variable_type(info: &HkxValue) -> Option<i64> {
    let members = info.as_object_members()?;
    members
        .iter()
        .find(|member| member.name == "type")
        .and_then(|member| numeric_value(&member.value))
}

pub(super) fn word_value(word: &HkxValue) -> Option<usize> {
    let members = word.as_object_members()?;
    members
        .iter()
        .find(|member| member.name == "value")
        .and_then(|member| numeric_value(&member.value))
        .and_then(|value| usize::try_from(value).ok())
}

pub(super) fn set_word_value(word: &mut HkxValue, value: usize) -> Result<(), String> {
    let members = word
        .as_object_members_mut()
        .ok_or_else(|| "initial word is not an object".to_string())?;
    let member = members
        .iter_mut()
        .find(|member| member.name == "value")
        .ok_or_else(|| "initial word has no value member".to_string())?;
    set_numeric_value(&mut member.value, value)
}

fn numeric_value(value: &HkxValue) -> Option<i64> {
    match value {
        HkxValue::I8(value) => Some(i64::from(*value)),
        HkxValue::U8(value) => Some(i64::from(*value)),
        HkxValue::I16(value) => Some(i64::from(*value)),
        HkxValue::U16(value) => Some(i64::from(*value)),
        HkxValue::I32(value) => Some(i64::from(*value)),
        HkxValue::U32(value) => Some(i64::from(*value)),
        HkxValue::I64(value) => Some(*value),
        HkxValue::U64(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn set_numeric_value(target: &mut HkxValue, value: usize) -> Result<(), String> {
    *target = match target {
        HkxValue::I8(_) => HkxValue::I8(i8::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::U8(_) => HkxValue::U8(u8::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::I16(_) => HkxValue::I16(i16::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::U16(_) => HkxValue::U16(u16::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::I32(_) => HkxValue::I32(i32::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::U32(_) => HkxValue::U32(u32::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::I64(_) => HkxValue::I64(i64::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::U64(_) => HkxValue::U64(u64::try_from(value).map_err(|e| e.to_string())?),
        _ => return Err("initial word value is not numeric".to_string()),
    };
    Ok(())
}

pub(super) fn push_string(
    object: &mut HkxObject,
    member_name: &str,
    value: String,
) -> Result<(), String> {
    push_array(
        object,
        member_name,
        HkxValue::String {
            value,
            is_null: false,
        },
    )
}

pub(super) fn push_array(
    object: &mut HkxObject,
    member_name: &str,
    value: HkxValue,
) -> Result<(), String> {
    let member = object
        .members
        .iter_mut()
        .find(|member| member.name == member_name)
        .ok_or_else(|| format!("missing array member {member_name}"))?;
    let HkxValue::Array(values) = &mut member.value else {
        return Err(format!("{member_name} is not an array"));
    };
    values.push(value);
    Ok(())
}

pub(super) fn mesh_roots_for_mod_path(mod_path: &Path) -> Vec<PathBuf> {
    [
        mod_path.join("data").join("Meshes"),
        mod_path.join("meshes"),
    ]
    .into_iter()
    .filter(|path| path.is_dir())
    .collect()
}

fn target_character_behavior_dir(target_extracted_dir: &Path) -> Option<PathBuf> {
    ["Meshes", "Actors", "Character", "Behaviors"]
        .into_iter()
        .try_fold(target_extracted_dir.to_path_buf(), |dir, name| {
            directory_named(&dir, name)
        })
}

pub(super) fn directory_named(dir: &Path, expected: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        (path.is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(expected)))
        .then_some(path)
    })
}

pub(super) fn walk_behavior_dirs(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Behaviors"))
        {
            visit(&path);
        } else {
            walk_behavior_dirs(&path, visit);
        }
    }
}

pub(super) fn file_named(dir: &Path, expected: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        (path.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(expected)))
        .then_some(path)
    })
}

fn root_behavior_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_root = path.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.to_ascii_lowercase().ends_with("rootbehavior.hkx"));
            is_root.then_some(path)
        })
        .collect()
}

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

    fn contract_file(
        events: &[&str],
        variables: &[(&str, i32, i32)],
        properties: &[&str],
        variants: Vec<HkxValue>,
    ) -> HkxFile {
        let data = HkxObject {
            name: Some("#0001".to_string()),
            offset: 0,
            signature: 0,
            class_name: "hkbBehaviorGraphData".to_string(),
            members: vec![
                member(
                    "variableInfos",
                    HkxValue::Array(
                        variables
                            .iter()
                            .map(|(_, variable_type, _)| info(*variable_type))
                            .collect(),
                    ),
                ),
                member(
                    "characterPropertyInfos",
                    HkxValue::Array(properties.iter().map(|_| info(0)).collect()),
                ),
                member(
                    "eventInfos",
                    HkxValue::Array(events.iter().map(|_| info(0)).collect()),
                ),
                member("variableInitialValues", HkxValue::Pointer(Some(2))),
                member("stringData", HkxValue::Pointer(Some(1))),
            ],
        };
        let string_data = HkxObject {
            name: Some("#0002".to_string()),
            offset: 0,
            signature: 0,
            class_name: "hkbBehaviorGraphStringData".to_string(),
            members: vec![
                member("eventNames", strings(events)),
                member(
                    "variableNames",
                    strings(
                        &variables
                            .iter()
                            .map(|(name, _, _)| *name)
                            .collect::<Vec<_>>(),
                    ),
                ),
                member("characterPropertyNames", strings(properties)),
            ],
        };
        let initial_values = HkxObject {
            name: Some("#0003".to_string()),
            offset: 0,
            signature: 0,
            class_name: "hkbVariableValueSet".to_string(),
            members: vec![
                member(
                    "wordVariableValues",
                    HkxValue::Array(
                        variables
                            .iter()
                            .map(|(_, _, initial)| word(*initial))
                            .collect(),
                    ),
                ),
                member("quadVariableValues", HkxValue::Array(Vec::new())),
                member("variantVariableValues", HkxValue::Array(variants)),
            ],
        };
        HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![data, string_data, initial_values],
        )
    }

    #[test]
    fn appends_weapon_contract_without_reordering_the_existing_root() {
        let gun = contract_file(
            &["sharedEvent", "gunEvent"],
            &[("gunVariable", 0, 0)],
            &["gunProperty"],
            Vec::new(),
        );
        let weapon = contract_file(
            &["sharedEvent", "weaponEvent"],
            &[("weaponVariable", 4, 1065353216), ("GunGripPointer", 5, 0)],
            &["weaponProperty"],
            vec![HkxValue::Pointer(None)],
        );
        let mut root = contract_file(
            &["rootEvent", "sharedEvent", "gunEvent"],
            &[("rootVariable", 3, 7), ("gunVariable", 0, 0)],
            &["rootProperty", "gunProperty"],
            Vec::new(),
        );

        assert!(contains_contract(
            &read_contract(&root).unwrap(),
            &read_contract(&gun).unwrap()
        ));
        assert!(merge_contract(&mut root, &read_contract(&weapon).unwrap()).unwrap());

        let merged = read_contract(&root).unwrap();
        assert_eq!(
            merged
                .events
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["rootEvent", "sharedEvent", "gunEvent", "weaponEvent"]
        );
        assert_eq!(
            merged
                .variables
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            [
                "rootVariable",
                "gunVariable",
                "weaponVariable",
                "GunGripPointer",
            ]
        );
        assert_eq!(word_value(&merged.variables[3].initial_word), Some(0));
        assert_eq!(merged.variant_values, vec![HkxValue::Pointer(None)]);
        assert_eq!(
            merged
                .character_properties
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["rootProperty", "gunProperty", "weaponProperty"]
        );
        assert!(!merge_contract(&mut root, &read_contract(&weapon).unwrap()).unwrap());
    }

    #[test]
    fn unrelated_roots_are_not_recognized_as_gun_mode_roots() {
        let gun = contract_file(
            &["gunEvent"],
            &[("gunVariable", 0, 0)],
            &["gunProperty"],
            Vec::new(),
        );
        let root = contract_file(
            &["rootEvent"],
            &[("rootVariable", 3, 7)],
            &["rootProperty"],
            Vec::new(),
        );

        assert!(!contains_contract(
            &read_contract(&root).unwrap(),
            &read_contract(&gun).unwrap()
        ));
    }

    #[test]
    fn patches_and_roundtrips_a_discovered_actor_root() {
        let tmp = tempfile::tempdir().unwrap();
        let behaviors = tmp.path().join("data/Meshes/Actors/MoleMiner/Behaviors");
        let target_behaviors = tmp.path().join("target/Meshes/Actors/Character/Behaviors");
        std::fs::create_dir_all(&behaviors).unwrap();
        std::fs::create_dir_all(&target_behaviors).unwrap();

        let gun = contract_file(
            &["gunEvent"],
            &[("gunVariable", 0, 0)],
            &["gunProperty"],
            Vec::new(),
        );
        let weapon = contract_file(
            &["weaponEvent"],
            &[("weaponVariable", 0, 1)],
            &["weaponProperty"],
            Vec::new(),
        );
        let melee = contract_file(
            &["meleeEvent"],
            &[("meleeVariable", 0, 0)],
            &["meleeProperty"],
            Vec::new(),
        );
        let shared_melee = contract_file(
            &["sharedMeleeEvent"],
            &[("sharedMeleeVariable", 0, 1)],
            &["sharedMeleeProperty"],
            Vec::new(),
        );
        let mt = contract_file(
            &["mtEvent"],
            &[("mtVariable", 0, 0)],
            &["mtProperty"],
            Vec::new(),
        );
        let shared_mt = contract_file(
            &["sharedMtEvent"],
            &[("sharedMtVariable", 0, 1)],
            &["sharedMtProperty"],
            Vec::new(),
        );
        let no_hand_ik_gun = contract_file(
            &["noHandIkGunEvent"],
            &[("noHandIkGunVariable", 0, 0)],
            &[],
            Vec::new(),
        );
        let no_hand_ik_weapon = contract_file(
            &["noHandIkWeaponEvent"],
            &[("noHandIkWeaponVariable", 0, 1)],
            &[],
            Vec::new(),
        );
        let root = contract_file(
            &["gunEvent", "meleeEvent", "mtEvent", "noHandIkGunEvent"],
            &[
                ("gunVariable", 0, 0),
                ("meleeVariable", 0, 0),
                ("mtVariable", 0, 0),
                ("noHandIkGunVariable", 0, 0),
            ],
            &["gunProperty", "meleeProperty", "mtProperty"],
            Vec::new(),
        );
        std::fs::write(behaviors.join("GunBehavior.hkx"), gun.save()).unwrap();
        std::fs::write(target_behaviors.join("WeaponBehavior.hkx"), weapon.save()).unwrap();
        std::fs::write(behaviors.join("MeleeBehavior.hkx"), melee.save()).unwrap();
        std::fs::write(
            target_behaviors.join("MeleeBehavior.hkx"),
            shared_melee.save(),
        )
        .unwrap();
        std::fs::write(behaviors.join("MTBehavior.hkx"), mt.save()).unwrap();
        std::fs::write(target_behaviors.join("MTBehavior.hkx"), shared_mt.save()).unwrap();
        std::fs::write(
            behaviors.join("NoHandIKGunWrappingBehavior.hkx"),
            no_hand_ik_gun.save(),
        )
        .unwrap();
        std::fs::write(
            target_behaviors.join("NoHandIKWeaponWrappingBehavior.hkx"),
            no_hand_ik_weapon.save(),
        )
        .unwrap();
        let root_path = behaviors.join("MoleMinerRootBehavior.hkx");
        std::fs::write(&root_path, root.save()).unwrap();

        let target = tmp.path().join("target");
        let first =
            normalize_weapon_behavior_contracts_in_mod_path(tmp.path(), Some(&target)).unwrap();
        // The root, plus NoHandIKGunWrappingBehavior adopting its renamed
        // FO4 counterpart's contract.
        assert_eq!(first.records_changed, 2);
        let second =
            normalize_weapon_behavior_contracts_in_mod_path(tmp.path(), Some(&target)).unwrap();
        assert!(second.is_no_op());

        let patched = read_packfile(&std::fs::read(root_path).unwrap()).unwrap();
        let contract = read_contract(&patched).unwrap();
        assert!(
            contract
                .events
                .iter()
                .any(|entry| entry.name == "weaponEvent")
        );
        assert!(
            contract
                .events
                .iter()
                .any(|entry| entry.name == "sharedMeleeEvent")
        );
        assert!(
            contract
                .events
                .iter()
                .any(|entry| entry.name == "sharedMtEvent")
        );
        assert!(
            contract
                .events
                .iter()
                .any(|entry| entry.name == "noHandIkWeaponEvent")
        );
        assert!(
            contract
                .variables
                .iter()
                .any(|entry| entry.name == "weaponVariable")
        );
        assert!(
            contract
                .variables
                .iter()
                .any(|entry| entry.name == "sharedMeleeVariable")
        );
        assert!(
            contract
                .variables
                .iter()
                .any(|entry| entry.name == "sharedMtVariable")
        );
        assert!(
            contract
                .variables
                .iter()
                .any(|entry| entry.name == "noHandIkWeaponVariable")
        );
        assert!(
            contract
                .character_properties
                .iter()
                .any(|entry| entry.name == "weaponProperty")
        );
        assert!(
            contract
                .character_properties
                .iter()
                .any(|entry| entry.name == "sharedMeleeProperty")
        );
        assert!(
            contract
                .character_properties
                .iter()
                .any(|entry| entry.name == "sharedMtProperty")
        );
    }

    #[test]
    fn hollow_wrapping_behaviors_adopt_the_shared_graph_contract() {
        let tmp = tempfile::tempdir().unwrap();
        let behaviors = tmp.path().join("data/Meshes/Actors/MoleMiner/Behaviors");
        let target_behaviors = tmp.path().join("target/Meshes/Actors/Character/Behaviors");
        std::fs::create_dir_all(&behaviors).unwrap();
        std::fs::create_dir_all(&target_behaviors).unwrap();

        std::fs::write(
            behaviors.join("GunBehavior.hkx"),
            contract_file(&["gunEvent"], &[("gunVariable", 0, 0)], &[], Vec::new()).save(),
        )
        .unwrap();
        std::fs::write(
            target_behaviors.join("WeaponBehavior.hkx"),
            contract_file(
                &["weaponEvent"],
                &[("weaponVariable", 0, 1)],
                &[],
                Vec::new(),
            )
            .save(),
        )
        .unwrap();
        std::fs::write(
            behaviors.join("MoleMinerRootBehavior.hkx"),
            contract_file(&["gunEvent"], &[("gunVariable", 0, 0)], &[], Vec::new()).save(),
        )
        .unwrap();

        // FO76 authors this wrapper with empty tables; FO4's same-named graph
        // carries the full locomotion contract it must forward.
        let wrapper = behaviors.join("FastWalk_MTWrappingBehavior.hkx");
        std::fs::write(&wrapper, contract_file(&[], &[], &[], Vec::new()).save()).unwrap();
        std::fs::write(
            target_behaviors.join("FastWalk_MTWrappingBehavior.hkx"),
            contract_file(
                &["mtEvent"],
                &[("Speed", 4, 0), ("Direction", 4, 0)],
                &["UpperBodyFeatheredSpine"],
                Vec::new(),
            )
            .save(),
        )
        .unwrap();

        normalize_weapon_behavior_contracts_in_mod_path(
            tmp.path(),
            Some(&tmp.path().join("target")),
        )
        .unwrap();

        let merged = read_contract_file(&wrapper).unwrap();
        assert_eq!(
            merged
                .variables
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["Speed", "Direction"]
        );
        assert_eq!(
            merged
                .events
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["mtEvent"]
        );
        assert_eq!(
            merged
                .character_properties
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["UpperBodyFeatheredSpine"]
        );
    }

    #[test]
    fn renamed_gun_wrappers_adopt_the_shared_weapon_contract() {
        let tmp = tempfile::tempdir().unwrap();
        let behaviors = tmp.path().join("data/Meshes/Actors/MoleMiner/Behaviors");
        let target_behaviors = tmp.path().join("target/Meshes/Actors/Character/Behaviors");
        std::fs::create_dir_all(&behaviors).unwrap();
        std::fs::create_dir_all(&target_behaviors).unwrap();

        std::fs::write(
            behaviors.join("GunBehavior.hkx"),
            contract_file(&["gunEvent"], &[("gunVariable", 0, 0)], &[], Vec::new()).save(),
        )
        .unwrap();
        std::fs::write(
            target_behaviors.join("WeaponBehavior.hkx"),
            contract_file(
                &["weaponEvent"],
                &[("weaponVariable", 0, 1)],
                &["weaponProperty"],
                Vec::new(),
            )
            .save(),
        )
        .unwrap();

        // FO76-only name: no same-named FO4 file, so it resolves through the
        // rename table to WeaponBehavior.
        let wrapper = behaviors.join("BigGunWrappingBehavior.hkx");
        std::fs::write(&wrapper, contract_file(&[], &[], &[], Vec::new()).save()).unwrap();

        normalize_weapon_behavior_contracts_in_mod_path(
            tmp.path(),
            Some(&tmp.path().join("target")),
        )
        .unwrap();

        let merged = read_contract_file(&wrapper).unwrap();
        assert_eq!(
            merged
                .variables
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["weaponVariable"]
        );
        assert_eq!(
            merged
                .character_properties
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["weaponProperty"]
        );
    }

    #[test]
    fn creature_behaviors_without_a_shared_counterpart_are_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let behaviors = tmp.path().join("data/Meshes/Actors/Snallygaster/Behaviors");
        let target_behaviors = tmp.path().join("target/Meshes/Actors/Character/Behaviors");
        std::fs::create_dir_all(&behaviors).unwrap();
        std::fs::create_dir_all(&target_behaviors).unwrap();

        std::fs::write(
            target_behaviors.join("MTBehavior.hkx"),
            contract_file(&["mtEvent"], &[("Speed", 4, 0)], &[], Vec::new()).save(),
        )
        .unwrap();
        let creature = behaviors.join("MTBehavior.hkx");
        let original = contract_file(&["ownEvent"], &[], &[], Vec::new()).save();
        std::fs::write(&creature, &original).unwrap();

        normalize_weapon_behavior_contracts_in_mod_path(
            tmp.path(),
            Some(&tmp.path().join("target")),
        )
        .unwrap();

        assert_eq!(std::fs::read(&creature).unwrap(), original);
    }
}
