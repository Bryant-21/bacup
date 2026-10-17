fn assert_skyrim_hazard_roundtrip(mut source: Record, interner: &StringInterner) {
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, ResolutionMode};

    const OUTPUT: &str = "B21_CreatureHazard.esp";
    let data = source
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DATA")
        .unwrap();
    let FieldValue::Struct(fields) = &data.value else {
        panic!("source hazard references must be typed before dependency traversal");
    };
    let mut expected = Vec::new();
    for name in [
        "limit",
        "radius",
        "lifetime",
        "image_space_radius",
        "target_interval",
        "flags",
    ] {
        let value = fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some(name))
            .unwrap();
        match value.1 {
            FieldValue::Uint(value) => expected.extend_from_slice(&(value as u32).to_le_bytes()),
            FieldValue::Float(value) => expected.extend_from_slice(&value.to_le_bytes()),
            _ => panic!("unexpected hazard scalar"),
        }
    }
    let mut mapper = FormKeyMapper::new(
        [],
        MapperOptions {
            output_plugin_name: OUTPUT.into(),
            resolution_mode: ResolutionMode::Strict,
            ..Default::default()
        },
        interner,
    );
    for (index, name) in ["spell", "light", "impact_data_set", "sound"]
        .into_iter()
        .enumerate()
    {
        let value = &fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some(name))
            .unwrap()
            .1;
        let raw = if let FieldValue::FormKey(key) = value {
            let target = FormKey {
                local: 0x900 + index as u32,
                plugin: interner.intern(OUTPUT),
            };
            mapper.add_mapping(*key, target);
            0x01000000 | target.local
        } else {
            assert_eq!(value, &FieldValue::Uint(0));
            0
        };
        expected.extend_from_slice(&raw.to_le_bytes());
    }
    expected.extend_from_slice(&[0; 12]);
    contact_translate(&mut source, interner);
    mapper.rewrite_record(&mut source).unwrap();
    assert!(
        !source
            .warnings
            .iter()
            .any(|warning| interner.resolve(*warning) == Some("skyrim_hazd_spell_effect_omitted"))
    );
    source.form_key = FormKey {
        local: 0x800,
        plugin: interner.intern(OUTPUT),
    };
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    crate::target_write::add_record_native(handle, source, &schema, interner).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        let parsed = jzargo_find_parsed(&slot.parsed.root_items, "HAZD", 0x800).unwrap();
        let data = parsed
            .subrecords
            .iter()
            .find(|field| field.signature.as_str() == "DNAM")
            .unwrap();
        assert_eq!(data.data.as_ref(), expected.as_slice());
        assert!(
            !parsed
                .subrecords
                .iter()
                .any(|field| field.signature.as_str() == "DATA")
        );
    }
    assert!(plugin_handle_close_native(reopened));
}

#[test]
fn skyrim_hazard_preserves_spell_and_remaps_all_dependencies_on_save_reload() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let mut data = 3u32.to_le_bytes().to_vec();
    for value in [128f32, 27.5, 512.0, 0.75] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&31u32.to_le_bytes());
    for raw in [0x01000123u32, 0x00000456, 0x02000789, 0x01000ABC] {
        data.extend_from_slice(&raw.to_le_bytes());
    }
    let raw = ParsedRecord {
        signature: "HAZD".into(),
        form_id: 0x02000800,
        flags: 0,
        version_control: 0,
        form_version: Some(44),
        version2: Some(0),
        raw_payload: None,
        parse_error: None,
        subrecords: vec![
            parsed_subrecord("EDID", b"HazardMasterRemap\0"),
            parsed_subrecord("DATA", &data),
        ],
    };
    let source = decode_record_from_parsed(
        &raw,
        &FormKey::parse("000800@Patch.esp", &interner).unwrap(),
        &schema,
        &["Skyrim.esm".into(), "Dawnguard.esm".into()],
        "Patch.esp",
        None,
        false,
        &interner,
    )
    .unwrap();
    let mut references = std::collections::HashSet::new();
    source
        .fields
        .iter()
        .for_each(|field| contact_form_keys(&field.value, &mut references));
    let expected: std::collections::HashSet<FormKey> = [
        "000123@Dawnguard.esm",
        "000456@Skyrim.esm",
        "000789@Patch.esp",
        "000ABC@Dawnguard.esm",
    ]
    .map(|key| FormKey::parse(key, &interner).unwrap())
    .into_iter()
    .collect();
    assert_eq!(references, expected);
    assert_skyrim_hazard_roundtrip(source, &interner);
}

#[test]
#[ignore = "requires installed source Skyrim.esm"]
fn source_creature_call_storm_hazard_roundtrips_native_fo4() {
    let path = std::env::var("MODKIT_SKYRIM_SOURCE_ESM").expect("source Skyrim.esm path");
    let handle = plugin_handle_load_no_py(&path, Some("skyrimse"), None, None, true).unwrap();
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let source =
        crate::source_read::read_record(handle, "0E4CB3@Skyrim.esm", &schema, &interner).unwrap();
    assert!(plugin_handle_close_native(handle));
    let value = &source
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DATA")
        .unwrap()
        .value;
    for (name, expected) in [
        ("limit", FieldValue::Uint(1)),
        ("radius", FieldValue::Float(5.0)),
        ("lifetime", FieldValue::Float(25.0)),
        ("target_interval", FieldValue::Float(10.0)),
        ("flags", FieldValue::Uint(2)),
        ("spell", FieldValue::Uint(0)),
        (
            "sound",
            FieldValue::FormKey(FormKey::parse("101014@Skyrim.esm", &interner).unwrap()),
        ),
    ] {
        assert_eq!(magic_condition_member(value, name, &interner), &expected);
    }
    assert_skyrim_hazard_roundtrip(source, &interner);
}
