use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::Serialize;

use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::run::{ConversionRun, SkyrimCreatureAncillaryRecordKind};
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::translator::Game;

use super::field;

#[derive(Serialize)]
pub(crate) struct MagicRuntimeOutput {
    version: u32,
    resource: ResourceKeys,
    spells: BTreeSet<String>,
    disease_spells: BTreeSet<String>,
    invisibility_effects: BTreeSet<String>,
    bow_stagger_effects: BTreeSet<String>,
    actors: Vec<ActorFormula>,
    regen: RegenSettings,
    cost_settings: CostSettings,
    #[serde(skip)]
    pub(crate) records_changed: u32,
    #[serde(skip)]
    plugin: String,
}

#[derive(Serialize)]
struct ResourceKeys {
    magicka: String,
    rate: String,
    multiplier: String,
    regen_delay: String,
    actor_level: String,
    stamina_delay: String,
    disease_resist: String,
}

#[derive(Serialize)]
struct RegenSettings {
    combat_multiplier: f32,
    damage_delay: f32,
    stamina_combat_multiplier: f32,
    stamina_damage_delay: f32,
}

#[derive(Serialize)]
struct CostSettings {
    area_mult: f32,
    duration_mult: f32,
    cost_scale: f32,
    target_mult: f32,
    npc: SkillCostSettings,
    player: SkillCostSettings,
}

#[derive(Serialize)]
struct SkillCostSettings {
    base: f32,
    mult: f32,
    scale: f32,
}

#[derive(Serialize)]
struct ActorFormula {
    npc: String,
    race: String,
    formula: super::actor_stats::SourceAttributeFormula,
    skill_formula: super::actor_stats::SourceSkillFormula,
    baseline_magicka: f32,
    rates: [f32; 3],
}

impl MagicRuntimeOutput {
    pub(crate) fn write(&self, mod_path: &Path) -> Result<(), String> {
        let directory = mod_path.join("F4SE/Plugins/B21_ConversionRuntime/Magic");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let path = directory.join(format!("{}.json", self.plugin));
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, bytes).map_err(|error| error.to_string())
    }
}

