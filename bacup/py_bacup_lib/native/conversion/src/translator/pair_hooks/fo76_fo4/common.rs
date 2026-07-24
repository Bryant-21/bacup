use super::*;

pub(super) const FO76_MASTER_NAME: &str = "SeventySix.esm";
pub(super) const FO4_MASTER_NAME: &str = "Fallout4.esm";

pub(super) fn trim_nul_suffix(mut bytes: &[u8]) -> &[u8] {
    while matches!(bytes.last(), Some(0)) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

pub(super) fn normalize_u8_field_value(value: &mut FieldValue, map: fn(u8) -> u8) {
    match value {
        FieldValue::Uint(n) if *n <= u64::from(u8::MAX) => *n = u64::from(map(*n as u8)),
        FieldValue::Int(n) if (0..=i64::from(u8::MAX)).contains(n) => {
            *n = i64::from(map(*n as u8));
        }
        FieldValue::Bytes(bytes) if !bytes.is_empty() => bytes[0] = map(bytes[0]),
        _ => {}
    }
}

pub(super) fn field_value_has_non_empty_text(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> bool {
    match value {
        FieldValue::String(value) => interner
            .resolve(*value)
            .is_some_and(|value| !value.trim_matches(['\0', ' ', '\t', '\r', '\n']).is_empty()),
        FieldValue::Bytes(bytes) => bytes
            .split(|byte| *byte == 0)
            .next()
            .is_some_and(|value| value.iter().any(|byte| !byte.is_ascii_whitespace())),
        FieldValue::List(values) => values
            .iter()
            .any(|value| field_value_has_non_empty_text(value, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| field_value_has_non_empty_text(value, interner)),
        _ => false,
    }
}

pub(super) fn project_u32_value(value: &FieldValue) -> Option<FieldValue> {
    match value {
        FieldValue::Uint(value) if u32::try_from(*value).is_ok() => Some(FieldValue::Uint(*value)),
        FieldValue::Int(value) if u32::try_from(*value).is_ok() => Some(FieldValue::Int(*value)),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => Some(bytes_value(bytes.get(0..4)?)),
        _ => None,
    }
}

pub(super) fn following_u16_value(
    fields: &[FieldEntry],
    start: usize,
    wanted_sig: &[u8; 4],
    default_value: u16,
) -> u16 {
    for entry in fields.iter().skip(start) {
        if entry.sig.0 == *b"LVLO" {
            break;
        }
        if entry.sig.0 == *wanted_sig {
            return field_value_to_u16(&entry.value).unwrap_or(default_value);
        }
    }
    default_value
}

pub(super) fn field_value_to_u16(value: &FieldValue) -> Option<u16> {
    match value {
        FieldValue::Uint(value) => u16::try_from(*value).ok(),
        FieldValue::Int(value) => u16::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u16::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u16)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes([bytes[0], bytes[1]]))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, candidate)| field_value_to_u16(candidate)),
        _ => None,
    }
}

pub(super) fn read_u32_le_at(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
}

pub(super) fn set_u32_le_at(bytes: &mut [u8], offset: usize, value: u32) {
    if let Some(chunk) = bytes.get_mut(offset..offset + 4) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
}

pub(super) fn project_formid_value(value: &FieldValue) -> Option<FieldValue> {
    match value {
        FieldValue::FormKey(_) => Some(value.clone()),
        FieldValue::Uint(value) if u32::try_from(*value).is_ok() => Some(FieldValue::Uint(*value)),
        FieldValue::Int(value) if u32::try_from(*value).is_ok() => Some(FieldValue::Int(*value)),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => Some(bytes_value(bytes.get(0..4)?)),
        _ => None,
    }
}

pub(super) fn field_value_to_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u32::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u32)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, candidate)| field_value_to_u32(candidate)),
        _ => None,
    }
}

pub(super) fn field_value_to_i64(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Uint(value) => i64::try_from(*value).ok(),
        FieldValue::Int(value) => Some(*value),
        FieldValue::Float(value) if value.is_finite() => Some(value.round() as i64),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64)
        }
        _ => None,
    }
}

pub(super) fn named_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<&'a FieldValue> {
    let key = interner.intern(name);
    fields
        .iter()
        .find_map(|(field_name, value)| (*field_name == key).then_some(value))
}

pub(super) fn named_value_canonical<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<&'a FieldValue> {
    field_index_canonical(fields, name, interner).map(|index| &fields[index].1)
}

