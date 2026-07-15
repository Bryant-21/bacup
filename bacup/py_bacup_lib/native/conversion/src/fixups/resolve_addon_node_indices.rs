//! Fixup: reconcile AddonNode (ADDN) `NodeIndex` values against the FO4 base
//! game + DLC masters.
//!
//! # What an AddonNode index is
//! Each ADDN record carries a `NodeIndex` in its `DATA` subrecord (a single
//! `uint32`). The index must be **globally unique across every loaded ESM** —
//! NIF meshes reference an addon node by that integer (a `BSValueNode` named
//! `"AddOnNode<idx>"` with `Value == idx`), not by FormID.
//!
//! # The reconciliation rule (FO76 → FO4)
//! Identity of an addon node is its **content**, not its index. For each ADDN in
//! the converted plugin:
//!
//! * **DROP** — its content matches a vanilla master ADDN byte-for-byte. The
//!   converted record is removed, every FormID reference to it is repointed to
//!   the vanilla master, and (if the indices differ) a NIF remap
//!   `old_index → master_index` is emitted so meshes name the surviving node.
//! * **KEEP** — its content matches no vanilla node. Its source index is
//!   preserved only when FO4 can load it and it is not already used by a
//!   nonmatching vanilla node. Colliding or out-of-range kept nodes are assigned
//!   a fresh index from the conversion pool and emit a NIF remap
//!   `old_index → new_index`.
//!
//! Source ADDN indices are globally unique, so every NIF-remap key (`old_index`)
//! is unique and the `old → new` map handed to the NIF phase is unambiguous.
//! Preserving noncolliding FO76 indices keeps converted records aligned with
//! source NIF `BSValueNode` names when the value is inside FO4's loader range.
//!
//! # Ordering
//! This fixup MUST run before the null/dangling passes (see `run.rs` registry):
//! dropped-ADDN references are repointed to master here; if this ran after
//! null/dangling, those references would already have been nulled.
//!
//! # DATA subrecord layout (FO4 ADDN)
//! | Offset | Size | Field      |
//! |--------|------|------------|
//! |      0 |    4 | index (u32)|

use std::fmt::Write as _;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};
use crate::sym::StringInterner;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Byte size of the ADDN DATA payload (single uint32).
const DATA_SIZE: usize = 4;

/// First index handed out to converted ADDN records whose source index collides
/// with a nonmatching vanilla FO4 node or exceeds FO4's loader range.
const CONVERSION_RANGE_START: u32 = 31_000;

/// FO4 stores ADDN `DATA` as uint32 on disk, but CK's loader indexes a dense
/// addon-node table with it. Values above 0xFFFF can address past that table.
const FO4_MAX_PRESERVED_INDEX: u32 = 0xFFFF;

/// Subrecords that make up an ADDN's *content* identity, in canonical order.
/// Excludes `EDID` (name), `DATA` (the index itself), and `MODB`/`MODT`
/// (game-specific model hash blobs that never match across games).
const CONTENT_SIGS: [&str; 8] = [
    "OBND", "MODL", "MODC", "MODF", "MODS", "SNAM", "LNAM", "DNAM",
];

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct ResolveAddonNodeIndicesFixup;

impl Fixup for ResolveAddonNodeIndicesFixup {
    fn name(&self) -> &'static str {
        "resolve_addon_node_indices"
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
        let addn_sig =
            SigCode::from_str("ADDN").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        // Copies the `&StringInterner` out of the mapper so `mapper` can still be
        // borrowed mutably below (the reference itself does not borrow `mapper`).
        let interner = mapper.interner;

        // ── 1. Collect every ADDN FormKey in the converted plugin ──────────
        let converted_fks = session
            .form_keys_of_sig(addn_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if converted_fks.is_empty() {
            return Ok(FixupReport::empty());
        }

        // ── 2. Build the vanilla content map from the masters ──────────────
        // content_key → (master FormKey, master node index), plus the set of
        // every vanilla index (so KEEP allocation never reuses one).
        let mut vanilla_by_content: FxHashMap<String, (FormKey, u32)> = FxHashMap::default();
        let mut vanilla_indices: FxHashSet<u32> = FxHashSet::default();
        for &handle in &config.target_master_handle_ids {
            let master_fks = match session.form_keys_of_sig_in_handle(handle, addn_sig, interner) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for fk in master_fks {
                let record =
                    match session.record_decoded_in_handle(handle, &fk, target_schema, interner) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };
                let Some(idx) = extract_node_index(&record) else {
                    continue;
                };
                vanilla_indices.insert(idx);
                let key = addn_content_key(&record, interner);
                vanilla_by_content.entry(key).or_insert((fk, idx));
            }
        }

