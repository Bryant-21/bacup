#[test]
fn pre_translate_relayouts_fnv_arma_actor_models_and_drops_source_companions() {
    let interner = StringInterner::new();
    let male = interner.intern("Armor\\Male.nif");
    let female = interner.intern("Armor\\Female.nif");
    let mut record = make_record("ARMA", &interner);
    push_field(&mut record, "EDID", FieldValue::None);
    push_field(&mut record, "MODL", FieldValue::String(male));
    push_field(
        &mut record,
        "MODT",
        FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
    );
    push_field(
        &mut record,
        "MOD2",
        FieldValue::String(interner.intern("Armor\\MaleGO.nif")),
    );
    push_field(
        &mut record,
        "MO2S",
        FieldValue::Bytes(smallvec::smallvec![1, 0, 0, 0]),
    );
    push_field(&mut record, "MOD3", FieldValue::String(female));
    push_field(
        &mut record,
        "MO3T",
        FieldValue::Struct(vec![(interner.intern("legacy"), FieldValue::Uint(1))]),
    );
    push_field(
        &mut record,
        "MOD4",
        FieldValue::String(interner.intern("Armor\\FemaleGO.nif")),
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
        vec!["EDID", "MOD2", "MOD3"]
    );
    assert_eq!(record.fields[1].value, FieldValue::String(male));
    assert_eq!(record.fields[2].value, FieldValue::String(female));
    assert!(record.warnings.iter().any(|warning| {
        interner.resolve(*warning) == Some("legacy_armor_alternate_textures_require_mswp_synthesis")
    }));
}

#[test]
fn pre_translate_relayouts_structured_fnv_arma_model_rows() {
    let interner = StringInterner::new();
    let male = interner.intern("Armor\\Male.nif");
    let female = interner.intern("Armor\\Female.nif");
    let mut record = make_record("ARMA", &interner);
    push_field(
        &mut record,
        "MODL",
        FieldValue::List(vec![FieldValue::Struct(vec![
            (interner.intern("MODL"), FieldValue::String(male)),
            (
                interner.intern("MOD2"),
                FieldValue::String(interner.intern("Armor\\MaleGO.nif")),
            ),
            (interner.intern("MOD3"), FieldValue::String(female)),
            (
                interner.intern("MOD4"),
                FieldValue::String(interner.intern("Armor\\FemaleGO.nif")),
            ),
            (
                interner.intern("MODT"),
                FieldValue::Bytes(smallvec::smallvec![1, 2]),
            ),
        ])]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record
            .fields
            .iter()
            .map(|field| (field.sig.as_str(), field.value.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("MOD2", FieldValue::String(male)),
            ("MOD3", FieldValue::String(female)),
        ]
    );
}

#[test]
fn pre_translate_relayouts_fo3_arma_actor_models() {
    let interner = StringInterner::new();
    let male = interner.intern("Armor\\Male.nif");
    let female = interner.intern("Armor\\Female.nif");
    let mut record = make_record("ARMA", &interner);
    push_field(&mut record, "MODL", FieldValue::String(male));
    push_field(&mut record, "MOD3", FieldValue::String(female));
    push_field(
        &mut record,
        "MO2S",
        FieldValue::Bytes(smallvec::smallvec![1, 0, 0, 0]),
    );

    Fo3Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record
            .fields
            .iter()
            .map(|field| (field.sig.as_str(), field.value.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("MOD2", FieldValue::String(male)),
            ("MOD3", FieldValue::String(female)),
        ]
    );
}

#[test]
fn pre_translate_drops_raw_and_structured_same_4cc_semantic_collisions() {
    let interner = StringInterner::new();
    for (record_sig, field_sig) in [
        ("MUSC", "FNAM"),
        ("INFO", "DNAM"),
        ("TERM", "PNAM"),
        ("WEAP", "NNAM"),
        ("REFR", "XRDO"),
    ] {
        let mut record = make_record(record_sig, &interner);
        push_field(
            &mut record,
            field_sig,
            FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
        );
        push_field(
            &mut record,
            field_sig,
            FieldValue::Struct(vec![(interner.intern("source"), FieldValue::Uint(1))]),
        );
        push_field(&mut record, "EDID", FieldValue::None);

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID"],
            "{record_sig}.{field_sig} must not pass through"
        );
    }
}

#[test]
fn pre_translate_preserves_collision_4ccs_in_unrelated_record_contexts() {
    let interner = StringInterner::new();
    let mut record = make_record("STAT", &interner);
    for sig in ["FNAM", "DNAM", "PNAM", "ANAM", "NNAM", "XRDO", "XRMR"] {
        push_field(
            &mut record,
            sig,
            FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
        );
    }

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        vec!["FNAM", "DNAM", "PNAM", "ANAM", "NNAM", "XRDO", "XRMR"]
    );
}
