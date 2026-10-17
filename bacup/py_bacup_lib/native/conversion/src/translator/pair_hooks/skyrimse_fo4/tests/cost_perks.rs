fn assert_cost_perk_roundtrip(mut records: Vec<Record>, interner: &StringInterner) {
    const OUTPUT: &str = "B21_CreatureCostPerks.esp";
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    plugin_handle_add_master_native(handle, "Skyrim.esm", None).unwrap();
    let mut expectations = Vec::new();
    for (index, record) in records.iter_mut().enumerate() {
        let source = record.form_key;
        let conditions = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .map(|field| field.value.clone())
            .collect::<Vec<_>>();
        assert!(
            conditions
                .iter()
                .all(|value| matches!(value, FieldValue::Struct(_)))
        );
        assert!(conditions.iter().any(|value| {
            magic_condition_member(value, "function", interner) == &FieldValue::Uint(713)
                && magic_condition_member(value, "parameter_1", interner)
                    == &FieldValue::FormKey(source)
        }));
        contact_translate(record, interner);
        record.form_key = FormKey {
            local: 0x800 + index as u32,
            plugin: interner.intern(OUTPUT),
        };
        expectations.push((record.form_key.local, source.local, conditions));
        crate::target_write::add_record_native(handle, record.clone(), &schema, interner).unwrap();
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
        for (local, source_local, conditions) in expectations {
            let parsed = jzargo_find_parsed(&slot.parsed.root_items, "PERK", local).unwrap();
            let entry = parsed
                .subrecords
                .iter()
                .position(|field| field.signature.as_str() == "PRKE")
                .unwrap();
            let effect = &parsed.subrecords[entry..];
            assert_eq!(
                effect
                    .iter()
                    .map(|field| field.signature.as_str())
                    .collect::<Vec<_>>(),
                ["PRKE", "DATA", "PRKC", "CTDA", "EPFT", "EPFD", "PRKF"]
            );
            assert_eq!(effect[0].data.as_ref(), &[2, 0, 0]);
            assert_eq!(effect[1].data.as_ref(), &[38, 3, 2]);
            assert_eq!(effect[2].data.as_ref(), &[1]);
            assert_eq!(effect[4].data.as_ref(), &[1]);
            assert_eq!(effect[5].data.as_ref(), &0.5f32.to_le_bytes());
            let saved = parsed
                .subrecords
                .iter()
                .filter(|field| field.signature.as_str() == "CTDA")
                .collect::<Vec<_>>();
            assert_eq!(saved.len(), conditions.len());
            for (row, value) in saved.into_iter().zip(conditions) {
                let function = u16::from_le_bytes(row.data[8..10].try_into().unwrap());
                assert_eq!(
                    magic_condition_member(&value, "function", interner),
                    &FieldValue::Uint(u64::from(function))
                );
                let FieldValue::FormKey(parameter) =
                    magic_condition_member(&value, "parameter_1", interner)
                else {
                    panic!("cost conditions must retain typed operands");
                };
                let master = if interner.resolve(parameter.plugin) == Some("Fallout4.esm") {
                    0
                } else {
                    1
                };
                assert_eq!(
                    jzargo_parsed_u32(&row.data, 12),
                    (master << 24) | parameter.local
                );
                if function == 713 {
                    assert_eq!(jzargo_parsed_u32(&row.data, 12), 0x01000000 | source_local);
                }
            }
        }
    }
    assert!(plugin_handle_close_native(reopened));
}

