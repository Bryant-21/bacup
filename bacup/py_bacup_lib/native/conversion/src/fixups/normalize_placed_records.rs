//! Post-copy normalization for placed records copied verbatim into cell slices.
//!
//! # Why this exists
//! The cell-bounded conversion copies placed children (REFR/ACHR/PGRE/PHZD/PGRD)
//! directly from the FO76 source handle into target cells
//! (`esp_authoring_core::cell_slice::plugin_handle_copy_cell_slice_children`),
//! applying only FormID remap + position offset. That path BYPASSES the
//! translate + apply_fixups pipeline, so the copied placed records keep FO76
//! values that crash FO4 on cell load:
//!   - out-of-domain enums (XLCM "$7000001") and unmasked flag bits (XRDO),
//!   - dangling / wild FormID pointers in placed-record fields.
//!
//! This pass runs AFTER the copy, over the placed records now present in the
//! TARGET handle, reusing existing normalization logic (no duplication):
//!   1. `class_a_normalize::normalize_flags_and_enums` — flag mask + enum clamp
//!      (clears XLCM out-of-domain enum, XRDO flag bits). Already per-record.
//!   2. `rewrite_placed_ref_location_record` — XLCN/XCZC/XLRT normalization. The
//!      XEZN→LCTN pointer is NO LONGER stripped here: the encounter-zone
//!      synthesis pass (which runs after this one) repoints placed XEZN→LCTN to
//!      the LCTN's synthesized ECZN, so XEZN must survive this pass intact. Any
//!      XEZN the shared rewrite would strip is captured and restored in place.
//!   3. Raw zero-master local refs in placed-only subrecords (XLKR/XAPR/etc.)
//!      are rebound to the output plugin when that object-id exists there. This
//!      catches paths that copied FO76-local `00xxxxxx` payloads after the normal
//!      mapper pass.
//!   4. (keystone-gated) wild-pointer null/strip for placed FormID fields via
//!      the schema struct-field layout + metadata-driven action — not wired
//!      here yet.

use bytes::Bytes;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec};

use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, WriteEffect, effective_subrecords_for_record,
    normalize_fo76_refr_lod_header_flags,
};

use crate::fixups::ref_index::validate_struct_fk_fields;
use crate::fixups::rewrite_raw_object_template_formids::{
    encode_target_form_id, rewrite_placed_ref_location_record,
    target_record_sigs_by_encoded_form_id,
};
use crate::fixups::{FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::{PluginSession, open_session};
use crate::sym::StringInterner;
use crate::translator::class_a_normalize::normalize_flags_and_enums;
use crate::translator::pair_hooks::fo76_fo4::namespace_fo76_radio_frequency;

/// Outcome counters for the placed-record normalization pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlacedNormalizeReport {
    pub records_seen: u32,
    pub records_changed: u32,
    pub xezn_stripped: u32,
    pub class_a_changed: u32,
    pub loc_ref_types_stripped: u32,
    /// Struct-internal wild/wrong-type FK slots nulled.
    pub wild_pointers_nulled: u32,
    /// XLKR linked-ref subrecords dropped because their keyword/ref is genuinely
    /// absent from the fully-assembled target (deferred from the cell-slice copy).
    pub dangling_xlkr_dropped: u32,
    /// Malformed zero-length XOWN owner subrecords stripped from placed records.
    pub empty_xown_stripped: u32,
    /// FO4 ownership payloads defaulted from owner-only XOWN to owner + No Crime.
    pub xown_no_crime_defaulted: u32,
}

const RECORD_FLAG_LOD_RESPECTS_ENABLE_STATE: u32 = 0x0000_0100;
const XALG_LOD_HEADER_MASK: u64 = 0x0000_0208;

pub fn normalize_placed_lod_header_flags(
    session: &mut PluginSession,
    mapper: &FormKeyMapper<'_>,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let Some(source_id) = session.source_id() else {
        return Ok(report);
    };
    let refr = SigCode::from_str("REFR").map_err(|e| FixupError::SchemaError(e.to_string()))?;

    let candidates = {
        use rayon::prelude::*;

        let source_scan = session
            .handle_raw_scan(source_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        source_scan
            .raw_form_ids_of_sig(refr)
            .par_iter()
            .filter_map(|raw_form_id| {
                if *raw_form_id == 0 || *raw_form_id >> 24 != 0 {
                    return None;
                }
                source_scan
                    .with_record(*raw_form_id, |record| {
                        let xalg_flags = raw_xalg_flags(record);
                        (record.flags & RECORD_FLAG_LOD_RESPECTS_ENABLE_STATE != 0
                            || xalg_flags & XALG_LOD_HEADER_MASK != 0)
                            .then_some((record.form_id & 0x00FF_FFFF, xalg_flags))
                    })
                    .flatten()
            })
            .collect::<Vec<_>>()
    };
    if candidates.is_empty() {
        return Ok(report);
    }

    let source_plugin_name = session
        .source_slot_opt()
        .map(|slot| slot.parsed.plugin_name.clone())
        .unwrap_or_default();
    let source_plugin = mapper.interner.intern(&source_plugin_name);
    let target_masters = session.target_masters().to_vec();
    let own_load_index = target_masters.len() as u32;
    if own_load_index > u8::MAX as u32 {
        return Ok(report);
    }
    let own_prefix = own_load_index << 24;
    let mapped_targets: FxHashMap<u32, u32> = mapper
        .source_to_target_iter()
        .filter(|(source, _)| source.plugin == source_plugin)
        .filter_map(|(source, target)| {
            encode_target_form_id(target, mapper.interner, target_masters.as_slice())
                .map(|raw| (source.local & 0x00FF_FFFF, raw))
        })
        .collect();

    let mut touched = SmallVec::<[u32; 4]>::new();
    for (source_local, xalg_flags) in candidates {
        let target_raw = match mapped_targets.get(&source_local).copied() {
            Some(raw) if raw >> 24 == own_load_index => raw,
            Some(_) => continue,
            None => own_prefix | source_local,
        };
        let Ok(record) = session.record_mut(target_raw) else {
            continue;
        };
        if record.signature.as_str() != "REFR" {
            continue;
        }
        let normalized = normalize_fo76_refr_lod_header_flags(record.flags, xalg_flags);
        if normalized == record.flags {
            continue;
        }
        record.flags = normalized;
        record.raw_payload = None;
        touched.push(target_raw);
        report.records_changed = report.records_changed.saturating_add(1);
    }

    if !touched.is_empty() {
        session.record_effect(WriteEffect::RecordContents { form_ids: touched });
    }
    Ok(report)
}

