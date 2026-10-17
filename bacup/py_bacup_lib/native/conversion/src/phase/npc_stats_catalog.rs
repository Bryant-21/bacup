//! `emit_npc_stats_catalog`: the FO76 curve tables behind converted NPC health,
//! resistances and XP, race stats and unarmed damage, and the weapon, explosion and
//! armor damage BACUP flattened, for the Tales runtime to read at an actor's level.
//! NPC entries are keyed by the `B21_NpcCurveSource` marker each converted NPC carries
//! (`flatten_npc_property_curves`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use esp_authoring_core::plugin_runtime::{ParsedSubrecord, effective_subrecords_for_record};
use serde_json::{Map, Value, json};

use crate::fixups::curve_table::{
    curve_points, curve_points_authored, curve_value_at_level, mean_curve_value,
};
use crate::fixups::flatten_armo_damage_curves::ARMOR_CURVE_LEVEL;
use crate::fixups::flatten_expl_damage_curves::expl_data_offset;
use crate::fixups::flatten_npc_property_curves::curve_source_marker_key;
use crate::fixups::synthesize_weap_data_blocks::curve_damage_base;
use crate::ids::{FormKey, SigCode};
use crate::phase::equipment_condition::curve_path;
use crate::phase::leveled_catalog::decode::{self, curve_json_path, editor_id};
use crate::phase::leveled_catalog::{Resolved, resolve_outputs, write_catalog};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::session::{HandleRawScan, open_session};
use crate::translator::Game;

pub struct NpcStatsCatalogPhase;

const SCHEMA_VERSION: u32 = 1;
const DELETED: u32 = 0x20;
const FO76_ROW_LEN: usize = 12;
const FO4_PROPERTY_ROW_LEN: usize = 8;
const ACBS_AUTO_CALC_STATS: u32 = 0x10;
const RACE_DATA_XP_OFFSET: usize = 144;
/// Health, DamageResist, EnergyResist, RadResistExposure, PoisonResist, FireResist, FrostResist.
const STAT_ACTOR_VALUES: [u32; 7] = [0x2D4, 0x2E3, 0x2EB, 0x2EA, 0x2E4, 0x2E5, 0x2E7];

/// A FO76 PRPS or DAMA row: actor value or damage type, the authored value's bits
/// (f32 in PRPS, u32 in DAMA), and the curve FO76 reads instead when one is set.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CurvedRow {
    form: u32,
    bits: u32,
    curve: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SourceNpc {
    editor_id: String,
    auto_calc: bool,
    health_curve: Option<u32>,
    xp_curve: Option<u32>,
    properties: Vec<CurvedRow>,
    race: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SourceRace {
    editor_id: String,
    xp: u16,
    properties: Vec<CurvedRow>,
    unarmed: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SourceDamage {
    editor_id: String,
    curve: Option<u32>,
    types: Vec<CurvedRow>,
}

/// Source records behind the catalog. NPCs, weapons, explosions and armors carry the
/// output FormKey of the own record converted from them.
#[derive(Default)]
struct Source {
    npcs: Vec<(String, u32, SourceNpc)>,
    races: BTreeMap<u32, SourceRace>,
    unarmed: HashMap<u32, SourceDamage>,
    weapons: Vec<(String, SourceDamage)>,
    explosions: Vec<(String, SourceDamage)>,
    armors: Vec<(String, SourceDamage)>,
}

trait StatKeys {
    /// `Plugin.esm:XXXXXX` of a raw source FormID.
    fn source(&self, raw: u32) -> Option<String>;
    /// Output FormKey of a source form present in the converted plugin or its masters.
    fn output(&self, raw: u32) -> Option<String>;
    /// Output FormKey the fixups wrote for a source actor value or damage type.
    fn reference(&self, raw: u32) -> Option<String>;
}

struct SessionKeys {
    resolved: Resolved,
    references: HashMap<u32, String>,
}

impl StatKeys for SessionKeys {
    fn source(&self, raw: u32) -> Option<String> {
        decode::Keys::source(&self.resolved, raw)
    }
    fn output(&self, raw: u32) -> Option<String> {
        decode::Keys::output(&self.resolved, raw)
    }
    fn reference(&self, raw: u32) -> Option<String> {
        self.references.get(&raw).cloned()
    }
}

fn internal(error: impl std::fmt::Display) -> PhaseError {
    PhaseError::Internal(error.to_string())
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
}

fn subrecord<'a>(fields: &'a [ParsedSubrecord], signature: &str) -> Option<&'a [u8]> {
    fields
        .iter()
        .find(|field| field.signature == signature)
        .map(|field| field.data.as_ref())
}

fn form(fields: &[ParsedSubrecord], signature: &str) -> Option<u32> {
    subrecord(fields, signature)
        .and_then(|data| u32_at(data, 0))
        .filter(|raw| *raw != 0)
}

