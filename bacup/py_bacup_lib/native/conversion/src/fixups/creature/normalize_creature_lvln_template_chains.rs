//! Fixup: collapse recursive creature LVLN template chains.
//!
//! FO4 supports LVLN template actors in some vanilla humanoid records, so the
//! converter must not blindly clear every LVLN template slot. FO76 creatures can
//! emit a different pattern that is unsafe for FO4 runtime animation init:
//!
//! `LVLN -> NPC_ entry -> TPLT/TPTA -> LVLN`
//!
//! For creature LVLNs, replace that NPC entry with the concrete entries from the
//! delegated LVLN. Also flatten direct nested creature LVLN entries. This keeps
//! normal template NPCs intact while removing recursive creature list selection.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::creature::creature_internal_fixup_applies;
use crate::fixups::creature::creature_predicate::{
    npc_race_form_key, record_has_actor_type_creature, record_has_actor_type_npc,
};
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const LVLO_REFERENCE_OFFSET: usize = 4;
const LVLO_COUNT_OFFSET: usize = 8;
const LVLO_MIN_LEN: usize = 12;
const MAX_LVLO_ENTRIES: usize = u8::MAX as usize;
const MAX_CONVERGENCE_WAVES: usize = 64;
const TPTA_SLOT_WIDTH: usize = 4;
const TPTA_SLOT_COUNT: usize = 13;

pub struct NormalizeCreatureLvlnTemplateChainsFixup;

