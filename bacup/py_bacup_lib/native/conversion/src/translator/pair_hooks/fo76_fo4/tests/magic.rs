

    #[test]
    fn effects_synthetic_false_for_other_records() {
        for sig in &["WEAP", "ARMO", "NPC_", "RACE"] {
            let s = SigCode::from_str(sig).unwrap();
            assert!(
                !Fo76Fo4Hook::is_effects_synthetic(s),
                "{sig} should not be synthetic"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Behavior 3: effects key routing
    // -------------------------------------------------------------------------

    #[test]
    fn translate_effects_key_reroutes_data_for_alch() {
        let record_sig = SigCode::from_str("ALCH").unwrap();
        let field_sig = SubrecordSig::from_str("DATA").unwrap();
        let route = Fo76Fo4Hook::translate_effects_key(record_sig, field_sig);
        assert!(route.is_some());
        let r = route.unwrap();
        assert_eq!(r.target_sig.as_str(), "EFID");
    }

    #[test]
    fn translate_effects_key_reroutes_efid_for_ench() {
        let record_sig = SigCode::from_str("ENCH").unwrap();
        let field_sig = SubrecordSig::from_str("EFID").unwrap();
        let route = Fo76Fo4Hook::translate_effects_key(record_sig, field_sig);
        assert!(route.is_some());
        assert_eq!(route.unwrap().target_sig.as_str(), "EFID");
    }

    #[test]
    fn translate_effects_key_reroutes_efit_for_spel() {
        let record_sig = SigCode::from_str("SPEL").unwrap();
        let field_sig = SubrecordSig::from_str("EFIT").unwrap();
        let route = Fo76Fo4Hook::translate_effects_key(record_sig, field_sig);
        assert!(route.is_some());
        assert_eq!(route.unwrap().target_sig.as_str(), "EFID");
    }

    #[test]
    fn translate_effects_key_reroutes_data_for_perk_to_data() {
        let record_sig = SigCode::from_str("PERK").unwrap();
        let field_sig = SubrecordSig::from_str("DATA").unwrap();
        let route = Fo76Fo4Hook::translate_effects_key(record_sig, field_sig);
        assert!(route.is_some());
        assert_eq!(route.unwrap().target_sig.as_str(), "DATA");
    }

    #[test]
    fn translate_effects_key_no_route_for_non_effects_record() {
        let record_sig = SigCode::from_str("WEAP").unwrap();
        let field_sig = SubrecordSig::from_str("DATA").unwrap();
        assert!(Fo76Fo4Hook::translate_effects_key(record_sig, field_sig).is_none());
    }

    #[test]
    fn translate_effects_key_no_route_for_unrelated_field_in_alch() {
        let record_sig = SigCode::from_str("ALCH").unwrap();
        let field_sig = SubrecordSig::from_str("FULL").unwrap();
        assert!(Fo76Fo4Hook::translate_effects_key(record_sig, field_sig).is_none());
    }

    #[test]
    fn post_translate_preserves_perk_vmad_and_normalizes_one_entry() {
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

    #[test]
    fn normalize_perk_entry_layout_preserves_three_complete_entry_groups() {
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
    fn post_translate_projects_fo76_crafting_workbench_data_to_fo4_byte() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        for record_sig in ["FURN", "TERM"] {
            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "WBDT",
                FieldValue::Bytes(SmallVec::from_vec(vec![7, 1])),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected WBDT bytes");
            };
            assert_eq!(bytes.as_slice(), &[7]);
        }
    }

    #[test]
    fn post_translate_clears_invalid_marker_bits_but_keeps_model_backed_point_0() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        // Has Model (0x40000000) is set: Interaction Point 0 is backed by the
        // model's default furniture marker, so it survives even without explicit
        // marker subrecords; the higher (invalid) interaction points still clear.
        for record_sig in ["FURN", "TERM"] {
            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "MNAM",
                raw_bytes(&0x4000_001F_u32.to_le_bytes()),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected MNAM bytes");
            };
            assert_eq!(
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                0x4000_0001,
                "model-backed Interaction Point 0 must survive; invalid points clear",
            );
        }
    }

    #[test]
    fn post_translate_preserves_race_late_field_order() {
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
