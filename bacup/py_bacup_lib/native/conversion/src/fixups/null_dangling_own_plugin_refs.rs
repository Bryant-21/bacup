//! Fixup: null or repair reference slots whose target was never emitted into the
//! output plugin, because the FO76 source record lives outside the converted slice
//! or has no FO4 equivalent.
//!
//! The FO76→FO4 conversion ports only the Appalachia exterior (terrain cells and
//! their placed children). Carried base records (LCTN, REFR, WRLD, NPC_, PACK, FACT)
//! still reference placed REFR/ACHR records in the dropped interiors. The translate
//! remap rewrites those to own-plugin (07) FormKeys that are never emitted, and
//! xEdit reports "Could not be resolved" on:
//!   * LCTN `LCEP` Ref + Actor (~1459): interior enable-parent markers
//!     (e.g. `LocBurnHighwayTownInteriorLocation`).
//!   * PACK `PLDT`/`PTDA`/`PDTO` + FACT `PLVD` (~636): targets in interior REFRs or
//!     dropped bases (e.g. PTDA `FeedFish04`).
//!   * REFR `XCZR` Current-Zone-Ref, `XRFG` Reference-Group (interior REFRs).
//!   * WRLD `WNAM` Parent-Worldspace (a synthesized id present in neither game).
//!   * NPC_ `CNTO` Item: the FO76 `CNCY Caps001` currency, which has no FO4 record
//!     (FO4 caps are a MISC under a different id).
//!
//! Emitting the ~8,700 interior cells (~1.9M placed children) is out of scope, so an
//! absent target becomes NULL (`local = 0`). A FK that resolves in a FO4 master
//! (e.g. PTDA `000DF42E` CombatRifle) is kept byte-identical. This complements
//! `fix_invalid_target_formkeys`, whose `is_invalid_target_fk` checks only target
//! masters, never the output plugin, and never visits LCTN in whole-plugin runs.
//!
//! Master-byte truncation is repaired instead of nulled: REFR `XTNM` `00510AF5`
//! addresses Fallout4.esm, but the MESG it names (`LC104DoorOverride_TurbineHall`)
//! was emitted at `07510AF5`. When a dangling leaf's object-id exists in the output,
//! its plugin sym is repaired (like `null_dangling_misc_refs`'s SNDR repair). This
//! check runs before the null decision.
//!
//! Each decision checks the leaf's full `(plugin, object_id)` against the object-id
//! set of the addressed handle, collected over ALL signatures by
//! `local_object_ids_in_handle`. A per-sig set would false-positive thousands of
//! valid refs (a CNTO item may be MISC/AMMO/WEAP/...).

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::remap_struct_internal_formids::union_type_holds_formid;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const FO4_MASTER_NAME: &str = "Fallout4.esm";
const FO4_PLAYER_REF_FORM_ID: u32 = 0x000014;

/// `(record sig, subrecord sig)` whose FormKey leaves are checked. Each subrecord
/// here decodes to a typed `FieldValue::FormKey` (codec `formid`) or, for
/// LCEP/LCUN/ACEP, a `List<Struct>` of FormKey leaves (see `source_read.rs`
/// `decode_lctn_lcep`/`decode_lctn_lcun`). Restricting to this allow-list keeps
/// the fixup from touching unrelated FK fields and bounds the per-record walk.
const TOUCHED_SUBRECORDS: &[(&str, &str)] = &[
    ("LCTN", "LCEP"),
    ("LCTN", "ACEP"),
    ("LCTN", "LCUN"),
    ("LCTN", "MNAM"),
    ("CELL", "XILW"),
    ("CELL", "XOWN"),
    ("REFR", "XCZR"),
    ("REFR", "XTNM"),
    ("REFR", "XRFG"),
    // REFR Teleport-Destination (codec `struct:I,f×6,I,I`: `door`→REFR @0,
    // `transition_interior`→CELL @32). The door is almost always a persistent placed
    // REFR (source flag 0x400) that only the persistent-cell phase emits, alongside
    // the phase-6 copy. The pre-copy pass would null the not-yet-emitted door, so
    // XTEL is deferred (see `DEFERRED_PLACED_CHILD_SUBRECORDS`) and resolved
    // post-copy. Not drop-on-null: dropping XTEL loses the teleport link.
    ("REFR", "XTEL"),
    ("WRLD", "WNAM"),
    ("WRLD", "NAM3"),
    ("CONT", "CNTO"),
    ("CONT", "COCT"),
    ("NPC_", "CNTO"),
    ("NPC_", "COCT"),
    // Drop-on-null formid slots: a NULL/unresolvable leaf here is
    // rejected by the FO4 grammar ("Found a NULL reference, expected: X"), so the
    // whole subrecord is DROPPED rather than left as a null leaf (see
    // `DROP_ON_NULL_SUBRECORDS`).
    ("INFO", "DNAM"),
    ("SCEN", "TNAM"),
    ("SCEN", "PTOP"),
    ("SCEN", "NTOP"),
    ("SCEN", "NETO"),
    ("SCEN", "QTOP"),
    ("SCEN", "NPOT"),
    ("SCEN", "NNGT"),
    ("SCEN", "NNUT"),
    ("SCEN", "NQUT"),
    // DIAL Dialogue-Branch is optional. If its DLBR target is pruned because the
    // required starting topic was not emitted, omit BNAM rather than leave a
    // dangling reference behind.
    ("DIAL", "BNAM"),
    // PACK/FACT value-selected-union ref slots. They decode to opaque `Bytes` (the
    // generic decoder can't evaluate the `type` selector), so `null_union_slot`
    // handles them by byte offset. `remap_value_selected_union_formids` already
    // rewrote the resolvable ones 00→07; targets that resolve nowhere (interior
    // REFRs, dropped bases) are nulled.
    ("PACK", "PTDA"),
    ("PACK", "PLDT"),
    ("PACK", "PDTO"),
    ("FACT", "PLVD"),
    // FACT VENC "Merchant Container" → REFR, a placed child re-inserted post-copy by
    // the cell-slice / interior-cell phases, so it looks dangling pre-copy. Without
    // deferral, `fix_invalid_target_formkeys` nulls it and
    // `validate_reference_target_types` strips the null VENC, and every converted
    // vendor faction loses its container. Dropped post-copy only if genuinely absent.
    ("FACT", "VENC"),
    // QUST alias Forced-Reference (repeatable in the `aliases` scope, so one or more
    // FormKey leaves). Its target is almost always a worldspace persistent ref
    // (REFR/ACHR, source flag 0x400) in the WRLD-embedded persistent cell, which only
    // the persistent-cell phase emits, so ALFR is deferred and resolved post-copy.
    // Null leaves stay (dropping one ALFR corrupts the repeatable alias block); a
    // null ALFR is CK-benign.
    ("QUST", "ALFR"),
    // RFGP Reference-Group anchor: RNAM points at its member REFR, a persistent
    // placed child (source flag 0x400) re-inserted only by the phase-6 cell-slice /
    // persistent-cell copy after the pre-copy fixups. Pre-copy, the sweep's
    // skip-record-sig rule nulls the leaf even though the REFR exists post-copy
    // (e.g. 002150→0017C2, 866182→866161, 864038→86402B), so RNAM is deferred. Not
    // drop-on-null: a null RFGP leaf is CK-benign, like XTEL/ALFR.
    ("RFGP", "RNAM"),
];

/// `(record sig, subrecord sig)` of the placed-ref-target class: refs to exterior
/// placed children (ACHR/REFR/...) or worldspace persistent refs (source flag 0x400)
/// that whole-plugin FO76→FO4 worldspace runs emit only in the phase-6 cell-slice
/// copy and the persistent-cell phase, after this fixup. With
/// `FixupConfig::defer_placed_child_ref_class` set, the pre-copy pass leaves them
/// intact and `repair_placed_child_refs` resolves them over the complete output. A
/// genuine interior dangler (e.g. FeedFish04) is still nulled then.
///
/// `(LCTN, MNAM)` is also drop-on-null; deferral moves that drop decision post-copy.
/// `(CELL, XILW|XOWN)`: interior CELL records are emitted after the registered fixup
/// pass, so their exact-type validation waits for the post-copy repair.
const DEFERRED_PLACED_CHILD_SUBRECORDS: &[(&str, &str)] = &[
    ("LCTN", "LCEP"),
    ("LCTN", "ACEP"),
    ("LCTN", "LCUN"),
    ("LCTN", "MNAM"),
    ("QUST", "ALFR"),
    ("REFR", "XTEL"),
    ("CELL", "XILW"),
    ("CELL", "XOWN"),
    // PACK target/location refs can point at interior placed furniture copied
    // after the pre-copy sweep. Resolve them only once that copy is complete.
    ("PACK", "PTDA"),
    ("PACK", "PLDT"),
    // FACT VENC merchant container; see `TOUCHED_SUBRECORDS`.
    ("FACT", "VENC"),
    // RFGP RNAM Reference-Group anchor; see `TOUCHED_SUBRECORDS`.
    ("RFGP", "RNAM"),
];

/// True when `(record_sig, sub_sig)` is the placed-ref-target class that is
/// deferred to the post-copy repair in whole-plugin FO76→FO4 worldspace runs.
/// Public so `fix_invalid_target_formkeys` (which runs FIRST and would otherwise
/// null these leaves as "invalid" while the targets are not yet copied) gates on
/// the SAME definition — keeping the deferral consistent across both passes.
pub fn is_deferred_placed_child(record_sig: &str, sub_sig: &str) -> bool {
    DEFERRED_PLACED_CHILD_SUBRECORDS
        .iter()
        .any(|(r, s)| *r == record_sig && *s == sub_sig)
}

/// Subrecords whose value is an opaque value-selected union (`[i32 type][fk@4]`)
/// rather than a typed FormKey leaf. Handled by `null_union_slot`. Which `type`
/// selector marks offset 4 as a FormID is shared with pack's remap via
/// `remap_struct_internal_formids::union_type_holds_formid` (the canonical
/// source) so the remap and this null pass agree by construction.
const UNION_SLOT_SUBRECORDS: &[&str] = &["PTDA", "PLDT", "PDTO", "PLVD"];

/// PACK package-data union `type` selector for the *Reference* variant — the one
/// the CK reports as "Package Location/Target Reference (00000000)" when its
/// offset-4 FK is null. Shared between PLDT/PLVD (location) and PTDA (target).
const PACK_UNION_REFERENCE_TYPE: i32 = 0;
/// Benign non-reference replacement for a null-valued PLDT/PLVD *location*:
/// "Near Package Start Location" (a self-relative location that needs no external
/// reference; its 4-byte value is cpIgnore). Mirrors the translator's
/// `neutralize_dangling_package_alias_targets` location replacement.
const PACK_LOCATION_NEAR_PACKAGE_START_TYPE: i32 = 2;
/// Benign non-reference replacement for a null-valued PTDA *target*: "Self"
/// (needs no external reference). Mirrors the translator's target replacement.
const PACK_TARGET_SELF_TYPE: i32 = 6;

