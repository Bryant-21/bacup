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
    Starfield,
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

fn copy_structured_fields(
    value: &mut FieldValue,
    mappings: &[(&str, &str)],
    interner: &StringInterner,
) {
    match value {
        FieldValue::List(items) => {
            for item in items {
                copy_structured_fields(item, mappings, interner);
            }
        }
        FieldValue::Struct(fields) => {
            let additions = mappings
                .iter()
                .filter(|(_, target)| field_name(fields, target, interner).is_none())
                .filter_map(|(source, target)| {
                    field_name(fields, source, interner)
                        .cloned()
                        .map(|value| (interner.intern(target), value))
                })
                .collect::<Vec<_>>();
            fields.extend(additions);
        }
        _ => {}
    }
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
        // Starfield's EFSH DATA/DNAM layout has not been mapped. Reusing a
        // legacy family's offsets would silently reinterpret unrelated bytes,
        // so take the safe-default DNAM instead of fabricating field values.
        SourceFamily::Starfield => false,
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
        // Unreachable: the length gate above rejects Starfield outright.
        SourceFamily::Starfield => {}
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

fn push_zero_field(record: &mut Record, sig: [u8; 4], len: usize) {
    record.fields.push(field(sig, raw_value(vec![0_u8; len])));
}

const FO4_WTHR_NAM0_SIZE: usize = 608;
const FO4_WTHR_NAM0_ROW: usize = 32;
/// Byte offset of the `ambient` colour in a Fallout 4 `NAM0`.  It is the fourth
/// category; the third (offset 64) is the engine's unused slot and is zero in
/// every vanilla weather.
const FO4_WTHR_NAM0_AMBIENT: usize = 96;
/// FO3 stores four periods per weather colour, so each of its ten colours is a
/// 16-byte row (160 bytes total).  FNV keeps the same ten colours in the same
/// order but adds Noon and Midnight, widening every row to 24 bytes (240
/// total).  Neither extra period has a Fallout 4 slot, so only the leading four
/// are carried — but the row width must still be read from the source length or
/// every colour past the first lands in the wrong category.
const FO3_WEATHER_PERIODS: usize = 4;
const FNV_WEATHER_PERIODS: usize = 6;
const FNV_WTHR_NAM0_SIZE: usize = 240;
/// Fallout 4 reads cloud layer speed as an offset from a still 127.
const FO4_WTHR_CLOUD_SPEED_STILL: u8 = 127;
/// Fallout 4's fog maximum, absent from the legacy 24-byte fog block.
const FO4_WTHR_FOG_MAX_DAY: f32 = 0.85;
const FO4_WTHR_FOG_MAX_NIGHT: f32 = 0.95;
/// FO3 and FNV name their four cloud layer textures rather than numbering them.
const LEGACY_CLOUD_LAYER_SIGS: [[u8; 4]; 4] = [*b"DNAM", *b"CNAM", *b"ANAM", *b"BNAM"];
const DEFAULT_CLOUD_ROWS: usize = 32;
const LEGACY_TO_FO4_WEATHER_PERIODS: [usize; 8] = [0, 1, 2, 3, 3, 0, 1, 2];
const FO4_GDRY_NONE: u32 = 0x001B_40E8;

fn fo4_cloud_layer_sig(layer: usize) -> [u8; 4] {
    [b'0' + layer as u8, b'0', b'T', b'X']
}

#[derive(Clone, Copy)]
enum LegacyWeatherProfile {
    None,
    Clear,
    Fog,
    Rain,
    Overcast,
    Radstorm,
    Dusty,
}

/// How many periods each periodised weather block stores in the source record.
///
/// FO3 stores four (Sunrise, Day, Sunset, Night). FNV widened every one of them
/// to six by appending Noon and Midnight, neither of which has a Fallout 4
/// slot. `NAM0` is required on every legacy weather and is the only block whose
/// length distinguishes the two unambiguously, so it drives `PNAM` too — and it
/// must be probed before the rewrite loop reshapes `NAM0` into target layout.
fn legacy_weather_source_periods(record: &Record) -> usize {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"NAM0")
        .and_then(|entry| raw(&entry.value))
        .map_or(FO3_WEATHER_PERIODS, |bytes| {
            if bytes.len() == FNV_WTHR_NAM0_SIZE {
                FNV_WEATHER_PERIODS
            } else {
                FO3_WEATHER_PERIODS
            }
        })
}

fn expand_legacy_weather_periods(
    source: &[u8],
    period_width: usize,
    source_periods: usize,
) -> Option<Vec<u8>> {
    if period_width == 0 || source.len() != period_width * source_periods {
        return None;
    }
    let mut target = vec![0_u8; period_width * 8];
    // Both games order the leading four periods identically, so FNV's extra two
    // are simply left behind rather than remapped onto Fallout 4 slots.
    for (target_period, source_period) in LEGACY_TO_FO4_WEATHER_PERIODS.iter().enumerate() {
        let source_start = source_period * period_width;
        let target_start = target_period * period_width;
        target[target_start..target_start + period_width]
            .copy_from_slice(&source[source_start..source_start + period_width]);
    }
    Some(target)
}

fn expand_legacy_weather_period_rows(
    value: &mut FieldValue,
    period_width: usize,
    already_target_layout: bool,
    source_periods: usize,
) -> Option<usize> {
    let bytes = raw(value)?;
    let source_row = period_width * source_periods;
    let target_row = period_width * 8;
    if already_target_layout {
        return (!bytes.is_empty() && bytes.len() % target_row == 0)
            .then(|| bytes.len() / target_row);
    }
    if bytes.is_empty() || bytes.len() % source_row != 0 {
        return None;
    }
    let rows = bytes.len() / source_row;
    let mut target = Vec::with_capacity(rows * target_row);
    for source in bytes.chunks_exact(source_row) {
        target.extend_from_slice(&expand_legacy_weather_periods(
            source,
            period_width,
            source_periods,
        )?);
    }
    *value = raw_value(target);
    Some(rows)
}

fn resize_raw_row_array(value: &mut FieldValue, row_width: usize, target_rows: usize) -> bool {
    let Some(bytes) = raw(value) else {
        return false;
    };
    if row_width == 0 || bytes.len() % row_width != 0 {
        return false;
    }
    let target_len = row_width * target_rows;
    let mut target = bytes[..bytes.len().min(target_len)].to_vec();
    target.resize(target_len, 0);
    *value = raw_value(target);
    true
}

fn resize_structured_row_array(value: &mut FieldValue, target_rows: usize) {
    if matches!(value, FieldValue::Struct(_)) {
        let first = std::mem::replace(value, FieldValue::None);
        *value = FieldValue::List(vec![first]);
    }
    if let FieldValue::List(rows) = value {
        rows.truncate(target_rows);
        rows.resize_with(target_rows, || FieldValue::Struct(Vec::new()));
    }
}

fn rebuild_legacy_wthr_nam0(value: &mut FieldValue, source_periods: usize) {
    let Some(source) = raw(value) else {
        *value = raw_value(vec![0_u8; FO4_WTHR_NAM0_SIZE]);
        return;
    };
    if source.len() == FO4_WTHR_NAM0_SIZE {
        return;
    }
    // Both source games carry the same ten colours; only the period count
    // differs, so the row width is what tells them apart.  Leave the FO4-only
    // trailing categories at zero instead of blacking the entire weather.
    let source_row = source_periods * 4;
    if source.is_empty() || source.len() % source_row != 0 {
        *value = raw_value(vec![0_u8; FO4_WTHR_NAM0_SIZE]);
        return;
    }

    let mut target = vec![0_u8; FO4_WTHR_NAM0_SIZE];
    for (source_colour, target_colour) in source
        .chunks_exact(source_row)
        .zip(target.chunks_exact_mut(FO4_WTHR_NAM0_ROW))
    {
        target_colour.copy_from_slice(
            &expand_legacy_weather_periods(source_colour, 4, source_periods)
                .expect("source weather color periods"),
        );
    }
    *value = raw_value(target);
}

/// Rename FO3/FNV's named cloud layer textures to Fallout 4's numbered layers.
/// Dropping them leaves the sky with no cloud layers at all.
fn carry_legacy_cloud_layer_textures(record: &mut Record) {
    for (layer, legacy) in LEGACY_CLOUD_LAYER_SIGS.iter().enumerate() {
        let target = fo4_cloud_layer_sig(layer);
        if record.fields.iter().any(|entry| entry.sig.0 == target) {
            continue;
        }
        if let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *legacy && !matches!(entry.value, FieldValue::None))
        {
            entry.sig = SubrecordSig(target);
        }
    }
    record
        .fields
        .retain(|entry| !LEGACY_CLOUD_LAYER_SIGS.contains(&entry.sig.0));
}

