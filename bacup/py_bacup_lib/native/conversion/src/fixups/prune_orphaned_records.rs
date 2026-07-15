//! Fixup: remove records that became orphaned after translation.
//!

//!
//! # What this does
//! After the DeathItem nullification pass, certain loot-chain records (LVLI,
//! AMMO, ALCH) may have no remaining incoming references from any other record
//! in the output plugin.  This fixup removes those orphans to keep the ESP clean.
//!
//! # Algorithm (two-pass)
//! 1. **Index pass** — for every record in the target plugin, collect all
//!    `FieldValue::FormKey` references via a recursive walk.  Build a map from
//!    FormKey → record index, and a set of all referenced FormKeys.
//! 2. **Reachability pass** — BFS from every non-prunable root record.  Any
//!    prunable-type record not reached by this traversal is an orphan.
//! 3. **Remove pass** — call `plugin_handle_remove_record_native` on each orphan.
//!
//! # Prunable types
//! Only records whose record signature is in `PRUNABLE_SIGS` are ever removed.
//! All other record types are treated as roots.
//!
//! # Guards
//! - `is_whole_plugin = true` → no-op (whole-plugin conversions have no loot
//!   chain to prune).
//! - Non-creature root type → no-op (only NPC_ / LVLN conversions produce the
//!   death-loot orphan pattern).
//!
//! # Root-sig set
//! BFS seeds from every record type present in the target plugin, not just the
//! prunable types. This prevents false-positive pruning of records reachable
//! via CELL, REFR, ACHR, etc.

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

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn referenced_prunable_record_is_kept() {
        let mut interner = StringInterner::new();

        let npc_fk = make_fk("000801", "Mod.esp", &mut interner);
        let lvli_fk = make_fk("000802", "Mod.esp", &mut interner);

        // NPC_ references the LVLI.
        let npc = make_record("NPC_", npc_fk, vec![lvli_fk], &mut interner);
        let lvli = make_record("LVLI", lvli_fk, vec![], &mut interner);

        let mut records = FxHashMap::default();
        records.insert(npc_fk, npc);
        records.insert(lvli_fk, lvli);
        let refs = build_refs(&records);

        let (orphans, _visited) = apply_to_records(&records, &refs);
        assert!(
            orphans.is_empty(),
            "LVLI referenced by NPC_ should not be pruned"
        );
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn unreferenced_prunable_record_is_dropped() {
        let mut interner = StringInterner::new();

        let npc_fk = make_fk("000801", "Mod.esp", &mut interner);
        let lvli_fk = make_fk("000802", "Mod.esp", &mut interner);

        // NPC_ does NOT reference the LVLI.
        let npc = make_record("NPC_", npc_fk, vec![], &mut interner);
        let lvli = make_record("LVLI", lvli_fk, vec![], &mut interner);

        let mut records = FxHashMap::default();
        records.insert(npc_fk, npc);
        records.insert(lvli_fk, lvli);
        let refs = build_refs(&records);

        let (orphans, _visited) = apply_to_records(&records, &refs);
        assert_eq!(orphans.len(), 1, "unreferenced LVLI should be pruned");
        assert_eq!(orphans[0], lvli_fk);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn non_prunable_record_never_dropped() {
        let mut interner = StringInterner::new();

        let npc_fk = make_fk("000801", "Mod.esp", &mut interner);
        let weap_fk = make_fk("000802", "Mod.esp", &mut interner);

        // Neither references the other.
        let npc = make_record("NPC_", npc_fk, vec![], &mut interner);
        let weap = make_record("WEAP", weap_fk, vec![], &mut interner);

        let mut records = FxHashMap::default();
        records.insert(npc_fk, npc);
        records.insert(weap_fk, weap);
        let refs = build_refs(&records);

        let (orphans, _visited) = apply_to_records(&records, &refs);
        assert!(
            orphans.is_empty(),
            "WEAP is not prunable, must not be dropped"
        );
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn transitive_chain_keeps_records_alive() {
        let mut interner = StringInterner::new();

        let npc_fk = make_fk("000801", "Mod.esp", &mut interner);
        let lvli_fk = make_fk("000802", "Mod.esp", &mut interner);
        let ammo_fk = make_fk("000803", "Mod.esp", &mut interner);

        // NPC_ → LVLI → AMMO (transitive chain)
        let npc = make_record("NPC_", npc_fk, vec![lvli_fk], &mut interner);
        let lvli = make_record("LVLI", lvli_fk, vec![ammo_fk], &mut interner);
        let ammo = make_record("AMMO", ammo_fk, vec![], &mut interner);

        let mut records = FxHashMap::default();
        records.insert(npc_fk, npc);
        records.insert(lvli_fk, lvli);
        records.insert(ammo_fk, ammo);
        let refs = build_refs(&records);

        let (orphans, _visited) = apply_to_records(&records, &refs);
        assert!(
            orphans.is_empty(),
            "transitively reachable LVLI/AMMO must not be pruned"
        );
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn empty_plugin_no_op() {
        let records: FxHashMap<FormKey, Record> = FxHashMap::default();
        let refs: FxHashMap<FormKey, FxHashSet<FormKey>> = FxHashMap::default();
        let (orphans, _) = apply_to_records(&records, &refs);
        assert!(orphans.is_empty());
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn collect_form_keys_recurses() {
        let mut interner = StringInterner::new();
        let fk1 = make_fk("000801", "Mod.esp", &mut interner);
        let fk2 = make_fk("000802", "Mod.esp", &mut interner);

        let nested = FieldValue::List(vec![
            FieldValue::Struct(vec![(interner.intern("ref"), FieldValue::FormKey(fk1))]),
            FieldValue::FormKey(fk2),
        ]);

        let mut out = FxHashSet::default();
        collect_form_keys(&nested, &mut out);
        assert!(out.contains(&fk1));
        assert!(out.contains(&fk2));
        assert_eq!(out.len(), 2);
    }

    // -----------------------------------------------------------------------
    //
    // When is_whole_plugin is set the fixup must be a complete no-op; the
    // applies_to gate handles this.
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_false_when_is_whole_plugin() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use std::sync::Arc;

        let mut interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        let root_sig = SigCode::from_str("NPC_").unwrap();
        let config = FixupConfig {
            is_whole_plugin: true,
            root_sig: Some(root_sig),
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

        let fixup = PruneOrphanedRecordsFixup;
        assert!(
            !fixup.applies_to(&ctx),
            "is_whole_plugin=true must make applies_to return false"
        );
    }

    // -----------------------------------------------------------------------
    //
    // WEAP is not a creature root type, so applies_to must return false
    // regardless of the record content.
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_false_for_non_creature_root() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use std::sync::Arc;

        let mut interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        let root_sig = SigCode::from_str("WEAP").unwrap();
        let config = FixupConfig {
            is_whole_plugin: false,
            root_sig: Some(root_sig),
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

        let fixup = PruneOrphanedRecordsFixup;
        assert!(
            !fixup.applies_to(&ctx),
            "WEAP root must not trigger orphan pruning"
        );
    }

    // -----------------------------------------------------------------------
    //
    // NPC_ is a creature root — applies_to must return true so the fixup runs.
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_true_for_npc_root() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use std::sync::Arc;

        let mut interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        let root_sig = SigCode::from_str("NPC_").unwrap();
        let config = FixupConfig {
            is_whole_plugin: false,
            root_sig: Some(root_sig),
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

        let fixup = PruneOrphanedRecordsFixup;
        assert!(
            fixup.applies_to(&ctx),
            "NPC_ root must enable the orphan pruning fixup"
        );
    }

    #[test]
    fn cell_root_keeps_referenced_lvli() {
        let mut interner = StringInterner::new();

        let cell_fk = make_fk("000801", "Mod.esp", &mut interner);
        let lvli_fk = make_fk("000802", "Mod.esp", &mut interner);

        // CELL references LVLI.
        let cell = make_record("CELL", cell_fk, vec![lvli_fk], &mut interner);
        let lvli = make_record("LVLI", lvli_fk, vec![], &mut interner);

        let mut records = FxHashMap::default();
        records.insert(cell_fk, cell);
        records.insert(lvli_fk, lvli);
        let refs = build_refs(&records);

        // With the dynamic BFS (CELL is a root because it's not prunable),
        // LVLI must be kept.
        let (orphans, _visited) = apply_to_records(&records, &refs);
        assert!(
            orphans.is_empty(),
            "LVLI reachable from CELL root must not be pruned (dynamic sig set)"
        );
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn is_creature_root_sig_coverage() {
        for sig_str in &["NPC_", "LVLN"] {
            let sig = SigCode::from_str(sig_str).unwrap();
            assert!(
                is_creature_root_sig(sig),
                "{sig_str} must be a creature root"
            );
        }
        for sig_str in &["WEAP", "ARMO", "CELL", "REFR", "ACHR", "RACE", "QUST"] {
            let sig = SigCode::from_str(sig_str).unwrap();
            assert!(
                !is_creature_root_sig(sig),
                "{sig_str} must NOT be a creature root"
            );
        }
    }
}
