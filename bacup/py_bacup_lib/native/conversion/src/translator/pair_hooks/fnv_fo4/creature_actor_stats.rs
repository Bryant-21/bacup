use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::run::ConversionRun;
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::translator::Game;

use super::creature_catalog::{LegacyCreatureGame, StableFormKey};
use super::creature_templates::{LegacyTemplatePlan, LegacyTemplateProfile, resolve_template_plan};

#[derive(Serialize)]
pub(crate) struct LegacyActorStatsOutput {
    version: u32,
    source_game: String,
    actor_level: String,
    actors: Vec<ActorStats>,
    #[serde(skip)]
    plugin: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct ActorStats {
    npc: String,
    race: String,
    health: u16,
    pc_level_mult: bool,
    level: u16,
    min_level: u16,
    max_level: u16,
    special: [u8; 7],
    ap_base: f32,
    ap_mult: f32,
}

impl LegacyActorStatsOutput {
    pub(crate) fn write(&self, mod_path: &Path) -> Result<(), String> {
        let directory = mod_path.join("F4SE/Plugins/B21_ConversionRuntime/LegacyStats");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        std::fs::write(
            directory.join(format!("{}.json", self.plugin)),
            serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }
}

pub(crate) fn prepare(run: &mut ConversionRun) -> Result<Option<LegacyActorStatsOutput>, String> {
    let game = match (run.source, run.target) {
        (Game::Fnv, Game::Fo4) => LegacyCreatureGame::Fnv,
        (Game::Fo3, Game::Fo4) => LegacyCreatureGame::Fo3,
        _ => return Ok(None),
    };
    if run.source_handle().is_none() {
        return Ok(None);
    }
    let mut owned = BTreeMap::new();
    for signature in [*b"CREA", *b"LVLC"] {
        for key in iter_form_keys_of_sig(run.source_handle_id, SigCode(signature), &run.interner)
            .map_err(|error| error.to_string())?
        {
            let record = read_record_relayout_by_form_key(
                run.source_handle_id,
                &key,
                &run.schema_source,
                &run.interner,
                None,
            )
            .map_err(|error| error.to_string())?;
            owned.insert(stable(key, &run.interner), record);
        }
    }
    let records = owned
        .iter()
        .map(|(key, record)| (key.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mappings = &run
        .mapper_state
        .as_ref()
        .ok_or("legacy actor stats mapper is unavailable")?
        .source_to_target;
    let mut pairs = Vec::new();
    for (key, source) in records
        .iter()
        .filter(|(_, record)| record.sig.0 == *b"CREA")
    {
        let Some(target) = mappings.get(&source.form_key) else {
            continue;
        };
        let Some(npc) = run
            .read_target_record_if_available(*target)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        if npc.sig.0 != *b"NPC_" {
            continue;
        }
        let Some(race) = reference(&npc, b"RNAM") else {
            continue;
        };
        if !is_output_key(run, race) {
            continue;
        }
        let plan = resolve_template_plan(key, game, &records, &run.interner)?;
        match &plan {
            LegacyTemplatePlan::Creature(profile) => pairs.push((profile.clone(), npc)),
            LegacyTemplatePlan::Leveled { .. } => {
                let list = reference(&npc, b"TPLT")
                    .ok_or("converted legacy template wrapper has no TPLT")?;
                collect_selected_profiles(&plan, list, &|key| emitted(run, key), &mut pairs)?;
            }
        }
    }
    if pairs.is_empty() {
        return Ok(None);
    }
    let settings = super::creature_magic_runtime::source_settings(run)?;
    for name in ["fAVDActionPointsBase", "fAVDActionPointsMult"] {
        if !settings
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case(name))
        {
            return Err(format!(
                "legacy actor stats require the winning source {name}"
            ));
        }
    }
    let ap_base = super::creature_magic_runtime::setting(&settings, "fAVDActionPointsBase", 65.0)?;
    let ap_mult = super::creature_magic_runtime::setting(&settings, "fAVDActionPointsMult", 3.0)?;
    let mut actors = BTreeMap::new();
    for (profile, npc) in pairs {
        let source = records
            .iter()
            .find(|(key, _)| {
                key.local == profile.owners[1].local
                    && key.plugin.eq_ignore_ascii_case(&profile.owners[1].plugin)
            })
            .map(|(_, record)| *record)
            .ok_or("legacy actor Stats owner is absent")?;
        let race = reference(&npc, b"RNAM").ok_or("selected legacy NPC has no RACE")?;
        if emitted(run, race)?.sig.0 != *b"RACE" {
            return Err("selected legacy NPC race was not emitted".into());
        }
        let row = actor_stats(
            &profile,
            source,
            npc.form_key,
            race,
            ap_base,
            ap_mult,
            &run.interner,
        )?;
        let key = (row.npc.clone(), row.race.clone());
        if let Some(previous) = actors.insert(key, row.clone()) {
            if previous != row {
                return Err("selected legacy NPC has conflicting source stat profiles".into());
            }
        }
    }
    let marker = ensure_marker(run)?;
    Ok(Some(LegacyActorStatsOutput {
        version: 1,
        source_game: run.source.as_str().to_string(),
        actor_level: catalog_key(marker, &run.interner),
        actors: actors.into_values().collect(),
        plugin: run.config.output_plugin_name.clone(),
    }))
}

fn collect_selected_profiles(
    plan: &LegacyTemplatePlan,
    target: FormKey,
    read: &impl Fn(FormKey) -> Result<Record, String>,
    output: &mut Vec<(LegacyTemplateProfile, Record)>,
) -> Result<(), String> {
    let record = read(target)?;
    match plan {
        LegacyTemplatePlan::Creature(profile) => {
            if record.sig.0 != *b"NPC_" {
                return Err("selected template branch did not emit an NPC".into());
            }
            output.push((profile.clone(), record));
        }
        LegacyTemplatePlan::Leveled { entries, .. } => {
            if record.sig.0 != *b"LVLN" {
                return Err("selected template branch did not emit an LVLN".into());
            }
            let targets = record
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"LVLO")
                .map(|field| {
                    first_key(&field.value).ok_or("target LVLN has an unresolved entry".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            if targets.len() != entries.len() {
                return Err("source and target template branch counts differ".into());
            }
            for (entry, target) in entries.iter().zip(targets) {
                collect_selected_profiles(entry, target, read, output)?;
            }
        }
    }
    Ok(())
}

fn actor_stats(
    profile: &LegacyTemplateProfile,
    source: &Record,
    npc: FormKey,
    race: FormKey,
    ap_base: f32,
    ap_mult: f32,
    interner: &StringInterner,
) -> Result<ActorStats, String> {
    let mut special = [0; 7];
    for (index, name) in [
        "attribute_strength",
        "attribute_perception",
        "attribute_endurance",
        "attribute_charisma",
        "attribute_intelligence",
        "attribute_agility",
        "attribute_luck",
    ]
    .into_iter()
    .enumerate()
    {
        special[index] = u8::try_from(number(source, b"DATA", name, 10 + index, 1, interner)?)
            .map_err(|_| "source SPECIAL is outside u8")?;
    }
    Ok(ActorStats {
        npc: catalog_key(npc, interner),
        race: catalog_key(race, interner),
        health: number(source, b"DATA", "health", 4, 2, interner)? as u16,
        pc_level_mult: profile.actor_flags & 0x80 != 0,
        level: number(source, b"ACBS", "level", 8, 2, interner)? as u16,
        min_level: number(source, b"ACBS", "calc_min", 10, 2, interner)? as u16,
        max_level: number(source, b"ACBS", "calc_max", 12, 2, interner)? as u16,
        special,
        ap_base,
        ap_mult,
    })
}

fn number(
    record: &Record,
    sig: &[u8; 4],
    name: &str,
    offset: usize,
    width: usize,
    interner: &StringInterner,
) -> Result<u32, String> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *sig)
        .map(|field| &field.value)
        .ok_or_else(|| {
            format!(
                "source creature is missing {}",
                String::from_utf8_lossy(sig)
            )
        })?;
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= offset + width => Ok(if width == 1 {
            bytes[offset].into()
        } else {
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()).into()
        }),
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some(name))
            .and_then(|(_, value)| match value {
                FieldValue::Uint(value) => u32::try_from(*value).ok(),
                FieldValue::Int(value) if width == 2 => Some(*value as u16 as u32),
                FieldValue::Int(value) => u32::try_from(*value).ok(),
                FieldValue::Bytes(bytes) if bytes.len() == width => Some(if width == 1 {
                    bytes[0].into()
                } else {
                    u16::from_le_bytes(bytes[..].try_into().unwrap()).into()
                }),
                _ => None,
            })
            .ok_or_else(|| format!("source creature field {name} is missing or invalid")),
        _ => Err(format!(
            "source creature field {name} has an invalid layout"
        )),
    }
}

