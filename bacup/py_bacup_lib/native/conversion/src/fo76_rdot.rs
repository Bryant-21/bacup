//! FO76 REGN.RDOT decoding and FO4 row-shape conversion.
//!
//! xEdit models FO76 RDOT as 74-byte object rows. Some dumped payloads append
//! a same-count 2-byte trailing array after the object rows. FO4 expects RDOT
//! as N 52-byte object rows.

use smallvec::SmallVec;

use crate::ids::FormKey;
use crate::record::FieldValue;
use crate::sym::StringInterner;

const FO76_OBJECT_ROW_SIZE: usize = 74;
const FO76_TRAILING_ROW_SIZE: usize = 2;
const FO76_LOGICAL_ROW_SIZE: usize = FO76_OBJECT_ROW_SIZE + FO76_TRAILING_ROW_SIZE;

const OBJECTS_FIELD: &str = "objects";
const TRAILING_FIELD: &str = "trailing_unknowns";

const UNKNOWN_0_FIELD: &str = "unknown_0";
const SINK_FIELD: &str = "sink";
const SINK_VARIANCE_FIELD: &str = "sink_variance";
const DENSITY_FIELD: &str = "density";
const SIZE_VARIANCE_FIELD: &str = "size_variance";
const RADIUS_FIELD: &str = "radius";
const MIN_HEIGHT_FIELD: &str = "min_height";
const MAX_HEIGHT_FIELD: &str = "max_height";
const ANGLE_UNKNOWN_FIELD: &str = "angle_unknown";
const CLUSTERING_FIELD: &str = "clustering";
const MIN_SLOPE_FIELD: &str = "min_slope";
const MAX_SLOPE_FIELD: &str = "max_slope";
const FLAGS_FIELD: &str = "flags";
const OBJECT_FIELD: &str = "object";
const PARENT_INDEX_FIELD: &str = "parent_index";

fn bytes_value(bytes: &[u8]) -> FieldValue {
    FieldValue::Bytes(SmallVec::from_slice(bytes))
}

fn intern(interner: &StringInterner, name: &str) -> crate::sym::Sym {
    interner.intern(name)
}

fn resolve_form_id(
    raw: u32,
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let master_index = ((raw >> 24) & 0xFF) as usize;
    let object_id = raw & 0x00FF_FFFF;
    let own_index = masters.len();
    let plugin = if master_index < masters.len() {
        masters[master_index].as_str()
    } else if master_index == own_index || master_index == 0xFF {
        plugin_name
    } else {
        plugin_name
    };
    Some(FormKey {
        local: object_id,
        plugin: interner.intern(plugin),
    })
}

