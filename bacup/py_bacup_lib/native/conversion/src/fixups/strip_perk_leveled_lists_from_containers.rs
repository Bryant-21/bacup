//! Fixup: strip perk-gated leveled lists (`LL_Perk_*`) from CONT inventories.
//!
//! # Why
//! FO76 gates container loot behind perk-conditional leveled lists. The
//! WhiteSpring trash can (`0711CEED`) carries a `CNTO` entry pointing at
//! `LL_Perk_CanDo` (`0759DD1D`), an `UseAll` list of five reward items. In FO76
//! that LVLI is only realized when the player owns the "Can Do" perk
//! (`00346E0A`) — the leveled-list entries carry perk conditions. FO4 leveled
//! lists cannot condition on a perk, so every `UseAll` entry drops into the
//! container unconditionally and containers end up stuffed with perk-reward
//! items. Bethesda's own FO4 equivalent (Scrounger etc.) drives this through a
//! Papyrus quest (`0004A09E`), never through container leveled lists.
//!
//! # What
//! Drop any CONT `CNTO` entry whose item is an `LL_Perk_*` leveled list owned by
//! the output plugin, keeping the `COCT` item count in lockstep (paired-array
//! rule — a stale `COCT` hard-CTDs FO4 on cell load). The LVLI records
//! themselves are left in place; they simply lose their container references.
//!
//! Scope is CONT only: the reported symptom is over-stuffed containers. NPC_
//! inventories use the same `CNTO`/`COCT` shape, so extending to NPC_ is a
//! one-line change to `CONTAINER_SIGS` if perk lists turn up in NPC loot.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};

/// Record signatures whose `CNTO` inventories are swept for perk leveled lists.
const CONTAINER_SIGS: &[&str] = &["CONT"];

pub struct StripPerkLeveledListsFromContainersFixup;

/// FO76 perk-gated leveled lists carry an `LL_Perk_` token in their EditorID:
/// `LL_Perk_CanDo`, `LL_Perk_Scrounger`, `zzz_LL_Perk_Scrounger_Babylon`,
/// `DEBUG_LL_Perk_Pack`, ... — always at the start or preceded by `_`.
fn is_perk_leveled_list_name(editor_id: &str) -> bool {
    editor_id.starts_with("LL_Perk_") || editor_id.contains("_LL_Perk_")
}

impl Fixup for StripPerkLeveledListsFromContainersFixup {
    fn name(&self) -> &'static str {
        "strip_perk_leveled_lists_from_containers"
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
        let interner = mapper.interner;
        let lvli_sig =
            SigCode::from_str("LVLI").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();

        // Pass 1: object-ids of `LL_Perk_*` leveled lists in the output plugin.
        let perk_ll_ids: FxHashSet<u32> = {
            let view = session
                .read_view()
                .map_err(|e| FixupError::Other(e.to_string()))?;
            view.form_keys_of_sig(lvli_sig, interner)
                .into_iter()
                .filter(
                    |fk| match view.record_decoded(fk, target_schema, interner) {
                        Ok(record) => record
                            .eid
                            .and_then(|sym| interner.resolve(sym))
                            .is_some_and(is_perk_leveled_list_name),
                        Err(_) => false,
                    },
                )
                .map(|fk| fk.local)
                .collect()
        };

        if perk_ll_ids.is_empty() {
            return Ok(FixupReport::empty());
        }

        // A `CNTO` item is a perk list only when both the object-id matches AND it
        // addresses the output plugin (guards against a base-master ref colliding
        // on object-id with an output-plugin perk list).
        let is_perk_item = |fk: &FormKey| -> bool {
            perk_ll_ids.contains(&fk.local)
                && interner
                    .resolve(fk.plugin)
                    .is_some_and(|plugin| plugin.eq_ignore_ascii_case(&target_plugin_name))
        };

