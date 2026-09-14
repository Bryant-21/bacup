#[test]
fn pre_translate_drops_scri_subrecord() {
    let mut interner = StringInterner::new();
    let mut record = make_record("NPC_", &mut interner);
    let scri_sym = interner.intern("SomeScript");
    push_field(&mut record, "SCRI", FieldValue::String(scri_sym));
    push_field(&mut record, "EDID", FieldValue::None);

    let hook = FnvFo4Hook;
    let mut ctx = make_ctx(&mut interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
    assert!(!sigs.contains(&"SCRI"), "SCRI should be dropped");
    assert!(sigs.contains(&"EDID"), "EDID should be preserved");
}

#[test]
fn pre_translate_is_noop_when_no_scri_field() {
    let mut interner = StringInterner::new();
    let mut record = make_record("WEAP", &mut interner);
    push_field(&mut record, "EDID", FieldValue::None);
    push_field(&mut record, "FULL", FieldValue::None);

    let hook = FnvFo4Hook;
    let mut ctx = make_ctx(&mut interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    assert_eq!(record.fields.len(), 2);
}

#[test]
fn pre_translate_drops_all_scri_fields_when_multiple_present() {
    let mut interner = StringInterner::new();
    let mut record = make_record("WEAP", &mut interner);
    let scri_sym = interner.intern("Script1");
    push_field(&mut record, "SCRI", FieldValue::String(scri_sym));
    push_field(&mut record, "EDID", FieldValue::None);
    push_field(&mut record, "SCRI", FieldValue::String(scri_sym));

    let hook = FnvFo4Hook;
    let mut ctx = make_ctx(&mut interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
    assert!(!sigs.contains(&"SCRI"));
    assert_eq!(sigs, vec!["EDID"]);
}

#[test]
fn fnv_pre_translate_clears_legacy_bptd_nam5_payloads() {
    let interner = StringInterner::new();
    let mut record = make_record("BPTD", &interner);
    push_field(&mut record, "BPTN", FieldValue::None);
    push_field(
        &mut record,
        "NAM5",
        FieldValue::Bytes(smallvec::smallvec![
            0xF0, 0xE1, 0x0C, 0x68, 0xFB, 0xF4, 0x02, 0x37,
        ]),
    );
    push_field(&mut record, "BPND", FieldValue::None);
    push_field(
        &mut record,
        "NAM5",
        FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        vec!["BPTN", "NAM5", "BPND", "NAM5"]
    );
    assert!(
        record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"NAM5")
            .all(|field| matches!(&field.value, FieldValue::Bytes(bytes) if bytes.is_empty()))
    );
}

#[test]
fn fo3_pre_translate_clears_legacy_bptd_nam5_payloads_only_from_bptd() {
    let interner = StringInterner::new();
    let mut bptd = make_record("BPTD", &interner);
    push_field(
        &mut bptd,
        "NAM5",
        FieldValue::Bytes(smallvec::smallvec![0xAA, 0xBB]),
    );
    Fo3Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut bptd)
        .unwrap();
    assert!(matches!(
        &bptd.fields[0].value,
        FieldValue::Bytes(bytes) if bytes.is_empty()
    ));

    let mut worldspace = make_record("WRLD", &interner);
    push_field(
        &mut worldspace,
        "NAM5",
        FieldValue::Bytes(smallvec::smallvec![0xAA, 0xBB]),
    );
    Fo3Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut worldspace)
        .unwrap();
    assert_eq!(worldspace.fields.len(), 1);
}

#[test]
fn pre_translate_drops_fnv_debr_legacy_modt_rows() {
    let interner = StringInterner::new();
    let mut record = make_record("DEBR", &interner);
    push_field(
        &mut record,
        "DATA",
        FieldValue::Bytes(smallvec::smallvec![50, b'a', b'.', b'n', b'i', b'f', 0, 1]),
    );
    push_field(
        &mut record,
        "MODT",
        FieldValue::Bytes(smallvec::smallvec![0x60, 0x0f, 0xf3, 0x85]),
    );
    push_field(
        &mut record,
        "DATA",
        FieldValue::Struct(vec![(interner.intern("percentage"), FieldValue::Uint(50))]),
    );
    push_field(
        &mut record,
        "MODT",
        FieldValue::Bytes(smallvec::smallvec![1, 2]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        vec!["DATA", "DATA"]
    );
    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!("raw debris row must remain bytes")
    };
    let target = std::str::from_utf8(&bytes[1..bytes.len() - 2]).unwrap();
    assert!(target.starts_with("b21_fnvfo3_debr_"));
    assert!(target.ends_with("\\a.nif"));
}

