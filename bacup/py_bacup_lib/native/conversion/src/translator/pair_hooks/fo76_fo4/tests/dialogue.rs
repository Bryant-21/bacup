

    #[test]
    fn dial_detection_category_is_remapped_only_at_final_target_boundary() {
        let interner = StringInterner::new();
        let mut record = make_record("DIAL", &interner);
        push_field(
            &mut record,
            "DATA",
            raw_bytes(&[0, FO76_DIAL_CATEGORY_DETECTION, 88, 0]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(
            data.value,
            raw_bytes(&[0, FO76_DIAL_CATEGORY_DETECTION, 88, 0])
        );

        Fo76Fo4Hook::normalize_dial_data_category(&interner, &mut record);

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(
            data.value,
            raw_bytes(&[0, FO4_DIAL_CATEGORY_DETECTION, 88, 0])
        );
    }

    #[test]
    fn pre_translate_maps_info_unknown_17_to_say_once() {
        let interner = StringInterner::new();
        let mut record = make_record("INFO", &interner);
        push_field(
            &mut record,
            "ENAM",
            FieldValue::Struct(vec![(
                interner.intern("Union0"),
                FieldValue::Struct(vec![(
                    interner.intern("FlagsFlags"),
                    FieldValue::Uint(u64::from(FO76_INFO_UNKNOWN_17 | 2)),
                )]),
            )]),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let FieldValue::Struct(union) = &record.fields[0].value else {
            panic!("expected ENAM union");
        };
        let FieldValue::Struct(flags) = &union[0].1 else {
            panic!("expected ENAM flags variant");
        };
        assert_eq!(
            flags[0].1,
            FieldValue::Uint(u64::from(FO4_INFO_SAY_ONCE | 2))
        );
    }

    #[test]
    fn pre_translate_maps_raw_info_unknown_17_to_say_once() {
        let interner = StringInterner::new();
        let mut record = make_record("INFO", &interner);
        push_field(
            &mut record,
            "ENAM",
            raw_bytes(&(FO76_INFO_UNKNOWN_17 | 2).to_le_bytes()),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            raw_bytes(&(FO4_INFO_SAY_ONCE | 2).to_le_bytes())
        );
    }

    #[test]
    fn pre_translate_preserves_source_scene_action_flags() {
        const CAMERA_SPEAKER_TARGET: u32 = 1 << 21;
        let interner = StringInterner::new();
        let mut record = make_record("SCEN", &interner);
        push_field(
            &mut record,
            "FNAM",
            raw_bytes(&CAMERA_SPEAKER_TARGET.to_le_bytes()),
        );
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        push_field(
            &mut record,
            "FNAM",
            raw_bytes(&CAMERA_SPEAKER_TARGET.to_le_bytes()),
        );
        push_field(&mut record, "ANAM", FieldValue::Uint(0));
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Uint(u64::from(CAMERA_SPEAKER_TARGET | 0x1000)),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let flags = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"FNAM")
            .map(|field| field.value.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            flags,
            vec![
                raw_bytes(&CAMERA_SPEAKER_TARGET.to_le_bytes()),
                raw_bytes(&CAMERA_SPEAKER_TARGET.to_le_bytes()),
                FieldValue::Uint(u64::from(CAMERA_SPEAKER_TARGET | 0x1000)),
            ]
        );
    }

    #[test]
    fn wayward_blueprint_entry_package_cannot_block_the_scene() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern(FO76_MASTER_NAME);
        let mut scene = Record::new(
            SigCode::from_str("SCEN").unwrap(),
            FormKey {
                local: WAYWARD_THE_PLAN_SCENE,
                plugin: source_plugin,
            },
        );
        push_field(&mut scene, "ANAM", FieldValue::Uint(1));
        push_field(&mut scene, "NAM0", FieldValue::String(interner.intern("")));
        push_field(&mut scene, "ALID", FieldValue::Uint(0));
        push_field(
            &mut scene,
            "INAM",
            FieldValue::Uint(u64::from(WAYWARD_BLUEPRINT_ENTRY_ACTION)),
        );
        push_field(&mut scene, "SNAM", FieldValue::Uint(3));
        push_field(&mut scene, "ENAM", FieldValue::Uint(3));
        push_field(
            &mut scene,
            "PNAM",
            form_key_value(&interner, WAYWARD_BLUEPRINT_PACKAGE),
        );
        push_field(&mut scene, "ANAM", FieldValue::Uint(1));
        push_field(&mut scene, "INAM", FieldValue::Uint(37));
        push_field(
            &mut scene,
            "FNAM",
            FieldValue::Uint(u64::from(SCEN_ACTION_IGNORE_FOR_COMPLETION)),
        );
        push_field(
            &mut scene,
            "PNAM",
            form_key_value(&interner, WAYWARD_BLUEPRINT_PACKAGE),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut scene)
            .unwrap();
        Fo76Fo4Hook::normalize_wayward_blueprint_package_completion(&mut scene);

        let action_flags = scene
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"FNAM")
            .map(|field| field_value_to_u32(&field.value))
            .collect::<Vec<_>>();
        assert_eq!(
            action_flags,
            vec![
                Some(SCEN_ACTION_IGNORE_FOR_COMPLETION),
                Some(SCEN_ACTION_IGNORE_FOR_COMPLETION),
            ]
        );
    }

    #[test]
    fn wayward_completion_fix_does_not_touch_other_package_actions() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        scene.form_key.local = WAYWARD_THE_PLAN_SCENE;
        push_field(&mut scene, "ANAM", FieldValue::Uint(1));
        push_field(
            &mut scene,
            "INAM",
            FieldValue::Uint(u64::from(WAYWARD_BLUEPRINT_ENTRY_ACTION)),
        );
        push_field(&mut scene, "PNAM", form_key_value(&interner, 0x59_1665));

        Fo76Fo4Hook::normalize_wayward_blueprint_package_completion(&mut scene);

        assert!(!scene.fields.iter().any(|field| field.sig.0 == *b"FNAM"));
    }

    #[test]
    fn crash_landing_start_scene_info_is_scoped_to_the_ussa_assaultron_alias() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern(FO76_MASTER_NAME);
        let phase = interner.intern("START--Assaultron");
        let mut scene = Record::new(
            SigCode::from_str("SCEN").unwrap(),
            FormKey {
                local: 0x55F6C6,
                plugin: source_plugin,
            },
        );
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        push_field(&mut scene, "NAM0", FieldValue::String(phase));
        push_field(&mut scene, "ALID", FieldValue::Int(9));
        push_field(&mut scene, "ESCS", form_key_value(&interner, 0x56BBCA));

        let plan = build_xdi_dialogue_plan(&[scene], &HashMap::new(), &HashSet::new()).unwrap();
        assert_eq!(
            plan.start_scene_actor_aliases.get(&(0x55F6C6, phase)),
            Some(&9)
        );

        let mut info = make_record("INFO", &interner);
        push_field(
            &mut info,
            "CTDA",
            raw_bytes(&hex::decode(
                "000000000000803F3602000000000000000000000100000000000000FFFFFFFF",
            )
            .unwrap()),
        );
        push_field(&mut info, "TSCE", form_key_value(&interner, 0x55F6C6));
        push_field(&mut info, "NAM0", FieldValue::String(phase));

        assert!(scope_start_scene_info_to_actor_alias(&mut info, &plan));
        let conditions = info
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .collect::<Vec<_>>();
        assert_eq!(conditions.len(), 2);
        assert_eq!(
            conditions[1].value,
            raw_bytes(&hex::decode(
                "000000000000803F3602000009000000000000000000000000000000FFFFFFFF",
            )
            .unwrap())
        );
        assert!(!scope_start_scene_info_to_actor_alias(&mut info, &plan));
    }

    #[test]
    fn concrete_info_speaker_is_not_replaced_with_a_scene_alias_gate() {
        let interner = StringInterner::new();
        let phase = interner.intern("Intro Hub");
        let mut plan = XdiDialoguePlan::default();
        plan.start_scene_actor_aliases.insert((0x5C7179, phase), 1);

        let mut info = make_record("INFO", &interner);
        push_field(&mut info, "ANAM", form_key_value(&interner, 0x5AD589));
        push_field(&mut info, "TSCE", form_key_value(&interner, 0x5C7179));
        push_field(&mut info, "NAM0", FieldValue::String(phase));

        assert!(!scope_start_scene_info_to_actor_alias(&mut info, &plan));
        assert!(!info.fields.iter().any(|field| field.sig.0 == *b"CTDA"));
    }

    #[test]
    fn pre_translate_maps_scen_escs_choices_to_fo4_player_dialogue_slots() {
        let interner = StringInterner::new();
        let mut record = make_record("SCEN", &interner);
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        push_field(&mut record, "INAM", FieldValue::Uint(56));
        push_field(&mut record, "DTGT", FieldValue::Int(0));
        push_field(&mut record, "ESCE", raw_bytes(&0x56A146_u32.to_le_bytes()));
        push_field(&mut record, "ESCS", raw_bytes(&0x56A145_u32.to_le_bytes()));
        push_field(&mut record, "ESCE", raw_bytes(&0x56A144_u32.to_le_bytes()));
        push_field(&mut record, "ESCS", raw_bytes(&0x56A143_u32.to_le_bytes()));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec![
                "ANAM", "INAM", "PTOP", "NTOP", "NPOT", "NNGT", "DTGT", "KSIZ", "KWDA"
            ]
        );

        let source_plugin = interner.intern(FO76_MASTER_NAME);
        assert_eq!(
            record.fields[2].value,
            FieldValue::FormKey(FormKey {
                local: 0x56A145,
                plugin: source_plugin
            })
        );
        assert_eq!(
            record.fields[3].value,
            FieldValue::FormKey(FormKey {
                local: 0x56A143,
                plugin: source_plugin
            })
        );
        assert_eq!(record.fields[4].value, form_key_value(&interner, 0x56A146));
        assert_eq!(record.fields[5].value, form_key_value(&interner, 0x56A144));
    }

    #[test]
    fn pre_translate_keeps_scen_headtracking_aliases_out_of_formkey_mapping() {
        let interner = StringInterner::new();
        let mut record = make_record("SCEN", &interner);
        push_field(&mut record, "ANAM", FieldValue::Uint(1));
        push_field(&mut record, "DATA", form_key_value(&interner, 0x700001));
        push_field(&mut record, "HTID", form_key_value(&interner, 0x700002));
        push_field(&mut record, "DMAX", FieldValue::Float(10.0));
        push_field(&mut record, "DMIN", FieldValue::Float(1.0));
        push_field(&mut record, "HTID", form_key_value(&interner, 107));

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let htid_values: Vec<_> = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"HTID")
            .map(|field| field.value.clone())
            .collect();
        assert_eq!(htid_values[0], form_key_value(&interner, 0x700002));
        assert_eq!(htid_values[1], raw_bytes(&107_u32.to_le_bytes()));

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                ..MapperOptions::default()
            },
            &interner,
        );
        let source_plugin = interner.intern(FO76_MASTER_NAME);
        let output_plugin = interner.intern("SeventySix.esm");
        mapper.add_mapping(
            FormKey {
                local: 0x700002,
                plugin: source_plugin,
            },
            FormKey {
                local: 0x700002,
                plugin: output_plugin,
            },
        );
        mapper.add_mapping(
            FormKey {
                local: 107,
                plugin: source_plugin,
            },
            FormKey {
                local: 0xB0002E,
                plugin: output_plugin,
            },
        );
        mapper.rewrite_record(&mut record).unwrap();

        let htid_values: Vec<_> = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"HTID")
            .map(|field| field.value.clone())
            .collect();
        assert_eq!(
            htid_values[0],
            FieldValue::FormKey(FormKey {
                local: 0x700002,
                plugin: output_plugin,
            })
        );
        assert_eq!(htid_values[1], raw_bytes(&107_u32.to_le_bytes()));
    }

    #[test]
    fn pre_translate_enables_xdi_for_three_choices() {
        let interner = StringInterner::new();
        let mut record = make_record("SCEN", &interner);
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        for (npc, player) in [
            (0x56A146_u32, 0x56A145_u32),
            (0x56A144_u32, 0x56A143_u32),
            (0x56A142_u32, 0x56A141_u32),
        ] {
            push_field(&mut record, "ESCE", raw_bytes(&npc.to_le_bytes()));
            push_field(&mut record, "ESCS", raw_bytes(&player.to_le_bytes()));
        }

        let info_parent_index = HashMap::from([(0x700001, 0x56A146), (0x700002, 0x56A146)]);
        let plan = build_xdi_dialogue_plan(&[record.clone()], &info_parent_index, &HashSet::new())
            .unwrap();
        assert!(plan.info_parent_overrides.is_empty());

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let player_topics: Vec<_> = record
            .fields
            .iter()
            .filter(|field| SCEN_PLAYER_RESPONSE_SIGS.contains(&field.sig.0))
            .map(|field| field.value.clone())
            .collect();
        let npc_topics: Vec<_> = record
            .fields
            .iter()
            .filter(|field| SCEN_NPC_RESPONSE_SIGS.contains(&field.sig.0))
            .map(|field| field.value.clone())
            .collect();
        assert_eq!(player_topics.len(), 3);
        assert_eq!(npc_topics.len(), 3);
        assert_eq!(npc_topics[0], form_key_value(&interner, 0x56A146));
        assert!(record.fields.iter().any(|field| {
            field.sig.0 == *b"KWDA"
                && field.value
                    == FieldValue::FormKey(FormKey {
                        local: XDI_SCENE_KEYWORD_FORM_ID,
                        plugin: interner.intern(XDI_MASTER_NAME),
                    })
        }));
    }

    #[test]
    fn pre_translate_enables_xdi_without_dropping_fifth_choice_from_plan() {
        let interner = StringInterner::new();
        let mut record = make_record("SCEN", &interner);
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        for index in 0..5_u32 {
            push_field(
                &mut record,
                "ESCE",
                raw_bytes(&(0x600100 + index * 2).to_le_bytes()),
            );
            push_field(
                &mut record,
                "ESCS",
                raw_bytes(&(0x600101 + index * 2).to_le_bytes()),
            );
        }

        let actions = scen_dialogue_actions(&record);
        assert_eq!(actions[0].player_topics.len(), 5);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|field| SCEN_PLAYER_RESPONSE_SIGS.contains(&field.sig.0))
                .count(),
            4
        );
        assert!(record.fields.iter().any(|field| field.sig.0 == *b"KWDA"));
    }

    #[test]
    fn xdi_plan_reparents_multi_info_fifth_topic_and_updates_counts() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        for index in 0..5_u32 {
            push_field(
                &mut scene,
                "ESCE",
                raw_bytes(&(0x620001 + index).to_le_bytes()),
            );
            push_field(
                &mut scene,
                "ESCS",
                raw_bytes(&(0x610001 + index).to_le_bytes()),
            );
        }
        let info_parent_index = HashMap::from([
            (0x710001, 0x610001),
            (0x710004, 0x610004),
            (0x710005, 0x610005),
            (0x710006, 0x610005),
            (0x720004, 0x620004),
            (0x720005, 0x620005),
        ]);

        let plan = build_xdi_dialogue_plan(&[scene], &info_parent_index, &HashSet::new()).unwrap();

        assert_eq!(plan.info_parent_overrides.get(&0x710005), Some(&0x610004));
        assert_eq!(plan.info_parent_overrides.get(&0x710006), Some(&0x610004));
        assert_eq!(plan.dial_info_count_overrides.get(&0x610004), Some(&3));
        assert_eq!(plan.dial_info_count_overrides.get(&0x610005), Some(&0));
        assert_eq!(plan.info_parent_overrides.get(&0x720005), Some(&0x620004));
        assert_eq!(plan.dial_info_count_overrides.get(&0x620004), Some(&2));
        assert_eq!(plan.dial_info_count_overrides.get(&0x620005), Some(&0));
    }

    #[test]
    fn dialogue_plan_splits_combined_player_info_into_empty_npc_topic() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        push_field(&mut scene, "ESCE", raw_bytes(&0x620001_u32.to_le_bytes()));
        push_field(&mut scene, "ESCS", raw_bytes(&0x610001_u32.to_le_bytes()));
        push_field(&mut scene, "ESCS", raw_bytes(&0x6F0001_u32.to_le_bytes()));
        let info_parent_index = HashMap::from([(0x710001, 0x610001)]);

        let plan =
            build_xdi_dialogue_plan(&[scene], &info_parent_index, &HashSet::from([0x710001]))
                .unwrap();

        assert_eq!(
            plan.combined_info_splits.get(&0x710001),
            Some(&PlayerDialogueInfoSplit {
                player_parent: 0x610001,
                npc_parent: 0x620001,
            })
        );
        assert_eq!(plan.info_parent_overrides.get(&0x710001), Some(&0x620001));
        assert_eq!(plan.dial_info_count_overrides.get(&0x620001), Some(&1));
        assert!(!plan.dial_info_count_overrides.contains_key(&0x610001));
    }

    #[test]
    fn dialogue_plan_moves_continuations_without_duplicate_player_choices() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        push_field(&mut scene, "ESCE", raw_bytes(&0x40BD38_u32.to_le_bytes()));
        push_field(&mut scene, "ESCS", raw_bytes(&0x538D70_u32.to_le_bytes()));
        push_field(&mut scene, "ESCS", raw_bytes(&0x6F0001_u32.to_le_bytes()));
        let info_parent_index = HashMap::from([
            (0x538DF5, 0x538D70),
            (0x538DF6, 0x538D70),
            (0x538DF7, 0x538D70),
        ]);

        let plan = build_xdi_dialogue_plan(
            &[scene],
            &info_parent_index,
            &HashSet::from([0x538DF5]),
        )
        .unwrap();

        assert_eq!(plan.combined_info_splits.len(), 1);
        assert!(plan.combined_info_splits.contains_key(&0x538DF5));
        assert!(!plan.combined_info_splits.contains_key(&0x538DF6));
        assert!(!plan.combined_info_splits.contains_key(&0x538DF7));
        for info in [0x538DF5, 0x538DF6, 0x538DF7] {
            assert_eq!(plan.info_parent_overrides.get(&info), Some(&0x40BD38));
        }
        assert_eq!(plan.dial_info_count_overrides.get(&0x538D70), Some(&1));
        assert_eq!(plan.dial_info_count_overrides.get(&0x40BD38), Some(&3));
    }

    #[test]
    fn combined_dialogue_root_excludes_previous_info_continuations() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        push_field(&mut info, "PNAM", raw_bytes(&0_u32.to_le_bytes()));
        push_field(
            &mut info,
            "RNAM",
            FieldValue::String(interner.intern("I'd better get going.")),
        );
        assert!(is_combined_player_dialogue_root(&info, false));

        info.fields[0].value = raw_bytes(&0x538DF5_u32.to_le_bytes());
        assert!(!is_combined_player_dialogue_root(&info, false));
    }

    #[test]
    fn dialogue_plan_does_not_split_when_npc_topic_already_has_infos() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        push_field(&mut scene, "ESCE", raw_bytes(&0x620001_u32.to_le_bytes()));
        push_field(&mut scene, "ESCS", raw_bytes(&0x610001_u32.to_le_bytes()));
        push_field(&mut scene, "ESCS", raw_bytes(&0x6F0001_u32.to_le_bytes()));
        let info_parent_index = HashMap::from([(0x710001, 0x610001), (0x720001, 0x620001)]);

        let plan =
            build_xdi_dialogue_plan(&[scene], &info_parent_index, &HashSet::from([0x710001]))
                .unwrap();

        assert!(plan.combined_info_splits.is_empty());
        assert!(plan.info_parent_overrides.is_empty());
        assert!(plan.dial_info_count_overrides.is_empty());
    }

    #[test]
    fn dialogue_plan_splits_fifth_choice_under_merged_xdi_parents() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        let mut info_parent_index = HashMap::new();
        let mut prompt_info_ids = HashSet::new();
        for index in 0..5_u32 {
            let npc_topic = 0x620001 + index;
            let player_topic = 0x610001 + index;
            let info = 0x710001 + index;
            push_field(&mut scene, "ESCE", raw_bytes(&npc_topic.to_le_bytes()));
            push_field(&mut scene, "ESCS", raw_bytes(&player_topic.to_le_bytes()));
            info_parent_index.insert(info, player_topic);
            prompt_info_ids.insert(info);
        }

        let plan = build_xdi_dialogue_plan(&[scene], &info_parent_index, &prompt_info_ids).unwrap();

        assert_eq!(
            plan.combined_info_splits.get(&0x710005),
            Some(&PlayerDialogueInfoSplit {
                player_parent: 0x610004,
                npc_parent: 0x620004,
            })
        );
        assert_eq!(plan.info_parent_overrides.get(&0x710005), Some(&0x620004));
        assert_eq!(plan.dial_info_count_overrides.get(&0x610004), Some(&2));
        assert_eq!(plan.dial_info_count_overrides.get(&0x610005), Some(&0));
        assert_eq!(plan.dial_info_count_overrides.get(&0x620004), Some(&2));
        assert_eq!(plan.dial_info_count_overrides.get(&0x620005), Some(&0));
    }

    #[test]
    fn combined_dialogue_split_uses_response_text_only_for_player_info() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        let npc_text = FieldValue::String(interner.intern("God damn it. *sigh*"));
        let player_text =
            FieldValue::String(interner.intern("The door's sealed tight. No one's getting in."));
        push_field(&mut info, "ENAM", raw_bytes(&[1, 0, 0, 0]));
        push_field(&mut info, "TRDA", raw_bytes(&[0; 20]));
        push_field(&mut info, "NAM1", npc_text.clone());
        push_field(&mut info, "CTDA", raw_bytes(&[0; 32]));
        push_field(
            &mut info,
            "CIS1",
            FieldValue::String(interner.intern("PlayerRef")),
        );
        push_field(&mut info, "RNAM", player_text.clone());
        push_field(&mut info, "TSCE", form_key_value(&interner, 0x405ED2));
        push_field(&mut info, "INAM", FieldValue::Uint(1));
        let player_form_key = FormKey {
            local: 0xF00001,
            plugin: interner.intern("SeventySix.esm"),
        };

        let player =
            split_fo76_combined_player_dialogue_info(&mut info, player_form_key, None, &interner)
                .unwrap();

        assert_eq!(player.form_key, player_form_key);
        assert_eq!(
            player
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"NAM1")
                .map(|field| &field.value),
            Some(&player_text)
        );
        assert_eq!(
            player
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"RNAM")
                .map(|field| &field.value),
            Some(&player_text)
        );
        assert!(player.fields.iter().any(|field| field.sig.0 == *b"CTDA"));
        assert!(player.fields.iter().any(|field| field.sig.0 == *b"CIS1"));
        assert!(!player.fields.iter().any(|field| field.sig.0 == *b"TSCE"));
        assert_eq!(
            info.fields
                .iter()
                .find(|field| field.sig.0 == *b"NAM1")
                .map(|field| &field.value),
            Some(&npc_text)
        );
        assert!(info.fields.iter().any(|field| field.sig.0 == *b"TSCE"));
        assert!(!info.fields.iter().any(|field| field.sig.0 == *b"RNAM"));
        assert!(!info.fields.iter().any(|field| field.sig.0 == *b"CTDA"));
        assert!(!info.fields.iter().any(|field| field.sig.0 == *b"CIS1"));
    }

    // Storm_MQ01_Breadcrumb: FO76 kept the choice prompt on the parent DIAL's
    // FULL, so the RNAM-only gate skipped the split and left the FO4
    // NPC-response topic with no INFO — the scene stalled on "dialogueentry".
    #[test]
    fn combined_dialogue_root_accepts_prompt_carried_by_the_parent_dial() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        push_field(&mut info, "PNAM", raw_bytes(&0_u32.to_le_bytes()));
        push_field(
            &mut info,
            "NAM1",
            FieldValue::String(interner.intern("Maybe it's all those nukes.")),
        );

        assert!(!is_combined_player_dialogue_root(&info, false));
        assert!(is_combined_player_dialogue_root(&info, true));

        // A continuation is still not a root, even with a DIAL prompt.
        info.fields[0].value = raw_bytes(&0x538DF5_u32.to_le_bytes());
        assert!(!is_combined_player_dialogue_root(&info, true));
    }

    #[test]
    fn combined_dialogue_split_falls_back_to_the_parent_dial_prompt() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        let npc_text = FieldValue::String(interner.intern(
            "Maybe it's all those nukes being dropped on that old mine shaft.",
        ));
        let dial_prompt = FieldValue::String(
            interner.intern("I thought Vault 63 was sealed? Any idea how it's door got here?"),
        );
        push_field(&mut info, "ENAM", raw_bytes(&[1, 0, 0, 0]));
        push_field(&mut info, "TRDA", raw_bytes(&[0; 20]));
        push_field(&mut info, "NAM1", npc_text.clone());
        let player_form_key = FormKey {
            local: 0xF00001,
            plugin: interner.intern("SeventySix.esm"),
        };

        let player = split_fo76_combined_player_dialogue_info(
            &mut info,
            player_form_key,
            Some(&dial_prompt),
            &interner,
        )
        .unwrap();

        // The player topic gets the prompt as both wheel label and spoken line.
        assert_eq!(
            player
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"RNAM")
                .map(|field| &field.value),
            Some(&dial_prompt)
        );
        assert_eq!(
            player
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"NAM1")
                .map(|field| &field.value),
            Some(&dial_prompt)
        );
        // The NPC reply stays on the record bound for the NPC-response topic.
        assert_eq!(
            info.fields
                .iter()
                .find(|field| field.sig.0 == *b"NAM1")
                .map(|field| &field.value),
            Some(&npc_text)
        );
    }

    #[test]
    fn combined_dialogue_split_prefers_rnam_over_the_dial_prompt() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        let rnam = FieldValue::String(interner.intern("Appreciate the heads up."));
        let dial_prompt = FieldValue::String(interner.intern("stale DIAL name"));
        push_field(&mut info, "NAM1", FieldValue::String(interner.intern("Sure.")));
        push_field(&mut info, "RNAM", rnam.clone());
        let player_form_key = FormKey {
            local: 0xF00002,
            plugin: interner.intern("SeventySix.esm"),
        };

        let player = split_fo76_combined_player_dialogue_info(
            &mut info,
            player_form_key,
            Some(&dial_prompt),
            &interner,
        )
        .unwrap();

        assert_eq!(
            player
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"RNAM")
                .map(|field| &field.value),
            Some(&rnam)
        );
    }

    #[test]
    fn combined_dialogue_split_errors_without_any_prompt_source() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        push_field(&mut info, "NAM1", FieldValue::String(interner.intern("Hi.")));
        let player_form_key = FormKey {
            local: 0xF00003,
            plugin: interner.intern("SeventySix.esm"),
        };

        assert!(
            split_fo76_combined_player_dialogue_info(&mut info, player_form_key, None, &interner)
                .is_err()
        );
    }

    #[test]
    fn combined_dialogue_split_moves_info_group_to_player_info() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        let info_group = form_key_value(&interner, 0x595806);
        let previous_info = form_key_value(&interner, 0x59FA55);
        push_field(&mut info, "PNAM", previous_info.clone());
        push_field(&mut info, "GNAM", info_group.clone());
        push_field(
            &mut info,
            "RNAM",
            FieldValue::String(interner.intern("Player prompt")),
        );
        let player_form_key = FormKey {
            local: 0xF00001,
            plugin: interner.intern("SeventySix.esm"),
        };

        let player =
            split_fo76_combined_player_dialogue_info(&mut info, player_form_key, None, &interner)
                .unwrap();

        assert_eq!(
            player
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"GNAM")
                .map(|field| &field.value),
            Some(&info_group)
        );
        assert!(!player.fields.iter().any(|field| field.sig.0 == *b"PNAM"));
        assert!(!info.fields.iter().any(|field| field.sig.0 == *b"GNAM"));
        assert_eq!(
            info.fields
                .iter()
                .find(|field| field.sig.0 == *b"PNAM")
                .map(|field| &field.value),
            Some(&previous_info)
        );
    }

    #[test]
    fn combined_dialogue_split_swaps_raw_condition_subject_and_target() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        push_field(
            &mut info,
            "RNAM",
            FieldValue::String(interner.intern("Have you seen the Overseer?")),
        );
        let source_condition =
            hex::decode("800000000000803F0E000000C6624E00000000000100000000000000FFFFFFFF")
                .unwrap();
        push_field(&mut info, "CTDA", raw_bytes(&source_condition));
        push_field(
            &mut info,
            "CIS1",
            FieldValue::String(interner.intern("condition parameter")),
        );
        let player_form_key = FormKey {
            local: 0xF00001,
            plugin: interner.intern("SeventySix.esm"),
        };

        let player =
            split_fo76_combined_player_dialogue_info(&mut info, player_form_key, None, &interner)
                .unwrap();

        let condition = player
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDA")
            .expect("condition moved to player INFO");
        assert_eq!(
            Fo76Fo4Hook::condition_run_on(&interner, &condition.value),
            Some(0)
        );
        let condition_index = player
            .fields
            .iter()
            .position(|field| field.sig.0 == *b"CTDA")
            .unwrap();
        assert_eq!(player.fields[condition_index + 1].sig.0, *b"CIS1");
        assert!(!info
            .fields
            .iter()
            .any(|field| matches!(&field.sig.0, b"CTDA" | b"CIS1")));
    }

    #[test]
    fn combined_dialogue_split_swaps_structured_condition_subject_and_target() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        push_field(
            &mut info,
            "RNAM",
            FieldValue::String(interner.intern("Player prompt")),
        );
        push_field(
            &mut info,
            "CTDA",
            FieldValue::Struct(vec![
                (interner.intern("Function"), FieldValue::Uint(14)),
                (interner.intern("Run On"), FieldValue::Uint(0)),
            ]),
        );
        let player_form_key = FormKey {
            local: 0xF00001,
            plugin: interner.intern("SeventySix.esm"),
        };

        let player =
            split_fo76_combined_player_dialogue_info(&mut info, player_form_key, None, &interner)
                .unwrap();

        let condition = player
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDA")
            .expect("condition moved to player INFO");
        assert_eq!(
            Fo76Fo4Hook::condition_run_on(&interner, &condition.value),
            Some(1)
        );
    }

    #[test]
    fn combined_dialogue_split_keeps_wayward_stage_gate_on_subject() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        push_field(
            &mut info,
            "RNAM",
            FieldValue::String(interner.intern("I'd better get going.")),
        );
        let stage_condition =
            hex::decode("00000000000000003B00000071020000000000000000000000000000FFFFFFFF")
                .unwrap();
        push_field(&mut info, "CTDA", raw_bytes(&stage_condition));
        let player_form_key = FormKey {
            local: 0xB00364,
            plugin: interner.intern("SeventySix.esm"),
        };

        let player =
            split_fo76_combined_player_dialogue_info(&mut info, player_form_key, None, &interner)
                .unwrap();
        let condition = player
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDA")
            .expect("stage gate copied to player INFO");

        assert_eq!(
            Fo76Fo4Hook::condition_run_on(&interner, &condition.value),
            Some(0)
        );
        let FieldValue::Bytes(bytes) = &condition.value else {
            panic!("Wayward stage gate must stay raw bytes");
        };
        assert_eq!(bytes.as_slice(), stage_condition.as_slice());
    }

    #[test]
    fn combined_dialogue_split_targets_special_checks_at_player_subject() {
        let interner = StringInterner::new();
        let mut info = make_record("INFO", &interner);
        push_field(
            &mut info,
            "RNAM",
            FieldValue::String(interner.intern("[Luck 2+] Player prompt")),
        );
        let luck_condition =
            hex::decode("64000000972C57080E000000C802000000000000050000000000000006000000")
                .unwrap();
        let custom_actor_value_condition =
            hex::decode("64000000972C57080E000000AF5D540800000000050000000000000006000000")
                .unwrap();
        push_field(&mut info, "CTDA", raw_bytes(&luck_condition));
        push_field(
            &mut info,
            "CTDA",
            raw_bytes(&custom_actor_value_condition),
        );
        let player_form_key = FormKey {
            local: 0xF00001,
            plugin: interner.intern("SeventySix.esm"),
        };

        let player =
            split_fo76_combined_player_dialogue_info(&mut info, player_form_key, None, &interner)
                .unwrap();
        let conditions = player
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .collect::<Vec<_>>();

        assert_eq!(
            Fo76Fo4Hook::condition_run_on(&interner, &conditions[0].value),
            Some(0)
        );
        let FieldValue::Bytes(luck) = &conditions[0].value else {
            panic!("Luck condition must stay raw bytes");
        };
        assert_eq!(Fo76Fo4Hook::raw_condition_parameter_3(luck), Some(u32::MAX));
        assert_eq!(
            Fo76Fo4Hook::condition_run_on(&interner, &conditions[1].value),
            Some(5)
        );
    }

    #[test]
    fn player_info_tree_links_follow_generated_player_header() {
        let interner = StringInterner::new();
        let source_header = FormKey {
            local: 0x42F513,
            plugin: interner.intern(FO76_MASTER_NAME),
        };
        let npc_header = FormKey {
            local: 0x42F513,
            plugin: interner.intern("SeventySix.esm"),
        };
        let player_header = FormKey {
            local: 0xB00123,
            plugin: interner.intern("SeventySix.esm"),
        };
        let mut source_info = make_record("INFO", &interner);
        push_field(&mut source_info, "PNAM", FieldValue::FormKey(source_header));
        push_field(&mut source_info, "GNAM", FieldValue::FormKey(source_header));
        let source_links = info_tree_links(&source_info);

        let mut translated_info = source_info;
        for field in &mut translated_info.fields {
            field.value = FieldValue::FormKey(npc_header);
        }
        let mut plan = XdiDialoguePlan::default();
        plan.combined_info_splits.insert(
            source_header.local,
            PlayerDialogueInfoSplit {
                player_parent: 0x42F3BE,
                npc_parent: 0x42F3BF,
            },
        );
        let generated = HashMap::from([(source_header, player_header)]);

        retarget_player_info_tree_links(
            &mut translated_info,
            &source_links,
            0x42F3BE,
            &plan,
            &generated,
        )
        .unwrap();

        assert!(
            translated_info
                .fields
                .iter()
                .all(|field| field.value == FieldValue::FormKey(player_header))
        );
    }

    #[test]
    fn xdi_plan_rejects_a_shared_merge_topic() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        for topic in 0x620001_u32..=0x620005 {
            push_field(&mut scene, "ESCS", raw_bytes(&topic.to_le_bytes()));
        }
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        push_field(&mut scene, "ESCS", raw_bytes(&0x620005_u32.to_le_bytes()));
        push_field(&mut scene, "ESCE", raw_bytes(&0x620100_u32.to_le_bytes()));

        let error =
            build_xdi_dialogue_plan(&[scene], &HashMap::new(), &HashSet::new()).unwrap_err();

        assert!(error.contains("620005"));
        assert!(error.contains("shared"));
    }

    #[test]
    fn xdi_plan_merges_a_reused_player_and_npc_tail_once() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        for index in 0..5_u32 {
            push_field(
                &mut scene,
                "ESCE",
                raw_bytes(&(0x650101 + index).to_le_bytes()),
            );
            push_field(
                &mut scene,
                "ESCS",
                raw_bytes(&(0x650001 + index).to_le_bytes()),
            );
        }
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        for index in 0..5_u32 {
            push_field(
                &mut scene,
                "ESCE",
                raw_bytes(&(0x650001 + index).to_le_bytes()),
            );
            push_field(
                &mut scene,
                "ESCS",
                raw_bytes(&(0x650201 + index).to_le_bytes()),
            );
        }
        let info_parent_index = HashMap::from([(0x750001, 0x650004), (0x750002, 0x650005)]);

        let plan = build_xdi_dialogue_plan(&[scene], &info_parent_index, &HashSet::new()).unwrap();

        assert_eq!(plan.info_parent_overrides.get(&0x750002), Some(&0x650004));
        assert_eq!(plan.dial_info_count_overrides.get(&0x650004), Some(&2));
        assert_eq!(plan.dial_info_count_overrides.get(&0x650005), Some(&0));
    }

    #[test]
    fn pre_translate_enables_xdi_for_vanilla_sized_player_dialogue() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        for topic in 0x630001_u32..=0x630004 {
            push_field(&mut scene, "ESCS", raw_bytes(&topic.to_le_bytes()));
        }

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut scene)
            .unwrap();

        assert!(scene.fields.iter().any(|field| {
            field.sig.0 == *b"KWDA"
                && field.value
                    == FieldValue::FormKey(FormKey {
                        local: XDI_SCENE_KEYWORD_FORM_ID,
                        plugin: interner.intern(XDI_MASTER_NAME),
                    })
        }));
    }

    #[test]
    fn pre_translate_resets_scen_choice_slots_for_each_action() {
        let interner = StringInterner::new();
        let mut record = make_record("SCEN", &interner);
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        push_field(&mut record, "ESCE", raw_bytes(&0x56A146_u32.to_le_bytes()));
        push_field(&mut record, "ESCS", raw_bytes(&0x56A145_u32.to_le_bytes()));
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        push_field(&mut record, "ESCE", raw_bytes(&0x58F9DC_u32.to_le_bytes()));
        push_field(&mut record, "ESCS", raw_bytes(&0x58F9DB_u32.to_le_bytes()));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let player_topics: Vec<_> = record
            .fields
            .iter()
            .filter(|field| SCEN_PLAYER_RESPONSE_SIGS.contains(&field.sig.0))
            .map(|field| field.value.clone())
            .collect();
        assert_eq!(player_topics.len(), 2);
        assert_eq!(player_topics[0], form_key_value(&interner, 0x56A145));
        assert_eq!(player_topics[1], form_key_value(&interner, 0x58F9DB));
    }

    #[test]
    fn pre_translate_drops_wrld_runtime_tables() {
        let interner = StringInterner::new();
        let mut record = make_record("WRLD", &interner);
        for sig in ["EDID", "RNAM", "MHDT", "OFST", "CLSZ", "NAM0"] {
            push_field(&mut record, sig, raw_bytes(&[0, 1, 2, 3]));
        }

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<_> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["EDID", "NAM0"]);
    }
