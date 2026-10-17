use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::ControlFlow;
use std::path::Path;

use esp_authoring_core::plugin_runtime::{
    NativePluginSlot, effective_subrecords_for_record, plugin_handle_store_ref,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::run::OwnedPluginHandle;

const QUEST: &str = "HTO_HostileTakeOver_Master_Quest";
const MASTER_SCRIPT: &str = "HostileTakeovers:HTO_MasterScript";
const FACTION_LIST: &str = "HTO_FactionSelection_FormList";
const MUTATION_LIST: &str = "HTO_BossMutation_Valid_FormList";
const REGIONS: &[(&str, &str, &str)] = &[
    (
        "Forest",
        "RegionForestFloodlandsLocation",
        "HTO_LCP_HostileTakeOver_EventToggle_TheForestRegion",
    ),
    (
        "ToxicValley",
        "RegionToxicValleyLocation",
        "HTO_LCP_HostileTakeOver_EventToggle_ToxicValleyRegion",
    ),
    (
        "CranberryBog",
        "RegionCranberryBogLocation",
        "HTO_LCP_HostileTakeOver_EventToggle_CranberryBogRegion",
    ),
    (
        "Mire",
        "RegionSwampForestLocation",
        "HTO_LCP_HostileTakeOver_EventToggle_TheMireRegion",
    ),
    (
        "SavageDivide",
        "RegionMountainLocation",
        "HTO_LCP_HostileTakeOver_EventToggle_SavageDivideRegion",
    ),
    (
        "AshHeap",
        "RegionMTRLocation",
        "HTO_LCP_HostileTakeOver_EventToggle_AshHeapRegion",
    ),
];
const FORMS: &[(&str, &str, &str)] = &[
    (
        "started_message",
        "MESG",
        "HTO_HostileTakeOver_QuestMessage_Started",
    ),
    (
        "ended_message",
        "MESG",
        "HTO_HostileTakeOver_QuestMessage_Ended",
    ),
    (
        "boss_message",
        "MESG",
        "HTO_HostileTakeOver_QuestMessage_BossSpawn",
    ),
    (
        "announce_toggle",
        "GLOB",
        "HTO_LCP_HostileTakeOver_QuestMessage_Toggle",
    ),
    ("boss_explosion", "EXPL", "HTO_crBossSpawnExplosionVFX"),
    ("boss_roar", "SNDR", "HTO_crBossSpawnRoarSFX"),
];
const REWARD: &[(&str, &str, &str)] = &[
    ("xp_global", "GLOB", "HTO_LCP_QuestReward_XP"),
    ("caps_global", "GLOB", "HTO_LCP_QuestReward_Caps"),
    (
        "scrip_global",
        "GLOB",
        "HTO_LCP_QuestReward_LegendaryTokens",
    ),
    ("toggle", "GLOB", "HTO_LCP_QuestReward_Toggle"),
    ("scrip_item", "CNCY", "LegendaryTokens"),
];
const TUNING: &[(&str, &str)] = &[
    (
        "expire_seconds",
        "HTO_LCP_HostileTakeOver_QuestExpireGlobal_5Hrs",
    ),
    (
        "marker_radius",
        "HTO_LCP_HostileTakeOver_TargetRadius_Marker",
    ),
    (
        "location_radius",
        "HTO_LCP_HostileTakeOver_TargetRadius_Location",
    ),
];
const FRACTIONS: &[(&str, &str)] = &[
    ("boss_kill_fraction", "MobKillPercentToSpawnBoss"),
    (
        "remaining_fraction",
        "MobKillPercentToShowRemainingObjective",
    ),
];
const REF_TYPES: &[&str] = &[
    "LocationCenterMarker",
    "MapMarkerRefType",
    "Boss",
    "LocationClearActor",
];

fn fields<'a>(record: &'a Value, name: &'a str) -> impl Iterator<Item = &'a Value> {
    record["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(move |field| field.get(name))
}

fn first<'a>(record: &'a Value, name: &str) -> &'a Value {
    record["fields"]
        .as_array()
        .and_then(|fields| fields.iter().find_map(|field| field.get(name)))
        .unwrap_or(&Value::Null)
}

