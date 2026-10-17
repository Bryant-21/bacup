fn imgs_floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn skyrim_imagespace_cloudy_exposure_survives_translation_and_save_reopen() {
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let cinematic = imgs_floats(&[1.6, 1.15, 1.5]);
    let tint = imgs_floats(&[0.5625, 0.88235295, 0.80784315, 0.7176471]);
    let mut dof = imgs_floats(&[0.5, 20000.0, 20000.0, 22.0]);
    let source = ParsedRecord {
        signature: "IMGS".into(),
        form_id: 0x012F87,
        flags: 0,
        version_control: 0,
        form_version: Some(44),
        version2: Some(0),
        subrecords: vec![
            parsed_subrecord("EDID", b"ISSkyrimCloudyDAY\0"),
            parsed_subrecord(
                "HNAM",
                &imgs_floats(&[40.0, 7.0, 0.8, 3.0, 0.7, 1.05, 3.0, 0.16, 5.0]),
            ),
            parsed_subrecord("CNAM", &cinematic),
            parsed_subrecord("TNAM", &tint),
            parsed_subrecord("DNAM", &dof),
        ],
        raw_payload: None,
        parse_error: None,
    };
    let mut record = decode_record_from_parsed(
        &source,
        &FormKey::parse("012F87@Skyrim.esm", &interner).unwrap(),
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
    let TranslateResult::Translated(mut record) = translator.translate(&record, &interner) else {
        panic!("translate IMGS")
    };
    translator.post_translate(&mut ctx, &mut record).unwrap();
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut record,
        )
        .unwrap();
    let TargetRecordNormalization::Keep(mut record) = (TargetRecordNormalizer {
        target_schema: &target_schema,
        source_record_def: source_schema.record_def("IMGS"),
        interner: Some(&interner),
    })
    .normalize(record) else {
        panic!("retain IMGS")
    };
    const OUTPUT: &str = "SkyrimExposure.esp";
    record.form_key = FormKey::parse(&format!("000800@{OUTPUT}"), &interner).unwrap();
    let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    crate::target_write::add_record_native(handle, record, &target_schema, &interner).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let handle =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let group = store[&handle]
            .parsed
            .root_items
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group) if group.label == *b"IMGS" => Some(group),
                _ => None,
            })
            .unwrap();
        let record = group
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Record(record) => Some(record),
                _ => None,
            })
            .unwrap();
        let payload = |sig| {
            record
                .subrecords
                .iter()
                .find(|entry| entry.signature.as_str() == sig)
                .unwrap()
                .data
                .as_ref()
        };
        assert_eq!(
            payload("HNAM"),
            imgs_floats(&[40.0, 0.02, 0.8, 3.0, 10.0, 0.5, 3.0, 0.16, 0.18])
        );
        assert_eq!(payload("CNAM"), cinematic);
        assert_eq!(payload("TNAM"), tint);
        dof.resize(24, 0);
        assert_eq!(payload("DNAM"), dof);
    }
    assert!(plugin_handle_close_native(handle));
}

#[test]
fn skyrim_imagespace_handles_old_enam_and_absent_or_short_dof() {
    let interner = StringInterner::new();
    for dof_size in [0, 12, 16] {
        let mut record = Record::new(
            SigCode(*b"IMGS"),
            FormKey::parse("000160@Skyrim.esm", &interner).unwrap(),
        );
        let legacy = hex::decode("000040400000e0409a99193f0000003f9a99193e6666e63f0000c03f6666663f0000c03fcdcc8c3f00000000000000000000000000000000").unwrap();
        push_bytes(&mut record, "ENAM", &legacy);
        let dof = imgs_floats(&[0.25, 1200.0, 2400.0, 14.0]);
        if dof_size > 0 {
            push_bytes(&mut record, "DNAM", &dof[..dof_size]);
        }
        run_pre_translate(&mut record, &interner);
        let payload = |sig: &[u8; 4]| {
            record
                .fields
                .iter()
                .find_map(|entry| {
                    if &entry.sig.0 == sig
                        && let FieldValue::Bytes(data) = &entry.value
                    {
                        Some(data.as_slice())
                    } else {
                        None
                    }
                })
                .unwrap()
        };
        assert_eq!(
            payload(b"HNAM"),
            imgs_floats(&[3.0, 0.02, 0.6, 0.5, 10.0, 0.5, 1.8, 1.5, 0.18])
        );
        assert_eq!(payload(b"CNAM"), &legacy[28..40]);
        assert_eq!(payload(b"TNAM"), &legacy[40..56]);
        let mut expected_dof = imgs_floats(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
        expected_dof[..dof_size].copy_from_slice(&dof[..dof_size]);
        assert_eq!(payload(b"DNAM"), expected_dof);
        assert!(record.fields.iter().all(|field| field.sig.0 != *b"ENAM"));
        let once = record.fields.clone();
        run_pre_translate(&mut record, &interner);
        assert_eq!(record.fields, once);
    }
}
