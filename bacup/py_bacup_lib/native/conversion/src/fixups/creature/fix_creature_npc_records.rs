//! Fixup: fix FO76-specific issues in creature NPC and Race records.
//!

//!
//! # Branches implemented at the raw-record level
//! Python's data model is the post-translation YAML canonical dict; the Rust
//! pipeline operates on schema-decoded `Record` / `FieldValue` produced by
//! `source_read::read_record`.  For subrecords whose codec is `struct:...` /
//! `array_struct:...`, `read_record` keeps the payload as `FieldValue::Bytes`
//! (the typed struct decode lands in a later phase).  The branches below were
//! ported by operating directly on those bytes; the FormID master-byte is
//! always `0x00` because every FO4-target plugin lists `Fallout4.esm` as
//! master index 0 (same convention as `augment_creature_factions`).
//!
//! 1. **TPTA.ModelAnimation LVLN cleanup** — clear only the ModelAnimation
//!    template slot when it points at a leveled NPC list. FO4 allows LVLN in
//!    other template slots such as BaseData; those remain untouched.
//! 2. **ACBS.template_flags** — recompute FO4-supported template flags from
//!    active TPTA slots, clearing FO76-only bits that make CK walk invalid
//!    template slots.
//! 3. **AIDT defaults** — when `assistance` byte (offset 5) is zero, write
//!    `HelpsAllies` (1).  When `aggro_aggro_radius_behavior` (offset 6) is
//!    zero, fill the aggro defaults (warn=3000, warn_attack=2500, attack=2000,
//!    no_slow_approach=true).  Synthesises the 24-byte AIDT if missing.
//! 4. **Template NPCs (no TPTA) get the `ActorTypeCreature` keyword** —
//!    appended to KWDA (raw bytes) with the matching KSIZ count synced.
//! 5. **Creature combat perks** — `crCreatureMeleeDamage` (`0A2775`),
//!    `crCreatureRangedDamage` (`0A2776`), `crRegeneration` (`1504FC`) are
//!    appended as `PRKR` entries with rank=1 when not already present.
//!    PRKZ is synced to the new PRKR count.
//! 6. **PRKR rank defensive backfill** — entries whose 5th byte (rank) is
//!    zero are bumped to 1 to mirror the Python `Rank=1` invariant.
//!
//! `ACBS.AutoCalcStats` is deliberately preserved.  Vanilla FO4 uses it together
//! with `PCLevelMult` on leveled actors such as Deathclaws, and FO76 source
//! creature records such as `EncOgua` carry the same combination.
//!
//! # Not implemented at the raw-record level
//! - **TPTA remap** (FO76→FO4 slot drop + presence backfill): needs typed-struct
//!   decode to map slot index → field name.
//! - **PRPS curve-table flatten**: FO4 PRPS is `array_struct:I,f` with no
//!   CurveTable column; a no-op on pure FO4 input.
//! - **RACE `NoKnockdowns` / `ActorTypeAnimal`**: needs RACE `DATA` flag-bit
//!   identification.

use crate::fixups::creature::{
    creature_internal_fixup_applies, npc_internal_fixup_applies_to_record,
};
use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};
use crate::sym::StringInterner;
use rustc_hash::{FxHashMap, FxHashSet};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// `auto_calc_stats` bit in `NPC_.ACBS.flags` (matches FO76 and FO4 schemas).
#[cfg(test)]
const ACBS_FLAG_AUTO_CALC_STATS: u32 = 0x0000_0010;

/// ACBS struct size (`struct:I,h,H,H,H,h,H,H,B,B` = 4+2*7+1*2 = 20).
const ACBS_SIZE: usize = 20;

/// ACBS.template_flags offset inside `struct:I,h,H,H,H,h,H,H,B,B`.
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;

/// FO4-supported NPC template flags.
const TEMPLATE_TRAITS: u16 = 0x0001;
const TEMPLATE_STATS: u16 = 0x0002;
const TEMPLATE_FACTIONS: u16 = 0x0004;
const TEMPLATE_AI_DATA: u16 = 0x0010;
const TEMPLATE_AI_PACKAGES: u16 = 0x0020;
const TEMPLATE_MODEL_ANIMATION: u16 = 0x0040;
const TEMPLATE_BASE_DATA: u16 = 0x0080;
const TEMPLATE_INVENTORY: u16 = 0x0100;
const TEMPLATE_SCRIPT: u16 = 0x0200;

/// TPTA has 13 slots, but FO4 exposes template flags for only this subset.
const TPTA_SLOT_TEMPLATE_FLAGS: [u16; 13] = [
    TEMPLATE_TRAITS,
    TEMPLATE_STATS,
    TEMPLATE_FACTIONS,
    0,
    TEMPLATE_AI_DATA,
    TEMPLATE_AI_PACKAGES,
    TEMPLATE_MODEL_ANIMATION,
    TEMPLATE_BASE_DATA,
    TEMPLATE_INVENTORY,
    TEMPLATE_SCRIPT,
    0,
    0,
    0,
];

const TPTA_SLOT_BYTES: usize = 4;
const TPTA_MODEL_ANIMATION_SLOT_INDEX: usize = 6;
const TPTA_MODEL_ANIMATION_SLOT_OFFSET: usize = TPTA_MODEL_ANIMATION_SLOT_INDEX * TPTA_SLOT_BYTES;

/// AIDT struct size (`struct:B,B,B,B,B,B,B,B,I,I,I,B,B,B,B`).
const AIDT_SIZE: usize = 24;

/// `HelpsAllies` value in `assistance_enum`.
const AIDT_ASSISTANCE_HELPS_ALLIES: u8 = 1;

const AIDT_AGGRO_WARN_DEFAULT: u32 = 3000;
const AIDT_AGGRO_WARN_ATTACK_DEFAULT: u32 = 2500;
const AIDT_AGGRO_ATTACK_DEFAULT: u32 = 2000;

/// PRKR entry size (`struct:I,B` = 5 bytes).
const PRKR_SIZE: usize = 5;

/// FO4 `ActorTypeCreature` keyword raw FormID (`013795:Fallout4.esm`).
/// Fallout4.esm is master index 0 in every FO4-target plugin.
const ACTOR_TYPE_CREATURE_FORM_ID: u32 = 0x00_013795;

/// FO4 creature combat perk FormIDs (master index 0 = Fallout4.esm).
/// 1. `crCreatureMeleeDamage`  (`0A2775`)
/// 2. `crCreatureRangedDamage` (`0A2776`)
/// 3. `crRegeneration`         (`1504FC`)
const CREATURE_PERKS: &[u32] = &[0x00_0A2775, 0x00_0A2776, 0x00_1504FC];

// ---------------------------------------------------------------------------
// Subrecord-sig helpers
// ---------------------------------------------------------------------------

fn sig(name: &str) -> Option<SubrecordSig> {
    SubrecordSig::from_str(name).ok()
}

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FixCreatureNpcRecordsFixup;

enum NpcRecordEdit {
    Replace(Record),
    Warn(String),
}