fn local(value: &Value) -> Option<u32> {
    u32::from_str_radix(value["reference"]["object_id"].as_str()?, 16).ok()
}

fn hex(id: u32) -> String {
    format!("{id:06X}")
}

#[derive(Deserialize)]
struct SourceRecord {
    signature: String,
    editor_id: String,
    value: Value,
}

struct Source {
    records: BTreeMap<u32, SourceRecord>,
    names: HashMap<(String, String), Vec<u32>>,
}

impl Source {
    fn new(
        records: BTreeMap<u32, SourceRecord>,
        names: HashMap<(String, String), Vec<u32>>,
    ) -> Self {
        Self { records, names }
    }

    fn lookup(&self, signature: &str, editor_id: &str) -> Result<(u32, &Value), String> {
        let ids = self
            .names
            .get(&(signature.into(), editor_id.into()))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if ids.len() != 1 {
            return Err(format!(
                "Expected one source {signature} {editor_id}, found {}",
                ids.len()
            ));
        }
        Ok((ids[0], &self.records[&ids[0]].value))
    }

    fn form_list(&self, name: &str) -> Result<Vec<u32>, String> {
        Ok(fields(self.lookup("FLST", name)?.1, "FormID")
            .filter_map(local)
            .filter(|id| *id != 0)
            .collect())
    }

    fn load(slot: &NativePluginSlot) -> Result<Self, String> {
        let mut records = BTreeMap::new();
        let mut names: HashMap<_, Vec<_>> = HashMap::new();
        slot.visit_lazy_records(
            &[
                "QUST", "LCTN", "GLOB", "MESG", "EXPL", "SNDR", "CNCY", "LCRT", "WRLD", "FLST",
                "KYWD", "LVLN",
            ],
            |record| {
                let subrecords = effective_subrecords_for_record(record);
                let editor_id = subrecords
                    .iter()
                    .find(|field| field.signature == "EDID")
                    .map(|field| {
                        encoding_rs::WINDOWS_1252
                            .decode(&field.data)
                            .0
                            .trim_end_matches('\0')
                            .to_owned()
                    })
                    .unwrap_or_default();
                let signature = record.signature.as_str();
                let wanted = match signature {
                    "LCTN" | "KYWD" => true,
                    "QUST" => editor_id == QUEST,
                    "WRLD" => editor_id == "APPALACHIA",
                    "FLST" => matches!(editor_id.as_str(), FACTION_LIST | MUTATION_LIST),
                    "LCRT" => REF_TYPES.contains(&editor_id.as_str()),
                    "LVLN" => {
                        editor_id.starts_with("HTO_LChar_Faction_")
                            || editor_id.starts_with("SDOW_HTO_LChar_Faction_")
                    }
                    "GLOB" => {
                        TUNING.iter().any(|(_, name)| *name == editor_id)
                            || REGIONS.iter().any(|(_, _, name)| *name == editor_id)
                            || FORMS
                                .iter()
                                .chain(REWARD)
                                .any(|(_, sig, name)| *sig == signature && *name == editor_id)
                    }
                    _ => FORMS
                        .iter()
                        .chain(REWARD)
                        .any(|(_, sig, name)| *sig == signature && *name == editor_id),
                };
                if wanted {
                    let mut projected = record.clone();
                    projected.subrecords = subrecords
                        .iter()
                        .filter(|field| match signature {
                            "QUST" => field.signature == "VMAD",
                            "LCTN" => matches!(field.signature.as_str(), "PNAM" | "LCSR"),
                            "GLOB" => field.signature == "FLTV",
                            "FLST" => field.signature == "LNAM",
                            _ => false,
                        })
                        .cloned()
                        .collect();
                    projected.raw_payload = None;
                    let value = if projected.subrecords.is_empty() {
                        Value::Null
                    } else {
                        slot.record_authoring_value(&projected)
                    };
                    let id = record.form_id & 0xFFFFFF;
                    if !editor_id.is_empty() {
                        names
                            .entry((signature.to_owned(), editor_id.clone()))
                            .or_default()
                            .push(id);
                    }
                    records.insert(
                        id,
                        SourceRecord {
                            signature: signature.to_owned(),
                            editor_id,
                            value,
                        },
                    );
                }
                ControlFlow::Continue(())
            },
        )?;
        Ok(Self::new(records, names))
    }
}

