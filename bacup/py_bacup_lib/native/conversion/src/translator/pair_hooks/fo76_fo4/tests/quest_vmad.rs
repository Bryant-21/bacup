

    #[test]
    fn pre_translate_marks_event_filled_qust_alias_optional_when_fill_is_stripped() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(14_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(13_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALID", FieldValue::Bytes(SmallVec::new()));
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFE",
            FieldValue::Bytes(SmallVec::from_vec(1329742913_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALED", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "ANAM", "ALST", "ALID", "FNAM", "ALED"]);
        let fnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "FNAM")
            .expect("alias FNAM");
        let FieldValue::Bytes(bytes) = &fnam.value else {
            panic!("FNAM should stay raw bytes");
        };
        let raw_flags = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        assert_eq!(
            raw_flags & QUST_ALIAS_OPTIONAL_FLAG,
            QUST_ALIAS_OPTIONAL_FLAG
        );
    }

    #[test]
    fn pre_translate_forces_proven_property_rich_daim_alias_to_player() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_alias_scripts(&[(1, &["dEfAuLtAlIaSiNvEnToRyMaNaGeMeNtl"])]),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
        );
        push_qust_event_alias(
            &mut record,
            1,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(
            !record
                .fields
                .iter()
                .any(|entry| matches!(&entry.sig.0, b"ALFE" | b"ALFD"))
        );
        let alfr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"ALFR")
            .expect("proven DAIM alias gets a forced reference");
        let FieldValue::FormKey(player) = &alfr.value else {
            panic!("ALFR should be a FormKey");
        };
        assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);
        assert_eq!(interner.resolve(player.plugin), Some(FO4_MASTER_NAME));
        assert_eq!(qust_alias_flags(&record), vec![0]);
    }

    #[test]
    fn pre_translate_player_producer_forces_exact_event_consumer_aliases_to_player() {
        for (script_name, flags) in [
            ("w05_mqr_202p_playerscript", 0),
            (
                "W05_MQR_PlayerVault79KeypadObjective",
                0x10 | QUST_ALIAS_OPTIONAL_FLAG,
            ),
        ] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            push_field(
                &mut record,
                "VMAD",
                qust_vmad_with_alias_scripts(&[(1, &[script_name])]),
            );
            push_field(
                &mut record,
                "ANAM",
                FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
            );
            push_qust_event_alias(
                &mut record,
                1,
                flags,
                FO76_QUEST_EVENT_SCPT,
                FO76_QUEST_EVENT_REFERENCE3,
            );

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert!(
                !record
                    .fields
                    .iter()
                    .any(|entry| matches!(&entry.sig.0, b"ALFE" | b"ALFD"))
            );
            let alfr = record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"ALFR")
                .expect("exact player-event consumer gets a forced reference");
            let FieldValue::FormKey(player) = &alfr.value else {
                panic!("ALFR should be a FormKey");
            };
            assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);
            assert_eq!(interner.resolve(player.plugin), Some(FO4_MASTER_NAME));
            assert_eq!(qust_alias_flags(&record), vec![flags]);
        }
    }

    #[test]
    fn pre_translate_forces_remove_players_alias_to_player() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_remove_players_aliases(&[0]),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_qust_event_alias(&mut record, 0, 0, u32::from_le_bytes(*b"CLOC"), 1);

        assert!(!qust_has_untranslatable_event_alias(&record));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(
            !record
                .fields
                .iter()
                .any(|entry| matches!(&entry.sig.0, b"ALFE" | b"ALFD"))
        );
        let alfr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"ALFR")
            .expect("remove-players alias gets a forced reference");
        let FieldValue::FormKey(player) = &alfr.value else {
            panic!("ALFR should be a FormKey");
        };
        assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);
        assert_eq!(interner.resolve(player.plugin), Some(FO4_MASTER_NAME));
    }

    #[test]
    fn pre_translate_forces_fragment_bound_player_alias_for_player_connect_event() {
        for property_name in ["Alias_Player", "aLiAs_cUrReNtPlAyEr"] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            push_field(
                &mut record,
                "VMAD",
                qust_vmad_with_fragment_alias_property(property_name, 0),
            );
            push_field(
                &mut record,
                "ANAM",
                FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
            );
            push_qust_event_alias(
                &mut record,
                0,
                0,
                u32::from_le_bytes(*b"PCON"),
                1,
            );

            assert!(!qust_has_untranslatable_event_alias(&record));
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            let alfr = record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"ALFR")
                .expect("fragment-bound player alias gets a forced reference");
            let FieldValue::FormKey(player) = &alfr.value else {
                panic!("ALFR should be a FormKey");
            };
            assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);
            assert_eq!(interner.resolve(player.plugin), Some(FO4_MASTER_NAME));
        }
    }

    #[test]
    fn fragment_player_alias_proof_requires_the_exact_property_name() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        for property_name in ["Alias_Players", "Alias_currentPlayers", "currentPlayer"] {
            let mut record = make_record("QUST", &interner);
            push_field(
                &mut record,
                "VMAD",
                qust_vmad_with_fragment_alias_property(property_name, 0),
            );
            push_qust_event_alias(
                &mut record,
                0,
                0,
                u32::from_le_bytes(*b"PCON"),
                1,
            );

            assert!(qust_has_untranslatable_event_alias(&record));
        }
    }

    #[test]
    fn unproven_event_alias_is_not_story_manager_safe() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_qust_event_alias(&mut record, 0, 0, u32::from_le_bytes(*b"CLOC"), 1);

        assert!(qust_has_untranslatable_event_alias(&record));
    }

    #[test]
    fn pre_translate_player_producer_near_names_use_generic_fallback() {
        for script_name in [
            "W05_MQR_202P_PlayerScriptHelper",
            "W05_MQR_202P_PlayerScrip",
            "W05_MQR_PlayerVault79KeypadObjectiveHelper",
            "W05_MQR_PlayerVault79KeypadObjectiv",
        ] {
            let interner = StringInterner::new();
            let fixture = qust_vmad_fixture(&[(1, &[script_name])]);

            let record = translate_daim_event_alias_with_vmad(&interner, fixture.bytes);

            assert_qust_event_alias_used_generic_fallback(&record);
        }
    }

    #[test]
    fn pre_translate_player_producer_rejects_mismatched_event_pairs() {
        for (script_name, event, event_data) in [
            (
                "W05_MQR_202P_PlayerScript",
                0x1234_5678,
                FO76_QUEST_EVENT_REFERENCE3,
            ),
            (
                "W05_MQR_PlayerVault79KeypadObjective",
                FO76_QUEST_EVENT_SCPT,
                1,
            ),
        ] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            push_field(
                &mut record,
                "VMAD",
                qust_vmad_with_alias_scripts(&[(1, &[script_name])]),
            );
            push_field(
                &mut record,
                "ANAM",
                FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
            );
            push_qust_event_alias(&mut record, 1, 0, event, event_data);

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_qust_event_alias_used_generic_fallback(&record);
        }
    }

    #[test]
    fn pre_translate_player_producer_rewrite_is_alias_scoped() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_alias_scripts(&[
                (1, &["W05_MQR_202P_PlayerScript"]),
                (2, &["W05_MQR_202P_PlayerScriptHelper"]),
            ]),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(3_u32.to_le_bytes().to_vec())),
        );
        push_qust_event_alias(
            &mut record,
            1,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );
        push_qust_event_alias(
            &mut record,
            2,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"ALFR")
                .count(),
            1
        );
        assert_eq!(qust_alias_flags(&record), vec![0, QUST_ALIAS_OPTIONAL_FLAG]);
    }

    #[test]
    fn pre_translate_daim_recognizer_rejects_unsupported_top_version() {
        let interner = StringInterner::new();
        let mut fixture = qust_vmad_fixture(&[(1, &["DefaultAliasInventoryManagement"])]);
        fixture.bytes[0..2].copy_from_slice(&(FO76_VMAD_VERSION + 1).to_le_bytes());

        let record = translate_daim_event_alias_with_vmad(&interner, fixture.bytes);

        assert_qust_event_alias_used_generic_fallback(&record);
    }

    #[test]
    fn pre_translate_daim_recognizer_rejects_object_format_one() {
        let interner = StringInterner::new();
        let mut fixture = qust_vmad_fixture(&[(1, &["DefaultAliasInventoryManagement"])]);
        fixture.bytes[2..4].copy_from_slice(&1_u16.to_le_bytes());

        let record = translate_daim_event_alias_with_vmad(&interner, fixture.bytes);

        assert_qust_event_alias_used_generic_fallback(&record);
    }

    #[test]
    fn pre_translate_daim_recognizer_rejects_wrong_fragment_version() {
        let interner = StringInterner::new();
        let mut fixture = qust_vmad_fixture(&[(1, &["DefaultAliasInventoryManagement"])]);
        fixture.bytes[fixture.fragment_version_offset] = FO76_QUST_FRAGMENT_VERSION - 1;

        let record = translate_daim_event_alias_with_vmad(&interner, fixture.bytes);

        assert_qust_event_alias_used_generic_fallback(&record);
    }

    #[test]
    fn pre_translate_daim_recognizer_rejects_wrong_alias_entry_version_or_format() {
        for corrupt_version in [true, false] {
            let interner = StringInterner::new();
            let mut fixture = qust_vmad_fixture(&[(1, &["DefaultAliasInventoryManagement"])]);
            let offset = if corrupt_version {
                fixture.alias_version_offsets[0]
            } else {
                fixture.alias_object_format_offsets[0]
            };
            fixture.bytes[offset..offset + 2].copy_from_slice(&1_u16.to_le_bytes());

            let record = translate_daim_event_alias_with_vmad(&interner, fixture.bytes);

            assert_qust_event_alias_used_generic_fallback(&record);
        }
    }

    #[test]
    fn pre_translate_daim_recognizer_rejects_representative_truncations() {
        let fixture = qust_vmad_fixture(&[(1, &["DefaultAliasInventoryManagement"])]);
        let truncation_offsets = [
            1,
            fixture.fragment_version_offset,
            fixture.fragment_version_offset + 1,
            fixture.alias_version_offsets[0] + 1,
            fixture.alias_object_format_offsets[0] + 1,
            fixture.alias_property_type_offsets[0] + 1,
            fixture.bytes.len() - 1,
        ];
        for truncation_offset in truncation_offsets {
            let interner = StringInterner::new();
            let mut bytes = fixture.bytes.clone();
            bytes.truncate(truncation_offset);

            let record = translate_daim_event_alias_with_vmad(&interner, bytes);

            assert_qust_event_alias_used_generic_fallback(&record);
        }
    }

    #[test]
    fn pre_translate_daim_recognizer_rejects_malformed_property_payload() {
        let interner = StringInterner::new();
        let mut fixture = qust_vmad_fixture(&[(1, &["DefaultAliasInventoryManagement"])]);
        fixture.bytes[fixture.alias_property_type_offsets[0]] = u8::MAX;

        let record = translate_daim_event_alias_with_vmad(&interner, fixture.bytes);

        assert_qust_event_alias_used_generic_fallback(&record);
    }

    #[test]
    fn pre_translate_daim_recognizer_rejects_trailing_garbage() {
        let interner = StringInterner::new();
        let mut fixture = qust_vmad_fixture(&[(1, &["DefaultAliasInventoryManagement"])]);
        fixture.bytes.extend_from_slice(&[0xAA, 0x55]);

        let record = translate_daim_event_alias_with_vmad(&interner, fixture.bytes);

        assert_qust_event_alias_used_generic_fallback(&record);
    }

    #[test]
    fn pre_translate_keeps_unproven_scpt_reference3_alias_on_generic_path() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_alias_scripts(&[(1, &["DefaultAliasInventoryManagementHelper"])]),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
        );
        push_qust_event_alias(
            &mut record,
            1,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(
            !record
                .fields
                .iter()
                .any(|entry| { matches!(&entry.sig.0, b"ALFE" | b"ALFD" | b"ALFR") })
        );
        assert_eq!(qust_alias_flags(&record), vec![QUST_ALIAS_OPTIONAL_FLAG]);
    }

    #[test]
    fn pre_translate_does_not_force_other_daim_event_pairs() {
        for (event, event_data) in [
            (0x1234_5678, FO76_QUEST_EVENT_REFERENCE3),
            (FO76_QUEST_EVENT_SCPT, 1),
        ] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            push_field(
                &mut record,
                "VMAD",
                qust_vmad_with_alias_scripts(&[(1, &["DefaultAliasInventoryManagement"])]),
            );
            push_field(
                &mut record,
                "ANAM",
                FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
            );
            push_qust_event_alias(&mut record, 1, 0, event, event_data);

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"ALFR"));
            assert_eq!(qust_alias_flags(&record), vec![QUST_ALIAS_OPTIONAL_FLAG]);
        }
    }

    #[test]
    fn pre_translate_daim_alias_rewrite_does_not_leak_to_sibling_aliases() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_alias_scripts(&[
                (1, &["DefaultAliasInventoryManagementA"]),
                (2, &["OtherAliasScript"]),
            ]),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(3_u32.to_le_bytes().to_vec())),
        );
        push_qust_event_alias(
            &mut record,
            1,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );
        push_qust_event_alias(
            &mut record,
            2,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"ALFR")
                .count(),
            1
        );
        assert_eq!(qust_alias_flags(&record), vec![0, QUST_ALIAS_OPTIONAL_FLAG]);
    }

    #[test]
    fn pre_translate_daim_alias_rewrite_preserves_authored_optional_flag() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_alias_scripts(&[(1, &["DefaultAliasInventoryManagementM"])]),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
        );
        push_qust_event_alias(
            &mut record,
            1,
            0x10 | QUST_ALIAS_OPTIONAL_FLAG,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            qust_alias_flags(&record),
            vec![0x10 | QUST_ALIAS_OPTIONAL_FLAG]
        );
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"ALFR")
                .count(),
            1
        );
    }

    #[test]
    fn build_fo4_qust_dnam_relayouts_20_byte_flags64_variant() {
        // FO76 form_version >= 202: flags is u64 (bytes 0..8).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8311_u64.to_le_bytes());
        data[8] = 5; // priority
        data[12..16].copy_from_slice(&1.5_f32.to_le_bytes()); // delay_time
        data[16] = 2; // quest_type

        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("dnam");
        assert_eq!(dnam.len(), FO4_QUST_DNAM_LEN);
        assert_eq!(u16::from_le_bytes([dnam[0], dnam[1]]), 0x8311);
        assert_eq!(dnam[0] & 0x01, 0x01, "start_game_enabled bit preserved");
        assert_eq!(dnam[2], 5, "priority");
        assert_eq!(f32::from_le_bytes(dnam[4..8].try_into().unwrap()), 1.5);
        assert_eq!(dnam[8], FO4_QUST_TYPE_SIDE_QUESTS, "quest_type");
    }
