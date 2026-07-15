//! Fixup: sync ARMO hand slots from referenced hand ARMA add-ons.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

mod keys {
    pub const FIRST_PERSON_FLAGS: &str = "FirstPersonFlags";
    pub const RACE: &str = "Race";
}

const SLOT_34_LEFT_HAND: u8 = 34;
const SLOT_35_RIGHT_HAND: u8 = 35;
const BIPED_SLOT_34_LEFT_HAND: u64 = 1 << (SLOT_34_LEFT_HAND - 30);
const BIPED_SLOT_35_RIGHT_HAND: u64 = 1 << (SLOT_35_RIGHT_HAND - 30);
const HAND_SLOT_MASK: u64 = BIPED_SLOT_34_LEFT_HAND | BIPED_SLOT_35_RIGHT_HAND;
const BARE_HUMAN_HAND_ADDONS: &[u32] = &[0x000D6C, 0x01D980, 0x0316C7];
const BARE_GHOUL_HAND_ADDON: u32 = 0x0EAFBA;
const GHOUL_RACE: u32 = 0x0EAFB6;

#[derive(Default)]
struct HandAddonIndex {
    hand_addons: FxHashSet<FormKey>,
    hand_only_addons: FxHashSet<FormKey>,
    mixed_hand_addons: FxHashSet<FormKey>,
    ghoul_body_addons: FxHashSet<FormKey>,
    ghoul_hand_addon: Option<FormKey>,
}

pub struct SyncArmoHandSlotsFromAddonsFixup;

impl Fixup for SyncArmoHandSlotsFromAddonsFixup {
    fn name(&self) -> &'static str {
        "sync_armo_hand_slots_from_addons"
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
        let arma_sig =
            SigCode::from_str("ARMA").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let armo_sig =
            SigCode::from_str("ARMO").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();
        let hand_addons = collect_hand_addons(
            session,
            mapper,
            target_schema,
            &config.target_master_handle_ids,
            &mut report,
            arma_sig,
        )?;
        if hand_addons.hand_addons.is_empty() {
            return Ok(report);
        }

        let armo_fks = session
            .form_keys_of_sig(armo_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut changed_records = Vec::new();
        let mut strip_mixed_hand_slots_from_addons = FxHashSet::default();
        let mut protected_mixed_hand_addons = FxHashSet::default();

        for fk in armo_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(e) => {
                    let warning = mapper.interner.intern(&format!(
                        "sync_armo_hand_slots_from_addons:armo_read_err:{e}"
                    ));
                    report.warnings.push(warning);
                    continue;
                }
            };

            collect_mixed_hand_addon_strip_candidates(
                &record,
                &hand_addons.hand_only_addons,
                &hand_addons.mixed_hand_addons,
                &mut strip_mixed_hand_slots_from_addons,
                &mut protected_mixed_hand_addons,
                mapper.interner,
            );
            let synced = sync_armo_hand_slots_from_addons(
                &mut record,
                &hand_addons.hand_addons,
                mapper.interner,
            );
            let added_ghoul_hands = ensure_ghoul_hand_addon_for_ghoul_capable_armo(
                &mut record,
                &hand_addons.ghoul_body_addons,
                hand_addons.ghoul_hand_addon,
                &hand_addons.hand_addons,
                mapper.interner,
            );
            let pruned = prune_redundant_human_hand_addons(
                &mut record,
                &hand_addons.hand_addons,
                mapper.interner,
            );
            if synced || added_ghoul_hands || pruned {
                changed_records.push(record);
                report.records_changed += 1;
            }
        }

        let stripped_addon_records = strip_redundant_mixed_hand_slots_from_addons(
            session,
            mapper,
            target_schema,
            &strip_mixed_hand_slots_from_addons,
            &protected_mixed_hand_addons,
            &mut report,
        )?;
        let expected = changed_records.len() + stripped_addon_records.len();
        changed_records.extend(stripped_addon_records);
        if expected > 0 {
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "sync_armo_hand_slots_from_addons replaced {replaced} of {expected} expected records"
                )));
            }
        }

        Ok(report)
    }
}

