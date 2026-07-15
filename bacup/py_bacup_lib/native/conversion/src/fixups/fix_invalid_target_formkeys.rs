//! Fixup: null out FormKey references that point to a target master ESM at a
//! FormKey that does not exist in that master.
//!

//!
//! # What this does
//! After `remap_formkey` blindly renames `SeventySix.esm` → `Fallout4.esm`,
//! many target-ESM FormKey references may point at FormIDs that do not exist
//! in Fallout4.esm.  This fixup finds every such "dangling" reference in the
//! target plugin and either rewrites it to a converted local record with the
//! same object ID or nulls it to `local=0`.
//!
//! This fixup implements a local-output rewrite when the converted record
//! already exists, else the nullification fallback (set `local=0`). Vanilla
//! remap and stub-allocation of unidentifiable refs are handled upstream.
//!
//! # Convergence
//! `convergent()` returns `true`.  Nullifying a FK leaf may expose a
//! previously-hidden dangling reference (e.g. after a struct collapses to a
//! simpler form), so the fixup re-runs until no more changes are needed.
//! In practice convergence occurs in at most 2 iterations.
//!
//! # Design
//! `apply_to_record` accepts an `is_invalid_fk: &dyn Fn(&FormKey) -> bool`
//! closure so unit tests can inject validation logic without real plugin handles.
//! The session path uses a resolver that performs the full null-check +
//! master-existence check against real handles (same pattern as
//! `clean_leveled_item_entries.rs`), and rewrites raw `array_struct` formid
//! slots such as COBJ.FVPA when those slots still decode as bytes.
//!

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FullPluginRunState;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::schema::{RecordDef, SubrecordDef};
use crate::session::PluginSession;
use crate::source_read::form_key_to_read_str;
use crate::sym::StringInterner;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FixInvalidTargetFormKeysFixup;

impl Fixup for FixInvalidTargetFormKeysFixup {
    fn name(&self) -> &'static str {
        "_fix_invalid_target_formkeys"
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
        true
    }

    fn run_full_plugin_worklist(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
        state: &FullPluginRunState,
    ) -> Result<Option<FixupReport>, FixupError> {
        Ok(Some(
            FixInvalidTargetFormKeysFixup::run_full_plugin_worklist(
                self, session, mapper, config, state,
            )?,
        ))
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut report = FixupReport::empty();
        let target_masters: Vec<(String, u64)> = session
            .target_masters()
            .iter()
            .cloned()
            .zip(config.target_master_handle_ids.iter().copied())
            .collect();
        let mut changed_records = Vec::new();

        let sigs = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for sig in sigs {
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;

            for fk in fks {
                let mut record =
                    match session.record_decoded(&fk, target_schema.as_ref(), mapper.interner) {
                        Ok(r) => r,
                        Err(e) => {
                            let w = mapper
                                .interner
                                .intern(&format!("fix_invalid_fk_read_err:{e}"));
                            report.warnings.push(w);
                            continue;
                        }
                    };

                if apply_to_record_in_session(
                    &mut record,
                    session,
                    mapper.interner,
                    &target_masters,
                    target_schema.as_ref(),
                ) {
                    changed_records.push(record);
                }
            }
        }

        let replaced = session
            .replace_records_contents(changed_records, target_schema.as_ref(), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);

        Ok(report)
    }
}