impl Fixup for FixCreatureNpcRecordsFixup {
    fn name(&self) -> &'static str {
        "fix_creature_npc_records"
    }

    fn scope(&self) -> FixupScope {
        // Whole-plugin: self-gates per NPC on the creature predicate below.
        FixupScope::CreatureGated
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        creature_internal_fixup_applies(ctx.config)
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        creature_internal_fixup_applies(config)
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let interner = mapper.interner;
        let lvln_sig =
            SigCode::from_str("LVLN").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_masters = session.target_masters().to_vec();
        let lvln_templates = build_lvln_template_resolution(
            session,
            target_schema,
            interner,
            target_masters.as_slice(),
            lvln_sig,
            npc_sig,
        )?;
        let mut warnings = Vec::new();
        let mut report = session.map_apply_by_sig(
            npc_sig,
            mapper,
            |view, _snapshot, fk| match view.record_decoded(fk, target_schema, interner) {
                Ok(mut record) => {
                    // Per-record gate (whole-plugin only): append
                    // ActorTypeCreature keyword + creature combat perks only on
                    // confirmed creatures. The predicate walks the template
                    // chain (UseTraits) and is conservative (Unknown/Human →
                    // skip), so HUMAN NPCs keep their data. No-op gate on a
                    // creature-graph walk (every NPC in scope is a creature).
                    if !npc_internal_fixup_applies_to_record(
                        &record,
                        view,
                        target_schema,
                        interner,
                        config,
                    ) {
                        return None;
                    }
                    apply_to_record_with_lvln_templates(&mut record, &lvln_templates)
                        .then_some(NpcRecordEdit::Replace(record))
                }
                Err(err) => Some(NpcRecordEdit::Warn(format!("fix_creature_npc_read:{err}"))),
            },
            |session, mapper, _fk, edit| match edit {
                NpcRecordEdit::Replace(record) => {
                    session
                        .replace_record(record, target_schema, mapper.interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    Ok(EditOutcome::Changed)
                }
                NpcRecordEdit::Warn(message) => {
                    warnings.push(mapper.interner.intern(&message));
                    Ok(EditOutcome::NoOp)
                }
            },
        )?;
        report.warnings.extend(warnings);
        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Apply every NPC_ branch ported above and return `true` when anything
/// changed.
pub fn apply_to_record(record: &mut Record) -> bool {
    let lvln_templates = LvlnTemplateResolution::default();
    apply_to_record_with_lvln_templates(record, &lvln_templates)
}

fn apply_to_record_with_lvln_templates(
    record: &mut Record,
    lvln_templates: &LvlnTemplateResolution,
) -> bool {
    let mut changed = false;

    changed |= resolve_lvln_template_slots(record, lvln_templates);
    changed |= clear_lvln_model_animation_template_slot(record, &lvln_templates.all_lvln_raw);
    changed |= sync_acbs_template_flags_from_tpta(record);
    changed |= apply_aidt_defaults(record);

    let has_tpta = sig("TPTA")
        .map(|s| record.fields.iter().any(|e| e.sig == s))
        .unwrap_or(false);
    let is_template_npc = !has_tpta;
    if is_template_npc {
        changed |= append_actor_type_creature_keyword(record);
    }

    changed |= append_creature_combat_perks(record);
    changed |= backfill_perk_ranks(record);
    changed |= sync_prkz_count(record);

    changed
}

#[derive(Default)]
struct LvlnTemplateResolution {
    all_lvln_raw: FxHashSet<u32>,
    raw_to_lvln_fk: FxHashMap<u32, FormKey>,
    raw_by_fk: FxHashMap<FormKey, u32>,
    lvln_entries: FxHashMap<FormKey, Vec<LvlnTemplateEntry>>,
    npc_infos: FxHashMap<FormKey, NpcTemplateInfo>,
}

#[derive(Clone, Copy)]
struct LvlnTemplateEntry {
    level: u16,
    target: FormKey,
}

#[derive(Clone)]
struct NpcTemplateInfo {
    race: Option<FormKey>,
    has_tpta: bool,
    editor_id_template_hint: bool,
}

fn build_lvln_template_resolution(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
    target_masters: &[String],
    lvln_sig: SigCode,
    npc_sig: SigCode,
) -> Result<LvlnTemplateResolution, FixupError> {
    let target_plugin_name = session.target_slot().parsed.plugin_name.clone();
    let mut resolution = LvlnTemplateResolution::default();

    let lvln_fks = session
        .form_keys_of_sig(lvln_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in lvln_fks {
        if let Some(raw) = encode_target_form_id(fk, interner, target_masters) {
            resolution.all_lvln_raw.insert(raw);
            resolution.raw_to_lvln_fk.insert(raw, fk);
            resolution.raw_by_fk.insert(fk, raw);
        }
        let Ok(record) = session.record_decoded(&fk, target_schema, interner) else {
            continue;
        };
        let entries = lvln_template_entries(&record, target_masters, &target_plugin_name, interner);
        resolution.lvln_entries.insert(fk, entries);
    }

    let npc_fks = session
        .form_keys_of_sig(npc_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in npc_fks {
        if let Some(raw) = encode_target_form_id(fk, interner, target_masters) {
            resolution.raw_by_fk.insert(fk, raw);
        }
        let Ok(record) = session.record_decoded(&fk, target_schema, interner) else {
            continue;
        };
        resolution.npc_infos.insert(
            fk,
            NpcTemplateInfo {
                race: npc_race_from_record(&record, &resolution),
                has_tpta: record_has_subrecord(&record, "TPTA"),
                editor_id_template_hint: record_editor_id_contains(&record, interner, "template"),
            },
        );
    }

    if let Ok(race_sig) = SigCode::from_str("RACE") {
        if let Ok(race_fks) = session.form_keys_of_sig(race_sig, interner) {
            for fk in race_fks {
                if let Some(raw) = encode_target_form_id(fk, interner, target_masters) {
                    resolution.raw_by_fk.insert(fk, raw);
                }
            }
        }
    }

    Ok(resolution)
}

fn lvln_template_entries(
    lvln: &Record,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<LvlnTemplateEntry> {
    let Some(lvlo_sig) = sig("LVLO") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in &lvln.fields {
        if entry.sig != lvlo_sig {
            continue;
        }
        if let Some(target) =
            lvlo_target_form_key(&entry.value, target_masters, target_plugin_name, interner)
        {
            out.push(LvlnTemplateEntry {
                level: lvlo_level(&entry.value, interner).unwrap_or(1),
                target,
            });
        }
    }
    out
}

fn lvlo_target_form_key(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
            let raw = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
            form_key_from_raw(raw, target_masters, target_plugin_name, interner)
        }
        FieldValue::Struct(fields) => {
            for wanted in ["NPC", "npc", "Reference", "reference"] {
                if let Some(value) = named_struct_value(fields, wanted, interner) {
                    if let Some(fk) =
                        form_key_from_value(value, target_masters, target_plugin_name, interner)
                    {
                        return Some(fk);
                    }
                }
            }
            None
        }
        _ => form_key_from_value(value, target_masters, target_plugin_name, interner),
    }
}

fn lvlo_level(value: &FieldValue, interner: &StringInterner) -> Option<u16> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes(bytes[0..2].try_into().ok()?))
        }
        FieldValue::Struct(fields) => {
            for wanted in ["Level", "level"] {
                if let Some(value) = named_struct_value(fields, wanted, interner) {
                    return field_value_to_u16(value);
                }
            }
            None
        }
        _ => None,
    }
}

