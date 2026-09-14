use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::{StringInterner, Sym};

const INCOMPATIBLE_LEGACY_FALLOUT_CONDITION_IDS: [u32; 3] = [53, 79, 420];

fn is_condition_string(sig: [u8; 4]) -> bool {
    matches!(sig, sig if sig == *b"CIS1" || sig == *b"CIS2")
}

fn is_incompatible_legacy_fallout_condition(value: &FieldValue) -> bool {
    let FieldValue::Bytes(bytes) = value else {
        return false;
    };
    let Some(raw_function_id) = bytes.get(8..12) else {
        return false;
    };
    let function_id = u32::from_le_bytes(raw_function_id.try_into().unwrap());
    INCOMPATIBLE_LEGACY_FALLOUT_CONDITION_IDS.contains(&function_id)
}

fn drop_incompatible_conditions_from_struct(
    fields: &mut Vec<(Sym, FieldValue)>,
    interner: &StringInterner,
) {
    let mut source = std::mem::take(fields).into_iter().peekable();
    while let Some((key, mut value)) = source.next() {
        if interner.resolve(key) == Some("CTDA") && is_incompatible_legacy_fallout_condition(&value)
        {
            while source.peek().is_some_and(|(next_key, _)| {
                matches!(interner.resolve(*next_key), Some("CIS1") | Some("CIS2"))
            }) {
                source.next();
            }
            continue;
        }
        drop_incompatible_conditions_from_value(&mut value, interner);
        fields.push((key, value));
    }
}

fn drop_incompatible_conditions_from_value(value: &mut FieldValue, interner: &StringInterner) {
    match value {
        FieldValue::List(items) => {
            for item in items {
                drop_incompatible_conditions_from_value(item, interner);
            }
        }
        FieldValue::Struct(fields) => drop_incompatible_conditions_from_struct(fields, interner),
        _ => {}
    }
}

pub(super) fn drop_incompatible_legacy_fallout_conditions(
    record: &mut Record,
    interner: &StringInterner,
) {
    let mut source = std::mem::take(&mut record.fields).into_iter().peekable();
    let mut output = Vec::with_capacity(source.size_hint().0);
    while let Some(mut entry) = source.next() {
        if entry.sig.0 == *b"CTDA" && is_incompatible_legacy_fallout_condition(&entry.value) {
            while source
                .peek()
                .is_some_and(|next| is_condition_string(next.sig.0))
            {
                source.next();
            }
            continue;
        }
        drop_incompatible_conditions_from_value(&mut entry.value, interner);
        output.push(entry);
    }
    record.fields = output.into_iter().collect();
}

fn legacy_term_model_path<'a>(record: &'a Record, interner: &'a StringInterner) -> Option<&'a str> {
    let model = record.fields.iter().find(|entry| entry.sig.0 == *b"MODL")?;
    match &model.value {
        FieldValue::String(value) => interner.resolve(*value),
        FieldValue::Bytes(value) => std::str::from_utf8(value)
            .ok()
            .map(|value| value.trim_end_matches('\0')),
        _ => None,
    }
    .filter(|value| !value.is_empty())
}

fn append_fo4_term_furniture_markers(record: &mut Record, interner: &StringInterner) {
    let Some(model_path) = legacy_term_model_path(record, interner) else {
        return;
    };
    let is_desk = model_path.to_ascii_lowercase().contains("desk");
    let (marker_model, offset_x, offset_y) = if is_desk {
        ("Markers\\MarkerDeskTerminal01.nif", -3.336_f32, -67.322_f32)
    } else {
        ("Markers\\MarkerWallTerminal3rdP.nif", 0.0_f32, -86.0_f32)
    };

    let mut marker_parameters = smallvec::SmallVec::<[u8; 32]>::new();
    marker_parameters.extend_from_slice(&offset_x.to_le_bytes());
    marker_parameters.extend_from_slice(&offset_y.to_le_bytes());
    marker_parameters.extend_from_slice(&0.0_f32.to_le_bytes());
    marker_parameters.extend_from_slice(&0.0_f32.to_le_bytes());
    marker_parameters.extend_from_slice(&0_u32.to_le_bytes());
    marker_parameters.extend_from_slice(&[0xff; 4]);

    let marker_fields = [
        (
            *b"PNAM",
            FieldValue::Bytes(smallvec::smallvec![0xcc, 0x4c, 0x33, 0x00]),
        ),
        (*b"FNAM", FieldValue::Bytes(smallvec::smallvec![])),
        (*b"COCT", FieldValue::Uint(0)),
        (*b"MNAM", FieldValue::Uint(0x4000_0001)),
        (*b"WBDT", FieldValue::Bytes(smallvec::smallvec![0])),
        (*b"XMRK", FieldValue::String(interner.intern(marker_model))),
        (*b"SNAM", FieldValue::Bytes(marker_parameters)),
        (*b"BSIZ", FieldValue::Uint(0)),
    ];
    record
        .fields
        .extend(marker_fields.into_iter().map(|(sig, value)| FieldEntry {
            sig: SubrecordSig(sig),
            value,
        }));
}

