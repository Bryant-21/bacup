use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;
use std::path::Path;

use esp_authoring_core::plugin_runtime::{
    effective_subrecords_for_record, plugin_handle_store_ref,
    rehydrate_filtered_strings_for_authoring,
};
use serde_json::{Value, json};

use crate::run::OwnedPluginHandle;

const SOURCE_TYPES: &[&str] = &[
    "None",
    "Primary",
    "Secondary",
    "SideQuest",
    "Server",
    "Daily",
    "PublicEvent",
    "Miscellaneous",
    "Event",
    "DailyOps",
    "Expedition",
    "Module",
    "Caravan",
    "Raid",
];

fn lead_target(object_id: u32) -> Option<&'static str> {
    match object_id {
        0x5E9591 => Some("5EAD3C"),
        0x699466 => Some("72A2A8"),
        0x78DE65 => Some("78DE64"),
        _ => None,
    }
}

fn first<'a>(record: &'a Value, key: &str) -> &'a Value {
    record["fields"]
        .as_array()
        .and_then(|fields| fields.iter().find_map(|field| field.get(key)))
        .unwrap_or(&Value::Null)
}

fn quest_type(value: &Value) -> Result<u8, String> {
    if let Some(name) = value.as_str() {
        if let Some(index) = SOURCE_TYPES.iter().position(|candidate| *candidate == name) {
            return Ok(index as u8);
        }
    }
    let number = if value.is_object() {
        &value["value"]
    } else {
        value
    };
    number
        .as_u64()
        .and_then(|number| u8::try_from(number).ok())
        .ok_or_else(|| format!("Unsupported source quest type: {value}"))
}

fn has_title(value: &Value) -> Result<bool, String> {
    match value {
        Value::Null => Ok(false),
        Value::String(title) => Ok(!title.trim().is_empty()),
        Value::Object(_) => Ok(value["Values"].as_array().is_some_and(|values| {
            values.iter().any(|row| {
                row["String"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty())
            })
        })),
        _ => Err("Invalid source QUST FULL".into()),
    }
}

fn build_catalog(
    records: &[(u32, Value)],
    converted: &[(String, String)],
    source_plugin: &str,
    output_plugin: &str,
    presentation: Value,
) -> Result<Value, String> {
    let mut by_name: HashMap<&str, Vec<&str>> = HashMap::new();
    for (key, editor_id) in converted {
        by_name.entry(editor_id).or_default().push(key);
    }
    let mut quests = Vec::new();
    let mut missing = Vec::new();
    let mut used = HashSet::new();
    for (object_id, record) in records {
        let editor_id = record["eid"].as_str().unwrap_or("");
        let general = first(record, "DATA");
        if !general.is_null() && !general.is_object() {
            return Err(format!("Invalid source QUST DATA: {object_id:06X}"));
        }
        let source_type = general
            .get("QuestType")
            .map(quest_type)
            .transpose()?
            .unwrap_or(0);
        let fallback = format!("{editor_id}fo76");
        let candidates = by_name
            .get(editor_id)
            .or_else(|| by_name.get(fallback.as_str()));
        let Some(candidates) = candidates.filter(|rows| !editor_id.is_empty() && rows.len() == 1)
        else {
            missing.push(format!("{object_id:06X}"));
            continue;
        };
        let key = candidates[0];
        if !used.insert(key) {
            return Err(format!("Duplicate converted quest binding: {key}"));
        }
        let titled = has_title(first(record, "Name"))?;
        let lead = if let Some(target) = lead_target(*object_id) {
            let adapter = first(record, "VirtualMachineAdapter");
            let matches = adapter["Script Fragments"]["Script"]["Properties"]
                .as_array()
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row["Type"] == "Object"
                            && row["Value"]["FormID"]["reference"]["object_id"] == target
                    })
                });
            if !matches {
                return Err(format!("Source lead binding changed: {object_id:06X}"));
            }
            true
        } else {
            false
        };
        let category = if lead {
            "LEADS"
        } else {
            match source_type {
                6 | 8 | 12 => "EVENTS",
                1 | 13 => "MAIN",
                2 | 3 | 5 | 9 => "SIDE",
                _ => "MISC",
            }
        };
        quests.push(json!({
            "form": key, "source": format!("{source_plugin}:{object_id:06X}"),
            "source_type": source_type, "has_title": titled, "category": category,
        }));
    }
    if quests.is_empty() {
        return Err("No converted quests available for the Pip-Boy catalog".into());
    }
    quests.sort_by(|left, right| left["form"].as_str().cmp(&right["form"].as_str()));
    missing.sort();
    Ok(json!({
        "schema_version": 1, "output_plugin": output_plugin,
        "categories": ["MAIN", "SIDE", "MISC", "LEADS", "EVENTS"],
        "presentation": presentation, "quests": quests, "missing_converted": missing,
    }))
}