fn ensure_marker(run: &mut ConversionRun) -> Result<FormKey, String> {
    const EDITOR_ID: &str = "B21_LegacyCreatureAttributeLevel";
    for key in iter_form_keys_of_sig(run.target_handle_id, SigCode(*b"AVIF"), &run.interner)
        .map_err(|error| error.to_string())?
    {
        let record = emitted(run, key)?;
        if record
            .eid
            .and_then(|eid| run.interner.resolve(eid))
            .is_some_and(|eid| eid == EDITOR_ID)
        {
            return Ok(key);
        }
    }
    let key = FormKeyMapper::from_state(
        run.mapper_state
            .as_mut()
            .ok_or("legacy stats mapper is unavailable")?,
        &run.interner,
    )
    .allocate_generated();
    let mut record = Record::new(SigCode(*b"AVIF"), key);
    record.eid = Some(run.interner.intern(EDITOR_ID));
    record.fields.extend(
        [
            (*b"NAM0", FieldValue::Float(0.0)),
            (*b"AVFL", FieldValue::Uint(0x2000)),
            (*b"NAM1", FieldValue::Uint(8)),
        ]
        .into_iter()
        .map(|(sig, value)| FieldEntry {
            sig: SubrecordSig(sig),
            value,
        }),
    );
    crate::target_write::add_record_native(
        run.target_handle_id,
        record,
        &run.schema_target,
        &run.interner,
    )
    .map_err(|error| error.to_string())?;
    Ok(key)
}