pub(crate) fn decode_fo76_regn_rdot(
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Option<FieldValue> {
    let layout = rdot_layout(data)?;
    let count = layout.count;
    let trailing_start = layout.trailing_start;
    let row_stride = layout.row_stride;

    let mut object_rows = Vec::with_capacity(count);
    let mut trailing_rows = Vec::with_capacity(count);

    for index in 0..count {
        let start = index.checked_mul(row_stride)?;
        let row = data.get(start..start + FO76_OBJECT_ROW_SIZE)?;
        let raw_object = u32::from_le_bytes(row[68..72].try_into().ok()?);
        let object_value = resolve_form_id(raw_object, masters, plugin_name, interner)
            .map(FieldValue::FormKey)
            .unwrap_or(FieldValue::None);

        object_rows.push(FieldValue::Struct(vec![
            (intern(interner, UNKNOWN_0_FIELD), bytes_value(&row[0..4])),
            (intern(interner, SINK_FIELD), bytes_value(&row[4..8])),
            (
                intern(interner, SINK_VARIANCE_FIELD),
                bytes_value(&row[8..12]),
            ),
            (intern(interner, DENSITY_FIELD), bytes_value(&row[12..16])),
            (
                intern(interner, SIZE_VARIANCE_FIELD),
                bytes_value(&row[16..20]),
            ),
            (intern(interner, RADIUS_FIELD), bytes_value(&row[44..48])),
            (
                intern(interner, MIN_HEIGHT_FIELD),
                bytes_value(&row[48..52]),
            ),
            (
                intern(interner, MAX_HEIGHT_FIELD),
                bytes_value(&row[52..56]),
            ),
            (
                intern(interner, ANGLE_UNKNOWN_FIELD),
                bytes_value(&row[56..64]),
            ),
            (
                intern(interner, CLUSTERING_FIELD),
                bytes_value(&row[64..65]),
            ),
            (intern(interner, MIN_SLOPE_FIELD), bytes_value(&row[65..66])),
            (intern(interner, MAX_SLOPE_FIELD), bytes_value(&row[66..67])),
            (intern(interner, FLAGS_FIELD), bytes_value(&row[67..68])),
            (intern(interner, OBJECT_FIELD), object_value),
            (
                intern(interner, PARENT_INDEX_FIELD),
                bytes_value(&row[72..74]),
            ),
        ]));

        let trailing = trailing_start
            .and_then(|start| {
                data.get(
                    start + index * FO76_TRAILING_ROW_SIZE
                        ..start + (index + 1) * FO76_TRAILING_ROW_SIZE,
                )
            })
            .unwrap_or(&[0, 0]);
        trailing_rows.push(bytes_value(trailing));
    }

    Some(FieldValue::Struct(vec![
        (
            intern(interner, OBJECTS_FIELD),
            FieldValue::List(object_rows),
        ),
        (
            intern(interner, TRAILING_FIELD),
            FieldValue::List(trailing_rows),
        ),
    ]))
}

struct RdotLayout {
    count: usize,
    row_stride: usize,
    trailing_start: Option<usize>,
}

fn rdot_layout(data: &[u8]) -> Option<RdotLayout> {
    if data.is_empty() {
        return Some(RdotLayout {
            count: 0,
            row_stride: FO76_OBJECT_ROW_SIZE,
            trailing_start: None,
        });
    }

    if data.len() % FO76_OBJECT_ROW_SIZE == 0 {
        return Some(RdotLayout {
            count: data.len() / FO76_OBJECT_ROW_SIZE,
            row_stride: FO76_OBJECT_ROW_SIZE,
            trailing_start: None,
        });
    }

    if data.len() % FO76_LOGICAL_ROW_SIZE != 0 {
        return None;
    }

    Some(RdotLayout {
        count: data.len() / FO76_LOGICAL_ROW_SIZE,
        row_stride: FO76_OBJECT_ROW_SIZE,
        trailing_start: Some((data.len() / FO76_LOGICAL_ROW_SIZE) * FO76_OBJECT_ROW_SIZE),
    })
}

pub(crate) fn convert_fo76_regn_rdot_to_fo4(
    value: &FieldValue,
    interner: &StringInterner,
) -> Option<FieldValue> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };

    let objects_sym = intern(interner, OBJECTS_FIELD);
    let trailing_sym = intern(interner, TRAILING_FIELD);
    let objects = fields.iter().find_map(|(name, value)| {
        if *name == objects_sym {
            if let FieldValue::List(items) = value {
                return Some(items.as_slice());
            }
        }
        None
    })?;
    let trailing = fields.iter().find_map(|(name, value)| {
        if *name == trailing_sym {
            if let FieldValue::List(items) = value {
                return Some(items.as_slice());
            }
        }
        None
    });

    let mut rows = Vec::with_capacity(objects.len());
    for (index, object_row) in objects.iter().enumerate() {
        if let Some(row) = convert_object_row(object_row, trailing, index, interner) {
            rows.push(row);
        }
    }

    if rows.is_empty() {
        None
    } else {
        Some(FieldValue::List(rows))
    }
}