pub(crate) fn prepare_magic_runtime_output(
    run: &mut ConversionRun,
) -> Result<Option<MagicRuntimeOutput>, String> {
    if run.source != Game::SkyrimSe || run.target != Game::Fo4 {
        return Ok(None);
    }
    let Some(resources) = run.skyrim_magic_resource_records else {
        return Ok(None);
    };
    for key in resources {
        if run
            .read_target_record_if_available(key)
            .map_err(|error| error.to_string())?
            .is_none_or(|record| record.sig.0 != *b"AVIF")
        {
            return Err(format!(
                "missing Skyrim resource {}",
                key.format(&run.interner)
            ));
        }
    }
    let mut sources = HashMap::new();
    for sig in [
        *b"RACE", *b"NPC_", *b"LVLN", *b"LVSP", *b"CLAS", *b"SPEL", *b"MGEF", *b"GMST",
    ] {
        for key in iter_form_keys_of_sig(run.source_handle_id, SigCode(sig), &run.interner)
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
            sources.insert(key, record);
        }
    }
    let mappings = &run
        .mapper_state
        .as_ref()
        .ok_or("Skyrim resource mapper is unavailable")?
        .source_to_target;
    let output_plugin = run.interner.intern(&run.config.output_plugin_name);
    let mut targets_to_source = HashMap::new();
    for (source, target) in mappings {
        if target.plugin == output_plugin && sources.contains_key(source) {
            targets_to_source.insert(*target, (*source, None));
        }
    }
    for reservation in run.skyrim_creature_ancillary_reservations() {
        let source = match reservation.kind {
            SkyrimCreatureAncillaryRecordKind::SyntheticNpc => Some(reservation.source_race),
            SkyrimCreatureAncillaryRecordKind::NpcVariant => {
                let plugin = reservation
                    .event
                    .strip_prefix("record_variant:")
                    .ok_or("NPC variant reservation has no source plugin")?;
                sources
                    .keys()
                    .find(|key| {
                        key.local == reservation.ordinal
                            && run
                                .interner
                                .resolve(key.plugin)
                                .is_some_and(|name| name.eq_ignore_ascii_case(plugin))
                    })
                    .copied()
            }
            _ => continue,
        }
        .ok_or("NPC variant source record is unavailable")?;
        targets_to_source.insert(reservation.target, (source, Some(reservation.source_race)));
    }
    let mut output = MagicRuntimeOutput {
        version: 1,
        resource: ResourceKeys {
            magicka: catalog_key(resources[0], &run.interner),
            rate: catalog_key(resources[1], &run.interner),
            multiplier: catalog_key(resources[2], &run.interner),
            regen_delay: catalog_key(resources[3], &run.interner),
            actor_level: catalog_key(resources[4], &run.interner),
            stamina_delay: catalog_key(resources[5], &run.interner),
            disease_resist: catalog_key(resources[6], &run.interner),
        },
        spells: BTreeSet::new(),
        disease_spells: BTreeSet::new(),
        invisibility_effects: BTreeSet::new(),
        bow_stagger_effects: BTreeSet::new(),
        actors: Vec::new(),
        regen: RegenSettings {
            combat_multiplier: source_setting(
                &sources,
                "fCombatMagickaRegenRateMult",
                0.33,
                &run.interner,
            )?,
            damage_delay: source_setting(
                &sources,
                "fDamagedMagickaRegenDelay",
                0.5,
                &run.interner,
            )?,
            stamina_combat_multiplier: source_setting(
                &sources,
                "fCombatStaminaRegenRateMult",
                0.35,
                &run.interner,
            )?,
            stamina_damage_delay: source_setting(
                &sources,
                "fDamagedStaminaRegenDelay",
                0.5,
                &run.interner,
            )?,
        },
        cost_settings: cost_settings(&sources, &run.interner)?,
        records_changed: 0,
        plugin: run.config.output_plugin_name.clone(),
    };
    let target_context = crate::source_read::plugin_context_for_handle(run.target_handle_id)
        .map_err(|error| error.to_string())?;
    let mut changed_records = Vec::new();
    let source_refs = sources
        .iter()
        .map(|(key, record)| (*key, record))
        .collect::<HashMap<_, _>>();
    let mut catalog_actors = BTreeSet::new();
    for sig in [*b"RACE", *b"NPC_"] {
        for key in iter_form_keys_of_sig(run.target_handle_id, SigCode(sig), &run.interner)
            .map_err(|error| error.to_string())?
        {
            let Some((source, forced_race)) = targets_to_source.get(&key) else {
                continue;
            };
            let original = &sources[source];
            let mut target = run
                .read_target_record_if_available(key)
                .map_err(|error| error.to_string())?
                .ok_or("mapped target disappeared")?;
            let mut authored_stats = None;
            if target.sig.0 == *b"RACE" {
                let stats = super::actor_stats::project_actor_stats(
                    original,
                    *forced_race,
                    &sources,
                    &run.interner,
                )?;
                restore_race_spells(&mut target, original, mappings, &sources)?;
                authored_stats = Some(stats);
            } else {
                let stats_owners = if original.sig.0 == *b"RACE" {
                    vec![original]
                } else {
                    restore_npc_template(&mut target, original, mappings, &run.interner)?;
                    super::super::npc_templates::owners(original, 0x2, &source_refs, &run.interner)?
                };
                let races = if original.sig.0 == *b"RACE" {
                    vec![original.form_key]
                } else if super::super::npc_templates::template_flags(original, &run.interner)? & 1
                    == 0
                {
                    vec![
                        forced_race
                            .or_else(|| {
                                value(original, b"RNAM")
                                    .and_then(super::super::npc_templates::first_key)
                            })
                            .ok_or("source NPC has no race")?,
                    ]
                } else {
                    super::super::npc_templates::owners(original, 1, &source_refs, &run.interner)?
                        .iter()
                        .map(|owner| {
                            value(owner, b"RNAM")
                                .and_then(super::super::npc_templates::first_key)
                                .ok_or("trait template has no race")
                        })
                        .collect::<Result<Vec<_>, _>>()?
                };
                for owner in &stats_owners {
                    let owner_target = if owner.form_key == original.form_key {
                        key
                    } else {
                        *mappings
                            .get(&owner.form_key)
                            .ok_or("stat template has no target mapping")?
                    };
                    if run
                        .read_target_record_if_available(owner_target)
                        .map_err(|error| error.to_string())?
                        .is_none()
                    {
                        return Err(format!(
                            "stat template {} was not emitted",
                            owner_target.format(&run.interner)
                        ));
                    }
                    for race in &races {
                        let race_target = if original.sig.0 == *b"RACE" {
                            *mappings
                                .get(race)
                                .ok_or("synthetic NPC race has no target mapping")?
                        } else if stats_owners.len() == 1
                            && races.len() == 1
                            && super::super::npc_templates::template_flags(original, &run.interner)?
                                & 1
                                == 0
                        {
                            value(&target, b"RNAM")
                                .and_then(super::super::npc_templates::first_key)
                                .or_else(|| mappings.get(race).copied())
                                .ok_or("target NPC race is unavailable")?
                        } else {
                            *mappings
                                .get(race)
                                .ok_or("trait template race has no target mapping")?
                        };
                        let stats = super::actor_stats::project_actor_stats(
                            owner,
                            Some(*race),
                            &sources,
                            &run.interner,
                        )?;
                        let actor_key = (
                            catalog_key(owner_target, &run.interner),
                            catalog_key(race_target, &run.interner),
                        );
                        if catalog_actors.insert(actor_key.clone()) {
                            output.actors.push(ActorFormula {
                                npc: actor_key.0,
                                race: actor_key.1,
                                formula: stats.formula.clone(),
                                skill_formula: stats.skill_formula.clone(),
                                baseline_magicka: stats.magicka,
                                rates: [stats.health_rate, stats.magicka_rate, stats.stamina_rate],
                            });
                        }
                        if owner.form_key == original.form_key && races.len() == 1 {
                            authored_stats = Some(stats);
                        }
                    }
                }
            }
            if let Some(stats) = authored_stats {
                let mut properties = vec![
                    (
                        FormKey {
                            local: 0x2D4,
                            plugin: run.interner.intern("Fallout4.esm"),
                        },
                        stats.health,
                    ),
                    (
                        FormKey {
                            local: 0x2D5,
                            plugin: run.interner.intern("Fallout4.esm"),
                        },
                        stats.stamina,
                    ),
                    (resources[0], stats.magicka),
                ];
                if target.sig.0 == *b"RACE" {
                    properties.extend([
                        (resources[1], stats.magicka_rate),
                        (
                            FormKey {
                                local: 0x2D7,
                                plugin: run.interner.intern("Fallout4.esm"),
                            },
                            stats.health_rate,
                        ),
                        (
                            FormKey {
                                local: 0x2D8,
                                plugin: run.interner.intern("Fallout4.esm"),
                            },
                            stats.stamina_rate,
                        ),
                    ]);
                }
                upsert_properties(&mut target, &properties, &target_context, &run.interner)?;
                if target.sig.0 == *b"NPC_" {
                    patch_actor_level(&mut target, stats.level, &run.interner)?;
                    patch_calculated_stats(
                        &mut target,
                        stats.health,
                        stats.stamina,
                        &run.interner,
                    )?;
                }
            }
            changed_records.push(target);
            output.records_changed += 1;
        }
    }
    for source in sources.values().filter(|record| record.sig.0 == *b"SPEL") {
        let Some(target_key) = mappings
            .get(&source.form_key)
            .filter(|key| key.plugin == output_plugin)
        else {
            continue;
        };
        let Some(target) = run
            .read_target_record_if_available(*target_key)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        if charged_spell_preserved(source, &target, &sources, run)? {
            output
                .spells
                .insert(catalog_key(*target_key, &run.interner));
        }
        if source_spell_type(source, &run.interner) == Some(1)
            && source_spell_type(&target, &run.interner) == Some(1)
        {
            output
                .disease_spells
                .insert(catalog_key(*target_key, &run.interner));
        }
    }
    for source in sources.values().filter(|record| record.sig.0 == *b"MGEF") {
        let Some(data) = value(source, b"DATA") else {
            continue;
        };
        let invisibility = data_word(data, 64, &["archtype", "archetype"], false, &run.interner)
            == Some(11)
            && data_word(data, 0, &["flags"], false, &run.interner)
                .is_some_and(|flags| flags & 0x400 != 0);
        let bow_stagger = source_bow_stagger_effect(data, &run.interner);
        if !invisibility && !bow_stagger {
            continue;
        }
        let Some(target) = mappings
            .get(&source.form_key)
            .filter(|key| key.plugin == output_plugin)
        else {
            continue;
        };
        if let Some(record) = run
            .read_target_record_if_available(*target)
            .map_err(|error| error.to_string())?
            .filter(|record| {
                record.sig.0 == *b"MGEF"
                    && !record.flags.contains(crate::record::RecordFlags::DELETED)
            })
        {
            if invisibility {
                output
                    .invisibility_effects
                    .insert(catalog_key(*target, &run.interner));
            }
            if bow_stagger {
                if !target_bow_stagger_effect(&record, &run.interner) {
                    return Err(format!(
                        "Bow stagger effect {} lost its native BowStaggerBonus actor value",
                        target.format(&run.interner)
                    ));
                }
                output
                    .bow_stagger_effects
                    .insert(catalog_key(*target, &run.interner));
            }
        }
    }
    output
        .actors
        .sort_by(|left, right| (&left.npc, &left.race).cmp(&(&right.npc, &right.race)));
    crate::target_write::replace_records_native(
        run.target_handle_id,
        changed_records,
        &run.schema_target,
        &run.interner,
    )
    .map_err(|error| error.to_string())?;
    Ok(Some(output))
}