fn emitted(run: &ConversionRun, key: FormKey) -> Result<Record, String> {
    if !is_output_key(run, key) {
        return Err("legacy actor stats reference is not owned by the output plugin".into());
    }
    run.read_target_record_if_available(key)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "legacy actor stats reference {} was not emitted",
                key.format(&run.interner)
            )
        })
}

fn is_output_key(run: &ConversionRun, key: FormKey) -> bool {
    run.interner
        .resolve(key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(&run.config.output_plugin_name))
}

fn stable(key: FormKey, interner: &StringInterner) -> StableFormKey {
    StableFormKey {
        local: key.local,
        plugin: interner.resolve(key.plugin).unwrap().to_string(),
    }
}

fn catalog_key(key: FormKey, interner: &StringInterner) -> String {
    format!(
        "{:06X}:{}",
        key.local,
        interner.resolve(key.plugin).unwrap()
    )
}

fn reference(record: &Record, sig: &[u8; 4]) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|field| field.sig.0 == *sig)
        .and_then(|field| first_key(&field.value))
}

fn first_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(key) if key.local != 0 => Some(*key),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_key(value)),
        FieldValue::List(values) => values.iter().find_map(first_key),
        _ => None,
    }
}

#[cfg(test)]
#[path = "creature_actor_stats_tests.rs"]
mod tests;
