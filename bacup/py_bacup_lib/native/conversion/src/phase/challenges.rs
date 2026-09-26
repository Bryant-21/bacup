use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::ops::ControlFlow;
use std::path::Path;
use std::time::Instant;

use esp_authoring_core::plugin_runtime::{
    NativePluginSlot, plugin_handle_store_ref, rehydrate_filtered_strings_for_authoring,
    serialize_record_payload_to_json,
};
use indexmap::IndexMap;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::run::OwnedPluginHandle;

type Fields = BTreeMap<String, Vec<Value>>;
type SourceRecord = (String, String, Fields);
type Groups = Vec<Vec<Value>>;
type LookupKey = (String, String);
type Resolver<'a> = dyn FnMut(&str, &str) -> Option<String> + 'a;

pub struct ChallengesPhase;

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
fn first<'a>(fields: &'a Fields, key: &str) -> &'a Value {
    fields
        .get(key)
        .and_then(|values| values.first())
        .unwrap_or(&Value::Null)
}
fn values<'a>(fields: &'a Fields, key: &str) -> &'a [Value] {
    fields.get(key).map(Vec::as_slice).unwrap_or_default()
}
fn reference_id(value: &Value) -> Option<u32> {
    u32::from_str_radix(value["reference"]["object_id"].as_str()?, 16).ok()
}
fn fields_by_name(record: &Value) -> Fields {
    let mut fields = Fields::new();
    for item in record["fields"].as_array().into_iter().flatten() {
        for (key, value) in item.as_object().into_iter().flatten() {
            fields.entry(key.clone()).or_default().push(value.clone());
        }
    }
    fields
}

trait Reader {
    fn records(&mut self, signature: &str) -> Vec<(u32, String)>;
    fn fields(&mut self, object_id: u32) -> Result<SourceRecord, String>;
}

struct NativeReader<'a> {
    slot: &'a NativePluginSlot,
    cache: HashMap<u32, SourceRecord>,
    rows: HashMap<String, Vec<(u32, String)>>,
}

impl Reader for NativeReader<'_> {
    fn records(&mut self, signature: &str) -> Vec<(u32, String)> {
        if let Some(rows) = self.rows.get(signature) {
            return rows.clone();
        }
        let mut rows = Vec::new();
        self.slot
            .visit_lazy_records(&[signature], |record| {
                let payload = serialize_record_payload_to_json(
                    &record,
                    &self.slot.parsed,
                    self.slot.strings_ref(),
                );
                let eid = text(&payload["eid"]).to_owned();
                rows.push((record.form_id & 0xFFFFFF, eid.clone()));
                self.cache.insert(
                    record.form_id,
                    (signature.to_owned(), eid, fields_by_name(&payload)),
                );
                ControlFlow::Continue(())
            })
            .expect("challenge inputs are index-only");
        self.rows.insert(signature.to_owned(), rows.clone());
        rows
    }

    fn fields(&mut self, object_id: u32) -> Result<SourceRecord, String> {
        if let Some(record) = self.cache.get(&object_id) {
            return Ok(record.clone());
        }
        let record = self
            .slot
            .lazy_record(object_id)
            .ok_or_else(|| format!("unresolved source reference {object_id:06X}"))?;
        let payload =
            serialize_record_payload_to_json(&record, &self.slot.parsed, self.slot.strings_ref());
        let result = (
            record.signature.to_string(),
            text(&payload["eid"]).to_owned(),
            fields_by_name(&payload),
        );
        self.cache.insert(object_id, result.clone());
        Ok(result)
    }
}

