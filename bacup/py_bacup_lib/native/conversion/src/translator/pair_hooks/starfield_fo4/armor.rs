use std::collections::HashMap;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

pub(crate) fn normalize(record: &mut Record, interner: &StringInterner) {
    if !matches!(record.sig.as_str(), "ARMO" | "ARMA") {
        return;
    }
    normalize_human_race(record, interner);
    if let Some(index) = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"BO64")
    {
        if let Some(source) = integer(&record.fields[index].value, 8) {
            let (target, unsupported, approximated) = project_slots(source);
            record.fields[index] = FieldEntry {
                sig: SubrecordSig(*b"BOD2"),
                value: FieldValue::Uint(u64::from(target)),
            };
            if unsupported != 0 {
                warn(
                    record,
                    interner,
                    &format!("starfield_armor_unmapped_biped_slots_{unsupported:016X}"),
                );
            }
            if approximated != 0 {
                warn(
                    record,
                    interner,
                    &format!(
                        "starfield_armor_projected_accessory_or_visual_slots_{approximated:016X}"
                    ),
                );
            }
        } else {
            warn(record, interner, "starfield_armor_invalid_bo64");
        }
    }
    if record.sig.0 != *b"ARMA" {
        return;
    }
    if let Some(index) = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"DNAM")
    {
        if matches!(&record.fields[index].value, FieldValue::Bytes(bytes) if bytes.len() == 12) {
            return;
        }
        if let Some((target, health_offset)) = project_dnam(&record.fields[index].value, interner) {
            record.fields[index].value = FieldValue::Bytes(target.into_iter().collect());
            if health_offset != 0.0 {
                warn(
                    record,
                    interner,
                    "starfield_armor_health_bar_offset_has_no_fo4_equivalent",
                );
            }
        } else {
            warn(record, interner, "starfield_armor_invalid_dnam");
        }
    }
}

pub(crate) fn post_normalize(record: &mut Record, interner: &StringInterner) {
    if record.sig.0 != *b"ARMA" {
        return;
    }
    let before = record.fields.len();
    record
        .fields
        .retain(|field| field.sig.0 != *b"MODL" || contains_form_key(&field.value));
    if before != record.fields.len() {
        warn(
            record,
            interner,
            "starfield_arma_additional_race_misclassified_as_model",
        );
    }
    for field in &mut record.fields {
        rewrite_first_person_paths(
            &mut field.value,
            matches!(&field.sig.0, b"MOD4" | b"MOD5"),
            interner,
        );
    }
}

pub(crate) fn first_person_model_path(path: &str) -> Option<String> {
    let path = super::super::model_paths::canonical_model_path(path)?;
    if !path.to_ascii_lowercase().ends_with(".nif") {
        return None;
    }
    if path.to_ascii_lowercase().starts_with("firstperson\\") {
        Some(path)
    } else {
        Some(format!("FirstPerson\\{path}"))
    }
}

fn rewrite_first_person_paths(
    value: &mut FieldValue,
    first_person: bool,
    interner: &StringInterner,
) {
    match value {
        FieldValue::String(sym) if first_person => {
            if let Some(path) = interner.resolve(*sym).and_then(first_person_model_path) {
                *sym = interner.intern(&path);
            }
        }
        FieldValue::Bytes(bytes) if first_person => {
            let nul = bytes.last() == Some(&0);
            if let Some(path) = std::str::from_utf8(bytes)
                .ok()
                .and_then(first_person_model_path)
            {
                bytes.clear();
                bytes.extend_from_slice(path.as_bytes());
                if nul {
                    bytes.push(0);
                }
            }
        }
        FieldValue::List(items) => {
            for item in items {
                rewrite_first_person_paths(item, first_person, interner);
            }
        }
        FieldValue::Struct(fields) => {
            for (key, value) in fields {
                let role = interner.resolve(*key);
                rewrite_first_person_paths(
                    value,
                    first_person || matches!(role, Some("MOD4" | "MOD5")),
                    interner,
                );
            }
        }
        _ => {}
    }
}

