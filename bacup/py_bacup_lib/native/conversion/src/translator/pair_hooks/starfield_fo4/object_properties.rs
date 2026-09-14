//! Starfield `PRPS` property rows for the FO4 layout.

use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::record::{FieldValue, Record};
use crate::translator::pair_hooks::fnv_mgef::{
    MgefNormalizeReport, mapped_raw_reference, read_u32,
};

/// Actor value, value, and a curve table FO4 has no slot for.
const STARFIELD_ROW_LEN: usize = 12;

/// Rows whose actor value has no FO4 EditorID match are dropped rather than
/// nulled, and an emptied list goes with them: most Starfield properties drive
/// scanner, outpost and stim systems FO4 lacks, and their raw ids are unrelated
/// FO4 forms.
pub(crate) fn normalize(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) {
    record.fields.retain_mut(|field| {
        if field.sig.0 != *b"PRPS" {
            return true;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            return true;
        };
        if bytes.len() % STARFIELD_ROW_LEN != 0 {
            report.unsupported_rows += 1;
            return false;
        }
        let mut rows = SmallVec::new();
        for row in bytes.chunks_exact(STARFIELD_ROW_LEN) {
            let actor_value = read_u32(row, 0);
            if let FieldValue::Bytes(target) =
                mapped_raw_reference("property_actor_value", actor_value, mapper, report)
            {
                rows.extend_from_slice(&target);
                rows.extend_from_slice(&row[4..8]);
            }
        }
        report.converted_rows += 1;
        if rows.is_empty() {
            return false;
        }
        field.value = FieldValue::Bytes(rows);
        true
    });
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::sym::StringInterner;

    const STRENGTH_TO_LUCK_AND_HEALTH: [u32; 8] = [0x2C2, 0x2C3, 0x2C4, 0x2C5, 0x2C6, 0x2C7, 0x2C8, 0x2D4];

    fn form_key(interner: &StringInterner, plugin: &str, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    /// Starfield and FO4 share these actor-value EditorIDs and object ids; the run
    /// pre-seeds them through the EditorID match, never through the shared id.
    fn mapper(interner: &StringInterner) -> FormKeyMapper<'_> {
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Starfield.esm".into(),
                source_plugin_name: "Starfield.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            interner,
        );
        for local in STRENGTH_TO_LUCK_AND_HEALTH {
            mapper.add_mapping(
                form_key(interner, "Starfield.esm", local),
                form_key(interner, "Fallout4.esm", local),
            );
        }
        mapper
    }

    fn record_with_prps(interner: &StringInterner, sig: &[u8; 4], local: u32, hex: &str) -> Record {
        let mut record = Record::new(SigCode(*sig), form_key(interner, "Starfield.esm", local));
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"PRPS"),
            value: FieldValue::Bytes(SmallVec::from_vec(hex::decode(hex).unwrap())),
        });
        record
    }

    fn prps(record: &Record) -> Option<Vec<u8>> {
        record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"PRPS")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => bytes.to_vec(),
                other => panic!("unexpected PRPS value: {other:?}"),
            })
    }

    #[test]
    fn class_special_rows_take_the_fo4_eight_byte_layout() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        // Shipped `Starfield.esm` CLAS 01326B `Citizen`.
        let mut record = record_with_prps(
            &interner,
            b"CLAS",
            0x1_326B,
            "c80200000000804000000000c70200000000804000000000c60200000000804000000000c50200000000804000000000c40200000000804000000000c30200000000804000000000c20200000000804000000000d40200000000000000000000",
        );
        let mut report = MgefNormalizeReport::default();

        normalize(&mut record, &mut mapper, &mut report);

        assert_eq!(
            prps(&record).unwrap(),
            hex::decode("c802000000008040c702000000008040c602000000008040c502000000008040c402000000008040c302000000008040c202000000008040d402000000000000").unwrap()
        );
        assert_eq!(report.converted_rows, 1);
    }

    #[test]
    fn starfield_only_actor_values_leave_no_property_list() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        // Shipped `Starfield.esm` FLOR 0242F8 `FloraBloodStoneTall`: HandScannerPlantReproduction,
        // whose object id is FO4's MrHandyArmDest02 art object.
        let mut record = record_with_prps(&interner, b"FLOR", 0x2_42F8, "05e923000000a04000000000");
        let mut report = MgefNormalizeReport::default();

        normalize(&mut record, &mut mapper, &mut report);

        assert_eq!(prps(&record), None);
    }

    #[test]
    fn only_rows_whose_actor_value_maps_survive() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        // Stim_Resist_Flame (Starfield-only), then `CollisionMarker`'s Health 100.
        let mut record = record_with_prps(
            &interner,
            b"STAT",
            0x21,
            "5ff21d000000c8c200000000d40200000000c84200000000",
        );
        let mut report = MgefNormalizeReport::default();

        normalize(&mut record, &mut mapper, &mut report);

        assert_eq!(prps(&record).unwrap(), hex::decode("d40200000000c842").unwrap());
    }

    #[test]
    fn a_payload_off_the_starfield_row_stride_is_dropped() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let mut record = record_with_prps(&interner, b"STAT", 0x21, "d40200000000c842");
        let mut report = MgefNormalizeReport::default();

        normalize(&mut record, &mut mapper, &mut report);

        assert_eq!(prps(&record), None);
        assert_eq!(report.unsupported_rows, 1);
    }
}
