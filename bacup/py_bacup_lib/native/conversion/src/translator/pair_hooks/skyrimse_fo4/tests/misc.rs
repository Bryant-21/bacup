#[test]
fn drops_skyrim_debr_legacy_72_byte_modt() {
    let interner = StringInterner::new();
    let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
    let mut record = Record::new(SigCode::from_str("DEBR").unwrap(), form_key);
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("DATA").unwrap(),
        value: FieldValue::Struct(vec![(
            interner.intern("model_file_name"),
            FieldValue::String(interner.intern("Effects\\IceShard.nif")),
        )]),
    });
    let mut legacy = smallvec::SmallVec::<[u8; 32]>::from_slice(&[0u8; 72]);
    legacy[..4].copy_from_slice(&0x85f3_0f60_u32.to_le_bytes());
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MODT").unwrap(),
        value: FieldValue::Bytes(legacy),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MODT").unwrap(),
        value: FieldValue::Bytes(smallvec::smallvec![1, 2, 3]),
    });

    SkyrimSeFo4Hook
        .pre_translate(
            &mut PairCtx {
                interner: &interner,
            },
            &mut record,
        )
        .unwrap();

    assert_eq!(record.fields.len(), 1);
    assert_eq!(record.fields[0].sig.as_str(), "DATA");
}

#[test]
fn drops_all_skyrim_vmad_before_fo4_translation() {
    let interner = StringInterner::new();
    let form_key = FormKey::parse("03ACDB@Skyrim_Merged.esm", &interner).unwrap();
    let mut quest = Record::new(SigCode::from_str("QUST").unwrap(), form_key);
    quest.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("EDID").unwrap(),
        value: FieldValue::String(interner.intern("Caravans")),
    });
    quest.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("VMAD").unwrap(),
        value: FieldValue::Bytes(smallvec::smallvec![5, 0, 2, 0, 1, 0]),
    });

    SkyrimSeFo4Hook
        .pre_translate(
            &mut PairCtx {
                interner: &interner,
            },
            &mut quest,
        )
        .unwrap();

    assert_eq!(quest.fields.len(), 1);
    assert_eq!(quest.fields[0].sig.as_str(), "EDID");

    let mut activator = Record::new(
        SigCode::from_str("ACTI").unwrap(),
        FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap(),
    );
    activator.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("VMAD").unwrap(),
        value: FieldValue::Bytes(smallvec::smallvec![5, 0, 2, 0, 0, 0]),
    });

    SkyrimSeFo4Hook
        .pre_translate(
            &mut PairCtx {
                interner: &interner,
            },
            &mut activator,
        )
        .unwrap();

    assert!(activator.fields.is_empty());
}