fn named_struct_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    wanted: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(name, _)| {
            interner
                .resolve(*name)
                .is_some_and(|actual| actual == wanted)
        })
        .map(|(_, value)| value)
}

fn field_value_to_u16(value: &FieldValue) -> Option<u16> {
    match value {
        FieldValue::Uint(value) => u16::try_from(*value).ok(),
        FieldValue::Int(value) => u16::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u16::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u16)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes(bytes[0..2].try_into().ok()?))
        }
        _ => None,
    }
}

fn form_key_from_value(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
            form_key_from_raw(*raw as u32, target_masters, target_plugin_name, interner)
        }
        FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => {
            form_key_from_raw(*raw as u32, target_masters, target_plugin_name, interner)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
            form_key_from_raw(raw, target_masters, target_plugin_name, interner)
        }
        _ => None,
    }
}

fn form_key_from_raw(
    raw: u32,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let load_index = (raw >> 24) as usize;
    let plugin_name = target_masters
        .get(load_index)
        .map(String::as_str)
        .unwrap_or(target_plugin_name);
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin_name),
    })
}

fn resolve_lvln_template_slots(record: &mut Record, resolution: &LvlnTemplateResolution) -> bool {
    if resolution.raw_to_lvln_fk.is_empty() && resolution.lvln_entries.is_empty() {
        return false;
    }
    let desired_race = npc_race_from_record(record, resolution);
    let Some(tplt_sig) = sig("TPLT") else {
        return false;
    };
    let Some(tpta_sig) = sig("TPTA") else {
        return false;
    };

    let mut changed = false;
    for entry in &mut record.fields {
        if entry.sig != tplt_sig && entry.sig != tpta_sig {
            continue;
        }
        changed |= replace_lvln_template_refs_in_value(&mut entry.value, desired_race, resolution);
    }
    changed
}

fn replace_lvln_template_refs_in_value(
    value: &mut FieldValue,
    desired_race: Option<FormKey>,
    resolution: &LvlnTemplateResolution,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) => {
            let mut changed = false;
            for offset in (0..bytes.len()).step_by(4) {
                let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
                    continue;
                };
                let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let Some(lvln_fk) = resolution.raw_to_lvln_fk.get(&raw).copied() else {
                    continue;
                };
                let Some(replacement_fk) =
                    select_lvln_template_replacement(lvln_fk, desired_race, resolution)
                else {
                    continue;
                };
                let Some(replacement_raw) = resolution.raw_by_fk.get(&replacement_fk).copied()
                else {
                    continue;
                };
                chunk.copy_from_slice(&replacement_raw.to_le_bytes());
                changed = true;
            }
            changed
        }
        FieldValue::FormKey(fk) => {
            if !resolution.lvln_entries.contains_key(fk) {
                return false;
            }
            let Some(replacement_fk) =
                select_lvln_template_replacement(*fk, desired_race, resolution)
            else {
                return false;
            };
            *fk = replacement_fk;
            true
        }
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
            let raw32 = *raw as u32;
            let Some(lvln_fk) = resolution.raw_to_lvln_fk.get(&raw32).copied() else {
                return false;
            };
            let Some(replacement_fk) =
                select_lvln_template_replacement(lvln_fk, desired_race, resolution)
            else {
                return false;
            };
            let Some(replacement_raw) = resolution.raw_by_fk.get(&replacement_fk).copied() else {
                return false;
            };
            *raw = u64::from(replacement_raw);
            true
        }
        FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => {
            let raw32 = *raw as u32;
            let Some(lvln_fk) = resolution.raw_to_lvln_fk.get(&raw32).copied() else {
                return false;
            };
            let Some(replacement_fk) =
                select_lvln_template_replacement(lvln_fk, desired_race, resolution)
            else {
                return false;
            };
            let Some(replacement_raw) = resolution.raw_by_fk.get(&replacement_fk).copied() else {
                return false;
            };
            *raw = i64::from(replacement_raw);
            true
        }
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            replace_lvln_template_refs_in_value(item, desired_race, resolution) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            replace_lvln_template_refs_in_value(item, desired_race, resolution) | changed
        }),
        _ => false,
    }
}

fn select_lvln_template_replacement(
    lvln_fk: FormKey,
    desired_race: Option<FormKey>,
    resolution: &LvlnTemplateResolution,
) -> Option<FormKey> {
    let mut visiting = FxHashSet::default();
    select_lvln_template_replacement_inner(lvln_fk, desired_race, resolution, &mut visiting)
}

fn select_lvln_template_replacement_inner(
    lvln_fk: FormKey,
    desired_race: Option<FormKey>,
    resolution: &LvlnTemplateResolution,
    visiting: &mut FxHashSet<FormKey>,
) -> Option<FormKey> {
    if !visiting.insert(lvln_fk) {
        return None;
    }
    let Some(entries) = resolution.lvln_entries.get(&lvln_fk) else {
        visiting.remove(&lvln_fk);
        return None;
    };

    let mut best_matching: Option<(CandidateScore, FormKey)> = None;
    let mut best_any: Option<(CandidateScore, FormKey)> = None;
    let mut distinct_races: FxHashSet<FormKey> = FxHashSet::default();

    for (order, entry) in entries.iter().enumerate() {
        let candidate_fk = if resolution.npc_infos.contains_key(&entry.target) {
            Some(entry.target)
        } else if resolution.lvln_entries.contains_key(&entry.target) {
            select_lvln_template_replacement_inner(entry.target, desired_race, resolution, visiting)
        } else {
            None
        };
        let Some(candidate_fk) = candidate_fk else {
            continue;
        };
        let Some(info) = resolution.npc_infos.get(&candidate_fk) else {
            continue;
        };
        if let Some(race) = info.race {
            distinct_races.insert(race);
        }
        let score = CandidateScore {
            template_rank: npc_template_rank(info),
            level: entry.level,
            order: order as u32,
        };
        if desired_race.is_some_and(|race| info.race == Some(race)) {
            update_best_candidate(&mut best_matching, score, candidate_fk);
        }
        update_best_candidate(&mut best_any, score, candidate_fk);
    }

    visiting.remove(&lvln_fk);

    if let Some((_, candidate)) = best_matching {
        return Some(candidate);
    }
    if desired_race.is_none() || distinct_races.len() <= 1 {
        return best_any.map(|(_, candidate)| candidate);
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateScore {
    template_rank: u8,
    level: u16,
    order: u32,
}

fn update_best_candidate(
    best: &mut Option<(CandidateScore, FormKey)>,
    score: CandidateScore,
    candidate: FormKey,
) {
    if best
        .as_ref()
        .map(|(best_score, _)| score < *best_score)
        .unwrap_or(true)
    {
        *best = Some((score, candidate));
    }
}

fn npc_template_rank(info: &NpcTemplateInfo) -> u8 {
    if !info.has_tpta {
        0
    } else if info.editor_id_template_hint {
        1
    } else {
        2
    }
}

fn npc_race_from_record(record: &Record, resolution: &LvlnTemplateResolution) -> Option<FormKey> {
    let Some(rnam_sig) = sig("RNAM") else {
        return None;
    };
    record
        .fields
        .iter()
        .find(|entry| entry.sig == rnam_sig)
        .and_then(|entry| match &entry.value {
            FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                let raw = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
                resolution.raw_to_form_key(raw)
            }
            FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
                resolution.raw_to_form_key(*raw as u32)
            }
            FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => {
                resolution.raw_to_form_key(*raw as u32)
            }
            _ => None,
        })
}

