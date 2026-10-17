use std::collections::{BTreeSet, HashMap};
use std::ops::ControlFlow;
use std::path::Path;

use esp_authoring_core::plugin_runtime::{
    plugin_handle_store_ref, rehydrate_filtered_strings_for_authoring,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::run::OwnedPluginHandle;

pub const SIGNATURES: [&str; 8] = [
    "ALCH", "AMMO", "ARMO", "BOOK", "KEYM", "MISC", "NOTE", "WEAP",
];
const HUMAN_RACE: &str = "013746:Fallout4.esm";
pub const KEYCARD: &str = "props/generickeycard01.nif";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Item {
    pub form: String,
    pub signature: String,
    pub editor_id: String,
    pub name: String,
    pub model: String,
    pub plugin: String,
    pub playable: bool,
    #[serde(default)]
    pub thrown: bool,
    #[serde(default)]
    pub addons: Vec<String>,
    #[serde(default)]
    pub worn: Vec<String>,
}

pub fn normalized_model(path: &str) -> String {
    let path = path
        .replace('\\', "/")
        .trim()
        .trim_matches('/')
        .to_lowercase();
    path.strip_prefix("meshes/").unwrap_or(&path).into()
}

fn values<'a>(node: &'a Value, key: &str, output: &mut Vec<&'a Value>) {
    match node {
        Value::Object(fields) => {
            for (name, value) in fields {
                if name == key {
                    output.push(value);
                }
                values(value, key, output);
            }
        }
        Value::Array(fields) => {
            for value in fields {
                values(value, key, output);
            }
        }
        _ => (),
    }
}

fn all<'a>(node: &'a Value, key: &str) -> Vec<&'a Value> {
    let mut output = Vec::new();
    values(node, key, &mut output);
    output
}

fn english(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value["Value"].as_str())
        .or_else(|| {
            value["Values"]
                .as_array()?
                .iter()
                .find(|v| v["Language"] == "English")?["String"]
                .as_str()
        })
}

fn reference(value: &Value) -> Option<String> {
    let reference = value.get("reference")?.as_object()?;
    Some(format!(
        "{}:{}",
        reference.get("object_id")?.as_str()?,
        reference.get("plugin")?.as_str()?
    ))
}

fn model(fields: &Value, keys: &[&str]) -> String {
    normalized_model(
        keys.iter()
            .flat_map(|key| all(fields, key))
            .filter_map(Value::as_str)
            .find(|value| !value.is_empty())
            .unwrap_or(""),
    )
}

