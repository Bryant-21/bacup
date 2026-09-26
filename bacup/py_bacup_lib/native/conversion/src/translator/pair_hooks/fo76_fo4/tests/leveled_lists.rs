

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

    /// CTDA with an explicit operator, comparison value and parameter 1 (the
    /// global a `GetGlobalValue` check reads).
    fn raw_ctda_on_global(
        function_id: u16,
        operator: u8,
        comparison_value: f32,
        global: u32,
    ) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[0] = operator << 5;
        bytes[4..8].copy_from_slice(&comparison_value.to_le_bytes());
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&global.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    /// `Gold_Treasury_Note_Loot_Enabled` (5A543C) ships as 1 and
    /// `Festive_Holiday_Enabled` (59CB08) ships as 0. A gate of `== 1` is
    /// therefore live for the first and dead for the second; assuming every
    /// global was 0 emptied `RA_LL_Rewards_PublicEvents_TreasuryNotes`.
    #[test]
    fn world_state_gates_are_classified_by_function_and_shipped_global_value() {
        let interner = StringInterner::new();
        let enabled = 0x5A543C_u32;
        let disabled = 0x59CB08_u32;
        let values = std::collections::HashMap::from([(enabled, 1.0_f32), (disabled, 0.0_f32)]);

        let live = raw_ctda_on_global(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 1.0, enabled);
        let dead = raw_ctda_on_global(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 1.0, disabled);

        assert_eq!(
            Fo76Fo4Hook::leveled_condition_disposition(
                &interner,
                &live,
                None,
                None,
                Some(&values)
            ),
            LeveledEntryDisposition::Keep
        );
        assert_eq!(
            Fo76Fo4Hook::leveled_condition_disposition(
                &interner,
                &dead,
                None,
                None,
                Some(&values)
            ),
            LeveledEntryDisposition::Reject
        );
        // Without the table both fall back to the old assume-zero behaviour.
        assert_eq!(
            Fo76Fo4Hook::leveled_condition_disposition(&interner, &live, None, None, None),
            LeveledEntryDisposition::Reject
        );

        for (name, condition, values, dropped) in [
            ("shipped-on global", live.clone(), Some(&values), false),
            ("shipped-off global", dead.clone(), Some(&values), true),
            (
                "nuke zone",
                raw_ctda_full(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID, 0, 0.0),
                None,
                true,
            ),
            (
                "global == 1",
                raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 1.0),
                None,
                true,
            ),
            (
                "global != 0",
                raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 1, 0.0),
                None,
                true,
            ),
            (
                "global >= 1",
                raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 3, 1.0),
                None,
                true,
            ),
            (
                "global == 0",
                raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 0.0),
                None,
                false,
            ),
            ("unrelated function", raw_ctda_full(56, 0, 1.0), None, false),
            (
                "unrelated condition form",
                raw_ctda_with_parameter_1(FO76_CONDITION_FORM_CONDITION_FUNCTION_ID, 0x123456),
                None,
                false,
            ),
        ] {
            assert_eq!(
                Fo76Fo4Hook::condition_gates_dropped_world_state(&interner, &condition, values),
                dropped,
                "{name}"
            );
        }
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

    /// `LPI_FloraRhododendron01` (525648): the normal-world leaf is gated by a
    /// `GetRandomPercent` chance roll, the nuke variant by a condition form
    /// (`Radstorm_NukaFlora_Spawn_Condition`). Keeping the condition-form entry
    /// and dropping the chance-gated one left placed refs resolving to the nuked
    /// plant everywhere.
    #[test]
    fn pre_translate_selects_fo4_reachable_leveled_entries() {
        type Fields = Vec<(&'static str, FieldValue)>;
        fn row(item: u32, conditions: Vec<FieldValue>, level: Option<f32>) -> Fields {
            let mut fields = vec![("LVLO", FieldValue::Uint(u64::from(item)))];
            fields.extend(conditions.into_iter().map(|condition| ("CTDA", condition)));
            if let Some(level) = level {
                fields.push(("LVIV", FieldValue::Float(1.0)));
                fields.push(("LVLV", FieldValue::Float(level)));
            }
            fields
        }
        fn header(use_all: bool, count: u64) -> Fields {
            let mut fields = Vec::new();
            if use_all {
                fields.push((
                    "LVLF",
                    FieldValue::Bytes(SmallVec::from_slice(&[LEVELED_LIST_USE_ALL_FLAG])),
                ));
            }
            fields.push(("LLCT", FieldValue::Uint(count)));
            fields
        }
        let global = GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID;
        let lock_row = |item, lock_global| {
            row(
                item,
                vec![raw_ctda_with_comparison_global(
                    GET_LOCK_LEVEL_CONDITION_FUNCTION_ID,
                    0,
                    lock_global,
                )],
                Some(1.0),
            )
        };
        let use_all = Some(LEVELED_LIST_USE_ALL_FLAG);

        let interner = StringInterner::new();
        for (name, signature, fields, expected, count, flags, normalize) in [
            (
                "special mole miner variants",
                "LVLN",
                vec![
                    header(false, 3),
                    row(
                        0x48FA64,
                        vec![raw_ctda_with_parameter_1(
                            FO76_CONDITION_FORM_CONDITION_FUNCTION_ID,
                            FO76_GLOWING_CREATURE_SPAWN_CONDITION_FORM,
                        )],
                        None,
                    ),
                    row(
                        0x3D14E4,
                        vec![raw_ctda_with_parameter_1(
                            FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                            FO76_SCORCHED_CREATURE_VARIANT_KEYWORD,
                        )],
                        None,
                    ),
                    row(0x3D14D9, vec![], None),
                ],
                vec![0x3D14D9],
                Some(1),
                None,
                false,
            ),
            (
                "novice lock and unlocked branches",
                "LVLI",
                vec![
                    header(true, 6),
                    lock_row(0x100001, FO76_LOCK_LEVEL_MASTER_GLOBAL),
                    lock_row(0x100002, FO76_LOCK_LEVEL_EXPERT_GLOBAL),
                    lock_row(0x100003, FO76_LOCK_LEVEL_ADVANCED_GLOBAL),
                    lock_row(0x100004, FO76_LOCK_LEVEL_NOVICE_GLOBAL),
                    row(
                        0x100005,
                        vec![raw_ctda_full(GET_LOCKED_CONDITION_FUNCTION_ID, 0, 1.0)],
                        Some(1.0),
                    ),
                    row(
                        0x100006,
                        vec![raw_ctda_full(GET_LOCKED_CONDITION_FUNCTION_ID, 0, 0.0)],
                        Some(1.0),
                    ),
                ],
                vec![0x100004, 0x100006],
                Some(2),
                use_all,
                true,
            ),
            (
                "unknown bonus beside unconditional rows",
                "LVLI",
                vec![
                    header(true, 3),
                    row(0x200001, vec![], None),
                    vec![
                        ("LVLO", FieldValue::Uint(0x200002)),
                        ("COED", raw_bytes(&[0; 20])),
                        ("CTDA", raw_ctda_full(300, 0, 1.0)),
                        ("LVIV", FieldValue::Float(1.0)),
                        ("LVLV", FieldValue::Float(1.0)),
                    ],
                    row(0x200003, vec![], None),
                ],
                vec![0x200001, 0x200003],
                None,
                use_all,
                false,
            ),
            (
                "chance-gated default leaf over condition-form variant",
                "LVLI",
                vec![
                    header(false, 4),
                    row(
                        0x525646,
                        vec![
                            raw_ctda_full(GET_RANDOM_PERCENT_CONDITION_FUNCTION_ID, 5, 100.0),
                            raw_ctda_full(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID, 0, 1.0),
                        ],
                        None,
                    ),
                    row(
                        0x525646,
                        vec![raw_ctda_full(FO76_CONDITION_FORM_CONDITION_FUNCTION_ID, 0, 1.0)],
                        None,
                    ),
                    row(
                        0x525642,
                        vec![raw_ctda_full(GET_RANDOM_PERCENT_CONDITION_FUNCTION_ID, 5, 65.0)],
                        None,
                    ),
                    row(0x525647, vec![], None),
                ],
                vec![0x525642, 0x525647],
                None,
                None,
                false,
            ),
            (
                "single lowest-level unknown fallback clears use-all",
                "LVLI",
                vec![
                    header(true, 3),
                    row(0x300001, vec![raw_ctda_full(300, 0, 1.0)], Some(10.0)),
                    row(0x300002, vec![raw_ctda_full(300, 0, 1.0)], Some(1.0)),
                    row(0x300003, vec![raw_ctda_full(300, 0, 1.0)], Some(5.0)),
                ],
                vec![0x300002],
                Some(1),
                Some(0),
                false,
            ),
            (
                "all perk-gated rows without fallback",
                "LVLI",
                vec![
                    header(false, 2),
                    row(0x400001, vec![raw_ctda_full(HAS_PERK_CONDITION_FUNCTION_ID, 0, 1.0)], None),
                    row(0x400002, vec![raw_ctda_full(HAS_PERK_CONDITION_FUNCTION_ID, 0, 1.0)], None),
                ],
                vec![],
                Some(0),
                None,
                false,
            ),
            (
                "overseer cache quest items behind untranslatable conditions",
                "LVLI",
                vec![
                    header(true, 4),
                    row(0x3D7F44, vec![raw_ctda_full(857, 0, 0.0)], None),
                    row(0x3D4725, vec![raw_ctda_full(857, 2, 0.0)], None),
                    row(0x564078, vec![raw_ctda_full(853, 0, 0.0)], None),
                    row(
                        0x1389EC,
                        vec![raw_ctda_full(857, 3, 1.0), raw_ctda_full(47, 0, 0.0)],
                        None,
                    ),
                ],
                vec![0x3D7F44, 0x3D4725, 0x564078, 0x1389EC],
                Some(4),
                use_all,
                true,
            ),
            (
                "event global off branch",
                "LVLI",
                vec![
                    header(false, 2),
                    row(0x500001, vec![raw_ctda_full(global, 0, 1.0)], None),
                    row(0x500002, vec![raw_ctda_full(global, 0, 0.0)], None),
                ],
                vec![0x500002],
                None,
                None,
                false,
            ),
            (
                "nuke and event gated entries",
                "LVLI",
                vec![
                    header(false, 3),
                    row(
                        0x58AFD6,
                        vec![raw_ctda_full(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID, 0, 1.0)],
                        None,
                    ),
                    row(0x5A0019, vec![raw_ctda_full(global, 0, 1.0)], None),
                    row(0x58AFD5, vec![], None),
                ],
                vec![0x58AFD5],
                Some(1),
                None,
                false,
            ),
        ] {
            let mut record = make_record(signature, &interner);
            for (sig, value) in fields.into_iter().flatten() {
                push_field(&mut record, sig, value);
            }
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            let value_of = |record: &Record, sig: &[u8; 4]| {
                record
                    .fields
                    .iter()
                    .find(|entry| entry.sig.0 == *sig)
                    .map(|entry| entry.value.clone())
            };
            assert_eq!(converted_lvlo_ids(&record, &interner), expected, "{name}");
            assert!(record.fields.iter().all(|entry| entry.sig.0 != *b"COED"), "{name}");
            if let Some(count) = count {
                assert_eq!(value_of(&record, b"LLCT"), Some(FieldValue::Uint(count)), "{name}");
            }
            if let Some(flags) = flags {
                assert_eq!(
                    value_of(&record, b"LVLF"),
                    Some(FieldValue::Bytes(SmallVec::from_slice(&[flags]))),
                    "{name}"
                );
            }
            if signature == "LVLN" {
                assert!(
                    record
                        .fields
                        .iter()
                        .all(|entry| !matches!(&entry.sig.0, b"CTDA" | b"CTDT")),
                    "{name}: creature-variant conditions are dropped"
                );
            }
            if normalize {
                let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
                let crate::target_normalize::TargetRecordNormalization::Keep(record) =
                    crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                        &schema, &interner,
                    )
                    .normalize(record)
                else {
                    panic!("{name}: LVLI is supported by FO4 schema")
                };
                assert_eq!(converted_lvlo_ids(&record, &interner), expected, "{name}");
                assert!(
                    record
                        .fields
                        .iter()
                        .all(|entry| !matches!(&entry.sig.0, b"CTDA" | b"CTDT")),
                    "{name}"
                );
            }
        }
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
    fn rare_encounter_entry_chance_none_survives_selection_and_full_translate() {
        let interner = StringInterner::new();
        for (flags, expected_count) in [(0x48, 1), (0x08, 2)] {
            let mut record = make_record("LVLN", &interner);
            push_field(&mut record, "LVLF", FieldValue::Uint(flags));
            for chance in [65.0, 95.0] {
                push_field(&mut record, "LVLO", FieldValue::Uint(0x2EE5A2));
                push_field(&mut record, "CTDA", raw_ctda_full(765, 0, 1.0));
                push_field(&mut record, "LVOV", FieldValue::Float(chance));
            }
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
            let entries: Vec<_> = record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "LVLO")
                .collect();
            assert_eq!(entries.len(), expected_count);
            let schema = AuthoringSchema::for_game("fo4").unwrap();
            let encoded = crate::target_write::encode_field_pub(
                entries.last().unwrap(),
                schema.record_def("LVLN"),
                &interner,
            )
            .unwrap();
            assert_eq!(encoded[10], 95);
            assert_eq!(
                record
                    .fields
                    .iter()
                    .find(|field| field.sig.as_str() == "LLCT")
                    .unwrap()
                    .value,
                FieldValue::Uint(expected_count as u64)
            );
        }

        use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};

        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let source_schema = AuthoringSchema::for_game("fo76").unwrap();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        for (signature, legacy) in [("LVLN", false), ("LVLN", true), ("LVLI", false)] {
            let mut record = make_record(signature, &interner);
            let reference = 0x1D50EA_u32;
            let value = if legacy {
                let mut bytes = vec![1, 0, 0, 0];
                bytes.extend_from_slice(&reference.to_le_bytes());
                bytes.extend_from_slice(&[1, 0, 95, 0]);
                FieldValue::Bytes(bytes.into())
            } else {
                FieldValue::Uint(u64::from(reference))
            };
            push_field(&mut record, "LVLO", value);
            if !legacy {
                push_field(&mut record, "LVOV", FieldValue::Float(95.0));
            }
            translator
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
            let mut record = match translator.translate(&record, &interner) {
                TranslateResult::Translated(record) => record,
                other => panic!("expected translated {signature}, got {other:?}"),
            };
            translator
                .post_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
            translator
                .run_target_hook(
                    &mut crate::translator::target_hook::TargetCtx { interner: &interner },
                    &mut record,
                )
                .unwrap();
            crate::translator::class_a_normalize::normalize_flags_and_enums(
                &mut record,
                &schema,
                &interner,
            );
            let normalizer = TargetRecordNormalizer {
                target_schema: &schema,
                source_record_def: source_schema.record_def(signature),
                interner: Some(&interner),
            };
            let TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
                panic!("expected normalized {signature}");
            };
            let entry = record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "LVLO")
                .unwrap();
            let encoded = crate::target_write::encode_field_pub(
                entry,
                schema.record_def(signature),
                &interner,
            )
            .unwrap();
            assert_eq!(encoded.len(), 12, "{signature}, legacy={legacy}");
            assert_eq!(encoded[10], 95, "{signature}, legacy={legacy}");
        }
    }

    #[test]
    fn pre_translate_converts_fo76_lvlo_shapes_to_encodable_fo4_entries() {
        let interner = StringInterner::new();
        let variant_sym = interner.intern("variant");
        let value_sym = interner.intern("value");
        let reference_variant = interner.intern("reference");
        let raw_twelve_byte = {
            let mut bytes = vec![1, 0, 0, 0];
            bytes.extend_from_slice(&0x0083_9C65_u32.to_le_bytes());
            bytes.extend_from_slice(&[1, 0, 0, 0]);
            bytes
        };
        for (name, signature, fields, plugin, expected) in [
            (
                "LVLN reference struct",
                "LVLN",
                vec![
                    ("LLCT", FieldValue::Uint(1)),
                    (
                        "LVLO",
                        FieldValue::Struct(vec![
                            (variant_sym, FieldValue::String(reference_variant)),
                            (value_sym, FieldValue::Uint(0x868BB8)),
                        ]),
                    ),
                    ("LVIV", FieldValue::Float(1.0)),
                    ("LVLV", FieldValue::Float(1.0)),
                ],
                None,
                vec![1, 0, 0, 0, 0xB8, 0x8B, 0x86, 0, 1, 0, 0, 0],
            ),
            (
                "raw 12-byte LVLO",
                "LVLI",
                vec![("LVLO", FieldValue::Bytes(SmallVec::from_vec(raw_twelve_byte)))],
                Some("SeventySix.esm"),
                vec![1, 0, 0, 0, 0x65, 0x9C, 0x83, 0, 1, 0, 0, 0],
            ),
            (
                "four-byte reference is not read as a level",
                "LVLI",
                vec![
                    (
                        "LVLO",
                        FieldValue::Bytes(SmallVec::from_vec(0x0083_9C65_u32.to_le_bytes().to_vec())),
                    ),
                    ("LVIV", FieldValue::Float(7.0)),
                    ("LVLV", FieldValue::Float(2.0)),
                ],
                None,
                vec![2, 0, 0, 0, 0x65, 0x9C, 0x83, 0, 7, 0, 0, 0],
            ),
            (
                "zero level clamps to one",
                "LVLI",
                vec![
                    ("LVLO", FieldValue::Uint(0x0083_9C65)),
                    ("LVIV", FieldValue::Float(1.0)),
                    ("LVLV", FieldValue::Float(0.0)),
                ],
                None,
                vec![1, 0, 0, 0, 0x65, 0x9C, 0x83, 0, 1, 0, 0, 0],
            ),
            (
                "FO76 caps remap to FO4 caps",
                "LVLI",
                vec![
                    ("LLCT", FieldValue::Uint(1)),
                    (
                        "LVLO",
                        FieldValue::Bytes(SmallVec::from_vec(vec![
                            1, 0, 0, 0, 0x0F, 0, 0, 0, 100, 0, 0, 0,
                        ])),
                    ),
                ],
                Some("Fallout4.esm"),
                vec![1, 0, 0, 0, 0x0F, 0, 0, 0, 100, 0, 0, 0],
            ),
        ] {
            let mut record = make_record(signature, &interner);
            for (sig, value) in fields {
                push_field(&mut record, sig, value);
            }
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            if let Some(plugin) = plugin {
                let lvlo_entry = record
                    .fields
                    .iter()
                    .find(|entry| entry.sig.as_str() == "LVLO")
                    .expect("converted LVLO");
                let FieldValue::Struct(fields) = &lvlo_entry.value else {
                    panic!("{name}: LVLO should be converted into an FO4 struct");
                };
                let Some(FieldValue::FormKey(fk)) = named_value(fields, "item", &interner) else {
                    panic!("{name}: LVLO item should be a typed FormKey");
                };
                assert_eq!(interner.resolve(fk.plugin), Some(plugin), "{name}");
            }

            let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
            let crate::target_normalize::TargetRecordNormalization::Keep(record) =
                crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                    &schema, &interner,
                )
                .normalize(record)
            else {
                panic!("{name}: {signature} is supported by FO4 schema")
            };
            let lvlo_entry = record
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "LVLO")
                .expect("converted LVLO");
            let encoded = crate::target_write::encode_field_pub(
                lvlo_entry,
                schema.record_def(signature),
                &interner,
            )
            .expect("converted LVLO encodes");
            assert_eq!(encoded, expected, "{name}");
        }
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
