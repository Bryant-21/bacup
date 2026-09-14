use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

use super::legacy_ammo::source_form_key_for_raw_form_id;

const REPRESENTABLE_FIELDS: &[[u8; 4]] = &[
    *b"EDID", *b"OBND", *b"LVLD", *b"LVLF", *b"LVLG", *b"LVLO", *b"COED",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyLvlcLoweringSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyLvlcLoweringSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn classify_legacy_lvlc(record: &Record) -> LegacyLvlcLoweringSupport {
    if record.sig.as_str() != "LVLC" {
        return LegacyLvlcLoweringSupport {
            reason_codes: vec!["not_legacy_lvlc".to_string()],
        };
    }
    let mut reason_codes = Vec::new();
    if record
        .fields
        .iter()
        .any(|field| !REPRESENTABLE_FIELDS.contains(&field.sig.0))
    {
        reason_codes.push("legacy_lvlc_field_not_representable_in_fo4_lvln".to_string());
    }
    let entries = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"LVLO")
        .collect::<Vec<_>>();
    if entries.is_empty() {
        reason_codes.push("legacy_lvlc_has_no_entries".to_string());
    }
    if entries.len() > u8::MAX as usize {
        reason_codes.push("legacy_lvlc_entry_count_exceeds_fo4_llct".to_string());
    }
    if entries
        .iter()
        .any(|entry| !representable_lvlc_entry(&entry.value))
    {
        reason_codes.push("legacy_lvlc_entry_layout_unrepresentable".to_string());
    }
    reason_codes.sort();
    reason_codes.dedup();
    LegacyLvlcLoweringSupport { reason_codes }
}

pub(crate) fn decode_legacy_lvlc_entries(
    record: &mut Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<(), String> {
    if record.sig.as_str() != "LVLC" {
        return Ok(());
    }
    for field in &mut record.fields {
        if field.sig.as_str() != "LVLO" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        if bytes.len() != 12 {
            return Err("legacy_lvlc_invalid_lvlo".to_string());
        }
        let raw_creature = u32::from_le_bytes(
            bytes[4..8]
                .try_into()
                .expect("legacy LVLC LVLO length checked"),
        );
        let creature = source_form_key_for_raw_form_id(
            raw_creature,
            source_plugin_name,
            source_master_names,
            interner,
        )
        .ok_or_else(|| format!("legacy_lvlc_unresolved_entry_formid_{raw_creature:08x}"))?;
        field.value = FieldValue::Struct(vec![
            (
                interner.intern("level"),
                FieldValue::Bytes(bytes[0..2].into()),
            ),
            (
                interner.intern("unknown_u8_1"),
                FieldValue::Bytes(bytes[2..3].into()),
            ),
            (
                interner.intern("unknown_u8_2"),
                FieldValue::Bytes(bytes[3..4].into()),
            ),
            (interner.intern("creature"), FieldValue::FormKey(creature)),
            (
                interner.intern("count"),
                FieldValue::Bytes(bytes[8..10].into()),
            ),
            (
                interner.intern("unknown_u8_5"),
                FieldValue::Bytes(bytes[10..11].into()),
            ),
            (
                interner.intern("unknown_u8_6"),
                FieldValue::Bytes(bytes[11..12].into()),
            ),
        ]);
    }
    Ok(())
}

pub(crate) fn lower_legacy_lvlc_to_lvln(
    record: &mut Record,
    interner: &StringInterner,
) -> Result<(), String> {
    let support = classify_legacy_lvlc(record);
    if !support.is_ready() {
        return Err(support.reason_codes.join(","));
    }
    for field in &mut record.fields {
        if field.sig.0 == *b"LVLO" {
            lower_entry(&mut field.value, interner)?;
        }
    }
    let count = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"LVLO")
        .count() as u64;
    let insert_at = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"LVLO")
        .expect("a classified LVLC has an LVLO entry");
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: SubrecordSig(*b"LLCT"),
            value: FieldValue::Uint(count),
        },
    );
    record.sig = SigCode::from_str("LVLN").expect("static signature");
    Ok(())
}