impl LvlnTemplateResolution {
    fn raw_to_form_key(&self, raw: u32) -> Option<FormKey> {
        if raw == 0 {
            return None;
        }
        self.raw_by_fk
            .iter()
            .find_map(|(fk, candidate_raw)| (*candidate_raw == raw).then_some(*fk))
    }
}

fn record_has_subrecord(record: &Record, sig_str: &str) -> bool {
    sig(sig_str)
        .map(|sub_sig| record.fields.iter().any(|entry| entry.sig == sub_sig))
        .unwrap_or(false)
}

fn record_editor_id_contains(
    record: &Record,
    interner: &StringInterner,
    needle_lower: &str,
) -> bool {
    let eid = record
        .eid
        .and_then(|sym| interner.resolve(sym))
        .map(str::to_ascii_lowercase)
        .or_else(|| {
            let edid_sig = sig("EDID")?;
            record.fields.iter().find_map(|entry| {
                if entry.sig != edid_sig {
                    return None;
                }
                match &entry.value {
                    FieldValue::String(sym) => interner.resolve(*sym).map(str::to_ascii_lowercase),
                    FieldValue::Bytes(bytes) => {
                        let end = bytes
                            .iter()
                            .position(|byte| *byte == 0)
                            .unwrap_or(bytes.len());
                        Some(String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase())
                    }
                    _ => None,
                }
            })
        });
    eid.as_deref().is_some_and(|eid| eid.contains(needle_lower))
}

fn clear_lvln_model_animation_template_slot(
    record: &mut Record,
    lvln_template_formids: &FxHashSet<u32>,
) -> bool {
    if lvln_template_formids.is_empty() {
        return false;
    }
    let Some(tpta_sig) = sig("TPTA") else {
        return false;
    };

    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if entry.sig != tpta_sig {
            continue;
        }
        let FieldValue::Bytes(data) = &mut entry.value else {
            continue;
        };
        let Some(slot) =
            data.get_mut(TPTA_MODEL_ANIMATION_SLOT_OFFSET..TPTA_MODEL_ANIMATION_SLOT_OFFSET + 4)
        else {
            continue;
        };
        let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
        if raw == 0 || !lvln_template_formids.contains(&raw) {
            continue;
        }
        slot.copy_from_slice(&0u32.to_le_bytes());
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Branch 1 — sync ACBS.template_flags from TPTA
// ---------------------------------------------------------------------------

fn sync_acbs_template_flags_from_tpta(record: &mut Record) -> bool {
    let Some(acbs_sig) = sig("ACBS") else {
        return false;
    };
    let Some(tpta_sig) = sig("TPTA") else {
        return false;
    };

    let template_flags = record
        .fields
        .iter()
        .find(|entry| entry.sig == tpta_sig)
        .and_then(|entry| match &entry.value {
            FieldValue::Bytes(data) => Some(template_flags_for_tpta_bytes(data.as_slice())),
            _ => None,
        })
        .unwrap_or(0);

    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if entry.sig != acbs_sig {
            continue;
        }
        let FieldValue::Bytes(data) = &mut entry.value else {
            continue;
        };
        if data.len() < ACBS_TEMPLATE_FLAGS_OFFSET + 2 {
            continue;
        }
        let current = u16::from_le_bytes([
            data[ACBS_TEMPLATE_FLAGS_OFFSET],
            data[ACBS_TEMPLATE_FLAGS_OFFSET + 1],
        ]);
        if current == template_flags {
            continue;
        }
        let bytes = template_flags.to_le_bytes();
        data[ACBS_TEMPLATE_FLAGS_OFFSET] = bytes[0];
        data[ACBS_TEMPLATE_FLAGS_OFFSET + 1] = bytes[1];
        changed = true;
    }
    changed
}

fn template_flags_for_tpta_bytes(data: &[u8]) -> u16 {
    let mut flags = 0_u16;
    for (slot_index, flag) in TPTA_SLOT_TEMPLATE_FLAGS.iter().enumerate() {
        if *flag == 0 {
            continue;
        }
        let offset = slot_index * 4;
        let Some(bytes) = data.get(offset..offset + 4) else {
            continue;
        };
        let raw = u32::from_le_bytes(bytes.try_into().unwrap());
        if raw != 0 {
            flags |= *flag;
        }
    }
    flags
}

// ---------------------------------------------------------------------------
// Branch 2 — AIDT defaults
// ---------------------------------------------------------------------------

fn apply_aidt_defaults(record: &mut Record) -> bool {
    let Some(aidt_sig) = sig("AIDT") else {
        return false;
    };

    let mut existing_idx: Option<usize> = None;
    for (i, e) in record.fields.iter().enumerate() {
        if e.sig == aidt_sig {
            existing_idx = Some(i);
            break;
        }
    }

    match existing_idx {
        Some(i) => {
            let entry = &mut record.fields[i];
            let FieldValue::Bytes(data) = &mut entry.value else {
                return false;
            };
            if data.len() < AIDT_SIZE {
                // Extend with zeros so the slot offsets are valid.
                data.resize(AIDT_SIZE, 0);
            }
            let mut local_changed = false;
            // Assistance @ offset 5.
            if data[5] == 0 {
                data[5] = AIDT_ASSISTANCE_HELPS_ALLIES;
                local_changed = true;
            }
            // Aggro defaults gated on aggro_aggro_radius_behavior @ offset 6.
            if data[6] == 0 {
                data[6] = 1; // bool true
                write_u32_le(data, 8, AIDT_AGGRO_WARN_DEFAULT);
                write_u32_le(data, 12, AIDT_AGGRO_WARN_ATTACK_DEFAULT);
                write_u32_le(data, 16, AIDT_AGGRO_ATTACK_DEFAULT);
                data[20] = 1; // no_slow_approach = true
                local_changed = true;
            }
            local_changed
        }
        None => {
            let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
            data.resize(AIDT_SIZE, 0);
            data[5] = AIDT_ASSISTANCE_HELPS_ALLIES;
            data[6] = 1;
            write_u32_le(&mut data, 8, AIDT_AGGRO_WARN_DEFAULT);
            write_u32_le(&mut data, 12, AIDT_AGGRO_WARN_ATTACK_DEFAULT);
            write_u32_le(&mut data, 16, AIDT_AGGRO_ATTACK_DEFAULT);
            data[20] = 1;
            record.fields.push(FieldEntry {
                sig: aidt_sig,
                value: FieldValue::Bytes(data),
            });
            true
        }
    }
}

// ---------------------------------------------------------------------------
// Branch 3 — append ActorTypeCreature keyword on template NPC (no TPTA)
// ---------------------------------------------------------------------------

