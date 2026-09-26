//! Fixup: remove records that became orphaned after translation.
//!
//! After DeathItem nullification, loot-chain records (LVLI, AMMO, ALCH; see
//! `is_prunable_sig`) may have no incoming references left. A BFS seeded from every
//! non-prunable record (all present types, so CELL/REFR/ACHR reachability counts)
//! marks the live set; unreached prunable records are removed. Runs only for
//! creature-root (NPC_/LVLN) graph conversions, the only ones that produce the
//! death-loot orphan pattern.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Prunable signatures
// ---------------------------------------------------------------------------

/// Record signatures that may be dropped when they have no incoming references:
/// the loot-chain types LVLI, AMMO, and ALCH.
fn is_prunable_sig(sig: SigCode) -> bool {
    matches!(sig.as_str(), "LVLI" | "AMMO" | "ALCH")
}

// ---------------------------------------------------------------------------
// Creature-root guard
// ---------------------------------------------------------------------------

/// Returns `true` for the creature root types `NPC_` and `LVLN`.
pub fn is_creature_root_sig(sig: SigCode) -> bool {
    matches!(sig.as_str(), "NPC_" | "LVLN")
}

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct PruneOrphanedRecordsFixup;

impl Fixup for PruneOrphanedRecordsFixup {
    fn name(&self) -> &'static str {
        "prune_orphaned_records"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::GraphOnly
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        // 1. Whole-plugin conversions skip this fixup entirely.
        if ctx.config.is_whole_plugin {
            return false;
        }
        // 2. Only creature root types (NPC_ / LVLN) need orphan pruning.
        match ctx.config.root_sig {
            Some(sig) => is_creature_root_sig(sig),
            // No root sig means unknown/whole-plugin — skip.
            None => false,
        }
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        if config.is_whole_plugin {
            return false;
        }
        match config.root_sig {
            Some(sig) => is_creature_root_sig(sig),
            None => false,
        }
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        // Collect all records present in the target plugin. Seeding the BFS
        // from the dynamic set of present record types (not just prunable ones)
        // avoids false-positive pruning of records reachable via CELL, REFR,
        // ACHR, etc.
        let all_sigs = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut records_by_fk: FxHashMap<FormKey, Record> = FxHashMap::default();