fn collect_hand_addons(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    target_master_handle_ids: &[u64],
    report: &mut FixupReport,
    arma_sig: SigCode,
) -> Result<HandAddonIndex, FixupError> {
    let interner = mapper.interner;
    let arma_fks = session
        .form_keys_of_sig(arma_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut hand_addons = HandAddonIndex::default();

    for fk in arma_fks {
        let record = match session.record_decoded(&fk, target_schema, interner) {
            Ok(record) => record,
            Err(e) => {
                let warning = interner.intern(&format!(
                    "sync_armo_hand_slots_from_addons:arma_read_err:{e}"
                ));
                report.warnings.push(warning);
                continue;
            }
        };

        index_arma_record(fk, &record, interner, &mut hand_addons, true);
    }

    for handle_id in target_master_handle_ids {
        let arma_fks = session
            .form_keys_of_sig_in_handle(*handle_id, arma_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in arma_fks {
            let record =
                match session.record_decoded_in_handle(*handle_id, &fk, target_schema, interner) {
                    Ok(record) => record,
                    Err(e) => {
                        let warning = interner.intern(&format!(
                            "sync_armo_hand_slots_from_addons:master_arma_read_err:{e}"
                        ));
                        report.warnings.push(warning);
                        continue;
                    }
                };
            index_arma_record(fk, &record, interner, &mut hand_addons, false);
        }
    }

    Ok(hand_addons)
}

fn index_arma_record(
    fk: FormKey,
    record: &Record,
    interner: &StringInterner,
    hand_addons: &mut HandAddonIndex,
    can_mutate: bool,
) {
    let owns_hand_slots = record_has_hand_bod2(record, interner);
    if owns_hand_slots {
        hand_addons.hand_addons.insert(fk);
        if record_has_non_hand_bod2(record, interner) {
            if can_mutate {
                hand_addons.mixed_hand_addons.insert(fk);
            }
        } else {
            hand_addons.hand_only_addons.insert(fk);
        }
        if is_bare_ghoul_hand_addon_formkey(&fk, interner) {
            hand_addons.ghoul_hand_addon = Some(fk);
        }
        return;
    }

    if record_supports_ghoul_race(record, interner) {
        hand_addons.ghoul_body_addons.insert(fk);
    }
}

pub fn sync_armo_hand_slots_from_addons(
    record: &mut Record,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    if !record_allows_human_hand_slots(record, interner) {
        return false;
    }
    if !record_references_any_addon(record, hand_addons) {
        return false;
    }

    let bod2_sig = match SubrecordSig::from_str("BOD2") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    for entry in &mut record.fields {
        if entry.sig == bod2_sig {
            return ensure_hand_slots(&mut entry.value, interner);
        }
    }
    false
}

fn collect_mixed_hand_addon_strip_candidates(
    record: &Record,
    hand_only_addons: &FxHashSet<FormKey>,
    mixed_hand_addons: &FxHashSet<FormKey>,
    strip_candidates: &mut FxHashSet<FormKey>,
    protected_candidates: &mut FxHashSet<FormKey>,
    interner: &StringInterner,
) {
    if !record_allows_human_hand_slots(record, interner) {
        return;
    }

    let addon_refs = record_addon_formkeys(record);
    if addon_refs.is_empty() {
        return;
    }

    let has_dedicated_hand_addon = addon_refs.iter().any(|fk| hand_only_addons.contains(fk));
    for fk in addon_refs {
        if !mixed_hand_addons.contains(&fk) {
            continue;
        }
        if has_dedicated_hand_addon {
            strip_candidates.insert(fk);
        } else {
            protected_candidates.insert(fk);
        }
    }
}

fn strip_redundant_mixed_hand_slots_from_addons(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    strip_candidates: &FxHashSet<FormKey>,
    protected_candidates: &FxHashSet<FormKey>,
    report: &mut FixupReport,
) -> Result<Vec<Record>, FixupError> {
    let mut changed_records = Vec::new();
    for fk in strip_candidates {
        if protected_candidates.contains(fk) {
            continue;
        }

        let mut record = match session.record_decoded(fk, target_schema, mapper.interner) {
            Ok(record) => record,
            Err(e) => {
                let warning = mapper.interner.intern(&format!(
                    "sync_armo_hand_slots_from_addons:strip_arma_read_err:{e}"
                ));
                report.warnings.push(warning);
                continue;
            }
        };
        if strip_hand_slots_from_record(&mut record, mapper.interner) {
            changed_records.push(record);
            report.records_changed += 1;
        }
    }
    Ok(changed_records)
}

pub fn prune_redundant_human_hand_addons(
    record: &mut Record,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    if !record_allows_human_hand_slots(record, interner) {
        return false;
    }
    if !record_references_bare_human_hand_addon(record, interner) {
        return false;
    }
    if !record_references_non_bare_hand_addon(record, hand_addons, interner) {
        return false;
    }

    remove_bare_human_hand_addon_entries(record, interner)
}

pub fn ensure_ghoul_hand_addon_for_ghoul_capable_armo(
    record: &mut Record,
    ghoul_body_addons: &FxHashSet<FormKey>,
    ghoul_hand_addon: Option<FormKey>,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    let Some(ghoul_hand_addon) = ghoul_hand_addon else {
        return false;
    };
    if record_references_bare_ghoul_hand_addon(record, interner) {
        return false;
    }
    if !record_references_bare_human_hand_addon(record, interner) {
        return false;
    }
    if record_references_non_bare_hand_addon(record, hand_addons, interner) {
        return false;
    }
    if !record_references_any_addon(record, ghoul_body_addons) {
        return false;
    }

    insert_addon_entry(record, ghoul_hand_addon)
}

fn record_has_hand_bod2(record: &Record, interner: &StringInterner) -> bool {
    let bod2_sig = match SubrecordSig::from_str("BOD2") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == bod2_sig)
        .any(|entry| {
            value_has_slot(&entry.value, SLOT_34_LEFT_HAND, interner)
                && value_has_slot(&entry.value, SLOT_35_RIGHT_HAND, interner)
        })
}

fn record_has_non_hand_bod2(record: &Record, interner: &StringInterner) -> bool {
    let bod2_sig = match SubrecordSig::from_str("BOD2") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == bod2_sig)
        .any(|entry| value_has_non_hand_slot(&entry.value, interner))
}

