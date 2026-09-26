

    #[test]
    fn translate_effects_key_routes_only_effect_fields_of_effect_records() {
        for sig in ["WEAP", "ARMO", "NPC_", "RACE"] {
            assert!(
                !Fo76Fo4Hook::is_effects_synthetic(SigCode::from_str(sig).unwrap()),
                "{sig} should not be synthetic"
            );
        }
        for (record, field, expected) in [
            ("ALCH", "DATA", Some("EFID")),
            ("ENCH", "EFID", Some("EFID")),
            ("SPEL", "EFIT", Some("EFID")),
            ("PERK", "DATA", Some("DATA")),
            ("WEAP", "DATA", None),
            ("ALCH", "FULL", None),
        ] {
            let route = Fo76Fo4Hook::translate_effects_key(
                SigCode::from_str(record).unwrap(),
                SubrecordSig::from_str(field).unwrap(),
            );
            assert_eq!(
                route.map(|route| route.target_sig.as_str().to_string()),
                expected.map(str::to_string),
                "{record}.{field}"
            );
        }
    }

    #[test]
    fn post_translate_and_normalize_preserve_perk_entries_and_race_order() {
        {
            let interner = StringInterner::new();
            let mut record = make_record("PERK", &interner);
            let vmad = vec![6, 0, 2, 0, 0, 0];
            push_field(
                &mut record,
                "VMAD",
                FieldValue::Bytes(SmallVec::from_vec(vmad.clone())),
            );
            push_field(
                &mut record,
                "PRKE",
                FieldValue::Bytes(SmallVec::from_slice(&[2, 0])),
            );
            push_field(
                &mut record,
                "DATA",
                FieldValue::Bytes(SmallVec::from_slice(&[0x0E, 0x09, 0x02, 0x00])),
            );
            push_field(&mut record, "PRKF", FieldValue::None);

            Fo76Fo4Hook
                .post_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(
                record
                    .fields
                    .iter()
                    .find(|entry| entry.sig.0 == *b"VMAD")
                    .map(|entry| &entry.value),
                Some(&FieldValue::Bytes(SmallVec::from_vec(vmad)))
            );
            let values: Vec<&FieldValue> = record
                .fields
                .iter()
                .filter(|entry| matches!(&entry.sig.0, b"PRKE" | b"DATA"))
                .map(|entry| &entry.value)
                .collect();
            assert_eq!(
                values,
                vec![
                    &FieldValue::Bytes(SmallVec::from_slice(&[2, 0, 0])),
                    &FieldValue::Bytes(SmallVec::from_slice(&[0x0E, 0x09, 0x02])),
                ]
            );
        }

        {
            let interner = StringInterner::new();
            let mut record = make_record("PERK", &interner);
            push_field(
                &mut record,
                "DATA",
                FieldValue::Bytes(SmallVec::from_slice(&[1, 0, 1])),
            );
            for rank in [3, 2, 1] {
                push_field(
                    &mut record,
                    "PRKE",
                    FieldValue::Bytes(SmallVec::from_slice(&[2, rank])),
                );
                push_field(
                    &mut record,
                    "DATA",
                    FieldValue::Bytes(SmallVec::from_slice(&[0x0E, 0x09, 0x02, 0x00])),
                );
                push_field(
                    &mut record,
                    "PRKC",
                    FieldValue::Bytes(SmallVec::from_slice(&[0])),
                );
                push_field(
                    &mut record,
                    "CTDA",
                    FieldValue::Bytes(SmallVec::from_slice(&[rank; 32])),
                );
                push_field(
                    &mut record,
                    "CIS1",
                    FieldValue::Bytes(SmallVec::from_slice(&[rank, 0])),
                );
                push_field(
                    &mut record,
                    "CIS2",
                    FieldValue::Bytes(SmallVec::from_slice(&[rank + 1, 0])),
                );
                push_field(
                    &mut record,
                    "EPFT",
                    FieldValue::Bytes(SmallVec::from_slice(&[4])),
                );
                push_field(
                    &mut record,
                    "EPFB",
                    FieldValue::Bytes(SmallVec::from_slice(&[rank])),
                );
                push_field(
                    &mut record,
                    "EPF2",
                    FieldValue::Bytes(SmallVec::from_slice(&[b'A' + rank, 0])),
                );
                push_field(
                    &mut record,
                    "EPF3",
                    FieldValue::Bytes(SmallVec::from_slice(&[0, 0])),
                );
                push_field(&mut record, "PRKF", FieldValue::None);
            }
            let original = record.fields.clone();

            Fo76Fo4Hook::normalize_perk_entry_layout(&mut record);

            assert_eq!(record.fields.len(), original.len());
            for (index, (actual, expected)) in record.fields.iter().zip(&original).enumerate() {
                assert_eq!(actual.sig, expected.sig, "field {index} moved");
                if actual.sig.0 == *b"PRKE" {
                    let FieldValue::Bytes(actual) = &actual.value else {
                        panic!("PRKE must remain raw bytes");
                    };
                    let FieldValue::Bytes(expected) = &expected.value else {
                        unreachable!();
                    };
                    assert_eq!(actual.as_slice(), &[expected[0], expected[1], 0]);
                } else if actual.sig.0 == *b"DATA" && index != 0 {
                    assert_eq!(
                        actual.value,
                        FieldValue::Bytes(SmallVec::from_slice(&[0x0E, 0x09, 0x02]))
                    );
                } else {
                    assert_eq!(actual.value, expected.value, "field {index} changed");
                }
            }

            let sigs: Vec<&str> = record
                .fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect();
            let expected_group = [
                "PRKE", "DATA", "PRKC", "CTDA", "CIS1", "CIS2", "EPFT", "EPFB", "EPF2", "EPF3",
                "PRKF",
            ];
            assert_eq!(&sigs[1..12], expected_group.as_slice());
            assert_eq!(&sigs[12..23], expected_group.as_slice());
            assert_eq!(&sigs[23..34], expected_group.as_slice());

            let mut expected_fields = record.fields.clone();
            for field in &mut expected_fields {
                if field.sig.0 == *b"PRKF" {
                    field.value = FieldValue::Bytes(SmallVec::new());
                }
            }
            let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
            let normalized =
                match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                    &schema, &interner,
                )
                .normalize(record)
                {
                    crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                    crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                        panic!("PERK is supported by FO4 schema")
                    }
                };
            assert_eq!(normalized.fields, expected_fields);
        }

        {
            let interner = StringInterner::new();
            let mut record = make_record("PERK", &interner);
            for (kind, bytes) in [(1, vec![0x81, 0xF1, 0x5C, 0]), (0, vec![1, 2, 3, 4, 5, 6, 7, 8])] {
                push_field(&mut record, "PRKE", FieldValue::Bytes(SmallVec::from_slice(&[kind, 0])));
                push_field(&mut record, "DATA", FieldValue::Bytes(SmallVec::from_slice(&bytes)));
                push_field(&mut record, "PRKF", FieldValue::None);
            }
            let original = record.fields.clone();
            Fo76Fo4Hook::normalize_perk_entry_layout(&mut record);
            for (before, after) in original.iter().zip(&record.fields) {
                if before.sig.0 != *b"PRKE" {
                    assert_eq!(before.value, after.value);
                }
            }
        }

        {
            let interner = StringInterner::new();
            let mut record = make_record("RACE", &interner);
            for sig in ["TTED", "MPPF", "MSM0", "BSMS", "MPPM", "TTGE", "MSM1"] {
                push_field(&mut record, sig, FieldValue::None);
            }

            let hook = Fo76Fo4Hook;
            let mut ctx = make_ctx(&interner);
            hook.post_translate(&mut ctx, &mut record).unwrap();

            let sigs: Vec<&str> = record
                .fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect();
            assert_eq!(
                sigs,
                vec!["TTED", "MPPF", "MSM0", "BSMS", "MPPM", "TTGE", "MSM1"]
            );
        }
    }

    fn property_row(interner: &StringInterner, property_id: u16) -> FieldValue {
        property_row_with_function_type(interner, property_id, 2)
    }

    #[test]
    fn pre_translate_normalizes_fo76_only_mgef_archetypes() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        for (source, expected) in [
            (FO76_MGEF_ARCHETYPE_PLAYER_FEAR, FO4_MGEF_ARCHETYPE_SCRIPT),
            (FO76_MGEF_ARCHETYPE_TURBO_FERT, FO4_MGEF_ARCHETYPE_SCRIPT),
            (
                FO76_MGEF_ARCHETYPE_CORPSE_HIGHLIGHT,
                FO4_MGEF_ARCHETYPE_SCRIPT,
            ),
            (FO76_MGEF_ARCHETYPE_STUN, FO4_MGEF_ARCHETYPE_STAGGER),
            (0, 0),
            (FO4_MGEF_ARCHETYPE_SCRIPT, FO4_MGEF_ARCHETYPE_SCRIPT),
            (19, 19),
            (21, 21),
            (FO4_MAX_MGEF_ARCHETYPE, FO4_MAX_MGEF_ARCHETYPE),
            (0x07000814, FO4_MGEF_ARCHETYPE_SCRIPT),
        ] {
            let mut record = make_record("MGEF", &interner);
            let mut data = vec![0_u8; FO4_MGEF_DATA_LEN];
            data[FO4_MGEF_DATA_ARCHETYPE_OFFSET..FO4_MGEF_DATA_ARCHETYPE_OFFSET + 4]
                .copy_from_slice(&source.to_le_bytes());
            push_field(
                &mut record,
                "DATA",
                FieldValue::Bytes(SmallVec::from_vec(data)),
            );

            hook.pre_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected raw DATA bytes");
            };
            assert_eq!(
                u32::from_le_bytes(
                    bytes[FO4_MGEF_DATA_ARCHETYPE_OFFSET..FO4_MGEF_DATA_ARCHETYPE_OFFSET + 4]
                        .try_into()
                        .unwrap()
                ),
                expected
            );
        }
    }

    #[test]
    fn lunchbox_reward_area_effect_group_is_dropped_keeping_self_effect() {
        let interner = crate::sym::StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let raw = |sig: &[u8; 4], bytes: Vec<u8>| FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::Bytes(bytes.into_iter().collect()),
        };
        let efit = |range: u32, area: u32| {
            [range, 0, area, 0].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()
        };
        let make = |local: u32, mgef: u32| {
            let mut record = Record::new(SigCode(*b"ALCH"), FormKey { plugin, local });
            record.fields.extend([
                raw(b"ENIT", vec![0; 4]),
                raw(b"EFID", mgef.to_le_bytes().to_vec()),
                raw(b"EFIT", efit(3, 0)),
                raw(b"MAGF", vec![0; 4]),
                raw(b"DURG", vec![0x39, 0x12, 0x3A, 0]),
                raw(b"CODV", vec![0; 4]),
                raw(b"EFID", mgef.to_le_bytes().to_vec()),
                raw(b"EFIT", efit(7, 500)),
                raw(b"MAGF", vec![0; 4]),
                raw(b"CTDA", vec![0; 32]),
                raw(b"DURG", vec![0x39, 0x12, 0x3A, 0]),
                raw(b"CODV", vec![0; 4]),
                raw(b"MIID", vec![0x0B, 0, 0, 0]),
            ]);
            record
        };
        let sigs = |record: &Record| record.fields.iter().map(|f| f.sig.as_str().to_string()).collect::<Vec<_>>();

        let mut lunchbox = make(0x3DF247, 0x3DF248);
        Fo76Fo4Hook::drop_lunchbox_reward_area_effect(&interner, &mut lunchbox);
        assert_eq!(sigs(&lunchbox), ["ENIT", "EFID", "EFIT", "MAGF", "DURG", "CODV", "MIID"]);
        assert_eq!(lunchbox.fields[2].value, raw(b"EFIT", efit(3, 0)).value);

        let mut other_effect = make(0x3DF247, 0x3DF249);
        Fo76Fo4Hook::drop_lunchbox_reward_area_effect(&interner, &mut other_effect);
        assert_eq!(other_effect.fields.len(), 13);

        let mut area_only = make(0x123456, 0x3DF248);
        area_only.fields.drain(1..6);
        Fo76Fo4Hook::drop_lunchbox_reward_area_effect(&interner, &mut area_only);
        assert_eq!(area_only.fields.len(), 8, "never strip the only reward effect");
    }
