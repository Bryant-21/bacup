use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::{StringInterner, Sym};
use smallvec::SmallVec;

const SKYRIM_TO_FO4_WEATHER_PERIODS: [usize; 8] = [0, 1, 2, 3, 3, 0, 1, 2];
const FO4_GDRY_NONE: u32 = 0x001B_40E8;
const FO4_WTHR_NAM0_SIZE: usize = 608;
const DEFAULT_CLOUD_ROWS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GodRayProfile {
    None,
    Clear,
    Fog,
    Misty,
    Rain,
    Overcast,
    Radstorm,
    Dusty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimeOfDay {
    Dawn,
    Day,
    Dusk,
    Night,
}

pub(crate) fn normalize_skyrim_weather(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"WTHR" {
        return;
    }

    let already_target_layout = record.fields.iter().any(|entry| {
        (entry.sig.0 == *b"NAM0"
            && matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.len() == FO4_WTHR_NAM0_SIZE))
            || (entry.sig.0 == *b"IMSP"
                && matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.len() == 32))
    });
    normalize_weather_volumetric_lighting(record);
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
                    if let Some(rows) =
                        expand_weather_period_rows(&mut entry.value, 4, already_target_layout)
                    {
                        pnam_rows = rows;
                    } else {
                        pnam_rows = DEFAULT_CLOUD_ROWS;
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
                    pnam_rows = structured_rows(&entry.value).unwrap_or(1).max(1);
                }
            }
            sig if sig == *b"JNAM" => {
                if raw(&entry.value).is_some() {
                    if expand_weather_period_rows(&mut entry.value, 4, already_target_layout)
                        .is_none()
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
                }
            }
            sig if sig == *b"NAM0" => rebuild_nam0(&mut entry.value),
            sig if sig == *b"NAM4" => {
                if let FieldValue::Bytes(bytes) = &entry.value
                    && (bytes.is_empty() || bytes.len() % 4 != 0)
                {
                    entry.value = raw_value(vec![0_u8; pnam_rows * 4]);
                }
            }
            sig if sig == *b"FNAM" => match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() == 72 => {}
                FieldValue::Bytes(bytes) if bytes.len() == 32 => {
                    let mut target = vec![0_u8; 72];
                    target[..32].copy_from_slice(bytes);
                    entry.value = raw_value(target);
                }
                FieldValue::Struct(_) => append_float_defaults(
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
                ),
                _ => entry.value = raw_value(vec![0_u8; 72]),
            },
            sig if sig == *b"IMSP" => {
                if raw(&entry.value).is_some() {
                    if expand_weather_period_rows(&mut entry.value, 4, already_target_layout)
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
    rebuild_dalc(record, interner);

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
        (*b"VNAM", 4),
        (*b"WNAM", 4),
    ] {
        ensure_single_required(record, sig, len);
    }

    record
        .fields
        .sort_by_key(|entry| weather_target_order(entry.sig.0));
}

pub(crate) fn rewrite_skyrim_weather_master_refs(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
) -> bool {
    if record.sig.0 != *b"WTHR" {
        return false;
    }

    let mut changed = false;
    for entry in &mut record.fields {
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let offsets: Vec<usize> = match entry.sig.0 {
            sig if sig == *b"WGDR" || sig == *b"IMSP" => (0..bytes.len()).step_by(4).collect(),
            sig if sig == *b"SNAM" => vec![0],
            sig if sig == *b"UNAM" => vec![0, 8],
            _ => continue,
        };
        for offset in offsets {
            let Some(raw) = bytes.get(offset..offset + 4) else {
                continue;
            };
            if u32::from_le_bytes(raw.try_into().expect("four-byte weather reference")) >> 24 == 0 {
                continue;
            }
            changed |= mapper
                .rewrite_raw_formid_at(bytes.as_mut_slice(), offset)
                .unwrap_or(false);
        }
    }
    changed
}