fn catalog_key(key: FormKey, interner: &StringInterner) -> String {
    format!(
        "{:06X}:{}",
        key.local,
        interner.resolve(key.plugin).unwrap()
    )
}

fn source_bow_stagger_effect(data: &FieldValue, interner: &StringInterner) -> bool {
    data_word(data, 64, &["archtype", "archetype"], false, interner) == Some(34)
        && data_word(data, 68, &["actorvalue"], false, interner) == Some(87)
}

fn target_bow_stagger_effect(record: &Record, interner: &StringInterner) -> bool {
    let Some(data @ FieldValue::Struct(fields)) = value(record, b"DATA") else {
        return false;
    };
    data_word(data, 64, &["archtype", "archetype"], false, interner) == Some(34)
        && fields.iter().any(|(name, value)| {
            interner.resolve(*name).is_some_and(|name| canonical(name) == "actorvalue")
                && matches!(value, FieldValue::FormKey(key) if key.local == 0x314 && interner.resolve(key.plugin).is_some_and(|plugin| plugin.eq_ignore_ascii_case("Fallout4.esm")))
        })
}

fn restore_race_spells(
    target: &mut Record,
    source: &Record,
    mappings: &HashMap<FormKey, FormKey, impl std::hash::BuildHasher>,
    sources: &HashMap<FormKey, Record>,
) -> Result<(), String> {
    let mut spells = Vec::new();
    for entry in source.fields.iter().filter(|field| field.sig.0 == *b"SPLO") {
        let key = super::super::npc_templates::first_key(&entry.value)
            .ok_or("source race SPLO is not a FormKey")?;
        if sources
            .get(&key)
            .is_some_and(|record| record.sig.0 == *b"SPEL" || record.sig.0 == *b"LVSP")
        {
            spells.push(
                *mappings
                    .get(&key)
                    .ok_or("race spell has no target mapping")?,
            );
        }
    }
    target
        .fields
        .retain(|field| field.sig.0 != *b"SPLO" && field.sig.0 != *b"SPCT");
    if !spells.is_empty() {
        target
            .fields
            .push(field(*b"SPCT", FieldValue::Uint(spells.len() as u64)));
        target.fields.extend(
            spells
                .into_iter()
                .map(|key| field(*b"SPLO", FieldValue::FormKey(key))),
        );
    }
    Ok(())
}

