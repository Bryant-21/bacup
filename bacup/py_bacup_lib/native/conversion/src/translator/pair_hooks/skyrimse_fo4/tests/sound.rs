fn sopm_record(interner: &StringInterner) -> Record {
    Record::new(
        SigCode::from_str("SOPM").unwrap(),
        FormKey::parse("07E5DC@Skyrim_Merged.esm", interner).unwrap(),
    )
}

fn push_bytes(record: &mut Record, sig: &str, bytes: &[u8]) {
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str(sig).unwrap(),
        value: FieldValue::Bytes(smallvec::SmallVec::from_slice(bytes)),
    });
}

fn source_attenuation(min_distance: f32, max_distance: f32) -> Vec<u8> {
    let mut source = vec![0x80, 0x9D, 0xFA, 0];
    source.extend_from_slice(&min_distance.to_le_bytes());
    source.extend_from_slice(&max_distance.to_le_bytes());
    source.extend_from_slice(&[100, 50, 20, 5, 0, 0, 0, 0]);
    source
}

fn run_pre_translate(record: &mut Record, interner: &StringInterner) {
    SkyrimSeFo4Hook
        .pre_translate(&mut PairCtx { interner }, record)
        .unwrap();
}

#[test]
fn converts_modern_hrtf_sopm_attenuation() {
    let interner = StringInterner::new();
    let mut record = sopm_record(&interner);
    push_bytes(&mut record, "NAM1", &[1, 0, 0, 30]);
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MNAM").unwrap(),
        value: FieldValue::Uint(0),
    });
    push_bytes(&mut record, "ANAM", &source_attenuation(150.0, 1800.0));

    run_pre_translate(&mut record, &interner);

    assert_eq!(
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        ["NAM1", "MNAM", "VNAM", "ATTN"]
    );
    let FieldValue::Bytes(attenuation) = &record.fields[3].value else {
        panic!("FO4 attenuation should be raw bytes");
    };
    assert_eq!(&attenuation[0..4], &0.0_f32.to_le_bytes());
    assert_eq!(&attenuation[4..8], &0.0_f32.to_le_bytes());
    assert_eq!(&attenuation[8..12], &150.0_f32.to_le_bytes());
    assert_eq!(&attenuation[12..16], &1800.0_f32.to_le_bytes());
    assert_eq!(&attenuation[16..24], &[0, 50, 80, 95, 50, 20, 5, 0]);
}

#[test]
fn orders_modern_defined_speaker_sopm_for_fo4() {
    let interner = StringInterner::new();
    let mut record = sopm_record(&interner);
    push_bytes(&mut record, "NAM1", &[1, 0, 0, 70]);
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MNAM").unwrap(),
        value: FieldValue::Uint(1),
    });
    let output_values = [
        100, 100, 0, 0, 50, 50, 50, 50, 100, 0, 0, 0, 100, 0, 100, 0, 0, 100, 0, 0, 0, 100, 0, 100,
    ];
    push_bytes(&mut record, "ONAM", &output_values);
    push_bytes(&mut record, "ANAM", &source_attenuation(80.0, 9000.0));

    run_pre_translate(&mut record, &interner);

    assert_eq!(
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        ["NAM1", "MNAM", "VNAM", "ONAM", "ATTN"]
    );
    assert_eq!(
        record.fields[3].value,
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&output_values))
    );
}

#[test]
fn converts_exact_legacy_defined_speaker_sopm() {
    let interner = StringInterner::new();
    let mut record = sopm_record(&interner);
    push_bytes(&mut record, "FNAM", &[1, 0, 0, 0]);
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MNAM").unwrap(),
        value: FieldValue::Uint(1),
    });
    push_bytes(&mut record, "CNAM", &[2, 0, 0, 0]);
    push_bytes(
        &mut record,
        "SNAM",
        &[100, 0, 0, 40, 100, 0, 100, 0, 0, 100, 0, 40, 0, 100, 0, 100],
    );
    push_bytes(&mut record, "ANAM", &source_attenuation(800.0, 9000.0));

    run_pre_translate(&mut record, &interner);

    assert_eq!(
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        ["NAM1", "MNAM", "VNAM", "ONAM", "ATTN"]
    );
    assert_eq!(
        record.fields[0].value,
        FieldValue::Bytes(smallvec::smallvec![1, 0, 0, 0])
    );
    assert_eq!(
        record.fields[3].value,
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[
            100, 100, 0, 40, 50, 50, 50, 50, 100, 0, 0, 40, 100, 0, 100, 0, 0, 100, 0, 40, 0, 100,
            0, 100,
        ]))
    );
}

#[test]
fn converts_exact_legacy_defined_speaker_sopm_without_lfe() {
    let interner = StringInterner::new();
    let mut record = sopm_record(&interner);
    push_bytes(&mut record, "FNAM", &[1, 0, 0, 0]);
    record_type(&mut record, 1);
    push_bytes(&mut record, "CNAM", &[2, 0, 0, 0]);
    push_bytes(
        &mut record,
        "SNAM",
        &[100, 0, 0, 0, 100, 0, 100, 0, 0, 100, 0, 0, 0, 100, 0, 100],
    );
    push_bytes(&mut record, "ANAM", &source_attenuation(800.0, 9000.0));

    run_pre_translate(&mut record, &interner);

    assert_eq!(
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        ["NAM1", "MNAM", "VNAM", "ONAM", "ATTN"]
    );
    assert_eq!(
        record.fields[3].value,
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[
            100, 100, 0, 0, 50, 50, 50, 50, 100, 0, 0, 0, 100, 0, 100, 0, 0, 100, 0, 0, 0, 100, 0,
            100,
        ]))
    );
}

