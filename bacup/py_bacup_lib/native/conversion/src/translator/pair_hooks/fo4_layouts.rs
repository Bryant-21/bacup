//! Source-family record layouts that must be rebuilt for Fallout 4.
//!
//! These helpers deliberately operate on a short allow-list of record/subrecord
//! pairs.  Equal 4CCs are not treated as proof of an equal byte contract.

use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceFamily {
    LegacyFallout,
    SkyrimSe,
}

fn raw(value: &FieldValue) -> Option<&[u8]> {
    match value {
        FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
        _ => None,
    }
}

fn raw_value(bytes: Vec<u8>) -> FieldValue {
    FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes))
}

fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(sig),
        value,
    }
}

fn field_name<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
}

fn append_uint_defaults(value: &mut FieldValue, names: &[&str], interner: &StringInterner) {
    match value {
        FieldValue::List(items) => {
            for item in items {
                append_uint_defaults(item, names, interner);
            }
        }
        FieldValue::Struct(fields) => {
            for name in names {
                if field_name(fields, name, interner).is_none() {
                    fields.push((interner.intern(name), FieldValue::Uint(0)));
                }
            }
        }
        _ => {}
    }
}

fn append_float_defaults(value: &mut FieldValue, names: &[&str], interner: &StringInterner) {
    match value {
        FieldValue::List(items) => {
            for item in items {
                append_float_defaults(item, names, interner);
            }
        }
        FieldValue::Struct(fields) => {
            for name in names {
                if field_name(fields, name, interner).is_none() {
                    fields.push((interner.intern(name), FieldValue::Float(0.0)));
                }
            }
        }
        _ => {}
    }
}

/// REFR.XLOC shares a twelve-byte semantic prefix across these source games:
/// level/reserved bytes, key FormID, flags, and three reserved bytes.  FO4's
/// deployed contract is sixteen bytes.  The target-only four-byte tail is not
/// source data and is always defaulted instead of truncating a source payload.
pub(crate) fn normalize_refr_xloc(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"REFR" {
        return;
    }

    for entry in &mut record.fields {
        if entry.sig.0 != *b"XLOC" {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(bytes) if matches!(bytes.len(), 12 | 16 | 20) => {
                let mut target = vec![0_u8; 16];
                target[..12].copy_from_slice(&bytes[..12]);
                entry.value = raw_value(target);
            }
            FieldValue::Struct(fields) => {
                // A decoded FormKey must stay structured so the normal mapper can
                // encode it.  Retain only the proven target prefix and provide a
                // deterministic target-only tail.
                fields.retain(|(key, _)| {
                    matches!(
                        interner.resolve(*key),
                        Some("level")
                            | Some("unknown_u8_1")
                            | Some("unknown_u8_2")
                            | Some("unknown_u8_3")
                            | Some("key")
                            | Some("flags")
                            | Some("unknown_u8_6")
                            | Some("unknown_u8_7")
                            | Some("unknown_u8_8")
                    )
                });
                fields.push((interner.intern("bytes_9"), raw_value(vec![0_u8; 4])));
            }
            // Explicit malformed policy: drop XLOC. Passing an undecoded source
            // layout to the FO4 lock loader is less safe than an unlocked ref.
            _ => entry.value = FieldValue::None,
        }
    }
    record
        .fields
        .retain(|entry| entry.sig.0 != *b"XLOC" || entry.value != FieldValue::None);
}

const FO4_EFSH_DNAM_SIZE: usize = 157;

fn valid_blend_mode(value: u32) -> bool {
    value <= 11
}

fn valid_blend_op(value: u32) -> bool {
    value <= 5
}

fn valid_z_test(value: u32) -> bool {
    matches!(value, 3 | 5 | 7 | 8)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("four-byte field"),
    )
}

fn normalize_efsh_enums(target: &mut [u8]) {
    if !valid_blend_mode(read_u32(target, 0)) {
        target[0..4].copy_from_slice(&5_u32.to_le_bytes());
    }
    if !valid_blend_op(read_u32(target, 4)) {
        target[4..8].copy_from_slice(&1_u32.to_le_bytes());
    }
    if !valid_z_test(read_u32(target, 8)) {
        target[8..12].copy_from_slice(&8_u32.to_le_bytes());
    }
    if !valid_blend_mode(read_u32(target, 88)) {
        target[88..92].copy_from_slice(&6_u32.to_le_bytes());
    }
}

fn default_efsh_dnam() -> Vec<u8> {
    let mut target = vec![0_u8; FO4_EFSH_DNAM_SIZE];
    target[0..4].copy_from_slice(&5_u32.to_le_bytes()); // SourceAlpha
    target[4..8].copy_from_slice(&1_u32.to_le_bytes()); // Add
    target[8..12].copy_from_slice(&8_u32.to_le_bytes()); // AlwaysShow
    target[88..92].copy_from_slice(&6_u32.to_le_bytes()); // InvSourceAlpha
    target
}

fn copy_if_present(
    source: &[u8],
    source_offset: usize,
    target: &mut [u8],
    target_offset: usize,
    len: usize,
) {
    if source.len() >= source_offset + len {
        target[target_offset..target_offset + len]
            .copy_from_slice(&source[source_offset..source_offset + len]);
    }
}

