

    /// CTDA with explicit operator (high 3 bits of the type byte) + comparison value.
    fn raw_ctda_full(function_id: u16, operator: u8, comparison_value: f32) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[0] = operator << 5;
        bytes[4..8].copy_from_slice(&comparison_value.to_le_bytes());
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn raw_ctda_with_comparison_global(
        function_id: u16,
        operator: u8,
        comparison_global: u32,
    ) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[0] = (operator << 5) | CTDA_COMPARISON_GLOBAL_FLAG;
        bytes[4..8].copy_from_slice(&comparison_global.to_le_bytes());
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn converted_lvlo_ids(record: &Record, interner: &StringInterner) -> Vec<u32> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"LVLO")
            .filter_map(|entry| {
                source_lvlo_reference(&entry.value, record.form_key.plugin, interner)
                    .map(|form_key| form_key.local)
            })
            .collect()
    }

    #[test]
    fn condition_gates_dropped_world_state_classifies_nuke_and_event_globals() {
        // Nuke-zone check (func 849): dropped regardless of operator/value.
        assert!(Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID, 0, 0.0,)
        ));
        // GetGlobalValue == 1 (event ON): dropped.
        assert!(Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 1.0,)
        ));
        // GetGlobalValue != 0 (event ON): dropped.
        assert!(Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 1, 0.0,)
        ));
        // GetGlobalValue >= 1 (event ON): dropped.
        assert!(Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 3, 1.0,)
        ));
        // GetGlobalValue == 0 (event OFF / FO4 default): kept.
        assert!(!Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 0.0,)
        ));
        // Unrelated condition function: kept.
        assert!(!Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(56, 0, 1.0)
        ));
    }

    #[test]
    fn pre_translate_selects_novice_and_unlocked_safe_branches() {
        let interner = StringInterner::new();
        let mut record = make_record("LVLI", &interner);
        push_field(
            &mut record,
            "LVLF",
            FieldValue::Bytes(SmallVec::from_slice(&[LEVELED_LIST_USE_ALL_FLAG])),
        );
        push_field(&mut record, "LLCT", FieldValue::Uint(6));
        for (item, lock_global) in [
            (0x100001, FO76_LOCK_LEVEL_MASTER_GLOBAL),
            (0x100002, FO76_LOCK_LEVEL_EXPERT_GLOBAL),
            (0x100003, FO76_LOCK_LEVEL_ADVANCED_GLOBAL),
            (0x100004, FO76_LOCK_LEVEL_NOVICE_GLOBAL),
        ] {
            push_field(&mut record, "LVLO", FieldValue::Uint(item));
            push_field(
                &mut record,
                "CTDA",
                raw_ctda_with_comparison_global(
                    GET_LOCK_LEVEL_CONDITION_FUNCTION_ID,
                    0,
                    lock_global,
                ),
            );
            push_field(&mut record, "LVIV", FieldValue::Float(1.0));
            push_field(&mut record, "LVLV", FieldValue::Float(1.0));
        }
        push_field(&mut record, "LVLO", FieldValue::Uint(0x100005));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_full(GET_LOCKED_CONDITION_FUNCTION_ID, 0, 1.0),
        );
        push_field(&mut record, "LVIV", FieldValue::Float(1.0));
        push_field(&mut record, "LVLV", FieldValue::Float(1.0));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x100006));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_full(GET_LOCKED_CONDITION_FUNCTION_ID, 0, 0.0),
        );
        push_field(&mut record, "LVIV", FieldValue::Float(1.0));
        push_field(&mut record, "LVLV", FieldValue::Float(1.0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            converted_lvlo_ids(&record, &interner),
            vec![0x100004, 0x100006]
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LLCT")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(2))
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LVLF")
                .map(|entry| &entry.value),
            Some(&FieldValue::Bytes(SmallVec::from_slice(&[
                LEVELED_LIST_USE_ALL_FLAG
            ])))
        );

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLI is supported by FO4 schema")
                }
            };
        assert_eq!(converted_lvlo_ids(&record, &interner).len(), 2);
        assert!(
            record
                .fields
                .iter()
                .all(|entry| !matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
        );
    }

    #[test]
    fn pre_translate_drops_unknown_bonus_when_unconditional_rows_survive() {
        let interner = StringInterner::new();
        let mut record = make_record("LVLI", &interner);
        push_field(
            &mut record,
            "LVLF",
            FieldValue::Bytes(SmallVec::from_slice(&[LEVELED_LIST_USE_ALL_FLAG])),
        );
        push_field(&mut record, "LLCT", FieldValue::Uint(3));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x200001));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x200002));
        push_field(&mut record, "COED", raw_bytes(&[0; 20]));
        push_field(&mut record, "CTDA", raw_ctda_full(300, 0, 1.0));
        push_field(&mut record, "LVIV", FieldValue::Float(1.0));
        push_field(&mut record, "LVLV", FieldValue::Float(1.0));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x200003));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            converted_lvlo_ids(&record, &interner),
            vec![0x200001, 0x200003]
        );
        assert!(record.fields.iter().all(|entry| entry.sig.0 != *b"COED"));
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LVLF")
                .map(|entry| &entry.value),
            Some(&FieldValue::Bytes(SmallVec::from_slice(&[
                LEVELED_LIST_USE_ALL_FLAG
            ])))
        );
    }

    #[test]
    fn pre_translate_preserves_location_theme_entry_and_fallback() {
        let interner = StringInterner::new();
        let mut record = make_record("LVLI", &interner);
        let location_theme_keyword = 0x004E_8561;
        push_field(&mut record, "LLCT", FieldValue::Uint(2));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x4EA90B));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                location_theme_keyword,
            ),
        );
        push_field(&mut record, "LVLO", FieldValue::Uint(0x4EA905));

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            converted_lvlo_ids(&record, &interner),
            vec![0x4EA90B, 0x4EA905]
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LLCT")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(2))
        );
        let ctda = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CTDA")
            .expect("location-theme condition survives");
        assert_eq!(
            Fo76Fo4Hook::condition_function_id(&interner, &ctda.value),
            Some(FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID)
        );
        assert_eq!(
            Fo76Fo4Hook::condition_parameter_1(&interner, &ctda.value),
            Some(location_theme_keyword)
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        assert!(record.fields.iter().any(|entry| {
            entry.sig.0 == *b"CTDA"
                && Fo76Fo4Hook::condition_function_id(&interner, &entry.value)
                    == Some(FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID)
        }));
    }

    #[test]
    fn pre_translate_uses_single_unknown_fallback_and_clears_use_all() {
        let interner = StringInterner::new();
        let mut record = make_record("LVLI", &interner);
        push_field(
            &mut record,
            "LVLF",
            FieldValue::Bytes(SmallVec::from_slice(&[LEVELED_LIST_USE_ALL_FLAG])),
        );
        push_field(&mut record, "LLCT", FieldValue::Uint(3));
        for (item, level) in [(0x300001, 10.0), (0x300002, 1.0), (0x300003, 5.0)] {
            push_field(&mut record, "LVLO", FieldValue::Uint(item));
            push_field(&mut record, "CTDA", raw_ctda_full(300, 0, 1.0));
            push_field(&mut record, "LVIV", FieldValue::Float(1.0));
            push_field(&mut record, "LVLV", FieldValue::Float(level));
        }

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(converted_lvlo_ids(&record, &interner), vec![0x300002]);
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LVLF")
                .map(|entry| &entry.value),
            Some(&FieldValue::Bytes(SmallVec::from_slice(&[0])))
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LLCT")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(1))
        );
    }

    #[test]
    fn pre_translate_drops_all_perk_gated_rows_without_fallback() {
        let interner = StringInterner::new();
        let mut record = make_record("LVLI", &interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(2));
        for item in [0x400001, 0x400002] {
            push_field(&mut record, "LVLO", FieldValue::Uint(item));
            push_field(
                &mut record,
                "CTDA",
                raw_ctda_full(HAS_PERK_CONDITION_FUNCTION_ID, 0, 1.0),
            );
        }

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(converted_lvlo_ids(&record, &interner).is_empty());
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LLCT")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(0))
        );
    }

    #[test]
    fn pre_translate_preserves_overseer_cache_quest_items_for_untranslatable_conditions() {
        let interner = StringInterner::new();
        let mut record = make_record("LVLI", &interner);
        push_field(
            &mut record,
            "LVLF",
            FieldValue::Bytes(SmallVec::from_slice(&[LEVELED_LIST_USE_ALL_FLAG])),
        );
        push_field(&mut record, "LLCT", FieldValue::Uint(4));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x3D7F44));
        push_field(&mut record, "CTDA", raw_ctda_full(857, 0, 0.0));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x3D4725));
        push_field(&mut record, "CTDA", raw_ctda_full(857, 2, 0.0));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x564078));
        push_field(&mut record, "CTDA", raw_ctda_full(853, 0, 0.0));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x1389EC));
        push_field(&mut record, "CTDA", raw_ctda_full(857, 3, 1.0));
        push_field(&mut record, "CTDA", raw_ctda_full(47, 0, 0.0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            converted_lvlo_ids(&record, &interner),
            vec![0x3D7F44, 0x3D4725, 0x564078, 0x1389EC]
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LLCT")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(4))
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"LVLF")
                .map(|entry| &entry.value),
            Some(&FieldValue::Bytes(SmallVec::from_slice(&[
                LEVELED_LIST_USE_ALL_FLAG
            ])))
        );

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLI is supported by FO4 schema")
                }
            };
        assert_eq!(
            converted_lvlo_ids(&record, &interner),
            vec![0x3D7F44, 0x3D4725, 0x564078, 0x1389EC]
        );
        assert!(
            record
                .fields
                .iter()
                .all(|entry| !matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
        );
    }

    #[test]
    fn pre_translate_selects_event_global_off_branch() {
        let interner = StringInterner::new();
        let mut record = make_record("LVLI", &interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(2));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x500001));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 1.0),
        );
        push_field(&mut record, "LVLO", FieldValue::Uint(0x500002));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 0.0),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(converted_lvlo_ids(&record, &interner), vec![0x500002]);
    }

    #[test]
    fn pre_translate_drops_nuke_and_event_gated_leveled_entries() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(3));
        // Nuke-zone-gated entry (radiation suit) -> dropped.
        push_field(&mut record, "LVLO", FieldValue::Uint(0x58AFD6));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_full(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID, 0, 1.0),
        );
        // Festive entry gated on GetGlobalValue(event) == 1.0 -> dropped.
        push_field(&mut record, "LVLO", FieldValue::Uint(0x5A0019));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 1.0),
        );
        // Normal ungated entry -> kept.
        push_field(&mut record, "LVLO", FieldValue::Uint(0x58AFD5));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let lvlo: Vec<_> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "LVLO")
            .collect();
        assert_eq!(
            lvlo.len(),
            1,
            "nuke + event-gated entries dropped, normal kept"
        );
        let count = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LLCT")
            .map(|entry| &entry.value);
        assert_eq!(count, Some(&FieldValue::Uint(1)));
    }

    #[test]
    fn pre_translate_converts_fo76_lvln_reference_to_fo4_npc_lvlo() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLN", &mut interner);
        let variant_sym = interner.intern("variant");
        let value_sym = interner.intern("value");
        let reference_variant = interner.intern("reference");
        push_field(&mut record, "LLCT", FieldValue::Uint(1));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Struct(vec![
                (variant_sym, FieldValue::String(reference_variant)),
                (value_sym, FieldValue::Uint(0x868BB8)),
            ]),
        );
        push_field(&mut record, "LVIV", FieldValue::Float(1.0));
        push_field(&mut record, "LVLV", FieldValue::Float(1.0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLN is supported by FO4 schema")
                }
            };
        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let encoded =
            crate::target_write::encode_field_pub(lvlo_entry, schema.record_def("LVLN"), &interner)
                .expect("converted LVLO encodes");
        assert_eq!(encoded, vec![1, 0, 0, 0, 0xB8, 0x8B, 0x86, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn pre_translate_converts_raw_fo76_lvlo_bytes_to_source_plugin_formkey() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        let mut raw_lvlo = vec![1, 0, 0, 0];
        raw_lvlo.extend_from_slice(&0x0083_9C65_u32.to_le_bytes());
        raw_lvlo.extend_from_slice(&[1, 0, 0, 0]);
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(SmallVec::from_vec(raw_lvlo)),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let FieldValue::Struct(fields) = &lvlo_entry.value else {
            panic!("LVLO should be converted into an FO4 struct");
        };
        let item = named_value(fields, "item", &interner).expect("item reference");
        let FieldValue::FormKey(fk) = item else {
            panic!("LVLO item should be a typed FormKey, got {item:?}");
        };
        assert_eq!(fk.local, 0x0083_9C65);
        assert_eq!(interner.resolve(fk.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn pre_translate_converts_four_byte_fo76_lvlo_reference_without_using_it_as_level() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(SmallVec::from_vec(0x0083_9C65_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "LVIV", FieldValue::Float(7.0));
        push_field(&mut record, "LVLV", FieldValue::Float(2.0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLI is supported by FO4 schema")
                }
            };
        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let encoded =
            crate::target_write::encode_field_pub(lvlo_entry, schema.record_def("LVLI"), &interner)
                .expect("converted LVLO encodes");
        assert_eq!(encoded, vec![2, 0, 0, 0, 0x65, 0x9C, 0x83, 0, 7, 0, 0, 0]);
    }

    #[test]
    fn pre_translate_remaps_raw_fo76_caps_lvlo_to_fo4_caps_misc() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        let raw_lvlo = vec![1, 0, 0, 0, 0x0F, 0, 0, 0, 100, 0, 0, 0];
        push_field(&mut record, "LLCT", FieldValue::Uint(1));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(SmallVec::from_vec(raw_lvlo)),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let FieldValue::Struct(fields) = &lvlo_entry.value else {
            panic!("LVLO should be converted into an FO4 struct");
        };
        let item = named_value(fields, "item", &interner).expect("item reference");
        let FieldValue::FormKey(fk) = item else {
            panic!("LVLO item should be a typed FormKey, got {item:?}");
        };
        assert_eq!(fk.local, 0x00000F);
        assert_eq!(interner.resolve(fk.plugin), Some("Fallout4.esm"));

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLI is supported by FO4 schema")
                }
            };
        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let encoded =
            crate::target_write::encode_field_pub(lvlo_entry, schema.record_def("LVLI"), &interner)
                .expect("converted LVLO encodes");
        assert_eq!(encoded, vec![1, 0, 0, 0, 0x0F, 0, 0, 0, 100, 0, 0, 0]);
    }

    #[test]
    fn pre_translate_preserves_npc_object_template_group() {
        // The full-plugin path carries the NPC Object Template (OBTE..STOP) so
        // modular robots render with their parts; post_translate's
        // strip_invalid_object_mod_properties + the raw-formid remap fixup keep
        // it FO4-safe. The cell-slice strip lives in a GraphOnly fixup instead.
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        let template_name = interner.intern("Default Template");
        let record_name = interner.intern("Thrasher");
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "OBTE",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 0, 0, 0])),
        );
        push_field(&mut record, "OBTF", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "FULL", FieldValue::String(template_name));
        push_field(
            &mut record,
            "OBTS",
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 25])),
        );
        push_field(&mut record, "STOP", FieldValue::Bytes(SmallVec::new()));
        push_field(
            &mut record,
            "CNAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3, 4])),
        );
        push_field(&mut record, "FULL", FieldValue::String(record_name));
        push_field(&mut record, "DATA", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(
            sigs,
            vec![
                "EDID", "OBTE", "OBTF", "FULL", "OBTS", "STOP", "CNAM", "FULL", "DATA"
            ]
        );
        let full_names: Vec<&str> = record
            .fields
            .iter()
            .filter_map(|field| match &field.value {
                FieldValue::String(sym) if field.sig.0 == *b"FULL" => interner.resolve(*sym),
                _ => None,
            })
            .collect();
        assert_eq!(full_names, vec!["Default Template", "Thrasher"]);
    }