fn raw_xalg_flags(record: &ParsedRecord) -> u64 {
    effective_subrecords_for_record(record)
        .iter()
        .filter(|subrecord| subrecord.signature.as_str() == "XALG")
        .fold(0u64, |flags, subrecord| {
            let mut bytes = [0u8; 8];
            let len = subrecord.data.len().min(bytes.len());
            bytes[..len].copy_from_slice(&subrecord.data[..len]);
            flags | u64::from_le_bytes(bytes)
        })
}

/// Normalize every placed-signature record currently in the target plugin
/// handle. Idempotent: re-running on already-normalized records is a no-op.
///
/// `placed_signatures` is the set of placed child sigs to walk (e.g.
/// `["REFR","ACHR","PGRE","PHZD","PGRD"]`); sigs lacking flag/enum/XEZN fields
/// are harmless no-ops. `class_a_normalize` / the XEZN strip already self-gate.
pub fn normalize_copied_placed_records(
    target_handle_id: u64,
    placed_signatures: &[String],
) -> Result<PlacedNormalizeReport, FixupError> {
    let interner = StringInterner::new();
    let mut session =
        open_session(target_handle_id, None).map_err(|e| FixupError::HandleError(e.to_string()))?;

    let schema = session
        .schema()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let target_masters = session.target_masters().to_vec();
    let output_own_prefix = (target_masters.len() as u32) << 24;

    // Encoded target FK → sig index, for the XEZN→ECZN type check. The placed
    // records are already remapped, so we resolve against the target plugin's
    // own records + masters.
    let target_sigs_by_encoded =
        target_record_sigs_by_encoded_form_id(&mut session, &interner, &target_masters)?;
    let forced_loc_ref_type_bases =
        collect_forced_loc_ref_type_bases(&session.target_slot().parsed.root_items);

    // Post-copy: refs are already target-encoded, so there is no source remap.
    let empty_encoded_targets: FxHashMap<u32, u32> = FxHashMap::default();
    // Master validity is permissive here — re-validating/nulling dangling target
    // masters is the wild-pointer step (Phase 2, keystone-gated), not this pass.
    let mut always_valid = |_raw: u32| true;

    let mut report = PlacedNormalizeReport::default();

    // Accumulate normalized records and apply them with the BATCH content-replace
    // (one GRUP-tree traversal per chunk) instead of a per-record `replace_record_contents`,
    // which re-scans the whole tree each call (O(n²) on a multi-million-record output).
    // Chunked to bound peak RAM. Content-replace (single or batch) mutates in place,
    // preserving each record's nested position under its cell Persistent/Temporary group
    // (a structural `replace_record` would relocate it to a top-level signature group).
    let mut pending: Vec<crate::record::Record> = Vec::new();
    const FLUSH_CHUNK: usize = 4096;

    for sig_str in placed_signatures {
        let Ok(sig) = SigCode::from_str(sig_str) else {
            continue;
        };
        let fks = session
            .form_keys_of_sig(sig, &interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in fks {
            let raw_form_id = output_own_prefix | (fk.local & 0x00FF_FFFF);
            let raw_normalized = {
                let raw_record = match session.record_mut(raw_form_id) {
                    Ok(record) => record,
                    Err(_) => continue,
                };
                let xprm_relaid_out = relayout_short_fo76_xprm_subrecords(raw_record);
                let radio_namespaced = namespace_fo76_radio_transmitter_frequency(raw_record);
                let remapped = remap_placed_raw_local_ref_subrecords(
                    raw_record,
                    output_own_prefix,
                    &target_sigs_by_encoded,
                );
                let ownership_xown = normalize_ownership_xown_subrecords(raw_record);
                let stripped = strip_xlrt_when_base_has_forced_loc_ref_type(
                    raw_record,
                    &forced_loc_ref_type_bases,
                );
                report.empty_xown_stripped += ownership_xown.empty_stripped;
                report.xown_no_crime_defaulted += ownership_xown.no_crime_defaulted;
                report.loc_ref_types_stripped += stripped;
                // Runs AFTER remap so valid 0x00-master XLKR refs are already
                // rebound; only genuinely-dangling own refs remain to drop.
                let xlkr_dropped = drop_dangling_xlkr_subrecords(
                    raw_record,
                    output_own_prefix,
                    &target_sigs_by_encoded,
                );
                report.dangling_xlkr_dropped += xlkr_dropped;
                xprm_relaid_out
                    + radio_namespaced
                    + remapped
                    + ownership_xown.changed()
                    + stripped
                    + xlkr_dropped
            };
            let raw_changed = raw_normalized > 0;
            if raw_changed {
                session.record_effect(WriteEffect::RecordContents {
                    form_ids: smallvec![raw_form_id],
                });
            }

            let mut record = match session.record_decoded(&fk, schema.as_ref(), &interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
            report.records_seen += 1;

            // 1. Flag mask + enum clamp (XLCM out-of-domain enum, XRDO flags).
            let class_a = normalize_flags_and_enums(&mut record, schema.as_ref(), &interner);
            let class_a_changed = !class_a.decisions.is_empty();

            // 2. XLCN/XCZC/XLRT normalization on placed refs. XEZN is NO LONGER
            // stripped here: the encounter-zone synthesis pass repoints placed
            // XEZN→LCTN to the LCTN's synthesized ECZN (and strips only the
            // residual non-ECZN targets). It runs AFTER this normalize, so the
            // XEZN must survive intact. `rewrite_placed_ref_location_record` would
            // otherwise strip a wrong-type XEZN, so capture it (with its original
            // field position) and restore it in place if that call removed it (the
            // call never rewrites XEZN here — `encoded_targets` is empty — it only
            // validates/strips).
            let saved_xezn = record
                .fields
                .iter()
                .position(|f| f.sig.0 == *b"XEZN")
                .map(|idx| (idx, record.fields[idx].clone()));
            let location_changed = rewrite_placed_ref_location_record(
                &mut record,
                &empty_encoded_targets,
                &target_sigs_by_encoded,
                &mut always_valid,
            );
            if let Some((idx, xezn)) = saved_xezn {
                if !record.fields.iter().any(|f| f.sig.0 == *b"XEZN") {
                    record.fields.insert(idx.min(record.fields.len()), xezn);
                }
            }
            let xezn_changed = location_changed;

            // 3. Wild/wrong-type struct-internal FK null — placed
            //    records carry struct-codec FK fields (e.g. linked-ref data)
            //    whose FO76 targets may be wrong-typed in FO4. Null them where
            //    NULL is allowed so the engine doesn't dereference a wild ptr.
            // form_version unions are rare on placed-record struct FK fields;
            // None selects the legacy/first variant (correct for these).
            let struct_fk = validate_struct_fk_fields(&mut record, schema.as_ref(), None, &|raw| {
                target_sigs_by_encoded.get(&raw).copied()
            });

            if class_a_changed {
                report.class_a_changed += 1;
            }
            if xezn_changed {
                report.xezn_stripped += 1;
            }
            report.wild_pointers_nulled += struct_fk.nulled;
            let decoded_changed = class_a_changed || xezn_changed || struct_fk.changed();
            if raw_changed || decoded_changed {
                report.records_changed += 1;
            }
            if decoded_changed {
                pending.push(record);
                if pending.len() >= FLUSH_CHUNK {
                    let chunk = std::mem::take(&mut pending);
                    session
                        .replace_records_contents(chunk, schema.as_ref(), &interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                }
            }
        }
    }

    if !pending.is_empty() {
        session
            .replace_records_contents(pending, schema.as_ref(), &interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
    }

    Ok(report)
}

fn namespace_fo76_radio_transmitter_frequency(record: &mut ParsedRecord) -> u32 {
    if record.signature.as_str() != "REFR" {
        return 0;
    }
    let mut changed = 0;
    for subrecord in &mut record.subrecords {
        if subrecord.signature.as_str() != "XRDO" || subrecord.data.len() < 4 {
            continue;
        }
        let mut data = subrecord.data.to_vec();
        let mut frequency =
            f32::from_le_bytes(data[..4].try_into().expect("XRDO frequency prefix"));
        if namespace_fo76_radio_frequency(&mut frequency) {
            data[..4].copy_from_slice(&frequency.to_le_bytes());
            subrecord.data = Bytes::from(data);
            changed += 1;
        }
    }
    if changed > 0 {
        record.raw_payload = None;
    }
    changed
}

fn relayout_short_fo76_xprm_subrecords(record: &mut ParsedRecord) -> u32 {
    if record.signature.as_str() != "REFR" {
        return 0;
    }

    let mut changed = 0;
    for subrecord in record
        .subrecords
        .iter_mut()
        .filter(|subrecord| subrecord.signature.as_str() == "XPRM")
    {
        let raw = subrecord.data.as_ref();
        if raw.len() != 16 {
            continue;
        }

        let mut relaid_out = Vec::with_capacity(32);
        relaid_out.extend_from_slice(&raw[..12]);
        for _ in 0..4 {
            relaid_out.extend_from_slice(&1.0_f32.to_le_bytes());
        }
        relaid_out.extend_from_slice(&raw[12..16]);
        subrecord.data = Bytes::from(relaid_out);
        changed += 1;
    }

    if changed > 0 {
        record.raw_payload = None;
    }
    changed
}

fn collect_forced_loc_ref_type_bases(items: &[ParsedItem]) -> FxHashSet<u32> {
    let mut out = FxHashSet::default();
    collect_forced_loc_ref_type_bases_in_items(items, &mut out);
    out
}

fn collect_forced_loc_ref_type_bases_in_items(items: &[ParsedItem], out: &mut FxHashSet<u32>) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                let subrecords = effective_subrecords_for_record(record);
                if subrecords
                    .iter()
                    .any(|subrecord| subrecord.signature.as_str() == "FTYP")
                {
                    out.insert(record.form_id);
                }
            }
            ParsedItem::Group(group) => {
                collect_forced_loc_ref_type_bases_in_items(&group.children, out);
            }
        }
    }
}