pub(super) fn field_index_canonical(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<usize> {
    let wanted = canonical_field_name(name);
    fields.iter().position(|(field_name, _)| {
        interner
            .resolve(*field_name)
            .is_some_and(|field_name| canonical_field_name(field_name) == wanted)
    })
}

pub(super) fn canonical_field_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn bytes_value(bytes: &[u8]) -> FieldValue {
    FieldValue::Bytes(smallvec::SmallVec::from_slice(bytes))
}

pub(super) fn set_u32_count(value: &mut FieldValue, count: u32) {
    match value {
        FieldValue::Uint(n) => *n = count as u64,
        FieldValue::Int(n) => *n = count as i64,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            bytes[..4].copy_from_slice(&count.to_le_bytes());
        }
        FieldValue::Bytes(bytes) => {
            bytes.clear();
            bytes.extend_from_slice(&count.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            if let Some((_, first_value)) = fields.first_mut() {
                set_u32_count(first_value, count);
            }
        }
        _ => *value = FieldValue::Uint(count as u64),
    }
}

pub(super) fn set_u32_bits(value: &mut FieldValue, mask: u32) {
    match value {
        FieldValue::Uint(n) => *n |= mask as u64,
        FieldValue::Int(n) => *n = ((*n as u32) | mask) as i64,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let mut raw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            raw |= mask;
            bytes[0..4].copy_from_slice(&raw.to_le_bytes());
        }
        FieldValue::Bytes(bytes) => {
            bytes.clear();
            bytes.extend_from_slice(&mask.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            if let Some((_, first_value)) = fields.first_mut() {
                set_u32_bits(first_value, mask);
            }
        }
        _ => *value = FieldValue::Uint(mask as u64),
    }
}

pub(super) fn clear_u32_bits(value: &mut FieldValue, mask: u32) {
    match value {
        FieldValue::Uint(n) => *n &= !(mask as u64),
        FieldValue::Int(n) => *n = ((*n as u32) & !mask) as i64,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let mut raw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            raw &= !mask;
            bytes[0..4].copy_from_slice(&raw.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            if let Some((_, first_value)) = fields.first_mut() {
                clear_u32_bits(first_value, mask);
            }
        }
        _ => {}
    }
}

pub(super) fn clear_low_byte_flag(value: &mut FieldValue, flag: u8) {
    match value {
        FieldValue::Uint(value) => *value &= !u64::from(flag),
        FieldValue::Int(value) => *value &= !i64::from(flag),
        FieldValue::Bytes(bytes) if !bytes.is_empty() => bytes[0] &= !flag,
        FieldValue::List(values) => {
            for value in values {
                clear_low_byte_flag(value, flag);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                clear_low_byte_flag(value, flag);
            }
        }
        _ => {}
    }
}

pub(super) fn truncate_raw_subrecord(record: &mut Record, sig: &[u8; 4], max_len: usize) {
    for entry in &mut record.fields {
        if entry.sig.0 != *sig {
            continue;
        }
        if let FieldValue::Bytes(bytes) = &mut entry.value {
            bytes.truncate(max_len);
        }
    }
}

pub(super) fn project_raw_array_rows(
    record: &mut Record,
    sig: &[u8; 4],
    source_row_len: usize,
    target_row_len: usize,
) {
    if source_row_len <= target_row_len || target_row_len == 0 {
        return;
    }

    for entry in &mut record.fields {
        if entry.sig.0 != *sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.is_empty() || bytes.len() % source_row_len != 0 {
            continue;
        }

        let mut projected = smallvec::SmallVec::new();
        for row in bytes.chunks_exact(source_row_len) {
            projected.extend_from_slice(&row[..target_row_len]);
        }
        *bytes = projected;
    }
}
impl Fo76Fo4Hook {
    pub(super) fn struct_field_name_is(
        interner: &crate::sym::StringInterner,
        name: crate::sym::Sym,
        expected: &str,
    ) -> bool {
        interner.resolve(name).is_some_and(|actual| {
            actual.eq_ignore_ascii_case(expected)
                || actual.replace('_', "").eq_ignore_ascii_case(expected)
        })
    }

    pub(super) fn positive_u32_struct_field(
        interner: &crate::sym::StringInterner,
        fields: &[(crate::sym::Sym, FieldValue)],
        field_name: &str,
    ) -> Option<u32> {
        fields
            .iter()
            .find(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
            .and_then(|(_, value)| Self::field_value_positive_u32(value))
    }
}