fn normalize_weather_volumetric_lighting(record: &mut Record) {
    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(entry.sig.0, sig if sig == *b"WGDR" || sig == *b"HNAM"))
        .unwrap_or(record.fields.len());
    let value = record
        .fields
        .iter()
        .find(|entry| {
            entry.sig.0 == *b"WGDR"
                && matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.len() == 32)
        })
        .map(|entry| entry.value.clone())
        .or_else(|| {
            record.fields.iter().find_map(|entry| {
                if entry.sig.0 != *b"HNAM" {
                    return None;
                }
                let FieldValue::Bytes(bytes) = &entry.value else {
                    return None;
                };
                expand_weather_periods(bytes, 4)
                    .map(|bytes| FieldValue::Bytes(SmallVec::from_vec(bytes)))
            })
        })
        .unwrap_or_else(|| FieldValue::Bytes(SmallVec::from_vec(none_wgdr_bytes())));

    record
        .fields
        .retain(|entry| !matches!(entry.sig.0, sig if sig == *b"WGDR" || sig == *b"HNAM"));
    record.fields.insert(
        insert_at.min(record.fields.len()),
        FieldEntry {
            sig: SubrecordSig(*b"WGDR"),
            value,
        },
    );
}

fn raw(value: &FieldValue) -> Option<&[u8]> {
    match value {
        FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
        _ => None,
    }
}

fn raw_value(bytes: Vec<u8>) -> FieldValue {
    FieldValue::Bytes(SmallVec::from_vec(bytes))
}

fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(sig),
        value,
    }
}

fn field_name<'a>(
    fields: &'a [(Sym, FieldValue)],
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

fn expand_weather_periods(source: &[u8], period_width: usize) -> Option<Vec<u8>> {
    if period_width == 0 || source.len() != period_width * 4 {
        return None;
    }
    let mut target = vec![0_u8; period_width * 8];
    for (target_period, source_period) in SKYRIM_TO_FO4_WEATHER_PERIODS.iter().enumerate() {
        let source_start = source_period * period_width;
        let target_start = target_period * period_width;
        target[target_start..target_start + period_width]
            .copy_from_slice(&source[source_start..source_start + period_width]);
    }
    Some(target)
}

fn expand_weather_period_rows(
    value: &mut FieldValue,
    period_width: usize,
    already_target_layout: bool,
) -> Option<usize> {
    let bytes = raw(value)?;
    let source_row = period_width * 4;
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
        target.extend_from_slice(&expand_weather_periods(source, period_width)?);
    }
    *value = raw_value(target);
    Some(rows)
}