fn curved_rows(fields: &[ParsedSubrecord], signature: &str) -> Vec<CurvedRow> {
    subrecord(fields, signature)
        .map(|data| {
            data.chunks_exact(FO76_ROW_LEN)
                .map(|row| CurvedRow {
                    form: u32_at(row, 0).unwrap_or(0),
                    bits: u32_at(row, 4).unwrap_or(0),
                    curve: u32_at(row, 8).filter(|raw| *raw != 0),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn decode_npc(fields: &[ParsedSubrecord]) -> SourceNpc {
    SourceNpc {
        editor_id: editor_id(fields),
        auto_calc: subrecord(fields, "ACBS")
            .and_then(|acbs| u32_at(acbs, 0))
            .is_some_and(|flags| flags & ACBS_AUTO_CALC_STATS != 0),
        health_curve: form(fields, "CVT0"),
        xp_curve: form(fields, "CVT2"),
        properties: curved_rows(fields, "PRPS"),
        race: form(fields, "RNAM"),
    }
}

fn decode_race(fields: &[ParsedSubrecord]) -> SourceRace {
    SourceRace {
        editor_id: editor_id(fields),
        xp: subrecord(fields, "DATA")
            .and_then(|data| data.get(RACE_DATA_XP_OFFSET..RACE_DATA_XP_OFFSET + 2))
            .map_or(0, |xp| u16::from_le_bytes([xp[0], xp[1]])),
        properties: curved_rows(fields, "PRPS"),
        unarmed: form(fields, "UNWP"),
    }
}

fn decode_weapon(fields: &[ParsedSubrecord]) -> SourceDamage {
    SourceDamage {
        editor_id: editor_id(fields),
        curve: form(fields, "CVT0"),
        types: curved_rows(fields, "DAMA"),
    }
}

fn decode_explosion(fields: &[ParsedSubrecord], curve_offset: usize) -> SourceDamage {
    SourceDamage {
        editor_id: editor_id(fields),
        curve: subrecord(fields, "DATA")
            .and_then(|data| u32_at(data, curve_offset))
            .filter(|raw| *raw != 0),
        types: Vec::new(),
    }
}

fn decode_armor(fields: &[ParsedSubrecord]) -> SourceDamage {
    SourceDamage {
        editor_id: editor_id(fields),
        curve: None,
        types: curved_rows(fields, "DAMA"),
    }
}

/// Hardcoded actor values live in the base master, which SeventySix.esm itself is.
fn is_stat_actor_value(raw: u32) -> bool {
    STAT_ACTOR_VALUES.contains(&raw)
}

/// `flatten_armo_damage_curves`'s resistance: the curve read at FO76's max drop level.
fn armor_resistance(json: &str) -> Result<u32, String> {
    curve_points(json).map(|points| curve_value_at_level(&points, f64::from(ARMOR_CURVE_LEVEL)))
}

/// Curve JSON loaded once per CURV, the curves the catalog references, and the ones that failed.
struct Curves<'a> {
    load: &'a dyn Fn(u32) -> Result<String, String>,
    texts: HashMap<u32, Result<String, String>>,
    used: BTreeSet<u32>,
    missing: BTreeSet<String>,
}

impl<'a> Curves<'a> {
    fn new(load: &'a dyn Fn(u32) -> Result<String, String>) -> Self {
        Self {
            load,
            texts: HashMap::new(),
            used: BTreeSet::new(),
            missing: BTreeSet::new(),
        }
    }

    fn text(&mut self, raw: u32) -> Result<&str, String> {
        let load = self.load;
        self.texts
            .entry(raw)
            .or_insert_with(|| load(raw))
            .as_deref()
            .map_err(String::clone)
    }

    fn reference(&mut self, raw: u32, keys: &dyn StatKeys) -> Option<String> {
        self.used.insert(raw);
        keys.source(raw)
    }

    /// The value a fixup computed from this curve; a failure is a curve it left unflattened.
    fn bake(
        &mut self,
        raw: u32,
        keys: &dyn StatKeys,
        bake: impl FnOnce(&str) -> Result<u32, String>,
    ) -> Option<u32> {
        match self.text(raw).and_then(bake) {
            Ok(value) => Some(value),
            Err(error) => {
                if let Some(key) = keys.source(raw) {
                    self.missing.insert(format!("{key}: {error}"));
                }
                None
            }
        }
    }

    fn finish(mut self, keys: &dyn StatKeys) -> (BTreeMap<String, Value>, Vec<String>) {
        let mut curves = BTreeMap::new();
        for raw in std::mem::take(&mut self.used) {
            let Some(key) = keys.source(raw) else {
                continue;
            };
            match self.text(raw).and_then(curve_points_authored) {
                Ok(points) => {
                    curves.insert(key, json!({ "points": points }));
                }
                Err(error) => {
                    self.missing.insert(format!("{key}: {error}"));
                }
            }
        }
        (curves, self.missing.into_iter().collect())
    }
}

fn fixup_reference(
    raw: u32,
    keys: &dyn StatKeys,
    unmapped: &mut BTreeSet<String>,
) -> Option<String> {
    let reference = keys.reference(raw);
    if reference.is_none()
        && let Some(key) = keys.source(raw)
    {
        unmapped.insert(key);
    }
    reference
}

/// The record's own Health and resistance rows, keyed by output actor value.
fn properties_json(
    rows: &[CurvedRow],
    keys: &dyn StatKeys,
    curves: &mut Curves<'_>,
    unmapped: &mut BTreeSet<String>,
) -> Value {
    let mut properties = BTreeMap::new();
    for row in rows.iter().filter(|row| is_stat_actor_value(row.form)) {
        let Some(actor_value) = fixup_reference(row.form, keys, unmapped) else {
            continue;
        };
        let mut property = Map::new();
        property.insert("value".into(), json!(f32::from_bits(row.bits)));
        if let Some(curve) = row.curve.and_then(|curve| curves.reference(curve, keys)) {
            property.insert("curve".into(), json!(curve));
        }
        properties
            .entry(actor_value)
            .or_insert(Value::Object(property));
    }
    json!(properties)
}

fn unarmed_json(
    weapon: &SourceDamage,
    keys: &dyn StatKeys,
    curves: &mut Curves<'_>,
    unmapped: &mut BTreeSet<String>,
) -> Map<String, Value> {
    let mut unarmed = Map::new();
    if let Some(curve) = weapon.curve.and_then(|curve| curves.reference(curve, keys)) {
        unarmed.insert("damage".into(), json!(curve));
    }
    let mut types = BTreeMap::new();
    for row in &weapon.types {
        let Some(curve) = row.curve else { continue };
        let Some(damage_type) = fixup_reference(row.form, keys, unmapped) else {
            continue;
        };
        if let Some(curve) = curves.reference(curve, keys) {
            types.entry(damage_type).or_insert(json!(curve));
        }
    }
    if !types.is_empty() {
        unarmed.insert("types".into(), json!(types));
    }
    unarmed
}

/// Damage-type rows a fixup flattened: curved, with a resolvable type and a baked value.
fn flattened_types<'r>(
    rows: impl Iterator<Item = &'r CurvedRow>,
    keys: &dyn StatKeys,
    curves: &mut Curves<'_>,
    unmapped: &mut BTreeSet<String>,
    bake: impl Fn(&mut Curves<'_>, u32) -> Option<u32>,
) -> BTreeMap<String, Value> {
    let mut types = BTreeMap::new();
    for row in rows {
        let Some(curve) = row.curve else { continue };
        let Some(damage_type) = fixup_reference(row.form, keys, unmapped) else {
            continue;
        };
        let Some(baked) = bake(curves, curve) else {
            continue;
        };
        types.entry(damage_type).or_insert_with(
            || json!({ "curve": curves.reference(curve, keys), "baked": f64::from(baked) }),
        );
    }
    types
}

fn catalog_json(
    source_plugin: &str,
    marker: Option<String>,
    source: &Source,
    keys: &dyn StatKeys,
    load_curve: &dyn Fn(u32) -> Result<String, String>,
) -> Value {
    let mut curves = Curves::new(load_curve);
    let mut unmapped = BTreeSet::new();

    let mut npcs = BTreeMap::new();
    for (output, raw, npc) in &source.npcs {
        let mut entry = Map::new();
        entry.insert("source".into(), json!(keys.source(*raw)));
        entry.insert("editor_id".into(), json!(npc.editor_id));
        entry.insert("auto_calc".into(), json!(npc.auto_calc));
        for (name, curve) in [
            ("health_curve", npc.health_curve),
            ("xp_curve", npc.xp_curve),
        ] {
            if let Some(curve) = curve.and_then(|curve| curves.reference(curve, keys)) {
                entry.insert(name.into(), json!(curve));
            }
        }
        entry.insert(
            "properties".into(),
            properties_json(&npc.properties, keys, &mut curves, &mut unmapped),
        );
        if let Some(race) = npc.race.and_then(|race| keys.source(race)) {
            entry.insert("race".into(), json!(race));
        }
        npcs.insert(output.clone(), Value::Object(entry));
    }

    let mut races = BTreeMap::new();
    for (raw, race) in &source.races {
        let Some(key) = keys.source(*raw) else {
            continue;
        };
        let mut entry = Map::new();
        if let Some(output) = keys.output(*raw) {
            entry.insert("output".into(), json!(output));
        }
        entry.insert("editor_id".into(), json!(race.editor_id));
        entry.insert("xp".into(), json!(race.xp));
        entry.insert(
            "properties".into(),
            properties_json(&race.properties, keys, &mut curves, &mut unmapped),
        );
        let unarmed = race
            .unarmed
            .and_then(|weapon| source.unarmed.get(&weapon))
            .map(|weapon| unarmed_json(weapon, keys, &mut curves, &mut unmapped))
            .unwrap_or_default();
        if !unarmed.is_empty() {
            entry.insert("unarmed".into(), Value::Object(unarmed));
        }
        races.insert(key, Value::Object(entry));
    }

    // synthesize_weap_data_blocks: CVT0 and curved DAMA rows become the curve mean.
    let mut weapons = BTreeMap::new();
    for (output, weapon) in &source.weapons {
        let mut entry = Map::new();
        entry.insert("editor_id".into(), json!(weapon.editor_id));
        if let Some(curve) = weapon.curve
            && let Some(mean) = curves.bake(curve, keys, mean_curve_value)
        {
            let damage = json!({ "curve": curves.reference(curve, keys), "baked": f64::from(curve_damage_base(mean)) });
            entry.insert("damage".into(), damage);
        }
        let types = flattened_types(
            weapon.types.iter(),
            keys,
            &mut curves,
            &mut unmapped,
            |curves, curve| curves.bake(curve, keys, mean_curve_value),
        );
        if !types.is_empty() {
            entry.insert("types".into(), json!(types));
        }
        if entry.len() > 1 {
            weapons.insert(output.clone(), Value::Object(entry));
        }
    }

    // flatten_expl_damage_curves: the curve mean, written as f32.
    let mut explosions = BTreeMap::new();
    for (output, explosion) in &source.explosions {
        let Some(curve) = explosion.curve else {
            continue;
        };
        let Some(mean) = curves.bake(curve, keys, mean_curve_value) else {
            continue;
        };
        let entry = json!({
            "editor_id": explosion.editor_id,
            "curve": curves.reference(curve, keys),
            "baked": f64::from(mean as f32),
        });
        explosions.insert(output.clone(), entry);
    }

    // flatten_armo_damage_curves: rows authored as 0 get a nonzero level-50 value.
    let mut armors = BTreeMap::new();
    for (output, armor) in &source.armors {
        let rows = armor.types.iter().filter(|row| row.bits == 0);
        let types = flattened_types(rows, keys, &mut curves, &mut unmapped, |curves, curve| {
            curves
                .bake(curve, keys, armor_resistance)
                .filter(|value| *value != 0)
        });
        if !types.is_empty() {
            armors.insert(
                output.clone(),
                json!({ "editor_id": armor.editor_id, "types": types }),
            );
        }
    }

    let (curves, missing_curves) = curves.finish(keys);
    json!({
        "version": SCHEMA_VERSION,
        "settings": {"source_plugin": source_plugin, "marker": marker},
        "curves": curves,
        "npcs": npcs,
        "races": races,
        "weapons": weapons,
        "explosions": explosions,
        "armors": armors,
        "diagnostics": {
            "npcs": npcs.len(),
            "races": races.len(),
            "weapons": weapons.len(),
            "explosions": explosions.len(),
            "armors": armors.len(),
            "missing_curves": missing_curves,
            "unmapped": unmapped,
        },
    })
}

fn read<R>(
    scan: &HandleRawScan<'_>,
    raw: u32,
    signature: &str,
    decode: impl FnOnce(&[ParsedSubrecord]) -> R,
) -> Option<R> {
    scan.with_record(raw, |record| {
        (record.flags & DELETED == 0 && record.signature.as_str() == signature)
            .then(|| decode(&effective_subrecords_for_record(record)))
    })
    .flatten()
}

/// Own output object id -> the source FormID it was converted from, from the mapper.
fn converted_sources(
    ctx: &PhaseCtx<'_>,
    keys: &Resolved,
    output_plugin: &str,
) -> HashMap<u32, u32> {
    let interner = &ctx.run.interner;
    let Some(state) = ctx.run.mapper_state.as_ref() else {
        return HashMap::new();
    };
    let source_index = |plugin: &str| {
        if plugin.eq_ignore_ascii_case(&keys.plugin) {
            Some(keys.masters.len())
        } else {
            keys.masters
                .iter()
                .position(|master| master.eq_ignore_ascii_case(plugin))
        }
    };
    state
        .source_to_target
        .iter()
        .filter_map(|(source, target)| {
            interner
                .resolve(target.plugin)
                .filter(|plugin| plugin.eq_ignore_ascii_case(output_plugin))?;
            let index = source_index(interner.resolve(source.plugin)?)?;
            Some((
                target.local & 0xFF_FFFF,
                ((index as u32) << 24) | (source.local & 0xFF_FFFF),
            ))
        })
        .collect()
}

#[derive(Default)]
struct Targets {
    npcs: Vec<(u32, u32)>,
    weapons: Vec<(u32, u32)>,
    explosions: Vec<(u32, u32)>,
    armors: Vec<(u32, u32)>,
}

/// The NPC's PRPS has the marker row with its own object id. A materialized clone
/// carries its template's id instead and is described by the template's entry.
fn names_itself(fields: &[ParsedSubrecord], marker: u32, local: u32) -> bool {
    let row = [marker.to_le_bytes(), (local as f32).to_le_bytes()].concat();
    subrecord(fields, "PRPS").is_some_and(|data| {
        data.chunks_exact(FO4_PROPERTY_ROW_LEN)
            .any(|candidate| candidate == row.as_slice())
    })
}

/// Own converted records as (output object id, source FormID); master overrides are skipped.
fn read_targets(
    scan: &HandleRawScan<'_>,
    own_index: u32,
    marker: Option<u32>,
    source_of: &dyn Fn(u32) -> Option<u32>,
) -> Targets {
    let own = |signature: &str| -> Vec<(u32, u32, u32)> {
        let sig = SigCode::from_str(signature).expect("four-character signature");
        scan.raw_form_ids_of_sig(sig)
            .into_iter()
            .filter(|raw| raw >> 24 == own_index)
            .filter_map(|raw| {
                source_of(raw & 0xFF_FFFF).map(|source| (raw, raw & 0xFF_FFFF, source))
            })
            .collect()
    };
    let pairs = |rows: Vec<(u32, u32, u32)>| {
        rows.into_iter()
            .map(|(_, local, source)| (local, source))
            .collect()
    };
    let npcs = marker.map_or_else(Vec::new, |marker| {
        own("NPC_")
            .into_iter()
            .filter(|(raw, local, _)| {
                read(scan, *raw, "NPC_", |fields| {
                    names_itself(fields, marker, *local)
                }) == Some(true)
            })
            .map(|(_, local, source)| (local, source))
            .collect()
    });
    Targets {
        npcs,
        weapons: pairs(own("WEAP")),
        explosions: pairs(own("EXPL")),
        armors: pairs(own("ARMO")),
    }
}

/// The catalog's source records, plus each CURV's JSON path.
fn read_source(
    scan: &HandleRawScan<'_>,
    targets: &Targets,
    output_plugin: &str,
    explosion_curve_offset: Option<usize>,
) -> (Source, HashMap<u32, String>) {
    let output = |local: u32| format!("{output_plugin}:{local:06X}");
    let records = |pairs: &[(u32, u32)],
                   signature: &str,
                   decode: &dyn Fn(&[ParsedSubrecord]) -> SourceDamage| {
        pairs
            .iter()
            .filter_map(|&(local, raw)| {
                read(scan, raw, signature, decode).map(|record| (output(local), record))
            })
            .collect::<Vec<_>>()
    };
    let mut source = Source {
        npcs: targets
            .npcs
            .iter()
            .filter_map(|&(local, raw)| {
                read(scan, raw, "NPC_", decode_npc).map(|npc| (output(local), raw, npc))
            })
            .collect(),
        weapons: records(&targets.weapons, "WEAP", &decode_weapon),
        armors: records(&targets.armors, "ARMO", &decode_armor),
        ..Source::default()
    };
    if let Some(offset) = explosion_curve_offset {
        source.explosions = records(&targets.explosions, "EXPL", &|fields| {
            decode_explosion(fields, offset)
        });
    }
    for raw in scan.raw_form_ids_of_sig(SigCode(*b"RACE")) {
        if let Some(race) = read(scan, raw, "RACE", decode_race) {
            source.races.insert(raw, race);
        }
    }
    let unarmed: BTreeSet<u32> = source
        .races
        .values()
        .filter_map(|race| race.unarmed)
        .collect();
    for raw in unarmed {
        if let Some(weapon) = read(scan, raw, "WEAP", decode_weapon) {
            source.unarmed.insert(raw, weapon);
        }
    }
    let curve_paths = scan
        .raw_form_ids_of_sig(SigCode(*b"CURV"))
        .into_iter()
        .filter_map(|raw| {
            read(scan, raw, "CURV", curve_json_path)
                .flatten()
                .map(|path| (raw, path))
        })
        .collect();
    (source, curve_paths)
}

/// Source actor values and damage types the catalog names.
fn reference_forms(source: &Source) -> BTreeSet<u32> {
    let properties = source
        .npcs
        .iter()
        .flat_map(|(_, _, npc)| &npc.properties)
        .chain(source.races.values().flat_map(|race| &race.properties))
        .filter(|row| is_stat_actor_value(row.form));
    let damage_types = source
        .weapons
        .iter()
        .chain(&source.armors)
        .map(|(_, record)| record)
        .chain(source.unarmed.values())
        .flat_map(|record| &record.types);
    properties.chain(damage_types).map(|row| row.form).collect()
}

/// What the fixups wrote for each source actor value or damage type
/// (`resolve_source_fk_to_target_raw`): the mapped target, else the same object id in Fallout4.esm.
fn fixup_references(
    ctx: &PhaseCtx<'_>,
    keys: &Resolved,
    target_masters: &[String],
    wanted: &BTreeSet<u32>,
) -> HashMap<u32, String> {
    let interner = &ctx.run.interner;
    let state = ctx.run.mapper_state.as_ref();
    let fallout4 = target_masters
        .first()
        .filter(|master| master.eq_ignore_ascii_case("Fallout4.esm"));
    wanted
        .iter()
        .filter_map(|&raw| {
            let owner = keys.owner(raw)?;
            let local = raw & 0xFF_FFFF;
            let mapped = state
                .and_then(|state| {
                    state.source_to_target.get(&FormKey {
                        local,
                        plugin: interner.intern(owner),
                    })
                })
                .and_then(|target| {
                    Some(format!(
                        "{}:{:06X}",
                        interner.resolve(target.plugin)?,
                        target.local & 0xFF_FFFF
                    ))
                });
            let fallback = || {
                fallout4
                    .filter(|_| owner == keys.plugin)
                    .map(|master| format!("{master}:{local:06X}"))
            };
            Some((raw, mapped.or_else(fallback)?))
        })
        .collect()
}

impl Phase for NpcStatsCatalogPhase {
    fn name(&self) -> &'static str {
        "emit_npc_stats_catalog"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        if ctx.mod_path.as_os_str().is_empty() {
            return Err(PhaseError::BadParams(
                "NPC stats catalog requires mod_path".into(),
            ));
        }
        let output_plugin = ctx.run.config.output_plugin_name.clone();
        let mut session = open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
            .map_err(internal)?;
        let (masters, plugin) = session
            .handle_load_order(ctx.run.source_handle_id)
            .map(|(masters, plugin)| (masters.to_vec(), plugin.to_string()))
            .map_err(internal)?;
        let target_masters = session.target_masters().to_vec();
        let own_index = target_masters.len() as u32;
        let mut keys = Resolved {
            masters,
            plugin,
            outputs: HashMap::new(),
            factions: HashSet::new(),
        };

        let interner = &ctx.run.interner;
        let marker = ctx
            .run
            .mapper_state
            .as_ref()
            .and_then(|state| {
                state
                    .source_to_target
                    .get(&curve_source_marker_key(interner))
            })
            .filter(|marker| {
                interner
                    .resolve(marker.plugin)
                    .is_some_and(|name| name.eq_ignore_ascii_case(&output_plugin))
            })
            .map(|marker| marker.local & 0xFF_FFFF);
        let converted = converted_sources(ctx, &keys, &output_plugin);
        // `source_key_for_target`: an output that keeps the source's name keeps unmapped ids.
        let same_name = (output_plugin.eq_ignore_ascii_case(&keys.plugin))
            .then_some((keys.masters.len() as u32) << 24);
        let source_of = |local: u32| {
            converted
                .get(&local)
                .copied()
                .or(same_name.map(|own| own | local))
        };
        let targets = {
            let scan = session
                .handle_raw_scan(ctx.run.target_handle_id)
                .map_err(internal)?;
            read_targets(
                &scan,
                own_index,
                marker.map(|local| (own_index << 24) | local),
                &source_of,
            )
        };
        ctx.check_cancel()?;
        let explosion_curve_offset =
            expl_data_offset(&ctx.run.schema_source, "damage_curve_table", None);
        let (source, curve_paths) = {
            let scan = session
                .handle_raw_scan(ctx.run.source_handle_id)
                .map_err(internal)?;
            read_source(&scan, &targets, &output_plugin, explosion_curve_offset)
        };
        ctx.check_cancel()?;
        let races: BTreeSet<u32> = source.races.keys().copied().collect();
        keys.outputs = resolve_outputs(ctx, &mut session, &keys, &races)?;
        drop(session);
        let references = fixup_references(ctx, &keys, &target_masters, &reference_forms(&source));
        let keys = SessionKeys {
            resolved: keys,
            references,
        };

        let source_root = ctx
            .run
            .config
            .source_extracted_dir
            .as_deref()
            .unwrap_or(ctx.source_extracted_dir);
        let load_curve = |raw: u32| -> Result<String, String> {
            let relative = curve_paths.get(&raw).ok_or("no CRVE/JASF")?;
            let path = curve_path(source_root, relative).map_err(|error| error.to_string())?;
            std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))
        };
        let marker = marker.map(|local| format!("{output_plugin}:{local:06X}"));
        let catalog = catalog_json(&keys.resolved.plugin, marker, &source, &keys, &load_curve);
        write_catalog(ctx.mod_path, "NpcStats", &output_plugin, &catalog)?;
        let diagnostics = &catalog["diagnostics"];
        let warnings: usize = ["missing_curves", "unmapped"]
            .iter()
            .map(|name| diagnostics[*name].as_array().map_or(0, Vec::len))
            .sum();
        Ok(PhaseReport {
            assets_written: 1,
            warnings: u32::try_from(warnings).unwrap_or(u32::MAX),
            ..PhaseReport::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };
    use smallvec::SmallVec;

    use super::*;
    use crate::fixups::flatten_npc_property_curves::curve_source_marker_key;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::run::{RunConfig, RunParams, create_run, drop_run, with_run};
    use crate::sym::StringInterner;
    use crate::translator::Game;

    const HEALTH: u32 = 0x0002D4;
    const STRENGTH: u32 = 0x0002C2;
    const DAMAGE_RESIST: u32 = 0x0002E3;
    const ENERGY_RESIST: u32 = 0x0002EB;
    const DT_ENERGY: u32 = 0x060A81;
    const DT_FIRE: u32 = 0x060A85;
    const DT_RADS: u32 = 0x060A87;
    const DT_PHYSICAL: u32 = 0x060A7F;
    // Authored out of order on purpose: FO76 reads points as written.
    const HEALTH_CURVE: &str = r#"{"curve":[{"x":1,"y":36},{"x":100,"y":842},{"x":23,"y":176}]}"#;
    const DAMAGE_CURVE: &str = r#"{"curve":[{"x":1,"y":10},{"x":50,"y":30},{"x":100,"y":500}]}"#;

    fn field(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: data.into(),
            semantic_type: None,
        }
    }

    fn form_field(signature: &str, raw: u32) -> ParsedSubrecord {
        field(signature, raw.to_le_bytes().to_vec())
    }

    fn rows(rows: &[(u32, u32, u32)]) -> Vec<u8> {
        rows.iter()
            .flat_map(|(form, bits, curve)| {
                [form.to_le_bytes(), bits.to_le_bytes(), curve.to_le_bytes()].concat()
            })
            .collect()
    }

    fn race_data(xp: u16) -> Vec<u8> {
        let mut data = vec![0u8; 146];
        data[144..146].copy_from_slice(&xp.to_le_bytes());
        data
    }

    fn explosion_data(curve: u32) -> Vec<u8> {
        let mut data = vec![0u8; 96];
        data[24..28].copy_from_slice(&curve.to_le_bytes());
        data
    }

    struct FakeKeys {
        outputs: HashMap<u32, String>,
        references: HashMap<u32, String>,
    }

    impl StatKeys for FakeKeys {
        fn source(&self, raw: u32) -> Option<String> {
            (raw >> 24 == 0).then(|| format!("SeventySix.esm:{:06X}", raw & 0xFF_FFFF))
        }
        fn output(&self, raw: u32) -> Option<String> {
            self.outputs.get(&raw).cloned()
        }
        fn reference(&self, raw: u32) -> Option<String> {
            self.references.get(&raw).cloned()
        }
    }

    fn fallout4(raw: u32) -> (u32, String) {
        (raw, format!("Fallout4.esm:{raw:06X}"))
    }

    #[test]
    fn source_records_decode_from_fo76_subrecords() {
        let npc = decode_npc(&[
            field("EDID", b"EncScorched_Template_Tier01\0".to_vec()),
            field("ACBS", {
                let mut acbs = vec![0u8; 20];
                acbs[0..4].copy_from_slice(&0x0000_0010u32.to_le_bytes());
                acbs
            }),
            form_field("RNAM", 0x06B4EC),
            form_field("CVT0", 0x7ABE3E),
            form_field("CVT2", 0x7ABE40),
            field(
                "PRPS",
                rows(&[
                    (HEALTH, 5.0f32.to_bits(), 0x7ABE3E),
                    (STRENGTH, 3.0f32.to_bits(), 0),
                ]),
            ),
        ]);
        assert_eq!(
            npc,
            SourceNpc {
                editor_id: "EncScorched_Template_Tier01".into(),
                auto_calc: true,
                health_curve: Some(0x7ABE3E),
                xp_curve: Some(0x7ABE40),
                properties: vec![
                    CurvedRow {
                        form: HEALTH,
                        bits: 5.0f32.to_bits(),
                        curve: Some(0x7ABE3E)
                    },
                    CurvedRow {
                        form: STRENGTH,
                        bits: 3.0f32.to_bits(),
                        curve: None
                    },
                ],
                race: Some(0x06B4EC),
            }
        );
        assert!(!decode_npc(&[field("ACBS", vec![0u8; 20])]).auto_calc);

        let race = decode_race(&[
            field("EDID", b"FeralGhoulRace\0".to_vec()),
            field("DATA", race_data(12)),
            field("PRPS", rows(&[(DAMAGE_RESIST, 0, 0x100)])),
            form_field("UNWP", 0x200),
        ]);
        assert_eq!(race.xp, 12);
        assert_eq!(race.unarmed, Some(0x200));
        assert_eq!(
            race.properties,
            vec![CurvedRow {
                form: DAMAGE_RESIST,
                bits: 0,
                curve: Some(0x100)
            }]
        );

        let weapon = decode_weapon(&[
            form_field("CVT0", 0x301),
            field("DAMA", rows(&[(DT_ENERGY, 0, 0x302)])),
        ]);
        assert_eq!(weapon.curve, Some(0x301));
        assert_eq!(
            weapon.types,
            vec![CurvedRow {
                form: DT_ENERGY,
                bits: 0,
                curve: Some(0x302)
            }]
        );
        assert_eq!(
            decode_explosion(&[field("DATA", explosion_data(0x401))], 24).curve,
            Some(0x401)
        );
        assert_eq!(
            decode_explosion(&[field("DATA", explosion_data(0))], 24).curve,
            None
        );
        assert_eq!(
            decode_armor(&[field("DAMA", rows(&[(DT_FIRE, 25, 0)]))])
                .types
                .len(),
            1
        );
    }

    #[test]
    fn catalog_lists_curves_by_record_and_the_values_the_fixups_baked() {
        let curves: HashMap<u32, &str> = [
            (0x7ABE3E, HEALTH_CURVE),
            (0x7ABE40, HEALTH_CURVE),
            (0x100, DAMAGE_CURVE),
            (0x201, DAMAGE_CURVE),
            (0x202, DAMAGE_CURVE),
            (0x301, DAMAGE_CURVE),
            (0x302, DAMAGE_CURVE),
            (0x401, DAMAGE_CURVE),
            (0x501, DAMAGE_CURVE),
        ]
        .into_iter()
        .collect();
        let load = |raw: u32| {
            curves
                .get(&raw)
                .map(|text| text.to_string())
                .ok_or_else(|| format!("{raw:06X}.json missing"))
        };
        let keys = FakeKeys {
            outputs: [(0x06B4EC, "SeventySix.esm:06B4EC".to_string())]
                .into_iter()
                .collect(),
            references: [
                fallout4(HEALTH),
                fallout4(DAMAGE_RESIST),
                fallout4(DT_ENERGY),
                fallout4(DT_FIRE),
                fallout4(DT_RADS),
            ]
            .into_iter()
            .collect(),
        };
        let source = Source {
            npcs: vec![(
                "SeventySix.esm:043C75".into(),
                0x043C75,
                SourceNpc {
                    editor_id: "EncScorched".into(),
                    auto_calc: true,
                    health_curve: Some(0x7ABE3E),
                    xp_curve: Some(0x7ABE40),
                    properties: vec![
                        CurvedRow {
                            form: HEALTH,
                            bits: 0,
                            curve: Some(0x7ABE3E),
                        },
                        CurvedRow {
                            form: STRENGTH,
                            bits: 3.0f32.to_bits(),
                            curve: None,
                        },
                        CurvedRow {
                            form: DAMAGE_RESIST,
                            bits: 15.0f32.to_bits(),
                            curve: None,
                        },
                        CurvedRow {
                            form: ENERGY_RESIST,
                            bits: 0,
                            curve: Some(0x100),
                        },
                    ],
                    race: Some(0x06B4EC),
                },
            )],
            races: [(
                0x06B4EC,
                SourceRace {
                    editor_id: "FeralGhoulRace".into(),
                    xp: 0,
                    properties: vec![CurvedRow {
                        form: DAMAGE_RESIST,
                        bits: 0,
                        curve: Some(0x100),
                    }],
                    unarmed: Some(0x200),
                },
            )]
            .into_iter()
            .collect(),
            unarmed: [(
                0x200,
                SourceDamage {
                    editor_id: "FeralGhoulUnarmed".into(),
                    curve: Some(0x201),
                    types: vec![CurvedRow {
                        form: DT_ENERGY,
                        bits: 0,
                        curve: Some(0x202),
                    }],
                },
            )]
            .into_iter()
            .collect(),
            weapons: vec![
                (
                    "SeventySix.esm:000300".into(),
                    SourceDamage {
                        editor_id: "CattleProd".into(),
                        curve: Some(0x301),
                        types: vec![
                            CurvedRow {
                                form: DT_ENERGY,
                                bits: 0,
                                curve: Some(0x302),
                            },
                            CurvedRow {
                                form: DT_PHYSICAL,
                                bits: 10,
                                curve: None,
                            },
                        ],
                    },
                ),
                (
                    "SeventySix.esm:000310".into(),
                    SourceDamage {
                        editor_id: "Flat".into(),
                        curve: None,
                        types: vec![],
                    },
                ),
            ],
            explosions: vec![(
                "SeventySix.esm:000400".into(),
                SourceDamage {
                    editor_id: "ExplosionM79".into(),
                    curve: Some(0x401),
                    types: vec![],
                },
            )],
            armors: vec![(
                "SeventySix.esm:000500".into(),
                SourceDamage {
                    editor_id: "Armor_T65_Torso".into(),
                    curve: None,
                    types: vec![
                        CurvedRow {
                            form: DT_ENERGY,
                            bits: 0,
                            curve: Some(0x501),
                        },
                        CurvedRow {
                            form: DT_FIRE,
                            bits: 25,
                            curve: Some(0x501),
                        },
                        CurvedRow {
                            form: DT_RADS,
                            bits: 0,
                            curve: Some(0x502),
                        },
                    ],
                },
            )],
        };

        let catalog = catalog_json(
            "SeventySix.esm",
            Some("SeventySix.esm:000801".into()),
            &source,
            &keys,
            &load,
        );

        assert_eq!(catalog["version"], 1);
        assert_eq!(catalog["settings"]["source_plugin"], "SeventySix.esm");
        assert_eq!(catalog["settings"]["marker"], "SeventySix.esm:000801");
        assert_eq!(
            catalog["curves"]["SeventySix.esm:7ABE3E"]["points"],
            json!([[1.0, 36.0], [100.0, 842.0], [23.0, 176.0]])
        );
        assert!(catalog["curves"].get("SeventySix.esm:000502").is_none());

        let npc = &catalog["npcs"]["SeventySix.esm:043C75"];
        assert_eq!(npc["source"], "SeventySix.esm:043C75");
        assert_eq!(npc["editor_id"], "EncScorched");
        assert_eq!(npc["auto_calc"], true);
        assert_eq!(npc["health_curve"], "SeventySix.esm:7ABE3E");
        assert_eq!(npc["xp_curve"], "SeventySix.esm:7ABE40");
        assert_eq!(npc["race"], "SeventySix.esm:06B4EC");
        assert_eq!(
            npc["properties"],
            json!({
                "Fallout4.esm:0002D4": {"value": 0.0, "curve": "SeventySix.esm:7ABE3E"},
                "Fallout4.esm:0002E3": {"value": 15.0},
            })
        );

        let race = &catalog["races"]["SeventySix.esm:06B4EC"];
        assert_eq!(race["output"], "SeventySix.esm:06B4EC");
        assert_eq!(race["editor_id"], "FeralGhoulRace");
        assert_eq!(race["xp"], 0);
        assert_eq!(
            race["properties"],
            json!({"Fallout4.esm:0002E3": {"value": 0.0, "curve": "SeventySix.esm:000100"}})
        );
        assert_eq!(
            race["unarmed"],
            json!({"damage": "SeventySix.esm:000201", "types": {"Fallout4.esm:060A81": "SeventySix.esm:000202"}})
        );

        // mean(x <= 50) of DAMAGE_CURVE = 20; level 50 = 30.
        assert_eq!(
            catalog["weapons"],
            json!({"SeventySix.esm:000300": {
                "editor_id": "CattleProd",
                "damage": {"curve": "SeventySix.esm:000301", "baked": 20.0},
                "types": {"Fallout4.esm:060A81": {"curve": "SeventySix.esm:000302", "baked": 20.0}},
            }})
        );
        assert_eq!(
            catalog["explosions"],
            json!({"SeventySix.esm:000400": {"editor_id": "ExplosionM79", "curve": "SeventySix.esm:000401", "baked": 20.0}})
        );
        assert_eq!(
            catalog["armors"],
            json!({"SeventySix.esm:000500": {
                "editor_id": "Armor_T65_Torso",
                "types": {"Fallout4.esm:060A81": {"curve": "SeventySix.esm:000501", "baked": 30.0}},
            }})
        );

        let diagnostics = &catalog["diagnostics"];
        assert_eq!(
            (
                &diagnostics["npcs"],
                &diagnostics["races"],
                &diagnostics["weapons"],
                &diagnostics["explosions"],
                &diagnostics["armors"]
            ),
            (&json!(1), &json!(1), &json!(1), &json!(1), &json!(1))
        );
        assert_eq!(
            diagnostics["missing_curves"],
            json!(["SeventySix.esm:000502: 000502.json missing"])
        );
        assert_eq!(diagnostics["unmapped"], json!(["SeventySix.esm:0002EB"]));
    }

    #[test]
    fn phase_exports_only_own_npcs_naming_themselves_and_own_flattened_weapons() {
        let source_handle = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let own = |local| FormKey { local, plugin };
        let master = |local| FormKey {
            local,
            plugin: fallout4,
        };
        let record = |sig: &str, form_key: FormKey, fields: Vec<(&str, Vec<u8>)>| Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key,
            eid: Some(interner.intern(&format!("{sig}{:06X}", form_key.local))),
            flags: RecordFlags::empty(),
            fields: fields
                .into_iter()
                .map(|(sig, bytes)| FieldEntry {
                    sig: SubrecordSig::from_str(sig).unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
                })
                .collect(),
            warnings: SmallVec::new(),
        };
        let marker = own(0x000801);
        let marker_raw = 0x0100_0801u32;
        let marked = |rows: &[(u32, f32)]| {
            rows.iter()
                .flat_map(|(form, value)| [form.to_le_bytes(), value.to_le_bytes()].concat())
                .collect::<Vec<u8>>()
        };
        let curve_raw = 0x7ABE3Eu32;

        {
            let mut session = crate::session::open_session(source_handle, None).unwrap();
            let schema = session.schema().unwrap();
            let mut acbs = vec![0u8; 20];
            acbs[0..4].copy_from_slice(&0x10u32.to_le_bytes());
            for source in [
                record(
                    "NPC_",
                    own(0x043C75),
                    vec![
                        ("ACBS", acbs),
                        ("RNAM", 0x06B4ECu32.to_le_bytes().to_vec()),
                        ("PRPS", rows(&[(HEALTH, 0, curve_raw)])),
                        ("CVT0", curve_raw.to_le_bytes().to_vec()),
                    ],
                ),
                record(
                    "NPC_",
                    own(0x05A1B3),
                    vec![("PRPS", rows(&[(HEALTH, 0, curve_raw)]))],
                ),
                record(
                    "RACE",
                    own(0x06B4EC),
                    vec![
                        ("DATA", race_data(7)),
                        ("UNWP", 0x0A0001u32.to_le_bytes().to_vec()),
                    ],
                ),
                record(
                    "WEAP",
                    own(0x0A0001),
                    vec![("CVT0", curve_raw.to_le_bytes().to_vec())],
                ),
                record(
                    "WEAP",
                    own(0x0B0001),
                    vec![("CVT0", curve_raw.to_le_bytes().to_vec())],
                ),
                record(
                    "WEAP",
                    own(0x0B0002),
                    vec![("CVT0", curve_raw.to_le_bytes().to_vec())],
                ),
                Record {
                    sig: SigCode::from_str("CURV").unwrap(),
                    form_key: own(curve_raw),
                    eid: Some(interner.intern("CT_Health")),
                    flags: RecordFlags::empty(),
                    fields: smallvec::smallvec![FieldEntry {
                        sig: SubrecordSig::from_str("JASF").unwrap(),
                        value: FieldValue::String(interner.intern("Creatures\\Health.json")),
                    }],
                    warnings: SmallVec::new(),
                },
            ] {
                session
                    .add_record(source, schema.as_ref(), &interner)
                    .unwrap();
            }
        }
        {
            let mut session = crate::session::open_session(target_handle, None).unwrap();
            let schema = session.schema().unwrap();
            for target in [
                record(
                    "NPC_",
                    own(0x043C75),
                    vec![(
                        "PRPS",
                        marked(&[(HEALTH, 36.0), (marker_raw, 0x043C75 as f32)]),
                    )],
                ),
                // A materialized clone keeps its template's marker.
                record(
                    "NPC_",
                    own(0x0F0001),
                    vec![("PRPS", marked(&[(marker_raw, 0x043C75 as f32)]))],
                ),
                record(
                    "NPC_",
                    master(0x000007),
                    vec![("PRPS", marked(&[(marker_raw, 7.0)]))],
                ),
                record("RACE", own(0x06B4EC), vec![]),
                record("WEAP", own(0x0B0001), vec![]),
                record("WEAP", master(0x000008), vec![]),
                record("AVIF", marker, vec![]),
            ] {
                session
                    .add_record(target, schema.as_ref(), &interner)
                    .unwrap();
            }
        }

        let temp_root = std::env::temp_dir().join(format!(
            "bacup_npc_stats_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let curve_path = temp_root.join("extracted/misc/curvetables/json/Creatures/Health.json");
        std::fs::create_dir_all(curve_path.parent().unwrap()).unwrap();
        std::fs::write(&curve_path, HEALTH_CURVE).unwrap();
        let mod_path = temp_root.join("mod");

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "SeventySix.esm".into(),
                is_whole_plugin: true,
                source_extracted_dir: Some(temp_root.join("extracted")),
                ..Default::default()
            },
        })
        .unwrap();
        let report = with_run(id, |run| {
            let mut state = MapperState::new(
                [],
                MapperOptions {
                    output_plugin_name: "SeventySix.esm".into(),
                    target_master_names: vec!["Fallout4.esm".into()],
                    preserve_source_ids: true,
                    ..Default::default()
                },
            );
            let run_plugin = run.interner.intern("SeventySix.esm");
            let run_fallout4 = run.interner.intern("Fallout4.esm");
            let key = |local, plugin| FormKey { local, plugin };
            state.source_to_target.insert(
                curve_source_marker_key(&run.interner),
                key(0x000801, run_plugin),
            );
            state
                .source_to_target
                .insert(key(0x06B4EC, run_plugin), key(0x06B4EC, run_plugin));
            state
                .source_to_target
                .insert(key(0x05A1B3, run_plugin), key(0x000007, run_fallout4));
            state
                .source_to_target
                .insert(key(0x0B0002, run_plugin), key(0x000008, run_fallout4));
            run.mapper_state = Some(state);
            let cancel = AtomicBool::new(false);
            let params = json!({});
            let extracted = temp_root.join("extracted");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &extracted,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            NpcStatsCatalogPhase
                .run(&mut ctx)
                .map_err(|error| crate::run::RunError::InvalidConfig(error.to_string()))
        })
        .unwrap();
        drop_run(id).unwrap();
        let written = std::fs::read_to_string(
            mod_path.join("F4SE/Plugins/B21_TalesFromAppalachia/NpcStats/SeventySix.esm.json"),
        );
        let _ = std::fs::remove_dir_all(&temp_root);
        assert!(plugin_handle_close_native(source_handle));
        assert!(plugin_handle_close_native(target_handle));
        let catalog: Value = serde_json::from_str(&written.expect("catalog written")).unwrap();

        assert_eq!(report.assets_written, 1);
        assert_eq!(catalog["settings"]["marker"], "SeventySix.esm:000801");
        let npcs: Vec<&String> = catalog["npcs"].as_object().unwrap().keys().collect();
        assert_eq!(npcs, vec!["SeventySix.esm:043C75"]);
        let npc = &catalog["npcs"]["SeventySix.esm:043C75"];
        assert_eq!(npc["auto_calc"], true);
        assert_eq!(npc["race"], "SeventySix.esm:06B4EC");
        assert_eq!(npc["health_curve"], "SeventySix.esm:7ABE3E");
        assert_eq!(
            npc["properties"]["Fallout4.esm:0002D4"]["curve"],
            "SeventySix.esm:7ABE3E"
        );
        let race = &catalog["races"]["SeventySix.esm:06B4EC"];
        assert_eq!(
            (&race["output"], &race["xp"]),
            (&json!("SeventySix.esm:06B4EC"), &json!(7))
        );
        assert_eq!(race["unarmed"]["damage"], "SeventySix.esm:7ABE3E");
        let weapons: Vec<&String> = catalog["weapons"].as_object().unwrap().keys().collect();
        assert_eq!(weapons, vec!["SeventySix.esm:0B0001"]);
        // mean(x <= 50) of HEALTH_CURVE: (36 + 176) / 2.
        assert_eq!(
            catalog["weapons"]["SeventySix.esm:0B0001"]["damage"]["baked"],
            106.0
        );
        assert_eq!(
            catalog["curves"]["SeventySix.esm:7ABE3E"]["points"],
            json!([[1.0, 36.0], [100.0, 842.0], [23.0, 176.0]])
        );
        assert_eq!(catalog["diagnostics"]["missing_curves"], json!([]));
    }
}