fn convert_object_row(
    object_row: &FieldValue,
    trailing: Option<&[FieldValue]>,
    index: usize,
    interner: &StringInterner,
) -> Option<FieldValue> {
    let FieldValue::Struct(fields) = object_row else {
        return None;
    };
    let object = named_formkey(fields, OBJECT_FIELD, interner)?;
    if object.local == 0 {
        return None;
    }

    let default_parent_index = [0xFF, 0xFF];
    let default_two = [0_u8; 2];
    let default_one = [0_u8; 1];
    let default_four = [0_u8; 4];
    let default_angle_unknown = [0_u8; 8];
    let density_default = 1.0f32.to_le_bytes();
    let zero_f32 = 0.0f32.to_le_bytes();

    let parent_index =
        named_bytes(fields, PARENT_INDEX_FIELD, interner).unwrap_or(&default_parent_index);
    let trailing_unknown = trailing
        .and_then(|items| items.get(index))
        .and_then(field_bytes)
        .unwrap_or(&default_two);
    let density = named_bytes(fields, DENSITY_FIELD, interner).unwrap_or(&density_default);
    let clustering = named_bytes(fields, CLUSTERING_FIELD, interner).unwrap_or(&default_one);
    let min_slope = named_bytes(fields, MIN_SLOPE_FIELD, interner).unwrap_or(&default_one);
    let max_slope = named_bytes(fields, MAX_SLOPE_FIELD, interner).unwrap_or(&default_one);
    let flags = named_bytes(fields, FLAGS_FIELD, interner).unwrap_or(&default_one);
    let radius = named_bytes(fields, RADIUS_FIELD, interner).unwrap_or(&default_four);
    let radius_u16 = radius_float_to_u16_bytes(radius);
    let min_height = named_bytes(fields, MIN_HEIGHT_FIELD, interner).unwrap_or(&zero_f32);
    let max_height = named_bytes(fields, MAX_HEIGHT_FIELD, interner).unwrap_or(&zero_f32);
    let sink = named_bytes(fields, SINK_FIELD, interner).unwrap_or(&zero_f32);
    let sink_variance = named_bytes(fields, SINK_VARIANCE_FIELD, interner).unwrap_or(&zero_f32);
    let size_variance = named_bytes(fields, SIZE_VARIANCE_FIELD, interner).unwrap_or(&zero_f32);
    let angle_unknown =
        named_bytes(fields, ANGLE_UNKNOWN_FIELD, interner).unwrap_or(&default_angle_unknown);
    let unknown_0 = named_bytes(fields, UNKNOWN_0_FIELD, interner).unwrap_or(&default_four);

    Some(FieldValue::Struct(vec![
        (intern(interner, "object"), FieldValue::FormKey(*object)),
        (
            intern(interner, "parent_index"),
            bytes_value(take_or_zero(parent_index, 2)),
        ),
        (
            intern(interner, "unknown_2"),
            bytes_value(take_or_zero(trailing_unknown, 2)),
        ),
        (
            intern(interner, "density"),
            bytes_value(take_or_zero(density, 4)),
        ),
        (
            intern(interner, "clustering"),
            bytes_value(take_or_zero(clustering, 1)),
        ),
        (
            intern(interner, "min_slope"),
            bytes_value(take_or_zero(min_slope, 1)),
        ),
        (
            intern(interner, "max_slope"),
            bytes_value(take_or_zero(max_slope, 1)),
        ),
        (
            intern(interner, "flags"),
            bytes_value(take_or_zero(flags, 1)),
        ),
        (
            intern(interner, "radius_wrt_parent"),
            bytes_value(&radius_u16),
        ),
        (intern(interner, "radius"), bytes_value(&radius_u16)),
        (
            intern(interner, "min_height"),
            bytes_value(take_or_zero(min_height, 4)),
        ),
        (
            intern(interner, "max_height"),
            bytes_value(take_or_zero(max_height, 4)),
        ),
        (intern(interner, "sink"), bytes_value(take_or_zero(sink, 4))),
        (
            intern(interner, "sink_variance"),
            bytes_value(take_or_zero(sink_variance, 4)),
        ),
        (
            intern(interner, "size_variance"),
            bytes_value(take_or_zero(size_variance, 4)),
        ),
        (
            intern(interner, "angle_variance_x"),
            bytes_value(slice_or_empty(angle_unknown, 0, 2)),
        ),
        (
            intern(interner, "angle_variance_y"),
            bytes_value(slice_or_empty(angle_unknown, 2, 2)),
        ),
        (
            intern(interner, "angle_variance_z"),
            bytes_value(slice_or_empty(angle_unknown, 4, 2)),
        ),
        (
            intern(interner, "unknown_46"),
            bytes_value(slice_or_empty(angle_unknown, 6, 2)),
        ),
        (
            intern(interner, "unknown_48"),
            bytes_value(take_or_zero(unknown_0, 4)),
        ),
    ]))
}

fn named_bytes<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a [u8]> {
    let sym = intern(interner, name);
    fields
        .iter()
        .find_map(|(field_name, value)| (*field_name == sym).then_some(value))
        .and_then(field_bytes)
}

