    const RADICAL_RUN_ON_SUBJECT: u32 = 0;

    fn radical_scene_action(
        interner: &StringInterner,
        scene_id: u32,
        phase: &str,
        actor_alias: u32,
        player_topic: Option<u32>,
        npc_topic: u32,
    ) -> Record {
        let source_plugin = interner.intern(FO76_MASTER_NAME);
        let mut scene = Record::new(
            SigCode::from_str("SCEN").unwrap(),
            FormKey {
                local: scene_id,
                plugin: source_plugin,
            },
        );
        push_field(&mut scene, "ANAM", FieldValue::Uint(3));
        push_field(
            &mut scene,
            "NAM0",
            FieldValue::String(interner.intern(phase)),
        );
        push_field(
            &mut scene,
            "ALID",
            FieldValue::Uint(u64::from(actor_alias)),
        );
        if let Some(player_topic) = player_topic {
            push_field(
                &mut scene,
                "ESCS",
                form_key_value(interner, player_topic),
            );
            push_field(&mut scene, "ESCE", form_key_value(interner, npc_topic));
        } else {
            push_field(&mut scene, "DATA", form_key_value(interner, npc_topic));
        }
        scene
    }

    fn raw_condition(hex_value: &str) -> FieldValue {
        raw_bytes(&hex::decode(hex_value).expect("valid W05 Radical CTDA fixture"))
    }

    fn condition_parts(value: &FieldValue) -> (u16, u32, u32) {
        let FieldValue::Bytes(bytes) = value else {
            panic!("expected raw CTDA bytes");
        };
        (
            u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
        )
    }

    #[test]
    fn radical_first_encounter_start_info_is_scoped_to_first_enc_npc() {
        let interner = StringInterner::new();
        let phase = interner.intern("Start");
        let scene = radical_scene_action(
            &interner,
            0x40F6B1,
            "Start",
            8,
            Some(0x40F5D1),
            0x40F5D2,
        );
        let plan = build_xdi_dialogue_plan(&[scene], &HashMap::new(), &HashSet::new()).unwrap();
        assert_eq!(plan.start_scene_actor_aliases.get(&(0x40F6B1, phase)), Some(&8));

        let mut info = make_record("INFO", &interner);
        info.form_key.local = 0x40F659;
        push_field(&mut info, "TSCE", form_key_value(&interner, 0x40F6B1));
        push_field(&mut info, "NAM0", FieldValue::String(phase));

        assert!(scope_start_scene_info_to_actor_alias(&mut info, &plan));
        let gates = info
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .map(|field| condition_parts(&field.value))
            .collect::<Vec<_>>();
        assert_eq!(gates, vec![(566, 8, RADICAL_RUN_ON_SUBJECT)]);
    }

    #[test]
    fn radical_you_crane_greeting_retains_rad_ganger_and_stage_gates() {
        let interner = StringInterner::new();
        let phase = interner.intern("ConversationStart");
        let scene = radical_scene_action(
            &interner,
            0x40F6B2,
            "ConversationStart",
            11,
            None,
            0x40F606,
        );
        let actions = scen_dialogue_actions(&scene);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].actor_alias, Some(11));

        let mut info = make_record("INFO", &interner);
        info.form_key.local = 0x54386A;
        for condition in [
            "000000000000803F3602000000000000000000000100000000000000FFFFFFFF",
            "00000000000000003B0000001F030000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000E8030000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000C6020000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000E9020000000000000000000000000000FFFFFFFF",
            "000000000000803F360200000B000000000000000000000000000000FFFFFFFF",
        ] {
            push_field(&mut info, "CTDA", raw_condition(condition));
        }
        push_field(&mut info, "TSCE", form_key_value(&interner, 0x40F6B2));
        push_field(&mut info, "NAM0", FieldValue::String(phase));

        let mut plan = XdiDialoguePlan::default();
        plan.start_scene_actor_aliases
            .insert((0x40F6B2, phase), 11);
        assert!(!scope_start_scene_info_to_actor_alias(&mut info, &plan));

        let gates = info
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .map(|field| condition_parts(&field.value))
            .collect::<Vec<_>>();
        assert_eq!(
            gates,
            vec![
                (566, 0, 1),
                (59, 799, RADICAL_RUN_ON_SUBJECT),
                (59, 1000, RADICAL_RUN_ON_SUBJECT),
                (59, 710, RADICAL_RUN_ON_SUBJECT),
                (59, 745, RADICAL_RUN_ON_SUBJECT),
                (566, 11, RADICAL_RUN_ON_SUBJECT),
            ]
        );
    }