        let mut report = FixupReport::empty();
        for sig_str in CONTAINER_SIGS {
            let sig =
                SigCode::from_str(sig_str).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            let sig_report = session.map_apply_by_sig(
                sig,
                mapper,
                |view, _snapshot, fk| {
                    let record = view.record_decoded(fk, target_schema, interner).ok()?;
                    record_has_perk_cnto(&record, &is_perk_item).then_some(record)
                },
                |session, mapper, _fk, mut record| {
                    if strip_perk_cntos(&mut record, &is_perk_item) {
                        session
                            .replace_record_contents(record, target_schema, mapper.interner)
                            .map_err(|e| FixupError::HandleError(e.to_string()))?;
                        Ok(EditOutcome::Changed)
                    } else {
                        Ok(EditOutcome::NoOp)
                    }
                },
            )?;
            report.records_changed += sig_report.records_changed;
            report.records_dropped += sig_report.records_dropped;
            report.records_added += sig_report.records_added;
            report.warnings.extend(sig_report.warnings);
        }
        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// First `FormKey` reachable inside a decoded `CNTO` value. FO4 `CNTO` decodes to
/// `Struct { Item: FormKey, Count }`, so the item ref is the first (and only)
/// FormKey leaf.
fn cnto_item_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, v)| cnto_item_form_key(v)),
        FieldValue::List(items) => items.iter().find_map(cnto_item_form_key),
        _ => None,
    }
}

fn record_has_perk_cnto(record: &Record, is_perk_item: &impl Fn(&FormKey) -> bool) -> bool {
    record.fields.iter().any(|entry| {
        entry.sig.as_str() == "CNTO"
            && cnto_item_form_key(&entry.value).is_some_and(|fk| is_perk_item(&fk))
    })
}

/// Drop every `CNTO` whose item satisfies `is_perk_item` and resync `COCT`.
/// Returns whether anything changed.
fn strip_perk_cntos(record: &mut Record, is_perk_item: &impl Fn(&FormKey) -> bool) -> bool {
    let before = record.fields.len();
    record.fields.retain(|entry| {
        if entry.sig.as_str() != "CNTO" {
            return true;
        }
        cnto_item_form_key(&entry.value).is_none_or(|fk| !is_perk_item(&fk))
    });
    if record.fields.len() == before {
        return false;
    }
    resync_container_count(record);
    true
}

/// Realign the `COCT` item count with the surviving `CNTO` rows, dropping `COCT`
/// entirely when no inventory rows remain.
fn resync_container_count(record: &mut Record) {
    let cnto_count = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "CNTO")
        .count();
    record.fields.retain_mut(|entry| {
        if entry.sig.as_str() != "COCT" {
            return true;
        }
        if cnto_count == 0 {
            return false;
        }
        set_count_value(&mut entry.value, cnto_count);
        true
    });
}

