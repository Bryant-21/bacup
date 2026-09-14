use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::{StringInterner, Sym};

const STANDARD_DESCRIPTOR_TYPE: u64 = 0x1EEF_540A;
const RANDOM_FREQUENCY_SHIFT: u32 = 1 << 0;
pub(super) const LOOP: u32 = 1 << 4;
const MENU_SOUND: u32 = 1 << 5;
const SOUND_2D: u32 = 1 << 6;
const DIALOGUE_SOUND: u32 = 1 << 8;
const ENVELOPE_FAST: u32 = 1 << 9;
const ENVELOPE_SLOW: u32 = 1 << 10;
const SOUND_2D_RADIUS: u32 = 1 << 11;

pub(super) const AUDIO_CATEGORY_SFX: u32 = 0x0172A1;
pub(super) const AUDIO_CATEGORY_UI: u32 = 0x064451;
pub(super) const SOM_UI_DEFAULT: u32 = 0x0B75FB;
const SOM_DIALOGUE_3D_DEFAULT: u32 = 0x0B5184;
const DISTANCE_OUTPUT_MODELS: &[(u32, u32)] = &[
    (500, 0x099391),
    (800, 0x074808),
    (1_500, 0x05A28A),
    (3_000, 0x0ABEF3),
    (5_000, 0x03F77F),
    (10_000, 0x0B4248),
];

#[derive(Clone, Copy)]
struct LegacySoundData {
    maximum_attenuation_distance: u8,
    frequency_adjustment: i8,
    flags: u32,
    static_attenuation_db: u16,
    priority: u8,
}

impl Default for LegacySoundData {
    fn default() -> Self {
        Self {
            maximum_attenuation_distance: 10,
            frequency_adjustment: 0,
            flags: 0,
            static_attenuation_db: 0,
            priority: 128,
        }
    }
}

fn read_legacy_sound_data(record: &Record) -> LegacySoundData {
    let mut result = LegacySoundData::default();
    let mut short_data = None;
    let mut extended_data = None;

    for field in &record.fields {
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        match field.sig.0 {
            sig if sig == *b"SNDX" && bytes.len() >= 12 => short_data = Some(bytes.as_slice()),
            sig if sig == *b"SNDD" && bytes.len() >= 12 => extended_data = Some(bytes.as_slice()),
            _ => {}
        }
    }

    if let Some(bytes) = extended_data.or(short_data) {
        result.maximum_attenuation_distance = bytes[1];
        result.frequency_adjustment = bytes[2] as i8;
        result.flags = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        result.static_attenuation_db =
            i16::from_le_bytes(bytes[8..10].try_into().unwrap()).max(0) as u16;
        if bytes.len() >= 28 {
            result.priority = i32::from_le_bytes(bytes[24..28].try_into().unwrap())
                .clamp(0, u8::MAX as i32) as u8;
        }
    }

    if extended_data.is_none() {
        result.priority = record
            .fields
            .iter()
            .rev()
            .find_map(|field| {
                if field.sig.0 != *b"HNAM" {
                    return None;
                }
                match &field.value {
                    FieldValue::Int(value) => Some((*value).clamp(0, u8::MAX as i64) as u8),
                    FieldValue::Uint(value) => Some((*value).min(u8::MAX as u64) as u8),
                    FieldValue::Bytes(bytes) if bytes.len() >= 4 => Some(
                        i32::from_le_bytes(bytes[0..4].try_into().unwrap()).clamp(0, u8::MAX as i32)
                            as u8,
                    ),
                    _ => None,
                }
            })
            .unwrap_or(result.priority);
    }

    result
}

fn canonical_fo4_sound_path(source: &str) -> Option<String> {
    let normalized = source
        .trim()
        .trim_matches('\0')
        .replace('/', "\\")
        .trim_start_matches('\\')
        .to_string();
    if normalized.is_empty() || normalized.contains(':') {
        return None;
    }

    let mut relative = normalized.as_str();
    for prefix in ["data\\sound\\", "sound\\"] {
        if relative
            .get(..prefix.len())
            .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
        {
            relative = &relative[prefix.len()..];
            break;
        }
    }
    if relative.is_empty() {
        None
    } else {
        Some(format!("data\\Sound\\{relative}"))
    }
}

fn is_ui_sound(record: &Record, paths: &[String], interner: &StringInterner) -> bool {
    record.eid.is_some_and(|editor_id| {
        interner
            .resolve(editor_id)
            .is_some_and(|value| value.to_ascii_lowercase().starts_with("ui"))
    }) || paths.iter().any(|path| {
        let lower = path.to_ascii_lowercase();
        lower.contains("\\ui\\") || lower.contains("\\pipboy\\")
    })
}

