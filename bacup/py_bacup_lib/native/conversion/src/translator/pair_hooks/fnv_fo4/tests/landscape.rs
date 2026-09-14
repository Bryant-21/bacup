fn assert_ltex_layout<H: PairHook>(hook: H, source_material: u8, target_material: u32) {
    let interner = StringInterner::new();
    let mut record = make_record("LTEX", &interner);
    push_field(
        &mut record,
        "HNAM",
        FieldValue::Bytes(smallvec::smallvec![source_material, 30, 25]),
    );
    push_field(&mut record, "SNAM", FieldValue::Uint(30));

    hook.pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(record.fields[0].sig.as_str(), "MNAM");
    let target = FormKey::parse(&format!("{target_material:06X}@Fallout4.esm"), &interner).unwrap();
    assert_eq!(record.fields[0].value, FieldValue::FormKey(target));
    assert_eq!(record.fields[1].sig.as_str(), "HNAM");
    assert_eq!(raw_field(&record, "HNAM"), &[30, 25]);
    assert_eq!(record.fields[2].sig.as_str(), "SNAM");
}

#[test]
fn fnv_ltex_splits_material_from_havok_data() {
    for (source, target) in [
        (0, 0x012F34),
        (2, 0x012F38),
        (4, 0x012F46),
        (10, 0x012F36),
        (18, 0x012F3E),
        (19, 0x055F39),
    ] {
        assert_ltex_layout(FnvFo4Hook, source, target);
    }
}

#[test]
fn fo3_ltex_uses_the_same_legacy_layout() {
    assert_ltex_layout(Fo3Fo4Hook, 19, 0x055F39);
}

fn land_layer_header(layer: i16) -> FieldValue {
    let mut bytes = vec![0; 8];
    bytes[6..8].copy_from_slice(&layer.to_le_bytes());
    FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes))
}

fn land_alpha(opacity: f32) -> FieldValue {
    let mut bytes = vec![0; 8];
    bytes[4..8].copy_from_slice(&opacity.to_le_bytes());
    FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes))
}

fn land_alpha_value(record: &Record, index: usize) -> f32 {
    let FieldValue::Bytes(bytes) = &record.fields[index].value else {
        panic!("VTXT should remain bytes");
    };
    f32::from_le_bytes(bytes[4..8].try_into().unwrap())
}

#[test]
fn fnv_land_preserves_fo4_late_slot_headroom_and_runtime_capacity() {
    let interner = StringInterner::new();
    let mut record = make_record("LAND", &interner);
    for layer in [0, 3, 4, 5] {
        push_field(&mut record, "ATXT", land_layer_header(layer));
        push_field(&mut record, "VTXT", land_alpha(1.0));
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
        ["ATXT", "VTXT", "ATXT", "VTXT", "ATXT", "VTXT"]
    );
    assert_eq!(land_alpha_value(&record, 1), 1.0);
    assert_eq!(land_alpha_value(&record, 3), 254.0 / 255.0);
    assert_eq!(land_alpha_value(&record, 5), 254.0 / 255.0);
}

#[test]
fn fo3_land_uses_the_same_fo4_blend_limits() {
    let interner = StringInterner::new();
    let mut record = make_record("LAND", &interner);
    push_field(&mut record, "ATXT", land_layer_header(3));
    push_field(&mut record, "VTXT", land_alpha(1.0));

    Fo3Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(land_alpha_value(&record, 1), 254.0 / 255.0);
}

#[test]
fn fnv_ltex_accepts_decoded_havok_data() {
    let interner = StringInterner::new();
    let mut record = make_record("LTEX", &interner);
    push_field(
        &mut record,
        "HNAM",
        FieldValue::Struct(vec![
            (interner.intern("material_type"), FieldValue::Uint(18)),
            (interner.intern("friction"), FieldValue::Uint(28)),
            (interner.intern("restitution"), FieldValue::Uint(24)),
        ]),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(raw_field(&record, "HNAM"), &[28, 24]);
    assert_eq!(
        record.fields[0].value,
        FieldValue::FormKey(FormKey::parse("012F3E@Fallout4.esm", &interner).unwrap())
    );
}
