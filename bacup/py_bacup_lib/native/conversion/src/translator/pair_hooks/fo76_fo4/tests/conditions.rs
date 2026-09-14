
    fn raw_condition_fixture(function_id: u16) -> Vec<u8> {
        let mut bytes = vec![
            0xA4, 0x11, 0x22, 0x33, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x55, 0x66, 0xEF, 0xBE,
            0xAD, 0xDE, 0x78, 0x56, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x0D, 0xF0, 0xAD, 0x0B,
            0xCA, 0xFE, 0xBA, 0xBE,
        ];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes
    }

    fn remapped_get_is_player_bytes(source: &[u8]) -> Vec<u8> {
        let mut expected = source.to_vec();
        expected[8..10].copy_from_slice(&FO4_GET_IS_ID_CONDITION_FUNCTION_ID.to_le_bytes());
        expected[12..16].copy_from_slice(&FO4_PLAYER_ACTOR_FORM_ID.to_le_bytes());
        expected
    }

    fn raw_ctda_from_hex(raw_hex: &str) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(
            hex::decode(raw_hex).expect("valid CTDA fixture hex"),
        ))
    }

    fn assert_story_manager_quest_start_condition_lowering(
        quest_form_id: u32,
        input_conditions: [&str; 3],
    ) {
        let interner = StringInterner::new();
        let mut record = make_record("SMQN", &interner);
        push_field(&mut record, "CITC", raw_bytes(&3_u32.to_le_bytes()));
        for condition in input_conditions {
            push_field(&mut record, "CTDA", raw_ctda_from_hex(condition));
        }

        let original_start = record
            .fields
            .iter()
            .find(|entry| Fo76Fo4Hook::story_manager_start_keyword(&entry.value).is_some())
            .expect("exact K1 start discriminator")
            .value
            .clone();
        for entry in &mut record.fields {
            if Fo76Fo4Hook::story_manager_active_keyword(&entry.value).is_some() {
                assert!(Fo76Fo4Hook::lower_story_manager_active_keyword_condition(
                    &mut entry.value,
                    quest_form_id,
                ));
            } else {
                Fo76Fo4Hook::lower_story_manager_completion_condition(&mut entry.value);
            }
        }

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let conditions: Vec<&FieldValue> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"CTDA")
            .map(|entry| &entry.value)
            .collect();
        assert_eq!(
            conditions.len(),
            3,
            "all three quest-start gates must survive"
        );
        assert_eq!(conditions[0], &original_start);
        assert_eq!(
            Fo76Fo4Hook::condition_function_id(&interner, conditions[0]),
            Some(FO76_GET_EVENT_DATA_CONDITION_FUNCTION_ID),
            "the K1 start-keyword discriminator must remain byte-for-byte"
        );

        assert_eq!(
            Fo76Fo4Hook::condition_function_id(&interner, conditions[1]),
            Some(FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID)
        );
        assert_eq!(
            Fo76Fo4Hook::condition_parameter_1(&interner, conditions[1])
                .map(|parameter| parameter & 0x00FF_FFFF),
            Some(quest_form_id)
        );
        assert_eq!(
            Fo76Fo4Hook::condition_function_id(&interner, conditions[2]),
            Some(FO4_GET_QUEST_COMPLETED_CONDITION_FUNCTION_ID)
        );
        assert_eq!(
            Fo76Fo4Hook::condition_parameter_1(&interner, conditions[2])
                .map(|parameter| parameter & 0x00FF_FFFF),
            Some(quest_form_id)
        );
        for condition in &conditions[1..] {
            let FieldValue::Bytes(bytes) = condition else {
                panic!("raw CTDA expected");
            };
            assert_eq!(Fo76Fo4Hook::raw_condition_parameter_2(bytes), Some(0));
            assert_eq!(Fo76Fo4Hook::raw_condition_run_on(bytes), Some(0));
            assert_eq!(Fo76Fo4Hook::raw_condition_reference(bytes), Some(0));
            assert_eq!(
                Fo76Fo4Hook::raw_condition_parameter_3(bytes),
                Some(u32::MAX)
            );
        }

        let condition_count = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CITC")
            .expect("condition count remains");
        assert_eq!(condition_count.value, raw_bytes(&3_u32.to_le_bytes()));
    }

    #[test]
    fn structural_lowering_preserves_exact_w05_mq001p_wayward_start_conditions() {
        assert_story_manager_quest_start_condition_lowering(
            0x0040_5E14,
            [
                "000B00000000803F4002944300004B31C65E40000000000000000000FFFFFFFF",
                "000000000000000030029443C85E400000000000070000000000000052330000",
                "A00000000000000059039443145E400000000000070000000000000052330000",
            ],
        );
    }

    #[test]
    fn structural_lowering_preserves_exact_w05_mq001p_lacey_isela_start_conditions() {
        assert_story_manager_quest_start_condition_lowering(
            0x0040_5E15,
            [
                "000B00000000803F4002944300004B31C75E40000000000000000000FFFFFFFF",
                "000000000000000030029443C95E400000000000070000000000000052330000",
                "A00000000000000059039443155E400000000000070000000000000052330000",
            ],
        );
    }

    #[test]
    fn story_manager_condition_shape_checks_reject_non_r3_and_wrong_operators() {
        let non_r3 =
            raw_ctda_from_hex("000000000000000030029443C85E4000000000000700000000000000FFFFFFFF");
        let wrong_operator =
            raw_ctda_from_hex("800000000000000059039443145E400000000000070000000000000052330000");
        assert_eq!(Fo76Fo4Hook::story_manager_active_keyword(&non_r3), None);
        assert_eq!(
            Fo76Fo4Hook::story_manager_completion_quest(&wrong_operator),
            None
        );
    }

    #[test]
    fn story_manager_completion_shape_accepts_equal_zero() {
        let mut equal_zero =
            raw_ctda_from_hex("00000000000000005903944331BF560000000000070000000000000052330000");

        assert_eq!(
            Fo76Fo4Hook::story_manager_completion_quest(&equal_zero),
            Some(0x0056_BF31)
        );
        assert_eq!(
            Fo76Fo4Hook::lower_quest_completion_count_condition(&mut equal_zero),
            Some(true)
        );
        let FieldValue::Bytes(bytes) = equal_zero else {
            panic!("raw CTDA bytes");
        };
        assert_eq!(
            Fo76Fo4Hook::raw_condition_function_id(&bytes),
            Some(FO4_GET_QUEST_COMPLETED_CONDITION_FUNCTION_ID)
        );
        assert_eq!(Fo76Fo4Hook::raw_condition_operator(&bytes), Some(0));
        assert_eq!(
            Fo76Fo4Hook::raw_condition_comparison_value(&bytes),
            Some(0.0)
        );
        assert_eq!(
            Fo76Fo4Hook::raw_condition_parameter_1(&bytes),
            Some(0x0056_BF31)
        );
        assert_eq!(Fo76Fo4Hook::raw_condition_parameter_2(&bytes), Some(0));
        assert_eq!(Fo76Fo4Hook::raw_condition_run_on(&bytes), Some(0));
        assert_eq!(Fo76Fo4Hook::raw_condition_reference(&bytes), Some(0));
        assert_eq!(
            Fo76Fo4Hook::raw_condition_parameter_3(&bytes),
            Some(u32::MAX)
        );
    }

    #[test]
    fn post_translate_remaps_get_is_player_on_acti_to_get_is_id_player() {
        let interner = StringInterner::new();
        let mut record = make_record("ACTI", &interner);
        let source = raw_condition_fixture(FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID);
        push_field(
            &mut record,
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(source.clone())),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDA")
            .expect("remapped CTDA remains");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(bytes.as_slice(), remapped_get_is_player_bytes(&source));
        assert_eq!(Fo76Fo4Hook::raw_condition_run_on(bytes), Some(0));
        assert!(
            FO76_REMAPPED_CONDITION_FUNCTION_IDS
                .contains(&FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID)
        );
    }

    #[test]
    fn post_translate_remaps_get_is_player_ctdt_on_non_acti_record() {
        let interner = StringInterner::new();
        let mut record = make_record("MGEF", &interner);
        let source = raw_condition_fixture(FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID);
        push_field(
            &mut record,
            "CTDT",
            FieldValue::Bytes(SmallVec::from_vec(source.clone())),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let ctdt = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDT")
            .expect("remapped CTDT remains");
        let FieldValue::Bytes(bytes) = &ctdt.value else {
            panic!("expected raw CTDT bytes");
        };
        assert_eq!(bytes.as_slice(), remapped_get_is_player_bytes(&source));
        assert_eq!(Fo76Fo4Hook::raw_condition_run_on(bytes), Some(0));
    }

    #[test]
    fn post_translate_leaves_non_get_is_player_condition_bytes_unchanged() {
        let interner = StringInterner::new();
        let mut record = make_record("ACTI", &interner);
        let source = raw_condition_fixture(203);
        push_field(
            &mut record,
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(source.clone())),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDA")
            .expect("FO4-compatible CTDA remains");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(bytes.as_slice(), source);
    }

    #[test]
    fn post_translate_scopes_wayward_gender_branches_to_player_ref() {
        let interner = StringInterner::new();
        for (local, source, expected) in [
            (
                0x56_A169,
                "000000000000803F4600000000000000000000000100000000000000FFFFFFFF",
                "000000000000803F4600000000000000000000000200000014000000FFFFFFFF",
            ),
            (
                0x56_A16A,
                "000000000000803F4600000001000000000000000100000000000000FFFFFFFF",
                "000000000000803F4600000001000000000000000200000014000000FFFFFFFF",
            ),
            (
                0x56_A178,
                "000000000000803F4600000000000000000000000100000000000000FFFFFFFF",
                "000000000000803F4600000000000000000000000200000014000000FFFFFFFF",
            ),
            (
                0x56_A179,
                "000000000000803F4600000001000000000000000100000000000000FFFFFFFF",
                "000000000000803F4600000001000000000000000200000014000000FFFFFFFF",
            ),
        ] {
            let mut record = make_record("INFO", &interner);
            record.form_key.local = local;
            push_field(&mut record, "CTDA", raw_ctda_from_hex(source));

            Fo76Fo4Hook
                .post_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected raw CTDA bytes");
            };
            assert_eq!(hex::encode_upper(bytes), expected);
        }
    }

    #[test]
    fn post_translate_does_not_retarget_unrelated_get_is_sex_target_condition() {
        let interner = StringInterner::new();
        let source =
            "000000000000803F4600000000000000000000000100000000000000FFFFFFFF";
        let mut record = make_record("INFO", &interner);
        record.form_key.local = 0x56_A168;
        push_field(&mut record, "CTDA", raw_ctda_from_hex(source));

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(hex::encode_upper(bytes), source);
    }

    #[test]
    fn post_translate_does_not_retarget_changed_wayward_gender_condition_shape() {
        let interner = StringInterner::new();
        let source =
            "00000000000000004600000000000000000000000100000000000000FFFFFFFF";
        let mut record = make_record("INFO", &interner);
        record.form_key.local = 0x56_A169;
        push_field(&mut record, "CTDA", raw_ctda_from_hex(source));

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(hex::encode_upper(bytes), source);
    }

    #[test]
    fn pre_translate_keeps_lvli_entry_gated_by_get_is_player() {
        let interner = StringInterner::new();
        let mut record = make_record("LVLI", &interner);
        let source = raw_condition_fixture(FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID);
        push_field(&mut record, "LLCT", FieldValue::Uint(1));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x0012_3456));
        push_field(
            &mut record,
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(source.clone())),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(converted_lvlo_ids(&record, &interner), vec![0x0012_3456]);
        assert_eq!(
            record
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"LLCT")
                .map(|field| &field.value),
            Some(&FieldValue::Uint(1)),
        );
        let ctda = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDA")
            .expect("GetIsID-gated LVLI entry remains");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(bytes.as_slice(), remapped_get_is_player_bytes(&source));
        assert_eq!(Fo76Fo4Hook::raw_condition_run_on(bytes), Some(0));

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let FieldValue::Bytes(bytes) = &record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDA")
            .expect("remapped LVLI CTDA survives post-translation")
            .value
        else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(bytes.as_slice(), remapped_get_is_player_bytes(&source));
    }

    #[test]
    fn source_reader_keeps_condition_struct_codecs_as_raw_bytes() {
        let interner = StringInterner::new();
        let source = raw_condition_fixture(FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID);

        let value = crate::source_read::decode_subrecord(
            "ACTI",
            "CTDA",
            "struct:B,B,B,B,I,H,B,B,I,I,I,I,i",
            &source,
            &[],
            "SeventySix.esm",
            None,
            &interner,
        )
        .expect("CTDA struct codec decodes");

        assert_eq!(value, FieldValue::Bytes(SmallVec::from_vec(source)));
    }

    #[test]
    fn drop_incompatible_conditions_reconciles_preexisting_citc_mismatch() {
        let interner = StringInterner::new();
        let mut record = make_record("MUST", &interner);
        push_field(&mut record, "CITC", raw_bytes(&2u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(74));

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        let citc = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CITC")
            .expect("CITC remains");
        assert_eq!(
            citc.value,
            raw_bytes(&1u32.to_le_bytes()),
            "preexisting stale CITC reconciled even when this hook drops nothing"
        );
    }

    /// FO76 `AttackScentAttractorMeat` (33D253), the package that pinned converted
    /// Mole Miners to `PreferredSpeed: Jog`. Function 854 (> FO4's 817 max) gates it
    /// on a scent-attractor hazard.
    const UNTRANSLATABLE_PACKAGE_GATE: &str =
        "000000000000803F56035F435AD03300000000000000000000000000FFFFFFFF";

    fn only_condition_bytes(record: &Record) -> Vec<u8> {
        let rows: Vec<_> = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "CTDA")
            .collect();
        assert_eq!(rows.len(), 1, "expected exactly one surviving CTDA");
        let FieldValue::Bytes(bytes) = &rows[0].value else {
            panic!("expected raw CTDA bytes");
        };
        bytes.to_vec()
    }

    /// `GetIsID(Player) == 1` — false for every actor a package runs on.
    fn assert_only_condition_is_a_closed_gate(record: &Record) {
        let bytes = only_condition_bytes(record);
        assert_eq!(
            Fo76Fo4Hook::raw_condition_function_id(&bytes),
            Some(FO4_GET_IS_ID_CONDITION_FUNCTION_ID)
        );
        assert_eq!(
            Fo76Fo4Hook::raw_condition_parameter_1(&bytes),
            Some(FO4_PLAYER_ACTOR_FORM_ID)
        );
        assert_eq!(
            Fo76Fo4Hook::raw_condition_comparison_value(&bytes),
            Some(1.0)
        );
        assert_eq!(bytes[0], 0, "comparison operator reset to `Equal to`");
        assert_eq!(Fo76Fo4Hook::raw_condition_reference(&bytes), Some(0));
    }

    #[test]
    fn untranslatable_package_gate_is_closed_rather_than_removed() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "CITC", raw_bytes(&1_u32.to_le_bytes()));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_from_hex(UNTRANSLATABLE_PACKAGE_GATE),
        );
        push_field(&mut record, "CIS1", raw_bytes(b"ScentAttractorMeat\0"));

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        assert_only_condition_is_a_closed_gate(&record);
        assert!(
            !record.fields.iter().any(|field| field.sig.as_str() == "CIS1"),
            "the parameter string named the function that was replaced"
        );
    }

    #[test]
    fn closing_a_package_gate_keeps_its_or_group_but_not_its_operator() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        let mut gate = hex::decode(UNTRANSLATABLE_PACKAGE_GATE).expect("valid fixture hex");
        // OR with the next row, compared `Not equal to` (operator 1 in bits 5-7).
        gate[0] = 0x01 | (1 << 5);
        push_field(
            &mut record,
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(gate)),
        );

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        let bytes = only_condition_bytes(&record);
        assert_eq!(
            bytes[0], 0x01,
            "OR membership survives; an inherited `Not equal to` would have inverted \
             the closed gate into an always-true one"
        );
    }

    #[test]
    fn untranslatable_gates_are_still_removed_outside_packages() {
        let interner = StringInterner::new();
        let mut record = make_record("INFO", &interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_from_hex(UNTRANSLATABLE_PACKAGE_GATE),
        );

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        assert!(
            !record.fields.iter().any(|field| field.sig.as_str() == "CTDA"),
            "closing the gate is scoped to PACK, where a lost condition means the \
             package outranks the rest of the stack instead of going quiet"
        );
    }

    #[test]
    fn music_tracks_map_strongest_enemy_keyword_gate_to_combat_target() {
        let interner = StringInterner::new();
        let mut record = make_record("MUST", &interner);
        push_field(&mut record, "CITC", raw_bytes(&2u32.to_le_bytes()));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO76_GET_STRONGEST_ENEMY_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                0x008A_ADC9,
            ),
        );
        push_field(&mut record, "CTDA", raw_ctda(5002));

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        let ctda = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "CTDA")
            .expect("music keyword gate remains");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(
            Fo76Fo4Hook::raw_condition_function_id(bytes),
            Some(FO4_GET_COMBAT_TARGET_HAS_KEYWORD_CONDITION_FUNCTION_ID),
        );
        assert_eq!(
            Fo76Fo4Hook::raw_condition_parameter_1(bytes),
            Some(0x008A_ADC9),
        );
        let citc = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "CITC")
            .expect("condition count remains");
        assert_eq!(citc.value, raw_bytes(&1u32.to_le_bytes()));
    }

    #[test]
    fn non_music_records_still_drop_strongest_enemy_keyword_condition() {
        let interner = StringInterner::new();
        let mut record = make_record("INFO", &interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO76_GET_STRONGEST_ENEMY_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                0x008A_ADC9,
            ),
        );

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        assert!(
            record
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "CTDA")
        );
    }

    #[test]
    fn pre_translate_converts_raw_npc_prkr_to_typed_formkey() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "PRKR",
            raw_bytes(&[0xF5, 0x64, 0x84, 0x00, 0x02]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let prkr = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "PRKR")
            .expect("PRKR remains");
        let FieldValue::Struct(fields) = &prkr.value else {
            panic!("PRKR should be structured");
        };
        let FieldValue::FormKey(perk) =
            named_value(fields, "Perk", &interner).expect("perk reference")
        else {
            panic!("PRKR perk should be a FormKey");
        };
        assert_eq!(perk.local, 0x8464F5);
        assert_eq!(interner.resolve(perk.plugin), Some("SeventySix.esm"));

        let expected_rank = raw_bytes(&[0x02]);
        assert_eq!(
            named_value(fields, "Rank", &interner).expect("perk rank"),
            &expected_rank
        );
    }

    #[test]
    fn post_translate_drops_orphaned_condition_strings_with_dropped_ctda() {
        let mut interner = StringInterner::new();
        let mut record = make_record("TERM", &mut interner);
        push_field(&mut record, "BSIZ", raw_bytes(&2_u32.to_le_bytes()));
        // Body-text row 1: FO76-only function → CTDA and its CIS2 must both drop.
        push_field(&mut record, "BTXT", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(10017));
        push_field(&mut record, "CIS2", raw_bytes(b"Fo76Only\0"));
        // Body-text row 2: FO4-compatible function → CTDA and its CIS2 survive.
        push_field(&mut record, "BTXT", raw_bytes(&2_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(560));
        push_field(&mut record, "CIS2", raw_bytes(b"Keep\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(
            sigs,
            vec!["BSIZ", "BTXT", "BTXT", "CTDA", "CIS2"],
            "a dropped CTDA must take its trailing CIS2 with it; a kept CTDA keeps its CIS2",
        );
        let cis2 = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CIS2")
            .expect("kept CIS2 survives");
        let FieldValue::Bytes(bytes) = &cis2.value else {
            panic!("expected raw CIS2 bytes");
        };
        assert_eq!(
            bytes.as_slice(),
            b"Keep\0",
            "surviving CIS2 must be the one paired with the kept CTDA",
        );
    }

    #[test]
    fn post_translate_keeps_raw_ctda_with_fo4_function_id() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MGEF", &mut interner);
        push_field(&mut record, "CTDA", raw_ctda(560));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_remaps_fo76_is_quest_active_to_get_quest_running() {
        // FO76 IsQuestActive (876) has no FO4 equivalent id (> 817) and would be
        // dropped; instead it is remapped to FO4 GetQuestRunning (56), which is
        // value-identical (`== 1`) and takes the same QUST in Parameter #1.
        let mut interner = StringInterner::new();
        let mut record = make_record("LSCR", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(876, 0x0000_FFED),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CTDA")
            .expect("remapped CTDA must survive the incompatibility drop");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        let function_id = u16::from_le_bytes([bytes[8], bytes[9]]);
        assert_eq!(
            function_id, 56,
            "876 IsQuestActive should remap to 56 GetQuestRunning"
        );
        let parameter_1 = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        assert_eq!(
            parameter_1, 0x0000_FFED,
            "quest Parameter #1 must be preserved"
        );
    }

    #[test]
    fn post_translate_remaps_fo76_get_quest_running_unique_on_dialogue_info() {
        let mut interner = StringInterner::new();
        let mut record = make_record("INFO", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_from_hex(
                "00000000000000005803E944558A5400000000000000000000000000FFFFFFFF",
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CTDA")
            .expect("remapped dialogue condition must survive");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(
            u16::from_le_bytes([bytes[8], bytes[9]]),
            FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID
        );
        assert_eq!(
            u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            0x0054_8A55,
            "W05_Community_BB_Quest must remain Parameter #1"
        );
        assert!(
            FO76_REMAPPED_CONDITION_FUNCTION_IDS
                .contains(&FO76_GET_QUEST_RUNNING_UNIQUE_CONDITION_FUNCTION_ID)
        );
    }

    #[test]
    fn post_translate_remaps_fo76_current_location_exact_to_get_in_current_location() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LSCR", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID,
                0x007A_8A73,
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CTDA")
            .expect("remapped CTDA must survive the incompatibility drop");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        let function_id = u16::from_le_bytes([bytes[8], bytes[9]]);
        assert_eq!(
            function_id, FO4_GET_IN_CURRENT_LOCATION_CONDITION_FUNCTION_ID,
            "844 GetIsCurrentLocationExact should remap to 359 GetInCurrentLocation"
        );
        let parameter_1 = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        assert_eq!(
            parameter_1, 0x007A_8A73,
            "location Parameter #1 must be preserved"
        );
    }

    #[test]
    fn post_translate_remaps_editor_location_has_keyword() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        let location_theme_keyword = 0x004E_8561;
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                location_theme_keyword,
            ),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CTDA")
            .expect("remapped CTDA survives");
        assert_eq!(
            Fo76Fo4Hook::condition_function_id(&interner, &ctda.value),
            Some(FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID)
        );
        assert_eq!(
            Fo76Fo4Hook::condition_parameter_1(&interner, &ctda.value),
            Some(location_theme_keyword)
        );
        assert!(
            FO76_REMAPPED_CONDITION_FUNCTION_IDS
                .contains(&FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID)
        );
    }

    #[test]
    fn post_translate_drops_raw_ctda_with_fo76_function_info_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MUST", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID, 0x0063_78CE),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_keeps_raw_ctda_with_fo76_function_info_without_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MUST", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda(FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    fn source_context_get_stage_condition(stage: u16, operator: u8) -> FieldValue {
        let mut bytes = vec![0u8; 32];
        bytes[0] = operator << 5;
        bytes[4..8].copy_from_slice(&(stage as f32).to_le_bytes());
        bytes[8..10].copy_from_slice(&FO4_GET_STAGE_CONDITION_FUNCTION_ID.to_le_bytes());
        bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    #[test]
    fn pair_hook_defers_source_shaped_get_stage_on_quest_context_records() {
        let interner = StringInterner::new();
        for sig in ["QUST", "SCEN", "PACK", "INFO", "DIAL"] {
            let mut record = make_record(sig, &interner);
            push_field(&mut record, "CITC", raw_bytes(&1u32.to_le_bytes()));
            let source = source_context_get_stage_condition(410, 3);
            push_field(&mut record, "CTDA", source.clone());
            push_field(&mut record, "CIS1", raw_bytes(b"stage\0"));

            Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

            assert_eq!(
                record
                    .fields
                    .iter()
                    .map(|field| field.sig.as_str())
                    .collect::<Vec<_>>(),
                vec!["CITC", "CTDA", "CIS1"],
                "source-shaped GetStage should reach the session fixup on {sig}",
            );
            assert_eq!(record.fields[1].value, source);
            assert_eq!(record.fields[0].value, raw_bytes(&1u32.to_le_bytes()));
        }
    }

    #[test]
    fn pair_hook_preserves_mqa206_pack_stage_gates_for_session_lowering() {
        let interner = StringInterner::new();
        let fixtures = [
            (
                "558978 Gail entrance",
                vec![
                    "000000000000803F3B009443C8000000000000000000000000000000FFFFFFFF",
                    "00000000000000003B0094430F270000000000000000000000000000FFFFFFFF",
                ],
            ),
            (
                "558977 RaRa entrance",
                vec![
                    "000000000000803F3B009443C8000000000000000000000000000000FFFFFFFF",
                    "00000000000000003B0094430F270000000000000000000000000000FFFFFFFF",
                ],
            ),
            (
                "5674A3 LeaveVault",
                vec![
                    "000000000000803F3B00944358020000000000000000000000000000FFFFFFFF",
                ],
            ),
        ];

        for (label, conditions) in fixtures {
            let mut record = make_record("PACK", &interner);
            push_field(
                &mut record,
                "QNAM",
                form_key_value(&interner, 0x0054_EDB9),
            );
            let source_conditions: Vec<_> = conditions
                .iter()
                .map(|raw| FieldValue::Bytes(SmallVec::from_vec(hex::decode(raw).unwrap())))
                .collect();
            for condition in &source_conditions {
                push_field(&mut record, "CTDA", condition.clone());
            }

            Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

            assert_eq!(
                record
                    .fields
                    .iter()
                    .map(|field| field.sig.as_str())
                    .collect::<Vec<_>>(),
                std::iter::once("QNAM")
                    .chain(std::iter::repeat_n("CTDA", source_conditions.len()))
                    .collect::<Vec<_>>(),
                "{label}",
            );
            assert_eq!(
                record
                    .fields
                    .iter()
                    .skip(1)
                    .map(|field| &field.value)
                    .collect::<Vec<_>>(),
                source_conditions.iter().collect::<Vec<_>>(),
                "{label}",
            );
        }
    }

    #[test]
    fn pair_hook_disables_context_get_stage_when_source_guards_or_context_are_missing() {
        let interner = StringInterner::new();

        let mut missing_owner = make_record("PACK", &interner);
        push_field(
            &mut missing_owner,
            "CTDA",
            source_context_get_stage_condition(410, 0),
        );
        push_field(&mut missing_owner, "CIS2", raw_bytes(b"stage\0"));
        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut missing_owner);
        assert_eq!(
            missing_owner
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["CTDA", "CIS2"],
            "the session fixup owns owner-resolution decisions",
        );

        for offset in [20usize, 24, 28] {
            let mut malformed = source_context_get_stage_condition(410, 0);
            let FieldValue::Bytes(bytes) = &mut malformed else {
                unreachable!();
            };
            bytes[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
            let mut record = make_record("PACK", &interner);
            push_field(&mut record, "QNAM", form_key_value(&interner, 0x0040_5E14));
            push_field(&mut record, "CTDA", malformed);
            Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);
            assert_eq!(
                record
                    .fields
                    .iter()
                    .map(|field| field.sig.as_str())
                    .collect::<Vec<_>>(),
                vec!["QNAM", "CTDA"],
            );
            // On a package the row stays and is closed instead: removing it would
            // leave the package ungated rather than inert.
            assert_only_condition_is_a_closed_gate(&record);
        }

        let mut non_context = make_record("TERM", &interner);
        push_field(
            &mut non_context,
            "CTDA",
            source_context_get_stage_condition(410, 0),
        );
        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut non_context);
        assert_eq!(
            non_context
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn post_translate_drops_quest_param_ctda_with_null_parameter_1() {
        let mut interner = StringInterner::new();
        let mut record = make_record("TERM", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        // GetStage (58) with a NULL QUST Parameter #1 → xEdit "Found NULL,
        // expected QUST"; the condition can't be retargeted → drop it.
        push_field(&mut record, "CTDA", raw_ctda(58));
        push_field(&mut record, "CIS1", raw_bytes(b"alias\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"EDID"));
        assert!(!sigs.contains(&"CTDA"), "null-quest CTDA must be dropped");
        assert!(
            !sigs.contains(&"CIS1"),
            "the dropped CTDA's trailing CIS1 must go with it",
        );
    }

    #[test]
    fn post_translate_keeps_quest_param_ctda_with_resolved_parameter_1() {
        let mut interner = StringInterner::new();
        let mut record = make_record("TERM", &mut interner);
        // GetStage (58) with a non-null QUST Parameter #1 → valid, keep.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(58, 0x0001_2345),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_keeps_non_quest_param_ctda_with_null_parameter_1() {
        let mut interner = StringInterner::new();
        let mut record = make_record("TERM", &mut interner);
        // Function 560 does not take a QUST in Parameter #1, so a NULL param is
        // not a quest-target violation → keep.
        push_field(&mut record, "CTDA", raw_ctda(560));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_drops_quest_alias_run_on_ctda_on_non_quest_record() {
        let mut interner = StringInterner::new();
        let mut record = make_record("ACTI", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        // GetStageDone(59) with a bogus non-zero Param1 (500) and RunOn=5
        // "Quest Alias" on an ACTI: no owning quest to resolve the alias against
        // -> xEdit cannot find an alias table. Drop it.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_run_on(59, 500, CTDA_RUN_ON_QUEST_ALIAS),
        );
        push_field(&mut record, "CIS2", raw_bytes(b"alias\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"EDID"));
        assert!(
            !sigs.contains(&"CTDA"),
            "quest-alias RunOn CTDA dropped on ACTI"
        );
        assert!(!sigs.contains(&"CIS2"), "trailing CIS2 dropped with it");
    }

    #[test]
    fn post_translate_keeps_quest_alias_run_on_ctda_on_quest_record() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        // FO4 supports quest aliases. On a QUST-context record, xEdit resolves
        // the alias against the owning quest's alias table.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_run_on(58, 500, CTDA_RUN_ON_QUEST_ALIAS),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(
            sigs.contains(&"CTDA"),
            "quest-context record keeps quest-alias RunOn CTDA"
        );
    }

    #[test]
    fn post_translate_keeps_get_is_alias_ref_ctda_on_quest_context_record() {
        let mut interner = StringInterner::new();
        let mut record = make_record("INFO", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS[0], 3),
        );
        push_field(&mut record, "CIS1", raw_bytes(b"alias\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"), "alias-index CTDA is kept");
        assert!(sigs.contains(&"CIS1"), "trailing CIS1 is kept with it");
    }

    #[test]
    fn post_translate_drops_get_is_alias_ref_ctda_without_quest_context() {
        let mut interner = StringInterner::new();
        let mut record = make_record("ACTI", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS[0], 3),
        );
        push_field(&mut record, "CIS1", raw_bytes(b"alias\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(
            !sigs.contains(&"CTDA"),
            "contextless alias-index CTDA is dropped"
        );
        assert!(!sigs.contains(&"CIS1"), "trailing CIS1 dropped with it");
    }

    #[test]
    fn post_translate_keeps_non_quest_alias_run_on_ctda_on_non_quest_record() {
        let mut interner = StringInterner::new();
        let mut record = make_record("ACTI", &mut interner);
        // RunOn=0 "Subject" (not Quest Alias), non-quest function -> keep.
        push_field(&mut record, "CTDA", raw_ctda_with_run_on(560, 0, 0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_maps_fo76_interior_acoustic_condition_to_fo4_interior() {
        let mut interner = StringInterner::new();
        let mut record = make_record("SNDR", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda(FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CTDA")
            .expect("mapped CTDA remains");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(
            Fo76Fo4Hook::raw_condition_function_id(bytes),
            Some(FO4_IS_IN_INTERIOR_CONDITION_FUNCTION_ID),
        );
    }

    #[test]
    fn post_translate_drops_fo76_only_raw_ctda_below_fo4_max() {
        let mut interner = StringInterner::new();
        let mut record = make_record("SNDR", &mut interner);
        push_field(&mut record, "CTDA", raw_ctda(737));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    /// FO76-only condition function 596 (below FO4's 817 max) on a dialogue INFO,
    /// carrying the `$73808CE` Parameter #1 seen on BS01 Brotherhood topics. The
    /// max-id guard misses it (596 < 817), so it must be caught by the explicit
    /// FO76-only id list and the whole CTDA dropped (xEdit `<Unknown:121112782>`).
    #[test]
    fn post_translate_drops_fo76_only_function_596_ctda() {
        let mut interner = StringInterner::new();
        let mut record = make_record("INFO", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(596, 0x0738_08CE),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"), "func=596 CTDA must be dropped");
    }

    /// Guard: function 699 carries the same `$73808CE` Parameter #1 on OTHER
    /// records but is FO4-VALID (xEdit does not flag it), so it must NOT be
    /// dropped: the drop is keyed on function 596, not the parameter.
    #[test]
    fn post_translate_keeps_fo4_valid_function_699_ctda() {
        let mut interner = StringInterner::new();
        let mut record = make_record("INFO", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(699, 0x0738_08CE),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(
            sigs.contains(&"CTDA"),
            "func=699 CTDA must be kept (FO4-valid)"
        );
    }

    fn workshop_cobj(interner: &StringInterner, eid: &str, bench: u32) -> Record {
        let mut record = make_record("COBJ", interner);
        record.eid = Some(interner.intern(eid));
        push_field(
            &mut record,
            "CNAM",
            FieldValue::FormKey(FormKey {
                local: 0x001000,
                plugin: interner.intern(FO76_MASTER_NAME),
            }),
        );
        push_field(
            &mut record,
            "BNAM",
            FieldValue::FormKey(FormKey {
                local: bench,
                plugin: interner.intern(FO76_MASTER_NAME),
            }),
        );
        record
    }

    #[test]
    fn post_translate_keeps_cobj_raw_ctda_without_cell_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("COBJ", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda(FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_keeps_non_cobj_raw_ctda_with_same_cell_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MGEF", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID,
                0x0000_DC58,
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_drops_structured_ctda_with_fo76_only_function_id() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MGEF", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            FieldValue::Struct(vec![(interner.intern("Function"), FieldValue::Uint(10017))]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_drops_structured_ctda_with_fo76_function_info_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MUST", &mut interner);
        let variant = interner.intern("variant");
        let value = interner.intern("value");
        push_field(
            &mut record,
            "CTDA",
            FieldValue::Struct(vec![
                (
                    interner.intern("Function"),
                    FieldValue::Uint(FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID as u64),
                ),
                (
                    interner.intern("Parameter1"),
                    FieldValue::Struct(vec![
                        (variant, FieldValue::String(interner.intern("base_object"))),
                        (value, FieldValue::Uint(0x0063_78CE)),
                    ]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_filters_pack_conditions_by_fo4_compatibility() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "XNAM", raw_bytes(&[0x0D]));
        push_field(&mut record, "ANAM", raw_bytes(b"Procedure\0"));
        push_field(&mut record, "CITC", raw_bytes(&4_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(562));
        push_field(&mut record, "CTDA", raw_ctda(560));
        push_field(&mut record, "CTDA", raw_ctda(362));
        push_field(&mut record, "CTDA", raw_ctda(596));
        push_field(&mut record, "CIS1", raw_bytes(b"fo76_only\0"));
        push_field(&mut record, "PNAM", raw_bytes(b"Trav"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let condition_ids: Vec<u16> = record
            .fields
            .iter()
            .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
            .filter_map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => Fo76Fo4Hook::raw_condition_function_id(bytes),
                _ => None,
            })
            .collect();
        assert_eq!(
            condition_ids,
            vec![562, 560, 362, FO4_GET_IS_ID_CONDITION_FUNCTION_ID],
            "the FO76-only 596 gate is closed in place, not removed — a package that \
             loses a condition runs unconditionally instead of going quiet",
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "EDID")
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "PNAM")
        );
        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "CIS1")
        );

        let citc = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CITC")
            .expect("CITC remains");
        assert_eq!(citc.value, raw_bytes(&4_u32.to_le_bytes()));
    }

    /// Build an 857 `GetNumTimesCompletedQuest` CTDA in the plain dialogue shape
    /// observed on INFO `006FD075`: Target run-on, literal comparison value,
    /// Parameter #3 = -1.
    fn get_num_times_completed_quest_ctda(type_byte: u8, comparison: f32, quest: u32) -> FieldValue {
        let mut bytes = vec![0u8; 32];
        bytes[0] = type_byte;
        bytes[4..8].copy_from_slice(&comparison.to_le_bytes());
        bytes[8..10].copy_from_slice(
            &FO76_GET_NUM_TIMES_COMPLETED_QUEST_CONDITION_FUNCTION_ID.to_le_bytes(),
        );
        bytes[12..16].copy_from_slice(&quest.to_le_bytes());
        bytes[20..24].copy_from_slice(&1_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn info_with_conditions(interner: &StringInterner, conditions: Vec<FieldValue>) -> Record {
        let mut record = make_record("INFO", interner);
        for condition in conditions {
            push_field(&mut record, "CTDA", condition);
        }
        record
    }

    #[test]
    fn dialogue_get_num_times_completed_quest_lowers_to_get_quest_completed_zero() {
        let interner = StringInterner::new();
        // Verbatim third CTDA of FO76 INFO 006FD075 (greeting for QUST
        // 006FD072 NPE_DQ01_BetterTomorrow): `count < 1` — "not completed".
        let source = "800000000000803F5903000072D06F00000000000100000000000000FFFFFFFF";
        let mut record = info_with_conditions(&interner, vec![raw_ctda_from_hex(source)]);

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        let conditions: Vec<&FieldValue> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"CTDA")
            .map(|entry| &entry.value)
            .collect();
        assert_eq!(conditions.len(), 1, "the ANDed gate must survive");
        let FieldValue::Bytes(bytes) = conditions[0] else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(
            hex::encode_upper(bytes.as_slice()),
            "00000000000000001F02000072D06F00000000000000000000000000FFFFFFFF",
            "GetQuestCompleted(543) == 0 against the same quest"
        );
        assert_eq!(
            Fo76Fo4Hook::condition_function_id(&interner, conditions[0]),
            Some(FO4_GET_QUEST_COMPLETED_CONDITION_FUNCTION_ID)
        );
        assert_eq!(
            Fo76Fo4Hook::condition_parameter_1(&interner, conditions[0]),
            Some(0x006F_D072)
        );
        assert_eq!(
            Fo76Fo4Hook::condition_operator(&interner, conditions[0]),
            Some(0)
        );
        assert_eq!(
            Fo76Fo4Hook::condition_comparison_value(&interner, conditions[0]),
            Some(0.0)
        );
    }

    #[test]
    fn quest_completion_count_predicates_map_to_the_right_boolean() {
        // (operator, comparison) -> (GetQuestCompleted value, exact?)
        let exact_not_completed = [(4_u8, 1.0_f32), (0, 0.0), (5, 0.0)];
        let exact_completed = [(3_u8, 1.0_f32), (2, 0.0), (1, 0.0)];
        // Widened: false at count 0, so it can only ever be as permissive as
        // GetQuestCompleted == 1, never more restrictive than the source.
        let widened_completed = [(0_u8, 1.0_f32), (3, 2.0), (2, 1.0)];
        for (operator, comparison) in exact_not_completed {
            assert_eq!(
                Fo76Fo4Hook::quest_completed_comparison_for_count_predicate(operator, comparison),
                Some((0.0, true)),
                "operator={operator} comparison={comparison}"
            );
        }
        for (operator, comparison) in exact_completed {
            assert_eq!(
                Fo76Fo4Hook::quest_completed_comparison_for_count_predicate(operator, comparison),
                Some((1.0, true)),
                "operator={operator} comparison={comparison}"
            );
        }
        for (operator, comparison) in widened_completed {
            assert_eq!(
                Fo76Fo4Hook::quest_completed_comparison_for_count_predicate(operator, comparison),
                Some((1.0, false)),
                "operator={operator} comparison={comparison}"
            );
        }
    }

    #[test]
    fn count_dependent_quest_completion_predicates_are_refused_not_guessed() {
        // `count < 3` on a repeatable daily, `count <= 1`, `count < 2`: true at
        // 0 AND at some non-zero count, so no boolean is a proper superset.
        // Plus the tautology `count >= 0` and the contradiction `count < 0`.
        for (operator, comparison) in [
            (4_u8, 3.0_f32),
            (5, 1.0),
            (4, 2.0),
            (3, 0.0),
            (4, 0.0),
            (1, 1.0),
        ] {
            assert_eq!(
                Fo76Fo4Hook::quest_completed_comparison_for_count_predicate(operator, comparison),
                None,
                "operator={operator} comparison={comparison} must be refused"
            );
        }
    }

    #[test]
    fn refused_quest_completion_shape_is_dropped_not_silently_mistranslated() {
        let interner = StringInterner::new();
        // `GetNumTimesCompletedQuest(006FD072) < 3` — a repeatable-daily shape.
        let mut record = info_with_conditions(
            &interner,
            vec![get_num_times_completed_quest_ctda(0x80, 3.0, 0x006F_D072)],
        );

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        assert!(
            record.fields.iter().all(|entry| entry.sig.0 != *b"CTDA"),
            "an unexpressible count predicate must be dropped, never approximated"
        );
    }

    #[test]
    fn global_backed_quest_completion_comparison_is_refused() {
        let interner = StringInterner::new();
        // Comparison-value flag 0x04 set: the comparison is a GLOB FormID, so
        // there is no compile-time count to map.
        let mut record = info_with_conditions(
            &interner,
            vec![get_num_times_completed_quest_ctda(0x84, 1.0, 0x006F_D072)],
        );

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        assert!(record.fields.iter().all(|entry| entry.sig.0 != *b"CTDA"));
    }

    #[test]
    fn quest_completion_lowering_preserves_or_flag_and_normalizes_run_on() {
        let interner = StringInterner::new();
        // OR flag (0x01) + `count >= 1`, Quest Alias run-on.
        let mut condition = get_num_times_completed_quest_ctda(0x61, 1.0, 0x0012_3456);
        if let FieldValue::Bytes(bytes) = &mut condition {
            bytes[20..24].copy_from_slice(&CTDA_RUN_ON_QUEST_ALIAS.to_le_bytes());
        }
        let mut record = info_with_conditions(&interner, vec![condition]);

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        let value = &record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CTDA")
            .expect("lowered condition survives")
            .value;
        let FieldValue::Bytes(bytes) = value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(bytes[0], 0x01, "OR flag kept, operator forced to Equal");
        assert_eq!(Fo76Fo4Hook::raw_condition_comparison_value(bytes), Some(1.0));
        assert_eq!(Fo76Fo4Hook::raw_condition_run_on(bytes), Some(0));
        assert_eq!(Fo76Fo4Hook::raw_condition_parameter_3(bytes), Some(u32::MAX));
    }

    #[test]
    fn story_manager_completion_gate_is_left_for_the_story_manager_phase() {
        let story_manager_shape =
            raw_ctda_from_hex("A00000000000000059039443145E400000000000070000000000000052330000");
        let mut value = story_manager_shape.clone();

        assert_eq!(
            Fo76Fo4Hook::lower_quest_completion_count_condition(&mut value),
            None
        );
        assert_eq!(value, story_manager_shape);
        assert!(Fo76Fo4Hook::story_manager_completion_quest(&value).is_some());
    }

    // -----------------------------------------------------------------------
    // FO76 condition forms (CNDF) — function-875 inlining
    // -----------------------------------------------------------------------

    /// The catalog is process-global (the pair hook sees one record at a time),
    /// so the tests that install one must not run concurrently.
    fn condition_form_test_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn install_condition_forms(forms: &[(u32, &[&str])]) {
        let mut catalog = ConditionFormCatalog::new(0);
        for (object_id, rows) in forms {
            catalog.insert(
                *object_id,
                rows.iter()
                    .map(|row| hex::decode(row).expect("valid CTDA fixture hex"))
                    .collect(),
                false,
            );
        }
        install_condition_form_catalog(catalog);
    }

    fn condition_rows_hex(record: &Record) -> Vec<String> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"CTDA")
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => hex::encode_upper(bytes.as_slice()),
                _ => panic!("expected raw CTDA bytes"),
            })
            .collect()
    }

    // Verbatim FO76 SeventySix.esm bytes.
    // CNDF 0055620D COMP_Cond_ActiveQuestType_FetchWeapon — its first row is
    // itself a function-875 dereference of CNDF 00555488.
    const CNDF_ACTIVE_QUEST_TYPE_FETCH_WEAPON: [&str; 2] = [
        "000000000000803F6B0300008854550000000000000000000000000000000000",
        "040000000A6255000E00140080545500000000000000000000000000FFFFFFFF",
    ];
    // CNDF 00555488 COMP_Cond_ActiveQuestType_Fetch — two ANDed conditions.
    const CNDF_ACTIVE_QUEST_TYPE_FETCH: [&str; 2] = [
        "000000000000803F300214001E3B5500000000000000000000000000FFFFFFFF",
        "04000000835455000E0014007F545500000000000000000000000000FFFFFFFF",
    ];
    // CNDF 00572CCB COMP_Cond_RadiantQuest_ReturnItem — an ANDed HasKeyword
    // followed by a one-row OR group.
    const CNDF_RADIANT_QUEST_RETURN_ITEM: [&str; 2] = [
        "000000000000803F300214001E3B5500000000000000000000000000FFFFFFFF",
        "0500000036C856000E001400B02B5600000000000000000000000000FFFFFFFF",
    ];
    // CNDF 0056F71B COMP_Cond_QuestStage_Active — two ANDed GetItemCount rows
    // (`>= global` and `< global`), i.e. a pure conjunction.
    const CNDF_QUEST_STAGE_ACTIVE: [&str; 2] = [
        "640000006B9B56000E001400B02B5600000000000000000000000000FFFFFFFF",
        "8400000036C856000E001400B02B5600000000000000000000000000FFFFFFFF",
    ];

    #[test]
    fn condition_form_inlining_recurses_through_nested_forms() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        install_condition_forms(&[
            (0x0055_620D, &CNDF_ACTIVE_QUEST_TYPE_FETCH_WEAPON),
            (0x0055_5488, &CNDF_ACTIVE_QUEST_TYPE_FETCH),
        ]);
        // Verbatim sole CTDA of FO76 INFO 005561F4
        // (COMP_Astronaut_QuestResponse_BasicInfo_FetchWeapon).
        let mut record = info_with_conditions(
            &interner,
            vec![raw_ctda_from_hex(
                "000000000000803F6B0300000D625500000000000000000000000000FFFFFFFF",
            )],
        );

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        // Fn875(FetchWeapon) == 1  =>  Fn875(Fetch) == 1 AND GetItemCount(555480)
        //                          =>  HasKeyword(553B1E) AND GetItemCount(55547F)
        //                              AND GetItemCount(555480) — every row
        //                              unflagged, so the predicate stays one
        //                              AND chain at both levels.
        assert_eq!(
            condition_rows_hex(&record),
            vec![
                "000000000000803F300214001E3B5500000000000000000000000000FFFFFFFF",
                "04000000835455000E0014007F545500000000000000000000000000FFFFFFFF",
                "040000000A6255000E00140080545500000000000000000000000000FFFFFFFF",
            ]
        );
        clear_condition_form_catalog();
    }

    #[test]
    fn condition_form_inlining_is_idempotent() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        install_condition_forms(&[
            (0x0055_620D, &CNDF_ACTIVE_QUEST_TYPE_FETCH_WEAPON),
            (0x0055_5488, &CNDF_ACTIVE_QUEST_TYPE_FETCH),
        ]);
        let mut record = info_with_conditions(
            &interner,
            vec![raw_ctda_from_hex(
                "000000000000803F6B0300000D625500000000000000000000000000FFFFFFFF",
            )],
        );

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);
        let once = condition_rows_hex(&record);
        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        assert_eq!(condition_rows_hex(&record), once);
        clear_condition_form_catalog();
    }

    #[test]
    fn condition_form_or_group_splices_verbatim_into_an_and_slot() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        install_condition_forms(&[(0x0057_2CCB, &CNDF_RADIANT_QUEST_RETURN_ITEM)]);
        // Verbatim CTDAs of FO76 INFO 0058C912
        // (GREETS__STORY_QuestTurnIn_Confidant---ALLY_Astronaut). The 875 sits
        // in an unflagged (AND) slot between two unflagged rows.
        let mut record = info_with_conditions(
            &interner,
            vec![
                raw_ctda_from_hex(
                    "000000000000803F3602E94401000000000000000000000000000000FFFFFFFF",
                ),
                raw_ctda_from_hex(
                    "000000000000803F6B03E944CB2C5700000000000000000000000000FFFFFFFF",
                ),
                raw_ctda_from_hex(
                    "00000000000000000E00E944F340570000000000050000000000000000000000",
                ),
            ],
        );

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        // The form's own group structure is carried bit-for-bit: the unflagged
        // HasKeyword stays ANDed and the flagged GetItemCount stays its own
        // one-row OR group. Neither boundary row can merge, because both host
        // neighbours are unflagged.
        assert_eq!(
            condition_rows_hex(&record),
            vec![
                "000000000000803F3602E94401000000000000000000000000000000FFFFFFFF",
                "000000000000803F300214001E3B5500000000000000000000000000FFFFFFFF",
                "0500000036C856000E001400B02B5600000000000000000000000000FFFFFFFF",
                "00000000000000000E00E944F340570000000000050000000000000000000000",
            ]
        );
        clear_condition_form_catalog();
    }

    #[test]
    fn negated_all_and_condition_form_becomes_a_de_morgan_or_group() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        install_condition_forms(&[(0x0056_F71B, &CNDF_QUEST_STAGE_ACTIVE)]);
        // `Fn875(COMP_Cond_QuestStage_Active) == 0` in an unflagged slot.
        let mut record = info_with_conditions(
            &interner,
            vec![raw_ctda_from_hex(
                "00000000000000006B03E9441BF75600000000000000000000000000FFFFFFFF",
            )],
        );

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        // NOT (count >= g AND count < g') == (count < g OR count >= g'): both
        // operators complemented (3 -> 4, 4 -> 3) and both rows Or-flagged so
        // they form one isolated OR group.
        assert_eq!(
            condition_rows_hex(&record),
            vec![
                "850000006B9B56000E001400B02B5600000000000000000000000000FFFFFFFF",
                "6500000036C856000E001400B02B5600000000000000000000000000FFFFFFFF",
            ]
        );
        clear_condition_form_catalog();
    }

    #[test]
    fn negating_a_condition_form_that_contains_an_or_group_is_refused() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        // CNDF 0056F71D COMP_Cond_RadiantQuest_Available: HasKeyword AND an
        // Or-flagged row. Negating it needs an OR of ANDs, which the flat model
        // cannot express, so the shape is refused outright.
        install_condition_forms(&[(
            0x0056_F71D,
            &[
                "000000000000803F300214001E3B5500000000000000000000000000FFFFFFFF",
                "850000006B9B56000E001400B02B5600000000000000000000000000FFFFFFFF",
            ],
        )]);
        // Verbatim third CTDA of FO76 INFO 0058949F
        // (GREETS__RADIANT_QuestUnavailable---ALLY_Astronaut).
        let source = "00000000000000006B03E9441DF75600000000000000000000000000FFFFFFFF";
        let mut record = info_with_conditions(&interner, vec![raw_ctda_from_hex(source)]);

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);
        assert_eq!(condition_rows_hex(&record), vec![source.to_uppercase()]);

        // Refused means "left for the existing incompatible-function pass",
        // which drops and traces it exactly as before.
        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);
        assert!(condition_rows_hex(&record).is_empty());
        clear_condition_form_catalog();
    }

    #[test]
    fn positive_multi_condition_form_in_an_or_slot_is_refused() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        install_condition_forms(&[(0x0057_2CCB, &CNDF_RADIANT_QUEST_RETURN_ITEM)]);
        // Same 875 row as the splice test, but Or-flagged: the slot is one
        // alternative of a host OR group and the replacement is a conjunction.
        // `A || (b && c)` has no flat encoding.
        let source = "010000000000803F6B03E944CB2C5700000000000000000000000000FFFFFFFF";
        let mut record = info_with_conditions(&interner, vec![raw_ctda_from_hex(source)]);

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        assert_eq!(condition_rows_hex(&record), vec![source.to_uppercase()]);
        clear_condition_form_catalog();
    }

    #[test]
    fn single_condition_form_in_an_or_slot_keeps_the_host_or_flag() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        install_condition_forms(&[(
            0x0057_2CCC,
            &["000000000000803F300214001E3B5500000000000000000000000000FFFFFFFF"],
        )]);
        let mut record = info_with_conditions(
            &interner,
            vec![raw_ctda_from_hex(
                "010000000000803F6B03E944CC2C5700000000000000000000000000FFFFFFFF",
            )],
        );

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        // A 1:1 substitution never changes grouping, so it is safe inside an OR
        // run — the host's Or flag rides onto the substituted row.
        assert_eq!(
            condition_rows_hex(&record),
            vec!["010000000000803F300214001E3B5500000000000000000000000000FFFFFFFF"]
        );
        clear_condition_form_catalog();
    }

    #[test]
    fn condition_form_cycle_is_refused_instead_of_recursing_forever() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        install_condition_forms(&[
            (
                0x0000_0100,
                &["000000000000803F6B03000000020000000000000000000000000000FFFFFFFF"],
            ),
            (
                0x0000_0200,
                &["000000000000803F6B03000000010000000000000000000000000000FFFFFFFF"],
            ),
        ]);
        let source = "000000000000803F6B03000000010000000000000000000000000000FFFFFFFF";
        let mut record = info_with_conditions(&interner, vec![raw_ctda_from_hex(source)]);

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        assert_eq!(condition_rows_hex(&record), vec![source.to_uppercase()]);
        clear_condition_form_catalog();
    }

    #[test]
    fn condition_form_with_an_fo4_incompatible_inner_function_is_refused() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        // Inner function 849 (FO76 nuke-zone check) has no FO4 slot. Inlining
        // would only move the drop one level down.
        install_condition_forms(&[(
            0x0057_2CCD,
            &["000000000000803F510314001E3B5500000000000000000000000000FFFFFFFF"],
        )]);
        let source = "000000000000803F6B03E944CD2C5700000000000000000000000000FFFFFFFF";
        let mut record = info_with_conditions(&interner, vec![raw_ctda_from_hex(source)]);

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        assert_eq!(condition_rows_hex(&record), vec![source.to_uppercase()]);
        clear_condition_form_catalog();
    }

    #[test]
    fn non_subject_run_on_condition_form_is_refused() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        install_condition_forms(&[(0x0056_F71B, &CNDF_QUEST_STAGE_ACTIVE)]);
        // Run On = 14 (an FO76-only scope). The form's own rows carry their own
        // Run On values, so re-scoping them is not expressible.
        let source = "000000000000803F6B0300001BF75600000000000E00000000000000FFFFFFFF";
        let mut record = info_with_conditions(&interner, vec![raw_ctda_from_hex(source)]);

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);

        assert_eq!(condition_rows_hex(&record), vec![source.to_uppercase()]);
        clear_condition_form_catalog();
    }

    #[test]
    fn inlined_conditions_go_through_the_normal_fo76_function_remap() {
        let _guard = condition_form_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interner = StringInterner::new();
        // Inner function 876 (FO76 IsQuestActive) is remapped to FO4 56
        // (GetQuestRunning) by the shared normalization pass, so the inlined row
        // must survive the incompatible-function drop.
        install_condition_forms(&[(
            0x0057_2CCE,
            &["000000000000803F6C0300003412000000000000000000000000000000000000"],
        )]);
        let mut record = info_with_conditions(
            &interner,
            vec![raw_ctda_from_hex(
                "000000000000803F6B03E944CE2C5700000000000000000000000000FFFFFFFF",
            )],
        );

        Fo76Fo4Hook::inline_fo76_condition_forms(&mut record);
        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        assert_eq!(condition_rows_hex(&record).len(), 1, "inlined row survives");
        let FieldValue::Bytes(bytes) = &record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CTDA")
            .unwrap()
            .value
        else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(
            Fo76Fo4Hook::raw_condition_function_id(bytes),
            Some(FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID)
        );
        clear_condition_form_catalog();
    }

    /// Exact source bytes of `BS01_MQ01_Trust_QuestNode` (`5C977E`) — the node
    /// that gated Steel Dawn's second quest behind `GetLevel >= 20` on FO76
    /// event-data member `0x3352`, which FO4 cannot resolve.
    #[test]
    fn story_manager_player_event_data_rows_re_run_on_subject() {
        let interner = StringInterner::new();
        let mut record = make_record("SMQN", &interner);
        // GetLevel >= 20, Run On = Event Data, member 0x3352.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_from_hex(
                "600000000000A041500000000000000000000000070000000000000052330000",
            ),
        );
        // GetEventData keyword match — already Run On = Subject, must not move.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_from_hex(
                "000000000000803F4002000000004B3169715C000000000000000000FFFFFFFF",
            ),
        );

        Fo76Fo4Hook::normalize_fo76_story_manager_event_data_run_on(&mut record);

        let rows = record
            .fields
            .iter()
            .filter_map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            Fo76Fo4Hook::raw_condition_run_on(rows[0]),
            Some(0),
            "the FO76 player event member has to become Run On = Subject"
        );
        assert_eq!(
            Fo76Fo4Hook::raw_condition_parameter_3(rows[0]),
            Some(u32::MAX)
        );
        assert_eq!(
            Fo76Fo4Hook::raw_condition_function_id(rows[0]),
            Some(80),
            "GetLevel keeps its function and comparison; only the Run On moves"
        );
        assert_eq!(&rows[0][0..8], &[0x60, 0, 0, 0, 0x00, 0x00, 0xA0, 0x41]);
        assert_eq!(
            Fo76Fo4Hook::raw_condition_run_on(rows[1]),
            Some(0),
            "an already-Subject row is untouched"
        );
    }

    #[test]
    fn increase_level_get_event_data_rows_lower_to_get_level() {
        let interner = StringInterner::new();
        for source in [
            "000B00000000A0414002944302005631000000000000000000000000FFFFFFFF",
            "000A00000000C84240025D4302005631000000000000000000000000FFFFFFFF",
        ] {
            let mut expected = hex::decode(source).expect("valid source CTDA");
            expected[8..10].copy_from_slice(&GET_LEVEL_CONDITION_FUNCTION_ID.to_le_bytes());
            expected[10..12].fill(0);
            expected[12..16].fill(0);
            let mut record = make_record("SMQN", &interner);
            push_field(
                &mut record,
                "PNAM",
                form_key_value(&interner, FO4_INCREASE_LEVEL_STORY_MANAGER_ROOT),
            );
            push_field(&mut record, "CTDA", raw_ctda_from_hex(source));

            Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

            let FieldValue::Bytes(bytes) = &record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"CTDA")
                .expect("level gate survives")
                .value
            else {
                panic!("raw CTDA bytes");
            };
            assert_eq!(
                Fo76Fo4Hook::raw_condition_function_id(bytes),
                Some(GET_LEVEL_CONDITION_FUNCTION_ID)
            );
            assert_eq!(Fo76Fo4Hook::raw_condition_parameter_1(bytes), Some(0));
            assert_eq!(Fo76Fo4Hook::raw_condition_parameter_2(bytes), Some(0));
            assert_eq!(Fo76Fo4Hook::raw_condition_run_on(bytes), Some(0));
            assert_eq!(Fo76Fo4Hook::raw_condition_reference(bytes), Some(0));
            assert_eq!(
                Fo76Fo4Hook::raw_condition_parameter_3(bytes),
                Some(u32::MAX)
            );
            assert_eq!(bytes.as_slice(), expected.as_slice());
        }
    }

    #[test]
    fn get_event_data_level_selector_requires_the_increase_level_root() {
        let interner = StringInterner::new();
        let mut record = make_record("SMQN", &interner);
        push_field(&mut record, "PNAM", form_key_value(&interner, 0x0002_9152));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_from_hex("000B00000000A0414002944302005631000000000000000000000000FFFFFFFF"),
        );

        Fo76Fo4Hook::normalize_fo76_story_manager_level_event_data(&mut record);

        let FieldValue::Bytes(bytes) = &record.fields[1].value else {
            panic!("raw CTDA bytes");
        };
        assert_eq!(
            Fo76Fo4Hook::raw_condition_function_id(bytes),
            Some(FO76_GET_EVENT_DATA_CONDITION_FUNCTION_ID)
        );
    }

    #[test]
    fn get_event_data_level_selector_rejects_non_exact_shapes() {
        let interner = StringInterner::new();
        let source =
            hex::decode("000B00000000A0414002944302005631000000000000000000000000FFFFFFFF")
                .expect("valid source CTDA");
        let mut variants = Vec::new();
        let mut extended = source.clone();
        extended.push(0);
        variants.push(extended);
        for (offset, replacement) in [
            (12, 0x03),
            (16, 0x01),
            (20, 0x01),
            (24, 0x01),
            (28, 0x00),
        ] {
            let mut variant = source.clone();
            variant[offset] = replacement;
            variants.push(variant);
        }

        for variant in variants {
            let mut record = make_record("SMQN", &interner);
            push_field(
                &mut record,
                "PNAM",
                form_key_value(&interner, FO4_INCREASE_LEVEL_STORY_MANAGER_ROOT),
            );
            push_field(
                &mut record,
                "CTDA",
                FieldValue::Bytes(SmallVec::from_vec(variant.clone())),
            );

            Fo76Fo4Hook::normalize_fo76_story_manager_level_event_data(&mut record);

            let FieldValue::Bytes(bytes) = &record.fields[1].value else {
                panic!("raw CTDA bytes");
            };
            assert_eq!(bytes.as_slice(), variant.as_slice());
        }

        let mut duplicate_parent = make_record("SMQN", &interner);
        for _ in 0..2 {
            push_field(
                &mut duplicate_parent,
                "PNAM",
                form_key_value(&interner, FO4_INCREASE_LEVEL_STORY_MANAGER_ROOT),
            );
        }
        push_field(
            &mut duplicate_parent,
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(source.clone())),
        );
        Fo76Fo4Hook::normalize_fo76_story_manager_level_event_data(&mut duplicate_parent);
        let FieldValue::Bytes(bytes) = &duplicate_parent.fields[2].value else {
            panic!("raw CTDA bytes");
        };
        assert_eq!(bytes.as_slice(), source.as_slice());
    }

    #[test]
    fn story_manager_event_data_rows_fo4_can_resolve_are_left_alone() {
        let interner = StringInterner::new();
        let mut record = make_record("SMQN", &interner);
        // Run On = Event Data, member 0x3152 — a member FO4 itself uses.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_from_hex(
                "600000000000A041500000000000000000000000070000000000000052310000",
            ),
        );

        Fo76Fo4Hook::normalize_fo76_story_manager_event_data_run_on(&mut record);

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("bytes");
        };
        assert_eq!(Fo76Fo4Hook::raw_condition_run_on(bytes), Some(7));
        assert_eq!(
            Fo76Fo4Hook::raw_condition_parameter_3(bytes),
            Some(0x0000_3152)
        );
    }

    #[test]
    fn non_story_manager_records_keep_their_event_data_run_on() {
        let interner = StringInterner::new();
        let mut record = make_record("INFO", &interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_from_hex(
                "600000000000A041500000000000000000000000070000000000000052330000",
            ),
        );

        Fo76Fo4Hook::normalize_fo76_story_manager_event_data_run_on(&mut record);

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("bytes");
        };
        assert_eq!(Fo76Fo4Hook::raw_condition_run_on(bytes), Some(7));
    }