pub(super) fn target_form_key(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        local,
        plugin: interner.intern("Fallout4.esm"),
    }
}

fn distance_output_model(maximum_attenuation_distance: u8) -> u32 {
    let target_distance = u32::from(maximum_attenuation_distance).saturating_mul(50);
    DISTANCE_OUTPUT_MODELS
        .iter()
        .min_by_key(|(distance, _)| distance.abs_diff(target_distance))
        .map(|(_, form_id)| *form_id)
        .unwrap()
}

fn output_model(data: LegacySoundData, ui: bool) -> u32 {
    if ui || data.flags & (MENU_SOUND | SOUND_2D | SOUND_2D_RADIUS) != 0 {
        SOM_UI_DEFAULT
    } else if data.flags & DIALOGUE_SOUND != 0 {
        SOM_DIALOGUE_3D_DEFAULT
    } else {
        distance_output_model(data.maximum_attenuation_distance)
    }
}

fn looping_value(flags: u32) -> u8 {
    if flags & LOOP != 0 {
        8
    } else if flags & ENVELOPE_FAST != 0 {
        16
    } else if flags & ENVELOPE_SLOW != 0 {
        32
    } else {
        0
    }
}

fn descriptor_data(data: LegacySoundData) -> SmallVec<[u8; 32]> {
    let (frequency_shift, frequency_variance) = if data.flags & RANDOM_FREQUENCY_SHIFT != 0 {
        (
            0,
            i16::from(data.frequency_adjustment)
                .unsigned_abs()
                .min(i8::MAX as u16) as u8,
        )
    } else {
        (data.frequency_adjustment as u8, 0)
    };
    let mut bytes = SmallVec::from_slice(&[frequency_shift, frequency_variance, data.priority, 0]);
    bytes.extend_from_slice(&data.static_attenuation_db.to_le_bytes());
    bytes
}

pub(super) fn rewrite_legacy_sound_descriptor(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"SOUN" {
        return;
    }

    let paths = record
        .fields
        .iter()
        .filter_map(|field| {
            if field.sig.0 != *b"FNAM" {
                return None;
            }
            let FieldValue::String(path) = &field.value else {
                return None;
            };
            canonical_fo4_sound_path(interner.resolve(*path)?)
        })
        .collect::<Vec<_>>();
    let data = read_legacy_sound_data(record);
    let ui = is_ui_sound(record, &paths, interner);
    let mut fields = SmallVec::new();
    fields.push(FieldEntry {
        sig: SubrecordSig(*b"CNAM"),
        value: FieldValue::Uint(STANDARD_DESCRIPTOR_TYPE),
    });
    fields.push(FieldEntry {
        sig: SubrecordSig(*b"GNAM"),
        value: FieldValue::FormKey(target_form_key(
            if ui {
                AUDIO_CATEGORY_UI
            } else {
                AUDIO_CATEGORY_SFX
            },
            interner,
        )),
    });
    fields.extend(paths.into_iter().map(|path| FieldEntry {
        sig: SubrecordSig(*b"ANAM"),
        value: FieldValue::String(interner.intern(&path)),
    }));
    fields.push(FieldEntry {
        sig: SubrecordSig(*b"ONAM"),
        value: FieldValue::FormKey(target_form_key(output_model(data, ui), interner)),
    });
    fields.push(FieldEntry {
        sig: SubrecordSig(*b"LNAM"),
        value: FieldValue::Bytes(SmallVec::from_slice(&[0, looping_value(data.flags), 0, 0])),
    });
    fields.push(FieldEntry {
        sig: SubrecordSig(*b"BNAM"),
        value: FieldValue::Bytes(descriptor_data(data)),
    });

    record.sig = SigCode(*b"SNDR");
    record.fields = fields;
}

pub(crate) fn legacy_sound_descriptor_substitution_mappings(
    source_entries: &[(Sym, FormKey, SigCode)],
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, SigCode)>>,
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    source_entries
        .iter()
        .filter(|(_, _, signature)| signature.0 == *b"SOUN")
        .filter_map(|(editor_id, source_form_key, _)| {
            let normalized = interner.resolve(*editor_id)?.to_ascii_lowercase();
            let normalized = interner.intern(&normalized);
            target_eid_index
                .get(&normalized)
                .and_then(|targets| {
                    targets
                        .iter()
                        .find(|(_, signature)| signature.0 == *b"SNDR")
                })
                .map(|(target_form_key, _)| (*source_form_key, *target_form_key))
        })
        .collect()
}