/// Fallout 4 walks all thirty-two cloud layers. The legacy games author at most
/// four, so the remainder must be masked off rather than rendered untextured.
fn rebuild_legacy_wthr_disabled_cloud_layers(record: &mut Record) {
    if record.fields.iter().any(|entry| entry.sig.0 == *b"NAM1") {
        return;
    }
    let disabled = (0..DEFAULT_CLOUD_ROWS).fold(0_u32, |mask, layer| {
        let sig = fo4_cloud_layer_sig(layer);
        if record.fields.iter().any(|entry| entry.sig.0 == sig) {
            mask
        } else {
            mask | (1 << layer)
        }
    });
    record
        .fields
        .push(field(*b"NAM1", raw_value(disabled.to_le_bytes().to_vec())));
}

fn normalize_legacy_dalc_value(mut value: FieldValue, interner: &StringInterner) -> FieldValue {
    match &mut value {
        FieldValue::Bytes(bytes) if bytes.len() == 32 => value,
        FieldValue::Bytes(bytes) if bytes.len() == 24 => {
            let mut target = vec![0_u8; 32];
            target[..24].copy_from_slice(bytes);
            raw_value(target)
        }
        FieldValue::Struct(_) | FieldValue::List(_) => {
            append_uint_defaults(
                &mut value,
                &[
                    "ambient_colors_specular_red",
                    "ambient_colors_specular_green",
                    "ambient_colors_specular_blue",
                    "unknown_u8_27",
                ],
                interner,
            );
            append_float_defaults(&mut value, &["ambient_colors_fresnel_power"], interner);
            value
        }
        _ => raw_value(vec![0_u8; 32]),
    }
}

fn rebuild_legacy_wthr_dalc(record: &mut Record, interner: &StringInterner) {
    let insert_at = record
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"DALC")
        .unwrap_or(record.fields.len());
    let mut rows = record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"DALC")
        .map(|entry| normalize_legacy_dalc_value(entry.value.clone(), interner))
        .collect::<Vec<_>>();
    record.fields.retain(|entry| entry.sig.0 != *b"DALC");

    if rows.is_empty()
        && let Some(ambient_periods) = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"NAM0")
            .and_then(|entry| raw(&entry.value))
            .and_then(|bytes| {
                bytes.get(FO4_WTHR_NAM0_AMBIENT..FO4_WTHR_NAM0_AMBIENT + FO4_WTHR_NAM0_ROW)
            })
    {
        rows = ambient_periods
            .chunks_exact(4)
            .map(|period| {
                let mut dalc = vec![0_u8; 32];
                for offset in [0, 4, 8, 12, 16, 20, 24] {
                    dalc[offset..offset + 3].copy_from_slice(&period[..3]);
                }
                dalc[28..32].copy_from_slice(&1.0_f32.to_le_bytes());
                raw_value(dalc)
            })
            .collect();
    }

    let target_rows = if rows.len() <= 4 {
        rows.resize_with(4, || raw_value(vec![0_u8; 32]));
        LEGACY_TO_FO4_WEATHER_PERIODS
            .iter()
            .map(|source_period| rows[*source_period].clone())
            .collect::<Vec<_>>()
    } else {
        rows.truncate(8);
        rows.resize_with(8, || raw_value(vec![0_u8; 32]));
        rows
    };
    for (offset, value) in target_rows.into_iter().enumerate() {
        record.fields.insert(
            (insert_at + offset).min(record.fields.len()),
            field(*b"DALC", value),
        );
    }
}

fn legacy_weather_profile(editor_id: &str) -> LegacyWeatherProfile {
    let editor_id = editor_id.to_ascii_lowercase();
    if ["interior", "cave", "sewer", "inverted", "nolighting", "off"]
        .iter()
        .any(|needle| editor_id.contains(needle))
    {
        LegacyWeatherProfile::None
    } else if ["radstorm", "nuke", "nuked", "fallout", "decay"]
        .iter()
        .any(|needle| editor_id.contains(needle))
    {
        LegacyWeatherProfile::Radstorm
    } else if ["dust", "desert", "sand"]
        .iter()
        .any(|needle| editor_id.contains(needle))
    {
        LegacyWeatherProfile::Dusty
    } else if ["rain", "thunder"]
        .iter()
        .any(|needle| editor_id.contains(needle))
    {
        LegacyWeatherProfile::Rain
    } else if ["fog", "hazy", "toxic", "smoke"]
        .iter()
        .any(|needle| editor_id.contains(needle))
    {
        LegacyWeatherProfile::Fog
    } else if editor_id.contains("clear") {
        LegacyWeatherProfile::Clear
    } else if ["overcast", "cloudy", "storm"]
        .iter()
        .any(|needle| editor_id.contains(needle))
    {
        LegacyWeatherProfile::Overcast
    } else {
        LegacyWeatherProfile::None
    }
}

fn legacy_weather_god_rays(editor_id: &str) -> Vec<u8> {
    let periods = match legacy_weather_profile(editor_id) {
        LegacyWeatherProfile::None => [FO4_GDRY_NONE; 4],
        LegacyWeatherProfile::Clear => [0x0021_6A93, 0x0021_6A92, 0x0021_6A94, FO4_GDRY_NONE],
        LegacyWeatherProfile::Fog => [0x0021_8FA0, 0x0021_8FA4, 0x0021_8FA1, 0x001C_C192],
        LegacyWeatherProfile::Rain => [0x0021_15D0, 0x001C_D09F, 0x0021_15D1, 0x001C_C192],
        LegacyWeatherProfile::Overcast => [0x001C_855C, 0x001C_855D, 0x001C_855E, FO4_GDRY_NONE],
        LegacyWeatherProfile::Radstorm => [0x001F_495D, 0x0022_4A94, 0x0022_4591, 0x0022_458F],
        LegacyWeatherProfile::Dusty => [0x001F_61AB, 0x001F_61AD, 0x001F_61AE, 0x001F_61AC],
    };
    LEGACY_TO_FO4_WEATHER_PERIODS
        .iter()
        .flat_map(|source_period| periods[*source_period].to_le_bytes())
        .collect()
}