fn function_name(index: u64) -> (&'static str, bool) {
    let supported = match index {
        72 => "GetIsID",
        5001 => "GetIsForm",
        560 => "HasKeyword",
        69 => "GetIsRace",
        372 => "IsInList",
        875 => "IsTrueForConditionForm",
        359 => "GetInCurrentLocation",
        8007 => "LocationHierarchyHasKeyword",
        310 => "GetInWorldspace",
        682 => "WornHasKeyword",
        882 => "CHAL_DoesTargetWeaponHaveKeyword",
        71 => "GetInFaction",
        68 => "GetIsClass",
        80 => "GetLevel",
        74 => "GetGlobalValue",
        828 => "GetIsPlayer",
        182 => "GetEquipped",
        854 => "HasActiveMagicEffect",
        699 => "HasMagicEffectKeyword",
        _ => "",
    };
    if !supported.is_empty() {
        return (supported, true);
    }
    (
        match index {
            14 => "GetValue",
            32 => "GetInSameCell",
            214 => "HasMagicEffect",
            264 => "HasSpell",
            639 => "GetWithinDistance",
            737 => "GetCurrentWeatherHasKeyword",
            801 => "IsInWater",
            839 => "IsMemberOfAPlayerTeam",
            871 => "GetNumActiveSpellsWithKeyword",
            884 => "CHAL_IsTargetWorkshopRecipeInCategory",
            902 => "IsPlayerInCampOwned",
            903 => "IsPlayerInCampTeam",
            904 => "CHAL_IsTargetWorkshopRecipe",
            905 => "DoesModApplyKeyword",
            906 => "HasCompletedChallenge",
            923 => "GetTeamType",
            927 => "IsInWorkshopFreeCameraMode",
            931 => "WornInOrOutOfPowerArmorHasKeyword",
            932 => "IsPlayerInBestBuildCamp",
            5004 => "PlayerHasQuest",
            9001 => "IsPlayerInShelterOwned",
            9002 => "IsPlayerInShelter",
            10000 => "IsChallengeTypeDaily",
            10001 => "IsChallengeTypeWeekly",
            10013 => "GetPublicEventHasMutation",
            _ => "",
        },
        false,
    )
}

fn decode_condition(raw: &Value, kind: &str) -> Result<Value, String> {
    let decoded;
    let raw = if let Some(raw_hex) = raw.get("raw_hex") {
        let bytes = hex::decode(text(raw_hex).split_whitespace().collect::<String>())
            .map_err(|_| "invalid CTDA raw_hex")?;
        if bytes.len() != 32 {
            return Err(format!("CTDA length {} instead of 32", bytes.len()));
        }
        let flags = bytes[0];
        let function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
        let parameter = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let parameter = if matches!(function, 80 | 828) {
            json!({"variant":"none","value":{}})
        } else {
            if parameter >> 24 != 0 {
                return Err("raw CTDA references a source master".into());
            }
            json!({"variant":"form","value":{"reference":{"plugin":"SeventySix.esm","object_id":format!("{parameter:06X}")}}})
        };
        decoded = json!({"Type":flags,"Function":function,
            "RunOn":u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
            "ComparisonValue":{"variant":if flags & 4 != 0 {"comparison_value_global"} else {"comparison_value_float"},
                "value":f32::from_le_bytes(bytes[4..8].try_into().unwrap()) as f64},
            "Parameter1":parameter,"Parameter2":{"variant":"none","value":{}}});
        &decoded
    } else {
        raw
    };
    let flags = raw["Type"].as_u64().unwrap_or(0);
    if flags & 4 != 0 || raw["ComparisonValue"]["variant"] != "comparison_value_float" {
        return Err("comparison value is a global".into());
    }
    if flags & 0x1E != 0 {
        return Err(format!("condition flags 0x{flags:02X}"));
    }
    let operator = match flags & 0xE0 {
        0 => "==",
        0x20 => "!=",
        0x40 => ">",
        0x60 => ">=",
        0x80 => "<",
        0xA0 => "<=",
        value => return Err(format!("condition operator 0x{value:02X}")),
    };
    let function = raw["Function"].as_u64().unwrap_or(0);
    let (name, supported) = function_name(function);
    if !supported {
        return Err(format!(
            "unsupported function {}",
            if name.is_empty() {
                format!("function {function}")
            } else {
                name.into()
            }
        ));
    }
    let run_on = &raw["RunOn"];
    let on = if run_on.is_null() || *run_on == 0 || *run_on == "Subject" {
        "subject"
    } else if *run_on == 1
        || *run_on == 13
        || matches!(text(run_on), "Target" | "TargetList" | "Player Teammates")
    {
        "target"
    } else {
        return Err(format!(
            "run-on {}",
            run_on
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| run_on.to_string())
        ));
    };
    if raw["Parameter2"]["variant"] != "none" {
        return Err(format!("{name} second parameter"));
    }
    let parameter = &raw["Parameter1"];
    let reference = if parameter["variant"] == "none" {
        Value::Null
    } else {
        let reference = &parameter["value"]["reference"];
        if !reference.is_object()
            || reference
                .as_object()
                .is_some_and(|object| object.is_empty())
        {
            return Err(format!("{name} parameter is not a form"));
        }
        reference.clone()
    };
    if !matches!(name, "GetLevel" | "GetIsPlayer")
        && u32::from_str_radix(text(&reference["object_id"]), 16).unwrap_or(0) == 0
    {
        return Err(format!("{name} parameter is not a form"));
    }
    let value = raw["ComparisonValue"]["value"]
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or("non-finite condition comparison")?;
    let on = if kind == "kill"
        && matches!(
            name,
            "WornHasKeyword" | "GetEquipped" | "GetLevel" | "CHAL_DoesTargetWeaponHaveKeyword"
        ) {
        "subject"
    } else {
        on
    };
    Ok(json!({"fn":name,"on":on,"op":operator,"value":value,"or":flags & 1 != 0,"param":reference}))
}

