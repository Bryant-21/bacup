//! Fixup: resolve FormKey references inside stub records injected during translation.
//!

//!
//! # Current status
//! This fixup is a no-op: `applies_to` returns `false` in all current
//! conversion runs. The full per-record algorithm is implemented in
//! `apply_to_record` and is reachable from tests so it can be verified in
//! isolation when the gate is eventually enabled.
//!
//! # Algorithm (when enabled)
//! When `applies_to` becomes active for a given record type, the fixup:
//!
//! 1. Collects source-plugin names (source ESM + all plugins found in the graph).
//! 2. Scans the translated record for any FormKey strings still pointing to
//!    source-game plugins ("stale" FormKeys).
//! 3. For each stale FK (skipping cycle-stack members and packed-data FKs):
//!    a. Load the source record for that FK.
//!    b. Look up its EditorID and record_type; skip if either is absent.
//!    c. Apply creature/creature-support/skip-type guards (same as the main sweep).
//!    d. Query the FormKeyMapper for the target mapping.
//!    e. If the strategy is "new_allocation" or "source_id_preserved" AND the
//!       source FK is not already in the existing-source-FK set, inject a stub
//!       record for it (recursive, with cycle protection via the stack set).
//! 4. Rewrite all remaining FormKeys in the translated record using the mapper's
//!    current mapping table.
//!
//! # Stub-ref semantics
//! A "stub ref" is a FormKey embedded in a translated record that still points to
//! a source-game plugin (e.g. `SeventySix.esm`) because the referenced record was
//! not part of the original conversion walk.  This fixup discovers those dangling
//! pointers and, for records that should exist in the output, injects a minimal
//! translated stub so the ESP build can emit a real record for them.

use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Local strategies that warrant stub injection.
// ---------------------------------------------------------------------------

/// Returns `true` when `strategy` is one of the local-output strategies that
/// require a stub record to be injected into the target plugin.
pub fn is_local_strategy(strategy: &str) -> bool {
    matches!(strategy, "new_allocation" | "source_id_preserved")
}

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct ResolveInjectedStubRefsFixup;

impl Fixup for ResolveInjectedStubRefsFixup {
    fn name(&self) -> &'static str {
        "resolve_injected_stub_refs"
    }

    fn uses_session(&self) -> bool {
        true
    }

    /// Always `false` — this fixup is currently a no-op. When it is enabled for
    /// specific record types, update this predicate to match that record-type
    /// set.
    fn applies_to(&self, _ctx: &FixupContext) -> bool {
        false
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        false
    }

    fn run_with_session(
        &self,
        _session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        // `applies_to_session` is always false, so `run_with_session` is never
        // called in practice.
        // Return an empty report defensively.
        Ok(FixupReport::empty())
    }
}

// ---------------------------------------------------------------------------
// Per-record algorithm (extracted for testability)
// ---------------------------------------------------------------------------

/// Describes one stale FormKey found in a translated record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleRef {
    /// The FormKey string still pointing to a source-game plugin.
    pub source_fk: String,
    /// EditorID of the source record, if available.
    pub editor_id: Option<String>,
    /// Record type (4-byte sig) of the source record, if available.
    pub record_type: Option<String>,
}

/// Decision produced by `apply_to_record` for each stale FK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StubRefAction {
    /// FK is already in the existing-source set — no injection needed.
    AlreadyKnown,
    /// FK is in the cycle-prevention stack — skip.
    InStack,
    /// FK looks like packed binary data — skip.
    PackedData,
    /// Source record could not be loaded or had no EditorID/record_type — skip.
    SourceNotUsable,
    /// Creature-guard fired: unwalked creature race — caller should null this ref.
    NullCreatureRace,
    /// Creature-support guard fired — caller should null this ref.
    NullCreatureSupport,
    /// Record type is in the skip list — skip.
    SkipRecordType,
    /// Mapping strategy is not local; FK remapping is handled by the normal sweep.
    NonLocalStrategy,
    /// Stub injection is warranted; contains the new_formkey to allocate.
    InjectStub { new_formkey: String },
}