/// `(record sig, subrecord sig)` slots whose FO4 schema forbids NULL: xEdit reports
/// "Found a NULL reference, expected: <type>" on a zeroed leaf. All are optional in
/// the FO4 grammar (INFO.DNAM Shared-INFO, WRLD.WNAM Parent Worldspace, WRLD.NAM3 LOD
/// Water, LCTN.MNAM World-Location-Marker, CELL.XILW Exterior-LOD Worldspace,
/// CELL.XOWN Owner, SCEN.TNAM Template-Scene), so a leaf that resolves to `Null`
/// drops its whole `FieldEntry`. CELL.XOWN may stay an opaque ownership struct; the
/// CELL-specific path resolves its leading FormID.
///
/// DLBR.SNAM Starting-Topic is not here: FO4 DLBR requires it, so `run_resolution`
/// drops the whole DLBR when the target DIAL is absent or has the wrong signature.
const DROP_ON_NULL_SUBRECORDS: &[(&str, &str)] = &[
    ("LCTN", "MNAM"),
    ("CELL", "XILW"),
    ("CELL", "XOWN"),
    ("INFO", "DNAM"),
    ("WRLD", "WNAM"),
    ("WRLD", "NAM3"),
    ("SCEN", "TNAM"),
    ("SCEN", "PTOP"),
    ("SCEN", "NTOP"),
    ("SCEN", "NETO"),
    ("SCEN", "QTOP"),
    ("SCEN", "NPOT"),
    ("SCEN", "NNGT"),
    ("SCEN", "NNUT"),
    ("SCEN", "NQUT"),
    ("DIAL", "BNAM"),
    // FACT VENC Merchant Container — OPTIONAL in the FO4 FACT grammar (present only
    // on vendor factions; the type-validator uses a Strip action, i.e. not required
    // and NULL-disallowed). When the container REFR is genuinely absent post-copy
    // (vendor whose container lives in an unconverted cell) the FO4-correct shape is
    // to OMIT VENC, not keep a NULL leaf xEdit rejects.
    ("FACT", "VENC"),
];

fn is_drop_on_null(record_sig: &str, sub_sig: &str) -> bool {
    DROP_ON_NULL_SUBRECORDS
        .iter()
        .any(|(r, s)| *r == record_sig && *s == sub_sig)
}

/// `CNTO` Item rows are `struct:I,i` (item FormID @ offset 0, count) and decode
/// to opaque `Bytes` after a plugin round trip. The FO76
/// `CNCY Caps001` currency (`0700000F`) has no FO4 record, so the item FormID
/// dangles. FO4 rejects a NULL/unresolvable CNTO item, so the whole CNTO
/// subrecord is dropped (count travels with it — inherently lockstep).
const CNTO_ITEM_OFFSET: usize = 0;

fn touched_record_sigs() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = TOUCHED_SUBRECORDS.iter().map(|(r, _)| *r).collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn record_touches_subrecord(record_sig: &str, sub_sig: &str) -> bool {
    TOUCHED_SUBRECORDS
        .iter()
        .any(|(r, s)| *r == record_sig && *s == sub_sig)
}

fn touched_subrecords_for(record_sig: &str) -> Vec<&'static str> {
    TOUCHED_SUBRECORDS
        .iter()
        .filter(|(r, _)| *r == record_sig)
        .map(|(_, s)| *s)
        .collect()
}

/// Which slice of the touched-subrecord allow-list a resolution pass acts on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ApplyMode {
    /// Pre-copy pass (the registered fixup). When `defer_placed_child = true`,
    /// the placed-ref-target class (LCTN LCUN/LCEP/ACEP) is LEFT UNTOUCHED so a
    /// later post-copy repair resolves it against the complete output plugin.
    PreCopy { defer_placed_child: bool },
    /// Post-copy repair (`repair_placed_child_refs`). Resolves ONLY the deferred
    /// placed-ref-target class against the now-complete output plugin.
    PostCopyPlacedChild,
}

impl ApplyMode {
    /// Should `(record_sig, sub_sig)` be processed in this mode?
    fn processes(self, record_sig: &str, sub_sig: &str) -> bool {
        match self {
            ApplyMode::PreCopy { defer_placed_child } => {
                !(defer_placed_child && is_deferred_placed_child(record_sig, sub_sig))
            }
            ApplyMode::PostCopyPlacedChild => is_deferred_placed_child(record_sig, sub_sig),
        }
    }
}

/// Resolve every dangling FK leaf in the touched records of `session`, restricted
/// to the slice of the allow-list selected by `mode`. Shared by the pre-copy
/// fixup and the post-copy placed-child repair so both use the identical
/// `LeafResolver`/`apply_to_record` logic over their respective output plugin.
fn run_resolution(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    mode: ApplyMode,
) -> Result<FixupReport, FixupError> {
    use rayon::prelude::*;

    let mut report = FixupReport::empty();
    let target_schema = config
        .target_schema
        .as_deref()
        .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

    let resolver = LeafResolver::build(session, config, mapper.interner)?;
    let (dropped_dlbrs, repaired_dlbrs) =
        prune_invalid_dlbr_records(session, target_schema, mapper.interner, &resolver)?;
    report.records_dropped = dropped_dlbrs.try_into().unwrap_or(u32::MAX);
    report.records_changed = repaired_dlbrs.try_into().unwrap_or(u32::MAX);

    // Rebuild after pruning so optional incoming DIAL.BNAM references observe
    // the now-authoritative output object set and are dropped in this same pass.
    let resolver = LeafResolver::build(session, config, mapper.interner)?;
    // No output records and no masters indexed — nothing resolvable, bail.
    if resolver.output_objids.is_empty() {
        return Ok(report);
    }

    // Diagnostic count of records that CARRY a deferred placed-child subrecord
    // (LCTN LCEP/ACEP/LCUN). Behaviour-free — used only for the trace line below.
    // For PreCopy this is the count left untouched when defer is on; for the
    // repair it is the count examined.
    let (deferred_present, changed_records) = {
        let view = session
            .target_read_view()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let available: FxHashSet<SigCode> = view.target_signatures().into_iter().collect();
        let mut deferred_present = 0u32;
        let mut changed_records = Vec::new();

        for sig_str in touched_record_sigs() {
            let Ok(sig) = SigCode::from_str(sig_str) else {
                continue;
            };
            if !available.contains(&sig) {
                continue;
            }
            let sub_sigs: Vec<&'static str> = touched_subrecords_for(sig_str)
                .into_iter()
                .filter(|s| mode.processes(sig_str, s))
                .collect();
            let deferred_sub_sigs: Vec<&'static str> = touched_subrecords_for(sig_str)
                .into_iter()
                .filter(|s| is_deferred_placed_child(sig_str, s))
                .collect();
            if sub_sigs.is_empty() && deferred_sub_sigs.is_empty() {
                continue;
            }

            let fks = view.form_keys_of_sig(sig, mapper.interner);
            let inspect = |fk: &FormKey| {
                let has_deferred = !deferred_sub_sigs.is_empty()
                    && view.record_has_any_subrecord(fk, &deferred_sub_sigs, mapper.interner);
                if sub_sigs.is_empty()
                    || !view.record_has_any_subrecord(fk, &sub_sigs, mapper.interner)
                {
                    return (has_deferred, None);
                }
                let Ok(mut record) = view.record_decoded(fk, target_schema, mapper.interner) else {
                    return (has_deferred, None);
                };
                let changed = apply_to_record(&mut record, &resolver, mapper.interner, mode);
                (has_deferred, changed.then_some(record))
            };
            let inspected: Vec<(bool, Option<Record>)> = if fks.len() < 64 {
                fks.iter().map(inspect).collect()
            } else {
                fks.par_iter().map(inspect).collect()
            };
            for (has_deferred, changed) in inspected {
                deferred_present = deferred_present.saturating_add(has_deferred as u32);
                changed_records.extend(changed);
            }
        }

        (deferred_present, changed_records)
    };

    match mode {
        ApplyMode::PreCopy { defer_placed_child } => eprintln!(
            "[trace_defer] null_dangling: defer={defer_placed_child} skipped={}",
            if defer_placed_child {
                deferred_present
            } else {
                0
            }
        ),
        ApplyMode::PostCopyPlacedChild => eprintln!(
            "[trace_defer] repair: examined={deferred_present} changed={}",
            changed_records.len()
        ),
    }

    let changed_records = dedupe_records_by_form_key(changed_records);
    let expected = changed_records.len();
    if expected == 0 {
        return Ok(report);
    }
    let replaced = session
        .replace_records_contents(changed_records, target_schema, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if replaced != expected {
        return Err(FixupError::HandleError(format!(
            "null_dangling_own_plugin_refs replaced {replaced} of {expected} expected records"
        )));
    }
    report.records_changed = report
        .records_changed
        .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
    Ok(report)
}

fn prune_invalid_dlbr_records(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
    resolver: &LeafResolver,
) -> Result<(usize, usize), FixupError> {
    let dlbr_sig = SigCode::from_str("DLBR")
        .map_err(|err| FixupError::Other(format!("invalid DLBR signature: {err}")))?;
    let dlbr_fks = session
        .form_keys_of_sig(dlbr_sig, interner)
        .map_err(|err| FixupError::HandleError(err.to_string()))?;
    let mut invalid = Vec::new();
    let mut repaired = Vec::new();

    for fk in dlbr_fks {
        let mut record = match session.record_decoded(&fk, target_schema, interner) {
            Ok(record) => record,
            Err(_) => continue,
        };
        match resolve_dlbr_starting_topic(&mut record, resolver, interner) {
            DlbrStartingTopicResolution::Keep => {}
            DlbrStartingTopicResolution::RepairToOutput => repaired.push(record),
            DlbrStartingTopicResolution::Invalid => invalid.push(fk),
        }
    }

    let dropped = session
        .remove_records(&invalid)
        .map_err(|err| FixupError::HandleError(err.to_string()))?;
    let expected_repaired = repaired.len();
    let replaced = session
        .replace_records_contents(repaired, target_schema, interner)
        .map_err(|err| FixupError::HandleError(err.to_string()))?;
    if replaced != expected_repaired {
        return Err(FixupError::HandleError(format!(
            "null_dangling_own_plugin_refs repaired {replaced} of {expected_repaired} expected DLBR records"
        )));
    }
    Ok((dropped, replaced))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DlbrStartingTopicResolution {
    Keep,
    RepairToOutput,
    Invalid,
}

fn resolve_dlbr_starting_topic(
    record: &mut Record,
    resolver: &LeafResolver,
    interner: &StringInterner,
) -> DlbrStartingTopicResolution {
    let Some(starting_topic) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "SNAM")
        .and_then(|entry| first_formkey_mut(&mut entry.value))
    else {
        return DlbrStartingTopicResolution::Invalid;
    };
    let resolution = resolver.resolve_dial(starting_topic, interner);
    if resolution == DlbrStartingTopicResolution::RepairToOutput {
        starting_topic.plugin = interner.intern(&resolver.output_plugin);
    }
    resolution
}

fn dedupe_records_by_form_key(records: Vec<Record>) -> Vec<Record> {
    let mut positions: FxHashMap<FormKey, usize> = FxHashMap::default();
    let mut deduped = Vec::with_capacity(records.len());
    for record in records {
        if let Some(&idx) = positions.get(&record.form_key) {
            deduped[idx] = record;
        } else {
            positions.insert(record.form_key, deduped.len());
            deduped.push(record);
        }
    }
    deduped
}

/// Post-copy resolution of the deferred placed-ref-target class
/// (`DEFERRED_PLACED_CHILD_SUBRECORDS`) against the complete output, called after
/// the phase-6 cell-slice copy and cell-location sync. Present targets are kept; a
/// genuine interior dangler (e.g. FeedFish04) is nulled and its LCUN row dropped in
/// lockstep. Effectively a no-op when the pre-copy pass did not defer the class.
pub fn repair_placed_child_refs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = run_resolution(session, mapper, config, ApplyMode::PostCopyPlacedChild)?;
    if config.defer_placed_child_ref_class {
        let ownership =
            crate::fixups::normalize_placed_records::normalize_ownership_xown_payloads_in_session(
                session,
            );
        report.records_changed = report
            .records_changed
            .saturating_add(ownership.records_changed);
    }
    Ok(report)
}

