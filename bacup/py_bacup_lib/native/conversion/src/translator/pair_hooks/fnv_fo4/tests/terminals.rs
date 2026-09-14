fn raw_term_condition(function_id: u32) -> FieldValue {
    let mut bytes = smallvec::smallvec![0; 32];
    bytes[8..12].copy_from_slice(&function_id.to_le_bytes());
    FieldValue::Bytes(bytes)
}

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
fn pre_translate_synthesizes_fo4_furniture_markers_for_legacy_term_models() {
    let interner = StringInterner::new();
    let cases = [
        (
            "Terminals\\Terminal01.NIF",
            "Markers\\MarkerWallTerminal3rdP.nif",
            (0.0_f32, -86.0_f32),
        ),
        (
            "Terminals\\TerminalDesk01.NIF",
            "Markers\\MarkerDeskTerminal01.nif",
            (-3.336_f32, -67.322_f32),
        ),
    ];

    for (model, expected_marker_model, (expected_x, expected_y)) in cases {
        let mut term = make_record("TERM", &interner);
        push_field(
            &mut term,
            "MODL",
            FieldValue::String(interner.intern(model)),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut term)
            .unwrap();

        let field = |sig: &str| {
            term.fields
                .iter()
                .find(|field| field.sig.as_str() == sig)
                .unwrap_or_else(|| panic!("{model} should gain {sig}"))
        };
        assert_eq!(
            field("PNAM").value,
            FieldValue::Bytes(smallvec::smallvec![0xcc, 0x4c, 0x33, 0x00])
        );
        assert_eq!(
            field("FNAM").value,
            FieldValue::Bytes(smallvec::smallvec![])
        );
        assert_eq!(field("COCT").value, FieldValue::Uint(0));
        assert_eq!(field("MNAM").value, FieldValue::Uint(0x4000_0001));
        assert_eq!(
            field("WBDT").value,
            FieldValue::Bytes(smallvec::smallvec![0])
        );
        let FieldValue::String(marker_model) = field("XMRK").value else {
            panic!("{model} marker model should be a string");
        };
        assert_eq!(interner.resolve(marker_model), Some(expected_marker_model));
        let FieldValue::Bytes(marker_parameters) = &field("SNAM").value else {
            panic!("{model} marker parameters should be bytes");
        };
        assert_eq!(marker_parameters.len(), 24);
        assert_eq!(
            f32::from_le_bytes(marker_parameters[0..4].try_into().unwrap()),
            expected_x
        );
        assert_eq!(
            f32::from_le_bytes(marker_parameters[4..8].try_into().unwrap()),
            expected_y
        );
        assert_eq!(&marker_parameters[20..24], &[0xff; 4]);
        assert_eq!(field("BSIZ").value, FieldValue::Uint(0));
    }
}

#[test]
fn pre_translate_leaves_modeless_term_without_furniture_markers() {
    let interner = StringInterner::new();
    let mut term = make_record("TERM", &interner);
    push_field(
        &mut term,
        "EDID",
        FieldValue::String(interner.intern("Submenu")),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut term)
        .unwrap();

    assert_eq!(
        term.fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        vec!["EDID"]
    );
}

