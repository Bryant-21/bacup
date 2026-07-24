#[test]
fn maps_skyrim_refr_marker_types_to_safe_fo4_icons() {
    let interner = StringInterner::new();
    let hook = SkyrimSeFo4Hook;
    let mut ctx = PairCtx {
        interner: &interner,
    };

    for (source_type, target_type) in [(4_u8, 0_u8), (49, 53), (255, 77)] {
        let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XMRK").unwrap(),
            value: FieldValue::None,
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TNAM").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![source_type, 7]),
        });

        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            record.fields[1].value,
            FieldValue::Bytes(smallvec::smallvec![target_type, 7])
        );
    }
}

#[test]
fn maps_decoded_marker_type_without_overwriting_unknown_byte() {
    let interner = StringInterner::new();
    let hook = SkyrimSeFo4Hook;
    let mut ctx = PairCtx {
        interner: &interner,
    };
    let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
    let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("XMRK").unwrap(),
        value: FieldValue::None,
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("TNAM").unwrap(),
        value: FieldValue::Struct(vec![
            (interner.intern("type"), FieldValue::Uint(4)),
            (interner.intern("unknown_u8_1"), FieldValue::Uint(9)),
        ]),
    });

    hook.pre_translate(&mut ctx, &mut record).unwrap();

    assert_eq!(
        record.fields[1].value,
        FieldValue::Struct(vec![
            (interner.intern("type"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_1"), FieldValue::Uint(9)),
        ])
    );
}

#[test]
fn leaves_non_marker_tnam_contexts_unchanged() {
    let interner = StringInterner::new();
    let hook = SkyrimSeFo4Hook;
    let mut ctx = PairCtx {
        interner: &interner,
    };

    for signature in ["REFR", "TERM"] {
        let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str(signature).unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TNAM").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![49, 3]),
        });

        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(smallvec::smallvec![49, 3])
        );
    }
}