#[derive(Clone, Deserialize)]
struct TargetRecord {
    signature: String,
    position: Option<[f64; 3]>,
    has_entries: bool,
}

fn target_record(slot: &NativePluginSlot, id: u32) -> Option<TargetRecord> {
    let raw_id = ((slot.parsed.header.masters.len() as u32) << 24) | id;
    let record = slot
        .lazy_record(raw_id)
        .filter(|record| record.form_id == raw_id)?;
    let fields = effective_subrecords_for_record(&record);
    let position = if matches!(record.signature.as_str(), "REFR" | "ACHR") {
        fields
            .iter()
            .find(|field| field.signature == "DATA")
            .filter(|field| field.data.len() >= 24)
            .and_then(|field| {
                let point = std::array::from_fn(|axis| {
                    f32::from_le_bytes(field.data[axis * 4..axis * 4 + 4].try_into().unwrap())
                        as f64
                });
                point.iter().all(|value| value.is_finite()).then_some(point)
            })
    } else {
        None
    };
    Some(TargetRecord {
        signature: record.signature.to_string(),
        position,
        has_entries: fields.iter().any(|field| field.signature == "LVLO"),
    })
}

fn required(
    converted: &mut impl FnMut(u32) -> Option<TargetRecord>,
    id: u32,
    signature: &str,
) -> Result<TargetRecord, String> {
    converted(id)
        .filter(|record| signature.is_empty() || record.signature == signature)
        .ok_or_else(|| {
            format!(
                "Converted {} {} is missing",
                if signature.is_empty() {
                    "record"
                } else {
                    signature
                },
                hex(id)
            )
        })
}

fn faction_lists(keyword: &str) -> (String, String) {
    let mob = if let Some(name) = keyword.strip_prefix("SDOW_HTO_FactionSelection_") {
        format!(
            "SDOW_HTO_LChar_Faction_{}",
            name.strip_suffix("Shadow").unwrap_or(name)
        )
    } else {
        format!(
            "HTO_LChar_Faction_{}",
            keyword
                .strip_prefix("HTO_FactionSelection_")
                .unwrap_or(keyword)
        )
    };
    let boss = format!("{mob}_Boss");
    (mob, boss)
}

fn build_factions(
    source: &Source,
    converted: &mut impl FnMut(u32) -> Option<TargetRecord>,
) -> Result<Vec<Value>, String> {
    let mut factions = Vec::new();
    for keyword_id in source.form_list(FACTION_LIST)? {
        let keyword = &source
            .records
            .get(&keyword_id)
            .ok_or_else(|| format!("Unable to inspect record {keyword_id:08X}"))?
            .editor_id;
        let (mob_name, boss_name) = faction_lists(keyword);
        let mob = source.lookup("LVLN", &mob_name)?.0;
        let boss = source.lookup("LVLN", &boss_name)?.0;
        for id in [mob, boss] {
            if !required(converted, id, "LVLN")?.has_entries {
                return Err(format!("Converted faction list {} is empty", hex(id)));
            }
        }
        factions.push(
            json!({"key": keyword.rsplit("_FactionSelection_").next().unwrap_or(keyword),
            "keyword": hex(keyword_id), "mob": hex(mob), "boss": hex(boss)}),
        );
    }
    if factions.is_empty() {
        return Err("The FO76 Infestation faction list is empty".into());
    }
    Ok(factions)
}

