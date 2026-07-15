//! Fixup: repair or null a handful of small-bucket FO76→FO4 reference slots that
//! the translate-time FormKey remap and the generic invalid-target nuller both
//! miss because the FormID lives inside a `struct:`/`array_struct:` codec (decoded
//! as opaque `Bytes`) or because the value is a master-byte-truncated leaf.
//!
//! # Buckets
//! Two distinct repairs, keyed by `(record sig, subrecord sig, byte offset)`:
//!
//! 1. **Null dangling** — MGEF `DATA` Assoc. Item (off 8), EFSH `DNAM` Ambient
//!    Sound (off 108), IDLE `ANAM` Animations-Parent (off 0), and COBJ `FVPA`
//!    component slots. These hold a FormID that resolves to no record in any
//!    target master *and* no record in the output plugin — a dangling reference
//!    with no FO4 equivalent (e.g. an FO76 `DMGT`/`STHD` Assoc. Item, an FO76-only
//!    `UTIL` component). xEdit reports "Could not be resolved". The FO4-correct
//!    representation of an absent reference in these formid fields is `0`, so we
//!    zero the 4 bytes.
//!    Cloak MGEFs are type-sensitive: their Assoc. Item must be a SPEL. A source
//!    object-id can collide with a non-SPEL Fallout4.esm record and look resolved,
//!    so that slot is repaired to a same-id output SPEL or nulled when none exists.
//!
//! 2. **Repair truncated master byte** — SNDR `BNAM` Base Descriptor (off 0,
//!    decoded as a FormKey leaf) and SNDR `CTDA` Parameter #1 (off 12, inside the
//!    `struct:` blob). These point at a record that WAS converted and emitted in
//!    the output plugin, but whose master byte was truncated to `00`
//!    (= Fallout4.esm, master index 0). We rewrite only the master byte to the
//!    output plugin index when the object-id exists in the output plugin and is
//!    NOT a real Fallout4.esm …20412 tokens truncated…alue at index 1 must drop.
//!
//! # Plugin-aware
//! Every decision is made on the *decoded* `(master_index, object_id)` of the raw
//! u32, checked against the actual object-id set of that specific handle. A value
//! that already resolves in its addressed master (e.g. a valid Fallout4.esm
//! Assoc. Item) or that addresses a master other than the truncation target is
//! left byte-identical. Sentinels such as SNDR.BNAM `0x00800000` (object-id far
//! above the Fallout4.esm range, present in neither master nor output) are never
//! touched by the repair path and never match the null path's slot list.

use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const MGEF_ASSOC_ITEM_OFFSET: usize = 8;
const MGEF_ARCHETYPE_OFFSET: usize = 64;
const MGEF_ARCHETYPE_CLOAK: u32 = 35;

/// A formid slot to repair, located by record sig + subrecord sig + byte offset
/// of the 4-byte little-endian FormID within the subrecord's raw bytes.
struct ByteSlot {
    record_sig: &'static str,
    subrec_sig: &'static str,
    offset: usize,
}

/// Slots whose unresolved FormID is nulled (no FO4 equivalent).
const NULL_SLOTS: &[ByteSlot] = &[
    ByteSlot {
        record_sig: "MGEF",
        subrec_sig: "DATA",
        offset: MGEF_ASSOC_ITEM_OFFSET,
    },
    ByteSlot {
        record_sig: "EFSH",
        subrec_sig: "DNAM",
        offset: 108,
    },
    ByteSlot {
        record_sig: "IDLE",
        subrec_sig: "ANAM",
        offset: 0,
    },
];

/// COBJ FVPA is an `array_struct:I,I` (component formid + count); every 8-byte
/// row's first dword is a component FormID. Handled separately from `NULL_SLOTS`
/// because it repeats per row.
const COBJ_FVPA_ROW_SIZE: usize = 8;

/// Slots whose master-byte-truncated FormID is repaired to the output plugin
/// when the object-id was actually emitted there.
const REPAIR_SLOTS: &[ByteSlot] = &[
    // SNDR.CTDA Parameter #1 lives at offset 12 of the CTDA struct blob.
    ByteSlot {
        record_sig: "SNDR",
        subrec_sig: "CTDA",
        offset: 12,
    },
];

