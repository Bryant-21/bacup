use std::collections::{BTreeMap, BTreeSet};

use esp_authoring_core::plugin_runtime::ParsedSubrecord;
use serde_json::{Value, json};

use super::{field, fo76_condition_references, key, text, u32_value};
use crate::phase::PhaseError;

pub(super) fn collect(
    containers: &[u32],
    candidates: &BTreeMap<u32, (&str, String)>,
    regular_perks: &BTreeSet<u32>,
    read: impl Fn(u32) -> Option<Vec<ParsedSubrecord>>,
    masters: &[String],
    plugin: &str,
) -> Result<(Value, BTreeSet<u32>), PhaseError> {
    let roots: BTreeSet<_> = candidates
        .iter()
        .filter(|(_, (signature, name))| {
            *signature == "LVLI" && (name.starts_with("LL_Perk_") || name.contains("_LL_Perk_"))
        })
        .map(|(id, _)| *id)
        .filter(|id| has_regular_gate(*id, candidates, regular_perks, &read, &mut BTreeSet::new()))
        .collect();
    let mut bindings = BTreeMap::new();
    let mut pending = BTreeSet::new();
    let mut references = BTreeSet::new();
    for id in containers {
        let Some(fields) = read(*id) else { continue };
        let mut rewards = Vec::new();
        for entry in fields.iter().filter(|entry| entry.signature == "CNTO") {
            if entry.data.len() != 8 { continue }
            let list = u32_value(&entry.data[..4])?;
            if !roots.contains(&list) { continue }
            rewards.push(json!({"list": key(list, masters, plugin)?,
                "count": u32_value(&entry.data[4..])?}));
            pending.insert(list);
        }
        if !rewards.is_empty() {
            bindings.insert(key(*id, masters, plugin)?, rewards);
            references.insert(*id);
        }
    }
    let mut lists = BTreeMap::new();
    while let Some(id) = pending.pop_first() {
        let source_key = key(id, masters, plugin)?;
        if lists.contains_key(&source_key) { continue }
        let Some(fields) = read(id) else { continue };
        references.insert(id);
        for entry in &fields {
            if entry.signature == "LVLO" {
                let child = match entry.data.len() {
                    4 => u32_value(&entry.data)?,
                    12 => u32_value(&entry.data[4..8])?,
                    _ => continue,
                };
                references.insert(child);
                if candidates.get(&child).is_some_and(|(signature, _)| *signature == "LVLI") {
                    pending.insert(child);
                }
            } else if entry.signature == "CTDA" {
                references.extend(fo76_condition_references(&entry.data));
            }
        }
        lists.insert(source_key, json!({"editor_id": field(&fields, "EDID").map(text).unwrap_or_default(),
            "fields": fields.iter().map(|entry| json!({"signature": entry.signature.as_str(),
                "hex": hex::encode(&entry.data)})).collect::<Vec<_>>()}));
    }
    Ok((json!({"containers": bindings, "lists": lists}), references))
}

fn has_regular_gate(
    id: u32,
    candidates: &BTreeMap<u32, (&str, String)>,
    perks: &BTreeSet<u32>,
    read: &impl Fn(u32) -> Option<Vec<ParsedSubrecord>>,
    visited: &mut BTreeSet<u32>,
) -> bool {
    if !visited.insert(id) { return false }
    let Some(fields) = read(id) else { return false };
    for entry in fields {
        if entry.signature == "CTDA" && entry.data.len() >= 32 {
            let function = u16::from_le_bytes(entry.data[8..10].try_into().unwrap());
            let perk = u32::from_le_bytes(entry.data[12..16].try_into().unwrap());
            if function == 448 && perks.contains(&perk) { return true }
        } else if entry.signature == "LVLO" {
            let child = match entry.data.len() {
                4 => u32::from_le_bytes(entry.data[..4].try_into().unwrap()),
                12 => u32::from_le_bytes(entry.data[4..8].try_into().unwrap()),
                _ => continue,
            };
            if candidates.get(&child).is_some_and(|(signature, _)| *signature == "LVLI")
                && has_regular_gate(child, candidates, perks, read, visited) { return true }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(signature: &str, data: &[u8]) -> ParsedSubrecord {
        ParsedSubrecord { signature: signature.into(), data: data.to_vec().into(), semantic_type: None }
    }

    #[test]
    fn retains_removed_container_binding_and_nested_item_identities() {
        let candidates = BTreeMap::from([
            (0x800, ("LVLI", "LL_Perk_Test".into())),
            (0x801, ("LVLI", "TestItems".into())),
        ]);
        let records = BTreeMap::from([
            (0x900, vec![sub("CNTO", &[0, 8, 0, 0, 1, 0, 0, 0])]),
            (0x800, vec![sub("LVLO", &[1, 8, 0, 0]), sub("CTDA", &[
                0,0,0,0, 0,0,128,63, 192,1,0,0, 4,8,0,0, 0,0,0,0, 1,0,0,0, 0,0,0,0, 255,255,255,255])]),
            (0x801, vec![sub("LVLO", &[2, 8, 0, 0])]),
        ]);
        let (data, references) = collect(&[0x900], &candidates, &BTreeSet::from([0x804]),
            |id| records.get(&id).cloned(), &[], "Source.esm").unwrap();
        assert_eq!(data["containers"]["000900:Source.esm"][0]["list"], "000800:Source.esm");
        assert_eq!(data["lists"].as_object().unwrap().len(), 2);
        assert!(references.contains(&0x802));
        assert!(references.contains(&0x900));
        let (excluded, _) = collect(&[0x900], &candidates, &BTreeSet::new(),
            |id| records.get(&id).cloned(), &[], "Source.esm").unwrap();
        assert!(excluded["containers"].as_object().unwrap().is_empty());
    }
}
