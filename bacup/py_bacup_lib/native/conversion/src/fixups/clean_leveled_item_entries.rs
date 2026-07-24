//! Fixup: strip invalid entries from LVLI / LVLN records.
//!

//!
//! # What this does
//! After `_fix_invalid_target_formkeys` and the translation sweep, some LVLI
//! (LeveledItem) and LVLN (LeveledNPC) records may still contain entries whose
//! leveled-entry FormKey is invalid:
//! - Null FormKey (`local == 0`).
//! - Direct self-reference to the owning leveled list.
//! - Pointer into the target master ESM or generated plugin for a record that
//!   doesn't exist there.
//! - Indirect generated-plugin cycles between leveled lists of the same kind.
//!
//! This fixup scans every LVLI / LVLN record in the target plugin, drops bad
//! entries, and writes the cleaned record back.
//!
//! # Entry structure
//! LVLI/LVLN entries can appear either as decoded structs with a named
//! `Reference`, `item`, or `npc` field or as raw `LVLO` bytes.  The raw FO4
//! LVLO payload stores its reference FormID at byte offset 4.
//!
//! # Master-validity check
//! The Python drops entries whose leveled-entry reference points to a target
//! master ESM at a FormKey that doesn't exist in that master.  The Rust path also
//! drops entries that point into the generated target plugin at a missing local
//! FormKey.  Both checks use `record_exists_in_handle` (an O(1) index lookup, no
//! decode).
//!
//! # Design note
//! `apply_to_record` accepts a `is_invalid_ref: &dyn Fn(&FormKey) -> bool`
//! closure so that unit tests can inject the validation logic without needing
//! real plugin handles.  The `run()` method constructs a closure that performs
//! the full null + master-existence check against real handles.

use std::collections::{HashMap, HashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};
use crate::source_read::form_key_to_read_str;
use crate::sym::{StringInterner, Sym};

const LVLO_REFERENCE_OFFSET: usize = 4;
const LVLO_MIN_LEN: usize = LVLO_REFERENCE_OFFSET + 4;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct CleanLeveledItemEntriesFixup;

enum LeveledEntryEdit {
    Candidate(Record),
    Warn(String),
}

impl Fixup for CleanLeveledItemEntriesFixup {
    fn name(&self) -> &'static str {
        "clean_leveled_item_entries"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
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
        let source_schema = config
            .source_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing source schema in fixup config".into()))?;
        let target_masters: Vec<(String, u64)> = session
            .target_masters()
            .iter()
            .cloned()
            .map(|name| name.to_ascii_lowercase())
            .zip(config.target_master_handle_ids.iter().copied())
            .collect();
        let target_master_names = session.target_masters().to_vec();
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let source_plugin_name = session
            .source_slot_opt()
            .map(|slot| slot.parsed.plugin_name.clone())
            .unwrap_or_else(|| target_plugin_name.clone());
        let source_master_names = session
            .source_slot_opt()
            .map(|slot| slot.parsed.header.masters.clone())
            .unwrap_or_default();
        // Keep the old "Reference" symbol for existing call sites/tests; decoded
        // schemas may also expose the same LVLO slot as "item" or "npc".
        let interner = mapper.interner;
        let reference_sym = interner.intern("Reference");
        let mut report = FixupReport::empty();
        let mut invalid_ref_cache: HashMap<FormKey, bool> = HashMap::new();