fn append_actor_type_creature_keyword(record: &mut Record) -> bool {
    let Some(ksiz_sig) = sig("KSIZ") else {
        return false;
    };
    let Some(kwda_sig) = sig("KWDA") else {
        return false;
    };

    // Locate (or create) KWDA payload.
    let mut kwda_idx: Option<usize> = None;
    for (i, e) in record.fields.iter().enumerate() {
        if e.sig == kwda_sig {
            kwda_idx = Some(i);
            break;
        }
    }

    let mut already_present = false;
    if let Some(i) = kwda_idx {
        if let FieldValue::Bytes(data) = &record.fields[i].value {
            for chunk in data.chunks_exact(4) {
                let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                if (raw & 0x00FF_FFFF) == ACTOR_TYPE_CREATURE_FORM_ID & 0x00FF_FFFF {
                    already_present = true;
                    break;
                }
            }
        }
    }
    if already_present {
        return false;
    }

    // Append FormID to KWDA.
    if let Some(i) = kwda_idx {
        if let FieldValue::Bytes(data) = &mut record.fields[i].value {
            data.extend_from_slice(&ACTOR_TYPE_CREATURE_FORM_ID.to_le_bytes());
        } else {
            return false;
        }
    } else {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        data.extend_from_slice(&ACTOR_TYPE_CREATURE_FORM_ID.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: kwda_sig,
            value: FieldValue::Bytes(data),
        });
    }

    // Sync (or create) KSIZ to match the new KWDA row count.
    update_count_subrecord(record, ksiz_sig, kwda_sig, 4);
    true
}

/// Recompute the uint32 count subrecord (`count_sig`) so it matches the row
/// count of the array subrecord (`array_sig`).
///
/// Repeated subrecords (`repeatable: true` in the schema, e.g. `PRKR`) come
/// back as one `FieldEntry` per row from `read_record`, each holding exactly
/// `row_size` payload bytes.  Bulk-packed array subrecords (`KWDA`'s
/// `formid_array`) come back as a single `FieldEntry` whose payload holds N
/// `row_size`-byte rows.  Sum across both shapes so the count is right in
/// every case.  Creates the count subrecord if it is missing.
fn update_count_subrecord(
    record: &mut Record,
    count_sig: SubrecordSig,
    array_sig: SubrecordSig,
    row_size: usize,
) {
    let mut row_count: u32 = 0;
    for e in record.fields.iter() {
        if e.sig != array_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &e.value {
            if row_size == 0 {
                continue;
            }
            row_count += (data.len() / row_size) as u32;
        }
    }

    let new_bytes = row_count.to_le_bytes();
    for e in record.fields.iter_mut() {
        if e.sig != count_sig {
            continue;
        }
        if let FieldValue::Uint(_) = &e.value {
            e.value = FieldValue::Uint(row_count as u64);
            return;
        }
        if let FieldValue::Bytes(data) = &mut e.value {
            if data.len() >= 4 {
                data[0] = new_bytes[0];
                data[1] = new_bytes[1];
                data[2] = new_bytes[2];
                data[3] = new_bytes[3];
                return;
            }
        }
    }

    // No existing count subrecord — append one.
    let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
    data.extend_from_slice(&new_bytes);
    record.fields.push(FieldEntry {
        sig: count_sig,
        value: FieldValue::Bytes(data),
    });
}

// ---------------------------------------------------------------------------
// Branch 4 — append creature combat perks
// ---------------------------------------------------------------------------

fn append_creature_combat_perks(record: &mut Record) -> bool {
    let Some(prkr_sig) = sig("PRKR") else {
        return false;
    };

    let mut existing: Vec<u32> = Vec::new();
    for e in record.fields.iter() {
        if e.sig != prkr_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &e.value {
            if data.len() >= 4 {
                let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                existing.push(raw & 0x00FF_FFFF);
            }
        }
    }

    let mut changed = false;
    for &perk_fid in CREATURE_PERKS {
        let needle = perk_fid & 0x00FF_FFFF;
        if existing.iter().any(|p| *p == needle) {
            continue;
        }
        let mut payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        payload.extend_from_slice(&perk_fid.to_le_bytes());
        payload.push(1); // rank = 1
        record.fields.push(FieldEntry {
            sig: prkr_sig,
            value: FieldValue::Bytes(payload),
        });
        existing.push(needle);
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Branch 5 — defensive PRKR.rank backfill
// ---------------------------------------------------------------------------

fn backfill_perk_ranks(record: &mut Record) -> bool {
    let Some(prkr_sig) = sig("PRKR") else {
        return false;
    };
    let mut changed = false;
    for e in record.fields.iter_mut() {
        if e.sig != prkr_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &mut e.value {
            if data.len() == PRKR_SIZE && data[4] == 0 {
                data[4] = 1;
                changed = true;
            }
        }
    }
    changed
}

// ---------------------------------------------------------------------------
// PRKZ count sync
// ---------------------------------------------------------------------------

fn sync_prkz_count(record: &mut Record) -> bool {
    let Some(prkz_sig) = sig("PRKZ") else {
        return false;
    };
    let Some(prkr_sig) = sig("PRKR") else {
        return false;
    };

    let perk_count = record.fields.iter().filter(|e| e.sig == prkr_sig).count() as u32;
    if perk_count == 0 {
        return false;
    }

    let mut existing_count: Option<u32> = None;
    for e in record.fields.iter() {
        if e.sig != prkz_sig {
            continue;
        }
        match &e.value {
            FieldValue::Uint(n) => existing_count = Some(*n as u32),
            FieldValue::Bytes(data) if data.len() >= 4 => {
                existing_count = Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]));
            }
            _ => {}
        }
        break;
    }

    if existing_count == Some(perk_count) {
        return false;
    }

    update_count_subrecord(record, prkz_sig, prkr_sig, PRKR_SIZE);
    true
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    let bytes = value.to_le_bytes();
    buf[offset] = bytes[0];
    buf[offset + 1] = bytes[1];
    buf[offset + 2] = bytes[2];
    buf[offset + 3] = bytes[3];
}