fn strip_xlrt_when_base_has_forced_loc_ref_type(
    record: &mut ParsedRecord,
    forced_loc_ref_type_bases: &FxHashSet<u32>,
) -> u32 {
    if !matches!(record.signature.as_str(), "ACHR" | "REFR" | "PGRE" | "PHZD") {
        return 0;
    }
    let Some(base) = raw_subrecord_form_id(record, "NAME") else {
        return 0;
    };
    if !forced_loc_ref_type_bases.contains(&base) {
        return 0;
    }
    let before = record.subrecords.len();
    record
        .subrecords
        .retain(|subrecord| subrecord.signature.as_str() != "XLRT");
    let removed = before.saturating_sub(record.subrecords.len()) as u32;
    if removed > 0 {
        record.raw_payload = None;
    }
    removed
}

/// Record signatures whose copied ownership payload must match FO4's
/// owner-formid + No-Crime byte layout.
pub(crate) const FO4_OWNERSHIP_XOWN_RECORD_SIGNATURES: &[&str] =
    &["CELL", "REFR", "ACHR", "PGRE", "PHZD"];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OwnershipXownNormalizeReport {
    pub records_changed: u32,
    pub empty_stripped: u32,
    pub no_crime_defaulted: u32,
    pub xprm_relaid_out: u32,
}

impl OwnershipXownNormalizeReport {
    pub(crate) fn changed(self) -> u32 {
        self.empty_stripped
            .saturating_add(self.no_crime_defaulted)
            .saturating_add(self.xprm_relaid_out)
    }