fn build_sites(
    source: &Source,
    converted: &mut impl FnMut(u32) -> Option<TargetRecord>,
    regions: &BTreeSet<u32>,
    world: u32,
) -> Result<(Vec<Value>, BTreeMap<&'static str, usize>), String> {
    let types = REF_TYPES
        .iter()
        .map(|name| source.lookup("LCRT", name).map(|row| row.0))
        .collect::<Result<Vec<_>, _>>()?;
    let mut sites = Vec::new();
    let mut skipped = BTreeMap::new();
    for (&id, record) in &source.records {
        if record.signature != "LCTN" {
            continue;
        }
        let mut region = Some(id);
        let mut found_region = false;
        for _ in 0..24 {
            match region {
                None => break,
                Some(parent) if regions.contains(&parent) => {
                    found_region = true;
                    break;
                }
                Some(parent) => {
                    region = source
                        .records
                        .get(&parent)
                        .filter(|record| record.signature == "LCTN")
                        .and_then(|record| local(first(&record.value, "ParentLocation")))
                }
            }
        }
        if !found_region || region == Some(id) {
            continue;
        }
        let mut by_type: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut worlds = BTreeSet::new();
        for row in first(&record.value, "MasterSpecialReferences")
            .as_array()
            .into_iter()
            .flatten()
        {
            worlds.insert(local(&row["MasterSpecialReferencesWorldCell"]));
            if let (Some(kind), Some(reference)) = (
                local(&row["MasterSpecialReferencesLocRefType"]),
                local(&row["MasterSpecialReferencesRef"]),
            ) {
                if kind != 0 && reference != 0 {
                    by_type.entry(kind).or_default().push(reference);
                }
            }
        }
        let refs: Vec<&[u32]> = types
            .iter()
            .map(|kind| by_type.get(kind).map(Vec::as_slice).unwrap_or(&[]))
            .collect();
        let skip = if worlds != BTreeSet::from([Some(world)]) {
            Some("outside_world")
        } else if refs[0].is_empty() {
            Some("no_center")
        } else if refs[1].is_empty() {
            Some("no_map_marker")
        } else if refs[2].is_empty() {
            Some("no_boss")
        } else if refs[3].len() < 4 {
            Some("few_clear_actors")
        } else {
            None
        };
        if let Some(reason) = skip {
            *skipped.entry(reason).or_default() += 1;
            continue;
        }
        let center = required(converted, id, "LCTN")
            .and_then(|_| required(converted, refs[0][0], ""))
            .ok()
            .and_then(|record| record.position);
        let clear: Vec<_> = refs[3]
            .iter()
            .copied()
            .filter(|id| converted(*id).is_some())
            .collect();
        let boss: Vec<_> = refs[2]
            .iter()
            .copied()
            .filter(|id| converted(*id).is_some())
            .collect();
        if center.is_none() || clear.len() < 4 || boss.is_empty() {
            *skipped.entry("not_converted").or_default() += 1;
            continue;
        }
        let unique = |ids: Vec<u32>| {
            ids.into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(hex)
                .collect::<Vec<_>>()
        };
        sites.push(json!({"location": hex(id), "editor_id": record.editor_id,
            "region": hex(region.unwrap()), "world": hex(world), "center": center.unwrap(),
            "marker": hex(refs[1][0]), "clear": unique(clear), "boss": unique(boss)}));
    }
    if sites.is_empty() {
        return Err("No FO76 location qualifies as an Infestation site".into());
    }
    Ok((sites, skipped))
}

fn resolved(
    source: &Source,
    converted: &mut impl FnMut(u32) -> Option<TargetRecord>,
    table: &[(&str, &str, &str)],
) -> Result<Map<String, Value>, String> {
    let mut output = Map::new();
    for &(key, signature, name) in table {
        let id = source.lookup(signature, name)?.0;
        required(
            converted,
            id,
            if signature == "CNCY" {
                "MISC"
            } else {
                signature
            },
        )?;
        output.insert(key.into(), json!(hex(id)));
    }
    Ok(output)
}

