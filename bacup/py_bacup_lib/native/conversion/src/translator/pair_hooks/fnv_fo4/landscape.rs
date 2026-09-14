use smallvec::SmallVec;

use super::common::struct_value;
use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const FO4_MATERIAL_STONE: u32 = 0x012F34;
const FO4_MATERIAL_DIRT: u32 = 0x012F38;
const FO4_MATERIAL_GRASS: u32 = 0x012F46;
const FO4_MATERIAL_STONE_HEAVY: u32 = 0x012F36;
const FO4_MATERIAL_SAND: u32 = 0x012F3E;
const FO4_MATERIAL_CONCRETE: u32 = 0x055F39;
// FO4 renders only ATXT slots 0..=4; exact saturation in slots 3 and 4 can black out LAND.
const FO4_MAX_ATXT_SLOT: i16 = 4;
const FO4_FIRST_ATXT_SLOT_REQUIRING_HEADROOM: i16 = 3;
const FO4_MAX_LATE_ATXT_ALPHA: f32 = 254.0 / 255.0;

fn uint8(value: &FieldValue) -> Option<u8> {
    match value {
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        _ => None,
    }
}

fn int16(value: &FieldValue) -> Option<i16> {
    match value {
        FieldValue::Int(value) => i16::try_from(*value).ok(),
        FieldValue::Uint(value) => i16::try_from(*value).ok(),
        _ => None,
    }
}

fn land_layer_slot(value: &FieldValue, interner: &StringInterner) -> Option<i16> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
            Some(i16::from_le_bytes(bytes[6..8].try_into().ok()?))
        }
        FieldValue::Struct(fields) => int16(struct_value(fields, "layer", interner)?),
        _ => None,
    }
}

fn cap_land_alpha(value: &mut FieldValue) {
    let FieldValue::Bytes(bytes) = value else {
        return;
    };
    for row in bytes.chunks_exact_mut(8) {
        let opacity = f32::from_le_bytes(row[4..8].try_into().expect("eight-byte VTXT row"));
        if opacity > FO4_MAX_LATE_ATXT_ALPHA {
            row[4..8].copy_from_slice(&FO4_MAX_LATE_ATXT_ALPHA.to_le_bytes());
        }
    }
}

pub(super) fn normalize_legacy_land_layers(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"LAND" {
        return;
    }

    let mut normalized = SmallVec::with_capacity(record.fields.len());
    let mut drop_next_vtxt = false;
    let mut cap_next_vtxt = false;
    for mut entry in record.fields.drain(..) {
        match entry.sig.0 {
            sig if sig == *b"ATXT" => {
                drop_next_vtxt = false;
                cap_next_vtxt = false;
                if let Some(slot) = land_layer_slot(&entry.value, interner) {
                    if slot > FO4_MAX_ATXT_SLOT {
                        drop_next_vtxt = true;
                        continue;
                    }
                    cap_next_vtxt = slot >= FO4_FIRST_ATXT_SLOT_REQUIRING_HEADROOM;
                }
                normalized.push(entry);
            }
            sig if sig == *b"VTXT" && drop_next_vtxt => {
                drop_next_vtxt = false;
            }
            sig if sig == *b"VTXT" => {
                if cap_next_vtxt {
                    cap_land_alpha(&mut entry.value);
                }
                cap_next_vtxt = false;
                normalized.push(entry);
            }
            _ => {
                drop_next_vtxt = false;
                cap_next_vtxt = false;
                normalized.push(entry);
            }
        }
    }
    record.fields = normalized;
}

fn legacy_havok_data(value: &FieldValue, interner: &StringInterner) -> Option<(u8, u8, u8)> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 3 => Some((bytes[0], bytes[1], bytes[2])),
        FieldValue::Struct(fields) => Some((
            uint8(struct_value(fields, "material_type", interner)?)?,
            uint8(struct_value(fields, "friction", interner)?)?,
            uint8(struct_value(fields, "restitution", interner)?)?,
        )),
        _ => None,
    }
}

fn fo4_material_type(source_material: u8, interner: &StringInterner) -> FormKey {
    let local = match source_material {
        0 => FO4_MATERIAL_STONE,
        2 => FO4_MATERIAL_DIRT,
        4 => FO4_MATERIAL_GRASS,
        10 => FO4_MATERIAL_STONE_HEAVY,
        18 => FO4_MATERIAL_SAND,
        19 => FO4_MATERIAL_CONCRETE,
        _ => FO4_MATERIAL_DIRT,
    };
    FormKey {
        local,
        plugin: interner.intern("Fallout4.esm"),
    }
}

pub(super) fn relayout_legacy_ltex(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"LTEX" {
        return;
    }

    let Some((material, friction, restitution)) = record.fields.iter().find_map(|entry| {
        (entry.sig.0 == *b"HNAM")
            .then(|| legacy_havok_data(&entry.value, interner))
            .flatten()
    }) else {
        return;
    };

    record
        .fields
        .retain(|entry| !matches!(entry.sig.0, sig if sig == *b"MNAM" || sig == *b"HNAM"));
    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(entry.sig.0, sig if sig == *b"SNAM" || sig == *b"GNAM"))
        .unwrap_or(record.fields.len());

    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: SubrecordSig(*b"MNAM"),
            value: FieldValue::FormKey(fo4_material_type(material, interner)),
        },
    );
    record.fields.insert(
        insert_at + 1,
        FieldEntry {
            sig: SubrecordSig(*b"HNAM"),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![friction, restitution])),
        },
    );
}
