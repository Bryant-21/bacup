

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
            vec!["ANAM", "INAM", "PTOP", "NTOP", "NPOT", "NNGT", "DTGT"]
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
    fn pre_translate_does_not_enable_xdi_for_three_choices() {
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
        assert!(!record.fields.iter().any(|field| field.sig.0 == *b"KWDA"));
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
    fn dialogue_plan_does_not_split_when_npc_topic_already_has_infos() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        push_field(&mut scene, "ESCE", raw_bytes(&0x620001_u32.to_le_bytes()));
        push_field(&mut scene, "ESCS", raw_bytes(&0x610001_u32.to_le_bytes()));
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
            split_fo76_combined_player_dialogue_info(&mut info, player_form_key, &interner)
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
        assert!(!player.fields.iter().any(|field| field.sig.0 == *b"RNAM"));
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
    fn xdi_plan_pads_under_four_action_in_the_same_scene() {
        let interner = StringInterner::new();
        let mut scene = make_record("SCEN", &interner);
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        for topic in 0x630001_u32..=0x630005 {
            push_field(&mut scene, "ESCS", raw_bytes(&topic.to_le_bytes()));
        }
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        for topic in 0x640001_u32..=0x640003 {
            push_field(&mut scene, "ESCS", raw_bytes(&topic.to_le_bytes()));
        }

        let plan =
            build_xdi_dialogue_plan(&[scene.clone()], &HashMap::new(), &HashSet::new()).unwrap();
        let filler = *plan.scene_player_topic_fillers.get(&0x000800).unwrap();

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut scene).unwrap();
        apply_xdi_scene_player_padding(&mut scene, filler);

        assert_eq!(
            scene
                .fields
                .iter()
                .filter(|field| SCEN_PLAYER_RESPONSE_SIGS.contains(&field.sig.0))
                .count(),
            8
        );
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