fn named_formkey<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FormKey> {
    let sym = intern(interner, name);
    fields.iter().find_map(|(field_name, value)| {
        if *field_name == sym {
            if let FieldValue::FormKey(fk) = value {
                return Some(fk);
            }
        }
        None
    })
}

fn field_bytes(value: &FieldValue) -> Option<&[u8]> {
    if let FieldValue::Bytes(bytes) = value {
        Some(bytes.as_slice())
    } else {
        None
    }
}

fn radius_float_to_u16_bytes(bytes: &[u8]) -> [u8; 2] {
    if bytes.len() < 4 {
        return [0, 0];
    }
    let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let clamped = if value.is_finite() {
        value.round().clamp(0.0, u16::MAX as f32) as u16
    } else {
        0
    };
    clamped.to_le_bytes()
}

fn take_or_zero(bytes: &[u8], len: usize) -> &[u8] {
    if bytes.len() >= len {
        &bytes[..len]
    } else {
        &[]
    }
}

fn slice_or_empty(bytes: &[u8], start: usize, len: usize) -> &[u8] {
    bytes.get(start..start.saturating_add(len)).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_fo76_rdot_active_xedit_layout() {
        let mut interner = StringInterner::new();
        let masters = vec!["SeventySix.esm".to_string()];
        let mut payload = vec![0_u8; FO76_LOGICAL_ROW_SIZE];
        payload[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        payload[44..48].copy_from_slice(&50.0f32.to_le_bytes());
        payload[48..52].copy_from_slice(&(-200000.0f32).to_le_bytes());
        payload[52..56].copy_from_slice(&200000.0f32.to_le_bytes());
        payload[64] = 1;
        payload[65] = 2;
        payload[66] = 3;
        payload[67] = 4;
        payload[68..72].copy_from_slice(&0x0000_0800u32.to_le_bytes());
        payload[72..74].copy_from_slice(&0xFFFFu16.to_le_bytes());

        let decoded = decode_fo76_regn_rdot(&payload, &masters, "Source.esm", &mut interner)
            .expect("RDOT decodes");
        let converted =
            convert_fo76_regn_rdot_to_fo4(&decoded, &mut interner).expect("RDOT converts");

        let FieldValue::List(rows) = converted else {
            panic!("expected FO4 RDOT rows");
        };
        assert_eq!(rows.len(), 1);
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("expected FO4 RDOT row struct");
        };
        let encoded_len: usize = fields
            .iter()
            .map(|(_, value)| match value {
                FieldValue::FormKey(_) => 4,
                FieldValue::Bytes(bytes) => bytes.len(),
                other => panic!("unexpected value in FO4 row: {other:?}"),
            })
            .sum();
        assert_eq!(encoded_len, 52);
    }

    #[test]
    fn decodes_fo76_rdot_74_byte_rows_from_object_region() {
        let mut interner = StringInterner::new();
        let masters = vec!["SeventySix.esm".to_string()];
        let payload = hex_bytes(
            "0000000000000000000000003333733FCDCC4C3D000000000000000000000000\
             0000000000000000DB0F4940000090410000000000A08C46FFFFFF000002A302\
             0032002638560100FFFF",
        );

        let decoded = decode_fo76_regn_rdot(&payload, &masters, "Source.esm", &mut interner)
            .expect("RDOT decodes");
        let converted =
            convert_fo76_regn_rdot_to_fo4(&decoded, &mut interner).expect("RDOT converts");

        let FieldValue::List(rows) = converted else {
            panic!("expected FO4 RDOT rows");
        };
        assert_eq!(rows.len(), 1);
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("expected FO4 RDOT row struct");
        };
        let object = fields
            .iter()
            .find_map(|(name, value)| (*name == interner.intern("object")).then_some(value))
            .expect("object field");
        let FieldValue::FormKey(object_fk) = object else {
            panic!("expected object FormKey");
        };
        assert_eq!(object_fk.local, 0x015638);
        assert_eq!(interner.resolve(object_fk.plugin), Some("SeventySix.esm"));
    }

    fn hex_bytes(text: &str) -> Vec<u8> {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        (0..compact.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&compact[i..i + 2], 16).unwrap())
            .collect()
    }
}