fn build_efsh_dnam_from_raw(source: &[u8], family: SourceFamily) -> Option<Vec<u8>> {
    let valid = match family {
        // Bethesda shipped several legacy DATA revisions.  The shared prefix is
        // 224 bytes; later revisions extend it through 308 without moving it.
        SourceFamily::LegacyFallout => (224..=308).contains(&source.len()),
        SourceFamily::SkyrimSe => source.len() == 400,
    };
    if !valid {
        return None;
    }

    let mut target = default_efsh_dnam();
    copy_if_present(source, 4, &mut target, 0, 12);
    copy_if_present(source, 16, &mut target, 12, 3);
    copy_if_present(source, 20, &mut target, 16, 36);
    copy_if_present(source, 56, &mut target, 52, 3);
    copy_if_present(source, 60, &mut target, 56, 36);
    copy_if_present(source, 248, &mut target, 92, 16);

    match family {
        SourceFamily::LegacyFallout => {
            target[145..149].copy_from_slice(&u32::from(source[0]).to_le_bytes());
        }
        SourceFamily::SkyrimSe => {
            copy_if_present(source, 308, &mut target, 108, 4);
            copy_if_present(source, 312, &mut target, 112, 3);
            copy_if_present(source, 316, &mut target, 116, 3);
            copy_if_present(source, 320, &mut target, 121, 24);
            copy_if_present(source, 384, &mut target, 145, 4);
            copy_if_present(source, 388, &mut target, 149, 8);
        }
    }
    normalize_efsh_enums(&mut target);
    Some(target)
}

fn numeric_bytes(value: &FieldValue, width: usize) -> Option<Vec<u8>> {
    let raw = match value {
        FieldValue::Uint(value) => value.to_le_bytes().to_vec(),
        FieldValue::Int(value) => value.to_le_bytes().to_vec(),
        FieldValue::Float(value) => value.to_le_bytes().to_vec(),
        FieldValue::Bytes(bytes) => bytes.to_vec(),
        _ => return None,
    };
    (raw.len() >= width).then(|| raw[..width].to_vec())
}

fn build_efsh_dnam_from_struct(
    fields: &[(crate::sym::Sym, FieldValue)],
    interner: &StringInterner,
) -> Vec<u8> {
    const MAP: &[(&str, usize, usize)] = &[
        ("membrane_shader_source_blend_mode", 0, 4),
        ("membrane_shader_blend_operation", 4, 4),
        ("membrane_shader_z_test_function", 8, 4),
        ("fill_texture_effect_color_red", 12, 1),
        ("fill_texture_effect_color_green", 13, 1),
        ("fill_texture_effect_color_blue", 14, 1),
        ("fill_texture_effect_alpha_fade_in_time", 16, 4),
        ("fill_texture_effect_full_alpha_time", 20, 4),
        ("fill_texture_effect_alpha_fade_out_time", 24, 4),
        ("fill_texture_effect_presistent_alpha_ratio", 28, 4),
        ("fill_texture_effect_alpha_pulse_amplitude", 32, 4),
        ("fill_texture_effect_alpha_pulse_frequency", 36, 4),
        ("fill_texture_effect_texture_animation_speed_u", 40, 4),
        ("fill_texture_effect_texture_animation_speed_v", 44, 4),
        ("edge_effect_fall_off", 48, 4),
        ("edge_effect_color_red", 52, 1),
        ("edge_effect_color_green", 53, 1),
        ("edge_effect_color_blue", 54, 1),
        ("edge_effect_alpha_fade_in_time", 56, 4),
        ("edge_effect_full_alpha_time", 60, 4),
        ("edge_effect_alpha_fade_out_time", 64, 4),
        ("edge_effect_persistent_alpha_ratio", 68, 4),
        ("edge_effect_alpha_pulse_amplitude", 72, 4),
        ("edge_effect_alpha_pusle_frequence", 76, 4),
        ("fill_texture_effect_full_alpha_ratio", 80, 4),
        ("edge_effect_full_alpha_ratio", 84, 4),
        ("membrane_shader_dest_blend_mode", 88, 4),
        ("holes_start_time", 92, 4),
        ("holes_end_time", 96, 4),
        ("holes_start_val", 100, 4),
        ("holes_end_val", 104, 4),
        ("fill_texture_effect_color_key_2_red", 112, 1),
        ("fill_texture_effect_color_key_2_green", 113, 1),
        ("fill_texture_effect_color_key_2_blue", 114, 1),
        ("fill_texture_effect_color_key_3_red", 116, 1),
        ("fill_texture_effect_color_key_3_green", 117, 1),
        ("fill_texture_effect_color_key_3_blue", 118, 1),
        (
            "fill_texture_effect_color_key_scale_time_color_key_1_scale",
            121,
            4,
        ),
        (
            "fill_texture_effect_color_key_scale_time_color_key_2_scale",
            125,
            4,
        ),
        (
            "fill_texture_effect_color_key_scale_time_color_key_3_scale",
            129,
            4,
        ),
        (
            "fill_texture_effect_color_key_scale_time_color_key_1_time",
            133,
            4,
        ),
        (
            "fill_texture_effect_color_key_scale_time_color_key_2_time",
            137,
            4,
        ),
        (
            "fill_texture_effect_color_key_scale_time_color_key_3_time",
            141,
            4,
        ),
        ("fill_texture_effect_texture_scale_u", 149, 4),
        ("fill_texture_effect_texture_scale_v", 153, 4),
    ];

    let mut target = default_efsh_dnam();
    for &(name, offset, width) in MAP {
        if let Some(bytes) =
            field_name(fields, name, interner).and_then(|value| numeric_bytes(value, width))
        {
            target[offset..offset + width].copy_from_slice(&bytes);
        }
    }
    if let Some(bytes) = fields
        .iter()
        .rev()
        .find(|(key, _)| interner.resolve(*key) == Some("flags"))
        .and_then(|(_, value)| numeric_bytes(value, 4))
    {
        target[145..149].copy_from_slice(&bytes);
    }
    // A decoded ambient FormKey cannot be safely raw-encoded in PairCtx.  It is
    // intentionally defaulted; byte payloads are remapped by the later raw-ID
    // fixup once mapper/master context exists.
    normalize_efsh_enums(&mut target);
    target
}