pub fn item(raw: u32, record: &Value, signature: &str, plugin: &str, masters: &[String]) -> Item {
    let fields = &record["fields"];
    let name = ["FULL", "Name"]
        .iter()
        .flat_map(|key| all(fields, key))
        .filter_map(english)
        .find(|value| !value.is_empty())
        .unwrap_or("")
        .into();
    let record_flags = u32::from_str_radix(
        record["flags"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or("0")
            .trim_start_matches("0x"),
        16,
    )
    .unwrap_or(0);
    let blocked = all(fields, "Flags")
        .into_iter()
        .filter_map(Value::as_array)
        .flatten()
        .any(|value| matches!(value.as_str(), Some("NotPlayable" | "NonPlayable")));
    let thrown = signature == "WEAP"
        && (all(fields, "AnimationType")
            .iter()
            .any(|v| matches!(v.as_str(), Some("Grenade" | "Mine")))
            || all(fields, "Keywords")
                .into_iter()
                .filter_map(Value::as_array)
                .flatten()
                .filter_map(reference)
                .any(|form| {
                    matches!(form.as_str(), "10C415:Fallout4.esm" | "10C414:Fallout4.esm")
                }));
    let body = signature == "ARMO"
        && all(fields, "BipedBodyTemplate")
            .into_iter()
            .filter_map(Value::as_array)
            .any(|values| values.iter().any(|v| v == "33BODY"));
    Item {
        form: format!(
            "{:06X}:{}",
            raw & 0xFFFFFF,
            masters
                .get((raw >> 24) as usize)
                .map(String::as_str)
                .unwrap_or(plugin)
        ),
        signature: signature.into(),
        editor_id: record["eid"].as_str().unwrap_or("").into(),
        name,
        model: model(fields, &["MODL", "ModelFileName", "MOD2", "MOD4"]),
        plugin: plugin.into(),
        playable: !(blocked
            || record_flags & 4 != 0 && matches!(signature, "ARMO" | "AMMO" | "WEAP")),
        thrown,
        addons: if body {
            all(fields, "ArmorAddon")
                .into_iter()
                .filter_map(reference)
                .collect()
        } else {
            Vec::new()
        },
        worn: Vec::new(),
    }
}

pub fn addon(record: &Value) -> (String, String) {
    let fields = &record["fields"];
    (
        all(fields, "RNAM")
            .into_iter()
            .find_map(reference)
            .unwrap_or_default(),
        model(fields, &["MOD2", "MOD3"]),
    )
}

pub fn worn_items(items: &mut [Item], addons: &HashMap<String, (String, String)>) {
    for item in items {
        let worn: BTreeSet<_> = item
            .addons
            .iter()
            .filter_map(|form| addons.get(form))
            .filter(|(race, model)| race == HUMAN_RACE && !model.is_empty())
            .map(|(_, model)| model.clone())
            .collect();
        if !worn.is_empty() {
            item.worn = worn.into_iter().collect();
        }
    }
}

pub fn icon_model(item: &Item) -> Option<String> {
    let model = item.model.as_str();
    if item.signature == "AMMO"
        && [
            "ammo/10mm/10mmammo.nif",
            "ammo/38/38ammo.nif",
            "ammo/45cal/45ammo.nif",
            "dlc04/ammo/762ammo.nif",
            "ammo/308/308ammo.nif",
            "ammo/44/44ammo.nif",
            "ammo/50cal/50calammo.nif",
            "ammo/556/556ammo.nif",
            "ammo/shotgunshell/shotgunshellammo.nif",
            "ammo/syringer/syringerammo.nif",
            "dlc03/ammo/4570ammo.nif",
        ]
        .contains(&model)
    {
        return None;
    }
    if matches!(item.signature.as_str(), "BOOK" | "NOTE") {
        return (model.starts_with("props/transceiverobject/")
            || model.starts_with("setdressing/kitchen/beerbottle_message.nif"))
        .then(|| model.into());
    }
    if item.signature == "KEYM" {
        if [
            "keycard",
            "idcard",
            "governmentid",
            "government_id",
            "hornwrightid",
            "congressionalid",
        ]
        .iter()
        .any(|text| model.contains(text))
        {
            return Some(KEYCARD.into());
        }
        if ["holotape", "note", "paper"]
            .iter()
            .any(|text| model.contains(text))
        {
            return None;
        }
    }
    Some(if item.worn.is_empty() {
        model.into()
    } else {
        item.worn.join("|")
    })
}

pub fn collect(plugins: &[(String, Option<String>)]) -> Result<Vec<Item>, String> {
    let mut items: IndexMap<String, Item> = IndexMap::new();
    let mut addons = HashMap::new();
    for (path, strings) in plugins {
        let handle = OwnedPluginHandle::load_index_with_strings(
            Path::new(path),
            "fo4",
            strings.as_deref(),
            Some("en"),
        )
        .map_err(|e| e.to_string())?;
        let mut store = plugin_handle_store_ref()
            .lock()
            .map_err(|e| e.to_string())?;
        let slot = store.get_mut(&handle.id()).ok_or("icon plugin closed")?;
        rehydrate_filtered_strings_for_authoring(slot);
        let plugin = slot.parsed.plugin_name.clone();
        let masters = slot.parsed.header.masters.clone();
        let signatures: Vec<_> = SIGNATURES.into_iter().chain(["ARMA"]).collect();
        slot.visit_lazy_records(&signatures, |record| {
            let signature = record.signature.as_str();
            let value = slot.record_authoring_value(record);
            if signature == "ARMA" {
                let form = format!(
                    "{:06X}:{}",
                    record.form_id & 0xFFFFFF,
                    masters
                        .get((record.form_id >> 24) as usize)
                        .unwrap_or(&plugin)
                );
                addons.insert(form, addon(&value));
            } else {
                let mut next = item(record.form_id, &value, signature, &plugin, &masters);
                if next.name.is_empty() {
                    if let Some(previous) = items.get(&next.form) {
                        next.name.clone_from(&previous.name);
                    }
                }
                items.insert(next.form.clone(), next);
            }
            ControlFlow::Continue(())
        })?;
        drop(store);
    }
    let mut items: Vec<_> = items
        .into_values()
        .filter(|item| {
            item.playable
                && !item.name.is_empty()
                && !item.model.is_empty()
                && !item.model.starts_with("actors/powerarmor/")
        })
        .collect();
    worn_items(&mut items, &addons);
    Ok(items)
}
