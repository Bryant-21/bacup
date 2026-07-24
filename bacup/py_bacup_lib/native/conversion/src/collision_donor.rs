use std::collections::HashMap;

use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const TARGET_RUNTIME_FALLBACK_SUBRECORDS: [[u8; 4]; 1] = [*b"PRPS"];

pub(crate) fn merge_target_collision_donor(
    record: &mut Record,
    donor: &Record,
    interner: &StringInterner,
) {
    if record.sig != donor.sig {
        return;
    }

    record.flags = donor.flags;
    replace_vmad(record, donor);
    merge_nested_flags(record, donor, interner);

    for signature in TARGET_RUNTIME_FALLBACK_SUBRECORDS {
        if record.fields.iter().any(|field| field.sig.0 == signature) {
            continue;
        }
        let Some((donor_index, field)) = donor
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.sig.0 == signature)
        else {
            continue;
        };
        insert_at_donor_position(record, donor, donor_index, field.clone());
    }
}

fn replace_vmad(record: &mut Record, donor: &Record) {
    let previous_position = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"VMAD");
    record.fields.retain(|field| field.sig.0 != *b"VMAD");

    let Some((donor_index, donor_vmad)) = donor
        .fields
        .iter()
        .enumerate()
        .find(|(_, field)| field.sig.0 == *b"VMAD")
    else {
        return;
    };

    if let Some(position) = previous_position {
        record
            .fields
            .insert(position.min(record.fields.len()), donor_vmad.clone());
    } else {
        insert_at_donor_position(record, donor, donor_index, donor_vmad.clone());
    }
}

fn insert_at_donor_position(
    record: &mut Record,
    donor: &Record,
    donor_index: usize,
    field: FieldEntry,
) {
    for neighbor in donor.fields[..donor_index].iter().rev() {
        if let Some(position) = record
            .fields
            .iter()
            .rposition(|candidate| candidate.sig == neighbor.sig)
        {
            record.fields.insert(position + 1, field);
            return;
        }
    }
    for neighbor in donor.fields[donor_index + 1..].iter() {
        if let Some(position) = record
            .fields
            .iter()
            .position(|candidate| candidate.sig == neighbor.sig)
        {
            record.fields.insert(position, field);
            return;
        }
    }
    record.fields.push(field);
}

fn merge_nested_flags(record: &mut Record, donor: &Record, interner: &StringInterner) {
    let mut occurrences: HashMap<[u8; 4], usize> = HashMap::new();
    for field in record.fields.iter_mut() {
        let occurrence = occurrences.entry(field.sig.0).or_default();
        let donor_field = donor
            .fields
            .iter()
            .filter(|candidate| candidate.sig == field.sig)
            .nth(*occurrence);
        *occurrence += 1;
        if let Some(donor_field) = donor_field {
            merge_flag_values(&mut field.value, &donor_field.value, interner);
        }
    }
}

fn merge_flag_values(value: &mut FieldValue, donor: &FieldValue, interner: &StringInterner) {
    match (value, donor) {
        (FieldValue::Struct(fields), FieldValue::Struct(donor_fields)) => {
            for (donor_key, donor_value) in donor_fields {
                if is_flag_field(*donor_key, interner) {
                    if let Some((_, value)) = fields.iter_mut().find(|(key, _)| key == donor_key) {
                        *value = donor_value.clone();
                    } else {
                        fields.push((*donor_key, donor_value.clone()));
                    }
                    continue;
                }
                if let Some((_, value)) = fields.iter_mut().find(|(key, _)| key == donor_key) {
                    merge_flag_values(value, donor_value, interner);
                }
            }
        }
        (FieldValue::List(values), FieldValue::List(donor_values)) => {
            for (value, donor_value) in values.iter_mut().zip(donor_values) {
                merge_flag_values(value, donor_value, interner);
            }
        }
        _ => {}
    }
}

