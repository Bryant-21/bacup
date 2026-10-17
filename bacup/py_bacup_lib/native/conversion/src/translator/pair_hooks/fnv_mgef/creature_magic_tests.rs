#[test]
fn chameleon_uses_native_invisibility_actor_value_without_losing_partial_magnitude() {
    let interner = StringInterner::new();
    let mut mapper = self::mapper(&interner, &[]);
    let mut data = legacy_data();
    set_u32(&mut data, 0, SOURCE_SELF | 2);
    set_u32(&mut data, 64, 12);
    set_i32(&mut data, 68, 49);
    let mut record = record_with_data(&interner, data);
    let report = normalize_legacy_mgef_data(&mut record, LegacyMgefFamily::Fnv, &mut mapper);
    let bytes = encoded_data(&record);
    assert_eq!(read_u32(&bytes, 0), 2);
    assert_eq!(read_u32(&bytes, 64), 49);
    assert_eq!(read_u32(&bytes, 68), 0x2F3);
    assert_eq!(read_u32(&bytes, 80), 0);
    assert_eq!(read_u32(&bytes, 84), 0);
    assert!(matches!(
        decision(&report, "actor_value"),
        MgefReferenceOutcome::PreservedHardcoded {
            source_value: 49,
            target_raw: 0x2F3
        }
    ));
    let FieldValue::Struct(fields) = &record.fields[0].value else {
        panic!("native DATA")
    };
    assert!(fields.iter().any(|(name, value)| interner.resolve(*name) == Some("actor_value") && matches!(value, FieldValue::FormKey(key) if key.local == 0x2F3 && interner.resolve(key.plugin) == Some("Fallout4.esm"))));
}

#[test]
#[ignore = "requires installed FalloutNV.esm"]
fn source_nightstalker_cloak_preserves_both_effects_and_relocates_native_actor_values() {
    use crate::schema::AuthoringSchema;
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use crate::translator::pair_hook::PairCtx;
    use crate::translator::pair_hooks::fnv_fo4::LegacySerialNormalizationState;
    use crate::translator::target_hook::TargetCtx;
    use crate::translator::{Game, TranslateResult, Translator};
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, plugin_handle_add_master_native, plugin_handle_close_native,
        plugin_handle_load_no_py, plugin_handle_new_no_py, plugin_handle_save_no_py,
        plugin_handle_store_ref,
    };

    fn parsed_record(items: &[ParsedItem], local: u32) -> Option<&ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id & 0x00ff_ffff == local => {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(record) = parsed_record(&group.children, local) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }
    fn payload<'a>(record: &'a ParsedRecord, signature: &str) -> &'a [u8] {
        record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == signature)
            .unwrap()
            .data
            .as_ref()
    }

    let path =
        std::env::var("MODKIT_FNV_CREATURE_MAGIC_PLUGIN").expect("FalloutNV.esm source path");
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("fnv").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let translator = Translator::new(Game::Fnv, Game::Fo4).unwrap();
    let source_handle = plugin_handle_load_no_py(&path, Some("fnv"), None, None, true).unwrap();
    const OUTPUT: &str = "B21_NightstalkerMagic.esp";
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            source_plugin_name: "FalloutNV.esm".into(),
            output_plugin_name: OUTPUT.into(),
            target_master_names: vec!["Preceding.esm".into(), "Fallout4.esm".into()],
            ..MapperOptions::default()
        },
        &interner,
    );
    let mut records = Vec::new();
    for (source_local, target_local) in [(0x43E95, 0x800), (0x31D73, 0x801), (0x7E474, 0x802)] {
        let source = form_key(&interner, "FalloutNV.esm", source_local);
        let target = form_key(&interner, OUTPUT, target_local);
        mapper.add_mapping(source, target);
        mapper.add_mapping(target, target);
        let record = crate::source_read::read_record(
            source_handle,
            &format!("{source_local:06X}@FalloutNV.esm"),
            &source_schema,
            &interner,
        )
        .unwrap();
        records.push((source, target, record));
    }
    assert!(plugin_handle_close_native(source_handle));
    let output = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    for master in ["Preceding.esm", "Fallout4.esm"] {
        plugin_handle_add_master_native(output, master, None).unwrap();
    }
    let mut state = LegacySerialNormalizationState::default();
    for (source, target, mut record) in records {
        let mut context = PairCtx::with_source(&interner, "FalloutNV.esm", &[]);
        translator.pre_translate(&mut context, &mut record).unwrap();
        let TranslateResult::Translated(mut record) =
            translator.translate_ignoring_skip(&record, &interner, record.sig.as_str())
        else {
            panic!("creature spell/effect translation")
        };
        translator
            .post_translate(&mut context, &mut record)
            .unwrap();
        record.form_key = target;
        let report = translator
            .normalize_serial_mapper_record_once(source, &mut record, &mut mapper, &mut state)
            .unwrap()
            .unwrap();
        report.register_target_identities(&mut mapper);
        mapper.rewrite_record(&mut record).unwrap();
        translator
            .run_target_hook(
                &mut TargetCtx {
                    interner: &interner,
                },
                &mut record,
            )
            .unwrap();
        let TargetRecordNormalization::Keep(record) =
            TargetRecordNormalizer::target_only_with_interner(&target_schema, &interner)
                .normalize(record)
        else {
            panic!("native spell/effect retained")
        };
        crate::target_write::add_record_native(output, record, &target_schema, &interner).unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(output, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(output));
    let output =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&output).unwrap();
        let chameleon = payload(
            parsed_record(&slot.parsed.root_items, 0x800).unwrap(),
            "DATA",
        );
        assert_eq!(chameleon.len(), 152);
        assert_eq!(read_u32(chameleon, 64), 49);
        assert_eq!(read_u32(chameleon, 68), 0x0100_02F3);
        assert_eq!(read_u32(chameleon, 80), 0);
        assert_eq!(read_u32(chameleon, 84), 0);
        let sneak = payload(
            parsed_record(&slot.parsed.root_items, 0x801).unwrap(),
            "DATA",
        );
        assert_eq!(read_u32(sneak, 68), 0x0100_037F);
        let spell = parsed_record(&slot.parsed.root_items, 0x802).unwrap();
        let spell_data = payload(spell, "SPIT");
        assert_eq!(spell_data.len(), 36);
        assert_eq!(read_u32(spell_data, 8), 4);
        assert_eq!(read_u32(spell_data, 16), 0);
        let effects = spell
            .subrecords
            .iter()
            .filter(|field| matches!(field.signature.as_str(), "EFID" | "EFIT"))
            .collect::<Vec<_>>();
        assert_eq!(effects.len(), 4);
        for (group, (target, magnitude)) in effects
            .chunks_exact(2)
            .zip([(0x0200_0800, 95.0), (0x0200_0801, 100.0)])
        {
            assert_eq!(group[0].signature.as_str(), "EFID");
            assert_eq!(read_u32(group[0].data.as_ref(), 0), target);
            assert_eq!(group[1].signature.as_str(), "EFIT");
            assert_eq!(group[1].data.len(), 12);
            assert_eq!(read_f32(group[1].data.as_ref(), 0), magnitude);
            assert_eq!(read_u32(group[1].data.as_ref(), 4), 0);
            assert_eq!(read_u32(group[1].data.as_ref(), 8), 0);
        }
    }
    assert!(plugin_handle_close_native(output));
}
