

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
    fn post_translate_keeps_model_marker_in_addition_to_explicit_markers() {
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
            0x4000_0007
        );
    }

    #[test]
    fn post_translate_keeps_sparse_wayward_blueprint_marker() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&(FURNITURE_HAS_MODEL_BIT | 1 | (1 << 20)).to_le_bytes()),
        );
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected MNAM bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            FURNITURE_HAS_MODEL_BIT | 1 | (1 << 20)
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
    fn post_translate_and_normalize_project_fo76_damage_type_rows_once() {
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
            raw.extend_from_slice(&0x090A_0B0C_u32.to_le_bytes());
            raw.extend_from_slice(&33_u32.to_le_bytes());
            raw.extend_from_slice(&0xC0C1_C2C3_u32.to_le_bytes());

            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "DAMA",
                FieldValue::Bytes(SmallVec::from_vec(raw)),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let target_schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
            let source_schema = crate::schema::AuthoringSchema::for_game("fo76").unwrap();
            let normalizer = crate::target_normalize::TargetRecordNormalizer {
                target_schema: &target_schema,
                source_record_def: source_schema.record_def(record_sig),
                interner: Some(&interner),
            };
            let crate::target_normalize::TargetRecordNormalization::Keep(record) =
                normalizer.normalize(record)
            else {
                panic!("damage record must survive");
            };

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected DAMA bytes");
            };
            assert_eq!(bytes.len(), 24);
            assert_eq!(&bytes[0..4], &0x0102_0304_u32.to_le_bytes());
            assert_eq!(&bytes[4..8], &11_u32.to_le_bytes());
            assert_eq!(&bytes[8..12], &0x0506_0708_u32.to_le_bytes());
            assert_eq!(&bytes[12..16], &22_u32.to_le_bytes());
            assert_eq!(&bytes[16..20], &0x090A_0B0C_u32.to_le_bytes());
            assert_eq!(&bytes[20..24], &33_u32.to_le_bytes());
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

    fn fo76_fo4_output_masters() -> Vec<String> {
        [
            "Fallout4.esm",
            "DLCRobot.esm",
            "DLCworkshop01.esm",
            "DLCCoast.esm",
            "DLCworkshop02.esm",
            "DLCworkshop03.esm",
            "DLCNukaWorld.esm",
            "XDI.esm",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }

    fn terminal_story_vmad(
        self_reference: u32,
        expected_quest: Option<u32>,
        unique_quest: Option<u32>,
        include_story_script: bool,
    ) -> Vec<u8> {
        let masters = fo76_fo4_output_masters();
        let own_index = masters.len() as u32;
        let mut scripts = vec![serde_json::json!({
            "ScriptName": "PreservedTerminalScript",
            "Properties": [{
                "propertyName": "PreservedText",
                "Type": "String",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": "unchanged",
            }],
        })];
        if include_story_script {
            let mut intended_row = vec![
                serde_json::json!({
                    "memberName": "iMenuItemTarget",
                    "Type": "Int32",
                    "Flags": VMAD_PROPERTY_FLAG_EDITED,
                    "Value": EN05_TERMINAL_MENU_ITEM_TARGET,
                }),
                raw_vmad_object_member(
                    "ActiveQuestKeyword",
                    (own_index << 24) | self_reference,
                ),
                serde_json::json!({
                    "memberName": "PreservedMember",
                    "Type": "String",
                    "Flags": VMAD_PROPERTY_FLAG_EDITED,
                    "Value": "keep",
                }),
            ];
            if let Some(expected_quest) = expected_quest {
                intended_row.push(raw_vmad_object_member(
                    EXPECTED_QUEST_TO_START_MEMBER,
                    (own_index << 24) | expected_quest,
                ));
            }
            if let Some(unique_quest) = unique_quest {
                intended_row.push(raw_vmad_object_member(
                    UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER,
                    (own_index << 24) | unique_quest,
                ));
            }
            let other_row = vec![
                serde_json::json!({
                    "memberName": "iMenuItemTarget",
                    "Type": "Int32",
                    "Flags": VMAD_PROPERTY_FLAG_EDITED,
                    "Value": 2,
                }),
                raw_vmad_object_member(
                    "OtherRowReference",
                    (own_index << 24) | self_reference,
                ),
            ];
            let properties = vec![serde_json::json!({
                "propertyName": "MenuData",
                "Type": "Array of Struct",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": [intended_row, other_row],
            }), serde_json::json!({
                "propertyName": "ExistingStage",
                "Type": "Int32",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": 7,
            })];
            scripts.push(serde_json::json!({
                "ScriptName": DEFAULT_SEND_STORY_EVENT_ON_MENU_ITEM_RUN_SCRIPT,
                "Properties": properties,
            }));
        }
        build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": FO4_VMAD_VERSION,
                "Object Format": FO4_VMAD_OBJECT_FORMAT,
                "Scripts": scripts,
            }),
            &masters,
            FO76_MASTER_NAME,
        )
        .expect("terminal fixture VMAD must encode")
    }

    fn mtr06_physical_exam_terminal_vmad(
        include_physical_script: bool,
        item_one_story_events: &[u32],
    ) -> Vec<u8> {
        let masters = fo76_fo4_output_masters();
        let own_index = masters.len() as u32;
        let mut scripts = vec![serde_json::json!({
            "ScriptName": "PreservedTerminalScript",
            "Properties": [{
                "propertyName": "PreservedText",
                "Type": "String",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": "unchanged",
            }],
        })];
        if include_physical_script {
            scripts.push(serde_json::json!({
                "ScriptName": MTR06_PHYSICAL_EXAM_TERMINAL_SCRIPT,
                "Properties": [{
                    "propertyName": "PreservedPhysicalProperty",
                    "Type": "Int32",
                    "Flags": VMAD_PROPERTY_FLAG_EDITED,
                    "Value": 42,
                }],
            }));
        }
        let mut rows = item_one_story_events
            .iter()
            .map(|story_event| {
                serde_json::json!([
                    {
                        "memberName": "iMenuItemTarget",
                        "Type": "Int32",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": MTR06_PHYSICAL_EXAM_MENU_ITEM_TARGET,
                    },
                    raw_vmad_object_member(
                        "StoryEventToSend",
                        (own_index << 24) | story_event,
                    ),
                    {
                        "memberName": "PreservedMember",
                        "Type": "String",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": "remove-with-item-one",
                    },
                ])
            })
            .collect::<Vec<_>>();
        rows.push(serde_json::json!([
            {
                "memberName": "iMenuItemTarget",
                "Type": "Int32",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": 2,
            },
            {
                "memberName": "PreservedOtherRow",
                "Type": "String",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": "keep",
            },
        ]));
        scripts.push(serde_json::json!({
            "ScriptName": DEFAULT_SEND_STORY_EVENT_ON_MENU_ITEM_RUN_SCRIPT,
            "Properties": [
                {
                    "propertyName": "MenuData",
                    "Type": "Array of Struct",
                    "Flags": VMAD_PROPERTY_FLAG_EDITED,
                    "Value": rows,
                },
                {
                    "propertyName": "PreservedStage",
                    "Type": "Int32",
                    "Flags": VMAD_PROPERTY_FLAG_EDITED,
                    "Value": 7,
                },
            ],
        }));
        build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": FO4_VMAD_VERSION,
                "Object Format": FO4_VMAD_OBJECT_FORMAT,
                "Scripts": scripts,
            }),
            &masters,
            FO76_MASTER_NAME,
        )
        .expect("MTR06 terminal fixture VMAD must encode")
    }

    fn en05_terminal(
        interner: &StringInterner,
        signature: &str,
        form_id: u32,
        editor_id: &str,
        vmad: FieldValue,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey {
                local: form_id,
                plugin: interner.intern(FO76_MASTER_NAME),
            },
        );
        let editor_id = interner.intern(editor_id);
        record.eid = Some(editor_id);
        push_field(&mut record, "EDID", FieldValue::String(editor_id));
        push_field(&mut record, "VMAD", vmad);
        record
    }

    fn decoded_terminal_vmad(record: &Record) -> serde_json::Value {
        esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
            &vmad_bytes(record),
            &fo76_fo4_output_masters(),
            FO76_MASTER_NAME,
            Some("TERM"),
        )
        .expect("terminal VMAD must decode")
    }

    #[test]
    fn mtr06_terminal_suppresses_only_the_duplicate_generic_item_one_row() {
        let interner = StringInterner::new();
        let mut record = en05_terminal(
            &interner,
            "TERM",
            MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
            MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
            raw_bytes(&mtr06_physical_exam_terminal_vmad(
                true,
                &[MTR06_PHYSICAL_EXAM_STORY_EVENT_FORM_ID],
            )),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let payload = decoded_terminal_vmad(&record);
        let scripts = payload["Scripts"].as_array().unwrap();
        assert_eq!(scripts.len(), 3);
        assert_eq!(scripts[0]["ScriptName"], "PreservedTerminalScript");
        assert_eq!(scripts[0]["Properties"][0]["Value"], "unchanged");
        assert_eq!(
            scripts[1]["ScriptName"],
            MTR06_PHYSICAL_EXAM_TERMINAL_SCRIPT
        );
        assert_eq!(scripts[1]["Properties"][0]["Value"], 42);
        let generic = scripts
            .iter()
            .find(|script| {
                script["ScriptName"].as_str()
                    == Some(DEFAULT_SEND_STORY_EVENT_ON_MENU_ITEM_RUN_SCRIPT)
            })
            .unwrap();
        let properties = generic["Properties"].as_array().unwrap();
        assert_eq!(properties.len(), 2);
        assert_eq!(properties[1]["propertyName"], "PreservedStage");
        assert_eq!(properties[1]["Value"], 7);
        let rows = properties[0]["Value"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(vmad_struct_entry_i32(&rows[0], "iMenuItemTarget"), Some(2));
        assert_eq!(rows[0][1]["memberName"], "PreservedOtherRow");
        assert_eq!(rows[0][1]["Value"], "keep");
    }

    #[test]
    fn mtr06_terminal_generic_item_one_suppression_is_idempotent() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = en05_terminal(
            &interner,
            "TERM",
            MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
            MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
            raw_bytes(&mtr06_physical_exam_terminal_vmad(
                true,
                &[MTR06_PHYSICAL_EXAM_STORY_EVENT_FORM_ID],
            )),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();
        let once = vmad_bytes(&record);
        hook.post_translate(&mut ctx, &mut record).unwrap();
        assert_eq!(vmad_bytes(&record), once);
    }

    #[test]
    fn mtr06_terminal_suppression_rejects_non_exact_bindings() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let exact_vmad = mtr06_physical_exam_terminal_vmad(
            true,
            &[MTR06_PHYSICAL_EXAM_STORY_EVENT_FORM_ID],
        );
        let missing_physical = mtr06_physical_exam_terminal_vmad(
            false,
            &[MTR06_PHYSICAL_EXAM_STORY_EVENT_FORM_ID],
        );
        let wrong_event = mtr06_physical_exam_terminal_vmad(true, &[0x000800]);
        let duplicate_item_one = mtr06_physical_exam_terminal_vmad(
            true,
            &[
                MTR06_PHYSICAL_EXAM_STORY_EVENT_FORM_ID,
                MTR06_PHYSICAL_EXAM_STORY_EVENT_FORM_ID,
            ],
        );
        let no_item_one = mtr06_physical_exam_terminal_vmad(true, &[]);
        let cases = [
            (
                "FURN",
                MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
                MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
                raw_bytes(&exact_vmad),
            ),
            (
                "TERM",
                MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID + 1,
                MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
                raw_bytes(&exact_vmad),
            ),
            (
                "TERM",
                MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
                "MTR06_PhysicalExamTerminal_Wrong",
                raw_bytes(&exact_vmad),
            ),
            (
                "TERM",
                0x3B3302,
                "RSVP00_Terminal_Kiosk_Main",
                raw_bytes(&exact_vmad),
            ),
            (
                "TERM",
                0x1820CF,
                "EN05_MarksmanshipCourseTerminal",
                raw_bytes(&exact_vmad),
            ),
            (
                "TERM",
                0x000800,
                "UnrelatedTerminal",
                raw_bytes(&exact_vmad),
            ),
            (
                "TERM",
                MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
                MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
                raw_bytes(&missing_physical),
            ),
            (
                "TERM",
                MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
                MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
                raw_bytes(&wrong_event),
            ),
            (
                "TERM",
                MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
                MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
                raw_bytes(&duplicate_item_one),
            ),
            (
                "TERM",
                MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
                MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
                raw_bytes(&no_item_one),
            ),
        ];

        for (signature, form_id, editor_id, vmad) in cases {
            let mut record = en05_terminal(&interner, signature, form_id, editor_id, vmad);
            let before = vmad_bytes(&record);
            hook.post_translate(&mut ctx, &mut record).unwrap();
            assert_eq!(
                vmad_bytes(&record),
                before,
                "{signature} {form_id:06X} {editor_id}"
            );
        }

        let mut nw_duplicate = en05_terminal(
            &interner,
            "TERM",
            MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID,
            MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID,
            raw_bytes(&exact_vmad),
        );
        nw_duplicate.form_key.plugin = interner.intern("NW.esm");
        let before = vmad_bytes(&nw_duplicate);
        hook.post_translate(&mut ctx, &mut nw_duplicate).unwrap();
        assert_eq!(vmad_bytes(&nw_duplicate), before);
    }

    fn quest_reference<'a>(
        payload: &'a serde_json::Value,
        member_name: &str,
    ) -> (&'a str, &'a str) {
        let properties = payload["Scripts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|script| {
                script["ScriptName"].as_str()
                    == Some(DEFAULT_SEND_STORY_EVENT_ON_MENU_ITEM_RUN_SCRIPT)
            })
            .unwrap()["Properties"]
            .as_array()
            .unwrap();
        let menu_data = properties
            .iter()
            .find(|property| {
                property["propertyName"].as_str() == Some("MenuData")
            })
            .expect("MenuData property");
        let intended_row = menu_data["Value"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                vmad_struct_entry_i32(row, "iMenuItemTarget")
                    == Some(EN05_TERMINAL_MENU_ITEM_TARGET)
            })
            .expect("intended menu row")
            .as_array()
            .unwrap();
        let quest = intended_row
            .iter()
            .find(|member| member["memberName"].as_str() == Some(member_name))
            .unwrap_or_else(|| panic!("{member_name} member"));
        (
            quest["Value"]["FormID"]["reference"]["plugin"]
                .as_str()
                .unwrap(),
            quest["Value"]["FormID"]["reference"]["object_id"]
                .as_str()
                .unwrap(),
        )
    }

    #[test]
    fn post_translate_backfills_exact_en05_terminal_direct_start_opt_in() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        for (terminal_form_id, editor_id, expected_quest, self_reference) in
            EN05_TERMINAL_STORY_QUEST_BINDINGS
        {
            let mut record = en05_terminal(
                &interner,
                "TERM",
                *terminal_form_id,
                editor_id,
                raw_bytes(&terminal_story_vmad(*self_reference, None, None, true)),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let payload = decoded_terminal_vmad(&record);
            let expected_quest = format!("{expected_quest:06X}");
            for member_name in [
                EXPECTED_QUEST_TO_START_MEMBER,
                UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER,
            ] {
                assert_eq!(
                    quest_reference(&payload, member_name),
                    (FO76_MASTER_NAME, expected_quest.as_str())
                );
            }
        }
    }

    #[test]
    fn en05_terminal_vmad_backfill_preserves_existing_scripts_and_properties() {
        let interner = StringInterner::new();
        let mut record = en05_terminal(
            &interner,
            "TERM",
            0x1820CF,
            "EN05_MarksmanshipCourseTerminal",
            raw_bytes(&terminal_story_vmad(0x1820D3, None, None, true)),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let payload = decoded_terminal_vmad(&record);
        assert_eq!(payload["Scripts"].as_array().unwrap().len(), 2);
        assert_eq!(payload["Scripts"][0]["ScriptName"], "PreservedTerminalScript");
        assert_eq!(
            payload["Scripts"][0]["Properties"][0]["propertyName"],
            "PreservedText"
        );
        assert_eq!(
            payload["Scripts"][0]["Properties"][0]["Value"],
            "unchanged"
        );
        assert_eq!(
            payload["Scripts"][1]["Properties"][0]["propertyName"],
            "MenuData"
        );
        assert_eq!(
            payload["Scripts"][1]["Properties"][1]["propertyName"],
            "ExistingStage"
        );
        assert_eq!(payload["Scripts"][1]["Properties"][1]["Value"], 7);
        assert_eq!(payload["Scripts"][1]["Properties"].as_array().unwrap().len(), 2);
        assert!(payload["Scripts"][1]["Properties"]
            .as_array()
            .unwrap()
            .iter()
            .all(|property| ![
                EXPECTED_QUEST_TO_START_MEMBER,
                UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER,
            ]
            .contains(&property["propertyName"].as_str().unwrap_or_default())));
        let rows = payload["Scripts"][1]["Properties"][0]["Value"]
            .as_array()
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].as_array().unwrap().len(), 5);
        assert_eq!(rows[0][2]["memberName"], "PreservedMember");
        assert_eq!(rows[0][2]["Value"], "keep");
        assert_eq!(rows[1].as_array().unwrap().len(), 2);
        assert!(rows[1]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| ![
                EXPECTED_QUEST_TO_START_MEMBER,
                UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER,
            ]
            .contains(&member["memberName"].as_str().unwrap_or_default())));
    }

    #[test]
    fn en05_terminal_vmad_backfill_is_idempotent_and_keeps_existing_binding() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = en05_terminal(
            &interner,
            "TERM",
            0x1820DD,
            "EN05_ObstacleCourseTerminal",
            raw_bytes(&terminal_story_vmad(0x18211C, None, None, true)),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();
        let once = vmad_bytes(&record);
        hook.post_translate(&mut ctx, &mut record).unwrap();
        assert_eq!(vmad_bytes(&record), once);

        let existing = terminal_story_vmad(0x18211C, Some(0x09C824), None, true);
        let mut already_bound = en05_terminal(
            &interner,
            "TERM",
            0x1820DD,
            "EN05_ObstacleCourseTerminal",
            raw_bytes(&existing),
        );
        hook.post_translate(&mut ctx, &mut already_bound).unwrap();
        let completed = vmad_bytes(&already_bound);
        assert_ne!(completed, existing);
        let payload = decoded_terminal_vmad(&already_bound);
        assert_eq!(
            quest_reference(&payload, UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER),
            (FO76_MASTER_NAME, "09C824")
        );
        hook.post_translate(&mut ctx, &mut already_bound).unwrap();
        assert_eq!(vmad_bytes(&already_bound), completed);

        let fully_bound =
            terminal_story_vmad(0x18211C, Some(0x09C824), Some(0x09C824), true);
        let mut opted_in = en05_terminal(
            &interner,
            "TERM",
            0x1820DD,
            "EN05_ObstacleCourseTerminal",
            raw_bytes(&fully_bound),
        );
        hook.post_translate(&mut ctx, &mut opted_in).unwrap();
        assert_eq!(vmad_bytes(&opted_in), fully_bound);
    }

    #[test]
    fn en05_terminal_vmad_backfill_rejects_wrong_or_malformed_records() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let valid_vmad = terminal_story_vmad(0x1820D3, None, None, true);
        let no_story_script = terminal_story_vmad(0x1820D3, None, None, false);
        let responder_expected_only =
            terminal_story_vmad(0x3B32EC, Some(0x3B32DC), None, true);
        let cases = [
            ("FURN", 0x1820CF, "EN05_MarksmanshipCourseTerminal", raw_bytes(&valid_vmad)),
            ("TERM", 0x1820CE, "EN05_MarksmanshipCourseTerminal", raw_bytes(&valid_vmad)),
            ("TERM", 0x1820CF, "EN05_MarksmanshipCourseTerminalWrong", raw_bytes(&valid_vmad)),
            ("TERM", 0x000800, "UnrelatedTerminal", raw_bytes(&valid_vmad)),
            ("TERM", 0x3B3302, "RSVP00_Terminal_Kiosk_Main", raw_bytes(&responder_expected_only)),
            ("TERM", 0x1820CF, "EN05_MarksmanshipCourseTerminal", raw_bytes(&no_story_script)),
            ("TERM", 0x1820CF, "EN05_MarksmanshipCourseTerminal", raw_bytes(&[6, 0, 2, 0, 1])),
        ];

        for (signature, form_id, editor_id, vmad) in cases {
            let mut record = en05_terminal(&interner, signature, form_id, editor_id, vmad);
            let before = vmad_bytes(&record);
            hook.post_translate(&mut ctx, &mut record).unwrap();
            assert_eq!(
                vmad_bytes(&record),
                before,
                "{signature} {form_id:06X} {editor_id}"
            );
        }

        let mut non_raw = en05_terminal(
            &interner,
            "TERM",
            0x1820CF,
            "EN05_MarksmanshipCourseTerminal",
            FieldValue::Struct(Vec::new()),
        );
        hook.post_translate(&mut ctx, &mut non_raw).unwrap();
        assert!(matches!(
            non_raw.fields[1].value,
            FieldValue::Struct(ref fields) if fields.is_empty()
        ));
    }

    #[test]
    fn translated_raw_en05_terminal_vmad_receives_output_quest_reference() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let source_vmad = terminal_story_vmad(0x18212A, None, None, true);
        let mut source = en05_terminal(
            &interner,
            "TERM",
            0x182129,
            "EN05_PatriotismTerminal",
            raw_bytes(&source_vmad),
        );

        translator
            .pre_translate(&mut make_ctx(&interner), &mut source)
            .unwrap();
        let mut translated = match translator.translate(&source, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated TERM, got {other:?}"),
        };
        assert_eq!(vmad_bytes(&translated), source_vmad);

        translator
            .post_translate(&mut make_ctx(&interner), &mut translated)
            .unwrap();

        let payload = decoded_terminal_vmad(&translated);
        assert_eq!(
            quest_reference(&payload, EXPECTED_QUEST_TO_START_MEMBER),
            (FO76_MASTER_NAME, "08C881")
        );
        assert_eq!(
            quest_reference(&payload, UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER),
            (FO76_MASTER_NAME, "08C881")
        );

        let raw_payload = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
            &vmad_bytes(&translated),
            &[FO4_MASTER_NAME.to_string()],
            FO76_MASTER_NAME,
            Some("TERM"),
        )
        .unwrap();
        let raw_properties = raw_payload["Scripts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|script| {
                script["ScriptName"].as_str()
                    == Some(DEFAULT_SEND_STORY_EVENT_ON_MENU_ITEM_RUN_SCRIPT)
            })
            .unwrap()["Properties"]
            .as_array()
            .unwrap();
        let raw_menu_data = raw_properties
            .iter()
            .find(|property| {
                property["propertyName"].as_str() == Some("MenuData")
            })
            .unwrap();
        let raw_intended_row = raw_menu_data["Value"]
            .as_array()
            .unwrap()[0]
            .as_array()
            .unwrap();
        let raw_expected_quest = raw_intended_row
            .iter()
            .find(|member| {
                member["memberName"].as_str() == Some(EXPECTED_QUEST_TO_START_MEMBER)
            })
            .unwrap();
        assert_eq!(raw_expected_quest["Value"]["FormID"]["raw"], "0808C881");
        let raw_unique_quest = raw_intended_row
            .iter()
            .find(|member| {
                member["memberName"].as_str() == Some(UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER)
            })
            .unwrap();
        assert_eq!(raw_unique_quest["Value"]["FormID"]["raw"], "0808C881");
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

    #[test]
    fn post_translate_persists_fo76_one_state_activator_animation() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("ACTI", &interner);
        push_field(
            &mut record,
            "VMAD",
            raw_bytes(&fo4_vmad_with_script(DEFAULT_ONE_STATE_ACTIVATOR_SCRIPT)),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let bytes = vmad_bytes(&record);
        assert!(vmad_contains_name(
            &bytes,
            DEFAULT_ONE_STATE_ACTIVATOR_SCRIPT
        ));
        assert!(vmad_contains_name(&bytes, "Anim"));
        assert!(vmad_contains_name(
            &bytes,
            FO76_DEFAULT_ONE_STATE_ANIMATION
        ));
    }

    #[test]
    fn post_translate_leaves_explicit_one_state_activator_properties_unchanged() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let carried = build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": FO4_VMAD_VERSION,
                "Object Format": FO4_VMAD_OBJECT_FORMAT,
                "Scripts": [{
                    "ScriptName": DEFAULT_ONE_STATE_ACTIVATOR_SCRIPT,
                    "Properties": [{
                        "propertyName": "Anim",
                        "Type": "String",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": "open",
                    }],
                }],
            }),
            &[FO4_MASTER_NAME.to_string()],
            FO76_MASTER_NAME,
        )
        .expect("fixture VMAD must encode");
        let mut record = make_record("ACTI", &interner);
        push_field(&mut record, "VMAD", raw_bytes(&carried));

        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(vmad_bytes(&record), carried);
    }

    #[test]
    fn post_translate_drops_zero_bench_type_but_keeps_real_workbenches() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        let mut instrument = make_record("FURN", &interner);
        push_field(&mut instrument, "WBDT", raw_bytes(&[0, 0]));
        hook.post_translate(&mut ctx, &mut instrument).unwrap();
        assert!(
            instrument
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "WBDT")
        );

        let mut workbench = make_record("FURN", &interner);
        push_field(
            &mut workbench,
            "WBDT",
            FieldValue::Struct(vec![
                (interner.intern("BenchType"), FieldValue::Uint(5)),
                (interner.intern("UnknownByte2"), FieldValue::Uint(0)),
            ]),
        );
        hook.post_translate(&mut ctx, &mut workbench).unwrap();
        assert!(
            workbench
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "WBDT")
        );
    }

    #[test]
    fn translates_music_instrument_links_into_fo4_script_properties() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let source_vmad = fo4_vmad_with_script("SourceFurnitureScript");
        let mut source = make_record("FURN", &interner);
        push_field(&mut source, "VMAD", raw_bytes(&source_vmad));
        let mut fnmu = Vec::new();
        for form_id in [0x06CA94_u32, 0x06CA95, 0x06C9D0, 0x06C9D1] {
            fnmu.extend_from_slice(&form_id.to_le_bytes());
        }
        push_field(&mut source, "FNMU", raw_bytes(&fnmu));

        let source_masters = Vec::new();
        let target_masters = fo76_fo4_output_masters();
        let mut ctx = PairCtx::with_source_and_target(
            &interner,
            FO76_MASTER_NAME,
            &source_masters,
            &target_masters,
        );
        translator
            .pre_translate(&mut ctx, &mut source)
            .unwrap();
        let mut translated = match translator.translate(&source, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated FURN, got {other:?}"),
        };
        translator
            .post_translate(&mut ctx, &mut translated)
            .unwrap();

        let bytes = vmad_bytes(&translated);
        assert!(vmad_contains_name(&bytes, MUSIC_INSTRUMENT_SCRIPT));
        assert!(!vmad_contains_name(&bytes, "SourceFurnitureScript"));
        let payload = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
            &bytes,
            &target_masters,
            FO76_MASTER_NAME,
            Some("FURN"),
        )
        .unwrap();
        let script = &payload["Scripts"][0];
        assert_eq!(script["ScriptName"], MUSIC_INSTRUMENT_SCRIPT);
        let properties = script["Properties"].as_array().unwrap();
        assert_eq!(properties.len(), 4);
        for ((name, _), form_id) in MUSIC_INSTRUMENT_ROLES
            .iter()
            .zip(["06CA94", "06CA95", "06C9D0", "06C9D1"])
        {
            let property = properties
                .iter()
                .find(|property| property["propertyName"].as_str() == Some(name))
                .unwrap();
            assert_eq!(
                property["Value"]["FormID"]["reference"]["plugin"],
                FO76_MASTER_NAME
            );
            assert_eq!(
                property["Value"]["FormID"]["reference"]["object_id"],
                form_id
            );
        }
    }

    #[test]
    fn still_drops_source_furniture_vmad_without_music_instrument_data() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let mut source = make_record("FURN", &interner);
        push_field(
            &mut source,
            "VMAD",
            raw_bytes(&fo4_vmad_with_script("SourceFurnitureScript")),
        );

        translator
            .pre_translate(&mut make_ctx(&interner), &mut source)
            .unwrap();
        let mut translated = match translator.translate(&source, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated FURN, got {other:?}"),
        };
        translator
            .post_translate(&mut make_ctx(&interner), &mut translated)
            .unwrap();

        assert!(
            translated
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "VMAD")
        );
    }
