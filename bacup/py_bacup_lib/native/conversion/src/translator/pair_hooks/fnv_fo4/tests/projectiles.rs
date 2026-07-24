#[test]
fn pre_translate_rebuilds_crashing_flame_projectile_data_and_drops_legacy_nam2() {
    const FLAME_PROJECTILE_ANT_DATA: &str = "8D0008000000000000803B460000204400000000000000000000000000000000000000000000000000000000CDCC4C3E0AD7233C0000C040000000000000000000000000";
    const FLAME_PROJECTILE_ANT_NAM2: &str = "B1B0106696E60762313010669BE6076273741074B3E1C96DB2B011662D9C07A132301166329C07A173651E74E527EFD8";

    let interner = StringInterner::new();
    let mut record = make_record("PROJ", &interner);
    push_field(
        &mut record,
        "DATA",
        FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes_from_hex(
            FLAME_PROJECTILE_ANT_DATA,
        ))),
    );
    push_field(
        &mut record,
        "NAM2",
        FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes_from_hex(
            FLAME_PROJECTILE_ANT_NAM2,
        ))),
    );

    let mut ctx = make_ctx(&interner);
    FnvFo4Hook.pre_translate(&mut ctx, &mut record).unwrap();

    assert!(raw_field(&record, "DATA").is_empty());
    assert!(
        record
            .fields
            .iter()
            .all(|entry| entry.sig.as_str() != "NAM2")
    );
    let dnam = raw_field(&record, "DNAM");
    assert_eq!(dnam.len(), 93);
    assert_eq!(u16_at(dnam, 0), 141);
    assert_eq!(u16_at(dnam, 2), 8);
    assert_eq!(f32_at(dnam, 4), 0.0);
    assert_eq!(f32_at(dnam, 8), 12_000.0);
    assert_eq!(f32_at(dnam, 12), 640.0);
    for offset in [16, 20, 32, 36, 52, 56, 60, 80, 84, 89] {
        assert_eq!(u32_at(dnam, offset), 0, "raw ref at offset {offset}");
    }
    assert_eq!(f32_at(dnam, 24), 0.0);
    assert_eq!(f32_at(dnam, 28), 0.0);
    assert_eq!(f32_at(dnam, 40), 0.2);
    assert_eq!(f32_at(dnam, 44), 0.01);
    assert_eq!(f32_at(dnam, 48), 6.0);
    for offset in [64, 68, 72, 76] {
        assert_eq!(f32_at(dnam, offset), 0.0, "FO4-only float at {offset}");
    }
    assert_eq!(dnam[88], 0);
}

#[test]
fn pre_translate_relayouts_84_byte_proj_and_preserves_compatible_refs() {
    let mut source = Vec::new();
    source.extend_from_slice(&0xffff_u16.to_le_bytes());
    source.extend_from_slice(&16_u16.to_le_bytes());
    for value in [1.25_f32, 2.5, 3.75] {
        source.extend_from_slice(&value.to_le_bytes());
    }
    source.extend_from_slice(&0x0011_1111_u32.to_le_bytes());
    source.extend_from_slice(&0x0022_2222_u32.to_le_bytes());
    source.extend_from_slice(&99.0_f32.to_le_bytes()); // dropped tracer chance
    source.extend_from_slice(&4.25_f32.to_le_bytes());
    source.extend_from_slice(&5.5_f32.to_le_bytes());
    source.extend_from_slice(&0x0033_3333_u32.to_le_bytes());
    source.extend_from_slice(&0x0044_4444_u32.to_le_bytes());
    for value in [6.75_f32, 7.0, 8.5] {
        source.extend_from_slice(&value.to_le_bytes());
    }
    source.extend_from_slice(&0x0055_5555_u32.to_le_bytes());
    source.extend_from_slice(&0x0066_6666_u32.to_le_bytes());
    source.extend_from_slice(&0x0077_7777_u32.to_le_bytes());
    for value in [9.0_f32, 10.0, 11.0, 12.0] {
        source.extend_from_slice(&value.to_le_bytes());
    }
    assert_eq!(source.len(), 84);

    let interner = StringInterner::new();
    let mut record = make_record("PROJ", &interner);
    push_field(
        &mut record,
        "DATA",
        FieldValue::Bytes(smallvec::SmallVec::from_vec(source)),
    );
    let mut ctx = make_ctx(&interner);
    FnvFo4Hook.pre_translate(&mut ctx, &mut record).unwrap();

    let dnam = raw_field(&record, "DNAM");
    assert_eq!(dnam.len(), 93);
    assert_eq!(u16_at(dnam, 0), 0x03ef);
    assert_eq!(u16_at(dnam, 2), 4); // FNV Continuous Beam -> FO4 Beam
    for (offset, expected) in [
        (4, 1.25_f32),
        (8, 2.5),
        (12, 3.75),
        (24, 4.25),
        (28, 5.5),
        (40, 6.75),
        (44, 7.0),
        (48, 8.5),
    ] {
        assert_eq!(f32_at(dnam, offset), expected);
    }
    for (offset, expected) in [
        (16, 0x0011_1111),
        (20, 0x0022_2222),
        (32, 0x0033_3333),
        (60, 0x0077_7777),
    ] {
        assert_eq!(u32_at(dnam, offset), expected);
    }
    for offset in [36, 52, 56] {
        assert_eq!(
            u32_at(dnam, offset),
            0,
            "legacy SOUN ref at target offset {offset}"
        );
    }
    assert_eq!(&dnam[64..93], &[0; 29]);
}

#[test]
fn pre_translate_defaults_malformed_proj_and_deduplicates_target_contract() {
    let interner = StringInterner::new();
    let mut record = make_record("PROJ", &interner);
    push_field(
        &mut record,
        "DATA",
        FieldValue::Bytes(smallvec::smallvec![1, 2, 3]),
    );
    push_field(&mut record, "DATA", FieldValue::None);
    push_field(
        &mut record,
        "DNAM",
        FieldValue::Bytes(smallvec::smallvec![9]),
    );
    push_field(
        &mut record,
        "NAM2",
        FieldValue::Bytes(smallvec::smallvec![8]),
    );

    let mut ctx = make_ctx(&interner);
    FnvFo4Hook.pre_translate(&mut ctx, &mut record).unwrap();

    assert!(raw_field(&record, "DATA").is_empty());
    let dnam = raw_field(&record, "DNAM");
    assert_eq!(dnam.len(), 93);
    assert_eq!(u16_at(dnam, 0), 0);
    assert_eq!(u16_at(dnam, 2), 1);
    assert!(dnam[4..].iter().all(|byte| *byte == 0));
    assert!(
        record
            .fields
            .iter()
            .all(|entry| entry.sig.as_str() != "NAM2")
    );
}
