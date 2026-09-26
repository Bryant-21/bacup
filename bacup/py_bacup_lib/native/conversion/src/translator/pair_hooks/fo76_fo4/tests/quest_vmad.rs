    fn w05_distance_vmad(root_name: &str) -> (Vec<u8>, usize) {
        fn string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        let mut bytes = vec![6, 0, 2, 0, 3, 0];
        for name in ["KeepBefore", root_name, "KeepAfter"] {
            string(&mut bytes, name);
            bytes.push(0);
            bytes.extend_from_slice(&1_u16.to_le_bytes());
            string(&mut bytes, "StageToSet");
            bytes.extend_from_slice(&[3, 1]);
            bytes.extend_from_slice(&900_i32.to_le_bytes());
        }
        let root_end = bytes.len();
        bytes.push(FO76_QUST_FRAGMENT_VERSION);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        string(&mut bytes, "DefaultQuestDistanceCheckScript");
        bytes.push(0);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        (bytes, root_end)
    }

    #[test]
    fn pre_translate_rebinds_only_scoped_w05_root_distance_scripts() {
        let interner = StringInterner::new();
        let (source, _) = w05_distance_vmad("DefaultQuestDistanceCheckScript");
        let (expected, _) = w05_distance_vmad("B21_W05QuestDistanceCheckScript");
        for local_id in [0x40D28D, 0x54EDB9, 0x53AF40] {
            let mut record = make_record("QUST", &interner);
            record.form_key.local = local_id;
            push_field(&mut record, "VMAD", FieldValue::Bytes(SmallVec::from_slice(&source)));
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, FieldValue::Bytes(SmallVec::from_slice(&expected)));
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, FieldValue::Bytes(SmallVec::from_slice(&expected)));
        }
    }

    #[test]
    fn pre_translate_keeps_distance_scripts_on_other_records_and_plugins() {
        let interner = StringInterner::new();
        let (source, _) = w05_distance_vmad("DefaultQuestDistanceCheckScript");
        for (signature, local_id, plugin) in [
            ("QUST", 0x40D28D, "Other.esm"),
            ("QUST", 0x031163, "SeventySix.esm"),
            ("PERK", 0x40D28D, "SeventySix.esm"),
        ] {
            let mut record = make_record(signature, &interner);
            record.form_key.local = local_id;
            record.form_key.plugin = interner.intern(plugin);
            let value = FieldValue::Bytes(SmallVec::from_slice(&source));
            push_field(&mut record, "VMAD", value.clone());
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, value);
        }
    }

    #[test]
    fn pre_translate_keeps_truncated_or_unknown_distance_root_payloads() {
        let interner = StringInterner::new();
        let (source, root_end) = w05_distance_vmad("DefaultQuestDistanceCheckScript");
        let mut variants: Vec<Vec<u8>> = (0..root_end).map(|length| source[..length].to_vec()).collect();
        variants.push(w05_distance_vmad("OtherScript").0);
        let mut wrong_version = source.clone();
        wrong_version[0] = 5;
        variants.push(wrong_version);
        for bytes in variants {
            let mut record = make_record("QUST", &interner);
            record.form_key.local = 0x40D28D;
            let value = FieldValue::Bytes(SmallVec::from_vec(bytes));
            push_field(&mut record, "VMAD", value.clone());
            normalize_w05_quest_distance_script(&interner, &mut record);
            assert_eq!(record.fields[0].value, value);
        }
    }

    fn w05_perk_vmad_fixtures() -> [(u32, &'static [u8]); 7] {
        [
            (0x593DD5, include_bytes!("fixtures/w05_perk_593dd5.vmad")),
            (0x5614E5, include_bytes!("fixtures/w05_perk_5614e5.vmad")),
            (0x41B765, include_bytes!("fixtures/w05_perk_41b765.vmad")),
            (0x40483A, include_bytes!("fixtures/w05_perk_40483a.vmad")),
            (0x40483B, include_bytes!("fixtures/w05_perk_40483b.vmad")),
            (0x59276C, include_bytes!("fixtures/w05_perk_59276c.vmad")),
            (0x5A11A2, include_bytes!("fixtures/w05_perk_5a11a2.vmad")),
        ]
    }

    #[test]
    fn pre_translate_normalizes_captured_w05_perks_without_changing_bindings() {
        let interner = StringInterner::new();
        for (local_id, source) in w05_perk_vmad_fixtures() {
            let mut offset = 6;
            for _ in 0..qust_vmad_read_u16(source, 4).unwrap() {
                qust_vmad_read_script(source, &mut offset, 2).unwrap();
            }
            let mut expected = source[..source.len() - 2].to_vec();
            expected[offset] = 3;
            let mut record = make_record("PERK", &interner);
            record.form_key.local = local_id;
            push_field(&mut record, "VMAD", FieldValue::Bytes(SmallVec::from_slice(source)));
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, FieldValue::Bytes(SmallVec::from_vec(expected.clone())));
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, FieldValue::Bytes(SmallVec::from_vec(expected)));
        }
    }

    #[test]
    fn pre_translate_keeps_unknown_or_truncated_w05_perk_layouts() {
        let interner = StringInterner::new();
        for (local_id, source) in w05_perk_vmad_fixtures() {
            let mut variants: Vec<Vec<u8>> = (0..source.len()).map(|length| source[..length].to_vec()).collect();
            let mut unknown_tail = source.to_vec();
            *unknown_tail.last_mut().unwrap() = 1;
            variants.push(unknown_tail);
            let mut wrong_header = source.to_vec();
            wrong_header[0] = 5;
            variants.push(wrong_header);
            for bytes in variants {
                let mut record = make_record("PERK", &interner);
                record.form_key.local = local_id;
                let value = FieldValue::Bytes(SmallVec::from_vec(bytes));
                push_field(&mut record, "VMAD", value.clone());
                Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
                assert_eq!(record.fields[0].value, value);
            }
        }
    }

    #[test]
    fn pre_translate_keeps_w05_perk_payload_on_other_records_and_plugins() {
        let interner = StringInterner::new();
        let (_, source) = w05_perk_vmad_fixtures()[0];
        for (signature, local_id, plugin) in [
            ("PERK", 0x593DD5, "Other.esm"),
            ("PERK", 0x593DD6, "SeventySix.esm"),
            ("TERM", 0x593DD5, "SeventySix.esm"),
            ("PERK", 0x5614E5, "SeventySix.esm"),
        ] {
            let mut record = make_record(signature, &interner);
            record.form_key.local = local_id;
            record.form_key.plugin = interner.intern(plugin);
            let value = FieldValue::Bytes(SmallVec::from_slice(source));
            push_field(&mut record, "VMAD", value.clone());
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, value);
        }
    }


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
        let interner = StringInterner::new();
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

    fn trade_secrets_clue_vmad(alias_id: u16, script: &str, cutoff: i32) -> FieldValue {
        let FieldValue::Bytes(mut bytes) = qust_vmad_with_re_alias_properties(script, &[
            ("StageToSet", 3, 427_i32.to_le_bytes().to_vec()),
            ("TurnOffStage", 3, cutoff.to_le_bytes().to_vec()),
            ("PrereqStage", 3, 400_i32.to_le_bytes().to_vec()),
        ]) else {
            unreachable!();
        };
        // This fixture has a six-byte header, empty fragment block and one alias.
        bytes[15..17].copy_from_slice(&alias_id.to_le_bytes());
        FieldValue::Bytes(bytes)
    }

    #[test]
    fn pre_translate_trade_secrets_keeps_clues_readable_after_wrong_codes() {
        let interner = StringInterner::new();
        for (alias_id, script) in [
            (22, "DefaultAliasOnRead"),
            (23, "DefaultAliasOnRead"),
            (24, "DefaultAliasOnRead"),
            (0, "DefaultAliasInventoryManagementC"),
            (0, "defaultaliasinventorymanagementd"),
            (0, "DefaultAliasInventoryManagementE"),
        ] {
            let mut record = make_record("QUST", &interner);
            record.form_key.local = 0x003F_28C3;
            push_field(&mut record, "VMAD", trade_secrets_clue_vmad(alias_id, script, 430));
            let expected = trade_secrets_clue_vmad(alias_id, script, 449);
            for _ in 0..2 {
                Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
                assert_eq!(record.fields[0].value, expected, "alias {alias_id}");
            }
        }
    }

    #[test]
    fn pre_translate_trade_secrets_clue_repair_is_scoped() {
        let interner = StringInterner::new();
        for (quest, plugin, alias, script, cutoff) in [
            (0x003F_28C7, "SeventySix.esm", 22, "DefaultAliasOnRead", 430),
            (0x003F_28C3, "Other.esm", 22, "DefaultAliasOnRead", 430),
            (0x003F_28C3, "SeventySix.esm", 21, "DefaultAliasOnRead", 430),
            (0x003F_28C3, "SeventySix.esm", 25, "DefaultAliasOnRead", 430),
            (0x003F_28C3, "SeventySix.esm", 22, "OtherAliasScript", 430),
            (0x003F_28C3, "SeventySix.esm", 22, "DefaultAliasOnRead", 450),
            (0x003F_28C3, "SeventySix.esm", 0, "DefaultAliasOnRead", 430),
            (0x003F_28C3, "SeventySix.esm", 0, "DefaultAliasInventoryManagementB", 430),
            (0x003F_28C3, "SeventySix.esm", 22, "DefaultAliasInventoryManagementC", 430),
        ] {
            let mut record = make_record("QUST", &interner);
            record.form_key.local = quest;
            record.form_key.plugin = interner.intern(plugin);
            let original = trade_secrets_clue_vmad(alias, script, cutoff);
            push_field(&mut record, "VMAD", original.clone());
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, original);
        }
    }

    #[test]
    fn pre_translate_trade_secrets_leaves_truncated_clue_binding_unchanged() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = 0x003F_28C3;
        let FieldValue::Bytes(mut bytes) = trade_secrets_clue_vmad(22, "DefaultAliasOnRead", 430) else {
            unreachable!();
        };
        bytes.pop();
        let original = FieldValue::Bytes(bytes);
        push_field(&mut record, "VMAD", original.clone());
        Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
        assert_eq!(record.fields[0].value, original);
    }

    #[test]
    fn pre_translate_preserves_non_player_default_alias_events() {
        for (script, source_property, target_property) in [
            ("DefaultAliasOnContainerChangedTo", "PlayerPickupType", "PlayerPickupOnly"),
            ("DefaultAliasOnHit", "PlayerHitType", "PlayerHitOnly"),
            ("defaultaliasonopen", "PlayerActivateType", "PlayerTriggerOnly"),
            ("DefaultAliasOnActivate", "PlayerActivateType", "PlayerActivateOnly"),
            ("DefaultAliasOnTriggerEnter", "PlayerTriggerType", "PlayerTriggerOnly"),
            ("defaultaliasontriggerentera", "PlayerTriggerType", "PlayerTriggerOnly"),
            ("DefaultAliasOnTriggerEnterB", "PlayerTriggerType", "PlayerTriggerOnly"),
            ("DefaultAliasOnTriggerLeave", "PlayerTriggerType", "PlayerTriggerOnly"),
        ] {
            for mode in -1_i32..=3 {
                let interner = StringInterner::new();
                let mut record = make_record("QUST", &interner);
                push_field(&mut record, "VMAD", qust_vmad_with_re_alias_properties(script, &[
                    ("StageToSet", 3, 1411_i32.to_le_bytes().to_vec()),
                    (source_property, 3, mode.to_le_bytes().to_vec()),
                    ("PrereqStage", 3, 1410_i32.to_le_bytes().to_vec()),
                ]));
                let expected = qust_vmad_with_re_alias_properties(script, &[
                    ("StageToSet", 3, 1411_i32.to_le_bytes().to_vec()),
                    (target_property, 5, vec![u8::from(mode > 0)]),
                    ("PrereqStage", 3, 1410_i32.to_le_bytes().to_vec()),
                ]);
                Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
                assert_eq!(record.fields[0].value, expected, "{script} mode {mode}");
                Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
                assert_eq!(record.fields[0].value, expected);
            }
        }
    }

    fn arena_chem_vmad(alias_id: u16, cutoff: Option<i32>) -> FieldValue {
        let mut properties = vec![
            ("StageToSet", 3, 5200_i32.to_le_bytes().to_vec()),
            ("PrereqStage", 3, 5100_i32.to_le_bytes().to_vec()),
            ("PlayerPickupOnly", 5, vec![1]),
        ];
        if let Some(cutoff) = cutoff {
            properties.push(("TurnOffStage", 3, cutoff.to_le_bytes().to_vec()));
        }
        let FieldValue::Bytes(mut bytes) = qust_vmad_with_re_alias_properties(
            "DefaultAliasOnContainerChangedTo", &properties,
        ) else { unreachable!() };
        bytes[15..17].copy_from_slice(&alias_id.to_le_bytes());
        FieldValue::Bytes(bytes)
    }

    #[test]
    fn pre_translate_arena_chem_pickup_stops_after_timeout() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = 0x0042_F31B;
        push_field(&mut record, "VMAD", arena_chem_vmad(40, None));
        for _ in 0..2 {
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, arena_chem_vmad(40, Some(5300)));
        }
    }

    #[test]
    fn pre_translate_arena_chem_cutoff_does_not_override_other_bindings() {
        let interner = StringInterner::new();
        for (quest, plugin, alias, cutoff) in [
            (0x0042_F31B, "Other.esm", 40, None),
            (0x0041_C9E6, "SeventySix.esm", 40, None),
            (0x0042_F31B, "SeventySix.esm", 41, None),
            (0x0042_F31B, "SeventySix.esm", 40, Some(5400)),
        ] {
            let mut record = make_record("QUST", &interner);
            record.form_key.local = quest;
            record.form_key.plugin = interner.intern(plugin);
            let original = arena_chem_vmad(alias, cutoff);
            push_field(&mut record, "VMAD", original.clone());
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, original);
        }
    }

    #[test]
    fn pre_translate_keeps_explicit_fo4_player_filter_and_unknown_source_modes() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(&mut record, "VMAD", qust_vmad_with_re_alias_properties("DefaultAliasOnHit", &[
            ("PlayerHitType", 3, 0_i32.to_le_bytes().to_vec()),
            ("PlayerHitOnly", 5, vec![1]),
        ]));
        Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
        assert_eq!(record.fields[0].value, qust_vmad_with_re_alias_properties("DefaultAliasOnHit", &[
            ("PlayerHitOnly", 5, vec![1]),
        ]));
        for (property_type, value) in [(3, 4_i32.to_le_bytes().to_vec()), (5, vec![0])] {
            let mut record = make_record("QUST", &interner);
            let original = qust_vmad_with_re_alias_properties("DefaultAliasOnHit", &[
                ("PlayerHitType", property_type, value),
            ]);
            push_field(&mut record, "VMAD", original.clone());
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, original);
        }
    }

    #[test]
    fn pre_translate_preserves_activation_item_requirements() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        let required_item = vec![0, 0, 255, 255, 0xAB, 0xA0, 0x5F, 8];
        push_field(&mut record, "VMAD", qust_vmad_with_re_alias_properties("DefaultAliasOnActivate", &[
            ("ItemRequired", 1, required_item.clone()),
            ("NumItemsRequired", 3, 1_i32.to_le_bytes().to_vec()),
            ("PlayerActivateType", 3, 3_i32.to_le_bytes().to_vec()),
            ("PrereqStage", 3, 2100_i32.to_le_bytes().to_vec()),
            ("StageToSet", 3, 2200_i32.to_le_bytes().to_vec()),
        ]));
        let expected = qust_vmad_with_re_alias_properties("B21_ActivateAliasWithRequiredItem", &[
            ("ItemRequired", 1, required_item),
            ("NumItemsRequired", 3, 1_i32.to_le_bytes().to_vec()),
            ("PlayerActivateOnly", 5, vec![1]),
            ("PrereqStage", 3, 2100_i32.to_le_bytes().to_vec()),
            ("StageToSet", 3, 2200_i32.to_le_bytes().to_vec()),
        ]);
        for _ in 0..2 {
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, expected);
        }
    }

    #[test]
    fn pre_translate_preserves_message_button_array_and_activation_stage() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        let stages: Vec<u8> = [6_i32, 0, 1310, 1315, 1320, 1325, 1330].iter().flat_map(|n| n.to_le_bytes()).collect();
        let properties = [
            ("StageToSet", 3, 1000_i32.to_le_bytes().to_vec()),
            ("ButtonStagesToSet", 13, stages),
        ];
        push_field(&mut record, "VMAD", qust_vmad_with_re_alias_properties("defaultshowmessageonactivatealias", &properties));
        let expected = qust_vmad_with_re_alias_properties("B21_ShowMessageOnActivateAlias", &properties);
        for _ in 0..2 {
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value, expected);
        }
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

    /// `RSVP03_QuestlineRestart`'s shape: a non-Optional `Player` alias filled
    /// from the player-connect event. Without an adapter the fill is unproven,
    /// `classify_quest` rejects the quest as `unsupported_quest`, and its SMQN
    /// is never emitted — which is why 29 of the 32 questline-restart quests
    /// had no Story Manager node at all.
    fn player_connect_alias_record(interner: &StringInterner, event_data: u32) -> Record {
        let mut record = make_record("QUST", interner);
        push_field(&mut record, "ENAM", raw_bytes(b"PCON"));
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(5_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(4_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALID",
            FieldValue::Bytes(SmallVec::from_vec(b"Player\0".to_vec())),
        );
        // FNAM 0 - the alias is NOT Optional, so an unproven fill disqualifies it.
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFE",
            FieldValue::Bytes(SmallVec::from_vec(
                FO76_QUEST_EVENT_PCON.to_le_bytes().to_vec(),
            )),
        );
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(event_data.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALED", FieldValue::None);
        record
    }

    #[test]
    fn player_connect_player1_alias_fills_the_player_and_keeps_the_quest_translatable() {
        let interner = StringInterner::new();
        let mut record = player_connect_alias_record(&interner, FO76_QUEST_EVENT_PLAYER1);

        assert!(!qust_has_untranslatable_event_alias_for_source(
            &interner, &record
        ));

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let alfr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"ALFR")
            .expect("a PCON/P1 alias resolves to the player");
        let FieldValue::FormKey(player) = &alfr.value else {
            panic!("ALFR should be a FormKey");
        };
        assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);
        assert_eq!(interner.resolve(player.plugin), Some(FO4_MASTER_NAME));
        // The event fill is replaced, not kept alongside the forced reference.
        assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"ALFE"));
        assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"ALFD"));
        // The alias stays required: it now fills, so it must not be weakened.
        let FieldValue::Bytes(flags) = &record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"FNAM")
            .expect("alias flags")
            .value
        else {
            panic!("FNAM should be bytes");
        };
        assert_eq!(u32::from_le_bytes(flags[..4].try_into().unwrap()) & 0x2, 0);
    }

    #[test]
    fn player_connect_alias_on_a_non_player_slot_is_left_to_the_existing_paths() {
        let interner = StringInterner::new();
        // The one FO76 outlier fills Reference1, not Player1; it must not be
        // swept into the player rewrite.
        let record = player_connect_alias_record(&interner, FO4_QUEST_EVENT_REFERENCE1);
        assert!(!qust_event_alias_is_player_connect_subject(
            FO76_QUEST_EVENT_PCON,
            FO4_QUEST_EVENT_REFERENCE1,
        ));
        assert!(qust_has_untranslatable_event_alias_for_source(
            &interner, &record
        ));
    }

    fn scpt_reference3_alias_record(interner: &StringInterner, quest: u32, alias_id: u32) -> Record {
        let mut record = make_record("QUST", interner);
        record.form_key.local = quest;
        push_field(&mut record, "ENAM", raw_bytes(b"SCPT"));
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(3_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(alias_id.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALID",
            FieldValue::Bytes(SmallVec::from_vec(b"currentPlayer\0".to_vec())),
        );
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(0x10_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFE",
            FieldValue::Bytes(SmallVec::from_vec(FO76_QUEST_EVENT_SCPT.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(
                FO76_QUEST_EVENT_REFERENCE3.to_le_bytes().to_vec(),
            )),
        );
        push_field(&mut record, "ALED", FieldValue::None);
        record
    }

    #[test]
    fn rsvp03_misc_pointer_reference3_alias_fills_the_player() {
        let interner = StringInterner::new();
        let mut record = scpt_reference3_alias_record(&interner, 0x4FC083, 2);

        assert!(!qust_has_untranslatable_event_alias_for_source(
            &interner, &record
        ));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let alfr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"ALFR")
            .expect("the proven R3 alias resolves to the player");
        let FieldValue::FormKey(player) = &alfr.value else {
            panic!("ALFR should be a FormKey");
        };
        assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);
        assert!(!record.fields.iter().any(|entry| entry.sig.0 == *b"ALFD"));
    }

    #[test]
    fn distance_marker_misc_quest_reference3_aliases_fill_the_player() {
        let interner = StringInterner::new();
        for (quest, alias_id) in [
            (0x46ED03, 2),
            (0x4F7BF4, 2),
            (0x4F7BFA, 2),
            (0x4F7BFD, 2),
            (0x4F852A, 2),
            (0x4F852F, 2),
            (0x4F8A93, 2),
            (0x4F900F, 2),
            (0x50DE51, 2),
            (0x50EAF6, 1),
            (0x50F989, 2),
        ] {
            let mut record = scpt_reference3_alias_record(&interner, quest, alias_id);
            assert!(
                !qust_has_untranslatable_event_alias_for_source(&interner, &record),
                "{quest:06X} alias {alias_id} should be proven"
            );
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
            let alfr = record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"ALFR")
                .expect("the proven R3 alias resolves to the player");
            let FieldValue::FormKey(player) = &alfr.value else {
                panic!("ALFR should be a FormKey");
            };
            assert_eq!(player.local, FO4_PLAYER_REF_FORM_ID);

            let other_alias = scpt_reference3_alias_record(&interner, quest, alias_id + 5);
            assert!(qust_has_untranslatable_event_alias_for_source(
                &interner,
                &other_alias
            ));
        }
    }

    #[test]
    fn reference3_alias_without_per_quest_proof_stays_unproven() {
        let interner = StringInterner::new();
        let record = scpt_reference3_alias_record(&interner, 0x4FC084, 2);

        assert!(qust_has_untranslatable_event_alias_for_source(
            &interner, &record
        ));
    }

    #[test]
    fn sfl02_child_quest_reference3_aliases_are_proven_only_on_their_listed_alias() {
        let interner = StringInterner::new();

        for (quest, alias_id) in [(0x018ECF, 0), (0x32BB59, 1)] {
            let record = scpt_reference3_alias_record(&interner, quest, alias_id);
            assert!(!qust_has_untranslatable_event_alias_for_source(
                &interner, &record
            ));
            let other_alias = scpt_reference3_alias_record(&interner, quest, alias_id + 5);
            assert!(qust_has_untranslatable_event_alias_for_source(
                &interner,
                &other_alias
            ));
        }
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

    #[test]
    fn owning_player_alias_matches_the_source_decoded_alias_name() {
        let interner = StringInterner::new();
        let mut record = owning_player_fragment_record(&interner, "owningPlayer", true, true);
        let alias_name = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"ALID")
            .expect("owningPlayer alias name");
        alias_name.value = FieldValue::String(interner.intern("owningPlayer"));

        assert!(!qust_has_untranslatable_event_alias_for_source(&interner, &record));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        assert!(record.fields.iter().any(|entry| entry.sig.0 == *b"ALFR"));
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

    /// A random encounter keeps the one event fill FO4's `RETriggerScript`
    /// actually sends, so the quest stays runtime-safe and its Story Manager
    /// node is emitted. The alias name decodes as an interned string from the
    /// live source and as raw bytes from a hand-built record; both must match.
    #[test]
    fn random_encounter_trigger_alias_keeps_its_reference1_fill() {
        fn random_encounter(
            interner: &StringInterner,
            editor_id: &str,
            alias_name: FieldValue,
            event: u32,
            event_data: u32,
        ) -> Record {
            let mut record = make_record("QUST", interner);
            record.form_key.local = 0x0048_45AA;
            record.eid = Some(interner.intern(editor_id));
            push_field(
                &mut record,
                "ENAM",
                FieldValue::Bytes(SmallVec::from_vec(
                    FO76_QUEST_EVENT_SCPT.to_le_bytes().to_vec(),
                )),
            );
            push_qust_event_alias(&mut record, 0, 0, event, event_data);
            record
                .fields
                .iter_mut()
                .find(|entry| entry.sig.0 == *b"ALID")
                .expect("trigger alias name")
                .value = alias_name;
            record
        }

        let interner = StringInterner::new();
        for alias_name in [
            FieldValue::String(interner.intern("TRIGGER")),
            FieldValue::Bytes(SmallVec::from_slice(b"TRIGGER\0")),
        ] {
            let mut exact = random_encounter(
                &interner,
                "RE_SceneSM04",
                alias_name,
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            );
            assert!(!qust_has_untranslatable_event_alias_for_source(
                &interner, &exact
            ));
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut exact)
                .unwrap();
            assert!(exact.fields.iter().any(|entry| {
                entry.sig.0 == *b"ALFD"
                    && field_value_to_u32(&entry.value) == Some(FO4_QUEST_EVENT_REFERENCE1)
            }));
        }

        // Expedition hub events and scoreboard quests share the `RE` letters but
        // have no trigger; a Reference2 fill has no sender either.
        for (editor_id, alias_name, event_data) in [
            (
                "XPD_HubRE_TakePhoto_CameraShy",
                b"TRIGGER\0".as_slice(),
                FO4_QUEST_EVENT_REFERENCE1,
            ),
            (
                "SCORE_S5_COMP_Quest_Camp_Lite_Inspector",
                b"TRIGGER\0".as_slice(),
                FO4_QUEST_EVENT_REFERENCE1,
            ),
            (
                "RE_SceneSM04",
                b"CenterMarker\0".as_slice(),
                FO4_QUEST_EVENT_REFERENCE1,
            ),
            (
                "RE_SceneSM04",
                b"TRIGGER\0".as_slice(),
                FO4_QUEST_EVENT_REFERENCE2,
            ),
        ] {
            let lookalike = random_encounter(
                &interner,
                editor_id,
                FieldValue::Bytes(SmallVec::from_slice(alias_name)),
                FO76_QUEST_EVENT_SCPT,
                event_data,
            );
            assert!(qust_has_untranslatable_event_alias_for_source(
                &interner, &lookalike
            ));
        }

        let mut no_event_scope = random_encounter(
            &interner,
            "RE_SceneSM04",
            FieldValue::Bytes(SmallVec::from_slice(b"TRIGGER\0")),
            FO76_QUEST_EVENT_SCPT,
            FO4_QUEST_EVENT_REFERENCE1,
        );
        no_event_scope.fields.retain(|entry| entry.sig.0 != *b"ENAM");
        assert!(qust_has_untranslatable_event_alias_for_source(
            &interner,
            &no_event_scope
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

    fn en07_code_data_vmad(include_invalid_topics: bool) -> Vec<u8> {
        fn string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        fn member(bytes: &mut Vec<u8>, name: &str, kind: u8, value: &[u8]) {
            string(bytes, name);
            bytes.extend_from_slice(&[kind, 1]);
            bytes.extend_from_slice(value);
        }
        let mut bytes = vec![6, 0, 2, 0, 2, 0];
        for script in ["EN07_NukeMasterScript", "OtherScript"] {
            string(&mut bytes, script);
            bytes.extend_from_slice(&[0, 1, 0]);
            string(&mut bytes, "CodeData");
            bytes.extend_from_slice(&[17, 1]);
            bytes.extend_from_slice(&6_u32.to_le_bytes());
            for index in 0..6_u32 {
                let topic = index < 3 && (include_invalid_topics || script == "OtherScript");
                bytes.extend_from_slice(&(3 + u32::from(topic)).to_le_bytes());
                member(&mut bytes, "iCodeID", 3, &index.to_le_bytes());
                member(&mut bytes, "bIsInCooldown", 5, &[u8::from(index == 4)]);
                member(&mut bytes, "NukeBlastMarker", 1, &[0, 0, 53, 0, 0x67, 0x0F, 0x2D, 8]);
                if topic {
                    member(&mut bytes, "InvalidCodeTopic", 1, &[0, 0, 0xFF, 0xFF, 2, 0x56, 0x3A, 8]);
                }
            }
        }
        bytes.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0]);
        bytes
    }

    #[test]
    fn pre_translate_en07_keeps_six_code_entries_without_dead_topic_members() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = FO76_EN07_NUKE_MASTER_FORM_ID;
        push_field(&mut record, "VMAD", FieldValue::Bytes(SmallVec::from_vec(en07_code_data_vmad(true))));
        for _ in 0..2 {
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(record.fields[0].value,
                FieldValue::Bytes(SmallVec::from_vec(en07_code_data_vmad(false))));
        }
    }

    #[test]
    fn pre_translate_en07_topic_repair_is_scoped_and_rejects_truncated_data() {
        let interner = StringInterner::new();
        let mut truncated = en07_code_data_vmad(true);
        truncated.truncate(truncated.len() - 15);
        for (id, bytes) in [(0x123, en07_code_data_vmad(true)), (FO76_EN07_NUKE_MASTER_FORM_ID, truncated)] {
            let mut record = make_record("QUST", &interner);
            record.form_key.local = id;
            push_field(&mut record, "VMAD", FieldValue::Bytes(SmallVec::from_vec(bytes.clone())));
            strip_en07_invalid_code_topic_bindings(&interner, &mut record);
            assert_eq!(record.fields[0].value, FieldValue::Bytes(SmallVec::from_vec(bytes)));
        }
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

    /// FO76 names a placed trigger's quest property `MyUniqueQuest` and its
    /// actor filter `PlayerTriggerType`; FO4's stock script reads `MyQuest`
    /// and `PlayerTriggerOnly`, so the unrenamed pair leaves the trigger with
    /// no quest and it never sets its stage.
    fn placed_trigger_vmad(quest_property: &str, filter: Option<(&str, u8, &[u8])>) -> Vec<u8> {
        fn string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        let mut bytes = vec![6, 0, 2, 0, 1, 0];
        string(&mut bytes, "DefaultRefOnTriggerEnter");
        bytes.push(0);
        let property_count: u16 = if filter.is_some() { 3 } else { 2 };
        bytes.extend_from_slice(&property_count.to_le_bytes());
        string(&mut bytes, "StageToSet");
        bytes.extend_from_slice(&[3, 1]);
        bytes.extend_from_slice(&20_i32.to_le_bytes());
        string(&mut bytes, quest_property);
        bytes.extend_from_slice(&[1, 1]);
        bytes.extend_from_slice(&0x0005_A243_u32.to_le_bytes());
        bytes.extend_from_slice(&(-1_i16).to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        if let Some((name, property_type, value)) = filter {
            string(&mut bytes, name);
            bytes.extend_from_slice(&[property_type, 1]);
            bytes.extend_from_slice(value);
        }
        bytes
    }

    #[test]
    fn pre_translate_renames_placed_default_ref_quest_and_trigger_properties() {
        let interner = StringInterner::new();
        let mut record = make_record("REFR", &interner);
        record.form_key.local = 0x0005_A312;
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(placed_trigger_vmad(
                "MyUniqueQuest",
                Some(("PlayerTriggerType", 3, &2_i32.to_le_bytes())),
            ))),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(SmallVec::from_vec(placed_trigger_vmad(
                "MyQuest",
                Some(("PlayerTriggerOnly", 5, &[1])),
            )))
        );
    }

    #[test]
    fn pre_translate_maps_any_actor_trigger_mode_to_no_player_filter() {
        let interner = StringInterner::new();
        let mut record = make_record("REFR", &interner);
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(placed_trigger_vmad(
                "MyUniqueQuest",
                Some(("PlayerTriggerType", 3, &0_i32.to_le_bytes())),
            ))),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(SmallVec::from_vec(placed_trigger_vmad(
                "MyQuest",
                Some(("PlayerTriggerOnly", 5, &[0])),
            )))
        );
    }

    #[test]
    fn pre_translate_leaves_fo4_named_placed_ref_properties_alone() {
        let interner = StringInterner::new();
        let mut record = make_record("REFR", &interner);
        let source = placed_trigger_vmad("MyQuest", Some(("PlayerTriggerOnly", 5, &[1])));
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_slice(&source)),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(SmallVec::from_vec(source))
        );
    }

    #[test]
    fn pre_translate_keeps_placed_properties_on_non_default_ref_scripts() {
        let interner = StringInterner::new();
        let mut record = make_record("REFR", &interner);
        let mut source = placed_trigger_vmad("MyUniqueQuest", None);
        // Same property on a quest-owned script the conversion does not retarget.
        let script_name = b"DefaultRefOnTriggerEnter";
        let replacement = b"Quests:MTR05:CustomTriggerScript";
        let start = source
            .windows(script_name.len())
            .position(|window| window == script_name)
            .expect("script name present");
        source.splice(
            start - 2..start + script_name.len(),
            (replacement.len() as u16)
                .to_le_bytes()
                .iter()
                .copied()
                .chain(replacement.iter().copied()),
        );
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_slice(&source)),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(SmallVec::from_vec(source))
        );
    }

    /// Defend and Retake need the same Reference1 fill as `GQ_WorkshopClear`:
    /// FO4's workshop system sends the workshop ref as Reference1, and the
    /// non-Optional `AttackLocation`/`CenterMarker` aliases fill from it.
    #[test]
    fn workshop_attack_family_keeps_its_workshop_reference1_fill() {
        for (local_id, alias_id, alias_name) in [
            (0x0001_B46A_u32, 23_u32, "WorkshopFromEvent"),
            (0x0001_1CCC, 12, "WorkshopFromEvent"),
            (0x0000_9179, 1, "Workshop"),
        ] {
            let interner = StringInterner::new();
            let mut record = make_record("QUST", &interner);
            record.form_key.local = local_id;
            push_qust_event_alias(
                &mut record,
                alias_id,
                0,
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            );
            let name_entry = record
                .fields
                .iter_mut()
                .find(|entry| entry.sig.0 == *b"ALID")
                .expect("workshop alias name");
            let mut name_bytes = alias_name.as_bytes().to_vec();
            name_bytes.push(0);
            name_entry.value = FieldValue::Bytes(SmallVec::from_vec(name_bytes));

            assert!(!qust_has_untranslatable_event_alias(&record), "{alias_name}");
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert!(
                record.fields.iter().any(|entry| entry.sig.0 == *b"ALFE"
                    && field_value_to_u32(&entry.value) == Some(FO76_QUEST_EVENT_SCPT)),
                "{local_id:06X} kept its event type"
            );
            assert!(
                record.fields.iter().any(|entry| entry.sig.0 == *b"ALFD"
                    && field_value_to_u32(&entry.value) == Some(FO4_QUEST_EVENT_REFERENCE1)),
                "{local_id:06X} kept Reference1"
            );
        }
    }

    /// The allowlist is per quest and per alias: the same fill on a different
    /// alias of the same quest stays unproven, so the quest is still treated as
    /// carrying an untranslatable event fill.
    #[test]
    fn workshop_attack_family_does_not_preserve_other_aliases() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        record.form_key.local = 0x0001_B46A;
        push_qust_event_alias(
            &mut record,
            7,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO4_QUEST_EVENT_REFERENCE1,
        );
        let name_entry = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"ALID")
            .expect("alias name");
        name_entry.value = FieldValue::Bytes(SmallVec::from_slice(b"WorkshopFromEvent "));

        assert!(qust_has_untranslatable_event_alias(&record));
    }