fn rebuild_legacy_wthr_god_rays(record: &mut Record, interner: &StringInterner) {
    let existing = record.fields.iter().find_map(|entry| {
        (entry.sig.0 == *b"WGDR")
            .then(|| raw(&entry.value))
            .flatten()
            .filter(|bytes| bytes.len() == 32)
            .map(|bytes| raw_value(bytes.to_vec()))
    });
    let editor_id = record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .unwrap_or_default();
    let value = existing.unwrap_or_else(|| raw_value(legacy_weather_god_rays(editor_id)));
    record.fields.retain(|entry| entry.sig.0 != *b"WGDR");
    record.fields.push(field(*b"WGDR", value));
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

fn ensure_single_weather_multiplier(record: &mut Record, sig: [u8; 4]) {
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
        if matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.len() == 4)
            || matches!(entry.value, FieldValue::Float(_))
        {
            return;
        }
        entry.value = raw_value(1.0_f32.to_le_bytes().to_vec());
    } else {
        record
            .fields
            .push(field(sig, raw_value(1.0_f32.to_le_bytes().to_vec())));
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

/// Rebuild legacy Fallout WTHR layouts for Fallout 4.
pub(crate) fn normalize_legacy_wthr(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"WTHR" {
        return;
    }
    let already_target_layout = record.fields.iter().any(|entry| {
        (entry.sig.0 == *b"NAM0"
            && matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.len() == FO4_WTHR_NAM0_SIZE))
            || (entry.sig.0 == *b"IMSP"
                && matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.len() == 32))
    });
    carry_legacy_cloud_layer_textures(record);

    // Probe before the loop: the rewrite below reshapes NAM0 into FO4 layout,
    // erasing the length that distinguishes a four-period FO3 record from a
    // six-period FNV one.
    let source_periods = legacy_weather_source_periods(record);
    let pnam_rows = DEFAULT_CLOUD_ROWS;
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
                    if expand_legacy_weather_period_rows(
                        &mut entry.value,
                        4,
                        already_target_layout,
                        source_periods,
                    )
                    .is_none()
                        || !resize_raw_row_array(&mut entry.value, 32, DEFAULT_CLOUD_ROWS)
                    {
                        entry.value = raw_value(vec![0_u8; pnam_rows * 32]);
                    }
                } else {
                    copy_structured_fields(
                        &mut entry.value,
                        &[
                            ("cloud_colors_night_red", "cloud_colors_early_sunrise_red"),
                            (
                                "cloud_colors_night_green",
                                "cloud_colors_early_sunrise_green",
                            ),
                            ("cloud_colors_night_blue", "cloud_colors_early_sunrise_blue"),
                            ("unknown_u8_15", "unknown_u8_19"),
                            ("cloud_colors_sunrise_red", "cloud_colors_late_sunrise_red"),
                            (
                                "cloud_colors_sunrise_green",
                                "cloud_colors_late_sunrise_green",
                            ),
                            (
                                "cloud_colors_sunrise_blue",
                                "cloud_colors_late_sunrise_blue",
                            ),
                            ("unknown_u8_3", "unknown_u8_23"),
                            ("cloud_colors_day_red", "cloud_colors_early_sunset_red"),
                            ("cloud_colors_day_green", "cloud_colors_early_sunset_green"),
                            ("cloud_colors_day_blue", "cloud_colors_early_sunset_blue"),
                            ("unknown_u8_7", "unknown_u8_27"),
                            ("cloud_colors_sunset_red", "cloud_colors_late_sunset_red"),
                            (
                                "cloud_colors_sunset_green",
                                "cloud_colors_late_sunset_green",
                            ),
                            ("cloud_colors_sunset_blue", "cloud_colors_late_sunset_blue"),
                            ("unknown_u8_11", "unknown_u8_31"),
                        ],
                        interner,
                    );
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
                    resize_structured_row_array(&mut entry.value, DEFAULT_CLOUD_ROWS);
                }
            }
            sig if sig == *b"NAM0" => rebuild_legacy_wthr_nam0(&mut entry.value, source_periods),
            sig if sig == *b"NAM4" => {
                if raw(&entry.value).is_some()
                    && !resize_raw_row_array(&mut entry.value, 4, DEFAULT_CLOUD_ROWS)
                {
                    entry.value = raw_value(vec![0_u8; pnam_rows * 4]);
                }
            }
            sig if sig == *b"JNAM" => {
                if raw(&entry.value).is_some() {
                    if expand_legacy_weather_period_rows(
                        &mut entry.value,
                        4,
                        already_target_layout,
                        source_periods,
                    )
                    .is_none()
                        || !resize_raw_row_array(&mut entry.value, 32, DEFAULT_CLOUD_ROWS)
                    {
                        entry.value = raw_value(vec![0_u8; pnam_rows * 32]);
                    }
                } else {
                    copy_structured_fields(
                        &mut entry.value,
                        &[
                            ("cloud_alphas_night", "cloud_alphas_early_sunrise"),
                            ("cloud_alphas_sunrise", "cloud_alphas_late_sunrise"),
                            ("cloud_alphas_day", "cloud_alphas_early_sunset"),
                            ("cloud_alphas_sunset", "cloud_alphas_late_sunset"),
                        ],
                        interner,
                    );
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
                    resize_structured_row_array(&mut entry.value, DEFAULT_CLOUD_ROWS);
                }
            }
            sig if sig == *b"FNAM" => match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() == 72 => {}
                FieldValue::Bytes(bytes) if bytes.len() == 24 => {
                    let mut target = vec![0_u8; 72];
                    target[..24].copy_from_slice(bytes);
                    target[24..28].copy_from_slice(&FO4_WTHR_FOG_MAX_DAY.to_le_bytes());
                    target[28..32].copy_from_slice(&FO4_WTHR_FOG_MAX_NIGHT.to_le_bytes());
                    entry.value = raw_value(target);
                }
                _ => entry.value = raw_value(vec![0_u8; 72]),
            },
            sig if sig == *b"IMSP" => {
                if raw(&entry.value).is_some() {
                    if expand_legacy_weather_period_rows(
                        &mut entry.value,
                        4,
                        already_target_layout,
                        source_periods,
                    )
                    .is_none()
                    {
                        entry.value = raw_value(vec![0_u8; 32]);
                    }
                } else {
                    copy_structured_fields(
                        &mut entry.value,
                        &[
                            ("night", "early_sunrise"),
                            ("sunrise", "late_sunrise"),
                            ("day", "early_sunset"),
                            ("sunset", "late_sunset"),
                        ],
                        interner,
                    );
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
            _ => {}
        }
    }

    if !record.fields.iter().any(|entry| entry.sig.0 == *b"PNAM") {
        push_zero_field(record, *b"PNAM", DEFAULT_CLOUD_ROWS * 32);
    }
    if !record.fields.iter().any(|entry| entry.sig.0 == *b"JNAM") {
        // Neither source game stores per-layer cloud alpha. Zero would hide
        // every cloud layer, so default to Fallout 4's fully opaque 1.0.
        record.fields.push(field(
            *b"JNAM",
            raw_value(
                std::iter::repeat_n(1.0_f32.to_le_bytes(), pnam_rows * 8)
                    .flatten()
                    .collect(),
            ),
        ));
    }
    for sig in [*b"RNAM", *b"QNAM"] {
        if !record.fields.iter().any(|entry| entry.sig.0 == sig) {
            record.fields.push(field(
                sig,
                raw_value(vec![FO4_WTHR_CLOUD_SPEED_STILL; DEFAULT_CLOUD_ROWS]),
            ));
        }
    }
    if !record.fields.iter().any(|entry| entry.sig.0 == *b"FNAM") {
        push_zero_field(record, *b"FNAM", 72);
    }
    if !record.fields.iter().any(|entry| entry.sig.0 == *b"IMSP") {
        push_zero_field(record, *b"IMSP", 32);
    }
    rebuild_legacy_wthr_disabled_cloud_layers(record);
    rebuild_legacy_wthr_god_rays(record, interner);
    rebuild_legacy_wthr_dalc(record, interner);

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
        (*b"WGDR", 32),
        (*b"UNAM", 24),
    ] {
        ensure_single_required(record, sig, len);
    }
    ensure_single_weather_multiplier(record, *b"VNAM");
    ensure_single_weather_multiplier(record, *b"WNAM");

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

/// FO4 landscape `TXST` object bounds, copied from every vanilla landscape
/// texture set (e.g. `LandscapeDirtGravel01` in `Fallout4.esm`).
const FO4_LANDSCAPE_TXST_BOUNDS: [(&str, i64); 6] = [
    ("object_bounds_x1", -8),
    ("object_bounds_y1", -30),
    ("object_bounds_z1", -20),
    ("object_bounds_x2", 7),
    ("object_bounds_y2", 30),
    ("object_bounds_z2", 20),
];

/// Derive FO4's `_s` smooth/spec path from a normal map path, matching the
/// `_n` → `_s` rename the texture engine's `LegacySpecGloss` task uses when it
/// bakes the normal's alpha (plus any env mask) into the FO4 spec map.
fn smooth_spec_path(normal: &str) -> Option<String> {
    let trimmed = normal.trim().trim_matches('\0');
    let stem_end = trimmed.rfind('.')?;
    let (stem, ext) = trimmed.split_at(stem_end);
    let index = stem.to_ascii_lowercase().rfind("_n")?;
    Some(format!("{}_s{ext}", &stem[..index]))
}

fn string_field<'a>(
    record: &'a Record,
    sig: [u8; 4],
    interner: &'a StringInterner,
) -> Option<&'a str> {
    let entry = record.fields.iter().find(|entry| entry.sig.0 == sig)?;
    let FieldValue::String(sym) = entry.value else {
        return None;
    };
    let value = interner.resolve(sym)?.trim().trim_matches('\0');
    (!value.is_empty()).then_some(value)
}

/// Point `TX07` — FO4's smooth/spec slot, verified against all 382 vanilla
/// `Fallout4.esm` texture sets — at the `_s` map derived from `TX01`.
///
/// FNV/FO3 have no spec slot at all, so `TX07` is always empty for them. Their
/// `TX02` env mask is folded into `_s.R` by the texture engine and never ships
/// standalone, so it is dropped rather than left dangling in FO4's unrelated
/// `TX02`. Skyrim does carry its own `_s` into `TX07`; an authored value there
/// is left alone and only an empty slot is filled.
pub(crate) fn normalize_txst_smooth_spec(
    record: &mut Record,
    family: SourceFamily,
    interner: &StringInterner,
) {
    if record.sig.0 != *b"TXST" {
        return;
    }
    if family == SourceFamily::LegacyFallout {
        record.fields.retain(|entry| entry.sig.0 != *b"TX02");
    }
    if string_field(record, *b"TX07", interner).is_some() {
        return;
    }

    let Some(spec) = string_field(record, *b"TX01", interner).and_then(smooth_spec_path) else {
        return;
    };
    record.fields.retain(|entry| entry.sig.0 != *b"TX07");
    record
        .fields
        .push(field(*b"TX07", FieldValue::String(interner.intern(&spec))));
}

