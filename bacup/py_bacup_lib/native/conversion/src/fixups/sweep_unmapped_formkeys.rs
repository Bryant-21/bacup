//! Fixup: replace FormKey references that still point at source-game plugins.
//!
//! # Decision tree (per source-plugin FK leaf)
//! 1. **packed data** — local id matches the `is_packed_data_fk_hex` pattern → null.
//! 2. **unreadable source record** — `read_record` fails on the source handle → null.
//! 3. **missing EID** — source record decodes but has no EditorID → null + warn.
//! 4. **creature snowball, off-graph race** — creature root + RACE sig + no mapping
//!    yet → null.
//! 5. **creature snowball, COBJ support** — creature root + COBJ sig → null.
//! 6. **skip-record-type** — sig is in `translator.maps.skip_records` → null.
//! 7. **resolution attempt** — `mapper.lookup` first; if non-creature root and
//!    nothing found, try `mapper.find_vanilla_fk(eid, sig)`, including known
//!    compatible target signatures such as FO76 `CNCY` → FO4 `MISC`.
//! 8. **weapon snowball gate** — non-creature root + sig in
//!    `SWEEP_NULL_TYPES_FOR_WEAPON_ROOTS` + not a base-master vanilla_remap → null.
//! 9. **apply resolution** — overwrite FK with resolved target FK.
//! 10. **stub injection** — call the supplied stub-injector (production:
//!     `stub_injection::inject_minimal_stub`) with a target-compatible signature.
//!     On success, overwrite the FK with the freshly-allocated target FK; on
//!     failure (or a no-op injector), null with a warning so output stays valid.
//!
//! Not convergent: runs a single pass.

use std::sync::OnceLock;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::stub_injection::{inject_minimal_stub, inject_minimal_stub_with_session};
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FullPluginRunState;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::source_read::{form_key_to_read_str, read_record};
use crate::sym::{StringInterner, Sym};

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct SweepUnmappedFormKeysFixup;