#[test]
fn full_translator_preserves_live_legacy_defined_speaker_sopm() {
    let interner = StringInterner::new();
    let translator = Translator::new(Game::SkyrimSe, Game::Fo4).unwrap();
    let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let form_key = FormKey::parse("0B4247@Skyrim_Merged.esm", &interner).unwrap();
    let source = ParsedRecord {
        signature: "SOPM".into(),
        form_id: 0x000B_4247,
        flags: 0,
        version_control: 2_579_995,
        form_version: Some(30),
        version2: Some(1),
        subrecords: vec![
            parsed_subrecord("EDID", b"SOMStereoRad09000DragonCrashLand\0"),
            parsed_subrecord("FNAM", &[1, 0, 0, 0]),
            parsed_subrecord("MNAM", &[1, 0, 0, 0]),
            parsed_subrecord("CNAM", &[2, 0, 0, 0]),
            parsed_subrecord(
                "SNAM",
                &[100, 0, 0, 40, 100, 0, 100, 0, 0, 100, 0, 40, 0, 100, 0, 100],
            ),
            parsed_subrecord(
                "ANAM",
                &[
                    0xF4, 0xBD, 0xEC, 0x00, 0x00, 0x00, 0x48, 0x44, 0x00, 0xA0, 0x0C, 0x46, 0x64,
                    0x4B, 0x32, 0x19, 0x00, 0x00, 0x00, 0x00,
                ],
            ),
        ],
        raw_payload: None,
        parse_error: None,
    };
    let mut record = decode_record_from_parsed(
        &source,
        &form_key,
        &source_schema,
        &[],
        "Skyrim_Merged.esm",
        None,
        false,
        &interner,
    )
    .unwrap();
    assert_eq!(
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        ["EDID", "FNAM", "MNAM", "CNAM", "SNAM", "ANAM"]
    );
    assert_eq!(record.fields[2].value, FieldValue::Uint(1));

    translator
        .pre_translate(
            &mut PairCtx {
                interner: &interner,
            },
            &mut record,
        )
        .unwrap();
    assert_eq!(
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        ["EDID", "NAM1", "MNAM", "VNAM", "ONAM", "ATTN"]
    );

    let mut translated = match translator.translate(&record, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated SOPM, got {other:?}"),
    };
    translator
        .post_translate(
            &mut PairCtx {
                interner: &interner,
            },
            &mut translated,
        )
        .unwrap();
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut translated,
        )
        .unwrap();

    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let normalized = match (TargetRecordNormalizer {
        target_schema: &target_schema,
        source_record_def: source_schema.record_def("SOPM"),
        interner: Some(&interner),
    })
    .normalize(translated)
    {
        TargetRecordNormalization::Keep(record) => record,
        TargetRecordNormalization::DropUnsupportedRecord => {
            panic!("FO4 target normalization should keep SOPM")
        }
    };

    assert_eq!(
        normalized
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        ["EDID", "NAM1", "MNAM", "VNAM", "ONAM", "ATTN"]
    );
    assert_eq!(
        normalized.eid.and_then(|eid| interner.resolve(eid)),
        Some("SOMStereoRad09000DragonCrashLand")
    );
}

fn parsed_subrecord(signature: &str, data: &[u8]) -> ParsedSubrecord {
    ParsedSubrecord {
        signature: signature.into(),
        data: Bytes::copy_from_slice(data),
        semantic_type: None,
    }
}

#[test]
fn leaves_player_first_person_sopm_unchanged() {
    let interner = StringInterner::new();
    let mut record = sopm_record(&interner);
    push_bytes(&mut record, "NAM1", &[2, 0, 0, 80]);
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MNAM").unwrap(),
        value: FieldValue::Uint(1),
    });
    push_bytes(&mut record, "ONAM", &[100; 24]);
    let original = record.fields.clone();

    run_pre_translate(&mut record, &interner);

    assert_eq!(record.fields, original);
}

#[test]
fn leaves_malformed_sopm_attenuation_unchanged() {
    let interner = StringInterner::new();
    let mut modern = sopm_record(&interner);
    push_bytes(&mut modern, "NAM1", &[1, 0, 0, 30]);
    record_type(&mut modern, 0);
    push_bytes(&mut modern, "ANAM", &[1, 0, 0, 0]);
    let modern_original = modern.fields.clone();

    let mut legacy = sopm_record(&interner);
    push_bytes(&mut legacy, "FNAM", &[1, 0, 0, 0]);
    record_type(&mut legacy, 1);
    push_bytes(&mut legacy, "CNAM", &[2, 0, 0, 0]);
    push_bytes(&mut legacy, "SNAM", &[0; 16]);
    push_bytes(&mut legacy, "ANAM", &source_attenuation(800.0, 9000.0));
    let legacy_original = legacy.fields.clone();

    run_pre_translate(&mut modern, &interner);
    run_pre_translate(&mut legacy, &interner);

    assert_eq!(modern.fields, modern_original);
    assert_eq!(legacy.fields, legacy_original);
}

fn record_type(record: &mut Record, value: u64) {
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MNAM").unwrap(),
        value: FieldValue::Uint(value),
    });
}