/// Pure per-record algorithm that decides what to do with each stale FK.
///
/// `stale_fks`: set of FormKey strings still pointing to source-game plugins.
/// `existing_source_fks`: set of source FKs already in the graph (no re-injection).
/// `stack`: cycle-prevention set (FK strings of in-progress records).
/// `lookup_record`: closure that returns `(editor_id, record_type)` for a source FK,
///                  or `None` if the record is not found.
/// `is_packed_data`: closure that tests whether a FK string is packed binary data.
/// `is_creature_race_unwalked`: closure — is this an unwalked creature race?
/// `is_creature_support`: closure — is this a creature-support record type?
/// `is_skip_type`: closure — is this record type in the skip list?
/// `map_formkey`: closure that returns `(strategy, new_formkey)` for a source FK.
///
/// Returns one `(source_fk, StubRefAction)` entry per stale FK, in sorted order.
pub fn apply_to_record<F1, F2, F3, F4, F5, F6>(
    stale_fks: &[String],
    existing_source_fks: &std::collections::HashSet<String>,
    stack: &std::collections::HashSet<String>,
    lookup_record: F1,
    is_packed_data: F2,
    is_creature_race_unwalked: F3,
    is_creature_support: F4,
    is_skip_type: F5,
    map_formkey: F6,
) -> Vec<(String, StubRefAction)>
where
    F1: Fn(&str) -> Option<(String, String)>, // (editor_id, record_type)
    F2: Fn(&str) -> bool,
    F3: Fn(&str, &str) -> bool, // (source_fk, record_type)
    F4: Fn(&str) -> bool,       // record_type
    F5: Fn(&str) -> bool,       // record_type
    F6: Fn(&str, &str, &str) -> (String, String), // (source_fk, editor_id, record_type) -> (strategy, new_formkey)
{
    let mut sorted: Vec<String> = stale_fks.to_vec();
    sorted.sort();

    let mut results = Vec::with_capacity(sorted.len());

    for fk in sorted {
        // Cycle-stack guard.
        if stack.contains(&fk) {
            results.push((fk, StubRefAction::InStack));
            continue;
        }

        // Packed-data guard.
        if is_packed_data(&fk) {
            results.push((fk, StubRefAction::PackedData));
            continue;
        }

        // Load source record metadata.
        let (editor_id, record_type) = match lookup_record(&fk) {
            Some(pair) => pair,
            None => {
                results.push((fk, StubRefAction::SourceNotUsable));
                continue;
            }
        };

        if editor_id.is_empty() || record_type.is_empty() {
            results.push((fk, StubRefAction::SourceNotUsable));
            continue;
        }

        // Creature-race guard.
        if is_creature_race_unwalked(&fk, &record_type) {
            results.push((fk, StubRefAction::NullCreatureRace));
            continue;
        }

        // Creature-support guard.
        if is_creature_support(&record_type) {
            results.push((fk, StubRefAction::NullCreatureSupport));
            continue;
        }

        // Skip-type guard.
        if is_skip_type(&record_type) {
            results.push((fk, StubRefAction::SkipRecordType));
            continue;
        }

        // FormKey mapping.
        let (strategy, new_fk) = map_formkey(&fk, &editor_id, &record_type);

        if is_local_strategy(&strategy) && !existing_source_fks.contains(&fk) {
            if !new_fk.is_empty() {
                results.push((
                    fk,
                    StubRefAction::InjectStub {
                        new_formkey: new_fk,
                    },
                ));
            } else {
                results.push((fk, StubRefAction::NonLocalStrategy));
            }
        } else {
            results.push((fk, StubRefAction::NonLocalStrategy));
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Helpers ----------------------------------------------------------------

    fn no_packed(_fk: &str) -> bool {
        false
    }

    fn no_creature_race(_fk: &str, _rt: &str) -> bool {
        false
    }

    fn no_creature_support(_rt: &str) -> bool {
        false
    }

    fn no_skip(_rt: &str) -> bool {
        false
    }

    fn local_strategy(_fk: &str, _eid: &str, _rt: &str) -> (String, String) {
        (
            "new_allocation".to_string(),
            "000801:Output.esp".to_string(),
        )
    }

    fn non_local_strategy(_fk: &str, _eid: &str, _rt: &str) -> (String, String) {
        (
            "direct_remap".to_string(),
            "000801:Fallout4.esm".to_string(),
        )
    }

    fn make_record(eid: &str, rt: &str) -> Option<(String, String)> {
        Some((eid.to_string(), rt.to_string()))
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn no_stale_fks_returns_empty() {
        let results = apply_to_record(
            &[],
            &HashSet::new(),
            &HashSet::new(),
            |_| None,
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );
        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn fk_in_stack_is_skipped() {
        let fk = "000800:SeventySix.esm".to_string();
        let mut stack = HashSet::new();
        stack.insert(fk.clone());

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(),
            &stack,
            |_| make_record("TestEid", "NPC_"),
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::InStack);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn packed_data_fk_is_skipped() {
        let fk = "AABBCC:SeventySix.esm".to_string();

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(),
            &HashSet::new(),
            |_| make_record("TestEid", "NPC_"),
            |_| true, // all FKs look like packed data
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::PackedData);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn source_record_not_found_is_not_usable() {
        let fk = "000800:SeventySix.esm".to_string();

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(),
            &HashSet::new(),
            |_| None, // not found
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::SourceNotUsable);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn empty_editor_id_is_not_usable() {
        let fk = "000800:SeventySix.esm".to_string();

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(),
            &HashSet::new(),
            |_| make_record("", "NPC_"),
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::SourceNotUsable);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn creature_race_guard_fires() {
        let fk = "000800:SeventySix.esm".to_string();

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(),
            &HashSet::new(),
            |_| make_record("TestRace", "RACE"),
            no_packed,
            |_, _| true, // always fires
            no_creature_support,
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::NullCreatureRace);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn creature_support_guard_fires() {
        let fk = "000800:SeventySix.esm".to_string();

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(),
            &HashSet::new(),
            |_| make_record("TestCobj", "COBJ"),
            no_packed,
            no_creature_race,
            |_| true, // always fires
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::NullCreatureSupport);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn skip_type_guard_fires() {
        let fk = "000800:SeventySix.esm".to_string();

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(),
            &HashSet::new(),
            |_| make_record("TestEid", "SKIP"),
            no_packed,
            no_creature_race,
            no_creature_support,
            |_| true, // always skip
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::SkipRecordType);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn non_local_strategy_no_injection() {
        let fk = "000800:SeventySix.esm".to_string();

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(),
            &HashSet::new(),
            |_| make_record("TestEid", "NPC_"),
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            non_local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::NonLocalStrategy);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn local_strategy_new_fk_injects_stub() {
        let fk = "000800:SeventySix.esm".to_string();

        let results = apply_to_record(
            &[fk.clone()],
            &HashSet::new(), // fk not already known
            &HashSet::new(),
            |_| make_record("TestEid", "NPC_"),
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].1,
            StubRefAction::InjectStub {
                new_formkey: "000801:Output.esp".to_string()
            }
        );
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn local_strategy_already_known_no_injection() {
        let fk = "000800:SeventySix.esm".to_string();
        let mut existing = HashSet::new();
        existing.insert(fk.clone());

        let results = apply_to_record(
            &[fk.clone()],
            &existing,
            &HashSet::new(),
            |_| make_record("TestEid", "NPC_"),
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, StubRefAction::NonLocalStrategy);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_fks_sorted_order() {
        let fks = vec![
            "CC0003:SeventySix.esm".to_string(),
            "AA0001:SeventySix.esm".to_string(),
            "BB0002:SeventySix.esm".to_string(),
        ];

        let results = apply_to_record(
            &fks,
            &HashSet::new(),
            &HashSet::new(),
            |_| make_record("SomeEid", "NPC_"),
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );

        assert_eq!(results.len(), 3);
        // Sorted: AA < BB < CC.
        assert!(results[0].0.starts_with("AA"));
        assert!(results[1].0.starts_with("BB"));
        assert!(results[2].0.starts_with("CC"));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn is_local_strategy_recognises_known_values() {
        assert!(is_local_strategy("new_allocation"));
        assert!(is_local_strategy("source_id_preserved"));
        assert!(!is_local_strategy("direct_remap"));
        assert!(!is_local_strategy("null_ref"));
        assert!(!is_local_strategy(""));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_is_always_false() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use crate::sym::StringInterner;

        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig::default();
        let mut interner = StringInterner::new();

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

        let fixup = ResolveInjectedStubRefsFixup;
        assert!(
            !fixup.applies_to(&ctx),
            "applies_to must return false until the Python gate is lifted"
        );
    }
}
