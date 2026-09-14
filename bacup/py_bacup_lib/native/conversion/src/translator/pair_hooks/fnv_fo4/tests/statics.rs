#[test]
fn post_translate_retargets_unshipped_old_world_blues_urban_statue_model() {
    let interner = StringInterner::new();
    let mut record = make_record("STAT", &interner);
    record.eid = Some(interner.intern("UrbanStatue01"));
    push_field(
        &mut record,
        "MODL",
        FieldValue::String(
            interner.intern("NVDLC03\\Architecture\\Urban\\Statues\\NVDLC03UrbanStatue01.NIF"),
        ),
    );

    FnvFo4Hook
        .post_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::String(model) = record.fields[0].value else {
        panic!("STAT MODL must remain a string");
    };
    assert_eq!(
        interner.resolve(model),
        Some("Architecture\\Urban\\Statues\\UrbanStatue01.NIF")
    );
}

#[test]
fn fo3_post_translate_adds_model_to_placed_house_chimney_static() {
    let interner = StringInterner::new();
    let mut record = make_record("STAT", &interner);
    record.eid = Some(interner.intern("HouseChimney01"));

    Fo3Fo4Hook
        .post_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let model = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"MODL")
        .expect("HouseChimney01 must receive MODL");
    let FieldValue::String(model) = model.value else {
        panic!("STAT MODL must be a string");
    };
    assert_eq!(
        interner.resolve(model),
        Some("Architecture\\Suburban\\HouseChimney01.NIF")
    );
}

#[test]
fn post_translate_leaves_unrelated_static_models_unchanged() {
    let interner = StringInterner::new();
    let mut record = make_record("STAT", &interner);
    record.eid = Some(interner.intern("UnrelatedStatic"));
    let source = "Architecture\\Test\\Unrelated.nif";
    push_field(
        &mut record,
        "MODL",
        FieldValue::String(interner.intern(source)),
    );

    FnvFo4Hook
        .post_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::String(model) = record.fields[0].value else {
        panic!("STAT MODL must remain a string");
    };
    assert_eq!(interner.resolve(model), Some(source));
}