fn group_conditions(conditions: Vec<Value>) -> Groups {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    for mut item in conditions {
        let or = item.as_object_mut().unwrap().remove("or") == Some(json!(true));
        current.push(item);
        if !or {
            groups.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn reject_more_conditions(fields: &Fields) -> Result<(), String> {
    fn contains(value: &Value) -> bool {
        match value {
            Value::Object(object) => object.contains_key("CTDA") || object.values().any(contains),
            Value::Array(array) => array.iter().any(contains),
            _ => false,
        }
    }
    if values(fields, "MoreConditions").iter().any(contains) {
        Err("conditions after the NEXT marker".into())
    } else {
        Ok(())
    }
}

fn stat_kind(stat: &str) -> Result<(&str, Option<(&str, &str, &str)>), String> {
    let keyword = match stat {
        "Total Robots Killed" => "ActorTypeRobot",
        "Total Creatures Killed" => "ActorTypeCreature",
        "Total Animals Killed" => "ActorTypeAnimal",
        "Total Insects Killed" => "ActorTypeBug",
        "Scorched Killed" => "ActorTypeScorched",
        "Super Mutants Killed" => "ActorTypeSuperMutant",
        "Feral Ghouls Killed" => "ActorTypeGhoul",
        _ => "",
    };
    if !keyword.is_empty() {
        return Ok(("kill", Some(("HasKeyword", "KYWD", keyword))));
    }
    if stat == "Total Caps Collected" {
        return Ok(("acquire", Some(("GetIsID", "MISC", "Caps001"))));
    }
    Ok((
        match stat {
            "Total Enemies Killed" => "kill",
            "Items Acquired" => "acquire",
            "Item Crafted" | "Weapons Crafted" | "Items Modded" => "craft",
            "Items Crafted or Scrapped" => "craft_or_scrap",
            "Workshop Objects Crafted" => "workshop",
            "Plants Harvested" => "harvest",
            "Locations Discovered" | "Location Changed" => "discover",
            "Items Consumed" => "consume",
            "Quests Completed" => "quest",
            "Level Up" => "level",
            "Locks Picked" => "lock",
            "Computers Hacked" => "hack",
            "Workshops Claimed" => "claim",
            _ => {
                return Err(format!(
                    "unsupported stat '{}'",
                    stat.replace('\\', "\\\\").replace('\'', "\\'")
                ));
            }
        },
        None,
    ))
}

fn parse_challenge(
    eid: &str,
    fields: &Fields,
    globals: &HashMap<u32, f64>,
) -> Result<Value, String> {
    let score = if eid.starts_with("SCORE_") {
        true
    } else if eid.starts_with("Challenge_") {
        false
    } else {
        return Err("outside the challenge pools".into());
    };
    let name_value = first(fields, "Name");
    let name = name_value
        .as_str()
        .or_else(|| {
            name_value["Values"]
                .as_array()?
                .iter()
                .find(|entry| entry["Language"] == "English")
                .and_then(|entry| entry["String"].as_str())
        })
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return Err("no English name".into());
    }
    let frequency = match text(first(fields, "ChallengeFrequency")) {
        "Lifetime" => "lifetime",
        "Daily" => "daily",
        "Weekly" => "weekly",
        _ => "unknown",
    };
    if (!score && frequency != "lifetime") || (score && !matches!(frequency, "daily" | "weekly")) {
        return Err(format!(
            "{frequency} frequency in the {} pool",
            if score { "score" } else { "lifetime" }
        ));
    }
    let category = if score {
        frequency
    } else {
        match text(first(fields, "ChallengeCategory")) {
            "Character" => "character",
            "Survival" => "survival",
            "Combat" => "combat",
            "Social" => "social",
            "World" => "world",
            "SubChallengeUnsorted" => "sub",
            value => {
                return Err(format!(
                    "category {}",
                    if value.is_empty() { "None" } else { value }
                ));
            }
        }
    };
    let stat = text(first(fields, "TrackedStatUsed")).trim();
    let sub_list = reference_id(first(fields, "SubChallengeCompletionList"));
    let kind = if sub_list.is_some() {
        "parent"
    } else {
        stat_kind(stat)?.0
    };
    reject_more_conditions(fields)?;
    let global = reference_id(first(fields, "RequiredCountGlobal")).filter(|id| *id != 0);
    let mut count = match global {
        Some(id) => *globals
            .get(&id)
            .ok_or_else(|| format!("unresolved count global {id:06X}"))?,
        None => 0.0,
    };
    if count < 1.0 {
        count = first(fields, "RequiredCount").as_f64().unwrap_or(0.0);
    }
    if !count.is_finite() || count.fract() != 0.0 {
        return Err("required count is not a finite integer".into());
    }
    if count < 1.0 {
        return Err("required count below 1".into());
    }
    let conditions = values(fields, "CTDA")
        .iter()
        .map(|raw| decode_condition(raw, kind))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(
        json!({"eid":eid,"name":name,"frequency":frequency,"category":category,"count":count as u64,"stat":stat,
        "conditions":group_conditions(conditions),"prerequisites":values(fields,"Challenge").iter().filter_map(reference_id).collect::<Vec<_>>(),"sub_list":sub_list}),
    )
}

fn reward(frequency: &str, count: u64, parent: bool) -> Value {
    let (caps, xp) = match frequency {
        "daily" => (100, 250),
        "weekly" => (300, 750),
        _ if parent => (500, 1500),
        _ if count <= 1 => (25, 50),
        _ if count <= 10 => (50, 100),
        _ if count <= 30 => (100, 250),
        _ if count <= 100 => (200, 500),
        _ => (400, 1000),
    };
    json!({"caps":caps,"xp":xp})
}

struct Builder<'a> {
    reader: &'a mut dyn Reader,
    resolver: &'a mut Resolver<'a>,
    source: &'a str,
    output: &'a str,
    forms: HashMap<(u32, String), Groups>,
    remaps: BTreeSet<(String, String, String, String)>,
}

impl Builder<'_> {
    fn source_id(&self, reference: &Value) -> Result<u32, String> {
        let plugin = text(&reference["plugin"]);
        if !plugin.eq_ignore_ascii_case(self.source) {
            return Err(format!("unresolved source plugin {plugin}"));
        }
        u32::from_str_radix(text(&reference["object_id"]), 16).map_err(|error| error.to_string())
    }

    fn resolve(&mut self, reference: &Value) -> Result<String, String> {
        let id = self.source_id(reference)?;
        let (sig, eid, _) = self.reader.fields(id)?;
        if eid.is_empty() {
            return Err(format!("{sig} reference {id:06X} has no EditorID"));
        }
        let target = (self.resolver)(&sig, &eid)
            .ok_or_else(|| format!("unresolved {sig} reference {id:06X}"))?;
        if !target.starts_with(&format!("{}:", self.output)) {
            self.remaps.insert((
                eid,
                sig,
                format!("{}:{id:06X}", self.source),
                target.clone(),
            ));
        }
        Ok(target)
    }

    fn expand(
        &mut self,
        groups: Groups,
        kind: &str,
        ancestors: &mut Vec<(String, String)>,
    ) -> Result<Groups, String> {
        let mut result = Vec::new();
        for group in groups {
            let mut expanded = Vec::new();
            for item in group {
                if item["fn"] != "IsTrueForConditionForm" {
                    expanded.push(item);
                    continue;
                }
                if ancestors.len() >= 4 {
                    return Err("condition form nesting deeper than 4".into());
                }
                let reference = &item["param"];
                let key = (
                    text(&reference["plugin"]).to_owned(),
                    text(&reference["object_id"]).to_owned(),
                );
                if ancestors.contains(&key) {
                    return Err("condition form cycle".into());
                }
                let negate = match (text(&item["op"]), item["value"].as_f64()) {
                    ("==", Some(1.0)) | ("!=", Some(0.0)) => false,
                    ("==", Some(0.0)) | ("!=", Some(1.0)) => true,
                    _ => return Err("condition form comparison".into()),
                };
                let id = self.source_id(reference)?;
                let cache_key = (id, kind.to_owned());
                if !self.forms.contains_key(&cache_key) {
                    let (sig, _, fields) = self.reader.fields(id)?;
                    if sig != "CNDF" {
                        return Err(format!("condition form reference {id:06X} is a {sig}"));
                    }
                    reject_more_conditions(&fields)?;
                    let decoded = values(&fields, "CTDA")
                        .iter()
                        .map(|raw| decode_condition(raw, kind))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.forms
                        .insert(cache_key.clone(), group_conditions(decoded));
                }
                ancestors.push(key);
                let nested = self.expand(self.forms[&cache_key].clone(), kind, ancestors)?;
                ancestors.pop();
                expanded
                    .push(json!({"fn":"group","on":item["on"],"negate":negate,"groups":nested}));
            }
            result.push(expanded);
        }
        Ok(result)
    }

    fn resolve_groups(&mut self, groups: &mut Value) -> Result<(), String> {
        for group in groups.as_array_mut().unwrap() {
            for item in group.as_array_mut().unwrap() {
                if item["fn"] == "group" {
                    self.resolve_groups(&mut item["groups"])?;
                } else {
                    let reference = item
                        .as_object_mut()
                        .unwrap()
                        .remove("param")
                        .unwrap_or(Value::Null);
                    item["params"] = if reference.is_null() {
                        json!([])
                    } else {
                        json!([self.resolve(&reference)?])
                    };
                }
            }
        }
        Ok(())
    }

    fn source_pack_reward(&mut self, fields: &Fields) -> Result<u64, String> {
        let reference = first(fields, "RewardRecord");
        if reference_id(reference).is_none() {
            return Ok(0);
        }
        let (sig, _, fields) = self
            .reader
            .fields(self.source_id(&reference["reference"])?)?;
        if sig != "GMRW" {
            return Err(format!("unsupported reward record {sig}"));
        }
        let mut packs = 0;
        for item in values(&fields, "RewardedItem") {
            if reference_id(&item["Item"]).is_none() {
                continue;
            }
            let (sig, eid, _) = self
                .reader
                .fields(self.source_id(&item["Item"]["reference"])?)?;
            if sig != "PPAK" {
                continue;
            }
            if eid != "PerkCardPack_Pack001" {
                return Err(format!("unsupported perk pack {eid}"));
            }
            let count = item["Count"]
                .as_u64()
                .filter(|count| *count > 0 && *count <= u32::MAX as u64 - packs)
                .ok_or("unsupported perk pack reward count")?;
            packs += count;
        }
        Ok(packs)
    }
}

