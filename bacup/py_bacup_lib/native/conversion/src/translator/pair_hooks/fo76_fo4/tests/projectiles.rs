    #[test]
    fn weapon_cone_force_only_changes_the_source_query_layer_contract() {
        let interner = StringInterner::new();
        let masters = vec!["Fallout4.esm".to_string()];
        let mut ctx = PairCtx::with_source(&interner, "SeventySix.esm", &masters);
        for (kind, layer, expected_force) in [
            (16_u16, 0x015B74D0_u32, 0.0_f32),
            (8, 0x015B74D0, 150.0),
            (16, 0x005B74D0, 150.0),
            (16, 0, 150.0),
        ] {
            let mut record = make_record("PROJ", &interner);
            let mut bytes = vec![0_u8; 93];
            bytes[2..4].copy_from_slice(&kind.to_le_bytes());
            bytes[8..12].copy_from_slice(&650.0_f32.to_le_bytes());
            bytes[48..52].copy_from_slice(&150.0_f32.to_le_bytes());
            bytes[84..88].copy_from_slice(&layer.to_le_bytes());
            let mut expected = bytes.clone();
            expected[48..52].copy_from_slice(&expected_force.to_le_bytes());
            push_field(&mut record, "DNAM", FieldValue::Bytes(SmallVec::from_vec(bytes)));
            Fo76Fo4Hook.pre_translate(&mut ctx, &mut record).unwrap();
            assert_eq!(record.fields[0].value, FieldValue::Bytes(SmallVec::from_vec(expected)));
            let before = record.fields[0].value.clone();
            Fo76Fo4Hook.pre_translate(&mut ctx, &mut record).unwrap();
            assert_eq!(record.fields[0].value, before);
        }
    }

    #[test]
    fn weapon_cone_force_handles_structured_source_data() {
        let interner = StringInterner::new();
        let mut record = make_record("PROJ", &interner);
        let force = interner.intern("impact_force");
        push_field(&mut record, "DNAM", FieldValue::Struct(vec![
            (interner.intern("type"), FieldValue::Uint(16)),
            (interner.intern("collision_layer"), form_key_value(&interner, 0x5B74D0)),
            (force, FieldValue::Float(150.0)),
        ]));
        Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
        let FieldValue::Struct(fields) = &record.fields[0].value else { panic!("DNAM") };
        assert_eq!(fields[2], (force, FieldValue::Float(0.0)));
    }