#[test]
fn synthesized_term_furniture_markers_survive_fo4_target_normalization() {
    let interner = StringInterner::new();
    let mut term = make_record("TERM", &interner);
    push_field(
        &mut term,
        "MODL",
        FieldValue::String(interner.intern("Terminals\\Terminal01.NIF")),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut term)
        .unwrap();

    let target_schema = crate::schema::AuthoringSchema::for_game("fo4").expect("fo4 schema");
    let source_schema = crate::schema::AuthoringSchema::for_game("fnv").expect("fnv schema");
    let crate::target_normalize::TargetRecordNormalization::Keep(term) =
        (crate::target_normalize::TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def("TERM"),
            interner: Some(&interner),
        })
        .normalize(term)
    else {
        panic!("TERM should be supported by the FO4 target schema");
    };

    let sigs = term
        .fields
        .iter()
        .map(|field| field.sig.as_str())
        .collect::<Vec<_>>();
    for sig in [
        "PNAM", "FNAM", "COCT", "MNAM", "WBDT", "XMRK", "SNAM", "BSIZ",
    ] {
        assert!(sigs.contains(&sig), "{sig} must survive: {sigs:?}");
    }
    let field = |sig: &str| {
        term.fields
            .iter()
            .find(|field| field.sig.as_str() == sig)
            .unwrap()
    };
    assert_eq!(
        field("PNAM").value,
        FieldValue::Bytes(smallvec::smallvec![0xcc, 0x4c, 0x33, 0x00])
    );
    assert_eq!(
        field("FNAM").value,
        FieldValue::Bytes(smallvec::smallvec![])
    );
    assert_eq!(
        field("COCT").value,
        FieldValue::Bytes(smallvec::smallvec![0, 0, 0, 0])
    );
    assert_eq!(
        field("MNAM").value,
        FieldValue::Bytes(smallvec::smallvec![1, 0, 0, 0x40])
    );
    assert_eq!(
        field("WBDT").value,
        FieldValue::Bytes(smallvec::smallvec![0])
    );
    let FieldValue::String(marker_model) = field("XMRK").value else {
        panic!("marker model should remain a string");
    };
    assert_eq!(
        interner.resolve(marker_model),
        Some("Markers\\MarkerWallTerminal3rdP.nif")
    );
    let xmrk_position = sigs.iter().position(|sig| *sig == "XMRK").unwrap();
    let snam_position = sigs.iter().position(|sig| *sig == "SNAM").unwrap();
    assert!(
        snam_position > xmrk_position,
        "marker parameters must use the late SNAM slot: {sigs:?}"
    );
    let FieldValue::Bytes(marker_parameters) = &term.fields[snam_position].value else {
        panic!("marker parameters should remain bytes");
    };
    assert_eq!(marker_parameters.len(), 24);
    assert_eq!(
        f32::from_le_bytes(marker_parameters[4..8].try_into().unwrap()),
        -86.0
    );
    assert_eq!(&marker_parameters[20..24], &[0xff; 4]);
    assert_eq!(
        field("BSIZ").value,
        FieldValue::Bytes(smallvec::smallvec![0, 0, 0, 0])
    );
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
                (interner.intern("CTDA"), raw_term_condition(53)),
                (
                    interner.intern("CIS1"),
                    FieldValue::String(interner.intern("ScriptVariable")),
                ),
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

#[test]
fn pre_translate_drops_legacy_term_condition_id_collisions() {
    let interner = StringInterner::new();
    let submenu = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
    let mut record = make_record("TERM", &interner);
    push_field(
        &mut record,
        "ITXT",
        FieldValue::String(interner.intern("Submenu")),
    );
    push_field(&mut record, "CTDA", raw_term_condition(53));
    push_field(
        &mut record,
        "CIS1",
        FieldValue::String(interner.intern("ScriptVariable")),
    );
    push_field(&mut record, "CTDA", raw_term_condition(79));
    push_field(
        &mut record,
        "CIS2",
        FieldValue::String(interner.intern("QuestVariable")),
    );
    push_field(&mut record, "CTDA", raw_term_condition(420));
    push_field(
        &mut record,
        "CIS1",
        FieldValue::String(interner.intern("InvalidCondition")),
    );
    push_field(&mut record, "CTDA", raw_term_condition(46));
    push_field(
        &mut record,
        "CIS1",
        FieldValue::String(interner.intern("Keep")),
    );
    push_field(&mut record, "TNAM", FieldValue::FormKey(submenu));

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let condition_ids = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"CTDA")
        .map(|field| {
            let FieldValue::Bytes(bytes) = &field.value else {
                panic!("raw CTDA expected");
            };
            u32::from_le_bytes(bytes[8..12].try_into().unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(condition_ids, vec![46]);

    let condition_strings = record
        .fields
        .iter()
        .filter(|field| matches!(&field.sig.0, b"CIS1" | b"CIS2"))
        .map(|field| {
            let FieldValue::String(value) = field.value else {
                panic!("condition string expected");
            };
            interner.resolve(value).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(condition_strings, vec!["Keep"]);
}

#[test]
fn pre_translate_drops_legacy_condition_id_collisions_from_non_term_records() {
    let interner = StringInterner::new();

    for signature in ["MESG", "IDLE", "CPTH"] {
        let mut record = make_record(signature, &interner);
        push_field(&mut record, "CTDA", raw_term_condition(53));
        push_field(&mut record, "CTDA", raw_term_condition(79));
        push_field(&mut record, "CTDA", raw_term_condition(420));
        push_field(&mut record, "CTDA", raw_term_condition(46));

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let condition_ids = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .map(|field| {
                let FieldValue::Bytes(bytes) = &field.value else {
                    panic!("raw CTDA expected");
                };
                u32::from_le_bytes(bytes[8..12].try_into().unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(condition_ids, vec![46], "{signature}");
    }
}

#[test]
fn fo3_pre_translate_drops_legacy_condition_id_collisions() {
    let interner = StringInterner::new();
    let mut record = make_record("CPTH", &interner);
    push_field(&mut record, "CTDA", raw_term_condition(53));
    push_field(&mut record, "CTDA", raw_term_condition(46));

    Fo3Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let condition_ids = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"CTDA")
        .map(|field| {
            let FieldValue::Bytes(bytes) = &field.value else {
                panic!("raw CTDA expected");
            };
            u32::from_le_bytes(bytes[8..12].try_into().unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(condition_ids, vec![46]);
}