pub(crate) fn register_target_identities(record: &Record, mapper: &mut FormKeyMapper<'_>) {
    if !matches!(&record.sig.0, b"ARMO" | b"ARMA") {
        return;
    }
    for field in &record.fields {
        if field.sig.0 != *b"RNAM" {
            continue;
        }
        if let FieldValue::FormKey(key) = field.value
            && key.local == 0x013746
            && mapper
                .interner
                .resolve(key.plugin)
                .is_some_and(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
        {
            mapper.add_mapping(key, key);
        }
    }
}

pub(crate) fn biped_mask(record: &Record) -> u32 {
    record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"BOD2")
        .and_then(|field| integer(&field.value, 4))
        .unwrap_or(0) as u32
}

pub(crate) fn union_addon_slots(record: &mut Record, addon_masks: &HashMap<FormKey, u32>) -> bool {
    if record.sig.0 != *b"ARMO" {
        return false;
    }
    let old = biped_mask(record);
    let mut combined = old;
    for field in &record.fields {
        if field.sig.0 == *b"MODL" {
            union_references(&field.value, addon_masks, &mut combined);
        }
    }
    if combined == old {
        return false;
    }
    if let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"BOD2")
    {
        field.value = FieldValue::Uint(u64::from(combined));
    } else {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"BOD2"),
            value: FieldValue::Uint(u64::from(combined)),
        });
    }
    true
}

fn union_references(value: &FieldValue, masks: &HashMap<FormKey, u32>, combined: &mut u32) {
    match value {
        FieldValue::FormKey(key) => *combined |= masks.get(key).copied().unwrap_or(0),
        FieldValue::List(items) => {
            for item in items {
                union_references(item, masks, combined);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                union_references(value, masks, combined);
            }
        }
        _ => {}
    }
}

fn normalize_human_race(record: &mut Record, interner: &StringInterner) {
    for field in &mut record.fields {
        if field.sig.0 != *b"RNAM" {
            continue;
        }
        let FieldValue::FormKey(key) = &mut field.value else {
            continue;
        };
        if key.local == 0x00347D
            && interner
                .resolve(key.plugin)
                .is_some_and(|name| name.eq_ignore_ascii_case("Starfield.esm"))
        {
            *key = FormKey {
                local: 0x013746,
                plugin: interner.intern("Fallout4.esm"),
            };
        }
    }
}

fn integer(value: &FieldValue, width: usize) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) if *value >= 0 => Some(*value as u64),
        FieldValue::Bytes(bytes) if bytes.len() == width => {
            let mut raw = [0_u8; 8];
            raw[..width].copy_from_slice(bytes);
            Some(u64::from_le_bytes(raw))
        }
        _ => None,
    }
}

fn project_slots(source: u64) -> (u32, u64, u64) {
    let mut target = 0;
    let mut unsupported = 0;
    let mut approximated = 0;
    for bit in 0..64 {
        if source & (1_u64 << bit) == 0 {
            continue;
        }
        let (mapped, approximate): (u32, bool) = match bit {
            0 | 32 => (1 << 0, false),
            1 | 33 => (1 << 1, true),
            2 | 34 => (1 << 2, false),
            3 | 35 => ((1 << 3) | (1 << 6), false),
            4 | 36 | 9 | 41 => (1 << 16, true),
            // Human clothing uses the nominal "Backpack" slot for jackets/shirts.
            5 => (1 << 6, true),
            6 | 38 => ((1 << 4) | (1 << 5), false),
            7 | 39 => ((1 << 9) | (1 << 10), false),
            8 | 40 => ((1 << 7) | (1 << 8), false),
            10..=12 | 42..=44 => ((1 << 3) | (1 << 6), true),
            13..=15 | 45..=47 => (1 << 3, true),
            16 | 48 | 26 => (1 << 17, true),
            17 | 49 | 27 | 59 => (1 << 18, true),
            18 | 50 => (1 << 26, true),
            19 | 51 => (1 << 22, true),
            20 | 52 | 30 | 63 => (1 << 31, true),
            22..=25 | 54 => (1 << 25, true),
            29 => (1 << 29, false),
            37 | 53 => (1 << 24, true),
            _ => (0, false),
        };
        target |= mapped;
        if mapped == 0 {
            unsupported |= 1_u64 << bit;
        }
        if approximate {
            approximated |= 1_u64 << bit;
        }
    }
    (target, unsupported, approximated)
}