impl Fixup for SweepUnmappedFormKeysFixup {
    fn name(&self) -> &'static str {
        "_sweep_unmapped_formkeys"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, _ctx: &FixupContext) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn convergent(&self) -> bool {
        false
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let source_schema = session
            .source_schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let source_plugin_name = match session.source_slot_opt() {
            Some(slot) => slot.parsed.plugin_name.clone(),
            None => return Ok(report),
        };
        let source_plugin_sym = mapper.interner.intern(&source_plugin_name);
        let null_plugin_sym = mapper.interner.intern("__null__");

        // Resolve target master plugin names once so the weapon snowball gate
        // can check "remap targets a master" cheaply.
        let target_master_names = session.target_masters().to_vec();

        // Pre-filter: a record can only contain a FormID pointing at the source
        // plugin if (a) source_plugin appears in target's master list, and (b)
        // the record's raw bytes contain that master index as a byte value
        // somewhere. If the source plugin isn't a target master, the entire
        // sweep is a no-op — after translate_all rewrote the cross-plugin refs,
        // there's nothing left for sweep to find.
        let source_master_byte: Option<u8> = target_master_names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&source_plugin_name))
            .and_then(|idx| u8::try_from(idx).ok());
        if source_master_byte.is_none() {
            return Ok(report);
        }
        let source_master_byte = source_master_byte.unwrap();

        let creature_root = config.root_sig.map(is_creature_root_type).unwrap_or(false);

        let sigs = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for sig in sigs {
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;

            for record_fk in fks {
                // Fast skip: if the record's raw subrecord bytes don't contain
                // the source master index byte anywhere, it can't contain a
                // source-plugin FormID — no decode needed. Massive win on the
                // 5M-record case where most records have no source-plugin refs.
                match session.record_bytes_contain_byte(&record_fk, source_master_byte) {
                    Ok(false) => continue,
                    Ok(true) => {}
                    Err(_) => {
                        // Fall through to existing decode path; it will surface
                        // a proper warning if the record really is missing.
                    }
                }

                let mut record = match session.record_decoded(
                    &record_fk,
                    target_schema.as_ref(),
                    mapper.interner,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper.interner.intern(&format!("sweep_read_err:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };

                let outcome = apply_to_record_in_session(
                    &mut record,
                    source_plugin_sym,
                    null_plugin_sym,
                    true,
                    None,
                    None,
                    session,
                    source_schema.as_ref(),
                    &config.skip_record_sigs,
                    &target_master_names,
                    creature_root,
                    config.defer_placed_child_ref_class,
                    mapper,
                );

                if outcome.changed {
                    session
                        .replace_record(record, target_schema.as_ref(), mapper.interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    report.records_changed += 1;
                }
                report.records_dropped += outcome.nulled;
                for w in outcome.warnings {
                    report.warnings.push(w);
                }
            }
        }

        Ok(report)
    }

    fn run_full_plugin_worklist(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
        state: &FullPluginRunState,
    ) -> Result<Option<FixupReport>, FixupError> {
        Ok(Some(SweepUnmappedFormKeysFixup::run_full_plugin_worklist(
            self, session, mapper, config, state,
        )?))
    }
}

impl SweepUnmappedFormKeysFixup {
    pub fn run_full_plugin_worklist(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
        state: &crate::full_plugin::FullPluginRunState,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let source_plugin_name = match session.source_slot_opt() {
            Some(slot) => slot.parsed.plugin_name.clone(),
            None => return Ok(report),
        };
        let source_plugin_sym = mapper.interner.intern(&source_plugin_name);
        let null_plugin_sym = mapper.interner.intern("__null__");
        let target_master_names = session.target_masters().to_vec();
        let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let output_plugin_sym = mapper.interner.intern(&output_plugin_name);
        let use_source_record_decision_tree = use_source_record_decision_tree(
            &source_plugin_name,
            &output_plugin_name,
            &target_master_names,
        );
        let source_schema = if use_source_record_decision_tree {
            Some(
                session
                    .source_schema()
                    .map_err(|e| FixupError::HandleError(e.to_string()))?,
            )
        } else {
            None
        };
        let target_handle_id = session.target_id();
        let mut target_contains_ref = |fk: FormKey| {
            let fk_str = form_key_to_read_str(&fk, mapper.interner);
            if fk_str.is_empty() {
                return false;
            }
            session
                .record_exists_in_handle(target_handle_id, &fk_str)
                .unwrap_or(false)
        };
        let unresolved_refs_by_owner = unresolved_source_refs_by_owner_with_target_check(
            state,
            mapper,
            &mut target_contains_ref,
        );
        let creature_root = config.root_sig.map(is_creature_root_type).unwrap_or(false);

        let supplemental_cobj_owners = if use_source_record_decision_tree {
            session
                .form_keys_of_sig(
                    SigCode::from_str("COBJ").expect("COBJ signature"),
                    mapper.interner,
                )
                .map_err(|e| FixupError::HandleError(e.to_string()))?
        } else {
            Vec::new()
        };

        let defer_placed_child = config.defer_placed_child_ref_class;
        let worklist_owners =
            sweep_worklist_owners(&unresolved_refs_by_owner, supplemental_cobj_owners);

        if !use_source_record_decision_tree {
            use rayon::prelude::*;

            const SWEEP_CHUNK_SIZE: usize = 65_536;

            let owner_count = worklist_owners.len();
            let mut deferred_skipped = 0u32;
            eprintln!(
                "[fixups_v2] sweep worklist owners={owner_count} mode=parallel chunk_size={SWEEP_CHUNK_SIZE}"
            );

            // A read view borrows the session, so each bounded chunk is fully
            // inspected before its changed records are batch-written.
            for (chunk_index, owner_chunk) in worklist_owners.chunks(SWEEP_CHUNK_SIZE).enumerate() {
                let inspected = {
                    let view = session
                        .target_read_view()
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    let inspect = |record_fk: &FormKey| {
                        let mut record = match view.record_decoded(
                            record_fk,
                            target_schema.as_ref(),
                            mapper.interner,
                        ) {
                            Ok(record) => record,
                            Err(error) => {
                                return (
                                    None,
                                    0,
                                    0,
                                    Vec::new(),
                                    Some(format!("sweep_worklist_read_err:{error}")),
                                );
                            }
                        };

                        let deferred = if defer_placed_child {
                            let record_sig = record.sig.as_str();
                            record
                                .fields
                                .iter()
                                .filter(|entry| {
                                    crate::fixups::null_dangling_own_plugin_refs::is_deferred_placed_child(
                                        record_sig,
                                        entry.sig.as_str(),
                                    )
                                })
                                .count() as u32
                        } else {
                            0
                        };
                        let outcome = apply_known_unresolved_refs_to_record(
                            &mut record,
                            source_plugin_sym,
                            null_plugin_sym,
                            Some(output_plugin_sym),
                            unresolved_refs_by_owner.get(record_fk),
                            defer_placed_child,
                            mapper,
                        );

                        (
                            outcome.changed.then_some(record),
                            outcome.nulled,
                            deferred,
                            outcome.warnings,
                            None,
                        )
                    };

                    if owner_chunk.len() < 64 {
                        owner_chunk.iter().map(inspect).collect::<Vec<_>>()
                    } else {
                        owner_chunk.par_iter().map(inspect).collect::<Vec<_>>()
                    }
                };

                let mut changed_records = Vec::new();
                for (changed_record, nulled, deferred, warnings, read_error) in inspected {
                    if let Some(message) = read_error {
                        report.warnings.push(mapper.interner.intern(&message));
                        continue;
                    }
                    changed_records.extend(changed_record);
                    report.records_dropped += nulled;
                    deferred_skipped += deferred;
                    report.warnings.extend(warnings);
                }

                let replaced = session
                    .replace_records_contents(
                        changed_records,
                        target_schema.as_ref(),
                        mapper.interner,
                    )
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                report.records_changed += replaced.try_into().unwrap_or(u32::MAX);

                let processed = ((chunk_index + 1) * SWEEP_CHUNK_SIZE).min(owner_count);
                eprintln!(
                    "[fixups_v2] sweep progress owners={processed}/{owner_count} changed={} nulled={}",
                    report.records_changed, report.records_dropped
                );
            }

            eprintln!(
                "[trace_defer] sweep: defer_placed_child={defer_placed_child} deferred_skipped={deferred_skipped}"
            );
            return Ok(report);
        }

        // Diagnostic count of LCTN LCEP/ACEP/LCUN entries left untouched by the
        // placed-child deferral (logged below; no behaviour).
        let mut deferred_skipped = 0u32;
        for record_fk in worklist_owners {
            let mut record =
                match session.record_decoded(&record_fk, target_schema.as_ref(), mapper.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("sweep_worklist_read_err:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };

            if defer_placed_child {
                let record_sig = record.sig.as_str();
                deferred_skipped += record
                    .fields
                    .iter()
                    .filter(|e| {
                        crate::fixups::null_dangling_own_plugin_refs::is_deferred_placed_child(
                            record_sig,
                            e.sig.as_str(),
                        )
                    })
                    .count() as u32;
            }

            let outcome = apply_to_record_in_session(
                &mut record,
                source_plugin_sym,
                null_plugin_sym,
                false,
                Some(output_plugin_sym),
                unresolved_refs_by_owner.get(&record_fk),
                session,
                source_schema
                    .as_ref()
                    .expect("source schema for decision tree"),
                &config.skip_record_sigs,
                &target_master_names,
                creature_root,
                defer_placed_child,
                mapper,
            );

            if outcome.changed {
                let replaced = session
                    .replace_record_contents(record, target_schema.as_ref(), mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                if replaced {
                    report.records_changed += 1;
                }
            }
            report.records_dropped += outcome.nulled;
            report.warnings.extend(outcome.warnings);
        }

        eprintln!(
            "[trace_defer] sweep: defer_placed_child={defer_placed_child} deferred_skipped={deferred_skipped}"
        );

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Per-record decision tree
// ---------------------------------------------------------------------------

fn sweep_worklist_owners(
    unresolved_refs_by_owner: &FxHashMap<FormKey, FxHashSet<FormKey>>,
    supplemental_owners: impl IntoIterator<Item = FormKey>,
) -> Vec<FormKey> {
    let mut seen = FxHashSet::default();
    let mut owners = Vec::new();
    for owner in unresolved_refs_by_owner.keys() {
        if seen.insert(*owner) {
            owners.push(*owner);
        }
    }
    for owner in supplemental_owners {
        if seen.insert(owner) {
            owners.push(owner);
        }
    }
    owners
}

#[cfg(test)]
fn unresolved_source_refs_by_owner(
    state: &FullPluginRunState,
    mapper: &FormKeyMapper,
) -> FxHashMap<FormKey, FxHashSet<FormKey>> {
    unresolved_source_refs_by_owner_with_target_check(state, mapper, &mut |_| true)
}

fn unresolved_source_refs_by_owner_with_target_check(
    state: &FullPluginRunState,
    mapper: &FormKeyMapper,
    target_contains_ref: &mut dyn FnMut(FormKey) -> bool,
) -> FxHashMap<FormKey, FxHashSet<FormKey>> {
    let mut by_owner: FxHashMap<FormKey, FxHashSet<FormKey>> = FxHashMap::default();
    for (source_ref, ref_owners) in &state.unresolved_source_ref_owners {
        if mapper.lookup(*source_ref) == Some(*source_ref) && target_contains_ref(*source_ref) {
            continue;
        }
        for ref_owner in ref_owners {
            by_owner
                .entry(ref_owner.owner)
                .or_default()
                .insert(*source_ref);
        }
    }
    by_owner
}

fn source_plugin_is_target_master(
    source_plugin_name: &str,
    target_master_names: &[String],
) -> bool {
    target_master_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(source_plugin_name))
}

fn use_source_record_decision_tree(
    source_plugin_name: &str,
    output_plugin_name: &str,
    target_master_names: &[String],
) -> bool {
    source_plugin_is_target_master(source_plugin_name, target_master_names)
        && !output_plugin_name.eq_ignore_ascii_case(source_plugin_name)
}

#[derive(Default)]
pub struct SweepOutcome {
    pub changed: bool,
    pub nulled: u32,
    pub warnings: Vec<Sym>,
}

#[allow(clippy::too_many_arguments)]
fn apply_to_record_in_session(
    record: &mut Record,
    source_plugin_sym: Sym,
    null_plugin_sym: Sym,
    allow_stub_injection: bool,
    output_plugin_sym: Option<Sym>,
    unresolved_source_refs: Option<&FxHashSet<FormKey>>,
    session: &mut PluginSession,
    schema_source: &AuthoringSchema,
    skip_record_sigs: &FxHashSet<String>,
    target_master_names: &[String],
    creature_root: bool,
    defer_placed_child: bool,
    mapper: &mut FormKeyMapper,
) -> SweepOutcome {
    let session = std::cell::RefCell::new(session);
    let lookup = |fk: &FormKey, interner: &StringInterner| -> Option<SourceInfo> {
        let mut session = session.borrow_mut();
        match session.source_record_decoded(fk, schema_source, interner) {
            Ok(r) => Some(SourceInfo {
                sig: r.sig,
                eid: r
                    .eid
                    .and_then(|s| interner.resolve(s))
                    .map(|s| s.to_string()),
            }),
            Err(_) => None,
        }
    };
    let inject = |source_fk: FormKey,
                  editor_id: &str,
                  sig: SigCode,
                  mapper: &mut FormKeyMapper|
     -> Option<FormKey> {
        if !allow_stub_injection {
            return None;
        }
        let mut session = session.borrow_mut();
        let target_sig = target_sig_for_source_sig(sig);
        inject_minimal_stub_with_session(&mut session, source_fk, editor_id, target_sig, mapper)
            .ok()
    };

    apply_to_record_with_persisted_source_refs(
        record,
        source_plugin_sym,
        null_plugin_sym,
        output_plugin_sym,
        unresolved_source_refs,
        &lookup,
        &inject,
        skip_record_sigs,
        target_master_names,
        creature_root,
        defer_placed_child,
        mapper,
    )
}

/// Pure-ish entry point used directly by tests. Walks every FK leaf in `record`
/// whose plugin sym matches `source_plugin_sym` and applies the decision tree.
///
/// Returns the per-record outcome (mutations applied in place).
///
/// `source_handle_id` / `target_handle_id` route through real plugin handles
/// for the lookup and stub-injection paths. Tests that don't want real handles
/// use `apply_to_record_with` instead.
#[allow(clippy::too_many_arguments)]
pub fn apply_to_record(
    record: &mut Record,
    source_plugin_sym: Sym,
    null_plugin_sym: Sym,
    source_handle_id: u64,
    target_handle_id: u64,
    schema_source: &AuthoringSchema,
    schema_target: &AuthoringSchema,
    skip_record_sigs: &FxHashSet<String>,
    target_master_names: &[String],
    creature_root: bool,
    mapper: &mut FormKeyMapper,
) -> SweepOutcome {
    let lookup = |fk: &FormKey, interner: &StringInterner| -> Option<SourceInfo> {
        let fk_str = form_key_to_read_str(fk, interner);
        if fk_str.is_empty() {
            return None;
        }
        match read_record(source_handle_id, &fk_str, schema_source, interner) {
            Ok(r) => Some(SourceInfo {
                sig: r.sig,
                eid: r
                    .eid
                    .and_then(|s| interner.resolve(s))
                    .map(|s| s.to_string()),
            }),
            Err(_) => None,
        }
    };
    let inject = |source_fk: FormKey,
                  editor_id: &str,
                  sig: SigCode,
                  m: &mut FormKeyMapper|
     -> Option<FormKey> {
        let target_sig = target_sig_for_source_sig(sig);
        inject_minimal_stub(
            target_handle_id,
            source_fk,
            editor_id,
            target_sig,
            m,
            schema_target,
        )
        .ok()
    };
    apply_to_record_with(
        record,
        source_plugin_sym,
        null_plugin_sym,
        &lookup,
        &inject,
        skip_record_sigs,
        target_master_names,
        creature_root,
        mapper,
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_to_record_with_persisted_source_refs<F, I>(
    record: &mut Record,
    source_plugin_sym: Sym,
    null_plugin_sym: Sym,
    output_plugin_sym: Option<Sym>,
    unresolved_source_refs: Option<&FxHashSet<FormKey>>,
    source_lookup: &F,
    stub_injector: &I,
    skip_record_sigs: &FxHashSet<String>,
    target_master_names: &[String],
    creature_root: bool,
    defer_placed_child: bool,
    mapper: &mut FormKeyMapper,
) -> SweepOutcome
where
    F: Fn(&FormKey, &StringInterner) -> Option<SourceInfo>,
    I: Fn(FormKey, &str, SigCode, &mut FormKeyMapper) -> Option<FormKey>,
{
    let mut outcome = SweepOutcome::default();
    let record_sig = record.sig.as_str().to_string();
    for field in record.fields.iter_mut() {
        // Whole-plugin FO76→FO4: leave the placed-ref-target class (LCTN
        // LCEP/ACEP/LCUN) untouched. Its targets (exterior ACHR/REFR) are in
        // `skip_record_sigs`, so rule 6 would null these leaves before the phase-6
        // cell-slice copy re-inserts them after all fixups; the post-copy
        // `null_dangling_own_plugin_refs::repair_placed_child_refs` resolves them.
        // This sweep runs first (before fix_invalid + null_dangling), so this is the
        // real null site.
        if defer_placed_child
            && crate::fixups::null_dangling_own_plugin_refs::is_deferred_placed_child(
                &record_sig,
                field.sig.as_str(),
            )
        {
            continue;
        }
        walk_field_value(
            &mut field.value,
            source_plugin_sym,
            null_plugin_sym,
            output_plugin_sym,
            unresolved_source_refs,
            source_lookup,
            stub_injector,
            skip_record_sigs,
            target_master_names,
            creature_root,
            mapper,
            &mut outcome,
        );
    }
    outcome
}

/// Info pulled out of the source DB for a stale FormKey reference.
#[derive(Clone, Debug)]
pub struct SourceInfo {
    pub sig: SigCode,
    pub eid: Option<String>,
}

/// Decision-tree entry point with injectable source-record lookup and
/// stub-injector closures. Tests stub both closures to avoid needing real
/// plugin handles.
///
/// `stub_injector` returns `Some(new_target_fk)` when injection succeeds (the
/// FK leaf is overwritten with it), or `None` when injection is unavailable
/// (the leaf is nulled with a `sweep:stub_unavailable` warning).
#[allow(clippy::too_many_arguments)]
pub fn apply_to_record_with<F, I>(
    record: &mut Record,
    source_plugin_sym: Sym,
    null_plugin_sym: Sym,
    source_lookup: &F,
    stub_injector: &I,
    skip_record_sigs: &FxHashSet<String>,
    target_master_names: &[String],
    creature_root: bool,
    mapper: &mut FormKeyMapper,
) -> SweepOutcome
where
    F: Fn(&FormKey, &StringInterner) -> Option<SourceInfo>,
    I: Fn(FormKey, &str, SigCode, &mut FormKeyMapper) -> Option<FormKey>,
{
    apply_to_record_with_persisted_source_refs(
        record,
        source_plugin_sym,
        null_plugin_sym,
        None,
        None,
        source_lookup,
        stub_injector,
        skip_record_sigs,
        target_master_names,
        creature_root,
        // Public per-record entry (graph path / tests): no worldspace deferral.
        false,
        mapper,
    )
}

fn apply_known_unresolved_refs_to_record(
    record: &mut Record,
    source_plugin_sym: Sym,
    null_plugin_sym: Sym,
    output_plugin_sym: Option<Sym>,
    unresolved_source_refs: Option<&FxHashSet<FormKey>>,
    defer_placed_child: bool,
    mapper: &FormKeyMapper,
) -> SweepOutcome {
    let Some(unresolved_source_refs) = unresolved_source_refs else {
        return SweepOutcome::default();
    };

    let mut outcome = SweepOutcome::default();
    let record_sig = record.sig.as_str().to_string();
    for field in record.fields.iter_mut() {
        // See `apply_to_record_with_persisted_source_refs`: defer the placed-child
        // class (LCTN LCEP/ACEP/LCUN) to the post-copy repair.
        if defer_placed_child
            && crate::fixups::null_dangling_own_plugin_refs::is_deferred_placed_child(
                &record_sig,
                field.sig.as_str(),
            )
        {
            continue;
        }
        walk_known_unresolved_refs(
            &mut field.value,
            source_plugin_sym,
            null_plugin_sym,
            output_plugin_sym,
            unresolved_source_refs,
            mapper,
            &mut outcome,
        );
    }
    outcome
}

fn walk_known_unresolved_refs(
    value: &mut FieldValue,
    source_plugin_sym: Sym,
    null_plugin_sym: Sym,
    output_plugin_sym: Option<Sym>,
    unresolved_source_refs: &FxHashSet<FormKey>,
    mapper: &FormKeyMapper,
    outcome: &mut SweepOutcome,
) {
    match value {
        FieldValue::FormKey(fk) => {
            let Some(source_fk) = source_fk_for_sweep(
                *fk,
                source_plugin_sym,
                output_plugin_sym,
                Some(unresolved_source_refs),
            ) else {
                return;
            };
            let replacement = mapper
                .lookup(source_fk)
                .filter(|mapped| *mapped != source_fk);
            let replacement = replacement.unwrap_or(FormKey {
                local: 0,
                plugin: null_plugin_sym,
            });
            if *fk != replacement {
                *fk = replacement;
                outcome.changed = true;
                if replacement.local == 0 && replacement.plugin == null_plugin_sym {
                    outcome.nulled += 1;
                }
            }
        }
        FieldValue::List(items) => {
            for item in items.iter_mut() {
                walk_known_unresolved_refs(
                    item,
                    source_plugin_sym,
                    null_plugin_sym,
                    output_plugin_sym,
                    unresolved_source_refs,
                    mapper,
                    outcome,
                );
            }
        }
        FieldValue::Struct(fields) => {
            for (_, item) in fields.iter_mut() {
                walk_known_unresolved_refs(
                    item,
                    source_plugin_sym,
                    null_plugin_sym,
                    output_plugin_sym,
                    unresolved_source_refs,
                    mapper,
                    outcome,
                );
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_field_value<F, I>(
    value: &mut FieldValue,
    source_plugin_sym: Sym,
    null_plugin_sym: Sym,
    output_plugin_sym: Option<Sym>,
    unresolved_source_refs: Option<&FxHashSet<FormKey>>,
    source_lookup: &F,
    stub_injector: &I,
    skip_record_sigs: &FxHashSet<String>,
    target_master_names: &[String],
    creature_root: bool,
    mapper: &mut FormKeyMapper,
    outcome: &mut SweepOutcome,
) where
    F: Fn(&FormKey, &StringInterner) -> Option<SourceInfo>,
    I: Fn(FormKey, &str, SigCode, &mut FormKeyMapper) -> Option<FormKey>,
{
    match value {
        FieldValue::FormKey(fk) => {
            let Some(mut source_fk) = source_fk_for_sweep(
                *fk,
                source_plugin_sym,
                output_plugin_sym,
                unresolved_source_refs,
            ) else {
                return;
            };
            let original_source_fk = source_fk;
            decide_fk(
                &mut source_fk,
                null_plugin_sym,
                source_lookup,
                stub_injector,
                skip_record_sigs,
                target_master_names,
                creature_root,
                mapper,
                outcome,
            );
            if source_fk != original_source_fk {
                *fk = source_fk;
            }
        }
        FieldValue::List(items) => {
            for item in items.iter_mut() {
                walk_field_value(
                    item,
                    source_plugin_sym,
                    null_plugin_sym,
                    output_plugin_sym,
                    unresolved_source_refs,
                    source_lookup,
                    stub_injector,
                    skip_record_sigs,
                    target_master_names,
                    creature_root,
                    mapper,
                    outcome,
                );
            }
        }
        FieldValue::Struct(fields) => {
            for (_, v) in fields.iter_mut() {
                walk_field_value(
                    v,
                    source_plugin_sym,
                    null_plugin_sym,
                    output_plugin_sym,
                    unresolved_source_refs,
                    source_lookup,
                    stub_injector,
                    skip_record_sigs,
                    target_master_names,
                    creature_root,
                    mapper,
                    outcome,
                );
            }
        }
        _ => {}
    }
}

fn source_fk_for_sweep(
    fk: FormKey,
    source_plugin_sym: Sym,
    output_plugin_sym: Option<Sym>,
    unresolved_source_refs: Option<&FxHashSet<FormKey>>,
) -> Option<FormKey> {
    if Some(fk.plugin) == output_plugin_sym {
        let source_fk = FormKey {
            local: fk.local,
            plugin: source_plugin_sym,
        };
        if let Some(refs) = unresolved_source_refs {
            return refs.contains(&source_fk).then_some(source_fk);
        }
    }

    if fk.plugin == source_plugin_sym {
        return Some(fk);
    }

    if Some(fk.plugin) != output_plugin_sym {
        return None;
    }

    let source_fk = FormKey {
        local: fk.local,
        plugin: source_plugin_sym,
    };
    unresolved_source_refs
        .is_some_and(|refs| refs.contains(&source_fk))
        .then_some(source_fk)
}

/// Run the 10-branch decision tree on a single FK leaf and mutate it in place.
#[allow(clippy::too_many_arguments)]
fn decide_fk<F, I>(
    fk: &mut FormKey,
    null_plugin_sym: Sym,
    source_lookup: &F,
    stub_injector: &I,
    skip_record_sigs: &FxHashSet<String>,
    target_master_names: &[String],
    creature_root: bool,
    mapper: &mut FormKeyMapper,
    outcome: &mut SweepOutcome,
) where
    F: Fn(&FormKey, &StringInterner) -> Option<SourceInfo>,
    I: Fn(FormKey, &str, SigCode, &mut FormKeyMapper) -> Option<FormKey>,
{
    let null_fk = FormKey {
        local: 0,
        plugin: null_plugin_sym,
    };

    // Branch 1: packed data → null.
    if is_packed_data_fk_hex(fk.local) {
        *fk = null_fk;
        outcome.changed = true;
        outcome.nulled += 1;
        return;
    }

    // Branch 2: try to read source record.
    let original_fk = *fk;
    let info = match source_lookup(&original_fk, mapper.interner) {
        Some(i) => i,
        None => {
            let w = mapper
                .interner
                .intern(&format!("sweep:unreadable {:06X}", original_fk.local));
            outcome.warnings.push(w);
            *fk = null_fk;
            outcome.changed = true;
            outcome.nulled += 1;
            return;
        }
    };

    let eid = info.eid.unwrap_or_default();
    let src_sig = info.sig;

    // Branch 3: source record had no EID.
    if eid.is_empty() {
        let w = mapper.interner.intern(&format!(
            "sweep:no_eid {:06X} {}",
            original_fk.local,
            src_sig.as_str()
        ));
        outcome.warnings.push(w);
        *fk = null_fk;
        outcome.changed = true;
        outcome.nulled += 1;
        return;
    }

    // Branch 4: creature snowball, off-graph race.
    // Walked-set approximation: a record translated by the pipeline got into
    // mapper.source_to_target. Records reached only as transitive references
    // that didn't translate won't be there. So absence ≈ "not walked".
    if creature_root && is_race_type(src_sig) && mapper.lookup(original_fk).is_none() {
        *fk = null_fk;
        outcome.changed = true;
        outcome.nulled += 1;
        return;
    }

    // Branch 5: creature snowball, COBJ support records.
    if creature_root && is_constructible_object_type(src_sig) {
        *fk = null_fk;
        outcome.changed = true;
        outcome.nulled += 1;
        return;
    }

    // Branch 6: skip-record-type.
    if skip_record_sigs.contains(src_sig.as_str()) {
        *fk = null_fk;
        outcome.changed = true;
        outcome.nulled += 1;
        return;
    }

    // Branch 7: resolution attempt.
    let mut resolved: Option<FormKey> = mapper.lookup(original_fk);
    if !creature_root && resolved.is_none() {
        if let Some(vfk) = find_vanilla_fk_with_compatible_sig(mapper, &eid, src_sig) {
            resolved = Some(vfk);
        }
    }

    // Branch 8: weapon snowball gate.
    if !creature_root && is_sweep_null_type_for_weapon_root(src_sig) {
        let is_base_master_remap = match resolved {
            Some(rfk) => is_target_master(rfk.plugin, target_master_names, mapper.interner),
            None => false,
        };
        if !is_base_master_remap {
            *fk = null_fk;
            outcome.changed = true;
            outcome.nulled += 1;
            return;
        }
    }

    // Branch 9: apply resolution.
    if let Some(rfk) = resolved {
        *fk = rfk;
        outcome.changed = true;
        return;
    }

    // Branch 10: no resolution. Try stub injection with a target-compatible
    // signature. On success the FK becomes the freshly allocated target FK and the
    // source→target mapping is registered on the mapper; on failure (no-op
    // injector or insert error) the leaf is nulled with a warning.
    let stub_sig = target_sig_for_source_sig(src_sig);
    if let Some(new_fk) = stub_injector(original_fk, &eid, stub_sig, mapper) {
        *fk = new_fk;
        outcome.changed = true;
        let w = mapper.interner.intern(&format!(
            "sweep:stub_injected {} ({})",
            eid,
            src_sig.as_str()
        ));
        outcome.warnings.push(w);
        return;
    }

    let w = mapper.interner.intern(&format!(
        "sweep:stub_unavailable {} ({})",
        eid,
        src_sig.as_str()
    ));
    outcome.warnings.push(w);
    *fk = null_fk;
    outcome.changed = true;
    outcome.nulled += 1;
}

fn find_vanilla_fk_with_compatible_sig(
    mapper: &mut FormKeyMapper,
    eid: &str,
    src_sig: SigCode,
) -> Option<FormKey> {
    mapper
        .find_vanilla_fk(eid, src_sig)
        .or_else(|| compatible_target_sig(src_sig).and_then(|sig| mapper.find_vanilla_fk(eid, sig)))
}

fn compatible_target_sig(src_sig: SigCode) -> Option<SigCode> {
    match src_sig.as_str() {
        "CNCY" => SigCode::from_str("MISC").ok(),
        _ => None,
    }
}

fn target_sig_for_source_sig(src_sig: SigCode) -> SigCode {
    compatible_target_sig(src_sig).unwrap_or(src_sig)
}

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

///
/// Bethesda YAML serialization can emit FormKey-like strings from packed DNAM
/// subrecords. Detect them by hex pattern so we null-replace cleanly.
fn is_packed_data_fk_hex(local: u32) -> bool {
    // Python checks the 6-hex-char `hex_part`. Local ids are 24 bits.
    let hex_part = format!("{:06X}", local & 0x00FF_FFFF);
    if hex_part == "000000" {
        return true;
    }
    // Trailing 4 zeros with non-zero top byte: 010000, 040000, 300000, etc.
    if &hex_part[2..] == "0000" && &hex_part[..2] != "00" {
        return true;
    }
    false
}

fn is_creature_root_type(sig: SigCode) -> bool {
    matches!(sig.as_str(), "NPC_" | "LVLN")
}

fn is_race_type(sig: SigCode) -> bool {
    sig.as_str() == "RACE"
}

fn is_constructible_object_type(sig: SigCode) -> bool {
    sig.as_str() == "COBJ"
}

/// SigCodes that trigger the weapon/armor snowball gate.
fn is_sweep_null_type_for_weapon_root(sig: SigCode) -> bool {
    static TABLE: OnceLock<FxHashSet<SigCode>> = OnceLock::new();
    let set = TABLE.get_or_init(|| {
        let mut s = FxHashSet::default();
        for name in [
            "KYWD", "IPDS", "MATT", "EQUP", "VTYP", "BPTD", "ASTS", "MOVT", "AMDL", "ZOOM", "INNR",
            "AORU", "MGEF", "SPEL", "ENCH", "PERK",
        ] {
            if let Ok(c) = SigCode::from_str(name) {
                s.insert(c);
            }
        }
        s
    });
    set.contains(&sig)
}

/// True when `plugin_sym` resolves to one of the target master filenames
/// (case-insensitive).
fn is_target_master(plugin_sym: Sym, master_names: &[String], interner: &StringInterner) -> bool {
    let plugin = match interner.resolve(plugin_sym) {
        Some(s) => s,
        None => return false,
    };
    let plugin_lower = plugin.to_ascii_lowercase();
    master_names
        .iter()
        .any(|m| m.to_ascii_lowercase() == plugin_lower)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::full_plugin::{FullPluginRunState, RefOwner};
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn make_record(fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        let sig = SigCode::from_str("WEAP").unwrap();
        let fk = make_fk(0x002000, "Output.esp", interner);
        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn fk_field(fk: FormKey) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str("KWDA").unwrap(),
            value: FieldValue::FormKey(fk),
        }
    }

    fn empty_skip() -> FxHashSet<String> {
        FxHashSet::default()
    }

    fn skip_with(sig: &str) -> FxHashSet<String> {
        let mut s = FxHashSet::default();
        s.insert(sig.to_string());
        s
    }

    #[test]
    fn source_record_decision_tree_requires_distinct_source_master() {
        assert!(!use_source_record_decision_tree(
            "SeventySix.esm",
            "SeventySix.esm",
            &["Fallout4.esm".to_string(), "SeventySix.esm".to_string()]
        ));
        assert!(use_source_record_decision_tree(
            "Source.esm",
            "Output.esp",
            &["Fallout4.esm".to_string(), "Source.esm".to_string()]
        ));
    }

    #[test]
    fn full_plugin_sweep_worklist_uses_unresolved_ref_owners_plus_supplemental() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Source.esp");
        let output_plugin = interner.intern("Output.esp");
        let owner = FormKey {
            local: 0x800,
            plugin: output_plugin,
        };
        let unrelated_owner = FormKey {
            local: 0x801,
            plugin: output_plugin,
        };
        let unresolved_ref = FormKey {
            local: 0x900,
            plugin: source_plugin,
        };
        let target_master_ref = FormKey {
            local: 0x901,
            plugin: interner.intern("Fallout4.esm"),
        };
        let owner_sig = SigCode::from_str("WEAP").unwrap();
        let mut state = FullPluginRunState::default();
        state
            .unresolved_source_ref_owners
            .entry(unresolved_ref)
            .or_default()
            .push(RefOwner { owner, owner_sig });
        state
            .target_master_ref_owners
            .entry(target_master_ref)
            .or_default()
            .push(RefOwner {
                owner: unrelated_owner,
                owner_sig,
            });
        let mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
            &interner,
        );
        let refs_by_owner = unresolved_source_refs_by_owner(&state, &mapper);

        assert_eq!(
            sweep_worklist_owners(&refs_by_owner, Vec::new()),
            vec![owner]
        );
        assert_eq!(
            sweep_worklist_owners(&refs_by_owner, vec![owner, unrelated_owner]),
            vec![owner, unrelated_owner]
        );
    }

    #[test]
    fn unresolved_source_refs_by_owner_skips_identity_mapped_refs_unless_missing_from_target() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let output_plugin = source_plugin;
        let identity_owner = FormKey {
            local: 0x800,
            plugin: output_plugin,
        };
        let unresolved_owner = FormKey {
            local: 0x801,
            plugin: output_plugin,
        };
        let identity_ref = FormKey {
            local: 0x900,
            plugin: source_plugin,
        };
        let unresolved_ref = FormKey {
            local: 0x901,
            plugin: source_plugin,
        };
        let mut state = FullPluginRunState::default();
        state
            .unresolved_source_ref_owners
            .entry(identity_ref)
            .or_default()
            .push(RefOwner {
                owner: identity_owner,
                owner_sig: SigCode::from_str("NPC_").unwrap(),
            });
        state
            .unresolved_source_ref_owners
            .entry(unresolved_ref)
            .or_default()
            .push(RefOwner {
                owner: unresolved_owner,
                owner_sig: SigCode::from_str("WEAP").unwrap(),
            });

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                ..Default::default()
            },
            &interner,
        );
        mapper.add_mapping(identity_ref, identity_ref);

        let refs_by_owner = unresolved_source_refs_by_owner(&state, &mapper);
        assert!(!refs_by_owner.contains_key(&identity_owner));
        assert_eq!(
            refs_by_owner.get(&unresolved_owner).unwrap(),
            &FxHashSet::from_iter([unresolved_ref])
        );
        assert_eq!(
            sweep_worklist_owners(&refs_by_owner, Vec::new()),
            vec![unresolved_owner]
        );

        let refs_by_owner =
            unresolved_source_refs_by_owner_with_target_check(&state, &mapper, &mut |_| false);
        assert_eq!(
            refs_by_owner.get(&identity_owner).unwrap(),
            &FxHashSet::from_iter([identity_ref]),
            "identity-mapped ref missing from the target is still unresolved"
        );
    }

    #[test]
    fn known_unresolved_refs_fast_path_nulls_only_captured_refs() {
        let interner = StringInterner::new();
        let source_sym = interner.intern("SeventySix.esm");
        let output_sym = source_sym;
        let null_sym = interner.intern("__null__");
        let stale_ref = make_fk(0x84B898, "SeventySix.esm", &interner);
        let valid_ref = make_fk(0x001234, "SeventySix.esm", &interner);
        let mut unresolved_source_refs = FxHashSet::default();
        unresolved_source_refs.insert(stale_ref);
        let mut record = make_record(vec![fk_field(stale_ref), fk_field(valid_ref)], &interner);
        let mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                ..Default::default()
            },
            &interner,
        );

        let outcome = apply_known_unresolved_refs_to_record(
            &mut record,
            source_sym,
            null_sym,
            Some(output_sym),
            Some(&unresolved_source_refs),
            false,
            &mapper,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.nulled, 1);
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_eq!(fk.local, 0);
            assert_eq!(fk.plugin, null_sym);
        } else {
            panic!("expected FormKey field");
        }
        if let FieldValue::FormKey(fk) = &record.fields[1].value {
            assert_eq!(*fk, valid_ref);
        } else {
            panic!("expected FormKey field");
        }
    }

    #[test]
    fn known_unresolved_refs_fast_path_applies_existing_remap() {
        let interner = StringInterner::new();
        let source_sym = interner.intern("SeventySix.esm");
        let output_sym = interner.intern("Output.esp");
        let null_sym = interner.intern("__null__");
        let source_ref = make_fk(0x417C46, "SeventySix.esm", &interner);
        let persisted_ref = make_fk(0x417C46, "Output.esp", &interner);
        let target_ref = make_fk(0x000123, "Fallout4.esm", &interner);
        let mut unresolved_source_refs = FxHashSet::default();
        unresolved_source_refs.insert(source_ref);
        let mut record = make_record(vec![fk_field(persisted_ref)], &interner);
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
            &interner,
        );
        mapper.add_mapping(source_ref, target_ref);

        let outcome = apply_known_unresolved_refs_to_record(
            &mut record,
            source_sym,
            null_sym,
            Some(output_sym),
            Some(&unresolved_source_refs),
            false,
            &mapper,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.nulled, 0);
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_eq!(*fk, target_ref);
        } else {
            panic!("expected FormKey field");
        }
    }

    #[test]
    fn persisted_output_ref_from_full_plugin_state_is_swept_as_source_ref() {
        let interner = StringInterner::new();
        let source_sym = interner.intern("SeventySix.esm");
        let output_sym = interner.intern("Output.esp");
        let null_sym = interner.intern("__null__");
        let source_ref = make_fk(0x417C46, "SeventySix.esm", &interner);
        let persisted_ref = make_fk(0x417C46, "Output.esp", &interner);
        let mut unresolved_source_refs = FxHashSet::default();
        unresolved_source_refs.insert(source_ref);
        let mut record = make_record(vec![fk_field(persisted_ref)], &interner);

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
            &interner,
        );
        let skip = empty_skip();
        let masters: Vec<String> = vec![];

        let outcome = apply_to_record_with_persisted_source_refs(
            &mut record,
            source_sym,
            null_sym,
            Some(output_sym),
            Some(&unresolved_source_refs),
            &lookup_returning("LGDI", Some("LegendaryItems_Weapons_Melee_Rank1")),
            &injector_none(),
            &skip,
            &masters,
            false,
            false,
            &mut mapper,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.nulled, 1);
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_eq!(fk.local, 0);
            assert_eq!(fk.plugin, null_sym);
        } else {
            panic!("expected FormKey field");
        }
    }

    #[test]
    fn persisted_output_ref_filters_unresolved_set_when_output_matches_source() {
        let interner = StringInterner::new();
        let source_sym = interner.intern("SeventySix.esm");
        let output_sym = source_sym;
        let null_sym = interner.intern("__null__");
        let stale_ref = make_fk(0x84B898, "SeventySix.esm", &interner);
        let valid_ref = make_fk(0x001234, "SeventySix.esm", &interner);
        let mut unresolved_source_refs = FxHashSet::default();
        unresolved_source_refs.insert(stale_ref);
        let mut record = make_record(vec![fk_field(stale_ref), fk_field(valid_ref)], &interner);

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                ..Default::default()
            },
            &interner,
        );
        let skip = empty_skip();
        let masters: Vec<String> = vec![];

        let outcome = apply_to_record_with_persisted_source_refs(
            &mut record,
            source_sym,
            null_sym,
            Some(output_sym),
            Some(&unresolved_source_refs),
            &lookup_returning("ENTM", Some("SCORE_S22_ENTM_Photomode_Frame_KnifeFrame")),
            &injector_none(),
            &skip,
            &masters,
            false,
            false,
            &mut mapper,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.nulled, 1);
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_eq!(fk.local, 0);
            assert_eq!(fk.plugin, null_sym);
        } else {
            panic!("expected FormKey field");
        }
        if let FieldValue::FormKey(fk) = &record.fields[1].value {
            assert_eq!(*fk, valid_ref);
        } else {
            panic!("expected FormKey field");
        }
    }

    fn lookup_returning(
        sig_str: &str,
        eid: Option<&str>,
    ) -> impl Fn(&FormKey, &StringInterner) -> Option<SourceInfo> {
        let sig = SigCode::from_str(sig_str).unwrap();
        let eid = eid.map(|s| s.to_string());
        move |_fk: &FormKey, _interner: &StringInterner| {
            Some(SourceInfo {
                sig,
                eid: eid.clone(),
            })
        }
    }

    fn lookup_none() -> impl Fn(&FormKey, &StringInterner) -> Option<SourceInfo> {
        |_fk: &FormKey, _interner: &StringInterner| None
    }

    /// No-op stub injector: always returns `None`, so Branch 10 falls back to
    /// the legacy null+warn path (used by every existing branch test).
    fn injector_none() -> impl Fn(FormKey, &str, SigCode, &mut FormKeyMapper) -> Option<FormKey> {
        |_src: FormKey, _eid: &str, _sig: SigCode, _m: &mut FormKeyMapper| None
    }

    /// Stub injector that mirrors `inject_minimal_stub` semantics without
    /// requiring a real plugin handle: allocates a target FK via the mapper,
    /// returns it. Used by Branch-10 success tests.
    fn injector_allocate(
        output_plugin: &str,
    ) -> impl Fn(FormKey, &str, SigCode, &mut FormKeyMapper) -> Option<FormKey> + '_ {
        move |src: FormKey, eid: &str, sig: SigCode, m: &mut FormKeyMapper| {
            let _ = output_plugin; // plugin name is governed by MapperOptions
            let eid_sym = if eid.is_empty() {
                None
            } else {
                Some(m.interner.intern(eid))
            };
            Some(m.allocate_or_resolve(src, eid_sym, sig))
        }
    }

    fn lookup_returning_for(
        sig_str: &str,
        eid: Option<&str>,
        target_local: u32,
    ) -> impl Fn(&FormKey, &StringInterner) -> Option<SourceInfo> {
        let sig = SigCode::from_str(sig_str).unwrap();
        let eid = eid.map(|s| s.to_string());
        move |fk: &FormKey, _interner: &StringInterner| {
            if fk.local == target_local {
                Some(SourceInfo {
                    sig,
                    eid: eid.clone(),
                })
            } else {
                None
            }
        }
    }

    fn boxed_lookup(
        source: Option<(&'static str, Option<&'static str>)>,
    ) -> Box<dyn Fn(&FormKey, &StringInterner) -> Option<SourceInfo>> {
        match source {
            None => Box::new(lookup_none()),
            Some((sig, eid)) => Box::new(lookup_returning(sig, eid)),
        }
    }

    fn output_mapper_options(preserve_source_ids: bool) -> MapperOptions {
        MapperOptions {
            output_plugin_name: "Output.esp".into(),
            preserve_source_ids,
            ..Default::default()
        }
    }

    fn has_warning(outcome: &SweepOutcome, mapper: &FormKeyMapper, needle: &str) -> bool {
        outcome
            .warnings
            .iter()
            .any(|s| mapper.interner.resolve(*s).unwrap_or("").contains(needle))
    }

    #[test]
    fn apply_to_record_null_branches() {
        #[allow(clippy::type_complexity)]
        let cases: [(
            &str,
            u32,
            Option<(&str, Option<&str>)>,
            Option<&str>,
            bool,
            bool,
            Option<&str>,
        ); 7] = [
            ("1: packed data", 0x300000, None, None, false, false, None),
            (
                "2: unreadable source warns",
                0x001234,
                None,
                None,
                false,
                false,
                Some(""),
            ),
            (
                "3: source without EID warns",
                0x001234,
                Some(("WEAP", None)),
                None,
                false,
                false,
                Some(""),
            ),
            (
                "4: creature root, unwalked RACE",
                0x001234,
                Some(("RACE", Some("HumanRace"))),
                None,
                true,
                false,
                None,
            ),
            (
                "5: creature root, COBJ",
                0x001234,
                Some(("COBJ", Some("SomeRecipe"))),
                None,
                true,
                false,
                None,
            ),
            (
                "6: skip record type",
                0x001234,
                Some(("NAVM", Some("NavMeshSomething"))),
                Some("NAVM"),
                false,
                false,
                None,
            ),
            (
                "10: unresolved without stub injection",
                0x001234,
                Some(("AMMO", Some("UnknownAmmo"))),
                None,
                false,
                true,
                Some("stub_unavailable"),
            ),
        ];
        for (name, local, source, skip_sig, creature_root, output_named, warning) in cases {
            let interner = StringInterner::new();
            let source_sym = interner.intern("SeventySix.esm");
            let null_sym = interner.intern("__null__");
            let mut record = make_record(
                vec![fk_field(make_fk(local, "SeventySix.esm", &interner))],
                &interner,
            );
            let mapper_interner = StringInterner::new();
            let options = if output_named {
                output_mapper_options(false)
            } else {
                MapperOptions::default()
            };
            let mut mapper = FormKeyMapper::new([], options, &mapper_interner);
            let skip = skip_sig.map_or_else(empty_skip, skip_with);

            let outcome = apply_to_record_with(
                &mut record,
                source_sym,
                null_sym,
                &boxed_lookup(source),
                &injector_none(),
                &skip,
                &[],
                creature_root,
                &mut mapper,
            );

            assert!(outcome.changed, "{name}");
            assert_eq!(outcome.nulled, 1, "{name}");
            let FieldValue::FormKey(fk) = &record.fields[0].value else {
                panic!("{name}: expected FormKey field");
            };
            assert_eq!((fk.local, fk.plugin), (0, null_sym), "{name}");
            if let Some(needle) = warning {
                assert!(
                    has_warning(&outcome, &mapper, needle),
                    "{name}: warning {needle:?}"
                );
            }
        }
    }

    #[test]
    fn apply_to_record_vanilla_eid_remaps() {
        // Branch 7 remaps through the vanilla EID index; branch 8 (weapon snowball
        // gate) only keeps KYWD remaps that land in a listed target master.
        #[allow(clippy::type_complexity)]
        let cases: [(
            &str,
            &str,
            &str,
            u32,
            (&str, &str),
            (&str, u32, &str, &str),
            u32,
            u32,
            Option<&str>,
        ); 5] = [
            (
                "KYWD remap into base master kept",
                "WEAP",
                "KWDA",
                0x001234,
                ("KYWD", "WeaponTypeRifle"),
                ("weapontyperifle", 0x0009A7, "Fallout4.esm", "KYWD"),
                0,
                0x0009A7,
                None,
            ),
            (
                "KYWD remap into unlisted DLC nulled",
                "WEAP",
                "KWDA",
                0x001234,
                ("KYWD", "AnimsHandmadeAssaultRifle"),
                (
                    "AnimsHandmadeAssaultRifle",
                    0x00ABCD,
                    "DLCNukaWorld.esm",
                    "KYWD",
                ),
                1,
                0,
                None,
            ),
            (
                "AMMO outside the snowball gate keeps vanilla remap",
                "WEAP",
                "KWDA",
                0x001234,
                ("AMMO", "Ammo308"),
                ("ammo308", 0x000C5C, "Fallout4.esm", "AMMO"),
                0,
                0x000C5C,
                None,
            ),
            (
                "SCOL ONAM member static remapped to base-game STAT",
                "SCOL",
                "ONAM",
                0x00FC7F,
                ("STAT", "StaticCollectionMember"),
                ("staticcollectionmember", 0x012345, "Fallout4.esm", "STAT"),
                0,
                0x012345,
                Some("Fallout4.esm"),
            ),
            (
                "FO76 currency remapped to FO4 caps MISC",
                "WEAP",
                "KWDA",
                0x00000F,
                ("CNCY", "Caps001"),
                ("caps001", 0x00000F, "Fallout4.esm", "MISC"),
                0,
                0x00000F,
                Some("Fallout4.esm"),
            ),
        ];
        for (
            name,
            record_sig,
            field_sig,
            local,
            (lookup_sig, lookup_eid),
            (index_eid, index_local, index_plugin, index_sig),
            nulled,
            expected_local,
            expected_plugin,
        ) in cases
        {
            let interner = StringInterner::new();
            let source_sym = interner.intern("SeventySix.esm");
            let null_sym = interner.intern("__null__");
            let mut record = make_record(
                vec![FieldEntry {
                    sig: SubrecordSig::from_str(field_sig).unwrap(),
                    value: FieldValue::FormKey(make_fk(local, "SeventySix.esm", &interner)),
                }],
                &interner,
            );
            record.sig = SigCode::from_str(record_sig).unwrap();
            let mapper_interner = StringInterner::new();
            let index_fk = FormKey {
                local: index_local,
                plugin: mapper_interner.intern(index_plugin),
            };
            let mut mapper = FormKeyMapper::new(
                [(
                    mapper_interner.intern(index_eid),
                    index_fk,
                    SigCode::from_str(index_sig).unwrap(),
                )],
                output_mapper_options(false),
                &mapper_interner,
            );

            let outcome = apply_to_record_with(
                &mut record,
                source_sym,
                null_sym,
                &lookup_returning(lookup_sig, Some(lookup_eid)),
                &injector_none(),
                &empty_skip(),
                &["Fallout4.esm".to_string()],
                false,
                &mut mapper,
            );

            assert!(outcome.changed, "{name}");
            assert_eq!(outcome.nulled, nulled, "{name}");
            let FieldValue::FormKey(fk) = &record.fields[0].value else {
                panic!("{name}: expected FormKey field");
            };
            assert_eq!(fk.local, expected_local, "{name}");
            if let Some(plugin) = expected_plugin {
                assert_eq!(mapper.interner.resolve(fk.plugin), Some(plugin), "{name}");
            }
        }
    }

    #[test]
    fn apply_to_record_injects_fo76_currency_stub_as_misc() {
        let interner = StringInterner::new();
        let source_sym = interner.intern("SeventySix.esm");
        let null_sym = interner.intern("__null__");

        let currency_ref = make_fk(0x3F7410, "SeventySix.esm", &interner);
        let mut record = make_record(vec![fk_field(currency_ref)], &interner);

        let mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], output_mapper_options(false), &mapper_interner);

        let seen_sig = std::cell::Cell::new(None::<SigCode>);
        let injector =
            |src: FormKey, eid: &str, sig: SigCode, m: &mut FormKeyMapper| -> Option<FormKey> {
                seen_sig.set(Some(sig));
                let eid_sym = if eid.is_empty() {
                    None
                } else {
                    Some(m.interner.intern(eid))
                };
                Some(m.allocate_or_resolve(src, eid_sym, sig))
            };

        let outcome = apply_to_record_with(
            &mut record,
            source_sym,
            null_sym,
            &lookup_returning("CNCY", Some("LegendaryTokens")),
            &injector,
            &empty_skip(),
            &[],
            false,
            &mut mapper,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.nulled, 0);
        assert_eq!(seen_sig.get().unwrap().as_str(), "MISC");
        let FieldValue::FormKey(fk) = &record.fields[0].value else {
            panic!("expected FormKey field");
        };
        assert_eq!(fk.local, 0x0000_0800);
        assert_eq!(mapper.interner.resolve(fk.plugin), Some("Output.esp"));
    }

    #[test]
    fn apply_to_record_creature_root_uses_mapper_lookup() {
        // Branch 7+9: creature roots skip find_vanilla_fk and use the direct mapping.
        let interner = StringInterner::new();
        let source_sym = interner.intern("SeventySix.esm");
        let null_sym = interner.intern("__null__");

        let stale_fk = make_fk(0x001234, "SeventySix.esm", &interner);
        let mut record = make_record(vec![fk_field(stale_fk)], &interner);

        let mapper_interner = StringInterner::new();
        let src_in_mapper = FormKey {
            local: 0x001234,
            plugin: mapper_interner.intern("SeventySix.esm"),
        };
        let tgt_in_mapper = FormKey {
            local: 0x000900,
            plugin: mapper_interner.intern("Output.esp"),
        };
        let mut mapper = FormKeyMapper::new([], output_mapper_options(false), &mapper_interner);
        mapper.add_mapping(src_in_mapper, tgt_in_mapper);

        let outcome = apply_to_record_with(
            &mut record,
            source_sym,
            null_sym,
            &lookup_returning_for("AMMO", Some("SomeAmmo"), 0x001234),
            &injector_none(),
            &empty_skip(),
            &[],
            true,
            &mut mapper,
        );
        assert!(outcome.changed);
        assert_eq!(outcome.nulled, 0);
        let FieldValue::FormKey(fk) = &record.fields[0].value else {
            panic!("expected FormKey field");
        };
        assert_eq!(fk.local, 0x000900);
    }

    #[test]
    fn apply_to_record_branch10_injects_stub_and_registers_mapping() {
        for (name, local, preserve_source_ids, eid, expected_local) in [
            (
                "new_allocation uses FIRST_ALLOCATION_ID",
                0x001234,
                false,
                "UnknownAmmo",
                0x0000_0800,
            ),
            (
                "source_id_preserved reuses source object id",
                0x003456,
                true,
                "PreservedAmmo",
                0x003456,
            ),
        ] {
            let interner = StringInterner::new();
            let source_sym = interner.intern("SeventySix.esm");
            let null_sym = interner.intern("__null__");
            let mut record = make_record(
                vec![fk_field(make_fk(local, "SeventySix.esm", &interner))],
                &interner,
            );
            let mapper_interner = StringInterner::new();
            let mut mapper = FormKeyMapper::new(
                [],
                output_mapper_options(preserve_source_ids),
                &mapper_interner,
            );

            let outcome = apply_to_record_with(
                &mut record,
                source_sym,
                null_sym,
                &lookup_returning("AMMO", Some(eid)),
                &injector_allocate("Output.esp"),
                &empty_skip(),
                &[],
                false,
                &mut mapper,
            );

            assert!(outcome.changed, "{name}");
            assert_eq!(outcome.nulled, 0, "{name}: stub injection must not null");
            let FieldValue::FormKey(fk) = &record.fields[0].value else {
                panic!("{name}: expected FormKey field");
            };
            assert_eq!(fk.local, expected_local, "{name}");
            assert_eq!(
                mapper.interner.resolve(fk.plugin),
                Some("Output.esp"),
                "{name}"
            );
            let pairs: Vec<_> = mapper.source_to_target_iter().collect();
            assert_eq!(pairs.len(), 1, "{name}: exactly one mapping registered");
            assert_eq!(
                (pairs[0].0.local, pairs[0].1.local),
                (local, expected_local),
                "{name}"
            );
            assert!(has_warning(&outcome, &mapper, "stub_injected"), "{name}");
        }
    }

    // -----------------------------------------------------------------------
    // Packed-data hex helper smoke
    // -----------------------------------------------------------------------

    #[test]
    fn is_packed_data_fk_hex_recognises_known_patterns() {
        assert!(is_packed_data_fk_hex(0x000000));
        assert!(is_packed_data_fk_hex(0x010000));
        assert!(is_packed_data_fk_hex(0x040000));
        assert!(is_packed_data_fk_hex(0x300000));
        assert!(is_packed_data_fk_hex(0xF00000));
        // Real FormKeys (3-byte local with non-zero low bytes) are NOT packed-data.
        assert!(!is_packed_data_fk_hex(0x000810));
        assert!(!is_packed_data_fk_hex(0x001234));
        // 000000 low nibble but high two non-zero only counts when last 4 are 0.
        assert!(!is_packed_data_fk_hex(0x300001));
    }

    // Sweep runs before fix_invalid/null_dangling and its Branch-6 (skip-record-
    // type) would null the LCTN LCEP/ACEP/LCUN source-ref leaf whose placed-child
    // target (ACHR/REFR) is in skip_record_sigs. With defer=true the class is left
    // intact for the post-copy repair; with defer=false it is nulled.

    fn lctn_lcep_record(ref_leaf: FormKey, interner: &StringInterner) -> Record {
        let record_fk = make_fk(0x000800, "Output.esp", interner);
        let ref_sym = interner.intern("master_enable_parent_ref");
        let lcep = FieldEntry {
            sig: SubrecordSig::from_str("LCEP").unwrap(),
            value: FieldValue::List(vec![FieldValue::Struct(vec![(
                ref_sym,
                FieldValue::FormKey(ref_leaf),
            )])]),
        };
        Record {
            sig: SigCode::from_str("LCTN").unwrap(),
            form_key: record_fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![lcep],
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn lcep_ref_local(rec: &Record) -> u32 {
        let e = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "LCEP")
            .unwrap();
        let FieldValue::List(rows) = &e.value else {
            panic!()
        };
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!()
        };
        let FieldValue::FormKey(f) = &fields[0].1 else {
            panic!()
        };
        f.local
    }

    #[test]
    fn sweep_lctn_lcep_placed_child_nulled_unless_deferred() {
        for (defer_placed_child, changed, nulled, local) in
            [(false, true, 1, 0), (true, false, 0, 0x7ACB4D)]
        {
            let interner = StringInterner::new();
            let null_sym = interner.intern("__null__");
            let source_sym = interner.intern("SeventySix.esm");
            let ref_leaf = make_fk(0x7ACB4D, "SeventySix.esm", &interner);
            let mut record = lctn_lcep_record(ref_leaf, &interner);
            let skip = skip_with("ACHR");
            let mut mapper = FormKeyMapper::new([], output_mapper_options(false), &interner);
            let outcome = apply_to_record_with_persisted_source_refs(
                &mut record,
                source_sym,
                null_sym,
                None,
                None,
                &lookup_returning("ACHR", Some("SomePlacedActor")),
                &injector_none(),
                &skip,
                &[],
                false,
                defer_placed_child,
                &mut mapper,
            );
            assert_eq!(outcome.changed, changed, "defer={defer_placed_child}");
            assert_eq!(outcome.nulled, nulled, "defer={defer_placed_child}");
            assert_eq!(lcep_ref_local(&record), local, "defer={defer_placed_child}");
        }
    }
}
