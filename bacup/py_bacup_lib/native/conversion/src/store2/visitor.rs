//! store2::visitor — fused fixup sweeps.
//!
//! A `RecordVisitor` adapts one legacy fixup's pure kernel to record-major
//! execution. `run_sweep` executes an ordered visitor list over the union of
//! their candidate records in ONE pass: serial per-visitor `applies` +
//! `gather` (at the sweep's registry position, so indices observe exactly
//! what each legacy fixup's own gather would have observed), parallel decide
//! (read-only over `ReadView`, rayon — the same split
//! `session::map_apply_by_sig` already proves safe), then serial apply in
//! enumeration order.
//!
//! Lanes are homogeneous per sweep:
//! - `Decoded` visitors mutate the schema-decoded `Record`; apply re-encodes
//!   via `replace_records_contents` — matching legacy decoded-lane fixups.
//! - `RawBytes` visitors emit per-subrecord byte patches; apply goes through
//!   `patch_all_subrecords_bytes` (no re-encode) — matching legacy raw-lane.
//!
//! Within one record, visitors compose in list order on the same in-memory
//! state (decoded `Record`, or materialized subrecord bytes for the raw
//! lane), mirroring the legacy fixup-major composition.

use std::any::Any;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::{FormKeyMapper, MapperSnapshot};
use crate::ids::{FormKey, SigCode};
use crate::record::Record;
use crate::schema::AuthoringSchema;
use crate::session::{PluginSession, ReadView};
use crate::sym::Sym;

/// Memoizes target-master plugin scans across sweeps. Fixups never mutate
/// master plugins, so per-master-handle gather products are stable for an
/// entire `apply_fixups_v2` run. Output-plugin-derived data must NOT go
/// through this cache — legacy segments mutate the output between sweeps.
#[derive(Default)]
pub struct MasterScanCache {
    objids_by_handle: FxHashMap<u64, Arc<FxHashSet<u32>>>,
}

impl MasterScanCache {
    /// `local_object_ids_in_handle` for each master handle, in list order —
    /// the master_objids scan shared by the Slot/Vmad/Htid resolvers.
    pub fn master_objid_sets(
        &mut self,
        session: &mut PluginSession,
        master_handle_ids: &[u64],
    ) -> Result<Vec<Arc<FxHashSet<u32>>>, FixupError> {
        master_handle_ids
            .iter()
            .map(|&handle_id| {
                if let Some(set) = self.objids_by_handle.get(&handle_id) {
                    return Ok(Arc::clone(set));
                }
                let set = Arc::new(
                    session
                        .local_object_ids_in_handle(handle_id)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?,
                );
                self.objids_by_handle.insert(handle_id, Arc::clone(&set));
                Ok(set)
            })
            .collect()
    }
}

/// Decide+apply mutation lane. A sweep never mixes lanes (re-encoding a
/// record that legacy only byte-patched would normalize unrelated bytes and
/// break byte parity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Decoded,
    RawBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitOutcome {
    Unchanged,
    Changed,
}

/// One raw-lane edit: replace the bytes of the `occurrence`-th subrecord
/// (0-based, counted among subrecords with signature `sig`) of the record.
/// Contract: only emit a patch whose `new_bytes` differ from the current
/// bytes (a no-op patch would inflate the changed count vs legacy).
#[derive(Debug, Clone)]
pub struct SubrecordPatch {
    pub sig: &'static str,
    pub occurrence: usize,
    pub new_bytes: Vec<u8>,
}

/// Output of a visitor's serial gather phase.
pub struct GatherOutput {
    /// Target signatures this visitor scans, in the SAME order the legacy
    /// fixup iterates them (warning/report order parity).
    pub candidate_sigs: Vec<SigCode>,
    /// The visitor's pre-built read-only index (the legacy fixup's own gather
    /// result), downcast back inside `visit_*`.
    pub index: Option<Box<dyn Any + Send + Sync>>,
    /// Warnings emitted while gathering (folded into this visitor's report).
    pub warnings: Vec<Sym>,
}

impl GatherOutput {
    pub fn sigs_only(candidate_sigs: Vec<SigCode>) -> Self {
        Self {
            candidate_sigs,
            index: None,
            warnings: Vec::new(),
        }
    }
}

