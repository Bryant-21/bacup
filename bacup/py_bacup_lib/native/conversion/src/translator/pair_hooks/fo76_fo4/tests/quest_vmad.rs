

    fn w05_mqs_205_vmad(include_shutdown_script: bool) -> Vec<u8> {
        fn write_string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }

        fn write_script(bytes: &mut Vec<u8>, name: &str, property_value: i32) {
            write_string(bytes, name);
            bytes.push(0);
            bytes.extend_from_slice(&1_u16.to_le_bytes());
            write_string(bytes, "StageToSet");
            bytes.push(3);
            bytes.push(1);
            bytes.extend_from_slice(&property_value.to_le_bytes());
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FO76_VMAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&FO76_VMAD_OBJECT_FORMAT.to_le_bytes());
        let script_count = if include_shutdown_script { 3_u16 } else { 2_u16 };
        bytes.extend_from_slice(&script_count.to_le_bytes());
        write_script(&mut bytes, "KeepBefore", 700);
        if include_shutdown_script {
            write_string(&mut bytes, "DefaultQuestShutdownScript");
            bytes.push(0);
            bytes.extend_from_slice(&0_u16.to_le_bytes());
        }
        write_script(&mut bytes, "KeepAfter", 800);

        bytes.push(FO76_QUST_FRAGMENT_VERSION);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        write_string(&mut bytes, "DefaultQuestShutdownScript");
        bytes.push(0);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes
    }

    #[test]
    fn pre_translate_strips_only_w05_mqs_205_root_shutdown_script_binding() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = 0x0041_CB6D;
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(w05_mqs_205_vmad(true))),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(SmallVec::from_vec(w05_mqs_205_vmad(false)))
        );
    }

    #[test]
    fn pre_translate_keeps_matching_shutdown_script_on_other_plugins() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = 0x0041_CB6D;
        record.form_key.plugin = interner.intern("Other.esm");
        let vmad = FieldValue::Bytes(SmallVec::from_vec(w05_mqs_205_vmad(true)));
        push_field(&mut record, "VMAD", vmad.clone());

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.fields[0].value, vmad);
    }

    #[test]
    fn pre_translate_maps_recollectionalias_properties_to_fo4_contract() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_re_alias_properties(
                "recollectionaliasscript",
                &[
                    ("groupIndex", 3, 1_i32.to_le_bytes().to_vec()),
                    ("Alias_SpawnMapCenter", 1, vec![0; 8]),
                    ("TrackOnDying", 5, vec![1]),
                    ("minimumCount", 3, 8_i32.to_le_bytes().to_vec()),
                    (
                        "RequiredStageToFireTrackDeath",
                        3,
                        20_i32.to_le_bytes().to_vec(),
                    ),
                ],
            ),
        );
        let expected = qust_vmad_with_re_alias_properties(
            "recollectionaliasscript",
            &[
                ("groupIndex", 3, 1_i32.to_le_bytes().to_vec()),
                ("TrackDeath", 5, vec![1]),
            ],
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.fields[0].value, expected);
    }

    #[test]
    fn pre_translate_maps_realiasscript_properties_to_fo4_contract() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_re_alias_properties(
                "REAliasScript",
                &[
                    ("PreferFurniture", 5, vec![1]),
                    ("TrackOnDying", 5, vec![1]),
                    ("OnHitStage", 3, 40_i32.to_le_bytes().to_vec()),
                ],
            ),
        );
        let expected = qust_vmad_with_re_alias_properties(
            "REAliasScript",
            &[
                ("TrackDeath", 5, vec![1]),
                ("OnHitStage", 3, 40_i32.to_le_bytes().to_vec()),
            ],
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.fields[0].value, expected);
    }

    #[test]
    fn pre_translate_merges_re_alias_death_tracking_flags() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_re_alias_properties(
                "REAliasScript",
                &[
                    ("TrackDeath", 5, vec![0]),
                    ("TrackOnDying", 5, vec![1]),
                ],
            ),
        );
        let expected = qust_vmad_with_re_alias_properties(
            "REAliasScript",
            &[("TrackDeath", 5, vec![1])],
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.fields[0].value, expected);
    }

    #[test]
    fn pre_translate_leaves_other_alias_scripts_unchanged() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        let vmad = qust_vmad_with_re_alias_properties(
            "OtherAliasScript",
            &[("TrackOnDying", 5, vec![1])],
        );
        push_field(&mut record, "VMAD", vmad.clone());

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.fields[0].value, vmad);
    }

    #[test]
    fn pre_translate_leaves_malformed_re_alias_vmad_unchanged() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        let FieldValue::Bytes(mut bytes) = qust_vmad_with_re_alias_properties(
            "REAliasScript",
            &[("TrackOnDying", 5, vec![1])],
        ) else {
            unreachable!();
        };
        bytes.pop();
        let malformed = FieldValue::Bytes(bytes);
        push_field(&mut record, "VMAD", malformed.clone());

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.fields[0].value, malformed);
    }

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
    fn pre_translate_preserves_daily_required_location_and_player_aliases() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_remove_players_aliases(&[2]),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(3_u32.to_le_bytes().to_vec())),
        );
        push_qust_location_event_alias(
            &mut record,
            1,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO4_QUEST_EVENT_LOCATION1,
        );
        push_qust_event_alias(
            &mut record,
            2,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );

        assert!(!qust_has_untranslatable_event_alias(&record));

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let event_fill: Vec<([u8; 4], u32)> = record
            .fields
            .iter()
            .filter(|entry| matches!(&entry.sig.0, b"ALFE" | b"ALFD"))
            .map(|entry| {
                (
                    entry.sig.0,
                    field_value_to_u32(&entry.value).expect("u32 event fill"),
                )
            })
            .collect();
        assert_eq!(
            event_fill,
            vec![
                (*b"ALFE", FO76_QUEST_EVENT_SCPT),
                (*b"ALFD", FO4_QUEST_EVENT_LOCATION1),
            ]
        );
        let player_alias = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"ALFR")
            .expect("player alias forced to the FO4 player");
        let FieldValue::FormKey(player) = &player_alias.value else {
            panic!("ALFR should be a FormKey");
        };
        assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);
        assert_eq!(interner.resolve(player.plugin), Some(FO4_MASTER_NAME));
        assert_eq!(qust_alias_flags(&record), vec![0, 0]);
    }

    #[test]
    fn pre_translate_rejects_unproven_required_script_event_location_alias() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
        );
        push_qust_location_event_alias(
            &mut record,
            1,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO4_QUEST_EVENT_LOCATION1 - 1,
        );

        assert!(qust_has_untranslatable_event_alias(&record));

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(
            !record
                .fields
                .iter()
                .any(|entry| matches!(&entry.sig.0, b"ALFE" | b"ALFD"))
        );
        assert_eq!(qust_alias_flags(&record), vec![QUST_ALIAS_OPTIONAL_FLAG]);
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

    fn owning_player_fragment_record(
        interner: &StringInterner,
        alias_name: &str,
        include_alfe: bool,
        include_alfd: bool,
    ) -> Record {
        let mut record = make_record("QUST", interner);
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_fragment_alias_property("Alias_owningPlayer", 1),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        let mut alias_name = alias_name.as_bytes().to_vec();
        alias_name.push(0);
        push_field(
            &mut record,
            "ALID",
            FieldValue::Bytes(SmallVec::from_vec(alias_name)),
        );
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        if include_alfe {
            push_field(
                &mut record,
                "ALFE",
                FieldValue::Bytes(SmallVec::from_vec(
                    FO76_QUEST_EVENT_SCPT.to_le_bytes().to_vec(),
                )),
            );
        }
        if include_alfd {
            push_field(
                &mut record,
                "ALFD",
                FieldValue::Bytes(SmallVec::from_vec(
                    FO76_QUEST_EVENT_REFERENCE3.to_le_bytes().to_vec(),
                )),
            );
        }
        push_field(&mut record, "ALED", FieldValue::None);
        record
    }

    #[test]
    fn owning_player_fragment_property_proves_matching_source_event_alias() {
        let interner = StringInterner::new();
        let mut record = owning_player_fragment_record(&interner, "owningPlayer", true, true);

        assert!(!qust_has_untranslatable_event_alias(&record));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let alfr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"ALFR")
            .expect("verified owningPlayer alias gets a forced reference");
        let FieldValue::FormKey(player) = &alfr.value else {
            panic!("ALFR should be a FormKey");
        };
        assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);
        assert_eq!(interner.resolve(player.plugin), Some(FO4_MASTER_NAME));
    }

    /// An unresolvable event fill on an OPTIONAL alias must not disqualify the
    /// quest: FO4 leaves the alias empty and the quest still starts. If
    /// `classify_quest` rejected it, its Story Manager node would be skipped, and
    /// an event-scoped quest with no SMQN can never start. `EN01_Misc` ("Uncle
    /// Sam") has event fills only on `NoteObject`/`NoteObjectStory`, both
    /// `FNAM = 0x06` (Optional set).
    #[test]
    fn optional_alias_with_unresolvable_event_fill_does_not_disqualify_the_quest() {
        const EN01_MISC_OPTIONAL_ALIAS_FLAGS: u32 = 0x0000_0006;
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_qust_event_alias(
            &mut record,
            11,
            EN01_MISC_OPTIONAL_ALIAS_FLAGS,
            FO76_QUEST_EVENT_SCPT,
            1,
        );

        assert!(!qust_has_untranslatable_event_alias(&record));
    }

    /// The guard still has to fire for a NON-optional alias — that one really
    /// can abort the quest start.
    #[test]
    fn required_alias_with_unresolvable_event_fill_still_disqualifies_the_quest() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_qust_event_alias(&mut record, 11, 0, FO76_QUEST_EVENT_SCPT, 1);

        assert!(qust_has_untranslatable_event_alias(&record));
    }

    #[test]
    fn gq_workshop_clear_keeps_its_required_workshop_reference1_fill() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = FO76_GQ_WORKSHOP_CLEAR_FORM_ID;
        push_qust_event_alias(
            &mut record,
            0,
            0x4000_0100,
            FO76_QUEST_EVENT_SCPT,
            FO4_QUEST_EVENT_REFERENCE1,
        );
        let alias_name = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"ALID")
            .expect("Workshop alias name");
        alias_name.value = FieldValue::Bytes(SmallVec::from_slice(b"Workshop\0"));

        assert!(!qust_has_untranslatable_event_alias(&record));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(record.fields.iter().any(|entry| {
            entry.sig.0 == *b"ALFE"
                && field_value_to_u32(&entry.value) == Some(FO76_QUEST_EVENT_SCPT)
        }));
        assert!(record.fields.iter().any(|entry| {
            entry.sig.0 == *b"ALFD"
                && field_value_to_u32(&entry.value) == Some(FO4_QUEST_EVENT_REFERENCE1)
        }));
    }

    #[test]
    fn tw002_keeps_only_its_exact_event_trigger_reference1_fill() {
        fn tw002_quest(
            interner: &StringInterner,
            local: u32,
            editor_id: &str,
            alias_name: &[u8],
        ) -> Record {
            let mut record = make_record("QUST", interner);
            record.form_key.local = local;
            record.eid = Some(interner.intern(editor_id));
            push_qust_event_alias(
                &mut record,
                FO76_TW002_EVENT_TRIGGER_ALIAS_ID,
                0,
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            );
            record
                .fields
                .iter_mut()
                .find(|entry| entry.sig.0 == *b"ALID")
                .expect("TW002 event alias name")
                .value = FieldValue::Bytes(SmallVec::from_slice(alias_name));
            record
        }

        let interner = StringInterner::new();
        let mut exact = tw002_quest(
            &interner,
            FO76_TW002_FORM_ID,
            "TW002",
            b"EventTrigger\0",
        );
        assert!(!qust_has_untranslatable_event_alias_for_source(
            &interner, &exact
        ));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut exact)
            .unwrap();
        assert!(exact.fields.iter().any(|entry| {
            entry.sig.0 == *b"ALFE"
                && field_value_to_u32(&entry.value) == Some(FO76_QUEST_EVENT_SCPT)
        }));
        assert!(exact.fields.iter().any(|entry| {
            entry.sig.0 == *b"ALFD"
                && field_value_to_u32(&entry.value) == Some(FO4_QUEST_EVENT_REFERENCE1)
        }));

        for (local, editor_id, alias_name) in [
            (
                FO76_TW002_FORM_ID + 1,
                "TW002",
                b"EventTrigger\0".as_slice(),
            ),
            (
                FO76_TW002_FORM_ID,
                "TW002_UnsafeCopy",
                b"EventTrigger\0".as_slice(),
            ),
            (
                FO76_TW002_FORM_ID,
                "TW002",
                b"EventTriggers\0".as_slice(),
            ),
        ] {
            let lookalike = tw002_quest(&interner, local, editor_id, alias_name);
            assert!(qust_has_untranslatable_event_alias_for_source(
                &interner, &lookalike
            ));
        }

        let mut foreign = tw002_quest(
            &interner,
            FO76_TW002_FORM_ID,
            "TW002",
            b"EventTrigger\0",
        );
        foreign.form_key.plugin = interner.intern("Foreign.esm");
        assert!(qust_has_untranslatable_event_alias_for_source(
            &interner, &foreign
        ));
    }

    #[test]
    fn workshop_reference1_proof_requires_the_exact_quest_and_alias_name() {
        for (local, alias_id, alias_name, event, event_data) in [
            (
                FO76_GQ_WORKSHOP_CLEAR_FORM_ID + 1,
                0,
                b"Workshop".as_slice(),
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            ),
            (
                FO76_GQ_WORKSHOP_CLEAR_FORM_ID,
                0,
                b"Workshops".as_slice(),
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            ),
            (
                FO76_GQ_WORKSHOP_CLEAR_FORM_ID,
                0,
                b"Workshop".as_slice(),
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE2,
            ),
            (
                FO76_GQ_WORKSHOP_CLEAR_FORM_ID,
                0,
                b"Workshop".as_slice(),
                u32::from_le_bytes(*b"PCON"),
                FO4_QUEST_EVENT_REFERENCE1,
            ),
            (
                FO76_GQ_WORKSHOP_CLEAR_FORM_ID,
                1,
                b"Workshop".as_slice(),
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            ),
        ] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            record.form_key.local = local;
            push_qust_event_alias(
                &mut record,
                alias_id,
                0x4000_0100,
                event,
                event_data,
            );
            let alias = record
                .fields
                .iter_mut()
                .find(|entry| entry.sig.0 == *b"ALID")
                .expect("alias name");
            let mut value = alias_name.to_vec();
            value.push(0);
            alias.value = FieldValue::Bytes(SmallVec::from_vec(value));

            assert!(qust_has_untranslatable_event_alias(&record));
        }
    }

    #[test]
    fn owning_player_fragment_property_rejects_a_different_quest() {
        let interner = StringInterner::new();
        let mut record = owning_player_fragment_record(&interner, "owningPlayer", true, true);
        record.form_key.local += 1;

        assert!(qust_has_untranslatable_event_alias(&record));
    }

    #[test]
    fn owning_player_fragment_property_rejects_a_different_alias() {
        let interner = StringInterner::new();
        let mut record = owning_player_fragment_record(&interner, "owningPlayer", true, true);
        let alst = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"ALST")
            .expect("reference alias");
        alst.value = FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec()));

        assert!(qust_has_untranslatable_event_alias(&record));
    }

    #[test]
    fn owning_player_fragment_property_requires_exact_alias_name() {
        let interner = StringInterner::new();
        let record = owning_player_fragment_record(&interner, "owningPlayers", true, true);

        assert!(qust_has_untranslatable_event_alias(&record));
    }

    #[test]
    fn owning_player_fragment_property_requires_complete_source_event_fill() {
        for (include_alfe, include_alfd) in [(true, false), (false, true)] {
            let interner = StringInterner::new();
            let record = owning_player_fragment_record(
                &interner,
                "owningPlayer",
                include_alfe,
                include_alfd,
            );

            assert!(qust_has_untranslatable_event_alias(&record));
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
    fn pre_translate_repairs_cb00_mine_repair_player_event_slot() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = FO76_CB00_MINE_REPAIR_FORM_ID;
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALID",
            FieldValue::Bytes(SmallVec::from_slice(b"Player\0")),
        );
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFE",
            FieldValue::Bytes(SmallVec::from_vec(
                FO76_QUEST_EVENT_SCPT.to_le_bytes().to_vec(),
            )),
        );
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(
                FO76_QUEST_EVENT_REFERENCE3.to_le_bytes().to_vec(),
            )),
        );
        push_field(&mut record, "ALED", FieldValue::None);
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALID",
            FieldValue::Bytes(SmallVec::from_slice(b"MINE\0")),
        );
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFE",
            FieldValue::Bytes(SmallVec::from_vec(
                FO76_QUEST_EVENT_SCPT.to_le_bytes().to_vec(),
            )),
        );
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(
                FO4_QUEST_EVENT_REFERENCE2.to_le_bytes().to_vec(),
            )),
        );
        push_field(&mut record, "ALED", FieldValue::None);

        assert!(!qust_has_untranslatable_event_alias(&record));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(record.fields.iter().any(|entry| {
            entry.sig.0 == *b"ALFE"
                && field_value_to_u32(&entry.value) == Some(FO76_QUEST_EVENT_SCPT)
        }));
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"ALFD")
                .filter_map(|entry| field_value_to_u32(&entry.value))
                .collect::<Vec<_>>(),
            vec![FO4_QUEST_EVENT_REFERENCE1, FO4_QUEST_EVENT_REFERENCE2]
        );
        assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"ALFR"));
        assert_eq!(qust_alias_flags(&record), vec![0, 0]);
    }

    #[test]
    fn pre_translate_preserves_workshop_vertibird_attack_target_event_slot() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = FO76_SQ_WORKSHOP_VERTIBIRD_FORM_ID;
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALID",
            FieldValue::Bytes(SmallVec::from_slice(b"AttackTarget\0")),
        );
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFE",
            FieldValue::Bytes(SmallVec::from_vec(
                FO76_QUEST_EVENT_SCPT.to_le_bytes().to_vec(),
            )),
        );
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(
                FO4_QUEST_EVENT_REFERENCE1.to_le_bytes().to_vec(),
            )),
        );
        push_field(&mut record, "ALED", FieldValue::None);

        assert!(!qust_has_untranslatable_event_alias(&record));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(record.fields.iter().any(|entry| {
            entry.sig.0 == *b"ALFE"
                && field_value_to_u32(&entry.value) == Some(FO76_QUEST_EVENT_SCPT)
        }));
        assert!(record.fields.iter().any(|entry| {
            entry.sig.0 == *b"ALFD"
                && field_value_to_u32(&entry.value) == Some(FO4_QUEST_EVENT_REFERENCE1)
        }));
        assert_eq!(qust_alias_flags(&record), vec![0]);
    }

    #[test]
    fn pre_translate_preserves_exact_companion_runtime_event_aliases() {
        for (local, eid, aliases) in [
            (
                0x0054_F1A4,
                "COMP_RQ_Fetch",
                [(1, FO4_QUEST_EVENT_REFERENCE1), (13, FO4_QUEST_EVENT_REFERENCE2)],
            ),
            (
                0x0056_FB76,
                "COMP_RQ_Kill",
                [(1, FO4_QUEST_EVENT_REFERENCE1), (18, FO4_QUEST_EVENT_REFERENCE2)],
            ),
            (
                0x0057_27AD,
                "COMP_RQ_Rescue",
                [(1, FO4_QUEST_EVENT_REFERENCE1), (18, FO4_QUEST_EVENT_REFERENCE2)],
            ),
            (
                0x0055_FD53,
                "COMP_Visitor",
                [(0, FO4_QUEST_EVENT_REFERENCE1), (1, FO4_QUEST_EVENT_REFERENCE2)],
            ),
        ] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            record.form_key.local = local;
            record.eid = Some(interner.intern(eid));
            push_field(
                &mut record,
                "ANAM",
                FieldValue::Bytes(SmallVec::from_vec(20_u32.to_le_bytes().to_vec())),
            );
            for (alias_id, event_data) in aliases {
                push_qust_event_alias(
                    &mut record,
                    alias_id,
                    0,
                    FO76_QUEST_EVENT_SCPT,
                    event_data,
                );
            }

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(
                record
                    .fields
                    .iter()
                    .filter(|entry| entry.sig.0 == *b"ALFD")
                    .filter_map(|entry| field_value_to_u32(&entry.value))
                    .collect::<Vec<_>>(),
                aliases.map(|(_, event_data)| event_data),
                "{eid}"
            );
            assert_eq!(qust_alias_flags(&record), vec![0, 0], "{eid}");
        }
    }

    #[test]
    fn pre_translate_preserves_beckett_specific_alias_wrapper_players_and_companions() {
        for (local, eid) in [
            (
                0x0058_215B,
                "COMP_RQ_Fetch_SpecificAliases_Beckett_000_SadDiary",
            ),
            (
                0x0058_2164,
                "COMP_RQ_Rescue_SpecificAliases_Beckett_001_CultistSage",
            ),
            (
                0x0058_2163,
                "COMP_RQ_Fetch_SpecificAliases_Beckett_002_Key",
            ),
            (
                0x0058_2160,
                "COMP_RQ_Kill_SpecificAliases_Beckett_003_Bronx",
            ),
            (
                0x0058_2167,
                "COMP_RQ_Fetch_SpecificAliases_Beckett_004_Cave",
            ),
            (
                0x0058_2165,
                "COMP_RQ_Kill_SpecificAliases_Beckett_005_Blood",
            ),
            (
                0x0058_215A,
                "COMP_RQ_Rescue_SpecificAliases_Beckett_006_Pet",
            ),
            (
                0x0058_215E,
                "COMP_RQ_Kill_SpecificAliases_Beckett_007_DJ",
            ),
            (
                0x0058_216A,
                "COMP_RQ_Rescue_SpecificAliases_Beckett_008_MissNanny",
            ),
            (
                0x0058_2168,
                "COMP_RQ_Fetch_SpecificAliases_Beckett_009_Holotapes",
            ),
            (
                0x0058_215F,
                "COMP_RQ_Fetch_SpecificAliases_Beckett_010_PoisonedFood",
            ),
            (
                0x0058_215D,
                "COMP_RQ_Kill_SpecificAliases_Beckett_011_Eye",
            ),
            (
                0x005A_272F,
                "COMP_RQ_Kill_SpecificAliases_Beckett_BloodEagleRandomLoc",
            ),
            (
                0x005A_2730,
                "COMP_RQ_Kill_SpecificAliases_Beckett_BloodEagleDungeon",
            ),
            (0x005A_272D, "COMP_RQ_Fetch_SpecificAliases_LegendaryArmor"),
            (0x005A_272E, "COMP_RQ_Fetch_SpecificAliases_LegendaryWeapon"),
        ] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            record.form_key.local = local;
            record.eid = Some(interner.intern(eid));
            push_field(
                &mut record,
                "VMAD",
                qust_vmad_with_remove_players_aliases(&[0]),
            );
            push_field(
                &mut record,
                "ANAM",
                FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
            );
            push_qust_event_alias(
                &mut record,
                0,
                0,
                FO76_QUEST_EVENT_SCPT,
                FO76_QUEST_EVENT_REFERENCE3,
            );
            push_qust_event_alias(
                &mut record,
                1,
                0,
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            );

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert!(record.fields.iter().any(|entry| {
                entry.sig.0 == *b"ALFR"
                    && matches!(&entry.value, FieldValue::FormKey(form_key) if form_key.local == FO4_PLAYER_REF_FORM_ID)
            }), "{eid}");
            assert_eq!(
                record
                    .fields
                    .iter()
                    .filter(|entry| entry.sig.0 == *b"ALFD")
                    .filter_map(|entry| field_value_to_u32(&entry.value))
                    .collect::<Vec<_>>(),
                vec![FO4_QUEST_EVENT_REFERENCE1],
                "{eid}"
            );
            assert_eq!(qust_alias_flags(&record), vec![0, 0], "{eid}");
        }
    }

    #[test]
    fn companion_runtime_event_alias_adapter_rejects_lookalikes() {
        for foreign_plugin in [false, true] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            record.form_key.local = 0x0054_F1A4;
            record.eid = Some(interner.intern(if foreign_plugin {
                "COMP_RQ_Fetch"
            } else {
                "COMP_RQ_Fetch_Lookalike"
            }));
            if foreign_plugin {
                record.form_key.plugin = interner.intern("Other.esm");
            }
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
                FO4_QUEST_EVENT_REFERENCE1,
            );

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"ALFE"));
            assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"ALFD"));
            assert_eq!(qust_alias_flags(&record), vec![QUST_ALIAS_OPTIONAL_FLAG]);
        }
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

    fn rd01_enc02_vmad(include_dead_topics: bool) -> Vec<u8> {
        fn write_string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }

        fn write_object_property(bytes: &mut Vec<u8>, name: &str, form_id: u32) {
            write_string(bytes, name);
            bytes.push(1);
            bytes.push(1);
            bytes.extend_from_slice(&form_id.to_le_bytes());
            bytes.extend_from_slice(&0_u32.to_le_bytes());
        }

        let dead_topics = [
            ("kDifficultyMaxedTopic", 0x0879_937D),
            ("kDifficultyIncreasedTopic", 0x0879_937C),
            ("kEncounterStartTopic", 0x0879_6EEE),
        ];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FO76_VMAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&FO76_VMAD_OBJECT_FORMAT.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());

        write_string(&mut bytes, "Raids:RD01:Enc02:QuestScript");
        bytes.push(0);
        let property_count = 1 + usize::from(include_dead_topics) * dead_topics.len();
        bytes.extend_from_slice(&(property_count as u16).to_le_bytes());
        write_string(&mut bytes, "iMaxDifficulty");
        bytes.push(3);
        bytes.push(1);
        bytes.extend_from_slice(&5_i32.to_le_bytes());
        if include_dead_topics {
            for (name, form_id) in dead_topics {
                write_object_property(&mut bytes, name, form_id);
            }
        }

        write_string(&mut bytes, "OtherScript");
        bytes.push(0);
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_object_property(&mut bytes, "kEncounterStartTopic", 0x0879_6EEE);

        bytes.push(FO76_QUST_FRAGMENT_VERSION);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        write_string(&mut bytes, "");
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes
    }

    #[test]
    fn pre_translate_strips_only_rd01_enc02_dead_topic_bindings() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = FO76_RD01_ENC02_FORM_ID;
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(rd01_enc02_vmad(true))),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(SmallVec::from_vec(rd01_enc02_vmad(false)))
        );
    }