        // ── 3. Decode the converted ADDN records ───────────────────────────
        let mut converted: Vec<(FormKey, String, u32)> = Vec::with_capacity(converted_fks.len());
        let mut decoded_by_fk: FxHashMap<FormKey, Record> = FxHashMap::default();
        for fk in &converted_fks {
            let record = match session.record_decoded(fk, target_schema, interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let Some(idx) = extract_node_index(&record) else {
                continue;
            };
            let key = addn_content_key(&record, interner);
            converted.push((*fk, key, idx));
            decoded_by_fk.insert(*fk, record);
        }
        if converted.is_empty() {
            return Ok(FixupReport::empty());
        }

        // ── 4. Decide DROP vs KEEP for each converted node ─────────────────
        let (plans, nif_remap) = plan_addon_reconciliation(
            &converted,
            &vanilla_by_content,
            &vanilla_indices,
            CONVERSION_RANGE_START,
            FO4_MAX_PRESERVED_INDEX,
        );

        let mut report = FixupReport::empty();

        // ── 5. Apply KEEPs (reassign index) and gather DROPs ───────────────
        // converted FormKey → vanilla master FormKey.
        let mut drops: FxHashMap<FormKey, FormKey> = FxHashMap::default();
        for plan in &plans {
            match plan {
                AddnPlan::Keep { fk, new_index } => {
                    let Some(mut record) = decoded_by_fk.remove(fk) else {
                        continue;
                    };
                    set_node_index(&mut record, *new_index);
                    session
                        .replace_record(record, target_schema, interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    report.records_changed += 1;
                }
                AddnPlan::Drop { fk, master_fk } => {
                    drops.insert(*fk, *master_fk);
                }
            }
        }

        // ── 6. Repoint references to dropped nodes, then remove them ────────
        if !drops.is_empty() {
            report.records_changed += repoint_dropped_refs(session, mapper, target_schema, &drops)?;

            for fk in drops.keys() {
                if session
                    .remove_record(fk)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?
                {
                    report.records_dropped += 1;
                }
            }
        }

        report.addon_index_remap = nif_remap;
        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Reconciliation plan (pure — unit-test entrypoint)
// ---------------------------------------------------------------------------

/// One reconciliation decision for a converted ADDN record.
#[derive(Debug, Clone, PartialEq)]
pub enum AddnPlan {
    /// Content matches a vanilla master node — remove and repoint to `master_fk`.
    Drop { fk: FormKey, master_fk: FormKey },
    /// Content is novel — keep the record but reassign it `new_index`.
    Keep { fk: FormKey, new_index: u32 },
}

/// Decide DROP/KEEP for every converted ADDN and build the NIF `old → new`
/// index remap.
///
/// * `converted` — `(FormKey, content_key, old_index)` for each converted node.
/// * `vanilla_by_content` — content_key → `(master FormKey, master index)`.
/// * `vanilla_indices` — every index already used by a master.
/// * `range_start` — first index handed to reassigned KEEP nodes.
/// * `max_preserved_index` — largest source index safe to preserve unchanged.
///
/// Returns the per-record plans and the `(old_index, new_index)` NIF remap. A
/// DROP whose index already equals the master's emits no remap entry.
pub fn plan_addon_reconciliation(
    converted: &[(FormKey, String, u32)],
    vanilla_by_content: &FxHashMap<String, (FormKey, u32)>,
    vanilla_indices: &FxHashSet<u32>,
    range_start: u32,
    max_preserved_index: u32,
) -> (Vec<AddnPlan>, Vec<(i64, i64)>) {
    let mut plans = Vec::with_capacity(converted.len());
    let mut nif_remap: Vec<(i64, i64)> = Vec::new();

    let mut used: FxHashSet<u32> = vanilla_indices.clone();
    let mut next = range_start;
    assert!(range_start <= max_preserved_index);

    for (fk, content, old_index) in converted {
        if let Some((master_fk, master_index)) = vanilla_by_content.get(content) {
            plans.push(AddnPlan::Drop {
                fk: *fk,
                master_fk: *master_fk,
            });
            if old_index != master_index {
                nif_remap.push((*old_index as i64, *master_index as i64));
            }
        } else if used.contains(old_index) || *old_index > max_preserved_index {
            while next <= max_preserved_index && used.contains(&next) {
                next = next.saturating_add(1);
            }
            assert!(
                next <= max_preserved_index,
                "exhausted FO4-safe AddonNode index range"
            );
            let new_index = next;
            used.insert(new_index);
            next = next.saturating_add(1);

            plans.push(AddnPlan::Keep { fk: *fk, new_index });
            if *old_index != new_index {
                nif_remap.push((*old_index as i64, new_index as i64));
            }
        } else {
            used.insert(*old_index);
            plans.push(AddnPlan::Keep {
                fk: *fk,
                new_index: *old_index,
            });
        }
    }

    (plans, nif_remap)
}

// ---------------------------------------------------------------------------
// Reference repointing
// ---------------------------------------------------------------------------

/// Repoint every FormID reference to a dropped ADDN onto its vanilla master.
///
/// Runs only when `drops` is non-empty. For each output signature it walks
/// candidate records with a cheap raw-bytes prefilter (skip any record whose
/// subrecord bytes don't contain a dropped node's on-disk FormID) before the
/// expensive schema decode. Returns the number of records rewritten.
fn repoint_dropped_refs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    drops: &FxHashMap<FormKey, FormKey>,
) -> Result<u32, FixupError> {
    let interner = mapper.interner;
    let addn_sig = SigCode::from_str("ADDN").map_err(|e| FixupError::SchemaError(e.to_string()))?;

    // Self-records encode their own FormIDs with the high byte equal to the
    // master count; build the raw 4-byte LE needle for each dropped node.
    let self_index = session.target_masters().len() as u32;
    let mut needles: FxHashSet<u32> = FxHashSet::default();
    for fk in drops.keys() {
        needles.insert((self_index << 24) | (fk.local & 0x00FF_FFFF));
    }

    let sigs = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;

    let mut changed = 0u32;
    for sig in sigs {
        if sig == addn_sig {
            continue;
        }
        let report = session.map_apply_by_sig(
            sig,
            mapper,
            |view, _snap, fk| {
                let raw = (self_index << 24) | (fk.local & 0x00FF_FFFF);
                let parsed = view.record(raw)?;
                if !record_raw_contains_any(parsed, &needles) {
                    return None;
                }
                let mut record = view.record_decoded(fk, target_schema, interner).ok()?;
                if rewrite_fk_leaves(&mut record, drops) {
                    Some(record)
                } else {
                    None
                }
            },
            |session, mapper, _fk, record| {
                session
                    .replace_record_contents(record, target_schema, mapper.interner)
                    .map(|_| EditOutcome::Changed)
                    .map_err(|e| FixupError::HandleError(e.to_string()))
            },
        )?;
        changed += report.records_changed;
    }

    Ok(changed)
}

/// True if any subrecord payload of `record` contains one of the raw 4-byte LE
/// `needles`. Zero false negatives; false positives are re-checked by the full
/// decode in the caller.
fn record_raw_contains_any(
    record: &esp_authoring_core::plugin_runtime::ParsedRecord,
    needles: &FxHashSet<u32>,
) -> bool {
    for sr in &record.subrecords {
        let data: &[u8] = &sr.data;
        if data.len() < 4 {
            continue;
        }
        for window in data.windows(4) {
            let value = u32::from_le_bytes([window[0], window[1], window[2], window[3]]);
            if needles.contains(&value) {
                return true;
            }
        }
    }
    false
}

/// Replace every `FormKey` leaf that is a key of `drops` with its mapped value,
/// recursing through `List` and `Struct` values. Returns whether anything
/// changed.
fn rewrite_fk_leaves(record: &mut Record, drops: &FxHashMap<FormKey, FormKey>) -> bool {
    let mut changed = false;
    for entry in &mut record.fields {
        if rewrite_value_leaves(&mut entry.value, drops) {
            changed = true;
        }
    }
    changed
}

fn rewrite_value_leaves(value: &mut FieldValue, drops: &FxHashMap<FormKey, FormKey>) -> bool {
    match value {
        FieldValue::FormKey(fk) => {
            if let Some(master_fk) = drops.get(fk) {
                *fk = *master_fk;
                true
            } else {
                false
            }
        }
        FieldValue::List(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                if rewrite_value_leaves(item, drops) {
                    changed = true;
                }
            }
            changed
        }
        FieldValue::Struct(pairs) => {
            let mut changed = false;
            for (_, item) in pairs.iter_mut() {
                if rewrite_value_leaves(item, drops) {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Content hashing
// ---------------------------------------------------------------------------

/// Build a stable content key for an ADDN record over the `CONTENT_SIGS`
/// fields. FormKey leaves normalise to `(plugin_lc, object_id)` so a converted
/// reference that already points at a vanilla target compares equal to vanilla;
/// `MODL` paths normalise to lowercase forward-slash form.
pub fn addn_content_key(record: &Record, interner: &StringInterner) -> String {
    let mut buf = String::new();
    for sig_str in CONTENT_SIGS {
        let Ok(sig) = SubrecordSig::from_str(sig_str) else {
            continue;
        };
        let Some(entry) = record.fields.iter().find(|e| e.sig == sig) else {
            continue;
        };
        buf.push_str(sig_str);
        buf.push(':');
        serialize_value(&mut buf, &entry.value, interner);
        buf.push('|');
    }
    buf
}

fn serialize_value(buf: &mut String, value: &FieldValue, interner: &StringInterner) {
    match value {
        FieldValue::None => buf.push('N'),
        FieldValue::Bool(b) => {
            buf.push('b');
            buf.push(if *b { '1' } else { '0' });
        }
        FieldValue::Int(i) => {
            let _ = write!(buf, "i{i}");
        }
        FieldValue::Uint(u) => {
            let _ = write!(buf, "u{u}");
        }
        FieldValue::Float(f) => {
            let _ = write!(buf, "f{}", f.to_bits());
        }
        FieldValue::String(sym) => {
            buf.push('s');
            let resolved = interner.resolve(*sym).unwrap_or("");
            for ch in resolved.chars() {
                let c = ch.to_ascii_lowercase();
                buf.push(if c == '\\' { '/' } else { c });
            }
        }
        FieldValue::Bytes(bytes) => {
            buf.push('x');
            for byte in bytes.iter() {
                let _ = write!(buf, "{byte:02x}");
            }
        }
        FieldValue::FormKey(fk) => {
            buf.push('k');
            let plugin = interner.resolve(fk.plugin).unwrap_or("");
            for ch in plugin.chars() {
                buf.push(ch.to_ascii_lowercase());
            }
            let _ = write!(buf, ":{:06x}", fk.local);
        }
        FieldValue::List(items) => {
            buf.push('[');
            for item in items {
                serialize_value(buf, item, interner);
                buf.push(',');
            }
            buf.push(']');
        }
        FieldValue::Struct(pairs) => {
            buf.push('{');
            for (key, item) in pairs {
                let name = interner.resolve(*key).unwrap_or("");
                buf.push_str(name);
                buf.push('=');
                serialize_value(buf, item, interner);
                buf.push(';');
            }
            buf.push('}');
        }
    }
}

// ---------------------------------------------------------------------------
// DATA index helpers
// ---------------------------------------------------------------------------

/// Extract the `NodeIndex` from an ADDN record's `DATA` subrecord.
///
/// Returns `None` when no `DATA` subrecord exists or its payload is malformed.
/// Handles the schema-decoded `Uint`/`Int` forms and the raw `Bytes` fallback.
pub fn extract_node_index(record: &Record) -> Option<u32> {
    let data_sig = SubrecordSig::from_str("DATA").ok()?;

    for entry in &record.fields {
        if entry.sig != data_sig {
            continue;
        }
        return match &entry.value {
            FieldValue::Uint(v) => Some(*v as u32),
            FieldValue::Int(v) if *v >= 0 => Some(*v as u32),
            FieldValue::Bytes(b) if b.len() >= DATA_SIZE => {
                Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }
            _ => None,
        };
    }
    None
}

/// Set the `NodeIndex` in an ADDN record's `DATA` subrecord, mutating in place
/// or appending a `Uint` entry if none exists.
pub fn set_node_index(record: &mut Record, new_index: u32) {
    let data_sig = match SubrecordSig::from_str("DATA") {
        Ok(s) => s,
        Err(_) => return,
    };

    for entry in &mut record.fields {
        if entry.sig != data_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Uint(v) => {
                *v = new_index as u64;
                return;
            }
            FieldValue::Int(v) => {
                *v = new_index as i64;
                return;
            }
            FieldValue::Bytes(b) if b.len() >= DATA_SIZE => {
                let le = new_index.to_le_bytes();
                b[0] = le[0];
                b[1] = le[1];
                b[2] = le[2];
                b[3] = le[3];
                return;
            }
            _ => {
                entry.value = FieldValue::Uint(new_index as u64);
                return;
            }
        }
    }

    record.fields.push(FieldEntry {
        sig: data_sig,
        value: FieldValue::Uint(new_index as u64),
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    /// Build an ADDN record with a DATA index plus arbitrary extra fields.
    fn addn(
        local: u32,
        plugin: &str,
        node_index: u32,
        extra: Vec<(&str, FieldValue)>,
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("ADDN").unwrap();
        let data_sig = SubrecordSig::from_str("DATA").unwrap();

        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        fields.push(FieldEntry {
            sig: data_sig,
            value: FieldValue::Uint(node_index as u64),
        });
        for (s, v) in extra {
            fields.push(FieldEntry {
                sig: SubrecordSig::from_str(s).unwrap(),
                value: v,
            });
        }

        Record {
            sig,
            form_key: fk(local, plugin, interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn modl(path: &str, interner: &StringInterner) -> FieldValue {
        FieldValue::String(interner.intern(path))
    }

    // -- content key -------------------------------------------------------

    #[test]
    fn content_key_ignores_edid_and_index() {
        let interner = StringInterner::new();
        let a = addn(
            0x01,
            "Out.esp",
            100,
            vec![("MODL", modl("Foo\\Bar.nif", &interner))],
            &interner,
        );
        // Different index, different FormKey — same content fields.
        let b = addn(
            0x02,
            "Out.esp",
            999,
            vec![("MODL", modl("Foo\\Bar.nif", &interner))],
            &interner,
        );
        assert_eq!(
            addn_content_key(&a, &interner),
            addn_content_key(&b, &interner)
        );
    }

    #[test]
    fn content_key_modl_path_normalised() {
        let interner = StringInterner::new();
        let a = addn(
            0x01,
            "Out.esp",
            1,
            vec![("MODL", modl("Effects\\Foo.NIF", &interner))],
            &interner,
        );
        let b = addn(
            0x02,
            "Out.esp",
            1,
            vec![("MODL", modl("effects/foo.nif", &interner))],
            &interner,
        );
        assert_eq!(
            addn_content_key(&a, &interner),
            addn_content_key(&b, &interner)
        );
    }

    #[test]
    fn content_key_differs_on_snam() {
        let interner = StringInterner::new();
        let a = addn(
            0x01,
            "Out.esp",
            1,
            vec![(
                "SNAM",
                FieldValue::FormKey(fk(0x10, "Fallout4.esm", &interner)),
            )],
            &interner,
        );
        let b = addn(
            0x02,
            "Out.esp",
            1,
            vec![(
                "SNAM",
                FieldValue::FormKey(fk(0x20, "Fallout4.esm", &interner)),
            )],
            &interner,
        );
        assert_ne!(
            addn_content_key(&a, &interner),
            addn_content_key(&b, &interner)
        );
    }

    #[test]
    fn content_key_resolves_formkey_target_equally() {
        let interner = StringInterner::new();
        // Same resolved (plugin, object_id) → equal regardless of casing.
        let a = addn(
            0x01,
            "Out.esp",
            1,
            vec![(
                "LNAM",
                FieldValue::FormKey(fk(0xAB, "Fallout4.esm", &interner)),
            )],
            &interner,
        );
        let b = addn(
            0x02,
            "Out.esp",
            1,
            vec![(
                "LNAM",
                FieldValue::FormKey(fk(0xAB, "fallout4.esm", &interner)),
            )],
            &interner,
        );
        assert_eq!(
            addn_content_key(&a, &interner),
            addn_content_key(&b, &interner)
        );
    }

    // -- reconciliation plan ----------------------------------------------

    #[test]
    fn plan_drops_on_content_match_with_remap() {
        let interner = StringInterner::new();
        let master_fk = fk(0x50, "Fallout4.esm", &interner);
        let mut vanilla: FxHashMap<String, (FormKey, u32)> = FxHashMap::default();
        vanilla.insert("C".to_string(), (master_fk, 50));
        let mut vanilla_idx: FxHashSet<u32> = FxHashSet::default();
        vanilla_idx.insert(50);

        let conv_fk = fk(0x0801, "Out.esp", &interner);
        let converted = vec![(conv_fk, "C".to_string(), 119u32)];

        let (plans, remap) = plan_addon_reconciliation(
            &converted,
            &vanilla,
            &vanilla_idx,
            CONVERSION_RANGE_START,
            FO4_MAX_PRESERVED_INDEX,
        );
        assert_eq!(
            plans,
            vec![AddnPlan::Drop {
                fk: conv_fk,
                master_fk
            }]
        );
        assert_eq!(remap, vec![(119, 50)]);
    }

    #[test]
    fn plan_drop_same_index_emits_no_remap() {
        let interner = StringInterner::new();
        let master_fk = fk(0x50, "Fallout4.esm", &interner);
        let mut vanilla: FxHashMap<String, (FormKey, u32)> = FxHashMap::default();
        vanilla.insert("C".to_string(), (master_fk, 119));
        let mut vanilla_idx: FxHashSet<u32> = FxHashSet::default();
        vanilla_idx.insert(119);

        let conv_fk = fk(0x0801, "Out.esp", &interner);
        let converted = vec![(conv_fk, "C".to_string(), 119u32)];

        let (plans, remap) = plan_addon_reconciliation(
            &converted,
            &vanilla,
            &vanilla_idx,
            CONVERSION_RANGE_START,
            FO4_MAX_PRESERVED_INDEX,
        );
        assert_eq!(
            plans,
            vec![AddnPlan::Drop {
                fk: conv_fk,
                master_fk
            }]
        );
        assert!(remap.is_empty());
    }

    #[test]
    fn plan_keeps_novel_content_at_source_index() {
        let interner = StringInterner::new();
        let vanilla: FxHashMap<String, (FormKey, u32)> = FxHashMap::default();
        let vanilla_idx: FxHashSet<u32> = FxHashSet::default();

        let f0 = fk(0x0801, "Out.esp", &interner);
        let f1 = fk(0x0802, "Out.esp", &interner);
        let converted = vec![(f0, "A".to_string(), 10u32), (f1, "B".to_string(), 11u32)];

        let (plans, remap) = plan_addon_reconciliation(
            &converted,
            &vanilla,
            &vanilla_idx,
            CONVERSION_RANGE_START,
            FO4_MAX_PRESERVED_INDEX,
        );
        assert_eq!(
            plans,
            vec![
                AddnPlan::Keep {
                    fk: f0,
                    new_index: 10
                },
                AddnPlan::Keep {
                    fk: f1,
                    new_index: 11
                },
            ]
        );
        assert!(remap.is_empty());
    }

    #[test]
    fn plan_keep_allocation_only_for_colliding_index() {
        let interner = StringInterner::new();
        let vanilla: FxHashMap<String, (FormKey, u32)> = FxHashMap::default();
        let mut vanilla_idx: FxHashSet<u32> = FxHashSet::default();
        vanilla_idx.insert(5);
        vanilla_idx.insert(CONVERSION_RANGE_START);

        let f0 = fk(0x0801, "Out.esp", &interner);
        let converted = vec![(f0, "novel".to_string(), 5u32)];

        let (plans, remap) = plan_addon_reconciliation(
            &converted,
            &vanilla,
            &vanilla_idx,
            CONVERSION_RANGE_START,
            FO4_MAX_PRESERVED_INDEX,
        );
        assert_eq!(
            plans,
            vec![AddnPlan::Keep {
                fk: f0,
                new_index: CONVERSION_RANGE_START + 1
            }]
        );
        assert_eq!(remap, vec![(5, i64::from(CONVERSION_RANGE_START + 1))]);
    }

    #[test]
    fn plan_keep_allocates_for_fo76_index_above_fo4_loader_limit() {
        let interner = StringInterner::new();
        let vanilla: FxHashMap<String, (FormKey, u32)> = FxHashMap::default();
        let vanilla_idx: FxHashSet<u32> = FxHashSet::default();

        let f0 = fk(0x0801, "Out.esp", &interner);
        let converted = vec![(f0, "novel".to_string(), 208_480_396u32)];

        let (plans, remap) = plan_addon_reconciliation(
            &converted,
            &vanilla,
            &vanilla_idx,
            CONVERSION_RANGE_START,
            FO4_MAX_PRESERVED_INDEX,
        );
        assert_eq!(
            plans,
            vec![AddnPlan::Keep {
                fk: f0,
                new_index: CONVERSION_RANGE_START
            }]
        );
        assert_eq!(
            remap,
            vec![(208_480_396, i64::from(CONVERSION_RANGE_START))]
        );
    }

    // -- FK leaf rewrite ---------------------------------------------------

    #[test]
    fn rewrite_replaces_nested_formkey_leaves() {
        let interner = StringInterner::new();
        let dropped = fk(0x0801, "Out.esp", &interner);
        let master = fk(0x50, "Fallout4.esm", &interner);
        let mut drops: FxHashMap<FormKey, FormKey> = FxHashMap::default();
        drops.insert(dropped, master);

        let sig = SigCode::from_str("REFR").unwrap();
        let list_sig = SubrecordSig::from_str("XLKR").unwrap();
        let mut record = Record {
            sig,
            form_key: fk(0x0900, "Out.esp", &interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: list_sig,
                value: FieldValue::List(vec![
                    FieldValue::Struct(vec![(
                        interner.intern("ref"),
                        FieldValue::FormKey(dropped),
                    )]),
                    FieldValue::FormKey(fk(0x0123, "Out.esp", &interner)),
                ]),
            }],
            warnings: smallvec::SmallVec::new(),
        };

        assert!(rewrite_fk_leaves(&mut record, &drops));
        // The dropped leaf is now the master; the unrelated leaf is untouched.
        if let FieldValue::List(items) = &record.fields[0].value {
            if let FieldValue::Struct(pairs) = &items[0] {
                assert_eq!(pairs[0].1, FieldValue::FormKey(master));
            } else {
                panic!("expected struct");
            }
            assert_eq!(
                items[1],
                FieldValue::FormKey(fk(0x0123, "Out.esp", &interner))
            );
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn rewrite_returns_false_without_match() {
        let interner = StringInterner::new();
        let mut drops: FxHashMap<FormKey, FormKey> = FxHashMap::default();
        drops.insert(
            fk(0x0801, "Out.esp", &interner),
            fk(0x50, "Fallout4.esm", &interner),
        );

        let mut record = addn(0x0900, "Out.esp", 1, vec![], &interner);
        assert!(!rewrite_fk_leaves(&mut record, &drops));
    }

    // -- DATA helpers ------------------------------------------------------

    #[test]
    fn extract_node_index_from_bytes() {
        let interner = StringInterner::new();
        let sig = SigCode::from_str("ADDN").unwrap();
        let data_sig = SubrecordSig::from_str("DATA").unwrap();
        let mut bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        bytes.extend_from_slice(&42u32.to_le_bytes());

        let record = Record {
            sig,
            form_key: fk(0x0801, "Out.esp", &interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: data_sig,
                value: FieldValue::Bytes(bytes),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        assert_eq!(extract_node_index(&record), Some(42));
    }

    #[test]
    fn extract_node_index_none_without_data() {
        let interner = StringInterner::new();
        let sig = SigCode::from_str("ADDN").unwrap();
        let record = Record {
            sig,
            form_key: fk(0x0801, "Out.esp", &interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        };
        assert_eq!(extract_node_index(&record), None);
    }

    #[test]
    fn set_node_index_mutates_in_place() {
        let interner = StringInterner::new();
        let mut record = addn(0x0801, "Out.esp", 10, vec![], &interner);
        set_node_index(&mut record, 760_005);
        assert_eq!(extract_node_index(&record), Some(760_005));
    }

    #[test]
    fn set_node_index_appends_when_missing() {
        let interner = StringInterner::new();
        let sig = SigCode::from_str("ADDN").unwrap();
        let mut record = Record {
            sig,
            form_key: fk(0x0801, "Out.esp", &interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        };
        set_node_index(&mut record, 77);
        assert_eq!(extract_node_index(&record), Some(77));
    }
}
