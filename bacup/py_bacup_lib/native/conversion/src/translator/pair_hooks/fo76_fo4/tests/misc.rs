
#[test]
fn post_translate_namespaces_raw_radio_receiver_frequency_once() {
    let interner = StringInterner::new();
    let mut record = make_record("ACTI", &interner);
    let mut receiver = vec![0_u8; 14];
    receiver[4..8].copy_from_slice(&98.2_f32.to_le_bytes());
    push_field(&mut record, "RADR", raw_bytes(&receiver));

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.post_translate(&mut ctx, &mut record).unwrap();
    let once = record.fields.clone();
    hook.post_translate(&mut ctx, &mut record).unwrap();

    let FieldValue::Bytes(receiver) = &record.fields[0].value else {
        panic!("expected raw RADR");
    };
    assert_eq!(
        f32::from_le_bytes(receiver[4..8].try_into().unwrap()),
        98.2 + FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET
    );
    assert_eq!(record.fields, once);
}

#[test]
fn post_translate_rewrites_fo76_font_aliases_in_localized_text() {
    let interner = StringInterner::new();
    let mut record = make_record("BOOK", &interner);
    push_field(
            &mut record,
            "DESC",
            FieldValue::String(interner.intern(
                "<font face='$Typewriter_Font'>typed</font> <font face='$76HandwrittenNeat_Font'>neat</font> <font face='$76HandwrittenIlliterate'>rough</font>",
            )),
        );

    Fo76Fo4Hook
        .post_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::String(text) = record.fields[0].value else {
        panic!("expected localized text");
    };
    assert_eq!(
        interner.resolve(text),
        Some(
            "<font face='$Terminal_Font'>typed</font> <font face='$HandwrittenFont'>neat</font> <font face='$HandwrittenFont'>rough</font>"
        )
    );
}

#[test]
fn post_translate_namespaces_structured_radio_receiver_frequency() {
    let interner = StringInterner::new();
    let mut record = make_record("ACTI", &interner);
    push_field(
        &mut record,
        "RADR",
        FieldValue::Struct(vec![
            (interner.intern("SoundModel"), FieldValue::Uint(0x0B5183)),
            (interner.intern("Frequency"), FieldValue::Float(80.5)),
        ]),
    );

    Fo76Fo4Hook
        .post_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::Struct(fields) = &record.fields[0].value else {
        panic!("expected structured RADR");
    };
    assert_eq!(
        fields[1].1,
        FieldValue::Float(80.5 + FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET)
    );
}

#[test]
fn pre_translate_strips_fo76_info_editor_id() {
    let interner = StringInterner::new();
    let mut record = make_record("INFO", &interner);
    push_field(&mut record, "ENAM", FieldValue::None);
    push_field(&mut record, "EDID", raw_bytes(b"FO76OnlyInfoEditorId\0"));

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let sigs: Vec<&str> = record
        .fields
        .iter()
        .map(|field| field.sig.as_str())
        .collect();
    assert_eq!(sigs, vec!["ENAM"]);
}

#[test]
fn pre_translate_drops_only_unanchored_term_condition_groups() {
    let interner = StringInterner::new();
    let mut record = make_record("TERM", &interner);
    push_field(&mut record, "BSIZ", raw_bytes(&1_u32.to_le_bytes()));
    push_field(&mut record, "BTXT", FieldValue::None);
    push_field(&mut record, "CTDA", raw_ctda(1));
    push_field(&mut record, "CIS1", FieldValue::None);
    push_field(&mut record, "ISIZ", raw_bytes(&1_u32.to_le_bytes()));
    push_field(&mut record, "CTDA", raw_ctda(2));
    push_field(&mut record, "CIS1", FieldValue::None);
    push_field(&mut record, "CIS2", FieldValue::None);
    push_field(&mut record, "ITXT", FieldValue::None);
    push_field(&mut record, "ANAM", FieldValue::None);
    push_field(&mut record, "ITID", FieldValue::None);
    push_field(&mut record, "CTDA", raw_ctda(3));
    push_field(&mut record, "CIS2", FieldValue::None);
    push_field(&mut record, "UNAM", FieldValue::None);
    push_field(&mut record, "CTDA", raw_ctda(4));
    push_field(&mut record, "CIS1", FieldValue::None);

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let sigs: Vec<&str> = record
        .fields
        .iter()
        .map(|field| field.sig.as_str())
        .collect();
    assert_eq!(
        sigs,
        vec![
            "BSIZ", "BTXT", "CTDA", "CIS1", "ISIZ", "ITXT", "ANAM", "ITID", "CTDA", "CIS2", "UNAM",
        ]
    );
}

