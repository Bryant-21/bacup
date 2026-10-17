fn skyrim_torch_data() -> Vec<u8> {
    hex::decode("f000000000020000fabe83000b0000000000803f0000b44200009041abaa2a3f0000003f00002041020000000000003f").unwrap()
}

fn assert_skyrim_light_layout(data: &[u8], source: &[u8]) {
    assert_eq!(data.len(), 64);
    assert_eq!(&data[..40], &source[..40]);
    for (offset, value) in [(40, 0.0_f32), (44, 1.0), (48, 2.0), (52, 10.0)] {
        assert_eq!(&data[offset..offset + 4], &value.to_le_bytes());
    }
    assert_eq!(&data[56..64], &source[40..48]);
}

#[test]
fn skyrim_light_torch_survives_source_decode_translation_and_save_reopen() {
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let source_data = skyrim_torch_data();
    let source = ParsedRecord {
        signature: "LIGH".into(),
        form_id: 0x0001_D4EC,
        flags: 0,
        version_control: 79128,
        form_version: Some(44),
        version2: Some(14),
        subrecords: vec![
            parsed_subrecord("EDID", b"Torch01\0"),
            parsed_subrecord("OBND", &[0; 12]),
            parsed_subrecord("DATA", &source_data),
            parsed_subrecord("FNAM", &1.35_f32.to_le_bytes()),
        ],
        raw_payload: None,
        parse_error: None,
    };
    let mut record = decode_record_from_parsed(
        &source,
        &FormKey::parse("01D4EC@Skyrim.esm", &interner).unwrap(),
        &source_schema,
        &[],
        "Skyrim.esm",
        None,
        false,
        &interner,
    )
    .unwrap();
    let translator = Translator::new(Game::SkyrimSe, Game::Fo4).unwrap();
    let mut ctx = PairCtx::new(&interner);
    translator.pre_translate(&mut ctx, &mut record).unwrap();
    let once = record.fields.clone();
    translator.pre_translate(&mut ctx, &mut record).unwrap();
    assert_eq!(record.fields, once);
    let TranslateResult::Translated(mut translated) = translator.translate(&record, &interner)
    else {
        panic!("Skyrim light must translate");
    };
    translator
        .post_translate(&mut ctx, &mut translated)
        .unwrap();
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut translated,
        )
        .unwrap();
    let TargetRecordNormalization::Keep(mut normalized) = TargetRecordNormalizer {
        target_schema: &target_schema,
        source_record_def: source_schema.record_def("LIGH"),
        interner: Some(&interner),
    }
    .normalize(translated) else {
        panic!("FO4 projection must retain the light");
    };
    const OUTPUT: &str = "SkyrimLightAttenuation.esp";
    normalized.form_key = FormKey::parse(&format!("000800@{OUTPUT}"), &interner).unwrap();
    let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    crate::target_write::add_record_native(handle, normalized, &target_schema, &interner).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        let group = slot
            .parsed
            .root_items
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group) if group.label == *b"LIGH" => Some(group),
                _ => None,
            })
            .unwrap();
        let light = group
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Record(record) if record.signature.as_str() == "LIGH" => Some(record),
                _ => None,
            })
            .unwrap();
        let data = &light
            .subrecords
            .iter()
            .find(|field| field.signature.as_str() == "DATA")
            .unwrap()
            .data;
        assert_skyrim_light_layout(data, &source_data);
        let fade = &light
            .subrecords
            .iter()
            .find(|field| field.signature.as_str() == "FNAM")
            .unwrap()
            .data;
        assert_eq!(fade.as_ref(), &1.35_f32.to_le_bytes());
    }
    assert!(plugin_handle_close_native(reopened));
}

#[test]
fn skyrim_light_maps_nonshadow_spot_and_preserves_shadow_and_flicker_flags() {
    let interner = StringInterner::new();
    for (flags, expected) in [(0x200_u32, 0x4000_u32), (0x3c8, 0x41c8), (0x3c08, 0x3c08)] {
        let mut source = skyrim_torch_data();
        source[12..16].copy_from_slice(&flags.to_le_bytes());
        let mut record = Record::new(
            SigCode(*b"LIGH"),
            FormKey::parse("000800@Skyrim.esm", &interner).unwrap(),
        );
        push_bytes(&mut record, "DATA", &source);
        run_pre_translate(&mut record, &interner);
        let FieldValue::Bytes(data) = &record.fields[0].value else {
            panic!("raw DATA")
        };
        source[12..16].copy_from_slice(&expected.to_le_bytes());
        assert_skyrim_light_layout(data, &source);
    }
}

#[test]
fn skyrim_light_ignores_non_skyrim_layouts_and_unrelated_records() {
    let interner = StringInterner::new();
    for (sig, size) in [("LIGH", 32), ("LIGH", 47), ("LIGH", 64), ("MISC", 48)] {
        let mut record = Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey::parse("000800@Skyrim.esm", &interner).unwrap(),
        );
        push_bytes(&mut record, "DATA", &vec![7; size]);
        let before = record.fields.clone();
        assert!(!lights::lower_skyrim_light(&mut record));
        assert_eq!(record.fields, before);
    }
}