pub fn build(
    source_path: &Path,
    converted_path: &Path,
    presentation: Value,
) -> Result<Value, String> {
    let source =
        OwnedPluginHandle::load_index(source_path, "fo76").map_err(|error| error.to_string())?;
    let converted =
        OwnedPluginHandle::load_index(converted_path, "fo4").map_err(|error| error.to_string())?;
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|error| error.to_string())?;
    let source_slot = store.get_mut(&source.id()).ok_or("source plugin closed")?;
    rehydrate_filtered_strings_for_authoring(source_slot);
    let source_name = source_slot.parsed.plugin_name.clone();
    let mut records = Vec::new();
    source_slot.visit_lazy_records(&["QUST"], |record| {
        let object_id = record.form_id & 0xFFFFFF;
        let mut projected = record.clone();
        projected.subrecords = effective_subrecords_for_record(record)
            .iter()
            .filter(|field| {
                matches!(field.signature.as_str(), "EDID" | "DATA" | "FULL")
                    || (field.signature == "VMAD" && lead_target(object_id).is_some())
            })
            .cloned()
            .collect();
        projected.raw_payload = None;
        let value = source_slot.record_authoring_value(&projected);
        if value.get("eid").is_some() {
            records.push((object_id, value));
        }
        ControlFlow::Continue(())
    })?;
    let target_slot = store
        .get(&converted.id())
        .ok_or("converted plugin closed")?;
    let output_name = target_slot.parsed.plugin_name.clone();
    let mut converted_rows = Vec::new();
    let mut invalid_master = None;
    target_slot.visit_lazy_records(&["QUST"], |record| {
        let mut projected = record.clone();
        projected.subrecords = effective_subrecords_for_record(record)
            .iter()
            .filter(|field| field.signature == "EDID")
            .cloned()
            .collect();
        projected.raw_payload = None;
        let value = target_slot.record_authoring_value(&projected);
        if let Some(editor_id) = value["eid"].as_str() {
            let index = (record.form_id >> 24) as usize;
            let owners = &target_slot.parsed.header.masters;
            let owner = if index == owners.len() {
                &output_name
            } else if let Some(owner) = owners.get(index) {
                owner
            } else {
                invalid_master = Some(format!(
                    "Invalid master index {index} for {output_name}:{:08X}",
                    record.form_id
                ));
                return ControlFlow::Break(());
            };
            converted_rows.push((
                format!("{owner}:{:06X}", record.form_id & 0xFFFFFF),
                editor_id.to_owned(),
            ));
        }
        ControlFlow::Continue(())
    })?;
    drop(store);
    if let Some(error) = invalid_master {
        return Err(error);
    }
    build_catalog(
        &records,
        &converted_rows,
        &source_name,
        &output_name,
        presentation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_titles_fallbacks_and_missing_match_source_contract() {
        let records: Vec<_> = (0..14).map(|index| (index + 1, json!({
            "eid": format!("Quest{index}"), "fields": [
                {"DATA": {"QuestType": SOURCE_TYPES[index as usize]}},
                {"Name": {"Values": [{"String": if index == 0 { "  " } else { "Title" }}]}},
            ],
        }))).collect();
        let converted: Vec<_> = (0..13)
            .map(|index| {
                (
                    format!("Output.esm:{:06X}", index + 1),
                    format!("Quest{index}fo76"),
                )
            })
            .collect();
        let result =
            build_catalog(&records, &converted, "Source.esm", "Output.esm", json!({})).unwrap();
        assert_eq!(result["missing_converted"], json!(["00000E"]));
        assert_eq!(result["quests"][0]["has_title"], false);
        assert_eq!(result["quests"][1]["category"], "MAIN");
        assert_eq!(result["quests"][2]["category"], "SIDE");
        assert_eq!(result["quests"][6]["category"], "EVENTS");
        assert!(quest_type(&json!(true)).is_err());
        assert!(quest_type(&json!(256)).is_err());
        assert_eq!(quest_type(&json!({"value": 255})).unwrap(), 255);
    }

    #[test]
    fn validates_lead_properties_and_duplicate_bindings() {
        let converted = vec![("Output.esm:000001".into(), "Lead".into())];
        for object_id in [0x5E9591, 0x699466, 0x78DE65] {
            let mut record = json!({"eid": "Lead", "fields": []});
            assert!(
                build_catalog(
                    &[(object_id, record.clone())],
                    &converted,
                    "Source.esm",
                    "Output.esm",
                    json!({})
                )
                .unwrap_err()
                .contains("Source lead binding changed")
            );
            record["fields"] = json!([{"VirtualMachineAdapter": {"Script Fragments": {"Script": {"Properties": [
                {"Type": "Object", "Value": {"FormID": {"reference": {"object_id": lead_target(object_id).unwrap()}}}}
            ]}}}}]);
            let result = build_catalog(
                &[(object_id, record.clone())],
                &converted,
                "Source.esm",
                "Output.esm",
                json!({}),
            )
            .unwrap();
            assert_eq!(result["quests"][0]["category"], "LEADS");
            assert!(
                build_catalog(
                    &[(object_id, record.clone()), (1, record.clone())],
                    &converted,
                    "Source.esm",
                    "Output.esm",
                    json!({})
                )
                .unwrap_err()
                .contains("Duplicate converted quest")
            );
            record["fields"][0]["VirtualMachineAdapter"]["Script Fragments"]["Script"]["Properties"]
                [0]["Value"]["FormID"]["reference"]["object_id"] = json!("001234");
            assert!(
                build_catalog(
                    &[(object_id, record)],
                    &converted,
                    "Source.esm",
                    "Output.esm",
                    json!({})
                )
                .unwrap_err()
                .contains("Source lead binding changed")
            );
        }
    }
}
