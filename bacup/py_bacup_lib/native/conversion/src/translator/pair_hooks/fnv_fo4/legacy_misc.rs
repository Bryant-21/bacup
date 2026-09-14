use std::collections::BTreeSet;

use crate::record::{FieldValue, Record};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyMiscSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyMiscSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn classify_legacy_misc(record: &Record) -> LegacyMiscSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "MISC" {
        reasons.insert("legacy_misc_wrong_signature".to_string());
    }

    let mut counts = [0usize; 9];
    let mut pickup_sounds = 0usize;
    let mut drop_sounds = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => counts[0] += 1,
            "OBND" if matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 12) => {
                counts[1] += 1
            }
            "FULL" if matches!(field.value, FieldValue::String(_)) => counts[2] += 1,
            "MODL" if matches!(field.value, FieldValue::String(_)) => counts[3] += 1,
            "MODT" if matches!(field.value, FieldValue::Bytes(_)) => counts[4] += 1,
            "ICON" if matches!(field.value, FieldValue::String(_)) => counts[5] += 1,
            "MICO" if matches!(field.value, FieldValue::String(_)) => counts[6] += 1,
            "YNAM" if matches!(field.value, FieldValue::FormKey(_)) => pickup_sounds += 1,
            "ZNAM" if matches!(field.value, FieldValue::FormKey(_)) => drop_sounds += 1,
            "DATA" if matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 8) => {
                counts[7] += 1
            }
            "SCRI" if matches!(field.value, FieldValue::FormKey(_)) => {
                counts[8] += 1;
                reasons.insert("legacy_misc_script_requires_nonportable_scpt".to_string());
            }
            _ => {
                reasons.insert("legacy_misc_subrecord_shape_unverified".to_string());
            }
        }
    }

    if counts[0] != 1
        || counts[1] != 1
        || counts[2] != 1
        || counts[3] != 1
        || counts[4] > 1
        || counts[5] != 1
        || counts[6] > 1
        || counts[7] != 1
        || counts[8] > 1
    {
        reasons.insert("legacy_misc_field_multiplicity_unverified".to_string());
    }
    if pickup_sounds != drop_sounds || pickup_sounds > 1 {
        reasons.insert("legacy_misc_pickup_drop_sound_pair_unverified".to_string());
    }

    LegacyMiscSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;

    use super::*;

    fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(sig),
            value,
        }
    }

    fn exact_record(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("MISC").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let sound = FormKey {
            local: 0x900,
            plugin: interner.intern("FalloutNV.esm"),
        };
        record.fields.extend([
            field(*b"EDID", FieldValue::String(interner.intern("AmmoPart"))),
            field(*b"OBND", FieldValue::Bytes(vec![0; 12].into())),
            field(*b"FULL", FieldValue::String(interner.intern("Ammo Part"))),
            field(
                *b"MODL",
                FieldValue::String(interner.intern("Clutter\\Junk\\AmmoPart.nif")),
            ),
            field(*b"MODT", FieldValue::Bytes(vec![1; 72].into())),
            field(
                *b"ICON",
                FieldValue::String(interner.intern("interface\\icons\\ammo_part.dds")),
            ),
            field(*b"YNAM", FieldValue::FormKey(sound)),
            field(*b"ZNAM", FieldValue::FormKey(sound)),
            field(*b"DATA", FieldValue::Bytes(vec![0; 8].into())),
        ]);
        record
    }

    #[test]
    fn exact_script_free_misc_shape_is_schema_compatible() {
        let interner = StringInterner::new();
        assert!(classify_legacy_misc(&exact_record(&interner)).is_ready());
    }

    #[test]
    fn scripts_and_shape_drift_remain_typed_blockers() {
        let interner = StringInterner::new();
        let mut scripted = exact_record(&interner);
        scripted.fields.push(field(
            *b"SCRI",
            FieldValue::FormKey(FormKey {
                local: 0xa00,
                plugin: interner.intern("FalloutNV.esm"),
            }),
        ));
        assert_eq!(
            classify_legacy_misc(&scripted).reason_codes,
            ["legacy_misc_script_requires_nonportable_scpt"]
        );

        let mut malformed = exact_record(&interner);
        malformed
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value = FieldValue::Bytes(vec![0; 4].into());
        assert_eq!(
            classify_legacy_misc(&malformed).reason_codes,
            [
                "legacy_misc_field_multiplicity_unverified",
                "legacy_misc_subrecord_shape_unverified",
            ]
        );
    }
}
