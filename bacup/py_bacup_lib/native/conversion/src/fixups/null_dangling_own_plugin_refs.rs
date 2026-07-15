//! Fixup: null or repair reference slots that point at a
//! record which was never emitted into the output plugin, because the source
//! FO76 record lives OUTSIDE the converted slice (or has no FO4 equivalent).
//!
//! # Root cause
//! The FO76→FO4 conversion ports only the APPALACHIA exterior worldspace (its
//! terrain cells + their placed children). Interior cells and other worldspaces
//! are NOT converted. But many *base* records that ARE carried (LCTN, REFR,
//! WRLD, NPC_, PACK, FACT) still reference placed REFR/ACHR records that lived in
//! those dropped interior cells. The translate-time remap dutifully rewrites the
//! FK to its own-plugin (07) target FormKey — but the target record is never
//! emitted, so the FK dangles. xEdit (full FO4 master order loaded) reports
//! "Could not be resolved" on (byte-verified counts vs the round-6 output):
//!   * LCTN `LCEP` Ref + Actor (~1459) — enable-parent markers of interior
//!     locations (e.g. `LocBurnHighwayTownInteriorLocation`).
//!   * PACK `PLDT`/`PTDA`/`PDTO` + FACT `PLVD` (~636) — package/faction targets
//!     pointing at interior REFRs / dropped bases (e.g. PTDA `FeedFish04`).
//!   * REFR `XCZR` Current-Zone-Ref, `XRFG` Reference-Group (interior REFRs).
//!   * WRLD `WNAM` Parent-Worldspace (a synthesized id present in neither game).
//!   * NPC_ `CNTO` Item — the FO76 `CNCY Caps001` currency, which has no FO4
//!     record (FO4 caps are a MISC item, allocated under a different id).
//!
//! NOTE this is emission-completeness ONLY for genuinely-absent targets: a FK
//! that resolves in a FO4 master (e.g. PTDA `000DF42E` CombatRifle, a vanilla
//! WEAP correctly inherited from Fallout4.esm and NOT re-emitted) is KEPT
//! byte-identical — it was never a real error, only a stale-dump (no-masters)
//! xEdit artifact. Emitting interiors is out of scope; the FO4-correct
//! representation of an absent reference is NULL.
//!
//! These are emission GAPS that are correct-by-design for an exterior-only port:
//! emitting the ~8,700 interior cells (and ~1.9M placed children) is out of
//! scope. The FO4-correct representation of a reference whose target legitimately
//! does not exist is NULL (`local = 0`) — the same resolution
//! `fix_invalid_target_formkeys` applies to dangling *master* refs, which this
//! fixup complements for dangling *own-plugin* refs (a class that fixup misses:
//! its `is_invalid_target_fk` only checks the target masters, never the output
//! plugin itself, and in whole-plugin runs its worklist never even visits LCTN).
//!
//! # The one repair case: master-byte truncation
//! REFR `XTNM` Teleport-Loc-Name `00510AF5` addresses Fallout4.esm but the MESG
//! `LC104DoorOverride_TurbineHall` it names WAS emitted in the output plugin at
//! `07510AF5`; the leaf merely lost its master byte. When an apparently-dangling
//! leaf's object-id DOES exist in the output plugin, we REPAIR the plugin sym to
//! the output rather than null it (mirrors `null_dangling_misc_refs`'s SNDR
//! repair). This is checked before the null decision.
//!
//! # Plugin-aware
//! Every decision is made on the leaf's full `(plugin, object_id)` against the
//! authoritative object-id set of the addressed handle (output plugin or the
//! named master), collected once via `local_object_ids_in_handle` over ALL
//! signatures — never a sig-filtered subset (a CNTO item may be MISC/AMMO/WEAP/…
//! so a per-sig "emitted set" would false-positive thousands of valid refs). A
//! leaf that already resolves in its addressed handle is left byte-identical, so
//! a correctly-remapped foreign-master FK is never clobbered.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::remap_struct_internal_formids::union_type_holds_formid;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

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
    // REFR Teleport-Destination (codec `struct:I,f×6,I,I` → two `formid` leaves:
    // `door`→REFR @offset 0, `transition_interior`→CELL @offset 32). The door is
    // almost always a PERSISTENT placed REFR (source flag 0x400) living in a
    // worldspace persistent cell (or the FO76 global persistent cell), materialized
    // in the output only by the persistent-cell phase that runs alongside the
    // phase-6 copy. XTEL is correctly remapped 00→07 by cell_slice's remap-offset
    // table, but the pre-copy pass would null the not-yet-emitted door with nothing
    // to rebind, so it is deferred (see `DEFERRED_PLACED_CHILD_SUBRECORDS`) and
    // resolved post-copy. NOT drop-on-null: dropping XTEL would lose the teleport
    // link — null-then-rebind. Both Struct FormKey leaves are walked by
    // `resolve_fk_leaves`.
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
    // PACK/FACT value-selected-union ref slots. These decode to opaque `Bytes`
    // at fixup time (the generic decoder can't evaluate the `type` selector), so
    // they're handled by the byte-offset union path (`null_union_slot`), not the
    // FormKey-leaf walk. `pack`'s `remap_value_selected_union_formids` already
    // rewrites the resolvable ones 00→07; the residue here is targets that
    // resolve in neither the output nor any master (interior REFRs, dropped
    // bases) → null them.
    ("PACK", "PTDA"),
    ("PACK", "PLDT"),
    ("PACK", "PDTO"),
    ("FACT", "PLVD"),
    // FACT VENC "Merchant Container" → REFR. The container is a PLACED CHILD
    // re-inserted post-copy by the cell-slice / interior-cell phases, so at
    // pre-copy fixup time it is ABSENT and the FK looks dangling. Without the
    // deferral (see `DEFERRED_PLACED_CHILD_SUBRECORDS`), `fix_invalid_target_formkeys`
    // nulls it and `validate_reference_target_types` then strips the present-but-null
    // VENC — every converted vendor faction loses its merchant container. Deferred
    // and resolved post-copy (drop-on-null when the container is genuinely absent).
    ("FACT", "VENC"),
    // QUST alias Forced-Reference (codec `formid`, repeatable in the `aliases`
    // scope → decodes to one-or-more `FieldValue::FormKey` leaves). Its target is
    // almost always a worldspace PERSISTENT ref (REFR/ACHR, source flag 0x400)
    // living in the WRLD-embedded persistent cell. That cell is materialized in
    // the output only by the persistent-cell phase (owner-G), so ALFR is deferred
    // (see `DEFERRED_PLACED_CHILD_SUBRECORDS`) and resolved post-copy. A null leaf
    // is left in place (NOT drop-on-null: dropping one ALFR would corrupt the
    // repeatable alias block) — matching the CK-benign state for ALFR targets that
    // resolve nowhere (interior/test-quest forced refs absent from an exterior port).
    ("QUST", "ALFR"),
    // RFGP Reference-Group anchor (codec `formid` → typed `FieldValue::FormKey`).
    // A "Reference Group" record's RNAM points at its member REFR — a PLACED CHILD
    // (source flag 0x400 persistent) re-inserted only by the phase-6 cell-slice /
    // persistent-cell copy that runs AFTER the pre-copy fixups. At fixup time the
    // target is ABSENT, so the sweep's skip-record-sig rule nulls the RNAM leaf even
    // though the REFR is present post-copy (confirmed over-null on 002150→0017C2,
    // 866182→866161, 864038→86402B). Deferred (see `DEFERRED_PLACED_CHILD_SUBRECORDS`)
    // and resolved post-copy. NOT drop-on-null: a valid self-ref resolves to Keep, a
    // genuine dangler nulls (null-leaf is CK-benign for RFGP, like XTEL/ALFR).
    ("RFGP", "RNAM"),
];