fn value_has_force_redraw(value: &FieldValue, interner: &crate::sym::StringInterner) -> bool {
    match value {
        FieldValue::Uint(value) => value & 2 != 0,
        FieldValue::Int(value) => *value >= 0 && (*value as u64) & 2 != 0,
        FieldValue::Bytes(bytes) => bytes.first().is_some_and(|value| value & 2 != 0),
        FieldValue::String(value) => interner
            .resolve(*value)
            .is_some_and(|value| value.eq_ignore_ascii_case("ForceRedraw")),
        FieldValue::List(values) => values
            .iter()
            .any(|value| value_has_force_redraw(value, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_has_force_redraw(value, interner)),
        _ => false,
    }
}
fn rewrite_structured_term_item(
    value: &mut FieldValue,
    interner: &crate::sym::StringInterner,
    item_id: u16,
) -> bool {
    let FieldValue::Struct(fields) = value else {
        return false;
    };
    let has_submenu = fields
        .iter()
        .any(|(key, _)| interner.resolve(*key) == Some("TNAM"));
    if !has_submenu {
        return false;
    }
    let force_redraw = fields.iter().any(|(key, value)| {
        interner.resolve(*key) == Some("ANAM") && value_has_force_redraw(value, interner)
    });
    fields.retain(|(key, _)| {
        !matches!(
            interner.resolve(*key),
            Some("ANAM") | Some("ITID") | Some("INAM")
        )
    });
    let insert_at = fields
        .iter()
        .position(|(key, _)| interner.resolve(*key) == Some("TNAM"))
        .unwrap_or(fields.len());
    fields.insert(
        insert_at,
        (
            interner.intern("ANAM"),
            FieldValue::Uint(if force_redraw { 6 } else { 4 }),
        ),
    );
    fields.insert(
        insert_at + 1,
        (interner.intern("ITID"), FieldValue::Uint(item_id as u64)),
    );
    true
}

pub(super) fn rewrite_term_menu_rows(record: &mut Record, interner: &crate::sym::StringInterner) {
    let source: Vec<_> = record.fields.drain(..).collect();
    let mut output = Vec::with_capacity(source.len());
    let mut first_menu_at = None;
    let mut next_item_id = 1_u32;
    let mut index = 0;
    while index < source.len() {
        if source[index].sig.0 != *b"ITXT" {
            if !matches!(
                source[index].sig.0,
                sig if sig == *b"ISIZ"
                    || sig == *b"ANAM"
                    || sig == *b"ITID"
                    || sig == *b"INAM"
            ) {
                output.push(source[index].clone());
            }
            index += 1;
            continue;
        }

        if matches!(source[index].value, FieldValue::List(_)) {
            let mut entry = source[index].clone();
            let FieldValue::List(items) = &mut entry.value else {
                unreachable!();
            };
            items.retain_mut(|item| {
                let Ok(item_id) = u16::try_from(next_item_id) else {
                    return false;
                };
                let keep = rewrite_structured_term_item(item, interner, item_id);
                if keep {
                    next_item_id += 1;
                }
                keep
            });
            if !items.is_empty() {
                first_menu_at.get_or_insert(output.len());
                output.push(entry);
            }
            index += 1;
            continue;
        }
        if matches!(source[index].value, FieldValue::Struct(_)) {
            let mut entry = source[index].clone();
            if let Ok(item_id) = u16::try_from(next_item_id)
                && rewrite_structured_term_item(&mut entry.value, interner, item_id)
            {
                first_menu_at.get_or_insert(output.len());
                output.push(entry);
                next_item_id += 1;
            }
            index += 1;
            continue;
        }

        let end = source[index + 1..]
            .iter()
            .position(|entry| entry.sig.0 == *b"ITXT")
            .map_or(source.len(), |offset| index + 1 + offset);
        let row = &source[index..end];
        let Some(tnam_at) = row.iter().position(|entry| entry.sig.0 == *b"TNAM") else {
            index = end;
            continue;
        };
        let Ok(item_id) = u16::try_from(next_item_id) else {
            index = end;
            continue;
        };
        first_menu_at.get_or_insert(output.len());
        let force_redraw = row
            .iter()
            .filter(|entry| entry.sig.0 == *b"ANAM")
            .any(|entry| value_has_force_redraw(&entry.value, interner));
        for (row_index, entry) in row.iter().enumerate() {
            if row_index == tnam_at {
                output.push(FieldEntry {
                    sig: SubrecordSig(*b"ANAM"),
                    value: FieldValue::Uint(if force_redraw { 6 } else { 4 }),
                });
                output.push(FieldEntry {
                    sig: SubrecordSig(*b"ITID"),
                    value: FieldValue::Uint(item_id as u64),
                });
            }
            if !matches!(
                entry.sig.0,
                sig if sig == *b"ANAM" || sig == *b"ITID" || sig == *b"INAM"
            ) {
                output.push(entry.clone());
            }
        }
        next_item_id += 1;
        index = end;
    }
    if let Some(insert_at) = first_menu_at {
        output.insert(
            insert_at,
            FieldEntry {
                sig: SubrecordSig(*b"ISIZ"),
                value: FieldValue::Uint((next_item_id - 1) as u64),
            },
        );
    }
    record.fields = output.into_iter().collect();
    append_fo4_term_furniture_markers(record, interner);
}
