use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{ParsedItem, ParsedRecord, WriteEffect};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::PluginSession;

const CLEAR_FOG_NEAR_FLOOR: f32 = 6_000.0;
const CLEAR_FOG_FAR_FLOOR: f32 = 600_000.0;
const FO76_GREEN_YPLUS: [u8; 3] = [0x05, 0xBC, 0x41];

const COMMONWEALTH_CLEAR_YPLUS: [[u8; 3]; 8] = [
    [0x22, 0x26, 0x2D],
    [0x52, 0x60, 0x6F],
    [0x25, 0x2B, 0x37],
    [0x0E, 0x14, 0x1B],
    [0x20, 0x24, 0x2B],
    [0x43, 0x4C, 0x5A],
    [0x3F, 0x48, 0x57],
    [0x16, 0x19, 0x1E],
];
const COMMONWEALTH_FOGGY_YPLUS: [[u8; 3]; 8] = [
    [0x17, 0x20, 0x27],
    [0x45, 0x59, 0x64],
    [0x17, 0x20, 0x27],
    [0x14, 0x1F, 0x25],
    [0x13, 0x1D, 0x25],
    [0x49, 0x51, 0x5C],
    [0x4D, 0x56, 0x61],
    [0x13, 0x1D, 0x25],
];
const COMMONWEALTH_MISTY_YPLUS: [[u8; 3]; 8] = [
    [0x1D, 0x25, 0x30],
    [0x4C, 0x5B, 0x66],
    [0x1B, 0x22, 0x2C],
    [0x10, 0x18, 0x1C],
    [0x14, 0x1D, 0x23],
    [0x46, 0x52, 0x5F],
    [0x46, 0x52, 0x5F],
    [0x14, 0x1D, 0x23],
];
const COMMONWEALTH_RAIN_YPLUS: [[u8; 3]; 8] = [
    [0x16, 0x1C, 0x20],
    [0x22, 0x2A, 0x2E],
    [0x16, 0x1C, 0x20],
    [0x09, 0x0D, 0x0F],
    [0x0A, 0x0E, 0x10],
    [0x1E, 0x23, 0x28],
    [0x1E, 0x23, 0x28],
    [0x0A, 0x0E, 0x10],
];
const COMMONWEALTH_OVERCAST_YPLUS: [[u8; 3]; 8] = [
    [0x13, 0x1B, 0x20],
    [0x23, 0x2F, 0x36],
    [0x13, 0x1B, 0x20],
    [0x07, 0x0A, 0x0C],
    [0x0D, 0x13, 0x16],
    [0x1C, 0x24, 0x2D],
    [0x1C, 0x24, 0x2D],
    [0x0D, 0x13, 0x16],
];
const COMMONWEALTH_RADSTORM_YPLUS: [[u8; 3]; 8] = [
    [0x12, 0x15, 0x0F],
    [0x15, 0x15, 0x0F],
    [0x12, 0x15, 0x0F],
    [0x10, 0x13, 0x0E],
    [0x12, 0x15, 0x0F],
    [0x12, 0x15, 0x0F],
    [0x12, 0x15, 0x0F],
    [0x12, 0x15, 0x0F],
];
const NUKA_WORLD_DUST_YPLUS: [[u8; 3]; 8] = [
    [0x19, 0x1D, 0x22],
    [0x3A, 0x49, 0x4E],
    [0x19, 0x1D, 0x22],
    [0x19, 0x1D, 0x22],
    [0x19, 0x1D, 0x22],
    [0x2D, 0x36, 0x3F],
    [0x2D, 0x36, 0x3F],
    [0x19, 0x1D, 0x22],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WeatherProfile {
    None,
    Clear,
    Fog,
    Misty,
    Rain,
    Overcast,
    Radstorm,
    Dusty,
}

#[derive(Clone, Copy)]
struct HeightRanges {
    day_near: f32,
    night_near: f32,
    day_far: f32,
    night_far: f32,
}

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
    let profile = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "EDID")
        .and_then(|subrecord| std::str::from_utf8(&subrecord.data).ok())
        .map(weather_profile)
        .unwrap_or(WeatherProfile::None);
    let mut changed = false;
    let mut dalc_period = 0;

    for subrecord in &mut record.subrecords {
        let subrecord_changed = match subrecord.signature.as_str() {
            "DALC" => {
                let changed = normalize_dalc_yplus(&mut subrecord.data, profile, dalc_period);
                dalc_period += 1;
                changed
            }
            "FNAM" => normalize_fog(&mut subrecord.data, profile),
            _ => false,
        };
        changed |= subrecord_changed;
    }

    if changed {
        record.raw_payload = None;
    }
    changed
}

