use super::*;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::FieldEntry;

fn key(interner: &StringInterner, local: u32) -> FormKey {
    FormKey {
        local,
        plugin: interner.intern("Skyrim.esm"),
    }
}

fn structured(interner: &StringInterner, fields: &[(&str, FieldValue)]) -> FieldValue {
    FieldValue::Struct(
        fields
            .iter()
            .map(|(name, value)| (interner.intern(name), value.clone()))
            .collect(),
    )
}

#[test]
fn shout_words_keep_order_distinct_spells_and_recovery() {
    let interner = StringInterner::new();
    let mut shout = Record::new(SigCode(*b"SHOU"), key(&interner, 0x100));
    for (index, recovery) in [15.0, 20.0, 45.0].into_iter().enumerate() {
        shout.fields.push(FieldEntry {
            sig: SubrecordSig(*b"SNAM"),
            value: structured(
                &interner,
                &[
                    (
                        "word",
                        FieldValue::FormKey(key(&interner, 0x300 + index as u32)),
                    ),
                    (
                        "spell",
                        FieldValue::FormKey(key(&interner, 0x200 + index as u32)),
                    ),
                    ("recovery_time", FieldValue::Float(recovery)),
                ],
            ),
        });
    }
    let words = shout_words(&shout, &interner).unwrap();
    assert_eq!(
        words
            .iter()
            .map(|word| (word.ordinal, word.spell.local, word.recovery))
            .collect::<Vec<_>>(),
        [(0, 0x200, 15.0), (1, 0x201, 20.0), (2, 0x202, 45.0)]
    );
    let FieldValue::Struct(pairs) = &mut shout.fields[1].value else {
        unreachable!()
    };
    pairs[2].1 = FieldValue::Float(f32::NAN);
    assert!(shout_words(&shout, &interner).is_err());
}

#[test]
fn resistance_abilities_and_touch_payloads_are_not_active_casts() {
    let interner = StringInterner::new();
    let mut spell = Record::new(SigCode(*b"SPEL"), key(&interner, 0x200));
    for (spell_type, cast_type, delivery, expected) in [
        (4, 0, 0, false),
        (0, 1, 1, false),
        (11, 2, 2, true),
        (0, 1, 2, true),
        (0, 2, 0, true),
    ] {
        spell.fields.clear();
        spell.fields.push(FieldEntry {
            sig: SubrecordSig(*b"SPIT"),
            value: structured(
                &interner,
                &[
                    ("type", FieldValue::Uint(spell_type)),
                    ("cast_type", FieldValue::Uint(cast_type)),
                    ("target_type", FieldValue::Uint(delivery)),
                ],
            ),
        });
        assert_eq!(is_active_spell(&spell, &interner), expected);
    }
}

#[test]
fn source_shout_timing_uses_winning_setting_value_and_provenance() {
    let interner = StringInterner::new();
    let mut setting = Record::new(SigCode(*b"GMST"), key(&interner, 0x14030));
    setting.fields.push(FieldEntry { sig: SubrecordSig(*b"EDID"), value: FieldValue::String(interner.intern("fShoutTime2")) });
    setting.fields.push(FieldEntry { sig: SubrecordSig(*b"DATA"), value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&1.75_f32.to_le_bytes())) });
    let records = HashMap::from([(setting.form_key, &setting)]);
    let timing = shout_timing_setting(&records, "fShoutTime2", &interner).unwrap();
    assert_eq!(timing.seconds, 1.75);
    assert_eq!(timing.source, "014030@Skyrim.esm");
    assert!(shout_timing_setting(&records, "fShoutTime1", &interner).is_err());
}

#[test]
fn binary_spit_keeps_source_voice_cast_duration_and_delivery() {
    let interner = StringInterner::new();
    let mut spell = Record::new(SigCode(*b"SPEL"), key(&interner, 0x252C2));
    let mut bytes = vec![0_u8; 36];
    bytes[8..12].copy_from_slice(&11_u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&2_u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&2_u32.to_le_bytes());
    bytes[24..28].copy_from_slice(&2.4_f32.to_le_bytes());
    spell.fields.push(FieldEntry { sig: SubrecordSig(*b"SPIT"), value: FieldValue::Bytes(bytes.into_iter().collect()) });
    assert!(is_active_spell(&spell, &interner));
    let data = spell_data(&spell, &interner).unwrap();
    assert_eq!(named(&data, "cast_duration", &interner).and_then(number), Some(2.4));
    let FieldValue::Bytes(bytes) = &mut spell.fields[0].value else { unreachable!() };
    bytes[20..24].copy_from_slice(&1_u32.to_le_bytes());
    assert!(!is_active_spell(&spell, &interner));
}