fn restore_npc_template(
    target: &mut Record,
    source: &Record,
    mappings: &HashMap<FormKey, FormKey, impl std::hash::BuildHasher>,
    interner: &StringInterner,
) -> Result<(), String> {
    let flags = super::super::npc_templates::template_flags(source, interner)?;
    let actor_flags = super::super::npc_templates::actor_flags(source, interner)?;
    let template = value(source, b"TPLT")
        .and_then(super::super::npc_templates::first_key)
        .map(|key| {
            mappings
                .get(&key)
                .copied()
                .ok_or("NPC template has no target mapping")
        })
        .transpose()?;
    if flags != 0 && template.is_none() {
        return Err("NPC inherits categories without a template".into());
    }
    target.fields.retain(|field| field.sig.0 != *b"TPLT");
    if let Some(key) = template {
        target
            .fields
            .push(field(*b"TPLT", FieldValue::FormKey(key)));
    }
    let config = target
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"ACBS")
        .ok_or("target NPC has no ACBS")?;
    match &mut config.value {
        FieldValue::Bytes(bytes) if bytes.len() == 20 => {
            let previous = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            bytes[..4]
                .copy_from_slice(&((previous & !0x310) | (actor_flags & 0x310)).to_le_bytes());
            bytes[14..16].copy_from_slice(&flags.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            for (name, value) in fields {
                match interner.resolve(*name).map(canonical).as_deref() {
                    Some("templateflags") => *value = FieldValue::Uint(flags.into()),
                    Some("flags") => {
                        let previous = match value {
                            FieldValue::Uint(value) => *value,
                            FieldValue::Int(value) => *value as u64,
                            _ => return Err("target NPC flags are not numeric".into()),
                        };
                        *value =
                            FieldValue::Uint((previous & !0x310) | u64::from(actor_flags & 0x310));
                    }
                    _ => {}
                }
            }
        }
        _ => return Err("target NPC ACBS has unsupported template layout".into()),
    }
    Ok(())
}