fn normalize_dalc_yplus(data: &mut Bytes, profile: WeatherProfile, period: usize) -> bool {
    if data.len() < 29 || data[8..11] != FO76_GREEN_YPLUS {
        return false;
    }
    let Some(replacement) = fo4_dalc_yplus(profile).get(period) else {
        return false;
    };

    let mut normalized = data.to_vec();
    normalized[8..11].copy_from_slice(replacement);
    *data = Bytes::from(normalized);
    true
}

fn fo4_dalc_yplus(profile: WeatherProfile) -> &'static [[u8; 3]; 8] {
    match profile {
        WeatherProfile::Clear | WeatherProfile::None => &COMMONWEALTH_CLEAR_YPLUS,
        WeatherProfile::Fog => &COMMONWEALTH_FOGGY_YPLUS,
        WeatherProfile::Misty => &COMMONWEALTH_MISTY_YPLUS,
        WeatherProfile::Rain => &COMMONWEALTH_RAIN_YPLUS,
        WeatherProfile::Overcast => &COMMONWEALTH_OVERCAST_YPLUS,
        WeatherProfile::Radstorm => &COMMONWEALTH_RADSTORM_YPLUS,
        WeatherProfile::Dusty => &NUKA_WORLD_DUST_YPLUS,
    }
}