/// Replaces legacy EFSH.DATA with the FO4 fv131 contract. Malformed/unknown
/// source DATA is discarded and yields a complete safe-default DNAM; raw source
/// bytes are never copied through as DATA or DNAM.
pub(crate) fn normalize_efsh(record: &mut Record, family: SourceFamily, interner: &StringInterner) {
    if record.sig.0 != *b"EFSH" {
        return;
    }

    let dnam = record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"DATA")
        .map(|entry| match &entry.value {
            FieldValue::Bytes(bytes) => build_efsh_dnam_from_raw(bytes, family),
            FieldValue::Struct(fields) => Some(build_efsh_dnam_from_struct(fields, interner)),
            _ => None,
        })
        .flatten()
        .unwrap_or_else(default_efsh_dnam);

    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(entry.sig.0, sig if sig == *b"DATA" || sig == *b"DNAM"))
        .unwrap_or(record.fields.len());
    record
        .fields
        .retain(|entry| !matches!(entry.sig.0, sig if sig == *b"DATA" || sig == *b"DNAM"));

    let mut insert_at = insert_at.min(record.fields.len());
    for sig in [*b"ICON", *b"NAM7", *b"NAM8"] {
        let mut seen = false;
        record.fields.retain(|entry| {
            if entry.sig.0 != sig {
                return true;
            }
            if seen {
                return false;
            }
            seen = true;
            true
        });
        if !seen {
            record.fields.insert(
                insert_at,
                field(sig, FieldValue::String(interner.intern(""))),
            );
            insert_at += 1;
        } else if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig.0 == sig)
            && !matches!(entry.value, FieldValue::String(_))
        {
            entry.value = FieldValue::String(interner.intern(""));
        }
    }
    record
        .fields
        .insert(insert_at, field(*b"DATA", raw_value(Vec::new())));
    record
        .fields
        .insert(insert_at + 1, field(*b"DNAM", raw_value(dnam)));
    record.fields.sort_by_key(|entry| match entry.sig.0 {
        sig if sig == *b"EDID" => 0,
        sig if sig == *b"ICON" => 1,
        sig if sig == *b"ICO2" => 2,
        sig if sig == *b"NAM7" => 3,
        sig if sig == *b"NAM8" => 4,
        sig if sig == *b"NAM9" => 5,
        sig if sig == *b"DATA" => 6,
        sig if sig == *b"DNAM" => 7,
        _ => 8,
    });
}

fn expand_raw_rows(value: &mut FieldValue, source_row: usize, target_row: usize) -> Option<usize> {
    let bytes = raw(value)?;
    if bytes.is_empty() || bytes.len() % source_row != 0 {
        return None;
    }
    let rows = bytes.len() / source_row;
    let mut target = vec![0_u8; rows * target_row];
    for (source, target) in bytes
        .chunks_exact(source_row)
        .zip(target.chunks_exact_mut(target_row))
    {
        target[..source_row].copy_from_slice(source);
    }
    *value = raw_value(target);
    Some(rows)
}

fn structured_rows(value: &FieldValue) -> Option<usize> {
    match value {
        FieldValue::List(rows) => Some(rows.len()),
        FieldValue::Struct(_) => Some(1),
        _ => None,
    }
}

fn push_zero_field(record: &mut Record, sig: [u8; 4], len: usize) {
    record.fields.push(field(sig, raw_value(vec![0_u8; len])));
}

const FO4_WTHR_NAM0_SIZE: usize = 608;
const DEFAULT_CLOUD_ROWS: usize = 32;

fn rebuild_wthr_nam0(value: &mut FieldValue, family: SourceFamily) {
    let Some(source) = raw(value) else {
        *value = raw_value(vec![0_u8; FO4_WTHR_NAM0_SIZE]);
        return;
    };
    if source.len() == FO4_WTHR_NAM0_SIZE {
        return;
    }
    let valid_len = match family {
        SourceFamily::LegacyFallout => source.len() == 160,
        SourceFamily::SkyrimSe => source.len() == 272,
    };
    if !valid_len {
        *value = raw_value(vec![0_u8; FO4_WTHR_NAM0_SIZE]);
        return;
    }

    // Both source families store four proven shared time colors in each
    // sixteen-byte semantic row. FO4 doubles each row to eight time colors;
    // early/late target-only slots default to zero. FO4 adds two high-fog rows
    // after Skyrim's seventeen rows, which likewise default to zero.
    let mut target = vec![0_u8; FO4_WTHR_NAM0_SIZE];
    for (source_row, target_row) in source.chunks_exact(16).zip(target.chunks_exact_mut(32)) {
        target_row[..16].copy_from_slice(source_row);
    }
    *value = raw_value(target);
}

fn normalize_fixed_or_default(value: &mut FieldValue, len: usize) {
    if let FieldValue::Bytes(bytes) = value
        && bytes.len() != len
    {
        *value = raw_value(vec![0_u8; len]);
    }
}

fn normalize_byte_array_or_default(value: &mut FieldValue, len: usize) {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == len => {}
        FieldValue::Bytes(_) | FieldValue::None => *value = raw_value(vec![0_u8; len]),
        // Decoded list/struct/scalar values keep normal schema-aware encoding.
        _ => {}
    }
}

fn ensure_single_required(record: &mut Record, sig: [u8; 4], len: usize) {
    let mut seen = false;
    record.fields.retain(|entry| {
        if entry.sig.0 != sig {
            return true;
        }
        if seen {
            return false;
        }
        seen = true;
        true
    });
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig.0 == sig) {
        normalize_fixed_or_default(&mut entry.value, len);
    } else {
        push_zero_field(record, sig, len);
    }
}

