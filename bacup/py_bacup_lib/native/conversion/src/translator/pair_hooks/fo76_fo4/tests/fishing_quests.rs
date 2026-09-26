fn fishing_quest_fixture(interner: &StringInterner, local: u32, eid: &str) -> Record {
    let mut record = make_record("QUST", interner);
    record.form_key.local = local;
    record.eid = Some(interner.intern(eid));
    let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
    data[..8].copy_from_slice(&0x8500_u64.to_le_bytes());
    data[16] = 2;
    push_field(&mut record, "DATA", raw_bytes(&data));
    push_field(
        &mut record,
        "ENAM",
        FieldValue::Uint(u64::from(u32::from_le_bytes(*b"CLOC"))),
    );
    push_field(&mut record, "ANAM", FieldValue::Uint(1));
    push_field(&mut record, "ALST", FieldValue::Uint(0));
    push_field(
        &mut record,
        "ALID",
        FieldValue::String(interner.intern("Player")),
    );
    push_field(&mut record, "FNAM", FieldValue::Uint(16));
    push_field(
        &mut record,
        "ALFE",
        FieldValue::Uint(u64::from(u32::from_le_bytes(*b"CLOC"))),
    );
    push_field(&mut record, "ALFD", FieldValue::Uint(0x3152));
    push_field(&mut record, "ALED", FieldValue::Bytes(Default::default()));
    record
}

#[test]
fn fishing_quests_keep_player_alias_daily_flags_voice_type_and_alias_order() {
    let interner = StringInterner::new();
    let player = FormKey {
        plugin: interner.intern(FO4_MASTER_NAME),
        local: FO4_PLAYER_REF_FORM_ID,
    };
    let mut record = fishing_quest_fixture(&interner, 0x7BD0A8, "Fishing_MQ01_ChangeLocation");
    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();
    assert!(!record
        .fields
        .iter()
        .any(|f| matches!(&f.sig.0, b"ALFE" | b"ALFD")));
    assert!(record
        .fields
        .iter()
        .any(|f| f.sig.0 == *b"ALFR" && f.value == FieldValue::FormKey(player)));
    let dnam = record.fields.iter().find(|f| f.sig.0 == *b"DNAM").unwrap();
    let FieldValue::Bytes(bytes) = &dnam.value else {
        panic!("DNAM");
    };
    assert_eq!(bytes[0] & 1, 0);
    assert!(record.fields.iter().any(|f| f.sig.0 == *b"ENAM"));
    assert!(!qust_uses_player_connect_autostart_fallback(
        &interner, &record
    ));
    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    let TranslateResult::Translated(translated) = translator.translate(&record, &interner) else {
        panic!("starter must translate");
    };
    let source_schema = AuthoringSchema::for_game("fo76").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let normalizer = crate::target_normalize::TargetRecordNormalizer {
        target_schema: &target_schema,
        source_record_def: source_schema.record_def("QUST"),
        interner: Some(&interner),
    };
    let crate::target_normalize::TargetRecordNormalization::Keep(mut translated) =
        normalizer.normalize(translated)
    else {
        panic!("starter must survive target normalization");
    };
    let mut mapper = FormKeyMapper::new(
        [],
        MapperOptions {
            source_plugin_name: FO76_MASTER_NAME.into(),
            output_plugin_name: FO76_MASTER_NAME.into(),
            target_master_names: vec![FO4_MASTER_NAME.into()],
            resolution_mode: crate::formkey_mapper::ResolutionMode::NullAndWarn,
            ..Default::default()
        },
        &interner,
    );
    mapper.add_mapping(player, player);
    mapper.rewrite_record(&mut translated).unwrap();
    let reference = translated
        .fields
        .iter()
        .find(|f| f.sig.0 == *b"ALFR")
        .unwrap();
    assert_eq!(reference.value, FieldValue::FormKey(player));
    assert_eq!(
        crate::target_write::encode_field_pub(
            reference,
            target_schema.record_def("QUST"),
            &interner
        )
        .unwrap(),
        FO4_PLAYER_REF_FORM_ID.to_le_bytes()
    );

    {
        let mut record = fishing_quest_fixture(&interner, 0x7B95E1, "Fishing_BigFish");
        record
            .fields
            .retain(|f| !matches!(&f.sig.0, b"ALFE" | b"ALFD"));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        assert_eq!(translated_run_once_flag(&record), 0);
        let flags = record.fields.iter().find(|f| f.sig.0 == *b"FNAM").unwrap();
        assert_eq!(field_value_to_u32(&flags.value), Some(0x210));
        let before = format!("{record:?}");
        Fo76Fo4Hook::adapt_fishing_quest_aliases(&interner, &mut record);
        assert_eq!(format!("{record:?}"), before);
    }

    {
        let mut record = fishing_quest_fixture(&interner, 0x7B95E1, "Fishing_BigFish");
        push_field(&mut record, "ALST", FieldValue::Uint(3));
        push_field(&mut record, "VTCK", form_key_value(&interner, 0x7ACD7A));
        Fo76Fo4Hook::adapt_fishing_quest_aliases(&interner, &mut record);
        assert_eq!(
            record.fields.last().unwrap().value,
            form_key_value(&interner, 0x7ACD7E)
        );
        for wrong_plugin in [false, true] {
            let mut other = fishing_quest_fixture(&interner, 0x7BD0A8, "Fishing_MQ01_ChangeLocation");
            if wrong_plugin {
                other.form_key.plugin = interner.intern("Other.esm");
            } else {
                other.eid = Some(interner.intern("Fishing_MQ01_ChangeLocationCopy"));
            }
            let before = format!("{other:?}");
            Fo76Fo4Hook::adapt_fishing_quest_aliases(&interner, &mut other);
            assert_eq!(format!("{other:?}"), before);
        }
    }

    {
        let mut record = fishing_quest_fixture(&interner, 0x7ACB4C, "Fishing_MQ01_Casting");
        for (location, location_form, reference, ref_type) in [
            (1, 0x7A8A73, 3, 0x7ACB4B),
            (12, 0x60D59A, 10, 0x7F6957),
        ] {
            push_field(&mut record, "ALLS", FieldValue::Uint(location));
            push_field(&mut record, "ALFL", form_key_value(&interner, location_form));
            push_field(&mut record, "ALED", FieldValue::Bytes(Default::default()));
            push_field(&mut record, "ALST", FieldValue::Uint(reference));
            push_field(&mut record, "ALFA", FieldValue::Uint(location));
            push_field(&mut record, "ALRT", form_key_value(&interner, ref_type));
            push_field(&mut record, "ALED", FieldValue::Bytes(Default::default()));
        }
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let TranslateResult::Translated(translated) = translator.translate(&record, &interner) else {
            panic!("fishing quest must translate");
        };
        let normalizer = crate::target_normalize::TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def("QUST"),
            interner: Some(&interner),
        };
        let crate::target_normalize::TargetRecordNormalization::Keep(translated) =
            normalizer.normalize(translated)
        else {
            panic!("fishing quest must survive normalization");
        };
        let anchors = translated.fields.iter()
            .filter(|f| matches!(&f.sig.0, b"ALST" | b"ALLS"))
            .map(|f| (f.sig.0, field_value_to_u32(&f.value).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(anchors, vec![
            (*b"ALST", 0), (*b"ALLS", 1), (*b"ALST", 3),
            (*b"ALLS", 12), (*b"ALST", 10),
        ]);
        let dependencies = translated.fields.iter()
            .filter(|f| f.sig.0 == *b"ALFA")
            .map(|f| field_value_to_u32(&f.value).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(dependencies, vec![1, 12]);
    }
}