fn is_flag_field(key: crate::sym::Sym, interner: &StringInterner) -> bool {
    let Some(name) = interner.resolve(key) else {
        return false;
    };
    let normalized: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    normalized.ends_with("flag") || normalized.ends_with("flags")
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::RecordFlags;

    fn record(interner: &StringInterner, editor_id: &str) -> Record {
        let mut record = Record::new(
            SigCode::from_str("MSTT").unwrap(),
            FormKey::parse("000800@Output.esm", interner).unwrap(),
        );
        record.eid = Some(interner.intern(editor_id));
        record
    }

    fn field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    #[test]
    fn fusion_generator_uses_fo4_vmad_and_keeps_source_model() {
        let interner = StringInterner::new();
        let mut converted = record(&interner, "PowerGenerator01fo76");
        converted.fields.push(field(
            "VMAD",
            FieldValue::Bytes(SmallVec::from_slice(b"fo76-script-contract")),
        ));
        converted.fields.push(field(
            "MODL",
            FieldValue::String(interner.intern("FO76\\PowerGenerator01.nif")),
        ));

        let mut donor = record(&interner, "PowerGenerator01");
        donor.fields.push(field(
            "VMAD",
            FieldValue::Bytes(SmallVec::from_slice(b"fo4-script-contract")),
        ));
        donor.fields.push(field(
            "MODL",
            FieldValue::String(interner.intern("SetDressing\\PowerGenerator01.nif")),
        ));
        donor
            .fields
            .push(field("PRPS", FieldValue::List(vec![FieldValue::Uint(7)])));

        merge_target_collision_donor(&mut converted, &donor, &interner);

        assert_eq!(
            converted
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"VMAD")
                .map(|field| &field.value),
            Some(&FieldValue::Bytes(SmallVec::from_slice(
                b"fo4-script-contract"
            )))
        );
        assert_eq!(
            converted
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"MODL")
                .map(|field| &field.value),
            Some(&FieldValue::String(
                interner.intern("FO76\\PowerGenerator01.nif")
            ))
        );
        assert!(converted.fields.iter().any(|field| field.sig.0 == *b"PRPS"));
    }

    #[test]
    fn car_inherits_fo4_destructible_flags_without_replacing_damage_model() {
        let interner = StringInterner::new();
        let header = interner.intern("Header");
        let health = interner.intern("Health");
        let flags = interner.intern("Flags");
        let damage_model = interner.intern("DMDL");

        let mut converted = record(&interner, "EngineCarDestructible01fo76");
        converted.flags = RecordFlags::from_bits_retain(0x10);
        converted.fields.push(field(
            "DEST",
            FieldValue::List(vec![FieldValue::Struct(vec![
                (
                    header,
                    FieldValue::Struct(vec![(health, FieldValue::Uint(200))]),
                ),
                (
                    damage_model,
                    FieldValue::String(interner.intern("FO76\\EngineCar01Hulk.nif")),
                ),
            ])]),
        ));

        let mut donor = record(&interner, "EngineCarDestructible01");
        donor.flags = RecordFlags::from_bits_retain(0x20);
        donor.fields.push(field(
            "DEST",
            FieldValue::List(vec![FieldValue::Struct(vec![
                (
                    header,
                    FieldValue::Struct(vec![
                        (health, FieldValue::Uint(200)),
                        (flags, FieldValue::Uint(1)),
                    ]),
                ),
                (
                    damage_model,
                    FieldValue::String(interner.intern("FO4\\EngineCar01Hulk.nif")),
                ),
            ])]),
        ));

        merge_target_collision_donor(&mut converted, &donor, &interner);

        assert_eq!(converted.flags, donor.flags);
        let FieldValue::List(rows) = &converted.fields[0].value else {
            panic!("DEST must remain a row list");
        };
        let FieldValue::Struct(row) = &rows[0] else {
            panic!("DEST row must remain structured");
        };
        let FieldValue::Struct(header_fields) =
            &row.iter().find(|(key, _)| *key == header).unwrap().1
        else {
            panic!("DEST header must remain structured");
        };
        assert_eq!(
            header_fields
                .iter()
                .find(|(key, _)| *key == flags)
                .map(|(_, value)| value),
            Some(&FieldValue::Uint(1))
        );
        assert_eq!(
            row.iter()
                .find(|(key, _)| *key == damage_model)
                .map(|(_, value)| value),
            Some(&FieldValue::String(
                interner.intern("FO76\\EngineCar01Hulk.nif")
            ))
        );
    }

    #[test]
    fn donor_without_vmad_removes_fo76_only_binding() {
        let interner = StringInterner::new();
        let mut converted = record(&interner, "NoTargetScriptfo76");
        converted.fields.push(field(
            "VMAD",
            FieldValue::Bytes(SmallVec::from_slice(b"fo76-only")),
        ));
        let donor = record(&interner, "NoTargetScript");

        merge_target_collision_donor(&mut converted, &donor, &interner);

        assert!(converted.fields.iter().all(|field| field.sig.0 != *b"VMAD"));
    }
}
