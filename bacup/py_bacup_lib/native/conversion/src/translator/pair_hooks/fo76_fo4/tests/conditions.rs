

    fn raw_condition_fixture(function_id: u16) -> Vec<u8> {
        let mut bytes = vec![
            0xA4, 0x11, 0x22, 0x33, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x55, 0x66, 0xEF,
            0xBE, 0xAD, 0xDE, 0x78, 0x56, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x0D, 0xF0,
            0xAD, 0x0B, 0xCA, 0xFE, 0xBA, 0xBE,
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
    /// dropped — proving the fix targets 596 specifically, not the parameter.
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
        assert_eq!(condition_ids, vec![562, 560, 362]);
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
        assert_eq!(citc.value, raw_bytes(&3_u32.to_le_bytes()));
    }