fn build_catalog(
    source: &Source,
    converted: &mut impl FnMut(u32) -> Option<TargetRecord>,
    data_version: &str,
) -> Result<Value, String> {
    let (quest_id, quest) = source.lookup("QUST", QUEST)?;
    required(converted, quest_id, "QUST")?;
    let script = first(quest, "VirtualMachineAdapter")["Scripts"]
        .as_array()
        .and_then(|scripts| {
            scripts
                .iter()
                .find(|script| script["ScriptName"] == MASTER_SCRIPT)
        })
        .ok_or_else(|| format!("{MASTER_SCRIPT} is not attached to {QUEST}"))?;
    let properties: HashMap<_, _> = script["Properties"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| Some((row["propertyName"].as_str()?, &row["Value"])))
        .collect();
    let mut tuning = Map::new();
    for &(key, name) in TUNING {
        let value = first(source.lookup("GLOB", name)?.1, "Value")
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("Global {name} has no value"))?;
        tuning.insert(key.into(), json!(value));
    }
    for &(key, name) in FRACTIONS {
        let value = properties
            .get(name)
            .and_then(|value| value.as_f64())
            .filter(|value| *value > 0.0 && *value <= 1.0)
            .ok_or_else(|| format!("{MASTER_SCRIPT}.{name} is not a kill fraction"))?;
        let rounded: f64 = format!("{value:.6}").parse().unwrap();
        tuning.insert(key.into(), json!(rounded));
    }
    let mut regions = Vec::new();
    let mut region_ids = BTreeSet::new();
    for &(key, location, toggle) in REGIONS {
        let location_id = source.lookup("LCTN", location)?.0;
        let toggle_id = source.lookup("GLOB", toggle)?.0;
        required(converted, location_id, "LCTN")?;
        required(converted, toggle_id, "GLOB")?;
        regions.push(json!({"key": key, "location": hex(location_id), "toggle": hex(toggle_id)}));
        region_ids.insert(location_id);
    }
    let world = source.lookup("WRLD", "APPALACHIA")?.0;
    required(converted, world, "WRLD")?;
    let mutations: Vec<_> = source
        .form_list(MUTATION_LIST)?
        .into_iter()
        .filter(|id| converted(*id).is_some())
        .map(hex)
        .collect();
    let (sites, skipped) = build_sites(source, converted, &region_ids, world)?;
    let mut forms = Map::from_iter([("quest".into(), json!(hex(quest_id)))]);
    forms.extend(resolved(source, converted, FORMS)?);
    let mut reward = resolved(source, converted, REWARD)?;
    reward.insert("boss_rank".into(), json!(4));
    Ok(json!({"version": 1, "plugin": "SeventySix.esm",
        "source": {"quest": hex(quest_id), "data_version": data_version}, "tuning": tuning,
        "forms": forms, "reward": reward, "mutations": mutations,
        "factions": build_factions(source, converted)?, "regions": regions, "sites": sites, "skipped": skipped}))
}