fn cost_settings(
    records: &HashMap<FormKey, Record>,
    interner: &StringInterner,
) -> Result<CostSettings, String> {
    let setting = |name, default| source_setting(records, name, default, interner);
    Ok(CostSettings {
        area_mult: setting("fMagicAreaBaseCostMult", 0.15)?,
        duration_mult: setting("fMagicDurMagBaseCostMult", 0.1)?,
        cost_scale: setting("fMagicCostScale", 1.1)?,
        target_mult: setting("fMagicRangeTargetCostMult", 1.5)?,
        npc: SkillCostSettings {
            base: setting("fMagicCasterSkillCostBase", 0.005)?,
            mult: setting("fMagicCasterSkillCostMult", 1.0)?,
            scale: setting("fMagicSkillCostScale", 0.5)?,
        },
        player: SkillCostSettings {
            base: setting("fMagicCasterPCSkillCostBase", 0.005)?,
            mult: setting("fMagicCasterPCSkillCostMult", 1.0)?,
            scale: setting("fMagicPCSkillCostScale", 0.5)?,
        },
    })
}

fn source_setting(
    records: &HashMap<FormKey, Record>,
    name: &str,
    default: f32,
    interner: &StringInterner,
) -> Result<f32, String> {
    let Some(record) = records.values().find(|record| {
        record.sig.0 == *b"GMST"
            && record
                .eid
                .and_then(|eid| interner.resolve(eid))
                .is_some_and(|eid| eid.eq_ignore_ascii_case(name))
    }) else {
        return Ok(default);
    };
    let value = match value(record, b"DATA") {
        Some(FieldValue::Float(value)) => Some(*value),
        Some(FieldValue::Bytes(bytes)) if bytes.len() == 4 => {
            Some(f32::from_le_bytes(bytes[..].try_into().unwrap()))
        }
        _ => None,
    }
    .ok_or_else(|| format!("source setting {name} is not a float"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid source setting {name}: {value}"));
    }
    Ok(value)
}

fn value<'a>(record: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|field| field.sig.0 == *sig)
        .map(|field| &field.value)
}

fn raw_key(
    key: FormKey,
    context: &(String, Vec<String>),
    interner: &StringInterner,
) -> Result<u32, String> {
    let name = interner
        .resolve(key.plugin)
        .ok_or("unresolved property plugin")?;
    let index = if context.0.eq_ignore_ascii_case(name) {
        context.1.len()
    } else {
        context
            .1
            .iter()
            .position(|master| master.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("property master {name} missing from output"))?
    };
    Ok(((index as u32) << 24) | key.local)
}