fn record_supports_ghoul_race(record: &Record, interner: &StringInterner) -> bool {
    race_formkey_from_record(record, interner).is_some_and(|fk| is_ghoul_race_formkey(fk, interner))
        || record_has_modl_ghoul_race(record, interner)
}

fn record_has_modl_ghoul_race(record: &Record, interner: &StringInterner) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| modl_value_has_ghoul_race(&entry.value, interner))
}

fn modl_value_has_ghoul_race(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::FormKey(fk) => is_ghoul_race_formkey(*fk, interner),
        FieldValue::List(items) => items.iter().any(|item| match item {
            FieldValue::FormKey(fk) => is_ghoul_race_formkey(*fk, interner),
            _ => false,
        }),
        _ => false,
    }
}

fn record_allows_human_hand_slots(record: &Record, interner: &StringInterner) -> bool {
    match race_formkey_from_record(record, interner) {
        Some(fk) => is_human_race_formkey(fk, interner),
        None => true,
    }
}

fn race_formkey_from_record(record: &Record, interner: &StringInterner) -> Option<FormKey> {
    let rnam_sig = SubrecordSig::from_str("RNAM").ok()?;
    let race_sym = interner.intern(keys::RACE);

    record
        .fields
        .iter()
        .find(|entry| entry.sig == rnam_sig)
        .and_then(|entry| match &entry.value {
            FieldValue::FormKey(fk) => Some(*fk),
            FieldValue::Struct(fields) => struct_find(fields, race_sym).and_then(|value| {
                if let FieldValue::FormKey(fk) = value {
                    Some(*fk)
                } else {
                    None
                }
            }),
            _ => None,
        })
}

