#[test]
fn pre_translate_relayouts_modern_txst_decal_data_for_fo4() {
    let interner = StringInterner::new();
    let mut record = make_record("TXST", &interner);
    push_field(
        &mut record,
        "DODT",
        FieldValue::Bytes(SmallVec::from_vec(modern_txst_decal_data(0x3A))),
    );

    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!("expected raw DODT bytes");
    };
    assert_eq!(bytes.len(), 36);
    assert_eq!(f32::from_le_bytes(bytes[20..24].try_into().unwrap()), 1.0);
    assert_eq!(f32::from_le_bytes(bytes[24..28].try_into().unwrap()), 1.0);
    assert_eq!(bytes[28], 16);
    assert_eq!(bytes[29], 0x0A);
    assert_eq!(u16::from_le_bytes(bytes[30..32].try_into().unwrap()), 127);
    assert_eq!(&bytes[32..36], &[255, 255, 255, 0]);

    let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema");
    crate::translator::class_a_normalize::normalize_flags_and_enums(
        &mut record,
        &schema,
        &interner,
    );
    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!("expected raw DODT bytes");
    };
    assert_eq!(bytes[29], 0x0A);
}

#[test]
fn pre_translate_maps_fo76_alt_no_subtextures_flag() {
    let interner = StringInterner::new();
    let mut record = make_record("TXST", &interner);
    push_field(
        &mut record,
        "DODT",
        FieldValue::Bytes(SmallVec::from_vec(modern_txst_decal_data(0xF2))),
    );

    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!("expected raw DODT bytes");
    };
    assert_eq!(bytes[29], 0x0A);
}

#[test]
fn pre_translate_keeps_legacy_txst_decal_data_layout() {
    let interner = StringInterner::new();
    let mut record = make_record("TXST", &interner);
    let source: Vec<u8> = (0..36).collect();
    push_field(
        &mut record,
        "DODT",
        FieldValue::Bytes(SmallVec::from_vec(source.clone())),
    );

    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!("expected raw DODT bytes");
    };
    assert_eq!(bytes.as_slice(), source);
}

fn modern_txst_decal_data(flags: u8) -> Vec<u8> {
    let mut source = Vec::new();
    for value in [128.0_f32, 256.0, 128.0, 256.0, 64.0, 1.0] {
        source.extend_from_slice(&value.to_le_bytes());
    }
    source.extend_from_slice(&[flags, 0x00, 0x10, 0x01]);
    source
}
