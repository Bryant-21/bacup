use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

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
}