    fn merge(&mut self, other: Self) {
        self.records_changed = self.records_changed.saturating_add(other.records_changed);
        self.empty_stripped = self.empty_stripped.saturating_add(other.empty_stripped);
        self.no_crime_defaulted = self
            .no_crime_defaulted
            .saturating_add(other.no_crime_defaulted);
        self.xprm_relaid_out = self.xprm_relaid_out.saturating_add(other.xprm_relaid_out);
    }
}

/// Normalize raw placed payloads after every production copy path has populated
/// the target. This shares the existing ownership walk so projected/exterior
/// records do not require a second full-plugin traversal.
pub(crate) fn normalize_ownership_xown_payloads_in_session(
    session: &mut PluginSession,
) -> OwnershipXownNormalizeReport {
    let mut report = OwnershipXownNormalizeReport::default();
    let mut touched = SmallVec::<[u32; 4]>::new();
    normalize_ownership_xown_payloads_in_items(
        &mut session.target_slot_mut().parsed.root_items,
        &mut report,
        &mut touched,
    );
    if !touched.is_empty() {
        session.record_effect(WriteEffect::RecordContents { form_ids: touched });
    }
    report
}

fn normalize_ownership_xown_payloads_in_items(
    items: &mut [ParsedItem],
    report: &mut OwnershipXownNormalizeReport,
    touched: &mut SmallVec<[u32; 4]>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                let mut delta = normalize_ownership_xown_subrecords(record);
                delta.xprm_relaid_out = relayout_short_fo76_xprm_subrecords(record);
                delta.records_changed = u32::from(delta.changed() > 0);
                if delta.changed() > 0 {
                    touched.push(record.form_id);
                    report.merge(delta);
                }
            }
            ParsedItem::Group(group) => {
                normalize_ownership_xown_payloads_in_items(&mut group.children, report, touched);
            }
        }
    }
}

fn normalize_ownership_xown_subrecords(record: &mut ParsedRecord) -> OwnershipXownNormalizeReport {
    if !FO4_OWNERSHIP_XOWN_RECORD_SIGNATURES.contains(&record.signature.as_str()) {
        return OwnershipXownNormalizeReport::default();
    }

    let before = record.subrecords.len();
    record
        .subrecords
        .retain(|subrecord| subrecord.signature.as_str() != "XOWN" || !subrecord.data.is_empty());
    let empty_stripped = before.saturating_sub(record.subrecords.len()) as u32;

    let mut no_crime_defaulted = 0u32;
    for subrecord in record
        .subrecords
        .iter_mut()
        .filter(|subrecord| subrecord.signature.as_str() == "XOWN")
    {
        if subrecord.data.len() == 4 {
            let mut data = subrecord.data.to_vec();
            data.push(0);
            subrecord.data = Bytes::from(data);
            no_crime_defaulted = no_crime_defaulted.saturating_add(1);
        }
    }

    let report = OwnershipXownNormalizeReport {
        records_changed: u32::from(empty_stripped > 0 || no_crime_defaulted > 0),
        empty_stripped,
        no_crime_defaulted,
        xprm_relaid_out: 0,
    };
    if report.changed() > 0 {
        record.raw_payload = None;
    }
    report
}

fn raw_subrecord_form_id(record: &ParsedRecord, sig: &str) -> Option<u32> {
    record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == sig)
        .and_then(|subrecord| {
            let data = subrecord.data.as_ref();
            let bytes = data.get(0..4)?;
            Some(u32::from_le_bytes(bytes.try_into().ok()?))
        })
}

/// Drop XLKR (linked-reference) subrecords whose own-plugin keyword or linked
/// ref is absent from the fully-assembled target.
///
/// The cell-slice copy intentionally does NOT drop XLKR (its per-pass
/// `target_existing_form_ids` snapshot can't see refs copied in another pass, so
/// dropping there strands actor packages whose location is a linked reference —
/// PLDT type 6 — null-derefing FO4's `BGSVisitProceduresInitActorLocation`).
/// This pass runs over the complete target, so a still-own-prefix ref missing
/// here is genuinely dangling and safe to remove.
fn drop_dangling_xlkr_subrecords(
    record: &mut ParsedRecord,
    output_own_prefix: u32,
    target_sigs_by_encoded: &FxHashMap<u32, SigCode>,
) -> u32 {
    if record.subrecords.is_empty() {
        record.subrecords = effective_subrecords_for_record(record).into_owned();
    }
    let before = record.subrecords.len();
    record.subrecords.retain(|subrecord| {
        subrecord.signature.as_str() != "XLKR"
            || !xlkr_has_dangling_own_ref(
                subrecord.data.as_ref(),
                output_own_prefix,
                target_sigs_by_encoded,
            )
    });
    let removed = before.saturating_sub(record.subrecords.len()) as u32;
    if removed > 0 {
        record.raw_payload = None;
    }
    removed
}

/// XLKR is `struct:I(keyword),I(ref)`. The link is dangling when either own-plugin
/// slot points at a form id that no target record provides. Master-prefixed and
/// zero slots are left to the engine.
fn xlkr_has_dangling_own_ref(
    data: &[u8],
    output_own_prefix: u32,
    target_sigs_by_encoded: &FxHashMap<u32, SigCode>,
) -> bool {
    [0usize, 4]
        .iter()
        .any(|offset| match read_u32_at(data, *offset) {
            Some(raw) => {
                raw != 0
                    && (raw & 0xFF00_0000) == output_own_prefix
                    && !target_sigs_by_encoded.contains_key(&raw)
            }
            None => false,
        })
}