fn upsert_properties(
    record: &mut Record,
    properties: &[(FormKey, f32)],
    context: &(String, Vec<String>),
    interner: &StringInterner,
) -> Result<(), String> {
    let mut rows = Vec::<(u32, f32)>::new();
    for entry in record.fields.iter().filter(|field| field.sig.0 == *b"PRPS") {
        match &entry.value {
            FieldValue::Bytes(bytes) if bytes.len() % 8 == 0 => {
                rows.extend(bytes.chunks_exact(8).map(|row| {
                    (
                        u32::from_le_bytes(row[..4].try_into().unwrap()),
                        f32::from_le_bytes(row[4..].try_into().unwrap()),
                    )
                }));
            }
            FieldValue::List(entries) => {
                for entry in entries {
                    let FieldValue::Struct(fields) = entry else {
                        return Err("PRPS row is not a struct".into());
                    };
                    let key = fields
                        .iter()
                        .find_map(|(_, value)| {
                            if let FieldValue::FormKey(key) = value {
                                Some(*key)
                            } else {
                                None
                            }
                        })
                        .ok_or("PRPS row has no actor value")?;
                    let value = fields
                        .iter()
                        .find_map(|(_, value)| {
                            if let FieldValue::Float(value) = value {
                                Some(*value)
                            } else {
                                None
                            }
                        })
                        .ok_or("PRPS row has no value")?;
                    rows.push((raw_key(key, context, interner)?, value));
                }
            }
            _ => return Err("unsupported target PRPS layout".into()),
        }
    }
    for (key, value) in properties {
        let raw = raw_key(*key, context, interner)?;
        rows.retain(|(key, _)| *key != raw);
        rows.push((raw, *value));
    }
    let mut bytes = Vec::with_capacity(rows.len() * 8);
    for (key, value) in rows {
        bytes.extend(key.to_le_bytes());
        bytes.extend(value.to_le_bytes());
    }
    record.fields.retain(|field| field.sig.0 != *b"PRPS");
    record.fields.push(field(
        *b"PRPS",
        FieldValue::Bytes(bytes.into_iter().collect()),
    ));
    Ok(())
}

fn patch_calculated_stats(
    record: &mut Record,
    health: f32,
    stamina: f32,
    interner: &StringInterner,
) -> Result<(), String> {
    let Some(entry) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"DNAM")
    else {
        return Ok(());
    };
    let health = health.clamp(0.0, u16::MAX as f32) as u16;
    let stamina = stamina.clamp(0.0, u16::MAX as f32) as u16;
    match &mut entry.value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            bytes[..2].copy_from_slice(&health.to_le_bytes());
            bytes[2..4].copy_from_slice(&stamina.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            for (name, value) in fields {
                match interner.resolve(*name).map(canonical).as_deref() {
                    Some("calculatedhealth") => *value = FieldValue::Uint(health.into()),
                    Some("calculatedactionpoints") => *value = FieldValue::Uint(stamina.into()),
                    _ => {}
                }
            }
        }
        _ => return Err("target NPC DNAM has unsupported stat layout".into()),
    }
    Ok(())
}

