fn magic_condition_source(function: u16, parameter: u32) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[4..8].copy_from_slice(&0.5f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    bytes[28..32].copy_from_slice(&(-1i32).to_le_bytes());
    bytes
}

fn magic_condition_member<'a>(
    value: &'a FieldValue,
    name: &str,
    interner: &StringInterner,
) -> &'a FieldValue {
    let FieldValue::Struct(fields) = value else {
        panic!("typed CTDA required")
    };
    &fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .unwrap()
        .1
}

#[test]
fn creature_perk_conditions_decode_school_keyword_and_actor_value_operands() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let ctx = PairCtx::with_source(&interner, "Skyrim.esm", &[]);
    for (function, source, target, plugin) in [
        (14, 74, 0x307, "Fallout4.esm"),
        (277, 19, 0x383, "Fallout4.esm"),
        (696, 18, 0x382, "Fallout4.esm"),
        (693, 0x1EA69, 0x1EA69, "Skyrim.esm"),
        (713, 0xC44BF, 0xC44BF, "Skyrim.esm"),
    ] {
        let value = decode_source_magic_condition("PERK", "CTDA", &magic_condition_source(function, source), &schema, &ctx).unwrap();
        jzargo_assert_form(magic_condition_member(&value, "parameter_1", &interner), target, plugin, &interner);
        assert_eq!(magic_condition_member(&value, "function", &interner), &FieldValue::Uint(u64::from(function)));
        assert_eq!(lowered_magic_condition_actor_value(&value, &interner).is_some(), plugin == "Fallout4.esm");
    }
}

#[test]
fn magic_conditions_decode_actor_values_and_reference_flags() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let masters = vec!["Skyrim.esm".to_string(), "Dawnguard.esm".to_string()];
    let ctx = PairCtx::with_source(&interner, "Dragonborn.esm", &masters);
    for (source, local, plugin) in [
        (24, 0x2D4, "Fallout4.esm"),
        (25, 1, "B21_SkyrimMagicRuntime.esm"),
    ] {
        let value = decode_source_magic_condition(
            "SPEL",
            "CTDA",
            &magic_condition_source(640, source),
            &schema,
            &ctx,
        )
        .unwrap();
        jzargo_assert_form(
            magic_condition_member(&value, "parameter_1", &interner),
            local,
            plugin,
            &interner,
        );
        assert_eq!(
            magic_condition_member(&value, "function", &interner),
            &FieldValue::Uint(640)
        );
    }
    let mut raw = magic_condition_source(560, 0x01001234);
    raw[0] = 0xA5;
    raw[4..8].copy_from_slice(&0x02000567u32.to_le_bytes());
    raw[20..24].copy_from_slice(&2u32.to_le_bytes());
    raw[24..28].copy_from_slice(&0x14u32.to_le_bytes());
    let value = decode_source_magic_condition("MGEF", "CTDA", &raw, &schema, &ctx).unwrap();
    jzargo_assert_form(
        magic_condition_member(&value, "parameter_1", &interner),
        0x1234,
        "Dawnguard.esm",
        &interner,
    );
    jzargo_assert_form(
        magic_condition_member(&value, "comparison_value", &interner),
        0x567,
        "Dragonborn.esm",
        &interner,
    );
    jzargo_assert_form(
        magic_condition_member(&value, "reference", &interner),
        0x14,
        "Skyrim.esm",
        &interner,
    );
    assert_eq!(
        magic_condition_member(&value, "type", &interner),
        &FieldValue::Uint(0xA5)
    );
    assert_eq!(
        magic_condition_member(&value, "run_on", &interner),
        &FieldValue::Uint(2)
    );
    assert_eq!(
        magic_condition_member(&value, "parameter_3", &interner),
        &FieldValue::Int(-1)
    );
    for flag in [2, 8] {
        let mut raw = magic_condition_source(719, 9);
        raw[0] = flag;
        raw[20..24].copy_from_slice(&5u32.to_le_bytes());
        raw[28..32].copy_from_slice(&3i32.to_le_bytes());
        let value = decode_source_magic_condition("SPEL", "CTDA", &raw, &schema, &ctx).unwrap();
        assert_eq!(
            magic_condition_member(&value, "parameter_1", &interner),
            &FieldValue::Uint(9)
        );
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        let encoded = crate::target_write::encode_field_pub(
            &FieldEntry {
                sig: SubrecordSig(*b"CTDA"),
                value,
            },
            target_schema.record_def("SPEL"),
            &interner,
        )
        .unwrap();
        assert_eq!(
            encoded, raw,
            "alias/package index and run-on selector must stay numeric"
        );
        assert!(
            decode_source_magic_condition("SPEL", "CTDA", &raw, &target_schema, &ctx).is_none()
        );
    }
}

#[test]
fn magic_conditions_preserve_infection_chance_and_compatible_opcodes() {
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let ctx = PairCtx::with_source(&interner, "Skyrim.esm", &[]);
    for function in [
        49, 71, 72, 74, 77, 80, 244, 289, 300, 323, 448, 560, 682, 698, 699, 700, 715, 719, 724,
    ] {
        let mut raw = magic_condition_source(function, 0);
        raw[0] = 0xA1;
        raw[4..8].copy_from_slice(&15f32.to_le_bytes());
        let value =
            decode_source_magic_condition("SPEL", "CTDA", &raw, &source_schema, &ctx).unwrap();
        let encoded = crate::target_write::encode_field_pub(
            &FieldEntry {
                sig: SubrecordSig(*b"CTDA"),
                value,
            },
            target_schema.record_def("SPEL"),
            &interner,
        )
        .unwrap();
        assert_eq!(
            encoded, raw,
            "function {function} must keep comparison and OR semantics"
        );
    }
}

