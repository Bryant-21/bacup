use super::*;
use crate::sym::StringInterner;

const EVENT_CENTERS: &[(u32, &str, u32, u32, u32)] = &[
    (0x003AF2, "enz01_above", 18, 3, 0x00BA1D),
    (0x00D02C, "enz04_bots", 11, 13, 0x3AAF4B),
    (0x00D053, "ens02_blast", 31, 25, 0x17CE71),
];

fn alias_range(record: &Record, id: u32) -> Option<(usize, usize)> {
    let start = record.fields.iter().position(|field| {
        field.sig.0 == *b"ALST" && field_value_to_u32(&field.value) == Some(id)
    })?;
    let end = record.fields[start..]
        .iter()
        .position(|field| field.sig.0 == *b"ALED")?
        + start;
    Some((start, end))
}

pub(super) fn adapt_event_center(interner: &StringInterner, record: &mut Record) {
    if record.sig.0 != *b"QUST"
        || !interner
            .resolve(record.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
    {
        return;
    }
    let Some(&(_, eid, selector_id, center_id, ref_type_id)) = EVENT_CENTERS
        .iter()
        .find(|&&(id, _, _, _, _)| record.form_key.local == id)
    else {
        return;
    };
    if qust_eid_lower(interner, record).as_deref() != Some(eid) {
        return;
    }
    let Some((selector_start, selector_end)) = alias_range(record, selector_id) else {
        return;
    };
    let selector = &record.fields[selector_start..selector_end];
    if !selector
        .iter()
        .any(|field| field.sig.0 == *b"ALFI" && field_value_to_u32(&field.value) == Some(center_id))
    {
        return;
    }
    let Some(ref_type) = selector.iter().find_map(|field| {
        if field.sig.0 != *b"ALFF" {
            return None;
        }
        match &field.value {
            FieldValue::FormKey(key) if key.local == ref_type_id => Some(field.value.clone()),
            _ => None,
        }
    }) else {
        return;
    };
    let Some((center_start, center_end)) = alias_range(record, center_id) else {
        return;
    };
    let center = &record.fields[center_start..center_end];
    if center
        .iter()
        .any(|field| matches!(&field.sig.0, b"ALFA" | b"ALRT"))
        || !center.iter().any(|field| {
            field.sig.0 == *b"ALFR" && qust_alias_forced_reference_is_null(&field.value)
        })
    {
        return;
    }
    let Some(next_alias_index) = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"ANAM")
    else {
        return;
    };
    let Some(event_alias_id) = field_value_to_u32(&record.fields[next_alias_index].value) else {
        return;
    };
    let Some(next_alias_id) = event_alias_id.checked_add(1) else {
        return;
    };
    if record.fields.iter().any(|field| {
        matches!(&field.sig.0, b"ALST" | b"ALLS" | b"ALCS")
            && field_value_to_u32(&field.value).is_some_and(|id| id >= event_alias_id)
    }) {
        return;
    }
    let Some(flags_index) = center.iter().position(|field| field.sig.0 == *b"FNAM") else {
        return;
    };
    let Some(flags) = field_value_to_u32(&center[flags_index].value) else {
        return;
    };
    if flags & 0x800 == 0 {
        return;
    }

    // FO76's ALFF search forces this alias. FO4 instead resolves the same
    // ref type inside the location supplied by the local event producer.
    let mut replacement: Vec<FieldEntry> = center
        .iter()
        .filter(|field| field.sig.0 != *b"ALFR")
        .cloned()
        .collect();
    for field in &mut replacement {
        if field.sig.0 == *b"FNAM" {
            set_u32_count(&mut field.value, flags & !0x800);
        }
    }
    replacement.push(FieldEntry {
        sig: SubrecordSig(*b"ALFA"),
        value: FieldValue::Uint(u64::from(event_alias_id)),
    });
    replacement.push(FieldEntry {
        sig: SubrecordSig(*b"ALRT"),
        value: ref_type,
    });
    record.fields.drain(center_start..center_end);
    for (offset, entry) in replacement.into_iter().enumerate() {
        record.fields.insert(center_start + offset, entry);
    }
    set_u32_count(&mut record.fields[next_alias_index].value, next_alias_id);
    let location = [
        FieldEntry {
            sig: SubrecordSig(*b"ALLS"),
            value: FieldValue::Uint(u64::from(event_alias_id)),
        },
        FieldEntry {
            sig: SubrecordSig(*b"ALID"),
            value: FieldValue::String(interner.intern("B21_EnclaveEventLocation")),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FNAM"),
            value: FieldValue::Uint(0),
        },
        FieldEntry {
            sig: SubrecordSig(*b"ALFE"),
            value: FieldValue::Uint(u64::from(FO76_QUEST_EVENT_SCPT)),
        },
        FieldEntry {
            sig: SubrecordSig(*b"ALFD"),
            value: FieldValue::Uint(u64::from(FO4_QUEST_EVENT_LOCATION1)),
        },
        FieldEntry {
            sig: SubrecordSig(*b"ALED"),
            value: FieldValue::None,
        },
    ];
    for entry in location.into_iter().rev() {
        record.fields.insert(next_alias_index + 1, entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }
    }

    fn fixture(interner: &StringInterner, spec: (u32, &str, u32, u32, u32)) -> Record {
        let (quest_id, eid, selector, center, ref_type) = spec;
        let plugin = interner.intern(FO76_MASTER_NAME);
        let mut record = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey {
                local: quest_id,
                plugin,
            },
        );
        record.eid = Some(interner.intern(eid));
        record.fields.extend([
            field(b"ENAM", FieldValue::Uint(u64::from(FO76_QUEST_EVENT_SCPT))),
            field(b"ANAM", FieldValue::Uint(100)),
            field(b"ALST", FieldValue::Uint(u64::from(selector))),
            field(b"ALID", FieldValue::String(interner.intern("Selector"))),
            field(b"FNAM", FieldValue::Uint(10)),
            field(b"ALFI", FieldValue::Uint(u64::from(center))),
            field(
                b"ALFF",
                FieldValue::FormKey(FormKey {
                    local: ref_type,
                    plugin,
                }),
            ),
            field(b"ALED", FieldValue::None),
            field(b"ALST", FieldValue::Uint(u64::from(center))),
            field(
                b"ALID",
                FieldValue::String(interner.intern("CenterMarkerActual")),
            ),
            field(b"FNAM", FieldValue::Uint(0x808)),
            field(b"ALFR", FieldValue::FormKey(FormKey { local: 0, plugin })),
            field(b"ALED", FieldValue::None),
        ]);
        record
    }

    #[test]
    fn enclave_centers_use_event_location_before_generic_alias_stripping() {
        let interner = StringInterner::new();
        for &spec in EVENT_CENTERS {
            let mut record = fixture(&interner, spec);
            Fo76Fo4Hook::strip_qust_runtime_scopes(&interner, &mut record);
            let (start, end) = alias_range(&record, spec.3).unwrap();
            let center = &record.fields[start..end];
            assert!(
                center.iter().any(|entry| entry.sig.0 == *b"ALFA"
                    && field_value_to_u32(&entry.value) == Some(100))
            );
            assert!(center.iter().any(|entry| entry.sig.0 == *b"ALRT"
                && matches!(entry.value, FieldValue::FormKey(key) if key.local == spec.4)));
            assert!(center.iter().any(
                |entry| entry.sig.0 == *b"FNAM" && field_value_to_u32(&entry.value) == Some(8)
            ));
            assert!(!center.iter().any(|entry| entry.sig.0 == *b"ALFR"));
            let location_start = record
                .fields
                .iter()
                .position(|entry| entry.sig.0 == *b"ALLS")
                .unwrap();
            assert!(location_start < start);
            assert!(record.fields.iter().any(|entry| entry.sig.0 == *b"ALFD"
                && field_value_to_u32(&entry.value) == Some(FO4_QUEST_EVENT_LOCATION1)));
            assert!(record.fields.iter().any(|entry| entry.sig.0 == *b"ENAM"
                && field_value_to_u32(&entry.value) == Some(FO76_QUEST_EVENT_SCPT)));
            assert!(
                record.fields.iter().any(|entry| entry.sig.0 == *b"ANAM"
                    && field_value_to_u32(&entry.value) == Some(101))
            );
            let before = format!("{:?}", record.fields);
            Fo76Fo4Hook::strip_qust_runtime_scopes(&interner, &mut record);
            assert_eq!(format!("{:?}", record.fields), before);
        }
    }

    #[test]
    fn enclave_center_adapter_requires_the_exact_source_contract() {
        let interner = StringInterner::new();
        for mutation in 0..5 {
            let mut record = fixture(&interner, EVENT_CENTERS[1]);
            match mutation {
                0 => record.form_key.plugin = interner.intern("Other.esm"),
                1 => record.eid = Some(interner.intern("OtherQuest")),
                2 => {
                    record
                        .fields
                        .iter_mut()
                        .find(|entry| entry.sig.0 == *b"ALFF")
                        .unwrap()
                        .value = FieldValue::FormKey(FormKey {
                        local: 1,
                        plugin: record.form_key.plugin,
                    })
                }
                3 => {
                    record
                        .fields
                        .iter_mut()
                        .find(|entry| entry.sig.0 == *b"ALFI")
                        .unwrap()
                        .value = FieldValue::Uint(99)
                }
                _ => {
                    record
                        .fields
                        .iter_mut()
                        .find(|entry| entry.sig.0 == *b"ALFR")
                        .unwrap()
                        .value = FieldValue::FormKey(FormKey {
                        local: 1,
                        plugin: record.form_key.plugin,
                    })
                }
            }
            let before = format!("{:?}", record.fields);
            adapt_event_center(&interner, &mut record);
            assert_eq!(format!("{:?}", record.fields), before);
        }
    }
}