#[test]
fn npc_daily_power_policy_uses_spell_type_not_shout_header_flags() {
    let interner = StringInterner::new();
    let mut spell = Record::new(SigCode(*b"SPEL"), key(&interner, 0x200));
    for (spell_type, daily) in [(0, false), (2, true), (3, false), (11, false)] {
        spell.fields = vec![FieldEntry { sig: SubrecordSig(*b"SPIT"), value: structured(&interner, &[("type", FieldValue::Uint(spell_type))]) }].into();
        assert_eq!(is_daily_power(&spell, &interner), daily);
    }
    let mut shout = Record::new(SigCode(*b"SHOU"), key(&interner, 0x100));
    shout.flags = crate::record::RecordFlags::from_bits_retain(0x80);
    assert!(!is_daily_power(&shout, &interner));
}

#[test]
#[ignore = "requires the installed Skyrim source shout corpus"]
fn source_shout_corpus_keeps_word_spell_timing_and_highest_npc_rank() {
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_load_no_py,
    };
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("MODKIT_SKYRIM_SHOUT_INPUTS").unwrap()).unwrap(),
    )
    .unwrap();
    let interner = StringInterner::new();
    let schema = crate::schema::AuthoringSchema::for_game("skyrimse").unwrap();
    let mut handles = HashMap::new();
    let mut count = 0;
    let mut dragon = false;
    let mut centurion = false;
    let mut source_shouts = Vec::new();
    for input in manifest.as_array().unwrap() {
        let path = input["path"].as_str().unwrap();
        let handle = *handles.entry(path.to_string()).or_insert_with(|| {
            plugin_handle_load_no_py(path, Some("skyrimse"), None, None, true).unwrap()
        });
        let locator = input["key"].as_str().unwrap().replace(':', "@");
        let shout =
            crate::source_read::read_record_relayout(handle, &locator, &schema, &interner, None)
                .unwrap();
        let words = shout_words(&shout, &interner).unwrap();
        source_shouts.push(shout.form_key);
        let selected = words
            .iter()
            .rev()
            .find(|word| word.word_present)
            .unwrap_or(&words[0]);
        for word in &words {
            let word_key = word.spell.format(&interner);
            let word_path = std::path::Path::new(path)
                .parent()
                .unwrap()
                .join(word_key.split_once('@').unwrap().1)
                .to_string_lossy()
                .into_owned();
            let word_handle = *handles.entry(word_path.clone()).or_insert_with(|| {
                plugin_handle_load_no_py(&word_path, Some("skyrimse"), None, None, true).unwrap()
            });
            let spell = crate::source_read::read_record_relayout(
                word_handle,
                &word.spell.format(&interner).replace(':', "@"),
                &schema,
                &interner,
                None,
            )
            .unwrap();
            assert_eq!(spell.sig.0, *b"SPEL");
            assert!(
                is_active_spell(&spell, &interner),
                "{locator} {} is not an active spell",
                word.spell.format(&interner)
            );
        }
        if locator == "0252C0@Skyrim.esm" {
            assert_eq!(
                (selected.ordinal, selected.spell.local, selected.recovery),
                (2, 0x252C2, 5.0)
            );
            dragon = true;
        }
        if locator == "0A82BC@Skyrim.esm" {
            assert_eq!(
                (selected.ordinal, selected.spell.local, selected.recovery),
                (0, 0x90355, 10.0)
            );
            centurion = true;
        }
        count += 1;
    }
    let source_path = handles.keys().find(|path| path.to_lowercase().ends_with("skyrim.esm")).unwrap();
    let handle = handles[source_path];
    let action_records = ["05A305@Skyrim.esm", "01DA53@Skyrim.esm", "01402F@Skyrim.esm", "014030@Skyrim.esm"]
        .into_iter().map(|key| crate::source_read::read_record_relayout(handle, key, &schema, &interner, None).unwrap())
        .collect::<Vec<_>>();
    let records = action_records.iter().map(|record| (record.form_key, record)).collect::<HashMap<_, _>>();
    for shout in source_shouts {
        let action = actions::dragon_shout_action(shout, &records, &interner).unwrap();
        if shout == key(&interner, 0x4DBA5) {
            assert_eq!((action.idle.as_str(), action.event.as_str()), ("05A305@Skyrim.esm", "shoutStartFireball"));
        } else {
            assert_eq!((action.idle.as_str(), action.event.as_str()), ("01DA53@Skyrim.esm", "shoutStart"));
        }
    }
    assert_eq!(shout_timing_setting(&records, "fShoutTime1", &interner).unwrap().seconds, 0.2);
    assert_eq!(shout_timing_setting(&records, "fShoutTime2", &interner).unwrap().seconds, 0.9);
    for handle in handles.values() {
        assert!(plugin_handle_close_native(*handle));
    }
    assert_eq!(count, 48);
    assert!(dragon && centurion);
}
