#[test]
fn pre_translate_drops_term_snam_because_fo4_v131_expects_24_byte_sound_rows() {
    let interner = StringInterner::new();
    let mut term = make_record("TERM", &interner);
    push_field(
        &mut term,
        "EDID",
        FieldValue::String(interner.intern("Terminal")),
    );
    push_field(
        &mut term,
        "SNAM",
        FieldValue::Bytes(smallvec::smallvec![0x34, 0x12, 0, 0]),
    );
    push_field(
        &mut term,
        "SNAM",
        FieldValue::Struct(vec![(interner.intern("sound"), FieldValue::Uint(0x1234))]),
    );
    push_field(
        &mut term,
        "DNAM",
        FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut term)
        .unwrap();

    assert_eq!(
        term.fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        vec!["EDID", "DNAM"]
    );

    let mut non_term = make_record("ACTI", &interner);
    push_field(
        &mut non_term,
        "SNAM",
        FieldValue::Bytes(smallvec::smallvec![0x34, 0x12, 0, 0]),
    );
    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut non_term)
        .unwrap();
    assert_eq!(non_term.fields[0].sig.as_str(), "SNAM");
}

#[test]
fn pre_translate_maps_raw_term_submenu_rows_and_drops_unsupported_rows() {
    let interner = StringInterner::new();
    let submenu = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
    let note = FormKey::parse("005678@FalloutNV.esm", &interner).unwrap();
    let submenu_2 = FormKey::parse("009ABC@FalloutNV.esm", &interner).unwrap();
    let mut record = make_record("TERM", &interner);
    push_field(&mut record, "ISIZ", FieldValue::Uint(99));
    push_field(
        &mut record,
        "ITXT",
        FieldValue::String(interner.intern("Submenu")),
    );
    push_field(
        &mut record,
        "RNAM",
        FieldValue::String(interner.intern("Loading")),
    );
    push_field(
        &mut record,
        "ANAM",
        FieldValue::Bytes(smallvec::smallvec![2]),
    );
    push_field(&mut record, "ITID", FieldValue::Uint(99));
    push_field(&mut record, "TNAM", FieldValue::FormKey(submenu));
    push_field(
        &mut record,
        "ITXT",
        FieldValue::String(interner.intern("Read note")),
    );
    push_field(&mut record, "ANAM", FieldValue::Uint(1));
    push_field(&mut record, "ITID", FieldValue::Uint(88));
    push_field(&mut record, "INAM", FieldValue::FormKey(note));
    push_field(
        &mut record,
        "ITXT",
        FieldValue::String(interner.intern("Submenu 2")),
    );
    push_field(&mut record, "ANAM", FieldValue::Uint(0));
    push_field(&mut record, "TNAM", FieldValue::FormKey(submenu_2));

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ISIZ", "ITXT", "RNAM", "ANAM", "ITID", "TNAM", "ITXT", "ANAM", "ITID", "TNAM"
        ]
    );
    assert_eq!(record.fields[0].value, FieldValue::Uint(2));
    assert_eq!(record.fields[3].value, FieldValue::Uint(6));
    assert_eq!(record.fields[4].value, FieldValue::Uint(1));
    assert_eq!(record.fields[7].value, FieldValue::Uint(4));
    assert_eq!(record.fields[8].value, FieldValue::Uint(2));
}

#[test]
fn pre_translate_maps_structured_term_submenu_rows() {
    let interner = StringInterner::new();
    let submenu = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
    let note = FormKey::parse("005678@FalloutNV.esm", &interner).unwrap();
    let submenu_2 = FormKey::parse("009ABC@FalloutNV.esm", &interner).unwrap();
    let mut record = make_record("TERM", &interner);
    push_field(&mut record, "ISIZ", FieldValue::Uint(99));
    push_field(
        &mut record,
        "ITXT",
        FieldValue::List(vec![
            FieldValue::Struct(vec![
                (
                    interner.intern("ITXT"),
                    FieldValue::String(interner.intern("Submenu")),
                ),
                (interner.intern("ANAM"), FieldValue::Uint(0)),
                (interner.intern("ITID"), FieldValue::Uint(99)),
                (interner.intern("TNAM"), FieldValue::FormKey(submenu)),
            ]),
            FieldValue::Struct(vec![
                (
                    interner.intern("ITXT"),
                    FieldValue::String(interner.intern("Read note")),
                ),
                (interner.intern("ANAM"), FieldValue::Uint(1)),
                (interner.intern("ITID"), FieldValue::Uint(88)),
                (interner.intern("INAM"), FieldValue::FormKey(note)),
            ]),
            FieldValue::Struct(vec![
                (
                    interner.intern("ITXT"),
                    FieldValue::String(interner.intern("Submenu 2")),
                ),
                (interner.intern("ANAM"), FieldValue::Uint(2)),
                (interner.intern("TNAM"), FieldValue::FormKey(submenu_2)),
            ]),
        ]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(record.fields[0].sig.as_str(), "ISIZ");
    assert_eq!(record.fields[0].value, FieldValue::Uint(2));
    let FieldValue::List(items) = &record.fields[1].value else {
        panic!("expected structured menu item list");
    };
    assert_eq!(items.len(), 2);
    let FieldValue::Struct(fields) = &items[0] else {
        panic!("expected structured menu item");
    };
    assert_eq!(
        fields
            .iter()
            .map(|(key, _)| interner.resolve(*key).unwrap())
            .collect::<Vec<_>>(),
        vec!["ITXT", "ANAM", "ITID", "TNAM"]
    );
    assert_eq!(fields[1].1, FieldValue::Uint(4));
    assert_eq!(fields[2].1, FieldValue::Uint(1));
    let FieldValue::Struct(fields) = &items[1] else {
        panic!("expected second structured menu item");
    };
    assert_eq!(fields[1].1, FieldValue::Uint(6));
    assert_eq!(fields[2].1, FieldValue::Uint(2));
}

#[test]
fn pre_translate_removes_term_count_and_item_ids_when_all_rows_drop() {
    let interner = StringInterner::new();
    let note = FormKey::parse("005678@FalloutNV.esm", &interner).unwrap();
    let mut record = make_record("TERM", &interner);
    push_field(&mut record, "EDID", FieldValue::None);
    push_field(&mut record, "ISIZ", FieldValue::Uint(1));
    push_field(
        &mut record,
        "ITXT",
        FieldValue::String(interner.intern("Read note")),
    );
    push_field(&mut record, "ANAM", FieldValue::Uint(1));
    push_field(&mut record, "ITID", FieldValue::Uint(77));
    push_field(&mut record, "INAM", FieldValue::FormKey(note));

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        vec!["EDID"]
    );
}
