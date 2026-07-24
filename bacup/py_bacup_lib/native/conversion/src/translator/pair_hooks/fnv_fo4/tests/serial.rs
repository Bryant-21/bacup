#[test]
fn serial_magic_dispatch_reports_unmapped_effect_reference() {
    let interner = StringInterner::new();
    let mut record = make_record("ALCH", &interner);
    push_field(
        &mut record,
        "EFID",
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&0x1000_u32.to_le_bytes())),
    );
    push_field(
        &mut record,
        "EFIT",
        FieldValue::Bytes(smallvec::SmallVec::from_vec(vec![0_u8; 20])),
    );
    let mut mapper = legacy_magic_mapper(&interner);
    let mut state = LegacySerialNormalizationState::default();
    let source_fk = record.form_key;

    let report = normalize_legacy_serial_record_once(
        Game::Fnv,
        Game::Fo4,
        source_fk,
        &mut record,
        &mut mapper,
        &mut state,
    )
    .expect("FNV ALCH dispatch")
    .expect("FNV ALCH normalization");
    let diagnostics = report.diagnostics(&record);
    let LegacySerialNormalizeReport::Effects(effect_report) = &report else {
        panic!("ALCH must produce an effects report");
    };
    assert_eq!(
        diagnostics.len(),
        1 + effect_report.references.len() + effect_report.enums.len()
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.warning
            && diagnostic.message.contains("UnmappedRaw")
            && diagnostic.message.contains("base_effect")
    }));
    assert!(!record.fields.iter().any(|field| field.sig.0 == *b"EFID"));
}

#[test]
fn serial_dispatch_isolated_from_fo76() {
    let interner = StringInterner::new();
    for (sig, field_sig, bytes) in [
        ("MGEF", "DATA", vec![0_u8; 72]),
        ("PERK", "PRKE", vec![2, 0, 0]),
        ("WRLD", "DATA", vec![0x80]),
    ] {
        let mut record = make_record(sig, &interner);
        push_field(
            &mut record,
            field_sig,
            FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes)),
        );
        let before = record.clone();
        let mut mapper = legacy_magic_mapper(&interner);
        let mut state = LegacySerialNormalizationState::default();
        let source_fk = record.form_key;

        assert!(
            normalize_legacy_serial_record_once(
                Game::Fo76,
                Game::Fo4,
                source_fk,
                &mut record,
                &mut mapper,
                &mut state,
            )
            .is_none()
        );
        assert_eq!(record.sig, before.sig);
        assert_eq!(record.form_key, before.form_key);
        assert_eq!(record.fields, before.fields);
    }
}

#[test]
fn serial_perk_dispatch_is_exactly_once_and_transient() {
    let interner = StringInterner::new();
    let mut record = make_record("PERK", &interner);
    push_field(
        &mut record,
        "PRKE",
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[2, 0, 0])),
    );
    push_field(
        &mut record,
        "DATA",
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[72, 3, 2])),
    );
    push_field(
        &mut record,
        "EPFT",
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1])),
    );
    push_field(
        &mut record,
        "EPFD",
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&1.25_f32.to_le_bytes())),
    );
    push_field(&mut record, "PRKF", FieldValue::None);
    let source_fk = record.form_key;
    let mut mapper = legacy_magic_mapper(&interner);
    let mut state = LegacySerialNormalizationState::default();

    let first = normalize_legacy_serial_record_once(
        Game::Fnv,
        Game::Fo4,
        source_fk,
        &mut record,
        &mut mapper,
        &mut state,
    )
    .expect("PERK dispatch")
    .expect("PERK normalization");
    let LegacySerialNormalizeReport::Perk(perk_report) = &first else {
        panic!("PERK must produce a PERK report");
    };
    assert_eq!(
        first.diagnostics(&record).len(),
        1 + perk_report.references.len() + perk_report.enums.len() + perk_report.drops.len()
    );
    let converted_fields = record.fields.clone();
    let converted_warnings = record.warnings.clone();

    let second = normalize_legacy_serial_record_once(
        Game::Fnv,
        Game::Fo4,
        source_fk,
        &mut record,
        &mut mapper,
        &mut state,
    )
    .expect("PERK redispatch")
    .expect("PERK redispatch no-op");
    assert!(matches!(
        second,
        LegacySerialNormalizeReport::PerkAlreadyNormalized
    ));
    assert_eq!(record.fields, converted_fields);
    assert_eq!(record.warnings, converted_warnings);
    assert!(record.warnings.is_empty(), "once-only guard must not leak");
}