/// `(record sig, subrecord sig)` of the placed-ref-target class — refs that
/// translate-remap to exterior placed children (ACHR/REFR/...). In a whole-plugin
/// FO76→FO4 worldspace run those children are SKIPPED by `translate_all` and
/// re-inserted by the phase-6 cell-slice copy, which runs AFTER this fixup. At
/// fixup time their targets are therefore ABSENT, so the pre-copy pass would
/// wrongly null refs that are actually present post-copy. When
/// `FixupConfig::defer_placed_child_ref_class` is set the pre-copy pass DEFERS
/// this class (leaves the refs intact); `repair_placed_child_refs` runs the
/// single authoritative resolution post-copy over the now-complete output. A
/// genuinely-interior dangler (e.g. FeedFish04) is still absent post-copy → still
/// nulled.
///
/// Refs that point at WORLDSPACE PERSISTENT children (REFR/ACHR, source flag
/// 0x400) living in the WRLD-embedded persistent cell. That cell is materialized
/// in the output only by the persistent-cell phase (which runs alongside the
/// phase-6 copy), so these must defer pre-copy and resolve in the post-copy
/// repair exactly like the LCTN enable-parent class:
///   * `(QUST, ALFR)` — alias Forced Reference (mostly worldspace-persistent →
///     resolve post-copy; the rest are interior/test refs that stay null = benign).
///   * `(LCTN, MNAM)` — World Location Marker Ref. MNAM is also in
///     `DROP_ON_NULL_SUBRECORDS`; deferring it means the drop decision is made
///     post-copy (keep if the marker is now present, else drop).
///
///   * `(REFR, XTEL)` — Teleport-Destination door. Its `door` formid points at a
///     persistent placed REFR emitted only by the persistent-cell phase, so it
///     defers pre-copy and rebinds post-copy. NOT drop-on-null (nulling-then-
///     rebinding preserves the teleport link).
///   * `(CELL, XILW|XOWN)` — interior CELL records are emitted after the registered
///     fixup pass. Defer their exact-type validation until the post-copy repair,
///     after interior and synthesized cells are present.
const DEFERRED_PLACED_CHILD_SUBRECORDS: &[(&str, &str)] = &[
    ("LCTN", "LCEP"),
    ("LCTN", "ACEP"),
    ("LCTN", "LCUN"),
    ("LCTN", "MNAM"),
    ("QUST", "ALFR"),
    ("REFR", "XTEL"),
    ("CELL", "XILW"),
    ("CELL", "XOWN"),
    // FACT VENC merchant container — see `TOUCHED_SUBRECORDS`. Its REFR target is a
    // placed child emitted only by the cell-slice / interior-cell copy that runs
    // AFTER the pre-copy fixups, so defer the null/validate decision to the
    // post-copy repair over the complete output (where the container is present).
    ("FACT", "VENC"),
    // RFGP RNAM Reference-Group anchor — points at a PERSISTENT placed REFR emitted
    // only by the phase-6 / persistent-cell copy, so it defers pre-copy and rebinds
    // post-copy exactly like XTEL/ALFR. See `TOUCHED_SUBRECORDS` for the over-null
    // evidence (002150→0017C2 et al.).
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

/// `(record sig, subrecord sig)` reference slots whose FO4 schema forbids a NULL
/// value: xEdit reports "Found a NULL reference, expected: <type>" when the
/// upstream dangling-nuller zeroed the leaf in place. CELL.XOWN may remain an
/// opaque ownership struct; the CELL-specific path resolves its leading FormID.
/// All are OPTIONAL subrecords
/// in the FO4 grammar (verified: INFO.DNAM Shared-INFO, WRLD.WNAM Parent
/// Worldspace, WRLD.NAM3 LOD Water, LCTN.MNAM World-Location-Marker,
/// CELL.XILW Exterior-LOD Worldspace, CELL.XOWN Owner, SCEN.TNAM Template-Scene), so the
/// FO4-correct representation of an absent target is to OMIT the subrecord, not to
/// keep a `local = 0` leaf. When the leaf resolves to `Null` (and only then) the
/// entire `FieldEntry` is dropped. A leaf that resolves (in-output or a master) or
/// repairs (truncated master byte) is kept/repaired exactly as for any other
/// touched slot.
///
/// DLBR.SNAM Starting-Topic is intentionally NOT here: the FO4 DLBR grammar
/// requires it, so `run_resolution` drops the whole DLBR when the target DIAL is
/// absent or has the wrong signature.
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

/// FO76 NPC_ `CNTO` Item rows are `struct:I,i` (item FormID @ offset 0, count) and
/// decode to opaque `Bytes` (the generic struct codec is byte-copied). The FO76
/// `CNCY Caps001` currency (`0700000F`) has no FO4 record, so the item FormID
/// dangles. FO4 rejects a NULL/unresolvable CNTO item, so the whole CNTO
/// subrecord is dropped (count travels with it — inherently lockstep).
const NPC_CNTO_ITEM_OFFSET: usize = 0;

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

/// Post-copy authoritative resolution of the deferred placed-ref-target class
/// (LCTN LCUN/LCEP/ACEP) against the now-COMPLETE output plugin. Called AFTER the
/// FO76→FO4 phase-6 cell-slice copy + cell-location sync re-insert the exterior
/// placed children, so a ref kept here resolves to a present record and a ref
/// still absent (a genuine interior dangler, e.g. FeedFish04) is nulled (and its
/// LCUN row dropped in lockstep). Only meaningful when the pre-copy pass deferred
/// this class; a no-op otherwise (the refs are already resolved/nulled).
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
        self.master_names
            .iter()
            .position(|m| m.eq_ignore_ascii_case(plugin_name))
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
    if bytes.len() < NPC_CNTO_ITEM_OFFSET + 4 {
        return None;
    }
    Some(u32::from_le_bytes([
        bytes[NPC_CNTO_ITEM_OFFSET],
        bytes[NPC_CNTO_ITEM_OFFSET + 1],
        bytes[NPC_CNTO_ITEM_OFFSET + 2],
        bytes[NPC_CNTO_ITEM_OFFSET + 3],
    ]))
}