        for sig_str in &["LVLI", "LVLN"] {
            let sig =
                SigCode::from_str(sig_str).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            let mut sig_warnings = Vec::new();
            let sig_report = session.map_apply_by_sig(
                sig,
                mapper,
                |view, _snapshot, fk| match view.record_decoded(fk, target_schema, interner) {
                    Ok(record) => record_has_candidate_entries(
                        &record,
                        reference_sym,
                        &target_master_names,
                        &target_plugin_name,
                        &target_masters,
                        interner,
                    )
                    .then_some(LeveledEntryEdit::Candidate(record)),
                    Err(err) => Some(LeveledEntryEdit::Warn(format!("lvl_clean_read_err:{err}"))),
                },
                |session, mapper, _fk, edit| match edit {
                    LeveledEntryEdit::Candidate(mut record) => {
                        let source_fk = FormKey {
                            local: record.form_key.local,
                            plugin: mapper.interner.intern(&source_plugin_name),
                        };
                        let mut changed = match source_chance_none_from_lvlg(
                            session,
                            source_schema,
                            &source_fk,
                            &source_master_names,
                            &source_plugin_name,
                            mapper.interner,
                        ) {
                            Ok(chance_none) => {
                                apply_source_chance_none_global(&mut record, chance_none)
                            }
                            Err(message) => {
                                sig_warnings.push(mapper.interner.intern(&message));
                                false
                            }
                        };
                        changed |= ensure_leveled_list_defaults(&mut record);
                        let target_handle_id = session.target_id();
                        let mut invalid_ref = |candidate_fk: &FormKey| {
                            if let Some(is_invalid) = invalid_ref_cache.get(candidate_fk) {
                                return *is_invalid;
                            }
                            let is_invalid = is_invalid_ref(
                                session,
                                candidate_fk,
                                mapper.interner,
                                &target_masters,
                                target_handle_id,
                                &target_plugin_name,
                            );
                            invalid_ref_cache.insert(*candidate_fk, is_invalid);
                            is_invalid
                        };
                        if apply_to_record(
                            &mut record,
                            reference_sym,
                            &target_master_names,
                            &target_plugin_name,
                            mapper.interner,
                            &mut invalid_ref,
                        ) {
                            changed = true;
                        }
                        if !changed {
                            Ok(EditOutcome::NoOp)
                        } else {
                            session
                                .replace_record_contents(record, target_schema, mapper.interner)
                                .map_err(|e| FixupError::HandleError(e.to_string()))?;
                            Ok(EditOutcome::Changed)
                        }
                    }
                    LeveledEntryEdit::Warn(message) => {
                        sig_warnings.push(mapper.interner.intern(&message));
                        Ok(EditOutcome::NoOp)
                    }
                },
            )?;
            report.records_changed += sig_report.records_changed;
            report.records_dropped += sig_report.records_dropped;
            report.records_added += sig_report.records_added;
            report.warnings.extend(sig_report.warnings);
            report.warnings.extend(sig_warnings);

            let cycle_report = drop_indirect_leveled_cycles(
                session,
                sig,
                target_schema,
                reference_sym,
                &target_master_names,
                &target_plugin_name,
                mapper.interner,
            )?;
            report.records_changed += cycle_report.records_changed;
            report.records_dropped += cycle_report.records_dropped;
            report.records_added += cycle_report.records_added;
            report.warnings.extend(cycle_report.warnings);
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Remove LVLO/LVLE subrecord entries whose leveled-entry FormKey is invalid.
///
/// Returns `true` when at least one entry was dropped.
///
/// # Entry identification
/// A subrecord is treated as a leveled-entry if its signature is `LVLO` or
/// `LVLE`.  Struct entries use the named `Reference`, `item`, or `npc` field.
/// Raw byte entries use the FO4 LVLO reference FormID at byte offset 4.
///
/// # Drop conditions (matching Python `_clean_leveled_item_entries` exactly)
/// An entry is dropped when `is_invalid_ref` returns `true` for its
/// leveled-entry FormKey.  The predicate should implement:
/// 1. null FormKey (`local == 0`) → invalid.
/// 2. non-null FK pointing at a target master or the generated plugin at a FK
///    that doesn't exist there → invalid.
///
/// # Parameters
/// - `reference_sym` — the interned `Sym` for `"Reference"` compatibility.
/// - `is_invalid_ref` — predicate that returns `true` when the entry FK
///   should cause the entry to be dropped.
pub fn apply_to_record<F>(
    record: &mut Record,
    reference_sym: Sym,
    target_master_names: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
    is_invalid_ref: &mut F,
) -> bool
where
    F: FnMut(&FormKey) -> bool,
{
    let mut any_dropped = false;
    let mut new_fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

    for mut entry in record.fields.drain(..) {
        // LLKC Filter-Keyword-Chances: a single `array_struct:I,I` subrecord whose
        // value is a row-list of {keyword→KYWD, chance}. FO76-only filter keywords
        // (Fallout4.esm:0xxxxxx with no FO4 home) leave the row's keyword FK
        // dangling — CK then empties the list and the parent ref collapses. Drop the
        // dangling rows; if the list empties, drop the whole subrecord. There is no
        // separate LLKC count subrecord (LLCT counts LVLO only), so the row-list
        // length IS the count — dropping a row is inherently lockstep.
        if entry.sig.as_str() == "LLKC" {
            match drop_dangling_llkc_rows(&mut entry.value, interner, is_invalid_ref) {
                LlkcOutcome::Unchanged => new_fields.push(entry),
                LlkcOutcome::RowsDropped => {
                    any_dropped = true;
                    new_fields.push(entry);
                }
                LlkcOutcome::Empty => {
                    any_dropped = true;
                    // drop the now-empty subrecord
                }
            }
            continue;
        }

        let should_drop = is_leveled_entry_sig(&entry) && {
            match extract_entry_reference(
                &entry.value,
                reference_sym,
                target_master_names,
                target_plugin_name,
                interner,
            ) {
                Some(fk) => {
                    !is_known_valid_leveled_ref(&fk, interner)
                        && (fk == record.form_key || is_invalid_ref(&fk))
                }
                // No leveled-entry reference field -> keep.
                None => false,
            }
        };

        if should_drop {
            any_dropped = true;
        } else {
            new_fields.push(entry);
        }
    }

    if any_dropped {
        sync_llct_count(&mut new_fields);
    }
    record.fields = new_fields;
    any_dropped
}

enum LlkcOutcome {
    /// No dangling rows — subrecord untouched.
    Unchanged,
    /// At least one dangling row removed; the list is non-empty.
    RowsDropped,
    /// All rows dangled — the subrecord should be dropped entirely.
    Empty,
}

/// Drop every LLKC row whose `keyword` FormKey is invalid (via `is_invalid_ref`,
/// the same master-existence predicate the LVLO drop uses — so this is inert in
/// master-less runs and only fires once FO4 masters are loaded). The row's keyword
/// field is identified by id `filter_keyword_chances_keyword`.
fn drop_dangling_llkc_rows<F>(
    value: &mut FieldValue,
    interner: &StringInterner,
    is_invalid_ref: &mut F,
) -> LlkcOutcome
where
    F: FnMut(&FormKey) -> bool,
{
    let FieldValue::List(rows) = value else {
        return LlkcOutcome::Unchanged;
    };
    let before = rows.len();
    rows.retain(|row| {
        let FieldValue::Struct(fields) = row else {
            return true;
        };
        for (sym, v) in fields {
            let Some(name) = interner.resolve(*sym) else {
                continue;
            };
            if name.ends_with("keyword") {
                if let FieldValue::FormKey(fk) = v {
                    return !is_invalid_ref(fk);
                }
            }
        }
        true
    });
    if rows.len() == before {
        LlkcOutcome::Unchanged
    } else if rows.is_empty() {
        LlkcOutcome::Empty
    } else {
        LlkcOutcome::RowsDropped
    }
}

fn drop_indirect_leveled_cycles(
    session: &mut PluginSession,
    sig: SigCode,
    target_schema: &crate::schema::AuthoringSchema,
    reference_sym: Sym,
    target_master_names: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Result<FixupReport, FixupError> {
    let target_plugin_sym = interner.intern(target_plugin_name);
    let mut form_keys = session
        .form_keys_of_sig(sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    form_keys.retain(|fk| fk.plugin == target_plugin_sym);
    form_keys.sort_by_key(|fk| fk.local);
    if form_keys.is_empty() {
        return Ok(FixupReport::empty());
    }

    let output_keys: HashSet<FormKey> = form_keys.iter().copied().collect();
    let mut graph: HashMap<FormKey, Vec<FormKey>> = HashMap::new();
    let mut warnings = Vec::new();

    for fk in &form_keys {
        let record = match session.record_decoded(fk, target_schema, interner) {
            Ok(record) => record,
            Err(err) => {
                warnings.push(interner.intern(&format!("lvl_cycle_read_err:{err}")));
                continue;
            }
        };
        let mut refs = Vec::new();
        for entry in &record.fields {
            if !is_leveled_entry_sig(entry) {
                continue;
            }
            let Some(entry_fk) = extract_entry_reference(
                &entry.value,
                reference_sym,
                target_master_names,
                target_plugin_name,
                interner,
            ) else {
                continue;
            };
            if output_keys.contains(&entry_fk) {
                refs.push(entry_fk);
            }
        }
        if !refs.is_empty() {
            refs.sort_by_key(|fk| fk.local);
            graph.insert(*fk, refs);
        }
    }

    let edges_to_drop = cycle_edges_to_drop(&form_keys, &graph);
    if edges_to_drop.is_empty() {
        let mut report = FixupReport::empty();
        report.warnings = warnings;
        return Ok(report);
    }

    let mut changed = 0;
    for (fk, refs_to_drop) in edges_to_drop {
        let mut record = match session.record_decoded(&fk, target_schema, interner) {
            Ok(record) => record,
            Err(err) => {
                warnings.push(interner.intern(&format!("lvl_cycle_rewrite_read_err:{err}")));
                continue;
            }
        };
        if !drop_entries_referencing(
            &mut record,
            reference_sym,
            target_master_names,
            target_plugin_name,
            interner,
            &refs_to_drop,
        ) {
            continue;
        }
        if session
            .replace_record_contents(record, target_schema, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            changed += 1;
        }
    }

    let mut report = FixupReport::empty();
    report.records_changed = changed;
    report.warnings = warnings;
    Ok(report)
}

fn cycle_edges_to_drop(
    nodes: &[FormKey],
    graph: &HashMap<FormKey, Vec<FormKey>>,
) -> HashMap<FormKey, HashSet<FormKey>> {
    let node_set: HashSet<FormKey> = nodes.iter().copied().collect();
    let mut state: HashMap<FormKey, u8> = HashMap::new();
    let mut edges_to_drop: HashMap<FormKey, HashSet<FormKey>> = HashMap::new();

    for node in nodes {
        if state.get(node).copied().unwrap_or_default() == 0 {
            visit_cycle_edges(*node, &node_set, graph, &mut state, &mut edges_to_drop);
        }
    }

    edges_to_drop
}

fn visit_cycle_edges(
    node: FormKey,
    node_set: &HashSet<FormKey>,
    graph: &HashMap<FormKey, Vec<FormKey>>,
    state: &mut HashMap<FormKey, u8>,
    edges_to_drop: &mut HashMap<FormKey, HashSet<FormKey>>,
) {
    state.insert(node, 1);
    let children = graph.get(&node).cloned().unwrap_or_default();
    for child in children {
        if !node_set.contains(&child) {
            continue;
        }
        match state.get(&child).copied().unwrap_or_default() {
            0 => visit_cycle_edges(child, node_set, graph, state, edges_to_drop),
            1 => {
                edges_to_drop.entry(node).or_default().insert(child);
            }
            _ => {}
        }
    }
    state.insert(node, 2);
}

fn drop_entries_referencing(
    record: &mut Record,
    reference_sym: Sym,
    target_master_names: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
    refs_to_drop: &HashSet<FormKey>,
) -> bool {
    let mut any_dropped = false;
    let mut new_fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

    for entry in record.fields.drain(..) {
        let should_drop = is_leveled_entry_sig(&entry) && {
            match extract_entry_reference(
                &entry.value,
                reference_sym,
                target_master_names,
                target_plugin_name,
                interner,
            ) {
                Some(fk) => refs_to_drop.contains(&fk),
                None => false,
            }
        };

        if should_drop {
            any_dropped = true;
        } else {
            new_fields.push(entry);
        }
    }

    if any_dropped {
        sync_llct_count(&mut new_fields);
    }
    record.fields = new_fields;
    any_dropped
}

/// Returns `true` when the subrecord sig is `LVLO` or `LVLE`.
fn is_leveled_entry_sig(entry: &FieldEntry) -> bool {
    matches!(entry.sig.as_str(), "LVLO" | "LVLE")
}

fn sync_llct_count(fields: &mut smallvec::SmallVec<[FieldEntry; 8]>) {
    let count = fields
        .iter()
        .filter(|entry| is_leveled_entry_sig(entry))
        .count()
        .min(u8::MAX as usize) as u64;
    let Ok(llct_sig) = SubrecordSig::from_str("LLCT") else {
        return;
    };
    if let Some(entry) = fields.iter_mut().find(|entry| entry.sig == llct_sig) {
        entry.value = FieldValue::Uint(count);
    }
}

/// Extract the leveled-list reference FormKey from a decoded or raw entry.
///
/// Raw LVLO payloads store a 32-bit FormID at offset 4. That FormID is resolved
/// against the target plugin's master table so the same invalid-reference
/// predicate can validate both decoded and raw entries.
pub(crate) fn extract_entry_reference(
    value: &FieldValue,
    sym: Sym,
    target_master_names: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::Bytes(data) if data.len() >= LVLO_MIN_LEN => {
            let raw = u32::from_le_bytes([
                data[LVLO_REFERENCE_OFFSET],
                data[LVLO_REFERENCE_OFFSET + 1],
                data[LVLO_REFERENCE_OFFSET + 2],
                data[LVLO_REFERENCE_OFFSET + 3],
            ]);
            Some(resolve_raw_form_id(
                raw,
                target_master_names,
                target_plugin_name,
                interner,
            ))
        }
        FieldValue::Struct(fields) => {
            for (field_sym, field_val) in fields {
                if *field_sym == sym || is_leveled_reference_field(*field_sym, interner) {
                    return if let FieldValue::FormKey(fk) = field_val {
                        Some(*fk)
                    } else {
                        None
                    };
                }
            }
            None
        }
        _ => None,
    }
}

fn is_leveled_reference_field(field_sym: Sym, interner: &StringInterner) -> bool {
    matches!(
        interner.resolve(field_sym),
        Some("Reference")
            | Some("reference")
            | Some("Item")
            | Some("item")
            | Some("NPC")
            | Some("npc")
    )
}

fn resolve_raw_form_id(
    raw: u32,
    target_master_names: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> FormKey {
    let object_id = raw & 0x00FF_FFFF;
    let master_index = ((raw >> 24) & 0xFF) as usize;
    let plugin_name = target_master_names
        .get(master_index)
        .map(String::as_str)
        .unwrap_or(target_plugin_name);
    FormKey {
        local: object_id,
        plugin: interner.intern(plugin_name),
    }
}

fn ensure_leveled_list_defaults(record: &mut Record) -> bool {
    if !matches!(record.sig.as_str(), "LVLI" | "LVLN") {
        return false;
    }

    let mut changed = false;
    if ensure_subrecord_default_before(
        record,
        "LVLD",
        FieldValue::Uint(0),
        &["LVLM", "LVLF", "LVLG", "LLCT", "LVLO", "LVLE", "COED"],
    ) {
        changed = true;
    }
    if ensure_lvlm_default(record) {
        changed = true;
    }
    changed
}

fn source_chance_none_from_lvlg(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    source_fk: &FormKey,
    source_master_names: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Result<Option<u8>, String> {
    let Ok(source_record) = session.source_record_decoded(source_fk, source_schema, interner)
    else {
        return Ok(None);
    };
    let Some(global_fk) = source_lvlg_reference(
        &source_record,
        source_master_names,
        source_plugin_name,
        interner,
    ) else {
        return Ok(None);
    };
    let global_record = session
        .source_record_decoded(&global_fk, source_schema, interner)
        .map_err(|err| format!("lvlg_global_read_err:{err}"))?;
    Ok(global_record_chance_none(&global_record))
}

fn apply_source_chance_none_global(record: &mut Record, chance_none: Option<u8>) -> bool {
    chance_none.is_some_and(|chance_none| set_lvld(record, chance_none))
}

fn set_lvld(record: &mut Record, chance_none: u8) -> bool {
    let Some(lvld_sig) = subrecord_sig("LVLD") else {
        return false;
    };
    let value = FieldValue::Uint(u64::from(chance_none));
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == lvld_sig) {
        if entry.value == value {
            return false;
        }
        entry.value = value;
        return true;
    }

    let before = ["LVLM", "LVLF", "LVLG", "LLCT", "LVLO", "LVLE", "COED"];
    let before_sigs: HashSet<SubrecordSig> = before
        .iter()
        .filter_map(|candidate| subrecord_sig(candidate))
        .collect();
    let index = record
        .fields
        .iter()
        .position(|entry| before_sigs.contains(&entry.sig))
        .unwrap_or(record.fields.len());
    record.fields.insert(
        index,
        FieldEntry {
            sig: lvld_sig,
            value,
        },
    );
    true
}

fn source_lvlg_reference(
    record: &Record,
    source_master_names: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if !matches!(record.sig.as_str(), "LVLI" | "LVLN") {
        return None;
    }
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "LVLG")
        .and_then(|entry| {
            source_form_key_from_value(
                &entry.value,
                source_master_names,
                source_plugin_name,
                interner,
            )
        })
}

fn source_form_key_from_value(
    value: &FieldValue,
    source_master_names: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            Some(resolve_raw_form_id(
                raw,
                source_master_names,
                source_plugin_name,
                interner,
            ))
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| {
            source_form_key_from_value(value, source_master_names, source_plugin_name, interner)
        }),
        _ => None,
    }
}

fn global_record_chance_none(record: &Record) -> Option<u8> {
    if record.sig.as_str() != "GLOB" {
        return None;
    }
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "FLTV")
        .and_then(|entry| chance_none_from_value(&entry.value))
}

