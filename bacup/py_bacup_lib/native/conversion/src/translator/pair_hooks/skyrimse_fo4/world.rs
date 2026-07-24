use super::SkyrimSeFo4Hook;
use crate::record::{FieldValue, Record};

impl SkyrimSeFo4Hook {
    pub(super) fn normalize_refr_map_marker_tnam(record: &mut Record) {
        if record.sig.0 != *b"REFR" || !record.fields.iter().any(|entry| entry.sig.0 == *b"XMRK") {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"TNAM" {
                continue;
            }
            let Some(source_type) = map_marker_type(&entry.value) else {
                continue;
            };
            write_map_marker_type(&mut entry.value, skyrim_map_marker_type_to_fo4(source_type));
        }
    }
}

pub(super) fn map_marker_type(value: &FieldValue) -> Option<u8> {
    match value {
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Bytes(bytes) => bytes.first().copied(),
        FieldValue::Struct(fields) => fields.first().and_then(|(_, value)| map_marker_type(value)),
        _ => None,
    }
}

fn write_map_marker_type(value: &mut FieldValue, target_type: u8) {
    match value {
        FieldValue::Uint(value) => *value = u64::from(target_type),
        FieldValue::Int(value) => *value = i64::from(target_type),
        FieldValue::Bytes(bytes) if !bytes.is_empty() => bytes[0] = target_type,
        FieldValue::Struct(fields) => {
            if let Some((_, value)) = fields.first_mut() {
                write_map_marker_type(value, target_type);
            }
        }
        _ => {}
    }
}

fn skyrim_map_marker_type_to_fo4(source_type: u8) -> u8 {
    match source_type {
        0 => 77,
        1 => 1,
        2 => 49,
        3 => 13,
        4 => 0,
        5 => 3,
        6 => 53,
        7 => 10,
        8 => 11,
        9 => 45,
        10 => 28,
        11 => 8,
        12 => 0,
        13 => 26,
        14 => 4,
        15 => 40,
        16 | 17 => 3,
        18 => 8,
        19 | 20 => 4,
        21 => 26,
        22 => 7,
        23 => 28,
        24 => 8,
        25 => 12,
        26 => 8,
        27 => 38,
        28 => 13,
        29 => 3,
        30 => 56,
        31 => 10,
        32 => 13,
        33 => 38,
        34 => 12,
        35 | 37 | 39 | 41 | 43 | 45 | 47 | 49 | 51 => 53,
        36 | 38 | 40 | 42 | 44 | 46 | 48 | 50 | 52 => 1,
        53 => 12,
        54 => 49,
        55 => 8,
        56 => 13,
        57 | 58 => 77,
        59 => 53,
        _ => 77,
    }
}