fn project_dnam(value: &FieldValue, interner: &StringInterner) -> Option<([u8; 12], f32)> {
    let mut target = [0_u8; 12];
    target[4] = 2;
    target[7] = 23;
    let health_offset;
    match value {
        FieldValue::Bytes(source) if source.len() == 13 => {
            target[0] = source[4];
            target[1] = source[5];
            target[6] = source[8];
            target[8..12].copy_from_slice(&source[0..4]);
            health_offset = f32::from_le_bytes(source[9..13].try_into().ok()?);
        }
        FieldValue::Bytes(source) if source.len() == 12 => return None,
        FieldValue::Struct(fields) => {
            let number = |name: &str| {
                fields.iter().find_map(|(key, value)| {
                    (interner.resolve(*key) == Some(name))
                        .then(|| match value {
                            FieldValue::Float(value) => Some(f64::from(*value)),
                            FieldValue::Uint(value) => Some(*value as f64),
                            FieldValue::Int(value) => Some(*value as f64),
                            _ => None,
                        })
                        .flatten()
                })
            };
            target[0] = number("male_priority")? as u8;
            target[1] = number("female_priority")? as u8;
            target[6] = number("detection_sound_value")? as u8;
            target[8..12].copy_from_slice(&(number("weapon_adjust")? as f32).to_le_bytes());
            health_offset = number("health_bar_offset").unwrap_or(0.0) as f32;
        }
        _ => return None,
    }
    Some((target, health_offset))
}

fn contains_form_key(value: &FieldValue) -> bool {
    match value {
        FieldValue::FormKey(_) => true,
        FieldValue::List(items) => items.iter().any(contains_form_key),
        FieldValue::Struct(fields) => fields.iter().any(|(_, value)| contains_form_key(value)),
        _ => false,
    }
}