fn chance_none_from_value(value: &FieldValue) -> Option<u8> {
    let value = match value {
        FieldValue::Float(value) => *value,
        FieldValue::Uint(value) => *value as f32,
        FieldValue::Int(value) => *value as f32,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
        _ => return None,
    };
    value
        .is_finite()
        .then(|| value.round().clamp(0.0, 100.0) as u8)
}

fn ensure_lvlm_default(record: &mut Record) -> bool {
    let Some(lvlm_sig) = subrecord_sig("LVLM") else {
        return false;
    };
    if record.fields.iter().any(|entry| entry.sig == lvlm_sig) {
        return false;
    }
    if let Some(lvld_sig) = subrecord_sig("LVLD") {
        if let Some(index) = record.fields.iter().position(|entry| entry.sig == lvld_sig) {
            record.fields.insert(
                index + 1,
                FieldEntry {
                    sig: lvlm_sig,
                    value: FieldValue::Uint(0),
                },
            );
            return true;
        }
    }
    ensure_subrecord_default_before(
        record,
        "LVLM",
        FieldValue::Uint(0),
        &["LVLF", "LVLG", "LLCT", "LVLO", "LVLE", "COED"],
    )
}

fn ensure_subrecord_default_before(
    record: &mut Record,
    sig_str: &str,
    value: FieldValue,
    before: &[&str],
) -> bool {
    let Some(sig) = subrecord_sig(sig_str) else {
        return false;
    };
    if record.fields.iter().any(|entry| entry.sig == sig) {
        return false;
    }
    let before_sigs: HashSet<SubrecordSig> = before
        .iter()
        .filter_map(|candidate| subrecord_sig(candidate))
        .collect();
    let index = record
        .fields
        .iter()
        .position(|entry| before_sigs.contains(&entry.sig))
        .unwrap_or(record.fields.len());
    record.fields.insert(index, FieldEntry { sig, value });
    true
}