pub fn build(
    source_path: &Path,
    converted_path: &Path,
    data_version: &str,
) -> Result<Value, String> {
    let source =
        OwnedPluginHandle::load_index(source_path, "fo76").map_err(|error| error.to_string())?;
    let converted =
        OwnedPluginHandle::load_index(converted_path, "fo4").map_err(|error| error.to_string())?;
    let store = plugin_handle_store_ref()
        .lock()
        .map_err(|error| error.to_string())?;
    let source_data = Source::load(store.get(&source.id()).ok_or("source plugin closed")?)?;
    let target = store
        .get(&converted.id())
        .ok_or("converted plugin closed")?;
    let mut cache = HashMap::new();
    build_catalog(
        &source_data,
        &mut |id| {
            cache
                .entry(id)
                .or_insert_with(|| target_record(target, id))
                .clone()
        },
        data_version,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Source, BTreeMap<u32, TargetRecord>, Value) {
        let value: Value =
            serde_json::from_str(include_str!("test_fixtures/infestations/catalog.json")).unwrap();
        let records: BTreeMap<u32, SourceRecord> =
            serde_json::from_value(value["records"].clone()).unwrap();
        let mut names: HashMap<_, Vec<_>> = HashMap::new();
        for (id, record) in &records {
            names
                .entry((record.signature.clone(), record.editor_id.clone()))
                .or_default()
                .push(*id);
        }
        (
            Source::new(records, names),
            serde_json::from_value(value["converted"].clone()).unwrap(),
            value["expected"].clone(),
        )
    }

    #[test]
    fn catalog_matches_python_fixture() {
        let (source, converted, expected) = fixture();
        let actual =
            build_catalog(&source, &mut |id| converted.get(&id).cloned(), "1.7.26.13").unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            serde_json::to_string(&actual).unwrap(),
            serde_json::to_string(&expected).unwrap()
        );
    }

    #[test]
    fn location_parent_cycles_and_missing_centers_are_ineligible() {
        let (mut source, converted, _) = fixture();
        let region_ids = BTreeSet::from([0x010000]);
        let site = source.records.get_mut(&0x030000).unwrap();
        site.value["fields"][0]["ParentLocation"]["reference"]["object_id"] = json!("030000");
        assert!(
            build_sites(
                &source,
                &mut |id| converted.get(&id).cloned(),
                &region_ids,
                0x25DA15
            )
            .unwrap_err()
            .contains("No FO76 location")
        );

        let (source, mut converted, _) = fixture();
        converted.get_mut(&0x700000).unwrap().position = None;
        assert!(
            build_sites(
                &source,
                &mut |id| converted.get(&id).cloned(),
                &region_ids,
                0x25DA15
            )
            .unwrap_err()
            .contains("No FO76 location")
        );
    }

    #[test]
    fn actor_threshold_precedes_deduplication_and_marker_need_not_be_converted() {
        let (mut source, mut converted, _) = fixture();
        let site = source.records.get_mut(&0x030000).unwrap();
        let refs = site.value["fields"][1]["MasterSpecialReferences"]
            .as_array_mut()
            .unwrap();
        for row in refs.iter_mut().skip(3) {
            row["MasterSpecialReferencesRef"]["reference"]["object_id"] = json!("70000A");
        }
        converted.remove(&0x700001);
        let result = build_catalog(&source, &mut |id| converted.get(&id).cloned(), "").unwrap();
        assert_eq!(result["sites"][0]["clear"], json!(["70000A"]));
        assert_eq!(result["sites"][0]["marker"], "700001");
    }

    #[test]
    fn missing_quest_wrong_currency_type_and_empty_factions_fail() {
        for (id, signature, entries, message) in [
            (0x865FA8, "", false, "865FA8"),
            (0x89000A, "CNCY", false, "MISC"),
            (0x85A300, "LVLN", false, "empty"),
        ] {
            let (source, mut converted, _) = fixture();
            if signature.is_empty() {
                converted.remove(&id);
            } else {
                let record = converted.get_mut(&id).unwrap();
                record.signature = signature.into();
                record.has_entries = entries;
            }
            assert!(
                build_catalog(&source, &mut |id| converted.get(&id).cloned(), "")
                    .unwrap_err()
                    .contains(message)
            );
        }
    }

    #[test]
    fn invalid_kill_fraction_and_duplicate_source_names_fail() {
        let (mut source, converted, _) = fixture();
        source.records.get_mut(&0x865FA8).unwrap().value["fields"][0]["VirtualMachineAdapter"]["Scripts"]
            [0]["Properties"][0]["Value"] = json!(1.5);
        assert!(
            build_catalog(&source, &mut |id| converted.get(&id).cloned(), "")
                .unwrap_err()
                .contains("kill fraction")
        );
        source
            .names
            .get_mut(&("QUST".into(), QUEST.into()))
            .unwrap()
            .push(0x865FA8);
        assert!(
            build_catalog(&source, &mut |id| converted.get(&id).cloned(), "")
                .unwrap_err()
                .contains("found 2")
        );
    }
}