pub struct NullDanglingMiscRefsFixup;

impl Fixup for NullDanglingMiscRefsFixup {
    fn name(&self) -> &'static str {
        "null_dangling_misc_refs"
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
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let resolver = SlotResolver::build(session, config, mapper.interner)?;

        // Record sigs we actually touch (union of all slot lists + SNDR.BNAM + COBJ).
        let touched_sigs = touched_record_sigs();
        let available: FxHashSet<SigCode> = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .into_iter()
            .collect();

        let mut changed_records = Vec::new();
        for sig_str in touched_sigs {
            let Ok(sig) = SigCode::from_str(sig_str) else {
                continue;
            };
            if !available.contains(&sig) {
                continue;
            }
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if apply_to_record(&mut record, &resolver, mapper.interner) {
                    changed_records.push(record);
                }
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "null_dangling_misc_refs replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

pub(crate) fn touched_record_sigs() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = NULL_SLOTS.iter().map(|s| s.record_sig).collect();
    v.extend(REPAIR_SLOTS.iter().map(|s| s.record_sig));
    v.push("SNDR"); // SNDR.BNAM FormKey leaf
    v.push("COBJ"); // COBJ.FVPA component rows
    v.sort_unstable();
    v.dedup();
    v
}

/// Resolves a raw `(master_index << 24) | object_id` FormID against the actual
/// object-id sets of the output plugin and each target master.
pub(crate) struct SlotResolver {
    /// Object-ids present in the output plugin.
    output_objids: FxHashSet<u32>,
    /// Object-ids present as SPEL records in the output plugin.
    output_spel_objids: FxHashSet<u32>,
    /// Per target-master object-id sets, indexed by master load order.
    /// `Arc` so the store2 master-scan cache can share them across sweeps.
    master_objids: Vec<Arc<FxHashSet<u32>>>,
    /// Per target-master SPEL object-id sets, parallel to `master_objids`.
    master_spel_objids: Vec<Arc<FxHashSet<u32>>>,
    /// Output plugin's own master index = number of target masters.
    output_master_index: u32,
    /// Name of the first target master (the master-byte-0 truncation target).
    first_master: Option<String>,
    /// Output plugin name (for rewriting a repaired FormKey leaf's plugin sym).
    output_plugin: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SlotResolution {
    /// Already resolves in its addressed handle, or is null — leave unchanged.
    Keep,
    /// Resolves nowhere — null it (zero the bytes).
    Null,
    /// Master byte truncated; rewrite to the output plugin index.
    RepairToOutput,
}

impl SlotResolver {
    pub(crate) fn build(
        session: &mut PluginSession,
        config: &FixupConfig,
        interner: &StringInterner,
    ) -> Result<Self, FixupError> {
        let mut master_objids = Vec::with_capacity(config.target_master_handle_ids.len());
        for &handle_id in &config.target_master_handle_ids {
            let set = session
                .local_object_ids_in_handle(handle_id)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            master_objids.push(Arc::new(set));
        }
        Self::build_with_master_objids(
            session,
            interner,
            master_objids,
            &config.target_master_handle_ids,
        )
    }

