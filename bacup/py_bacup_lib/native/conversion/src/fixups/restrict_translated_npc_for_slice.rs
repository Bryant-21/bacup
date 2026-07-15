//! Fixup: re-apply the cell-slice-only NPC template-inheritance strips.
//!
//! # Why this exists
//! The full-plugin path carries an NPC's inherited inventory (TPTA slot 8) and
//! its Object Template block (`OBTE`..`STOP`) so converted robots (Mr Handy,
//! etc.) render with their modular body parts and inherited gear. Stripping
//! them unconditionally leaves placed NPCs invisible in the shipped master.
//!
//! Cell-slice / bounded conversions still want the conservative behaviour: an
//! isolated record graph can't always expand inherited FO4 object-template
//! items without CK choking. This fixup restores that strip for the bounded
//! path only — it is `GraphOnly`, so the whole-plugin run skips it.
//!
//! For each `NPC_` it:
//! 1. Zeroes the `TPTA` inventory slot (slot 8, offset 32) and clears the
//!    matching `Inventory` (`0x0100`) bit in `ACBS.template_flags`.
//! 2. Drops the Object Template block — the contiguous `OBTE`..`STOP` run.
//!
//! `ACBS` codec `struct:I,h,H,H,H,h,H,H,B,B` — template_flags is the u16 at
//! offset 14. `TPTA` codec `struct:I×13` — 13 FormID slots, inventory is slot 8
//! (offset 32). Both decode to `FieldValue::Bytes` on the native path.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;

const TPTA_INVENTORY_SLOT_OFFSET: usize = 8 * 4;
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;
const NPC_TEMPLATE_INVENTORY: u16 = 0x0100;

pub struct RestrictTranslatedNpcForSliceFixup;

