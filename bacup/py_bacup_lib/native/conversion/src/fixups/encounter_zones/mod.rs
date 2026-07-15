//! FO76→FO4 encounter-zone synthesis (post-copy).
//!
//! FO76 has no `ECZN`; the encounter-zone role lives on `LCTN` (band on `DATA`,
//! parent on `PNAM`, cell footprint on `LCEC`, keywords on `KWDA`). This module
//! synthesizes one FO4 `ECZN` per qualifying Location, stamps `CELL.XEZN` on each
//! footprint cell, and rewrites workshop `LCTN` keyword arrays to FO4's
//! settlement contract.
//!
//! Runs as a free function (NOT a registry `Fixup`) because exterior CELLs do
//! not exist at fixup time — the translator skips top-level CELL/REFR/ACHR and
//! the cell-slice copy inserts them post-fixups. `synthesize_encounter_zones`
//! is invoked by `ConversionRun::synthesize_encounter_zones` AFTER the
//! persistent-cell synthesis (cells present) and with the source handle still
//! open (LCTN readable).

mod build;
mod cell_index;
pub mod model;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

use build::{build_eczn_record, rebuild_keyword_fields};
use cell_index::{PLACED_SIGNATURES, build_target_index};
use model::{
    KW_WORKSHOP_PUBLIC, KW_WORKSHOP_SHELTER, WorkshopClass, clamp_level, classify,
    decode_lcec_footprint, decode_lctn_info, eczn_editor_id, eczn_flags, resolve_band,
};

const FALLOUT4_ESM: &str = "Fallout4.esm";
const SEVENTYSIX_ESM: &str = "SeventySix.esm";
const SYNTH_ECZN_PLUGIN: &str = "__synth_eczn__";
const SYNTH_ESS_SPAWN_PLUGIN: &str = "__synth_ess_spawn__";
const FO76_ESS_LOCATION_CONDITION_FUNCTION_ID: u16 = 579;

struct ZonePlan {
    source_lctn: FormKey,
    class: WorkshopClass,
    own_band: Option<(u8, u8)>,
    footprint: Vec<(FormKey, i32, i32)>,
    editor_id: String,
}

struct PreparedPlacedActor {
    actor: cell_index::PlacedActorRef,
    actor_record: Record,
    base_fk: FormKey,
    base_npc: Arc<Record>,
}

struct DeferredPlacedActor {
    prepared: PreparedPlacedActor,
    base_local: u32,
    template_local: u32,
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        FieldValue::List(items) => items.iter().find_map(first_form_key),
        _ => None,
    }
}

fn lvlo_entry_form_key(value: &FieldValue, source_plugin: crate::sym::Sym) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Uint(local) if *local != 0 && *local <= 0x00FF_FFFF => Some(FormKey {
            local: *local as u32,
            plugin: source_plugin,
        }),
        FieldValue::Int(local) if *local > 0 && *local <= 0x00FF_FFFF => Some(FormKey {
            local: *local as u32,
            plugin: source_plugin,
        }),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            let local = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF;
            (local != 0).then_some(FormKey {
                local,
                plugin: source_plugin,
            })
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
            let local = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) & 0x00FF_FFFF;
            (local != 0).then_some(FormKey {
                local,
                plugin: source_plugin,
            })
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| lvlo_entry_form_key(value, source_plugin)),
        FieldValue::List(items) => items
            .iter()
            .find_map(|value| lvlo_entry_form_key(value, source_plugin)),
        _ => None,
    }
}

fn field_value_as_f32(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Uint(value) => Some(*value as f32),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        _ => None,
    }
}

fn lctn_property_avifs(record: &Record) -> Vec<(FormKey, f32)> {
    let mut out = Vec::new();
    for field in &record.fields {
        if field.sig.as_str() != "PRPS" {
            continue;
        }
        match &field.value {
            FieldValue::Bytes(bytes) => {
                for row in bytes.chunks_exact(12) {
                    let raw = u32::from_le_bytes([row[0], row[1], row[2], row[3]]);
                    if raw == 0 {
                        continue;
                    }
                    let value = f32::from_le_bytes([row[4], row[5], row[6], row[7]]);
                    out.push((
                        FormKey {
                            local: raw & 0x00FF_FFFF,
                            plugin: record.form_key.plugin,
                        },
                        value,
                    ));
                }
            }
            FieldValue::List(items) => {
                for item in items {
                    let Some(avif) = first_form_key(item) else {
                        continue;
                    };
                    let value = match item {
                        FieldValue::Struct(fields) => fields
                            .iter()
                            .find_map(|(_, value)| field_value_as_f32(value))
                            .unwrap_or(0.0),
                        _ => 0.0,
                    };
                    out.push((avif, value));
                }
            }
            FieldValue::Struct(_) => {
                if let Some(avif) = first_form_key(&field.value) {
                    out.push((avif, 0.0));
                }
            }
            _ => {}
        }
    }
    out
}

fn avif_actor_value_keyword(record: &Record) -> Option<u32> {
    for field in &record.fields {
        if field.sig.as_str() != "NAM3" {
            continue;
        }
        return first_form_key(&field.value).map(|fk| fk.local & 0x00FF_FFFF);
    }
    None
}

fn source_avif_actor_value_keyword(
    session: &mut PluginSession,
    schema_source: &std::sync::Arc<crate::schema::AuthoringSchema>,
    interner: &crate::sym::StringInterner,
    cache: &mut HashMap<u32, Option<u32>>,
    avif: FormKey,
) -> Option<u32> {
    let local = avif.local & 0x00FF_FFFF;
    if let Some(value) = cache.get(&local) {
        return *value;
    }
    let value = session
        .source_record_decoded(&avif, schema_source.as_ref(), interner)
        .ok()
        .and_then(|record| avif_actor_value_keyword(&record));
    cache.insert(local, value);
    value
}

fn raw_condition_function_id(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 10 {
        return None;
    }
    Some(u16::from_le_bytes([bytes[8], bytes[9]]))
}

fn raw_condition_parameter_1(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 16 {
        return None;
    }
    Some(u32::from_le_bytes([
        bytes[12], bytes[13], bytes[14], bytes[15],
    ]))
}

fn struct_field_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    interner: &crate::sym::StringInterner,
    name: &str,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(sym, _)| interner.resolve(*sym) == Some(name))
        .map(|(_, value)| value)
}

fn numeric_or_form_key_local(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::FormKey(fk) => Some(fk.local & 0x00FF_FFFF),
        FieldValue::Uint(value) if *value <= 0x00FF_FFFF => Some(*value as u32),
        FieldValue::Int(value) if *value >= 0 && *value <= 0x00FF_FFFF => Some(*value as u32),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| numeric_or_form_key_local(value)),
        FieldValue::List(items) => items.iter().find_map(numeric_or_form_key_local),
        _ => None,
    }
}

fn condition_function_id(value: &FieldValue, interner: &crate::sym::StringInterner) -> Option<u16> {
    match value {
        FieldValue::Bytes(bytes) => raw_condition_function_id(bytes.as_slice()),
        FieldValue::Struct(fields) => struct_field_value(fields, interner, "function")
            .and_then(numeric_or_form_key_local)
            .and_then(|value| u16::try_from(value).ok()),
        _ => None,
    }
}

fn condition_parameter_1(value: &FieldValue, interner: &crate::sym::StringInterner) -> Option<u32> {
    match value {
        FieldValue::Bytes(bytes) => raw_condition_parameter_1(bytes.as_slice()),
        FieldValue::Struct(fields) => {
            struct_field_value(fields, interner, "parameter_1").and_then(numeric_or_form_key_local)
        }
        _ => None,
    }
}

fn lvln_condition_branch_keywords(
    record: &Record,
    interner: &crate::sym::StringInterner,
) -> HashMap<u32, FormKey> {
    let mut branches = HashMap::new();
    let mut current_entry = None;
    for field in &record.fields {
        match field.sig.as_str() {
            "LVLO" => current_entry = lvlo_entry_form_key(&field.value, record.form_key.plugin),
            "CTDA" => {
                if condition_function_id(&field.value, interner)
                    != Some(FO76_ESS_LOCATION_CONDITION_FUNCTION_ID)
                {
                    continue;
                }
                let Some(entry) = current_entry else {
                    continue;
                };
                let Some(keyword) = condition_parameter_1(&field.value, interner) else {
                    continue;
                };
                let keyword = keyword & 0x00FF_FFFF;
                if keyword != 0 {
                    branches.entry(keyword).or_insert(entry);
                }
            }
            _ => {}
        }
    }
    branches
}

fn replace_form_key(value: &mut FieldValue, old: FormKey, new: FormKey) -> bool {
    match value {
        FieldValue::FormKey(fk) if *fk == old => {
            *fk = new;
            true
        }
        FieldValue::Bytes(bytes) => replace_raw_form_key_slots(bytes.as_mut_slice(), old, new),
        FieldValue::Uint(raw)
            if can_replace_raw_form_key_slots(old, new)
                && *raw <= u32::MAX as u64
                && raw_form_key_slot_matches(*raw as u32, old) =>
        {
            *raw = replacement_raw_form_key_slot(*raw as u32, new) as u64;
            true
        }
        FieldValue::Int(raw)
            if can_replace_raw_form_key_slots(old, new)
                && *raw >= 0
                && *raw <= u32::MAX as i64
                && raw_form_key_slot_matches(*raw as u32, old) =>
        {
            *raw = replacement_raw_form_key_slot(*raw as u32, new) as i64;
            true
        }
        FieldValue::Struct(fields) => {
            let mut changed = false;
            for (_, value) in fields {
                changed |= replace_form_key(value, old, new);
            }
            changed
        }
        FieldValue::List(items) => {
            let mut changed = false;
            for value in items {
                changed |= replace_form_key(value, old, new);
            }
            changed
        }
        _ => false,
    }
}

fn replace_raw_form_key_slots(bytes: &mut [u8], old: FormKey, new: FormKey) -> bool {
    if !can_replace_raw_form_key_slots(old, new) {
        return false;
    }
    let mut changed = false;
    for chunk in bytes.chunks_exact_mut(4) {
        let raw = u32::from_le_bytes(chunk.try_into().unwrap());
        if raw_form_key_slot_matches(raw, old) {
            chunk.copy_from_slice(&replacement_raw_form_key_slot(raw, new).to_le_bytes());
            changed = true;
        }
    }
    changed
}

fn can_replace_raw_form_key_slots(old: FormKey, new: FormKey) -> bool {
    old.plugin == new.plugin && old.local != 0 && new.local != 0
}

fn raw_form_key_slot_matches(raw: u32, key: FormKey) -> bool {
    raw != 0 && raw & 0x00FF_FFFF == key.local & 0x00FF_FFFF
}

fn replacement_raw_form_key_slot(raw: u32, key: FormKey) -> u32 {
    (raw & 0xFF00_0000) | (key.local & 0x00FF_FFFF)
}

fn replace_template_form_key(record: &mut Record, old: FormKey, new: FormKey) -> bool {
    let mut changed = false;
    for field in &mut record.fields {
        if matches!(field.sig.as_str(), "TPLT" | "TPTA") {
            changed |= replace_form_key(&mut field.value, old, new);
        }
    }
    changed
}

fn template_lvln_local(
    record: &Record,
    output_plugin: crate::sym::Sym,
    branch_index: &HashMap<u32, HashMap<u32, FormKey>>,
) -> Option<u32> {
    for field in &record.fields {
        if !matches!(field.sig.as_str(), "TPLT" | "TPTA") {
            continue;
        }
        if let Some(local) = template_lvln_local_in_value(&field.value, output_plugin, branch_index)
        {
            return Some(local);
        }
    }
    None
}

fn template_lvln_local_in_value(
    value: &FieldValue,
    output_plugin: crate::sym::Sym,
    branch_index: &HashMap<u32, HashMap<u32, FormKey>>,
) -> Option<u32> {
    match value {
        FieldValue::FormKey(fk) => {
            let local = fk.local & 0x00FF_FFFF;
            (fk.plugin == output_plugin && branch_index.contains_key(&local)).then_some(local)
        }
        FieldValue::Bytes(bytes) => bytes.chunks_exact(4).find_map(|chunk| {
            let local = u32::from_le_bytes(chunk.try_into().unwrap()) & 0x00FF_FFFF;
            (local != 0 && branch_index.contains_key(&local)).then_some(local)
        }),
        FieldValue::Uint(value) if *value <= 0x00FF_FFFF => {
            let local = *value as u32;
            (local != 0 && branch_index.contains_key(&local)).then_some(local)
        }
        FieldValue::Int(value) if *value > 0 && *value <= 0x00FF_FFFF => {
            let local = *value as u32;
            branch_index.contains_key(&local).then_some(local)
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| {
            template_lvln_local_in_value(value, output_plugin, branch_index)
        }),
        FieldValue::List(items) => items
            .iter()
            .find_map(|value| template_lvln_local_in_value(value, output_plugin, branch_index)),
        _ => None,
    }
}

fn collect_template_candidate_locals(
    value: &FieldValue,
    output_plugin: crate::sym::Sym,
    out: &mut HashSet<u32>,
) {
    match value {
        FieldValue::FormKey(fk) if fk.plugin == output_plugin => {
            let local = fk.local & 0x00FF_FFFF;
            if local != 0 {
                out.insert(local);
            }
        }
        FieldValue::Bytes(bytes) => {
            for chunk in bytes.chunks_exact(4) {
                let local = u32::from_le_bytes(chunk.try_into().unwrap()) & 0x00FF_FFFF;
                if local != 0 {
                    out.insert(local);
                }
            }
        }
        FieldValue::Uint(value) if *value <= 0x00FF_FFFF && *value != 0 => {
            out.insert(*value as u32);
        }
        FieldValue::Int(value) if *value > 0 && *value <= 0x00FF_FFFF => {
            out.insert(*value as u32);
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_template_candidate_locals(value, output_plugin, out);
            }
        }
        FieldValue::List(items) => {
            for value in items {
                collect_template_candidate_locals(value, output_plugin, out);
            }
        }
        _ => {}
    }
}

fn collect_npc_template_candidate_locals(
    record: &Record,
    output_plugin: crate::sym::Sym,
    out: &mut HashSet<u32>,
) {
    for field in &record.fields {
        if matches!(field.sig.as_str(), "TPLT" | "TPTA") {
            collect_template_candidate_locals(&field.value, output_plugin, out);
        }
    }
}

fn set_editor_id(record: &mut Record, editor_id: &str, interner: &crate::sym::StringInterner) {
    let sym = interner.intern(editor_id);
    record.eid = Some(sym);
    for field in &mut record.fields {
        if field.sig.as_str() == "EDID" {
            field.value = FieldValue::String(sym);
            return;
        }
    }
    if let Ok(sig) = SubrecordSig::from_str("EDID") {
        record.fields.push(FieldEntry {
            sig,
            value: FieldValue::String(sym),
        });
    }
}

fn branch_target_form_key(
    source: FormKey,
    target: &cell_index::TargetIndex,
    mapper: &FormKeyMapper,
    output_plugin: crate::sym::Sym,
    config: &FixupConfig,
) -> Option<FormKey> {
    if let Some(fk) = mapper.lookup(source) {
        return Some(fk);
    }
    if config.preserve_source_ids
        && target
            .all_object_ids
            .contains(&(source.local & 0x00FF_FFFF))
    {
        return Some(FormKey {
            local: source.local & 0x00FF_FFFF,
            plugin: output_plugin,
        });
    }
    None
}

fn warning_once(
    warnings: &mut Vec<crate::sym::Sym>,
    seen: &mut HashSet<String>,
    interner: &crate::sym::StringInterner,
    message: String,
) {
    if warnings.len() >= 200 {
        return;
    }
    if seen.insert(message.clone()) {
        warnings.push(interner.intern(&message));
    }
}

fn field_form_key(record: &Record, sig: &str) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == sig)
        .and_then(|field| first_form_key(&field.value))
}

fn replace_field_form_key(record: &mut Record, sig: &str, old: FormKey, new: FormKey) -> bool {
    let mut changed = false;
    for field in &mut record.fields {
        if field.sig.as_str() == sig {
            changed |= replace_form_key(&mut field.value, old, new);
        }
    }
    changed
}

/// Walk the FO76 template / leveled-list chain from `start` to the first `NPC_`
/// that carries its own `FULL`, returning that `FULL` (and optional `SHRT`) so a
/// synthesized ESS clone can bake a concrete display name.
///
/// The FO76 `Lvl*` actor families resolve their name by templating (Use-Traits)
/// through a leveled list whose leaves are themselves name-less templated NPCs.
/// FO4 does not chase a display name across a leveled-list hop, so a placed ESS
/// clone that inherits its name this way renders empty above the health bar.
/// Baking the resolved `FULL` onto the clone gives it a stable name; it is
/// harmless if FO4 does resolve the template (it lands on the same string).
fn scalar_form_key(value: &FieldValue, plugin: crate::sym::Sym) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Uint(local) if *local != 0 && *local <= 0x00FF_FFFF => Some(FormKey {
            local: *local as u32,
            plugin,
        }),
        FieldValue::Int(local) if *local > 0 && *local <= 0x00FF_FFFF => Some(FormKey {
            local: *local as u32,
            plugin,
        }),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let local = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF;
            (local != 0).then_some(FormKey { local, plugin })
        }
        _ => None,
    }
}