    /// `build` with the master scan supplied by the caller (the store2
    /// master-scan cache); everything output-derived is gathered fresh.
    pub(crate) fn build_with_master_objids(
        session: &mut PluginSession,
        interner: &StringInterner,
        master_objids: Vec<Arc<FxHashSet<u32>>>,
        target_master_handle_ids: &[u64],
    ) -> Result<Self, FixupError> {
        let masters = session.target_masters().to_vec();
        let output_master_index = masters.len() as u32;
        let first_master = masters.first().cloned();
        let output_plugin = Some(session.target_slot().parsed.plugin_name.clone());
        let target_id = session.target_id();
        let output_objids = session
            .local_object_ids_in_handle(target_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let spel_sig =
            SigCode::from_str("SPEL").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let output_spel_objids = session
            .form_keys_of_sig(spel_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .into_iter()
            .map(|fk| fk.local & 0x00FF_FFFF)
            .collect();
        let mut master_spel_objids = Vec::with_capacity(target_master_handle_ids.len());
        for &handle_id in target_master_handle_ids {
            let object_ids = session
                .form_keys_of_sig_in_handle(handle_id, spel_sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?
                .into_iter()
                .map(|fk| fk.local & 0x00FF_FFFF)
                .collect();
            master_spel_objids.push(Arc::new(object_ids));
        }
        Ok(Self {
            output_objids,
            output_spel_objids,
            master_objids,
            master_spel_objids,
            output_master_index,
            first_master,
            output_plugin,
        })
    }

    fn object_exists(&self, master_index: u32, object_id: u32) -> bool {
        if master_index == self.output_master_index {
            self.output_objids.contains(&object_id)
        } else {
            self.master_objids
                .get(master_index as usize)
                .is_some_and(|set| set.contains(&object_id))
        }
    }

    /// Decide what to do with a raw FormID in a NULL slot.
    fn resolve_null_slot(&self, raw: u32) -> SlotResolution {
        if raw == 0 {
            return SlotResolution::Keep;
        }
        let master_index = raw >> 24;
        let object_id = raw & 0x00FF_FFFF;
        if self.object_exists(master_index, object_id) {
            SlotResolution::Keep
        } else {
            SlotResolution::Null
        }
    }

    /// Decide what to do with a raw FormID in a REPAIR slot. Only repairs a
    /// master-index-0 truncation when the object-id exists in the output plugin
    /// and is not a real Fallout4.esm (master 0) record.
    fn resolve_repair_slot(&self, raw: u32) -> SlotResolution {
        if raw == 0 {
            return SlotResolution::Keep;
        }
        let master_index = raw >> 24;
        let object_id = raw & 0x00FF_FFFF;
        if self.object_exists(master_index, object_id) {
            return SlotResolution::Keep;
        }
        if master_index == 0
            && !self.object_exists(0, object_id)
            && self.output_objids.contains(&object_id)
        {
            SlotResolution::RepairToOutput
        } else {
            SlotResolution::Keep
        }
    }

    fn repair_raw(&self, raw: u32) -> u32 {
        (self.output_master_index << 24) | (raw & 0x00FF_FFFF)
    }

    fn resolve_cloak_assoc_item(&self, raw: u32) -> SlotResolution {
        if raw == 0 {
            return SlotResolution::Keep;
        }
        let master_index = raw >> 24;
        let object_id = raw & 0x00FF_FFFF;
        if master_index == self.output_master_index {
            return if self.output_spel_objids.contains(&object_id) {
                SlotResolution::Keep
            } else {
                SlotResolution::Null
            };
        }
        if self
            .master_spel_objids
            .get(master_index as usize)
            .is_some_and(|spells| spells.contains(&object_id))
        {
            return SlotResolution::Keep;
        }
        if self.output_spel_objids.contains(&object_id) {
            SlotResolution::RepairToOutput
        } else {
            SlotResolution::Null
        }
    }
}

pub(crate) fn apply_to_record(
    record: &mut Record,
    resolver: &SlotResolver,
    interner: &StringInterner,
) -> bool {
    let sig = record.sig.as_str();
    let mut changed = false;

    for entry in record.fields.iter_mut() {
        let sub = entry.sig.as_str();

        if sig == "MGEF" && sub == "DATA" {
            if let FieldValue::Bytes(bytes) = &mut entry.value {
                if repair_cloak_assoc_item(bytes, resolver) {
                    changed = true;
                }
            }
        }

        // Null slots (byte-offset within an opaque struct blob).
        for slot in NULL_SLOTS {
            if slot.record_sig == sig && slot.subrec_sig == sub {
                if let FieldValue::Bytes(bytes) = &mut entry.value {
                    if zero_dangling_at(bytes, slot.offset, resolver) {
                        changed = true;
                    }
                }
            }
        }

        // Repair slots (byte-offset within an opaque struct blob).
        for slot in REPAIR_SLOTS {
            if slot.record_sig == sig && slot.subrec_sig == sub {
                if let FieldValue::Bytes(bytes) = &mut entry.value {
                    if repair_at(bytes, slot.offset, resolver) {
                        changed = true;
                    }
                }
            }
        }

        // COBJ.FVPA component rows (array_struct:I,I).
        if sig == "COBJ" && sub == "FVPA" {
            if let FieldValue::Bytes(bytes) = &mut entry.value {
                if null_fvpa_components(bytes, resolver) {
                    changed = true;
                }
            }
        }

        // SNDR.BNAM Base Descriptor — decoded as a FormKey leaf (repair only).
        if sig == "SNDR" && sub == "BNAM" {
            if let FieldValue::FormKey(fk) = &mut entry.value {
                if repair_formkey_leaf(fk, resolver, interner) {
                    changed = true;
                }
            }
        }
    }

    changed
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn repair_cloak_assoc_item(bytes: &mut [u8], resolver: &SlotResolver) -> bool {
    if read_u32(bytes, MGEF_ARCHETYPE_OFFSET) != Some(MGEF_ARCHETYPE_CLOAK) {
        return false;
    }
    let Some(raw) = read_u32(bytes, MGEF_ASSOC_ITEM_OFFSET) else {
        return false;
    };
    let replacement = match resolver.resolve_cloak_assoc_item(raw) {
        SlotResolution::Keep => return false,
        SlotResolution::Null => 0,
        SlotResolution::RepairToOutput => resolver.repair_raw(raw),
    };
    bytes[MGEF_ASSOC_ITEM_OFFSET..MGEF_ASSOC_ITEM_OFFSET + 4]
        .copy_from_slice(&replacement.to_le_bytes());
    true
}

fn zero_dangling_at(bytes: &mut [u8], offset: usize, resolver: &SlotResolver) -> bool {
    let Some(raw) = read_u32(bytes, offset) else {
        return false;
    };
    if resolver.resolve_null_slot(raw) == SlotResolution::Null {
        bytes[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
        true
    } else {
        false
    }
}

fn repair_at(bytes: &mut [u8], offset: usize, resolver: &SlotResolver) -> bool {
    let Some(raw) = read_u32(bytes, offset) else {
        return false;
    };
    if resolver.resolve_repair_slot(raw) == SlotResolution::RepairToOutput {
        let repaired = resolver.repair_raw(raw);
        bytes[offset..offset + 4].copy_from_slice(&repaired.to_le_bytes());
        true
    } else {
        false
    }
}

fn null_fvpa_components(bytes: &mut [u8], resolver: &SlotResolver) -> bool {
    if bytes.len() < COBJ_FVPA_ROW_SIZE || bytes.len() % COBJ_FVPA_ROW_SIZE != 0 {
        return false;
    }
    let mut changed = false;
    for row in bytes.chunks_exact_mut(COBJ_FVPA_ROW_SIZE) {
        let raw = u32::from_le_bytes([row[0], row[1], row[2], row[3]]);
        if resolver.resolve_null_slot(raw) == SlotResolution::Null {
            row[0..4].copy_from_slice(&0u32.to_le_bytes());
            changed = true;
        }
    }
    changed
}

/// Repair a master-byte-truncated SNDR.BNAM FormKey leaf to the output plugin.
fn repair_formkey_leaf(
    fk: &mut FormKey,
    resolver: &SlotResolver,
    interner: &StringInterner,
) -> bool {
    if fk.local == 0 {
        return false;
    }
    let object_id = fk.local & 0x00FF_FFFF;
    // The leaf is the truncation target only when it addresses the first target
    // master (master index 0 = Fallout4.esm).
    let Some(plugin_name) = interner.resolve(fk.plugin) else {
        return false;
    };
    let addresses_master_0 = resolver
        .first_master
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case(plugin_name));
    if !addresses_master_0 {
        return false;
    }
    if resolver.object_exists(0, object_id) {
        return false; // a real Fallout4.esm descriptor — keep
    }
    if !resolver.output_objids.contains(&object_id) {
        return false; // sentinel / dangling — leave untouched
    }
    let Some(output_plugin) = resolver.output_plugin.as_deref() else {
        return false;
    };
    fk.plugin = interner.intern(output_plugin);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use smallvec::SmallVec;

    fn resolver(output: &[u32], fo4: &[u32], masters_len: u32) -> SlotResolver {
        resolver_with_spels(output, &[], fo4, &[], masters_len)
    }

    fn resolver_with_spels(
        output: &[u32],
        output_spels: &[u32],
        fo4: &[u32],
        fo4_spels: &[u32],
        masters_len: u32,
    ) -> SlotResolver {
        let mut master_objids = Vec::new();
        master_objids.push(Arc::new(fo4.iter().copied().collect()));
        let mut master_spel_objids = Vec::new();
        master_spel_objids.push(Arc::new(fo4_spels.iter().copied().collect()));
        for _ in 1..masters_len {
            master_objids.push(Arc::new(FxHashSet::default()));
            master_spel_objids.push(Arc::new(FxHashSet::default()));
        }
        SlotResolver {
            output_objids: output.iter().copied().collect(),
            output_spel_objids: output_spels.iter().copied().collect(),
            master_objids,
            master_spel_objids,
            output_master_index: masters_len,
            first_master: Some("Fallout4.esm".to_string()),
            output_plugin: Some("Output.esm".to_string()),
        }
    }

    fn mgef_record(archetype: u32, assoc_item: u32, interner: &StringInterner) -> Record {
        let mut data = vec![0u8; MGEF_ARCHETYPE_OFFSET + 4];
        data[MGEF_ASSOC_ITEM_OFFSET..MGEF_ASSOC_ITEM_OFFSET + 4]
            .copy_from_slice(&assoc_item.to_le_bytes());
        data[MGEF_ARCHETYPE_OFFSET..MGEF_ARCHETYPE_OFFSET + 4]
            .copy_from_slice(&archetype.to_le_bytes());
        let mut record = Record::new(
            SigCode::from_str("MGEF").unwrap(),
            FormKey {
                local: 0x10F26C,
                plugin: interner.intern("Output.esm"),
            },
        );
        record.fields.push(crate::record::FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(data)),
        });
        record
    }

    fn mgef_assoc_item(record: &Record) -> u32 {
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("MGEF DATA must remain bytes");
        };
        read_u32(bytes, MGEF_ASSOC_ITEM_OFFSET).unwrap()
    }

    #[test]
    fn nulls_dangling_assoc_item() {
        let r = resolver(&[], &[0x001234], 7);
        // 0x0002FA14 resolves nowhere -> null
        assert_eq!(r.resolve_null_slot(0x0002_FA14), SlotResolution::Null);
        // 0x00001234 is a real Fallout4.esm record -> keep
        assert_eq!(r.resolve_null_slot(0x0000_1234), SlotResolution::Keep);
        // null stays null/keep
        assert_eq!(r.resolve_null_slot(0), SlotResolution::Keep);
    }

    #[test]
    fn repair_truncated_to_output() {
        // object-id 0x4FD271 exists in output, not in Fallout4.esm.
        let r = resolver(&[0x4FD271], &[0x001234], 7);
        assert_eq!(
            r.resolve_repair_slot(0x0004_FD271 & 0x00FF_FFFF),
            SlotResolution::RepairToOutput
        );
        assert_eq!(r.repair_raw(0x004F_D271), 0x074F_D271);
    }

    #[test]
    fn repair_keeps_valid_fo4_descriptor() {
        let r = resolver(&[0x4FD271], &[0x001234], 7);
        // 0x00001234 is a real Fallout4.esm record -> keep
        assert_eq!(r.resolve_repair_slot(0x0000_1234), SlotResolution::Keep);
    }

    #[test]
    fn repair_keeps_sentinel_not_in_output() {
        // 0x00800000 sentinel: not in FO4, not in output -> keep (never touched)
        let r = resolver(&[0x4FD271], &[0x001234], 7);
        assert_eq!(r.resolve_repair_slot(0x0080_0000), SlotResolution::Keep);
    }

    #[test]
    fn null_keeps_value_present_in_output() {
        // a dangling-looking value that actually exists in output -> keep
        let r = resolver(&[0x07_0000 & 0xFFFFFF], &[], 7);
        // build raw addressing output master index 7
        let raw = (7u32 << 24) | 0x070000;
        assert_eq!(r.resolve_null_slot(raw), SlotResolution::Keep);
    }

    #[test]
    fn cloak_assoc_exact_10f280_wrong_type_collision_targets_output_spell() {
        let r = resolver_with_spels(&[0x10F280], &[0x10F280], &[0x10F280], &[], 7);
        assert_eq!(
            r.resolve_cloak_assoc_item(0x0010_F280),
            SlotResolution::RepairToOutput
        );
        assert_eq!(r.repair_raw(0x0010_F280), 0x0710_F280);
    }

    #[test]
    fn cloak_assoc_keeps_valid_addressed_master_spell() {
        let interner = StringInterner::new();
        let r = resolver_with_spels(&[], &[], &[0x73E4], &[0x73E4], 7);
        let mut record = mgef_record(MGEF_ARCHETYPE_CLOAK, 0x0000_73E4, &interner);

        assert!(!apply_to_record(&mut record, &r, &interner));
        assert_eq!(mgef_assoc_item(&record), 0x0000_73E4);
    }

    #[test]
    fn cloak_assoc_repairs_wrong_type_master_collision_to_output_spell() {
        let interner = StringInterner::new();
        let r = resolver_with_spels(&[0x334455], &[0x334455], &[0x334455], &[], 7);
        let mut record = mgef_record(MGEF_ARCHETYPE_CLOAK, 0x0033_4455, &interner);

        assert!(apply_to_record(&mut record, &r, &interner));
        assert_eq!(mgef_assoc_item(&record), 0x0733_4455);
    }

    #[test]
    fn cloak_assoc_keeps_valid_output_spell() {
        let interner = StringInterner::new();
        let r = resolver_with_spels(&[0x334455], &[0x334455], &[], &[], 7);
        let mut record = mgef_record(MGEF_ARCHETYPE_CLOAK, 0x0733_4455, &interner);

        assert!(!apply_to_record(&mut record, &r, &interner));
        assert_eq!(mgef_assoc_item(&record), 0x0733_4455);
    }

    #[test]
    fn cloak_assoc_nulls_missing_spell_and_keeps_null() {
        let interner = StringInterner::new();
        let r = resolver_with_spels(&[], &[], &[], &[], 7);
        let mut missing = mgef_record(MGEF_ARCHETYPE_CLOAK, 0x0002_FA14, &interner);
        let mut null = mgef_record(MGEF_ARCHETYPE_CLOAK, 0, &interner);

        assert!(apply_to_record(&mut missing, &r, &interner));
        assert_eq!(mgef_assoc_item(&missing), 0);
        assert!(!apply_to_record(&mut null, &r, &interner));
        assert_eq!(mgef_assoc_item(&null), 0);
    }

    #[test]
    fn cloak_assoc_nulls_wrong_type_master_without_output_spell() {
        let interner = StringInterner::new();
        let r = resolver_with_spels(&[], &[], &[0x10F280], &[], 7);
        let mut record = mgef_record(MGEF_ARCHETYPE_CLOAK, 0x0010_F280, &interner);

        assert!(apply_to_record(&mut record, &r, &interner));
        assert_eq!(mgef_assoc_item(&record), 0);
    }

    #[test]
    fn non_cloak_assoc_wrong_type_collision_is_untouched() {
        let interner = StringInterner::new();
        let r = resolver_with_spels(&[0x10F280], &[0x10F280], &[0x10F280], &[], 7);
        let mut record = mgef_record(1, 0x0010_F280, &interner);

        assert!(!apply_to_record(&mut record, &r, &interner));
        assert_eq!(mgef_assoc_item(&record), 0x0010_F280);
    }

    #[test]
    fn cloak_assoc_repair_is_idempotent() {
        let interner = StringInterner::new();
        let r = resolver_with_spels(&[0x10F280], &[0x10F280], &[0x10F280], &[], 7);
        let mut record = mgef_record(MGEF_ARCHETYPE_CLOAK, 0x0010_F280, &interner);

        assert!(apply_to_record(&mut record, &r, &interner));
        assert!(!apply_to_record(&mut record, &r, &interner));
        assert_eq!(mgef_assoc_item(&record), 0x0710_F280);
    }
}