fn read_u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn remap_placed_raw_local_ref_subrecords(
    record: &mut ParsedRecord,
    output_own_prefix: u32,
    target_sigs_by_encoded: &FxHashMap<u32, SigCode>,
) -> u32 {
    let mut remapped = 0;
    for subrecord in record.subrecords.iter_mut() {
        let sig = subrecord.signature.as_str();
        let mut data = subrecord.data.to_vec();
        let mut subrecord_remapped = 0;
        if sig == "XLRT" {
            for offset in (0..data.len()).step_by(4) {
                if remap_raw_zero_master_ref(
                    &mut data,
                    offset,
                    output_own_prefix,
                    target_sigs_by_encoded,
                    &[],
                ) {
                    subrecord_remapped += 1;
                }
            }
        } else if sig == "XLKR" {
            if remap_raw_zero_master_ref(
                &mut data,
                0,
                output_own_prefix,
                target_sigs_by_encoded,
                LINKED_KEYWORD_OR_REF_TARGETS,
            ) {
                subrecord_remapped += 1;
            }
            if remap_raw_zero_master_ref(
                &mut data,
                4,
                output_own_prefix,
                target_sigs_by_encoded,
                PLACED_REF_TARGETS,
            ) {
                subrecord_remapped += 1;
            }
        } else if sig == "XLOC" {
            if remap_raw_zero_master_ref(
                &mut data,
                4,
                output_own_prefix,
                target_sigs_by_encoded,
                &["KEYM"],
            ) {
                subrecord_remapped += 1;
            }
        } else if sig == "XPLK" {
            if remap_raw_zero_master_ref(
                &mut data,
                0,
                output_own_prefix,
                target_sigs_by_encoded,
                &["REFR", "ACHR"],
            ) {
                subrecord_remapped += 1;
            }
        } else if matches!(sig, "XESP" | "XAPR") {
            if remap_raw_zero_master_ref(
                &mut data,
                0,
                output_own_prefix,
                target_sigs_by_encoded,
                PLACED_REF_TARGETS,
            ) {
                subrecord_remapped += 1;
            }
        } else {
            for offset in placed_local_ref_offsets(sig) {
                if remap_raw_zero_master_ref(
                    &mut data,
                    *offset,
                    output_own_prefix,
                    target_sigs_by_encoded,
                    &[],
                ) {
                    subrecord_remapped += 1;
                }
            }
        }
        if subrecord_remapped > 0 {
            subrecord.data = Bytes::from(data);
            remapped += subrecord_remapped;
        }
    }
    if remapped > 0 {
        record.raw_payload = None;
    }
    remapped
}

fn placed_local_ref_offsets(sig: &str) -> &'static [usize] {
    match sig {
        "XTEL" => &[0, 32],
        "XMSP" | "XLYR" | "XCZC" | "XLCN" | "XEZN" | "XLRL" | "XRFG" | "XOWN" | "XPWR" | "XEMI"
        | "XATR" | "XLIB" | "XNDP" | "XTNM" => &[0],
        _ => &[],
    }
}

const PLACED_REF_TARGETS: &[&str] = &[
    "PLYR", "ACHR", "REFR", "PGRE", "PHZD", "PMIS", "PARW", "PBAR", "PBEA", "PCON", "PFLA",
];

const LINKED_KEYWORD_OR_REF_TARGETS: &[&str] = &[
    "KYWD", "PLYR", "ACHR", "REFR", "PGRE", "PHZD", "PMIS", "PARW", "PBAR", "PBEA", "PCON", "PFLA",
];

