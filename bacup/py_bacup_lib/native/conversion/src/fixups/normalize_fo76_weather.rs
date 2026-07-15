use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{ParsedItem, ParsedRecord, WriteEffect};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::PluginSession;

const CLEAR_FOG_NEAR_FLOOR: f32 = 6_000.0;
const CLEAR_FOG_FAR_FLOOR: f32 = 600_000.0;

pub struct NormalizeFo76WeatherFixup;

impl Fixup for NormalizeFo76WeatherFixup {
    fn name(&self) -> &'static str {
        "normalize_fo76_weather"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        source_game == Some("fo76") && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut changed_form_ids = SmallVec::<[u32; 4]>::new();
        let records_changed = normalize_weather_items(
            &mut session.target_slot_mut().parsed.root_items,
            &mut changed_form_ids,
        );
        if records_changed > 0 {
            session.record_effect(WriteEffect::RecordContents {
                form_ids: changed_form_ids,
            });
        }

        let mut report = FixupReport::empty();
        report.records_changed = records_changed;
        Ok(report)
    }
}

fn normalize_weather_items(
    items: &mut [ParsedItem],
    changed_form_ids: &mut SmallVec<[u32; 4]>,
) -> u32 {
    let mut changed = 0;
    for item in items {
        match item {
            ParsedItem::Group(group) => {
                changed += normalize_weather_items(&mut group.children, changed_form_ids);
            }
            ParsedItem::Record(record) if record.signature.as_str() == "WTHR" => {
                if normalize_weather_record(record) {
                    changed_form_ids.push(record.form_id);
                    changed += 1;
                }
            }
            _ => {}
        }
    }
    changed
}

fn normalize_weather_record(record: &mut ParsedRecord) -> bool {
    let is_clear = record.subrecords.iter().any(|subrecord| {
        subrecord.signature.as_str() == "EDID"
            && subrecord.data.windows(5).any(|window| window == b"Clear")
    });
    let mut changed = false;

    for subrecord in &mut record.subrecords {
        let subrecord_changed = match subrecord.signature.as_str() {
            "DALC" => normalize_dalc_yplus(&mut subrecord.data),
            "FNAM" if is_clear => normalize_clear_fog(&mut subrecord.data),
            _ => false,
        };
        changed |= subrecord_changed;
    }

    if changed {
        record.raw_payload = None;
    }
    changed
}

fn normalize_dalc_yplus(data: &mut Bytes) -> bool {
    if data.len() < 29 {
        return false;
    }

    let mut normalized = data.to_vec();
    let mut changed = false;
    for channel in 0..3 {
        let replacement = rounded_byte_mean(normalized[channel], normalized[16 + channel]);
        if normalized[8 + channel] != replacement {
            normalized[8 + channel] = replacement;
            changed = true;
        }
    }
    if changed {
        *data = Bytes::from(normalized);
    }
    changed
}

fn rounded_byte_mean(left: u8, right: u8) -> u8 {
    let sum = u16::from(left) + u16::from(right);
    let floor = sum / 2;
    let rounded = if sum % 2 == 1 && floor % 2 == 1 {
        floor + 1
    } else {
        floor
    };
    rounded as u8
}

fn normalize_clear_fog(data: &mut Bytes) -> bool {
    if data.len() < 16 {
        return false;
    }

    let mut normalized = data.to_vec();
    let mut changed = false;
    for (offset, floor) in [
        (0, CLEAR_FOG_NEAR_FLOOR),
        (4, CLEAR_FOG_FAR_FLOOR),
        (8, CLEAR_FOG_NEAR_FLOOR),
        (12, CLEAR_FOG_FAR_FLOOR),
    ] {
        let value = f32::from_le_bytes(normalized[offset..offset + 4].try_into().unwrap());
        if value < floor {
            normalized[offset..offset + 4].copy_from_slice(&floor.to_le_bytes());
            changed = true;
        }
    }
    if changed {
        *data = Bytes::from(normalized);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use esp_authoring_core::plugin_runtime::ParsedSubrecord;
    use smol_str::SmolStr;

    fn subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(signature),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn weather(editor_id: &str, dalc: Vec<u8>, fog: [f32; 6]) -> ParsedRecord {
        let mut fog_data = Vec::new();
        for value in fog {
            fog_data.extend_from_slice(&value.to_le_bytes());
        }
        ParsedRecord {
            signature: SmolStr::new("WTHR"),
            form_id: 0x0743_98AE,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                subrecord("EDID", [editor_id.as_bytes(), b"\0"].concat()),
                subrecord("DALC", dalc),
                subrecord("FNAM", fog_data),
            ],
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        }
    }

    fn fog_values(record: &ParsedRecord) -> Vec<f32> {
        let data = &record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "FNAM")
            .unwrap()
            .data;
        data.chunks_exact(4)
            .map(|value| f32::from_le_bytes(value.try_into().unwrap()))
            .collect()
    }

    #[test]
    fn normalizes_dalc_yplus_with_python_rounding() {
        let mut dalc = vec![0; 29];
        dalc[0..3].copy_from_slice(&[0, 1, 2]);
        dalc[8..11].copy_from_slice(&[100, 100, 100]);
        dalc[16..19].copy_from_slice(&[1, 2, 3]);
        let mut record = weather("NewWeatherRain", dalc, [1.0; 6]);

        assert!(normalize_weather_record(&mut record));
        let dalc = &record.subrecords[1].data;
        assert_eq!(&dalc[8..11], &[0, 2, 2]);
        assert!(record.raw_payload.is_none());
        assert!(!normalize_weather_record(&mut record));
    }

    #[test]
    fn floors_only_clear_weather_near_and_far_distances() {
        let mut record = weather(
            "NewWeatherClear_i",
            vec![0; 29],
            [500.0, 135_000.0, 500.0, 100_000.0, 1.0, 0.5],
        );

        assert!(normalize_weather_record(&mut record));
        assert_eq!(
            fog_values(&record),
            vec![6_000.0, 600_000.0, 6_000.0, 600_000.0, 1.0, 0.5]
        );
    }

    #[test]
    fn clear_weather_floors_never_lower_existing_distances() {
        let mut record = weather(
            "Shelters_Weather_Flatlands_Clear",
            vec![0; 29],
            [69_000.0, 200_000.0, 800.0, 650_000.0, 1.0, 0.5],
        );

        assert!(normalize_weather_record(&mut record));
        assert_eq!(
            fog_values(&record),
            vec![69_000.0, 600_000.0, 6_000.0, 650_000.0, 1.0, 0.5]
        );
    }

    #[test]
    fn ignores_lowercase_clear_inside_nuclear_weather() {
        let mut record = weather(
            "Shelters_Weather_NuclearTestBunker_Smoky01",
            vec![0; 29],
            [450.0, 4_500.0, 450.0, 4_500.0, 1.0, 0.5],
        );

        assert!(!normalize_weather_record(&mut record));
        assert_eq!(
            fog_values(&record),
            vec![450.0, 4_500.0, 450.0, 4_500.0, 1.0, 0.5]
        );
    }

    #[test]
    fn ignores_short_dalc_and_fnam_payloads() {
        let mut record = weather("ATX_Weather_Clear", vec![0; 28], [6_000.0; 6]);
        record.subrecords[2].data = Bytes::from(vec![0; 15]);

        assert!(!normalize_weather_record(&mut record));
        assert!(record.raw_payload.is_some());
    }
}
