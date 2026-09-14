use std::collections::BTreeSet;

use crate::record::{FieldValue, Record};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyKeySupport {
    pub reason_codes: Vec<String>,
}

impl LegacyKeySupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn classify_legacy_key(record: &Record) -> LegacyKeySupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "KEYM" {
        reasons.insert("legacy_key_wrong_signature".to_string());
    }

    let mut counts = [0usize; 8];
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
            _ => {
                reasons.insert("legacy_key_subrecord_shape_unverified".to_string());
            }
        }
    }

    if counts[..4] != [1, 1, 1, 1] || counts[4] > 1 || counts[5..] != [1, 1, 1] {
        reasons.insert("legacy_key_field_multiplicity_unverified".to_string());
    }
    if pickup_sounds != 1 || drop_sounds != 1 {
        reasons.insert("legacy_key_pickup_drop_sound_pair_unverified".to_string());
    }

    LegacyKeySupport {
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
            SigCode::from_str("KEYM").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let string = |value| FieldValue::String(interner.intern(value));
        let sound = FormKey {
            local: 0x900,
            plugin: interner.intern("FalloutNV.esm"),
        };
        record.fields.extend([
            field(*b"EDID", string("CreatureKey")),
            field(*b"OBND", FieldValue::Bytes(vec![0; 12].into())),
            field(*b"FULL", string("Creature Key")),
            field(*b"MODL", string("Clutter\\Key.nif")),
            field(*b"MODT", FieldValue::Bytes(vec![1; 72].into())),
            field(*b"ICON", string("interface\\icons\\key.dds")),
            field(*b"MICO", string("interface\\icons\\key_small.dds")),
            field(*b"YNAM", FieldValue::FormKey(sound)),
            field(*b"ZNAM", FieldValue::FormKey(sound)),
            field(*b"DATA", FieldValue::Bytes(vec![0; 8].into())),
        ]);
        record
    }

    #[test]
    fn exact_official_key_shape_is_schema_compatible() {
        let interner = StringInterner::new();
        assert!(classify_legacy_key(&exact_record(&interner)).is_ready());
    }

    #[test]
    fn a_missing_sound_or_shape_drift_is_not_admitted() {
        let interner = StringInterner::new();
        let mut record = exact_record(&interner);
        record.fields.retain(|field| field.sig.as_str() != "ZNAM");
        record
            .fields
            .push(field(*b"DATA", FieldValue::Bytes(vec![0; 8].into())));
        assert_eq!(
            classify_legacy_key(&record).reason_codes,
            [
                "legacy_key_field_multiplicity_unverified",
                "legacy_key_pickup_drop_sound_pair_unverified",
            ]
        );
    }
}