/// Read-only shared state for one sweep.
pub struct SweepCtx<'a> {
    pub config: &'a FixupConfig,
    pub schema: &'a AuthoringSchema,
    pub snapshot: MapperSnapshot,
    /// `StringInterner` is `Sync`; legacy kernels intern warnings / resolve
    /// EDID syms from rayon decide closures today (`map_apply_by_sig` users).
    pub interner: &'a crate::sym::StringInterner,
}

pub trait RecordVisitor: Send + Sync {
    /// MUST equal the legacy fixup's `name()` — report-stream parity.
    fn name(&self) -> &'static str;

    fn lane(&self) -> Lane;

    /// Mirrors the legacy `applies_to_session` gate. A non-applying visitor
    /// contributes nothing and emits NO report entry (legacy parity:
    /// `FixupRegistry::run_all` `continue`s without a report).
    fn applies(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    /// Serial pre-pass at the sweep's registry position. Calls the legacy
    /// fixup's existing gather function(s) and returns candidate sigs + the
    /// pre-built index. `master_cache` memoizes master-derived scans across
    /// sweeps; output-plugin-derived data must be gathered fresh every time.
    fn gather(
        &self,
        session: &mut PluginSession,
        mapper: &FormKeyMapper,
        config: &FixupConfig,
        master_cache: &mut MasterScanCache,
    ) -> Result<GatherOutput, FixupError>;

    /// Decoded lane. Mutate `record` in place via the legacy kernel.
    fn visit_decoded(
        &self,
        _record: &mut Record,
        _index: Option<&(dyn Any + Send + Sync)>,
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        unreachable!("{}: decoded visit on raw-lane visitor", self.name())
    }

    /// Raw lane. Inspect the record's subrecords ((signature, bytes) in record
    /// order, reflecting earlier same-sweep visitors' patches) and return
    /// patches against THIS state.
    fn visit_raw(
        &self,
        _subrecords: &[(&str, &[u8])],
        _index: Option<&(dyn Any + Send + Sync)>,
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> Vec<SubrecordPatch> {
        unreachable!("{}: raw visit on decoded-lane visitor", self.name())
    }
}

pub struct Sweep {
    pub label: &'static str,
    pub visitors: Vec<Box<dyn RecordVisitor>>,
}

const PAR_THRESHOLD: usize = 64;

enum LaneEdit {
    Decoded(Record),
    /// Final composed bytes per (sig, occurrence) after in-order visitor
    /// application against the materialized subrecord copy.
    Raw(FxHashMap<(&'static str, usize), Vec<u8>>),
}

struct Stash {
    fk: FormKey,
    edit: LaneEdit,
    /// Active-visitor indices that changed this record.
    changed_by: Vec<usize>,
    /// (active-visitor index, warnings) emitted while deciding this record.
    warnings: Vec<(usize, Vec<Sym>)>,
}

/// Execute one lane-homogeneous sweep. Returns one `(name, FixupReport)` per
/// APPLYING visitor, in visitor order (non-applying visitors are absent,
/// matching the legacy registry).
pub fn run_sweep(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    sweep: &Sweep,
    master_cache: &mut MasterScanCache,
) -> Result<Vec<(String, FixupReport)>, FixupError> {
    use rayon::prelude::*;
    use std::time::Instant;

    let start = Instant::now();
    if sweep.visitors.is_empty() {
        return Ok(Vec::new());
    }
    let lane = sweep.visitors[0].lane();
    assert!(
        sweep.visitors.iter().all(|v| v.lane() == lane),
        "sweep '{}' mixes lanes",
        sweep.label
    );

    let schema = config
        .target_schema
        .as_deref()
        .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
    let interner = mapper.interner;

    // ---- serial: applies + gather, in visitor order ----------------------
    let active: Vec<&dyn RecordVisitor> = sweep
        .visitors
        .iter()
        .map(|v| v.as_ref())
        .filter(|v| v.applies(session, config))
        .collect();
    if active.is_empty() {
        return Ok(Vec::new());
    }

    let mut gathers: Vec<GatherOutput> = Vec::with_capacity(active.len());
    for v in &active {
        gathers.push(v.gather(session, &*mapper, config, master_cache)?);
    }

    let cx = SweepCtx {
        config,
        schema,
        snapshot: mapper.as_read_snapshot(),
        interner,
    };

    // ---- candidates: union over visitors, first-seen order ----------------
    // Per record we track which active visitors want it as a bitmask whose
    // ascending bit order equals visitor order (entries are only ever added
    // with non-decreasing vi, so the mask loses no ordering information).
    assert!(
        active.len() <= 64,
        "sweep '{}' exceeds the 64-visitor wanting bitmask",
        sweep.label
    );
    let stashes: Vec<Stash> = {
        let view = session
            .read_view()
            .map_err(|e| FixupError::Other(e.to_string()))?;

        // Materialize per-(visitor, sig) candidate lists first so the union
        // containers can be exactly sized.
        let mut sig_lists: Vec<(usize, Vec<FormKey>)> = Vec::new();
        let mut total = 0usize;
        let mut seen_sigs: FxHashSet<SigCode> = FxHashSet::default();
        let mut sigs_overlap = false;
        for (vi, gather) in gathers.iter().enumerate() {
            for sig in &gather.candidate_sigs {
                sigs_overlap |= !seen_sigs.insert(*sig);
                let fks = view.form_keys_of_sig(*sig, interner);
                total += fks.len();
                sig_lists.push((vi, fks));
            }
        }

        let mut order: Vec<(FormKey, u64)> = Vec::with_capacity(total);
        if !sigs_overlap {
            // No signature appears twice across the gathered lists, so no
            // FormKey can repeat: first-seen order is plain concatenation.
            for (vi, fks) in sig_lists {
                let bit = 1u64 << vi;
                order.extend(fks.into_iter().map(|fk| (fk, bit)));
            }
        } else {
            let mut idx_by_fk: FxHashMap<FormKey, u32> =
                FxHashMap::with_capacity_and_hasher(total, Default::default());
            for (vi, fks) in sig_lists {
                let bit = 1u64 << vi;
                for fk in fks {
                    match idx_by_fk.entry(fk) {
                        std::collections::hash_map::Entry::Occupied(e) => {
                            order[*e.get() as usize].1 |= bit;
                        }
                        std::collections::hash_map::Entry::Vacant(e) => {
                            e.insert(order.len() as u32);
                            order.push((fk, bit));
                        }
                    }
                }
            }
        }

        let decide = |&(fk, mask): &(FormKey, u64)| -> Option<Stash> {
            let wanting = (0..active.len()).filter(|vi| mask & (1u64 << vi) != 0);
            let mut changed_by = Vec::new();
            let mut rec_warnings: Vec<(usize, Vec<Sym>)> = Vec::new();
            match lane {
                Lane::Decoded => {
                    // Legacy fixups skip records that fail to decode.
                    let mut record = view.record_decoded(&fk, schema, interner).ok()?;
                    for vi in wanting {
                        let mut w = Vec::new();
                        let outcome = active[vi].visit_decoded(
                            &mut record,
                            gathers[vi].index.as_deref(),
                            &cx,
                            &mut w,
                        );
                        if !w.is_empty() {
                            rec_warnings.push((vi, w));
                        }
                        if outcome == VisitOutcome::Changed {
                            changed_by.push(vi);
                        }
                    }
                    if changed_by.is_empty() && rec_warnings.is_empty() {
                        return None;
                    }
                    Some(Stash {
                        fk,
                        edit: LaneEdit::Decoded(record),
                        changed_by,
                        warnings: rec_warnings,
                    })
                }
                Lane::RawBytes => {
                    let parsed = view.record_parsed(&fk, interner)?;
                    let mut subs: Vec<(&str, Vec<u8>)> = parsed
                        .subrecords
                        .iter()
                        .map(|sr| (sr.signature.as_str(), sr.data.to_vec()))
                        .collect();
                    let mut final_patches: FxHashMap<(&'static str, usize), Vec<u8>> =
                        FxHashMap::default();
                    for vi in wanting {
                        let sub_view: Vec<(&str, &[u8])> =
                            subs.iter().map(|(s, d)| (*s, d.as_slice())).collect();
                        let mut w = Vec::new();
                        let patches = active[vi].visit_raw(
                            &sub_view,
                            gathers[vi].index.as_deref(),
                            &cx,
                            &mut w,
                        );
                        if !w.is_empty() {
                            rec_warnings.push((vi, w));
                        }
                        if patches.is_empty() {
                            continue;
                        }
                        changed_by.push(vi);
                        for p in patches {
                            let mut occ = 0usize;
                            for (s, d) in subs.iter_mut() {
                                if *s == p.sig {
                                    if occ == p.occurrence {
                                        *d = p.new_bytes.clone();
                                        break;
                                    }
                                    occ += 1;
                                }
                            }
                            final_patches.insert((p.sig, p.occurrence), p.new_bytes);
                        }
                    }
                    if changed_by.is_empty() && rec_warnings.is_empty() {
                        return None;
                    }
                    Some(Stash {
                        fk,
                        edit: LaneEdit::Raw(final_patches),
                        changed_by,
                        warnings: rec_warnings,
                    })
                }
            }
        };

        if order.len() < PAR_THRESHOLD {
            order.iter().filter_map(decide).collect()
        } else {
            // Indexed par_iter + collect preserves input order.
            order.par_iter().filter_map(decide).collect()
        }
    };

    // ---- serial apply, enumeration order ----------------------------------
    let mut changed_counts = vec![0u32; active.len()];
    let mut visit_warnings: Vec<Vec<Sym>> = vec![Vec::new(); active.len()];
    let mut decoded_changed: Vec<Record> = Vec::new();
    let mut decoded_changed_index_by_form_key: FxHashMap<FormKey, usize> = FxHashMap::default();

    for stash in stashes {
        for &vi in &stash.changed_by {
            changed_counts[vi] += 1;
        }
        for (vi, w) in stash.warnings {
            visit_warnings[vi].extend(w);
        }
        match stash.edit {
            LaneEdit::Decoded(record) => {
                if !stash.changed_by.is_empty() {
                    if let Some(index) = decoded_changed_index_by_form_key
                        .get(&record.form_key)
                        .copied()
                    {
                        decoded_changed[index] = record;
                    } else {
                        decoded_changed_index_by_form_key
                            .insert(record.form_key, decoded_changed.len());
                        decoded_changed.push(record);
                    }
                }
            }
            LaneEdit::Raw(final_patches) => {
                let mut by_sig: FxHashMap<&'static str, Vec<(usize, Vec<u8>)>> =
                    FxHashMap::default();
                for ((sig, occurrence), bytes) in final_patches {
                    by_sig.entry(sig).or_default().push((occurrence, bytes));
                }
                for (sig, patches) in by_sig {
                    let mut occurrence = 0usize;
                    session
                        .patch_all_subrecords_bytes(&stash.fk, sig, |buf| {
                            let this = occurrence;
                            occurrence += 1;
                            if let Some((_, bytes)) = patches.iter().find(|(o, _)| *o == this) {
                                *buf = bytes.clone();
                                true
                            } else {
                                false
                            }
                        })
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                }
            }
        }
    }

    if !decoded_changed.is_empty() {
        let expected = decoded_changed.len();
        let replaced = session
            .replace_records_contents(decoded_changed, schema, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "sweep '{}' replaced {replaced} of {expected} records",
                sweep.label
            )));
        }
    }

    let elapsed = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let mut reports = Vec::with_capacity(active.len());
    for (vi, v) in active.iter().enumerate() {
        let mut r = FixupReport::empty();
        r.records_changed = changed_counts[vi];
        r.elapsed_ms = elapsed; // sweep-shared; per-visitor split is not separable
        r.warnings = std::mem::take(&mut gathers[vi].warnings);
        r.warnings.extend(std::mem::take(&mut visit_warnings[vi]));
        reports.push((v.name().to_string(), r));
    }
    Ok(reports)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::session::open_session;
    use crate::store2::test_util::handle_records;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        WriteEffect, insert_parsed_record_in_slot, plugin_handle_new_native,
        plugin_handle_store_ref,
    };
    use smallvec::SmallVec;
    use std::sync::Arc;

    fn ammo_record(eid: &str, local: u32, interner: &StringInterner, plugin: &str) -> Record {
        let eid_sym = interner.intern(eid);
        Record {
            sig: SigCode::from_str("AMMO").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(eid_sym),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("OBND").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_slice(&[0u8; 12])),
                },
            ],
            warnings: SmallVec::new(),
        }
    }

    struct Fixture {
        handle: u64,
        plugin: String,
        interner: StringInterner,
        mapper_state: MapperState,
        config: FixupConfig,
    }

    fn seed_fixture(plugin: &str, eids: &[(&str, u32)]) -> Fixture {
        let handle = plugin_handle_new_native(plugin, Some("fo4")).expect("handle");
        let interner = StringInterner::new();
        let mut session = open_session(handle, None).expect("session");
        let schema = session.schema().expect("schema");
        for (eid, local) in eids {
            let record = ammo_record(eid, *local, &interner, plugin);
            session
                .add_record(record, schema.as_ref(), &interner)
                .expect("add record");
        }
        drop(session);
        let mut config = FixupConfig::default();
        config.target_schema = Some(Arc::clone(&schema));
        Fixture {
            handle,
            plugin: plugin.to_string(),
            interner,
            mapper_state: MapperState::new(std::iter::empty(), MapperOptions::default()),
            config,
        }
    }

    fn run_test_sweep(fx: &mut Fixture, sweep: &Sweep) -> Vec<(String, FixupReport)> {
        let mut mapper = FormKeyMapper::from_state(&mut fx.mapper_state, &fx.interner);
        let mut session = open_session(fx.handle, None).expect("session");
        let mut master_cache = MasterScanCache::default();
        run_sweep(
            &mut session,
            &mut mapper,
            &fx.config,
            sweep,
            &mut master_cache,
        )
        .expect("sweep")
    }

    fn ammo_sigs() -> Vec<SigCode> {
        vec![SigCode::from_str("AMMO").unwrap()]
    }

    fn obnd_fill(record: &Record) -> Option<u8> {
        record.fields.iter().find_map(|e| {
            if e.sig.as_str() != "OBND" {
                return None;
            }
            match &e.value {
                FieldValue::Bytes(b) if !b.is_empty() && b.iter().all(|x| *x == b[0]) => Some(b[0]),
                _ => None,
            }
        })
    }

    fn set_obnd(record: &mut Record, fill: u8) {
        for e in record.fields.iter_mut() {
            if e.sig.as_str() == "OBND" {
                e.value = FieldValue::Bytes(SmallVec::from_slice(&[fill; 12]));
            }
        }
    }

    /// Rewrites OBND to `[fill; 12]` on records whose resolved EDID starts
    /// with `prefix` (optionally gated on the CURRENT in-memory OBND fill —
    /// the composition probe). Content-only mutation: flags/EDID untouched
    /// (the `replace_record_contents` guard rejects those).
    /// Carries pre-resolved EDIDs through the gather index (Sym → &str needs
    /// the interner, which visit_decoded doesn't receive).
    struct EidPrefixObndVisitor {
        name: &'static str,
        prefix: &'static str,
        fill: u8,
        require_fill: Option<u8>,
    }
    struct EidIndex {
        eids: FxHashMap<u32, String>,
    }
    impl RecordVisitor for EidPrefixObndVisitor {
        fn name(&self) -> &'static str {
            self.name
        }
        fn lane(&self) -> Lane {
            Lane::Decoded
        }
        fn gather(
            &self,
            session: &mut PluginSession,
            mapper: &FormKeyMapper,
            _config: &FixupConfig,
            _master_cache: &mut MasterScanCache,
        ) -> Result<GatherOutput, FixupError> {
            let sig = SigCode::from_str("AMMO").unwrap();
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let mut eids = FxHashMap::default();
            let schema = session
                .schema()
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                if let Ok(rec) = session.record_decoded(&fk, schema.as_ref(), mapper.interner) {
                    if let Some(eid) = rec.eid.and_then(|s| mapper.interner.resolve(s)) {
                        eids.insert(fk.local, eid.to_string());
                    }
                }
            }
            Ok(GatherOutput {
                candidate_sigs: vec![sig],
                index: Some(Box::new(EidIndex { eids })),
                warnings: Vec::new(),
            })
        }
        fn visit_decoded(
            &self,
            record: &mut Record,
            index: Option<&(dyn Any + Send + Sync)>,
            _cx: &SweepCtx<'_>,
            _warnings: &mut Vec<Sym>,
        ) -> VisitOutcome {
            if let Some(required) = self.require_fill {
                if obnd_fill(record) != Some(required) {
                    return VisitOutcome::Unchanged;
                }
            }
            let idx = index
                .and_then(|i| i.downcast_ref::<EidIndex>())
                .expect("eid index");
            let starts = idx
                .eids
                .get(&record.form_key.local)
                .is_some_and(|e| e.starts_with(self.prefix));
            if starts && obnd_fill(record) != Some(self.fill) {
                set_obnd(record, self.fill);
                VisitOutcome::Changed
            } else {
                VisitOutcome::Unchanged
            }
        }
    }

    fn parsed_obnd_by_id(handle: u64) -> FxHashMap<u32, Vec<u8>> {
        handle_records(handle)
            .iter()
            .map(|r| {
                let obnd = r
                    .subrecords
                    .iter()
                    .find(|s| s.signature.as_str() == "OBND")
                    .expect("fixture must encode OBND bytes")
                    .data
                    .to_vec();
                (r.form_id & 0x00FF_FFFF, obnd)
            })
            .collect()
    }

    #[test]
    fn decoded_sweep_changes_matching_records_and_reports_counts() {
        let mut fx = seed_fixture(
            "SweepDecoded.esp",
            &[("MarkOne", 0x801), ("Other", 0x802), ("MarkTwo", 0x803)],
        );
        let sweep = Sweep {
            label: "test",
            visitors: vec![Box::new(EidPrefixObndVisitor {
                name: "marker_obnd",
                prefix: "Mark",
                fill: 9,
                require_fill: None,
            })],
        };
        let reports = run_test_sweep(&mut fx, &sweep);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].0, "marker_obnd");
        assert_eq!(reports[0].1.records_changed, 2);

        let obnd = parsed_obnd_by_id(fx.handle);
        assert_eq!(obnd[&0x801], vec![9u8; 12]);
        assert_eq!(obnd[&0x802], vec![0u8; 12]);
        assert_eq!(obnd[&0x803], vec![9u8; 12]);
    }

    #[test]
    fn decoded_sweep_batches_duplicate_record_identity_once() {
        let mut fx = seed_fixture("SweepDuplicate.esp", &[("MarkOne", 0x801)]);
        let duplicate = handle_records(fx.handle)[0].clone();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&fx.handle).unwrap();
            insert_parsed_record_in_slot(slot, duplicate);
            slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
        }
        let sweep = Sweep {
            label: "duplicate",
            visitors: vec![Box::new(EidPrefixObndVisitor {
                name: "marker_obnd",
                prefix: "Mark",
                fill: 9,
                require_fill: None,
            })],
        };

        let reports = run_test_sweep(&mut fx, &sweep);

        assert_eq!(reports[0].1.records_changed, 2);
        assert!(handle_records(fx.handle).iter().any(|record| {
            record.subrecords.iter().any(|subrecord| {
                subrecord.signature.as_str() == "OBND" && subrecord.data.as_ref() == [9u8; 12]
            })
        }));
    }

    #[test]
    fn visitors_compose_in_order_on_the_same_in_memory_record() {
        let mut fx = seed_fixture("SweepCompose.esp", &[("MarkOne", 0x801), ("Other", 0x802)]);
        // Second visitor only acts when it can SEE the first visitor's
        // in-memory OBND mutation — proving composition order parity with the
        // legacy fixup-major sequence.
        let sweep = Sweep {
            label: "compose",
            visitors: vec![
                Box::new(EidPrefixObndVisitor {
                    name: "first",
                    prefix: "Mark",
                    fill: 1,
                    require_fill: None,
                }),
                Box::new(EidPrefixObndVisitor {
                    name: "second",
                    prefix: "Mark",
                    fill: 2,
                    require_fill: Some(1),
                }),
            ],
        };
        let reports = run_test_sweep(&mut fx, &sweep);
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].1.records_changed, 1);
        assert_eq!(
            reports[1].1.records_changed, 1,
            "second visitor must see first's in-memory mutation"
        );

        let obnd = parsed_obnd_by_id(fx.handle);
        assert_eq!(
            obnd[&0x801],
            vec![2u8; 12],
            "final state is second visitor's fill"
        );
        assert_eq!(obnd[&0x802], vec![0u8; 12]);
    }

    #[test]
    fn parallel_path_matches_expectations_over_threshold() {
        let eids: Vec<(String, u32)> = (0..200u32)
            .map(|i| (format!("Mark{i:03}"), 0x801 + i))
            .collect();
        let eid_refs: Vec<(&str, u32)> = eids.iter().map(|(e, l)| (e.as_str(), *l)).collect();
        let mut fx = seed_fixture("SweepPar.esp", &eid_refs);
        let sweep = Sweep {
            label: "par",
            visitors: vec![Box::new(EidPrefixObndVisitor {
                name: "marker_obnd",
                prefix: "Mark",
                fill: 9,
                require_fill: None,
            })],
        };
        let reports = run_test_sweep(&mut fx, &sweep);
        assert_eq!(reports[0].1.records_changed, 200);
        let obnd = parsed_obnd_by_id(fx.handle);
        assert_eq!(obnd.len(), 200);
        assert!(obnd.values().all(|b| *b == vec![9u8; 12]));
    }

    /// Raw-lane visitor: patches OBND occurrence 0 to a fixed pattern.
    struct ObndPatchVisitor;
    impl RecordVisitor for ObndPatchVisitor {
        fn name(&self) -> &'static str {
            "obnd_patch"
        }
        fn lane(&self) -> Lane {
            Lane::RawBytes
        }
        fn gather(
            &self,
            _session: &mut PluginSession,
            _mapper: &FormKeyMapper,
            _config: &FixupConfig,
            _master_cache: &mut MasterScanCache,
        ) -> Result<GatherOutput, FixupError> {
            Ok(GatherOutput::sigs_only(ammo_sigs()))
        }
        fn visit_raw(
            &self,
            subrecords: &[(&str, &[u8])],
            _index: Option<&(dyn Any + Send + Sync)>,
            _cx: &SweepCtx<'_>,
            _warnings: &mut Vec<Sym>,
        ) -> Vec<SubrecordPatch> {
            for (sig, data) in subrecords {
                if *sig == "OBND" && data.iter().all(|b| *b == 0) {
                    return vec![SubrecordPatch {
                        sig: "OBND",
                        occurrence: 0,
                        new_bytes: vec![7u8; data.len()],
                    }];
                }
            }
            Vec::new()
        }
    }

    #[test]
    fn raw_sweep_patches_bytes_in_place_without_reencode() {
        let mut fx = seed_fixture("SweepRaw.esp", &[("RawOne", 0x801)]);
        // Capture the seeded parsed state to prove only OBND bytes change.
        let before = handle_records(fx.handle);
        let obnd_before: Vec<u8> = before[0]
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == "OBND")
            .expect("fixture must encode OBND bytes")
            .data
            .to_vec();
        assert!(obnd_before.iter().all(|b| *b == 0));

        let sweep = Sweep {
            label: "raw",
            visitors: vec![Box::new(ObndPatchVisitor)],
        };
        let reports = run_test_sweep(&mut fx, &sweep);
        assert_eq!(reports[0].1.records_changed, 1);

        let after = handle_records(fx.handle);
        let obnd_after: Vec<u8> = after[0]
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == "OBND")
            .unwrap()
            .data
            .to_vec();
        assert_eq!(obnd_after, vec![7u8; obnd_before.len()]);
        // Every other subrecord byte-identical (no re-encode).
        for (b, a) in before[0].subrecords.iter().zip(after[0].subrecords.iter()) {
            if b.signature.as_str() != "OBND" {
                assert_eq!(b.data.as_ref(), a.data.as_ref());
            }
        }
    }

    struct NeverAppliesVisitor;
    impl RecordVisitor for NeverAppliesVisitor {
        fn name(&self) -> &'static str {
            "never_applies"
        }
        fn lane(&self) -> Lane {
            Lane::Decoded
        }
        fn applies(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
            false
        }
        fn gather(
            &self,
            _session: &mut PluginSession,
            _mapper: &FormKeyMapper,
            _config: &FixupConfig,
            _master_cache: &mut MasterScanCache,
        ) -> Result<GatherOutput, FixupError> {
            panic!("gather must not run for non-applying visitor");
        }
    }

    #[test]
    fn non_applying_visitor_emits_no_report() {
        let mut fx = seed_fixture("SweepSkip.esp", &[("MarkOne", 0x801)]);
        let sweep = Sweep {
            label: "skip",
            visitors: vec![Box::new(NeverAppliesVisitor)],
        };
        let reports = run_test_sweep(&mut fx, &sweep);
        assert!(reports.is_empty());
    }
}
