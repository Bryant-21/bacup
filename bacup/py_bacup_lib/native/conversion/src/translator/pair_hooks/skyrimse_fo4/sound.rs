use super::SkyrimSeFo4Hook;
use super::world::map_marker_type;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};

impl SkyrimSeFo4Hook {
    pub(super) fn normalize_sopm_attenuation(record: &mut Record) {
        if record.sig.0 != *b"SOPM" {
            return;
        }

        if Self::normalize_legacy_sopm(record) {
            return;
        }

        let attenuates_with_distance = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"NAM1")
            .and_then(|entry| map_marker_type(&entry.value))
            .is_some_and(|flags| flags & 1 != 0);
        if !attenuates_with_distance {
            return;
        }

        let Some(attenuation) = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"ANAM")
            .and_then(|entry| fo4_attenuation(&entry.value))
        else {
            return;
        };

        record.fields.retain(|entry| {
            !matches!(entry.sig.0, sig if sig == *b"ANAM" || sig == *b"VNAM" || sig == *b"ATTN")
        });
        let vnam_index = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"ONAM")
            .or_else(|| {
                record
                    .fields
                    .iter()
                    .position(|entry| entry.sig.0 == *b"MNAM")
                    .map(|index| index + 1)
            })
            .unwrap_or(record.fields.len());
        record.fields.insert(
            vnam_index,
            FieldEntry {
                sig: SubrecordSig::from_str("VNAM").unwrap(),
                value: FieldValue::Bytes(smallvec::smallvec![0, 0]),
            },
        );
        let attenuation_index = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"ONAM")
            .map(|index| index + 1)
            .unwrap_or(vnam_index + 1);
        record.fields.insert(
            attenuation_index,
            FieldEntry {
                sig: SubrecordSig::from_str("ATTN").unwrap(),
                value: FieldValue::Bytes(attenuation),
            },
        );
    }

    fn normalize_legacy_sopm(record: &mut Record) -> bool {
        const LEGACY_FIELD_ORDER: [[u8; 4]; 5] = [*b"FNAM", *b"MNAM", *b"CNAM", *b"SNAM", *b"ANAM"];
        const LEGACY_SPEAKER_ROWS_WITH_LFE: [u8; 16] =
            [100, 0, 0, 40, 100, 0, 100, 0, 0, 100, 0, 40, 0, 100, 0, 100];
        const LEGACY_SPEAKER_ROWS_DRY: [u8; 16] =
            [100, 0, 0, 0, 100, 0, 100, 0, 0, 100, 0, 0, 0, 100, 0, 100];

        let legacy_start = usize::from(
            record
                .fields
                .first()
                .is_some_and(|entry| entry.sig.0 == *b"EDID"),
        );
        let legacy_fields = &record.fields[legacy_start..];
        if !legacy_fields
            .iter()
            .map(|entry| entry.sig.0)
            .eq(LEGACY_FIELD_ORDER)
        {
            return false;
        }
        let FieldValue::Bytes(data) = &legacy_fields[0].value else {
            return false;
        };
        if data.as_slice() != [1, 0, 0, 0] || field_u32(&legacy_fields[1].value) != Some(1) {
            return false;
        }
        let FieldValue::Bytes(input_channels) = &legacy_fields[2].value else {
            return false;
        };
        let FieldValue::Bytes(speaker_rows) = &legacy_fields[3].value else {
            return false;
        };
        let known_speaker_rows = speaker_rows.as_slice() == LEGACY_SPEAKER_ROWS_WITH_LFE
            || speaker_rows.as_slice() == LEGACY_SPEAKER_ROWS_DRY;
        if input_channels.as_slice() != [2, 0, 0, 0] || !known_speaker_rows {
            return false;
        }
        let Some(attenuation) = fo4_attenuation(&legacy_fields[4].value) else {
            return false;
        };

        let mut output_values = smallvec::SmallVec::<[u8; 32]>::from_slice(&[
            100,
            100,
            0,
            speaker_rows[3],
            50,
            50,
            50,
            50,
        ]);
        output_values.extend_from_slice(speaker_rows);
        let output_type = legacy_fields[1].clone();
        let editor_id = (legacy_start == 1).then(|| record.fields[0].clone());
        let mut normalized = smallvec::SmallVec::<[FieldEntry; 8]>::new();
        normalized.extend(editor_id);
        normalized.extend([
            FieldEntry {
                sig: SubrecordSig::from_str("NAM1").unwrap(),
                value: FieldValue::Bytes(data.clone()),
            },
            output_type,
            FieldEntry {
                sig: SubrecordSig::from_str("VNAM").unwrap(),
                value: FieldValue::Bytes(smallvec::smallvec![0, 0]),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("ONAM").unwrap(),
                value: FieldValue::Bytes(output_values),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("ATTN").unwrap(),
                value: FieldValue::Bytes(attenuation),
            },
        ]);
        record.fields = normalized;
        true
    }
}

fn field_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(u32::from_le_bytes(bytes.as_slice().try_into().unwrap()))
        }
        _ => None,
    }
}

fn fo4_attenuation(value: &FieldValue) -> Option<smallvec::SmallVec<[u8; 32]>> {
    let FieldValue::Bytes(source) = value else {
        return None;
    };
    if source.len() != 20 {
        return None;
    }
    let min_distance = f32::from_le_bytes(source[4..8].try_into().unwrap());
    let max_distance = f32::from_le_bytes(source[8..12].try_into().unwrap());
    if !min_distance.is_finite()
        || !max_distance.is_finite()
        || min_distance < 0.0
        || max_distance <= min_distance
    {
        return None;
    }

    let mut attenuation = smallvec::SmallVec::<[u8; 32]>::new();
    attenuation.extend_from_slice(&0.0_f32.to_le_bytes());
    attenuation.extend_from_slice(&0.0_f32.to_le_bytes());
    attenuation.extend_from_slice(&min_distance.to_le_bytes());
    attenuation.extend_from_slice(&max_distance.to_le_bytes());
    attenuation.extend_from_slice(&[0, 50, 80, 95]);
    attenuation.extend_from_slice(&source[13..17]);
    Some(attenuation)
}
