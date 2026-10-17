const TEMP_LIGHT_01_DATA: &str = "fffffffff4010000f3e29c00000000000000803f0000b4420000000000000000";

fn assert_fo4_light_attenuation(data: &[u8]) {
    assert_eq!(data.len(), 64);
    assert_eq!(f32_at(data, 24), 0.001);
    for offset in [28, 32, 36, 40] {
        assert_eq!(f32_at(data, offset), 0.0, "offset {offset}");
    }
    assert_eq!(f32_at(data, 44), 1.0);
    assert_eq!(f32_at(data, 48), 2.0);
    assert_eq!(f32_at(data, 52), 10.0);
}

#[test]
fn legacy_light_hooks_preserve_source_values_and_supply_radial_attenuation() {
    let interner = StringInterner::new();
    for hook in [&FnvFo4Hook as &dyn PairHook, &Fo3Fo4Hook] {
        let mut source = bytes_from_hex(TEMP_LIGHT_01_DATA);
        source[24..28].copy_from_slice(&123_u32.to_le_bytes());
        source[28..32].copy_from_slice(&2.5_f32.to_le_bytes());
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(source.clone().into()),
        );
        push_field(&mut record, "FNAM", FieldValue::Float(1.1));
        hook.pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let target = raw_field(&record, "DATA");
        assert_fo4_light_attenuation(target);
        assert_eq!(&target[..24], &source[..24]);
        assert_eq!(&target[56..64], &source[24..32]);
        assert_eq!(record.fields[1].value, FieldValue::Float(1.1));
        let once = record.fields.clone();
        hook.pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        assert_eq!(record.fields, once);
    }
}

#[test]
fn legacy_light_nonshadow_spot_flag_moves_without_losing_other_flags() {
    let interner = StringInterner::new();
    for flags in [0x200_u32, 0x401, 0x608, 0x1e7] {
        let mut source = bytes_from_hex(TEMP_LIGHT_01_DATA);
        source[12..16].copy_from_slice(&flags.to_le_bytes());
        let mut record = make_record("LIGH", &interner);
        push_field(&mut record, "DATA", FieldValue::Bytes(source.into()));
        assert!(lower_legacy_light(&mut record));
        let expected = match flags {
            0x200 => 0x4000,
            0x608 => 0x4408,
            _ => flags,
        };
        assert_eq!(u32_at(raw_field(&record, "DATA"), 12), expected);
    }
}

#[test]
fn legacy_light_lowering_leaves_other_layouts_untouched() {
    let interner = StringInterner::new();
    for (signature, length) in [("LIGH", 0), ("LIGH", 31), ("LIGH", 64), ("MISC", 32)] {
        let mut record = make_record(signature, &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(vec![7; length].into()),
        );
        let before = record.fields.clone();
        assert!(!lower_legacy_light(&mut record));
        assert_eq!(record.fields, before);
    }
}

#[test]
fn legacy_light_attenuation_survives_translation_projection_save_reopen() {
    use crate::schema::AuthoringSchema;
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use crate::translator::{TranslateResult, Translator};
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, plugin_handle_add_master_native, plugin_handle_close_native,
        plugin_handle_load_no_py, plugin_handle_new_no_py, plugin_handle_save_no_py,
        plugin_handle_store_ref,
    };

    fn find_light(items: &[ParsedItem], local: u32) -> Option<&ParsedRecord> {
        items.iter().find_map(|item| match item {
            ParsedItem::Record(record)
                if record.signature.as_str() == "LIGH" && record.form_id & 0x00ff_ffff == local =>
            {
                Some(record)
            }
            ParsedItem::Group(group) => find_light(&group.children, local),
            _ => None,
        })
    }

    const OUTPUT: &str = "LegacyLightAttenuation.esp";
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    for (index, source_game) in [Game::Fnv, Game::Fo3].into_iter().enumerate() {
        let mut record = make_record("LIGH", &interner);
        record.form_key =
            FormKey::parse(&format!("{:06X}@{OUTPUT}", 0x800 + index), &interner).unwrap();
        let editor_id = interner.intern(&format!("LightAttenuation{index}"));
        record.eid = Some(editor_id);
        push_field(&mut record, "EDID", FieldValue::String(editor_id));
        push_field(&mut record, "OBND", FieldValue::Bytes(vec![0; 12].into()));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(bytes_from_hex(TEMP_LIGHT_01_DATA).into()),
        );
        push_field(&mut record, "FNAM", FieldValue::Float(1.1));
        let translator = Translator::new(source_game, Game::Fo4).unwrap();
        let mut ctx = make_ctx(&interner);
        translator.pre_translate(&mut ctx, &mut record).unwrap();
        let TranslateResult::Translated(mut translated) = translator.translate(&record, &interner)
        else {
            panic!("{source_game:?} LIGH must translate");
        };
        translator
            .post_translate(&mut ctx, &mut translated)
            .unwrap();
        let TargetRecordNormalization::Keep(normalized) =
            TargetRecordNormalizer::target_only_with_interner(&schema, &interner)
                .normalize(translated)
        else {
            panic!("{source_game:?} LIGH must survive target projection");
        };
        crate::target_write::add_record_native(handle, normalized, &schema, &interner).unwrap();
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
        for local in [0x800, 0x801] {
            let record = find_light(&slot.parsed.root_items, local).unwrap();
            let data = &record
                .subrecords
                .iter()
                .find(|field| field.signature.as_str() == "DATA")
                .unwrap()
                .data;
            assert_fo4_light_attenuation(data);
            assert_eq!(&data[..24], &bytes_from_hex(TEMP_LIGHT_01_DATA)[..24]);
            let fade = &record
                .subrecords
                .iter()
                .find(|field| field.signature.as_str() == "FNAM")
                .unwrap()
                .data;
            assert_eq!(f32_at(fade, 0), 1.1);
        }
    }
    assert!(plugin_handle_close_native(reopened));
}