#[test]
fn pre_translate_maps_refr_marker_types_to_two_byte_fo4_layout() {
    let interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);

    for (source_type, target_type) in [(2_u16, 3_u8), (7, 8), (54, 58), (64, 6), (65, 4), (66, 8)] {
        let mut record = make_record("REFR", &interner);
        push_field(
            &mut record,
            "TNAM",
            raw_bytes(&u32::from(source_type).to_le_bytes()),
        );

        hook.pre_translate(&mut ctx, &mut record).unwrap();

        match &record.fields[0].value {
            FieldValue::Bytes(bytes) => assert_eq!(bytes.as_slice(), &[target_type, 0]),
            value => panic!("expected TNAM bytes, got {value:?}"),
        }
    }
}

#[test]
fn pre_translate_keeps_unrelated_acti_activation_conditions() {
    let interner = StringInterner::new();
    let mut record = make_record("ACTI", &interner);
    record.eid = Some(interner.intern("OtherSearchActivator"));
    push_field(&mut record, "CITC", raw_bytes(&1_u32.to_le_bytes()));
    push_field(&mut record, "CTDA", raw_ctda(203));

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let sigs: Vec<&str> = record
        .fields
        .iter()
        .map(|field| field.sig.as_str())
        .collect();
    assert_eq!(sigs, vec!["CITC", "CTDA"]);
}

#[test]
fn pre_translate_renames_only_furniture_marker_parameters() {
    let interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);

    let mut furniture = make_record("FURN", &interner);
    push_field(
        &mut furniture,
        "ZNAM",
        raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN]),
    );
    hook.pre_translate(&mut ctx, &mut furniture).unwrap();
    assert_eq!(furniture.fields[0].sig.as_str(), "SNAM");

    let mut terminal = make_record("TERM", &interner);
    push_field(&mut terminal, "SNAM", raw_bytes(&1_u32.to_le_bytes()));
    push_field(
        &mut terminal,
        "ZNAM",
        raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN]),
    );
    hook.pre_translate(&mut ctx, &mut terminal).unwrap();

    let sigs: Vec<&str> = terminal
        .fields
        .iter()
        .map(|field| field.sig.as_str())
        .collect();
    assert_eq!(sigs, vec!["SNAM", "ZNAM"]);
}

#[test]
fn pre_translate_drops_codv_subrecord() {
    let mut interner = StringInterner::new();
    let mut record = make_record("ARMO", &mut interner);
    push_field(&mut record, "CODV", FieldValue::None);
    push_field(
        &mut record,
        "FULL",
        FieldValue::String(interner.intern("Armor")),
    );

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&mut interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
    assert!(!sigs.contains(&"CODV"));
    assert!(sigs.contains(&"FULL"));
}

#[test]
fn pre_translate_drops_version_control_sigs() {
    let mut interner = StringInterner::new();
    let mut record = make_record("WEAP", &mut interner);
    // Drop VCTX (VersionControl) and FVER (FormVersion)
    push_field(&mut record, "VCTX", FieldValue::None);
    push_field(&mut record, "FVER", FieldValue::None);
    push_field(&mut record, "EDID", FieldValue::None);

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&mut interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
    assert!(!sigs.contains(&"VCTX"));
    assert!(!sigs.contains(&"FVER"));
    assert!(sigs.contains(&"EDID"));
}

#[test]
fn pre_translate_drops_all_global_sigs() {
    let mut interner = StringInterner::new();
    let mut record = make_record("NPC_", &mut interner);
    for sig in &[
        "VCTX", "FVER", "FL76", "FLWR", "MIID", "MAGF", "CODV", "OPDS",
    ] {
        push_field(&mut record, sig, FieldValue::None);
    }
    push_field(&mut record, "EDID", FieldValue::None);

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&mut interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    assert_eq!(record.fields.len(), 1);
    assert_eq!(record.fields[0].sig.as_str(), "EDID");
}

