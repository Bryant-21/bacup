//! Let the player fast travel out of converted FO76 interior cells.
//!
//! FO4 reads CELL.DATA 0x0004 inverted for interiors: on an exterior the bit
//! blocks fast travel, but an interior blocks fast travel unless the bit is set.
//! FO76 interior cells arrive with the bit clear, which leaves every converted
//! interior reporting "You cannot fast travel from this location."

use crate::fixups::mark_public_wastelanders_hubs::set_cell_data_flag;
use crate::record::Record;
use crate::sym::StringInterner;

const FO4_INTERIOR_CAN_TRAVEL_FROM_FLAG: u16 = 0x0004;

/// Returns true only when this invocation changes the DATA flags.
pub(crate) fn allow_fast_travel_from_interior(
    target_cell: &mut Record,
    interner: &StringInterner,
) -> bool {
    target_cell
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"DATA")
        .is_some_and(|entry| {
            set_cell_data_flag(
                &mut entry.value,
                FO4_INTERIOR_CAN_TRAVEL_FROM_FLAG,
                interner,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};

    fn interior_cell(interner: &StringInterner, flags: u16) -> Record {
        let plugin = interner.intern("SeventySix.esm");
        let mut record = Record::new(
            SigCode(*b"CELL"),
            FormKey {
                local: 0x0000_0800,
                plugin,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Uint(u64::from(flags)),
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

    #[test]
    fn sets_can_travel_from_bit_and_preserves_other_flags() {
        let interner = StringInterner::new();
        let mut cell = interior_cell(&interner, 0x0001 | 0x0020 | 0x0400);

        assert!(allow_fast_travel_from_interior(&mut cell, &interner));
        assert_eq!(data_flags(&cell), 0x0001 | 0x0004 | 0x0020 | 0x0400);
    }

    #[test]
    fn already_travelable_interior_is_unchanged() {
        let interner = StringInterner::new();
        let mut cell = interior_cell(&interner, 0x0001 | FO4_INTERIOR_CAN_TRAVEL_FROM_FLAG);

        assert!(!allow_fast_travel_from_interior(&mut cell, &interner));
        assert_eq!(data_flags(&cell), 0x0005);
    }
}