fn weather_profile(editor_id: &str) -> WeatherProfile {
    let editor_id = editor_id.to_ascii_lowercase();
    if editor_id.contains("off") {
        WeatherProfile::None
    } else if contains_any(&editor_id, &["radstorm", "nuke", "nukastorm", "corrupt"]) {
        WeatherProfile::Radstorm
    } else if contains_any(&editor_id, &["ash", "desert", "sand", "outwaste"]) {
        WeatherProfile::Dusty
    } else if contains_any(&editor_id, &["mistyrainy", "rain", "thunderstorm"]) {
        WeatherProfile::Rain
    } else if contains_any(&editor_id, &["fog", "flooded"]) {
        WeatherProfile::Fog
    } else if contains_any(&editor_id, &["misty", "pollen", "mothman"]) {
        WeatherProfile::Misty
    } else if contains_any(
        &editor_id,
        &["clear", "fireworks", "fallfoliage", "bigbloom", "aurora"],
    ) {
        WeatherProfile::Clear
    } else if contains_any(&editor_id, &["overcast", "storm", "snow"]) {
        WeatherProfile::Overcast
    } else {
        WeatherProfile::None
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn fo4_height_ranges(profile: WeatherProfile) -> Option<HeightRanges> {
    match profile {
        WeatherProfile::Clear | WeatherProfile::Dusty => Some(HeightRanges {
            day_near: 25_000.0,
            night_near: 10_000.0,
            day_far: 15_000.0,
            night_far: 10_000.0,
        }),
        WeatherProfile::Fog | WeatherProfile::Radstorm => Some(HeightRanges {
            day_near: 10_000.0,
            night_near: 10_000.0,
            day_far: 10_000.0,
            night_far: 10_000.0,
        }),
        WeatherProfile::Misty => Some(HeightRanges {
            day_near: 10_000.0,
            night_near: 10_000.0,
            day_far: 15_000.0,
            night_far: 10_000.0,
        }),
        WeatherProfile::Rain | WeatherProfile::Overcast => Some(HeightRanges {
            day_near: 15_000.0,
            night_near: 15_000.0,
            day_far: 10_000.0,
            night_far: 10_000.0,
        }),
        WeatherProfile::None => None,
    }
}

fn normalize_fog(data: &mut Bytes, profile: WeatherProfile) -> bool {
    if data.len() < 16 {
        return false;
    }

    let mut normalized = data.to_vec();
    let mut changed = false;

    if profile == WeatherProfile::Clear {
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
    }

    if normalized.len() >= 72 {
        if let Some(ranges) = fo4_height_ranges(profile) {
            for (offset, replacement) in [
                (36, ranges.day_near),
                (44, ranges.night_near),
                (60, ranges.day_far),
                (68, ranges.night_far),
            ] {
                let value = f32::from_le_bytes(normalized[offset..offset + 4].try_into().unwrap());
                if value != replacement {
                    normalized[offset..offset + 4].copy_from_slice(&replacement.to_le_bytes());
                    changed = true;
                }
            }
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

    fn weather(editor_id: &str, dalc: Vec<u8>, fog: &[f32]) -> ParsedRecord {
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
    fn replaces_only_fo76_green_dalc_yplus_from_fo4_profile_donor() {
        let mut sunrise = vec![0; 32];
        sunrise[8..11].copy_from_slice(&[100, 100, 100]);
        let mut day = sunrise.clone();
        day[8..11].copy_from_slice(&[5, 188, 65]);
        let mut record = weather("NewWeatherClear_i", sunrise, &[6_000.0; 6]);
        record.subrecords.insert(2, subrecord("DALC", day));

        assert!(normalize_weather_record(&mut record));
        assert_eq!(&record.subrecords[1].data[8..11], &[100, 100, 100]);
        assert_eq!(&record.subrecords[2].data[8..11], &[82, 96, 111]);
        assert!(record.raw_payload.is_none());
        assert!(!normalize_weather_record(&mut record));
    }

    #[test]
    fn selects_dalc_donor_by_weather_profile() {
        for (profile, expected) in [
            (WeatherProfile::None, [82, 96, 111]),
            (WeatherProfile::Clear, [82, 96, 111]),
            (WeatherProfile::Fog, [69, 89, 100]),
            (WeatherProfile::Misty, [76, 91, 102]),
            (WeatherProfile::Rain, [34, 42, 46]),
            (WeatherProfile::Overcast, [35, 47, 54]),
            (WeatherProfile::Radstorm, [21, 21, 15]),
            (WeatherProfile::Dusty, [58, 73, 78]),
        ] {
            let mut dalc = vec![0; 32];
            dalc[8..11].copy_from_slice(&FO76_GREEN_YPLUS);
            let mut dalc = Bytes::from(dalc);

            assert!(normalize_dalc_yplus(&mut dalc, profile, 1));
            assert_eq!(&dalc[8..11], &expected);
        }
    }

    #[test]
    fn floors_only_clear_weather_near_and_far_distances() {
        let mut record = weather(
            "NewWeatherClear_i",
            vec![0; 29],
            &[500.0, 135_000.0, 500.0, 100_000.0, 1.0, 0.5],
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
            &[69_000.0, 200_000.0, 800.0, 650_000.0, 1.0, 0.5],
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
            &[450.0, 4_500.0, 450.0, 4_500.0, 1.0, 0.5],
        );

        assert!(!normalize_weather_record(&mut record));
        assert_eq!(
            fog_values(&record),
            vec![450.0, 4_500.0, 450.0, 4_500.0, 1.0, 0.5]
        );
    }

    #[test]
    fn ignores_short_dalc_and_fnam_payloads() {
        let mut record = weather("ATX_Weather_Clear", vec![0; 28], &[6_000.0; 6]);
        record.subrecords[2].data = Bytes::from(vec![0; 15]);

        assert!(!normalize_weather_record(&mut record));
        assert!(record.raw_payload.is_some());
    }

    #[test]
    fn replaces_height_ranges_with_fo4_profile_donors() {
        let source = [
            6_000.0, 600_000.0, 6_000.0, 600_000.0, 1.0, 0.7, 0.5, 0.85, 4_000.0, 8_000.0, 4_000.0,
            8_000.0, 0.7, 0.975, 4_000.0, 5_000.0, 4_000.0, 7_000.0,
        ];

        for (editor_id, expected) in [
            (
                "NewWeatherClear_i",
                [25_000.0, 10_000.0, 15_000.0, 10_000.0],
            ),
            (
                "Burn_DesertSandStormWeather",
                [25_000.0, 10_000.0, 15_000.0, 10_000.0],
            ),
            ("NewWeatherFoggy", [10_000.0, 10_000.0, 10_000.0, 10_000.0]),
            ("NewWeatherMisty", [10_000.0, 10_000.0, 15_000.0, 10_000.0]),
            ("NewWeatherRain", [15_000.0, 15_000.0, 10_000.0, 10_000.0]),
            (
                "NewWeatherOvercast",
                [15_000.0, 15_000.0, 10_000.0, 10_000.0],
            ),
            (
                "NewWeatherRadstorm",
                [10_000.0, 10_000.0, 10_000.0, 10_000.0],
            ),
        ] {
            let mut record = weather(editor_id, vec![0; 29], &source);

            assert!(normalize_weather_record(&mut record), "{editor_id}");
            let fog = fog_values(&record);
            assert_eq!([fog[9], fog[11], fog[15], fog[17]], expected, "{editor_id}");
            assert_eq!(
                [fog[8], fog[10], fog[12], fog[13], fog[14], fog[16]],
                [
                    source[8], source[10], source[12], source[13], source[14], source[16]
                ]
            );
        }
    }

    #[test]
    fn leaves_unclassified_weather_height_ranges_unchanged() {
        let source = [
            6_000.0, 600_000.0, 6_000.0, 600_000.0, 1.0, 0.7, 0.5, 0.85, 4_000.0, 8_000.0, 4_000.0,
            8_000.0, 0.7, 0.975, 4_000.0, 5_000.0, 4_000.0, 7_000.0,
        ];
        let mut record = weather("Babylon_WeatherSear", vec![0; 29], &source);

        assert!(!normalize_weather_record(&mut record));
        assert_eq!(fog_values(&record), source);
    }
}