fn resolve_branch_display_name(
    session: &mut PluginSession,
    schema: &crate::schema::AuthoringSchema,
    interner: &crate::sym::StringInterner,
    current: FormKey,
    visited: &mut HashSet<FormKey>,
    depth: u32,
) -> Option<(FieldEntry, Option<FieldEntry>)> {
    if depth > 8 || !visited.insert(current) {
        return None;
    }
    let record = session.record_decoded(&current, schema, interner).ok()?;
    let plugin = record.form_key.plugin;
    match record.sig.as_str() {
        "LVLN" => {
            // Try each entry until one resolves a name (leaves may be external).
            for field in &record.fields {
                if field.sig.as_str() != "LVLO" {
                    continue;
                }
                let leaf = match &field.value {
                    // FO4 LVLO decodes as a struct; the entry form is the `npc` field
                    // (not the leading `level`, which is also a non-zero uint).
                    FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
                        (interner.resolve(*name) == Some("npc"))
                            .then(|| scalar_form_key(value, plugin))
                            .flatten()
                    }),
                    FieldValue::FormKey(fk) => Some(*fk),
                    // Raw-fallback LVLO (struct:H,B,B,I,...): npc formid is at offset 4.
                    FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
                        let local = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
                            & 0x00FF_FFFF;
                        (local != 0).then_some(FormKey { local, plugin })
                    }
                    _ => None,
                };
                if let Some(leaf) = leaf {
                    if let Some(found) = resolve_branch_display_name(
                        session,
                        schema,
                        interner,
                        leaf,
                        visited,
                        depth + 1,
                    ) {
                        return Some(found);
                    }
                }
            }
            None
        }
        "NPC_" => {
            if let Some(full) = record.fields.iter().find(|f| f.sig.as_str() == "FULL") {
                let shrt = record
                    .fields
                    .iter()
                    .find(|f| f.sig.as_str() == "SHRT")
                    .cloned();
                return Some((full.clone(), shrt));
            }
            // No own name: follow the Traits template (TPTA slot 0 = traits), then TPLT.
            let next = record
                .fields
                .iter()
                .find(|f| f.sig.as_str() == "TPTA")
                .and_then(|f| match &f.value {
                    FieldValue::Struct(fields) => fields
                        .first()
                        .and_then(|(_, value)| scalar_form_key(value, plugin)),
                    other => scalar_form_key(other, plugin),
                })
                .or_else(|| {
                    record
                        .fields
                        .iter()
                        .find(|f| f.sig.as_str() == "TPLT")
                        .and_then(|f| scalar_form_key(&f.value, plugin))
                })?;
            resolve_branch_display_name(session, schema, interner, next, visited, depth + 1)
        }
        _ => None,
    }
}

/// Insert a resolved `FULL` (+ optional `SHRT`) into an NPC record if it has no
/// own name, matching vanilla NPC_ order (`CNAM, FULL, SHRT, DATA`).
fn bake_display_name(record: &mut Record, full: FieldEntry, shrt: Option<FieldEntry>) {
    if record.fields.iter().any(|f| f.sig.as_str() == "FULL") {
        return;
    }
    let mut pos = record
        .fields
        .iter()
        .position(|f| f.sig.as_str() == "DATA")
        .unwrap_or(record.fields.len());
    record.fields.insert(pos, full);
    pos += 1;
    if let Some(shrt) = shrt {
        record.fields.insert(pos, shrt);
    }
}

fn selected_branch_for_location(
    branches: &HashMap<u32, FormKey>,
    encounter_keywords: &[(u32, f32)],
) -> Option<(u32, FormKey)> {
    encounter_keywords.iter().find_map(|(keyword, _)| {
        branches
            .get(keyword)
            .copied()
            .map(|branch| (*keyword, branch))
    })
}

enum SpecializeOutcome {
    Specialized,
    ReplaceFailed,
    NoChange,
}

#[allow(clippy::too_many_arguments)]
fn specialize_actor_to_branch(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    npc_sig: SigCode,
    synth_plugin: crate::sym::Sym,
    clone_records: &mut Vec<Record>,
    clone_replacements: &mut Vec<Record>,
    actor_changes: &mut Vec<Record>,
    warnings: &mut Vec<crate::sym::Sym>,
    seen_warnings: &mut HashSet<String>,
    output_plugin: crate::sym::Sym,
    mut actor_record: Record,
    base_fk: FormKey,
    base_local: u32,
    base_npc: &Record,
    template_local: u32,
    branch_target: FormKey,
) -> SpecializeOutcome {
    // The clone registry survives repeated specialization work (see
    // `MapperState::ess_clone_registry`): the same (base, branch) resolves to one
    // clone and different (base, branch) never alias.
    let cache_key = (base_local, branch_target.local & 0x00FF_FFFF);
    let clone_fk = if let Some(clone_fk) = mapper.ess_clone_lookup(cache_key) {
        clone_fk
    } else {
        let mut clone = base_npc.clone();
        let source_fk = FormKey {
            local: mapper.ess_clone_next_source_local(),
            plugin: synth_plugin,
        };
        let clone_fk = mapper.allocate_or_resolve(source_fk, None, npc_sig);
        clone.form_key = clone_fk;
        let base_eid = base_npc
            .eid
            .and_then(|sym| mapper.interner.resolve(sym))
            .unwrap_or("SynthNpc");
        let clone_eid = format!("{}_ESS_{:06X}", base_eid, branch_target.local & 0x00FF_FFFF);
        set_editor_id(&mut clone, &clone_eid, mapper.interner);
        let old_template = FormKey {
            local: template_local,
            plugin: output_plugin,
        };
        if !replace_template_form_key(&mut clone, old_template, branch_target) {
            warning_once(
                warnings,
                seen_warnings,
                mapper.interner,
                format!(
                    "ess_spawn_fallback:template_replace_failed base={:06X} template={:06X} branch={:06X}",
                    base_local,
                    template_local,
                    branch_target.local & 0x00FF_FFFF
                ),
            );
            return SpecializeOutcome::ReplaceFailed;
        }
        // The clone inherits its name by templating through `branch_target` (a
        // leveled list of name-less templated NPCs) — a chain FO4 will not chase
        // for the display name. Resolve it now and bake a concrete FULL so the
        // placed actor is not nameless above the health bar.
        let mut name_visited: HashSet<FormKey> = HashSet::new();
        if let Some((full, shrt)) = resolve_branch_display_name(
            session,
            target_schema,
            mapper.interner,
            branch_target,
            &mut name_visited,
            0,
        ) {
            bake_display_name(&mut clone, full, shrt);
        }
        mapper.ess_clone_register(cache_key, clone_fk);
        if session
            .record_decoded(&clone_fk, target_schema, mapper.interner)
            .is_ok()
        {
            clone_replacements.push(clone);
        } else {
            clone_records.push(clone);
        }
        clone_fk
    };

    if replace_field_form_key(&mut actor_record, "NAME", base_fk, clone_fk) {
        actor_changes.push(actor_record);
        SpecializeOutcome::Specialized
    } else {
        SpecializeOutcome::NoChange
    }
}