fn rebuild_nam0(value: &mut FieldValue) {
    let Some(source) = raw(value) else {
        *value = raw_value(vec![0_u8; FO4_WTHR_NAM0_SIZE]);
        return;
    };
    if source.len() == FO4_WTHR_NAM0_SIZE {
        return;
    }
    if source.len() != 272 {
        *value = raw_value(vec![0_u8; FO4_WTHR_NAM0_SIZE]);
        return;
    }
    let mut target = vec![0_u8; FO4_WTHR_NAM0_SIZE];
    for (source_row, target_row) in source.chunks_exact(16).zip(target.chunks_exact_mut(32)) {
        target_row.copy_from_slice(
            &expand_weather_periods(source_row, 4).expect("four source weather colors"),
        );
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
        _ => {}
    }
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

fn normalize_dalc_value(mut value: FieldValue, interner: &StringInterner) -> FieldValue {
    match &mut value {
        FieldValue::Bytes(bytes) if bytes.len() == 32 => value,
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

fn rebuild_dalc(record: &mut Record, interner: &StringInterner) {
    let insert_at = record
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"DALC")
        .unwrap_or(record.fields.len());
    let mut rows: Vec<_> = record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"DALC")
        .map(|entry| normalize_dalc_value(entry.value.clone(), interner))
        .collect();
    record.fields.retain(|entry| entry.sig.0 != *b"DALC");

    let target_rows = if rows.len() <= 4 {
        rows.resize_with(4, || raw_value(vec![0_u8; 32]));
        SKYRIM_TO_FO4_WEATHER_PERIODS
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

fn weather_target_order(sig: [u8; 4]) -> usize {
    match sig {
        sig if sig == *b"EDID" => 0,
        sig if sig[2] == b'T' && sig[3] == b'X' => 10,
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
        _ => 200,
    }
}

pub(crate) fn skyrimse_fo4_voli_gdry_substitution_mappings(
    source_entries: &[(Sym, FormKey, SigCode)],
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    let target_plugin = interner.intern("Fallout4.esm");
    source_entries
        .iter()
        .filter_map(|(editor_id, source_form_key, signature)| {
            if signature.0 != *b"VOLI" {
                return None;
            }
            let editor_id = interner.resolve(*editor_id)?;
            Some((
                *source_form_key,
                FormKey {
                    local: god_ray_donor(editor_id),
                    plugin: target_plugin,
                },
            ))
        })
        .collect()
}

fn none_wgdr_bytes() -> Vec<u8> {
    let mut target = Vec::with_capacity(32);
    for _ in 0..8 {
        target.extend_from_slice(&FO4_GDRY_NONE.to_le_bytes());
    }
    target
}

fn god_ray_donor(editor_id: &str) -> u32 {
    let editor_id = editor_id.to_ascii_lowercase();
    let profile = god_ray_profile(&editor_id);
    let time = time_of_day(&editor_id);

    match (profile, time) {
        (GodRayProfile::None, _) => FO4_GDRY_NONE,
        (GodRayProfile::Clear, TimeOfDay::Dawn) => 0x0021_6A93,
        (GodRayProfile::Clear, TimeOfDay::Day) => 0x0021_6A92,
        (GodRayProfile::Clear, TimeOfDay::Dusk) => 0x0021_6A94,
        (GodRayProfile::Clear, TimeOfDay::Night) => FO4_GDRY_NONE,
        (GodRayProfile::Fog, TimeOfDay::Dawn) => 0x0021_8FA0,
        (GodRayProfile::Fog, TimeOfDay::Day) => 0x0021_8FA4,
        (GodRayProfile::Fog, TimeOfDay::Dusk) => 0x0021_8FA1,
        (GodRayProfile::Fog, TimeOfDay::Night) => 0x001C_C192,
        (GodRayProfile::Misty, TimeOfDay::Dawn) => 0x0021_6A88,
        (GodRayProfile::Misty, TimeOfDay::Day) => 0x0021_6A84,
        (GodRayProfile::Misty, TimeOfDay::Dusk) => 0x001C_C191,
        (GodRayProfile::Misty, TimeOfDay::Night) => 0x001C_C192,
        (GodRayProfile::Rain, TimeOfDay::Dawn) => 0x0021_15D0,
        (GodRayProfile::Rain, TimeOfDay::Day) => 0x001C_D09F,
        (GodRayProfile::Rain, TimeOfDay::Dusk) => 0x0021_15D1,
        (GodRayProfile::Rain, TimeOfDay::Night) => 0x001C_C192,
        (GodRayProfile::Overcast, TimeOfDay::Dawn) => 0x001C_855C,
        (GodRayProfile::Overcast, TimeOfDay::Day) => 0x001C_855D,
        (GodRayProfile::Overcast, TimeOfDay::Dusk) => 0x001C_855E,
        (GodRayProfile::Overcast, TimeOfDay::Night) => FO4_GDRY_NONE,
        (GodRayProfile::Radstorm, TimeOfDay::Dawn) => 0x001F_495D,
        (GodRayProfile::Radstorm, TimeOfDay::Day) => 0x0022_4A94,
        (GodRayProfile::Radstorm, TimeOfDay::Dusk) => 0x0022_4591,
        (GodRayProfile::Radstorm, TimeOfDay::Night) => 0x0022_458F,
        (GodRayProfile::Dusty, TimeOfDay::Dawn) => 0x001F_61AB,
        (GodRayProfile::Dusty, TimeOfDay::Day) => 0x001F_61AD,
        (GodRayProfile::Dusty, TimeOfDay::Dusk) => 0x001F_61AE,
        (GodRayProfile::Dusty, TimeOfDay::Night) => 0x001F_61AC,
    }
}

fn god_ray_profile(editor_id: &str) -> GodRayProfile {
    if editor_id.contains("off") {
        GodRayProfile::None
    } else if contains_any(editor_id, &["radstorm", "nuke", "nukastorm", "corrupt"]) {
        GodRayProfile::Radstorm
    } else if contains_any(editor_id, &["ash", "desert", "sand", "outwaste"]) {
        GodRayProfile::Dusty
    } else if contains_any(editor_id, &["mistyrainy", "rain", "thunderstorm"]) {
        GodRayProfile::Rain
    } else if contains_any(editor_id, &["fog", "flooded"]) {
        GodRayProfile::Fog
    } else if contains_any(editor_id, &["misty", "pollen", "mothman"]) {
        GodRayProfile::Misty
    } else if contains_any(
        editor_id,
        &["clear", "fireworks", "fallfoliage", "bigbloom", "aurora"],
    ) {
        GodRayProfile::Clear
    } else if contains_any(editor_id, &["overcast", "cloudy", "storm", "snow"]) {
        GodRayProfile::Overcast
    } else {
        GodRayProfile::None
    }
}

fn time_of_day(editor_id: &str) -> TimeOfDay {
    if editor_id.contains("night") {
        TimeOfDay::Night
    } else if contains_any(editor_id, &["dawn", "sunrise"]) {
        TimeOfDay::Dawn
    } else if contains_any(editor_id, &["dusk", "sunset", "dim"]) {
        TimeOfDay::Dusk
    } else {
        TimeOfDay::Day
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weather(interner: &mut StringInterner) -> Record {
        Record::new(
            SigCode(*b"WTHR"),
            FormKey::parse("000800@Skyrim.esm", interner).unwrap(),
        )
    }

    fn push_bytes(record: &mut Record, sig: [u8; 4], bytes: Vec<u8>) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(sig),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        });
    }

    fn wgdr_values(record: &Record) -> Vec<u32> {
        let bytes = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"WGDR")
            .and_then(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .unwrap();
        bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    fn bytes<'a>(record: &'a Record, sig: &[u8; 4]) -> &'a [u8] {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *sig)
            .and_then(|entry| raw(&entry.value))
            .unwrap()
    }

    const REQUIRED_ORDER: &[&str] = &[
        "LNAM", "MNAM", "NNAM", "RNAM", "QNAM", "PNAM", "JNAM", "NAM0", "NAM4", "FNAM", "DATA",
        "NAM1", "IMSP", "WGDR", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC", "DALC",
        "UNAM", "VNAM", "WNAM",
    ];

    fn assert_required_contract(record: &Record, cloud_rows: usize) {
        let required = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .filter(|sig| REQUIRED_ORDER.contains(sig))
            .collect::<Vec<_>>();
        assert_eq!(required, REQUIRED_ORDER);
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
            assert_eq!(bytes(record, &sig).len(), len);
            assert_eq!(
                record
                    .fields
                    .iter()
                    .filter(|entry| entry.sig.0 == sig)
                    .count(),
                1
            );
        }
        let dalc = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"DALC")
            .collect::<Vec<_>>();
        assert_eq!(dalc.len(), 8);
        assert!(
            dalc.iter()
                .all(|entry| raw(&entry.value).unwrap().len() == 32)
        );
    }

    #[test]
    fn skyrim_hnam_expands_to_one_fo4_wgdr_with_exact_period_mapping() {
        let mut interner = StringInterner::new();
        let mut record = weather(&mut interner);
        let source = [0x0102_0304_u32, 0x1112_1314, 0x2122_2324, 0x3132_3334];
        push_bytes(
            &mut record,
            *b"HNAM",
            source
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect(),
        );
        push_bytes(&mut record, *b"HNAM", vec![0xCC; 16]);

        normalize_skyrim_weather(&mut record, &interner);

        assert_eq!(
            wgdr_values(&record),
            vec![
                source[0], source[1], source[2], source[3], source[3], source[0], source[1],
                source[2]
            ]
        );
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"WGDR")
                .count(),
            1
        );
        assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"HNAM"));

        let once = record.fields.clone();
        normalize_skyrim_weather(&mut record, &interner);
        assert_eq!(record.fields, once);
    }

    #[test]
    fn valid_wgdr_wins_and_malformed_or_missing_source_uses_none_fallback() {
        let mut interner = StringInterner::new();
        let mut record = weather(&mut interner);
        let valid = (0_u32..8)
            .map(|value| 0xA000_0000 | value)
            .collect::<Vec<_>>();
        push_bytes(&mut record, *b"WGDR", vec![0xCC; 7]);
        push_bytes(
            &mut record,
            *b"WGDR",
            valid.iter().flat_map(|value| value.to_le_bytes()).collect(),
        );
        push_bytes(&mut record, *b"HNAM", vec![0xDD; 16]);

        normalize_skyrim_weather(&mut record, &interner);

        assert_eq!(wgdr_values(&record), valid);
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"WGDR")
                .count(),
            1
        );
        assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"HNAM"));

        let mut malformed = weather(&mut interner);
        push_bytes(&mut malformed, *b"HNAM", vec![0xEE; 15]);
        normalize_skyrim_weather(&mut malformed, &interner);
        assert_eq!(wgdr_values(&malformed), vec![FO4_GDRY_NONE; 8]);

        let mut missing = weather(&mut interner);
        normalize_skyrim_weather(&mut missing, &interner);
        assert_eq!(wgdr_values(&missing), vec![FO4_GDRY_NONE; 8]);
    }

    #[test]
    fn skyrim_weather_maps_every_period_layout_and_dalc_exactly() {
        let mut interner = StringInterner::new();
        let mut record = weather(&mut interner);
        push_bytes(&mut record, *b"DATA", vec![0x10; 19]);
        let source_periods = [0x1111_1111_u32, 0x2222_2222, 0x3333_3333, 0x4444_4444];
        let source_period_bytes = source_periods
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        push_bytes(&mut record, *b"PNAM", source_period_bytes.clone());
        push_bytes(&mut record, *b"JNAM", source_period_bytes.clone());
        push_bytes(&mut record, *b"IMSP", source_period_bytes);
        let mut nam0 = vec![0_u8; 272];
        for (row, bytes) in nam0.chunks_exact_mut(16).enumerate() {
            for period in 0..4 {
                let value = ((row as u32 + 1) << 8) | period as u32;
                bytes[period * 4..period * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        push_bytes(&mut record, *b"NAM0", nam0);
        push_bytes(&mut record, *b"FNAM", vec![0x40; 32]);
        let source_dalc = (0_u8..4)
            .map(|period| {
                let mut row = vec![period; 32];
                row[0] = 0xA0 + period;
                row[31] = 0xF0 + period;
                row
            })
            .collect::<Vec<_>>();
        for row in &source_dalc {
            push_bytes(&mut record, *b"DALC", row.clone());
        }

        normalize_skyrim_weather(&mut record, &interner);

        assert_required_contract(&record, 1);
        let expected_periods = [
            source_periods[0],
            source_periods[1],
            source_periods[2],
            source_periods[3],
            source_periods[3],
            source_periods[0],
            source_periods[1],
            source_periods[2],
        ];
        for sig in [*b"PNAM", *b"JNAM", *b"IMSP"] {
            let actual = bytes(&record, &sig)
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                expected_periods,
                "{}",
                std::str::from_utf8(&sig).unwrap()
            );
        }
        for row in 0..17 {
            let actual = bytes(&record, b"NAM0")[row * 32..row * 32 + 32]
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect::<Vec<_>>();
            let source = (0..4)
                .map(|period| ((row as u32 + 1) << 8) | period)
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                vec![
                    source[0], source[1], source[2], source[3], source[3], source[0], source[1],
                    source[2]
                ]
            );
        }
        assert!(bytes(&record, b"NAM0")[576..].iter().all(|byte| *byte == 0));
        let dalc = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"DALC")
            .map(|entry| raw(&entry.value).unwrap())
            .collect::<Vec<_>>();
        for (actual, source_index) in dalc.iter().zip(SKYRIM_TO_FO4_WEATHER_PERIODS) {
            assert_eq!(*actual, source_dalc[source_index]);
        }
        assert_eq!(&bytes(&record, b"DATA")[..19], &[0x10; 19]);
        assert_eq!(bytes(&record, b"DATA")[19], 0);
        assert_eq!(&bytes(&record, b"FNAM")[..32], &[0x40; 32]);
        assert!(bytes(&record, b"FNAM")[32..].iter().all(|byte| *byte == 0));

        let once = record.fields.clone();
        normalize_skyrim_weather(&mut record, &interner);
        assert_eq!(record.fields, once);
    }

    #[test]
    fn structured_period_fields_copy_distinct_source_values_with_exact_mapping() {
        const PNAM_SOURCE_NAMES: [&str; 16] = [
            "cloud_colors_sunrise_red",
            "cloud_colors_sunrise_green",
            "cloud_colors_sunrise_blue",
            "unknown_u8_3",
            "cloud_colors_day_red",
            "cloud_colors_day_green",
            "cloud_colors_day_blue",
            "unknown_u8_7",
            "cloud_colors_sunset_red",
            "cloud_colors_sunset_green",
            "cloud_colors_sunset_blue",
            "unknown_u8_11",
            "cloud_colors_night_red",
            "cloud_colors_night_green",
            "cloud_colors_night_blue",
            "unknown_u8_15",
        ];
        const PNAM_TARGET_NAMES: [&str; 16] = [
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
        ];
        const PNAM_SOURCE_INDEX_BY_TARGET: [usize; 16] =
            [12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

        let mut interner = StringInterner::new();
        let mut record = weather(&mut interner);
        let pnam_rows = [0_u64, 100]
            .into_iter()
            .map(|base| {
                FieldValue::Struct(
                    PNAM_SOURCE_NAMES
                        .iter()
                        .enumerate()
                        .map(|(index, name)| {
                            (
                                interner.intern(name),
                                FieldValue::Uint(base + index as u64 + 1),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        record
            .fields
            .push(field(*b"PNAM", FieldValue::List(pnam_rows)));
        record.fields.push(field(
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
        let source_plugin = interner.intern("Skyrim.esm");
        record.fields.push(field(
            *b"IMSP",
            FieldValue::Struct(
                ["sunrise", "day", "sunset", "night"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| {
                        (
                            interner.intern(name),
                            FieldValue::FormKey(FormKey {
                                local: index as u32 + 1,
                                plugin: source_plugin,
                            }),
                        )
                    })
                    .collect(),
            ),
        ));

        normalize_skyrim_weather(&mut record, &interner);

        let FieldValue::List(pnam_rows) = &record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"PNAM")
            .unwrap()
            .value
        else {
            panic!("structured PNAM list");
        };
        for (row_index, row) in pnam_rows.iter().enumerate() {
            let FieldValue::Struct(fields) = row else {
                panic!("structured PNAM row");
            };
            for (target_name, source_index) in
                PNAM_TARGET_NAMES.iter().zip(PNAM_SOURCE_INDEX_BY_TARGET)
            {
                assert_eq!(
                    field_name(fields, target_name, &interner),
                    Some(&FieldValue::Uint(
                        row_index as u64 * 100 + source_index as u64 + 1
                    )),
                    "{target_name}"
                );
            }
        }

        let FieldValue::Struct(jnam) = &record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"JNAM")
            .unwrap()
            .value
        else {
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

        let FieldValue::Struct(imsp) = &record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"IMSP")
            .unwrap()
            .value
        else {
            panic!("structured IMSP");
        };
        for (name, expected_local) in [
            ("early_sunrise", 4),
            ("late_sunrise", 1),
            ("early_sunset", 2),
            ("late_sunset", 3),
        ] {
            assert_eq!(
                field_name(imsp, name, &interner),
                Some(&FieldValue::FormKey(FormKey {
                    local: expected_local,
                    plugin: source_plugin,
                })),
                "{name}"
            );
        }

        let once = record.fields.clone();
        normalize_skyrim_weather(&mut record, &interner);
        assert_eq!(record.fields, once);
    }

    #[test]
    fn malformed_skyrim_weather_fields_use_complete_safe_defaults() {
        let mut interner = StringInterner::new();
        let mut record = weather(&mut interner);
        for sig in [
            *b"LNAM", *b"MNAM", *b"NNAM", *b"RNAM", *b"QNAM", *b"PNAM", *b"JNAM", *b"NAM0",
            *b"NAM4", *b"FNAM", *b"DATA", *b"NAM1", *b"IMSP", *b"HNAM", *b"DALC", *b"UNAM",
            *b"VNAM", *b"WNAM",
        ] {
            push_bytes(&mut record, sig, vec![0xCC; 3]);
        }

        normalize_skyrim_weather(&mut record, &interner);

        assert_required_contract(&record, DEFAULT_CLOUD_ROWS);
        for entry in &record.fields {
            if REQUIRED_ORDER.contains(&entry.sig.as_str()) {
                assert!(!raw(&entry.value).unwrap().contains(&0xCC));
            }
        }
        assert_eq!(wgdr_values(&record), vec![FO4_GDRY_NONE; 8]);
    }

    #[test]
    fn skyrim_donor_profiles_include_cloudy_and_target_exact_fo4_ids() {
        for (editor_id, expected) in [
            ("SkyrimClearSunrise", 0x0021_6A93),
            ("SkyrimFogDay", 0x0021_8FA4),
            ("SkyrimMistySunset", 0x001C_C191),
            ("SkyrimRainNight", 0x001C_C192),
            ("SkyrimCloudySunrise", 0x001C_855C),
            ("SkyrimSnowSunset", 0x001C_855E),
            ("SkyrimClearNight", FO4_GDRY_NONE),
            ("SkyrimUnclassified", FO4_GDRY_NONE),
        ] {
            assert_eq!(god_ray_donor(editor_id), expected, "{editor_id}");
        }
    }

    #[test]
    fn skyrim_voli_mappings_target_fallout4_gdry_donors_only() {
        let mut interner = StringInterner::new();
        let source = FormKey::parse("000800@Skyrim.esm", &mut interner).unwrap();
        let entries = vec![
            (
                interner.intern("SkyrimCloudyDay"),
                source,
                SigCode(*b"VOLI"),
            ),
            (
                interner.intern("SkyrimCloudyWeather"),
                FormKey::parse("000801@Skyrim.esm", &mut interner).unwrap(),
                SigCode(*b"WTHR"),
            ),
        ];

        let mappings = skyrimse_fo4_voli_gdry_substitution_mappings(&entries, &interner);

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].0, source);
        assert_eq!(mappings[0].1.local, 0x001C_855D);
        assert_eq!(interner.resolve(mappings[0].1.plugin), Some("Fallout4.esm"));
    }
}