/// Apply the vanilla FO4 landscape `TXST` shape: real object bounds and the
/// `NoSpecularMap` flag. Every vanilla landscape texture set carries both, and
/// the FO76 terrain emitter already writes them; translated Gamebryo texture
/// sets inherit an empty `OBND` and no flags instead.
pub(crate) fn apply_fo4_landscape_txst_shape(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"TXST" {
        return;
    }

    let bounds = FieldValue::Struct(
        FO4_LANDSCAPE_TXST_BOUNDS
            .iter()
            .map(|(name, value)| (interner.intern(name), FieldValue::Int(*value)))
            .collect(),
    );
    match record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"OBND")
    {
        Some(entry) => entry.value = bounds,
        None => record.fields.insert(0, field(*b"OBND", bounds)),
    }

    // DNAM bit 0 — the flag vanilla landscape texture sets set alongside a
    // populated TX02.
    const NO_SPECULAR_MAP: u16 = 0x0001;
    match record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"DNAM")
    {
        Some(entry) => {
            let flags = match &entry.value {
                FieldValue::Int(value) => *value as u16,
                FieldValue::Uint(value) => *value as u16,
                _ => 0,
            };
            entry.value = FieldValue::Uint(u64::from(flags | NO_SPECULAR_MAP));
        }
        None => record.fields.push(field(
            *b"DNAM",
            FieldValue::Uint(u64::from(NO_SPECULAR_MAP)),
        )),
    }
}

/// FO3 and FNV share one 196-byte `WATR.DNAM`; only bytes 0..16 differ (FNV
/// names them wind/wave globals, FO3 leaves them unknown). Every field from
/// `sun_power` at offset 16 onward sits at an identical offset in both, so a
/// single map serves the whole legacy family. A handful of records stop at 184
/// bytes (the three trailing amplitude scales are absent), so every read is
/// bounds-guarded.
const LEGACY_WATR_DNAM_MIN_SIZE: usize = 184;
const LEGACY_WATR_DNAM_SIZE: usize = 196;
const FO4_WATR_DNAM_SIZE: usize = 201;