#[test]
fn pre_translate_converts_fo76_lvli_split_rows_to_fo4_lvlo() {
    let mut interner = StringInterner::new();
    let mut record = make_record("LVLI", &mut interner);
    let variant_sym = interner.intern("variant");
    let value_sym = interner.intern("value");
    let reference_variant = interner.intern("reference");
    push_field(&mut record, "LLCT", FieldValue::Uint(2));
    push_field(
        &mut record,
        "LVLO",
        FieldValue::Struct(vec![
            (variant_sym, FieldValue::String(reference_variant)),
            (value_sym, FieldValue::Uint(0x08E3A8)),
        ]),
    );
    push_field(&mut record, "LVIV", FieldValue::Float(2.0));
    push_field(&mut record, "LVLV", FieldValue::Float(3.0));
    push_field(&mut record, "LVLO", FieldValue::Uint(0x02C59E));
    push_field(&mut record, "LVIV", FieldValue::Float(1.0));
    push_field(&mut record, "LVLV", FieldValue::Float(1.0));

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let count = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "LLCT")
        .map(|entry| &entry.value);
    assert_eq!(count, Some(&FieldValue::Uint(2)));
    let lvlo_entries: Vec<_> = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "LVLO")
        .collect();
    assert_eq!(lvlo_entries.len(), 2);

    let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
    let record = match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
        &schema, &interner,
    )
    .normalize(record)
    {
        crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
        crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
            panic!("LVLI is supported by FO4 schema")
        }
    };
    let lvlo_entries: Vec<_> = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "LVLO")
        .collect();
    assert_eq!(lvlo_entries.len(), 2);
    let first = crate::target_write::encode_field_pub(
        lvlo_entries[0],
        schema.record_def("LVLI"),
        &interner,
    )
    .expect("first converted LVLO encodes");
    assert_eq!(first, vec![3, 0, 0, 0, 0xA8, 0xE3, 0x08, 0, 2, 0, 0, 0]);
    let second = crate::target_write::encode_field_pub(
        lvlo_entries[1],
        schema.record_def("LVLI"),
        &interner,
    )
    .expect("second converted LVLO encodes");
    assert_eq!(second, vec![1, 0, 0, 0, 0x9E, 0xC5, 0x02, 0, 1, 0, 0, 0]);
}

#[test]
fn post_translate_trims_fo76_movement_speed_data_to_fo4_ck_size() {
    let interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    let mut record = make_record("MOVT", &interner);
    let raw = (0_u8..124).collect::<Vec<_>>();
    push_field(
        &mut record,
        "SPED",
        FieldValue::Bytes(SmallVec::from_vec(raw)),
    );

    hook.post_translate(&mut ctx, &mut record).unwrap();

    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!("expected SPED bytes");
    };
    assert_eq!(bytes.len(), FO4_MOVEMENT_SPEED_DATA_LEN);
    assert_eq!(bytes.as_slice(), (0_u8..112).collect::<Vec<_>>().as_slice());
}

#[test]
fn post_translate_sets_missing_raw_light_radius_from_value() {
    let interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    let mut record = make_record("LIGH", &interner);
    let mut raw = vec![0_u8; 68];
    raw[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
        .copy_from_slice(&400_u32.to_le_bytes());
    push_field(
        &mut record,
        "DATA",
        FieldValue::Bytes(SmallVec::from_vec(raw)),
    );

    hook.post_translate(&mut ctx, &mut record).unwrap();

    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!("expected raw DATA bytes");
    };
    assert_eq!(
        u32::from_le_bytes(
            bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                .try_into()
                .unwrap()
        ),
        400
    );
    assert_eq!(bytes.len(), FO4_LIGH_DATA_LEN);
    assert_eq!(
        f32::from_le_bytes(
            bytes[FO4_LIGH_DATA_SCALAR_OFFSET..FO4_LIGH_DATA_SCALAR_OFFSET + 4]
                .try_into()
                .unwrap()
        ),
        FO4_LIGH_DEFAULT_SCALAR
    );
    assert_eq!(
        f32::from_le_bytes(
            bytes[FO4_LIGH_DATA_EXPONENT_OFFSET..FO4_LIGH_DATA_EXPONENT_OFFSET + 4]
                .try_into()
                .unwrap()
        ),
        FO4_LIGH_DEFAULT_EXPONENT
    );
    assert_eq!(
        u32::from_le_bytes(
            bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
                .try_into()
                .unwrap()
        ),
        FO4_LIGH_DEFAULT_VALUE
    );
    assert_eq!(
        f32::from_le_bytes(
            bytes[FO4_LIGH_DATA_WEIGHT_OFFSET..FO4_LIGH_DATA_WEIGHT_OFFSET + 4]
                .try_into()
                .unwrap()
        ),
        FO4_LIGH_DEFAULT_WEIGHT
    );
}