fn wthr_target_order(sig: [u8; 4]) -> usize {
    match sig {
        sig if sig == *b"EDID" => 0,
        sig if sig == *b"LNAM" => 100,
        sig if sig == *b"MNAM" => 101,
        sig if sig == *b"NNAM" => 102,
        sig if sig == *b"RNAM" => 103,
        sig if sig == *b"QNAM" => 104,
        sig if sig == *b"PNAM" => 105,
        sig if sig == *b"JNAM" => 106,
        sig if sig == *b"NAM0" => 107,
        sig if sig == *b"NAM4" => 108,
        sig if sig == *b"FNAM" => 109,
        sig if sig == *b"DATA" => 110,
        sig if sig == *b"NAM1" => 111,
        sig if sig == *b"SNAM" => 112,
        sig if sig == *b"TNAM" => 113,
        sig if sig == *b"IMSP" => 114,
        sig if sig == *b"WGDR" => 115,
        sig if sig == *b"DALC" => 116,
        sig if sig == *b"MODL" => 117,
        sig if sig == *b"MODT" => 118,
        sig if sig == *b"MODC" => 119,
        sig if sig == *b"MODS" => 120,
        sig if sig == *b"MODF" => 121,
        sig if sig == *b"GNAM" => 122,
        sig if sig == *b"UNAM" => 123,
        sig if sig == *b"VNAM" => 124,
        sig if sig == *b"WNAM" => 125,
        // Target texture-layer sigs end in `TX`; keep those before LNAM.
        sig if sig[2] == b'T' && sig[3] == b'X' => 10,
        _ => 200,
    }
}

/// Rebuild WTHR's expanded FO4 time-of-day layouts. Source-only DNAM is
/// removed. Missing required target fields are synthesized with explicit zero
/// defaults; source semantics are only retained in the shared prefix.
pub(crate) fn normalize_wthr(record: &mut Record, family: SourceFamily, interner: &StringInterner) {
    if record.sig.0 != *b"WTHR" {
        return;
    }
    record.fields.retain(|entry| entry.sig.0 != *b"DNAM");

    let mut pnam_rows = 1_usize;
    for entry in &mut record.fields {
        match entry.sig.0 {
            sig if sig == *b"LNAM" || sig == *b"MNAM" || sig == *b"NNAM" => {
                normalize_fixed_or_default(&mut entry.value, 4)
            }
            sig if sig == *b"RNAM" || sig == *b"QNAM" => {
                normalize_byte_array_or_default(&mut entry.value, DEFAULT_CLOUD_ROWS)
            }
            sig if sig == *b"DATA" => match &mut entry.value {
                FieldValue::Bytes(bytes) if matches!(bytes.len(), 15 | 19 | 20) => {
                    let mut target = vec![0_u8; 20];
                    target[..bytes.len()].copy_from_slice(bytes);
                    entry.value = raw_value(target);
                }
                FieldValue::Struct(_) => append_uint_defaults(
                    &mut entry.value,
                    &[
                        "visual_effect_begin",
                        "visual_effect_end",
                        "wind_direction",
                        "wind_direction_range",
                        "wind_turbulance",
                    ],
                    interner,
                ),
                _ => entry.value = raw_value(vec![0_u8; 20]),
            },
            sig if sig == *b"PNAM" => {
                if raw(&entry.value).is_some() {
                    if let Some(rows) = expand_raw_rows(&mut entry.value, 16, 32) {
                        pnam_rows = rows;
                    } else {
                        pnam_rows = DEFAULT_CLOUD_ROWS;
                        entry.value = raw_value(vec![0_u8; pnam_rows * 32]);
                    }
                } else {
                    append_uint_defaults(
                        &mut entry.value,
                        &[
                            "cloud_colors_early_sunrise_red",
                            "cloud_colors_early_sunrise_green",
                            "cloud_colors_early_sunrise_blue",
                            "unknown_u8_19",
                            "cloud_colors_late_sunrise_red",
                            "cloud_colors_late_sunrise_green",
                            "cloud_colors_late_sunrise_blue",
                            "unknown_u8_23",
                            "cloud_colors_early_sunset_red",
                            "cloud_colors_early_sunset_green",
                            "cloud_colors_early_sunset_blue",
                            "unknown_u8_27",
                            "cloud_colors_late_sunset_red",
                            "cloud_colors_late_sunset_green",
                            "cloud_colors_late_sunset_blue",
                            "unknown_u8_31",
                        ],
                        interner,
                    );
                    pnam_rows = structured_rows(&entry.value).unwrap_or(1).max(1);
                }
            }
            sig if sig == *b"NAM0" => rebuild_wthr_nam0(&mut entry.value, family),
            sig if sig == *b"NAM4" => {
                if let FieldValue::Bytes(bytes) = &entry.value
                    && (bytes.is_empty() || bytes.len() % 4 != 0)
                {
                    entry.value = raw_value(vec![0_u8; pnam_rows * 4]);
                }
            }
            sig if sig == *b"JNAM" => {
                if raw(&entry.value).is_some() {
                    if expand_raw_rows(&mut entry.value, 16, 32).is_none() {
                        entry.value = raw_value(vec![0_u8; pnam_rows * 32]);
                    }
                } else {
                    append_float_defaults(
                        &mut entry.value,
                        &[
                            "cloud_alphas_early_sunrise",
                            "cloud_alphas_late_sunrise",
                            "cloud_alphas_early_sunset",
                            "cloud_alphas_late_sunset",
                        ],
                        interner,
                    );
                }
            }
            sig if sig == *b"FNAM" => match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() == 72 => {}
                FieldValue::Bytes(bytes)
                    if family == SourceFamily::SkyrimSe && bytes.len() == 32 =>
                {
                    let mut target = vec![0_u8; 72];
                    target[..32].copy_from_slice(bytes);
                    entry.value = raw_value(target);
                }
                FieldValue::Struct(_) if family == SourceFamily::SkyrimSe => {
                    append_float_defaults(
                        &mut entry.value,
                        &[
                            "day_near_height_mid",
                            "day_near_height_range",
                            "night_near_height_mid",
                            "night_near_height_range",
                            "day_high_density_scale",
                            "night_high_density_scale",
                            "day_far_height_mid",
                            "day_far_height_range",
                            "night_far_height_mid",
                            "night_far_height_range",
                        ],
                        interner,
                    );
                }
                _ => entry.value = raw_value(vec![0_u8; 72]),
            },
            sig if sig == *b"IMSP" => {
                if raw(&entry.value).is_some() {
                    if expand_raw_rows(&mut entry.value, 16, 32).is_none() {
                        entry.value = raw_value(vec![0_u8; 32]);
                    }
                } else {
                    append_uint_defaults(
                        &mut entry.value,
                        &[
                            "early_sunrise",
                            "late_sunrise",
                            "early_sunset",
                            "late_sunset",
                        ],
                        interner,
                    );
                }
            }
            sig if sig == *b"NAM1" => normalize_fixed_or_default(&mut entry.value, 4),
            sig if sig == *b"UNAM" => normalize_fixed_or_default(&mut entry.value, 24),
            sig if sig == *b"VNAM" || sig == *b"WNAM" => {
                normalize_fixed_or_default(&mut entry.value, 4)
            }
            sig if sig == *b"DALC" => {
                if raw(&entry.value).is_some() {
                    if expand_raw_rows(&mut entry.value, 24, 32).is_none() {
                        entry.value = raw_value(vec![0_u8; 32]);
                    }
                } else {
                    append_uint_defaults(
                        &mut entry.value,
                        &[
                            "ambient_colors_specular_red",
                            "ambient_colors_specular_green",
                            "ambient_colors_specular_blue",
                            "unknown_u8_27",
                        ],
                        interner,
                    );
                    append_float_defaults(
                        &mut entry.value,
                        &["ambient_colors_fresnel_power"],
                        interner,
                    );
                }
            }
            _ => {}
        }
    }

    if !record.fields.iter().any(|entry| entry.sig.0 == *b"PNAM") {
        push_zero_field(record, *b"PNAM", DEFAULT_CLOUD_ROWS * 32);
        pnam_rows = DEFAULT_CLOUD_ROWS;
    }
    if !record.fields.iter().any(|entry| entry.sig.0 == *b"JNAM") {
        push_zero_field(record, *b"JNAM", pnam_rows * 32);
    }
    if !record.fields.iter().any(|entry| entry.sig.0 == *b"FNAM") {
        push_zero_field(record, *b"FNAM", 72);
    }
    if !record.fields.iter().any(|entry| entry.sig.0 == *b"IMSP") {
        push_zero_field(record, *b"IMSP", 32);
    }

    let mut dalc_seen = 0_usize;
    record.fields.retain(|entry| {
        if entry.sig.0 != *b"DALC" {
            return true;
        }
        dalc_seen += 1;
        dalc_seen <= 8
    });
    while dalc_seen < 8 {
        push_zero_field(record, *b"DALC", 32);
        dalc_seen += 1;
    }

    for (sig, len) in [
        (*b"LNAM", 4),
        (*b"MNAM", 4),
        (*b"NNAM", 4),
        (*b"RNAM", DEFAULT_CLOUD_ROWS),
        (*b"QNAM", DEFAULT_CLOUD_ROWS),
        (*b"PNAM", pnam_rows * 32),
        (*b"JNAM", pnam_rows * 32),
        (*b"NAM0", FO4_WTHR_NAM0_SIZE),
        (*b"NAM4", pnam_rows * 4),
        (*b"FNAM", 72),
        (*b"DATA", 20),
        (*b"NAM1", 4),
        (*b"IMSP", 32),
        (*b"UNAM", 24),
        (*b"VNAM", 4),
        (*b"WNAM", 4),
    ] {
        ensure_single_required(record, sig, len);
    }

    // Rebuild target-required fields into FO4 schema order. Stable sorting
    // preserves repeated DALC rows and optional fields within each rank.
    record
        .fields
        .sort_by_key(|entry| wthr_target_order(entry.sig.0));
}