#[test]
fn capture_scri_target_returns_script_name_when_present() {
    let mut interner = StringInterner::new();
    let mut record = make_record("NPC_", &mut interner);
    let scri_sym = interner.intern("MyCustomScript");
    push_field(&mut record, "SCRI", FieldValue::String(scri_sym));

    let result = FnvFo4Hook::capture_scri_target(&record, &interner);
    assert_eq!(result.as_deref(), Some("MyCustomScript"));
}

#[test]
fn capture_scri_target_renders_formkey_for_live_fnv_npc_links() {
    let interner = StringInterner::new();
    for (npc_local, scpt_local) in [(0x1300F0, 0x166305), (0x123193, 0x123191)] {
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse(&format!("{npc_local:06X}@FalloutNV.esm"), &interner).unwrap(),
        );
        let script = FormKey::parse(&format!("{scpt_local:06X}@FalloutNV.esm"), &interner).unwrap();
        push_field(&mut record, "SCRI", FieldValue::FormKey(script));

        assert_eq!(
            FnvFo4Hook::capture_scri_target(&record, &interner),
            Some(format!("{scpt_local:06X}:FalloutNV.esm")),
            "NPC {npc_local:06X} must retain its SCPT link"
        );
    }
}

#[test]
fn capture_scri_target_returns_none_when_scri_absent() {
    let mut interner = StringInterner::new();
    let mut record = make_record("NPC_", &mut interner);
    push_field(&mut record, "EDID", FieldValue::None);

    let result = FnvFo4Hook::capture_scri_target(&record, &interner);
    assert!(result.is_none());
}

#[test]
fn capture_scri_target_returns_none_when_scri_is_empty_string() {
    let mut interner = StringInterner::new();
    let mut record = make_record("NPC_", &mut interner);
    let scri_sym = interner.intern("   ");
    push_field(&mut record, "SCRI", FieldValue::String(scri_sym));

    let result = FnvFo4Hook::capture_scri_target(&record, &interner);
    assert!(result.is_none());
}

#[test]
fn capture_scri_target_rejects_non_string_non_formkey_values() {
    let mut interner = StringInterner::new();
    for value in [
        FieldValue::Int(42),
        FieldValue::Bytes(smallvec::smallvec![0x05, 0x63, 0x16, 0x00]),
    ] {
        let mut record = make_record("NPC_", &mut interner);
        push_field(&mut record, "SCRI", value);
        assert!(FnvFo4Hook::capture_scri_target(&record, &interner).is_none());
    }
}

#[test]
fn pre_translate_drops_only_the_proven_fnv_npc_tail_collisions() {
    let interner = StringInterner::new();

    for npc_local in [0x123193, 0x1300F0] {
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse(&format!("{npc_local:06X}@FalloutNV.esm"), &interner).unwrap(),
        );
        push_field(&mut record, "NAM4", FieldValue::Uint(6));
        push_field(&mut record, "NAM6", FieldValue::Float(1.0));
        push_field(&mut record, "NAM7", FieldValue::Float(1.0));

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(
            !record
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "NAM4"),
            "NPC {npc_local:06X} must not map FNV Impact Material to FO4 HeightMax"
        );
        assert!(
            !record
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "NAM7"),
            "NPC {npc_local:06X} must not map FNV weight to FO4 Unused"
        );
        assert!(
            record
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "NAM6"),
            "NPC {npc_local:06X} keeps its compatible scalar height"
        );
    }
}

// -------------------------------------------------------------------------
// post_translate / synthesize_records
// -------------------------------------------------------------------------

#[test]
fn synthesize_records_returns_empty() {
    let mut interner = StringInterner::new();
    let hook = FnvFo4Hook;
    let mut ctx = make_ctx(&mut interner);
    assert!(hook.synthesize_records(&mut ctx).is_empty());
}
