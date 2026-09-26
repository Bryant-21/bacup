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

    fn ligh_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn ligh_f32(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn ligh_struct_field<'a>(
        interner: &StringInterner,
        record: &'a Record,
        field_name: &str,
    ) -> Option<&'a FieldValue> {
        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured LIGH DATA");
        };
        fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some(field_name))
            .map(|(_, value)| value)
    }

    fn term_snam_values(record: &Record) -> Vec<&FieldValue> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"SNAM")
            .map(|entry| &entry.value)
            .collect()
    }

    fn shadow_ligh_data(flags: u32, radius: u32) -> Vec<u8> {
        let mut bytes = fo76_ligh_data(flags, 10.0, 0.0);
        bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .copy_from_slice(&radius.to_le_bytes());
        bytes
    }

    #[test]
    fn post_translate_normalizes_raw_light_data() {
        let interner = StringInterner::new();
        let cage_bulb = "data\\Textures\\Effects\\Gobos\\CageBulbGobo01_d.DDS";
        let raw = |len: usize, radius: u32, value: u32, near_clip: f32| {
            let mut data = vec![0_u8; len];
            data[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                .copy_from_slice(&radius.to_le_bytes());
            data[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
                .copy_from_slice(&value.to_le_bytes());
            data[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
                .copy_from_slice(&near_clip.to_le_bytes());
            // FO76 stores colour temperature here; byte-copied it reads as a denormal.
            data[FO4_LIGH_DATA_GOD_RAYS_NEAR_CLIP_OFFSET
                ..FO4_LIGH_DATA_GOD_RAYS_NEAR_CLIP_OFFSET + 4]
                .copy_from_slice(&4000_u32.to_le_bytes());
            data
        };
        for (name, eid, gobo, data, expected_radius, expected_near_clip) in [
            ("keeps_existing_radius", None, None, raw(64, 128, 400, 0.0), 128, None),
            ("derives_missing_radius_from_value", None, None, raw(68, 0, 400, 0.0), 400, None),
            (
                "clamps_missing_radius_from_large_value",
                None,
                None,
                raw(68, 0, 250_000, 0.0),
                FO4_LIGH_MAX_SYNTHETIC_RADIUS,
                None,
            ),
            (
                "caps_cage_bulb_gobo",
                None,
                Some(cage_bulb),
                raw(68, 1200, 0, 1.0),
                FO4_CAGE_BULB_GOBO_MAX_RADIUS,
                Some(FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP),
            ),
            (
                "caps_omni_sunlight",
                Some("LGT_SunlightNS_Dim"),
                None,
                raw(64, 882, 0, 0.0),
                FO4_SUNLIGHT_MAX_RADIUS,
                None,
            ),
            ("skips_spot_sunlight", Some("LGT_SunlightSpotNS"), None, raw(64, 882, 0, 0.0), 882, None),
            (
                "skips_fake_sunlight",
                Some("LGT_SMI_FakeSunlight_red_NS"),
                None,
                raw(64, 882, 0, 0.0),
                882,
                None,
            ),
            (
                "skips_storm_sunlight",
                Some("Storm_LGT_SunlightNS_Dim_BrownHouse"),
                None,
                raw(64, 882, 0, 0.0),
                882,
                None,
            ),
            (
                "caps_shadow_caster",
                None,
                None,
                shadow_ligh_data(FO4_LIGH_FLAGS_SHADOW_SPOTLIGHT, 1631),
                FO4_LIGH_SHADOW_CASTER_MAX_RADIUS,
                None,
            ),
        ] {
            let derives_radius = ligh_u32(&data, FO4_LIGH_DATA_RADIUS_OFFSET) == 0;
            let value = ligh_u32(&data, FO76_LIGH_DATA_VALUE_OFFSET);
            let mut record = ligh_with_data(&interner, data, gobo);
            record.eid = eid.map(|eid| interner.intern(eid));

            Fo76Fo4Hook
                .post_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            let bytes = ligh_data_bytes(&record);
            assert_eq!(bytes.len(), FO4_LIGH_DATA_LEN, "{name}");
            assert_eq!(
                ligh_u32(&bytes, FO4_LIGH_DATA_RADIUS_OFFSET),
                expected_radius,
                "{name}"
            );
            assert_eq!(
                ligh_f32(&bytes, FO4_LIGH_DATA_GOD_RAYS_NEAR_CLIP_OFFSET),
                FO4_LIGH_DEFAULT_GOD_RAYS_NEAR_CLIP,
                "{name}"
            );
            if let Some(near_clip) = expected_near_clip {
                assert_eq!(
                    ligh_f32(&bytes, FO4_LIGH_DATA_NEAR_CLIP_OFFSET),
                    near_clip,
                    "{name}"
                );
            }
            if derives_radius {
                assert_eq!(
                    ligh_f32(&bytes, FO4_LIGH_DATA_SCALAR_OFFSET),
                    FO4_LIGH_DEFAULT_SCALAR,
                    "{name}"
                );
                assert_eq!(
                    ligh_f32(&bytes, FO4_LIGH_DATA_EXPONENT_OFFSET),
                    FO4_LIGH_DEFAULT_EXPONENT,
                    "{name}"
                );
                assert_eq!(
                    ligh_f32(&bytes, FO4_LIGH_DATA_WEIGHT_OFFSET),
                    FO4_LIGH_DEFAULT_WEIGHT,
                    "{name}"
                );
                // Lumens survive translate for the post-copy `normalize_light_radii` pass.
                assert_eq!(ligh_u32(&bytes, FO4_LIGH_DATA_VALUE_OFFSET), value, "{name}");
            }
        }
    }

    #[test]
    fn post_translate_normalizes_structured_light_data() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;

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
        hook.post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        assert_eq!(
            ligh_struct_field(&interner, &record, "Radius"),
            Some(&FieldValue::Uint(400))
        );
        // `Value` holds FO76's lumens and is kept for the post-copy radii pass;
        // `Bytes19` is FO76-only padding with no FO4 meaning.
        assert_eq!(
            ligh_struct_field(&interner, &record, "Value"),
            Some(&FieldValue::Uint(400))
        );
        assert_eq!(ligh_struct_field(&interner, &record, "Bytes19"), None);
        assert_eq!(
            ligh_struct_field(&interner, &record, "Scalar"),
            Some(&FieldValue::Float(FO4_LIGH_DEFAULT_SCALAR))
        );
        assert_eq!(
            ligh_struct_field(&interner, &record, "Exponent"),
            Some(&FieldValue::Float(FO4_LIGH_DEFAULT_EXPONENT))
        );

        for (name, eid, gobo, data, expected_radius) in [
            (
                "clamps_missing_radius_from_large_value",
                None,
                None,
                vec![("Value", FieldValue::Uint(250_000))],
                FO4_LIGH_MAX_SYNTHETIC_RADIUS,
            ),
            (
                "caps_cage_bulb_gobo",
                None,
                Some("Textures\\Effects\\Gobos\\CageBulbGobo01_d.DDS"),
                vec![
                    ("Radius", FieldValue::Uint(1200)),
                    ("NearClip", FieldValue::Float(1.0)),
                    ("Value", FieldValue::Uint(1200)),
                ],
                FO4_CAGE_BULB_GOBO_MAX_RADIUS,
            ),
            (
                "caps_omni_sunlight",
                Some("LGT_SunlightS_NoAtten"),
                None,
                vec![("Radius", FieldValue::Uint(882))],
                FO4_SUNLIGHT_MAX_RADIUS,
            ),
            (
                "caps_shadow_caster",
                None,
                None,
                vec![
                    (
                        "Flags",
                        FieldValue::Uint(u64::from(FO4_LIGH_FLAGS_SHADOW_SPOTLIGHT)),
                    ),
                    ("Radius", FieldValue::Uint(1631)),
                ],
                FO4_LIGH_SHADOW_CASTER_MAX_RADIUS,
            ),
            (
                "keeps_non_shadow_radius",
                None,
                None,
                vec![
                    ("Flags", FieldValue::Uint(0x0000_4000)),
                    ("Radius", FieldValue::Uint(3000)),
                ],
                3000,
            ),
        ] {
            let mut record = make_record("LIGH", &interner);
            record.eid = eid.map(|eid| interner.intern(eid));
            push_field(
                &mut record,
                "DATA",
                FieldValue::Struct(
                    data.into_iter()
                        .map(|(field, value)| (interner.intern(field), value))
                        .collect(),
                ),
            );
            if let Some(path) = gobo {
                push_field(
                    &mut record,
                    "NAM0",
                    FieldValue::String(interner.intern(path)),
                );
            }

            hook.post_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(
                ligh_struct_field(&interner, &record, "Radius"),
                Some(&FieldValue::Uint(u64::from(expected_radius))),
                "{name}"
            );
            if gobo.is_some() {
                assert_eq!(
                    ligh_struct_field(&interner, &record, "NearClip"),
                    Some(&FieldValue::Float(FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP)),
                    "{name}"
                );
            }
        }
    }

    #[test]
    fn post_translate_inserts_light_fade_value_only_when_missing() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 64])),
        );

        hook.post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

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

        let mut record = make_record("LIGH", &interner);
        push_field(&mut record, "FNAM", FieldValue::Float(0.25));
        hook.post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let fades: Vec<&FieldValue> = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "FNAM")
            .map(|field| &field.value)
            .collect();
        assert_eq!(fades, vec![&FieldValue::Float(0.25)]);
    }

    #[test]
    fn light_normalize_raw_data_table() {
        let interner = StringInterner::new();
        let work_light = Some("Data\\textures\\effects\\gobos\\worklightgobo_d.dds");
        let scratches = Some("data\\Textures\\Effects\\Gobos\\OmniScratchesGobo01.DDS");
        let flags = FO4_LIGH_DATA_FLAGS_OFFSET;
        let near = FO4_LIGH_DATA_NEAR_CLIP_OFFSET;
        let amp = FO4_LIGH_DATA_FLICKER_INTENSITY_AMP_OFFSET;
        // (name, flags, near clip, flicker amp, gobo, checked offset, expected bytes there)
        for (name, in_flags, in_near, in_amp, gobo, offset, expected) in [
            // Barrel flags 0x8009 (unknown0 + flicker + non_specular) plus an FO76-only bit.
            ("masks_non_specular_and_fo76_bits", 0x0080_8009, 1.0, 10.0, None, flags, 0x0000_0009_u32.to_le_bytes()),
            ("clears_attenuation_only_from_shadow_spot", 0x0081_8401, 1.0, 0.0, scratches, flags, 0x0000_0401_u32.to_le_bytes()),
            ("keeps_attenuation_only_on_non_spot", 0x0001_0001, 1.0, 0.0, None, flags, 0x0001_0001_u32.to_le_bytes()),
            ("floors_small_near_clip", 0x0009, 1.0, 0.4, None, near, FO4_LIGH_MIN_NEAR_CLIP.to_le_bytes()),
            ("keeps_large_near_clip", 0x0009, 64.0, 0.4, None, near, 64.0_f32.to_le_bytes()),
            ("clamps_gobo_flicker", 0x8009, 1.0, 10.0, work_light, amp, FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO.to_le_bytes()),
            ("clamps_non_gobo_flicker", 0x8009, 1.0, 30000.0, None, amp, FO4_LIGH_MAX_FLICKER_INTENSITY_AMP.to_le_bytes()),
            ("keeps_in_range_gobo_flicker", 0x8009, 32.0, 0.45, work_light, amp, 0.45_f32.to_le_bytes()),
        ] {
            let mut record =
                ligh_with_data(&interner, fo76_ligh_data(in_flags, in_near, in_amp), gobo);

            Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);

            let data = ligh_data_bytes(&record);
            assert_eq!(data.len(), FO4_LIGH_DATA_LEN, "{name}: truncated to FO4 DATA length");
            assert_eq!(data[offset..offset + 4], expected, "{name}");
        }
    }

    #[test]
    fn light_normalize_clamps_fire_flicker_and_structured_flags() {
        let interner = StringInterner::new();
        let mut data = fo76_ligh_data(0x8009, 1.0, 2.0);
        data[FO4_LIGH_DATA_FLICKER_PERIOD_OFFSET..FO4_LIGH_DATA_FLICKER_PERIOD_OFFSET + 4]
            .copy_from_slice(&1.0_f32.to_le_bytes());
        let mut record = ligh_with_data(&interner, data, None);
        record.eid = Some(interner.intern("LGT_MM_OmniNS_06K_fire_Flicker"));
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        assert_eq!(
            ligh_f32(&data, FO4_LIGH_DATA_FLICKER_PERIOD_OFFSET),
            FO4_LIGH_MAX_FIRE_FLICKER_PERIOD
        );
        assert_eq!(
            ligh_f32(&data, FO4_LIGH_DATA_FLICKER_INTENSITY_AMP_OFFSET),
            FO4_LIGH_MAX_FIRE_FLICKER_INTENSITY_AMP
        );

        let mut record = make_record("LIGH", &interner);
        record.eid = Some(interner.intern("LGT_MetalBarrelFireGrating_Omni_NS_200_Flicker"));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (interner.intern("Flags"), FieldValue::Uint(0x0001_0401)),
                (interner.intern("FlickerEffectPeriod"), FieldValue::Float(1.0)),
                (
                    interner.intern("FlickerEffectIntensityAmplitude"),
                    FieldValue::Float(2.0),
                ),
            ]),
        );
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        assert_eq!(
            ligh_struct_field(&interner, &record, "Flags"),
            Some(&FieldValue::Uint(0x0000_0401))
        );
        assert_eq!(
            ligh_struct_field(&interner, &record, "FlickerEffectPeriod"),
            Some(&FieldValue::Float(FO4_LIGH_MAX_FIRE_FLICKER_PERIOD))
        );
        assert_eq!(
            ligh_struct_field(&interner, &record, "FlickerEffectIntensityAmplitude"),
            Some(&FieldValue::Float(FO4_LIGH_MAX_FIRE_FLICKER_INTENSITY_AMP))
        );
    }

    #[test]
    fn shadow_caster_radius_cap_table() {
        let interner = StringInterner::new();
        for (name, flags, radius, expected) in [
            ("spot_over_budget", FO4_LIGH_FLAGS_SHADOW_SPOTLIGHT, 20000, FO4_LIGH_SHADOW_CASTER_MAX_RADIUS),
            ("spot_within_budget", FO4_LIGH_FLAGS_SHADOW_SPOTLIGHT, 700, 700),
            ("hemisphere", FO4_LIGH_FLAGS_SHADOW_HEMISPHERE, 20000, FO4_LIGH_SHADOW_CASTER_MAX_RADIUS),
            ("omnidirectional", FO4_LIGH_FLAGS_SHADOW_OMNIDIRECTIONAL, 20000, FO4_LIGH_SHADOW_CASTER_MAX_RADIUS),
            // NonShadowSpotlight renders no shadow map, so its radius costs nothing.
            ("non_shadow_spot", 0x0000_4000, 3000, 3000),
        ] {
            let mut record = ligh_with_data(&interner, shadow_ligh_data(flags, radius), None);
            Fo76Fo4Hook::clamp_shadow_caster_radius_for_fo4(&interner, &mut record);
            assert_eq!(
                ligh_u32(&ligh_data_bytes(&record), FO4_LIGH_DATA_RADIUS_OFFSET),
                expected,
                "{name}"
            );
        }
    }
