#[test]
fn pre_translate_relayouts_fo76_watr_for_fo4() {
    let interner = StringInterner::new();
    let mut record = make_record("WATR", &interner);
    push_field(
        &mut record,
        "DNAM",
        FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 148])),
    );
    push_field(
        &mut record,
        "NAM2",
        FieldValue::String(interner.intern("data\\Textures\\Water\\DefaultWaterTile_n.DDS")),
    );

    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let dnam = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DNAM")
        .expect("DNAM kept");
    let FieldValue::Bytes(bytes) = &dnam.value else {
        panic!("expected raw DNAM bytes");
    };
    assert_eq!(bytes.len(), 201);
    assert_eq!(f32::from_le_bytes(bytes[100..104].try_into().unwrap()), 2430.0);

    let nam2 = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"NAM2")
        .expect("NAM2 kept");
    let FieldValue::String(sym) = nam2.value else {
        panic!("expected NAM2 string");
    };
    assert_eq!(
        interner.resolve(sym),
        Some("data\\Textures\\Water\\DefaultWaterTile.dds")
    );
}