fn representable_lvlc_entry(value: &FieldValue) -> bool {
    match value {
        FieldValue::Bytes(bytes) => bytes.len() == 12,
        FieldValue::Struct(fields) => {
            fields.len() == 7
                && fields
                    .iter()
                    .any(|(_, value)| matches!(value, FieldValue::FormKey(_)))
        }
        _ => false,
    }
}

fn lower_entry(value: &mut FieldValue, interner: &StringInterner) -> Result<(), String> {
    let FieldValue::Struct(fields) = value else {
        return Ok(());
    };
    let creature = interner.intern("creature");
    let npc = interner.intern("npc");
    let source_chance = interner.intern("unknown_u8_5");
    let target_chance = interner.intern("chance_none");
    let mut creature_count = 0;
    for (name, value) in fields {
        if *name == creature {
            if !matches!(value, FieldValue::FormKey(_)) {
                return Err("legacy LVLC creature entry is not a FormKey".to_string());
            }
            *name = npc;
            creature_count += 1;
        } else if *name == source_chance {
            *name = target_chance;
        }
    }
    if creature_count != 1 {
        return Err("legacy LVLC entry does not contain exactly one creature field".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use crate::ids::FormKey;

    use super::*;

    fn source_lvlc(interner: &StringInterner) -> Record {
        let plugin = interner.intern("FalloutNV.esm");
        let mut record = Record::new(
            SigCode::from_str("LVLC").unwrap(),
            FormKey {
                local: 0x100,
                plugin,
            },
        );
        record.fields = smallvec![
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("CreatureList")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"LVLO"),
                value: FieldValue::Struct(vec![
                    (interner.intern("level"), FieldValue::Uint(5)),
                    (interner.intern("unknown_u8_1"), FieldValue::Uint(0)),
                    (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
                    (
                        interner.intern("creature"),
                        FieldValue::FormKey(FormKey {
                            local: 0x200,
                            plugin,
                        }),
                    ),
                    (interner.intern("count"), FieldValue::Uint(1)),
                    (interner.intern("unknown_u8_5"), FieldValue::Uint(0)),
                    (interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
                ]),
            },
        ];
        record
    }

    #[test]
    fn decodes_raw_lvlc_entries_with_source_load_order_identity() {
        let interner = StringInterner::new();
        let mut record = source_lvlc(&interner);
        record.fields[1].value = FieldValue::Bytes(
            [7, 0, 1, 2, 0x34, 0x12, 0x00, 0x01, 3, 0, 4, 5]
                .as_slice()
                .into(),
        );

        decode_legacy_lvlc_entries(
            &mut record,
            "Owner.esp",
            &["FalloutNV.esm".to_string()],
            &interner,
        )
        .unwrap();

        let FieldValue::Struct(fields) = &record.fields[1].value else {
            panic!("LVLO was not decoded");
        };
        assert!(fields.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("creature")
                && matches!(value, FieldValue::FormKey(key) if key.local == 0x1234 && interner.resolve(key.plugin) == Some("Owner.esp"))
        }));
    }

    #[test]
    fn lowers_structured_lvlc_to_fo4_lvln_without_changing_entry_identity() {
        let interner = StringInterner::new();
        let mut record = source_lvlc(&interner);
        lower_legacy_lvlc_to_lvln(&mut record, &interner).unwrap();
        assert_eq!(record.sig.as_str(), "LVLN");
        assert!(matches!(record.fields[1].value, FieldValue::Uint(1)));
        let FieldValue::Struct(fields) = &record.fields[2].value else {
            panic!("LVLO must remain structured");
        };
        assert!(fields.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("npc")
                && matches!(value, FieldValue::FormKey(key) if key.local == 0x200)
        }));
        assert!(
            fields
                .iter()
                .any(|(name, _)| { interner.resolve(*name) == Some("chance_none") })
        );
    }

    #[test]
    fn unsupported_source_model_payload_is_typed_blocker() {
        let interner = StringInterner::new();
        let mut record = source_lvlc(&interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"MODT"),
            value: FieldValue::Bytes(smallvec![1, 2, 3]),
        });
        assert_eq!(
            classify_legacy_lvlc(&record).reason_codes,
            vec!["legacy_lvlc_field_not_representable_in_fo4_lvln"]
        );
    }
}