fn specialize_placed_actor_templates(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    target_schema: &crate::schema::AuthoringSchema,
    target: &cell_index::TargetIndex,
    prepared_actors: Vec<PreparedPlacedActor>,
    conditional_lvln_branches: &HashMap<u32, HashMap<u32, FormKey>>,
    location_encounter_keywords: &HashMap<u32, Vec<(u32, f32)>>,
    cell_source_lctn_assignment: &HashMap<u32, (u32, u32)>,
    output_plugin: crate::sym::Sym,
) -> Result<(u32, u32, Vec<crate::sym::Sym>), FixupError> {
    if conditional_lvln_branches.is_empty() || prepared_actors.is_empty() {
        return Ok((0, 0, Vec::new()));
    }

    let npc_sig = SigCode::from_str("NPC_").map_err(FixupError::SchemaError)?;
    let synth_plugin = mapper.interner.intern(SYNTH_ESS_SPAWN_PLUGIN);
    let mut clone_records: Vec<Record> = Vec::new();
    let mut clone_replacements: Vec<Record> = Vec::new();
    let mut actor_changes: Vec<Record> = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_warnings = HashSet::new();
    let mut n_template_actors = 0u32;
    let mut n_no_cell_location = 0u32;
    let mut n_no_keywords = 0u32;
    let mut n_no_branch = 0u32;
    let mut n_unmapped_branch = 0u32;
    let mut n_external_branch = 0u32;
    let mut n_replace_failed = 0u32;
    let mut n_specialized = 0u32;
    let mut n_specialized_default = 0u32;
    let mut n_default_unresolved = 0u32;
    // Actors whose location evidence could not select a branch: retried below
    // with the template's deterministic default branch. FO4 cannot express the
    // conditional broad list, so leaving them on it is never an option.
    let mut deferred: Vec<DeferredPlacedActor> = Vec::new();
    // template_local -> keyword -> resolved-selection count (drives defaults).
    let mut selection_counts: HashMap<u32, HashMap<u32, u32>> = HashMap::new();

    for prepared in prepared_actors {
        let actor = prepared.actor;
        let actor_record = prepared.actor_record;
        let base_fk = prepared.base_fk;
        let base_npc = Arc::clone(&prepared.base_npc);
        let base_local = base_fk.local & 0x00FF_FFFF;
        let Some(template_local) =
            template_lvln_local(&base_npc, output_plugin, conditional_lvln_branches)
        else {
            continue;
        };
        n_template_actors += 1;

        let Some(branches) = conditional_lvln_branches.get(&template_local) else {
            continue;
        };
        let selection = match cell_source_lctn_assignment.get(&actor.cell_objid) {
            None => {
                n_no_cell_location += 1;
                warning_once(
                    &mut warnings,
                    &mut seen_warnings,
                    mapper.interner,
                    format!(
                        "ess_spawn_fallback:no_cell_location actor={:06X} cell={:06X} base={:06X} template={:06X}",
                        actor.ref_objid, actor.cell_objid, base_local, template_local
                    ),
                );
                None
            }
            Some((_, source_lctn)) => match location_encounter_keywords.get(source_lctn) {
                None => {
                    n_no_keywords += 1;
                    warning_once(
                        &mut warnings,
                        &mut seen_warnings,
                        mapper.interner,
                        format!(
                            "ess_spawn_fallback:no_source_location_keywords actor={:06X} cell={:06X} source_lctn={:06X} template={:06X}",
                            actor.ref_objid, actor.cell_objid, source_lctn, template_local
                        ),
                    );
                    None
                }
                Some(encounter_keywords) => {
                    let selected = selected_branch_for_location(branches, encounter_keywords);
                    if selected.is_none() {
                        n_no_branch += 1;
                        warning_once(
                            &mut warnings,
                            &mut seen_warnings,
                            mapper.interner,
                            format!(
                                "ess_spawn_fallback:no_matching_branch actor={:06X} cell={:06X} source_lctn={:06X} template={:06X}",
                                actor.ref_objid, actor.cell_objid, source_lctn, template_local
                            ),
                        );
                    }
                    selected
                }
            },
        };
        let Some((keyword, source_branch)) = selection else {
            deferred.push(DeferredPlacedActor {
                prepared: PreparedPlacedActor {
                    actor,
                    actor_record,
                    base_fk,
                    base_npc,
                },
                base_local,
                template_local,
            });
            continue;
        };
        *selection_counts
            .entry(template_local)
            .or_default()
            .entry(keyword)
            .or_default() += 1;
        let Some(branch_target) =
            branch_target_form_key(source_branch, target, mapper, output_plugin, config)
        else {
            n_unmapped_branch += 1;
            warning_once(
                &mut warnings,
                &mut seen_warnings,
                mapper.interner,
                format!(
                    "ess_spawn_fallback:unmapped_branch actor={:06X} branch={:06X} keyword={:06X}",
                    actor.ref_objid,
                    source_branch.local & 0x00FF_FFFF,
                    keyword
                ),
            );
            continue;
        };
        if branch_target.plugin != output_plugin {
            n_external_branch += 1;
            warning_once(
                &mut warnings,
                &mut seen_warnings,
                mapper.interner,
                format!(
                    "ess_spawn_fallback:external_branch actor={:06X} branch={:06X} target={:06X}",
                    actor.ref_objid,
                    source_branch.local & 0x00FF_FFFF,
                    branch_target.local & 0x00FF_FFFF
                ),
            );
            continue;
        }

        match specialize_actor_to_branch(
            session,
            mapper,
            target_schema,
            npc_sig,
            synth_plugin,
            &mut clone_records,
            &mut clone_replacements,
            &mut actor_changes,
            &mut warnings,
            &mut seen_warnings,
            output_plugin,
            actor_record,
            base_fk,
            base_local,
            &base_npc,
            template_local,
            branch_target,
        ) {
            SpecializeOutcome::Specialized => n_specialized += 1,
            SpecializeOutcome::ReplaceFailed => n_replace_failed += 1,
            SpecializeOutcome::NoChange => {}
        }
    }

    // Deterministic default branch per template: the most-selected keyword
    // among location-resolved actors (tie → lowest keyword id); a template
    // with no resolved selection falls back to its lowest keyword id.
    let mut default_branches: HashMap<u32, (u32, FormKey)> = HashMap::new();
    for (template_local, branches) in conditional_lvln_branches {
        let keyword = selection_counts
            .get(template_local)
            .and_then(|counts| {
                counts
                    .iter()
                    .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                    .map(|(keyword, _)| *keyword)
            })
            .filter(|keyword| branches.contains_key(keyword))
            .or_else(|| branches.keys().min().copied());
        if let Some(keyword) = keyword {
            if let Some(branch) = branches.get(&keyword).copied() {
                default_branches.insert(*template_local, (keyword, branch));
            }
        }
    }

    for deferred_actor in deferred {
        let actor = deferred_actor.prepared.actor;
        let actor_record = deferred_actor.prepared.actor_record;
        let base_fk = deferred_actor.prepared.base_fk;
        let base_npc = deferred_actor.prepared.base_npc;
        let base_local = deferred_actor.base_local;
        let template_local = deferred_actor.template_local;
        let Some((keyword, source_branch)) = default_branches.get(&template_local).copied() else {
            n_default_unresolved += 1;
            continue;
        };
        let Some(branch_target) =
            branch_target_form_key(source_branch, target, mapper, output_plugin, config)
                .filter(|fk| fk.plugin == output_plugin)
        else {
            n_unmapped_branch += 1;
            warning_once(
                &mut warnings,
                &mut seen_warnings,
                mapper.interner,
                format!(
                    "ess_spawn_fallback:default_branch_unmapped actor={:06X} branch={:06X} keyword={:06X}",
                    actor.ref_objid,
                    source_branch.local & 0x00FF_FFFF,
                    keyword
                ),
            );
            continue;
        };
        match specialize_actor_to_branch(
            session,
            mapper,
            target_schema,
            npc_sig,
            synth_plugin,
            &mut clone_records,
            &mut clone_replacements,
            &mut actor_changes,
            &mut warnings,
            &mut seen_warnings,
            output_plugin,
            actor_record,
            base_fk,
            base_local,
            &base_npc,
            template_local,
            branch_target,
        ) {
            SpecializeOutcome::Specialized => {
                n_specialized_default += 1;
                warning_once(
                    &mut warnings,
                    &mut seen_warnings,
                    mapper.interner,
                    format!(
                        "ess_spawn_fallback:default_branch actor={:06X} cell={:06X} template={:06X} keyword={:06X} branch={:06X}",
                        actor.ref_objid,
                        actor.cell_objid,
                        template_local,
                        keyword,
                        branch_target.local & 0x00FF_FFFF
                    ),
                );
            }
            SpecializeOutcome::ReplaceFailed => n_replace_failed += 1,
            SpecializeOutcome::NoChange => {}
        }
    }

    warnings.push(mapper.interner.intern(&format!(
        "ess_spawn: summary template_actors={} specialized={} specialized_default={} clones={} skip_no_cell_location={} skip_no_keywords={} skip_no_branch={} skip_unmapped_branch={} skip_external_branch={} skip_replace_failed={} default_unresolved={}",
        n_template_actors,
        n_specialized,
        n_specialized_default,
        clone_records.len() + clone_replacements.len(),
        n_no_cell_location,
        n_no_keywords,
        n_no_branch,
        n_unmapped_branch,
        n_external_branch,
        n_replace_failed,
        n_default_unresolved
    )));

    let added = session
        .add_records(clone_records, target_schema, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    let replaced_clones = session
        .replace_records_contents(clone_replacements, target_schema, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    let changed = replaced_clones
        + session
            .replace_records_contents(actor_changes, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    Ok((added, changed, warnings))
}

fn output_to_source_object_ids(
    mapper: &FormKeyMapper,
    output_plugin: crate::sym::Sym,
) -> HashMap<u32, u32> {
    mapper
        .source_to_target_iter()
        .filter_map(|(source, target)| {
            (target.plugin == output_plugin)
                .then_some((target.local & 0x00FF_FFFF, source.local & 0x00FF_FFFF))
        })
        .collect()
}

fn source_object_id_for_target(
    target_local: u32,
    output_to_source: &HashMap<u32, u32>,
    config: &FixupConfig,
) -> Option<u32> {
    output_to_source
        .get(&(target_local & 0x00FF_FFFF))
        .copied()
        .or_else(|| {
            config
                .preserve_source_ids
                .then_some(target_local & 0x00FF_FFFF)
        })
}

fn source_plugin_sym(
    session: &mut PluginSession,
    interner: &crate::sym::StringInterner,
) -> Option<crate::sym::Sym> {
    session
        .source_slot_opt()
        .map(|slot| interner.intern(&slot.parsed.plugin_name))
}

fn prepare_placed_actor_templates(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    target: &cell_index::TargetIndex,
    output_plugin: crate::sym::Sym,
    interner: &crate::sym::StringInterner,
) -> Result<(HashSet<u32>, Vec<PreparedPlacedActor>), FixupError> {
    use rayon::prelude::*;

    let (actor_records, base_npcs) = {
        let view = session
            .target_read_view()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let actor_records: Vec<(cell_index::PlacedActorRef, Record, FormKey)> = target
            .placed_actors
            .par_iter()
            .filter_map(|actor| {
                let actor_fk = FormKey {
                    local: actor.ref_objid,
                    plugin: output_plugin,
                };
                let actor_record = view
                    .record_decoded(&actor_fk, target_schema, interner)
                    .ok()?;
                let base_fk = field_form_key(&actor_record, "NAME")?;
                (base_fk.plugin == output_plugin).then_some((*actor, actor_record, base_fk))
            })
            .collect();
        let unique_base_fks: HashSet<FormKey> = actor_records
            .iter()
            .map(|(_, _, base_fk)| *base_fk)
            .collect();
        let base_npcs: HashMap<FormKey, Arc<Record>> = unique_base_fks
            .par_iter()
            .filter_map(|base_fk| {
                let record = view.record_decoded(base_fk, target_schema, interner).ok()?;
                (record.sig.as_str() == "NPC_").then(|| (*base_fk, Arc::new(record)))
            })
            .collect();
        (actor_records, base_npcs)
    };

    let mut candidates = HashSet::new();
    for base_npc in base_npcs.values() {
        collect_npc_template_candidate_locals(base_npc, output_plugin, &mut candidates);
    }
    let prepared = actor_records
        .into_iter()
        .filter_map(|(actor, actor_record, base_fk)| {
            Some(PreparedPlacedActor {
                actor,
                actor_record,
                base_fk,
                base_npc: Arc::clone(base_npcs.get(&base_fk)?),
            })
        })
        .collect();
    Ok((candidates, prepared))
}

fn build_conditional_lvln_branch_index_for_target_templates(
    session: &mut PluginSession,
    schema_source: &std::sync::Arc<crate::schema::AuthoringSchema>,
    interner: &crate::sym::StringInterner,
    target: &cell_index::TargetIndex,
    mapper: &FormKeyMapper,
    output_plugin: crate::sym::Sym,
    source_plugin: crate::sym::Sym,
    config: &FixupConfig,
    target_template_locals: &HashSet<u32>,
    output_to_source: &HashMap<u32, u32>,
) -> HashMap<u32, HashMap<u32, FormKey>> {
    let mut out = HashMap::new();
    for target_local in target_template_locals {
        let Some(source_local) =
            source_object_id_for_target(*target_local, output_to_source, config)
        else {
            continue;
        };
        let source_fk = FormKey {
            local: source_local,
            plugin: source_plugin,
        };
        let Ok(record) =
            session.source_record_decoded(&source_fk, schema_source.as_ref(), interner)
        else {
            continue;
        };
        let branches = lvln_condition_branch_keywords(&record, interner);
        if branches.is_empty() {
            continue;
        }
        if let Some(target_fk) =
            branch_target_form_key(source_fk, target, mapper, output_plugin, config)
                .filter(|fk| fk.plugin == output_plugin)
        {
            out.insert(target_fk.local & 0x00FF_FFFF, branches);
        }
    }
    out
}

const LCTN_PARENT_WALK_CAP: usize = 16;

fn location_encounter_keywords_for_sources(
    session: &mut PluginSession,
    schema_source: &std::sync::Arc<crate::schema::AuthoringSchema>,
    interner: &crate::sym::StringInterner,
    source_plugin: crate::sym::Sym,
    source_lctn_locals: &HashSet<u32>,
) -> HashMap<u32, Vec<(u32, f32)>> {
    let mut avif_keyword_cache: HashMap<u32, Option<u32>> = HashMap::new();
    // Per-location decode cache: own PRPS-derived keywords + PNAM parent.
    let mut lctn_cache: HashMap<u32, (Vec<(u32, f32)>, Option<u32>)> = HashMap::new();
    let mut out = HashMap::new();
    for source_local in source_lctn_locals {
        // FO76 stores encounter-species AVIFs on the nearest location that
        // defines them; leaf locations named by a cell's XLCN often carry
        // none. Walk the PNAM parent chain nearest-first so ancestor
        // properties select a branch when the leaf has no matching family.
        let mut keywords: Vec<(u32, f32)> = Vec::new();
        let mut seen_keywords: HashSet<u32> = HashSet::new();
        let mut seen_locals: HashSet<u32> = HashSet::new();
        let mut current = Some(*source_local & 0x00FF_FFFF);
        while let Some(local) = current {
            if !seen_locals.insert(local) || seen_locals.len() > LCTN_PARENT_WALK_CAP {
                break;
            }
            let (own, parent) = match lctn_cache.get(&local) {
                Some(entry) => entry.clone(),
                None => {
                    let fk = FormKey {
                        local,
                        plugin: source_plugin,
                    };
                    let entry = match session.source_record_decoded(
                        &fk,
                        schema_source.as_ref(),
                        interner,
                    ) {
                        Ok(record) => {
                            let mut own = Vec::new();
                            for (avif, value) in lctn_property_avifs(&record) {
                                if !value.is_finite() || value <= 0.0 {
                                    continue;
                                }
                                if let Some(keyword) = source_avif_actor_value_keyword(
                                    session,
                                    schema_source,
                                    interner,
                                    &mut avif_keyword_cache,
                                    avif,
                                ) {
                                    own.push((keyword, value));
                                }
                            }
                            let parent = decode_lctn_info(&record, interner)
                                .parent
                                .map(|fk| fk.local & 0x00FF_FFFF);
                            (own, parent)
                        }
                        Err(_) => (Vec::new(), None),
                    };
                    lctn_cache.insert(local, entry.clone());
                    entry
                }
            };
            for (keyword, value) in own {
                if seen_keywords.insert(keyword) {
                    keywords.push((keyword, value));
                }
            }
            current = parent;
        }
        out.insert(*source_local & 0x00FF_FFFF, keywords);
    }
    out
}

fn source_location_assignments_for_target_cells(
    session: &mut PluginSession,
    schema_source: &std::sync::Arc<crate::schema::AuthoringSchema>,
    interner: &crate::sym::StringInterner,
    target: &cell_index::TargetIndex,
    mapper: &FormKeyMapper,
    config: &FixupConfig,
    output_to_source: &HashMap<u32, u32>,
    actor_cells: &HashSet<u32>,
) -> Result<(HashSet<u32>, HashMap<u32, (u32, u32)>), FixupError> {
    use rayon::prelude::*;

    let mut source_lctn_locals = HashSet::new();
    let mut cell_source_lctn_assignment: HashMap<u32, (u32, u32)> = HashMap::new();

    let lctn_sig = SigCode::from_str("LCTN").map_err(FixupError::SchemaError)?;
    let source_lctn_fks = session
        .source_form_keys_of_sig(lctn_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let (source_masters, source_plugin_name) = match session.source_slot_opt() {
        Some(slot) => (
            slot.parsed.header.masters.clone(),
            slot.parsed.plugin_name.clone(),
        ),
        None => (Vec::new(), String::new()),
    };

    let decoded_lctns = {
        let source_id = session
            .source_id()
            .ok_or_else(|| FixupError::HandleError("missing source handle".into()))?;
        let source_scan = session
            .handle_raw_scan(source_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let raw_form_ids = source_scan.raw_form_ids_of_sig(lctn_sig);
        debug_assert_eq!(source_lctn_fks.len(), raw_form_ids.len());
        source_lctn_fks
            .par_iter()
            .zip(raw_form_ids.par_iter())
            .filter_map(|(fk, raw_form_id)| {
                let record = source_scan
                    .record_decoded(*raw_form_id, fk, schema_source.as_ref(), interner)?
                    .ok()?;
                let info = decode_lctn_info(&record, interner);
                let source_lctn = info.form_key.local & 0x00FF_FFFF;
                let parent = info.parent.map(|parent| parent.local & 0x00FF_FFFF);
                let footprint =
                    decode_lcec_footprint(&record, interner, &source_masters, &source_plugin_name);
                Some((source_lctn, parent, footprint))
            })
            .collect::<Vec<_>>()
    };
    let mut parents: HashMap<u32, Option<u32>> = HashMap::new();
    let mut footprints: Vec<(u32, Vec<(FormKey, i32, i32)>)> = Vec::new();
    for (source_lctn, parent, footprint) in decoded_lctns {
        parents.insert(source_lctn, parent);
        if !footprint.is_empty() {
            footprints.push((source_lctn, footprint));
        }
    }

    let depth_of = |start: u32| -> u32 {
        let (mut depth, mut current, mut seen) = (
            0u32,
            parents.get(&start).and_then(|parent| *parent),
            vec![start],
        );
        while let Some(parent) = current {
            if seen.contains(&parent) {
                break;
            }
            seen.push(parent);
            depth += 1;
            current = parents.get(&parent).and_then(|next| *next);
        }
        depth
    };

    for (source_lctn, footprint) in footprints {
        let depth = depth_of(source_lctn);
        for (world_fk, grid_x, grid_y) in footprint {
            let world_objid = mapper
                .lookup(world_fk)
                .map(|fk| fk.local)
                .unwrap_or(world_fk.local)
                & 0x00FF_FFFF;
            let Some(cell_objid) = target.grid.get(&(world_objid, grid_x, grid_y)).copied() else {
                continue;
            };
            if !actor_cells.contains(&cell_objid) {
                continue;
            }
            let entry = cell_source_lctn_assignment
                .entry(cell_objid)
                .or_insert((u32::MIN, source_lctn));
            if depth >= entry.0 {
                *entry = (depth, source_lctn);
            }
        }
    }

    for (cell_objid, lctn_objid) in &target.cell_locations {
        if !actor_cells.contains(cell_objid) {
            continue;
        }
        let Some(source_lctn) = source_object_id_for_target(*lctn_objid, output_to_source, config)
        else {
            continue;
        };
        cell_source_lctn_assignment.insert(*cell_objid, (u32::MAX, source_lctn));
    }

    // Fallback: a target cell that lost its XLCN and has no LCEC footprint hit
    // can still resolve through the SOURCE cell's own XLCN (ids preserved).
    if !source_plugin_name.is_empty() {
        let source_plugin = interner.intern(&source_plugin_name);
        for cell_objid in actor_cells {
            if cell_source_lctn_assignment.contains_key(cell_objid) {
                continue;
            }
            let fk = FormKey {
                local: *cell_objid,
                plugin: source_plugin,
            };
            let Ok(record) = session.source_record_decoded(&fk, schema_source.as_ref(), interner)
            else {
                continue;
            };
            if record.sig.as_str() != "CELL" {
                continue;
            }
            let Some(xlcn) = cell_xlcn_local(&record) else {
                continue;
            };
            cell_source_lctn_assignment.insert(*cell_objid, (u32::MAX - 1, xlcn));
        }
    }

    source_lctn_locals.extend(
        cell_source_lctn_assignment
            .values()
            .map(|(_, source_lctn)| *source_lctn),
    );
    Ok((source_lctn_locals, cell_source_lctn_assignment))
}

fn cell_xlcn_local(record: &Record) -> Option<u32> {
    let field = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "XLCN")?;
    match &field.value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF;
            (raw != 0).then_some(raw)
        }
        value => first_form_key(value)
            .map(|fk| fk.local & 0x00FF_FFFF)
            .filter(|local| *local != 0),
    }
}

/// Late ESS placed-actor specialization. This runs after placed-child reference
/// repair has normalized ACHR `NAME`, so the repair pass cannot overwrite the
/// clone redirect back to the generic source base.
pub fn specialize_placed_actor_templates_after_ref_repair(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let log_timing = |name: &str, started: std::time::Instant| {
        eprintln!(
            "[ess_timing] {name} elapsed_ms={}",
            started.elapsed().as_millis()
        );
    };
    let mut report = FixupReport::empty();
    let target_schema = config
        .target_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing target schema".into()))?;
    let source_schema = config
        .source_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing source schema".into()))?;
    let output_plugin = mapper.output_plugin_sym();
    let Some(source_plugin) = source_plugin_sym(session, mapper.interner) else {
        report
            .warnings
            .push(mapper.interner.intern("ess_spawn: skip=no_source_slot"));
        return Ok(report);
    };

    let started = std::time::Instant::now();
    let target = build_target_index(&session.target_slot().parsed.root_items, true);
    log_timing("build_target_index", started);
    if target.placed_actors.is_empty() {
        report
            .warnings
            .push(mapper.interner.intern("ess_spawn: skip=no_placed_actors"));
        return Ok(report);
    }
    mapper.reserve_object_ids(target.all_object_ids.iter().copied());
    let output_to_source = output_to_source_object_ids(mapper, output_plugin);
    let started = std::time::Instant::now();
    let (candidate_templates, prepared_actors) = prepare_placed_actor_templates(
        session,
        target_schema.as_ref(),
        &target,
        output_plugin,
        mapper.interner,
    )?;
    log_timing("prepare_placed_actors", started);
    if candidate_templates.is_empty() {
        report.warnings.push(mapper.interner.intern(&format!(
            "ess_spawn: skip=no_candidate_templates placed_actors={}",
            target.placed_actors.len()
        )));
        return Ok(report);
    }
    let started = std::time::Instant::now();
    let conditional_lvln_branches = build_conditional_lvln_branch_index_for_target_templates(
        session,
        &source_schema,
        mapper.interner,
        &target,
        mapper,
        output_plugin,
        source_plugin,
        config,
        &candidate_templates,
        &output_to_source,
    );
    log_timing("build_conditional_branches", started);
    if conditional_lvln_branches.is_empty() {
        report.warnings.push(mapper.interner.intern(&format!(
            "ess_spawn: skip=no_conditional_lvln_branches placed_actors={} candidate_templates={} output_to_source={}",
            target.placed_actors.len(),
            candidate_templates.len(),
            output_to_source.len()
        )));
        return Ok(report);
    }

    let actor_cells: HashSet<u32> = target
        .placed_actors
        .iter()
        .map(|actor| actor.cell_objid)
        .collect();
    let started = std::time::Instant::now();
    let (source_lctn_locals, cell_source_lctn_assignment) =
        source_location_assignments_for_target_cells(
            session,
            &source_schema,
            mapper.interner,
            &target,
            mapper,
            config,
            &output_to_source,
            &actor_cells,
        )?;
    log_timing("source_location_assignments", started);
    // No early return on empty assignments: unresolvable actors still get the
    // deterministic default branch below — FO4 must never keep the broad list.
    let started = std::time::Instant::now();
    let location_encounter_keywords = location_encounter_keywords_for_sources(
        session,
        &source_schema,
        mapper.interner,
        source_plugin,
        &source_lctn_locals,
    );
    log_timing("location_encounter_keywords", started);
    report.warnings.push(mapper.interner.intern(&format!(
        "ess_spawn: gates placed_actors={} actor_cells={} candidate_templates={} branch_templates={} assigned_cells={} keyword_locations={} keyword_locations_nonempty={} output_to_source={}",
        target.placed_actors.len(),
        actor_cells.len(),
        candidate_templates.len(),
        conditional_lvln_branches.len(),
        cell_source_lctn_assignment.len(),
        location_encounter_keywords.len(),
        location_encounter_keywords
            .values()
            .filter(|v| !v.is_empty())
            .count(),
        output_to_source.len()
    )));
    let started = std::time::Instant::now();
    let (added, changed, warnings) = specialize_placed_actor_templates(
        session,
        mapper,
        config,
        target_schema.as_ref(),
        &target,
        prepared_actors,
        &conditional_lvln_branches,
        &location_encounter_keywords,
        &cell_source_lctn_assignment,
        output_plugin,
    )?;
    log_timing("specialize_and_apply", started);
    report.records_added = added;
    report.records_changed = changed;
    report.warnings.extend(warnings);
    Ok(report)
}

fn eczn_location_object_id(record: &Record, interner: &StringInterner) -> Option<u32> {
    let data = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "DATA")?;
    if let FieldValue::Bytes(bytes) = &data.value {
        if bytes.len() >= 8 {
            let raw = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let object_id = raw & 0x00FF_FFFF;
            return (object_id != 0).then_some(object_id);
        }
    }
    let FieldValue::Struct(fields) = &data.value else {
        return first_form_key(&data.value)
            .map(|fk| fk.local & 0x00FF_FFFF)
            .filter(|object_id| *object_id != 0);
    };
    for (name, value) in fields {
        if interner
            .resolve(*name)
            .is_some_and(|name| name.eq_ignore_ascii_case("location"))
        {
            if let Some(fk) = first_form_key(value) {
                let object_id = fk.local & 0x00FF_FFFF;
                if object_id != 0 {
                    return Some(object_id);
                }
            }
        }
    }
    fields
        .iter()
        .filter_map(|(_, value)| first_form_key(value))
        .map(|fk| fk.local & 0x00FF_FFFF)
        .find(|object_id| *object_id != 0)
}

fn points_to_known_eczn(
    fk: FormKey,
    output_plugin: crate::sym::Sym,
    output_eczn_object_ids: &HashSet<u32>,
    master_eczn_object_ids: &HashMap<crate::sym::Sym, HashSet<u32>>,
) -> bool {
    let object_id = fk.local & 0x00FF_FFFF;
    if fk.plugin == output_plugin {
        return output_eczn_object_ids.contains(&object_id);
    }
    master_eczn_object_ids
        .get(&fk.plugin)
        .is_some_and(|ids| ids.contains(&object_id))
}

/// Final target-only cleanup for placed-record `XEZN`.
///
/// Early encounter-zone synthesis can repoint copied placed refs, but later
/// placed-child repair/copy paths can leave or restore FO76-style `XEZN` fields
/// that still name `LCTN`. At this point the output tree is complete, so build
/// the authoritative map from existing target `ECZN.DATA.Location` and normalize
/// every placed record once.
pub fn finalize_placed_xezn_targets(
    session: &mut PluginSession,
    config: &FixupConfig,
    interner: &StringInterner,
) -> Result<FixupReport, FixupError> {
    use rayon::prelude::*;

    let mut report = FixupReport::empty();
    let target_schema = config
        .target_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing target schema".into()))?;
    let eczn_sig = SigCode::from_str("ECZN").map_err(FixupError::SchemaError)?;
    let xezn_sig = SubrecordSig::from_str("XEZN").map_err(FixupError::SchemaError)?;
    let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
    let output_plugin = interner.intern(&output_plugin_name);

    let target_masters = session.target_masters().to_vec();
    let eczn_fks = session
        .form_keys_of_sig(eczn_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut lctn_to_eczn: HashMap<u32, FormKey> = HashMap::new();
    let mut output_eczn_object_ids: HashSet<u32> = HashSet::new();
    for eczn_fk in eczn_fks {
        output_eczn_object_ids.insert(eczn_fk.local & 0x00FF_FFFF);
        let Ok(record) = session.record_decoded(&eczn_fk, target_schema.as_ref(), interner) else {
            continue;
        };
        let Some(location_objid) = eczn_location_object_id(&record, interner) else {
            continue;
        };
        lctn_to_eczn.entry(location_objid).or_insert(eczn_fk);
    }

    let mut master_eczn_object_ids: HashMap<crate::sym::Sym, HashSet<u32>> = HashMap::new();
    for (master_name, handle_id) in target_masters
        .iter()
        .zip(config.target_master_handle_ids.iter().copied())
    {
        let fks = session
            .form_keys_of_sig_in_handle(handle_id, eczn_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if !fks.is_empty() {
            master_eczn_object_ids.insert(
                interner.intern(master_name),
                fks.into_iter().map(|fk| fk.local & 0x00FF_FFFF).collect(),
            );
        }
    }

    let mut placed_fks = Vec::new();
    for placed_sig in PLACED_SIGNATURES {
        let sig = SigCode::from_str(placed_sig).map_err(FixupError::SchemaError)?;
        placed_fks.extend(
            session
                .form_keys_of_sig(sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?,
        );
    }
    let placed_changes: Vec<Record> = {
        let view = session
            .target_read_view()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        placed_fks
            .par_iter()
            .filter_map(|placed_fk| {
                if !view.record_has_any_subrecord(placed_fk, &["XEZN"], interner) {
                    return None;
                }
                let mut record = view
                    .record_decoded(placed_fk, target_schema.as_ref(), interner)
                    .ok()?;
                let Some(idx) = record.fields.iter().position(|field| field.sig == xezn_sig) else {
                    return None;
                };
                let current = match &record.fields[idx].value {
                    FieldValue::FormKey(fk) => *fk,
                    _ => return None,
                };
                let current_objid = current.local & 0x00FF_FFFF;
                if current_objid == 0 {
                    return None;
                }

                let changed = if let Some(eczn_fk) = lctn_to_eczn.get(&current_objid).copied() {
                    if current == eczn_fk {
                        false
                    } else {
                        record.fields[idx].value = FieldValue::FormKey(eczn_fk);
                        true
                    }
                } else if points_to_known_eczn(
                    current,
                    output_plugin,
                    &output_eczn_object_ids,
                    &master_eczn_object_ids,
                ) {
                    false
                } else {
                    record.fields.remove(idx);
                    true
                };
                changed.then_some(record)
            })
            .collect()
    };

    report.records_changed = session
        .replace_records_contents(placed_changes, target_schema.as_ref(), interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    Ok(report)
}

/// `identity_resolve` enables the slice (bounded cell-slice) path: the host run
/// never ran `translate_all`, so the mapper is empty. With `preserve_source_ids`
/// the slice keeps object-ids 1:1, so we resolve a source `LCTN` to its target
/// by identity (against actual target presence) and reserve every existing
/// target id so a synthesized ECZN never steals a preserved LCTN id. It is a
/// no-op on the whole-plugin path, where `lookup` always hits after
/// `translate_all`.
pub fn synthesize_encounter_zones(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    identity_resolve: bool,
) -> Result<FixupReport, FixupError> {
    use rayon::prelude::*;

    let mut report = FixupReport::empty();
    let log_timing = |name: &str, started: std::time::Instant| {
        eprintln!(
            "[eczn_timing] {name} elapsed_ms={}",
            started.elapsed().as_millis()
        );
    };
    let phase_started = std::time::Instant::now();
    let output_plugin = mapper.output_plugin_sym();
    let lctn_sig = SigCode::from_str("LCTN").map_err(FixupError::SchemaError)?;
    let eczn_sig = SigCode::from_str("ECZN").map_err(FixupError::SchemaError)?;
    let xezn_sig = SubrecordSig::from_str("XEZN").map_err(FixupError::SchemaError)?;

    let target_schema = config
        .target_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing target schema".into()))?;
    let source_schema = config
        .source_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing source schema".into()))?;
    let started = std::time::Instant::now();
    let source_lctn_fks = session
        .source_form_keys_of_sig(lctn_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    log_timing("source_form_keys_of_sig", started);

    // Source plugin master order + name, needed to resolve the raw `world`
    // form_id inside a rawified LCEC footprint (see model::decode_lcec_footprint).
    let (source_masters, source_plugin_name) = match session.source_slot_opt() {
        Some(slot) => (
            slot.parsed.header.masters.clone(),
            slot.parsed.plugin_name.clone(),
        ),
        None => (Vec::new(), String::new()),
    };

    let started = std::time::Instant::now();
    let target = build_target_index(&session.target_slot().parsed.root_items, true);
    mapper.reserve_object_ids(target.all_object_ids.iter().copied());
    log_timing("build_target_index", started);
    let target_index = &target.grid;

    // Target-LCTN object-ids referenced by an interior cell's XLCN. Lets an
    // interior-only Location (band present, empty exterior footprint) qualify
    // for an ECZN so its interior cells can be stamped.
    let interior_lctn_targets: rustc_hash::FxHashSet<u32> = target
        .interior_cells
        .iter()
        .filter_map(|(_, xlcn)| *xlcn)
        .collect();

    // Target object-ids named by some placed record's XEZN. Lets a zero-band,
    // footprint-less, interior-cell-less Location still synthesize an ECZN when a
    // placed ref points its encounter zone at it — so the repoint step has a
    // target instead of stripping. Keyed by TARGET object-id (post-copy refs are
    // already remapped); compared against each LCTN's resolved target id.
    let placed_xezn_targets = &target.placed_xezn_targets;

    let started = std::time::Instant::now();
    let decoded_plans = {
        let source_id = session
            .source_id()
            .ok_or_else(|| FixupError::HandleError("missing source handle".into()))?;
        let scan_started = std::time::Instant::now();
        let source_scan = session
            .handle_raw_scan(source_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        log_timing("source_raw_scan_prime", scan_started);
        let raw_form_ids = source_scan.raw_form_ids_of_sig(lctn_sig);
        debug_assert_eq!(source_lctn_fks.len(), raw_form_ids.len());
        let decode = |(fk, raw_form_id): (&FormKey, &u32)| {
            let rec = match source_scan.record_decoded(
                *raw_form_id,
                fk,
                source_schema.as_ref(),
                mapper.interner,
            ) {
                Some(Ok(record)) => record,
                _ => return None,
            };
            let info = decode_lctn_info(&rec, mapper.interner);
            let class = classify(info.location_type, &info.keyword_locals);
            let footprint =
                decode_lcec_footprint(&rec, mapper.interner, &source_masters, &source_plugin_name);
            let editor_id = rec
                .eid
                .and_then(|sym| mapper.interner.resolve(sym))
                .map(str::to_string)
                .unwrap_or_default();
            Some((
                info.form_key.local,
                info.own_band,
                info.parent.map(|parent| parent.local),
                ZonePlan {
                    source_lctn: info.form_key,
                    class,
                    own_band: info.own_band,
                    footprint,
                    editor_id,
                },
            ))
        };
        if source_lctn_fks.len() < 64 {
            source_lctn_fks
                .iter()
                .zip(&raw_form_ids)
                .filter_map(decode)
                .collect::<Vec<_>>()
        } else {
            source_lctn_fks
                .par_iter()
                .zip(raw_form_ids.par_iter())
                .filter_map(decode)
                .collect::<Vec<_>>()
        }
    };
    log_timing("decode_source_lctns", started);
    let mut bands: HashMap<u32, Option<(u8, u8)>> = HashMap::new();
    let mut parents: HashMap<u32, Option<u32>> = HashMap::new();
    let mut plans: Vec<ZonePlan> = Vec::with_capacity(decoded_plans.len());
    for (local, own_band, parent, plan) in decoded_plans {
        bands.insert(local, own_band);
        parents.insert(local, parent);
        plans.push(plan);
    }

    let depth_of = |start: u32| -> u32 {
        let (mut d, mut cur, mut seen) = (0u32, parents.get(&start).and_then(|p| *p), vec![start]);
        while let Some(p) = cur {
            if seen.contains(&p) {
                break;
            }
            seen.push(p);
            d += 1;
            cur = parents.get(&p).and_then(|x| *x);
        }
        d
    };

    let mut cell_assignment: HashMap<u32, (u32, FormKey)> = HashMap::new();
    let mut workshop_lctns: Vec<(FormKey, WorkshopClass)> = Vec::new();
    // target-LCTN object-id -> synthesized ECZN FormKey. The synthesized ECZN's
    // DATA.location and the translated interior cell's XLCN both carry the target
    // LCTN, so keying by its object-id lets the interior pass match cells to
    // ECZNs without inverse-mapping back to the source.
    let mut target_lctn_to_eczn: HashMap<u32, FormKey> = HashMap::new();

    let started = std::time::Instant::now();
    for plan in &plans {
        let band = if plan.own_band.is_some() {
            plan.own_band
        } else {
            resolve_band(plan.source_lctn.local, &bands, &parents)
        };

        // Require a real source->target LCTN mapping. Without it we would emit
        // an ECZN whose DATA.location dangles at the source plugin and a keyword
        // rewrite that silently misses. On the slice path the mapper is empty
        // (no translate_all), so fall back to the preserved-id identity FK when
        // the target actually holds that LCTN; otherwise skip.
        let target_lctn = match mapper.lookup(plan.source_lctn) {
            Some(fk) => fk,
            None if identity_resolve
                && config.preserve_source_ids
                && target.lctn_object_ids.contains(&plan.source_lctn.local) =>
            {
                FormKey {
                    local: plan.source_lctn.local,
                    plugin: output_plugin,
                }
            }
            None => continue,
        };

        let target_objid = target_lctn.local & 0x00FF_FFFF;
        let has_interior_cells = interior_lctn_targets.contains(&target_objid);
        let referenced_by_placed_ref = placed_xezn_targets.contains(&target_objid);
        // Synthesize an ECZN for EVERY Location referenced as an encounter zone,
        // not just workshops: workshop class, OR a band (own/inherited), OR an
        // exterior footprint, OR interior cells pointing back at it (XLCN), OR a
        // placed ref's XEZN pointing at it. Zero-band zones get a default (0,0)
        // band below.
        let create = match plan.class {
            WorkshopClass::Settlement | WorkshopClass::Shelter => true,
            WorkshopClass::NonWorkshop => {
                band.is_some()
                    || !plan.footprint.is_empty()
                    || has_interior_cells
                    || referenced_by_placed_ref
            }
        };
        if !create {
            continue;
        }

        let synth_source = FormKey {
            local: plan.source_lctn.local,
            plugin: mapper.interner.intern(SYNTH_ECZN_PLUGIN),
        };
        let eczn_fk = mapper.allocate_or_resolve(synth_source, None, eczn_sig);

        let (min, max) = band.unwrap_or((0, 0));
        let eid = if plan.editor_id.is_empty() {
            format!("Synth{:06X}EncounterZone", plan.source_lctn.local)
        } else {
            eczn_editor_id(&plan.editor_id)
        };
        let eczn = build_eczn_record(
            eczn_fk,
            &eid,
            target_lctn,
            clamp_level(min),
            eczn_flags(plan.class),
            clamp_level(max),
            mapper.interner,
        );
        if session
            .add_record(eczn, target_schema.as_ref(), mapper.interner)
            .is_err()
        {
            continue;
        }
        report.records_added += 1;
        target_lctn_to_eczn.insert(target_lctn.local & 0x00FF_FFFF, eczn_fk);

        let depth = depth_of(plan.source_lctn.local);
        for (world_fk, gx, gy) in &plan.footprint {
            let world_objid = mapper
                .lookup(*world_fk)
                .map(|f| f.local)
                .unwrap_or(world_fk.local)
                & 0x00FF_FFFF;
            if let Some(cell_objid) = target_index.get(&(world_objid, *gx, *gy)).copied() {
                let e = cell_assignment
                    .entry(cell_objid)
                    .or_insert((u32::MIN, eczn_fk));
                if depth >= e.0 {
                    *e = (depth, eczn_fk);
                }
            }
        }
        if matches!(
            plan.class,
            WorkshopClass::Settlement | WorkshopClass::Shelter
        ) {
            workshop_lctns.push((target_lctn, plan.class));
        }
    }
    log_timing("plan_loop_add_eczns", started);

    // Repoint placed-ref XEZN → synthesized ECZN. FO76 placed refs carry
    // XEZN→LCTN (FO76 encounter zones are Locations); post-copy the formid is
    // already remapped to the target LCTN object-id. Now that ECZNs exist, set
    // XEZN to the LCTN's ECZN. Safety net: an XEZN still pointing at a target
    // LCTN with no synthesized ECZN (should not happen — every XEZN target is in
    // placed_xezn_targets and thus qualified above; only a master-resident LCTN
    // remains) is stripped, since FO4 hard-requires XEZN→ECZN. Placed records are
    // nested under cell groups, so use replace_record_contents (preserves the
    // tree position; replace_record would relocate them to a top-level group).
    // Only placed records that actually carry a non-null XEZN need touching —
    // `build_target_index` recorded their object-ids on its single raw walk, so we
    // decode just those instead of every placed record in the worldspace. Collect
    // the edits and apply them with ONE batched, single-traversal replace (the
    // per-record `replace_record_contents` is an O(changed × tree) scan).
    let started = std::time::Instant::now();
    let mut placed_changes: Vec<crate::record::Record> = Vec::new();
    for &ref_objid in &target.placed_xezn_ref_objids {
        let pfk = FormKey {
            local: ref_objid,
            plugin: output_plugin,
        };
        let mut rec = match session.record_decoded(&pfk, target_schema.as_ref(), mapper.interner) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let Some(idx) = rec.fields.iter().position(|f| f.sig == xezn_sig) else {
            continue;
        };
        // Only an output-plugin XEZN can name a converted LCTN with a synthesized
        // ECZN. A null XEZN is harmless (leave it). A master-resident XEZN (FO76
        // LCTN in a master, or any non-output plugin) has no FO4 ECZN equivalent,
        // so it falls through to the strip branch.
        let eczn_for_target = match &rec.fields[idx].value {
            FieldValue::FormKey(fk) if fk.local & 0x00FF_FFFF == 0 => continue,
            FieldValue::FormKey(fk) if fk.plugin == output_plugin => {
                target_lctn_to_eczn.get(&(fk.local & 0x00FF_FFFF)).copied()
            }
            FieldValue::FormKey(_) => None,
            // Non-formkey XEZN — leave it.
            _ => continue,
        };
        let changed = match eczn_for_target {
            Some(eczn_fk) => {
                if rec.fields[idx].value == FieldValue::FormKey(eczn_fk) {
                    false
                } else {
                    rec.fields[idx].value = FieldValue::FormKey(eczn_fk);
                    true
                }
            }
            // No synthesized ECZN for this target (master-resident LCTN, or a
            // converted LCTN that didn't qualify); strip the wrong-type XEZN so
            // FO4 doesn't dereference it.
            None => {
                rec.fields.remove(idx);
                true
            }
        };
        if changed {
            placed_changes.push(rec);
        }
    }
    report.records_changed += session
        .replace_records_contents(placed_changes, target_schema.as_ref(), mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    log_timing("xezn_repoint", started);

    // Interior pull-model: each interior cell names its own Location via XLCN.
    // Stamp XEZN when that target LCTN has a synthesized ECZN. Interior cells are
    // not in the grid footprint, so they never collide with the exterior
    // depth-priority assignment; insert them with max priority.
    for (cell_objid, xlcn) in &target.interior_cells {
        let Some(lctn_objid) = xlcn else { continue };
        if let Some(eczn_fk) = target_lctn_to_eczn.get(lctn_objid) {
            cell_assignment.insert(*cell_objid, (u32::MAX, *eczn_fk));
        }
    }

    // Stamp XEZN in place with ONE batched tree traversal. A structural
    // `replace_record` would (a) re-scan the whole tree + rebuild the index per
    // cell (O(cells × tree)) and (b) RELOCATE each cell to a top-level CELL group —
    // catastrophic for exterior footprint cells, which must stay nested under WRLD.
    // Content-replace mutates the cell where it sits, preserving position.
    let started = std::time::Instant::now();
    let mut cell_changes: Vec<crate::record::Record> = Vec::new();
    for (cell_objid, (_, eczn_fk)) in &cell_assignment {
        let cell_fk = FormKey {
            local: *cell_objid,
            plugin: eczn_fk.plugin,
        };
        let mut cell =
            match session.record_decoded(&cell_fk, target_schema.as_ref(), mapper.interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
        if let Some(f) = cell.fields.iter_mut().find(|f| f.sig == xezn_sig) {
            f.value = FieldValue::FormKey(*eczn_fk);
        } else {
            cell.fields.push(FieldEntry {
                sig: xezn_sig,
                value: FieldValue::FormKey(*eczn_fk),
            });
        }
        cell_changes.push(cell);
    }
    report.records_changed += session
        .replace_records_contents(cell_changes, target_schema.as_ref(), mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    log_timing("cell_xezn_stamp", started);

    let started = std::time::Instant::now();
    let fallout4 = mapper.interner.intern(FALLOUT4_ESM);
    let seventysix = mapper.interner.intern(SEVENTYSIX_ESM);
    let mut drop_fks: Vec<FormKey> = [
        mapper.lookup(FormKey {
            local: KW_WORKSHOP_PUBLIC,
            plugin: seventysix,
        }),
        mapper.lookup(FormKey {
            local: KW_WORKSHOP_SHELTER,
            plugin: seventysix,
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    if identity_resolve {
        // The slice mapper is empty; preserve_source_ids keeps FO76-only keyword
        // records (no FO4 equivalent) at their source id under the output plugin,
        // so that is the form the translated KWDA carries.
        for local in [KW_WORKSHOP_PUBLIC, KW_WORKSHOP_SHELTER] {
            drop_fks.push(FormKey {
                local,
                plugin: output_plugin,
            });
        }
    }
    let mut workshop_changes: Vec<crate::record::Record> = Vec::new();
    for (target_lctn, class) in workshop_lctns {
        let mut rec =
            match session.record_decoded(&target_lctn, target_schema.as_ref(), mapper.interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
        rebuild_keyword_fields(&mut rec, class, &drop_fks, fallout4, mapper.interner);
        workshop_changes.push(rec);
    }
    report.records_changed += session
        .replace_records_contents(workshop_changes, target_schema.as_ref(), mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    log_timing("workshop_keyword_rewrite", started);
    log_timing("synthesize_encounter_zones_total", phase_started);

    Ok(report)
}

#[cfg(test)]
mod interior_tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, ResolutionMode};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, ParsedSubrecord, ensure_interior_cell_and_child_group,
        insert_placed_child_into_cell_group, plugin_handle_new_native, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    const OUTPUT_PLUGIN: &str = "Converted.esm";

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn hexv(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn decoded_record(sig: &str, local: u32, plugin: crate::sym::Sym) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey { local, plugin },
            eid: None,
            flags: crate::record::RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn decoded_field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    /// Parser regression against the REAL `LChar_MainAll` (`51C55B`) payload
    /// sequence. The first entry carries five non-`EditorLocationHasKeyword`
    /// CTDAs (GetRandomPercent-vs-global + FO76-only functions) that must be
    /// ignored without poisoning the keyword-conditioned branches that follow.
    #[test]
    fn lvln_branch_parser_real_lchar_mainall_sequence() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let mut record = decoded_record("LVLN", 0x51C55B, plugin);
        record
            .fields
            .push(decoded_field("LVLO", FieldValue::Uint(0x5A75D9)));
        for ctda in [
            "8500000039755A004D00000000000000000000000000000000000000FFFFFFFF",
            "86000000361C62004D00000000000000000000000000000000000000FFFFFFFF",
            "00000000000000002C01000000000000000000000000000000000000FFFFFFFF",
            "010000000000803F6B030000A3785A00000000000000000000000000FFFFFFFF",
            "000000000000803F4A000000A5785A00000000000000000000000000FFFFFFFF",
        ] {
            record.fields.push(decoded_field(
                "CTDA",
                FieldValue::Bytes(hexv(ctda).into_iter().collect()),
            ));
        }
        record
            .fields
            .push(decoded_field("LVOV", FieldValue::Float(0.0)));
        record
            .fields
            .push(decoded_field("LVLO", FieldValue::Uint(0x51C577)));
        record.fields.push(decoded_field(
            "CTDA",
            FieldValue::Bytes(
                hexv("000000000000803F43020000DE810500000000000000000000000000FFFFFFFF")
                    .into_iter()
                    .collect(),
            ),
        ));
        record
            .fields
            .push(decoded_field("LVOV", FieldValue::Float(0.0)));

        let branches = lvln_condition_branch_keywords(&record, &interner);
        assert_eq!(branches.len(), 1, "only the keyword-conditioned entry maps");
        assert_eq!(
            branches.get(&0x0581DE).map(|fk| fk.local & 0x00FF_FFFF),
            Some(0x51C577),
            "ghoul keyword selects LChar_TGroupGhouls"
        );
    }

    /// `lctn_property_avifs` against the REAL Whitespring golf LCTN (`09A451`)
    /// PRPS bytes: ESSChanceMainGhouls (`594F`) + ESSChanceSubRadroach
    /// (`3E9D4F`), both value 5.0.
    #[test]
    fn lctn_property_avifs_real_whitespring_golf_prps() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let mut record = decoded_record("LCTN", 0x09A451, plugin);
        record.fields.push(decoded_field(
            "PRPS",
            FieldValue::Bytes(
                hexv("4F5900000000A040000000004F9D3E000000A04000000000")
                    .into_iter()
                    .collect(),
            ),
        ));
        let avifs = lctn_property_avifs(&record);
        assert_eq!(avifs.len(), 2);
        assert_eq!(avifs[0].0.local & 0x00FF_FFFF, 0x594F);
        assert_eq!(avifs[0].1, 5.0);
        assert_eq!(avifs[1].0.local & 0x00FF_FFFF, 0x3E9D4F);
        assert_eq!(avifs[1].1, 5.0);
    }

    /// An interior CELL (DATA interior bit set) carrying EDID/DATA/XCLW and an
    /// optional XLCN→`xlcn` (raw form_id incl. master byte).
    fn interior_cell_record(form_id: u32, eid: &str, xlcn: Option<u32>) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        let mut subrecords = vec![
            sub("EDID", edid),
            sub("DATA", vec![0x01u8, 0x00]),
            sub("XCLW", 3.0_f32.to_le_bytes().to_vec()),
        ];
        if let Some(loc) = xlcn {
            subrecords.push(sub("XLCN", loc.to_le_bytes().to_vec()));
        }
        ParsedRecord {
            signature: SmolStr::new("CELL"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    /// FO76 LCTN with EDID, a `struct:I,B,B,B,B` DATA carrying `(min,
    /// location_type, max)`, and an optional KWDA keyword array. The conversion
    /// decoder rawifies `struct:` codecs, so class qualification in these tests
    /// rides on the KWDA workshop keyword (decoded as a formid_array), mirroring
    /// the exterior path's `classify` reliance. Placed directly into the source
    /// slot's root.
    fn source_lctn_record(
        form_id: u32,
        eid: &str,
        min: u8,
        lt: u8,
        max: u8,
        keywords: &[u32],
    ) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        let data = vec![0, 0, 0, 0, 0, min, lt, max];
        let mut subrecords = vec![sub("EDID", edid), sub("DATA", data)];
        if !keywords.is_empty() {
            let mut kwda = Vec::with_capacity(keywords.len() * 4);
            for kw in keywords {
                kwda.extend_from_slice(&kw.to_le_bytes());
            }
            subrecords.push(sub("KSIZ", (keywords.len() as u32).to_le_bytes().to_vec()));
            subrecords.push(sub("KWDA", kwda));
        }
        ParsedRecord {
            signature: SmolStr::new("LCTN"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(155),
            version2: Some(0),
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn put_source_lctn(source: u64, record: ParsedRecord) {
        put_record_in_group(source, *b"LCTN", record);
    }

    fn put_record_in_group(handle: u64, label: [u8; 4], record: ParsedRecord) {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle).unwrap();
        if let Some(ParsedItem::Group(group)) = slot
            .parsed
            .root_items
            .iter_mut()
            .find(|item| matches!(item, ParsedItem::Group(g) if g.label == label))
        {
            group.children.push(ParsedItem::Record(record));
        } else {
            slot.parsed.root_items.push(ParsedItem::Group(
                esp_authoring_core::plugin_runtime::ParsedGroup {
                    label,
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Record(record)],
                },
            ));
        }
        slot.invalidate_sections();
    }

    fn source_avif_record(form_id: u32, eid: &str, keyword: u32) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        ParsedRecord {
            signature: SmolStr::new("AVIF"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(208),
            version2: Some(0),
            subrecords: vec![
                sub("EDID", edid),
                sub("NAM3", keyword.to_le_bytes().to_vec()),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn source_lctn_record_with_prps(
        form_id: u32,
        eid: &str,
        min: u8,
        lt: u8,
        max: u8,
        keywords: &[u32],
        avif: u32,
        value: f32,
    ) -> ParsedRecord {
        let mut record = source_lctn_record(form_id, eid, min, lt, max, keywords);
        let mut prps = Vec::new();
        prps.extend_from_slice(&avif.to_le_bytes());
        prps.extend_from_slice(&value.to_le_bytes());
        prps.extend_from_slice(&0u32.to_le_bytes());
        record.subrecords.insert(1, sub("PRPS", prps));
        record
    }

    fn source_lctn_record_with_raw_prps(
        form_id: u32,
        eid: &str,
        min: u8,
        lt: u8,
        max: u8,
        keywords: &[u32],
        prps: &[u8],
    ) -> ParsedRecord {
        let mut record = source_lctn_record(form_id, eid, min, lt, max, keywords);
        record.subrecords.insert(1, sub("PRPS", prps.to_vec()));
        record
    }

    fn source_lctn_record_with_raw_prps_and_lcec(
        form_id: u32,
        eid: &str,
        prps: &[u8],
        world: u32,
        grid_x: i16,
        grid_y: i16,
    ) -> ParsedRecord {
        let mut record = source_lctn_record_with_raw_prps(form_id, eid, 0, 0, 0, &[], prps);
        let mut lcec = Vec::new();
        lcec.extend_from_slice(&world.to_le_bytes());
        lcec.extend_from_slice(&grid_y.to_le_bytes());
        lcec.extend_from_slice(&grid_x.to_le_bytes());
        record.subrecords.push(sub("LCEC", lcec));
        record
    }

    fn ess_location_ctda(keyword: u32) -> Vec<u8> {
        let mut data = vec![0u8; 32];
        data[4..8].copy_from_slice(&1.0f32.to_le_bytes());
        data[8..10].copy_from_slice(&FO76_ESS_LOCATION_CONDITION_FUNCTION_ID.to_le_bytes());
        data[12..16].copy_from_slice(&keyword.to_le_bytes());
        data[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        data
    }

    fn source_lvln_condition_record(
        form_id: u32,
        eid: &str,
        branch: u32,
        keyword: u32,
    ) -> ParsedRecord {
        source_lvln_condition_record_with_ctda(form_id, eid, branch, &ess_location_ctda(keyword))
    }

    /// LVLN with two keyword-conditioned branch entries. `keyword_a` MUST be
    /// the higher keyword id so tests can distinguish keyword-selected
    /// branches from the lowest-keyword default fallback.
    fn source_lvln_two_branch_record(
        form_id: u32,
        eid: &str,
        branch_a: u32,
        keyword_a: u32,
        branch_b: u32,
        keyword_b: u32,
    ) -> ParsedRecord {
        let mut record = source_lvln_condition_record(form_id, eid, branch_a, keyword_a);
        record
            .subrecords
            .push(sub("LVLO", branch_b.to_le_bytes().to_vec()));
        record
            .subrecords
            .push(sub("CTDA", ess_location_ctda(keyword_b)));
        record
    }

    fn source_lvln_condition_record_with_ctda(
        form_id: u32,
        eid: &str,
        branch: u32,
        ctda: &[u8],
    ) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        ParsedRecord {
            signature: SmolStr::new("LVLN"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(208),
            version2: Some(0),
            subrecords: vec![
                sub("EDID", edid),
                sub("LLCT", vec![1]),
                sub("LVLO", branch.to_le_bytes().to_vec()),
                sub("CTDA", ctda.to_vec()),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn target_lvln_record(form_id: u32, eid: &str) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        ParsedRecord {
            signature: SmolStr::new("LVLN"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("EDID", edid), sub("LLCT", vec![0])],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn target_npc_template_record(form_id: u32, eid: &str, template: u32) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        ParsedRecord {
            signature: SmolStr::new("NPC_"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                sub("EDID", edid),
                sub("TPLT", template.to_le_bytes().to_vec()),
                sub("TPTA", template_actor_slots(template)),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn template_actor_slots(template: u32) -> Vec<u8> {
        let mut slots = vec![0u8; 13 * 4];
        for slot in [0usize, 1, 2, 4, 5, 7, 8, 9] {
            let offset = slot * 4;
            slots[offset..offset + 4].copy_from_slice(&template.to_le_bytes());
        }
        slots
    }

    /// FO4 LVLN LVLO entry (`struct:H,B,B,I,H,B,B`, 12 bytes) pointing at `npc`.
    fn lvlo_entry_bytes(npc: u32) -> Vec<u8> {
        let mut v = Vec::with_capacity(12);
        v.extend_from_slice(&1u16.to_le_bytes()); // level
        v.push(0); // unk
        v.push(0); // unk
        v.extend_from_slice(&npc.to_le_bytes()); // npc formid
        v.extend_from_slice(&1u16.to_le_bytes()); // count
        v.push(0); // chance none
        v.push(0); // unk
        v
    }

    fn target_lvln_record_with_entry(form_id: u32, eid: &str, leaf: u32) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        ParsedRecord {
            signature: SmolStr::new("LVLN"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                sub("EDID", edid),
                sub("LLCT", vec![1]),
                sub("LVLO", lvlo_entry_bytes(leaf)),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn target_npc_named_record(form_id: u32, eid: &str, name: &str) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        let mut full = name.as_bytes().to_vec();
        full.push(0);
        ParsedRecord {
            signature: SmolStr::new("NPC_"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("EDID", edid), sub("FULL", full)],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn find_cell<'a>(items: &'a [ParsedItem], object_id: u32) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(r)
                    if r.signature.as_str() == "CELL" && r.form_id & 0x00FF_FFFF == object_id =>
                {
                    return Some(r);
                }
                ParsedItem::Group(g) => {
                    if let Some(found) = find_cell(&g.children, object_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn xezn_target(record: &ParsedRecord) -> Option<u32> {
        record
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == "XEZN")
            .map(|s| u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]) & 0x00FF_FFFF)
    }

    /// A placed REFR carrying NAME + an XEZN raw form_id (the target LCTN it
    /// references as its encounter zone). Inserted into an interior cell's
    /// Temporary (group_type 9) child group.
    fn placed_refr(form_id: u32, xezn_target_form_id: u32) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("REFR"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                sub("NAME", 0x01000800u32.to_le_bytes().to_vec()),
                sub("XEZN", xezn_target_form_id.to_le_bytes().to_vec()),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn placed_achr(form_id: u32, base_form_id: u32) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("ACHR"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("NAME", base_form_id.to_le_bytes().to_vec())],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn exterior_cell_record(form_id: u32, eid: &str, grid_x: i32, grid_y: i32) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        let mut xclc = Vec::new();
        xclc.extend_from_slice(&grid_x.to_le_bytes());
        xclc.extend_from_slice(&grid_y.to_le_bytes());
        ParsedRecord {
            signature: SmolStr::new("CELL"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("EDID", edid), sub("XCLC", xclc)],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn ensure_exterior_cell_and_child_group(target: u64, world: u32, cell: ParsedRecord) {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&target).unwrap();
        let cell_id = cell.form_id;
        let world_label = world.to_le_bytes();
        let cell_child = ParsedItem::Group(esp_authoring_core::plugin_runtime::ParsedGroup {
            label: cell_id.to_le_bytes(),
            group_type: 6,
            tail: Bytes::new(),
            children: Vec::new(),
        });
        let children = vec![ParsedItem::Record(cell), cell_child];

        if let Some(ParsedItem::Group(wrld_group)) = slot
            .parsed
            .root_items
            .iter_mut()
            .find(|item| matches!(item, ParsedItem::Group(group) if group.label == *b"WRLD"))
        {
            if let Some(ParsedItem::Group(world_group)) =
                wrld_group.children.iter_mut().find(|item| {
                    matches!(
                        item,
                        ParsedItem::Group(group)
                            if group.group_type == 1 && group.label == world_label
                    )
                })
            {
                world_group.children.extend(children);
            } else {
                wrld_group.children.push(ParsedItem::Group(
                    esp_authoring_core::plugin_runtime::ParsedGroup {
                        label: world_label,
                        group_type: 1,
                        tail: Bytes::new(),
                        children,
                    },
                ));
            }
        } else {
            slot.parsed.root_items.push(ParsedItem::Group(
                esp_authoring_core::plugin_runtime::ParsedGroup {
                    label: *b"WRLD",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Group(
                        esp_authoring_core::plugin_runtime::ParsedGroup {
                            label: world_label,
                            group_type: 1,
                            tail: Bytes::new(),
                            children,
                        },
                    )],
                },
            ));
        }
        slot.invalidate_sections();
    }

    fn find_refr<'a>(items: &'a [ParsedItem], object_id: u32) -> Option<&'a ParsedRecord> {
        find_record_by_sig_id(items, "REFR", object_id)
    }

    fn find_record_by_sig_id<'a>(
        items: &'a [ParsedItem],
        sig: &str,
        object_id: u32,
    ) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(r)
                    if r.signature.as_str() == sig && r.form_id & 0x00FF_FFFF == object_id =>
                {
                    return Some(r);
                }
                ParsedItem::Group(g) => {
                    if let Some(found) = find_record_by_sig_id(&g.children, sig, object_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn subrecord_formid(record: &ParsedRecord, sig: &str) -> Option<u32> {
        record
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == sig)
            .map(|s| u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]) & 0x00FF_FFFF)
    }

    fn subrecord_formids(record: &ParsedRecord, sig: &str) -> Vec<u32> {
        record
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == sig)
            .map(|s| {
                s.data
                    .chunks_exact(4)
                    .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()) & 0x00FF_FFFF)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn invalidate_handle(handle: u64) {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        store.get_mut(&handle).unwrap().invalidate_sections();
    }

    fn config() -> FixupConfig {
        FixupConfig {
            preserve_source_ids: true,
            is_whole_plugin: true,
            target_schema: Some(AuthoringSchema::for_game("fo4").unwrap()),
            source_schema: Some(AuthoringSchema::for_game("fo76").unwrap()),
            ..FixupConfig::default()
        }
    }

    fn mapper_options() -> MapperOptions {
        MapperOptions {
            output_plugin_name: OUTPUT_PLUGIN.into(),
            resolution_mode: ResolutionMode::DeferAndFixup,
            ..MapperOptions::default()
        }
    }

    /// Interior cell whose Location LCTN has a synthesized ECZN ends up with
    /// XEZN = that ECZN.
    #[test]
    fn interior_cell_gets_xezn_when_lctn_has_eczn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        // Source workshop LCTN (no exterior footprint — interior-only). Workshop
        // class qualifies unconditionally via the KWDA keyword.
        let src_lctn_local = 0x0989F5u32;
        put_source_lctn(
            source,
            source_lctn_record(
                src_lctn_local,
                "LocVaultX",
                20,
                0,
                99,
                &[crate::fixups::encounter_zones::model::KW_WORKSHOP],
            ),
        );
        // Target LCTN id (preserved identity) + interior cell pointing at it.
        let tgt_lctn_local = src_lctn_local;
        let cell_local = 0x00275EDEu32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "VaultXCell", Some(tgt_lctn_local)),
        )
        .unwrap();
        // The target must also hold the LCTN id so identity-resolve qualifies it.
        put_target_lctn(target, tgt_lctn_local);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: tgt_lctn_local,
                plugin: out,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(report.records_added, 1, "one ECZN synthesized");

        // The synthesized ECZN object-id is what the cell's XEZN must point at.
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let eczn = find_record_by_sig(&slot.parsed.root_items, "ECZN").expect("ECZN added");
        let eczn_objid = eczn.form_id & 0x00FF_FFFF;
        let cell = find_cell(&slot.parsed.root_items, cell_local).expect("interior cell");
        assert_eq!(
            xezn_target(cell),
            Some(eczn_objid),
            "interior cell XEZN points at synthesized ECZN"
        );
    }

    /// Interior cell whose Location has NO synthesizable ECZN (its XLCN names a
    /// LCTN with no source record — e.g. a master-resident Location) gets no
    /// XEZN. Under the relaxed gate ANY LCTN with a source record + an interior
    /// cell qualifies, so the only "no ECZN" case left is an unresolvable target.
    #[test]
    fn interior_cell_without_eczn_gets_no_xezn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        // The interior cell points at a target LCTN id for which there is NO
        // source LCTN record, so synthesis can decode nothing and creates no ECZN.
        let missing_lctn_local = 0x00ABCDEFu32;
        let cell_local = 0x00275EE0u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "PlainYCell", Some(missing_lctn_local)),
        )
        .unwrap();

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());

        {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let report =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            assert_eq!(report.records_added, 0, "no ECZN for unresolvable LCTN");
        }

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let cell = find_cell(&slot.parsed.root_items, cell_local).expect("interior cell");
        assert_eq!(xezn_target(cell), None, "no XEZN stamped");
    }

    /// A NON-workshop Location with a band (no keyword, no footprint)
    /// synthesizes an ECZN.
    #[test]
    fn non_workshop_banded_lctn_synthesizes_eczn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        // Banded, non-workshop LCTN: min=10, type=0 (not workshop), max=40, no kw.
        let src_lctn_local = 0x0989F7u32;
        put_source_lctn(
            source,
            source_lctn_record(src_lctn_local, "LocBandedA", 10, 0, 40, &[]),
        );
        let tgt_lctn_local = src_lctn_local;
        put_target_lctn(target, tgt_lctn_local);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: tgt_lctn_local,
                plugin: out,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(
            report.records_added, 1,
            "banded non-workshop LCTN synthesizes one ECZN"
        );
    }

    #[test]
    fn synthesized_eczn_skips_existing_target_object_id() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let src_lctn_local = 0x0989F9u32;
        put_source_lctn(
            source,
            source_lctn_record(src_lctn_local, "LocCollisionA", 10, 0, 40, &[]),
        );
        put_target_lctn(target, src_lctn_local);

        let collision_local = 0x00A0033Fu32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(collision_local, "ExistingGeneratedBandCell", None),
        )
        .unwrap();

        let mut state = crate::formkey_mapper::MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: OUTPUT_PLUGIN.into(),
                generated_object_id_floor: collision_local,
                resolution_mode: ResolutionMode::DeferAndFixup,
                ..MapperOptions::default()
            },
        );
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: src_lctn_local,
                plugin: out,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(report.records_added, 1, "one ECZN synthesized");

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let eczn = find_record_by_sig(&slot.parsed.root_items, "ECZN").expect("ECZN added");
        assert_eq!(
            eczn.form_id & 0x00FF_FFFF,
            collision_local + 1,
            "ECZN allocation skips target records that were not allocated by the mapper"
        );
        assert!(
            find_cell(&slot.parsed.root_items, collision_local).is_some(),
            "preexisting target CELL remains at the collision id"
        );
    }

    /// A zero-band Location referenced ONLY by a placed ref's XEZN still
    /// synthesizes a (default-band) ECZN, and that ref's XEZN is REPOINTED to the
    /// ECZN — not stripped.
    #[test]
    fn placed_ref_xezn_repointed_to_synthesized_eczn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        // Zero-band, non-workshop, footprint-less, interior-cell-less LCTN. Only a
        // placed ref's XEZN references it.
        let src_lctn_local = 0x0989F8u32;
        put_source_lctn(
            source,
            source_lctn_record(src_lctn_local, "LocZeroBandA", 0, 0, 0, &[]),
        );
        let tgt_lctn_local = src_lctn_local;
        put_target_lctn(target, tgt_lctn_local);

        // An interior cell to host the placed REFR (the cell's own XLCN is
        // unrelated — set to None so the cell itself doesn't pull an ECZN).
        let cell_local = 0x00275EF0u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "HostCell", None),
        )
        .unwrap();
        let refr_local = 0x00300010u32;
        // XEZN raw form_id carries the output-plugin master byte; masked it is the
        // target LCTN object-id.
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(refr_local, tgt_lctn_local),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: tgt_lctn_local,
                plugin: out,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(
            report.records_added, 1,
            "zero-band LCTN referenced by a ref's XEZN synthesizes a default-band ECZN"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let eczn = find_record_by_sig(&slot.parsed.root_items, "ECZN").expect("ECZN added");
        let eczn_objid = eczn.form_id & 0x00FF_FFFF;
        let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(refr),
            Some(eczn_objid),
            "placed ref XEZN repointed to the synthesized ECZN (not stripped)"
        );
    }

    /// A placed ref whose XEZN points at a LCTN with no synthesizable ECZN
    /// (master-resident, no source record) is stripped by the synthesis safety
    /// net.
    #[test]
    fn placed_ref_xezn_stripped_when_no_eczn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let cell_local = 0x00275EF2u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "HostCell2", None),
        )
        .unwrap();
        let refr_local = 0x00300012u32;
        // XEZN → a LCTN id with no source LCTN record → no ECZN → safety-net strip.
        let missing_lctn_local = 0x00ABCDEFu32;
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(refr_local, missing_lctn_local),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());

        {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
        }

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(refr),
            None,
            "placed ref XEZN with no ECZN target is stripped"
        );
    }

    #[test]
    fn finalizer_repoints_placed_xezn_from_existing_eczn_location() {
        let interner = StringInterner::new();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let lctn_local = 0x0955B5u32;
        let eczn_local = 0xA15300u32;
        put_target_lctn(target, lctn_local);
        add_target_eczn(target, &interner, eczn_local, lctn_local);

        let cell_local = 0x00275EF4u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "HostCell3", None),
        )
        .unwrap();
        let refr_local = 0x00300014u32;
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(refr_local, lctn_local),
        )
        .unwrap();
        invalidate_handle(target);

        let report = {
            let mut session = open_session(target, None).unwrap();
            let report = finalize_placed_xezn_targets(&mut session, &config(), &interner).unwrap();
            session.flush_pending_effects();
            report
        };
        assert_eq!(report.records_changed, 1, "stale placed XEZN repaired");

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(refr),
            Some(eczn_local),
            "placed REFR XEZN points at the existing ECZN tied to its LCTN"
        );
        assert_eq!(
            subrecord_formid(refr, "XEZN"),
            Some(eczn_local),
            "decoded and raw XEZN agree"
        );
    }

    #[test]
    fn finalizer_repoints_after_late_repair_restores_lctn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let src_lctn_local = 0x098A10u32;
        put_source_lctn(
            source,
            source_lctn_record(src_lctn_local, "LocLateRepairA", 10, 0, 20, &[]),
        );
        put_target_lctn(target, src_lctn_local);
        let cell_local = 0x00275EF6u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "LateRepairHost", None),
        )
        .unwrap();
        let refr_local = 0x00300016u32;
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(refr_local, src_lctn_local),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: src_lctn_local,
                plugin: out,
            },
        );

        {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
        }

        let eczn_objid = {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&target).unwrap();
            let eczn = find_record_by_sig(&slot.parsed.root_items, "ECZN").expect("ECZN added");
            let eczn_objid = eczn.form_id & 0x00FF_FFFF;
            let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
            assert_eq!(
                xezn_target(refr),
                Some(eczn_objid),
                "synthesis initially repoints the placed XEZN"
            );
            eczn_objid
        };

        set_placed_xezn(target, refr_local, src_lctn_local);

        {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&target).unwrap();
            let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
            assert_eq!(
                xezn_target(refr),
                Some(src_lctn_local),
                "simulated late repair restored the stale LCTN"
            );
        }

        let report = {
            let mut session = open_session(target, None).unwrap();
            let report = finalize_placed_xezn_targets(&mut session, &config(), &interner).unwrap();
            session.flush_pending_effects();
            report
        };
        assert_eq!(
            report.records_changed, 1,
            "finalizer repairs late stale XEZN"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(refr),
            Some(eczn_objid),
            "late finalizer restores XEZN to the synthesized ECZN"
        );
    }

    #[test]
    fn finalizer_strips_placed_xezn_when_no_eczn_for_lctn() {
        let interner = StringInterner::new();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let lctn_local = 0x0955B6u32;
        put_target_lctn(target, lctn_local);
        let cell_local = 0x00275EF8u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "HostCell4", None),
        )
        .unwrap();
        let refr_local = 0x00300018u32;
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(refr_local, lctn_local),
        )
        .unwrap();
        invalidate_handle(target);

        let report = {
            let mut session = open_session(target, None).unwrap();
            let report = finalize_placed_xezn_targets(&mut session, &config(), &interner).unwrap();
            session.flush_pending_effects();
            report
        };
        assert_eq!(report.records_changed, 1, "wrong-type XEZN stripped");

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(refr),
            None,
            "placed REFR XEZN without an ECZN target is stripped"
        );
    }

    #[test]
    fn placed_actor_template_specialized_from_location_ess_property() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let src_lctn_local = 0x09A451u32;
        let avif_local = 0x00594Fu32;
        let ghoul_keyword = 0x0581DEu32;
        let main_all = 0x51C55Bu32;
        let ghoul_branch = 0x51C577u32;
        let base_npc = 0x1D513Du32;
        let cell_local = 0xA044E2u32;
        let actor_local = 0x1F0F76u32;

        put_record_in_group(
            source,
            *b"AVIF",
            source_avif_record(avif_local, "ESSChanceMainGhouls", ghoul_keyword),
        );
        put_source_lctn(
            source,
            source_lctn_record_with_prps(
                src_lctn_local,
                "LocWhitespringGolf",
                0,
                0,
                0,
                &[],
                avif_local,
                5.0,
            ),
        );
        put_record_in_group(
            source,
            *b"LVLN",
            source_lvln_condition_record(main_all, "LChar_MainAll", ghoul_branch, ghoul_keyword),
        );

        put_target_lctn(target, src_lctn_local);
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(main_all, "LChar_MainAll"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(ghoul_branch, "LChar_TGroupGhouls"),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_template_record(base_npc, "LvlMainMelee", main_all),
        );
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "WhitespringGolfClub", Some(src_lctn_local)),
        )
        .unwrap();
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_achr(actor_local, base_npc),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        for local in [src_lctn_local, main_all, ghoul_branch, base_npc] {
            state.source_to_target.insert(
                FormKey {
                    local,
                    plugin: sevsix,
                },
                FormKey { local, plugin: out },
            );
        }

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let mut r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            let specialized = specialize_placed_actor_templates_after_ref_repair(
                &mut session,
                &mut mapper,
                &config(),
            )
            .unwrap();
            r.records_added += specialized.records_added;
            r.records_changed += specialized.records_changed;
            session.flush_pending_effects();
            r
        };
        assert!(
            report.records_added >= 2,
            "ECZN plus specialized NPC clone should be added"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let actor = find_record_by_sig_id(&slot.parsed.root_items, "ACHR", actor_local)
            .expect("placed actor");
        let clone_local = subrecord_formid(actor, "NAME").expect("ACHR NAME");
        assert_ne!(clone_local, base_npc, "ACHR base is redirected to a clone");
        let clone = find_record_by_sig_id(&slot.parsed.root_items, "NPC_", clone_local)
            .expect("specialized NPC clone");
        assert_eq!(
            subrecord_formid(clone, "TPLT"),
            Some(ghoul_branch),
            "clone template points at the location-selected ghoul branch"
        );
        let template_actor_slots = subrecord_formids(clone, "TPTA");
        for slot in [0usize, 1, 2, 4, 5, 7, 8, 9] {
            assert_eq!(
                template_actor_slots.get(slot).copied(),
                Some(ghoul_branch),
                "clone TemplateActors slot {slot} points at the location-selected ghoul branch"
            );
        }
        assert!(
            !template_actor_slots.contains(&main_all),
            "clone no longer inherits TemplateActors from LChar_MainAll"
        );
    }

    #[test]
    fn placed_actor_template_specialized_after_ref_repair_from_whitespring_payloads() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let src_lctn_local = 0x09A451u32;
        let avif_local = 0x00594Fu32;
        let ghoul_keyword = 0x0581DEu32;
        let main_all = 0x51C55Bu32;
        let ghoul_branch = 0x51C577u32;
        let base_npc = 0x1D513Du32;
        let cell_local = 0xA044E2u32;
        let actor_local = 0x1F0F76u32;
        let whitespring_prps = [
            0x4F, 0x59, 0x00, 0x00, 0x00, 0x00, 0xA0, 0x40, 0x00, 0x00, 0x00, 0x00, 0x4F, 0x9D,
            0x3E, 0x00, 0x00, 0x00, 0xA0, 0x40, 0x00, 0x00, 0x00, 0x00,
        ];
        let real_ghoul_ctda = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x43, 0x02, 0x00, 0x00, 0xDE, 0x81,
            0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];

        put_record_in_group(
            source,
            *b"AVIF",
            source_avif_record(avif_local, "ESSChanceMainGhouls", ghoul_keyword),
        );
        put_source_lctn(
            source,
            source_lctn_record_with_raw_prps(
                src_lctn_local,
                "LocWhitespringTheWhitespringGolfLocation",
                0,
                0,
                0,
                &[],
                &whitespring_prps,
            ),
        );
        put_record_in_group(
            source,
            *b"LVLN",
            source_lvln_condition_record_with_ctda(
                main_all,
                "LChar_MainAll",
                ghoul_branch,
                &real_ghoul_ctda,
            ),
        );

        put_target_lctn(target, src_lctn_local);
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(main_all, "LChar_MainAll"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(ghoul_branch, "LChar_TGroupGhouls"),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_template_record(base_npc, "LvlMainMelee", main_all),
        );
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "WhitespringGolfClub", Some(src_lctn_local)),
        )
        .unwrap();
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_achr(actor_local, base_npc),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        for local in [src_lctn_local, main_all, ghoul_branch, base_npc] {
            state.source_to_target.insert(
                FormKey {
                    local,
                    plugin: sevsix,
                },
                FormKey { local, plugin: out },
            );
        }

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r = specialize_placed_actor_templates_after_ref_repair(
                &mut session,
                &mut mapper,
                &config(),
            )
            .unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(report.records_added, 1, "late pass adds one NPC clone");
        assert_eq!(
            report.records_changed, 1,
            "late pass redirects the placed actor base"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let actor = find_record_by_sig_id(&slot.parsed.root_items, "ACHR", actor_local)
            .expect("placed actor");
        let clone_local = subrecord_formid(actor, "NAME").expect("ACHR NAME");
        assert_ne!(clone_local, base_npc, "ACHR base is redirected to a clone");
        let clone = find_record_by_sig_id(&slot.parsed.root_items, "NPC_", clone_local)
            .expect("late specialized NPC clone");
        assert_eq!(subrecord_formid(clone, "TPLT"), Some(ghoul_branch));
        assert!(
            !subrecord_formids(clone, "TPTA").contains(&main_all),
            "late clone no longer inherits TemplateActors from LChar_MainAll"
        );
    }

    /// Regression: repeated specialization invocations share one clone identity
    /// per (base, branch). An actor specialized later must get a clone derived
    /// from its own base, never the clone another base minted earlier.
    #[test]
    fn second_pass_actor_not_cross_wired_onto_first_pass_clone() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();
        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");

        // Family 1 ("critter"): base1 → main1 → branch1, selected by kw1 at loc1.
        let (avif1, kw1, main1, branch1, base1, lctn1, cell1, actor1) = (
            0x005001u32,
            0x058101u32,
            0x51C401u32,
            0x3E52FDu32,
            0x1D5301u32,
            0x09A001u32,
            0xA00001u32,
            0x1F0001u32,
        );
        // Family 2 ("turret"): base2 → main2 → branch2, selected by kw2 at loc2.
        let (avif2, kw2, main2, branch2, base2, lctn2, cell2, actor2) = (
            0x005002u32,
            0x058102u32,
            0x51C402u32,
            0x2B131Eu32,
            0x1D5302u32,
            0x09A002u32,
            0xA00002u32,
            0x1F0002u32,
        );

        for (avif, kw, main, branch, base, lctn, cell, base_eid, main_eid, branch_eid, cell_eid) in [
            (
                avif1,
                kw1,
                main1,
                branch1,
                base1,
                lctn1,
                cell1,
                "LvlCritterA",
                "LChar_CritterA",
                "TCritter",
                "CritterCell",
            ),
            (
                avif2,
                kw2,
                main2,
                branch2,
                base2,
                lctn2,
                cell2,
                "LvlMainTurret",
                "LChar_MainTurret",
                "TTurret",
                "TurretCell",
            ),
        ] {
            put_record_in_group(source, *b"AVIF", source_avif_record(avif, "ESSChance", kw));
            put_source_lctn(
                source,
                source_lctn_record_with_prps(lctn, "Loc", 0, 0, 0, &[], avif, 5.0),
            );
            put_record_in_group(
                source,
                *b"LVLN",
                source_lvln_condition_record(main, main_eid, branch, kw),
            );
            put_target_lctn(target, lctn);
            put_record_in_group(target, *b"LVLN", target_lvln_record(main, main_eid));
            put_record_in_group(target, *b"LVLN", target_lvln_record(branch, branch_eid));
            put_record_in_group(
                target,
                *b"NPC_",
                target_npc_template_record(base, base_eid, main),
            );
            ensure_interior_cell_and_child_group(
                target,
                interior_cell_record(cell, cell_eid, Some(lctn)),
            )
            .unwrap();
        }

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        for local in [lctn1, main1, branch1, base1, lctn2, main2, branch2, base2] {
            state.source_to_target.insert(
                FormKey {
                    local,
                    plugin: sevsix,
                },
                FormKey { local, plugin: out },
            );
        }

        // Pass 1 sees only actor1.
        insert_placed_child_into_cell_group(target, cell1, 9, placed_achr(actor1, base1)).unwrap();
        invalidate_handle(target);
        let report1 = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r = specialize_placed_actor_templates_after_ref_repair(
                &mut session,
                &mut mapper,
                &config(),
            )
            .unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(report1.records_added, 1, "pass 1 mints one clone for base1");

        let clone1 = {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&target).unwrap();
            let a1 = find_record_by_sig_id(&slot.parsed.root_items, "ACHR", actor1).unwrap();
            let c1 = subrecord_formid(a1, "NAME").unwrap();
            let clone = find_record_by_sig_id(&slot.parsed.root_items, "NPC_", c1).unwrap();
            assert_eq!(
                subrecord_formid(clone, "TPLT"),
                Some(branch1),
                "actor1 clone → branch1"
            );
            c1
        };

        // Pass 2 (SAME state) now also sees actor2, first specialized here.
        insert_placed_child_into_cell_group(target, cell2, 9, placed_achr(actor2, base2)).unwrap();
        invalidate_handle(target);
        let report2 = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r = specialize_placed_actor_templates_after_ref_repair(
                &mut session,
                &mut mapper,
                &config(),
            )
            .unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(
            report2.records_added, 1,
            "pass 2 mints a NEW clone for base2 (not the reused/aliased pass-1 clone)"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let a2 = find_record_by_sig_id(&slot.parsed.root_items, "ACHR", actor2).unwrap();
        let clone2 = subrecord_formid(a2, "NAME").unwrap();
        assert_ne!(
            clone2, clone1,
            "actor2 must NOT be cross-wired onto base1's clone from pass 1"
        );
        let clone2_rec = find_record_by_sig_id(&slot.parsed.root_items, "NPC_", clone2).unwrap();
        assert_eq!(
            subrecord_formid(clone2_rec, "TPLT"),
            Some(branch2),
            "actor2's clone is derived from base2 (turret branch), not base1 (critter)"
        );
        // actor1 stays on its own, uncorrupted clone.
        let a1 = find_record_by_sig_id(&slot.parsed.root_items, "ACHR", actor1).unwrap();
        assert_eq!(
            subrecord_formid(a1, "NAME"),
            Some(clone1),
            "actor1 clone unchanged"
        );
        let clone1_rec = find_record_by_sig_id(&slot.parsed.root_items, "NPC_", clone1).unwrap();
        assert_eq!(
            subrecord_formid(clone1_rec, "TPLT"),
            Some(branch1),
            "actor1's clone still points at branch1 (not overwritten by pass 2)"
        );
    }

    /// The ESS clone inherits its name by templating through the branch (a
    /// leveled list of name-less templated NPCs) — FO4 will not chase that chain
    /// for the display name, so the clone must bake a concrete FULL. This mirrors
    /// the real `LvlMainLong_ESS_51C56D` → `LChar_TGroupMoleMiners` → leaf →
    /// `EncMoleMiner_*` ("Mole Miner") case.
    #[test]
    fn resolve_and_bake_display_name_walks_lvln_and_template_chain() {
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();
        let out = interner.intern(OUTPUT_PLUGIN);

        let (branch, leaf, terminal) = (0x51C56Du32, 0x44CF82u32, 0x3D14E2u32);
        // branch LVLN → leaf; leaf has no own FULL, Traits(TPTA slot 0) → terminal;
        // terminal carries the concrete FULL.
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record_with_entry(branch, "LChar_TGroupMoleMiners", leaf),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_template_record(leaf, "LvlMoleMinerMissile", terminal),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_named_record(terminal, "EncMoleMiner_Voice01", "Mole Miner"),
        );
        invalidate_handle(target);

        let mut session = open_session(target, None).unwrap();
        let mut visited: HashSet<FormKey> = HashSet::new();
        let resolved = resolve_branch_display_name(
            &mut session,
            &schema,
            &interner,
            FormKey {
                local: branch,
                plugin: out,
            },
            &mut visited,
            0,
        );
        let (full, _shrt) = resolved.expect("resolves FULL through LVLN + template hop");
        assert_eq!(full.sig.as_str(), "FULL", "resolved field is a FULL");

        // Bake it into a name-less clone: inserted before DATA, and idempotent.
        let mut clone = decoded_record("NPC_", 0x00F0_001F, out);
        clone.fields.push(decoded_field(
            "CNAM",
            FieldValue::FormKey(FormKey {
                local: 0x31FA79,
                plugin: out,
            }),
        ));
        clone.fields.push(decoded_field("DATA", FieldValue::None));
        bake_display_name(&mut clone, full.clone(), None);
        let full_pos = clone
            .fields
            .iter()
            .position(|f| f.sig.as_str() == "FULL")
            .expect("FULL baked onto clone");
        let data_pos = clone
            .fields
            .iter()
            .position(|f| f.sig.as_str() == "DATA")
            .unwrap();
        assert!(
            full_pos < data_pos,
            "FULL inserted before DATA (vanilla order)"
        );
        bake_display_name(&mut clone, full, None);
        assert_eq!(
            clone
                .fields
                .iter()
                .filter(|f| f.sig.as_str() == "FULL")
                .count(),
            1,
            "baking is idempotent — no duplicate FULL"
        );
    }

    /// Runs the late pass against a prepared source/target pair and returns
    /// the specialized clone's TPLT branch for the given placed actor.
    fn run_late_pass_and_clone_tplt(
        source: u64,
        target: u64,
        interner: &StringInterner,
        state: &mut crate::formkey_mapper::MapperState,
        actor_local: u32,
        base_npc: u32,
    ) -> Option<u32> {
        let report = {
            let mut mapper = crate::formkey_mapper::FormKeyMapper::from_state(state, interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let report = specialize_placed_actor_templates_after_ref_repair(
                &mut session,
                &mut mapper,
                &config(),
            )
            .unwrap();
            session.flush_pending_effects();
            report
        };
        assert_eq!(report.records_added, 1, "one NPC clone added");
        assert_eq!(report.records_changed, 1, "placed actor base redirected");
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let actor = find_record_by_sig_id(&slot.parsed.root_items, "ACHR", actor_local)
            .expect("placed actor");
        let clone_local = subrecord_formid(actor, "NAME").expect("ACHR NAME");
        assert_ne!(clone_local, base_npc, "ACHR base redirected to a clone");
        let clone = find_record_by_sig_id(&slot.parsed.root_items, "NPC_", clone_local)
            .expect("specialized NPC clone");
        subrecord_formid(clone, "TPLT")
    }

    /// A leaf LCTN with no PRPS of its own must inherit encounter keywords
    /// from its PNAM parent chain (real Whitespring shape: species AVIFs live
    /// on ancestor locations).
    #[test]
    fn placed_actor_specialized_via_lctn_parent_chain_keywords() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let leaf_lctn = 0x09A452u32;
        let parent_lctn = 0x0989F5u32;
        let avif_local = 0x00594Fu32;
        let ghoul_keyword = 0x0581DEu32;
        let other_keyword = 0x0581DDu32;
        let main_all = 0x51C55Bu32;
        let ghoul_branch = 0x51C577u32;
        let other_branch = 0x51C561u32;
        let base_npc = 0x1D513Du32;
        let cell_local = 0xA044E2u32;
        let actor_local = 0x1F0F76u32;
        // Real 09A451-style PRPS row: AVIF 594F, value 5.0.
        let parent_prps = [
            0x4F, 0x59, 0x00, 0x00, 0x00, 0x00, 0xA0, 0x40, 0x00, 0x00, 0x00, 0x00,
        ];

        put_record_in_group(
            source,
            *b"AVIF",
            source_avif_record(avif_local, "ESSChanceMainGhouls", ghoul_keyword),
        );
        let mut leaf = source_lctn_record(leaf_lctn, "LocGolfLeaf", 0, 0, 0, &[]);
        leaf.subrecords
            .push(sub("PNAM", parent_lctn.to_le_bytes().to_vec()));
        put_source_lctn(source, leaf);
        put_source_lctn(
            source,
            source_lctn_record_with_raw_prps(
                parent_lctn,
                "LocWhitespring",
                0,
                0,
                0,
                &[],
                &parent_prps,
            ),
        );
        put_record_in_group(
            source,
            *b"LVLN",
            // ghoul keyword is the HIGHER id: a lowest-keyword default pick
            // would choose other_branch, so TPLT==ghoul proves keyword walk.
            source_lvln_two_branch_record(
                main_all,
                "LChar_MainAll",
                ghoul_branch,
                ghoul_keyword,
                other_branch,
                other_keyword,
            ),
        );

        put_target_lctn(target, leaf_lctn);
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(main_all, "LChar_MainAll"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(ghoul_branch, "LChar_TGroupGhouls"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(other_branch, "LChar_TGroupOther"),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_template_record(base_npc, "LvlMainMelee", main_all),
        );
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "GolfLeafCell", Some(leaf_lctn)),
        )
        .unwrap();
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_achr(actor_local, base_npc),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        for local in [leaf_lctn, main_all, ghoul_branch, other_branch, base_npc] {
            state.source_to_target.insert(
                FormKey {
                    local,
                    plugin: sevsix,
                },
                FormKey { local, plugin: out },
            );
        }

        let tplt = run_late_pass_and_clone_tplt(
            source,
            target,
            &interner,
            &mut state,
            actor_local,
            base_npc,
        );
        assert_eq!(
            tplt,
            Some(ghoul_branch),
            "parent-chain PRPS selects the ghoul branch, not the lowest-keyword default"
        );
    }

    /// A target cell that lost its XLCN resolves through the SOURCE cell's
    /// own XLCN before any default fallback.
    #[test]
    fn placed_actor_specialized_via_source_cell_xlcn_fallback() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let src_lctn = 0x09A451u32;
        let avif_local = 0x00594Fu32;
        let ghoul_keyword = 0x0581DEu32;
        let other_keyword = 0x0581DDu32;
        let main_all = 0x51C55Bu32;
        let ghoul_branch = 0x51C577u32;
        let other_branch = 0x51C561u32;
        let base_npc = 0x1D513Du32;
        let cell_local = 0xA044E2u32;
        let actor_local = 0x1F0F76u32;
        let prps = [
            0x4F, 0x59, 0x00, 0x00, 0x00, 0x00, 0xA0, 0x40, 0x00, 0x00, 0x00, 0x00,
        ];

        put_record_in_group(
            source,
            *b"AVIF",
            source_avif_record(avif_local, "ESSChanceMainGhouls", ghoul_keyword),
        );
        put_source_lctn(
            source,
            source_lctn_record_with_raw_prps(src_lctn, "LocGolf", 0, 0, 0, &[], &prps),
        );
        // SOURCE cell carries the XLCN the target cell lost.
        put_record_in_group(
            source,
            *b"CELL",
            interior_cell_record(cell_local, "SrcGolfCell", Some(src_lctn)),
        );
        put_record_in_group(
            source,
            *b"LVLN",
            source_lvln_two_branch_record(
                main_all,
                "LChar_MainAll",
                ghoul_branch,
                ghoul_keyword,
                other_branch,
                other_keyword,
            ),
        );

        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(main_all, "LChar_MainAll"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(ghoul_branch, "LChar_TGroupGhouls"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(other_branch, "LChar_TGroupOther"),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_template_record(base_npc, "LvlMainMelee", main_all),
        );
        // Target cell has NO XLCN.
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "GolfCell", None),
        )
        .unwrap();
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_achr(actor_local, base_npc),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        for local in [main_all, ghoul_branch, other_branch, base_npc] {
            state.source_to_target.insert(
                FormKey {
                    local,
                    plugin: sevsix,
                },
                FormKey { local, plugin: out },
            );
        }

        let tplt = run_late_pass_and_clone_tplt(
            source,
            target,
            &interner,
            &mut state,
            actor_local,
            base_npc,
        );
        assert_eq!(
            tplt,
            Some(ghoul_branch),
            "source-cell XLCN fallback selects the ghoul branch"
        );
    }

    /// An actor with no resolvable location evidence must STILL leave the
    /// broad conditional template: it gets the deterministic default branch
    /// (lowest keyword id when nothing was location-selected).
    #[test]
    fn unresolvable_placed_actor_specialized_to_default_branch() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let ghoul_keyword = 0x0581DEu32;
        let other_keyword = 0x0581DDu32;
        let main_all = 0x51C55Bu32;
        let ghoul_branch = 0x51C577u32;
        let other_branch = 0x51C561u32;
        let base_npc = 0x1D513Du32;
        let cell_local = 0xA044E2u32;
        let actor_local = 0x1F0F76u32;

        // No LCTN, no AVIF, no source cell: nothing to resolve a location by.
        put_record_in_group(
            source,
            *b"LVLN",
            source_lvln_two_branch_record(
                main_all,
                "LChar_MainAll",
                ghoul_branch,
                ghoul_keyword,
                other_branch,
                other_keyword,
            ),
        );

        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(main_all, "LChar_MainAll"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(ghoul_branch, "LChar_TGroupGhouls"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(other_branch, "LChar_TGroupOther"),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_template_record(base_npc, "LvlMainMelee", main_all),
        );
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "NowhereCell", None),
        )
        .unwrap();
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_achr(actor_local, base_npc),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        for local in [main_all, ghoul_branch, other_branch, base_npc] {
            state.source_to_target.insert(
                FormKey {
                    local,
                    plugin: sevsix,
                },
                FormKey { local, plugin: out },
            );
        }

        let tplt = run_late_pass_and_clone_tplt(
            source,
            target,
            &interner,
            &mut state,
            actor_local,
            base_npc,
        );
        assert_eq!(
            tplt,
            Some(other_branch),
            "default fallback picks the lowest keyword id branch deterministically"
        );
    }

    #[test]
    fn placed_actor_template_specialized_after_ref_repair_from_source_footprint() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let src_lctn_local = 0x0989F5u32;
        let avif_local = 0x00594Fu32;
        let ghoul_keyword = 0x0581DEu32;
        let main_all = 0x51C55Bu32;
        let ghoul_branch = 0x51C577u32;
        let base_npc = 0x1D5137u32;
        let world = 0x25DA15u32;
        let cell_local = 0xA03695u32;
        let actor_local = 0x1EE0B8u32;
        let grid_x = -4i16;
        let grid_y = -13i16;
        let whitespring_prps = [
            0x4F, 0x59, 0x00, 0x00, 0x00, 0x00, 0xA0, 0x40, 0x00, 0x00, 0x00, 0x00,
        ];
        let real_ghoul_ctda = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x43, 0x02, 0x00, 0x00, 0xDE, 0x81,
            0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];

        put_record_in_group(
            source,
            *b"AVIF",
            source_avif_record(avif_local, "ESSChanceMainGhouls", ghoul_keyword),
        );
        put_source_lctn(
            source,
            source_lctn_record_with_raw_prps_and_lcec(
                src_lctn_local,
                "LocWhitespringTheWhitespringLocation",
                &whitespring_prps,
                world,
                grid_x,
                grid_y,
            ),
        );
        put_record_in_group(
            source,
            *b"LVLN",
            source_lvln_condition_record_with_ctda(
                main_all,
                "LChar_MainAll",
                ghoul_branch,
                &real_ghoul_ctda,
            ),
        );

        put_target_lctn(target, src_lctn_local);
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(main_all, "LChar_MainAll"),
        );
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(ghoul_branch, "LChar_TGroupGhouls"),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_template_record(base_npc, "LvlMainShort", main_all),
        );
        ensure_exterior_cell_and_child_group(
            target,
            world,
            exterior_cell_record(
                cell_local,
                "WhitespringExterior",
                grid_x.into(),
                grid_y.into(),
            ),
        );
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_achr(actor_local, base_npc),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        for local in [main_all, ghoul_branch, base_npc] {
            state.source_to_target.insert(
                FormKey {
                    local,
                    plugin: sevsix,
                },
                FormKey { local, plugin: out },
            );
        }

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r = specialize_placed_actor_templates_after_ref_repair(
                &mut session,
                &mut mapper,
                &config(),
            )
            .unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(
            report.records_added, 1,
            "late exterior pass adds one NPC clone"
        );
        assert_eq!(
            report.records_changed, 1,
            "late exterior pass redirects the placed actor base"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let actor = find_record_by_sig_id(&slot.parsed.root_items, "ACHR", actor_local)
            .expect("placed actor");
        let clone_local = subrecord_formid(actor, "NAME").expect("ACHR NAME");
        assert_ne!(clone_local, base_npc, "ACHR base is redirected to a clone");
        let clone = find_record_by_sig_id(&slot.parsed.root_items, "NPC_", clone_local)
            .expect("late specialized NPC clone");
        assert_eq!(subrecord_formid(clone, "TPLT"), Some(ghoul_branch));
        assert!(
            !subrecord_formids(clone, "TPTA").contains(&main_all),
            "late clone no longer inherits TemplateActors from LChar_MainAll"
        );
    }

    #[test]
    fn placed_actor_template_falls_back_when_selected_branch_maps_to_master() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let src_lctn_local = 0x09A451u32;
        let avif_local = 0x00594Fu32;
        let ghoul_keyword = 0x0581DEu32;
        let main_all = 0x51C55Bu32;
        let ghoul_branch = 0x51C577u32;
        let base_npc = 0x1D513Du32;
        let cell_local = 0xA044E2u32;
        let actor_local = 0x1F0F76u32;

        put_record_in_group(
            source,
            *b"AVIF",
            source_avif_record(avif_local, "ESSChanceMainGhouls", ghoul_keyword),
        );
        put_source_lctn(
            source,
            source_lctn_record_with_prps(
                src_lctn_local,
                "LocWhitespringTheWhitespringGolfLocation",
                0,
                0,
                0,
                &[],
                avif_local,
                5.0,
            ),
        );
        put_record_in_group(
            source,
            *b"LVLN",
            source_lvln_condition_record(main_all, "LChar_MainAll", ghoul_branch, ghoul_keyword),
        );

        put_target_lctn(target, src_lctn_local);
        put_record_in_group(
            target,
            *b"LVLN",
            target_lvln_record(main_all, "LChar_MainAll"),
        );
        put_record_in_group(
            target,
            *b"NPC_",
            target_npc_template_record(base_npc, "LvlMainMelee", main_all),
        );
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "WhitespringGolfClub", Some(src_lctn_local)),
        )
        .unwrap();
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_achr(actor_local, base_npc),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        for local in [src_lctn_local, main_all, base_npc] {
            state.source_to_target.insert(
                FormKey {
                    local,
                    plugin: sevsix,
                },
                FormKey { local, plugin: out },
            );
        }
        state.source_to_target.insert(
            FormKey {
                local: ghoul_branch,
                plugin: sevsix,
            },
            FormKey {
                local: 0x2196DC,
                plugin: fallout4,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r = specialize_placed_actor_templates_after_ref_repair(
                &mut session,
                &mut mapper,
                &config(),
            )
            .unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(report.records_added, 0);
        assert_eq!(report.records_changed, 0);
        assert!(report.warnings.iter().any(|warning| {
            interner
                .resolve(*warning)
                .is_some_and(|text| text.contains("ess_spawn_fallback:external_branch"))
        }));
    }

    fn put_target_lctn(target: u64, local: u32) {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&target).unwrap();
        let mut edid = b"TgtLctn".to_vec();
        edid.push(0);
        let record = ParsedRecord {
            signature: SmolStr::new("LCTN"),
            form_id: local,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("EDID", edid)],
            raw_payload: None,
            parse_error: None,
        };
        if let Some(ParsedItem::Group(group)) = slot
            .parsed
            .root_items
            .iter_mut()
            .find(|item| matches!(item, ParsedItem::Group(g) if &g.label == b"LCTN"))
        {
            group.children.push(ParsedItem::Record(record));
        } else {
            slot.parsed.root_items.push(ParsedItem::Group(
                esp_authoring_core::plugin_runtime::ParsedGroup {
                    label: *b"LCTN",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Record(record)],
                },
            ));
        }
        slot.invalidate_sections();
    }

    fn add_target_eczn(target: u64, interner: &StringInterner, eczn_local: u32, lctn_local: u32) {
        let out = interner.intern(OUTPUT_PLUGIN);
        let cfg = config();
        let schema = cfg.target_schema.as_ref().unwrap();
        let mut session = open_session(target, None).unwrap();
        session
            .add_record(
                build_eczn_record(
                    FormKey {
                        local: eczn_local,
                        plugin: out,
                    },
                    "ExistingLocationEncounterZone",
                    FormKey {
                        local: lctn_local,
                        plugin: out,
                    },
                    10,
                    0,
                    20,
                    interner,
                ),
                schema.as_ref(),
                interner,
            )
            .unwrap();
        session.flush_pending_effects();
    }

    fn set_placed_xezn(target: u64, object_id: u32, raw_form_id: u32) {
        fn walk(items: &mut [ParsedItem], object_id: u32, raw_form_id: u32) -> bool {
            for item in items {
                match item {
                    ParsedItem::Record(record)
                        if record.form_id & 0x00FF_FFFF == object_id
                            && record.signature.as_str() == "REFR" =>
                    {
                        let raw = raw_form_id.to_le_bytes().to_vec();
                        if let Some(subrecord) = record
                            .subrecords
                            .iter_mut()
                            .find(|subrecord| subrecord.signature.as_str() == "XEZN")
                        {
                            subrecord.data = Bytes::from(raw);
                        } else {
                            record.subrecords.push(sub("XEZN", raw));
                        }
                        return true;
                    }
                    ParsedItem::Group(group) => {
                        if walk(&mut group.children, object_id, raw_form_id) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            false
        }

        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&target).unwrap();
        assert!(walk(&mut slot.parsed.root_items, object_id, raw_form_id));
        slot.invalidate_sections();
    }

    fn find_record_by_sig<'a>(items: &'a [ParsedItem], sig: &str) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(r) if r.signature.as_str() == sig => return Some(r),
                ParsedItem::Group(g) => {
                    if let Some(found) = find_record_by_sig(&g.children, sig) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// Live-data gate diagnostic for ESS placed-actor specialization. Loads the
/// REAL generated output ESM and the REAL FO76 source ESM (paths overridable
/// via `ESS_LIVE_TARGET` / `ESS_LIVE_SOURCE`) and replays every gate of
/// `specialize_placed_actor_templates_after_ref_repair` with printed counts
/// plus Whitespring-chain probes (actor 1EE0B8, cell A044E2, LCTN 09A451,
/// LVLN 51C55B). Run:
///
/// ```text
/// cargo test -p conversion_native --release live_ess_spawn_gate_diagnostic -- --ignored --nocapture
/// ```
#[cfg(test)]
mod live_ess_diagnostics {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState, ResolutionMode};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        parse_plugin_file, plugin_handle_new_native, plugin_handle_store_ref,
    };

    const PROBE_ACTOR: u32 = 0x1EE0B8;
    const PROBE_CELL: u32 = 0xA044E2;
    const PROBE_LCTN: u32 = 0x09A451;
    const PROBE_LVLN: u32 = 0x51C55B;
    const PROBE_BASE: u32 = 0x1D5136;
    const PROBE_KYWD: u32 = 0x0581DE;

    fn load_real_handle(path: &str, game: &str) -> u64 {
        let parsed =
            parse_plugin_file(path, Some(game.to_string()), true).expect("parse real plugin");
        let plugin_name = parsed.plugin_name.clone();
        let handle = plugin_handle_new_native(&plugin_name, Some(game)).expect("new handle");
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle).unwrap();
        slot.parsed = parsed;
        slot.invalidate_sections();
        handle
    }

    fn describe(value: &FieldValue) -> String {
        match value {
            FieldValue::None => "None".into(),
            FieldValue::Bool(b) => format!("Bool({b})"),
            FieldValue::Int(v) => format!("Int({v})"),
            FieldValue::Uint(v) => format!("Uint({v:#X})"),
            FieldValue::Float(v) => format!("Float({v})"),
            FieldValue::String(_) => "String".into(),
            FieldValue::FormKey(fk) => format!("FormKey({:06X})", fk.local & 0x00FF_FFFF),
            FieldValue::Bytes(b) => format!(
                "Bytes[{}]({})",
                b.len(),
                b.iter()
                    .take(32)
                    .map(|x| format!("{x:02X}"))
                    .collect::<String>()
            ),
            FieldValue::Struct(fields) => format!("Struct{{{} fields}}", fields.len()),
            FieldValue::List(items) => format!("List[{}]", items.len()),
        }
    }

    #[test]
    #[ignore]
    fn live_ess_spawn_gate_diagnostic() {
        let Ok(target_path) = std::env::var("ESS_LIVE_TARGET") else {
            eprintln!("live_ess: SKIP (ESS_LIVE_TARGET unset)");
            return;
        };
        let Ok(source_path) = std::env::var("ESS_LIVE_SOURCE") else {
            eprintln!("live_ess: SKIP (ESS_LIVE_SOURCE unset)");
            return;
        };
        if !std::path::Path::new(&target_path).exists()
            || !std::path::Path::new(&source_path).exists()
        {
            eprintln!("live_ess: SKIP (target or source ESM missing)");
            return;
        }

        let interner = StringInterner::new();
        eprintln!("live_ess: parsing target {target_path}");
        let target = load_real_handle(&target_path, "fo4");
        eprintln!("live_ess: parsing source {source_path}");
        let source = load_real_handle(&source_path, "fo76");

        let config = FixupConfig {
            preserve_source_ids: true,
            is_whole_plugin: true,
            target_schema: Some(AuthoringSchema::for_game("fo4").unwrap()),
            source_schema: Some(AuthoringSchema::for_game("fo76").unwrap()),
            ..FixupConfig::default()
        };
        let opts = MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            preserve_source_ids: true,
            resolution_mode: ResolutionMode::DeferAndFixup,
            ..MapperOptions::default()
        };
        let mut state = MapperState::new([], opts);
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut session = open_session(target, Some(source)).unwrap();

        let target_schema = config.target_schema.clone().unwrap();
        let source_schema = config.source_schema.clone().unwrap();
        let output_plugin = mapper.output_plugin_sym();

        // Gate 1: source slot / plugin sym.
        let source_plugin = source_plugin_sym(&mut session, mapper.interner);
        eprintln!(
            "gate1 source_plugin = {:?}",
            source_plugin.and_then(|s| mapper.interner.resolve(s))
        );
        let Some(source_plugin) = source_plugin else {
            panic!("gate1 FAILED: no source plugin");
        };

        // Gate 2: target index / placed actors.
        let tindex = build_target_index(&session.target_slot().parsed.root_items, true);
        eprintln!(
            "gate2 placed_actors={} cell_locations={} grid={} all_ids={}",
            tindex.placed_actors.len(),
            tindex.cell_locations.len(),
            tindex.grid.len(),
            tindex.all_object_ids.len()
        );
        let probe = tindex
            .placed_actors
            .iter()
            .find(|a| a.ref_objid == PROBE_ACTOR);
        eprintln!(
            "  probe actor {PROBE_ACTOR:06X}: {:?}",
            probe.map(|a| format!("cell={:06X}", a.cell_objid))
        );
        eprintln!(
            "  probe cell {PROBE_CELL:06X} XLCN: {:?}",
            tindex
                .cell_locations
                .get(&PROBE_CELL)
                .map(|v| format!("{v:06X}"))
        );

        mapper.reserve_object_ids(tindex.all_object_ids.iter().copied());
        let output_to_source = output_to_source_object_ids(&mapper, output_plugin);
        eprintln!("  output_to_source entries={}", output_to_source.len());

        // Gate 3: candidate templates from placed-actor bases.
        let (candidates, prepared_actors) = prepare_placed_actor_templates(
            &mut session,
            target_schema.as_ref(),
            &tindex,
            output_plugin,
            mapper.interner,
        )
        .unwrap();
        eprintln!(
            "gate3 candidate_templates={} contains_{PROBE_LVLN:06X}={}",
            candidates.len(),
            candidates.contains(&PROBE_LVLN)
        );
        if candidates.is_empty() {
            // Deep probe: decode the known actor + base directly.
            let actor_fk = FormKey {
                local: PROBE_ACTOR,
                plugin: output_plugin,
            };
            match session.record_decoded(&actor_fk, target_schema.as_ref(), mapper.interner) {
                Ok(rec) => {
                    eprintln!("  actor decode ok sig={}", rec.sig.as_str());
                    for f in &rec.fields {
                        if f.sig.as_str() == "NAME" {
                            eprintln!("    NAME = {}", describe(&f.value));
                        }
                    }
                }
                Err(e) => eprintln!("  actor decode FAILED: {e:?}"),
            }
            let base_fk = FormKey {
                local: PROBE_BASE,
                plugin: output_plugin,
            };
            match session.record_decoded(&base_fk, target_schema.as_ref(), mapper.interner) {
                Ok(rec) => {
                    eprintln!("  base decode ok sig={}", rec.sig.as_str());
                    for f in &rec.fields {
                        if matches!(f.sig.as_str(), "TPLT" | "TPTA") {
                            eprintln!("    {} = {}", f.sig.as_str(), describe(&f.value));
                        }
                    }
                }
                Err(e) => eprintln!("  base decode FAILED: {e:?}"),
            }
        }

        // Gate 4: conditional LVLN branch index from the SOURCE ESM.
        let branches = build_conditional_lvln_branch_index_for_target_templates(
            &mut session,
            &source_schema,
            mapper.interner,
            &tindex,
            &mapper,
            output_plugin,
            source_plugin,
            &config,
            &candidates,
            &output_to_source,
        );
        eprintln!("gate4 conditional_lvln_branches={}", branches.len());
        match branches.get(&PROBE_LVLN) {
            Some(b) => {
                let mut kws: Vec<_> = b
                    .iter()
                    .map(|(k, v)| format!("{k:06X}->{:06X}", v.local & 0x00FF_FFFF))
                    .collect();
                kws.sort();
                eprintln!("  {PROBE_LVLN:06X} branches: {}", kws.join(" "));
                eprintln!(
                    "  ghoul keyword {PROBE_KYWD:06X} present: {}",
                    b.contains_key(&PROBE_KYWD)
                );
            }
            None => {
                eprintln!("  {PROBE_LVLN:06X} NOT in branch index — deep probe:");
                let fk = FormKey {
                    local: PROBE_LVLN,
                    plugin: source_plugin,
                };
                match session.source_record_decoded(&fk, source_schema.as_ref(), mapper.interner) {
                    Ok(rec) => {
                        eprintln!(
                            "  source {PROBE_LVLN:06X} decoded sig={} fields={} warnings={:?}",
                            rec.sig.as_str(),
                            rec.fields.len(),
                            rec.warnings
                                .iter()
                                .filter_map(|w| mapper.interner.resolve(*w))
                                .collect::<Vec<_>>()
                        );
                        for f in rec.fields.iter().take(20) {
                            eprintln!("    {} = {}", f.sig.as_str(), describe(&f.value));
                        }
                        let parsed = lvln_condition_branch_keywords(&rec, mapper.interner);
                        eprintln!("  direct branch parse -> {} branches", parsed.len());
                    }
                    Err(e) => eprintln!("  source {PROBE_LVLN:06X} decode FAILED: {e:?}"),
                }
            }
        }

        // Gate 5: source location assignment for actor cells.
        let actor_cells: HashSet<u32> = tindex.placed_actors.iter().map(|a| a.cell_objid).collect();
        eprintln!(
            "  actor_cells={} contains_{PROBE_CELL:06X}={}",
            actor_cells.len(),
            actor_cells.contains(&PROBE_CELL)
        );
        let (lctn_locals, assignment) = source_location_assignments_for_target_cells(
            &mut session,
            &source_schema,
            mapper.interner,
            &tindex,
            &mapper,
            &config,
            &output_to_source,
            &actor_cells,
        )
        .unwrap();
        eprintln!(
            "gate5 source_lctn_locals={} assignments={}",
            lctn_locals.len(),
            assignment.len()
        );
        eprintln!(
            "  {PROBE_CELL:06X} assignment: {:?}",
            assignment
                .get(&PROBE_CELL)
                .map(|(d, l)| format!("depth={d} lctn={l:06X}"))
        );

        // Gate 6: encounter keywords per source location.
        let kw = location_encounter_keywords_for_sources(
            &mut session,
            &source_schema,
            mapper.interner,
            source_plugin,
            &lctn_locals,
        );
        let nonempty = kw.values().filter(|v| !v.is_empty()).count();
        eprintln!("gate6 keyword_locations={} nonempty={}", kw.len(), nonempty);
        if let Some(k) = kw.get(&PROBE_LCTN) {
            eprintln!(
                "  {PROBE_LCTN:06X} keywords: {:?}",
                k.iter()
                    .map(|(k, v)| format!("{k:06X}@{v}"))
                    .collect::<Vec<_>>()
            );
        } else {
            eprintln!("  {PROBE_LCTN:06X} not in keyword map");
        }

        // Full per-actor pass.
        let (added, changed, warnings) = specialize_placed_actor_templates(
            &mut session,
            &mut mapper,
            &config,
            target_schema.as_ref(),
            &tindex,
            prepared_actors,
            &branches,
            &kw,
            &assignment,
            output_plugin,
        )
        .unwrap();
        eprintln!(
            "pass: added={added} changed={changed} warnings={}",
            warnings.len()
        );
        for w in warnings.iter().take(60) {
            eprintln!("  WARN {}", mapper.interner.resolve(*w).unwrap_or("?"));
        }
    }
}