#[test]
fn source_cost_perk_preserves_entrypoint_and_casting_perk_condition() {
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let mut skill = magic_condition_source(277, 20);
    skill[0] = 0x60;
    skill[4..8].copy_from_slice(&25f32.to_le_bytes());
    let mut previous = magic_condition_source(448, 0xF2CA8);
    previous[4..8].copy_from_slice(&1f32.to_le_bytes());
    let mut casting = magic_condition_source(713, 0xC44BF);
    casting[4..8].copy_from_slice(&1f32.to_le_bytes());
    let source = ParsedRecord {
        signature: "PERK".into(),
        form_id: 0xC44BF,
        flags: 0,
        version_control: 0,
        form_version: Some(44),
        version2: Some(0),
        raw_payload: None,
        parse_error: None,
        subrecords: vec![
            parsed_subrecord("EDID", b"DestructionApprentice25\0"),
            parsed_subrecord("CTDA", &skill),
            parsed_subrecord("CTDA", &previous),
            parsed_subrecord("DATA", &[0, 0, 1, 1, 0]),
            parsed_subrecord("PRKE", &[2, 0, 0]),
            parsed_subrecord("DATA", &[38, 3, 2]),
            parsed_subrecord("PRKC", &[1]),
            parsed_subrecord("CTDA", &casting),
            parsed_subrecord("EPFT", &[1]),
            parsed_subrecord("EPFD", &0.5f32.to_le_bytes()),
            parsed_subrecord("PRKF", &[]),
        ],
    };
    let form = FormKey::parse("0C44BF@Skyrim.esm", &interner).unwrap();
    let decoded = decode_record_from_parsed(
        &source,
        &form,
        &source_schema,
        &[],
        "Skyrim.esm",
        None,
        false,
        &interner,
    )
    .unwrap();
    assert!(decoded.warnings.is_empty(), "{:?}", decoded.warnings);
    assert_cost_perk_roundtrip(vec![decoded], &interner);
}

#[test]
#[ignore = "requires installed source Skyrim.esm"]
fn source_creature_cost_perk_corpus_roundtrips_native_fo4() {
    let path = std::env::var("MODKIT_SKYRIM_SOURCE_ESM").expect("source Skyrim.esm path");
    let source = plugin_handle_load_no_py(&path, Some("skyrimse"), None, None, true).unwrap();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let interner = StringInterner::new();
    let records = [
        0xC44BD, 0xC44BF, 0xC44C0, 0xC44C1, 0xC44C2, 0xC44C8, 0xF2CA7, 0xF2CA8, 0xF2CAA,
    ]
    .into_iter()
    .map(|local| {
        crate::source_read::read_record(
            source,
            &format!("{local:06X}@Skyrim.esm"),
            &schema,
            &interner,
        )
        .unwrap()
    })
    .collect();
    assert!(plugin_handle_close_native(source));
    assert_cost_perk_roundtrip(records, &interner);
}

#[test]
fn source_perk_ability_and_combat_hit_spell_are_typed_dependencies() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let source = ParsedRecord {
        signature: "PERK".into(),
        form_id: 0x01000800,
        flags: 0,
        version_control: 0,
        form_version: Some(44),
        version2: Some(0),
        raw_payload: None,
        parse_error: None,
        subrecords: vec![
            parsed_subrecord("DATA", &[0, 0, 1, 0, 0]),
            parsed_subrecord("PRKE", &[1, 0, 0]),
            parsed_subrecord("DATA", &0x01000810u32.to_le_bytes()),
            parsed_subrecord("PRKF", &[]),
            parsed_subrecord("PRKE", &[2, 0, 0]),
            parsed_subrecord("DATA", &[51, 10, 3]),
            parsed_subrecord("EPFT", &[4]),
            parsed_subrecord("EPFD", &0x00109D7Bu32.to_le_bytes()),
            parsed_subrecord("PRKF", &[]),
        ],
    };
    let form = FormKey::parse("000800@Dawnguard.esm", &interner).unwrap();
    let decoded = decode_record_from_parsed(
        &source,
        &form,
        &schema,
        &["Skyrim.esm".to_string()],
        "Dawnguard.esm",
        None,
        false,
        &interner,
    )
    .unwrap();
    let refs = decoded
        .fields
        .iter()
        .filter_map(|field| match field.value {
            FieldValue::FormKey(key) => Some((field.sig.as_str(), key)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        refs,
        [
            (
                "DATA",
                FormKey::parse("000810@Dawnguard.esm", &interner).unwrap()
            ),
            (
                "EPFD",
                FormKey::parse("109D7B@Skyrim.esm", &interner).unwrap()
            ),
        ]
    );
}
