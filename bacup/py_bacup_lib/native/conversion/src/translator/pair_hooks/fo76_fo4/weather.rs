use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::Record;
use crate::sym::{StringInterner, Sym};

const FO4_GDRY_NONE: u32 = 0x001B_40E8;

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

impl super::Fo76Fo4Hook {
    pub(super) fn translate_weather_volumetric_lighting(record: &mut Record) {
        if record.sig.0 != *b"WTHR" {
            return;
        }

        if record.fields.iter().any(|entry| entry.sig.0 == *b"WGDR") {
            record.fields.retain(|entry| entry.sig.0 != *b"HNAM");
            return;
        }

        if let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"HNAM")
        {
            entry.sig = SubrecordSig(*b"WGDR");
        }
    }
}

pub(crate) fn fo76_fo4_voli_gdry_substitution_mappings(
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
    } else if contains_any(editor_id, &["overcast", "storm", "snow"]) {
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
    use crate::record::{FieldEntry, FieldValue};
    use smallvec::SmallVec;

    #[test]
    fn weather_hnam_becomes_fo4_wgdr() {
        let mut interner = StringInterner::new();
        let mut weather = Record::new(
            SigCode(*b"WTHR"),
            FormKey::parse("4398AE@SeventySix.esm", &mut interner).unwrap(),
        );
        weather.fields.push(FieldEntry {
            sig: SubrecordSig(*b"HNAM"),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![0; 32])),
        });

        super::super::Fo76Fo4Hook::translate_weather_volumetric_lighting(&mut weather);

        assert_eq!(weather.fields[0].sig.0, *b"WGDR");
        assert_eq!(
            weather.fields[0].value,
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 32]))
        );
    }

    #[test]
    fn existing_wgdr_wins_over_fo76_hnam() {
        let mut interner = StringInterner::new();
        let mut weather = Record::new(
            SigCode(*b"WTHR"),
            FormKey::parse("4398AE@SeventySix.esm", &mut interner).unwrap(),
        );
        weather.fields.push(FieldEntry {
            sig: SubrecordSig(*b"WGDR"),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![1; 32])),
        });
        weather.fields.push(FieldEntry {
            sig: SubrecordSig(*b"HNAM"),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![2; 32])),
        });

        super::super::Fo76Fo4Hook::translate_weather_volumetric_lighting(&mut weather);

        assert_eq!(weather.fields.len(), 1);
        assert_eq!(weather.fields[0].sig.0, *b"WGDR");
    }

    #[test]
    fn donor_profiles_cover_source_weather_families() {
        for (editor_id, expected) in [
            ("GRay_OFF", FO4_GDRY_NONE),
            ("GRay_Clear_Day_i", 0x0021_6A92),
            ("GRay_Fog_Dawn_Swamp", 0x0021_8FA0),
            ("GRay_Misty_Night_MTR", 0x001C_C192),
            ("GRay_MistyRainy_Dusk", 0x0021_15D1),
            ("GRay_Storm_Overcast_Day", 0x001C_855D),
            ("GRay_Storm_Overcast_Dawn_Clear", 0x0021_6A93),
            ("GRay_Radstorm_Night", 0x0022_458F),
            ("Burn_GRay_DesertSand_Dawn_i", 0x001F_61AB),
            ("Unclassified_VOLI", FO4_GDRY_NONE),
        ] {
            assert_eq!(god_ray_donor(editor_id), expected, "{editor_id}");
        }
    }

    #[test]
    fn voli_mappings_target_fallout4_gdry_donors() {
        let mut interner = StringInterner::new();
        let source = FormKey::parse("4398AC@SeventySix.esm", &mut interner).unwrap();
        let entries = vec![
            (
                interner.intern("GRay_Clear_Day_i"),
                source,
                SigCode(*b"VOLI"),
            ),
            (
                interner.intern("NewWeatherClear_i"),
                FormKey::parse("4398AE@SeventySix.esm", &mut interner).unwrap(),
                SigCode(*b"WTHR"),
            ),
        ];

        let mappings = fo76_fo4_voli_gdry_substitution_mappings(&entries, &interner);

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].0, source);
        assert_eq!(mappings[0].1.local, 0x0021_6A92);
        assert_eq!(interner.resolve(mappings[0].1.plugin), Some("Fallout4.esm"));
    }
}
