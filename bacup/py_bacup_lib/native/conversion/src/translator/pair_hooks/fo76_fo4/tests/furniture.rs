

    #[test]
    fn structured_dial_misc_category_is_remapped_at_final_target_boundary() {
        let interner = StringInterner::new();
        let mut record = make_record("DIAL", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (interner.intern("topic_flags"), FieldValue::Uint(0)),
                (
                    interner.intern("category"),
                    FieldValue::Uint(u64::from(FO76_DIAL_CATEGORY_MISCELLANEOUS)),
                ),
                (interner.intern("subtype"), FieldValue::Uint(118)),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("DATA remains");
        let FieldValue::Struct(fields) = &data.value else {
            panic!("expected structured DIAL DATA");
        };
        let category = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("category"))
            .map(|(_, value)| value)
            .expect("category field remains");
        assert_eq!(
            category,
            &FieldValue::Uint(u64::from(FO76_DIAL_CATEGORY_MISCELLANEOUS))
        );

        Fo76Fo4Hook::normalize_dial_data_category(&interner, &mut record);

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("DATA remains");
        let FieldValue::Struct(fields) = &data.value else {
            panic!("expected structured DIAL DATA");
        };
        let category = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("category"))
            .map(|(_, value)| value)
            .expect("category field remains");
        assert_eq!(
            category,
            &FieldValue::Uint(u64::from(FO4_DIAL_CATEGORY_MISCELLANEOUS))
        );
    }

    #[test]
    fn post_translate_clears_all_marker_bits_without_model_or_markers() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        // No Has Model flag and no marker subrecords: nothing backs any
        // interaction point, so all of them clear.
        for record_sig in ["FURN", "TERM"] {
            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "MNAM",
                raw_bytes(&0x0000_001F_u32.to_le_bytes()),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected MNAM bytes");
            };
            assert_eq!(
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                0x0000_0000
            );
        }
    }

    #[test]
    fn post_translate_keeps_furniture_marker_bits_with_target_markers() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&0x4000_0007_u32.to_le_bytes()),
        );
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN * 2]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected MNAM bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x4000_0003
        );
    }

    #[test]
    fn post_translate_adds_terminal_player_path_keyword_once() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let fo4 = interner.intern(FO4_MASTER_NAME);
        let mut record = make_record("TERM", &interner);
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&(FURNITURE_HAS_MODEL_BIT | 1).to_le_bytes()),
        );
        push_field(&mut record, "KSIZ", FieldValue::Uint(1));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::List(vec![FieldValue::FormKey(FormKey {
                local: FO4_POWER_ARMOR_FIRST_PERSON_KEYWORD,
                plugin: fo4,
            })]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ksiz = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "KSIZ")
            .expect("terminal should retain KSIZ");
        assert_eq!(ksiz.value, FieldValue::Uint(2));
        let kwda = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "KWDA")
            .expect("terminal should retain KWDA");
        let FieldValue::List(keywords) = &kwda.value else {
            panic!("KWDA should remain a FormKey list");
        };
        assert_eq!(keywords.len(), 2);
        assert!(fo4_keyword_value(
            &kwda.value,
            &interner,
            FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
        ));
    }

    #[test]
    fn post_translate_adds_terminal_keyword_block_when_missing() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("TERM", &interner);
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&(FURNITURE_HAS_MODEL_BIT | 1).to_le_bytes()),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["MNAM", "KSIZ", "KWDA"]);
        assert_eq!(record.fields[1].value, FieldValue::Uint(1));
        assert!(fo4_keyword_value(
            &record.fields[2].value,
            &interner,
            FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
        ));
    }

    #[test]
    fn post_translate_adds_power_armor_battery_script_and_keeps_markers() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let fo4 = interner.intern(FO4_MASTER_NAME);
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "KWDA",
            FieldValue::List(vec![FieldValue::FormKey(FormKey {
                local: FO4_POWER_ARMOR_FURNITURE_KEYWORD,
                plugin: fo4,
            })]),
        );
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&0x4000_0003_u32.to_le_bytes()),
        );
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN * 2]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let vmad = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "VMAD")
            .expect("power armor furniture should have VMAD");
        let FieldValue::Bytes(bytes) = &vmad.value else {
            panic!("VMAD should be raw bytes");
        };
        let (script_name, properties) = read_power_armor_vmad(bytes);
        assert_eq!(script_name, POWER_ARMOR_BATTERY_INSERT_SCRIPT);
        assert_eq!(
            properties,
            vec![
                (
                    "firstPersonKW".to_string(),
                    FO4_POWER_ARMOR_FIRST_PERSON_KEYWORD
                ),
                (
                    "batteryInsertAnimKW".to_string(),
                    FO4_POWER_ARMOR_BATTERY_INSERT_ANIM_KEYWORD,
                ),
                (
                    "PlayerPathToFurniture".to_string(),
                    FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
                ),
                (
                    "batteryItemKW".to_string(),
                    FO4_POWER_ARMOR_BATTERY_ITEM_KEYWORD
                ),
                (
                    "powerArmorFurnitureKW".to_string(),
                    FO4_POWER_ARMOR_FURNITURE_KEYWORD,
                ),
            ]
        );

        let mnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "MNAM")
            .expect("MNAM");
        let FieldValue::Bytes(bytes) = &mnam.value else {
            panic!("MNAM should be raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x4000_0003
        );
    }

    #[test]
    fn post_translate_recognizes_raw_power_armor_furniture_keywords() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "KWDA",
            raw_bytes(&FO4_POWER_ARMOR_FURNITURE_KEYWORD.to_le_bytes()),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "VMAD")
        );
    }

    #[test]
    fn post_translate_projects_fo76_damage_type_rows_to_fo4_ck_size() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        for record_sig in ["ARMO", "WEAP"] {
            let mut raw = Vec::new();
            raw.extend_from_slice(&0x0102_0304_u32.to_le_bytes());
            raw.extend_from_slice(&11_u32.to_le_bytes());
            raw.extend_from_slice(&0xA0A1_A2A3_u32.to_le_bytes());
            raw.extend_from_slice(&0x0506_0708_u32.to_le_bytes());
            raw.extend_from_slice(&22_u32.to_le_bytes());
            raw.extend_from_slice(&0xB0B1_B2B3_u32.to_le_bytes());

            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "DAMA",
                FieldValue::Bytes(SmallVec::from_vec(raw)),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected DAMA bytes");
            };
            assert_eq!(bytes.len(), 16);
            assert_eq!(&bytes[0..4], &0x0102_0304_u32.to_le_bytes());
            assert_eq!(&bytes[4..8], &11_u32.to_le_bytes());
            assert_eq!(&bytes[8..12], &0x0506_0708_u32.to_le_bytes());
            assert_eq!(&bytes[12..16], &22_u32.to_le_bytes());
        }
    }

    #[test]
    fn term_looping_sound_snam_is_stripped() {
        let interner = StringInterner::new();
        let mut record = make_record("TERM", &interner);
        push_field(
            &mut record,
            "SNAM",
            FieldValue::FormKey(FormKey::parse("800000@SeventySix.esm", &interner).unwrap()),
        );
        push_field(&mut record, "SNAM", FieldValue::None);
        push_field(
            &mut record,
            "SNAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 4])),
        );

        Fo76Fo4Hook::strip_term_looping_sound_snam(&mut record);

        assert!(term_snam_values(&record).is_empty());
    }

    #[test]
    fn term_marker_parameter_snam_rows_are_kept() {
        let interner = StringInterner::new();
        let mut record = make_record("TERM", &interner);
        push_field(
            &mut record,
            "SNAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![
                0_u8;
                FURNITURE_MARKER_PARAMETERS_ROW_LEN
                    * 2
            ])),
        );
        push_field(
            &mut record,
            "SNAM",
            FieldValue::List(vec![FieldValue::Struct(Vec::new())]),
        );

        Fo76Fo4Hook::strip_term_looping_sound_snam(&mut record);

        assert_eq!(term_snam_values(&record).len(), 2);
    }

    #[test]
    fn furn_snam_is_not_touched_by_term_strip() {
        let interner = StringInterner::new();
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "SNAM",
            FieldValue::FormKey(FormKey::parse("000123@SeventySix.esm", &interner).unwrap()),
        );

        Fo76Fo4Hook::strip_term_looping_sound_snam(&mut record);

        assert_eq!(term_snam_values(&record).len(), 1);
    }

    fn fo4_vmad_with_script(script_name: &str) -> Vec<u8> {
        let payload = serde_json::json!({
            "Version": FO4_VMAD_VERSION,
            "Object Format": FO4_VMAD_OBJECT_FORMAT,
            "Scripts": [{ "ScriptName": script_name, "Properties": [] }],
        });
        build_vmad_bytes_from_payload(
            &payload,
            &[FO4_MASTER_NAME.to_string()],
            FO76_MASTER_NAME,
        )
        .expect("fixture VMAD must encode")
    }

    fn workbench_furn(interner: &StringInterner, keyword: u32, vmad: Option<Vec<u8>>) -> Record {
        let mut record = make_record("FURN", interner);
        if let Some(vmad) = vmad {
            push_field(&mut record, "VMAD", raw_bytes(&vmad));
        }
        push_field(
            &mut record,
            "KWDA",
            FieldValue::FormKey(FormKey {
                local: keyword,
                plugin: interner.intern(FO4_MASTER_NAME),
            }),
        );
        record
    }

    fn vmad_script_count(record: &Record) -> u16 {
        let entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "VMAD")
            .expect("VMAD present");
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("expected VMAD bytes");
        };
        u16::from_le_bytes([bytes[4], bytes[5]])
    }

    fn vmad_bytes(record: &Record) -> Vec<u8> {
        let entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "VMAD")
            .expect("VMAD present");
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("expected VMAD bytes");
        };
        bytes.to_vec()
    }

    #[test]
    fn post_translate_appends_workbench_script_beside_carried_fo76_script() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = workbench_furn(
            &interner,
            FO4_WORKBENCH_GENERAL_KEYWORD,
            Some(fo4_vmad_with_script("DefaultPlaySoundScript")),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(vmad_script_count(&record), 2);
        let bytes = vmad_bytes(&record);
        assert!(vmad_contains_name(&bytes, WORKBENCH_SCRIPT));
        assert!(vmad_contains_name(&bytes, "DefaultPlaySoundScript"));
        assert!(vmad_contains_name(&bytes, "WorkshopItemKeyword"));
    }

    #[test]
    fn post_translate_adds_workbench_script_when_record_has_no_vmad() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = workbench_furn(&interner, FO4_WORKBENCH_GENERAL_KEYWORD, None);

        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(vmad_script_count(&record), 1);
        assert!(vmad_contains_name(&vmad_bytes(&record), WORKBENCH_SCRIPT));
    }

    #[test]
    fn post_translate_adds_workbench_script_only_once() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = workbench_furn(
            &interner,
            FO4_WORKBENCH_GENERAL_KEYWORD,
            Some(fo4_vmad_with_script("DefaultPlaySoundScript")),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();
        let first = vmad_bytes(&record);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(vmad_script_count(&record), 2);
        assert_eq!(vmad_bytes(&record), first);
    }

    #[test]
    fn post_translate_leaves_non_workbench_furniture_vmad_alone() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let carried = fo4_vmad_with_script("DefaultPlaySoundScript");
        // A furniture keyword that is not Workbench_General.
        let mut record = workbench_furn(
            &interner,
            FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
            Some(carried.clone()),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(vmad_bytes(&record), carried);
    }