fn remap_raw_zero_master_ref(
    bytes: &mut [u8],
    offset: usize,
    output_own_prefix: u32,
    target_sigs_by_encoded: &FxHashMap<u32, SigCode>,
    allowed_target_sigs: &[&str],
) -> bool {
    let Some(slot) = bytes.get_mut(offset..offset.saturating_add(4)) else {
        return false;
    };
    let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
    if raw == 0 || raw >> 24 != 0 {
        return false;
    }
    let output_raw = output_own_prefix | (raw & 0x00FF_FFFF);
    let Some(target_sig) = target_sigs_by_encoded.get(&output_raw) else {
        return false;
    };
    if output_raw == raw
        || (!allowed_target_sigs.is_empty() && !allowed_target_sigs.contains(&target_sig.as_str()))
    {
        return false;
    }
    slot.copy_from_slice(&output_raw.to_le_bytes());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedSubrecord, plugin_handle_close_native, plugin_handle_new_native,
        plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::FormKey;

    fn subrecord(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn record_with_sig(signature: &str, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new(signature),
            form_id: 0x0715781F,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        }
    }

    fn record(subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        record_with_sig("REFR", subrecords)
    }

    #[test]
    fn post_copy_lod_normalizer_repairs_every_fo76_refr_flag_combination() {
        let cases = [
            (0x0010_0000, 0x0000_0500, 0x0000_2000_u64, 0x0000_0400),
            (0x0010_0001, 0x0000_0100, 0x0000_2001_u64, 0),
            (0x0010_0002, 0x0000_0100, 0x0000_2200_u64, 0x0000_8100),
            (0x0010_0003, 0x0001_0100, 0, 0x0001_0100),
        ];
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native("Converted.esm", Some("fo4")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_items = cases
                .iter()
                .map(|(form_id, flags, xalg_flags, _)| {
                    let subrecords = (*xalg_flags != 0)
                        .then(|| subrecord("XALG", xalg_flags.to_le_bytes().to_vec()))
                        .into_iter()
                        .collect();
                    let mut record = record_with_sig("REFR", subrecords);
                    record.form_id = *form_id;
                    record.flags = *flags;
                    ParsedItem::Record(record)
                })
                .collect();
            store.get_mut(&source).unwrap().parsed.root_items = source_items;

            let target_items = cases
                .iter()
                .map(|(form_id, flags, _, _)| {
                    let mut record = record_with_sig("REFR", Vec::new());
                    record.form_id = *form_id;
                    record.flags = *flags;
                    ParsedItem::Record(record)
                })
                .collect();
            store.get_mut(&target).unwrap().parsed.root_items = target_items;
        }

        let interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let target_plugin = interner.intern("Converted.esm");
        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        for (form_id, _, _, _) in cases {
            state.source_to_target.insert(
                FormKey {
                    local: form_id,
                    plugin: source_plugin,
                },
                FormKey {
                    local: form_id,
                    plugin: target_plugin,
                },
            );
        }
        let mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut session = open_session(target, Some(source)).unwrap();

        let report = normalize_placed_lod_header_flags(&mut session, &mapper).unwrap();

        assert_eq!(report.records_changed, 3);
        assert_eq!(
            normalize_placed_lod_header_flags(&mut session, &mapper)
                .unwrap()
                .records_changed,
            0
        );
        for (form_id, _, _, expected) in cases {
            assert_eq!(session.record_mut(form_id).unwrap().flags, expected);
        }
        drop(session);
        assert!(plugin_handle_close_native(source));
        assert!(plugin_handle_close_native(target));
    }

    #[test]
    fn namespaces_fo76_radio_transmitter_frequency_once() {
        let mut transmitter = vec![0_u8; 16];
        transmitter[..4].copy_from_slice(&98.2_f32.to_le_bytes());
        let mut placed = record(vec![subrecord("XRDO", transmitter)]);

        assert_eq!(namespace_fo76_radio_transmitter_frequency(&mut placed), 1);
        let once = placed.subrecords[0].data.clone();
        assert_eq!(namespace_fo76_radio_transmitter_frequency(&mut placed), 0);

        assert_eq!(placed.subrecords[0].data, once);
        assert!(placed.raw_payload.is_none());
        assert_eq!(
            f32::from_le_bytes(placed.subrecords[0].data[..4].try_into().unwrap()),
            98.2 + crate::translator::pair_hooks::fo76_fo4::FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET
        );
    }

    #[test]
    fn relayouts_live_short_fo76_xprm_shape_for_fo4() {
        let short = hex::decode("00008045000080450000004501000000").unwrap();
        let mut placed = record(vec![subrecord("XPRM", short)]);

        let changed = relayout_short_fo76_xprm_subrecords(&mut placed);

        assert_eq!(changed, 1);
        assert!(placed.raw_payload.is_none());
        let xprm = placed.subrecords[0].data.as_ref();
        assert_eq!(xprm.len(), 32);
        assert_eq!(
            &xprm[..12],
            &hex::decode("000080450000804500000045").unwrap()
        );
        assert_eq!(&xprm[12..28], &[0, 0, 128, 63].repeat(4));
        assert_eq!(&xprm[28..32], &1_u32.to_le_bytes());
    }

    #[test]
    fn short_fo76_xprm_relayout_is_idempotent() {
        let mut placed = record(vec![subrecord("XPRM", vec![0; 16])]);

        assert_eq!(relayout_short_fo76_xprm_subrecords(&mut placed), 1);
        let once = placed.subrecords[0].data.clone();
        assert_eq!(relayout_short_fo76_xprm_subrecords(&mut placed), 0);
        assert_eq!(placed.subrecords[0].data, once);
    }

    #[test]
    fn short_fo76_xprm_relayout_does_not_cross_record_or_payload_shapes() {
        let short = vec![0xAA; 16];
        let full = vec![0xBB; 32];
        let malformed = vec![0xCC; 20];
        let mut actor = record_with_sig("ACHR", vec![subrecord("XPRM", short.clone())]);
        let mut placed = record(vec![
            subrecord("XPRM", full.clone()),
            subrecord("XPRM", malformed.clone()),
            subrecord("XRDO", short.clone()),
        ]);

        assert_eq!(relayout_short_fo76_xprm_subrecords(&mut actor), 0);
        assert_eq!(relayout_short_fo76_xprm_subrecords(&mut placed), 0);
        assert_eq!(actor.subrecords[0].data.as_ref(), short.as_slice());
        assert_eq!(placed.subrecords[0].data.as_ref(), full.as_slice());
        assert_eq!(placed.subrecords[1].data.as_ref(), malformed.as_slice());
        assert_eq!(placed.subrecords[2].data.as_ref(), short.as_slice());
        assert!(actor.raw_payload.is_some());
        assert!(placed.raw_payload.is_some());
    }

    #[test]
    fn post_copy_raw_payload_walk_relayouts_projected_refr_xprm() {
        let mut items = vec![
            ParsedItem::Record(record(vec![subrecord("XPRM", vec![0x11; 16])])),
            ParsedItem::Record(record_with_sig(
                "ACHR",
                vec![subrecord("XPRM", vec![0x22; 16])],
            )),
        ];
        let mut report = OwnershipXownNormalizeReport::default();
        let mut touched = SmallVec::<[u32; 4]>::new();

        normalize_ownership_xown_payloads_in_items(&mut items, &mut report, &mut touched);

        assert_eq!(report.records_changed, 1);
        assert_eq!(report.xprm_relaid_out, 1);
        assert_eq!(touched.as_slice(), &[0x0715781F]);
        let ParsedItem::Record(refr) = &items[0] else {
            panic!("expected REFR")
        };
        let ParsedItem::Record(achr) = &items[1] else {
            panic!("expected ACHR")
        };
        assert_eq!(refr.subrecords[0].data.len(), 32);
        assert_eq!(achr.subrecords[0].data.len(), 16);
    }

    #[test]
    fn remaps_xlkr_and_xapr_raw_zero_master_refs_to_output_when_target_exists() {
        let own_prefix: u32 = 0x0700_0000;
        let refr = SigCode::from_str("REFR").unwrap();
        let kywd = SigCode::from_str("KYWD").unwrap();
        let mut target_sigs = FxHashMap::default();
        target_sigs.insert(own_prefix | 0x003E6572, kywd);
        target_sigs.insert(own_prefix | 0x00467622, refr);
        target_sigs.insert(own_prefix | 0x003D28BD, refr);

        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&0x003E6572_u32.to_le_bytes());
        xlkr.extend_from_slice(&0x00467622_u32.to_le_bytes());
        let mut xapr = Vec::new();
        xapr.extend_from_slice(&0x003D28BD_u32.to_le_bytes());
        xapr.extend_from_slice(&0.087473735_f32.to_le_bytes());
        let mut placed = record(vec![subrecord("XLKR", xlkr), subrecord("XAPR", xapr)]);

        let changed = remap_placed_raw_local_ref_subrecords(&mut placed, own_prefix, &target_sigs);

        assert_eq!(changed, 3);
        assert!(placed.raw_payload.is_none());
        let xlkr = &placed.subrecords[0].data;
        let xapr = &placed.subrecords[1].data;
        assert_eq!(
            u32::from_le_bytes([xlkr[0], xlkr[1], xlkr[2], xlkr[3]]),
            0x073E6572
        );
        assert_eq!(
            u32::from_le_bytes([xlkr[4], xlkr[5], xlkr[6], xlkr[7]]),
            0x07467622
        );
        assert_eq!(
            u32::from_le_bytes([xapr[0], xapr[1], xapr[2], xapr[3]]),
            0x073D28BD
        );
        assert_eq!(
            f32::from_le_bytes([xapr[4], xapr[5], xapr[6], xapr[7]]),
            0.087473735_f32
        );
    }

    #[test]
    fn leaves_raw_zero_master_refs_without_output_target() {
        let own_prefix: u32 = 0x0700_0000;
        let refr = SigCode::from_str("REFR").unwrap();
        let mut target_sigs = FxHashMap::default();
        target_sigs.insert(own_prefix | 0x003D28BD, refr);

        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&0x003D28BD_u32.to_le_bytes());
        xlkr.extend_from_slice(&0x0005FE27_u32.to_le_bytes());
        let mut placed = record(vec![subrecord("XLKR", xlkr)]);

        let changed = remap_placed_raw_local_ref_subrecords(&mut placed, own_prefix, &target_sigs);

        assert_eq!(changed, 1);
        let xlkr = &placed.subrecords[0].data;
        assert_eq!(
            u32::from_le_bytes([xlkr[0], xlkr[1], xlkr[2], xlkr[3]]),
            0x073D28BD
        );
        assert_eq!(
            u32::from_le_bytes([xlkr[4], xlkr[5], xlkr[6], xlkr[7]]),
            0x0005FE27
        );
    }

    #[test]
    fn remaps_xplk_raw_zero_master_ref_only_to_valid_placed_target() {
        let own_prefix: u32 = 0x0700_0000;
        let refr = SigCode::from_str("REFR").unwrap();
        let land = SigCode::from_str("LAND").unwrap();
        let mut target_sigs = FxHashMap::default();
        target_sigs.insert(own_prefix | 0x0039E691, refr);
        target_sigs.insert(own_prefix | 0x00444444, land);

        let mut valid = Vec::new();
        valid.extend_from_slice(&0x0039E691_u32.to_le_bytes());
        valid.extend_from_slice(&0u32.to_le_bytes());
        let mut wrong_type = Vec::new();
        wrong_type.extend_from_slice(&0x00444444_u32.to_le_bytes());
        wrong_type.extend_from_slice(&0u32.to_le_bytes());
        let mut placed = record(vec![
            subrecord("XPLK", valid),
            subrecord("XPLK", wrong_type),
        ]);

        let changed = remap_placed_raw_local_ref_subrecords(&mut placed, own_prefix, &target_sigs);

        assert_eq!(changed, 1);
        assert_eq!(
            u32::from_le_bytes(placed.subrecords[0].data[0..4].try_into().unwrap()),
            0x0739E691
        );
        assert_eq!(
            u32::from_le_bytes(placed.subrecords[1].data[0..4].try_into().unwrap()),
            0x00444444,
            "wrong-type output collision left for master-aware post-copy repair"
        );
    }

    #[test]
    fn remaps_xloc_key_only_to_output_keym() {
        let own_prefix: u32 = 0x0700_0000;
        let keym = SigCode::from_str("KEYM").unwrap();
        let refr = SigCode::from_str("REFR").unwrap();
        let mut target_sigs = FxHashMap::default();
        target_sigs.insert(own_prefix | 0x0055ADA7, keym);
        target_sigs.insert(own_prefix | 0x00444444, refr);

        let mut valid = vec![0, 0, 0, 0];
        valid.extend_from_slice(&0x0055ADA7_u32.to_le_bytes());
        valid.extend_from_slice(&[0, 0, 0, 0]);
        let mut wrong_type = vec![0, 0, 0, 0];
        wrong_type.extend_from_slice(&0x00444444_u32.to_le_bytes());
        wrong_type.extend_from_slice(&[0, 0, 0, 0]);
        let mut placed = record(vec![
            subrecord("XLOC", valid),
            subrecord("XLOC", wrong_type),
        ]);

        let changed = remap_placed_raw_local_ref_subrecords(&mut placed, own_prefix, &target_sigs);

        assert_eq!(changed, 1);
        assert_eq!(
            u32::from_le_bytes(placed.subrecords[0].data[4..8].try_into().unwrap()),
            0x0755ADA7
        );
        assert_eq!(
            u32::from_le_bytes(placed.subrecords[1].data[4..8].try_into().unwrap()),
            0x00444444
        );
    }

    #[test]
    fn defaults_four_byte_placed_xown_to_owner_plus_no_crime() {
        let owner = 0x0710A3D6_u32.to_le_bytes().to_vec();
        let mut placed = record(vec![subrecord("XOWN", owner)]);

        let report = normalize_ownership_xown_subrecords(&mut placed);

        assert_eq!(report.records_changed, 1);
        assert_eq!(report.empty_stripped, 0);
        assert_eq!(report.no_crime_defaulted, 1);
        assert!(placed.raw_payload.is_none());
        assert_eq!(placed.subrecords[0].data.len(), 5);
        assert_eq!(
            u32::from_le_bytes(placed.subrecords[0].data[0..4].try_into().unwrap()),
            0x0710A3D6
        );
        assert_eq!(placed.subrecords[0].data[4], 0);
    }

    #[test]
    fn defaults_four_byte_cell_xown_to_owner_plus_no_crime() {
        let owner = 0x0710A3D6_u32.to_le_bytes().to_vec();
        let mut cell = record_with_sig("CELL", vec![subrecord("XOWN", owner)]);

        let report = normalize_ownership_xown_subrecords(&mut cell);

        assert_eq!(report.records_changed, 1);
        assert_eq!(report.empty_stripped, 0);
        assert_eq!(report.no_crime_defaulted, 1);
        assert!(cell.raw_payload.is_none());
        assert_eq!(cell.subrecords[0].data.len(), 5);
        assert_eq!(
            u32::from_le_bytes(cell.subrecords[0].data[0..4].try_into().unwrap()),
            0x0710A3D6
        );
        assert_eq!(cell.subrecords[0].data[4], 0);
    }

    #[test]
    fn strips_empty_ownership_xown_rows() {
        let mut placed = record(vec![subrecord("XOWN", Vec::new())]);

        let report = normalize_ownership_xown_subrecords(&mut placed);

        assert_eq!(report.records_changed, 1);
        assert_eq!(report.empty_stripped, 1);
        assert_eq!(report.no_crime_defaulted, 0);
        assert!(placed.raw_payload.is_none());
        assert!(placed.subrecords.is_empty());
    }

    #[test]
    fn strips_empty_placed_xown_and_defaults_owner_formid() {
        let mut placed = record(vec![
            subrecord("XOWN", Vec::new()),
            subrecord("XOWN", 0x0710A3D6_u32.to_le_bytes().to_vec()),
        ]);

        let report = normalize_ownership_xown_subrecords(&mut placed);

        assert_eq!(report.records_changed, 1);
        assert_eq!(report.empty_stripped, 1);
        assert_eq!(report.no_crime_defaulted, 1);
        assert!(placed.raw_payload.is_none());
        assert_eq!(placed.subrecords.len(), 1);
        assert_eq!(placed.subrecords[0].data.len(), 5);
        assert_eq!(
            u32::from_le_bytes(placed.subrecords[0].data[0..4].try_into().unwrap()),
            0x0710A3D6
        );
        assert_eq!(placed.subrecords[0].data[4], 0);
    }

    #[test]
    fn preserves_existing_longer_xown_payloads() {
        let mut payload = 0x0710A3D6_u32.to_le_bytes().to_vec();
        payload.push(1);
        let mut placed = record(vec![subrecord("XOWN", payload.clone())]);

        let report = normalize_ownership_xown_subrecords(&mut placed);

        assert_eq!(report.records_changed, 0);
        assert_eq!(report.empty_stripped, 0);
        assert_eq!(report.no_crime_defaulted, 0);
        assert!(placed.raw_payload.is_some());
        assert_eq!(placed.subrecords[0].data.as_ref(), payload.as_slice());
    }

    #[test]
    fn keeps_xlkr_when_keyword_and_ref_exist_in_target() {
        let own_prefix: u32 = 0x0700_0000;
        let kywd = SigCode::from_str("KYWD").unwrap();
        let achr = SigCode::from_str("ACHR").unwrap();
        let mut target_sigs = FxHashMap::default();
        target_sigs.insert(own_prefix | 0x0040B2B5, kywd); // keyword
        target_sigs.insert(own_prefix | 0x00404505, achr); // linked ref (copied in another pass)

        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&(own_prefix | 0x0040B2B5).to_le_bytes());
        xlkr.extend_from_slice(&(own_prefix | 0x00404505).to_le_bytes());
        let mut placed = record(vec![subrecord("XLKR", xlkr)]);

        let dropped = drop_dangling_xlkr_subrecords(&mut placed, own_prefix, &target_sigs);

        assert_eq!(dropped, 0);
        assert!(
            placed
                .subrecords
                .iter()
                .any(|s| s.signature.as_str() == "XLKR")
        );
    }

    #[test]
    fn drops_xlkr_when_linked_ref_is_genuinely_absent() {
        let own_prefix: u32 = 0x0700_0000;
        let kywd = SigCode::from_str("KYWD").unwrap();
        let mut target_sigs = FxHashMap::default();
        target_sigs.insert(own_prefix | 0x0040B2B5, kywd); // keyword present
        // linked ref own-prefix id is NOT in the assembled target → dangling.

        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&(own_prefix | 0x0040B2B5).to_le_bytes());
        xlkr.extend_from_slice(&(own_prefix | 0x00DEAD99).to_le_bytes());
        let mut placed = record(vec![subrecord("XLKR", xlkr)]);

        let dropped = drop_dangling_xlkr_subrecords(&mut placed, own_prefix, &target_sigs);

        assert_eq!(dropped, 1);
        assert!(
            placed
                .subrecords
                .iter()
                .all(|s| s.signature.as_str() != "XLKR")
        );
        assert!(placed.raw_payload.is_none());
    }

    #[test]
    fn keeps_xlkr_with_master_prefixed_refs() {
        // Master-plugin (Fallout4.esm) keyword/ref: not own-prefix, so never our
        // call to validate — leave for the engine.
        let own_prefix: u32 = 0x0700_0000;
        let target_sigs = FxHashMap::default();
        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&0x0000_0F2A_u32.to_le_bytes());
        xlkr.extend_from_slice(&0x0001_2345_u32.to_le_bytes());
        let mut placed = record(vec![subrecord("XLKR", xlkr)]);

        let dropped = drop_dangling_xlkr_subrecords(&mut placed, own_prefix, &target_sigs);

        assert_eq!(dropped, 0);
        assert!(
            placed
                .subrecords
                .iter()
                .any(|s| s.signature.as_str() == "XLKR")
        );
    }

    #[test]
    fn strips_xlrt_when_base_has_forced_loc_ref_type() {
        let mut forced_bases = FxHashSet::default();
        forced_bases.insert(0x0708F6B7);
        let mut placed = record(vec![
            subrecord("NAME", 0x0708F6B7_u32.to_le_bytes().to_vec()),
            subrecord("XLRT", 0x073D4B0D_u32.to_le_bytes().to_vec()),
        ]);

        let stripped = strip_xlrt_when_base_has_forced_loc_ref_type(&mut placed, &forced_bases);

        assert_eq!(stripped, 1);
        assert!(placed.raw_payload.is_none());
        assert!(
            placed
                .subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "XLRT")
        );
    }

    #[test]
    fn keeps_xlrt_when_base_has_no_forced_loc_ref_type() {
        let forced_bases = FxHashSet::default();
        let mut placed = record(vec![
            subrecord("NAME", 0x0708F6B7_u32.to_le_bytes().to_vec()),
            subrecord("XLRT", 0x073D4B0D_u32.to_le_bytes().to_vec()),
        ]);

        let stripped = strip_xlrt_when_base_has_forced_loc_ref_type(&mut placed, &forced_bases);

        assert_eq!(stripped, 0);
        assert!(placed.raw_payload.is_some());
        assert!(
            placed
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature.as_str() == "XLRT")
        );
    }
}
