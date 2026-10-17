fn contact_translate(record: &mut Record, interner: &StringInterner) {
    let translator = Translator::new(Game::SkyrimSe, Game::Fo4).unwrap();
    let mut ctx = PairCtx::new(interner);
    translator.pre_translate(&mut ctx, record).unwrap();
    let once = record.fields.clone();
    translator.pre_translate(&mut ctx, record).unwrap();
    assert_eq!(record.fields, once, "contact lowering must be idempotent");
    let TranslateResult::Translated(mut translated) = translator.translate(record, interner) else {
        panic!("contact magic must translate");
    };
    translator
        .post_translate(&mut ctx, &mut translated)
        .unwrap();
    translator
        .run_target_hook(&mut TargetCtx { interner }, &mut translated)
        .unwrap();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let TargetRecordNormalization::Keep(normalized) =
        TargetRecordNormalizer::target_only_with_interner(&schema, interner).normalize(translated)
    else {
        panic!("FO4 must retain contact magic");
    };
    *record = normalized;
}

fn contact_form_keys(value: &FieldValue, keys: &mut std::collections::HashSet<FormKey>) {
    match value {
        FieldValue::FormKey(key) => {
            keys.insert(*key);
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .for_each(|(_, value)| contact_form_keys(value, keys)),
        FieldValue::List(values) => values
            .iter()
            .for_each(|value| contact_form_keys(value, keys)),
        _ => {}
    }
}

#[test]
fn contact_magic_does_not_reinterpret_lowered_null_actor_values_as_source_enums() {
    let interner = StringInterner::new();
    let mut record = jzargo_record(&interner, "MGEF", 0x153D3, "PerkImpactStaggerPushFFAimed");
    jzargo_push(
        &mut record,
        *b"DATA",
        jzargo_object(
            &interner,
            [
                ("Archtype", FieldValue::Uint(33)),
                ("CastingType", FieldValue::Uint(1)),
                ("Delivery", FieldValue::Uint(2)),
                ("MagicSkill", FieldValue::Uint(22)),
                ("ActorValue", FieldValue::Int(-1)),
                ("SecondActorValue", FieldValue::Int(-1)),
                ("ResistValue", FieldValue::Int(-1)),
            ],
        ),
    );
    let receipt = lower_magic_record_for_fo4(&mut record, &interner)
        .unwrap()
        .unwrap();
    assert_eq!(receipt.component, MagicComponent::NativeEffect);
    let once = record.fields.clone();
    assert!(
        lower_magic_record_for_fo4(&mut record, &interner)
            .unwrap()
            .is_none()
    );
    assert_eq!(record.fields, once);
    assert_eq!(
        jzargo_named(&record, *b"DATA", "ActorValue", &interner),
        &FieldValue::Uint(0)
    );
    assert_eq!(
        jzargo_named(&record, *b"DATA", "ActorValue1", &interner),
        &FieldValue::Uint(0)
    );
    contact_translate(&mut record, &interner);
}

#[test]
fn contact_magic_preserves_bow_stagger_actor_value_and_native_master_index() {
    let interner = StringInterner::new();
    let expected = FieldValue::FormKey(FormKey {
        local: 0x314,
        plugin: interner.intern("Fallout4.esm"),
    });
    for value in [
        FieldValue::Uint(87),
        FieldValue::String(interner.intern("BowStaggerBonus")),
    ] {
        assert_eq!(
            skyrim_actor_value_for_fo4(&value, &interner),
            Some(expected.clone())
        );
    }
    assert!(skyrim_actor_value_for_fo4(&FieldValue::Uint(135), &interner).is_none());
    let mut record = jzargo_record(&interner, "MGEF", 0x6C0F3, "AbFortifyBowStagger");
    jzargo_push(
        &mut record,
        *b"DATA",
        jzargo_object(
            &interner,
            [
                ("Archtype", FieldValue::Uint(34)),
                ("CastingType", FieldValue::Uint(0)),
                ("Delivery", FieldValue::Uint(0)),
                ("ActorValue", FieldValue::Uint(87)),
                ("BaseCost", FieldValue::Float(5.0)),
            ],
        ),
    );
    contact_translate(&mut record, &interner);
    assert_eq!(
        jzargo_named(&record, *b"DATA", "ActorValue", &interner),
        &expected
    );
    const OUTPUT: &str = "B21_BowStaggerTest.esp";
    record.form_key = FormKey {
        local: 0x800,
        plugin: interner.intern(OUTPUT),
    };
    let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    for master in ["Preceding.esm", "Fallout4.esm"] {
        plugin_handle_add_master_native(handle, master, None).unwrap();
    }
    crate::target_write::add_record_native(
        handle,
        record,
        &AuthoringSchema::for_game("fo4").unwrap(),
        &interner,
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        let parsed = jzargo_find_parsed(&slot.parsed.root_items, "MGEF", 0x800).unwrap();
        let data = jzargo_parsed_subrecord(parsed, "DATA");
        assert_eq!(data.len(), 152);
        assert_eq!(jzargo_parsed_u32(data, 68), 0x01000314);
        assert_eq!(jzargo_parsed_u32(data, 4), 5.0_f32.to_bits());
    }
    assert!(plugin_handle_close_native(reopened));
}

#[test]
fn contact_magic_keeps_multiple_effects_and_condition_groups() {
    let interner = StringInterner::new();
    let mut record = jzargo_record(&interner, "SPEL", 0x800, "B21_ContactSpell");
    jzargo_push(
        &mut record,
        *b"SPIT",
        jzargo_object(
            &interner,
            [
                ("BaseCost", FieldValue::Uint(73)),
                ("Flags", FieldValue::Uint(0x500001)),
                ("Type", FieldValue::String(interner.intern("Disease"))),
                ("CastType", FieldValue::Uint(0)),
                ("Delivery", FieldValue::Uint(1)),
                ("ChargeTime", FieldValue::Float(1.25)),
                ("Range", FieldValue::Float(39.0)),
            ],
        ),
    );
    let source_plugin = record.form_key.plugin;
    for (index, magnitude) in [15.0_f32, 25.0, 3.0].into_iter().enumerate() {
        jzargo_push(
            &mut record,
            *b"EFID",
            FieldValue::FormKey(FormKey {
                local: 0x900 + index as u32,
                plugin: source_plugin,
            }),
        );
        let mut efit = magnitude.to_le_bytes().to_vec();
        efit.extend_from_slice(&7_u32.to_le_bytes());
        efit.extend_from_slice(&12_u32.to_le_bytes());
        jzargo_push(
            &mut record,
            *b"EFIT",
            FieldValue::Bytes(efit.into_iter().collect()),
        );
        let mut ctda = vec![0_u8; 32];
        ctda[4..8].copy_from_slice(&(10.0_f32 + index as f32).to_le_bytes());
        ctda[8..10].copy_from_slice(&77_u16.to_le_bytes());
        jzargo_push(
            &mut record,
            *b"CTDA",
            FieldValue::Bytes(ctda.into_iter().collect()),
        );
    }
    let expected = record.fields[2..].to_vec();
    let MagicSupport::RequiresLinkedRecords(requirements) =
        classify_magic_component(&record, &interner)
    else {
        panic!("contact dependencies")
    };
    assert_eq!(requirements.linked_records.len(), 3);
    contact_translate(&mut record, &interner);
    assert_eq!(record.fields[2..], expected);
    assert_eq!(
        jzargo_named(&record, *b"SPIT", "Type", &interner),
        &FieldValue::Uint(1)
    );
    assert_eq!(
        jzargo_named(&record, *b"SPIT", "CastType", &interner),
        &FieldValue::Uint(0)
    );
    assert_eq!(
        jzargo_named(&record, *b"SPIT", "BaseCost", &interner),
        &FieldValue::Uint(73)
    );
    assert_eq!(
        jzargo_named(&record, *b"SPIT", "Flags", &interner),
        &FieldValue::Uint(0x500001)
    );
    record.fields.remove(3);
    assert!(matches!(
        classify_magic_component(&record, &interner),
        MagicSupport::Unsupported {
            reason: MagicUnsupportedReason::EffectCardinality
        }
    ));
    let original = record.fields.clone();
    assert!(matches!(
        lower_magic_record_for_fo4(&mut record, &interner),
        Err(MagicContractError::UnsupportedNativeMagic {
            reason: MagicUnsupportedReason::EffectCardinality,
            ..
        })
    ));
    assert_eq!(
        record.fields, original,
        "unsupported contact magic must not become a self-targeting fallback"
    );
}

#[test]
fn contact_magic_decodes_source_before_relayout_and_keeps_dual_actor_values() {
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let mut data = vec![0_u8; 152];
    for (offset, value) in [
        (0, 0x10400007_u32),
        (12, 20),
        (16, 43),
        (24, 0x0000_1234),
        (64, 5),
        (68, 24),
        (80, 1),
        (84, 1),
        (88, 26),
    ] {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    data[60..64].copy_from_slice(&2.0_f32.to_le_bytes());
    let parsed = ParsedRecord {
        signature: "MGEF".into(),
        form_id: 0x01000800,
        flags: 0,
        version_control: 0,
        form_version: Some(44),
        version2: Some(0),
        subrecords: vec![parsed_subrecord("DATA", &data)],
        raw_payload: None,
        parse_error: None,
    };
    let mut record = crate::source_read::decode_record_from_parsed_relayout(
        &parsed,
        &FormKey::parse("000800@Dawnguard.esm", &interner).unwrap(),
        &source_schema,
        &["Skyrim.esm".to_string()],
        "Dawnguard.esm",
        None,
        false,
        &interner,
        Some(&crate::struct_relayout::StructRelayoutCtx {
            target_schema: &target_schema,
            target_form_version: 131,
            legacy_bptd_only: false,
        }),
    )
    .unwrap();
    assert!(matches!(
        jzargo_field(&record, *b"DATA"),
        FieldValue::Struct(_)
    ));
    jzargo_assert_form(
        jzargo_named(&record, *b"DATA", "casting_light", &interner),
        0x1234,
        "Skyrim.esm",
        &interner,
    );
    contact_translate(&mut record, &interner);
    assert_eq!(
        jzargo_named(&record, *b"DATA", "Archetype", &interner),
        &FieldValue::Uint(5)
    );
    assert_eq!(
        jzargo_named(&record, *b"DATA", "Flags", &interner),
        &FieldValue::Uint(0x10400007)
    );
    jzargo_assert_form(
        jzargo_named(&record, *b"DATA", "ActorValue", &interner),
        0x2D4,
        "Fallout4.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&record, *b"DATA", "AssociatedSkill", &interner),
        0x384,
        "Fallout4.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&record, *b"DATA", "ActorValue1", &interner),
        0x2D5,
        "Fallout4.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&record, *b"DATA", "ResistValue", &interner),
        0x2E7,
        "Fallout4.esm",
        &interner,
    );
    assert_eq!(
        jzargo_named(&record, *b"DATA", "SecondAVWeight", &interner),
        &FieldValue::Float(2.0)
    );
}

#[test]
fn contact_magic_disease_skills_grant_shared_perk() {
    let interner = StringInterner::new();
    let mut record = jzargo_record(&interner, "MGEF", 0x801, "B21_ContactDisease");
    jzargo_push(
        &mut record,
        *b"DATA",
        jzargo_object(
            &interner,
            [
                ("Delivery", FieldValue::Uint(1)),
                ("Archtype", FieldValue::Uint(5)),
                ("ActorValue", FieldValue::Int(96)),
                ("SecondActorValue", FieldValue::Int(97)),
                ("ResistValue", FieldValue::Int(-1)),
                ("SecondAVWeight", FieldValue::Float(1.0)),
                ("Flags", FieldValue::Uint(0x400A06)),
            ],
        ),
    );
    contact_translate(&mut record, &interner);
    assert_eq!(
        jzargo_named(&record, *b"DATA", "CastingType", &interner),
        &FieldValue::Uint(0)
    );
    jzargo_assert_form(
        jzargo_named(&record, *b"DATA", "ActorValue", &interner),
        1,
        "B21_SkyrimSkillRuntime.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&record, *b"DATA", "ActorValue1", &interner),
        2,
        "B21_SkyrimSkillRuntime.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&record, *b"DATA", "PerkToApply", &interner),
        5,
        "B21_SkyrimSkillRuntime.esm",
        &interner,
    );
}

#[test]
#[ignore = "requires the source Skyrim creature spell corpus and installed source plugins"]
fn source_contact_magic_corpus_preserves_native_effect_payloads() {
    let path = std::env::var("MODKIT_SKYRIM_ATTACK_MAGIC_INPUTS").expect("source corpus manifest");
    audit_source_magic_corpus(path, (43, 31));
}

#[test]
#[ignore = "requires the full source Skyrim creature ability corpus and installed source plugins"]
fn source_creature_ability_magic_corpus_preserves_native_effect_payloads() {
    let path = std::env::var("MODKIT_SKYRIM_ABILITY_MAGIC_INPUTS")
        .expect("source ability corpus manifest");
    audit_source_magic_corpus(path, (355, 264));
}

fn audit_source_magic_corpus(path: String, expected: (usize, usize)) {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let relayout = crate::struct_relayout::StructRelayoutCtx {
        target_schema: &target_schema,
        target_form_version: 131,
        legacy_bptd_only: false,
    };
    let mut handles = std::collections::HashMap::new();
    let mut records = Vec::new();
    for input in manifest.as_array().unwrap() {
        let path = input["path"].as_str().unwrap();
        let handle = *handles.entry(path.to_string()).or_insert_with(|| {
            plugin_handle_load_no_py(path, Some("skyrimse"), None, None, true).unwrap()
        });
        let key = input["key"].as_str().unwrap().replace(':', "@");
        let record = crate::source_read::read_record_relayout(
            handle,
            &key,
            &source_schema,
            &interner,
            Some(&relayout),
        )
        .unwrap();
        assert!(
            classify_magic_component(&record, &interner)
                .plan()
                .is_some(),
            "{} {}: {:?}",
            key,
            record.sig.as_str(),
            classify_magic_component(&record, &interner)
        );
        records.push(record);
    }
    for handle in handles.values() {
        assert!(plugin_handle_close_native(*handle));
    }
    let effects: std::collections::HashSet<_> = records
        .iter()
        .filter(|record| record.sig.0 == *b"MGEF")
        .map(|record| record.form_key)
        .collect();
    assert_eq!(effects.len(), expected.1);
    assert_eq!(records.len() - effects.len(), expected.0);
    let signatures = records
        .iter()
        .map(|record| (record.form_key, record.sig))
        .collect::<std::collections::HashMap<_, _>>();
    for record in &records {
        if let MagicSupport::RequiresLinkedRecords(requirements) =
            classify_magic_component(record, &interner)
        {
            assert!(
                requirements
                    .linked_records
                    .iter()
                    .all(|requirement| signatures.get(&requirement.form_key)
                        == Some(&requirement.signature)),
                "every source spell's effects must be supported"
            );
        }
    }
    const OUTPUT: &str = "B21_ContactMagicCorpus.esp";
    let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    for master in [
        "Fallout4.esm",
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
        "B21_SkyrimMagicRuntime.esm",
        "B21_SkyrimSkillRuntime.esm",
    ] {
        plugin_handle_add_master_native(handle, master, None).unwrap();
    }
    let mut expectations = Vec::new();
    for (index, mut record) in records.into_iter().enumerate() {
        let original = record.clone();
        contact_translate(&mut record, &interner);
        let mut source_refs = std::collections::HashSet::new();
        let mut target_refs = std::collections::HashSet::new();
        for field in &original.fields {
            contact_form_keys(&field.value, &mut source_refs);
        }
        for field in &record.fields {
            contact_form_keys(&field.value, &mut target_refs);
        }
        assert!(
            source_refs.is_subset(&target_refs),
            "{} dropped source magic assets or effects: {:?}",
            index,
            source_refs.difference(&target_refs).collect::<Vec<_>>()
        );
        for signature in [*b"EFIT", *b"CTDA", *b"CIS1", *b"CIS2", *b"SNDD"] {
            let values = |record: &Record| {
                record
                    .fields
                    .iter()
                    .filter(|field| field.sig.0 == signature)
                    .map(|field| field.value.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                values(&record),
                values(&original),
                "{} must preserve {:?}",
                index,
                signature
            );
        }
        record.form_key = FormKey {
            local: 0x800 + index as u32,
            plugin: interner.intern(OUTPUT),
        };
        let signature = if record.sig.0 == *b"SPEL" {
            *b"SPIT"
        } else {
            *b"DATA"
        };
        let scalar_fields = if signature == *b"SPIT" {
            vec![
                (0, "BaseCost"),
                (4, "Flags"),
                (8, "Type"),
                (12, "ChargeTime"),
                (16, "CastType"),
                (20, "TargetType"),
                (24, "CastDuration"),
                (28, "Range"),
            ]
        } else {
            vec![
                (0, "Flags"),
                (4, "BaseCost"),
                (28, "TaperWeight"),
                (40, "MinimumSkillLevel"),
                (44, "SpellmakingArea"),
                (48, "SpellmakingCastingTime"),
                (52, "TaperCurve"),
                (56, "TaperDuration"),
                (60, "SecondAVWeight"),
                (64, "Archetype"),
                (80, "CastingType"),
                (84, "Delivery"),
                (104, "SkillUsageMultiplier"),
                (112, "DualCastingScale"),
                (140, "CastingSoundLevel"),
                (144, "ScriptEffectAIScore"),
                (148, "ScriptEffectAIDelayTime"),
            ]
        };
        let mut values = scalar_fields
            .into_iter()
            .map(|(offset, name)| {
                let value = match jzargo_named(&record, signature, name, &interner) {
                    FieldValue::Uint(value) => *value as u32,
                    FieldValue::Int(value) => *value as u32,
                    FieldValue::Float(value) => value.to_bits(),
                    FieldValue::Bytes(bytes) if bytes.len() == 4 => {
                        u32::from_le_bytes(bytes.as_slice().try_into().unwrap())
                    }
                    _ => panic!("native scalar {name}"),
                };
                (offset, value)
            })
            .collect::<Vec<_>>();
        if signature == *b"DATA" {
            let expected_skill =
                match jzargo_named(&record, signature, "AssociatedSkill", &interner) {
                    FieldValue::FormKey(form) => {
                        assert_eq!(interner.resolve(form.plugin), Some("Fallout4.esm"));
                        form.local
                    }
                    FieldValue::Uint(0) => 0,
                    FieldValue::Bytes(bytes) if bytes.as_slice() == [0; 4] => 0,
                    _ => panic!("native associated skill"),
                };
            values.push((12, expected_skill));
        }
        let effect_rows = record
            .fields
            .iter()
            .filter_map(|field| {
                if matches!(&field.sig.0, b"EFIT" | b"CTDA" | b"CIS1" | b"CIS2") {
                    let bytes = match &field.value {
                        FieldValue::Bytes(bytes) => Some(bytes.to_vec()),
                        FieldValue::String(value) => {
                            let mut bytes = interner.resolve(*value).unwrap().as_bytes().to_vec();
                            bytes.push(0);
                            Some(bytes)
                        }
                        FieldValue::Struct(_) if field.sig.0 == *b"CTDA" => None,
                        _ => panic!("unexpected effect group representation"),
                    };
                    return Some((field.sig.as_str().to_string(), bytes));
                }
                None
            })
            .collect::<Vec<_>>();
        expectations.push((
            record.sig.as_str().to_string(),
            record.form_key.local,
            signature,
            values,
            effect_rows,
        ));
        crate::target_write::add_record_native(handle, record, &target_schema, &interner).unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        for (signature, local, subrecord, values, effect_rows) in expectations {
            let parsed = jzargo_find_parsed(&slot.parsed.root_items, &signature, local).unwrap();
            let data = jzargo_parsed_subrecord(parsed, std::str::from_utf8(&subrecord).unwrap());
            assert_eq!(data.len(), if signature == "SPEL" { 36 } else { 152 });
            for (offset, value) in values {
                assert_eq!(
                    jzargo_parsed_u32(data, offset),
                    value,
                    "{signature}:{local:X}+{offset}"
                );
            }
            let saved_rows = parsed
                .subrecords
                .iter()
                .filter(|field| {
                    matches!(field.signature.as_str(), "EFIT" | "CTDA" | "CIS1" | "CIS2")
                })
                .map(|field| (field.signature.to_string(), field.data.to_vec()))
                .collect::<Vec<_>>();
            assert_eq!(
                saved_rows.len(),
                effect_rows.len(),
                "{signature}:{local:X} changed effect/condition count"
            );
            for ((saved_signature, saved_bytes), (expected_signature, expected_bytes)) in
                saved_rows.iter().zip(effect_rows)
            {
                assert_eq!(
                    *saved_signature, expected_signature,
                    "{signature}:{local:X} changed effect/condition group order"
                );
                if let Some(expected_bytes) = expected_bytes {
                    assert_eq!(
                        *saved_bytes, expected_bytes,
                        "{signature}:{local:X} changed {saved_signature} payload"
                    );
                } else {
                    assert_eq!(
                        saved_bytes.len(),
                        32,
                        "{signature}:{local:X} must serialize a complete FO4 condition"
                    );
                }
            }
        }
    }
    assert!(plugin_handle_close_native(reopened));
}