impl FixInvalidTargetFormKeysFixup {
    pub fn run_full_plugin_worklist(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
        state: &FullPluginRunState,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut report = FixupReport::empty();
        let target_masters: Vec<(String, u64)> = session
            .target_masters()
            .iter()
            .cloned()
            .zip(config.target_master_handle_ids.iter().copied())
            .collect();
        let mut changed_records = Vec::new();

        let mut invalid_refs = FxHashSet::default();
        for referenced_fk in state.target_master_ref_owners.keys() {
            if is_invalid_target_fk(session, mapper.interner, &target_masters, referenced_fk) {
                invalid_refs.insert(*referenced_fk);
            }
        }
        let known_target_refs: FxHashSet<FormKey> =
            state.target_master_ref_owners.keys().copied().collect();

        let mut supplemental_owners = Vec::new();
        for sig_str in ["COBJ", "FLST"] {
            let sig = SigCode::from_str(sig_str).expect("supplemental cleanup signature");
            supplemental_owners.extend(
                session
                    .form_keys_of_sig(sig, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?,
            );
        }
        let resolution_cache = std::cell::RefCell::new(FxHashMap::default());

        // Diagnostic count of subrecord entries left untouched by the placed-child
        // deferral. Behaviour-free; only feeds the trace below.
        let mut deferred_skipped = 0u32;

        for owner_fk in invalid_target_worklist_owners(state, &invalid_refs, supplemental_owners) {
            let mut record =
                match session.record_decoded(&owner_fk, target_schema.as_ref(), mapper.interner) {
                    Ok(record) => record,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("fix_invalid_worklist_read_err:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };

            if config.defer_placed_child_ref_class {
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

            if apply_to_record_in_full_plugin_worklist(
                &mut record,
                session,
                mapper.interner,
                &target_masters,
                target_schema.as_ref(),
                &known_target_refs,
                &invalid_refs,
                &resolution_cache,
                config.defer_placed_child_ref_class,
            ) {
                changed_records.push(record);
            }
        }

        eprintln!(
            "[trace_defer] fix_invalid: defer_placed_child={} deferred_skipped={deferred_skipped}",
            config.defer_placed_child_ref_class
        );

        let replaced = session
            .replace_records_contents(changed_records, target_schema.as_ref(), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

fn apply_to_record_in_session(
    record: &mut Record,
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[(String, u64)],
    target_schema: &crate::schema::AuthoringSchema,
) -> bool {
    let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
    let output_plugin_sym = interner.intern(&output_plugin_name);
    let target_master_names = session.target_masters().to_vec();
    let output_master_index = target_master_names.len() as u32;
    let target_handle_id = session.target_id();
    let session = std::cell::RefCell::new(session);
    let record_def = target_schema.record_def(record.sig.as_str());
    let rec_sig = record.sig.as_str().to_string();
    let rec_local = record.form_key.local;
    apply_to_record_with_resolution(
        record,
        record_def,
        &|fk| {
            let mut session = session.borrow_mut();
            let resolution = resolve_target_fk(
                &mut session,
                interner,
                target_masters,
                target_handle_id,
                output_plugin_sym,
                fk,
            );
            trace_dlc_kw_resolution(&rec_sig, rec_local, interner, fk, resolution);
            resolution
        },
        Some(RawFormIdContext {
            master_names: &target_master_names,
            output_master_index,
            interner,
        }),
        // Per-record (graph-scoped) path: never the whole-plugin worldspace run,
        // so the placed-child class is resolved here as usual.
        false,
    )
}

fn apply_to_record_in_full_plugin_worklist(
    record: &mut Record,
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[(String, u64)],
    target_schema: &crate::schema::AuthoringSchema,
    known_target_refs: &FxHashSet<FormKey>,
    invalid_refs: &FxHashSet<FormKey>,
    resolution_cache: &std::cell::RefCell<FxHashMap<FormKey, TargetFkResolution>>,
    defer_placed_child: bool,
) -> bool {
    let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
    let output_plugin_sym = interner.intern(&output_plugin_name);
    let target_master_names = session.target_masters().to_vec();
    let output_master_index = target_master_names.len() as u32;
    let target_handle_id = session.target_id();
    let session = std::cell::RefCell::new(session);
    let record_def = target_schema.record_def(record.sig.as_str());
    let rec_sig = record.sig.as_str().to_string();
    let rec_local = record.form_key.local;
    apply_to_record_with_resolution(
        record,
        record_def,
        &|fk| {
            if let Some(resolution) = resolution_cache.borrow().get(fk).copied() {
                trace_dlc_kw_resolution(&rec_sig, rec_local, interner, fk, resolution);
                return resolution;
            }
            let mut session = session.borrow_mut();
            let resolution = resolve_target_fk_with_known_ref_sets(
                &mut session,
                interner,
                target_masters,
                target_handle_id,
                output_plugin_sym,
                known_target_refs,
                invalid_refs,
                fk,
            );
            resolution_cache.borrow_mut().insert(*fk, resolution);
            trace_dlc_kw_resolution(&rec_sig, rec_local, interner, fk, resolution);
            resolution
        },
        Some(RawFormIdContext {
            master_names: &target_master_names,
            output_master_index,
            interner,
        }),
        defer_placed_child,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetFkResolution {
    Keep,
    Null,
    Rewrite(FormKey),
}

/// Env-gated (`MODBOX_TRACE_DROPS`) trace of how a watched DLC-keyword reference
/// resolves in this fixup — the prime suspect for nulling the FO76→FO4 weapon
/// family keyword (all FO4 DLC master refs vanish). Logs Keep/Null/Rewrite so a
/// regen shows whether `fix_invalid_target_formkeys` is the stage that drops it.
#[inline]
fn trace_dlc_kw_resolution(
    rec_sig: &str,
    rec_local: u32,
    interner: &StringInterner,
    fk: &FormKey,
    resolution: TargetFkResolution,
) {
    if !crate::drop_trace::enabled() || !crate::drop_trace::is_dlc_kw_watched(fk.local) {
        return;
    }
    let plug = interner.resolve(fk.plugin).unwrap_or("?");
    let outcome = match resolution {
        TargetFkResolution::Keep => "keep".to_string(),
        TargetFkResolution::Null => "NULL".to_string(),
        TargetFkResolution::Rewrite(r) => format!("rewrite->{:06X}", r.local),
    };
    crate::drop_trace::trace(
        "fix_invalid_target",
        rec_sig,
        rec_local,
        "",
        &format!("ref {plug}:{:06X} -> {outcome}", fk.local),
    );
}

fn resolve_target_fk(
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[(String, u64)],
    target_handle_id: u64,
    output_plugin_sym: crate::sym::Sym,
    fk: &FormKey,
) -> TargetFkResolution {
    if is_known_valid_target_fk(fk, interner) {
        return TargetFkResolution::Keep;
    }
    if !is_invalid_target_fk(session, interner, target_masters, fk) {
        return TargetFkResolution::Keep;
    }

    let local_fk = FormKey {
        plugin: output_plugin_sym,
        local: fk.local,
    };
    let local_fk_str = form_key_to_read_str(&local_fk, interner);
    if !local_fk_str.is_empty()
        && session
            .record_exists_in_handle(target_handle_id, &local_fk_str)
            .unwrap_or(false)
    {
        return TargetFkResolution::Rewrite(local_fk);
    }

    TargetFkResolution::Null
}

fn resolve_target_fk_with_known_ref_sets(
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[(String, u64)],
    target_handle_id: u64,
    output_plugin_sym: crate::sym::Sym,
    known_target_refs: &FxHashSet<FormKey>,
    invalid_refs: &FxHashSet<FormKey>,
    fk: &FormKey,
) -> TargetFkResolution {
    if fk.local == 0 || is_known_valid_target_fk(fk, interner) {
        return TargetFkResolution::Keep;
    }

    let invalid = if invalid_refs.contains(fk) {
        true
    } else if known_target_refs.contains(fk) {
        false
    } else {
        is_invalid_target_fk(session, interner, target_masters, fk)
    };
    if !invalid {
        return TargetFkResolution::Keep;
    }

    let local_fk = FormKey {
        plugin: output_plugin_sym,
        local: fk.local,
    };
    let local_fk_str = form_key_to_read_str(&local_fk, interner);
    if !local_fk_str.is_empty()
        && session
            .record_exists_in_handle(target_handle_id, &local_fk_str)
            .unwrap_or(false)
    {
        return TargetFkResolution::Rewrite(local_fk);
    }

    TargetFkResolution::Null
}

fn is_known_valid_target_fk(fk: &FormKey, interner: &StringInterner) -> bool {
    fk.local == 0x00000F
        && interner
            .resolve(fk.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case("Fallout4.esm"))
}

fn is_invalid_target_fk(
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[(String, u64)],
    fk: &FormKey,
) -> bool {
    if fk.local == 0 {
        return false;
    }
    let Some(plugin_part) = interner.resolve(fk.plugin) else {
        return false;
    };
    for (master_name, master_id) in target_masters {
        if plugin_part.eq_ignore_ascii_case(master_name) {
            let canonical_fk_str = format!("{master_name}:{:06X}", fk.local);
            return session
                .record_exists_in_handle(*master_id, &canonical_fk_str)
                .map(|exists| !exists)
                .unwrap_or(true);
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetRawResolution {
    Keep,
    Null,
    Rewrite(u32),
}

#[derive(Clone, Copy)]
struct RawFormIdContext<'a> {
    master_names: &'a [String],
    output_master_index: u32,
    interner: &'a StringInterner,
}

fn raw_resolver_from_fk_resolver(
    raw: u32,
    master_names: &[String],
    output_master_index: u32,
    interner: &StringInterner,
    resolve_fk: &dyn Fn(&FormKey) -> TargetFkResolution,
) -> TargetRawResolution {
    if raw == 0 {
        return TargetRawResolution::Keep;
    }
    let master_index = (raw >> 24) as usize;
    let object_id = raw & 0x00FF_FFFF;
    let Some(master_name) = master_names.get(master_index) else {
        return TargetRawResolution::Keep;
    };
    let fk = FormKey {
        plugin: interner.intern(master_name),
        local: object_id,
    };
    match resolve_fk(&fk) {
        TargetFkResolution::Keep => TargetRawResolution::Keep,
        TargetFkResolution::Null => TargetRawResolution::Null,
        TargetFkResolution::Rewrite(replacement) => {
            if replacement.local == 0 {
                TargetRawResolution::Null
            } else {
                TargetRawResolution::Rewrite(
                    (output_master_index << 24) | (replacement.local & 0x00FF_FFFF),
                )
            }
        }
    }
}

fn apply_to_record_with_resolution(
    record: &mut Record,
    record_def: Option<&RecordDef>,
    resolve_fk: &dyn Fn(&FormKey) -> TargetFkResolution,
    raw_ctx: Option<RawFormIdContext<'_>>,
    defer_placed_child: bool,
) -> bool {
    let record_sig = record.sig.as_str().to_string();
    let mut any_changed = false;
    record.fields.retain_mut(|entry| {
        // Whole-plugin FO76→FO4: leave the placed-ref-target class (LCTN
        // LCEP/ACEP/LCUN) untouched. Their targets — exterior placed children —
        // are re-inserted by the phase-6 cell-slice copy AFTER all fixups, so
        // nulling them here (they look "invalid" pre-copy) is wrong. The post-copy
        // `null_dangling_own_plugin_refs::repair_placed_child_refs` resolves them
        // authoritatively once the targets exist.
        if defer_placed_child
            && crate::fixups::null_dangling_own_plugin_refs::is_deferred_placed_child(
                &record_sig,
                entry.sig.as_str(),
            )
        {
            return true;
        }
        let subrecord_def = record_def.and_then(|def| def.subrecord_def(entry.sig.as_str()));
        let codec = subrecord_def.and_then(|def| def.codec.as_deref());
        if subrecord_def.is_some_and(|def| def.multiple)
            && codec == Some("formid")
            && resolve_invalid_or_null_formid(&mut entry.value, resolve_fk)
        {
            any_changed = true;
            return !is_null_formid(&entry.value);
        }

        let is_formid_array = codec.is_some_and(|codec| codec == "formid_array");
        if is_formid_array && resolve_formid_array_items(&mut entry.value, resolve_fk) {
            any_changed = true;
            return true;
        }

        if let Some(subrecord_def) = subrecord_def {
            if let Some(raw_ctx) = raw_ctx {
                let raw_changed = resolve_array_struct_formids(
                    &mut entry.value,
                    subrecord_def,
                    raw_ctx,
                    resolve_fk,
                );
                if raw_changed {
                    any_changed = true;
                }
            }
        }

        if resolve_fk_leaves(&mut entry.value, resolve_fk) {
            any_changed = true;
        }
        true
    });
    any_changed
}

fn invalid_target_worklist_owners(
    state: &FullPluginRunState,
    invalid_refs: &FxHashSet<FormKey>,
    supplemental_owners: impl IntoIterator<Item = FormKey>,
) -> Vec<FormKey> {
    let mut seen = FxHashSet::default();
    let mut owners = Vec::new();
    for referenced_fk in invalid_refs {
        let Some(ref_owners) = state.target_master_ref_owners.get(referenced_fk) else {
            continue;
        };
        for ref_owner in ref_owners {
            if seen.insert(ref_owner.owner) {
                owners.push(ref_owner.owner);
            }
        }
    }
    for owner in supplemental_owners {
        if seen.insert(owner) {
            owners.push(owner);
        }
    }
    owners
}

/// Walk every `FieldValue::FormKey` leaf in `record` and null out any leaf for
/// which `is_invalid_fk` returns `true`.
///
/// "Null out" means setting `fk.local = 0` and keeping the plugin sym — matching
/// Python's `null_replacements = {fk: None for fk in null_fks}` semantics which
/// replaces the FK value with `None` / zero in the serialised record.
///
/// Returns `true` when at least one leaf was nullified.
///
/// # Parameters
/// - `is_invalid_fk` — predicate; returns `true` when the FK should be zeroed.
pub fn apply_to_record(
    record: &mut Record,
    record_def: Option<&RecordDef>,
    is_invalid_fk: &dyn Fn(&FormKey) -> bool,
) -> bool {
    apply_to_record_with_resolution(
        record,
        record_def,
        &|fk| {
            if is_invalid_fk(fk) {
                TargetFkResolution::Null
            } else {
                TargetFkResolution::Keep
            }
        },
        None,
        // Public per-record entry (graph path / tests): no worldspace deferral.
        false,
    )
}

fn is_null_formid(value: &FieldValue) -> bool {
    match value {
        FieldValue::None => true,
        FieldValue::FormKey(fk) => fk.local == 0,
        FieldValue::Uint(value) => *value == 0,
        FieldValue::Int(value) => *value == 0,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == 0
        }
        _ => false,
    }
}

fn resolve_invalid_or_null_formid(
    value: &mut FieldValue,
    resolve_fk: &dyn Fn(&FormKey) -> TargetFkResolution,
) -> bool {
    if is_null_formid(value) {
        return true;
    }
    let FieldValue::FormKey(fk) = value else {
        return false;
    };
    match resolve_fk(fk) {
        TargetFkResolution::Keep => false,
        TargetFkResolution::Null => {
            fk.local = 0;
            true
        }
        TargetFkResolution::Rewrite(replacement) => {
            *fk = replacement;
            true
        }
    }
}

fn resolve_formid_array_items(
    value: &mut FieldValue,
    resolve_fk: &dyn Fn(&FormKey) -> TargetFkResolution,
) -> bool {
    let FieldValue::List(items) = value else {
        return false;
    };
    let mut changed = false;
    items.retain_mut(|item| {
        let FieldValue::FormKey(fk) = item else {
            return true;
        };
        match resolve_fk(fk) {
            TargetFkResolution::Keep => true,
            TargetFkResolution::Null => {
                changed = true;
                false
            }
            TargetFkResolution::Rewrite(replacement) => {
                *fk = replacement;
                changed = true;
                true
            }
        }
    });
    changed
}

/// Recursively walk `value` and null out every `FieldValue::FormKey` leaf for
/// which `is_invalid_fk` returns `true`.
///
/// Returns `true` when any leaf was modified.
fn resolve_fk_leaves(
    value: &mut FieldValue,
    resolve_fk: &dyn Fn(&FormKey) -> TargetFkResolution,
) -> bool {
    match value {
        FieldValue::FormKey(fk) => match resolve_fk(fk) {
            TargetFkResolution::Keep => false,
            TargetFkResolution::Null => {
                fk.local = 0;
                true
            }
            TargetFkResolution::Rewrite(replacement) => {
                *fk = replacement;
                true
            }
        },
        FieldValue::List(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                if resolve_fk_leaves(item, resolve_fk) {
                    changed = true;
                }
            }
            changed
        }
        FieldValue::Struct(fields) => {
            let mut changed = false;
            for (_, v) in fields.iter_mut() {
                if resolve_fk_leaves(v, resolve_fk) {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

fn resolve_array_struct_formids(
    value: &mut FieldValue,
    subrecord_def: &SubrecordDef,
    raw_ctx: RawFormIdContext<'_>,
    resolve_fk: &dyn Fn(&FormKey) -> TargetFkResolution,
) -> bool {
    let FieldValue::Bytes(bytes) = value else {
        return false;
    };
    let Some((row_size, formid_offsets)) = array_struct_formid_offsets(subrecord_def) else {
        return false;
    };
    if row_size == 0 || formid_offsets.is_empty() || bytes.len() % row_size != 0 {
        return false;
    }

    let mut changed = false;
    for row in bytes.chunks_exact_mut(row_size) {
        for offset in &formid_offsets {
            let offset = *offset;
            if offset + 4 > row.len() {
                continue;
            }
            let raw = u32::from_le_bytes([
                row[offset],
                row[offset + 1],
                row[offset + 2],
                row[offset + 3],
            ]);
            match raw_resolver_from_fk_resolver(
                raw,
                raw_ctx.master_names,
                raw_ctx.output_master_index,
                raw_ctx.interner,
                resolve_fk,
            ) {
                TargetRawResolution::Keep => {}
                TargetRawResolution::Null => {
                    row[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
                    changed = true;
                }
                TargetRawResolution::Rewrite(replacement) => {
                    row[offset..offset + 4].copy_from_slice(&replacement.to_le_bytes());
                    changed = true;
                }
            }
        }
    }
    changed
}

fn array_struct_formid_offsets(subrecord_def: &SubrecordDef) -> Option<(usize, Vec<usize>)> {
    let codec = subrecord_def.codec.as_deref()?;
    let body = codec.strip_prefix("array_struct:")?;
    let sizes: Option<Vec<usize>> = body
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(array_struct_token_size)
        .collect();
    let sizes = sizes?;
    if sizes.is_empty() || sizes.len() != subrecord_def.fields.len() {
        return None;
    }

    let mut row_size = 0usize;
    let mut offsets = Vec::new();
    for (field, size) in subrecord_def.fields.iter().zip(sizes.iter().copied()) {
        if is_formid_field(field) && size == 4 {
            offsets.push(row_size);
        }
        row_size += size;
    }
    Some((row_size, offsets))
}

fn array_struct_token_size(token: &str) -> Option<usize> {
    match token {
        "B" | "b" | "u8" | "i8" | "uint8" | "int8" => Some(1),
        "H" | "h" | "u16" | "i16" | "uint16" | "int16" => Some(2),
        "I" | "i" | "f" | "u32" | "i32" | "uint32" | "int32" | "float" | "float32" | "formid"
        | "form_id" => Some(4),
        "Q" | "q" | "d" | "u64" | "i64" | "uint64" | "int64" | "double" => Some(8),
        _ => None,
    }
}

fn is_formid_field(field: &crate::schema::FieldDef) -> bool {
    field.kind.eq_ignore_ascii_case("formid")
        || field
            .codec
            .as_deref()
            .is_some_and(|codec| codec.eq_ignore_ascii_case("formid"))
}

// ---------------------------------------------------------------------------
// Helper: collect all invalid FK strings in a record's field tree
// ---------------------------------------------------------------------------

/// Collect every `FieldValue::FormKey` leaf in `record` for which `is_invalid_fk`
/// returns `true`.  Used for scanning without mutation (e.g. convergence tests).
///
/// Returned strings are in "XXXXXX@Plugin" format.
pub fn collect_invalid_fks(
    record: &Record,
    is_invalid_fk: &dyn Fn(&FormKey) -> bool,
    interner: &StringInterner,
) -> FxHashSet<String> {
    let mut found = FxHashSet::default();
    for entry in &record.fields {
        collect_fk_leaves_in_value(&entry.value, is_invalid_fk, interner, &mut found);
    }
    found
}

fn collect_fk_leaves_in_value(
    value: &FieldValue,
    is_invalid_fk: &dyn Fn(&FormKey) -> bool,
    interner: &StringInterner,
    out: &mut FxHashSet<String>,
) {
    match value {
        FieldValue::FormKey(fk) => {
            if is_invalid_fk(fk) {
                if let Some(plugin) = interner.resolve(fk.plugin) {
                    out.insert(format!("{:06X}@{plugin}", fk.local));
                }
            }
        }
        FieldValue::List(items) => {
            for item in items {
                collect_fk_leaves_in_value(item, is_invalid_fk, interner, out);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, v) in fields {
                collect_fk_leaves_in_value(v, is_invalid_fk, interner, out);
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

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_fk(hex: &str, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(&format!("{hex}@{plugin}"), interner).unwrap()
    }

    fn null_fk(interner: &StringInterner) -> FormKey {
        make_fk("000000", "Fallout4.esm", interner)
    }

    fn invalid_master_fk(interner: &StringInterner) -> FormKey {
        // local > 0x8000 → simulated-missing predicate marks it invalid.
        make_fk("009000", "Fallout4.esm", interner)
    }

    fn valid_master_fk(interner: &StringInterner) -> FormKey {
        // local < 0x8000 → predicate marks it as valid.
        make_fk("001234", "Fallout4.esm", interner)
    }

    fn non_master_fk(interner: &StringInterner) -> FormKey {
        make_fk("001234", "SomeMod.esp", interner)
    }

    fn make_record_with_fields(fk: FormKey, fields: Vec<FieldEntry>) -> Record {
        make_record_with_sig("WEAP", fk, fields)
    }

    fn make_record_with_sig(sig: &str, fk: FormKey, fields: Vec<FieldEntry>) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn simple_fk_field(sig: &str, fk: FormKey) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::FormKey(fk),
        }
    }

    /// Predicate: FK is "invalid" when it points at Fallout4.esm with local > 0x8000.
    fn master_invalid_pred(interner: &StringInterner) -> Box<dyn Fn(&FormKey) -> bool> {
        let fo4_sym = interner.intern("Fallout4.esm");
        Box::new(move |fk: &FormKey| fk.local != 0 && fk.plugin == fo4_sym && fk.local > 0x8000)
    }

    #[test]
    fn known_valid_target_fk_keeps_fo4_caps() {
        let mut interner = StringInterner::new();
        let caps = make_fk("00000F", "Fallout4.esm", &mut interner);
        let source_caps = make_fk("00000F", "SeventySix.esm", &mut interner);
        let other_fo4 = make_fk("000010", "Fallout4.esm", &mut interner);

        assert!(is_known_valid_target_fk(&caps, &interner));
        assert!(!is_known_valid_target_fk(&source_caps, &interner));
        assert!(!is_known_valid_target_fk(&other_fo4, &interner));
    }

    #[test]
    fn full_plugin_invalid_target_worklist_deduplicates_referenced_fks() {
        let interner = StringInterner::new();
        let output = interner.intern("Output.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let owner_a = FormKey {
            plugin: output,
            local: 0x800,
        };
        let owner_b = FormKey {
            plugin: output,
            local: 0x801,
        };
        let target_ref = FormKey {
            plugin: fallout4,
            local: 0x123456,
        };

        let mut state = FullPluginRunState::default();
        state
            .target_master_ref_owners
            .entry(target_ref)
            .or_default()
            .push(crate::full_plugin::RefOwner {
                owner: owner_a,
                owner_sig: SigCode::from_str("REFR").unwrap(),
            });
        state
            .target_master_ref_owners
            .entry(target_ref)
            .or_default()
            .push(crate::full_plugin::RefOwner {
                owner: owner_b,
                owner_sig: SigCode::from_str("REFR").unwrap(),
            });
        let mut invalid_refs = FxHashSet::default();
        invalid_refs.insert(target_ref);

        assert_eq!(state.target_master_ref_count(), 1);
        assert_eq!(state.target_master_ref_owners[&target_ref].len(), 2);
        assert_eq!(
            invalid_target_worklist_owners(&state, &invalid_refs, Vec::new()),
            vec![owner_a, owner_b]
        );
    }

    #[test]
    fn full_plugin_invalid_target_worklist_includes_supplemental_owners() {
        let interner = StringInterner::new();
        let output = interner.intern("Output.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let owner = FormKey {
            plugin: output,
            local: 0x800,
        };
        let supplemental_owner = FormKey {
            plugin: output,
            local: 0x801,
        };
        let target_ref = FormKey {
            plugin: fallout4,
            local: 0x123456,
        };
        let mut state = FullPluginRunState::default();
        state
            .target_master_ref_owners
            .entry(target_ref)
            .or_default()
            .push(crate::full_plugin::RefOwner {
                owner,
                owner_sig: SigCode::from_str("REFR").unwrap(),
            });
        let mut invalid_refs = FxHashSet::default();
        invalid_refs.insert(target_ref);

        assert_eq!(
            invalid_target_worklist_owners(&state, &invalid_refs, vec![owner, supplemental_owner]),
            vec![owner, supplemental_owner]
        );
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_no_op_when_all_fks_valid() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let field = simple_fk_field("DNAM", valid_master_fk(&mut interner));
        let mut record = make_record_with_fields(record_fk, vec![field]);

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, None, &pred);
        assert!(!changed, "no FK should be nullified when all are valid");
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_ne!(fk.local, 0, "valid FK must not be nullified");
        } else {
            panic!("expected FormKey field");
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_nullifies_invalid_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let bad_field = simple_fk_field("DNAM", invalid_master_fk(&mut interner));
        let mut record = make_record_with_fields(record_fk, vec![bad_field]);

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, None, &pred);
        assert!(changed, "invalid FK should be nullified");
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_eq!(fk.local, 0, "nullified FK must have local=0");
        } else {
            panic!("expected FormKey field");
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_nullifies_only_invalid_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let good_field = simple_fk_field("DNAM", valid_master_fk(&mut interner));
        let bad_field = simple_fk_field("ZNAM", invalid_master_fk(&mut interner));
        let mut record = make_record_with_fields(record_fk, vec![good_field, bad_field]);

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, None, &pred);
        assert!(changed, "one FK was nullified");
        // First field (DNAM, valid) must be unchanged.
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_ne!(fk.local, 0, "valid FK must remain");
        }
        // Second field (ZNAM, invalid) must be zeroed.
        if let FieldValue::FormKey(fk) = &record.fields[1].value {
            assert_eq!(fk.local, 0, "invalid FK must be nullified");
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_skips_already_null_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let null_field = simple_fk_field("DNAM", null_fk(&mut interner));
        let mut record = make_record_with_fields(record_fk, vec![null_field]);

        // Predicate returns false for local=0 (already null), so no change.
        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, None, &pred);
        assert!(!changed, "already-null FK must not trigger a change");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_nullifies_struct_nested_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let inner_sym = interner.intern("InnerRef");
        let struct_field = FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Struct(vec![(
                inner_sym,
                FieldValue::FormKey(invalid_master_fk(&mut interner)),
            )]),
        };
        let mut record = make_record_with_fields(record_fk, vec![struct_field]);

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, None, &pred);
        assert!(changed, "struct-nested invalid FK should be nullified");
        if let FieldValue::Struct(fields) = &record.fields[0].value {
            if let FieldValue::FormKey(fk) = &fields[0].1 {
                assert_eq!(fk.local, 0, "nested FK must be nullified");
            }
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_nullifies_list_nested_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let list_field = FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::List(vec![
                FieldValue::FormKey(valid_master_fk(&mut interner)),
                FieldValue::FormKey(invalid_master_fk(&mut interner)),
            ]),
        };
        let mut record = make_record_with_fields(record_fk, vec![list_field]);

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, None, &pred);
        assert!(changed, "list-nested invalid FK should be nullified");
        if let FieldValue::List(items) = &record.fields[0].value {
            assert_eq!(items.len(), 2);
            if let FieldValue::FormKey(fk) = &items[0] {
                assert_ne!(fk.local, 0, "valid list item must remain");
            }
            if let FieldValue::FormKey(fk) = &items[1] {
                assert_eq!(fk.local, 0, "invalid list item must be nullified");
            }
        }
    }

    #[test]
    fn apply_to_record_drops_invalid_formid_array_items() {
        let mut interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let kwda = FieldEntry {
            sig: SubrecordSig::from_str("KWDA").unwrap(),
            value: FieldValue::List(vec![
                FieldValue::FormKey(valid_master_fk(&mut interner)),
                FieldValue::FormKey(invalid_master_fk(&mut interner)),
                FieldValue::FormKey(non_master_fk(&mut interner)),
            ]),
        };
        let mut record = make_record_with_fields(record_fk, vec![kwda]);

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, schema.record_def("WEAP"), &pred);

        assert!(changed, "invalid KWDA entry should be dropped");
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("KWDA should stay a list");
        };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], FieldValue::FormKey(fk) if fk.local == 0x001234));
        assert!(
            matches!(&items[1], FieldValue::FormKey(fk) if interner.resolve(fk.plugin) == Some("SomeMod.esp"))
        );
    }

    #[test]
    fn apply_to_record_drops_invalid_repeatable_scalar_formids() {
        let mut interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let null_plugin = interner.intern("__null__");
        let mut record = make_record_with_sig(
            "FLST",
            record_fk,
            vec![
                simple_fk_field("LNAM", valid_master_fk(&mut interner)),
                simple_fk_field("LNAM", invalid_master_fk(&mut interner)),
                simple_fk_field("LNAM", non_master_fk(&mut interner)),
                FieldEntry {
                    sig: SubrecordSig::from_str("LNAM").unwrap(),
                    value: FieldValue::FormKey(FormKey {
                        plugin: null_plugin,
                        local: 0,
                    }),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("LNAM").unwrap(),
                    value: FieldValue::None,
                },
            ],
        );

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, schema.record_def("FLST"), &pred);

        assert!(changed, "invalid FLST.LNAM entries should be dropped");
        assert_eq!(record.fields.len(), 2);
        assert!(matches!(&record.fields[0].value, FieldValue::FormKey(fk) if fk.local == 0x001234));
        assert!(
            matches!(&record.fields[1].value, FieldValue::FormKey(fk) if interner.resolve(fk.plugin) == Some("SomeMod.esp"))
        );
    }

    #[test]
    fn apply_to_record_rewrites_raw_array_struct_formid_to_local_output() {
        let mut interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let record_fk = make_fk("000800", "Output.esm", &mut interner);
        let fallout4 = interner.intern("Fallout4.esm");
        let output = interner.intern("Output.esm");
        let master_names = vec!["Fallout4.esm".to_string()];
        let mut bytes = smallvec::SmallVec::<[u8; 32]>::new();
        for (raw, count) in [
            (0x0000_1234u32, 1u32),
            (0x0000_9000u32, 2u32),
            (0x0000_A000u32, 3u32),
        ] {
            bytes.extend_from_slice(&raw.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
        }
        let fvpa = FieldEntry {
            sig: SubrecordSig::from_str("FVPA").unwrap(),
            value: FieldValue::Bytes(bytes),
        };
        let mut record = make_record_with_sig("COBJ", record_fk, vec![fvpa]);

        let changed = apply_to_record_with_resolution(
            &mut record,
            schema.record_def("COBJ"),
            &|fk| {
                if fk.plugin == fallout4 && fk.local == 0x009000 {
                    TargetFkResolution::Rewrite(FormKey {
                        plugin: output,
                        local: fk.local,
                    })
                } else if fk.plugin == fallout4 && fk.local == 0x00A000 {
                    TargetFkResolution::Null
                } else {
                    TargetFkResolution::Keep
                }
            },
            Some(RawFormIdContext {
                master_names: &master_names,
                output_master_index: 7,
                interner: &interner,
            }),
            false,
        );

        assert!(changed, "raw FVPA formids should be rewritten");
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("FVPA should stay raw bytes");
        };
        let raws: Vec<u32> = bytes
            .chunks_exact(8)
            .map(|row| u32::from_le_bytes(row[0..4].try_into().unwrap()))
            .collect();
        assert_eq!(raws, vec![0x0000_1234, 0x0700_9000, 0x0000_0000]);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_keeps_non_master_fk() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let field = simple_fk_field("DNAM", non_master_fk(&mut interner));
        let mut record = make_record_with_fields(record_fk, vec![field]);

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, None, &pred);
        assert!(!changed, "non-master FK must not be nullified");
    }

    // -----------------------------------------------------------------------
    //
    // This test verifies that after apply_to_record nullifies invalid FKs,
    // a second pass returns false (is_no_op equivalent).  It mirrors the
    // convergent() guarantee in the fixup trait.
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_converges_after_one_pass() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let bad_field = simple_fk_field("DNAM", invalid_master_fk(&mut interner));
        let mut record = make_record_with_fields(record_fk, vec![bad_field]);

        let pred = master_invalid_pred(&mut interner);

        // First pass: should make a change.
        let changed_1 = apply_to_record(&mut record, None, &pred);
        assert!(changed_1, "first pass must nullify the invalid FK");

        // Second pass: already null → predicate returns false → no change.
        let changed_2 = apply_to_record(&mut record, None, &pred);
        assert!(!changed_2, "second pass must be a no-op (convergence)");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_empty_record_no_op() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let mut record = make_record_with_fields(record_fk, vec![]);

        let pred = master_invalid_pred(&mut interner);
        let changed = apply_to_record(&mut record, None, &pred);
        assert!(!changed, "empty record must be a no-op");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn collect_invalid_fks_finds_expected_fks() {
        let mut interner = StringInterner::new();
        let record_fk = make_fk("000800", "Mod.esp", &mut interner);
        let bad1 = invalid_master_fk(&mut interner); // 009000@Fallout4.esm
        let good = valid_master_fk(&mut interner); // 001234@Fallout4.esm
        let bad2 = make_fk("00A000", "Fallout4.esm", &mut interner); // also invalid

        let fields = vec![
            simple_fk_field("DNAM", bad1),
            simple_fk_field("ZNAM", good),
            simple_fk_field("SNAM", bad2),
        ];
        let record = make_record_with_fields(record_fk, fields);

        let pred = master_invalid_pred(&mut interner);
        let found = collect_invalid_fks(&record, &pred, &interner);
        assert_eq!(found.len(), 2, "exactly 2 invalid FKs should be found");
        assert!(found.contains("009000@Fallout4.esm"));
        assert!(found.contains("00A000@Fallout4.esm"));
        assert!(
            !found.contains("001234@Fallout4.esm"),
            "valid FK must not appear"
        );
    }

    // -----------------------------------------------------------------------
    // fix_invalid_target_formkeys runs BEFORE null_dangling and walks struct/list
    // FK leaves generically, so a placed-child target that is "invalid" pre-copy
    // (the exterior child is not copied until phase 6) would be nulled here. With
    // `defer_placed_child=true` this class must be LEFT UNTOUCHED; the post-copy
    // repair resolves it. Models a real-shaped LCTN LCEP row (List<Struct>{ref}).
    // -----------------------------------------------------------------------

    fn lctn_lcep_record(leaf: FormKey, interner: &StringInterner) -> Record {
        let record_fk = FormKey {
            plugin: interner.intern("Output.esm"),
            local: 0x000800,
        };
        let ref_sym = interner.intern("loc_enable_parent_ref");
        let lcep = FieldEntry {
            sig: SubrecordSig::from_str("LCEP").unwrap(),
            value: FieldValue::List(vec![FieldValue::Struct(vec![(
                ref_sym,
                FieldValue::FormKey(leaf),
            )])]),
        };
        make_record_with_sig("LCTN", record_fk, vec![lcep])
    }

    fn lcep_leaf_local(rec: &Record) -> u32 {
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

    /// Resolver that Nulls everything — emulates the pre-copy invalid-target
    /// verdict on a not-yet-copied placed child.
    fn null_everything(_fk: &FormKey) -> TargetFkResolution {
        TargetFkResolution::Null
    }

    #[test]
    fn lctn_lcep_placed_child_nulled_when_not_deferred() {
        // defer=false reproduces the OLD (buggy) behavior: the LCEP ref is nulled.
        let interner = StringInterner::new();
        let leaf = make_fk("7ACB4D", "Output.esm", &interner);
        let mut rec = lctn_lcep_record(leaf, &interner);
        let changed =
            apply_to_record_with_resolution(&mut rec, None, &null_everything, None, false);
        assert!(changed, "without deferral the placed-child ref is nulled");
        assert_eq!(lcep_leaf_local(&rec), 0);
    }

    #[test]
    fn lctn_lcep_placed_child_deferred_left_intact() {
        // defer=true is the FIX: the placed-ref-target class is skipped entirely,
        // so the LCEP ref survives for the post-copy repair. This FAILS on the
        // pre-fix code (the class was always walked + nulled).
        let interner = StringInterner::new();
        let leaf = make_fk("7ACB4D", "Output.esm", &interner);
        let mut rec = lctn_lcep_record(leaf, &interner);
        let changed = apply_to_record_with_resolution(&mut rec, None, &null_everything, None, true);
        assert!(!changed, "deferred class must be untouched pre-copy");
        assert_eq!(
            lcep_leaf_local(&rec),
            0x7ACB4D,
            "ref left intact for repair"
        );
    }

    // -----------------------------------------------------------------------
    // FACT VENC merchant container — the same placed-child deferral. The
    // container is a REFR re-inserted post-copy; pre-copy it looks dangling, so
    // without the deferral this pass nulls it and the type-validator then strips
    // the present-but-null VENC (every converted vendor loses its container).
    // -----------------------------------------------------------------------

    fn fact_venc_record(leaf: FormKey, interner: &StringInterner) -> Record {
        let record_fk = FormKey {
            plugin: interner.intern("Output.esm"),
            local: 0x000844,
        };
        let venc = FieldEntry {
            sig: SubrecordSig::from_str("VENC").unwrap(),
            value: FieldValue::FormKey(leaf),
        };
        make_record_with_sig("FACT", record_fk, vec![venc])
    }

    fn venc_leaf_local(rec: &Record) -> u32 {
        let e = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "VENC")
            .unwrap();
        let FieldValue::FormKey(f) = &e.value else {
            panic!()
        };
        f.local
    }

    #[test]
    fn fact_venc_merchant_container_nulled_when_not_deferred() {
        // defer=false reproduces the OBSERVED bug (drop_trace: typecheck.strip
        // present-but-null VENC): this pass nulls the merchant-container FK.
        let interner = StringInterner::new();
        let leaf = make_fk("629E0C", "Output.esm", &interner);
        let mut rec = fact_venc_record(leaf, &interner);
        let changed =
            apply_to_record_with_resolution(&mut rec, None, &null_everything, None, false);
        assert!(
            changed,
            "without deferral the merchant container ref is nulled"
        );
        assert_eq!(venc_leaf_local(&rec), 0);
    }

    #[test]
    fn fact_venc_merchant_container_deferred_left_intact() {
        // defer=true is the FIX: VENC is in the placed-child-target class, so it's
        // skipped pre-copy and survives for the post-copy repair (where the
        // container REFR is present). FAILS on the pre-fix code (always nulled).
        let interner = StringInterner::new();
        let leaf = make_fk("629E0C", "Output.esm", &interner);
        let mut rec = fact_venc_record(leaf, &interner);
        let changed = apply_to_record_with_resolution(&mut rec, None, &null_everything, None, true);
        assert!(!changed, "deferred FACT VENC must be untouched pre-copy");
        assert_eq!(
            venc_leaf_local(&rec),
            0x629E0C,
            "VENC left intact for repair"
        );
    }
}