fn ids(entry: &Value, field: &str) -> Vec<u32> {
    entry[field]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_u64().map(|id| id as u32))
        .collect()
}

fn prune(entries: &mut IndexMap<u32, Value>, dropped: &mut Vec<Value>) {
    loop {
        let mut parents = HashMap::new();
        for (id, entry) in entries.iter() {
            for sub in ids(entry, "subs") {
                parents.entry(sub).or_insert(*id);
            }
        }
        let mut removed = Vec::new();
        for (id, entry) in entries.iter() {
            let subs = ids(entry, "subs");
            let reason = if let Some(missing) = ids(entry, "prerequisites")
                .iter()
                .find(|p| !entries.contains_key(*p))
            {
                Some(format!("prerequisite {missing:06X} not included"))
            } else if entry["category"] == "sub" && !parents.contains_key(id) {
                Some("sub-challenge without an included parent".into())
            } else if (!subs.is_empty() || !entry["sub_list"].is_null())
                && !subs.iter().any(|sub| entries.contains_key(sub))
            {
                Some("no included sub-challenges".into())
            } else {
                None
            };
            if let Some(reason) = reason {
                removed.push((*id, reason));
            }
        }
        if removed.is_empty() {
            let included = entries.keys().copied().collect::<BTreeSet<_>>();
            for (id, entry) in entries.iter_mut() {
                entry["parent"] = json!(parents.get(id));
                if parents.contains_key(id) {
                    entry["category"] = json!("sub");
                }
                let subs = ids(entry, "subs");
                if !subs.is_empty() {
                    let subs = subs
                        .into_iter()
                        .filter(|sub| included.contains(sub))
                        .collect::<Vec<_>>();
                    entry["count"] = json!(entry["count"].as_u64().unwrap().min(subs.len() as u64));
                    entry["subs"] = json!(subs);
                }
            }
            return;
        }
        for (id, reason) in removed {
            let entry = entries.shift_remove(&id).unwrap();
            dropped.push(json!({"eid":entry["eid"],"reason":reason}));
        }
    }
}