fn set_count_value(value: &mut FieldValue, count: usize) {
    match value {
        FieldValue::Uint(existing) => *existing = count as u64,
        FieldValue::Int(existing) => *existing = count as i64,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let count = count.min(u32::MAX as usize) as u32;
            bytes[0..4].copy_from_slice(&count.to_le_bytes());
        }
        other => *other = FieldValue::Uint(count as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, RecordFlags};
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    const OUTPUT_PLUGIN: &str = "SeventySix.esm";
    const PERK_LL_ID: u32 = 0x59DD1D;
    const NORMAL_ITEM_ID: u32 = 0x0673B5;

    fn cnto(item_local: u32, count: u64, interner: &StringInterner) -> FieldEntry {
        let item = FormKey {
            local: item_local,
            plugin: interner.intern(OUTPUT_PLUGIN),
        };
        let count_sym = interner.intern("Count");
        let item_sym = interner.intern("Item");
        FieldEntry {
            sig: SubrecordSig::from_str("CNTO").unwrap(),
            value: FieldValue::Struct(vec![
                (item_sym, FieldValue::FormKey(item)),
                (count_sym, FieldValue::Uint(count)),
            ]),
        }
    }

    fn coct(count: u64) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str("COCT").unwrap(),
            value: FieldValue::Uint(count),
        }
    }

    fn container(fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("CONT").unwrap(),
            form_key: FormKey {
                local: 0x11CEED,
                plugin: interner.intern(OUTPUT_PLUGIN),
            },
            eid: Some(interner.intern("WhiteSpring_Trashcan")),
            flags: RecordFlags::empty(),
            fields: SmallVec::from_vec(fields),
            warnings: SmallVec::new(),
        }
    }

    fn perk_predicate<'a>(
        interner: &'a StringInterner,
        perk_ll_ids: &'a FxHashSet<u32>,
    ) -> impl Fn(&FormKey) -> bool + 'a {
        move |fk: &FormKey| {
            perk_ll_ids.contains(&fk.local)
                && interner
                    .resolve(fk.plugin)
                    .is_some_and(|p| p.eq_ignore_ascii_case(OUTPUT_PLUGIN))
        }
    }

    #[test]
    fn name_matcher_covers_the_perk_families() {
        assert!(is_perk_leveled_list_name("LL_Perk_CanDo"));
        assert!(is_perk_leveled_list_name("LL_Perk_CanDo_Items"));
        assert!(is_perk_leveled_list_name("zzz_LL_Perk_Scrounger_Babylon"));
        assert!(is_perk_leveled_list_name("DEBUG_LL_Perk_Pack"));
        assert!(!is_perk_leveled_list_name("LL_Junk_Common"));
        assert!(!is_perk_leveled_list_name("WhiteSpring_Trashcan"));
    }

    #[test]
    fn drops_perk_cnto_and_decrements_coct() {
        let interner = StringInterner::new();
        let perk_ll_ids: FxHashSet<u32> = [PERK_LL_ID].into_iter().collect();
        let is_perk = perk_predicate(&interner, &perk_ll_ids);

        let mut record = container(
            vec![
                coct(2),
                cnto(NORMAL_ITEM_ID, 1, &interner),
                cnto(PERK_LL_ID, 1, &interner),
            ],
            &interner,
        );

        assert!(record_has_perk_cnto(&record, &is_perk));
        assert!(strip_perk_cntos(&mut record, &is_perk));

        let cnto_items: Vec<u32> = record
            .fields
            .iter()
            .filter(|e| e.sig.as_str() == "CNTO")
            .filter_map(|e| cnto_item_form_key(&e.value))
            .map(|fk| fk.local)
            .collect();
        assert_eq!(cnto_items, vec![NORMAL_ITEM_ID], "perk CNTO dropped");

        let coct_value = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "COCT")
            .map(|e| &e.value);
        assert!(
            matches!(coct_value, Some(FieldValue::Uint(1))),
            "COCT must track the surviving CNTO row, got {coct_value:?}"
        );
    }

    #[test]
    fn drops_coct_when_all_cnto_removed() {
        let interner = StringInterner::new();
        let perk_ll_ids: FxHashSet<u32> = [PERK_LL_ID].into_iter().collect();
        let is_perk = perk_predicate(&interner, &perk_ll_ids);

        let mut record = container(vec![coct(1), cnto(PERK_LL_ID, 1, &interner)], &interner);

        assert!(strip_perk_cntos(&mut record, &is_perk));
        assert!(
            record.fields.iter().all(|e| e.sig.as_str() != "CNTO"),
            "only CNTO was perk-gated → removed"
        );
        assert!(
            record.fields.iter().all(|e| e.sig.as_str() != "COCT"),
            "stale COCT dropped with the final CNTO row"
        );
    }

    #[test]
    fn leaves_non_perk_container_untouched() {
        let interner = StringInterner::new();
        let perk_ll_ids: FxHashSet<u32> = [PERK_LL_ID].into_iter().collect();
        let is_perk = perk_predicate(&interner, &perk_ll_ids);

        let mut record = container(vec![coct(1), cnto(NORMAL_ITEM_ID, 1, &interner)], &interner);

        assert!(!record_has_perk_cnto(&record, &is_perk));
        assert!(!strip_perk_cntos(&mut record, &is_perk));
        assert!(
            matches!(
                record
                    .fields
                    .iter()
                    .find(|e| e.sig.as_str() == "COCT")
                    .map(|e| &e.value),
                Some(FieldValue::Uint(1))
            ),
            "untouched container keeps its COCT"
        );
    }

    #[test]
    fn ignores_perk_id_owned_by_foreign_plugin() {
        let interner = StringInterner::new();
        let perk_ll_ids: FxHashSet<u32> = [PERK_LL_ID].into_iter().collect();
        let is_perk = perk_predicate(&interner, &perk_ll_ids);

        // Same object-id, but the CNTO addresses a base master → not our perk list.
        let foreign = FormKey {
            local: PERK_LL_ID,
            plugin: interner.intern("Fallout4.esm"),
        };
        let count_sym = interner.intern("Count");
        let item_sym = interner.intern("Item");
        let mut record = container(
            vec![
                coct(1),
                FieldEntry {
                    sig: SubrecordSig::from_str("CNTO").unwrap(),
                    value: FieldValue::Struct(vec![
                        (item_sym, FieldValue::FormKey(foreign)),
                        (count_sym, FieldValue::Uint(1)),
                    ]),
                },
            ],
            &interner,
        );

        assert!(!strip_perk_cntos(&mut record, &is_perk));
    }
}