pub struct NullDanglingOwnPluginRefsFixup;

impl Fixup for NullDanglingOwnPluginRefsFixup {
    fn name(&self) -> &'static str {
        "null_dangling_own_plugin_refs"
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
        run_resolution(
            session,
            mapper,
            config,
            ApplyMode::PreCopy {
                defer_placed_child: config.defer_placed_child_ref_class,
            },
        )
    }
}

/// Resolves a decoded `FormKey` leaf against the authoritative object-id sets of
/// the output plugin and each target master.
struct LeafResolver {
    /// Every object-id present in the output plugin (all signatures).
    output_objids: FxHashSet<u32>,
    /// Per target-master object-id set, indexed by master load order.
    master_objids: Vec<FxHashSet<u32>>,
    /// DIAL-only object-id sets parallel to the general object-id sets. DLBR's
    /// required SNAM must resolve to this exact signature, not merely any form.
    output_dial_objids: FxHashSet<u32>,
    master_dial_objids: Vec<FxHashSet<u32>>,
    output_wrld_objids: FxHashSet<u32>,
    master_wrld_objids: Vec<FxHashSet<u32>>,
    output_owner_objids: FxHashSet<u32>,
    master_owner_objids: Vec<FxHashSet<u32>>,
    /// Target master names, parallel to `master_objids`, for matching a leaf's
    /// plugin sym to a master.
    master_names: Vec<String>,
    /// Output plugin name (a leaf whose plugin == this addresses the output).
    output_plugin: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LeafResolution {
    /// Resolves in its addressed handle (or is null) — leave unchanged.
    Keep,
    /// Object-id exists in the output plugin but the leaf addresses a master
    /// (truncated/mis-prefixed master byte) — repoint the plugin sym to output.
    RepairToOutput,
    /// Resolves nowhere — null it (`local = 0`).
    Null,
}

impl LeafResolver {
    fn build(
        session: &mut PluginSession,
        config: &FixupConfig,
        interner: &StringInterner,
    ) -> Result<Self, FixupError> {
        let master_names = session.target_masters().to_vec();
        let output_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_id = session.target_id();
        let output_objids = session
            .local_object_ids_in_handle(target_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut master_objids = Vec::with_capacity(config.target_master_handle_ids.len());
        for &handle_id in &config.target_master_handle_ids {
            let set = session
                .local_object_ids_in_handle(handle_id)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            master_objids.push(set);
        }
        let dial_sig = SigCode::from_str("DIAL")
            .map_err(|err| FixupError::Other(format!("invalid DIAL signature: {err}")))?;
        let output_dial_objids = session
            .form_keys_of_sig(dial_sig, interner)
            .map_err(|err| FixupError::HandleError(err.to_string()))?
            .into_iter()
            .map(|fk| fk.local & 0x00FF_FFFF)
            .collect();
        let mut master_dial_objids = Vec::with_capacity(config.target_master_handle_ids.len());
        for &handle_id in &config.target_master_handle_ids {
            let ids = session
                .form_keys_of_sig_in_handle(handle_id, dial_sig, interner)
                .map_err(|err| FixupError::HandleError(err.to_string()))?
                .into_iter()
                .map(|fk| fk.local & 0x00FF_FFFF)
                .collect();
            master_dial_objids.push(ids);
        }
        let output_wrld_objids = object_ids_of_signatures(session, None, &["WRLD"], interner)?;
        let output_owner_objids =
            object_ids_of_signatures(session, None, &["FACT", "NPC_"], interner)?;
        let mut master_wrld_objids = Vec::with_capacity(config.target_master_handle_ids.len());
        let mut master_owner_objids = Vec::with_capacity(config.target_master_handle_ids.len());
        for &handle_id in &config.target_master_handle_ids {
            master_wrld_objids.push(object_ids_of_signatures(
                session,
                Some(handle_id),
                &["WRLD"],
                interner,
            )?);
            master_owner_objids.push(object_ids_of_signatures(
                session,
                Some(handle_id),
                &["FACT", "NPC_"],
                interner,
            )?);
        }
        Ok(Self {
            output_objids,
            master_objids,
            output_dial_objids,
            master_dial_objids,
            output_wrld_objids,
            master_wrld_objids,
            output_owner_objids,
            master_owner_objids,
            master_names,
            output_plugin,
        })
    }

    fn exists_in_output(&self, object_id: u32) -> bool {
        self.output_objids.contains(&object_id)
    }

    fn exists_in_master(&self, plugin_name: &str, object_id: u32) -> bool {
        let master_index = self
            .master_names
            .iter()
            .position(|m| m.eq_ignore_ascii_case(plugin_name));
        // PlayerRef is engine-defined and may be absent from the flat master
        // record index used to build `master_objids`.
        if plugin_name.eq_ignore_ascii_case(FO4_MASTER_NAME) && object_id == FO4_PLAYER_REF_FORM_ID
        {
            return master_index.is_some();
        }
        master_index
            .and_then(|idx| self.master_objids.get(idx))
            .is_some_and(|set| set.contains(&object_id))
    }

    fn first_master_index_with_object_id(&self, object_id: u32) -> Option<usize> {
        self.master_objids
            .iter()
            .position(|set| set.contains(&object_id))
    }

    fn resolve(&self, fk: &FormKey, interner: &StringInterner) -> LeafResolution {
        if fk.local == 0 {
            return LeafResolution::Keep;
        }
        let object_id = fk.local & 0x00FF_FFFF;
        let Some(plugin_name) = interner.resolve(fk.plugin) else {
            return LeafResolution::Keep;
        };
        let addresses_output = plugin_name.eq_ignore_ascii_case(&self.output_plugin);
        if addresses_output {
            // Own-plugin leaf: keep iff the record was actually emitted.
            return if self.exists_in_output(object_id) {
                LeafResolution::Keep
            } else {
                LeafResolution::Null
            };
        }
        // Master-addressed leaf: keep iff it resolves in that master.
        if self.exists_in_master(plugin_name, object_id) {
            return LeafResolution::Keep;
        }
        // Doesn't resolve in its addressed master. If the object-id was emitted
        // in the output plugin, the master byte was truncated/mis-prefixed —
        // repair it. Otherwise it dangles nowhere — null it.
        if self.exists_in_output(object_id) {
            LeafResolution::RepairToOutput
        } else {
            LeafResolution::Null
        }
    }

    fn resolve_exact(
        &self,
        fk: &FormKey,
        interner: &StringInterner,
        output_objids: &FxHashSet<u32>,
        master_objids: &[FxHashSet<u32>],
    ) -> LeafResolution {
        if fk.local == 0 {
            return LeafResolution::Null;
        }
        let object_id = fk.local & 0x00FF_FFFF;
        let Some(plugin_name) = interner.resolve(fk.plugin) else {
            return LeafResolution::Null;
        };
        if plugin_name.eq_ignore_ascii_case(&self.output_plugin) {
            return if output_objids.contains(&object_id) {
                LeafResolution::Keep
            } else {
                LeafResolution::Null
            };
        }
        if self
            .master_names
            .iter()
            .position(|master| master.eq_ignore_ascii_case(plugin_name))
            .and_then(|index| master_objids.get(index))
            .is_some_and(|ids| ids.contains(&object_id))
        {
            return LeafResolution::Keep;
        }
        if output_objids.contains(&object_id) {
            LeafResolution::RepairToOutput
        } else {
            LeafResolution::Null
        }
    }

    fn resolve_exact_raw(
        &self,
        raw: u32,
        output_objids: &FxHashSet<u32>,
        master_objids: &[FxHashSet<u32>],
    ) -> LeafResolution {
        if raw == 0 {
            return LeafResolution::Null;
        }
        let master_index = (raw >> 24) as usize;
        let object_id = raw & 0x00FF_FFFF;
        if master_index == self.master_names.len() {
            return if output_objids.contains(&object_id) {
                LeafResolution::Keep
            } else {
                LeafResolution::Null
            };
        }
        if master_objids
            .get(master_index)
            .is_some_and(|ids| ids.contains(&object_id))
        {
            return LeafResolution::Keep;
        }
        if output_objids.contains(&object_id) {
            LeafResolution::RepairToOutput
        } else {
            LeafResolution::Null
        }
    }

    fn resolve_dial(&self, fk: &FormKey, interner: &StringInterner) -> DlbrStartingTopicResolution {
        if fk.local == 0 {
            return DlbrStartingTopicResolution::Invalid;
        }
        let object_id = fk.local & 0x00FF_FFFF;
        let Some(plugin_name) = interner.resolve(fk.plugin) else {
            return DlbrStartingTopicResolution::Invalid;
        };
        if plugin_name.eq_ignore_ascii_case(&self.output_plugin) {
            return if self.output_dial_objids.contains(&object_id) {
                DlbrStartingTopicResolution::Keep
            } else {
                DlbrStartingTopicResolution::Invalid
            };
        }
        let resolves_in_master = self
            .master_names
            .iter()
            .position(|master| master.eq_ignore_ascii_case(plugin_name))
            .and_then(|index| self.master_dial_objids.get(index))
            .is_some_and(|ids| ids.contains(&object_id));
        if resolves_in_master {
            DlbrStartingTopicResolution::Keep
        } else if self.output_dial_objids.contains(&object_id) {
            DlbrStartingTopicResolution::RepairToOutput
        } else {
            DlbrStartingTopicResolution::Invalid
        }
    }

    /// Resolve a raw `[master_index << 24 | object_id]` FormID (as it sits in an
    /// opaque union byte slot). The output plugin's own master index is the
    /// number of target masters. Mirrors `resolve` but on the encoded form.
    fn resolve_raw(&self, raw: u32) -> LeafResolution {
        if raw == 0 {
            return LeafResolution::Keep;
        }
        let master_index = (raw >> 24) as usize;
        let object_id = raw & 0x00FF_FFFF;
        let output_master_index = self.master_names.len();
        if master_index == output_master_index {
            return if self.exists_in_output(object_id) {
                LeafResolution::Keep
            } else {
                LeafResolution::Null
            };
        }
        if let Some(set) = self.master_objids.get(master_index) {
            if set.contains(&object_id) {
                return LeafResolution::Keep;
            }
        } else {
            // master_index beyond the known masters — can't prove it dangles.
            return LeafResolution::Keep;
        }
        if self.exists_in_output(object_id) {
            LeafResolution::RepairToOutput
        } else {
            LeafResolution::Null
        }
    }
}

fn object_ids_of_signatures(
    session: &mut PluginSession,
    handle_id: Option<u64>,
    signatures: &[&str],
    interner: &StringInterner,
) -> Result<FxHashSet<u32>, FixupError> {
    let mut object_ids = FxHashSet::default();
    for signature in signatures {
        let sig = SigCode::from_str(signature).map_err(|error| {
            FixupError::Other(format!("invalid {signature} signature: {error}"))
        })?;
        let form_keys = match handle_id {
            Some(handle_id) => session.form_keys_of_sig_in_handle(handle_id, sig, interner),
            None => session.form_keys_of_sig(sig, interner),
        }
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
        object_ids.extend(
            form_keys
                .into_iter()
                .map(|form_key| form_key.local & 0x00FF_FFFF),
        );
    }
    Ok(object_ids)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ExactReferenceDecision {
    Keep,
    RepairToOutput,
    Drop,
}

fn resolve_exact_reference_value(
    value: &mut FieldValue,
    resolver: &LeafResolver,
    interner: &StringInterner,
    output_objids: &FxHashSet<u32>,
    master_objids: &[FxHashSet<u32>],
) -> ExactReferenceDecision {
    if let Some(form_key) = first_formkey_mut(value) {
        return match resolver.resolve_exact(form_key, interner, output_objids, master_objids) {
            LeafResolution::Keep => ExactReferenceDecision::Keep,
            LeafResolution::RepairToOutput => {
                form_key.plugin = interner.intern(&resolver.output_plugin);
                ExactReferenceDecision::RepairToOutput
            }
            LeafResolution::Null => ExactReferenceDecision::Drop,
        };
    }

    let FieldValue::Bytes(bytes) = value else {
        return ExactReferenceDecision::Drop;
    };
    let Some(raw_bytes) = bytes.get(0..4) else {
        return ExactReferenceDecision::Drop;
    };
    let raw = u32::from_le_bytes(raw_bytes.try_into().expect("four-byte FormID slot"));
    match resolver.resolve_exact_raw(raw, output_objids, master_objids) {
        LeafResolution::Keep => ExactReferenceDecision::Keep,
        LeafResolution::RepairToOutput => {
            let repaired = raw_form_id(resolver.master_names.len(), raw & 0x00FF_FFFF);
            bytes[0..4].copy_from_slice(&repaired.to_le_bytes());
            ExactReferenceDecision::RepairToOutput
        }
        LeafResolution::Null => ExactReferenceDecision::Drop,
    }
}

fn apply_to_record(
    record: &mut Record,
    resolver: &LeafResolver,
    interner: &StringInterner,
    mode: ApplyMode,
) -> bool {
    let record_sig = record.sig.as_str().to_string();
    let output_sym = interner.intern(&resolver.output_plugin);
    let output_master_index = resolver.master_names.len() as u32;
    let mut changed = false;
    // `retain_mut` so a subrecord/row whose required FormID dangles can be DROPPED
    // (the FO4-correct shape for an absent ref in a non-null-allowed slot) rather
    // than left as a `local = 0` leaf that xEdit rejects.
    record.fields.retain_mut(|entry| {
        let sub_sig = entry.sig.as_str().to_string();
        if !record_touches_subrecord(&record_sig, &sub_sig)
            || !mode.processes(&record_sig, &sub_sig)
        {
            return true;
        }
        if record_sig == "CELL" && matches!(sub_sig.as_str(), "XILW" | "XOWN") {
            let (output_objids, master_objids) = if sub_sig == "XILW" {
                (&resolver.output_wrld_objids, &resolver.master_wrld_objids)
            } else {
                (&resolver.output_owner_objids, &resolver.master_owner_objids)
            };
            return match resolve_exact_reference_value(
                &mut entry.value,
                resolver,
                interner,
                output_objids,
                master_objids,
            ) {
                ExactReferenceDecision::Keep => true,
                ExactReferenceDecision::RepairToOutput => {
                    changed = true;
                    true
                }
                ExactReferenceDecision::Drop => {
                    changed = true;
                    false
                }
            };
        }
        if UNION_SLOT_SUBRECORDS.contains(&sub_sig.as_str()) {
            // Opaque value-selected union `Bytes`: null/repair the FK at offset 4
            // when its `type` selector designates a FormID variant.
            if let FieldValue::Bytes(bytes) = &mut entry.value {
                if null_union_slot(bytes, &sub_sig, resolver, output_master_index) {
                    changed = true;
                }
                // Benignify a now-null Reference-variant location/target so the CK
                // sees valid self-relative data instead of "Reference (00000000)".
                // Catches both this pass's nulls AND the wrong-type FKs zeroed
                // earlier by `validate_reference_target_types` (it runs first).
                if benignify_value0_reference_union(bytes, &sub_sig) {
                    changed = true;
                }
            }
            return true;
        }
        // NPC_ CNTO Item (struct:I,i, opaque Bytes): repair the item FormID if
        // it exists in the output plugin or a target master; drop only rows with
        // truly dangling items (count travels with the item → lockstep).
        if record_sig == "NPC_" && sub_sig == "CNTO" {
            if let FieldValue::Bytes(bytes) = &mut entry.value {
                match repair_or_drop_cnto_item(bytes.as_mut_slice(), resolver) {
                    CntoItemDecision::Keep => {}
                    CntoItemDecision::Changed => changed = true,
                    CntoItemDecision::Drop => {
                        changed = true;
                        return false;
                    }
                }
            }
            return true;
        }
        if record_sig == "CONT" && sub_sig == "CNTO" {
            if let FieldValue::Bytes(bytes) = &mut entry.value {
                if read_cnto_item(bytes.as_slice()).is_none_or(|raw| raw & 0x00FF_FFFF == 0) {
                    changed = true;
                    return false;
                }
                match repair_or_drop_cnto_item(bytes.as_mut_slice(), resolver) {
                    CntoItemDecision::Keep => {}
                    CntoItemDecision::Changed => changed = true,
                    CntoItemDecision::Drop => {
                        changed = true;
                        return false;
                    }
                }
                return true;
            }
            let Some(item) = first_formkey(&entry.value) else {
                changed = true;
                return false;
            };
            if item.local == 0 || resolver.resolve(&item, interner) == LeafResolution::Null {
                changed = true;
                return false;
            }
            if resolve_fk_leaves(&mut entry.value, resolver, interner, output_sym) {
                changed = true;
            }
            return true;
        }
        // LCTN LCUN rows (List<Struct> of {npc, actor_ref→ACHR, location}): drop
        // any row whose actor_ref leaf resolves to NULL (the FO4 grammar's "#N Ref
        // -> Found a NULL reference, expected: ACHR"). The npc/location leaves are
        // still resolved/repaired in surviving rows by `resolve_fk_leaves` below.
        if record_sig == "LCTN" && sub_sig == "LCUN" {
            if drop_null_lcun_rows(&mut entry.value, resolver, interner) {
                changed = true;
            }
            // fall through so surviving rows' other leaves are resolved too
        }
        // Drop-on-null formid slots: if the single FK leaf resolves to NULL, drop
        // the entire subrecord instead of nulling it in place.
        if is_drop_on_null(&record_sig, &sub_sig) {
            if matches!(entry.value, FieldValue::None) {
                changed = true;
                return false;
            }
            if first_formkey(&entry.value)
                .is_some_and(|fk| resolver.resolve(&fk, interner) == LeafResolution::Null)
            {
                changed = true;
                return false;
            }
        }
        if resolve_fk_leaves(&mut entry.value, resolver, interner, output_sym) {
            changed = true;
        }
        true
    });
    if matches!(record_sig.as_str(), "CONT" | "NPC_") && sync_inventory_count(record) {
        changed = true;
    }
    changed
}

fn first_formkey(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::List(items) => items.iter().find_map(first_formkey),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_formkey(value)),
        _ => None,
    }
}

fn first_formkey_mut(value: &mut FieldValue) -> Option<&mut FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(fk),
        FieldValue::List(items) => items.iter_mut().find_map(first_formkey_mut),
        FieldValue::Struct(fields) => fields
            .iter_mut()
            .find_map(|(_, value)| first_formkey_mut(value)),
        _ => None,
    }
}