/// Skyrim's 92-byte projectile DATA is not the FO4 DATA contract. Rebuild the
/// exact FO4 DATA(empty)+DNAM(93) pair and default unsafe source SOUN slots.
pub(crate) fn normalize_skyrim_proj(record: &mut Record) {
    if record.sig.0 != *b"PROJ" {
        return;
    }
    let source = record.fields.iter().find_map(|entry| {
        (entry.sig.0 == *b"DATA")
            .then(|| raw(&entry.value))
            .flatten()
            .filter(|bytes| bytes.len() == 92)
    });
    let mut target = vec![0_u8; 93];
    target[2..4].copy_from_slice(&1_u16.to_le_bytes());
    if let Some(source) = source {
        // Intersection by semantic enum name, not merely matching bit value.
        // 0x0010 and 0x0800 have different/unknown meanings across the games.
        let flags = u16::from_le_bytes([source[0], source[1]]) & 0x07ef;
        target[0..2].copy_from_slice(&flags.to_le_bytes());
        let source_type = u16::from_le_bytes([source[2], source[3]]);
        let target_type = match source_type {
            1 | 2 | 4 | 8 | 16 | 32 | 64 => source_type,
            _ => 1,
        };
        target[2..4].copy_from_slice(&target_type.to_le_bytes());
        for (source_offset, target_offset) in [
            (4, 4),
            (8, 8),
            (12, 12),
            (16, 16),
            (20, 20),
            (28, 24),
            (32, 28),
            (36, 32),
            (40, 36),
            (44, 40),
            (48, 44),
            (52, 48),
            (56, 52),
            (60, 56),
            (64, 60),
            (68, 64),
            (72, 68),
            (76, 72),
            (80, 76),
            (84, 80),
            (88, 84),
        ] {
            target[target_offset..target_offset + 4]
                .copy_from_slice(&source[source_offset..source_offset + 4]);
        }
        // Skyrim and FO4 both type the three sound slots as SNDR. The later
        // schema-aware raw FormID fixup remaps these copied refs with mapper
        // context. Target-only tracer frequency and VATS PROJ remain zero.
    }

    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(entry.sig.0, sig if sig == *b"DATA" || sig == *b"DNAM" || sig == *b"NAM2"))
        .unwrap_or(record.fields.len());
    record.fields.retain(|entry| {
        !matches!(entry.sig.0, sig if sig == *b"DATA" || sig == *b"DNAM" || sig == *b"NAM2")
    });
    let insert_at = insert_at.min(record.fields.len());
    record
        .fields
        .insert(insert_at, field(*b"DATA", raw_value(Vec::new())));
    record
        .fields
        .insert(insert_at + 1, field(*b"DNAM", raw_value(target)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};

    fn record(sig: &str, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey::parse("000800@Test.esm", interner).unwrap(),
        )
    }

    fn push(record: &mut Record, sig: [u8; 4], bytes: Vec<u8>) {
        record.fields.push(field(sig, raw_value(bytes)));
    }

    fn bytes<'a>(record: &'a Record, sig: &[u8; 4]) -> &'a [u8] {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *sig)
            .and_then(|entry| raw(&entry.value))
            .unwrap()
    }

    const WTHR_REQUIRED_ORDER: &[&str] = &[
        "LNAM", "MNAM", "NNAM", "RNAM", "QNAM", "PNAM", "JNAM", "NAM0", "NAM4", "FNAM", "DATA",
        "NAM1", "IMSP", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC", "UNAM",
        "VNAM", "WNAM",
    ];

    fn assert_wthr_contract(wthr: &Record, cloud_rows: usize) {
        let required: Vec<_> = wthr
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .filter(|sig| WTHR_REQUIRED_ORDER.contains(sig))
            .collect();
        assert_eq!(required, WTHR_REQUIRED_ORDER);
        for (sig, len) in [
            (*b"LNAM", 4),
            (*b"MNAM", 4),
            (*b"NNAM", 4),
            (*b"RNAM", 32),
            (*b"QNAM", 32),
            (*b"PNAM", cloud_rows * 32),
            (*b"JNAM", cloud_rows * 32),
            (*b"NAM0", 608),
            (*b"NAM4", cloud_rows * 4),
            (*b"FNAM", 72),
            (*b"DATA", 20),
            (*b"NAM1", 4),
            (*b"IMSP", 32),
            (*b"UNAM", 24),
            (*b"VNAM", 4),
            (*b"WNAM", 4),
        ] {
            let sig_name = std::str::from_utf8(&sig).unwrap();
            assert_eq!(bytes(wthr, &sig).len(), len, "{sig_name} length");
            assert_eq!(
                wthr.fields
                    .iter()
                    .filter(|entry| entry.sig.0 == sig)
                    .count(),
                1,
                "{sig_name} count"
            );
        }
        let dalc: Vec<_> = wthr
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"DALC")
            .collect();
        assert_eq!(dalc.len(), 8);
        assert!(
            dalc.iter()
                .all(|entry| raw(&entry.value).unwrap().len() == 32)
        );
    }

    fn assert_efsh_contract(efsh: &Record) {
        let required: Vec<_> = efsh
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .filter(|sig| ["ICON", "NAM7", "NAM8", "DATA", "DNAM"].contains(sig))
            .collect();
        assert_eq!(required, ["ICON", "NAM7", "NAM8", "DATA", "DNAM"]);
        for sig in [*b"ICON", *b"NAM7", *b"NAM8", *b"DATA", *b"DNAM"] {
            assert_eq!(
                efsh.fields
                    .iter()
                    .filter(|entry| entry.sig.0 == sig)
                    .count(),
                1
            );
        }
        assert!(matches!(
            efsh.fields
                .iter()
                .find(|entry| entry.sig.0 == *b"ICON")
                .map(|entry| &entry.value),
            Some(FieldValue::String(_))
        ));
        assert!(bytes(efsh, b"DATA").is_empty());
        assert_eq!(bytes(efsh, b"DNAM").len(), 157);
    }

    #[test]
    fn xloc_rebuilds_legacy_variants_and_is_record_scoped() {
        let interner = StringInterner::new();
        for source_len in [12, 20] {
            let mut refr = record("REFR", &interner);
            let source: Vec<u8> = (0..source_len as u8).collect();
            push(&mut refr, *b"XLOC", source.clone());
            normalize_refr_xloc(&mut refr, &interner);
            assert_eq!(bytes(&refr, b"XLOC"), [&source[..12], &[0; 4]].concat());
        }

        let mut non_target = record("TERM", &interner);
        push(&mut non_target, *b"XLOC", vec![0xAA; 20]);
        let before = non_target.fields.clone();
        normalize_refr_xloc(&mut non_target, &interner);
        assert_eq!(non_target.fields, before);
    }

    #[test]
    fn efsh_fnv_224_maps_shared_fields_and_builds_required_contract() {
        let interner = StringInterner::new();
        let mut source = vec![0_u8; 224];
        source[0] = 0x35;
        source[4..8].copy_from_slice(&5_u32.to_le_bytes());
        source[8..12].copy_from_slice(&1_u32.to_le_bytes());
        source[12..16].copy_from_slice(&7_u32.to_le_bytes());
        source[16..19].copy_from_slice(&[10, 20, 30]);
        source[20..24].copy_from_slice(&1.25_f32.to_le_bytes());
        let mut efsh = record("EFSH", &interner);
        push(&mut efsh, *b"DATA", source);

        normalize_efsh(&mut efsh, SourceFamily::LegacyFallout, &interner);

        assert_efsh_contract(&efsh);
        let dnam = bytes(&efsh, b"DNAM");
        assert_eq!(&dnam[0..12], &[5, 0, 0, 0, 1, 0, 0, 0, 7, 0, 0, 0]);
        assert_eq!(&dnam[12..15], &[10, 20, 30]);
        assert_eq!(&dnam[16..20], &1.25_f32.to_le_bytes());
        assert_eq!(u32::from_le_bytes(dnam[145..149].try_into().unwrap()), 0x35);
    }

    #[test]
    fn efsh_fo3_224_builds_the_same_complete_fo4_contract() {
        let interner = StringInterner::new();
        let mut source = vec![0_u8; 224];
        source[4..8].copy_from_slice(&5_u32.to_le_bytes());
        source[8..12].copy_from_slice(&1_u32.to_le_bytes());
        source[12..16].copy_from_slice(&7_u32.to_le_bytes());
        let mut efsh = record("EFSH", &interner);
        push(&mut efsh, *b"DATA", source);

        normalize_efsh(&mut efsh, SourceFamily::LegacyFallout, &interner);

        assert_efsh_contract(&efsh);
    }

    #[test]
    fn efsh_skyrim_400_maps_extended_fields_without_carrying_data() {
        let interner = StringInterner::new();
        let mut source = vec![0_u8; 400];
        source[4..8].copy_from_slice(&5_u32.to_le_bytes());
        source[8..12].copy_from_slice(&1_u32.to_le_bytes());
        source[12..16].copy_from_slice(&7_u32.to_le_bytes());
        source[248..264].copy_from_slice(&[0x11; 16]);
        source[308..312].copy_from_slice(&0x0012_3456_u32.to_le_bytes());
        source[312..315].copy_from_slice(&[1, 2, 3]);
        source[316..319].copy_from_slice(&[4, 5, 6]);
        source[320..344].copy_from_slice(&[0x22; 24]);
        source[384..388].copy_from_slice(&0x1122_3344_u32.to_le_bytes());
        source[388..396].copy_from_slice(&[0x33; 8]);
        let mut efsh = record("EFSH", &interner);
        push(&mut efsh, *b"DATA", source);

        normalize_efsh(&mut efsh, SourceFamily::SkyrimSe, &interner);

        assert_efsh_contract(&efsh);
        let dnam = bytes(&efsh, b"DNAM");
        assert_eq!(&dnam[92..108], &[0x11; 16]);
        assert_eq!(&dnam[108..112], &0x0012_3456_u32.to_le_bytes());
        assert_eq!(&dnam[112..115], &[1, 2, 3]);
        assert_eq!(&dnam[116..119], &[4, 5, 6]);
        assert_eq!(&dnam[121..145], &[0x22; 24]);
        assert_eq!(&dnam[145..149], &0x1122_3344_u32.to_le_bytes());
        assert_eq!(&dnam[149..157], &[0x33; 8]);
    }

    #[test]
    fn malformed_efsh_data_is_dropped_to_valid_defaults() {
        let interner = StringInterner::new();
        let mut efsh = record("EFSH", &interner);
        push(&mut efsh, *b"DATA", vec![0xCC; 17]);
        push(&mut efsh, *b"DNAM", vec![0xDD; 400]);

        normalize_efsh(&mut efsh, SourceFamily::LegacyFallout, &interner);

        assert_efsh_contract(&efsh);
        let dnam = bytes(&efsh, b"DNAM");
        assert_eq!(u32::from_le_bytes(dnam[8..12].try_into().unwrap()), 8);
        assert!(!dnam.contains(&0xCC));
        assert!(!dnam.contains(&0xDD));
    }

    fn legacy_wthr_fixture(interner: &StringInterner) -> Record {
        let mut wthr = record("WTHR", interner);
        push(&mut wthr, *b"LNAM", 32_u32.to_le_bytes().to_vec());
        push(&mut wthr, *b"RNAM", (0_u8..32).collect());
        push(&mut wthr, *b"QNAM", (32_u8..64).collect());
        push(&mut wthr, *b"DATA", (0_u8..15).collect());
        push(&mut wthr, *b"PNAM", (0_u8..32).collect());
        let mut nam0 = vec![0_u8; 160];
        for (index, byte) in nam0.iter_mut().enumerate() {
            *byte = (index & 0xff) as u8;
        }
        push(&mut wthr, *b"NAM0", nam0);
        push(&mut wthr, *b"FNAM", vec![0x5A; 72]);
        wthr
    }

    fn assert_legacy_wthr_rebuild(mut wthr: Record, interner: &StringInterner) {
        normalize_wthr(&mut wthr, SourceFamily::LegacyFallout, interner);
        assert_wthr_contract(&wthr, 2);
        assert_eq!(
            &bytes(&wthr, b"PNAM")[..16],
            &(0_u8..16).collect::<Vec<_>>()
        );
        assert_eq!(&bytes(&wthr, b"PNAM")[16..32], &[0; 16]);
        assert_eq!(
            &bytes(&wthr, b"NAM0")[..16],
            &(0_u8..16).collect::<Vec<_>>()
        );
        assert_eq!(&bytes(&wthr, b"NAM0")[16..32], &[0; 16]);
        assert_eq!(bytes(&wthr, b"FNAM"), &[0x5A; 72]);
    }

    #[test]
    fn wthr_fnv_rebuilds_complete_fo4_required_contract() {
        let interner = StringInterner::new();
        assert_legacy_wthr_rebuild(legacy_wthr_fixture(&interner), &interner);
    }

    #[test]
    fn wthr_fo3_rebuilds_complete_fo4_required_contract() {
        let interner = StringInterner::new();
        assert_legacy_wthr_rebuild(legacy_wthr_fixture(&interner), &interner);
    }

    #[test]
    fn wthr_skyrim_maps_all_source_layouts_and_fills_late_times() {
        let interner = StringInterner::new();
        let mut wthr = record("WTHR", &interner);
        push(&mut wthr, *b"DATA", vec![0x10; 19]);
        push(&mut wthr, *b"PNAM", vec![0x20; 16]);
        push(&mut wthr, *b"JNAM", vec![0x30; 16]);
        let mut nam0 = vec![0_u8; 272];
        nam0[..16].copy_from_slice(&[0x35; 16]);
        push(&mut wthr, *b"NAM0", nam0);
        push(&mut wthr, *b"FNAM", vec![0x40; 32]);
        push(&mut wthr, *b"IMSP", vec![0x50; 16]);
        for _ in 0..4 {
            push(&mut wthr, *b"DALC", vec![0x60; 24]);
        }

        normalize_wthr(&mut wthr, SourceFamily::SkyrimSe, &interner);

        assert_wthr_contract(&wthr, 1);
        for (sig, source_len, target_len, fill) in [
            (*b"DATA", 19, 20, 0x10),
            (*b"PNAM", 16, 32, 0x20),
            (*b"JNAM", 16, 32, 0x30),
            (*b"FNAM", 32, 72, 0x40),
            (*b"IMSP", 16, 32, 0x50),
        ] {
            let output = bytes(&wthr, &sig);
            assert_eq!(output.len(), target_len);
            assert_eq!(&output[..source_len], vec![fill; source_len]);
            assert!(output[source_len..].iter().all(|byte| *byte == 0));
        }
        let dalc: Vec<_> = wthr
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"DALC")
            .map(|entry| raw(&entry.value).unwrap())
            .collect();
        assert_eq!(dalc.len(), 8);
        assert_eq!(&dalc[0][..24], &[0x60; 24]);
        assert!(dalc[0][24..].iter().all(|byte| *byte == 0));
        assert!(dalc[4].iter().all(|byte| *byte == 0));
        assert_eq!(&bytes(&wthr, b"NAM0")[..16], &[0x35; 16]);
        assert_eq!(&bytes(&wthr, b"NAM0")[16..32], &[0; 16]);
        assert!(bytes(&wthr, b"NAM0")[576..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn malformed_wthr_source_fields_never_raw_pass_into_required_contract() {
        let interner = StringInterner::new();
        let mut wthr = record("WTHR", &interner);
        for sig in [
            *b"LNAM", *b"MNAM", *b"NNAM", *b"RNAM", *b"QNAM", *b"PNAM", *b"JNAM", *b"NAM0",
            *b"NAM4", *b"FNAM", *b"DATA", *b"NAM1", *b"IMSP", *b"DALC", *b"UNAM", *b"VNAM",
            *b"WNAM",
        ] {
            push(&mut wthr, sig, vec![0xCC; 3]);
        }

        normalize_wthr(&mut wthr, SourceFamily::SkyrimSe, &interner);

        assert_wthr_contract(&wthr, DEFAULT_CLOUD_ROWS);
        for entry in &wthr.fields {
            if WTHR_REQUIRED_ORDER.contains(&entry.sig.as_str()) {
                assert!(!raw(&entry.value).unwrap().contains(&0xCC));
            }
        }
    }

    #[test]
    fn skyrim_proj_rebuilds_dnam_and_drops_legacy_nam2() {
        let interner = StringInterner::new();
        let mut source = vec![0_u8; 92];
        source[0..2].copy_from_slice(&0xffff_u16.to_le_bytes());
        source[2..4].copy_from_slice(&16_u16.to_le_bytes());
        for offset in (4..92).step_by(4) {
            source[offset..offset + 4].copy_from_slice(&(offset as u32).to_le_bytes());
        }
        let mut proj = record("PROJ", &interner);
        push(&mut proj, *b"DATA", source);
        push(&mut proj, *b"NAM2", vec![0xAA; 40]);

        normalize_skyrim_proj(&mut proj);

        assert!(bytes(&proj, b"DATA").is_empty());
        let dnam = bytes(&proj, b"DNAM");
        assert_eq!(dnam.len(), 93);
        assert_eq!(u16::from_le_bytes(dnam[0..2].try_into().unwrap()), 0x07ef);
        assert_eq!(u16::from_le_bytes(dnam[2..4].try_into().unwrap()), 16);
        assert_eq!(u32::from_le_bytes(dnam[24..28].try_into().unwrap()), 28);
        assert_eq!(u32::from_le_bytes(dnam[80..84].try_into().unwrap()), 84);
        assert_eq!(u32::from_le_bytes(dnam[84..88].try_into().unwrap()), 88);
        for (offset, expected) in [(36, 40), (52, 56), (56, 60)] {
            assert_eq!(
                u32::from_le_bytes(dnam[offset..offset + 4].try_into().unwrap()),
                expected
            );
        }
        assert_eq!(u32::from_le_bytes(dnam[89..93].try_into().unwrap()), 0);
        assert!(!proj.fields.iter().any(|entry| entry.sig.0 == *b"NAM2"));
    }

    #[test]
    fn skyrim_proj_preserves_shared_extended_types_aim_flag_and_sndr_refs() {
        let interner = StringInterner::new();
        for projectile_type in [16_u16, 32, 64] {
            let mut source = vec![0_u8; 92];
            source[0..2].copy_from_slice(&0x0400_u16.to_le_bytes());
            source[2..4].copy_from_slice(&projectile_type.to_le_bytes());
            source[40..44].copy_from_slice(&0x0011_1111_u32.to_le_bytes());
            source[56..60].copy_from_slice(&0x0022_2222_u32.to_le_bytes());
            source[60..64].copy_from_slice(&0x0033_3333_u32.to_le_bytes());
            let mut proj = record("PROJ", &interner);
            push(&mut proj, *b"DATA", source);

            normalize_skyrim_proj(&mut proj);

            let dnam = bytes(&proj, b"DNAM");
            assert_eq!(u16::from_le_bytes(dnam[0..2].try_into().unwrap()), 0x0400);
            assert_eq!(
                u16::from_le_bytes(dnam[2..4].try_into().unwrap()),
                projectile_type
            );
            assert_eq!(
                u32::from_le_bytes(dnam[36..40].try_into().unwrap()),
                0x0011_1111
            );
            assert_eq!(
                u32::from_le_bytes(dnam[52..56].try_into().unwrap()),
                0x0022_2222
            );
            assert_eq!(
                u32::from_le_bytes(dnam[56..60].try_into().unwrap()),
                0x0033_3333
            );
        }
    }

    #[test]
    fn all_layout_helpers_leave_unrelated_record_untouched() {
        let interner = StringInterner::new();
        let mut stat = record("STAT", &interner);
        push(&mut stat, *b"DATA", vec![1, 2, 3]);
        push(&mut stat, *b"PNAM", vec![4, 5, 6]);
        let before = stat.fields.clone();

        normalize_refr_xloc(&mut stat, &interner);
        normalize_efsh(&mut stat, SourceFamily::LegacyFallout, &interner);
        normalize_wthr(&mut stat, SourceFamily::SkyrimSe, &interner);
        normalize_skyrim_proj(&mut stat);

        assert_eq!(stat.fields, before);
    }
}