fn canonical(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn patch_actor_level(
    record: &mut Record,
    level: super::actor_stats::SourceActorLevel,
    interner: &StringInterner,
) -> Result<(), String> {
    use super::actor_stats::SourceActorLevel;
    let (scaled, encoded, minimum, maximum) = match level {
        SourceActorLevel::Fixed { level } => (false, level, level, level),
        SourceActorLevel::PlayerMultiplier {
            multiplier,
            minimum,
            maximum,
        } => (true, (multiplier * 1000.0).round() as u16, minimum, maximum),
    };
    let entry = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"ACBS")
        .ok_or("target NPC has no ACBS")?;
    match &mut entry.value {
        FieldValue::Bytes(bytes) if bytes.len() == 20 => {
            let flags = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            bytes[..4]
                .copy_from_slice(&((flags & !0x80) | if scaled { 0x80 } else { 0 }).to_le_bytes());
            bytes[6..8].copy_from_slice(&encoded.to_le_bytes());
            bytes[8..10].copy_from_slice(&minimum.to_le_bytes());
            bytes[10..12].copy_from_slice(&maximum.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            for (name, value) in fields {
                match interner.resolve(*name).map(canonical).as_deref() {
                    Some("flags") => {
                        let flags = match value {
                            FieldValue::Uint(flags) => *flags,
                            FieldValue::Int(flags) => *flags as u64,
                            _ => return Err("target NPC flags are not numeric".into()),
                        };
                        *value = FieldValue::Uint((flags & !0x80) | if scaled { 0x80 } else { 0 });
                    }
                    Some("level") => *value = FieldValue::Uint(encoded.into()),
                    Some("calcminlevel") => *value = FieldValue::Uint(minimum.into()),
                    Some("calcmaxlevel") => *value = FieldValue::Uint(maximum.into()),
                    _ => {}
                }
            }
        }
        _ => return Err("target NPC ACBS has unsupported level layout".into()),
    }
    Ok(())
}

fn charged_spell_preserved(
    source: &Record,
    target: &Record,
    sources: &HashMap<FormKey, Record>,
    run: &ConversionRun,
) -> Result<bool, String> {
    let (Some(source_data), Some(target_data)) = (
        spell_words(source, &run.interner),
        spell_words(target, &run.interner),
    ) else {
        return Ok(false);
    };
    if source_data[2] != 0
        || !matches!(source_data[4], 1 | 2)
        || source_data[5] == 1
        || source_data != target_data
    {
        return Ok(false);
    }
    let source_effects = source
        .fields
        .iter()
        .filter(|field| matches!(&field.sig.0, b"EFID" | b"EFIT"))
        .collect::<Vec<_>>();
    let target_effects = target
        .fields
        .iter()
        .filter(|field| matches!(&field.sig.0, b"EFID" | b"EFIT"))
        .collect::<Vec<_>>();
    if source_effects.is_empty() || source_effects.len() != target_effects.len() {
        return Ok(false);
    }
    let mappings = &run.mapper_state.as_ref().unwrap().source_to_target;
    for (source, target) in source_effects.into_iter().zip(target_effects) {
        if source.sig != target.sig {
            return Ok(false);
        }
        if source.sig.0 == *b"EFIT" {
            if effect_words(&source.value, &run.interner).is_none()
                || effect_words(&source.value, &run.interner)
                    != effect_words(&target.value, &run.interner)
            {
                return Ok(false);
            }
        } else {
            let (FieldValue::FormKey(source), FieldValue::FormKey(target)) =
                (&source.value, &target.value)
            else {
                return Ok(false);
            };
            if mappings.get(source) != Some(target) {
                return Ok(false);
            }
            let Some(source_effect) = sources.get(source) else {
                return Ok(false);
            };
            let Some(target_effect) = run
                .read_target_record_if_available(*target)
                .map_err(|error| error.to_string())?
            else {
                return Ok(false);
            };
            let (Some(source), Some(target)) = (
                value(source_effect, b"DATA"),
                value(&target_effect, b"DATA"),
            ) else {
                return Ok(false);
            };
            let source_cost = data_word(source, 4, &["basecost"], true, &run.interner);
            if source_cost.is_none()
                || source_cost != data_word(target, 4, &["basecost"], true, &run.interner)
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn data_word(
    data: &FieldValue,
    offset: usize,
    names: &[&str],
    float: bool,
    interner: &StringInterner,
) -> Option<u32> {
    match data {
        FieldValue::Bytes(bytes) => bytes
            .get(offset..offset + 4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap())),
        FieldValue::Struct(fields) => {
            let value = fields.iter().find_map(|(name, value)| {
                names
                    .contains(&canonical(interner.resolve(*name)?).as_str())
                    .then_some(value)
            })?;
            match (value, float) {
                (FieldValue::Float(value), true) => Some(value.to_bits()),
                (FieldValue::Uint(value), false) => (*value).try_into().ok(),
                (FieldValue::Int(value), false) => (*value).try_into().ok(),
                _ => None,
            }
        }
        _ => None,
    }
}

fn spell_words(record: &Record, interner: &StringInterner) -> Option<[u32; 8]> {
    let data = value(record, b"SPIT")?;
    Some([
        data_word(data, 0, &["basecost"], false, interner)?,
        data_word(data, 4, &["flags"], false, interner)?,
        data_word(data, 8, &["type"], false, interner)?,
        data_word(data, 12, &["chargetime"], true, interner)?,
        data_word(data, 16, &["casttype"], false, interner)?,
        data_word(data, 20, &["delivery", "targettype"], false, interner)?,
        data_word(data, 24, &["castduration"], true, interner)?,
        data_word(data, 28, &["range"], true, interner)?,
    ])
}

fn effect_words(data: &FieldValue, interner: &StringInterner) -> Option<[u32; 3]> {
    Some([
        data_word(data, 0, &["magnitude"], true, interner)?,
        data_word(data, 4, &["area"], false, interner)?,
        data_word(data, 8, &["duration"], false, interner)?,
    ])
}

fn source_spell_type(record: &Record, interner: &StringInterner) -> Option<u32> {
    data_word(value(record, b"SPIT")?, 8, &["type"], false, interner)
}

#[cfg(test)]
mod bow_stagger_tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::FieldEntry;

    #[test]
    fn bow_stagger_allowlist_requires_source_archetype_and_native_target_actor_value() {
        let interner = StringInterner::new();
        let mut source = vec![0; 152];
        source[64..68].copy_from_slice(&34u32.to_le_bytes());
        source[68..72].copy_from_slice(&87u32.to_le_bytes());
        assert!(source_bow_stagger_effect(
            &FieldValue::Bytes(source.clone().into()),
            &interner
        ));
        source[64..68].copy_from_slice(&0u32.to_le_bytes());
        assert!(!source_bow_stagger_effect(
            &FieldValue::Bytes(source.into()),
            &interner
        ));
        let mut target = Record::new(
            SigCode(*b"MGEF"),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Converted.esm"),
            },
        );
        target.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Struct(vec![
                (interner.intern("archtype"), FieldValue::Uint(34)),
                (
                    interner.intern("actor_value"),
                    FieldValue::FormKey(FormKey {
                        local: 0x314,
                        plugin: interner.intern("Fallout4.esm"),
                    }),
                ),
            ]),
        });
        assert!(target_bow_stagger_effect(&target, &interner));
        let FieldValue::Struct(fields) = &mut target.fields[0].value else {
            panic!()
        };
        fields[1].1 = FieldValue::FormKey(FormKey {
            local: 0x314,
            plugin: interner.intern("Other.esm"),
        });
        assert!(!target_bow_stagger_effect(&target, &interner));
    }

    #[test]
    fn bow_stagger_effect_publication_requires_a_live_mapped_native_effect() {
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::target_write::{add_record_native, replace_record_native};
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_no_py,
        };
        let source_handle = plugin_handle_new_no_py("Skyrim.esm", Some("skyrimse"));
        let target_handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Converted.esm".into(),
                ..Default::default()
            },
        })
        .unwrap();
        with_run(id, |run| -> Result<(), RunError> {
            let mut state = MapperState::new(
                [],
                MapperOptions {
                    output_plugin_name: "Converted.esm".into(),
                    source_plugin_name: "Skyrim.esm".into(),
                    target_master_names: vec!["Fallout4.esm".into()],
                    ..Default::default()
                },
            );
            let mut mapper = FormKeyMapper::from_state(&mut state, &run.interner);
            let (resources, records) =
                super::super::allocate_magic_resource_records(&mut mapper, None, &run.interner);
            for record in records {
                add_record_native(target_handle, record, &run.schema_target, &run.interner)
                    .unwrap();
            }
            let source_key = FormKey {
                local: 0x6C0F3,
                plugin: run.interner.intern("Skyrim.esm"),
            };
            let target_key = FormKey {
                local: 0x2000,
                plugin: run.interner.intern("Converted.esm"),
            };
            mapper.add_mapping(source_key, target_key);
            let mut source = Record::new(SigCode(*b"MGEF"), source_key);
            let mut data = vec![0; 152];
            data[64..68].copy_from_slice(&34u32.to_le_bytes());
            data[68..72].copy_from_slice(&87u32.to_le_bytes());
            source.fields.push(FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(data.into()),
            });
            add_record_native(source_handle, source, &run.schema_source, &run.interner).unwrap();
            let mut target = Record::new(SigCode(*b"MGEF"), target_key);
            target.fields.push(FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Struct(vec![
                    (run.interner.intern("archtype"), FieldValue::Uint(34)),
                    (
                        run.interner.intern("actor_value"),
                        FieldValue::FormKey(FormKey {
                            local: 0x314,
                            plugin: run.interner.intern("Fallout4.esm"),
                        }),
                    ),
                ]),
            });
            add_record_native(
                target_handle,
                target.clone(),
                &run.schema_target,
                &run.interner,
            )
            .unwrap();
            run.mapper_state = Some(state);
            run.skyrim_magic_resource_records = Some(resources);
            let output = prepare_magic_runtime_output(run).unwrap().unwrap();
            assert_eq!(
                output.bow_stagger_effects,
                BTreeSet::from(["002000:Converted.esm".into()])
            );
            assert!(output.invisibility_effects.is_empty());
            let FieldValue::Struct(fields) = &mut target.fields[0].value else {
                panic!()
            };
            fields[1].1 = FieldValue::FormKey(FormKey {
                local: 0x2D4,
                plugin: run.interner.intern("Fallout4.esm"),
            });
            replace_record_native(target_handle, target, &run.schema_target, &run.interner)
                .unwrap();
            assert!(
                prepare_magic_runtime_output(run)
                    .err()
                    .unwrap()
                    .contains("lost its native BowStaggerBonus")
            );
            Ok(())
        })
        .unwrap();
        drop_run(id).unwrap();
        assert!(plugin_handle_close_native(source_handle));
        assert!(plugin_handle_close_native(target_handle));
    }
}