#[test]
#[ignore = "requires the full source Skyrim creature ability corpus and installed source plugins"]
fn source_magic_conditions_corpus_serializes_all_operands() {
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var("MODKIT_SKYRIM_ABILITY_MAGIC_INPUTS")
                .expect("source ability corpus manifest"),
        )
        .unwrap(),
    )
    .unwrap();
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    const OUTPUT: &str = "B21_MagicConditionCorpus.esp";
    let target_masters = [
        "Fallout4.esm",
        "Dragonborn.esm",
        "Dawnguard.esm",
        "Update.esm",
        "Skyrim.esm",
        "HearthFires.esm",
        "B21_SkyrimMagicRuntime.esm",
        "B21_SkyrimSkillRuntime.esm",
    ];
    let output = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    for master in target_masters {
        plugin_handle_add_master_native(output, master, None).unwrap();
    }
    let mut handles = std::collections::HashMap::new();
    let mut expectations = Vec::new();
    let mut functions = std::collections::BTreeMap::<u16, usize>::new();
    for input in manifest.as_array().unwrap() {
        let path = input["path"].as_str().unwrap();
        let handle = *handles.entry(path.to_string()).or_insert_with(|| {
            plugin_handle_load_no_py(path, Some("skyrimse"), None, None, true).unwrap()
        });
        let key = input["key"].as_str().unwrap().replace(':', "@");
        let form = FormKey::parse(&key, &interner).unwrap();
        let snapshot =
            crate::source_read::snapshot_records_by_form_keys(handle, &[form], &interner).unwrap();
        let raw = crate::source_read::uncompressed_record_view(&snapshot.records[0].raw_record);
        let remap_reference = |raw: u32| {
            if raw == 0 {
                return 0;
            }
            let source_index = (raw >> 24) as usize;
            let plugin = if source_index == snapshot.masters.len() {
                snapshot.plugin_name.as_str()
            } else {
                snapshot.masters[source_index].as_str()
            };
            let target_index = target_masters
                .iter()
                .position(|name| name.eq_ignore_ascii_case(plugin))
                .unwrap();
            ((target_index as u32) << 24) | (raw & 0x00FF_FFFF)
        };
        let mut expected = Vec::new();
        for row in raw
            .subrecords
            .iter()
            .filter(|row| row.signature.as_str() == "CTDA")
        {
            let mut bytes = row.data.to_vec();
            assert_eq!(bytes.len(), 32, "{key}");
            let function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
            *functions.entry(function).or_default() += 1;
            let parameter = jzargo_parsed_u32(&bytes, 12);
            let converted = match function {
                640 => match parameter {
                    24 => 0x2D4,
                    25 => 0x06000001,
                    _ => panic!("unverified AV {parameter} in {key}"),
                },
                71 | 72 | 74 | 448 | 560 | 682 | 699 => remap_reference(parameter),
                719 if bytes[0] & 0x0A == 0 => remap_reference(parameter),
                49 | 77 | 80 | 244 | 289 | 300 | 323 | 698 | 700 | 715 | 719 | 724 => parameter,
                _ => panic!("unverified source function {function} in {key}"),
            };
            bytes[12..16].copy_from_slice(&converted.to_le_bytes());
            if bytes[0] & 4 != 0 {
                let value = remap_reference(jzargo_parsed_u32(&bytes, 4));
                bytes[4..8].copy_from_slice(&value.to_le_bytes());
            }
            if jzargo_parsed_u32(&bytes, 20) == 2 {
                let value = remap_reference(jzargo_parsed_u32(&bytes, 24));
                bytes[24..28].copy_from_slice(&value.to_le_bytes());
            }
            expected.push(bytes);
        }
        if expected.is_empty() {
            continue;
        }
        let mut record =
            crate::source_read::read_record(handle, &key, &source_schema, &interner).unwrap();
        let conditions = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .collect::<Vec<_>>();
        assert_eq!(conditions.len(), expected.len(), "{key}");
        assert!(
            conditions
                .iter()
                .all(|row| matches!(row.value, FieldValue::Struct(_))),
            "every source condition must decode: {key}"
        );
        contact_translate(&mut record, &interner);
        record.form_key = FormKey {
            local: 0x800 + expectations.len() as u32,
            plugin: interner.intern(OUTPUT),
        };
        expectations.push((
            record.sig.as_str().to_string(),
            record.form_key.local,
            expected,
        ));
        crate::target_write::add_record_native(output, record, &target_schema, &interner).unwrap();
    }
    for handle in handles.into_values() {
        assert!(plugin_handle_close_native(handle));
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(output, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(output));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        for (signature, local, expected) in expectations {
            let record = jzargo_find_parsed(&slot.parsed.root_items, &signature, local).unwrap();
            let saved = record
                .subrecords
                .iter()
                .filter(|row| row.signature.as_str() == "CTDA")
                .map(|row| row.data.to_vec())
                .collect::<Vec<_>>();
            assert_eq!(
                saved, expected,
                "{signature}:{local:X} condition semantics after save/reopen"
            );
        }
    }
    assert!(plugin_handle_close_native(reopened));
    assert_eq!(functions.len(), 20);
    assert!(functions.values().sum::<usize>() >= 242);
    assert_eq!(functions.get(&77), Some(&9));
    println!("verified Skyrim magic condition census: {functions:?}");
}