fn sync_inventory_count(record: &mut Record) -> bool {
    let cnto_count = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "CNTO")
        .count();
    let mut changed = false;
    record.fields.retain_mut(|entry| {
        if entry.sig.as_str() != "COCT" {
            return true;
        }
        if cnto_count == 0 {
            changed = true;
            return false;
        }
        if set_count_value(&mut entry.value, cnto_count) {
            changed = true;
        }
        true
    });
    changed
}

fn set_count_value(value: &mut FieldValue, count: usize) -> bool {
    match value {
        FieldValue::Uint(existing) => {
            let count = count as u64;
            if *existing == count {
                false
            } else {
                *existing = count;
                true
            }
        }
        FieldValue::Int(existing) => {
            let count = count as i64;
            if *existing == count {
                false
            } else {
                *existing = count;
                true
            }
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let count = count.min(u32::MAX as usize) as u32;
            let existing = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if existing == count {
                false
            } else {
                bytes[0..4].copy_from_slice(&count.to_le_bytes());
                true
            }
        }
        _ => {
            *value = FieldValue::Uint(count as u64);
            true
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CntoItemDecision {
    Keep,
    Changed,
    Drop,
}

fn repair_or_drop_cnto_item(bytes: &mut [u8], resolver: &LeafResolver) -> CntoItemDecision {
    let Some(raw) = read_cnto_item(bytes) else {
        return CntoItemDecision::Keep;
    };
    let object_id = raw & 0x00FF_FFFF;
    match resolver.resolve_raw(raw) {
        LeafResolution::Keep => CntoItemDecision::Keep,
        LeafResolution::RepairToOutput => {
            let repaired = raw_form_id(resolver.master_names.len(), object_id);
            if raw == repaired {
                CntoItemDecision::Keep
            } else {
                write_cnto_item(bytes, repaired);
                CntoItemDecision::Changed
            }
        }
        LeafResolution::Null => {
            if let Some(master_index) = resolver.first_master_index_with_object_id(object_id) {
                let repaired = raw_form_id(master_index, object_id);
                if raw == repaired {
                    CntoItemDecision::Keep
                } else {
                    write_cnto_item(bytes, repaired);
                    CntoItemDecision::Changed
                }
            } else {
                CntoItemDecision::Drop
            }
        }
    }
}

fn read_cnto_item(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < CNTO_ITEM_OFFSET + 4 {
        return None;
    }
    Some(u32::from_le_bytes([
        bytes[CNTO_ITEM_OFFSET],
        bytes[CNTO_ITEM_OFFSET + 1],
        bytes[CNTO_ITEM_OFFSET + 2],
        bytes[CNTO_ITEM_OFFSET + 3],
    ]))
}

fn write_cnto_item(bytes: &mut [u8], raw: u32) {
    if bytes.len() >= CNTO_ITEM_OFFSET + 4 {
        bytes[CNTO_ITEM_OFFSET..CNTO_ITEM_OFFSET + 4].copy_from_slice(&raw.to_le_bytes());
    }
}

fn raw_form_id(master_index: usize, object_id: u32) -> u32 {
    ((master_index as u32) << 24) | (object_id & 0x00FF_FFFF)
}

/// Drop every LCUN row (a `Struct` of three FormKey leaves) whose `actor_ref`
/// (the ACHR-typed middle leaf) resolves to NULL. Returns `true` if any row was
/// removed. The actor_ref leaf is identified by field id `*_actor_ref`.
fn drop_null_lcun_rows(
    value: &mut FieldValue,
    resolver: &LeafResolver,
    interner: &StringInterner,
) -> bool {
    let FieldValue::List(rows) = value else {
        return false;
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
            if name.ends_with("actor_ref") {
                if let FieldValue::FormKey(fk) = v {
                    if resolver.resolve(fk, interner) == LeafResolution::Null {
                        return false;
                    }
                }
            }
        }
        true
    });
    rows.len() != before
}

/// Null/repair the FormID at offset 4 of a value-selected-union subrecord
/// (`[i32 type][fk @ 4][...]`) when (a) the `type` selector marks offset 4 as a
/// FormID and (b) the FK resolves in neither the output plugin nor any master.
/// A FormID that already resolves (in-output or in a master) is left untouched;
/// a truncated master byte whose object-id exists in the output is repaired.
fn null_union_slot(
    bytes: &mut [u8],
    sub_sig: &str,
    resolver: &LeafResolver,
    output_master_index: u32,
) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    let kind = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if !union_type_holds_formid(sub_sig, kind) {
        return false;
    }
    let raw = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    match resolver.resolve_raw(raw) {
        LeafResolution::Keep => false,
        LeafResolution::Null => {
            bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
            true
        }
        LeafResolution::RepairToOutput => {
            let repaired = (output_master_index << 24) | (raw & 0x00FF_FFFF);
            bytes[4..8].copy_from_slice(&repaired.to_le_bytes());
            true
        }
    }
}