impl Fixup for RestrictTranslatedNpcForSliceFixup {
    fn name(&self) -> &'static str {
        "restrict_translated_npc_for_slice"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::GraphOnly
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let npc_fks = session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in &npc_fks {
            let mut record =
                match session.record_decoded(fk, target_schema.as_ref(), mapper.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("restrict_npc_slice_read:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };
            if apply_to_record(&mut record) {
                session
                    .replace_record(record, target_schema.as_ref(), mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                report.records_changed += 1;
            }
        }

        Ok(report)
    }
}

/// Apply both cell-slice strips. Returns `true` when the record changed.
pub fn apply_to_record(record: &mut Record) -> bool {
    let mut changed = false;
    changed |= strip_inherited_inventory(record);
    changed |= drop_object_template_block(record);
    changed
}

/// Zero the `TPTA` inventory slot and clear the `Inventory` template-flag bit.
fn strip_inherited_inventory(record: &mut Record) -> bool {
    let Ok(tpta_sig) = SubrecordSig::from_str("TPTA") else {
        return false;
    };
    let Ok(acbs_sig) = SubrecordSig::from_str("ACBS") else {
        return false;
    };

    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if entry.sig != tpta_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &mut entry.value {
            if let Some(slot) =
                data.get_mut(TPTA_INVENTORY_SLOT_OFFSET..TPTA_INVENTORY_SLOT_OFFSET + 4)
            {
                if slot.iter().any(|b| *b != 0) {
                    slot.fill(0);
                    changed = true;
                }
            }
        }
        break;
    }

    for entry in record.fields.iter_mut() {
        if entry.sig != acbs_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &mut entry.value {
            if data.len() >= ACBS_TEMPLATE_FLAGS_OFFSET + 2 {
                let mut flags = u16::from_le_bytes([
                    data[ACBS_TEMPLATE_FLAGS_OFFSET],
                    data[ACBS_TEMPLATE_FLAGS_OFFSET + 1],
                ]);
                if flags & NPC_TEMPLATE_INVENTORY != 0 {
                    flags &= !NPC_TEMPLATE_INVENTORY;
                    data[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                        .copy_from_slice(&flags.to_le_bytes());
                    changed = true;
                }
            }
        }
        break;
    }

    changed
}

/// Drop the contiguous `OBTE`..`STOP` Object Template block. The block is a
/// single scope run in FO4 schema order and the NPC's name `FULL` sits after
/// `STOP`, so draining the inclusive range leaves the name intact.
fn drop_object_template_block(record: &mut Record) -> bool {
    let (Ok(obte_sig), Ok(stop_sig)) = (
        SubrecordSig::from_str("OBTE"),
        SubrecordSig::from_str("STOP"),
    ) else {
        return false;
    };

    let Some(start) = record.fields.iter().position(|e| e.sig == obte_sig) else {
        return false;
    };
    let Some(end) = record
        .fields
        .iter()
        .rposition(|e| e.sig == stop_sig)
        .filter(|end| *end >= start)
    else {
        return false;
    };

    record.fields.drain(start..=end);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FormKey;
    use crate::record::{FieldEntry, RecordFlags};
    use crate::sym::StringInterner;

    const NPC_TEMPLATE_AI_PACKAGES: u16 = 0x0020;
    const NPC_TEMPLATE_SCRIPT: u16 = 0x0200;

    fn npc(interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("NPC_").unwrap(),
            form_key: FormKey {
                local: 0x000800,
                plugin: interner.intern("Output.esp"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn bytes(data: Vec<u8>) -> FieldValue {
        let mut buf: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        buf.extend_from_slice(&data);
        FieldValue::Bytes(buf)
    }

    fn make_acbs(template_flags: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 20];
        buf[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
            .copy_from_slice(&template_flags.to_le_bytes());
        buf
    }

    fn make_tpta(slots: [u32; 13]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(13 * 4);
        for slot in slots {
            buf.extend_from_slice(&slot.to_le_bytes());
        }
        buf
    }

    fn first_bytes<'a>(record: &'a Record, sig: &str) -> Option<&'a [u8]> {
        let s = SubrecordSig::from_str(sig).unwrap();
        record.fields.iter().find(|e| e.sig == s).and_then(|e| {
            if let FieldValue::Bytes(b) = &e.value {
                Some(b.as_slice())
            } else {
                None
            }
        })
    }

    fn count_sig(record: &Record, sig: &str) -> usize {
        let s = SubrecordSig::from_str(sig).unwrap();
        record.fields.iter().filter(|e| e.sig == s).count()
    }

    #[test]
    fn zeroes_tpta_inventory_slot_and_clears_template_flag() {
        let interner = StringInterner::new();
        let mut record = npc(&interner);
        let mut slots = [0u32; 13];
        slots[5] = 0x00_157C5F; // ai_packages
        slots[8] = 0x00_157C5F; // inventory
        slots[9] = 0x00_157C5F; // script
        push(
            &mut record,
            "ACBS",
            bytes(make_acbs(
                NPC_TEMPLATE_AI_PACKAGES | NPC_TEMPLATE_INVENTORY | NPC_TEMPLATE_SCRIPT,
            )),
        );
        push(&mut record, "TPTA", bytes(make_tpta(slots)));

        assert!(apply_to_record(&mut record));

        let tpta = first_bytes(&record, "TPTA").unwrap();
        let inv = u32::from_le_bytes(
            tpta[TPTA_INVENTORY_SLOT_OFFSET..TPTA_INVENTORY_SLOT_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(inv, 0, "inventory slot must be zeroed");
        // Other slots untouched.
        assert_eq!(
            u32::from_le_bytes(tpta[20..24].try_into().unwrap()),
            0x00_157C5F
        );
        assert_eq!(
            u32::from_le_bytes(tpta[36..40].try_into().unwrap()),
            0x00_157C5F
        );

        let acbs = first_bytes(&record, "ACBS").unwrap();
        let flags = u16::from_le_bytes(acbs[14..16].try_into().unwrap());
        assert_eq!(flags, NPC_TEMPLATE_AI_PACKAGES | NPC_TEMPLATE_SCRIPT);
    }

    #[test]
    fn drops_object_template_block_keeping_name_full() {
        let interner = StringInterner::new();
        let mut record = npc(&interner);
        push(&mut record, "EDID", bytes(b"Friedrich\0".to_vec()));
        // Object template block: OBTE..STOP, with a combination-name FULL inside.
        push(&mut record, "OBTE", bytes(1u32.to_le_bytes().to_vec()));
        push(&mut record, "OBTF", bytes(vec![]));
        push(&mut record, "FULL", bytes(b"Default\0".to_vec()));
        push(&mut record, "OBTS", bytes(vec![0u8; 16]));
        push(&mut record, "STOP", bytes(vec![]));
        // NPC name FULL comes after STOP in FO4 schema order.
        push(&mut record, "FULL", bytes(b"Friedrich\0".to_vec()));

        assert!(apply_to_record(&mut record));

        assert_eq!(count_sig(&record, "OBTE"), 0);
        assert_eq!(count_sig(&record, "OBTF"), 0);
        assert_eq!(count_sig(&record, "OBTS"), 0);
        assert_eq!(count_sig(&record, "STOP"), 0);
        // The combination FULL inside the block is gone; the NPC name FULL stays.
        assert_eq!(count_sig(&record, "FULL"), 1);
        let name = first_bytes(&record, "FULL").unwrap();
        assert_eq!(name, b"Friedrich\0");
        assert_eq!(count_sig(&record, "EDID"), 1);
    }

    #[test]
    fn noop_when_no_inventory_or_object_template() {
        let interner = StringInterner::new();
        let mut record = npc(&interner);
        let mut slots = [0u32; 13];
        slots[9] = 0x00_157C5F; // script only — no inventory
        push(&mut record, "ACBS", bytes(make_acbs(NPC_TEMPLATE_SCRIPT)));
        push(&mut record, "TPTA", bytes(make_tpta(slots)));

        assert!(!apply_to_record(&mut record));
    }
}