fn write_cnto_item(bytes: &mut [u8], raw: u32) {
    if bytes.len() >= NPC_CNTO_ITEM_OFFSET + 4 {
        bytes[NPC_CNTO_ITEM_OFFSET..NPC_CNTO_ITEM_OFFSET + 4].copy_from_slice(&raw.to_le_bytes());
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

/// Rewrite a PLDT/PLVD/PTDA *Reference*-variant union whose offset-4 FK is null
/// (`[type=0][value=0]`) to a benign self-relative selector, so the FO4 CK reads
/// valid package data instead of warning "Package Location/Target Reference
/// (00000000)". PLDT/PLVD location → "Near Package Start" (type 2); PTDA target →
/// "Self" (type 6). Both replacements treat offset 4 as cpIgnore, so the already-
/// zero value stays valid. Only fires on the Reference variant (`type == 0`) with
/// a null value — a resolvable reference (value != 0) or a non-reference variant
/// is left untouched. This mirrors the translator's
/// `neutralize_dangling_package_alias_targets` (which handles the *alias* variants
/// {8,9,14}/{4}); here we cover the *reference* variant nulled either by
/// `null_union_slot` above or by `validate_reference_target_types` (wrong-type FK
/// zeroed in place — it is registered before this fixup). PDTO is excluded: its
/// null is a Topic Data variant the CK does not flag as a package loc/target ref.
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

    #[test]
    fn nulls_own_plugin_leaf_with_missing_target() {
        // LCEP Ref 07854F2E: addresses the output plugin at an interior REFR that
        // was never emitted → null.
        let interner = StringInterner::new();
        let r = resolver(
            &[0x001234],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let leaf = fk(0x854F2E, "SeventySix.esm", &interner);
        assert_eq!(r.resolve(&leaf, &interner), LeafResolution::Null);
    }

    #[test]
    fn keeps_own_plugin_leaf_that_was_emitted() {
        let interner = StringInterner::new();
        let r = resolver(&[0x854F2E], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let leaf = fk(0x854F2E, "SeventySix.esm", &interner);
        assert_eq!(r.resolve(&leaf, &interner), LeafResolution::Keep);
    }

    #[test]
    fn repairs_truncated_master_byte_to_output() {
        // XTNM 00510AF5: addresses Fallout4.esm but 510AF5 isn't a FO4 record and
        // IS emitted in the output (the converted MESG) → repair plugin sym.
        let interner = StringInterner::new();
        let r = resolver(
            &[0x510AF5],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let leaf = fk(0x510AF5, "Fallout4.esm", &interner);
        assert_eq!(r.resolve(&leaf, &interner), LeafResolution::RepairToOutput);
    }

    #[test]
    fn nulls_master_leaf_resolving_nowhere() {
        // XCZR 00552965: addresses Fallout4.esm, not a FO4 record, and the source
        // REFR (interior) was not emitted → null (NOT repaired).
        let interner = StringInterner::new();
        let r = resolver(
            &[0x001234],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let leaf = fk(0x552965, "Fallout4.esm", &interner);
        assert_eq!(r.resolve(&leaf, &interner), LeafResolution::Null);
    }

    #[test]
    fn keeps_valid_master_leaf() {
        // A leaf that genuinely resolves in DLCCoast must never be touched
        // (the task #10 plugin-blind-clobber guard).
        let interner = StringInterner::new();
        let r = resolver(
            &[0x000001],
            &[("Fallout4.esm", &[]), ("DLCCoast.esm", &[0x0247C1])],
            "SeventySix.esm",
        );
        let leaf = fk(0x0247C1, "DLCCoast.esm", &interner);
        assert_eq!(r.resolve(&leaf, &interner), LeafResolution::Keep);
    }

    #[test]
    fn keeps_null_leaf() {
        let interner = StringInterner::new();
        let r = resolver(&[], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let leaf = fk(0, "SeventySix.esm", &interner);
        assert_eq!(r.resolve(&leaf, &interner), LeafResolution::Keep);
    }

    //
    // In a whole-plugin FO76→FO4 worldspace run the exterior placed children
    // (ACHR/REFR) targeted by LCTN LCUN/LCEP/ACEP are re-inserted by the phase-6
    // cell-slice copy AFTER the pre-copy fixup. So the pre-copy pass DEFERS that
    // class (PreCopy{defer=true} leaves the refs intact) and the post-copy repair
    // (PostCopyPlacedChild) resolves it against the now-complete output: a target
    // present post-copy is KEPT, one still absent (a genuine interior dangler) is
    // NULLED (+LCUN row dropped). PreCopy{defer=false} preserves HEAD behavior.

    fn lcep_record(local: u32, plugin: &str, interner: &StringInterner) -> Record {
        // Minimal LCTN with one LCEP ref leaf (List<Struct>{ref}).
        let ref_sym = interner.intern("loc_enable_parent_ref");
        record(
            "LCTN",
            vec![(
                "LCEP",
                FieldValue::List(vec![FieldValue::Struct(vec![(
                    ref_sym,
                    FieldValue::FormKey(fk(local, plugin, interner)),
                )])]),
            )],
            interner,
        )
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
    fn pre_copy_defers_placed_child_lcep() {
        // Target (the not-yet-copied ACHR) absent at pre-copy time, but the class
        // is deferred → the LCEP ref is left intact (would otherwise null).
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = lcep_record(0x7ACB4D, "SeventySix.esm", &interner);
        let changed = apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PreCopy {
                defer_placed_child: true,
            },
        );
        assert!(!changed, "deferred class must be untouched pre-copy");
        assert_eq!(lcep_ref_local(&rec), 0x7ACB4D, "ref left intact");
    }

    #[test]
    fn pre_copy_without_defer_nulls_placed_child_lcep() {
        // HEAD behavior (non-worldspace pipelines): defer=false → absent target
        // nulls in the pre-copy pass exactly as before.
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = lcep_record(0x7ACB4D, "SeventySix.esm", &interner);
        let changed = apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PreCopy {
                defer_placed_child: false,
            },
        );
        assert!(changed);
        assert_eq!(lcep_ref_local(&rec), 0, "absent target nulled");
    }

    #[test]
    fn post_copy_repair_keeps_present_placed_child_lcep() {
        // After the copy the ACHR IS in the output → repair keeps the LCEP ref.
        let interner = StringInterner::new();
        let r = resolver(&[0x7ACB4D], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = lcep_record(0x7ACB4D, "SeventySix.esm", &interner);
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(!changed, "present target → kept");
        assert_eq!(lcep_ref_local(&rec), 0x7ACB4D);
    }

    #[test]
    fn post_copy_repair_nulls_genuine_dangler_lcep() {
        // FeedFish04-style: still absent post-copy (a true interior ref) → nulled.
        let interner = StringInterner::new();
        let r = resolver(
            &[0x7ACB4D],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let mut rec = lcep_record(0x845542, "SeventySix.esm", &interner);
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(changed);
        assert_eq!(lcep_ref_local(&rec), 0, "genuine dangler nulled");
    }

    //
    // QUST alias Forced-Reference (ALFR) and LCTN World-Location-Marker (MNAM)
    // point at worldspace PERSISTENT children materialized in the output only by
    // the persistent-cell phase. Both are deferred pre-copy and resolved by the
    // post-copy repair: target present → kept; absent (interior/test) → ALFR
    // null-in-place, MNAM subrecord dropped.

    fn qust_alfr_record(local: u32, plugin: &str, interner: &StringInterner) -> Record {
        record(
            "QUST",
            vec![("ALFR", FieldValue::FormKey(fk(local, plugin, interner)))],
            interner,
        )
    }

    fn alfr_local(rec: &Record) -> Option<u32> {
        rec.fields
            .iter()
            .find(|e| e.sig.as_str() == "ALFR")
            .and_then(|e| match &e.value {
                FieldValue::FormKey(f) => Some(f.local),
                _ => None,
            })
    }

    #[test]
    fn pre_copy_defers_qust_alfr() {
        // Persistent target absent at pre-copy time, but the class is deferred →
        // the ALFR ref is left intact (would otherwise null as an own-plugin
        // leaf with a missing target).
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = qust_alfr_record(0x343DB5, "SeventySix.esm", &interner);
        let changed = apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PreCopy {
                defer_placed_child: true,
            },
        );
        assert!(!changed, "deferred ALFR must be untouched pre-copy");
        assert_eq!(alfr_local(&rec), Some(0x343DB5), "ref left intact");
    }

    #[test]
    fn pre_copy_without_defer_nulls_qust_alfr() {
        // Neutralize-and-fail anchor: with defer OFF, an own-plugin ALFR whose
        // target was not emitted nulls in place pre-copy. This is the regression
        // the deferral prevents — and the production sweep would have nulled it
        // even earlier. If `("QUST","ALFR")` were dropped from
        // DEFERRED_PLACED_CHILD_SUBRECORDS, `pre_copy_defers_qust_alfr` above
        // would start nulling (defer=true would behave like defer=false here).
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = qust_alfr_record(0x343DB5, "SeventySix.esm", &interner);
        let changed = apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PreCopy {
                defer_placed_child: false,
            },
        );
        assert!(changed);
        assert_eq!(alfr_local(&rec), Some(0), "absent target nulled");
    }

    #[test]
    fn post_copy_repair_keeps_present_qust_alfr() {
        // After the persistent cell lands the ALFR target IS in the output → kept.
        let interner = StringInterner::new();
        let r = resolver(&[0x343DB5], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = qust_alfr_record(0x343DB5, "SeventySix.esm", &interner);
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(!changed, "present target → kept");
        assert_eq!(alfr_local(&rec), Some(0x343DB5));
    }

    #[test]
    fn post_copy_repair_nulls_absent_qust_alfr() {
        // Interior/test-quest forced ref still absent post-copy → null in place
        // (NOT dropped: ALFR is repeatable-scoped, so dropping would corrupt the
        // alias block). Matches the CK-benign state for the ~253 unresolvable.
        let interner = StringInterner::new();
        let r = resolver(
            &[0x111111],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let mut rec = qust_alfr_record(0x845542, "SeventySix.esm", &interner);
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(changed);
        assert_eq!(
            alfr_local(&rec),
            Some(0),
            "absent forced ref nulled in place"
        );
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "ALFR"),
            "ALFR subrecord kept (not dropped)"
        );
    }

    //
    // XTEL decodes to a Struct with a `door` FormKey leaf (the persistent placed
    // REFR teleport target). It defers pre-copy and rebinds post-copy exactly like
    // ALFR: target present post-copy → kept; absent → door leaf nulled in place
    // (NOT dropped — that would lose the teleport link). The transition_interior
    // leaf is walked too but is null/absent in this exterior port.

    fn xtel_record(door: u32, plugin: &str, interner: &StringInterner) -> Record {
        let door_sym = interner.intern("door");
        record(
            "REFR",
            vec![(
                "XTEL",
                FieldValue::Struct(vec![(
                    door_sym,
                    FieldValue::FormKey(fk(door, plugin, interner)),
                )]),
            )],
            interner,
        )
    }

    fn xtel_door_local(rec: &Record) -> u32 {
        let e = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "XTEL")
            .unwrap();
        let FieldValue::Struct(fields) = &e.value else {
            panic!()
        };
        let FieldValue::FormKey(f) = &fields[0].1 else {
            panic!()
        };
        f.local
    }

    #[test]
    fn pre_copy_defers_refr_xtel() {
        // Persistent door absent at pre-copy time, but XTEL is deferred → the door
        // leaf is left intact (would otherwise null as an own-plugin missing target).
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = xtel_record(0x49994D, "SeventySix.esm", &interner);
        let changed = apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PreCopy {
                defer_placed_child: true,
            },
        );
        assert!(!changed, "deferred XTEL must be untouched pre-copy");
        assert_eq!(xtel_door_local(&rec), 0x49994D, "door left intact");
    }

    #[test]
    fn post_copy_repair_keeps_present_refr_xtel() {
        // After the persistent cell lands the door REFR IS in the output → kept.
        let interner = StringInterner::new();
        let r = resolver(&[0x49994D], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = xtel_record(0x49994D, "SeventySix.esm", &interner);
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(!changed, "present door → kept");
        assert_eq!(xtel_door_local(&rec), 0x49994D);
    }

    #[test]
    fn post_copy_repair_nulls_absent_refr_xtel() {
        // Door still absent post-copy → nulled in place (subrecord kept: dropping
        // XTEL would lose the teleport link).
        let interner = StringInterner::new();
        let r = resolver(
            &[0x111111],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let mut rec = xtel_record(0x845542, "SeventySix.esm", &interner);
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(changed);
        assert_eq!(xtel_door_local(&rec), 0, "absent door nulled in place");
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "XTEL"),
            "XTEL subrecord kept (not dropped)"
        );
    }

    #[test]
    fn pre_copy_defers_lctn_mnam() {
        // MNAM is in BOTH DEFERRED and DROP_ON_NULL. With defer on, pre-copy must
        // NOT drop it even though its persistent-marker target is absent yet.
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "LCTN",
            vec![(
                "MNAM",
                FieldValue::FormKey(fk(0x35D2A1, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        let changed = apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PreCopy {
                defer_placed_child: true,
            },
        );
        assert!(!changed, "deferred MNAM untouched pre-copy");
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "MNAM"),
            "MNAM not dropped pre-copy"
        );
    }

    #[test]
    fn post_copy_repair_keeps_present_lctn_mnam() {
        let interner = StringInterner::new();
        let r = resolver(&[0x35D2A1], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "LCTN",
            vec![(
                "MNAM",
                FieldValue::FormKey(fk(0x35D2A1, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(!changed, "present marker → MNAM kept");
        assert!(rec.fields.iter().any(|e| e.sig.as_str() == "MNAM"));
    }

    #[test]
    fn post_copy_repair_drops_absent_lctn_mnam() {
        // Marker still absent post-copy → MNAM is a drop-on-null slot, so the
        // subrecord is dropped (FO4 forbids a NULL MNAM leaf).
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "LCTN",
            vec![(
                "MNAM",
                FieldValue::FormKey(fk(0x35D2A1, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(changed);
        assert!(
            rec.fields.iter().all(|e| e.sig.as_str() != "MNAM"),
            "absent MNAM dropped"
        );
    }

    fn fact_venc_record(container: u32, plugin: &str, interner: &StringInterner) -> Record {
        record(
            "FACT",
            vec![("VENC", FieldValue::FormKey(fk(container, plugin, interner)))],
            interner,
        )
    }

    #[test]
    fn pre_copy_defers_fact_venc() {
        // Merchant container REFR absent at pre-copy time, but VENC is deferred → the
        // FK is left intact. Without the deferral fix_invalid_target_formkeys nulls
        // it and the type-validator strips the present-but-null VENC → every vendor
        // loses its merchant container (the bug this reproduces).
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = fact_venc_record(0x629E0C, "SeventySix.esm", &interner);
        let changed = apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PreCopy {
                defer_placed_child: true,
            },
        );
        assert!(!changed, "deferred FACT VENC must be untouched pre-copy");
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "VENC"),
            "VENC not dropped pre-copy"
        );
    }

    #[test]
    fn post_copy_repair_keeps_present_fact_venc() {
        // After the cell copy lands the container REFR IS in the output → VENC kept.
        let interner = StringInterner::new();
        let r = resolver(&[0x629E0C], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = fact_venc_record(0x629E0C, "SeventySix.esm", &interner);
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(!changed, "present container → VENC kept");
        assert!(rec.fields.iter().any(|e| e.sig.as_str() == "VENC"));
    }

    #[test]
    fn post_copy_repair_drops_absent_fact_venc() {
        // Container still absent post-copy → VENC is a drop-on-null slot, so the
        // subrecord is dropped (FO4 forbids a NULL VENC leaf).
        let interner = StringInterner::new();
        let r = resolver(&[0x001234], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = fact_venc_record(0x629E0C, "SeventySix.esm", &interner);
        let changed = apply_to_record(&mut rec, &r, &interner, ApplyMode::PostCopyPlacedChild);
        assert!(changed);
        assert!(
            rec.fields.iter().all(|e| e.sig.as_str() != "VENC"),
            "absent container → VENC dropped"
        );
    }

    #[test]
    fn post_copy_repair_drops_absent_lcun_row_keeps_present() {
        // LCUN repair: the row whose actor_ref is still absent post-copy is
        // dropped in lockstep; the row whose actor_ref is present is kept.
        let interner = StringInterner::new();
        let r = resolver(&[0x2B47D1], &[("Fallout4.esm", &[])], "SeventySix.esm");
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
        let mut rec = record(
            "LCTN",
            vec![(
                "LCUN",
                FieldValue::List(vec![
                    row(0x35C950, 0x35C953, 0x2B47D1), // actor still absent → drop row
                    row(0x35C950, 0x2B47D1, 0x2B47D1), // actor present → keep row
                ]),
            )],
            &interner,
        );
        assert!(apply_to_record(
            &mut rec,
            &r,
            &interner,
            ApplyMode::PostCopyPlacedChild
        ));
        let e = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "LCUN")
            .unwrap();
        let FieldValue::List(rows) = &e.value else {
            panic!()
        };
        assert_eq!(rows.len(), 1, "absent-actor row dropped");
    }

    fn union_bytes(kind: i32, raw: u32) -> smallvec::SmallVec<[u8; 32]> {
        let mut b = smallvec::SmallVec::<[u8; 32]>::new();
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&raw.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b
    }

    #[test]
    fn nulls_dangling_pack_ptda_reference() {
        // PTDA 008A483A (FeedFish04 REFR, interior, no FO4 equiv): type-0
        // reference, resolves nowhere → null offset-4 FK.
        let r = resolver(
            &[0x111111],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let mut b = union_bytes(0, 0x008A483A);
        assert!(null_union_slot(&mut b, "PTDA", &r, 7));
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0);
    }

    #[test]
    fn keeps_master_inherited_pack_ptda_object_id() {
        // PTDA 000DF42E (CombatRifle): type-1 object_id resolving in Fallout4.esm
        // (master-inherited) → keep byte-identical.
        let r = resolver(&[], &[("Fallout4.esm", &[0x0DF42E])], "SeventySix.esm");
        let mut b = union_bytes(1, 0x000DF42E);
        assert!(!null_union_slot(&mut b, "PTDA", &r, 7));
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0x000DF42E);
    }

    #[test]
    fn keeps_emitted_07_pack_pldt() {
        // PLDT type-0 already remapped to an emitted 07 record → keep.
        let r = resolver(&[0x525F60], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut b = union_bytes(0, 0x07525F60);
        assert!(!null_union_slot(&mut b, "PLDT", &r, 7));
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0x07525F60);
    }

    #[test]
    fn skips_pack_ptda_scalar_type2() {
        // PTDA kind 2 = object_type (a u32 form-type code, NOT a FormID): never
        // touched even if the offset-4 u32 collides with a dangling-looking value.
        let r = resolver(&[], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut b = union_bytes(2, 0x0000000F);
        assert!(!null_union_slot(&mut b, "PTDA", &r, 7));
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0x0000000F);
    }

    #[test]
    fn nulls_pack_ptda_keyword_type3_resolving_nowhere() {
        // Round-8 #7: a kind-3 keyword PTDA whose target resolves in neither output
        // nor any master → null offset-4 (the gate now admits kind 3).
        let r = resolver(
            &[0x111111],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let mut b = union_bytes(3, 0x008A483A);
        assert!(null_union_slot(&mut b, "PTDA", &r, 7));
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0);
    }

    #[test]
    fn benignifies_null_reference_ptda_to_self() {
        // PTDA type-0 Reference with a null FK → CK "Package Target Reference
        // (00000000)". Rewrite the selector to Self (type 6); value stays 0.
        let mut b = union_bytes(PACK_UNION_REFERENCE_TYPE, 0);
        assert!(benignify_value0_reference_union(&mut b, "PTDA"));
        assert_eq!(
            i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            PACK_TARGET_SELF_TYPE
        );
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0);
    }

    #[test]
    fn benignifies_null_reference_pldt_and_plvd_to_near_package_start() {
        for sig in ["PLDT", "PLVD"] {
            let mut b = union_bytes(PACK_UNION_REFERENCE_TYPE, 0);
            assert!(benignify_value0_reference_union(&mut b, sig), "{sig}");
            assert_eq!(
                i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                PACK_LOCATION_NEAR_PACKAGE_START_TYPE,
                "{sig}"
            );
        }
    }

    #[test]
    fn benignify_leaves_resolvable_reference_untouched() {
        // A Reference variant whose FK is NON-null (a valid target) must not be
        // rewritten — only null-valued references are benignified.
        let mut b = union_bytes(PACK_UNION_REFERENCE_TYPE, 0x07525F60);
        assert!(!benignify_value0_reference_union(&mut b, "PTDA"));
        assert_eq!(
            i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            PACK_UNION_REFERENCE_TYPE
        );
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0x07525F60);
    }

    #[test]
    fn benignify_skips_non_reference_variant() {
        // A non-Reference selector (e.g. PTDA type-2 object_type scalar) is left
        // alone even with a zero value — it is not a "Reference (00000000)".
        let mut b = union_bytes(2, 0);
        assert!(!benignify_value0_reference_union(&mut b, "PTDA"));
        assert_eq!(i32::from_le_bytes([b[0], b[1], b[2], b[3]]), 2);
    }

    #[test]
    fn null_then_benignify_feedfish04_ptda() {
        // End-to-end of the apply path on the FeedFish04 PTDA (type-0 reference,
        // interior REFR absent everywhere): null_union_slot zeros the FK, then
        // benignify rewrites the selector to Self → CK-clean self target.
        let r = resolver(
            &[0x111111],
            &[("Fallout4.esm", &[0x000010])],
            "SeventySix.esm",
        );
        let mut b = union_bytes(PACK_UNION_REFERENCE_TYPE, 0x008A483A);
        assert!(null_union_slot(&mut b, "PTDA", &r, 7));
        assert!(benignify_value0_reference_union(&mut b, "PTDA"));
        assert_eq!(
            i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            PACK_TARGET_SELF_TYPE
        );
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0);
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

    #[test]
    fn keeps_dlbr_with_emitted_output_dial_starting_topic() {
        let interner = StringInterner::new();
        let r = resolver_with_dials(
            &[0x0565C1],
            &[0x0565C1],
            &[("Fallout4.esm", &[], &[])],
            "SeventySix.esm",
        );
        let mut rec = record(
            "DLBR",
            vec![(
                "SNAM",
                FieldValue::FormKey(fk(0x0565C1, "SeventySix.esm", &interner)),
            )],
            &interner,
        );

        assert_eq!(
            resolve_dlbr_starting_topic(&mut rec, &r, &interner),
            DlbrStartingTopicResolution::Keep
        );
    }

    #[test]
    fn keeps_dlbr_with_valid_master_dial_starting_topic() {
        let interner = StringInterner::new();
        let r = resolver_with_dials(
            &[0x000800],
            &[],
            &[("Fallout4.esm", &[0x01A2B3], &[0x01A2B3])],
            "SeventySix.esm",
        );
        let mut rec = record(
            "DLBR",
            vec![(
                "SNAM",
                FieldValue::FormKey(fk(0x01A2B3, "Fallout4.esm", &interner)),
            )],
            &interner,
        );

        assert_eq!(
            resolve_dlbr_starting_topic(&mut rec, &r, &interner),
            DlbrStartingTopicResolution::Keep
        );
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
    fn rejects_dlbr_with_null_missing_dangling_or_wrong_type_starting_topic() {
        let interner = StringInterner::new();
        let r = resolver_with_dials(
            &[0x0565C1],
            &[],
            &[("Fallout4.esm", &[], &[])],
            "SeventySix.esm",
        );
        let null = record(
            "DLBR",
            vec![(
                "SNAM",
                FieldValue::FormKey(fk(0, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        let decoded_null = record("DLBR", vec![("SNAM", FieldValue::None)], &interner);
        let missing = record("DLBR", vec![], &interner);
        let dangling = record(
            "DLBR",
            vec![(
                "SNAM",
                FieldValue::FormKey(fk(0x2C505F, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        let wrong_type = record(
            "DLBR",
            vec![(
                "SNAM",
                FieldValue::FormKey(fk(0x0565C1, "SeventySix.esm", &interner)),
            )],
            &interner,
        );

        for mut rec in [null, decoded_null, missing, dangling, wrong_type] {
            assert_eq!(
                resolve_dlbr_starting_topic(&mut rec, &r, &interner),
                DlbrStartingTopicResolution::Invalid
            );
        }
    }

    #[test]
    fn drops_incoming_dial_branch_ref_after_dlbr_prune() {
        let interner = StringInterner::new();
        let r = resolver(&[0x0565C1], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "DIAL",
            vec![(
                "BNAM",
                FieldValue::FormKey(fk(0x28950A, "SeventySix.esm", &interner)),
            )],
            &interner,
        );

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(rec.fields.iter().all(|entry| entry.sig.as_str() != "BNAM"));
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

    #[test]
    fn drops_null_info_dnam_subrecord() {
        // INFO DNAM Shared-INFO whose target (an own-plugin INFO) was never emitted
        // → drop the subrecord, don't leave a NULL leaf.
        let interner = StringInterner::new();
        let r = resolver(&[0x111111], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "INFO",
            vec![
                (
                    "GNAM",
                    FieldValue::FormKey(fk(0x111111, "SeventySix.esm", &interner)),
                ),
                (
                    "DNAM",
                    FieldValue::FormKey(fk(0x37F7FC, "SeventySix.esm", &interner)),
                ), // missing
            ],
            &interner,
        );
        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(
            rec.fields.iter().all(|e| e.sig.as_str() != "DNAM"),
            "null DNAM must be dropped"
        );
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "GNAM"),
            "GNAM kept"
        );
    }

    #[test]
    fn keeps_resolvable_wrld_wnam() {
        // WRLD WNAM Parent that DOES resolve in output → kept (no drop).
        let interner = StringInterner::new();
        let r = resolver(&[0x00F7F5], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "WRLD",
            vec![(
                "WNAM",
                FieldValue::FormKey(fk(0x00F7F5, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        assert!(!apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "WNAM"),
            "resolvable WNAM kept"
        );
    }

    #[test]
    fn drops_null_wrld_wnam_subrecord() {
        let interner = StringInterner::new();
        let r = resolver(&[], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "WRLD",
            vec![(
                "WNAM",
                FieldValue::FormKey(fk(0x123456, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(rec.fields.is_empty(), "null WNAM dropped");
    }

    #[test]
    fn drops_null_wrld_nam3_subrecord() {
        let interner = StringInterner::new();
        let r = resolver(&[], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "WRLD",
            vec![(
                "NAM3",
                FieldValue::FormKey(fk(0x123456, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(rec.fields.is_empty(), "null NAM3 dropped");
    }

    #[test]
    fn keeps_resolvable_wrld_nam3() {
        let interner = StringInterner::new();
        let r = resolver(&[0x00F7F5], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "WRLD",
            vec![(
                "NAM3",
                FieldValue::FormKey(fk(0x00F7F5, "SeventySix.esm", &interner)),
            )],
            &interner,
        );
        assert!(!apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "NAM3"),
            "resolvable NAM3 kept"
        );
    }

    #[test]
    fn drops_null_cell_xilw_struct_subrecord() {
        let interner = StringInterner::new();
        let worldspace = interner.intern("Worldspace");
        let r = resolver(&[], &[("Fallout4.esm", &[])], "SeventySix.esm");
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
        assert!(rec.fields.is_empty(), "unresolved XILW struct dropped");
    }

    #[test]
    fn keeps_resolvable_cell_xilw_struct_subrecord() {
        let interner = StringInterner::new();
        let worldspace = interner.intern("Worldspace");
        let r = resolver(&[], &[("Fallout4.esm", &[0x635F96])], "SeventySix.esm");
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

        assert!(!apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(rec.fields.iter().any(|entry| entry.sig.as_str() == "XILW"));
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
    fn drops_cell_xilw_when_same_id_output_record_is_wrong_type() {
        let interner = StringInterner::new();
        let r = resolver_with_cell_types(
            &[0x635F96],
            &[],
            &[0x635F96],
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

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(rec.fields.is_empty());
    }

    #[test]
    fn keeps_cell_xown_with_valid_master_owner() {
        let interner = StringInterner::new();
        let r = resolver_with_cell_types(
            &[],
            &[],
            &[],
            &[("Fallout4.esm", &[0x01C21C], &[], &[0x01C21C])],
            "SeventySix.esm",
        );
        let mut payload = smallvec::SmallVec::<[u8; 32]>::new();
        payload.extend_from_slice(&0x0001_C21C_u32.to_le_bytes());
        payload.extend_from_slice(&[0; 8]);
        let mut rec = record(
            "CELL",
            vec![("XOWN", FieldValue::Bytes(payload.clone()))],
            &interner,
        );

        assert!(!apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(matches!(&rec.fields[0].value, FieldValue::Bytes(bytes) if bytes == &payload));
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
    fn drops_cell_xown_when_same_id_output_record_is_wrong_type() {
        let interner = StringInterner::new();
        let r = resolver_with_cell_types(
            &[0x2744B3],
            &[0x2744B3],
            &[],
            &[("Fallout4.esm", &[], &[], &[])],
            "SeventySix.esm",
        );
        let mut payload =
            smallvec::SmallVec::<[u8; 32]>::from_slice(&0x0027_44B3_u32.to_le_bytes());
        payload.extend_from_slice(&[0; 8]);
        let mut rec = record(
            "CELL",
            vec![("XOWN", FieldValue::Bytes(payload))],
            &interner,
        );

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(rec.fields.is_empty());
    }

    #[test]
    fn drops_cell_xown_with_missing_owner() {
        let interner = StringInterner::new();
        let r = resolver_with_cell_types(
            &[],
            &[],
            &[],
            &[("Fallout4.esm", &[], &[], &[])],
            "SeventySix.esm",
        );
        let mut payload =
            smallvec::SmallVec::<[u8; 32]>::from_slice(&0x0042_A260_u32.to_le_bytes());
        payload.extend_from_slice(&[0; 8]);
        let mut rec = record(
            "CELL",
            vec![("XOWN", FieldValue::Bytes(payload))],
            &interner,
        );

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert!(rec.fields.is_empty());
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
    fn drops_null_scen_player_dialogue_response_subrecord() {
        let interner = StringInterner::new();
        let r = resolver(&[0x59958B], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "SCEN",
            vec![
                ("ANAM", FieldValue::Uint(1)),
                ("PTOP", FieldValue::None),
                (
                    "NTOP",
                    FieldValue::FormKey(fk(0x59958B, "SeventySix.esm", &interner)),
                ),
                ("NPOT", FieldValue::None),
            ],
            &interner,
        );

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        let sigs: Vec<&str> = rec.fields.iter().map(|entry| entry.sig.as_str()).collect();
        assert_eq!(sigs, vec!["ANAM", "NTOP"]);
    }

    #[test]
    fn drops_dangling_scen_dialogue_response_subrecord() {
        let interner = StringInterner::new();
        let r = resolver(&[0x59958B], &[("Fallout4.esm", &[])], "SeventySix.esm");
        let mut rec = record(
            "SCEN",
            vec![
                ("ANAM", FieldValue::Uint(1)),
                (
                    "PTOP",
                    FieldValue::FormKey(fk(0x59957F, "SeventySix.esm", &interner)),
                ),
                (
                    "NTOP",
                    FieldValue::FormKey(fk(0x59958B, "SeventySix.esm", &interner)),
                ),
            ],
            &interner,
        );

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        let sigs: Vec<&str> = rec.fields.iter().map(|entry| entry.sig.as_str()).collect();
        assert_eq!(sigs, vec!["ANAM", "NTOP"]);
    }

    #[test]
    fn drops_lcun_row_with_null_actor_ref() {
        let interner = StringInterner::new();
        let r = resolver(
            &[0x35C950, 0x2B47D1],
            &[("Fallout4.esm", &[])],
            "SeventySix.esm",
        );
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
        let mut rec = record(
            "LCTN",
            vec![(
                "LCUN",
                FieldValue::List(vec![
                    row(0x35C950, 0x35C953, 0x2B47D1), // actor 35C953 not emitted → drop row
                    row(0x35C950, 0x2B47D1, 0x2B47D1), // actor 2B47D1 emitted → keep row
                ]),
            )],
            &interner,
        );
        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        let e = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "LCUN")
            .expect("LCUN kept");
        let FieldValue::List(rows) = &e.value else {
            panic!()
        };
        assert_eq!(rows.len(), 1, "the null-actor row must be dropped");
    }

    #[test]
    fn drops_npc_cnto_with_dangling_item() {
        // NPC_ CNTO struct:I,i — item 0700000F (FO76 Caps001, no FO4 record) → drop.
        // The raw FormID addresses the output plugin (master index 7 = number of
        // target masters), so we model 7 masters and an output set without 00000F.
        let interner = StringInterner::new();
        let mut cnto = smallvec::SmallVec::<[u8; 32]>::new();
        cnto.extend_from_slice(&(0x0700_000Fu32).to_le_bytes()); // item @ output master index 7
        cnto.extend_from_slice(&1u32.to_le_bytes()); // count
        let mut rec = record(
            "NPC_",
            vec![
                ("COCT", FieldValue::Uint(1)),
                ("CNTO", FieldValue::Bytes(cnto)),
                (
                    "NAM8",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1, 0, 0, 0])),
                ),
            ],
            &interner,
        );
        let r7 = LeafResolver {
            output_objids: [0x222222].into_iter().collect(),
            master_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_dial_objids: FxHashSet::default(),
            master_dial_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_wrld_objids: FxHashSet::default(),
            master_wrld_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_owner_objids: FxHashSet::default(),
            master_owner_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            master_names: (0..7).map(|i| format!("M{i}.esm")).collect(),
            output_plugin: "SeventySix.esm".to_string(),
        };
        assert!(apply_to_record(&mut rec, &r7, &interner, PRE_COPY));
        assert!(
            rec.fields.iter().all(|e| e.sig.as_str() != "CNTO"),
            "dangling CNTO dropped"
        );
        assert!(
            rec.fields.iter().all(|e| e.sig.as_str() != "COCT"),
            "stale inventory count dropped with the final CNTO row"
        );
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "NAM8"),
            "NAM8 kept"
        );
    }

    #[test]
    fn syncs_npc_coct_after_dropping_some_cnto_rows() {
        let interner = StringInterner::new();
        let mut dangling_cnto = smallvec::SmallVec::<[u8; 32]>::new();
        dangling_cnto.extend_from_slice(&(0x0700_000Fu32).to_le_bytes());
        dangling_cnto.extend_from_slice(&1u32.to_le_bytes());
        let mut kept_cnto = smallvec::SmallVec::<[u8; 32]>::new();
        kept_cnto.extend_from_slice(&(0x0700_0022u32).to_le_bytes());
        kept_cnto.extend_from_slice(&3u32.to_le_bytes());
        let mut rec = record(
            "NPC_",
            vec![
                ("COCT", FieldValue::Uint(2)),
                ("CNTO", FieldValue::Bytes(dangling_cnto)),
                ("CNTO", FieldValue::Bytes(kept_cnto)),
            ],
            &interner,
        );
        let r7 = LeafResolver {
            output_objids: [0x000022].into_iter().collect(),
            master_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_dial_objids: FxHashSet::default(),
            master_dial_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_wrld_objids: FxHashSet::default(),
            master_wrld_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_owner_objids: FxHashSet::default(),
            master_owner_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            master_names: (0..7).map(|i| format!("M{i}.esm")).collect(),
            output_plugin: "SeventySix.esm".to_string(),
        };
        assert!(apply_to_record(&mut rec, &r7, &interner, PRE_COPY));
        assert_eq!(
            rec.fields
                .iter()
                .filter(|e| e.sig.as_str() == "CNTO")
                .count(),
            1
        );
        assert!(
            matches!(
                rec.fields
                    .iter()
                    .find(|e| e.sig.as_str() == "COCT")
                    .map(|e| &e.value),
                Some(FieldValue::Uint(1))
            ),
            "COCT must match surviving CNTO rows"
        );
    }

    #[test]
    fn repairs_npc_cnto_to_matching_master_item() {
        let interner = StringInterner::new();
        let mut cnto = smallvec::SmallVec::<[u8; 32]>::new();
        cnto.extend_from_slice(&(0x0711_3339u32).to_le_bytes());
        cnto.extend_from_slice(&1u32.to_le_bytes());
        let mut rec = record(
            "NPC_",
            vec![
                ("COCT", FieldValue::Uint(1)),
                ("CNTO", FieldValue::Bytes(cnto)),
            ],
            &interner,
        );
        let mut master_objids: Vec<FxHashSet<u32>> = (0..7).map(|_| FxHashSet::default()).collect();
        master_objids[0].insert(0x113339);
        let r7 = LeafResolver {
            output_objids: FxHashSet::default(),
            master_objids,
            output_dial_objids: FxHashSet::default(),
            master_dial_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_wrld_objids: FxHashSet::default(),
            master_wrld_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_owner_objids: FxHashSet::default(),
            master_owner_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            master_names: (0..7).map(|i| format!("M{i}.esm")).collect(),
            output_plugin: "SeventySix.esm".to_string(),
        };

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

    #[test]
    fn keeps_npc_cnto_with_resolvable_item() {
        let interner = StringInterner::new();
        let mut cnto = smallvec::SmallVec::<[u8; 32]>::new();
        cnto.extend_from_slice(&(0x0700_0022u32).to_le_bytes());
        cnto.extend_from_slice(&3u32.to_le_bytes());
        let mut rec = record("NPC_", vec![("CNTO", FieldValue::Bytes(cnto))], &interner);
        let r7 = LeafResolver {
            output_objids: [0x000022].into_iter().collect(),
            master_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_dial_objids: FxHashSet::default(),
            master_dial_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_wrld_objids: FxHashSet::default(),
            master_wrld_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            output_owner_objids: FxHashSet::default(),
            master_owner_objids: (0..7).map(|_| FxHashSet::default()).collect(),
            master_names: (0..7).map(|i| format!("M{i}.esm")).collect(),
            output_plugin: "SeventySix.esm".to_string(),
        };
        assert!(!apply_to_record(&mut rec, &r7, &interner, PRE_COPY));
        assert!(
            rec.fields.iter().any(|e| e.sig.as_str() == "CNTO"),
            "resolvable CNTO kept"
        );
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
    fn drops_invalid_cont_cnto_rows_and_syncs_coct() {
        let interner = StringInterner::new();
        let mut rec = record(
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
            &interner,
        );
        let r = resolver(&[0x000022], &[("Fallout4.esm", &[])], "SeventySix.esm");

        assert!(apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert_eq!(
            rec.fields
                .iter()
                .filter(|entry| entry.sig.as_str() == "CNTO")
                .count(),
            1
        );
        assert!(matches!(
            rec.fields
                .iter()
                .find(|entry| entry.sig.as_str() == "COCT")
                .map(|entry| &entry.value),
            Some(FieldValue::Uint(1))
        ));
    }

    #[test]
    fn keeps_valid_cont_cnto_rows_and_matching_coct() {
        let interner = StringInterner::new();
        let mut rec = record(
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
            &interner,
        );
        let r = resolver(
            &[0x000022],
            &[("Fallout4.esm", &[0x001234])],
            "SeventySix.esm",
        );

        assert!(!apply_to_record(&mut rec, &r, &interner, PRE_COPY));
        assert_eq!(
            rec.fields
                .iter()
                .filter(|entry| entry.sig.as_str() == "CNTO")
                .count(),
            2
        );
        assert!(matches!(
            rec.fields
                .iter()
                .find(|entry| entry.sig.as_str() == "COCT")
                .map(|entry| &entry.value),
            Some(FieldValue::Uint(2))
        ));
    }
}
