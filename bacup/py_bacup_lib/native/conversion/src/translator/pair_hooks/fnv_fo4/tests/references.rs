#[test]
fn pre_translate_relayouts_raw_refr_xrmr() {
    let interner = StringInterner::new();
    let room = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
    let mut record = make_record("REFR", &interner);
    push_field(
        &mut record,
        "XRMR",
        FieldValue::Bytes(smallvec::smallvec![2, 0, 0xAA, 0xBB]),
    );
    push_field(&mut record, "XLRM", FieldValue::FormKey(room));

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record.fields[0].value,
        FieldValue::Bytes(smallvec::smallvec![2, 0, 1, 0])
    );
    assert_eq!(record.fields[1].sig.as_str(), "XLRM");
}

#[test]
fn pre_translate_relayouts_structured_refr_xrmr() {
    let interner = StringInterner::new();
    let mut record = make_record("REFR", &interner);
    push_field(
        &mut record,
        "XRMR",
        FieldValue::Struct(vec![
            (interner.intern("linked_rooms_count"), FieldValue::Uint(3)),
            (interner.intern("unknown_u8_1"), FieldValue::Uint(0xAA)),
            (interner.intern("unknown_u8_2"), FieldValue::Uint(0xBB)),
        ]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::Struct(fields) = &record.fields[0].value else {
        panic!("expected structured XRMR");
    };
    assert_eq!(
        fields
            .iter()
            .map(|(key, value)| (interner.resolve(*key).unwrap(), value.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("linked_rooms_count", FieldValue::Uint(3)),
            ("flags", FieldValue::Uint(0)),
            ("unknown_u8_2", FieldValue::Uint(1)),
            ("unknown_u8_3", FieldValue::Uint(0)),
        ]
    );
}

#[test]
fn pre_translate_drops_overflowing_refr_xrmr_row() {
    let interner = StringInterner::new();
    let room = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
    let mut record = make_record("REFR", &interner);
    push_field(&mut record, "EDID", FieldValue::None);
    push_field(
        &mut record,
        "XRMR",
        FieldValue::Bytes(smallvec::smallvec![0, 1, 0, 0]),
    );
    push_field(&mut record, "XLRM", FieldValue::FormKey(room));
    push_field(&mut record, "DATA", FieldValue::None);

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        vec!["EDID", "DATA"]
    );
}

#[test]
fn pre_translate_relayouts_raw_addn_dnam_with_safe_flags() {
    let interner = StringInterner::new();
    let mut record = make_record("ADDN", &interner);
    push_field(
        &mut record,
        "DNAM",
        FieldValue::Bytes(smallvec::smallvec![0x34, 0x12, 0xAA, 0xBB]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record.fields[0].value,
        FieldValue::Bytes(smallvec::smallvec![0x34, 0x12, 0, 0])
    );
}

#[test]
fn pre_translate_relayouts_structured_addn_dnam_with_safe_flags() {
    let interner = StringInterner::new();
    let mut record = make_record("ADDN", &interner);
    push_field(
        &mut record,
        "DNAM",
        FieldValue::Struct(vec![
            (
                interner.intern("master_particle_system_cap"),
                FieldValue::Uint(0x1234),
            ),
            (interner.intern("unknown_u8_1"), FieldValue::Uint(0xAA)),
            (interner.intern("unknown_u8_2"), FieldValue::Uint(0xBB)),
        ]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::Struct(fields) = &record.fields[0].value else {
        panic!("expected structured ADDN.DNAM");
    };
    assert_eq!(
        fields
            .iter()
            .map(|(key, value)| (interner.resolve(*key).unwrap(), value.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("master_particle_system_cap", FieldValue::Uint(0x1234)),
            ("flags", FieldValue::Uint(0)),
        ]
    );
}

// -------------------------------------------------------------------------
// Behavior 2: SCRI metadata capture
// -------------------------------------------------------------------------