fn warn(record: &mut Record, interner: &StringInterner, message: &str) {
    let warning = interner.intern(message);
    if !record.warnings.contains(&warning) {
        record.warnings.push(warning);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, ResolutionMode};
    use crate::ids::SigCode;
    use crate::translator::pair_hook::PairCtx;
    use crate::translator::{Game, TranslateResult, Translator};

    fn record(signature: &str, local: u32, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey {
                local,
                plugin: interner.intern("Starfield.esm"),
            },
        )
    }

    fn field(signature: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*signature),
            value,
        }
    }

    #[test]
    fn constellation_addon_retains_all_four_model_roles_and_additional_race() {
        let interner = StringInterner::new();
        let mut addon = record("ARMA", 0x1E2B15, &interner);
        addon.fields.push(field(
            b"BO64",
            FieldValue::Bytes((1_u64 << 35).to_le_bytes().into_iter().collect()),
        ));
        addon.fields.push(field(
            b"RNAM",
            FieldValue::FormKey(FormKey {
                local: 0x00347D,
                plugin: addon.form_key.plugin,
            }),
        ));
        addon.fields.push(field(
            b"DNAM",
            FieldValue::Bytes([0_u8; 13].into_iter().collect()),
        ));
        for (sig, path) in [
            (b"MOD2", "Torso_M.nif"),
            (b"MOD3", "Torso_F.nif"),
            (b"MOD4", "Torso_M_1st.nif"),
            (b"MOD5", "Torso_F_1st.nif"),
        ] {
            addon
                .fields
                .push(field(sig, FieldValue::String(interner.intern(path))));
        }
        let additional_race = field(
            b"MODL",
            FieldValue::FormKey(FormKey {
                local: 0x1EE4DA,
                plugin: addon.form_key.plugin,
            }),
        );
        addon.fields.push(additional_race.clone());
        normalize(&mut addon, &interner);
        post_normalize(&mut addon, &interner);
        assert_eq!(biped_mask(&addon), (1 << 3) | (1 << 6));
        assert!(addon.fields.contains(&additional_race));
        assert_eq!(
            addon
                .fields
                .iter()
                .filter(|f| matches!(&f.sig.0, b"MOD2" | b"MOD3" | b"MOD4" | b"MOD5"))
                .count(),
            4
        );
        assert_eq!(
            addon
                .fields
                .iter()
                .find(|f| f.sig.0 == *b"DNAM")
                .unwrap()
                .value,
            FieldValue::Bytes(smallvec::smallvec![0, 0, 0, 0, 2, 0, 0, 23, 0, 0, 0, 0])
        );
        assert_eq!(
            addon
                .fields
                .iter()
                .find(|f| f.sig.0 == *b"RNAM")
                .unwrap()
                .value,
            FieldValue::FormKey(FormKey {
                local: 0x013746,
                plugin: interner.intern("Fallout4.esm")
            })
        );
        let once = addon.fields.clone();
        let warnings = addon.warnings.clone();
        normalize(&mut addon, &interner);
        assert_eq!(addon.fields, once);
        assert_eq!(addon.warnings, warnings);
    }

    #[test]
    fn addon_dnam_projects_priorities_detection_and_weapon_adjust_at_target_offsets() {
        let mut source = [0_u8; 13];
        source[..4].copy_from_slice(&1.5_f32.to_le_bytes());
        source[4] = 7;
        source[5] = 9;
        source[8] = 11;
        source[9..].copy_from_slice(&0.25_f32.to_le_bytes());
        let (target, health) = project_dnam(
            &FieldValue::Bytes(source.into_iter().collect()),
            &StringInterner::new(),
        )
        .unwrap();
        assert_eq!(target[..8], [7, 9, 0, 0, 2, 0, 11, 23]);
        assert_eq!(f32::from_le_bytes(target[8..].try_into().unwrap()), 1.5);
        assert_eq!(health, 0.25);
        assert!(
            project_dnam(
                &FieldValue::Bytes(smallvec::smallvec![0; 11]),
                &StringInterner::new()
            )
            .is_none()
        );
    }

    #[test]
    fn composite_suit_unions_all_addons_without_changing_order_or_references() {
        let interner = StringInterner::new();
        let mut armor = record("ARMO", 0x1E2B18, &interner);
        armor
            .fields
            .push(field(b"BO64", FieldValue::Uint(1_u64 << 35)));
        let source_slots = [
            (0x1E2B15, 35),
            (0x05954D, 38),
            (0x313B83, 39),
            (0x313B87, 44),
            (0x313B86, 43),
            (0x313B85, 40),
            (0x313B84, 42),
            (0x3A8DE7, 54),
        ];
        let mut masks = HashMap::new();
        for (id, slot) in source_slots {
            let key = FormKey {
                local: id,
                plugin: armor.form_key.plugin,
            };
            armor.fields.push(field(b"INDX", FieldValue::Uint(0)));
            armor.fields.push(field(b"MODL", FieldValue::FormKey(key)));
            masks.insert(key, project_slots(1_u64 << slot).0);
        }
        let references = armor.fields[1..].to_vec();
        normalize(&mut armor, &interner);
        assert!(union_addon_slots(&mut armor, &masks));
        assert_eq!(
            biped_mask(&armor),
            (1 << 3)
                | (1 << 4)
                | (1 << 5)
                | (1 << 6)
                | (1 << 7)
                | (1 << 8)
                | (1 << 9)
                | (1 << 10)
                | (1 << 25)
        );
        assert_eq!(&armor.fields[1..], references);
        assert!(!union_addon_slots(&mut armor, &masks));
    }

    #[test]
    fn accessory_projection_preserves_pipboy_and_reports_unknown_slots() {
        let interner = StringInterner::new();
        let mut armor = record("ARMO", 1, &interner);
        armor.fields.push(field(
            b"BO64",
            FieldValue::Uint((1_u64 << 37) | (1_u64 << 54) | (1_u64 << 60)),
        ));
        normalize(&mut armor, &interner);
        assert_eq!(biped_mask(&armor), (1 << 24) | (1 << 25));
        assert_eq!(biped_mask(&armor) & (1 << 30), 0);
        assert!(armor.warnings.iter().any(|w| {
            interner
                .resolve(*w)
                .unwrap()
                .contains("unmapped_biped_slots_1000000000000000")
        }));
    }

    #[test]
    fn nonhuman_races_and_inventory_models_are_not_replaced() {
        let interner = StringInterner::new();
        let mut armor = record("ARMO", 1, &interner);
        armor.fields.push(field(
            b"RNAM",
            FieldValue::FormKey(FormKey {
                local: 0x2BBC09,
                plugin: armor.form_key.plugin,
            }),
        ));
        armor.fields.push(field(
            b"MOD2",
            FieldValue::String(interner.intern("SetDressing\\SpaceSuitContainer01.nif")),
        ));
        let original = armor.fields.clone();
        normalize(&mut armor, &interner);
        assert_eq!(armor.fields, original);
    }

    fn translate_mapped(
        mut source: Record,
        translator: &Translator,
        mapper: &mut FormKeyMapper<'_>,
        interner: &StringInterner,
    ) -> Record {
        let mut ctx = PairCtx::with_source(interner, "Starfield.esm", &[]);
        translator.pre_translate(&mut ctx, &mut source).unwrap();
        let TranslateResult::Translated(mut target) =
            translator.translate_ignoring_skip(&source, interner, source.sig.as_str())
        else {
            panic!("armor contract could not translate");
        };
        register_target_identities(&target, mapper);
        mapper.rewrite_record(&mut target).unwrap();
        translator.post_translate(&mut ctx, &mut target).unwrap();
        target
    }

    fn strict_mapper(interner: &StringInterner) -> FormKeyMapper<'_> {
        FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                source_plugin_name: "Starfield.esm".into(),
                output_plugin_name: "B21_StarfieldArmorTest.esp".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                resolution_mode: ResolutionMode::Strict,
                ..MapperOptions::default()
            },
            interner,
        )
    }

    #[test]
    fn embedded_translator_preserves_addon_models_data_and_typed_additional_races() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let mut mapper = strict_mapper(&interner);
        let mut addon = record("ARMA", 0x1E2B15, &interner);
        addon.fields.push(field(
            b"BO64",
            FieldValue::Bytes((1_u64 << 35).to_le_bytes().into_iter().collect()),
        ));
        addon.fields.push(field(
            b"RNAM",
            FieldValue::FormKey(FormKey {
                local: 0x00347D,
                plugin: addon.form_key.plugin,
            }),
        ));
        addon.fields.push(field(
            b"DNAM",
            FieldValue::Bytes([0_u8; 13].into_iter().collect()),
        ));
        for (sig, path) in [
            (b"MOD2", "Clothes\\Constellation\\Torso_M.nif"),
            (b"MOD3", "Clothes\\Constellation\\Torso_F.nif"),
            (b"MOD4", "Clothes\\Constellation\\Torso_M_1st.nif"),
            (b"MOD5", "Clothes\\Constellation\\Torso_F_1st.nif"),
        ] {
            addon
                .fields
                .push(field(sig, FieldValue::String(interner.intern(path))));
        }
        let source_race = FormKey {
            local: 0x1EE4DA,
            plugin: addon.form_key.plugin,
        };
        let mapped_race = FormKey {
            local: 0x800,
            plugin: interner.intern("B21_StarfieldArmorTest.esp"),
        };
        mapper.add_mapping(source_race, mapped_race);
        addon
            .fields
            .push(field(b"MODL", FieldValue::FormKey(source_race)));
        let translated = translate_mapped(addon, &translator, &mut mapper, &interner);
        assert_eq!(biped_mask(&translated), (1 << 3) | (1 << 6));
        assert_eq!(
            translated
                .fields
                .iter()
                .find(|f| f.sig.0 == *b"DNAM")
                .unwrap()
                .value,
            FieldValue::Bytes(smallvec::smallvec![0, 0, 0, 0, 2, 0, 0, 23, 0, 0, 0, 0])
        );
        assert_eq!(
            translated
                .fields
                .iter()
                .find(|f| f.sig.0 == *b"MODL")
                .unwrap()
                .value,
            FieldValue::FormKey(mapped_race)
        );
        assert_eq!(
            translated
                .fields
                .iter()
                .find(|f| f.sig.0 == *b"RNAM")
                .unwrap()
                .value,
            FieldValue::FormKey(FormKey {
                local: 0x013746,
                plugin: interner.intern("Fallout4.esm")
            })
        );
        let paths = translated
            .fields
            .iter()
            .filter_map(|f| match &f.value {
                FieldValue::String(path) => {
                    Some((f.sig.as_str(), interner.resolve(*path).unwrap()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                ("MOD2", "Clothes\\Constellation\\Torso_M.nif"),
                ("MOD3", "Clothes\\Constellation\\Torso_F.nif"),
                (
                    "MOD4",
                    "FirstPerson\\Clothes\\Constellation\\Torso_M_1st.nif"
                ),
                (
                    "MOD5",
                    "FirstPerson\\Clothes\\Constellation\\Torso_F_1st.nif"
                )
            ]
        );
    }

    #[test]
    fn embedded_translator_preserves_composite_armor_addon_pairs_and_compatible_data() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let mut mapper = strict_mapper(&interner);
        let mut armor = record("ARMO", 0x1E2B18, &interner);
        armor
            .fields
            .push(field(b"BO64", FieldValue::Uint(1_u64 << 35)));
        armor.fields.push(field(
            b"RNAM",
            FieldValue::FormKey(FormKey {
                local: 0x00347D,
                plugin: armor.form_key.plugin,
            }),
        ));
        let mut expected = Vec::new();
        let mut masks = HashMap::new();
        for (id, slot) in [
            (0x1E2B15, 35),
            (0x05954D, 38),
            (0x313B83, 39),
            (0x313B87, 44),
            (0x313B86, 43),
            (0x313B85, 40),
            (0x313B84, 42),
            (0x3A8DE7, 54),
        ] {
            let source = FormKey {
                local: id,
                plugin: armor.form_key.plugin,
            };
            let target = FormKey {
                local: id,
                plugin: interner.intern("B21_StarfieldArmorTest.esp"),
            };
            mapper.add_mapping(source, target);
            armor.fields.push(field(b"INDX", FieldValue::Uint(0)));
            armor
                .fields
                .push(field(b"MODL", FieldValue::FormKey(source)));
            expected.push(field(b"INDX", FieldValue::Uint(0)));
            expected.push(field(b"MODL", FieldValue::FormKey(target)));
            masks.insert(target, project_slots(1_u64 << slot).0);
        }
        let data = field(
            b"DATA",
            FieldValue::Bytes(smallvec::smallvec![
                7, 33, 0, 0, 205, 204, 4, 65, 0, 0, 0, 0
            ]),
        );
        let rating = field(
            b"FNAM",
            FieldValue::Bytes(smallvec::smallvec![58, 0, 0, 0, 0, 0, 0, 0]),
        );
        armor.fields.push(data.clone());
        armor.fields.push(rating.clone());
        let mut translated = translate_mapped(armor, &translator, &mut mapper, &interner);
        assert!(union_addon_slots(&mut translated, &masks));
        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|f| matches!(&f.sig.0, b"INDX" | b"MODL"))
                .cloned()
                .collect::<Vec<_>>(),
            expected
        );
        assert!(translated.fields.contains(&data));
        assert!(translated.fields.contains(&rating));
        assert_eq!(biped_mask(&translated), 0x020007F8);
    }

    #[test]
    fn first_person_variant_rewrites_only_first_person_roles_in_nested_model_groups() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let mut mapper = strict_mapper(&interner);
        let mut addon = record("ARMA", 0x3A8DE7, &interner);
        let path = "Clothes\\Constellation\\Watch_M.nif";
        addon.fields.push(field(
            b"MOD2",
            FieldValue::List(vec![FieldValue::Struct(vec![
                (
                    interner.intern("MOD2"),
                    FieldValue::String(interner.intern(path)),
                ),
                (
                    interner.intern("MOD3"),
                    FieldValue::String(interner.intern("Watch_F.nif")),
                ),
                (
                    interner.intern("MOD4"),
                    FieldValue::String(interner.intern(path)),
                ),
                (
                    interner.intern("MOD5"),
                    FieldValue::String(interner.intern("Watch_F.nif")),
                ),
            ])]),
        ));
        let translated = translate_mapped(addon, &translator, &mut mapper, &interner);
        let FieldValue::List(rows) = &translated.fields[0].value else {
            panic!("model group missing");
        };
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("model row missing");
        };
        let paths = fields
            .iter()
            .map(|(role, value)| {
                let FieldValue::String(path) = value else {
                    panic!("model path missing");
                };
                (
                    interner.resolve(*role).unwrap(),
                    interner.resolve(*path).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                ("MOD2", path),
                ("MOD3", "Watch_F.nif"),
                ("MOD4", "FirstPerson\\Clothes\\Constellation\\Watch_M.nif"),
                ("MOD5", "FirstPerson\\Watch_F.nif")
            ]
        );
        assert_eq!(
            first_person_model_path("Data/Meshes/Starfield/Clothes/Gloves.nif"),
            Some("FirstPerson\\Clothes\\Gloves.nif".into())
        );
        assert_eq!(
            first_person_model_path("FirstPerson\\Clothes\\Gloves.nif"),
            Some("FirstPerson\\Clothes\\Gloves.nif".into())
        );
    }
}