// Constants referenced via assertions in tests to surface drift quickly.
#[allow(dead_code)]
const _: () = assert!(ACBS_SIZE == 20);
#[allow(dead_code)]
const _: () = assert!(AIDT_SIZE == 24);
#[allow(dead_code)]
const _: () = assert!(PRKR_SIZE == 5);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::{FixupConfig, FixupContext, FixupRegistry};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // Test record builders
    // -----------------------------------------------------------------------

    fn npc(local: u32, plugin: &str, interner: &StringInterner) -> Record {
        let sig = SigCode::from_str("NPC_").unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn push_bytes(record: &mut Record, sig_str: &str, data: Vec<u8>) {
        let s = SubrecordSig::from_str(sig_str).unwrap();
        let mut buf: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        buf.extend_from_slice(&data);
        record.fields.push(FieldEntry {
            sig: s,
            value: FieldValue::Bytes(buf),
        });
    }

    fn push_formkey(record: &mut Record, sig_str: &str, fk: FormKey) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig_str).unwrap(),
            value: FieldValue::FormKey(fk),
        });
    }

    fn make_acbs(flags: u32) -> Vec<u8> {
        let mut buf = vec![0u8; ACBS_SIZE];
        buf[0..4].copy_from_slice(&flags.to_le_bytes());
        buf
    }

    fn make_acbs_with_template_flags(flags: u32, template_flags: u16) -> Vec<u8> {
        let mut buf = make_acbs(flags);
        buf[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
            .copy_from_slice(&template_flags.to_le_bytes());
        buf
    }

    fn make_tpta(slots: [u32; 13]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(13 * 4);
        for slot in slots {
            buf.extend_from_slice(&slot.to_le_bytes());
        }
        buf
    }

    fn make_aidt_zeroed() -> Vec<u8> {
        vec![0u8; AIDT_SIZE]
    }

    fn first_bytes<'a>(record: &'a Record, sig_str: &str) -> Option<&'a [u8]> {
        let s = SubrecordSig::from_str(sig_str).ok()?;
        for e in record.fields.iter() {
            if e.sig == s {
                if let FieldValue::Bytes(data) = &e.value {
                    return Some(data.as_slice());
                }
            }
        }
        None
    }

    fn count_fields(record: &Record, sig_str: &str) -> usize {
        let s = SubrecordSig::from_str(sig_str).unwrap();
        record.fields.iter().filter(|e| e.sig == s).count()
    }

    fn first_tpta_slot(record: &Record, slot_index: usize) -> u32 {
        let data = first_bytes(record, "TPTA").expect("TPTA must be present");
        let offset = slot_index * TPTA_SLOT_BYTES;
        u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
    }

    fn acbs_template_flags(record: &Record) -> u16 {
        let data = first_bytes(record, "ACBS").expect("ACBS must be present");
        u16::from_le_bytes([
            data[ACBS_TEMPLATE_FLAGS_OFFSET],
            data[ACBS_TEMPLATE_FLAGS_OFFSET + 1],
        ])
    }

    fn first_raw(record: &Record, sig_str: &str) -> u32 {
        let data = first_bytes(record, sig_str).expect("raw FormID field must be present");
        u32::from_le_bytes(data[0..4].try_into().unwrap())
    }

    #[test]
    fn preserves_acbs_auto_calc_stats_when_set() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000100, "Out.esp", &mut interner);
        // Flags = AutoCalcStats | Unique
        push_bytes(&mut r, "ACBS", make_acbs(0x10 | 0x20));
        let changed = apply_to_record(&mut r);
        assert!(changed);
        let data = first_bytes(&r, "ACBS").unwrap();
        let new = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        assert_eq!(new & 0x10, 0x10, "AutoCalcStats bit must be preserved");
        assert_eq!(new & 0x20, 0x20, "Unique must remain");
    }

    #[test]
    fn does_not_synthesize_acbs_auto_calc_stats_when_absent() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000101, "Out.esp", &mut interner);
        push_bytes(&mut r, "ACBS", make_acbs(0x20));
        let changed = apply_to_record(&mut r);
        assert!(changed);
        let data = first_bytes(&r, "ACBS").unwrap();
        let new = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        assert_eq!(new & 0x10, 0, "AutoCalcStats bit must remain absent");
    }

    #[test]
    fn syncs_acbs_template_flags_from_tpta_and_drops_fo76_only_bits() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000108, "Out.esp", &mut interner);
        push_bytes(&mut r, "ACBS", make_acbs_with_template_flags(0x18, 0x1F28));
        push_bytes(
            &mut r,
            "TPTA",
            make_tpta([
                0, 0, 0, 0x00157C5F, 0, 0x00157C5F, 0, 0, 0x00157C5F, 0x00157C5F, 0x00157C5F,
                0x00157C5F, 0x00157C5F,
            ]),
        );

        let changed = apply_to_record(&mut r);
        assert!(changed);
        let data = first_bytes(&r, "ACBS").unwrap();
        let flags = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let template_flags = u16::from_le_bytes([
            data[ACBS_TEMPLATE_FLAGS_OFFSET],
            data[ACBS_TEMPLATE_FLAGS_OFFSET + 1],
        ]);
        assert_eq!(flags & ACBS_FLAG_AUTO_CALC_STATS, ACBS_FLAG_AUTO_CALC_STATS);
        assert_eq!(
            template_flags,
            TEMPLATE_AI_PACKAGES | TEMPLATE_INVENTORY | TEMPLATE_SCRIPT
        );
    }

    #[test]
    fn clears_lvln_model_animation_template_but_keeps_lvln_base_data() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x00010B, "Out.esp", &mut interner);
        let model_animation_lvln = 0x07AA_BBCC;
        let base_data_lvln = 0x07DD_EEFF;
        push_bytes(
            &mut r,
            "ACBS",
            make_acbs_with_template_flags(0, TEMPLATE_MODEL_ANIMATION | TEMPLATE_BASE_DATA),
        );
        let mut slots = [0u32; 13];
        slots[TPTA_MODEL_ANIMATION_SLOT_INDEX] = model_animation_lvln;
        slots[7] = base_data_lvln;
        push_bytes(&mut r, "TPTA", make_tpta(slots));

        let mut lvln_templates = LvlnTemplateResolution::default();
        lvln_templates.all_lvln_raw.insert(model_animation_lvln);
        lvln_templates.all_lvln_raw.insert(base_data_lvln);
        let changed = apply_to_record_with_lvln_templates(&mut r, &lvln_templates);

        assert!(changed);
        assert_eq!(first_tpta_slot(&r, TPTA_MODEL_ANIMATION_SLOT_INDEX), 0);
        assert_eq!(first_tpta_slot(&r, 7), base_data_lvln);
        assert_eq!(
            acbs_template_flags(&r),
            TEMPLATE_BASE_DATA,
            "only ModelAnimation should be cleared"
        );
    }

    #[test]
    fn keeps_non_lvln_model_animation_template() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x00010C, "Out.esp", &mut interner);
        let model_animation_npc = 0x0712_3456;
        push_bytes(
            &mut r,
            "ACBS",
            make_acbs_with_template_flags(0, TEMPLATE_MODEL_ANIMATION),
        );
        let mut slots = [0u32; 13];
        slots[TPTA_MODEL_ANIMATION_SLOT_INDEX] = model_animation_npc;
        push_bytes(&mut r, "TPTA", make_tpta(slots));

        let lvln_templates = LvlnTemplateResolution::default();
        let changed = apply_to_record_with_lvln_templates(&mut r, &lvln_templates);

        assert!(changed);
        assert_eq!(
            first_tpta_slot(&r, TPTA_MODEL_ANIMATION_SLOT_INDEX),
            model_animation_npc
        );
        assert_eq!(acbs_template_flags(&r), TEMPLATE_MODEL_ANIMATION);
    }

    #[test]
    fn resolves_lvln_template_slots_to_same_race_template_npc() {
        let mut interner = StringInterner::new();
        let plugin = "Out.esp";
        let race = fk(0x00D191, plugin, &mut interner);
        let lvln_fk = fk(0x0026BD37, plugin, &mut interner);
        let child_fk = fk(0x003BA686, plugin, &mut interner);
        let template_fk = fk(0x0000D194, plugin, &mut interner);
        let lvln_raw = 0x0126_BD37;
        let child_raw = 0x013B_A686;
        let template_raw = 0x0100_D194;

        let mut resolution = LvlnTemplateResolution::default();
        resolution.all_lvln_raw.insert(lvln_raw);
        resolution.raw_to_lvln_fk.insert(lvln_raw, lvln_fk);
        resolution.raw_by_fk.insert(lvln_fk, lvln_raw);
        resolution.raw_by_fk.insert(child_fk, child_raw);
        resolution.raw_by_fk.insert(template_fk, template_raw);
        resolution.raw_by_fk.insert(race, 0x0100_D191);
        resolution.lvln_entries.insert(
            lvln_fk,
            vec![
                LvlnTemplateEntry {
                    level: 1,
                    target: child_fk,
                },
                LvlnTemplateEntry {
                    level: 1,
                    target: template_fk,
                },
            ],
        );
        resolution.npc_infos.insert(
            child_fk,
            NpcTemplateInfo {
                race: Some(race),
                has_tpta: true,
                editor_id_template_hint: false,
            },
        );
        resolution.npc_infos.insert(
            template_fk,
            NpcTemplateInfo {
                race: Some(race),
                has_tpta: false,
                editor_id_template_hint: true,
            },
        );

        let mut r = npc(0x00891912, plugin, &mut interner);
        push_bytes(
            &mut r,
            "ACBS",
            make_acbs_with_template_flags(0, TEMPLATE_TRAITS | TEMPLATE_STATS | TEMPLATE_BASE_DATA),
        );
        push_formkey(&mut r, "RNAM", race);
        push_bytes(&mut r, "TPLT", lvln_raw.to_le_bytes().to_vec());
        let mut slots = [0u32; 13];
        slots[0] = lvln_raw;
        slots[1] = lvln_raw;
        slots[7] = lvln_raw;
        push_bytes(&mut r, "TPTA", make_tpta(slots));

        let changed = apply_to_record_with_lvln_templates(&mut r, &resolution);

        assert!(changed);
        assert_eq!(first_raw(&r, "TPLT"), template_raw);
        assert_eq!(first_tpta_slot(&r, 0), template_raw);
        assert_eq!(first_tpta_slot(&r, 1), template_raw);
        assert_eq!(first_tpta_slot(&r, 7), template_raw);
        assert_eq!(
            acbs_template_flags(&r) & (TEMPLATE_TRAITS | TEMPLATE_STATS | TEMPLATE_BASE_DATA),
            TEMPLATE_TRAITS | TEMPLATE_STATS | TEMPLATE_BASE_DATA
        );
    }

    #[test]
    fn mixed_race_lvln_template_is_not_guessed_without_same_race_candidate() {
        let mut interner = StringInterner::new();
        let plugin = "Out.esp";
        let wanted_race = fk(0x000300, plugin, &mut interner);
        let race_a = fk(0x000100, plugin, &mut interner);
        let race_b = fk(0x000200, plugin, &mut interner);
        let lvln_fk = fk(0x000900, plugin, &mut interner);
        let npc_a = fk(0x000901, plugin, &mut interner);
        let npc_b = fk(0x000902, plugin, &mut interner);
        let lvln_raw = 0x0100_0900;

        let mut resolution = LvlnTemplateResolution::default();
        resolution.all_lvln_raw.insert(lvln_raw);
        resolution.raw_to_lvln_fk.insert(lvln_raw, lvln_fk);
        resolution.raw_by_fk.insert(lvln_fk, lvln_raw);
        resolution.raw_by_fk.insert(npc_a, 0x0100_0901);
        resolution.raw_by_fk.insert(npc_b, 0x0100_0902);
        resolution.raw_by_fk.insert(wanted_race, 0x0100_0300);
        resolution.lvln_entries.insert(
            lvln_fk,
            vec![
                LvlnTemplateEntry {
                    level: 1,
                    target: npc_a,
                },
                LvlnTemplateEntry {
                    level: 1,
                    target: npc_b,
                },
            ],
        );
        resolution.npc_infos.insert(
            npc_a,
            NpcTemplateInfo {
                race: Some(race_a),
                has_tpta: false,
                editor_id_template_hint: true,
            },
        );
        resolution.npc_infos.insert(
            npc_b,
            NpcTemplateInfo {
                race: Some(race_b),
                has_tpta: false,
                editor_id_template_hint: true,
            },
        );

        let mut r = npc(0x000A00, plugin, &mut interner);
        push_bytes(
            &mut r,
            "ACBS",
            make_acbs_with_template_flags(0, TEMPLATE_TRAITS),
        );
        push_formkey(&mut r, "RNAM", wanted_race);
        push_bytes(&mut r, "TPLT", lvln_raw.to_le_bytes().to_vec());
        let mut slots = [0u32; 13];
        slots[0] = lvln_raw;
        push_bytes(&mut r, "TPTA", make_tpta(slots));

        let _ = apply_to_record_with_lvln_templates(&mut r, &resolution);

        assert_eq!(first_raw(&r, "TPLT"), lvln_raw);
        assert_eq!(first_tpta_slot(&r, 0), lvln_raw);
    }

    #[test]
    fn mixed_race_lvln_template_uses_matching_race_even_when_not_first() {
        let mut interner = StringInterner::new();
        let plugin = "Out.esp";
        let race_a = fk(0x000100, plugin, &mut interner);
        let race_b = fk(0x000200, plugin, &mut interner);
        let lvln_fk = fk(0x000900, plugin, &mut interner);
        let npc_a = fk(0x000901, plugin, &mut interner);
        let npc_b = fk(0x000902, plugin, &mut interner);
        let lvln_raw = 0x0100_0900;
        let npc_b_raw = 0x0100_0902;

        let mut resolution = LvlnTemplateResolution::default();
        resolution.all_lvln_raw.insert(lvln_raw);
        resolution.raw_to_lvln_fk.insert(lvln_raw, lvln_fk);
        resolution.raw_by_fk.insert(lvln_fk, lvln_raw);
        resolution.raw_by_fk.insert(npc_a, 0x0100_0901);
        resolution.raw_by_fk.insert(npc_b, npc_b_raw);
        resolution.raw_by_fk.insert(race_b, 0x0100_0200);
        resolution.lvln_entries.insert(
            lvln_fk,
            vec![
                LvlnTemplateEntry {
                    level: 1,
                    target: npc_a,
                },
                LvlnTemplateEntry {
                    level: 1,
                    target: npc_b,
                },
            ],
        );
        resolution.npc_infos.insert(
            npc_a,
            NpcTemplateInfo {
                race: Some(race_a),
                has_tpta: false,
                editor_id_template_hint: true,
            },
        );
        resolution.npc_infos.insert(
            npc_b,
            NpcTemplateInfo {
                race: Some(race_b),
                has_tpta: true,
                editor_id_template_hint: false,
            },
        );

        let mut r = npc(0x000A00, plugin, &mut interner);
        push_bytes(
            &mut r,
            "ACBS",
            make_acbs_with_template_flags(0, TEMPLATE_TRAITS),
        );
        push_formkey(&mut r, "RNAM", race_b);
        push_bytes(&mut r, "TPLT", lvln_raw.to_le_bytes().to_vec());
        let mut slots = [0u32; 13];
        slots[0] = lvln_raw;
        push_bytes(&mut r, "TPTA", make_tpta(slots));

        let _ = apply_to_record_with_lvln_templates(&mut r, &resolution);

        assert_eq!(first_raw(&r, "TPLT"), npc_b_raw);
        assert_eq!(first_tpta_slot(&r, 0), npc_b_raw);
    }

    #[test]
    fn aidt_defaults_filled_on_zero_assistance() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000102, "Out.esp", &mut interner);
        push_bytes(&mut r, "AIDT", make_aidt_zeroed());
        let changed = apply_aidt_defaults(&mut r);
        assert!(changed);
        let data = first_bytes(&r, "AIDT").unwrap();
        assert_eq!(data[5], AIDT_ASSISTANCE_HELPS_ALLIES);
        assert_eq!(data[6], 1, "aggro_aggro_radius_behavior must be true");
        assert_eq!(
            u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            AIDT_AGGRO_WARN_DEFAULT
        );
        assert_eq!(
            u32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            AIDT_AGGRO_WARN_ATTACK_DEFAULT
        );
        assert_eq!(
            u32::from_le_bytes([data[16], data[17], data[18], data[19]]),
            AIDT_AGGRO_ATTACK_DEFAULT
        );
        assert_eq!(data[20], 1, "no_slow_approach must be true");
    }

    #[test]
    fn aidt_synthesised_when_missing() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000103, "Out.esp", &mut interner);
        let changed = apply_aidt_defaults(&mut r);
        assert!(changed);
        let data = first_bytes(&r, "AIDT").unwrap();
        assert_eq!(data.len(), AIDT_SIZE);
        assert_eq!(data[5], AIDT_ASSISTANCE_HELPS_ALLIES);
    }

    #[test]
    fn aidt_untouched_when_already_configured() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000104, "Out.esp", &mut interner);
        let mut buf = make_aidt_zeroed();
        buf[5] = 2; // HelpsFriendsAndAllies
        buf[6] = 1; // aggro behaviour true
        push_bytes(&mut r, "AIDT", buf);
        let changed = apply_aidt_defaults(&mut r);
        assert!(!changed);
    }

    #[test]
    fn template_npc_gains_actor_type_creature_keyword() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000105, "Out.esp", &mut interner);
        let changed = apply_to_record(&mut r);
        assert!(changed);

        let kwda = first_bytes(&r, "KWDA").expect("KWDA must be present");
        let raw = u32::from_le_bytes([kwda[0], kwda[1], kwda[2], kwda[3]]);
        assert_eq!(raw & 0x00FF_FFFF, ACTOR_TYPE_CREATURE_FORM_ID & 0x00FF_FFFF);

        let ksiz = first_bytes(&r, "KSIZ").expect("KSIZ must be present");
        let count = u32::from_le_bytes([ksiz[0], ksiz[1], ksiz[2], ksiz[3]]);
        assert_eq!(count, 1);
    }

    #[test]
    fn child_npc_with_tpta_skips_keyword_inject() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000106, "Out.esp", &mut interner);
        // Empty TPTA — its mere presence marks the record as a child NPC.
        push_bytes(&mut r, "TPTA", vec![0u8; 13 * 4]);
        let _ = apply_to_record(&mut r);
        assert_eq!(count_fields(&r, "KWDA"), 0);
        assert_eq!(count_fields(&r, "KSIZ"), 0);
    }

    #[test]
    fn existing_actor_type_creature_keyword_not_duplicated() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000107, "Out.esp", &mut interner);
        let mut kwda = Vec::new();
        kwda.extend_from_slice(&ACTOR_TYPE_CREATURE_FORM_ID.to_le_bytes());
        push_bytes(&mut r, "KWDA", kwda);
        push_bytes(&mut r, "KSIZ", 1u32.to_le_bytes().to_vec());

        let changed = append_actor_type_creature_keyword(&mut r);
        assert!(!changed);

        let kwda_after = first_bytes(&r, "KWDA").unwrap();
        assert_eq!(kwda_after.len(), 4, "must not duplicate FormID");
    }

    #[test]
    fn appends_creature_combat_perks_and_syncs_prkz() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000108, "Out.esp", &mut interner);
        let changed = apply_to_record(&mut r);
        assert!(changed);

        assert_eq!(count_fields(&r, "PRKR"), CREATURE_PERKS.len());
        for e in r.fields.iter() {
            if e.sig != SubrecordSig::from_str("PRKR").unwrap() {
                continue;
            }
            if let FieldValue::Bytes(data) = &e.value {
                assert_eq!(data.len(), PRKR_SIZE);
                assert_eq!(data[4], 1, "rank must be 1");
            }
        }

        let prkz = first_bytes(&r, "PRKZ").expect("PRKZ must be present");
        let n = u32::from_le_bytes([prkz[0], prkz[1], prkz[2], prkz[3]]);
        assert_eq!(n as usize, CREATURE_PERKS.len());
    }

    #[test]
    fn existing_creature_perk_not_duplicated() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x000109, "Out.esp", &mut interner);
        // Add crCreatureMeleeDamage with rank 2 already.
        let mut payload = Vec::new();
        payload.extend_from_slice(&CREATURE_PERKS[0].to_le_bytes());
        payload.push(2);
        push_bytes(&mut r, "PRKR", payload);

        let changed = append_creature_combat_perks(&mut r);
        assert!(changed); // The other two perks are added.
        assert_eq!(count_fields(&r, "PRKR"), CREATURE_PERKS.len());

        // First entry's rank must still be 2 (not overwritten).
        let prkr_sig = SubrecordSig::from_str("PRKR").unwrap();
        let first = r.fields.iter().find(|e| e.sig == prkr_sig).unwrap();
        if let FieldValue::Bytes(data) = &first.value {
            assert_eq!(data[4], 2, "existing perk rank must be preserved");
        }
    }

    #[test]
    fn backfill_perk_ranks_promotes_zero_to_one() {
        let mut interner = StringInterner::new();
        let mut r = npc(0x00010A, "Out.esp", &mut interner);
        let mut payload = Vec::new();
        payload.extend_from_slice(&0x00_001234u32.to_le_bytes());
        payload.push(0);
        push_bytes(&mut r, "PRKR", payload);

        let changed = backfill_perk_ranks(&mut r);
        assert!(changed);
        let prkr_sig = SubrecordSig::from_str("PRKR").unwrap();
        let entry = r.fields.iter().find(|e| e.sig == prkr_sig).unwrap();
        if let FieldValue::Bytes(data) = &entry.value {
            assert_eq!(data[4], 1);
        }
    }

    #[test]
    fn applies_to_npc_root() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            ..Default::default()
        };
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(FixCreatureNpcRecordsFixup.applies_to(&ctx));
    }

    #[test]
    fn applies_to_lvln_root() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("LVLN").unwrap()),
            ..Default::default()
        };
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(FixCreatureNpcRecordsFixup.applies_to(&ctx));
    }

    #[test]
    fn does_not_apply_to_weap_root() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(!FixCreatureNpcRecordsFixup.applies_to(&ctx));
    }

    #[test]
    fn does_not_apply_when_no_root_sig() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig::default();
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(!FixCreatureNpcRecordsFixup.applies_to(&ctx));
    }

    #[test]
    fn registry_runs_with_no_npc_records_as_no_op() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let target_handle = plugin_handle_new_native("FixCreatureNpcRecordsTest.esp", Some("fo4"))
            .expect("test plugin handle");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            target_schema: Some(schema.clone()),
            ..Default::default()
        };
        let mut mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let mut registry = FixupRegistry::new();
        registry.register(Box::new(FixCreatureNpcRecordsFixup));
        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all_in_session");
        assert_eq!(reports.len(), 1);
        assert!(reports[0].1.is_no_op());

        // Suppress unused-import warning for Arc.
        let _: Option<Arc<AuthoringSchema>> = None;
    }
}
