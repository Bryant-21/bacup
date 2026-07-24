

    #[test]
    fn post_translate_keeps_existing_positive_raw_light_radius() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        let mut raw = vec![0_u8; 64];
        raw[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .copy_from_slice(&128_u32.to_le_bytes());
        raw[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
            .copy_from_slice(&400_u32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            128
        );
    }

    #[test]
    fn post_translate_clamps_missing_raw_light_radius_from_large_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        let mut raw = vec![0_u8; 68];
        raw[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
            .copy_from_slice(&250_000_u32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_MAX_SYNTHETIC_RADIUS
        );
        assert_eq!(
            f32::from_le_bytes(
                bytes[FO4_LIGH_DATA_SCALAR_OFFSET..FO4_LIGH_DATA_SCALAR_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_DEFAULT_SCALAR
        );
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_DEFAULT_VALUE
        );
    }

    #[test]
    fn post_translate_sets_structured_light_radius_from_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (interner.intern("Value"), FieldValue::Uint(400)),
                (
                    interner.intern("Bytes19"),
                    FieldValue::Bytes(SmallVec::from_vec(vec![0; 8])),
                ),
            ]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured DATA");
        };
        let radius = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Radius"))
            .map(|(_, value)| value);
        assert_eq!(radius, Some(&FieldValue::Uint(400)));
        assert!(
            fields
                .iter()
                .all(|(name, _)| interner.resolve(*name) != Some("Value")
                    && interner.resolve(*name) != Some("Bytes19"))
        );
        let scalar = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Scalar"))
            .map(|(_, value)| value);
        let exponent = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Exponent"))
            .map(|(_, value)| value);
        assert_eq!(scalar, Some(&FieldValue::Float(FO4_LIGH_DEFAULT_SCALAR)));
        assert_eq!(
            exponent,
            Some(&FieldValue::Float(FO4_LIGH_DEFAULT_EXPONENT))
        );
    }

    #[test]
    fn post_translate_clamps_missing_structured_light_radius_from_large_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![(interner.intern("Value"), FieldValue::Uint(250_000))]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured DATA");
        };
        let radius = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Radius"))
            .map(|(_, value)| value);
        assert_eq!(
            radius,
            Some(&FieldValue::Uint(u64::from(FO4_LIGH_MAX_SYNTHETIC_RADIUS)))
        );
    }

    #[test]
    fn post_translate_caps_raw_cage_bulb_gobo_light_radius() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        let mut raw = vec![0_u8; 68];
        raw[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .copy_from_slice(&1200_u32.to_le_bytes());
        raw[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
            .copy_from_slice(&1.0_f32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );
        push_field(
            &mut record,
            "NAM0",
            FieldValue::String(
                interner.intern("data\\Textures\\Effects\\Gobos\\CageBulbGobo01_d.DDS"),
            ),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_CAGE_BULB_GOBO_MAX_RADIUS
        );
        assert_eq!(
            f32::from_le_bytes(
                bytes[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP
        );
    }

    #[test]
    fn post_translate_caps_structured_cage_bulb_gobo_light_radius() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (interner.intern("Radius"), FieldValue::Uint(1200)),
                (interner.intern("NearClip"), FieldValue::Float(1.0)),
                (interner.intern("Value"), FieldValue::Uint(1200)),
            ]),
        );
        push_field(
            &mut record,
            "NAM0",
            FieldValue::String(interner.intern("Textures\\Effects\\Gobos\\CageBulbGobo01_d.DDS")),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured DATA");
        };
        let radius = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Radius"))
            .map(|(_, value)| value);
        let near_clip = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("NearClip"))
            .map(|(_, value)| value);
        assert_eq!(
            radius,
            Some(&FieldValue::Uint(u64::from(FO4_CAGE_BULB_GOBO_MAX_RADIUS)))
        );
        assert_eq!(
            near_clip,
            Some(&FieldValue::Float(FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP))
        );
    }

    #[test]
    fn post_translate_inserts_missing_light_fade_value_after_data() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 64])),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["DATA", "FNAM"]);
        assert_eq!(
            record.fields[1].value,
            FieldValue::Float(FO4_LIGH_DEFAULT_FADE)
        );
    }

    #[test]
    fn post_translate_keeps_existing_light_fade_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(&mut record, "FNAM", FieldValue::Float(0.25));

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let fades: Vec<&FieldValue> = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "FNAM")
            .map(|field| &field.value)
            .collect();
        assert_eq!(fades, vec![&FieldValue::Float(0.25)]);
    }

    #[test]
    fn post_translate_drops_perk_vmad() {
        let interner = StringInterner::new();
        let mut record = make_record("PERK", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3, 4])),
        );
        push_field(
            &mut record,
            "FULL",
            FieldValue::String(interner.intern("Perk")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "FULL"]);
    }

    fn ligh_with_data(interner: &StringInterner, data: Vec<u8>, gobo: Option<&str>) -> Record {
        let mut record = make_record("LIGH", interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(data)),
        );
        if let Some(path) = gobo {
            push_field(
                &mut record,
                "NAM0",
                FieldValue::String(interner.intern(path)),
            );
        }
        record
    }

    fn ligh_data_bytes(record: &Record) -> Vec<u8> {
        record
            .fields
            .iter()
            .find(|e| e.sig.0 == *b"DATA")
            .and_then(|e| match &e.value {
                FieldValue::Bytes(b) => Some(b.to_vec()),
                _ => None,
            })
            .expect("DATA bytes")
    }

    #[test]
    fn light_normalize_clears_nonspecular_and_masks_fo76_bits() {
        let interner = StringInterner::new();
        // FO76 barrel flags 0x8009 (unknown0 + flicker + non_specular) plus an
        // FO76-only high bit (0x800000) that FO4 does not define.
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x0080_8009, 1.0, 10.0), None);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let flags = u32::from_le_bytes(data[12..16].try_into().unwrap());
        assert_eq!(
            flags, 0x0000_0009,
            "non_specular and FO76-only bit dropped, unknown0 + flicker kept"
        );
    }

    #[test]
    fn light_normalize_clears_attenuation_only_from_shadow_spotlights() {
        let interner = StringInterner::new();
        let mut record = ligh_with_data(
            &interner,
            fo76_ligh_data(0x0081_8401, 1.0, 0.0),
            Some("data\\Textures\\Effects\\Gobos\\OmniScratchesGobo01.DDS"),
        );

        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);

        let data = ligh_data_bytes(&record);
        let flags = u32::from_le_bytes(data[12..16].try_into().unwrap());
        assert_eq!(flags, 0x0000_0401);
    }

    #[test]
    fn light_normalize_preserves_attenuation_only_on_non_spotlights() {
        let interner = StringInterner::new();
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x0001_0001, 1.0, 0.0), None);

        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);

        let data = ligh_data_bytes(&record);
        let flags = u32::from_le_bytes(data[12..16].try_into().unwrap());
        assert_eq!(flags, 0x0001_0001);
    }

    #[test]
    fn light_normalize_clears_attenuation_only_from_structured_shadow_spotlights() {
        let interner = StringInterner::new();
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![(
                interner.intern("Flags"),
                FieldValue::Uint(0x0001_0401),
            )]),
        );

        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured LIGH DATA");
        };
        let flags = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Flags"))
            .map(|(_, value)| value)
            .expect("Flags remains");
        assert_eq!(flags, &FieldValue::Uint(0x0000_0401));
    }

    #[test]
    fn light_normalize_floors_small_near_clip() {
        let interner = StringInterner::new();
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x0009, 1.0, 0.4), None);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let near = f32::from_le_bytes(data[24..28].try_into().unwrap());
        assert_eq!(near, FO4_LIGH_MIN_NEAR_CLIP);
    }

    #[test]
    fn light_normalize_preserves_large_near_clip() {
        let interner = StringInterner::new();
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x0009, 64.0, 0.4), None);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let near = f32::from_le_bytes(data[24..28].try_into().unwrap());
        assert_eq!(near, 64.0);
    }

    #[test]
    fn light_normalize_clamps_flicker_gobo_tighter_and_truncates() {
        let interner = StringInterner::new();
        let gobo = Some("Data\\textures\\effects\\gobos\\worklightgobo_d.dds");
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x8009, 1.0, 10.0), gobo);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let amp = f32::from_le_bytes(data[32..36].try_into().unwrap());
        assert_eq!(amp, FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO);
        assert_eq!(
            data.len(),
            FO4_LIGH_DATA_LEN,
            "truncated to FO4 DATA length"
        );
    }

    #[test]
    fn light_normalize_clamps_flicker_nongobo_to_ceiling() {
        let interner = StringInterner::new();
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x8009, 1.0, 30000.0), None);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let amp = f32::from_le_bytes(data[32..36].try_into().unwrap());
        assert_eq!(amp, FO4_LIGH_MAX_FLICKER_INTENSITY_AMP);
    }

    #[test]
    fn light_normalize_leaves_in_range_flicker_untouched() {
        let interner = StringInterner::new();
        let gobo = Some("Data\\textures\\effects\\gobos\\worklightgobo_d.dds");
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x8009, 32.0, 0.45), gobo);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let amp = f32::from_le_bytes(data[32..36].try_into().unwrap());
        assert_eq!(
            amp, 0.45,
            "value already inside FO4's gobo range is preserved"
        );
    }

    fn term_snam_values(record: &Record) -> Vec<&FieldValue> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"SNAM")
            .map(|entry| &entry.value)
            .collect()
    }
