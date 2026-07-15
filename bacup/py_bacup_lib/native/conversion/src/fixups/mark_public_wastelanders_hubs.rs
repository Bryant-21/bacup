//! Mark the three Wastelanders social hubs as public FO4 interior cells.
//!
//! Fallout 76 relies on online faction/runtime access rules for these cells.
//! In Fallout 4, a faction-owned interior without CELL.DATA `Public Area`
//! (0x0020) treats a non-member player as trespassing.  Match the source local
//! object id before mapper relocation, set only the FO4-supported public bit,
//! and deliberately leave CELL.XOWN intact.

use crate::ids::SigCode;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

const CELL_OBJECT_ID_MASK: u32 = 0x00FF_FFFF;
const FO4_CELL_PUBLIC_AREA_FLAG: u16 = 0x0020;

/// Source-local CELL object ids for the public Wastelanders hubs.
pub(crate) const WASTELANDERS_PUBLIC_HUB_CELL_LOCAL_IDS: [u32; 3] = [
    0x0040_41F2, // The Wayward
    0x0040_A2C1, // Crater Core
    0x003F_880F, // Foundation Interior
];

/// Set FO4's CELL.DATA Public Area flag for an allowlisted source cell.
///
/// Returns true only when this invocation changes the DATA flags.  The helper
/// intentionally has no access to XOWN, so ownership cannot be cleared or
/// replaced as a side effect.
pub(crate) fn mark_wastelanders_public_hub(
    source_local: u32,
    target_cell: &mut Record,
    interner: &StringInterner,
) -> bool {
    if target_cell.sig.0 != *b"CELL"
        || !WASTELANDERS_PUBLIC_HUB_CELL_LOCAL_IDS.contains(&(source_local & CELL_OBJECT_ID_MASK))
    {
        return false;
    }

    target_cell
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"DATA")
        .is_some_and(|entry| set_public_area_flag(&mut entry.value, interner))
}

fn set_public_area_flag(value: &mut FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(flags) => {
            let Ok(flags16) = u16::try_from(*flags) else {
                return false;
            };
            let updated = flags16 | FO4_CELL_PUBLIC_AREA_FLAG;
            if updated == flags16 {
                return false;
            }
            *flags = u64::from(updated);
            true
        }
        FieldValue::Int(flags) => {
            let Ok(flags16) = u16::try_from(*flags) else {
                return false;
            };
            let updated = flags16 | FO4_CELL_PUBLIC_AREA_FLAG;
            if updated == flags16 {
                return false;
            }
            *flags = i64::from(updated);
            true
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            let flags = u16::from_le_bytes([bytes[0], bytes[1]]);
            let updated = flags | FO4_CELL_PUBLIC_AREA_FLAG;
            if updated == flags {
                return false;
            }
            bytes[0..2].copy_from_slice(&updated.to_le_bytes());
            true
        }
        FieldValue::Struct(fields) => fields
            .iter_mut()
            .find(|(name, _)| {
                interner
                    .resolve(*name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("flags"))
            })
            .is_some_and(|(_, flags)| set_public_area_flag(flags, interner)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::FieldEntry;
    use smallvec::SmallVec;

    fn cell(interner: &StringInterner, local: u32, flags: u16, owner: u32) -> Record {
        let plugin = interner.intern("SeventySix.esm");
        let mut record = Record::new(SigCode(*b"CELL"), FormKey { local, plugin });
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Uint(u64::from(flags)),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"XOWN"),
            value: FieldValue::Bytes(SmallVec::from_slice(&owner.to_le_bytes())),
        });
        record
    }

    fn data_flags(record: &Record) -> u16 {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"DATA")
            .and_then(|entry| match entry.value {
                FieldValue::Uint(flags) => u16::try_from(flags).ok(),
                _ => None,
            })
            .expect("typed CELL.DATA flags")
    }

    fn owner_value(record: &Record) -> FieldValue {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"XOWN")
            .map(|entry| entry.value.clone())
            .expect("CELL.XOWN")
    }

    #[test]
    fn changes_exactly_the_three_allowlisted_cells_and_preserves_owners() {
        let interner = StringInterner::new();
        let locals = [
            WASTELANDERS_PUBLIC_HUB_CELL_LOCAL_IDS[0],
            WASTELANDERS_PUBLIC_HUB_CELL_LOCAL_IDS[1],
            WASTELANDERS_PUBLIC_HUB_CELL_LOCAL_IDS[2],
            0x0040_41F3,
        ];
        let mut cells: Vec<_> = locals
            .iter()
            .enumerate()
            .map(|(index, local)| cell(&interner, *local, 0x0001, 0x0001_1000 + index as u32))
            .collect();
        let owners_before: Vec<_> = cells.iter().map(owner_value).collect();

        let changed = cells
            .iter_mut()
            .zip(locals)
            .map(|(cell, local)| mark_wastelanders_public_hub(local, cell, &interner))
            .filter(|changed| *changed)
            .count();

        assert_eq!(changed, 3);
        for cell in &cells[..3] {
            assert_eq!(data_flags(cell), 0x0001 | FO4_CELL_PUBLIC_AREA_FLAG);
        }
        assert_eq!(data_flags(&cells[3]), 0x0001);
        assert_eq!(
            cells.iter().map(owner_value).collect::<Vec<_>>(),
            owners_before,
            "setting Public Area must not clear or rewrite CELL.XOWN"
        );
    }

    #[test]
    fn source_id_match_is_independent_of_target_form_id_and_idempotent() {
        let interner = StringInterner::new();
        let source_local = WASTELANDERS_PUBLIC_HUB_CELL_LOCAL_IDS[0];
        let mut relocated_target = cell(&interner, 0x0000_0800, 0x0001, 0x0001_1000);

        assert!(mark_wastelanders_public_hub(
            source_local,
            &mut relocated_target,
            &interner
        ));
        assert!(!mark_wastelanders_public_hub(
            source_local,
            &mut relocated_target,
            &interner
        ));
        assert_eq!(data_flags(&relocated_target), 0x0021);
    }
}