fn subrecord_sig(sig_str: &str) -> Option<SubrecordSig> {
    SubrecordSig::from_str(sig_str).ok()
}

fn record_has_candidate_entries(
    record: &Record,
    reference_sym: Sym,
    target_master_names: &[String],
    target_plugin_name: &str,
    target_masters: &[(String, u64)],
    interner: &StringInterner,
) -> bool {
    if record_needs_leveled_list_defaults(record) {
        return true;
    }
    record.fields.iter().any(|entry| {
        // LLKC rows carry KYWD formids that can also dangle into a master/output;
        // flag the record so `apply_to_record` gets a chance to prune them.
        if entry.sig.as_str() == "LLKC" {
            return llkc_has_candidate_rows(
                &entry.value,
                target_master_names,
                target_plugin_name,
                target_masters,
                interner,
            );
        }
        if !is_leveled_entry_sig(entry) {
            return false;
        }
        let Some(fk) = extract_entry_reference(
            &entry.value,
            reference_sym,
            target_master_names,
            target_plugin_name,
            interner,
        ) else {
            return false;
        };
        if fk.local == 0 {
            return true;
        }
        let Some(plugin_name) = interner.resolve(fk.plugin) else {
            return true;
        };
        plugin_name.eq_ignore_ascii_case(target_plugin_name)
            || target_masters
                .iter()
                .any(|(master_name, _)| plugin_name.eq_ignore_ascii_case(master_name))
    })
}