#[test]
fn serial_perk_typed_target_is_idempotent_under_strict_mapper_rewrite() {
    let interner = StringInterner::new();
    let source_spell = FormKey {
        local: 0x1234,
        plugin: interner.intern("FalloutNV.esm"),
    };
    let target_spell = FormKey {
        local: 0x5678,
        plugin: interner.intern("Fallout4.esm"),
    };
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: "Out.esm".into(),
            source_plugin_name: "FalloutNV.esm".into(),
            target_master_names: vec!["Fallout4.esm".into()],
            resolution_mode: ResolutionMode::Strict,
            ..MapperOptions::default()
        },
        &interner,
    );
    mapper.add_mapping(source_spell, target_spell);
    let mut record = make_record("PERK", &interner);
    push_field(
        &mut record,
        "PRKE",
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1, 0, 0])),
    );
    push_field(&mut record, "DATA", FieldValue::FormKey(source_spell));
    push_field(&mut record, "PRKF", FieldValue::None);
    let source_fk = record.form_key;
    let mut state = LegacySerialNormalizationState::default();

    let report = normalize_legacy_serial_record_once(
        Game::Fnv,
        Game::Fo4,
        source_fk,
        &mut record,
        &mut mapper,
        &mut state,
    )
    .expect("PERK dispatch")
    .expect("PERK normalization");
    report.register_target_identities(&mut mapper);

    mapper
        .rewrite_record(&mut record)
        .expect("generic strict rewrite must accept normalized target FormKeys");
    assert!(record.fields.iter().any(|field| {
        field.sig.0 == *b"DATA" && field.value == FieldValue::FormKey(target_spell)
    }));
}

#[test]
fn serial_wrld_dispatch_returns_atomic_drop_diagnostic() {
    let interner = StringInterner::new();
    let mut record = make_record("WRLD", &interner);
    push_field(
        &mut record,
        "DATA",
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0x80, 0x01])),
    );
    let source_fk = record.form_key;
    let before = record.fields.clone();
    let mut mapper = legacy_magic_mapper(&interner);
    let mut state = LegacySerialNormalizationState::default();

    let diagnostic = normalize_legacy_serial_record_once(
        Game::Fnv,
        Game::Fo4,
        source_fk,
        &mut record,
        &mut mapper,
        &mut state,
    )
    .expect("WRLD dispatch")
    .expect_err("unsupported WRLD DATA must drop");

    assert!(diagnostic.warning);
    assert!(diagnostic.message.contains("UnsupportedDataValue"));
    assert_eq!(record.fields, before);
}

#[test]
fn serial_wrld_dispatch_reports_each_change_and_reference() {
    let interner = StringInterner::new();
    let mut record = make_record("WRLD", &interner);
    push_field(
        &mut record,
        "DATA",
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0x80, 0xFF, 0xFF, 0xFF])),
    );
    let source_fk = record.form_key;
    let mut mapper = legacy_magic_mapper(&interner);
    let mut state = LegacySerialNormalizationState::default();

    let report = normalize_legacy_serial_record_once(
        Game::Fnv,
        Game::Fo4,
        source_fk,
        &mut record,
        &mut mapper,
        &mut state,
    )
    .expect("WRLD dispatch")
    .expect("WRLD normalization");
    let LegacySerialNormalizeReport::Wrld(wrld_report) = &report else {
        panic!("WRLD must produce a WRLD report");
    };

    assert_eq!(
        report.diagnostics(&record).len(),
        1 + wrld_report.data_changes.len() + wrld_report.references.len()
    );
}
