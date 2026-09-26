//! Fixup: resolve FormKey references inside stub records injected during translation.
//!
//! Disabled: `applies_to` returns `false` for every conversion run. The per-record
//! algorithm lives in `apply_to_record`, which tests exercise directly.
//!
//! A "stub ref" is a FormKey in a translated record that still points at a
//! source-game plugin (e.g. `SeventySix.esm`) because its target was outside the
//! conversion walk. For each stale FK (skipping cycle-stack members and packed-data
//! FKs), the source record's EditorID and type are looked up, the
//! creature/creature-support/skip-type guards applied, and a stub is injected when
//! the mapper strategy is "new_allocation" or "source_id_preserved" and the FK is
//! not already in the graph. Remaining FormKeys are then rewritten through the mapper.

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

/// Pure per-record algorithm: decide what to do with each stale FK (a FormKey
/// string still pointing at a source-game plugin). FKs in `existing_source_fks` are
/// never re-injected; `stack` holds in-progress FKs for cycle prevention.
/// `lookup_record` returns `(editor_id, record_type)` for a source FK and
/// `map_formkey` its `(strategy, new_formkey)`. Returns one
/// `(source_fk, StubRefAction)` per stale FK, in sorted order.
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

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn stale_fk_guards_pick_the_first_matching_action() {
        type Strategy = fn(&str, &str, &str) -> (String, String);
        let fk = "000800:SeventySix.esm".to_string();
        let known = HashSet::from([fk.clone()]);
        let none = HashSet::new();
        let npc = Some(("TestEid", "NPC_"));
        let inject = StubRefAction::InjectStub {
            new_formkey: "000801:Output.esp".to_string(),
        };
        for (name, stack, existing, source, guards, strategy, expected) in [
            (
                "in stack",
                &known,
                &none,
                npc,
                [false; 4],
                local_strategy as Strategy,
                StubRefAction::InStack,
            ),
            (
                "packed data",
                &none,
                &none,
                npc,
                [true, false, false, false],
                local_strategy,
                StubRefAction::PackedData,
            ),
            (
                "source missing",
                &none,
                &none,
                None,
                [false; 4],
                local_strategy,
                StubRefAction::SourceNotUsable,
            ),
            (
                "empty editor id",
                &none,
                &none,
                Some(("", "NPC_")),
                [false; 4],
                local_strategy,
                StubRefAction::SourceNotUsable,
            ),
            (
                "creature race",
                &none,
                &none,
                Some(("TestRace", "RACE")),
                [false, true, false, false],
                local_strategy,
                StubRefAction::NullCreatureRace,
            ),
            (
                "creature support",
                &none,
                &none,
                Some(("TestCobj", "COBJ")),
                [false, false, true, false],
                local_strategy,
                StubRefAction::NullCreatureSupport,
            ),
            (
                "skipped type",
                &none,
                &none,
                Some(("TestEid", "SKIP")),
                [false, false, false, true],
                local_strategy,
                StubRefAction::SkipRecordType,
            ),
            (
                "non local strategy",
                &none,
                &none,
                npc,
                [false; 4],
                non_local_strategy,
                StubRefAction::NonLocalStrategy,
            ),
            (
                "local strategy injects",
                &none,
                &none,
                npc,
                [false; 4],
                local_strategy,
                inject.clone(),
            ),
            (
                "local strategy already known",
                &none,
                &known,
                npc,
                [false; 4],
                local_strategy,
                StubRefAction::NonLocalStrategy,
            ),
        ] {
            let [packed, race, support, skip] = guards;
            let results = apply_to_record(
                std::slice::from_ref(&fk),
                existing,
                stack,
                |_| source.and_then(|(eid, rt)| make_record(eid, rt)),
                |_| packed,
                |_, _| race,
                |_| support,
                |_| skip,
                strategy,
            );
            assert_eq!(results.len(), 1, "{name}");
            assert_eq!(results[0].1, expected, "{name}");
        }

        let results = apply_to_record(
            &[],
            &none,
            &none,
            |_| None,
            no_packed,
            no_creature_race,
            no_creature_support,
            no_skip,
            local_strategy,
        );
        assert!(results.is_empty());

        assert!(is_local_strategy("new_allocation"));
        assert!(is_local_strategy("source_id_preserved"));
        assert!(!is_local_strategy("direct_remap"));
        assert!(!is_local_strategy("null_ref"));
        assert!(!is_local_strategy(""));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------
}