fn is_human_race_formkey(fk: FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    fk.local == 0x013746
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn is_ghoul_race_formkey(fk: FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    fk.local == GHOUL_RACE
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn struct_find<'a>(fields: &'a [(Sym, FieldValue)], key: Sym) -> Option<&'a FieldValue> {
    fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn struct_find_mut<'a>(
    fields: &'a mut Vec<(Sym, FieldValue)>,
    key: Sym,
) -> Option<&'a mut FieldValue> {
    fields.iter_mut().find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn record_references_any_addon(record: &Record, hand_addons: &FxHashSet<FormKey>) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_references_any_addon(&entry.value, hand_addons))
}

fn value_references_any_addon(value: &FieldValue, hand_addons: &FxHashSet<FormKey>) -> bool {
    match value {
        FieldValue::FormKey(fk) => hand_addons.contains(fk),
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_references_any_addon(item, hand_addons)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_references_any_addon(value, hand_addons)),
        _ => false,
    }
}

fn record_addon_formkeys(record: &Record) -> FxHashSet<FormKey> {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return FxHashSet::default(),
    };
    let mut refs = FxHashSet::default();
    for entry in record.fields.iter().filter(|entry| entry.sig == modl_sig) {
        collect_formkeys_from_value(&entry.value, &mut refs);
    }
    refs
}

fn collect_formkeys_from_value(value: &FieldValue, refs: &mut FxHashSet<FormKey>) {
    match value {
        FieldValue::FormKey(fk) => {
            refs.insert(*fk);
        }
        FieldValue::List(items) => {
            for item in items {
                collect_formkeys_from_value(item, refs);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_formkeys_from_value(value, refs);
            }
        }
        _ => {}
    }
}

fn record_references_bare_human_hand_addon(record: &Record, interner: &StringInterner) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_references_bare_human_hand_addon(&entry.value, interner))
}

fn record_references_bare_ghoul_hand_addon(record: &Record, interner: &StringInterner) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_references_bare_ghoul_hand_addon(&entry.value, interner))
}

fn record_references_non_bare_hand_addon(
    record: &Record,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_references_non_bare_hand_addon(&entry.value, hand_addons, interner))
}

fn value_references_bare_human_hand_addon(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::FormKey(fk) => is_bare_human_hand_addon_formkey(fk, interner),
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_references_bare_human_hand_addon(item, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_references_bare_human_hand_addon(value, interner)),
        _ => false,
    }
}

fn value_references_bare_ghoul_hand_addon(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::FormKey(fk) => is_bare_ghoul_hand_addon_formkey(fk, interner),
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_references_bare_ghoul_hand_addon(item, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_references_bare_ghoul_hand_addon(value, interner)),
        _ => false,
    }
}

fn value_references_non_bare_hand_addon(
    value: &FieldValue,
    hand_addons: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    match value {
        FieldValue::FormKey(fk) => {
            hand_addons.contains(fk)
                && !is_bare_human_hand_addon_formkey(fk, interner)
                && !is_bare_ghoul_hand_addon_formkey(fk, interner)
        }
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_references_non_bare_hand_addon(item, hand_addons, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_references_non_bare_hand_addon(value, hand_addons, interner)),
        _ => false,
    }
}

fn insert_addon_entry(record: &mut Record, addon_fk: FormKey) -> bool {
    let indx_sig = match SubrecordSig::from_str("INDX") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    let insert_at = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.sig == modl_sig)
        .map(|(index, _)| index + 1)
        .last()
        .unwrap_or(record.fields.len());

    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: indx_sig,
            value: FieldValue::Uint(0),
        },
    );
    record.fields.insert(
        insert_at + 1,
        FieldEntry {
            sig: modl_sig,
            value: FieldValue::FormKey(addon_fk),
        },
    );
    true
}

