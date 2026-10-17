fn skyrim_ammo_source_data() -> Vec<u8> {
    [
        0x03BE11_u32.to_le_bytes(),
        4_u32.to_le_bytes(),
        8_f32.to_le_bytes(),
        1_u32.to_le_bytes(),
        0.1_f32.to_le_bytes(),
    ]
    .concat()
}

#[test]
fn skyrim_ammo_layout_survives_source_decode_translate_save_reopen() {
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let source = ParsedRecord {
        signature: "AMMO".into(),
        form_id: 0x01397D,
        flags: 0,
        version_control: 0,
        form_version: Some(44),
        version2: None,
        subrecords: vec![
            parsed_subrecord("EDID", b"IronArrow\0"),
            parsed_subrecord("DATA", &skyrim_ammo_source_data()),
        ],
        raw_payload: None,
        parse_error: None,
    };
    let mut record = decode_record_from_parsed(
        &source,
        &FormKey::parse("01397D@Skyrim.esm", &interner).unwrap(),
        &source_schema,
        &[],
        "Skyrim.esm",
        None,
        false,
        &interner,
    )
    .unwrap();
    let translator = Translator::new(Game::SkyrimSe, Game::Fo4).unwrap();
    let mut ctx = PairCtx::with_source(&interner, "Skyrim.esm", &[]);
    translator.pre_translate(&mut ctx, &mut record).unwrap();
    let once = record.fields.clone();
    translator.pre_translate(&mut ctx, &mut record).unwrap();
    assert_eq!(record.fields, once);
    let TranslateResult::Translated(mut translated) = translator.translate(&record, &interner)
    else {
        panic!("ammo translation failed");
    };
    translator
        .post_translate(&mut ctx, &mut translated)
        .unwrap();
    let TargetRecordNormalization::Keep(record) = TargetRecordNormalizer {
        target_schema: &target_schema,
        source_record_def: source_schema.record_def("AMMO"),
        interner: Some(&interner),
    }
    .normalize(translated) else {
        panic!("ammo target normalization failed");
    };
    let handle = plugin_handle_new_no_py("Skyrim.esm", Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    crate::target_write::add_record_native(handle, record, &target_schema, &interner).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Skyrim.esm");
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let saved = jzargo_find_parsed(
            &store.get(&reopened).unwrap().parsed.root_items,
            "AMMO",
            0x01397D,
        )
        .unwrap();
        let data = jzargo_parsed_subrecord(saved, "DATA");
        assert_eq!(data.len(), 8);
        assert_eq!(jzargo_parsed_u32(data, 0), 1);
        assert_eq!(f32::from_le_bytes(data[4..8].try_into().unwrap()), 0.1);
        let dnam = jzargo_parsed_subrecord(saved, "DNAM");
        assert_eq!(dnam.len(), 16);
        assert_eq!(jzargo_parsed_u32(dnam, 0), 0x0103BE11);
        assert_eq!(jzargo_parsed_u32(dnam, 4), 0);
        assert_eq!(f32::from_le_bytes(dnam[8..12].try_into().unwrap()), 8.0);
        assert_eq!(jzargo_parsed_u32(dnam, 12), 0);
    }
    assert!(plugin_handle_close_native(reopened));
}

#[test]
fn skyrim_ammo_layout_preserves_flags_and_source_projectile_ownership() {
    let interner = StringInterner::new();
    let masters = ["Skyrim.esm".to_string()];
    let ctx = PairCtx::with_source(&interner, "Dawnguard.esm", &masters);
    for length in [16, 20] {
        let mut bytes = skyrim_ammo_source_data();
        bytes[..4].copy_from_slice(&0x01000800_u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&7_u32.to_le_bytes());
        bytes.truncate(length);
        let mut record = Record::new(
            SigCode(*b"AMMO"),
            FormKey::parse("000801@Dawnguard.esm", &interner).unwrap(),
        );
        push_bytes(&mut record, "DATA", &bytes);
        ammo::normalize_skyrim_ammo(&mut record, &ctx).unwrap();
        assert_eq!(
            skyrim_weapon_layout_value(&record, b"DNAM", "flags", &interner),
            &FieldValue::Uint(3)
        );
        assert_eq!(
            skyrim_weapon_layout_value(&record, b"DNAM", "projectile", &interner),
            &FieldValue::FormKey(FormKey::parse("000800@Dawnguard.esm", &interner).unwrap())
        );
        assert_eq!(
            skyrim_weapon_layout_value(&record, b"DATA", "weight", &interner),
            &FieldValue::Float(if length == 20 { 0.1 } else { 0.0 })
        );
    }
}

#[test]
fn skyrim_ammo_layout_rejects_truncated_data_without_mutation() {
    let interner = StringInterner::new();
    let mut record = Record::new(
        SigCode(*b"AMMO"),
        FormKey::parse("000800@Skyrim.esm", &interner).unwrap(),
    );
    push_bytes(&mut record, "DATA", &skyrim_ammo_source_data()[..15]);
    let before = record.fields.clone();
    assert!(ammo::normalize_skyrim_ammo(&mut record, &PairCtx::new(&interner)).is_err());
    assert_eq!(record.fields, before);
}

#[test]
#[ignore = "requires the installed Skyrim Special Edition source masters"]
fn skyrim_ammo_layout_accepts_installed_master_corpus() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let mut total = 0;
    for plugin in [
        "Skyrim.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let path =
            std::path::Path::new("N:/Steam Games/steamapps/common/Skyrim Special Edition/Data")
                .join(plugin);
        let handle =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("skyrimse"), None, None, true)
                .unwrap();
        let (name, masters) = crate::source_read::plugin_context_for_handle(handle).unwrap();
        let ctx = PairCtx::with_source(&interner, &name, &masters);
        for key in
            crate::source_read::iter_form_keys_of_sig(handle, SigCode(*b"AMMO"), &interner).unwrap()
        {
            let mut record = crate::source_read::read_record_relayout_by_form_key(
                handle, &key, &schema, &interner, None,
            )
            .unwrap();
            ammo::normalize_skyrim_ammo(&mut record, &ctx)
                .unwrap_or_else(|error| panic!("{plugin}:{:06X}: {error}", key.local));
            if record.fields.iter().any(|field| field.sig.0 == *b"DATA") {
                assert!(record.fields.iter().any(|field| field.sig.0 == *b"DNAM"));
            }
            total += 1;
        }
        assert!(plugin_handle_close_native(handle));
    }
    assert!(total > 20);
    eprintln!("Skyrim ammo source layout corpus: {total} records");
}