/// True when any LLKC row's keyword FormKey points at the output plugin or a
/// target master — i.e. a row whose existence must be verified by `apply_to_record`
/// (mirrors the LVLO candidate gate). FKs into untracked plugins are never dropped.
fn llkc_has_candidate_rows(
    value: &FieldValue,
    _target_master_names: &[String],
    target_plugin_name: &str,
    target_masters: &[(String, u64)],
    interner: &StringInterner,
) -> bool {
    let FieldValue::List(rows) = value else {
        return false;
    };
    rows.iter().any(|row| {
        let FieldValue::Struct(fields) = row else {
            return false;
        };
        fields.iter().any(|(sym, v)| {
            let Some(name) = interner.resolve(*sym) else {
                return false;
            };
            if !name.ends_with("keyword") {
                return false;
            }
            let FieldValue::FormKey(fk) = v else {
                return false;
            };
            if fk.local == 0 {
                return true;
            }
            let Some(plugin_name) = interner.resolve(fk.plugin) else {
                return true;
            };
            plugin_name.eq_ignore_ascii_case(target_plugin_name)
                || target_masters
                    .iter()
                    .any(|(master_name, _)| plugin_name.eq_ignore_ascii_case(master_name))
        })
    })
}

fn record_needs_leveled_list_defaults(record: &Record) -> bool {
    if !matches!(record.sig.as_str(), "LVLI" | "LVLN") {
        return false;
    }
    let Some(lvld_sig) = subrecord_sig("LVLD") else {
        return false;
    };
    let Some(lvlm_sig) = subrecord_sig("LVLM") else {
        return false;
    };
    !record.fields.iter().any(|entry| entry.sig == lvld_sig)
        || !record.fields.iter().any(|entry| entry.sig == lvlm_sig)
}

fn is_invalid_ref(
    session: &mut PluginSession,
    fk: &FormKey,
    interner: &StringInterner,
    target_masters: &[(String, u64)],
    target_handle_id: u64,
    target_plugin_name: &str,
) -> bool {
    if fk.local == 0 {
        return true;
    }
    let fk_str = form_key_to_read_str(fk, interner);
    if fk_str.is_empty() {
        return true;
    }
    if let Some((plugin_part, _)) = fk_str.split_once(':') {
        if plugin_part.eq_ignore_ascii_case(target_plugin_name) {
            return session
                .record_exists_in_handle(target_handle_id, &fk_str)
                .map(|exists| !exists)
                .unwrap_or(true);
        }
        for (master_name, master_id) in target_masters {
            if plugin_part.eq_ignore_ascii_case(master_name) {
                return session
                    .record_exists_in_handle(*master_id, &fk_str)
                    .map(|exists| !exists)
                    .unwrap_or(true);
            }
        }
    }
    false
}