/// Rewrite a null PLDT/PLVD/PTDA *Reference*-variant union (`[type=0][value=0]`) to
/// a benign self-relative selector, so the FO4 CK stops warning "Package
/// Location/Target Reference (00000000)": PLDT/PLVD → "Near Package Start" (type 2),
/// PTDA → "Self" (type 6). Both treat offset 4 as cpIgnore. Other variants and
/// non-null references are untouched. The translator's
/// `neutralize_dangling_package_alias_targets` covers the alias variants
/// ({8,9,14}/{4}); this covers reference variants nulled by `null_union_slot` or by
/// `validate_reference_target_types` (registered before this fixup). PDTO is
/// excluded: its null is a Topic Data variant the CK does not flag.
fn benignify_value0_reference_union(bytes: &mut [u8], sub_sig: &str) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    let replacement = match sub_sig {
        "PLDT" | "PLVD" => PACK_LOCATION_NEAR_PACKAGE_START_TYPE,
        "PTDA" => PACK_TARGET_SELF_TYPE,
        _ => return false,
    };
    let kind = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let value = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if kind != PACK_UNION_REFERENCE_TYPE || value != 0 {
        return false;
    }
    bytes[0..4].copy_from_slice(&replacement.to_le_bytes());
    true
}

/// Recursively walk a field value, resolving every `FormKey` leaf. Nulls the
/// leaf (`local = 0`, plugin sym preserved) or repoints it to the output plugin.
fn resolve_fk_leaves(
    value: &mut FieldValue,
    resolver: &LeafResolver,
    interner: &StringInterner,
    output_sym: crate::sym::Sym,
) -> bool {
    match value {
        FieldValue::FormKey(fk) => match resolver.resolve(fk, interner) {
            LeafResolution::Keep => false,
            LeafResolution::Null => {
                fk.local = 0;
                true
            }
            LeafResolution::RepairToOutput => {
                fk.plugin = output_sym;
                true
            }
        },
        FieldValue::List(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                if resolve_fk_leaves(item, resolver, interner, output_sym) {
                    changed = true;
                }
            }
            changed
        }
        FieldValue::Struct(fields) => {
            let mut changed = false;
            for (_, v) in fields.iter_mut() {
                if resolve_fk_leaves(v, resolver, interner, output_sym) {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    fn resolver(output: &[u32], masters: &[(&str, &[u32])], output_plugin: &str) -> LeafResolver {
        LeafResolver {
            output_objids: output.iter().copied().collect(),
            master_objids: masters
                .iter()
                .map(|(_, ids)| ids.iter().copied().collect())
                .collect(),
            output_dial_objids: FxHashSet::default(),
            master_dial_objids: masters.iter().map(|_| FxHashSet::default()).collect(),
            output_wrld_objids: output.iter().copied().collect(),
            master_wrld_objids: masters
                .iter()
                .map(|(_, ids)| ids.iter().copied().collect())
                .collect(),
            output_owner_objids: output.iter().copied().collect(),
            master_owner_objids: masters
                .iter()
                .map(|(_, ids)| ids.iter().copied().collect())
                .collect(),
            master_names: masters.iter().map(|(n, _)| n.to_string()).collect(),
            output_plugin: output_plugin.to_string(),
        }
    }

    fn resolver_with_cell_types(
        output: &[u32],
        output_worldspaces: &[u32],
        output_owners: &[u32],
        masters: &[(&str, &[u32], &[u32], &[u32])],
        output_plugin: &str,
    ) -> LeafResolver {
        LeafResolver {
            output_objids: output.iter().copied().collect(),
            master_objids: masters
                .iter()
                .map(|(_, ids, _, _)| ids.iter().copied().collect())
                .collect(),
            output_dial_objids: FxHashSet::default(),
            master_dial_objids: masters.iter().map(|_| FxHashSet::default()).collect(),
            output_wrld_objids: output_worldspaces.iter().copied().collect(),
            master_wrld_objids: masters
                .iter()
                .map(|(_, _, ids, _)| ids.iter().copied().collect())
                .collect(),
            output_owner_objids: output_owners.iter().copied().collect(),
            master_owner_objids: masters
                .iter()
                .map(|(_, _, _, ids)| ids.iter().copied().collect())
                .collect(),
            master_names: masters
                .iter()
                .map(|(name, _, _, _)| name.to_string())
                .collect(),
            output_plugin: output_plugin.to_string(),
        }
    }

    fn resolver_with_dials(
        output: &[u32],
        output_dials: &[u32],
        masters: &[(&str, &[u32], &[u32])],
        output_plugin: &str,
    ) -> LeafResolver {
        LeafResolver {
            output_objids: output.iter().copied().collect(),
            master_objids: masters
                .iter()
                .map(|(_, ids, _)| ids.iter().copied().collect())
                .collect(),
            output_dial_objids: output_dials.iter().copied().collect(),
            master_dial_objids: masters
                .iter()
                .map(|(_, _, ids)| ids.iter().copied().collect())
                .collect(),
            output_wrld_objids: output.iter().copied().collect(),
            master_wrld_objids: masters
                .iter()
                .map(|(_, ids, _)| ids.iter().copied().collect())
                .collect(),
            output_owner_objids: output.iter().copied().collect(),
            master_owner_objids: masters
                .iter()
                .map(|(_, ids, _)| ids.iter().copied().collect())
                .collect(),
            master_names: masters
                .iter()
                .map(|(name, _, _)| name.to_string())
                .collect(),
            output_plugin: output_plugin.to_string(),
        }
    }

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, Record, RecordFlags};

    fn record(sig: &str, fields: Vec<(&str, FieldValue)>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                plugin: interner.intern("SeventySix.esm"),
                local: 0x000800,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields
                .into_iter()
                .map(|(s, v)| FieldEntry {
                    sig: SubrecordSig::from_str(s).unwrap(),
                    value: v,
                })
                .collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    /// Default mode for the legacy per-record tests: pre-copy, no deferral (the
    /// HEAD semantics for non-worldspace pipelines).
    const PRE_COPY: ApplyMode = ApplyMode::PreCopy {
        defer_placed_child: false,
    };

    /// 7 empty masters, so raw master index 7 addresses the output plugin.
    fn resolver7(output: &[u32], first_master: &[u32]) -> LeafResolver {
        let mut master_objids: Vec<FxHashSet<u32>> = (0..7).map(|_| FxHashSet::default()).collect();
        master_objids[0].extend(first_master.iter().copied());
        LeafResolver {
            output_objids: output.iter().copied().collect(),
            master_objids,
            output_dial_objids: FxHashSet::default(),
            master_dial_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_wrld_objids: FxHashSet::default(),
            master_wrld_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_owner_objids: FxHashSet::default(),
            master_owner_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            master_names: (0..7).map(|i| format!("M{i}.esm")).collect(),
            output_plugin: "SeventySix.esm".to_string(),
        }
    }

    fn raw_cnto(item: u32, count: u32) -> FieldValue {
        let mut cnto = smallvec::SmallVec::<[u8; 32]>::new();
        cnto.extend_from_slice(&item.to_le_bytes());
        cnto.extend_from_slice(&count.to_le_bytes());
        FieldValue::Bytes(cnto)
    }

    fn raw_xown(owner: u32) -> FieldValue {
        let mut payload = smallvec::SmallVec::<[u8; 32]>::from_slice(&owner.to_le_bytes());
        payload.extend_from_slice(&[0; 8]);
        FieldValue::Bytes(payload)
    }

    fn first_fk_local(value: &FieldValue) -> Option<u32> {
        match value {
            FieldValue::FormKey(f) => Some(f.local),
            FieldValue::Struct(fields) => fields.iter().find_map(|(_, v)| first_fk_local(v)),
            FieldValue::List(items) => items.iter().find_map(first_fk_local),
            _ => None,
        }
    }

    fn subrecord_fk_local(rec: &Record, sig: &str) -> Option<u32> {
        rec.fields
            .iter()
            .find(|e| e.sig.as_str() == sig)
            .and_then(|e| first_fk_local(&e.value))
    }

    #[test]
    fn resolves_own_plugin_and_master_leaves() {
        let interner = StringInterner::new();
        let fo4_with_10: &[(&str, &[u32])] = &[("Fallout4.esm", &[0x000010])];
        for (name, output, masters, leaf, expected) in [
            (
                "LCEP 07854F2E own-plugin interior REFR never emitted",
                &[0x001234][..],
                fo4_with_10,
                (0x854F2E, "SeventySix.esm"),
                LeafResolution::Null,
            ),
            (
                "own-plugin leaf that was emitted",
                &[0x854F2E],
                &[("Fallout4.esm", &[][..])][..],
                (0x854F2E, "SeventySix.esm"),
                LeafResolution::Keep,
            ),
            (
                "XTNM 00510AF5 truncated master byte, emitted in output",
                &[0x510AF5],
                fo4_with_10,
                (0x510AF5, "Fallout4.esm"),
                LeafResolution::RepairToOutput,
            ),
            (
                "XCZR 00552965 master leaf resolving nowhere is nulled, not repaired",
                &[0x001234],
                fo4_with_10,
                (0x552965, "Fallout4.esm"),
                LeafResolution::Null,
            ),
            (
                "valid DLCCoast leaf is never clobbered",
                &[0x000001],
                &[("Fallout4.esm", &[][..]), ("DLCCoast.esm", &[0x0247C1][..])][..],
                (0x0247C1, "DLCCoast.esm"),
                LeafResolution::Keep,
            ),
            (
                "null leaf",
                &[],
                &[("Fallout4.esm", &[][..])][..],
                (0, "SeventySix.esm"),
                LeafResolution::Keep,
            ),
        ] {
            let r = resolver(output, masters, "SeventySix.esm");
            assert_eq!(
                r.resolve(&fk(leaf.0, leaf.1, &interner), &interner),
                expected,
                "{name}"
            );
        }
    }

    // In a whole-plugin FO76→FO4 worldspace run the placed children targeted by
    // LCTN LCEP/MNAM, QUST ALFR, REFR XTEL and FACT VENC arrive with the cell copy,
    // after the pre-copy fixup. PreCopy{defer=true} leaves those refs intact; the
    // post-copy repair keeps present targets and nulls (LCEP/ALFR/XTEL, whose
    // subrecord must survive) or drops (MNAM/VENC, where FO4 forbids a NULL leaf)
    // the genuine danglers. PreCopy{defer=false} resolves the class pre-copy.
    #[test]
    fn placed_child_slots_defer_pre_copy_and_resolve_post_copy() {
        let interner = StringInterner::new();
        let lcep = |local: u32| {
            record(
                "LCTN",
                vec![(
                    "LCEP",
                    FieldValue::List(vec![FieldValue::Struct(vec![(
                        interner.intern("loc_enable_parent_ref"),
                        FieldValue::FormKey(fk(local, "SeventySix.esm", &interner)),
                    )])]),
                )],
                &interner,
            )
        };
        let interner_ref = &interner;
        let single = |record_sig: &'static str, sig: &'static str| {
            move |local: u32| {
                record(
                    record_sig,
                    vec![(
                        sig,
                        FieldValue::FormKey(fk(local, "SeventySix.esm", interner_ref)),
                    )],
                    interner_ref,
                )
            }
        };
        let xtel = |local: u32| {
            record(
                "REFR",
                vec![(
                    "XTEL",
                    FieldValue::Struct(vec![(
                        interner.intern("door"),
                        FieldValue::FormKey(fk(local, "SeventySix.esm", &interner)),
                    )]),
                )],
                &interner,
            )
        };
        let alfr = single("QUST", "ALFR");
        let mnam = single("LCTN", "MNAM");
        let venc = single("FACT", "VENC");
        let slots: [(&str, &dyn Fn(u32) -> Record, u32, Option<u32>); 5] = [
            ("LCEP", &lcep, 0x7ACB4D, Some(0)),
            ("ALFR", &alfr, 0x343DB5, Some(0)),
            ("XTEL", &xtel, 0x49994D, Some(0)),
            ("MNAM", &mnam, 0x35D2A1, None),
            ("VENC", &venc, 0x629E0C, None),
        ];
        let not_yet_copied = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let absent = resolver(
            &[0x111111],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        for (sig, make, local, absent_value) in slots {
            let present = resolver(&[local], &[("Fallout4.esm", &[])], "SeventySix.esm");
            for (phase, r, mode, changed, expected) in [
                (
                    "pre-copy deferred",
                    &not_yet_copied,
                    ApplyMode::PreCopy {
                        defer_placed_child: true,
                    },
                    false,
                    Some(local),
                ),
                (
                    "pre-copy without defer",
                    &not_yet_copied,
                    PRE_COPY,
                    true,
                    absent_value,
                ),
                (
                    "post-copy present",
                    &present,
                    ApplyMode::PostCopyPlacedChild,
                    false,
                    Some(local),
                ),
                (
                    "post-copy absent",
                    &absent,
                    ApplyMode::PostCopyPlacedChild,
                    true,
                    absent_value,
                ),
            ] {
                let mut rec = make(local);
                assert_eq!(
                    apply_to_record(&mut rec, r, &interner, mode),
                    changed,
                    "{sig} {phase}"
                );
                assert_eq!(subrecord_fk_local(&rec, sig), expected, "{sig} {phase}");
            }
        }
    }

    #[test]
    fn post_copy_repair_keeps_builtin_player_ref_qust_alfr() {
        let interner = StringInterner::new();
        let r = resolver(&[], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "QUST",
            vec![(
                "ALFR",
                FieldValue::FormKey(fk(0x000014, "Fallout4.esm", &interner)),
            )],
            &interner,
        );

        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);

        assert!(
            !changed,
            "engine-defined PlayerRef must survive post-copy repair"
        );
        assert_eq!(subrecord_fk_local(&rec, "ALFR"), Some(0x000014));

        let missing_master = resolver(&[], &[], "SeventySix.esm");
        assert_eq!(
            missing_master.resolve(&fk(0x000014, "Fallout4.esm", &interner), &interner),
            LeafResolution::Null,
            "PlayerRef is valid only when Fallout4.esm is a target master"
        );
    }

    #[test]
    fn drops_lcun_row_with_absent_actor_ref_pre_and_post_copy() {
        let interner = StringInterner::new();
        let npc = interner.intern("master_unique_npcs_npc");
        let actor = interner.intern("master_unique_npcs_actor_ref");
        let loc = interner.intern("master_unique_npcs_location");
        let row = |a: u32, b: u32, c: u32| {
            FieldValue::Struct(vec![
                (npc, FieldValue::FormKey(fk(a, "SeventySix.esm", &interner))),
                (
                    actor,
                    FieldValue::FormKey(fk(b, "SeventySix.esm", &interner)),
                ),
                (loc, FieldValue::FormKey(fk(c, "SeventySix.esm", &interner))),
            ])
        };
        for (mode, output) in [
            (PRE_COPY, &[0x35C950, 0x2B47D1][..]),
            (ApplyMode::PostCopyPlacedChild, &[0x2B47D1][..]),
        ] {
            let r = resolver(output, &[("Fallout4.esm", &[])], "SeventySix.esm");
            let mut rec = record(
                "LCTN",
                vec![(
                    "LCUN",
                    FieldValue::List(vec![
                        row(0x35C950, 0x35C953, 0x2B47D1), // actor 35C953 absent → drop row
                        row(0x35C950, 0x2B47D1, 0x2B47D1), // actor 2B47D1 emitted → keep row
                    ]),
                )],
                &interner,
            );
            assert!(apply_to_record(&mut rec, &r, &interner, mode));
            let e = rec
                .fields
                .iter()
                .find(|e| e.sig.as_str() == "LCUN")
                .expect("LCUN kept");
            let FieldValue::List(rows) = &e.value else {
                panic!()
            };
            assert_eq!(rows.len(), 1, "the absent-actor row must be dropped");
        }
    }

    fn union_bytes(kind: i32, raw: u32) -> smallvec::SmallVec<[u8; 32]> {
        let mut b = smallvec::SmallVec::<[u8; 32]>::new();
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&raw.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b
    }

    fn union_kind_and_value(b: &[u8]) -> (i32, u32) {
        (
            i32::from_le_bytes(b[0..4].try_into().unwrap()),
            u32::from_le_bytes(b[4..8].try_into().unwrap()),
        )
    }

    #[test]
    fn null_union_slot_handles_pack_target_kinds() {
        let dangling = resolver(
            &[0x111111],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let master_combat_rifle = resolver(&[], &[("Fallout4.esm", &[0x0DF42E])], "SeventySix.esm");
        let emitted = resolver(&[0x525F60], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let empty = resolver(&[], &[("Fallout4.esm", &[])], "SeventySix.esm");
        for (name, sig, kind, raw, r, changed, expected) in [
            (
                "FeedFish04 type-0 reference resolving nowhere",
                "PTDA",
                0,
                0x008A483A,
                &dangling,
                true,
                0,
            ),
            (
                "master-inherited type-1 object id (CombatRifle)",
                "PTDA",
                1,
                0x000DF42E,
                &master_combat_rifle,
                false,
                0x000DF42E,
            ),
            (
                "emitted 07 PLDT reference",
                "PLDT",
                0,
                0x07525F60,
                &emitted,
                false,
                0x07525F60,
            ),
            (
                "type-2 object_type scalar is not a FormID",
                "PTDA",
                2,
                0x0000000F,
                &empty,
                false,
                0x0000000F,
            ),
            (
                "type-3 keyword resolving nowhere",
                "PTDA",
                3,
                0x008A483A,
                &dangling,
                true,
                0,
            ),
        ] {
            let mut b = union_bytes(kind, raw);
            assert_eq!(null_union_slot(&mut b, sig, r, 7), changed, "{name}");
            assert_eq!(union_kind_and_value(&b), (kind, expected), "{name}");
        }
    }

    fn pack_union_record(sig: &str, kind: i32, raw: u32, interner: &StringInterner) -> Record {
        record(
            "PACK",
            vec![(sig, FieldValue::Bytes(union_bytes(kind, raw)))],
            interner,
        )
    }

    fn pack_union(rec: &Record) -> (i32, u32) {
        let FieldValue::Bytes(bytes) = &rec.fields[0].value else {
            panic!("raw package union expected");
        };
        union_kind_and_value(bytes)
    }

    #[test]
    fn pack_placed_reference_targets_defer_pre_copy_and_resolve_post_copy() {
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");

        for sig in ["PTDA", "PLDT"] {
            let mut rec = pack_union_record(sig, 0, 0x01405EA6, &interner);
            assert!(!apply_to_record(
                &mut rec,
                &r,
                &interner,
                ApplyMode::PreCopy {
                    defer_placed_child: true,
                },
            ));
            assert_eq!(pack_union(&rec), (0, 0x01405EA6), "{sig} deferred");

            let mut eager = pack_union_record(sig, 0, 0x01405EA6, &interner);
            assert!(apply_to_record(&mut eager, &r, &interner, PRE_COPY));
            let expected_benign_type = if sig == "PTDA" { 6 } else { 2 };
            assert_eq!(pack_union(&eager), (expected_benign_type, 0), "{sig} eager");
        }

        let present = resolver(&[0x405EA6], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = pack_union_record("PTDA", 0, 0x01405EA6, &interner);
        assert!(!apply_to_record(
            &mut rec,
            &present,
            &interner,
            ApplyMode::PostCopyPlacedChild,
        ));
        assert_eq!(
            pack_union(&rec),
            (0, 0x01405EA6),
            "post-copy present target kept"
        );
    }

    #[test]
    fn benignifies_only_null_reference_pack_unions() {
        for (name, sig, kind, raw, changed, expected_kind) in [
            (
                "null PTDA reference → Self",
                "PTDA",
                PACK_UNION_REFERENCE_TYPE,
                0,
                true,
                PACK_TARGET_SELF_TYPE,
            ),
            (
                "null PLDT reference → near package start",
                "PLDT",
                PACK_UNION_REFERENCE_TYPE,
                0,
                true,
                PACK_LOCATION_NEAR_PACKAGE_START_TYPE,
            ),
            (
                "null PLVD reference → near package start",
                "PLVD",
                PACK_UNION_REFERENCE_TYPE,
                0,
                true,
                PACK_LOCATION_NEAR_PACKAGE_START_TYPE,
            ),
            (
                "resolvable reference untouched",
                "PTDA",
                PACK_UNION_REFERENCE_TYPE,
                0x07525F60,
                false,
                PACK_UNION_REFERENCE_TYPE,
            ),
            ("non-reference selector untouched", "PTDA", 2, 0, false, 2),
        ] {
            let mut b = union_bytes(kind, raw);
            assert_eq!(
                benignify_value0_reference_union(&mut b, sig),
                changed,
                "{name}"
            );
            assert_eq!(union_kind_and_value(&b), (expected_kind, raw), "{name}");
        }

        // FeedFish04 end to end: null the absent interior REFR, then benignify to Self.
        let r = resolver(
            &[0x111111],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let mut b = union_bytes(PACK_UNION_REFERENCE_TYPE, 0x008A483A);
        assert!(null_union_slot(&mut b, "PTDA", &r, 7));
        assert!(benignify_value0_reference_union(&mut b, "PTDA"));
        assert_eq!(union_kind_and_value(&b), (PACK_TARGET_SELF_TYPE, 0));
    }

    #[test]
    fn nulls_lcep_list_struct_leaves_in_place() {
        // End-to-end over a List<Struct> LCEP shape: the missing Ref nulls, the
        // emitted Ref stays, the tail Bytes field is untouched.
        let interner = StringInterner::new();
        let r = resolver(&[0x111111], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let output_sym = interner.intern("SeventySix.esm");
        let ref_sym = interner.intern("ref");
        let parent_sym = interner.intern("parent");
        let tail_sym = interner.intern("tail");
        let mut value = FieldValue::List(vec![FieldValue::Struct(vec![
            (
                ref_sym,
                FieldValue::FormKey(fk(0x854F2E, "SeventySix.esm", &interner)),
            ), // missing → null
            (
                parent_sym,
                FieldValue::FormKey(fk(0x111111, "SeventySix.esm", &interner)),
            ), // emitted → keep
            (
                tail_sym,
                FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1, 0, 0, 0])),
            ),
        ])]);
        let changed = resolve_fk_leaves(&mut value, &r, &interner, output_sym);
        assert!(changed);
        let FieldValue::List(items) = &value else {
            panic!()
        };
        let FieldValue::Struct(fields) = &items[0] else {
            panic!()
        };
        assert!(matches!(&fields[0].1, FieldValue::FormKey(f) if f.local == 0));
        assert!(matches!(&fields[1].1, FieldValue::FormKey(f) if f.local == 0x111111));
        assert!(matches!(&fields[2].1, FieldValue::Bytes(b) if b.as_slice() == [1, 0, 0, 0]));
    }

    #[test]
    fn dlbr_starting_topic_must_be_a_resolvable_dial() {
        let interner = StringInterner::new();
        let emitted_dial = resolver_with_dials(
            &[0x0565C1],
            &[0x0565C1],
            &[("Fallout4.esm", &[], &[])],
            "SeventySix.esm",
        );
        let master_dial = resolver_with_dials(
            &[0x000800],
            &[],
            &[("Fallout4.esm", &[0x01A2B3], &[0x01A2B3])],
            "SeventySix.esm",
        );
        let non_dial = resolver_with_dials(
            &[0x0565C1],
            &[],
            &[("Fallout4.esm", &[], &[])],
            "SeventySix.esm",
        );
        let snam = |local: u32, plugin: &str| {
            record(
                "DLBR",
                vec![("SNAM", FieldValue::FormKey(fk(local, plugin, &interner)))],
                &interner,
            )
        };
        for (name, r, mut rec, expected) in [
            (
                "emitted output DIAL",
                &emitted_dial,
                snam(0x0565C1, "SeventySix.esm"),
                DlbrStartingTopicResolution::Keep,
            ),
            (
                "valid master DIAL",
                &master_dial,
                snam(0x01A2B3, "Fallout4.esm"),
                DlbrStartingTopicResolution::Keep,
            ),
            (
                "null",
                &non_dial,
                snam(0, "SeventySix.esm"),
                DlbrStartingTopicResolution::Invalid,
            ),
            (
                "decoded null",
                &non_dial,
                record("DLBR", vec![("SNAM", FieldValue::None)], &interner),
                DlbrStartingTopicResolution::Invalid,
            ),
            (
                "missing",
                &non_dial,
                record("DLBR", vec![], &interner),
                DlbrStartingTopicResolution::Invalid,
            ),
            (
                "dangling",
                &non_dial,
                snam(0x2C505F, "SeventySix.esm"),
                DlbrStartingTopicResolution::Invalid,
            ),
            (
                "wrong type",
                &non_dial,
                snam(0x0565C1, "SeventySix.esm"),
                DlbrStartingTopicResolution::Invalid,
            ),
        ] {
            assert_eq!(
                resolve_dlbr_starting_topic(&mut rec, r, &interner),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn repairs_dlbr_master_prefixed_output_dial_and_preserves_incoming_bnam() {
        let interner = StringInterner::new();
        let r = resolver_with_dials(
            &[0x000800, 0x0565C1],
            &[0x0565C1],
            &[("Fallout4.esm", &[], &[])],
            "SeventySix.esm",
        );
        let mut branch = record(
            "DLBR",
            vec![(
                "SNAM",
                FieldValue::FormKey(fk(0x0565C1, "Fallout4.esm", &interner)),
            )],
            &interner,
        );

        assert_eq!(
            resolve_dlbr_starting_topic(&mut branch, &r, &interner),
            DlbrStartingTopicResolution::RepairToOutput
        );
        let repaired = first_formkey(&branch.fields[0].value).expect("repaired SNAM formkey");
        assert_eq!(
            interner.resolve(repaired.plugin).as_deref(),
            Some("SeventySix.esm")
        );
        assert_eq!(
            resolve_dlbr_starting_topic(&mut branch, &r, &interner),
            DlbrStartingTopicResolution::Keep
        );

        let mut topic = record(
            "DIAL",
            vec![(
                "BNAM",
                FieldValue::FormKey(fk(0x000800, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        assert!(!apply_to_record(&mut topic, &r, &interner, PRE_COPY));
        assert!(
            topic
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "BNAM")
        );
    }

    #[test]
    fn dedupe_records_by_form_key_keeps_last_record_in_first_position() {
        let interner = StringInterner::new();
        let mut first = record(
            "INFO",
            vec![(
                "GNAM",
                FieldValue::FormKey(fk(0x111111, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        first.form_key.local = 0x10;
        let mut duplicate = record(
            "INFO",
            vec![(
                "DNAM",
                FieldValue::FormKey(fk(0x222222, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        duplicate.form_key.local = 0x10;
        let mut other = record(
            "WRLD",
            vec![(
                "WNAM",
                FieldValue::FormKey(fk(0x333333, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        other.form_key.local = 0x20;

        let deduped = dedupe_records_by_form_key(vec![first, other, duplicate]);

        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].form_key.local, 0x10);
        assert!(
            deduped[0]
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "DNAM"),
            "duplicate FormKey should keep the last changed record"
        );
        assert_eq!(deduped[1].form_key.local, 0x20);
    }

    fn structured_cnto(item: Option<FormKey>, interner: &StringInterner) -> FieldValue {
        let mut fields = Vec::new();
        if let Some(item) = item {
            fields.push((interner.intern("Item"), FieldValue::FormKey(item)));
        }
        fields.push((interner.intern("Count"), FieldValue::Int(1)));
        FieldValue::Struct(fields)
    }

    #[test]
    fn pre_copy_keeps_resolvable_and_drops_dangling_subrecords() {
        let interner = StringInterner::new();
        let own = |local: u32| FieldValue::FormKey(fk(local, "SeventySix.esm", &interner));
        let xilw = |local: u32| {
            FieldValue::Struct(vec![(
                interner.intern("Worldspace"),
                FieldValue::FormKey(fk(local, "Fallout4.esm", &interner)),
            )])
        };
        let fo4_plain = || resolver(&[], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let output = |ids: &[u32]| resolver(ids, &[("Fallout4.esm", &[])], "SeventySix.esm");
        let cell_types = |output: &[u32], worldspaces: &[u32], owners: &[u32]| {
            resolver_with_cell_types(
                output,
                worldspaces,
                owners,
                &[("Fallout4.esm", &[], &[], &[])],
                "SeventySix.esm",
            )
        };
        #[allow(clippy::type_complexity)]
        let cases: Vec<(
            &str,
            LeafResolver,
            &str,
            Vec<(&str, FieldValue)>,
            bool,
            Vec<&str>,
            Option<FieldValue>,
        )> = vec![
            (
                "INFO DNAM shared-INFO never emitted is dropped, not left NULL",
                output(&[0x111111]),
                "INFO",
                vec![("GNAM", own(0x111111)), ("DNAM", own(0x37F7FC))],
                true,
                vec!["GNAM"],
                None,
            ),
            (
                "resolvable WRLD WNAM kept",
                output(&[0x00F7F5]),
                "WRLD",
                vec![("WNAM", own(0x00F7F5))],
                false,
                vec!["WNAM"],
                None,
            ),
            (
                "null WRLD WNAM dropped",
                fo4_plain(),
                "WRLD",
                vec![("WNAM", own(0x123456))],
                true,
                vec![],
                None,
            ),
            (
                "resolvable WRLD NAM3 kept",
                output(&[0x00F7F5]),
                "WRLD",
                vec![("NAM3", own(0x00F7F5))],
                false,
                vec!["NAM3"],
                None,
            ),
            (
                "null WRLD NAM3 dropped",
                fo4_plain(),
                "WRLD",
                vec![("NAM3", own(0x123456))],
                true,
                vec![],
                None,
            ),
            (
                "unresolved CELL XILW struct dropped",
                fo4_plain(),
                "CELL",
                vec![("XILW", xilw(0x635F96))],
                true,
                vec![],
                None,
            ),
            (
                "resolvable CELL XILW struct kept",
                resolver(&[], &[("Fallout4.esm", &[0x635F96])], "SeventySix.esm"),
                "CELL",
                vec![("XILW", xilw(0x635F96))],
                false,
                vec!["XILW"],
                None,
            ),
            (
                "CELL XILW with same-id output record of the wrong type dropped",
                cell_types(&[0x635F96], &[], &[0x635F96]),
                "CELL",
                vec![(
                    "XILW",
                    FieldValue::FormKey(fk(0x635F96, "Fallout4.esm", &interner)),
                )],
                true,
                vec![],
                None,
            ),
            (
                "CELL XOWN with valid master owner kept",
                resolver_with_cell_types(
                    &[],
                    &[],
                    &[],
                    &[("Fallout4.esm", &[0x01C21C], &[], &[0x01C21C])],
                    "SeventySix.esm",
                ),
                "CELL",
                vec![("XOWN", raw_xown(0x0001_C21C))],
                false,
                vec!["XOWN"],
                None,
            ),
            (
                "CELL XOWN with same-id output record of the wrong type dropped",
                cell_types(&[0x2744B3], &[0x2744B3], &[]),
                "CELL",
                vec![("XOWN", raw_xown(0x0027_44B3))],
                true,
                vec![],
                None,
            ),
            (
                "CELL XOWN with missing owner dropped",
                cell_types(&[], &[], &[]),
                "CELL",
                vec![("XOWN", raw_xown(0x0042_A260))],
                true,
                vec![],
                None,
            ),
            (
                "null SCEN player dialogue responses dropped",
                output(&[0x59958B]),
                "SCEN",
                vec![
                    ("ANAM", FieldValue::Uint(1)),
                    ("PTOP", FieldValue::None),
                    ("NTOP", own(0x59958B)),
                    ("NPOT", FieldValue::None),
                ],
                true,
                vec!["ANAM", "NTOP"],
                None,
            ),
            (
                "dangling SCEN dialogue response dropped",
                output(&[0x59958B]),
                "SCEN",
                vec![
                    ("ANAM", FieldValue::Uint(1)),
                    ("PTOP", own(0x59957F)),
                    ("NTOP", own(0x59958B)),
                ],
                true,
                vec!["ANAM", "NTOP"],
                None,
            ),
            (
                "DIAL BNAM to a pruned DLBR dropped",
                output(&[0x0565C1]),
                "DIAL",
                vec![("BNAM", own(0x28950A))],
                true,
                vec![],
                None,
            ),
            (
                "NPC_ CNTO FO76 Caps001 0700000F dropped with the stale COCT",
                resolver7(&[0x222222], &[]),
                "NPC_",
                vec![
                    ("COCT", FieldValue::Uint(1)),
                    ("CNTO", raw_cnto(0x0700_000F, 1)),
                    (
                        "NAM8",
                        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1, 0, 0, 0])),
                    ),
                ],
                true,
                vec!["NAM8"],
                None,
            ),
            (
                "NPC_ COCT synced after dropping some CNTO rows",
                resolver7(&[0x000022], &[]),
                "NPC_",
                vec![
                    ("COCT", FieldValue::Uint(2)),
                    ("CNTO", raw_cnto(0x0700_000F, 1)),
                    ("CNTO", raw_cnto(0x0700_0022, 3)),
                ],
                true,
                vec!["COCT", "CNTO"],
                Some(FieldValue::Uint(1)),
            ),
            (
                "NPC_ CNTO with resolvable item kept",
                resolver7(&[0x000022], &[]),
                "NPC_",
                vec![("CNTO", raw_cnto(0x0700_0022, 3))],
                false,
                vec!["CNTO"],
                None,
            ),
            (
                "valid raw CONT CNTO row kept",
                output(&[0x3D7F48]),
                "CONT",
                vec![
                    ("COCT", FieldValue::Uint(1)),
                    ("CNTO", raw_cnto(0x013D_7F48, 1)),
                ],
                false,
                vec!["COCT", "CNTO"],
                Some(FieldValue::Uint(1)),
            ),
            (
                "invalid CONT CNTO rows dropped and COCT synced",
                output(&[0x000022]),
                "CONT",
                vec![
                    ("COCT", FieldValue::Uint(4)),
                    (
                        "CNTO",
                        structured_cnto(Some(fk(0, "SeventySix.esm", &interner)), &interner),
                    ),
                    (
                        "CNTO",
                        structured_cnto(Some(fk(0x00DEAD, "SeventySix.esm", &interner)), &interner),
                    ),
                    ("CNTO", structured_cnto(None, &interner)),
                    (
                        "CNTO",
                        structured_cnto(Some(fk(0x000022, "SeventySix.esm", &interner)), &interner),
                    ),
                ],
                true,
                vec!["COCT", "CNTO"],
                Some(FieldValue::Uint(1)),
            ),
            (
                "valid CONT CNTO rows and matching COCT kept",
                resolver(
                    &[0x000022],
                    &[("Fallout4.esm", &[0x001234])],
                    "SeventySix.esm",
                ),
                "CONT",
                vec![
                    ("COCT", FieldValue::Uint(2)),
                    (
                        "CNTO",
                        structured_cnto(Some(fk(0x000022, "SeventySix.esm", &interner)), &interner),
                    ),
                    (
                        "CNTO",
                        structured_cnto(Some(fk(0x001234, "Fallout4.esm", &interner)), &interner),
                    ),
                ],
                false,
                vec!["COCT", "CNTO", "CNTO"],
                Some(FieldValue::Uint(2)),
            ),
        ];
        for (name, r, record_sig, fields, changed, remaining, coct) in cases {
            let mut rec = record(record_sig, fields, &interner);
            let before = rec.fields.clone();
            assert_eq!(
                apply_to_record(&mut rec, &r, &interner, PRE_COPY),
                changed,
                "{name}"
            );
            let sigs: Vec<&str> = rec.fields.iter().map(|entry| entry.sig.as_str()).collect();
            assert_eq!(sigs, remaining, "{name}");
            if !changed {
                assert_eq!(
                    rec.fields, before,
                    "{name}: unchanged record keeps its bytes"
                );
            }
            if let Some(coct) = coct {
                let actual = rec
                    .fields
                    .iter()
                    .find(|e| e.sig.as_str() == "COCT")
                    .map(|e| &e.value);
                assert_eq!(
                    actual,
                    Some(&coct),
                    "{name}: COCT must match surviving CNTO rows"
                );
            }
        }
    }
    #[test]
    fn repairs_cell_xilw_to_same_id_output_worldspace_idempotently() {
        let interner = StringInterner::new();
        let worldspace = interner.intern("Worldspace");
        let r = resolver_with_cell_types(
            &[0x635F96],
            &[0x635F96],
            &[],
            &[("Fallout4.esm", &[], &[], &[])],
            "SeventySix.esm",
        );
        let mut rec = record(
            "CELL",
            vec![(
                "XILW",
                FieldValue::Struct(vec![(
                    worldspace,
                    FieldValue::FormKey(fk(0x635F96, "Fallout4.esm", &interner)),
                )]),
            )],
            &interner,
        );

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(!apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        let worldspace = first_formkey(&rec.fields[0].value).expect("XILW worldspace");
        assert_eq!(interner.resolve(worldspace.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn defers_cell_references_until_post_copy_repair() {
        let interner = StringInterner::new();
        let r = resolver_with_cell_types(
            &[0x635F96],
            &[0x635F96],
            &[],
            &[("Fallout4.esm", &[], &[], &[])],
            "SeventySix.esm",
        );
        let mut rec = record(
            "CELL",
            vec![(
                "XILW",
                FieldValue::FormKey(fk(0x635F96, "Fallout4.esm", &interner)),
            )],
            &interner,
        );
        let deferred = ApplyMode::PreCopy {
            defer_placed_child: true,
        };

        assert!(!apply_to_record(&mut rec, &r, &interner, deferred));
        assert!(apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PostCopyPlacedChild
        ));
        assert!(ApplyMode::PostCopyPlacedChild.processes("CELL", "XOWN"));
        let worldspace = first_formkey(&rec.fields[0].value).expect("XILW worldspace");
        assert_eq!(interner.resolve(worldspace.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn repairs_cell_xown_to_same_id_output_owner_idempotently() {
        let interner = StringInterner::new();
        let r = resolver_with_cell_types(
            &[0x2744B3],
            &[],
            &[0x2744B3],
            &[("Fallout4.esm", &[], &[], &[])],
            "SeventySix.esm",
        );
        let mut payload = smallvec::SmallVec::<[u8; 32]>::new();
        payload.extend_from_slice(&0x0027_44B3_u32.to_le_bytes());
        payload.extend_from_slice(&[0; 8]);
        let mut rec = record(
            "CELL",
            vec![("XOWN", FieldValue::Bytes(payload))],
            &interner,
        );

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(!apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        let FieldValue::Bytes(bytes) = &rec.fields[0].value else {
            panic!("XOWN should remain raw")
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x0127_44B3
        );
    }

    #[test]
    fn repairs_scen_tnam_truncated_master_byte() {
        // SCEN TNAM 0055DE1F: master byte 00 but the template SCEN was emitted in
        // output → repair plugin sym, do NOT drop.
        let interner = StringInterner::new();
        let r = resolver(&[0x55DE1F], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "SCEN",
            vec![(
                "TNAM",
                FieldValue::FormKey(fk(0x55DE1F, "Fallout4.esm", &interner)),
            )],
            &interner,
        );
        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        let e = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "TNAM")
            .expect("TNAM kept");
        let FieldValue::FormKey(f) = &e.value else {
            panic!()
        };
        assert_eq!(f.local, 0x55DE1F);
        assert_eq!(interner.resolve(f.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn repairs_npc_cnto_to_matching_master_item() {
        let interner = StringInterner::new();
        let mut rec = record(
            "NPC_",
            vec![
                ("COCT", FieldValue::Uint(1)),
                ("CNTO", raw_cnto(0x0711_3339, 1)),
            ],
            &interner,
        );
        let r7 = resolver7(&[], &[0x113339]);

        assert!(apply_to_record(&mut rec, &r7, &interner, PRE_COPY));
        let cnto = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "CNTO")
            .expect("CNTO kept");
        let FieldValue::Bytes(bytes) = &cnto.value else {
            panic!("CNTO should remain opaque bytes")
        };
        assert_eq!(read_cnto_item(bytes.as_slice()), Some(0x0011_3339));
        assert!(
            matches!(
                rec.fields
                    .iter()
                    .find(|e| e.sig.as_str() == "COCT")
                    .map(|e| &e.value),
                Some(FieldValue::Uint(1))
            ),
            "COCT stays aligned with the repaired inventory row"
        );
    }
}
