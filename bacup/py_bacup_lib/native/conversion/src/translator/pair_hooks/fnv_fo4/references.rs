use super::common::struct_value;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
fn uint_from_struct(
    value: &FieldValue,
    key: &str,
    interner: &crate::sym::StringInterner,
) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) if *value >= 0 => Some(*value as u64),
        FieldValue::Struct(fields) => match struct_value(fields, key, interner)? {
            FieldValue::Uint(value) => Some(*value),
            FieldValue::Int(value) if *value >= 0 => Some(*value as u64),
            _ => None,
        },
        _ => None,
    }
}

fn relayout_xrmr(value: &FieldValue, interner: &crate::sym::StringInterner) -> Option<FieldValue> {
    let count = match value {
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            u16::from_le_bytes([bytes[0], bytes[1]]) as u64
        }
        _ => uint_from_struct(value, "linked_rooms_count", interner)?,
    };
    let count = u8::try_from(count).ok()?;
    // FO4 narrows the count and replaces FNV's two unknown bytes with flags
    // plus fixed default bytes. Never reinterpret the source bytes as flags.
    match value {
        FieldValue::Bytes(_) => Some(FieldValue::Bytes(smallvec::smallvec![count, 0, 1, 0])),
        _ => Some(FieldValue::Struct(vec![
            (
                interner.intern("linked_rooms_count"),
                FieldValue::Uint(count as u64),
            ),
            (interner.intern("flags"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_2"), FieldValue::Uint(1)),
            (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
        ])),
    }
}

pub(super) fn relayout_refr_xrmr(record: &mut Record, interner: &crate::sym::StringInterner) {
    let source: Vec<_> = record.fields.drain(..).collect();
    let mut output = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index].sig.0 != *b"XRMR" {
            output.push(source[index].clone());
            index += 1;
            continue;
        }
        if let Some(value) = relayout_xrmr(&source[index].value, interner) {
            output.push(FieldEntry {
                sig: source[index].sig,
                value,
            });
            index += 1;
        } else {
            index += 1;
            while index < source.len() && source[index].sig.0 == *b"XLRM" {
                index += 1;
            }
        }
    }
    record.fields = output.into_iter().collect();
}

pub(super) fn relayout_addn_dnam(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        // The trailing FNV bytes are unknown, while FO4 treats them as flags.
        // Preserve only the shared particle-system cap and default FO4 flags.
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(FieldValue::Bytes(smallvec::smallvec![
                bytes[0], bytes[1], 0, 0
            ]))
        }
        _ => {
            let cap = uint_from_struct(value, "master_particle_system_cap", interner)?;
            let cap = u16::try_from(cap).ok()?;
            Some(FieldValue::Struct(vec![
                (
                    interner.intern("master_particle_system_cap"),
                    FieldValue::Uint(cap as u64),
                ),
                (interner.intern("flags"), FieldValue::Uint(0)),
            ]))
        }
    }
}