/// FO4-only `WATR.DNAM` fields have no legacy counterpart: FO4 models water
/// with colour/alpha depth ranges, silt, and per-layer noise falloff that
/// FO3/FNV's shader simply did not have. Left zero they produce invisible,
/// unlit water, so they are seeded from one real vanilla exterior water —
/// `Fallout4.esm:0C8633 ExtLakeWater` — rather than mixing per-field medians,
/// which would not describe any water the engine actually ships.
fn default_fo4_watr_dnam() -> Vec<u8> {
    let mut out = vec![0_u8; FO4_WATR_DNAM_SIZE];
    for (offset, value) in [
        (0_usize, 1_412.999_9_f32), // fog depth amount
        (12, 1.0),                  // colour shallow range
        (16, 0.869_850_1),          // colour deep range
        (20, 0.0),                  // shallow alpha
        (24, 1.0),                  // deep alpha — 1.0 in all 42 vanilla waters
        (28, 1.0),                  // alpha shallow range
        (32, 0.891_994_83),         // alpha deep range
        (52, 0.402_199_98),         // normal magnitude
        (56, 1.0),                  // shallow normal falloff
        (60, 0.997_394),            // deep normal falloff
        (72, 0.996_331_3),          // surface effect falloff
        (104, 4.858_999_7),         // sun specular magnitude
        (108, 10000.0),             // sun sparkle power
        (112, 3.626_999_9),         // sun sparkle magnitude
        (124, 211.0),               // interior specular power
        // Layer UV scales stay vanilla: they are calibrated to the noise
        // texture, and NAM2..NAM4 below ship FO4's textures, not the legacy
        // one. Legacy UV scales (median 100) are ~25x off FO4's and would
        // tile the FO4 noise maps into visible garbage.
        (164, 2572.0),
        (168, 1232.0),
        (172, 398.999_97),
        (176, 2_433.843),   // layer 1 noise falloff
        (180, 4_066.508_8), // layer 2 noise falloff
        (184, 3_324.313_5), // layer 3 noise falloff
        (188, 1.0),         // silt amount
    ] {
        out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    out[192..195].copy_from_slice(&[106, 82, 64]); // silt light colour
    out[196..199].copy_from_slice(&[53, 49, 36]); // silt dark colour
    out[200] = 1; // screen space reflections
    out
}

/// `(legacy_offset, fo4_offset)` for float fields that carry the same meaning
/// in both shaders. Fields with no shared meaning — legacy above-water fog,
/// rain simulator, distortion/shininess/HDR multiplier, normals noise scale
/// and depth falloff — are intentionally absent and keep their vanilla default.
const LEGACY_TO_FO4_WATR_FLOATS: &[(usize, usize)] = &[
    (140, 40),  // under-water fog amount
    (148, 48),  // under-water fog distance far plane
    (20, 64),   // reflectivity amount
    (24, 68),   // fresnel amount
    (76, 76),   // displacement simulator force
    (80, 80),   // displacement simulator velocity
    (84, 84),   // displacement simulator falloff
    (88, 88),   // displacement simulator dampener
    (72, 92),   // displacement simulator starting size
    (16, 100),  // sun power -> sun specular power
    (164, 116), // light radius -> interior specular radius
    (168, 120), // light brightness -> interior specular brightness
    (100, 128), // noise layer 1 wind direction
    (104, 132), // noise layer 2 wind direction
    (108, 136), // noise layer 3 wind direction
    (112, 140), // noise layer 1 wind speed
    (116, 144), // noise layer 2 wind speed
    (120, 148), // noise layer 3 wind speed
    (184, 152), // noise layer 1 amplitude scale
    (188, 156), // noise layer 2 amplitude scale
    (192, 160), // noise layer 3 amplitude scale
];

fn read_f32(source: &[u8], offset: usize) -> Option<f32> {
    let slice = source.get(offset..offset + 4)?;
    Some(f32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn write_f32(target: &mut [u8], offset: usize, value: f32) {
    target[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn copy_rgb(source: &[u8], source_offset: usize, target: &mut [u8], target_offset: usize) {
    let Some(rgb) = source.get(source_offset..source_offset + 3) else {
        return;
    };
    target[target_offset..target_offset + 3].copy_from_slice(rgb);
}

fn build_fo4_watr_dnam(source: Option<&[u8]>) -> Vec<u8> {
    let mut target = default_fo4_watr_dnam();
    let Some(source) = source
        .filter(|bytes| (LEGACY_WATR_DNAM_MIN_SIZE..=LEGACY_WATR_DNAM_SIZE).contains(&bytes.len()))
    else {
        return target;
    };

    for &(source_offset, target_offset) in LEGACY_TO_FO4_WATR_FLOATS {
        if let Some(value) = read_f32(source, source_offset) {
            write_f32(&mut target, target_offset, value);
        }
    }

    copy_rgb(source, 40, &mut target, 4); // shallow colour
    copy_rgb(source, 44, &mut target, 8); // deep colour
    copy_rgb(source, 48, &mut target, 96); // reflection colour
    // FO4 tints everything seen through the surface with its own under-water
    // colour, which the legacy shader had no slot for. Seeding it from the
    // legacy deep colour keeps each water's identity (irradiated green stays
    // green, blood stays red) instead of forcing one constant tint everywhere.
    copy_rgb(source, 44, &mut target, 36);

    // Legacy "above water fog distance far plane" is the same concept as FO4's
    // fog depth amount: how far you see into the water. Zero would collapse the
    // shallow->deep blend, so fall back to the vanilla default.
    if let Some(far_plane) = read_f32(source, 36).filter(|value| *value > 0.0) {
        write_f32(&mut target, 0, far_plane);
    }
    // Every vanilla FO4 water keeps the near fog plane at or below zero; a few
    // legacy records store a positive one, which inverts the depth gradient.
    if let Some(near_fog) = read_f32(source, 144) {
        write_f32(&mut target, 44, near_fog.min(0.0));
    }

    target
}

/// FO4's water shader reads three tiling noise textures. FO3/FNV have a single
/// `NNAM` noise map whose content and channel use do not transfer, and which
/// the asset pipeline does not port, so all three layers take stock FO4
/// textures — the same triple `ExtLakeWater` uses.
const FO4_WATR_NOISE_TEXTURES: [(&[u8; 4], &str); 3] = [
    (b"NAM2", "data\\Textures\\Water\\DefaultWater.dds"),
    (b"NAM3", "data\\Textures\\Water\\ChurningWaterTile.dds"),
    (b"NAM4", "data\\Textures\\Water\\DefaultWater.dds"),
];

/// Rebuild a legacy `WATR` into Fallout 4's contract.
///
/// All 42 vanilla FO4 waters carry `DATA`, `DNAM`, `NAM0`..`NAM4`; a legacy
/// record supplies only `DATA`/`DNAM`, and its `DNAM` uses an incompatible
/// layout that FO4 would otherwise read field-for-field from the wrong offset.
pub(crate) fn normalize_legacy_watr(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"WATR" {
        return;
    }
    // Already-FO4-shaped input (re-run, or a record that reached here through
    // another path) must not be rebuilt from its own output.
    if record.fields.iter().any(|entry| {
        entry.sig.0 == *b"DNAM"
            && matches!(raw(&entry.value), Some(bytes) if bytes.len() == FO4_WATR_DNAM_SIZE)
    }) {
        return;
    }

    let source_dnam = record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"DNAM")
        .and_then(|entry| raw(&entry.value))
        .map(<[u8]>::to_vec);
    let target_dnam = build_fo4_watr_dnam(source_dnam.as_deref());

    // NNAM (legacy noise map) and MNAM have no FO4 WATR slot at all.
    record.fields.retain(|entry| {
        !matches!(entry.sig.0, sig if sig == *b"DATA"
            || sig == *b"DNAM"
            || sig == *b"NNAM"
            || sig == *b"MNAM"
            || sig == *b"NAM0"
            || sig == *b"NAM1"
            || sig == *b"NAM2"
            || sig == *b"NAM3"
            || sig == *b"NAM4")
    });

    // Anchor on GNAM *after* the retain: the removed NNAM/MNAM sit before DATA
    // in the legacy field order, so an index taken beforehand would be stale.
    // Vanilla FO4 orders these DATA, DNAM, GNAM, NAM0..NAM4.
    let insert_at = record
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"GNAM")
        .unwrap_or(record.fields.len());

    // Vanilla FO4 DATA is empty; the legacy uint16 damage value has no slot.
    record
        .fields
        .insert(insert_at, field(*b"DATA", raw_value(Vec::new())));
    record
        .fields
        .insert(insert_at + 1, field(*b"DNAM", raw_value(target_dnam)));

    // Linear and angular velocity: zero is still water, and is what every
    // vanilla NAM1 and the most common vanilla NAM0 already hold.
    push_zero_field(record, *b"NAM0", 12);
    push_zero_field(record, *b"NAM1", 12);
    for (sig, path) in FO4_WATR_NOISE_TEXTURES {
        record
            .fields
            .push(field(*sig, FieldValue::String(interner.intern(path))));
    }
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

    fn push_str(record: &mut Record, sig: [u8; 4], value: &str, interner: &StringInterner) {
        record
            .fields
            .push(field(sig, FieldValue::String(interner.intern(value))));
    }

    fn slot<'a>(record: &'a Record, sig: [u8; 4], interner: &'a StringInterner) -> Option<&'a str> {
        string_field(record, sig, interner)
    }

    fn txst_with_slots(interner: &StringInterner, slots: &[([u8; 4], &str)]) -> Record {
        let mut txst = record("TXST", interner);
        for (sig, value) in slots {
            push_str(&mut txst, *sig, value, interner);
        }
        txst
    }

    #[test]
    fn legacy_txst_gains_spec_slot_derived_from_the_normal() {
        let interner = StringInterner::new();
        let mut txst = txst_with_slots(
            &interner,
            &[
                (*b"TX00", "architecture\\suburban\\suburbanrubble01.dds"),
                (*b"TX01", "architecture\\suburban\\suburbanrubble01_n.dds"),
            ],
        );

        normalize_txst_smooth_spec(&mut txst, SourceFamily::LegacyFallout, &interner);

        assert_eq!(
            slot(&txst, *b"TX07", &interner),
            Some("architecture\\suburban\\suburbanrubble01_s.dds"),
            "FO4 reads the smooth/spec map from TX07, not TX02"
        );
    }

    #[test]
    fn legacy_txst_drops_the_env_mask_that_never_ships() {
        // FNV/FO3 env masks are baked into `_s.R` by the texture engine and are
        // never written as standalone textures, so carrying TX02 through leaves
        // a dangling ref in an unrelated FO4 slot.
        let interner = StringInterner::new();
        let mut txst = txst_with_slots(
            &interner,
            &[
                (*b"TX00", "weapons\\2handhandle\\gatlinglaser.dds"),
                (*b"TX01", "weapons\\2handhandle\\gatlinglaser_n.dds"),
                (*b"TX02", "weapons\\2handhandle\\gatlinglaser_m.dds"),
            ],
        );

        normalize_txst_smooth_spec(&mut txst, SourceFamily::LegacyFallout, &interner);

        assert_eq!(slot(&txst, *b"TX02", &interner), None);
        assert_eq!(
            slot(&txst, *b"TX07", &interner),
            Some("weapons\\2handhandle\\gatlinglaser_s.dds")
        );
    }

    #[test]
    fn skyrim_txst_keeps_its_own_spec_map_and_env_mask() {
        let interner = StringInterner::new();
        let mut txst = txst_with_slots(
            &interner,
            &[
                (*b"TX00", "actors\\character\\female\\femalebody_1.dds"),
                (*b"TX01", "actors\\character\\female\\femalebody_1_n.dds"),
                (*b"TX02", "actors\\character\\female\\femalebody_1_sk.dds"),
                (*b"TX07", "actors\\character\\female\\femalebody_1_s.dds"),
            ],
        );

        normalize_txst_smooth_spec(&mut txst, SourceFamily::SkyrimSe, &interner);

        assert_eq!(
            slot(&txst, *b"TX07", &interner),
            Some("actors\\character\\female\\femalebody_1_s.dds"),
            "an authored Skyrim spec map must not be overwritten"
        );
        assert_eq!(
            slot(&txst, *b"TX02", &interner),
            Some("actors\\character\\female\\femalebody_1_sk.dds"),
            "Skyrim env masks do ship; only the legacy Fallout ones are dropped"
        );
    }

    #[test]
    fn model_space_normals_yield_no_spec_slot() {
        // `_msn` carries no `_n` token, so there is no `_s` sibling to name.
        let interner = StringInterner::new();
        let mut txst = txst_with_slots(
            &interner,
            &[(*b"TX01", "actors\\character\\female\\femalehead_msn.dds")],
        );

        normalize_txst_smooth_spec(&mut txst, SourceFamily::SkyrimSe, &interner);

        assert_eq!(slot(&txst, *b"TX07", &interner), None);
    }

    #[test]
    fn landscape_txst_gains_vanilla_bounds_and_no_specular_map_flag() {
        let interner = StringInterner::new();
        let mut txst = record("TXST", &interner);
        txst.fields
            .push(field(*b"OBND", FieldValue::Struct(Default::default())));
        push_str(
            &mut txst,
            *b"TX00",
            "landscape\\burntground01.dds",
            &interner,
        );

        apply_fo4_landscape_txst_shape(&mut txst, &interner);

        let FieldValue::Struct(bounds) = &txst
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"OBND")
            .unwrap()
            .value
        else {
            panic!("object bounds should stay a struct");
        };
        let x1 = bounds
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("object_bounds_x1"))
            .map(|(_, value)| value.clone());
        assert_eq!(x1, Some(FieldValue::Int(-8)));

        let dnam = txst
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"DNAM")
            .map(|entry| entry.value.clone());
        assert_eq!(dnam, Some(FieldValue::Uint(1)));
    }

    const WTHR_REQUIRED_ORDER: &[&str] = &[
        "LNAM", "MNAM", "NNAM", "RNAM", "QNAM", "PNAM", "JNAM", "NAM0", "NAM4", "FNAM", "DATA",
        "NAM1", "IMSP", "WGDR", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC",
        "UNAM", "VNAM", "WNAM",
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
            (*b"WGDR", 32),
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

    fn legacy_wthr_fixture(interner: &StringInterner, nam0_len: usize) -> Record {
        let mut wthr = record("WTHR", interner);
        wthr.eid = Some(interner.intern("NVWastelandClear"));
        push(&mut wthr, *b"LNAM", 32_u32.to_le_bytes().to_vec());
        push(&mut wthr, *b"RNAM", (0_u8..32).collect());
        push(&mut wthr, *b"QNAM", (32_u8..64).collect());
        push(&mut wthr, *b"DATA", (0_u8..15).collect());
        // PNAM's row width tracks NAM0's: both widen from four periods to six
        // in FNV, so the fixture must stay internally consistent.
        let pnam_len = if nam0_len == 240 { 96 } else { 64 };
        push(
            &mut wthr,
            *b"PNAM",
            (0..pnam_len).map(|index| (index & 0xff) as u8).collect(),
        );
        let mut nam0 = vec![0_u8; nam0_len];
        for (index, byte) in nam0.iter_mut().enumerate() {
            *byte = (index & 0xff) as u8;
        }
        push(&mut wthr, *b"NAM0", nam0);
        push(&mut wthr, *b"FNAM", vec![0x5A; 72]);
        for period in 0_u8..4 {
            push(&mut wthr, *b"DALC", vec![period + 1; 24]);
        }
        wthr
    }

    /// Ten weather colours tagged `(row + 1) << 8 | period`, laid out with the
    /// caller's period count so a test can state which source game it means.
    fn legacy_nam0(row_width: usize) -> Vec<u8> {
        let mut source = vec![0_u8; 10 * row_width];
        for (row, chunk) in source.chunks_exact_mut(row_width).enumerate() {
            for period in 0..row_width / 4 {
                let value = ((row as u32 + 1) << 8) | period as u32;
                chunk[period * 4..period * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        source
    }

    fn set_field(record: &mut Record, sig: [u8; 4], bytes: Vec<u8>) {
        record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == sig)
            .expect("fixture field")
            .value = raw_value(bytes);
    }

    fn assert_ten_colours_expand_to_fo4_periods(wthr: &Record) {
        for row in 0..10 {
            let actual = period_values(&bytes(wthr, b"NAM0")[row * 32..row * 32 + 32]);
            let source = (0..4)
                .map(|period| ((row as u32 + 1) << 8) | period)
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                vec![
                    source[0], source[1], source[2], source[3], source[3], source[0], source[1],
                    source[2]
                ],
                "weather colour row {row}"
            );
        }
        // FO3 and FNV both stop at ten colours; the Fallout 4-only categories
        // past that point stay zero rather than inheriting neighbouring rows.
        assert!(bytes(wthr, b"NAM0")[320..].iter().all(|byte| *byte == 0));
    }

    fn period_values(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    fn assert_legacy_wthr_rebuild(mut wthr: Record, interner: &StringInterner) {
        normalize_legacy_wthr(&mut wthr, interner);
        assert_wthr_contract(&wthr, DEFAULT_CLOUD_ROWS);
        let periods = [0_usize, 1, 2, 3, 3, 0, 1, 2];
        let source_pnam = (0_u8..16).collect::<Vec<_>>();
        let expected_pnam = periods
            .iter()
            .flat_map(|period| source_pnam[period * 4..period * 4 + 4].iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(&bytes(&wthr, b"PNAM")[..32], expected_pnam);
        // The fixture authors four cloud layers; the remaining twenty-eight
        // Fallout 4 layers stay zero.
        assert!(bytes(&wthr, b"PNAM")[128..].iter().all(|byte| *byte == 0));

        let source_nam0 = (0_u8..16).collect::<Vec<_>>();
        let expected_nam0 = periods
            .iter()
            .flat_map(|period| source_nam0[period * 4..period * 4 + 4].iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(&bytes(&wthr, b"NAM0")[..32], expected_nam0);

        let dalc = wthr
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"DALC")
            .map(|entry| raw(&entry.value).unwrap())
            .collect::<Vec<_>>();
        for (row, source_period) in dalc.iter().zip(periods) {
            assert_eq!(&row[..24], &[source_period as u8 + 1; 24]);
            assert_eq!(&row[24..], &[0; 8]);
        }

        assert_eq!(
            period_values(bytes(&wthr, b"WGDR")),
            vec![
                0x0021_6A93,
                0x0021_6A92,
                0x0021_6A94,
                0x001B_40E8,
                0x001B_40E8,
                0x0021_6A93,
                0x0021_6A92,
                0x0021_6A94,
            ]
        );
        assert_eq!(bytes(&wthr, b"FNAM"), &[0x5A; 72]);
        assert_eq!(
            f32::from_le_bytes(bytes(&wthr, b"VNAM").try_into().unwrap()),
            1.0
        );
        assert_eq!(
            f32::from_le_bytes(bytes(&wthr, b"WNAM").try_into().unwrap()),
            1.0
        );
    }

    #[test]
    fn wthr_fnv_rebuilds_complete_fo4_required_contract() {
        let interner = StringInterner::new();
        assert_legacy_wthr_rebuild(legacy_wthr_fixture(&interner, 240), &interner);
    }

    #[test]
    fn wthr_fo3_rebuilds_complete_fo4_required_contract() {
        let interner = StringInterner::new();
        assert_legacy_wthr_rebuild(legacy_wthr_fixture(&interner, 160), &interner);
    }

    #[test]
    fn legacy_wthr_preserves_source_fog_and_synthesizes_directional_ambient() {
        let interner = StringInterner::new();
        let mut wthr = legacy_wthr_fixture(&interner, 160);
        wthr.fields.retain(|entry| entry.sig.0 != *b"DALC");
        let source_fog = (1_u32..=6)
            .flat_map(|value| f32::from_bits(value).to_le_bytes())
            .collect::<Vec<_>>();
        set_field(&mut wthr, *b"FNAM", source_fog.clone());

        normalize_legacy_wthr(&mut wthr, &interner);

        assert_eq!(&bytes(&wthr, b"FNAM")[..24], source_fog);
        // Fallout 4 clamps fog with a maximum the legacy 24-byte block has no
        // slot for; zero there leaves distance fog switched off entirely.
        let fog = bytes(&wthr, b"FNAM");
        assert_eq!(f32::from_le_bytes(fog[24..28].try_into().unwrap()), 0.85);
        assert_eq!(f32::from_le_bytes(fog[28..32].try_into().unwrap()), 0.95);
        assert!(fog[32..].iter().all(|byte| *byte == 0));

        // Directional ambient is synthesised from the `ambient` colour, which
        // is the fourth NAM0 category.  The third is the engine's unused slot.
        let target_ambient = &bytes(&wthr, b"NAM0")[96..128];
        let dalc = wthr
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"DALC")
            .map(|entry| raw(&entry.value).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(dalc.len(), 8);
        for (period, row) in target_ambient.chunks_exact(4).zip(dalc) {
            for offset in [0, 4, 8, 12, 16, 20, 24] {
                assert_eq!(&row[offset..offset + 3], &period[..3]);
            }
            assert_eq!(f32::from_le_bytes(row[28..32].try_into().unwrap()), 1.0);
        }
    }

    /// Four cloud layers tagged `(layer + 1) << 8 | period`.
    fn legacy_pnam(row_width: usize) -> Vec<u8> {
        let mut source = vec![0_u8; 4 * row_width];
        for (layer, chunk) in source.chunks_exact_mut(row_width).enumerate() {
            for period in 0..row_width / 4 {
                let value = ((layer as u32 + 1) << 8) | period as u32;
                chunk[period * 4..period * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        source
    }

    fn assert_four_layers_expand_to_fo4_periods(wthr: &Record) {
        for layer in 0..4 {
            let actual = period_values(&bytes(wthr, b"PNAM")[layer * 32..layer * 32 + 32]);
            let source = (0..4)
                .map(|period| ((layer as u32 + 1) << 8) | period)
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                vec![
                    source[0], source[1], source[2], source[3], source[3], source[0], source[1],
                    source[2]
                ],
                "cloud layer {layer}"
            );
        }
    }

    #[test]
    fn wthr_fnv_reads_six_periods_per_cloud_layer() {
        let interner = StringInterner::new();
        let mut wthr = legacy_wthr_fixture(&interner, 240);
        set_field(&mut wthr, *b"PNAM", legacy_pnam(24));

        normalize_legacy_wthr(&mut wthr, &interner);

        assert_wthr_contract(&wthr, DEFAULT_CLOUD_ROWS);
        assert_four_layers_expand_to_fo4_periods(&wthr);
    }

    #[test]
    fn wthr_fo3_reads_four_periods_per_cloud_layer() {
        let interner = StringInterner::new();
        let mut wthr = legacy_wthr_fixture(&interner, 160);
        set_field(&mut wthr, *b"PNAM", legacy_pnam(16));

        normalize_legacy_wthr(&mut wthr, &interner);

        assert_wthr_contract(&wthr, DEFAULT_CLOUD_ROWS);
        assert_four_layers_expand_to_fo4_periods(&wthr);
    }

    #[test]
    fn wthr_fnv_reads_six_periods_per_weather_colour() {
        let interner = StringInterner::new();
        let mut wthr = legacy_wthr_fixture(&interner, 240);
        set_field(&mut wthr, *b"NAM0", legacy_nam0(24));

        normalize_legacy_wthr(&mut wthr, &interner);

        assert_wthr_contract(&wthr, DEFAULT_CLOUD_ROWS);
        assert_ten_colours_expand_to_fo4_periods(&wthr);
    }

    #[test]
    fn wthr_fo3_reads_four_periods_per_weather_colour() {
        let interner = StringInterner::new();
        let mut wthr = legacy_wthr_fixture(&interner, 160);
        set_field(&mut wthr, *b"NAM0", legacy_nam0(16));

        normalize_legacy_wthr(&mut wthr, &interner);

        assert_wthr_contract(&wthr, DEFAULT_CLOUD_ROWS);
        assert_ten_colours_expand_to_fo4_periods(&wthr);
    }

    #[test]
    fn legacy_wthr_carries_cloud_layer_textures_and_disables_unused_layers() {
        let interner = StringInterner::new();
        let mut wthr = legacy_wthr_fixture(&interner, 240);
        for (sig, path) in [
            (*b"DNAM", "sky\\upper.dds\0"),
            (*b"CNAM", "sky\\lower.dds\0"),
            (*b"ANAM", "sky\\two.dds\0"),
            (*b"BNAM", "sky\\three.dds\0"),
        ] {
            push(&mut wthr, sig, path.as_bytes().to_vec());
        }

        normalize_legacy_wthr(&mut wthr, &interner);

        for (sig, path) in [
            (*b"00TX", "sky\\upper.dds\0"),
            (*b"10TX", "sky\\lower.dds\0"),
            (*b"20TX", "sky\\two.dds\0"),
            (*b"30TX", "sky\\three.dds\0"),
        ] {
            assert_eq!(bytes(&wthr, &sig), path.as_bytes(), "layer {sig:?}");
        }
        for sig in [*b"DNAM", *b"CNAM", *b"ANAM", *b"BNAM"] {
            assert!(!wthr.fields.iter().any(|entry| entry.sig.0 == sig));
        }
        // Only the four authored layers exist, so Fallout 4 must be told not to
        // walk the other twenty-eight.
        assert_eq!(
            u32::from_le_bytes(bytes(&wthr, b"NAM1").try_into().unwrap()),
            !0b1111_u32
        );
    }

    #[test]
    fn legacy_wthr_defaults_clouds_to_visible_and_still() {
        let interner = StringInterner::new();
        let mut wthr = legacy_wthr_fixture(&interner, 240);
        wthr.fields
            .retain(|entry| !matches!(entry.sig.0, sig if sig == *b"RNAM" || sig == *b"QNAM"));

        normalize_legacy_wthr(&mut wthr, &interner);

        // Zero alpha hides every cloud layer, and zero speed scrolls them at
        // full rate; Fallout 4's neutral values are 1.0 and 127.
        for alpha in bytes(&wthr, b"JNAM").chunks_exact(4) {
            assert_eq!(f32::from_le_bytes(alpha.try_into().unwrap()), 1.0);
        }
        for sig in [*b"RNAM", *b"QNAM"] {
            assert!(bytes(&wthr, &sig).iter().all(|speed| *speed == 127));
        }
    }

    #[test]
    fn legacy_wthr_structured_periods_copy_source_values() {
        let interner = StringInterner::new();
        let mut wthr = record("WTHR", &interner);
        wthr.fields.push(field(
            *b"PNAM",
            FieldValue::Struct(
                [
                    ("cloud_colors_sunrise_red", 1_u64),
                    ("cloud_colors_day_red", 2),
                    ("cloud_colors_sunset_red", 3),
                    ("cloud_colors_night_red", 4),
                ]
                .into_iter()
                .map(|(name, value)| (interner.intern(name), FieldValue::Uint(value)))
                .collect(),
            ),
        ));
        wthr.fields.push(field(
            *b"JNAM",
            FieldValue::Struct(
                [
                    ("cloud_alphas_sunrise", 1.0_f32),
                    ("cloud_alphas_day", 2.0),
                    ("cloud_alphas_sunset", 3.0),
                    ("cloud_alphas_night", 4.0),
                ]
                .into_iter()
                .map(|(name, value)| (interner.intern(name), FieldValue::Float(value)))
                .collect(),
            ),
        ));
        wthr.fields.push(field(
            *b"IMSP",
            FieldValue::Struct(
                [("sunrise", 1_u64), ("day", 2), ("sunset", 3), ("night", 4)]
                    .into_iter()
                    .map(|(name, value)| (interner.intern(name), FieldValue::Uint(value)))
                    .collect(),
            ),
        ));

        normalize_legacy_wthr(&mut wthr, &interner);

        let FieldValue::List(pnam_rows) = &wthr
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"PNAM")
            .unwrap()
            .value
        else {
            panic!("structured PNAM rows");
        };
        assert_eq!(pnam_rows.len(), DEFAULT_CLOUD_ROWS);
        let FieldValue::Struct(pnam) = &pnam_rows[0] else {
            panic!("structured PNAM");
        };
        for (name, expected) in [
            ("cloud_colors_early_sunrise_red", 4),
            ("cloud_colors_late_sunrise_red", 1),
            ("cloud_colors_early_sunset_red", 2),
            ("cloud_colors_late_sunset_red", 3),
        ] {
            assert_eq!(
                field_name(pnam, name, &interner),
                Some(&FieldValue::Uint(expected)),
                "{name}"
            );
        }

        let FieldValue::List(jnam_rows) = &wthr
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"JNAM")
            .unwrap()
            .value
        else {
            panic!("structured JNAM rows");
        };
        assert_eq!(jnam_rows.len(), DEFAULT_CLOUD_ROWS);
        let FieldValue::Struct(jnam) = &jnam_rows[0] else {
            panic!("structured JNAM");
        };
        for (name, expected) in [
            ("cloud_alphas_early_sunrise", 4.0),
            ("cloud_alphas_late_sunrise", 1.0),
            ("cloud_alphas_early_sunset", 2.0),
            ("cloud_alphas_late_sunset", 3.0),
        ] {
            assert_eq!(
                field_name(jnam, name, &interner),
                Some(&FieldValue::Float(expected)),
                "{name}"
            );
        }

        let FieldValue::Struct(imsp) = &wthr
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"IMSP")
            .unwrap()
            .value
        else {
            panic!("structured IMSP");
        };
        for (name, expected) in [
            ("early_sunrise", 4),
            ("late_sunrise", 1),
            ("early_sunset", 2),
            ("late_sunset", 3),
        ] {
            assert_eq!(
                field_name(imsp, name, &interner),
                Some(&FieldValue::Uint(expected)),
                "{name}"
            );
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
        normalize_legacy_wthr(&mut stat, &interner);
        normalize_skyrim_proj(&mut stat);

        assert_eq!(stat.fields, before);
    }

    /// The real 196-byte `DNAM` of `FalloutNV.esm:1009CA NVCleanWater` — the
    /// record that shipped brown. Its bytes 8..12 are the float `0.2` (wave
    /// amplitude), which FO4 was reading as the deep fog colour RGBA
    /// (205,204,76), and its bytes 140..152 are colour/float bytes that FO4 was
    /// reading as denormal under-water fog distances.
    const NV_CLEAN_WATER_DNAM: &[u8] = b"\
        \x00\x00\x40\x40\x00\x00\xb4\x42\xcd\xcc\x4c\x3e\x00\x00\x80\x3e\
        \x00\x80\x4e\x44\x9a\x99\x19\x3f\x00\x00\x40\x3f\x00\x00\x00\x00\
        \x00\x00\xa0\xc2\x00\x80\x54\x44\x34\x4b\x42\x00\x16\x29\x22\x00\
        \x15\x20\x17\x00\x00\x1d\x47\x00\xcd\xcc\xcc\x3d\x9a\x99\x19\x3f\
        \xf6\x28\x7c\x3f\x00\x00\x00\x40\x0a\xd7\x23\x3c\xcd\xcc\xcc\x3e\
        \x9a\x99\x19\x3f\xf6\x28\x7c\x3f\x00\x00\x20\x41\xcd\xcc\x4c\x3d\
        \x5c\x8f\x56\x41\x00\x00\x34\x43\x00\x00\x20\x41\x00\x00\x86\x42\
        \xb8\x1e\x85\x3d\x02\x2b\x07\x3d\x68\x91\xed\x3c\x00\x00\x00\x00\
        \xea\x95\x32\x3c\x00\x00\x40\x3f\x00\x00\x7a\x44\x00\x00\x80\x3f\
        \x00\x40\x1c\xc5\x00\xe0\xab\x45\x00\x00\x16\x44\x00\x00\xfa\x43\
        \x00\x00\x80\x3f\x00\x40\x1c\x46\x00\x00\xc8\x42\x00\x00\x00\x00\
        \x00\x00\x00\x00\x00\x00\x00\x00\x9a\x99\x99\x3e\x9d\x80\x06\x3f\
        \x3b\x01\x0d\x3e";

    /// Mirrors the real field order of `FalloutNV.esm:1009CA` — importantly
    /// NNAM and MNAM come *before* DATA, so any index into `fields` taken
    /// before they are dropped goes stale.
    fn legacy_watr(interner: &StringInterner, dnam: &[u8]) -> Record {
        let mut watr = record("WATR", interner);
        push_str(
            &mut watr,
            *b"NNAM",
            "Data\\Textures\\Water\\WastelandWaterPotomac.dds",
            interner,
        );
        push(&mut watr, *b"ANAM", vec![50]);
        push(&mut watr, *b"FNAM", vec![1]);
        push_str(&mut watr, *b"MNAM", "", interner);
        push(&mut watr, *b"SNAM", vec![0_u8; 4]);
        push(&mut watr, *b"XNAM", vec![0_u8; 4]);
        push(&mut watr, *b"DATA", vec![0, 0]);
        push(&mut watr, *b"DNAM", dnam.to_vec());
        push(&mut watr, *b"GNAM", vec![0_u8; 12]);
        watr
    }

    fn sig_order(record: &Record) -> Vec<String> {
        record
            .fields
            .iter()
            .map(|entry| String::from_utf8(entry.sig.0.to_vec()).unwrap())
            .collect()
    }

    fn f32_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn legacy_watr_dnam_relayouts_colours_out_of_the_float_slots() {
        let interner = StringInterner::new();
        assert_eq!(NV_CLEAN_WATER_DNAM.len(), LEGACY_WATR_DNAM_SIZE);
        let mut watr = legacy_watr(&interner, NV_CLEAN_WATER_DNAM);

        normalize_legacy_watr(&mut watr, &interner);

        let dnam = bytes(&watr, b"DNAM");
        assert_eq!(dnam.len(), FO4_WATR_DNAM_SIZE);
        // The bug: FO4 read the deep fog colour as (205,204,76) — the bytes of
        // the float 0.2. It must now be the legacy deep colour.
        assert_ne!(&dnam[8..11], &[205, 204, 76]);
        assert_eq!(&dnam[8..11], &[22, 41, 34], "deep colour");
        assert_eq!(&dnam[4..7], &[52, 75, 66], "shallow colour");
        assert_eq!(&dnam[96..99], &[21, 32, 23], "reflection colour");
        // Under-water colour is seeded from the legacy deep colour so each
        // water keeps its tint rather than taking one shared constant.
        assert_eq!(&dnam[36..39], &[22, 41, 34], "under-water colour");
    }

    #[test]
    fn legacy_watr_underwater_fog_is_no_longer_denormal() {
        let interner = StringInterner::new();
        let mut watr = legacy_watr(&interner, NV_CLEAN_WATER_DNAM);

        normalize_legacy_watr(&mut watr, &interner);

        let dnam = bytes(&watr, b"DNAM");
        // Guards against denormals (6.09e-39 / 3.14e-39 / 2.12e-39), which give
        // a zero-distance, fully dense fog tinting everything seen through the surface.
        assert_eq!(f32_at(dnam, 40), 1.0, "under-water fog amount");
        assert_eq!(f32_at(dnam, 44), -2500.0, "under-water near fog");
        assert_eq!(f32_at(dnam, 48), 5500.0, "under-water far fog");
        assert!(f32_at(dnam, 52).is_normal(), "normal magnitude");
    }

    #[test]
    fn legacy_watr_carries_shared_scalar_fields_to_fo4_offsets() {
        let interner = StringInterner::new();
        let mut watr = legacy_watr(&interner, NV_CLEAN_WATER_DNAM);

        normalize_legacy_watr(&mut watr, &interner);

        let dnam = bytes(&watr, b"DNAM");
        assert_eq!(f32_at(dnam, 64), 0.6000000238418579, "reflectivity");
        assert_eq!(f32_at(dnam, 68), 0.75, "fresnel");
        assert_eq!(f32_at(dnam, 100), 826.0, "sun specular power");
        // Legacy above-water fog far plane becomes FO4's fog depth amount.
        assert_eq!(f32_at(dnam, 0), 850.0, "fog depth amount");
    }

    #[test]
    fn legacy_watr_gains_the_fo4_subrecord_contract() {
        let interner = StringInterner::new();
        let mut watr = legacy_watr(&interner, NV_CLEAN_WATER_DNAM);

        normalize_legacy_watr(&mut watr, &interner);

        // All 42 vanilla FO4 waters carry these.
        for sig in [
            b"DATA", b"DNAM", b"NAM0", b"NAM1", b"NAM2", b"NAM3", b"NAM4",
        ] {
            assert!(
                watr.fields.iter().any(|entry| entry.sig.0 == *sig),
                "missing {}",
                std::str::from_utf8(sig).unwrap()
            );
        }
        // Legacy-only slots with no FO4 WATR counterpart.
        for sig in [b"NNAM", b"MNAM"] {
            assert!(
                !watr.fields.iter().any(|entry| entry.sig.0 == *sig),
                "kept legacy {}",
                std::str::from_utf8(sig).unwrap()
            );
        }
        // Vanilla FO4 orders these DATA, DNAM, GNAM, NAM0..NAM4, and the
        // dropped NNAM/MNAM precede DATA in the legacy record.
        assert_eq!(
            sig_order(&watr),
            [
                "ANAM", "FNAM", "SNAM", "XNAM", "DATA", "DNAM", "GNAM", "NAM0", "NAM1", "NAM2",
                "NAM3", "NAM4"
            ]
        );
        assert!(bytes(&watr, b"DATA").is_empty(), "vanilla DATA is empty");
        assert_eq!(bytes(&watr, b"NAM0").len(), 12);
        assert_eq!(bytes(&watr, b"NAM1").len(), 12);
        assert_eq!(
            slot(&watr, *b"NAM3", &interner),
            Some("data\\Textures\\Water\\ChurningWaterTile.dds")
        );
    }

    #[test]
    fn legacy_watr_normalization_is_idempotent() {
        let interner = StringInterner::new();
        let mut watr = legacy_watr(&interner, NV_CLEAN_WATER_DNAM);

        normalize_legacy_watr(&mut watr, &interner);
        let once = watr.fields.clone();
        normalize_legacy_watr(&mut watr, &interner);

        assert_eq!(watr.fields, once, "second pass must not rebuild output");
    }

    #[test]
    fn legacy_watr_without_usable_dnam_still_emits_the_fo4_shape() {
        let interner = StringInterner::new();
        // A malformed/absent source DNAM must still yield valid FO4 water
        // rather than a zeroed struct, which renders as invisible flat water.
        let mut watr = legacy_watr(&interner, &[0_u8; 24]);

        normalize_legacy_watr(&mut watr, &interner);

        let dnam = bytes(&watr, b"DNAM");
        assert_eq!(dnam.len(), FO4_WATR_DNAM_SIZE);
        assert_eq!(f32_at(dnam, 24), 1.0, "deep alpha");
        assert_eq!(dnam[200], 1, "screen space reflections");
    }

    #[test]
    fn legacy_watr_short_dnam_maps_what_is_present() {
        let interner = StringInterner::new();
        // 184-byte records exist in FalloutNV.esm: the three trailing
        // amplitude scales are absent and must not be read out of bounds.
        let mut watr = legacy_watr(&interner, &NV_CLEAN_WATER_DNAM[..LEGACY_WATR_DNAM_MIN_SIZE]);

        normalize_legacy_watr(&mut watr, &interner);

        let dnam = bytes(&watr, b"DNAM");
        assert_eq!(dnam.len(), FO4_WATR_DNAM_SIZE);
        assert_eq!(&dnam[8..11], &[22, 41, 34], "deep colour still mapped");
        // Layer 1 amplitude (legacy offset 184) is past the end; the vanilla
        // default of 0.0 is left in place rather than reading garbage.
        assert_eq!(f32_at(dnam, 152), 0.0, "absent amplitude stays default");
    }

    #[test]
    fn legacy_watr_clamps_a_positive_near_fog_plane() {
        let interner = StringInterner::new();
        let mut source = NV_CLEAN_WATER_DNAM.to_vec();
        // A few legacy records store a positive near plane, which inverts the
        // depth gradient in FO4; every vanilla FO4 water keeps it <= 0.
        source[144..148].copy_from_slice(&746.0_f32.to_le_bytes());
        let mut watr = legacy_watr(&interner, &source);

        normalize_legacy_watr(&mut watr, &interner);

        assert_eq!(f32_at(bytes(&watr, b"DNAM"), 44), 0.0);
    }
}