#[test]
fn pre_translate_strips_race_tints_without_dropping_conditions_or_morphs() {
    let interner = StringInterner::new();
    let mut race = make_record("RACE", &interner);
    push_field(&mut race, "EDID", FieldValue::None);
    push_field(&mut race, "ATKD", FieldValue::None);
    push_field(&mut race, "CTDA", FieldValue::None);
    push_field(&mut race, "CIS1", FieldValue::None);
    push_field(&mut race, "CIS2", FieldValue::None);
    push_field(&mut race, "HEAD", FieldValue::None);
    push_field(&mut race, "CTDA", FieldValue::None);
    push_field(&mut race, "CIS1", FieldValue::None);
    push_field(&mut race, "CIS2", FieldValue::None);
    for sig in [
        "TINL", "TTGP", "TETI", "TTEF", "CTDA", "CIS1", "CIS2", "TTET", "TTEB", "TTEC", "TTED",
        "TTGE",
    ] {
        push_field(&mut race, sig, FieldValue::None);
    }
    for sig in [
        "MPGN", "MPPC", "MPPI", "MPPN", "MPPM", "MPPT", "MPPF", "MPPK", "MPGS",
    ] {
        push_field(&mut race, sig, FieldValue::None);
    }

    let hook = Fo76Fo4Hook;
    hook.pre_translate(&mut make_ctx(&interner), &mut race)
        .unwrap();
    assert_eq!(
        race.fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        vec![
            "EDID", "ATKD", "CTDA", "CIS1", "CIS2", "HEAD", "CTDA", "CIS1", "CIS2", "MPGN", "MPPC",
            "MPPI", "MPPN", "MPPM", "MPPT", "MPPF", "MPPK", "MPGS",
        ]
    );

    let mut npc = make_record("NPC_", &interner);
    push_field(&mut npc, "QNAM", FieldValue::None);
    hook.pre_translate(&mut make_ctx(&interner), &mut npc)
        .unwrap();
    assert_eq!(npc.fields[0].sig.as_str(), "QNAM");
}

#[test]
fn post_translate_does_not_mask_idlf_on_other_records() {
    let interner = StringInterner::new();
    let mut record = make_record("PACK", &interner);
    push_field(&mut record, "IDLF", FieldValue::Uint(0x28));

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.post_translate(&mut ctx, &mut record).unwrap();

    assert_eq!(record.fields[0].value, FieldValue::Uint(0x28));
}

#[test]
fn post_translate_idlm_flag_mask_is_idempotent() {
    let interner = StringInterner::new();
    let mut record = make_record("IDLM", &interner);
    push_field(&mut record, "IDLF", FieldValue::Uint(0x28));

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.post_translate(&mut ctx, &mut record).unwrap();
    let once = record.fields.clone();
    hook.post_translate(&mut ctx, &mut record).unwrap();

    assert_eq!(record.fields, once);
    assert_eq!(record.fields[0].value, FieldValue::Uint(0x08));
}

#[test]
fn post_translate_drops_raw_regn_rdot() {
    let mut interner = StringInterner::new();
    let mut record = make_record("REGN", &mut interner);
    push_field(&mut record, "RDAT", FieldValue::None);
    push_field(
        &mut record,
        "RDOT",
        FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 456])),
    );
    push_field(&mut record, "RDWT", FieldValue::None);

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&mut interner);
    hook.post_translate(&mut ctx, &mut record).unwrap();

    let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
    assert!(sigs.contains(&"RDAT"));
    assert!(!sigs.contains(&"RDOT"));
    assert!(sigs.contains(&"RDWT"));
}

/// Build a 72-byte FO76-style LIGH DATA blob: flags @ +12, near clip @ +24,
/// flicker intensity amplitude @ +32.
fn fo76_ligh_data(flags: u32, near_clip: f32, flicker_intensity_amp: f32) -> Vec<u8> {
    let mut bytes = vec![0_u8; 72];
    bytes[12..16].copy_from_slice(&flags.to_le_bytes());
    bytes[24..28].copy_from_slice(&near_clip.to_le_bytes());
    bytes[32..36].copy_from_slice(&flicker_intensity_amp.to_le_bytes());
    bytes
}
#[test]
fn synthesize_records_returns_empty() {
    let mut interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&mut interner);
    assert!(hook.synthesize_records(&mut ctx).is_empty());
}