fn build_catalog(
    reader: &mut dyn Reader,
    resolver: &mut Resolver<'_>,
    output: &str,
    source: &str,
    sha256: &str,
) -> Result<(Value, Value), String> {
    let globals = reader
        .records("GLOB")
        .into_iter()
        .map(|(id, _)| {
            let fields = reader.fields(id)?.2;
            Ok((id, first(&fields, "Value").as_f64().unwrap_or(0.0)))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    let mut rows = reader
        .records("CHAL")
        .into_iter()
        .filter(|(_, eid)| eid.starts_with("Challenge_") || eid.starts_with("SCORE_"))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| (&a.1, a.0).cmp(&(&b.1, b.0)));
    let mut source_subs = BTreeSet::new();
    for (id, _) in &rows {
        if let Some(sub_list) =
            reference_id(first(&reader.fields(*id)?.2, "SubChallengeCompletionList"))
        {
            if let Ok((sig, _, fields)) = reader.fields(sub_list) {
                if sig == "FLST" {
                    source_subs.extend(values(&fields, "FormID").iter().filter_map(reference_id));
                }
            }
        }
    }
    let mut builder = Builder {
        reader,
        resolver,
        source,
        output,
        forms: HashMap::new(),
        remaps: BTreeSet::new(),
    };
    let mut entries = IndexMap::new();
    let mut dropped = Vec::new();
    for (id, eid) in &rows {
        let result = (|| -> Result<Value, String> {
            let fields = builder.reader.fields(*id)?.2;
            let mut entry = parse_challenge(eid, &fields, &globals)?;
            entry["perk_card_packs"] = json!(builder.source_pack_reward(&fields)?);
            if source_subs.contains(id) {
                entry["category"] = json!("sub");
            }
            let stat = text(&entry["stat"]).to_owned();
            let sub_list = entry["sub_list"].as_u64().map(|id| id as u32);
            let (kind, implied) = if sub_list.is_some() {
                ("parent", None)
            } else {
                stat_kind(&stat)?
            };
            entry["kind"] = json!(kind);
            let groups: Groups = serde_json::from_value(entry["conditions"].take())
                .map_err(|error| error.to_string())?;
            entry["conditions"] = json!(builder.expand(groups, kind, &mut Vec::new())?);
            builder.resolve_groups(&mut entry["conditions"])?;
            if let Some((function, sig, implied_eid)) = implied {
                let target = (builder.resolver)(sig, implied_eid)
                    .ok_or_else(|| format!("unresolved {sig} {implied_eid}"))?;
                entry["conditions"].as_array_mut().unwrap().push(
                    json!([{"fn":function,"on":"target","op":"==","value":1.0,"params":[target]}]),
                );
            }
            let mut subs = BTreeSet::new();
            if let Some(sub_list) = sub_list {
                let (sig, _, fields) = builder.reader.fields(sub_list)?;
                if sig != "FLST" {
                    return Err(format!("sub-challenge list {sub_list:06X} is a {sig}"));
                }
                subs.extend(values(&fields, "FormID").iter().filter_map(reference_id));
            }
            entry["subs"] = json!(subs);
            Ok(entry)
        })();
        match result {
            Ok(entry) => {
                entries.insert(*id, entry);
            }
            Err(reason) => dropped.push(json!({"eid":eid,"reason":reason})),
        }
    }
    prune(&mut entries, &mut dropped);
    let mut challenges = Vec::new();
    for (id, entry) in entries {
        let mut prerequisites = ids(&entry, "prerequisites");
        prerequisites.sort();
        let subs = ids(&entry, "subs");
        let mut rewards = reward(
            text(&entry["frequency"]),
            entry["count"].as_u64().unwrap(),
            !subs.is_empty(),
        );
        if entry["perk_card_packs"].as_u64().unwrap_or(0) > 0 {
            rewards["perkCardPacks"] = entry["perk_card_packs"].clone();
        }
        challenges.push(json!({"id":format!("{id:06X}"),"eid":entry["eid"],"name":entry["name"],
            "frequency":entry["frequency"],"category":entry["category"],"kind":entry["kind"],"count":entry["count"],"conditions":entry["conditions"],
            "prerequisites":prerequisites.iter().map(|id| format!("{id:06X}")).collect::<Vec<_>>(),
            "parent":entry["parent"].as_u64().map(|id| format!("{id:06X}")),
            "sub_challenges":subs.iter().map(|id| format!("{id:06X}")).collect::<Vec<_>>(),"reward":rewards}));
    }
    challenges.sort_by(|a, b| text(&a["eid"]).cmp(text(&b["eid"])));
    dropped.sort_by(|a, b| text(&a["eid"]).cmp(text(&b["eid"])));
    let categories = [
        ("character", "Character"),
        ("survival", "Survival"),
        ("combat", "Combat"),
        ("social", "Social"),
        ("world", "World"),
    ]
    .into_iter()
    .filter(|(key, _)| challenges.iter().any(|entry| entry["category"] == *key))
    .map(|(key, label)| json!({"key":key,"label":label}))
    .collect::<Vec<_>>();
    let counter = |field: &str| {
        let mut counts = BTreeMap::<String, usize>::new();
        for entry in &challenges {
            *counts
                .entry(
                    entry[field]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| entry[field].to_string()),
                )
                .or_default() += 1;
        }
        counts
    };
    let mut reasons = BTreeMap::<String, usize>::new();
    for row in &dropped {
        let reason = text(&row["reason"]);
        let family = [
            "prerequisite",
            "unresolved",
            "unsupported",
            "category",
            "condition",
        ]
        .into_iter()
        .find(|prefix| reason.starts_with(prefix))
        .unwrap_or(reason);
        *reasons.entry(family.to_owned()).or_default() += 1;
    }
    let report = json!({"schema_version":1,"included":challenges.len(),"dropped":dropped,
        "remapped":builder.remaps.into_iter().map(|(eid,sig,src,dst)| json!({"eid":eid,"signature":sig,"from":src,"to":dst})).collect::<Vec<_>>(),
        "counts":counter("count"),"kinds":counter("kind"),"frequencies":counter("frequency"),"reasons":reasons,
        "perk_card_pack_rewards":challenges.iter().filter(|entry| entry["reward"]["perkCardPacks"].as_u64().unwrap_or(0) > 0)
            .map(|entry| json!({"id":entry["id"],"eid":entry["eid"],"count":entry["reward"]["perkCardPacks"],"owner":if entry["kind"] == "level" {"level_progression"} else {"challenge"}})).collect::<Vec<_>>()});
    Ok((
        json!({"schema_version":1,"source":{"plugin":source,"sha256":sha256,"records":rows.len()},"output_plugin":output,"categories":categories,"challenges":challenges}),
        report,
    ))
}