fn is_bare_human_hand_addon_formkey(fk: &FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    BARE_HUMAN_HAND_ADDONS.contains(&fk.local)
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn is_bare_ghoul_hand_addon_formkey(fk: &FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    fk.local == BARE_GHOUL_HAND_ADDON
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn remove_bare_human_hand_addon_entries(record: &mut Record, interner: &StringInterner) -> bool {
    let indx_sig = match SubrecordSig::from_str("INDX") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return false,
    };

    let mut changed = false;
    let mut index = 0;
    while index < record.fields.len() {
        let is_indexed_bare_hand_addon = index + 1 < record.fields.len()
            && record.fields[index].sig == indx_sig
            && record.fields[index + 1].sig == modl_sig
            && value_references_bare_human_hand_addon(&record.fields[index + 1].value, interner);
        if is_indexed_bare_hand_addon {
            record.fields.remove(index + 1);
            record.fields.remove(index);
            changed = true;
            continue;
        }

        let is_bare_hand_addon = record.fields[index].sig == modl_sig
            && value_references_bare_human_hand_addon(&record.fields[index].value, interner);
        if is_bare_hand_addon {
            record.fields.remove(index);
            changed = true;
            continue;
        }

        index += 1;
    }
    changed
}

fn strip_hand_slots_from_record(record: &mut Record, interner: &StringInterner) -> bool {
    let bod2_sig = match SubrecordSig::from_str("BOD2") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    for entry in &mut record.fields {
        if entry.sig == bod2_sig {
            return remove_hand_slots(&mut entry.value, interner);
        }
    }
    false
}

fn remove_hand_slots(value: &mut FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(mask) => {
            let before = *mask;
            *mask &= !HAND_SLOT_MASK;
            *mask != before
        }
        FieldValue::Int(mask) if *mask >= 0 => {
            let before = *mask;
            *mask &= !(HAND_SLOT_MASK as i64);
            *mask != before
        }
        FieldValue::List(items) => {
            let before = items.len();
            items.retain(|item| {
                let FieldValue::String(sym) = item else {
                    return true;
                };
                !interner
                    .resolve(*sym)
                    .and_then(biped_slot_from_token)
                    .is_some_and(|slot| slot == SLOT_34_LEFT_HAND || slot == SLOT_35_RIGHT_HAND)
            });
            items.len() != before
        }
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            if let Some(flags_value) = struct_find_mut(fields, first_person_flags_sym) {
                remove_hand_slots(flags_value, interner)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn ensure_hand_slots(value: &mut FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(mask) => {
            let before = *mask;
            *mask |= HAND_SLOT_MASK;
            *mask != before
        }
        FieldValue::Int(mask) if *mask >= 0 => {
            let before = *mask;
            *mask |= HAND_SLOT_MASK as i64;
            *mask != before
        }
        FieldValue::List(items) => ensure_hand_slots_in_list(items, interner),
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            if let Some(flags_value) = struct_find_mut(fields, first_person_flags_sym) {
                ensure_hand_slots(flags_value, interner)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn ensure_hand_slots_in_list(items: &mut Vec<FieldValue>, interner: &StringInterner) -> bool {
    let mut changed = false;
    if !list_has_slot(items, SLOT_34_LEFT_HAND, interner) {
        items.push(FieldValue::String(interner.intern("34LHand")));
        changed = true;
    }
    if !list_has_slot(items, SLOT_35_RIGHT_HAND, interner) {
        items.push(FieldValue::String(interner.intern("35RHand")));
        changed = true;
    }
    changed
}

fn value_has_slot(value: &FieldValue, slot: u8, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(mask) => *mask & mask_for_slot(slot) != 0,
        FieldValue::Int(mask) if *mask >= 0 => (*mask as u64) & mask_for_slot(slot) != 0,
        FieldValue::List(items) => list_has_slot(items, slot, interner),
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            struct_find(fields, first_person_flags_sym)
                .map(|value| value_has_slot(value, slot, interner))
                .unwrap_or(false)
        }
        _ => false,
    }
}

fn value_has_non_hand_slot(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(mask) => *mask & !HAND_SLOT_MASK != 0,
        FieldValue::Int(mask) if *mask >= 0 => (*mask as u64) & !HAND_SLOT_MASK != 0,
        FieldValue::List(items) => items.iter().any(|item| {
            let FieldValue::String(sym) = item else {
                return false;
            };
            interner
                .resolve(*sym)
                .and_then(biped_slot_from_token)
                .is_some_and(|slot| slot != SLOT_34_LEFT_HAND && slot != SLOT_35_RIGHT_HAND)
        }),
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            struct_find(fields, first_person_flags_sym)
                .map(|value| value_has_non_hand_slot(value, interner))
                .unwrap_or(false)
        }
        _ => false,
    }
}

fn list_has_slot(items: &[FieldValue], slot: u8, interner: &StringInterner) -> bool {
    items.iter().any(|item| {
        let FieldValue::String(sym) = item else {
            return false;
        };
        interner
            .resolve(*sym)
            .and_then(biped_slot_from_token)
            .is_some_and(|item_slot| item_slot == slot)
    })
}

fn biped_slot_from_token(token: &str) -> Option<u8> {
    let digits: String = token.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u8>().ok()
}

fn mask_for_slot(slot: u8) -> u64 {
    1 << (slot - 30)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn make_fk(hex: &str, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(&format!("{hex}@{plugin}"), interner).unwrap()
    }

    fn make_record(sig: &str, fk: FormKey) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn push_bod2_tokens(record: &mut Record, tokens: &[&str], interner: &StringInterner) {
        push_field(
            record,
            "BOD2",
            FieldValue::List(
                tokens
                    .iter()
                    .map(|token| FieldValue::String(interner.intern(token)))
                    .collect(),
            ),
        );
    }

    fn push_rnam(record: &mut Record, local: u32, plugin: &str, interner: &StringInterner) {
        push_field(
            record,
            "RNAM",
            FieldValue::FormKey(FormKey {
                local,
                plugin: interner.intern(plugin),
            }),
        );
    }

    fn push_addon(record: &mut Record, addon_fk: FormKey) {
        push_field(record, "INDX", FieldValue::Uint(0));
        push_field(record, "MODL", FieldValue::FormKey(addon_fk));
    }

    fn bod2_tokens(record: &Record, interner: &StringInterner) -> Vec<String> {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "BOD2")
            .and_then(|entry| {
                let FieldValue::List(items) = &entry.value else {
                    return None;
                };
                Some(
                    items
                        .iter()
                        .filter_map(|item| {
                            let FieldValue::String(sym) = item else {
                                return None;
                            };
                            interner.resolve(*sym).map(str::to_string)
                        })
                        .collect(),
                )
            })
            .unwrap_or_default()
    }

    fn addon_formkeys(record: &Record) -> Vec<FormKey> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "MODL")
            .filter_map(|entry| match entry.value {
                FieldValue::FormKey(fk) => Some(fk),
                _ => None,
            })
            .collect()
    }

    fn addon_index_count(record: &Record) -> usize {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "INDX")
            .count()
    }

    #[test]
    fn detects_converted_hand_addon_from_bod2_slots() {
        let interner = StringInterner::new();
        let fk = make_fk("58B272", "SeventySix.esm", &interner);
        let mut addon = make_record("ARMA", fk);
        push_bod2_tokens(&mut addon, &["34LHand", "35RHand"], &interner);

        assert!(record_has_hand_bod2(&addon, &interner));
    }

    #[test]
    fn adds_hand_slots_when_armo_references_hand_addon() {
        let interner = StringInterner::new();
        let addon_fk = make_fk("58B272", "SeventySix.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(addon_fk);

        let mut armo = make_record("ARMO", make_fk("58B273", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_field(&mut armo, "MODL", FieldValue::FormKey(addon_fk));

        assert!(sync_armo_hand_slots_from_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(
            bod2_tokens(&armo, &interner),
            vec!["33BODY", "34LHand", "35RHand"]
        );
    }

    #[test]
    fn adds_hand_slots_when_armo_references_master_hand_addon() {
        let interner = StringInterner::new();
        let addon_fk = make_fk("07239F", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(addon_fk);

        let mut armo = make_record("ARMO", make_fk("3B7D7F", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_field(&mut armo, "MODL", FieldValue::FormKey(addon_fk));

        assert!(sync_armo_hand_slots_from_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(
            bod2_tokens(&armo, &interner),
            vec!["33BODY", "34LHand", "35RHand"]
        );
    }

    #[test]
    fn skips_armo_without_matching_hand_addon() {
        let interner = StringInterner::new();
        let addon_fk = make_fk("58B271", "SeventySix.esm", &interner);
        let hand_addons = FxHashSet::default();

        let mut armo = make_record("ARMO", make_fk("58B273", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_field(&mut armo, "MODL", FieldValue::FormKey(addon_fk));

        assert!(!sync_armo_hand_slots_from_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(bod2_tokens(&armo, &interner), vec!["33BODY"]);
    }

    #[test]
    fn skips_explicit_nonhuman_armo() {
        let interner = StringInterner::new();
        let addon_fk = make_fk("58B272", "SeventySix.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(addon_fk);

        let mut armo = make_record("ARMO", make_fk("58B273", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY"], &interner);
        push_rnam(&mut armo, 0x6356DD, "SeventySix.esm", &interner);
        push_field(&mut armo, "MODL", FieldValue::FormKey(addon_fk));

        assert!(!sync_armo_hand_slots_from_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(bod2_tokens(&armo, &interner), vec!["33BODY"]);
    }

    #[test]
    fn strips_hand_slots_from_mixed_body_addon_when_dedicated_gloves_exist() {
        let interner = StringInterner::new();
        let body_fk = make_fk("787E52", "SeventySix.esm", &interner);
        let gloves_fk = make_fk("7AC69F", "SeventySix.esm", &interner);

        let mut body_addon = make_record("ARMA", body_fk);
        push_bod2_tokens(
            &mut body_addon,
            &["33BODY", "34LHand", "35RHand"],
            &interner,
        );
        let mut gloves_addon = make_record("ARMA", gloves_fk);
        push_bod2_tokens(&mut gloves_addon, &["34LHand", "35RHand"], &interner);

        let mut index = HandAddonIndex::default();
        index_arma_record(body_fk, &body_addon, &interner, &mut index, true);
        index_arma_record(gloves_fk, &gloves_addon, &interner, &mut index, true);

        let mut armo = make_record("ARMO", make_fk("787E5A", "SeventySix.esm", &interner));
        push_bod2_tokens(
            &mut armo,
            &[
                "33BODY", "34LHand", "35RHand", "36UTorso", "37ULArm", "38URArm", "39ULLeg",
                "40URLeg",
            ],
            &interner,
        );
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, body_fk);
        push_addon(&mut armo, gloves_fk);

        let mut strip_candidates = FxHashSet::default();
        let mut protected_candidates = FxHashSet::default();
        collect_mixed_hand_addon_strip_candidates(
            &armo,
            &index.hand_only_addons,
            &index.mixed_hand_addons,
            &mut strip_candidates,
            &mut protected_candidates,
            &interner,
        );

        assert!(strip_candidates.contains(&body_fk));
        assert!(!protected_candidates.contains(&body_fk));
        assert!(strip_hand_slots_from_record(&mut body_addon, &interner));
        assert_eq!(bod2_tokens(&body_addon, &interner), vec!["33BODY"]);
        assert_eq!(
            bod2_tokens(&gloves_addon, &interner),
            vec!["34LHand", "35RHand"]
        );
    }

    #[test]
    fn protects_mixed_body_addon_when_used_without_dedicated_gloves() {
        let interner = StringInterner::new();
        let body_fk = make_fk("787E52", "SeventySix.esm", &interner);
        let mut body_addon = make_record("ARMA", body_fk);
        push_bod2_tokens(
            &mut body_addon,
            &["33BODY", "34LHand", "35RHand"],
            &interner,
        );

        let mut index = HandAddonIndex::default();
        index_arma_record(body_fk, &body_addon, &interner, &mut index, true);

        let mut armo = make_record("ARMO", make_fk("787E5A", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, body_fk);

        let mut strip_candidates = FxHashSet::default();
        let mut protected_candidates = FxHashSet::default();
        collect_mixed_hand_addon_strip_candidates(
            &armo,
            &index.hand_only_addons,
            &index.mixed_hand_addons,
            &mut strip_candidates,
            &mut protected_candidates,
            &interner,
        );

        assert!(!strip_candidates.contains(&body_fk));
        assert!(protected_candidates.contains(&body_fk));
    }

    #[test]
    fn prunes_naked_hands_when_another_addon_owns_hand_slots() {
        let interner = StringInterner::new();
        let resident_addon = make_fk("0E5083", "Fallout4.esm", &interner);
        let naked_hands = make_fk("000D6C", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(resident_addon);
        hand_addons.insert(naked_hands);

        let mut armo = make_record("ARMO", make_fk("3B7D81", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, resident_addon);
        push_addon(&mut armo, naked_hands);

        assert!(prune_redundant_human_hand_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(addon_formkeys(&armo), vec![resident_addon]);
        assert_eq!(addon_index_count(&armo), 1);
    }

    #[test]
    fn adds_ghoul_hands_when_bare_human_hands_back_a_ghoul_body_addon() {
        let interner = StringInterner::new();
        let responder_jumpsuit = make_fk("3E5769", "SeventySix.esm", &interner);
        let naked_hands = make_fk("000D6C", "Fallout4.esm", &interner);
        let naked_ghoul_hands = make_fk("0EAFBA", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(naked_hands);
        hand_addons.insert(naked_ghoul_hands);
        let mut ghoul_body_addons = FxHashSet::default();
        ghoul_body_addons.insert(responder_jumpsuit);

        let mut armo = make_record("ARMO", make_fk("3B7D64", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, responder_jumpsuit);
        push_addon(&mut armo, naked_hands);

        assert!(ensure_ghoul_hand_addon_for_ghoul_capable_armo(
            &mut armo,
            &ghoul_body_addons,
            Some(naked_ghoul_hands),
            &hand_addons,
            &interner
        ));
        assert_eq!(
            addon_formkeys(&armo),
            vec![responder_jumpsuit, naked_hands, naked_ghoul_hands]
        );
        assert_eq!(addon_index_count(&armo), 3);
        assert!(!prune_redundant_human_hand_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
    }

    #[test]
    fn skips_ghoul_hands_when_a_non_bare_hand_addon_is_present() {
        let interner = StringInterner::new();
        let responder_jumpsuit = make_fk("3E5769", "SeventySix.esm", &interner);
        let glove_addon = make_fk("58B272", "SeventySix.esm", &interner);
        let naked_hands = make_fk("000D6C", "Fallout4.esm", &interner);
        let naked_ghoul_hands = make_fk("0EAFBA", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(glove_addon);
        hand_addons.insert(naked_hands);
        hand_addons.insert(naked_ghoul_hands);
        let mut ghoul_body_addons = FxHashSet::default();
        ghoul_body_addons.insert(responder_jumpsuit);

        let mut armo = make_record("ARMO", make_fk("3B7D64", "SeventySix.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, responder_jumpsuit);
        push_addon(&mut armo, glove_addon);
        push_addon(&mut armo, naked_hands);

        assert!(!ensure_ghoul_hand_addon_for_ghoul_capable_armo(
            &mut armo,
            &ghoul_body_addons,
            Some(naked_ghoul_hands),
            &hand_addons,
            &interner
        ));
        assert_eq!(
            addon_formkeys(&armo),
            vec![responder_jumpsuit, glove_addon, naked_hands]
        );
    }

    #[test]
    fn keeps_naked_hands_when_it_is_the_only_hand_addon() {
        let interner = StringInterner::new();
        let naked_hands = make_fk("000D6C", "Fallout4.esm", &interner);
        let mut hand_addons = FxHashSet::default();
        hand_addons.insert(naked_hands);

        let mut armo = make_record("ARMO", make_fk("000D64", "Fallout4.esm", &interner));
        push_bod2_tokens(&mut armo, &["33BODY", "34LHand", "35RHand"], &interner);
        push_rnam(&mut armo, 0x013746, "Fallout4.esm", &interner);
        push_addon(&mut armo, naked_hands);

        assert!(!prune_redundant_human_hand_addons(
            &mut armo,
            &hand_addons,
            &interner
        ));
        assert_eq!(addon_formkeys(&armo), vec![naked_hands]);
        assert_eq!(addon_index_count(&armo), 1);
    }
}