        for sig in all_sigs {
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                match session.record_decoded(&fk, target_schema.as_ref(), mapper.interner) {
                    Ok(record) => {
                        records_by_fk.insert(fk, record);
                    }
                    Err(e) => {
                        // Non-fatal — skip record.
                        let _ = mapper
                            .interner
                            .intern(&format!("orphan_prune_read_err:{e}"));
                    }
                }
            }
        }

        if records_by_fk.is_empty() {
            return Ok(FixupReport::empty());
        }

        // Build adjacency: collect all FormKey references from each record.
        let mut refs_by_fk: FxHashMap<FormKey, FxHashSet<FormKey>> = FxHashMap::default();
        for (fk, record) in &records_by_fk {
            let mut refs: FxHashSet<FormKey> = FxHashSet::default();
            for field in &record.fields {
                collect_form_keys(&field.value, &mut refs);
            }
            refs.remove(fk); // discard self-references
            refs_by_fk.insert(*fk, refs);
        }

        // BFS from non-prunable roots.
        let (records_to_drop, records_visited) = apply_to_records(&records_by_fk, &refs_by_fk);

        let _ = records_visited; // only needed for tests; unused in prod

        if records_to_drop.is_empty() {
            return Ok(FixupReport::empty());
        }

        let mut report = FixupReport::empty();
        for fk in &records_to_drop {
            if session
                .remove_record(fk)
                .map_err(|e| FixupError::HandleError(e.to_string()))?
            {
                report.records_dropped += 1;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Core reachability logic (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Given a snapshot of all records and their outgoing FormKey references,
/// returns `(orphaned_fks, visited_fks)`.
///
/// `orphaned_fks` are prunable records with no incoming path from any
/// non-prunable root.  `visited_fks` is the reachable set (for tests).
pub fn apply_to_records(
    records_by_fk: &FxHashMap<FormKey, Record>,
    refs_by_fk: &FxHashMap<FormKey, FxHashSet<FormKey>>,
) -> (Vec<FormKey>, FxHashSet<FormKey>) {
    // Seed BFS from non-prunable records.
    let mut queue: Vec<FormKey> = records_by_fk
        .keys()
        .filter(|fk| {
            records_by_fk
                .get(fk)
                .map(|r| !is_prunable_sig(r.sig))
                .unwrap_or(false)
        })
        .copied()
        .collect();

    if queue.is_empty() {
        // No roots — nothing is reachable, but also nothing to safely prune
        // (we'd delete everything). Return empty.
        return (Vec::new(), FxHashSet::default());
    }

    let mut visited: FxHashSet<FormKey> = FxHashSet::default();

    while let Some(fk) = queue.pop() {
        if !visited.insert(fk) {
            continue;
        }
        if let Some(refs) = refs_by_fk.get(&fk) {
            for &ref_fk in refs {
                if records_by_fk.contains_key(&ref_fk) && !visited.contains(&ref_fk) {
                    queue.push(ref_fk);
                }
            }
        }
    }

    let orphans: Vec<FormKey> = records_by_fk
        .keys()
        .filter(|fk| {
            !visited.contains(fk)
                && records_by_fk
                    .get(fk)
                    .map(|r| is_prunable_sig(r.sig))
                    .unwrap_or(false)
        })
        .copied()
        .collect();

    (orphans, visited)
}

// ---------------------------------------------------------------------------
// FormKey reference collector (recursive FieldValue walk)
// ---------------------------------------------------------------------------

/// Recursively collect all `FieldValue::FormKey` values from `value` into `out`.
pub fn collect_form_keys(value: &FieldValue, out: &mut FxHashSet<FormKey>) {
    match value {
        FieldValue::FormKey(fk) => {
            out.insert(*fk);
        }
        FieldValue::List(items) => {
            for item in items {
                collect_form_keys(item, out);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, v) in fields {
                collect_form_keys(v, out);
            }
        }
        _ => {}
    }
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

    fn make_fk(hex: &str, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(&format!("{hex}@{plugin}"), interner).unwrap()
    }

    fn make_record(
        sig: &str,
        fk: FormKey,
        refs: Vec<FormKey>,
        _interner: &StringInterner,
    ) -> Record {
        let sig_code = SigCode::from_str(sig).unwrap();
        let ref_sig = SubrecordSig::from_str("DATA").unwrap();

        let fields: smallvec::SmallVec<[FieldEntry; 8]> = refs
            .into_iter()
            .map(|r| FieldEntry {
                sig: ref_sig,
                value: FieldValue::FormKey(r),
            })
            .collect();

        Record {
            sig: sig_code,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn build_refs(records: &FxHashMap<FormKey, Record>) -> FxHashMap<FormKey, FxHashSet<FormKey>> {
        let mut out = FxHashMap::default();
        for (fk, record) in records {
            let mut refs = FxHashSet::default();
            for field in &record.fields {
                collect_form_keys(&field.value, &mut refs);
            }
            refs.remove(fk);
            out.insert(*fk, refs);
        }
        out
    }

    #[test]
    fn only_unreachable_prunable_records_are_orphans() {
        let interner = StringInterner::new();
        let fk = |local: u32| make_fk(&format!("{local:06X}"), "Mod.esp", &interner);
        for (name, records, expected_orphans) in [
            (
                "referenced LVLI kept",
                vec![("NPC_", 0x801, vec![0x802]), ("LVLI", 0x802, vec![])],
                vec![],
            ),
            (
                "unreferenced LVLI dropped",
                vec![("NPC_", 0x801, vec![]), ("LVLI", 0x802, vec![])],
                vec![0x802],
            ),
            (
                "non-prunable WEAP kept",
                vec![("NPC_", 0x801, vec![]), ("WEAP", 0x802, vec![])],
                vec![],
            ),
            (
                "transitive chain kept",
                vec![
                    ("NPC_", 0x801, vec![0x802]),
                    ("LVLI", 0x802, vec![0x803]),
                    ("AMMO", 0x803, vec![]),
                ],
                vec![],
            ),
            ("empty plugin", vec![], vec![]),
            (
                "CELL root keeps referenced LVLI",
                vec![("CELL", 0x801, vec![0x802]), ("LVLI", 0x802, vec![])],
                vec![],
            ),
        ] {
            let records: FxHashMap<FormKey, Record> = records
                .into_iter()
                .map(|(sig, local, refs)| {
                    let form_key = fk(local);
                    let refs = refs.into_iter().map(fk).collect();
                    (form_key, make_record(sig, form_key, refs, &interner))
                })
                .collect();
            let refs = build_refs(&records);

            let (orphans, _visited) = apply_to_records(&records, &refs);
            let expected: Vec<FormKey> = expected_orphans.into_iter().map(fk).collect();
            assert_eq!(orphans, expected, "{name}");
        }

        let fk1 = fk(0x801);
        let fk2 = fk(0x802);
        let nested = FieldValue::List(vec![
            FieldValue::Struct(vec![(interner.intern("ref"), FieldValue::FormKey(fk1))]),
            FieldValue::FormKey(fk2),
        ]);
        let mut out = FxHashSet::default();
        collect_form_keys(&nested, &mut out);
        assert_eq!(out, FxHashSet::from_iter([fk1, fk2]));
    }

    #[test]
    fn applies_only_to_creature_root_slices() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use std::sync::Arc;

        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        for (name, is_whole_plugin, root, expected) in [
            ("whole plugin", true, "NPC_", false),
            ("non-creature root", false, "WEAP", false),
            ("npc root", false, "NPC_", true),
        ] {
            let config = FixupConfig {
                is_whole_plugin,
                root_sig: Some(SigCode::from_str(root).unwrap()),
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
            assert_eq!(
                PruneOrphanedRecordsFixup.applies_to(&ctx),
                expected,
                "{name}"
            );
        }

        for sig_str in ["NPC_", "LVLN"] {
            assert!(
                is_creature_root_sig(SigCode::from_str(sig_str).unwrap()),
                "{sig_str}"
            );
        }
        for sig_str in ["WEAP", "ARMO", "CELL", "REFR", "ACHR", "RACE", "QUST"] {
            assert!(
                !is_creature_root_sig(SigCode::from_str(sig_str).unwrap()),
                "{sig_str}"
            );
        }
    }
}