fn batch_lookup(
    slot: &NativePluginSlot,
    queries: &BTreeSet<LookupKey>,
    found: &mut HashMap<LookupKey, String>,
) -> Result<(), String> {
    let signatures = queries
        .iter()
        .filter(|key| !found.contains_key(*key))
        .map(|key| key.0.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut error = None;
    slot.visit_lazy_records(&signatures, |record| {
        let Some(eid) = record
            .subrecords
            .iter()
            .find(|field| field.signature == "EDID")
        else {
            return ControlFlow::Continue(());
        };
        let eid = encoding_rs::WINDOWS_1252
            .decode(&eid.data)
            .0
            .trim_end_matches('\0')
            .to_owned();
        let key = (record.signature.to_string(), eid);
        if queries.contains(&key) && !found.contains_key(&key) {
            let index = (record.form_id >> 24) as usize;
            let masters = &slot.parsed.header.masters;
            if index > masters.len() {
                error = Some(format!(
                    "Invalid master index {index} for {}:{:08X}",
                    slot.parsed.plugin_name, record.form_id
                ));
                return ControlFlow::Break(());
            }
            let owner = masters.get(index).unwrap_or(&slot.parsed.plugin_name);
            found.insert(key, format!("{owner}:{:06X}", record.form_id & 0xFFFFFF));
        }
        ControlFlow::Continue(())
    })?;
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn write_json(path: &Path, mut value: Value) -> Result<(), String> {
    value.sort_all_objects();
    let pretty = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    let mut ascii = String::with_capacity(pretty.len());
    for ch in pretty.chars() {
        if ch >= '\u{7f}' {
            use std::fmt::Write;
            for unit in ch.encode_utf16(&mut [0; 2]) {
                write!(ascii, "\\u{unit:04x}").unwrap();
            }
        } else {
            ascii.push(ch);
        }
    }
    ascii.push('\n');
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, ascii).map_err(|error| error.to_string())
}

pub fn emit_catalog(
    source: &Path,
    converted: &Path,
    target_data: Option<&Path>,
    catalog_path: &Path,
    report_path: &Path,
    sha256: Option<&str>,
) -> Result<usize, String> {
    let started = Instant::now();
    let source_handle =
        OwnedPluginHandle::load_index(source, "fo76").map_err(|error| error.to_string())?;
    let converted_handle =
        OwnedPluginHandle::load_index(converted, "fo4").map_err(|error| error.to_string())?;
    let masters = {
        let store = plugin_handle_store_ref().lock().unwrap();
        store[&converted_handle.id()].parsed.header.masters.clone()
    };
    let mut lookups = vec![converted_handle];
    if let Some(data) = target_data {
        for master in masters {
            let path = data.join(master);
            if path.is_file() {
                lookups.push(
                    OwnedPluginHandle::load_index(&path, "fo4")
                        .map_err(|error| error.to_string())?,
                );
            }
        }
    }
    let sha256 = match sha256 {
        Some(hash) => hash.to_owned(),
        None => {
            let mut source = std::fs::File::open(source).map_err(|error| error.to_string())?;
            let mut hash = Sha256::new();
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                let count = source
                    .read(&mut buffer)
                    .map_err(|error| error.to_string())?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            format!("{:x}", hash.finalize())
        }
    };
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store.get_mut(&source_handle.id()).unwrap();
    rehydrate_filtered_strings_for_authoring(slot);
    let mut reader = NativeReader {
        slot,
        cache: HashMap::new(),
        rows: HashMap::new(),
    };
    let source_name = source.file_name().unwrap().to_string_lossy();
    let output = converted.file_name().unwrap().to_string_lossy();
    let mut queries = BTreeSet::new();
    let discovery = Instant::now();
    // Successful placeholders discover every potentially needed lookup, including forms
    // behind a missing target. The final pass preserves failure order and pruning.
    build_catalog(
        &mut reader,
        &mut |sig, eid| {
            queries.insert((sig.to_owned(), eid.to_owned()));
            Some("pending:000001".into())
        },
        &output,
        &source_name,
        &sha256,
    )?;
    let discovery_time = discovery.elapsed();
    let lookup = Instant::now();
    let cache = std::mem::take(&mut reader.cache);
    let rows = std::mem::take(&mut reader.rows);
    let mut found = HashMap::new();
    for handle in &lookups {
        batch_lookup(&store[&handle.id()], &queries, &mut found)?;
    }
    let lookup_time = lookup.elapsed();
    let mut reader = NativeReader {
        slot: &store[&source_handle.id()],
        cache,
        rows,
    };
    let (catalog, report) = build_catalog(
        &mut reader,
        &mut |sig, eid| found.get(&(sig.to_owned(), eid.to_owned())).cloned(),
        &output,
        &source_name,
        &sha256,
    )?;
    let included = catalog["challenges"].as_array().unwrap().len();
    drop(store);
    write_json(catalog_path, catalog)?;
    write_json(report_path, report)?;
    eprintln!(
        "[challenge_catalog_timing] total={:.3}s discovery={:.3}s lookup={:.3}s queries={} resolved={} plugins={}",
        started.elapsed().as_secs_f64(),
        discovery_time.as_secs_f64(),
        lookup_time.as_secs_f64(),
        queries.len(),
        found.len(),
        lookups.len()
    );
    Ok(included)
}

impl Phase for ChallengesPhase {
    fn name(&self) -> &'static str {
        "emit_challenge_catalog"
    }
    fn requires_source_plugin(&self) -> bool {
        false
    }
    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let param = |key: &str| {
            ctx.params[key]
                .as_str()
                .ok_or_else(|| PhaseError::BadParams(format!("missing {key}")))
        };
        emit_catalog(
            Path::new(param("source_plugin")?),
            Path::new(param("converted_plugin")?),
            ctx.target_data_dir,
            Path::new(param("catalog_path")?),
            Path::new(param("report_path")?),
            ctx.params["source_sha256"].as_str(),
        )
        .map_err(PhaseError::Internal)?;
        Ok(PhaseReport {
            assets_written: 2,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests;