impl Fixup for NormalizeCreatureLvlnTemplateChainsFixup {
    fn name(&self) -> &'static str {
        "normalize_creature_lvln_template_chains"
    }

    fn scope(&self) -> FixupScope {
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
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let interner = mapper.interner;
        let target_masters = session.target_masters().to_vec();
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();

        let lvln_sig =
            SigCode::from_str("LVLN").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let mut lvln_records: FxHashMap<FormKey, Record> = FxHashMap::default();
        for fk in session
            .form_keys_of_sig(lvln_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            match session.record_decoded(&fk, target_schema, interner) {
                Ok(record) => {
                    lvln_records.insert(fk, record);
                }
                Err(_) => {}
            }
        }
        if lvln_records.is_empty() {
            return Ok(FixupReport::empty());
        }
        let lvln_fks: FxHashSet<FormKey> = lvln_records.keys().copied().collect();

        let mut creature_npcs: FxHashSet<FormKey> = FxHashSet::default();
        let mut npc_template_lvlns: FxHashMap<FormKey, Vec<FormKey>> = FxHashMap::default();

        for fk in session
            .form_keys_of_sig(npc_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            let Ok(record) = session.record_decoded(&fk, target_schema, interner) else {
                continue;
            };
            if npc_is_direct_creature(&record, session, target_schema, interner) {
                creature_npcs.insert(fk);
            }
            let template_lvlns = npc_template_lvln_refs(
                &record,
                &lvln_fks,
                target_masters.as_slice(),
                &target_plugin_name,
                interner,
            );
            if !template_lvlns.is_empty() {
                npc_template_lvlns.insert(fk, template_lvlns);
            }
        }

        let creature_lvlns = infer_creature_lvlns(
            &lvln_records,
            &npc_template_lvlns,
            &mut creature_npcs,
            target_masters.as_slice(),
            &target_plugin_name,
            interner,
        );
        if creature_lvlns.is_empty() {
            return Ok(FixupReport::empty());
        }

        let records_changed = converge_lvln_worklist(
            self.name(),
            &mut lvln_records,
            &npc_template_lvlns,
            &creature_lvlns,
            target_masters.as_slice(),
            &target_plugin_name,
            interner,
            |changed_fks, changed_records, lvln_records| {
                let records_changed = session
                    .replace_records_contents(changed_records, target_schema, interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                session.flush_pending_effects();
                for fk in changed_fks {
                    match session.record_decoded(fk, target_schema, interner) {
                        Ok(record) => {
                            lvln_records.insert(*fk, record);
                        }
                        Err(_) => {
                            lvln_records.remove(fk);
                        }
                    }
                }
                Ok(records_changed)
            },
        )?;

        let mut report = FixupReport::empty();
        report.records_changed = records_changed.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn converge_lvln_worklist(
    fixup_name: &'static str,
    lvln_records: &mut FxHashMap<FormKey, Record>,
    npc_template_lvlns: &FxHashMap<FormKey, Vec<FormKey>>,
    creature_lvlns: &FxHashSet<FormKey>,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
    mut apply_wave: impl FnMut(
        &[FormKey],
        Vec<Record>,
        &mut FxHashMap<FormKey, Record>,
    ) -> Result<usize, FixupError>,
) -> Result<usize, FixupError> {
    let reverse_dependencies = build_reverse_dependencies(
        lvln_records,
        npc_template_lvlns,
        creature_lvlns,
        target_masters,
        target_plugin_name,
        interner,
    );
    let mut pending = lvln_records
        .keys()
        .copied()
        .filter(|fk| creature_lvlns.contains(fk))
        .collect::<Vec<_>>();
    let mut completed_waves = 0usize;
    let mut records_changed = 0usize;

    while !pending.is_empty() {
        let changed_records = normalize_lvln_candidates(
            pending.as_slice(),
            lvln_records,
            npc_template_lvlns,
            creature_lvlns,
            target_masters,
            target_plugin_name,
            interner,
        );
        if changed_records.is_empty() {
            break;
        }
        let changed_fks = changed_records
            .iter()
            .map(|record| record.form_key)
            .collect::<Vec<_>>();
        records_changed = records_changed.saturating_add(apply_wave(
            changed_fks.as_slice(),
            changed_records,
            lvln_records,
        )?);
        completed_waves += 1;
        if completed_waves > MAX_CONVERGENCE_WAVES {
            return Err(FixupError::ConvergenceFailure(fixup_name));
        }
        pending = dependents_of_changed(changed_fks.as_slice(), &reverse_dependencies);
    }

    Ok(records_changed)
}

fn normalize_lvln_candidates(
    candidates: &[FormKey],
    lvln_records: &FxHashMap<FormKey, Record>,
    npc_template_lvlns: &FxHashMap<FormKey, Vec<FormKey>>,
    creature_lvlns: &FxHashSet<FormKey>,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<Record> {
    let mut changed_records = Vec::new();
    for fk in candidates {
        let Some(mut record) = lvln_records.get(fk).cloned() else {
            continue;
        };
        if normalize_lvln_record(
            &mut record,
            lvln_records,
            npc_template_lvlns,
            creature_lvlns,
            target_masters,
            target_plugin_name,
            interner,
        ) {
            changed_records.push(record);
        }
    }
    changed_records
}

fn build_reverse_dependencies(
    lvln_records: &FxHashMap<FormKey, Record>,
    npc_template_lvlns: &FxHashMap<FormKey, Vec<FormKey>>,
    creature_lvlns: &FxHashSet<FormKey>,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> FxHashMap<FormKey, FxHashSet<FormKey>> {
    let mut reverse_dependencies: FxHashMap<FormKey, FxHashSet<FormKey>> = FxHashMap::default();

    for (dependent_fk, record) in lvln_records {
        if !creature_lvlns.contains(dependent_fk) {
            continue;
        }
        for reference in lvln_entry_refs(record, target_masters, target_plugin_name, interner) {
            if creature_lvlns.contains(&reference) {
                reverse_dependencies
                    .entry(reference)
                    .or_default()
                    .insert(*dependent_fk);
                continue;
            }
            let Some(template_lvlns) = npc_template_lvlns.get(&reference) else {
                continue;
            };
            for template_fk in template_lvlns {
                if creature_lvlns.contains(template_fk) {
                    reverse_dependencies
                        .entry(*template_fk)
                        .or_default()
                        .insert(*dependent_fk);
                }
            }
        }
    }

    reverse_dependencies
}

fn dependents_of_changed(
    changed_fks: &[FormKey],
    reverse_dependencies: &FxHashMap<FormKey, FxHashSet<FormKey>>,
) -> Vec<FormKey> {
    let mut pending = FxHashSet::default();
    for fk in changed_fks {
        if let Some(dependents) = reverse_dependencies.get(fk) {
            pending.extend(dependents.iter().copied());
        }
    }
    pending.into_iter().collect()
}

fn npc_is_direct_creature(
    npc: &Record,
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> bool {
    if record_has_actor_type_creature(npc) {
        return true;
    }
    if record_has_actor_type_npc(npc) {
        return false;
    }
    let Some(race_fk) = npc_race_form_key(npc) else {
        return false;
    };
    session
        .record_decoded(&race_fk, target_schema, interner)
        .map(|race| record_has_actor_type_creature(&race))
        .unwrap_or(false)
}

fn infer_creature_lvlns(
    lvln_records: &FxHashMap<FormKey, Record>,
    npc_template_lvlns: &FxHashMap<FormKey, Vec<FormKey>>,
    creature_npcs: &mut FxHashSet<FormKey>,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> FxHashSet<FormKey> {
    let mut creature_lvlns: FxHashSet<FormKey> = FxHashSet::default();

    loop {
        let mut changed = false;

        for (npc_fk, refs) in npc_template_lvlns {
            if creature_npcs.contains(npc_fk) {
                continue;
            }
            if refs.iter().any(|fk| creature_lvlns.contains(fk)) {
                changed |= creature_npcs.insert(*npc_fk);
            }
        }

        for (lvln_fk, record) in lvln_records {
            if creature_lvlns.contains(lvln_fk) {
                continue;
            }
            let entries = lvln_entry_refs(record, target_masters, target_plugin_name, interner);
            if entries
                .iter()
                .any(|fk| creature_npcs.contains(fk) || creature_lvlns.contains(fk))
            {
                changed |= creature_lvlns.insert(*lvln_fk);
            }
        }

        if !changed {
            break;
        }
    }

    creature_lvlns
}

fn normalize_lvln_record(
    record: &mut Record,
    lvln_records: &FxHashMap<FormKey, Record>,
    npc_template_lvlns: &FxHashMap<FormKey, Vec<FormKey>>,
    creature_lvlns: &FxHashSet<FormKey>,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> bool {
    let Ok(lvlo_sig) = SubrecordSig::from_str("LVLO") else {
        return false;
    };

    let mut changed = false;
    let mut retained = smallvec::SmallVec::with_capacity(record.fields.len());
    let mut seen_replacements: FxHashSet<(u16, FormKey, u16)> = FxHashSet::default();

    for entry in record.fields.drain(..) {
        if entry.sig != lvlo_sig {
            retained.push(entry);
            continue;
        }

        let Some(reference) =
            lvlo_reference(&entry.value, target_masters, target_plugin_name, interner)
        else {
            retained.push(entry);
            continue;
        };

        let replacement_lvlns = if creature_lvlns.contains(&reference) {
            vec![reference]
        } else {
            npc_template_lvlns
                .get(&reference)
                .map(|refs| {
                    refs.iter()
                        .copied()
                        .filter(|fk| creature_lvlns.contains(fk))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };

        if replacement_lvlns.is_empty() {
            retained.push(entry);
            continue;
        }

        let parent_level = lvlo_level(&entry.value, interner).unwrap_or(1);
        let parent_count = lvlo_count(&entry.value, interner).unwrap_or(1);
        let mut replacement_entries = Vec::new();
        for replacement_fk in replacement_lvlns {
            if replacement_fk == record.form_key {
                continue;
            }
            let Some(replacement_lvln) = lvln_records.get(&replacement_fk) else {
                continue;
            };
            replacement_entries.extend(clone_lvlo_entries_from(
                replacement_lvln,
                parent_level,
                parent_count,
                target_masters,
                target_plugin_name,
                interner,
                &mut seen_replacements,
            ));
        }

        if replacement_entries.is_empty() {
            retained.push(entry);
            continue;
        }

        retained.extend(replacement_entries);
        changed = true;
    }

    if changed {
        cap_lvlo_entries(&mut retained);
        sync_llct_count(&mut retained);
    }
    record.fields = retained;
    changed
}

fn clone_lvlo_entries_from(
    lvln: &Record,
    parent_level: u16,
    parent_count: u16,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
    seen: &mut FxHashSet<(u16, FormKey, u16)>,
) -> Vec<FieldEntry> {
    let Ok(lvlo_sig) = SubrecordSig::from_str("LVLO") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in &lvln.fields {
        if entry.sig != lvlo_sig {
            continue;
        }
        let Some(reference) =
            lvlo_reference(&entry.value, target_masters, target_plugin_name, interner)
        else {
            continue;
        };
        let level = parent_level.max(lvlo_level(&entry.value, interner).unwrap_or(1));
        let count = parent_count
            .saturating_mul(lvlo_count(&entry.value, interner).unwrap_or(1))
            .max(1);
        if !seen.insert((level, reference, count)) {
            continue;
        }
        let mut cloned = entry.clone();
        set_lvlo_level_and_count(&mut cloned.value, level, count, interner);
        out.push(cloned);
    }
    out
}

fn npc_template_lvln_refs(
    npc: &Record,
    lvln_fks: &FxHashSet<FormKey>,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<FormKey> {
    let tplt_sig = SubrecordSig::from_str("TPLT").ok();
    let tpta_sig = SubrecordSig::from_str("TPTA").ok();
    let mut refs = Vec::new();

    for entry in &npc.fields {
        if Some(entry.sig) == tplt_sig {
            if let Some(fk) =
                form_key_value(&entry.value, target_masters, target_plugin_name, interner)
            {
                push_unique_lvln(&mut refs, fk, lvln_fks);
            }
            continue;
        }
        if Some(entry.sig) != tpta_sig {
            continue;
        }
        collect_tpta_lvln_refs(
            &entry.value,
            lvln_fks,
            target_masters,
            target_plugin_name,
            interner,
            &mut refs,
        );
    }

    refs
}

fn collect_tpta_lvln_refs(
    value: &FieldValue,
    lvln_fks: &FxHashSet<FormKey>,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
    out: &mut Vec<FormKey>,
) {
    match value {
        FieldValue::Bytes(bytes) => {
            for chunk in bytes.chunks_exact(TPTA_SLOT_WIDTH).take(TPTA_SLOT_COUNT) {
                let raw = u32::from_le_bytes(chunk.try_into().unwrap());
                if let Some(fk) =
                    resolve_raw_form_id(raw, target_masters, target_plugin_name, interner)
                {
                    push_unique_lvln(out, fk, lvln_fks);
                }
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                if let Some(fk) =
                    form_key_value(value, target_masters, target_plugin_name, interner)
                {
                    push_unique_lvln(out, fk, lvln_fks);
                }
            }
        }
        FieldValue::List(items) => {
            for value in items {
                if let Some(fk) =
                    form_key_value(value, target_masters, target_plugin_name, interner)
                {
                    push_unique_lvln(out, fk, lvln_fks);
                }
            }
        }
        _ => {}
    }
}

fn push_unique_lvln(out: &mut Vec<FormKey>, fk: FormKey, lvln_fks: &FxHashSet<FormKey>) {
    if lvln_fks.contains(&fk) && !out.contains(&fk) {
        out.push(fk);
    }
}

fn lvln_entry_refs(
    lvln: &Record,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<FormKey> {
    let Ok(lvlo_sig) = SubrecordSig::from_str("LVLO") else {
        return Vec::new();
    };
    lvln.fields
        .iter()
        .filter(|entry| entry.sig == lvlo_sig)
        .filter_map(|entry| {
            lvlo_reference(&entry.value, target_masters, target_plugin_name, interner)
        })
        .collect()
}

fn lvlo_reference(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= LVLO_REFERENCE_OFFSET + 4 => {
            let raw = u32::from_le_bytes(
                bytes[LVLO_REFERENCE_OFFSET..LVLO_REFERENCE_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            resolve_raw_form_id(raw, target_masters, target_plugin_name, interner)
        }
        FieldValue::Struct(fields) => {
            for wanted in ["npc", "NPC", "Reference", "reference"] {
                if let Some(value) = named_struct_value(fields, wanted, interner) {
                    if let Some(fk) =
                        form_key_value(value, target_masters, target_plugin_name, interner)
                    {
                        return Some(fk);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn form_key_value(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
            resolve_raw_form_id(*raw as u32, target_masters, target_plugin_name, interner)
        }
        FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => {
            resolve_raw_form_id(*raw as u32, target_masters, target_plugin_name, interner)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            resolve_raw_form_id(raw, target_masters, target_plugin_name, interner)
        }
        _ => None,
    }
}

fn resolve_raw_form_id(
    raw: u32,
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let load_index = (raw >> 24) as usize;
    let own_index = target_masters.len();
    let plugin_name = if load_index < own_index {
        target_masters[load_index].as_str()
    } else {
        target_plugin_name
    };
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin_name),
    })
}

fn lvlo_level(value: &FieldValue, interner: &StringInterner) -> Option<u16> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes(bytes[0..2].try_into().unwrap()))
        }
        FieldValue::Struct(fields) => {
            for wanted in ["level", "Level"] {
                if let Some(value) = named_struct_value(fields, wanted, interner) {
                    return field_value_to_u16(value);
                }
            }
            None
        }
        _ => None,
    }
}

fn lvlo_count(value: &FieldValue, interner: &StringInterner) -> Option<u16> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= LVLO_MIN_LEN => Some(u16::from_le_bytes(
            bytes[LVLO_COUNT_OFFSET..LVLO_COUNT_OFFSET + 2]
                .try_into()
                .unwrap(),
        )),
        FieldValue::Struct(fields) => {
            for wanted in ["count", "Count"] {
                if let Some(value) = named_struct_value(fields, wanted, interner) {
                    return field_value_to_u16(value);
                }
            }
            None
        }
        _ => None,
    }
}

fn set_lvlo_level_and_count(
    value: &mut FieldValue,
    level: u16,
    count: u16,
    interner: &StringInterner,
) {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= LVLO_MIN_LEN => {
            bytes[0..2].copy_from_slice(&level.to_le_bytes());
            bytes[LVLO_COUNT_OFFSET..LVLO_COUNT_OFFSET + 2].copy_from_slice(&count.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            set_struct_u16_field(fields, &["level", "Level"], level, interner);
            set_struct_u16_field(fields, &["count", "Count"], count, interner);
        }
        _ => {}
    }
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
            Some(u16::from_le_bytes(bytes[0..2].try_into().unwrap()))
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

fn set_struct_u16_field(
    fields: &mut Vec<(crate::sym::Sym, FieldValue)>,
    wanted: &[&str],
    value: u16,
    interner: &StringInterner,
) {
    for (name, field_value) in fields.iter_mut() {
        let Some(actual) = interner.resolve(*name) else {
            continue;
        };
        if wanted
            .iter()
            .any(|wanted| actual.eq_ignore_ascii_case(wanted))
        {
            match field_value {
                FieldValue::Uint(existing) => *existing = u64::from(value),
                FieldValue::Int(existing) => *existing = i64::from(value),
                FieldValue::Float(existing) => *existing = f32::from(value),
                FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                    bytes[0..2].copy_from_slice(&value.to_le_bytes());
                }
                other => *other = FieldValue::Uint(u64::from(value)),
            }
            return;
        }
    }
}

fn sync_llct_count(fields: &mut smallvec::SmallVec<[FieldEntry; 8]>) {
    let count = fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "LVLO")
        .count()
        .min(MAX_LVLO_ENTRIES) as u64;
    let Ok(llct_sig) = SubrecordSig::from_str("LLCT") else {
        return;
    };
    if let Some(entry) = fields.iter_mut().find(|entry| entry.sig == llct_sig) {
        entry.value = FieldValue::Uint(count);
    } else {
        fields.insert(
            0,
            FieldEntry {
                sig: llct_sig,
                value: FieldValue::Uint(count),
            },
        );
    }
}

fn cap_lvlo_entries(fields: &mut smallvec::SmallVec<[FieldEntry; 8]>) {
    let mut seen = 0usize;
    fields.retain(|entry| {
        if entry.sig.as_str() != "LVLO" {
            return true;
        }
        seen += 1;
        seen <= MAX_LVLO_ENTRIES
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::RecordFlags;

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn record(sig_str: &str, local: u32, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig_str).unwrap(),
            form_key: fk(local, "Out.esm", interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push_lvlo(record: &mut Record, level: u16, reference: u32, count: u16) {
        let mut bytes = smallvec::SmallVec::<[u8; 32]>::new();
        bytes.extend_from_slice(&level.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&reference.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Bytes(bytes),
        });
    }

    fn push_lvlo_struct(
        record: &mut Record,
        level: u16,
        reference: FormKey,
        count: u16,
        interner: &StringInterner,
    ) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Struct(vec![
                (interner.intern("Level"), FieldValue::Uint(u64::from(level))),
                (interner.intern("NPC"), FieldValue::FormKey(reference)),
                (interner.intern("Count"), FieldValue::Uint(u64::from(count))),
            ]),
        });
    }

    fn push_llct(record: &mut Record, count: u64) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LLCT").unwrap(),
            value: FieldValue::Uint(count),
        });
    }

    fn converge_with_full_scans(
        lvln_records: &mut FxHashMap<FormKey, Record>,
        npc_template_lvlns: &FxHashMap<FormKey, Vec<FormKey>>,
        creature_lvlns: &FxHashSet<FormKey>,
        interner: &StringInterner,
    ) -> usize {
        let mut records_changed = 0usize;
        loop {
            let candidates = lvln_records
                .keys()
                .copied()
                .filter(|fk| creature_lvlns.contains(fk))
                .collect::<Vec<_>>();
            let changed_records = normalize_lvln_candidates(
                candidates.as_slice(),
                lvln_records,
                npc_template_lvlns,
                creature_lvlns,
                &[],
                "Out.esm",
                interner,
            );
            if changed_records.is_empty() {
                return records_changed;
            }
            records_changed += changed_records.len();
            for record in changed_records {
                lvln_records.insert(record.form_key, record);
            }
        }
    }

    fn converge_with_worklist(
        lvln_records: &mut FxHashMap<FormKey, Record>,
        npc_template_lvlns: &FxHashMap<FormKey, Vec<FormKey>>,
        creature_lvlns: &FxHashSet<FormKey>,
        interner: &StringInterner,
    ) -> Result<usize, FixupError> {
        converge_lvln_worklist(
            "normalize_creature_lvln_template_chains",
            lvln_records,
            npc_template_lvlns,
            creature_lvlns,
            &[],
            "Out.esm",
            interner,
            |_changed_fks, changed_records, lvln_records| {
                let records_changed = changed_records.len();
                for record in changed_records {
                    lvln_records.insert(record.form_key, record);
                }
                Ok(records_changed)
            },
        )
    }

    #[test]
    fn flattens_creature_lvln_entry_that_points_to_lvln_template_npc() {
        let mut interner = StringInterner::new();
        let parent_fk = fk(0x0100, "Out.esm", &interner);
        let child_fk = fk(0x0200, "Out.esm", &interner);
        let bad_npc_fk = fk(0x0300, "Out.esm", &interner);
        let good_npc_fk = fk(0x0400, "Out.esm", &interner);

        let mut parent = record("LVLN", 0x0100, &interner);
        push_llct(&mut parent, 2);
        push_lvlo(&mut parent, 1, 0x0100_0300, 1);
        push_lvlo(&mut parent, 5, 0x0100_0400, 1);

        let mut child = record("LVLN", 0x0200, &interner);
        push_llct(&mut child, 2);
        push_lvlo(&mut child, 10, 0x0100_0500, 1);
        push_lvlo(&mut child, 20, 0x0100_0600, 2);

        let mut lvln_records = FxHashMap::default();
        lvln_records.insert(parent_fk, parent.clone());
        lvln_records.insert(child_fk, child);
        let mut npc_template_lvlns = FxHashMap::default();
        npc_template_lvlns.insert(bad_npc_fk, vec![child_fk]);
        let creature_lvlns = FxHashSet::from_iter([parent_fk, child_fk]);

        let changed = normalize_lvln_record(
            &mut parent,
            &lvln_records,
            &npc_template_lvlns,
            &creature_lvlns,
            &[],
            "Out.esm",
            &interner,
        );

        assert!(changed);
        let refs = lvln_entry_refs(&parent, &[], "Out.esm", &interner);
        assert_eq!(
            refs,
            vec![
                fk(0x0500, "Out.esm", &interner),
                fk(0x0600, "Out.esm", &interner),
                good_npc_fk
            ]
        );
        let entries: Vec<_> = parent
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "LVLO")
            .collect();
        assert_eq!(lvlo_level(&entries[0].value, &interner), Some(10));
        assert_eq!(lvlo_count(&entries[1].value, &interner), Some(2));
        let llct = parent
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LLCT")
            .map(|entry| &entry.value);
        assert_eq!(llct, Some(&FieldValue::Uint(3)));
    }

    #[test]
    fn keeps_template_npc_entry_when_template_is_not_lvln() {
        let mut interner = StringInterner::new();
        let parent_fk = fk(0x0100, "Out.esm", &interner);
        let template_npc_fk = fk(0x0300, "Out.esm", &interner);
        let mut parent = record("LVLN", 0x0100, &interner);
        push_llct(&mut parent, 1);
        push_lvlo(&mut parent, 1, 0x0100_0300, 1);

        let mut lvln_records = FxHashMap::default();
        lvln_records.insert(parent_fk, parent.clone());
        let npc_template_lvlns = FxHashMap::default();
        let creature_lvlns = FxHashSet::from_iter([parent_fk]);

        let changed = normalize_lvln_record(
            &mut parent,
            &lvln_records,
            &npc_template_lvlns,
            &creature_lvlns,
            &[],
            "Out.esm",
            &interner,
        );

        assert!(!changed);
        assert_eq!(
            lvln_entry_refs(&parent, &[], "Out.esm", &interner),
            vec![template_npc_fk]
        );
    }

    #[test]
    fn caps_flattened_lvlo_entries_to_llct_capacity() {
        let mut interner = StringInterner::new();
        let parent_fk = fk(0x0100, "Out.esm", &interner);
        let child_fk = fk(0x0200, "Out.esm", &interner);
        let bad_npc_fk = fk(0x0300, "Out.esm", &interner);

        let mut parent = record("LVLN", 0x0100, &interner);
        push_llct(&mut parent, 1);
        push_lvlo(&mut parent, 1, 0x0100_0300, 1);

        let mut child = record("LVLN", 0x0200, &interner);
        push_llct(&mut child, 260);
        for i in 0..260 {
            push_lvlo(&mut child, 1, 0x0100_1000 + i, 1);
        }

        let mut lvln_records = FxHashMap::default();
        lvln_records.insert(parent_fk, parent.clone());
        lvln_records.insert(child_fk, child);
        let mut npc_template_lvlns = FxHashMap::default();
        npc_template_lvlns.insert(bad_npc_fk, vec![child_fk]);
        let creature_lvlns = FxHashSet::from_iter([parent_fk, child_fk]);

        let changed = normalize_lvln_record(
            &mut parent,
            &lvln_records,
            &npc_template_lvlns,
            &creature_lvlns,
            &[],
            "Out.esm",
            &interner,
        );

        assert!(changed);
        let lvlo_count = parent
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "LVLO")
            .count();
        assert_eq!(lvlo_count, MAX_LVLO_ENTRIES);
        let llct = parent
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LLCT")
            .map(|entry| &entry.value);
        assert_eq!(llct, Some(&FieldValue::Uint(MAX_LVLO_ENTRIES as u64)));
    }

    #[test]
    fn flattens_struct_decoded_lvln_and_preserves_uint_level_count() {
        let mut interner = StringInterner::new();
        let parent_fk = fk(0x0100, "Out.esm", &interner);
        let child_fk = fk(0x0200, "Out.esm", &interner);
        let bad_npc_fk = fk(0x0300, "Out.esm", &interner);
        let good_npc_fk = fk(0x0400, "Out.esm", &interner);

        let mut parent = record("LVLN", 0x0100, &interner);
        push_llct(&mut parent, 1);
        push_lvlo_struct(&mut parent, 5, bad_npc_fk, 2, &interner);

        let mut child = record("LVLN", 0x0200, &interner);
        push_llct(&mut child, 1);
        push_lvlo_struct(&mut child, 10, good_npc_fk, 3, &interner);

        let mut lvln_records = FxHashMap::default();
        lvln_records.insert(parent_fk, parent.clone());
        lvln_records.insert(child_fk, child);
        let mut npc_template_lvlns = FxHashMap::default();
        npc_template_lvlns.insert(bad_npc_fk, vec![child_fk]);
        let creature_lvlns = FxHashSet::from_iter([parent_fk, child_fk]);

        let changed = normalize_lvln_record(
            &mut parent,
            &lvln_records,
            &npc_template_lvlns,
            &creature_lvlns,
            &[],
            "Out.esm",
            &interner,
        );

        assert!(changed);
        assert_eq!(
            lvln_entry_refs(&parent, &[], "Out.esm", &interner),
            vec![good_npc_fk]
        );
        let lvlo = parent
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("flattened LVLO");
        let FieldValue::Struct(fields) = &lvlo.value else {
            panic!("expected structured LVLO");
        };
        assert_eq!(
            named_struct_value(fields, "Level", &interner),
            Some(&FieldValue::Uint(10))
        );
        assert_eq!(
            named_struct_value(fields, "Count", &interner),
            Some(&FieldValue::Uint(6))
        );
    }

    #[test]
    fn worklist_matches_full_scans_for_three_level_transitive_chain() {
        let interner = StringInterner::new();
        let top_fk = fk(0x0100, "Out.esm", &interner);
        let middle_fk = fk(0x0200, "Out.esm", &interner);
        let leaf_fk = fk(0x0300, "Out.esm", &interner);
        let concrete_npc_fk = fk(0x0400, "Out.esm", &interner);

        let mut top = record("LVLN", top_fk.local, &interner);
        push_llct(&mut top, 1);
        push_lvlo(&mut top, 1, 0x0100_0200, 1);

        let mut middle = record("LVLN", middle_fk.local, &interner);
        push_llct(&mut middle, 1);
        push_lvlo(&mut middle, 1, 0x0100_0300, 1);

        let mut leaf = record("LVLN", leaf_fk.local, &interner);
        push_llct(&mut leaf, 1);
        push_lvlo(&mut leaf, 1, 0x0100_0400, 1);

        let original_records =
            FxHashMap::from_iter([(top_fk, top), (middle_fk, middle), (leaf_fk, leaf)]);
        let npc_template_lvlns = FxHashMap::default();
        let creature_lvlns = FxHashSet::from_iter([top_fk, middle_fk, leaf_fk]);
        let mut full_scan_records = original_records.clone();
        let mut worklist_records = original_records;

        let full_scan_changes = converge_with_full_scans(
            &mut full_scan_records,
            &npc_template_lvlns,
            &creature_lvlns,
            &interner,
        );
        let worklist_changes = converge_with_worklist(
            &mut worklist_records,
            &npc_template_lvlns,
            &creature_lvlns,
            &interner,
        )
        .unwrap();

        assert_eq!(worklist_changes, full_scan_changes);
        assert_eq!(worklist_changes, 3);
        for fk in [top_fk, middle_fk, leaf_fk] {
            assert_eq!(worklist_records[&fk].fields, full_scan_records[&fk].fields);
            assert_eq!(
                lvln_entry_refs(&worklist_records[&fk], &[], "Out.esm", &interner),
                vec![concrete_npc_fk]
            );
        }
    }

    #[test]
    fn worklist_returns_convergence_failure_for_three_node_cycle() {
        let interner = StringInterner::new();
        let a_fk = fk(0x0100, "Out.esm", &interner);
        let b_fk = fk(0x0200, "Out.esm", &interner);
        let c_fk = fk(0x0300, "Out.esm", &interner);

        let mut a = record("LVLN", a_fk.local, &interner);
        push_lvlo(&mut a, 1, 0x0100_0200, 1);
        let mut b = record("LVLN", b_fk.local, &interner);
        push_lvlo(&mut b, 1, 0x0100_0300, 1);
        let mut c = record("LVLN", c_fk.local, &interner);
        push_lvlo(&mut c, 1, 0x0100_0100, 1);

        let mut lvln_records = FxHashMap::from_iter([(a_fk, a), (b_fk, b), (c_fk, c)]);
        let npc_template_lvlns = FxHashMap::default();
        let creature_lvlns = FxHashSet::from_iter([a_fk, b_fk, c_fk]);
        let mut applied_waves = 0usize;

        let result = converge_lvln_worklist(
            "normalize_creature_lvln_template_chains",
            &mut lvln_records,
            &npc_template_lvlns,
            &creature_lvlns,
            &[],
            "Out.esm",
            &interner,
            |_changed_fks, changed_records, lvln_records| {
                applied_waves += 1;
                let records_changed = changed_records.len();
                for record in changed_records {
                    lvln_records.insert(record.form_key, record);
                }
                Ok(records_changed)
            },
        );

        assert!(matches!(
            result,
            Err(FixupError::ConvergenceFailure(
                "normalize_creature_lvln_template_chains"
            ))
        ));
        assert_eq!(applied_waves, MAX_CONVERGENCE_WAVES + 1);
    }
}