fn is_known_valid_leveled_ref(fk: &FormKey, interner: &StringInterner) -> bool {
    fk.local == 0x00000F
        && interner
            .resolve(fk.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case("Fallout4.esm"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn lvli_sig() -> SigCode {
        SigCode::from_str("LVLI").unwrap()
    }

    fn lvln_sig() -> SigCode {
        SigCode::from_str("LVLN").unwrap()
    }

    fn make_fk(hex: &str, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(&format!("{hex}@{plugin}"), interner).unwrap()
    }

    fn null_fk(interner: &StringInterner) -> FormKey {
        make_fk("000000", "Fallout4.esm", interner)
    }

    fn valid_master_fk(interner: &StringInterner) -> FormKey {
        make_fk("001234", "Fallout4.esm", interner)
    }

    fn non_master_fk(interner: &StringInterner) -> FormKey {
        make_fk("001234", "SomeMod.esp", interner)
    }

    fn missing_output_fk(interner: &StringInterner) -> FormKey {
        make_fk("605FC4", "Mod.esp", interner)
    }

    fn lvlo_named_entry(field_name: &str, fk: FormKey, interner: &StringInterner) -> FieldEntry {
        let ref_sym = interner.intern(field_name);
        FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Struct(vec![(ref_sym, FieldValue::FormKey(fk))]),
        }
    }

    fn lvlo_entry(fk: FormKey, interner: &StringInterner) -> FieldEntry {
        lvlo_named_entry("Reference", fk, interner)
    }

    fn lvlo_raw_entry(raw_reference: u32) -> FieldEntry {
        let mut data = smallvec::smallvec![0u8; 12];
        data[LVLO_REFERENCE_OFFSET..LVLO_REFERENCE_OFFSET + 4]
            .copy_from_slice(&raw_reference.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Bytes(data),
        }
    }

    fn formkey_field(sig: &str, fk: FormKey) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::FormKey(fk),
        }
    }

    fn float_field(sig: &str, value: f32) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Float(value),
        }
    }

    fn make_lvli(fk: FormKey, entries: Vec<FieldEntry>) -> Record {
        Record {
            sig: lvli_sig(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: entries.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn make_lvln(fk: FormKey, entries: Vec<FieldEntry>) -> Record {
        Record {
            sig: lvln_sig(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: entries.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn make_glob(fk: FormKey, value: f32) -> Record {
        Record {
            sig: SigCode::from_str("GLOB").unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: vec![float_field("FLTV", value)].into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn uint_field(sig: &str, value: u64) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Uint(value),
        }
    }

    // Predicate: drop null FKs only (no master validity check).
    fn null_pred() -> Box<dyn FnMut(&FormKey) -> bool> {
        Box::new(|fk: &FormKey| fk.local == 0)
    }

    // Predicate: drop null FKs OR FKs pointing at Fallout4.esm with local > 0x8000
    // (simulates "valid in master" vs "missing from master" without a real handle).
    fn master_pred(interner: &StringInterner) -> Box<dyn FnMut(&FormKey) -> bool> {
        let fo4_sym = interner.intern("Fallout4.esm");
        Box::new(move |fk: &FormKey| {
            if fk.local == 0 {
                return true;
            }
            if fk.plugin == fo4_sym && fk.local > 0x8000 {
                return true; // "missing" in master
            }
            false
        })
    }

    fn output_plugin_pred(interner: &StringInterner) -> Box<dyn FnMut(&FormKey) -> bool> {
        let output_sym = interner.intern("Mod.esp");
        Box::new(move |fk: &FormKey| fk.plugin == output_sym && fk.local == 0x605FC4)
    }

    fn ref_sym(interner: &StringInterner) -> Sym {
        interner.intern("Reference")
    }

    fn target_master_names() -> Vec<String> {
        vec!["Fallout4.esm".to_string()]
    }

    fn apply_for_test<F>(record: &mut Record, interner: &StringInterner, pred: &mut F) -> bool
    where
        F: FnMut(&FormKey) -> bool,
    {
        let masters = target_master_names();
        apply_to_record(
            record,
            ref_sym(interner),
            &masters,
            "Mod.esp",
            interner,
            pred,
        )
    }

    #[test]
    fn source_chance_none_global_sets_lvld_and_keeps_lvlg() {
        let interner = StringInterner::new();
        let record_fk = make_fk("4510AF", "SeventySix.esm", &interner);
        let global_fk = make_fk("4510B1", "SeventySix.esm", &interner);
        let mut record = make_lvln(
            record_fk,
            vec![
                uint_field("LVLD", 0),
                uint_field("LVLM", 0),
                formkey_field("LVLG", global_fk),
                uint_field("LVLF", 8),
            ],
        );

        let changed = apply_source_chance_none_global(&mut record, Some(50));

        assert!(changed);
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "LVLD")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(50))
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "LVLG")
        );
    }

    #[test]
    fn source_chance_none_global_inserts_lvld_before_leveled_fields() {
        let interner = StringInterner::new();
        let record_fk = make_fk("519701", "SeventySix.esm", &interner);
        let global_fk = make_fk("369793", "SeventySix.esm", &interner);
        let mut record = make_lvli(
            record_fk,
            vec![formkey_field("LVLG", global_fk), uint_field("LVLF", 3)],
        );

        let changed = apply_source_chance_none_global(&mut record, Some(25));

        assert!(changed);
        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["LVLD", "LVLG", "LVLF"]);
        assert_eq!(record.fields[0].value, FieldValue::Uint(25));
    }

    #[test]
    fn source_lvlg_reference_and_global_value_decode() {
        let interner = StringInterner::new();
        let record_fk = make_fk("4510AF", "SeventySix.esm", &interner);
        let global_fk = make_fk("4510B1", "SeventySix.esm", &interner);
        let source = make_lvln(record_fk, vec![formkey_field("LVLG", global_fk)]);
        let global = make_glob(global_fk, 50.0);

        assert_eq!(
            source_lvlg_reference(&source, &[], "SeventySix.esm", &interner),
            Some(global_fk)
        );
        assert_eq!(global_record_chance_none(&global), Some(50));
    }

    #[test]
    fn ensure_leveled_list_defaults_inserts_lvld_and_lvlm_before_entries() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let mut record = make_lvln(
            record_fk,
            vec![
                uint_field("LVLF", 0),
                uint_field("LLCT", 1),
                lvlo_entry(valid_master_fk(&mut interner), &mut interner),
            ],
        );

        let changed = ensure_leveled_list_defaults(&mut record);

        assert!(changed);
        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["LVLD", "LVLM", "LVLF", "LLCT", "LVLO"]);
        assert_eq!(record.fields[0].value, FieldValue::Uint(0));
        assert_eq!(record.fields[1].value, FieldValue::Uint(0));
    }

    #[test]
    fn ensure_leveled_list_defaults_preserves_existing_values() {
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let mut record = make_lvln(
            record_fk,
            vec![
                uint_field("LVLD", 42),
                uint_field("LVLM", 3),
                uint_field("LLCT", 0),
            ],
        );

        let changed = ensure_leveled_list_defaults(&mut record);

        assert!(!changed);
        assert_eq!(record.fields[0].value, FieldValue::Uint(42));
        assert_eq!(record.fields[1].value, FieldValue::Uint(3));
    }

    #[test]
    fn record_has_candidate_entries_flags_missing_leveled_defaults() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let record = make_lvln(record_fk, vec![uint_field("LLCT", 0)]);
        let masters = target_master_names();
        let target_masters: Vec<(String, u64)> = Vec::new();

        assert!(record_has_candidate_entries(
            &record,
            ref_sym(&mut interner),
            &masters,
            "Mod.esp",
            &target_masters,
            &interner,
        ));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_no_op_when_all_entries_valid() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let entry = lvlo_entry(valid_master_fk(&mut interner), &mut interner);
        let mut record = make_lvli(record_fk, vec![entry]);

        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(
            !changed,
            "no entries should be dropped when reference is valid"
        );
        assert_eq!(record.fields.len(), 1);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_drops_null_reference() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let null_entry = lvlo_entry(null_fk(&mut interner), &mut interner);
        let mut record = make_lvli(record_fk, vec![null_entry]);

        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "null reference entry should be dropped");
        assert!(record.fields.is_empty(), "the entry should be removed");
    }

    #[test]
    fn apply_to_record_drops_null_item_field() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let null_entry = lvlo_named_entry("item", null_fk(&mut interner), &mut interner);
        let mut record = make_lvli(record_fk, vec![null_entry]);

        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "null item entry should be dropped");
        assert!(record.fields.is_empty(), "the entry should be removed");
    }

    #[test]
    fn apply_to_record_drops_missing_master_npc_field() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let missing_fk = make_fk("009000", "Fallout4.esm", &mut interner);
        let bad_entry = lvlo_named_entry("npc", missing_fk, &mut interner);
        let mut record = make_lvli(record_fk, vec![bad_entry]);

        let pred = master_pred(&mut interner);
        let mut pred = pred;
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "missing-master npc entry should be dropped");
        assert!(record.fields.is_empty());
    }

    #[test]
    fn apply_to_record_drops_missing_output_plugin_entry() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let bad_entry = lvlo_entry(missing_output_fk(&mut interner), &mut interner);
        let mut record = make_lvli(record_fk, vec![bad_entry]);

        let pred = output_plugin_pred(&mut interner);
        let mut pred = pred;
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "missing output-plugin entry should be dropped");
        assert!(record.fields.is_empty());
    }

    #[test]
    fn record_has_candidate_entries_flags_output_plugin_entry() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let entry = lvlo_entry(missing_output_fk(&mut interner), &mut interner);
        let record = make_lvli(record_fk, vec![entry]);
        let masters = target_master_names();
        let target_masters: Vec<(String, u64)> = Vec::new();

        assert!(record_has_candidate_entries(
            &record,
            ref_sym(&mut interner),
            &masters,
            "Mod.esp",
            &target_masters,
            &interner,
        ));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_keeps_valid_drops_null() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let valid_entry = lvlo_entry(valid_master_fk(&mut interner), &mut interner);
        let null_entry = lvlo_entry(null_fk(&mut interner), &mut interner);
        let mut record = make_lvli(record_fk, vec![valid_entry, null_entry]);

        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed);
        assert_eq!(record.fields.len(), 1, "only the valid entry should remain");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_preserves_non_lvlo_fields() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let llct = FieldEntry {
            sig: SubrecordSig::from_str("LLCT").unwrap(),
            value: FieldValue::Uint(1),
        };
        let null_entry = lvlo_entry(null_fk(&mut interner), &mut interner);

        let mut record = make_lvli(record_fk, vec![llct, null_entry]);
        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed);
        assert_eq!(record.fields.len(), 1, "LLCT should be preserved");
        assert_eq!(record.fields[0].sig.as_str(), "LLCT");
    }

    #[test]
    fn apply_to_record_syncs_llct_after_drop() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let llct = FieldEntry {
            sig: SubrecordSig::from_str("LLCT").unwrap(),
            value: FieldValue::Uint(2),
        };
        let valid_entry = lvlo_entry(valid_master_fk(&mut interner), &mut interner);
        let null_entry = lvlo_entry(null_fk(&mut interner), &mut interner);

        let mut record = make_lvli(record_fk, vec![llct, valid_entry, null_entry]);
        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed);
        let llct = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LLCT")
            .expect("LLCT survives");
        assert_eq!(llct.value, FieldValue::Uint(1));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_empty_record_no_op() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let mut record = make_lvli(record_fk, vec![]);

        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(!changed);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_keeps_entry_with_no_reference_field() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let count_sym = interner.intern("Count");
        let no_ref_entry = FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Struct(vec![(count_sym, FieldValue::Uint(1))]),
        };

        let mut record = make_lvli(record_fk, vec![no_ref_entry]);
        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(!changed, "entry with no Reference field should be kept");
        assert_eq!(record.fields.len(), 1);
    }

    // -----------------------------------------------------------------------
    //
    // An LVLO struct with a null "Condition" FK but no "Reference" field must
    // be kept — only the named Reference triggers a drop.
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_does_not_drop_on_other_named_null_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let cond_sym = interner.intern("Condition");
        let entry = FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Struct(vec![(
                cond_sym,
                FieldValue::FormKey(null_fk(&mut interner)),
            )]),
        };

        let mut record = make_lvli(record_fk, vec![entry]);
        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(
            !changed,
            "null FK in 'Condition' must not trigger a drop — only 'Reference' counts"
        );
        assert_eq!(record.fields.len(), 1);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    fn llkc_entry(keywords: &[FormKey], interner: &StringInterner) -> FieldEntry {
        let kw_sym = interner.intern("filter_keyword_chances_keyword");
        let chance_sym = interner.intern("filter_keyword_chances_chance");
        let rows = keywords
            .iter()
            .map(|fk| {
                FieldValue::Struct(vec![
                    (kw_sym, FieldValue::FormKey(*fk)),
                    (chance_sym, FieldValue::Uint(100)),
                ])
            })
            .collect();
        FieldEntry {
            sig: SubrecordSig::from_str("LLKC").unwrap(),
            value: FieldValue::List(rows),
        }
    }

    fn llkc_rows(record: &Record) -> Option<usize> {
        record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "LLKC")
            .map(|e| match &e.value {
                FieldValue::List(rows) => rows.len(),
                _ => 0,
            })
    }

    #[test]
    fn llkc_drops_dangling_keyword_row_keeps_valid() {
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let valid = make_fk("001234", "Fallout4.esm", &interner); // < 0x8000 → valid
        let dangling = make_fk("009000", "Fallout4.esm", &interner); // > 0x8000 → missing
        let mut record = make_lvli(record_fk, vec![llkc_entry(&[valid, dangling], &interner)]);

        let mut pred = master_pred(&interner);
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "dangling LLKC keyword row should be dropped");
        assert_eq!(llkc_rows(&record), Some(1), "valid row kept");
    }

    #[test]
    fn llkc_drops_whole_subrecord_when_all_rows_dangle() {
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let dangling = make_fk("009000", "Fallout4.esm", &interner);
        let mut record = make_lvli(record_fk, vec![llkc_entry(&[dangling], &interner)]);

        let mut pred = master_pred(&interner);
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed);
        assert_eq!(llkc_rows(&record), None, "empty LLKC subrecord dropped");
    }

    #[test]
    fn llkc_inert_when_keyword_resolves() {
        // All keywords valid in master → no drop (this is the master-less-run shape:
        // with no masters loaded, the predicate never flags FO4 keywords).
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let valid_a = make_fk("001234", "Fallout4.esm", &interner);
        let valid_b = make_fk("005678", "Fallout4.esm", &interner);
        let mut record = make_lvli(record_fk, vec![llkc_entry(&[valid_a, valid_b], &interner)]);

        let mut pred = master_pred(&interner);
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(!changed, "all-valid LLKC untouched");
        assert_eq!(llkc_rows(&record), Some(2));
    }

    #[test]
    fn record_has_candidate_entries_flags_llkc_master_keyword() {
        // A LVLI whose only suspect subrecord is an LLKC pointing at a master must
        // still be selected as a candidate so apply_to_record gets to prune it.
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let kw = make_fk("009000", "Fallout4.esm", &interner);
        let record = make_lvli(
            record_fk,
            vec![
                uint_field("LVLD", 1),
                uint_field("LVLM", 1),
                llkc_entry(&[kw], &interner),
            ],
        );
        let masters = vec![("fallout4.esm".to_string(), 1u64)];
        assert!(record_has_candidate_entries(
            &record,
            ref_sym(&interner),
            &target_master_names(),
            "Mod.esp",
            &masters,
            &interner,
        ));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_drops_missing_master_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        // local 0x9000 > 0x8000 → simulated predicate marks it missing from master.
        let missing_fk = make_fk("009000", "Fallout4.esm", &mut interner);
        let bad_entry = lvlo_entry(missing_fk, &mut interner);
        let mut record = make_lvli(record_fk, vec![bad_entry]);

        let pred = master_pred(&mut interner);
        let mut pred = pred;
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "entry with missing-master FK should be dropped");
        assert!(record.fields.is_empty());
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_keeps_existing_master_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        // local 0x1234 < 0x8000 → predicate marks it as valid.
        let good_entry = lvlo_entry(valid_master_fk(&mut interner), &mut interner);
        let mut record = make_lvli(record_fk, vec![good_entry]);

        let pred = master_pred(&mut interner);
        let mut pred = pred;
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(!changed, "entry with existing master FK should be kept");
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn apply_to_record_keeps_fo4_caps_when_master_probe_flags_invalid() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let caps = make_fk("00000F", "Fallout4.esm", &mut interner);
        let mut record = make_lvli(record_fk, vec![lvlo_entry(caps, &mut interner)]);
        let mut pred = Box::new(|_: &FormKey| true);

        let changed = apply_for_test(&mut record, &interner, &mut pred);

        assert!(
            !changed,
            "FO4 caps should remain a valid leveled item entry"
        );
        assert_eq!(record.fields.len(), 1);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_keeps_non_master_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let entry = lvlo_entry(non_master_fk(&mut interner), &mut interner);
        let mut record = make_lvli(record_fk, vec![entry]);

        let pred = master_pred(&mut interner);
        let mut pred = pred;
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(!changed, "entry with non-master FK should be kept");
        assert_eq!(record.fields.len(), 1);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_mixed_multi_entry_keeps_only_valid() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        // good: non-master (SomeMod.esp) — kept by master_pred.
        let good1 = lvlo_entry(non_master_fk(&mut interner), &mut interner);
        // bad: null FK.
        let bad_null = lvlo_entry(null_fk(&mut interner), &mut interner);
        // bad: Fallout4.esm local > 0x8000 → missing from master.
        let bad_missing = lvlo_entry(
            make_fk("009000", "Fallout4.esm", &mut interner),
            &mut interner,
        );
        // good: Fallout4.esm local < 0x8000 → kept.
        let good2 = lvlo_entry(valid_master_fk(&mut interner), &mut interner);

        let mut record = make_lvli(record_fk, vec![good1, bad_null, bad_missing, good2]);

        let pred = master_pred(&mut interner);
        let mut pred = pred;
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed);
        assert_eq!(
            record.fields.len(),
            2,
            "only the two valid entries should survive"
        );
    }

    #[test]
    fn apply_to_record_drops_raw_null_reference() {
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let mut record = make_lvli(record_fk, vec![lvlo_raw_entry(0)]);

        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "raw null reference entry should be dropped");
        assert!(record.fields.is_empty());
    }

    #[test]
    fn apply_to_record_drops_raw_self_reference() {
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let mut record = make_lvli(record_fk, vec![lvlo_raw_entry(0x0100_0800)]);

        let mut pred = null_pred();
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "raw self-reference entry should be dropped");
        assert!(record.fields.is_empty());
    }

    #[test]
    fn apply_to_record_drops_raw_missing_master_fk() {
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let mut record = make_lvli(record_fk, vec![lvlo_raw_entry(0x0000_9000)]);

        let pred = master_pred(&interner);
        let mut pred = pred;
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(changed, "raw missing-master FK should be dropped");
        assert!(record.fields.is_empty());
    }

    #[test]
    fn apply_to_record_keeps_raw_existing_master_fk() {
        let interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &interner);
        let mut record = make_lvli(record_fk, vec![lvlo_raw_entry(0x0000_1234)]);

        let pred = master_pred(&interner);
        let mut pred = pred;
        let changed = apply_for_test(&mut record, &interner, &mut pred);
        assert!(!changed, "raw existing-master FK should be kept");
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn cycle_edges_to_drop_marks_closing_edge_only() {
        let interner = StringInterner::new();
        let lpi_fusion_core = make_fk("18ABE1", "Mod.esp", &interner);
        let lls_fusion_core_all = make_fk("43C16B", "Mod.esp", &interner);
        let mut graph = HashMap::new();
        graph.insert(lpi_fusion_core, vec![lls_fusion_core_all]);
        graph.insert(lls_fusion_core_all, vec![lpi_fusion_core]);

        let drops = cycle_edges_to_drop(&[lpi_fusion_core, lls_fusion_core_all], &graph);

        assert!(
            !drops
                .get(&lpi_fusion_core)
                .is_some_and(|targets| targets.contains(&lls_fusion_core_all))
        );
        assert!(
            drops
                .get(&lls_fusion_core_all)
                .is_some_and(|targets| targets.contains(&lpi_fusion_core))
        );
    }

    #[test]
    fn drop_entries_referencing_removes_cycle_edge_and_syncs_llct() {
        let mut interner = StringInterner::new();
        let owner_fk = make_fk("43C16B", "Mod.esp", &interner);
        let cycle_fk = make_fk("18ABE1", "Mod.esp", &interner);
        let valid_fk = make_fk("39424B", "Mod.esp", &interner);
        let llct = FieldEntry {
            sig: SubrecordSig::from_str("LLCT").unwrap(),
            value: FieldValue::Uint(2),
        };
        let mut record = make_lvli(
            owner_fk,
            vec![
                llct,
                lvlo_entry(cycle_fk, &mut interner),
                lvlo_entry(valid_fk, &mut interner),
            ],
        );
        let mut refs_to_drop = HashSet::new();
        refs_to_drop.insert(cycle_fk);
        let masters = target_master_names();

        let changed = drop_entries_referencing(
            &mut record,
            ref_sym(&mut interner),
            &masters,
            "Mod.esp",
            &interner,
            &refs_to_drop,
        );

        assert!(changed, "cycle-closing entry should be removed");
        assert_eq!(record.fields.len(), 2, "LLCT and one LVLO should remain");
        let llct = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LLCT")
            .expect("LLCT survives");
        assert_eq!(llct.value, FieldValue::Uint(1));
        let remaining = record
            .fields
            .iter()
            .find_map(|entry| {
                extract_entry_reference(
                    &entry.value,
                    ref_sym(&mut interner),
                    &masters,
                    "Mod.esp",
                    &interner,
                )
            })
            .expect("remaining LVLO reference");
        assert_eq!(remaining, valid_fk);
    }
}
