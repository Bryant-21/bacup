

    #[test]
    fn build_fo4_qust_dnam_relayouts_16_byte_flags32_variant() {
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS32_LEN];
        data[0..4].copy_from_slice(&0x0000_0111_u32.to_le_bytes());
        data[4] = 7; // priority
        data[8..12].copy_from_slice(&2.0_f32.to_le_bytes());
        data[12] = 3; // quest_type

        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("dnam");
        assert_eq!(u16::from_le_bytes([dnam[0], dnam[1]]), 0x0111);
        assert_eq!(dnam[2], 7);
        assert_eq!(f32::from_le_bytes(dnam[4..8].try_into().unwrap()), 2.0);
        assert_eq!(dnam[8], FO4_QUST_TYPE_SIDE_QUESTS);
    }

    #[test]
    fn build_fo4_qust_dnam_masks_fo76_only_high_flag_bits() {
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        // 0x80000 = holotape_only (FO76-only) ORed with 0x8311 standard low bits.
        data[0..8].copy_from_slice(&0x0000_0000_0008_8311_u64.to_le_bytes());
        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("dnam");
        assert_eq!(
            u16::from_le_bytes([dnam[0], dnam[1]]),
            0x8311,
            "FO76-only high flag bits masked off"
        );
    }

    #[test]
    fn build_fo4_qust_dnam_rejects_unknown_length() {
        assert!(build_fo4_qust_dnam_from_fo76_data(&[0u8; 13]).is_none());
    }

    #[test]
    fn build_fo4_qust_dnam_maps_quest_type_enums_between_games() {
        assert_eq!(fo76_qust_type_to_fo4(0), FO4_QUST_TYPE_NONE);
        assert_eq!(fo76_qust_type_to_fo4(1), FO4_QUST_TYPE_MAIN_QUEST);
        assert_eq!(fo76_qust_type_to_fo4(2), FO4_QUST_TYPE_SIDE_QUESTS);
        assert_eq!(fo76_qust_type_to_fo4(3), FO4_QUST_TYPE_SIDE_QUESTS);
        assert_eq!(fo76_qust_type_to_fo4(7), FO4_QUST_TYPE_MISCELLANEOUS);
        assert_eq!(
            fo76_qust_type_to_fo4(FO76_QUST_TYPE_PUBLIC_EVENT),
            FO4_QUST_TYPE_NONE
        );
        assert_eq!(
            fo76_qust_type_to_fo4(FO76_QUST_TYPE_EVENT),
            FO4_QUST_TYPE_NONE
        );
    }

    #[test]
    fn pre_translate_converts_qust_data_to_fo4_dnam() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8311_u64.to_le_bytes());
        data[8] = 5;
        data[16] = 2;
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"DATA"), "FO76 DATA renamed away");
        assert!(sigs.contains(&"DNAM"), "FO4 DNAM emitted");
        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM should be raw bytes");
        };
        assert_eq!(bytes.len(), FO4_QUST_DNAM_LEN);
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]), 0x8311);
    }

    #[test]
    fn pre_translate_qust_keeps_existing_dnam_untouched() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "DNAM",
            raw_bytes(&[0x01, 0x00, 9, 0, 0, 0, 0, 0, 1, 0, 0, 0]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes");
        };
        assert_eq!(bytes[2], 9, "existing DNAM priority preserved");
    }

    #[test]
    fn pre_translate_qust_data_to_dnam_ignores_non_qust() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(
            &mut record,
            "DATA",
            raw_bytes(&[0u8; FO76_QUST_DATA_FLAGS64_LEN]),
        );

        Fo76Fo4Hook::convert_qust_data_to_fo4_dnam(&interner, &mut record);

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"DATA"), "non-QUST DATA left untouched");
        assert!(!sigs.contains(&"DNAM"));
    }

    #[test]
    fn qust_eid_dialogue_match_explicit_containers_only() {
        let mut interner = StringInterner::new();
        // String variant, mixed case.
        let mut r1 = make_record("QUST", &mut interner);
        push_field(
            &mut r1,
            "EDID",
            FieldValue::String(interner.intern("XPD_Dialogue_WhitespringGreeter")),
        );
        assert!(qust_eid_is_dialogue_conversation(&interner, &r1));

        // Bytes variant, lower case, NUL-terminated.
        let mut r2 = make_record("QUST", &mut interner);
        push_field(&mut r2, "EDID", raw_bytes(b"some_dialogue_thing\x00"));
        assert!(qust_eid_is_dialogue_conversation(&interner, &r2));

        let mut r3 = make_record("QUST", &mut interner);
        r3.eid = Some(interner.intern("NPCConversation_Biv"));
        assert!(qust_eid_is_dialogue_conversation(&interner, &r3));

        // Has dialogue content, but is not a dialogue-container quest.
        let mut r4 = make_record("QUST", &mut interner);
        r4.eid = Some(interner.intern("TW043"));
        assert!(!qust_eid_is_dialogue_conversation(&interner, &r4));

        // Non-dialogue gameplay quest -> no match.
        let mut r5 = make_record("QUST", &mut interner);
        push_field(
            &mut r5,
            "EDID",
            FieldValue::String(interner.intern("EN07_MQ_Nuke_Master")),
        );
        assert!(!qust_eid_is_dialogue_conversation(&interner, &r5));

        // No EDID field at all -> no match (no force-start).
        let r6 = make_record("QUST", &mut interner);
        assert!(!qust_eid_is_dialogue_conversation(&interner, &r6));
    }

    // Build-independent ground-truth test: run the REAL source decode
    // (decode_record_from_parsed_relayout, exactly what translate_v2 Pass P
    // uses) on a real-shaped FO76 QUST, then the DNAM relayout. This catches
    // any divergence between the hand-built Record unit tests and the actual
    // decoded field shape (e.g. DATA not surfacing as Bytes(20)).
    #[test]
    fn real_decode_qust_data_and_swf_path_survive_translation() {
        use crate::source_read::decode_record_from_parsed_relayout;
        use crate::struct_relayout::StructRelayoutCtx;
        use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
        use smol_str::SmolStr;

        let fo76 = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let fo4 = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let fk = FormKey {
            local: 0x0065_3177,
            plugin: interner.intern("SeventySix.esm"),
        };

        let mut edid = b"XPD_Dialogue_WhitespringGreeter".to_vec();
        edid.push(0);
        // Real source DATA bytes captured from SeventySix.esm 0x653177:
        // flags low16 = 0x8500 (has_dialogue_data set, SGE not set).
        let data: Vec<u8> = vec![
            0x00, 0x85, 0x80, 0x02, 0x00, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(data.len(), 20);
        let swf_path = b"components/quest vault boys/quests/swamp forest_color.swf\0".to_vec();

        let mk = |sig: &str, d: Vec<u8>| ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: bytes::Bytes::from(d),
            semantic_type: None,
        };
        let raw = ParsedRecord {
            signature: SmolStr::new("QUST"),
            form_id: 0x0065_3177,
            flags: 0,
            version_control: 0,
            form_version: Some(202),
            version2: None,
            subrecords: vec![
                mk("EDID", edid),
                mk("DATA", data),
                mk("SNAM", swf_path.clone()),
            ],
            raw_payload: None,
            parse_error: None,
        };

        let ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: false,
        };
        let mut record = decode_record_from_parsed_relayout(
            &raw,
            &fk,
            &fo76,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
            Some(&ctx),
        )
        .expect("decode");

        let data_field = record.fields.iter().find(|f| f.sig.as_str() == "DATA");
        eprintln!(
            "DATA after real decode = {:?}",
            data_field.map(|f| &f.value)
        );
        let snam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "SNAM")
            .expect("source SNAM");
        let FieldValue::Bytes(snam_bytes) = &snam.value else {
            panic!("QUST SNAM must stay raw bytes");
        };
        assert_eq!(snam_bytes.as_slice(), swf_path.as_slice());

        // Drive the REAL Pass-P sequence: full pre_translate (every hook step,
        // in order) then translate (map-driven drops/transforms). This is what
        // the whole-plugin translate_v2 path runs per record.
        let translator = crate::translator::Translator::new(
            crate::translator::Game::Fo76,
            crate::translator::Game::Fo4,
        )
        .expect("translator");

        let mut ctx = crate::translator::pair_hook::PairCtx {
            interner: &interner,
        };
        translator
            .pre_translate(&mut ctx, &mut record)
            .expect("pre_translate");
        let after_pt = record.fields.iter().find(|f| f.sig.as_str() == "DNAM");
        eprintln!(
            "DNAM after full pre_translate = {:?}",
            after_pt.map(|f| &f.value)
        );

        let translated = match translator.translate(&record, &interner) {
            crate::translator::TranslateResult::Translated(r) => r,
            crate::translator::TranslateResult::Dropped { .. } => panic!("translate Dropped"),
            crate::translator::TranslateResult::Deferred(_) => panic!("translate Deferred"),
        };
        let dnam = translated.fields.iter().find(|f| f.sig.as_str() == "DNAM");
        eprintln!("DNAM after translate = {:?}", dnam.map(|f| &f.value));

        let dnam = dnam.expect("DNAM must survive full pre_translate + translate");
        let FieldValue::Bytes(b) = &dnam.value else {
            panic!("DNAM should be Bytes");
        };
        assert_eq!(
            u16::from_le_bytes([b[0], b[1]]) & 0x0001,
            0,
            "record relayout must preserve the source startup state"
        );
        let encoded = crate::target_write::encode_field_pub(
            translated
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "SNAM")
                .expect("top-level SNAM"),
            fo4.record_def("QUST"),
            &interner,
        )
        .expect("encode");
        assert_eq!(encoded, swf_path);
    }

    #[test]
    fn pre_translate_does_not_force_start_dialogue_named_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("XPD_Dialogue_WhitespringGreeter")),
        );
        // FO76 20-byte DATA: flags u64 low word 0x8500 (has_dialogue_data, NOT SGE).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8500_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "EditorID classification must not invent Start-Game-Enabled"
        );
    }

    #[test]
    fn pre_translate_preserves_instanced_story_manager_quest_autostart() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("WhitespringQuest")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0001_8111_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));
        push_field(
            &mut record,
            "ENAM",
            FieldValue::Bytes(SmallVec::from_vec(0x434F_4C49_u32.to_le_bytes().to_vec())),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001, 1);
        assert_eq!(bytes[8], FO4_QUST_TYPE_NONE);
        assert!(record.warnings.is_empty());
    }

    #[test]
    fn pre_translate_preserves_pennington_dialogue_controller_autostart() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("W05_MQ_001P_Wayward_PenningtonScene")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0401_8511_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));
        push_field(
            &mut record,
            "ENAM",
            FieldValue::Bytes(SmallVec::from_vec(0x434F_4C49_u32.to_le_bytes().to_vec())),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            1,
            "Pennington's persistent dialogue carrier must initialize its aliases"
        );
        assert_eq!(bytes[8], FO4_QUST_TYPE_NONE);
    }

    #[test]
    fn pre_translate_does_not_force_start_gameplay_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("RE_SceneKMK01")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8500_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "gameplay quest left non-SGE"
        );
    }

    #[test]
    fn pre_translate_disables_event_quest_even_when_dialogue_named() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("Dialogue_EventActivity")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        data[16] = FO76_QUST_TYPE_EVENT;
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001, 0);
        assert_eq!(bytes[8], FO4_QUST_TYPE_NONE);
        assert!(record.warnings.iter().any(|warning| {
            interner
                .resolve(*warning)
                .is_some_and(|message| message.contains("reason=quest_type_event"))
        }));
    }

    #[test]
    fn pre_translate_disables_en_event_quest_and_logs_reason() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("EN07_MQ_Nuke_Master")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0001_8111_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001, 0);
        let warning = record
            .warnings
            .iter()
            .find_map(|warning| interner.resolve(*warning))
            .expect("quest disable warning");
        assert!(warning.contains("qust_start_game_disabled:"));
        assert!(warning.contains("form=SeventySix.esm:000800"));
        assert!(warning.contains("editor_id=en07_mq_nuke_master"));
        assert!(warning.contains("reason=event_editor_id_prefix_en"));
        assert!(warning.contains("source_flags=0x8111"));
    }

    #[test]
    fn pre_translate_disables_public_event_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        data[16] = FO76_QUST_TYPE_PUBLIC_EVENT;
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001, 0);
        assert_eq!(bytes[8], FO4_QUST_TYPE_NONE);
    }

    #[test]
    fn pre_translate_does_not_force_start_test_dialogue_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("test_VHarbison_Dialogue_Someone")),
        );
        // has_dialogue_data, NOT start-game-enabled in source.
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8500_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "test/dev dialogue quest must NOT be force-started (scene CTD)"
        );
    }

    #[test]
    fn pre_translate_clears_sge_on_faithfully_sge_test_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("TestDialogueExpressions")),
        );
        // has_dialogue_data AND start-game-enabled in source (FO76 data0=0x11 family).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "developer test quest must never auto-start, even if FO76 marked it SGE"
        );
    }

    #[test]
    fn pre_translate_clears_sge_on_debug_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("DebugCorrieQuest")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8319_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001, 0);
        assert!(record.warnings.iter().any(|warning| {
            interner
                .resolve(*warning)
                .is_some_and(|message| message.contains("reason=test_or_dev_editor_id"))
        }));
    }

    #[test]
    fn pre_translate_preserves_non_instanced_gameplay_quest_sge() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("RE_SceneKMK01")),
        );
        // has_dialogue_data AND start-game-enabled in source (0x8501).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            1,
            "ordinary source startup state must be preserved"
        );
    }

    #[test]
    fn pre_translate_preserves_persistent_radio_station_autostart() {
        for (editor_id, flags) in [
            ("SQ_RadioAppalachia", 0x0000_0000_0401_8511_u64),
            ("BoS_Radio", 0x0000_0000_0001_8119_u64),
        ] {
            let mut interner = StringInterner::new();
            let mut record = make_record("QUST", &mut interner);
            push_field(
                &mut record,
                "EDID",
                FieldValue::String(interner.intern(editor_id)),
            );
            let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
            data[0..8].copy_from_slice(&flags.to_le_bytes());
            push_field(&mut record, "DATA", raw_bytes(&data));

            let hook = Fo76Fo4Hook;
            let mut ctx = make_ctx(&interner);
            hook.pre_translate(&mut ctx, &mut record).unwrap();

            let dnam = record
                .fields
                .iter()
                .find(|f| f.sig.as_str() == "DNAM")
                .unwrap();
            let FieldValue::Bytes(bytes) = &dnam.value else {
                panic!("DNAM bytes")
            };
            assert_eq!(
                u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
                0x0001,
                "{editor_id} keeps its FO76 start-game-enabled flag"
            );
        }
    }

    #[test]
    fn pre_translate_hard_disables_high_school_pa_startup() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("CB_HighSchoolPASystem_RadioScenes")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        // Deliberately omit the unique-instance flag: the exact quest remains
        // disabled even if its source flags change or radio policy broadens.
        data[0..8].copy_from_slice(&0x0000_0000_0400_8111_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "High School PA scenes must never start in FO4"
        );
        assert!(record.warnings.iter().any(|warning| {
            interner.resolve(*warning).is_some_and(|message| {
                message.contains("reason=explicit_high_school_pa_exclusion")
            })
        }));
    }

    #[test]
    fn pre_translate_preserves_other_cb_region_quest_autostart() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("CB_RegionPatrol")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0001_8111_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001, 1);
        assert!(record.warnings.is_empty());
    }

    #[test]
    fn pre_translate_does_not_enable_mq_radio_segment() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("W05_MQ_003P_Radio")),
        );
        // MQ radio segment: has_dialogue_data, NOT start-game-enabled in source.
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8500_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "radio quest with no FO76 SGE (main-quest segment) stays off"
        );
    }

    #[test]
    fn pre_translate_strips_fo76_only_story_manager_event() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "DNAM",
            raw_bytes(&[0x11, 0x03, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        );
        push_field(
            &mut record,
            "ENAM",
            FieldValue::Bytes(SmallVec::from_vec(0x434F_4C49_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "LNAM", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "DNAM", "LNAM"]);
        let FieldValue::Bytes(dnam) = &record.fields[1].value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([dnam[0], dnam[1]]) & 1, 1);
    }

    #[test]
    fn pre_translate_strips_all_fo76_only_quest_events_but_keeps_scpt() {
        for event in [b"ADBO", b"CBGN", b"ILOC", b"LCPG", b"PCON", b"QPMT"] {
            let mut interner = StringInterner::new();
            let mut record = make_record("QUST", &mut interner);
            push_field(&mut record, "ENAM", raw_bytes(event));

            let hook = Fo76Fo4Hook;
            let mut ctx = make_ctx(&interner);
            hook.pre_translate(&mut ctx, &mut record).unwrap();

            assert!(
                record.fields.iter().all(|field| field.sig.as_str() != "ENAM"),
                "{} must not reach FO4",
                String::from_utf8_lossy(event)
            );
        }

        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "ENAM", raw_bytes(b"SCPT"));
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(record.fields.iter().any(|field| field.sig.as_str() == "ENAM"));
    }

    #[test]
    fn pre_translate_disables_existing_dnam_event_types() {
        for quest_type in [FO76_QUST_TYPE_PUBLIC_EVENT, FO76_QUST_TYPE_EVENT] {
            let mut interner = StringInterner::new();
            let mut record = make_record("QUST", &mut interner);
            let mut dnam = vec![0u8; FO4_QUST_DNAM_LEN];
            dnam[0..2].copy_from_slice(&0x8501_u16.to_le_bytes());
            dnam[8] = quest_type;
            push_field(&mut record, "DNAM", raw_bytes(&dnam));

            let hook = Fo76Fo4Hook;
            let mut ctx = make_ctx(&interner);
            hook.pre_translate(&mut ctx, &mut record).unwrap();

            let dnam = record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "DNAM")
                .unwrap();
            let FieldValue::Bytes(bytes) = &dnam.value else {
                panic!("DNAM bytes")
            };
            assert_eq!(
                u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
                0,
                "quest type {quest_type} must not start at game load"
            );
        }
    }

    #[test]
    fn pre_translate_strips_qust_objective_targets_but_keeps_alias_chain() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(300));
        push_field(&mut record, "INDX", FieldValue::None);
        push_field(&mut record, "QSDT", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(300));
        push_field(&mut record, "QOBJ", FieldValue::Uint(10));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "NNAM", FieldValue::None);
        push_field(
            &mut record,
            "QSTA",
            FieldValue::Bytes(SmallVec::from_vec(vec![3, 0, 0, 0])),
        );
        push_field(&mut record, "CTDA", raw_ctda(300));
        push_field(&mut record, "CIS1", FieldValue::None);
        push_field(&mut record, "CIS2", FieldValue::None);
        push_field(
            &mut record,
            "QSTA",
            FieldValue::Bytes(SmallVec::from_vec(vec![4, 0, 0, 0])),
        );
        push_field(&mut record, "QOBJ", FieldValue::Uint(20));
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(5_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALST", FieldValue::Bytes(SmallVec::new()));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        // Objective-target conditions (the QSTA-led CTDA/CIS1/CIS2 runs) are
        // stripped, but the post-ANAM alias chain (ALST and the rest) is now
        // retained so the FO4 alias table is rebuilt.
        assert_eq!(
            sigs,
            vec![
                "EDID", "CTDA", "INDX", "QSDT", "CTDA", "QOBJ", "FNAM", "NNAM", "QOBJ", "ANAM",
                "ALST",
            ]
        );
    }

    #[test]
    fn pre_translate_drops_only_objective_scope_qust_snam() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "SNAM", raw_bytes(b"Interface/Quest.swf\0"));
        push_field(&mut record, "QOBJ", FieldValue::Uint(11));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "QOTM", FieldValue::None);
        push_field(&mut record, "SNAM", raw_bytes(&u16::MAX.to_le_bytes()));
        push_field(&mut record, "NNAM", FieldValue::None);
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALST", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "SNAM", raw_bytes(b"AliasDisplayName\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let snam_values: Vec<&[u8]> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "SNAM")
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => bytes.as_slice(),
                other => panic!("SNAM must stay Bytes, got {other:?}"),
            })
            .collect();
        assert_eq!(
            snam_values,
            vec![
                b"Interface/Quest.swf\0".as_slice(),
                b"AliasDisplayName\0".as_slice()
            ]
        );
    }

    #[test]
    fn pre_translate_keeps_qust_alias_like_sigs_before_anam() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "CTDA", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "FNAM", FieldValue::Bytes(SmallVec::new()));
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 4])),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "CTDA", "FNAM", "ANAM"]);
    }

    #[test]
    fn pre_translate_is_noop_when_no_global_fields_present() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "FULL", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields.len(), 2);
    }

    // -------------------------------------------------------------------------
    // Behavior 2: synthetic-source-field identification
    // -------------------------------------------------------------------------

    #[test]
    fn effects_synthetic_true_for_alch_ench_perk_spel() {
        for sig in &["ALCH", "ENCH", "PERK", "SPEL"] {
            let s = SigCode::from_str(sig).unwrap();
            assert!(
                Fo76Fo4Hook::is_effects_synthetic(s),
                "{sig} should be synthetic"
            );
        }
    }
