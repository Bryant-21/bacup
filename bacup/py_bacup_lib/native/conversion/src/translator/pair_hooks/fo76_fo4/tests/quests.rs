
    fn qust_vmad_with_top_level_scripts(script_names: &[&str]) -> FieldValue {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FO76_VMAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&FO76_VMAD_OBJECT_FORMAT.to_le_bytes());
        bytes.extend_from_slice(&(script_names.len() as u16).to_le_bytes());
        for script_name in script_names {
            bytes.extend_from_slice(&(script_name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(script_name.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(&0_u16.to_le_bytes());
        }
        bytes.push(FO76_QUST_FRAGMENT_VERSION);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        raw_bytes(&bytes)
    }

    #[test]
    fn build_fo4_qust_dnam_relayouts_masks_flags_and_maps_types() {
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS32_LEN];
        data[0..4].copy_from_slice(&0x0000_0111_u32.to_le_bytes());
        data[4] = 7; // priority
        data[8..12].copy_from_slice(&2.0_f32.to_le_bytes());
        data[12] = 2; // quest_type
        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("flags32 dnam");
        assert_eq!(u16::from_le_bytes([dnam[0], dnam[1]]), 0x0111);
        assert_eq!(dnam[2], 7);
        assert_eq!(f32::from_le_bytes(dnam[4..8].try_into().unwrap()), 2.0);
        assert_eq!(dnam[8], FO4_QUST_TYPE_SIDE_QUESTS);

        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8111_u64.to_le_bytes());
        data[16] = 5; // Real FO76 QUST Daily enum from the generated schema.
        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("daily dnam");
        let flags = u16::from_le_bytes([dnam[0], dnam[1]]);
        assert_eq!(flags & QUST_DNAM_FLAG_RUN_ONCE, 0, "daily clears run-once");
        assert_eq!(flags & QUST_DNAM_FLAG_START_GAME_ENABLED, 1);
        assert_eq!(dnam[8], FO4_QUST_TYPE_SIDE_QUESTS);

        data[16] = 2;
        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("side quest dnam");
        assert_eq!(
            u16::from_le_bytes([dnam[0], dnam[1]]) & QUST_DNAM_FLAG_RUN_ONCE,
            QUST_DNAM_FLAG_RUN_ONCE,
            "non-daily keeps run-once"
        );

        // 0x80000 = holotape_only (FO76-only) ORed with 0x8311 standard low bits.
        data[0..8].copy_from_slice(&0x0000_0000_0008_8311_u64.to_le_bytes());
        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("masked dnam");
        assert_eq!(
            u16::from_le_bytes([dnam[0], dnam[1]]),
            0x8311,
            "FO76-only high flag bits masked off"
        );

        assert!(build_fo4_qust_dnam_from_fo76_data(&[0u8; 13]).is_none());

        for (fo76, fo4) in [
            (0, FO4_QUST_TYPE_NONE),
            (1, FO4_QUST_TYPE_MAIN_QUEST),
            (2, FO4_QUST_TYPE_SIDE_QUESTS),
            (3, FO4_QUST_TYPE_SIDE_QUESTS),
            (5, FO4_QUST_TYPE_SIDE_QUESTS),
            (7, FO4_QUST_TYPE_MISCELLANEOUS),
            (FO76_QUST_TYPE_PUBLIC_EVENT, FO4_QUST_TYPE_SIDE_QUESTS),
            (FO76_QUST_TYPE_EVENT, FO4_QUST_TYPE_SIDE_QUESTS),
        ] {
            assert_eq!(fo76_qust_type_to_fo4(fo76), fo4, "FO76 quest type {fo76}");
        }
    }

    fn translated_run_once_flag(record: &Record) -> u16 {
        translated_qust_dnam(record).0 & QUST_DNAM_FLAG_RUN_ONCE
    }

    fn translated_qust_dnam(record: &Record) -> (u16, u8) {
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .expect("DNAM");
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        (u16::from_le_bytes([bytes[0], bytes[1]]), bytes[8])
    }

    #[derive(Clone, Copy, Debug)]
    struct QustCase {
        form_id: u32,
        plugin: Option<&'static str>,
        eid: Option<&'static str>,
        scripts: &'static [&'static str],
        flags: u64,
        quest_type: u8,
        existing_dnam: bool,
        qsdd_flags: Option<u64>,
        enam: Option<&'static [u8; 4]>,
        objectives: bool,
    }

    const QUST_CASE: QustCase = QustCase {
        form_id: 0x0000_0800,
        plugin: None,
        eid: None,
        scripts: &[],
        flags: 0,
        quest_type: 0,
        existing_dnam: false,
        qsdd_flags: None,
        enam: None,
        objectives: false,
    };

    fn qust_case_record(interner: &StringInterner, case: QustCase) -> Record {
        let mut record = make_record("QUST", interner);
        record.form_key.local = case.form_id;
        if let Some(plugin) = case.plugin {
            record.form_key.plugin = interner.intern(plugin);
        }
        record.eid = case.eid.map(|eid| interner.intern(eid));
        if !case.scripts.is_empty() {
            push_field(
                &mut record,
                "VMAD",
                qust_vmad_with_top_level_scripts(case.scripts),
            );
        }
        if case.existing_dnam {
            let mut dnam = vec![0u8; FO4_QUST_DNAM_LEN];
            dnam[0..2].copy_from_slice(&(case.flags as u16).to_le_bytes());
            dnam[8] = case.quest_type;
            push_field(&mut record, "DNAM", raw_bytes(&dnam));
        } else {
            let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
            data[0..8].copy_from_slice(&case.flags.to_le_bytes());
            data[16] = case.quest_type;
            push_field(&mut record, "DATA", raw_bytes(&data));
        }
        if let Some(flags) = case.qsdd_flags {
            push_field(
                &mut record,
                "QSDD",
                FieldValue::Struct(vec![(interner.intern("flags"), FieldValue::Uint(flags))]),
            );
        }
        if let Some(enam) = case.enam {
            push_field(&mut record, "ENAM", raw_bytes(enam));
        }
        if case.objectives {
            push_field(&mut record, "QOBJ", raw_bytes(&100u16.to_le_bytes()));
        }
        record
    }

    #[test]
    fn pre_translate_run_once_repeatability_requires_exact_quest_identity() {
        let interner = StringInterner::new();
        let remove_players: &'static [&'static str] = &["DefaultQuestRemovePlayersScript"];
        let engine_daily = QustCase {
            scripts: remove_players,
            flags: 0x0290_8500,
            quest_type: FO76_QUST_TYPE_DAILY,
            ..QUST_CASE
        };
        let public_event = QustCase {
            flags: 0x8500,
            quest_type: FO76_QUST_TYPE_PUBLIC_EVENT,
            ..QUST_CASE
        };
        let bosz01 = QustCase {
            form_id: BOSZ01_REPEATABLE_FORM_ID,
            eid: Some("BoSZ01"),
            flags: 0x0290_8500,
            quest_type: 2,
            ..QUST_CASE
        };
        let grunt = QustCase {
            form_id: BURN_GRUNT_HUNT_FORM_ID,
            eid: Some("Burn_BountyHunt_GruntHunt"),
            flags: 0x0294_8500,
            quest_type: 2,
            qsdd_flags: Some(u64::from(FO76_QSDD_AUTO_RESTART_FLAG)),
            ..QUST_CASE
        };
        let companion = QustCase {
            form_id: 0x0054_F1A4,
            eid: Some("COMP_RQ_Fetch"),
            flags: 0x8500,
            ..QUST_CASE
        };

        let mut cases = vec![
            (bosz01, true),
            (QustCase { form_id: BOSZ01_REPEATABLE_FORM_ID + 1, ..bosz01 }, false),
            (QustCase { eid: Some("BoSZ01_Copy"), ..bosz01 }, false),
            (QustCase { plugin: Some("UnsafeCopy.esm"), ..bosz01 }, false),
            (grunt, true),
            (QustCase { form_id: BURN_GRUNT_HUNT_FORM_ID + 1, ..grunt }, false),
            (QustCase { eid: Some("Burn_BountyHunt_GruntHunt_Copy"), ..grunt }, false),
            (QustCase { plugin: Some("UnsafeCopy.esm"), ..grunt }, false),
            (QustCase { qsdd_flags: Some(0), ..grunt }, false),
            (
                QustCase {
                    form_id: 0x0006_5DFE,
                    scripts: &["DefaultQuestRemovePlayersScript", "DefaultDailyQuestScript"],
                    flags: 0x0390_8500,
                    quest_type: FO76_QUST_TYPE_DAILY,
                    ..QUST_CASE
                },
                true,
            ),
            (
                QustCase {
                    form_id: 0x0006_5DFE,
                    scripts: &["DefaultDailyQuestScript"],
                    flags: 0x8500,
                    quest_type: FO4_QUST_TYPE_SIDE_QUESTS,
                    existing_dnam: true,
                    ..QUST_CASE
                },
                true,
            ),
            (
                QustCase {
                    form_id: 0x001E_D40B,
                    scripts: &[
                        "DefaultQuestRemovePlayersScript",
                        "mtr04_gamescomplete",
                        "OBSOLETEQuestCleanupItemsOnShutdown",
                    ],
                    flags: 0x0390_8500,
                    ..engine_daily
                },
                false,
            ),
            (
                QustCase {
                    form_id: 0x0045_E3D2,
                    flags: 0x8500,
                    quest_type: FO4_QUST_TYPE_SIDE_QUESTS,
                    existing_dnam: true,
                    ..engine_daily
                },
                true,
            ),
            (QustCase { form_id: 0x0045_E3D2, plugin: Some("Other.esm"), ..engine_daily }, false),
            (
                QustCase {
                    form_id: 0x0013_FB17,
                    eid: Some("LookoutTowerQuest"),
                    flags: 0x8500,
                    ..QUST_CASE
                },
                true,
            ),
            (QustCase { eid: Some("COMP_RQ_Fetch_UnsafeCopy"), ..companion }, false),
            (QustCase { plugin: Some("Foreign.esm"), ..companion }, false),
            (QustCase { form_id: 0x0058_3D15, ..public_event }, false),
            (QustCase { form_id: 0x0058_3D14, plugin: Some("Other.esm"), ..public_event }, false),
            // CB02_MaskTeam has no ENAM (CB02_QuestScript starts it), so
            // repeatability must key on the quest, not on event scoping.
            (
                QustCase {
                    form_id: 0x0051_AA0B,
                    flags: 0x8100,
                    quest_type: FO76_QUST_TYPE_PUBLIC_EVENT,
                    ..QUST_CASE
                },
                true,
            ),
        ];
        for existing_dnam in [false, true] {
            for (form_id, eid, flags, repeatable) in [
                (0x002D_0F69, "EN07_MQ_FleeBlast", 0x8100, true),
                (0x002D_0F69, "EN07_MQ_FleeBlast_Copy", 0x8100, false),
                (0x0034_43FB, "MTR07_Earth", 0x8108, true),
                (0x0034_43FB, "MTR07_EarthMisc", 0x8108, false),
            ] {
                cases.push((
                    QustCase {
                        form_id,
                        eid: Some(eid),
                        flags,
                        quest_type: 2,
                        existing_dnam,
                        ..QUST_CASE
                    },
                    repeatable,
                ));
            }
        }
        for (form_id, eid) in [
            (0x0054_F1A4, "COMP_RQ_Fetch"),
            (0x0056_FB76, "COMP_RQ_Kill"),
            (0x0057_27AD, "COMP_RQ_Rescue"),
            (0x0055_FD53, "COMP_Visitor"),
            (0x0058_215B, "COMP_RQ_Fetch_SpecificAliases_Beckett_000_SadDiary"),
            (0x0058_2164, "COMP_RQ_Rescue_SpecificAliases_Beckett_001_CultistSage"),
            (0x0058_2163, "COMP_RQ_Fetch_SpecificAliases_Beckett_002_Key"),
            (0x0058_2160, "COMP_RQ_Kill_SpecificAliases_Beckett_003_Bronx"),
            (0x0058_2167, "COMP_RQ_Fetch_SpecificAliases_Beckett_004_Cave"),
            (0x0058_2165, "COMP_RQ_Kill_SpecificAliases_Beckett_005_Blood"),
            (0x0058_215A, "COMP_RQ_Rescue_SpecificAliases_Beckett_006_Pet"),
            (0x0058_215E, "COMP_RQ_Kill_SpecificAliases_Beckett_007_DJ"),
            (0x0058_216A, "COMP_RQ_Rescue_SpecificAliases_Beckett_008_MissNanny"),
            (0x0058_2168, "COMP_RQ_Fetch_SpecificAliases_Beckett_009_Holotapes"),
            (0x0058_215F, "COMP_RQ_Fetch_SpecificAliases_Beckett_010_PoisonedFood"),
            (0x0058_215D, "COMP_RQ_Kill_SpecificAliases_Beckett_011_Eye"),
            (0x005A_272F, "COMP_RQ_Kill_SpecificAliases_Beckett_BloodEagleRandomLoc"),
            (0x005A_2730, "COMP_RQ_Kill_SpecificAliases_Beckett_BloodEagleDungeon"),
            (0x005A_272D, "COMP_RQ_Fetch_SpecificAliases_LegendaryArmor"),
            (0x005A_272E, "COMP_RQ_Fetch_SpecificAliases_LegendaryWeapon"),
        ] {
            cases.push((QustCase { form_id, eid: Some(eid), ..companion }, true));
        }
        for form_id in [
            0x001E_D40A, 0x0045_E3D2, 0x0045_E3D3, 0x0047_CF15, 0x0047_CF16, 0x0063_D33F,
            0x0063_BED4, 0x0063_D5BD, 0x0062_1FB7, 0x006F_D072,
        ] {
            cases.push((QustCase { form_id, ..engine_daily }, true));
        }
        for form_id in [
            0x0068_FD4A, 0x006A_173A, 0x006A_0F94, 0x006A_1030, 0x006A_17A9, 0x0069_F2C7,
            0x006A_21E3, 0x006A_21E4,
        ] {
            cases.push((QustCase { form_id, ..engine_daily }, false));
        }
        let repeatable_public_events = [
            0x0058_3D14, 0x0068_F383, 0x007E_BDF4, 0x0046_48C3, 0x0073_3DB5, 0x0004_E257,
            0x0045_4CB6, 0x0064_31CE, 0x0009_210E, 0x0049_8662, 0x007F_1E8A, 0x0012_E67E,
            0x0009_3187, 0x0031_1433, 0x0004_2F7E, 0x0062_16DA, 0x0065_E071, 0x006A_D506,
            0x0010_BAE1, 0x0051_09AF, 0x0056_2877, 0x0069_0659, 0x003E_271D, 0x0063_461B,
            0x0080_BFD0, 0x0063_4B0B, 0x0025_C090, 0x0004_A357, 0x005F_E4D7, 0x0018_7531,
            0x0065_B0A8, 0x0003_64D0,
        ];
        for form_id in repeatable_public_events {
            cases.push((QustCase { form_id, ..public_event }, true));
        }
        // Riding Shotgun, Always Vigilant, Powering Up, Retake.
        for form_id in [0x0056_0B13, 0x000A_73DC, 0x003E_4E89, 0x0000_9179] {
            cases.push((
                QustCase {
                    form_id,
                    flags: 0x8100,
                    quest_type: FO76_QUST_TYPE_EVENT,
                    ..QUST_CASE
                },
                true,
            ));
        }

        for (case, repeatable) in cases {
            let mut record = qust_case_record(&interner, case);
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
            let (flags, quest_type) = translated_qust_dnam(&record);
            assert_eq!(
                flags & QUST_DNAM_FLAG_RUN_ONCE,
                if repeatable { 0 } else { QUST_DNAM_FLAG_RUN_ONCE },
                "{case:?}"
            );
            if case.quest_type == FO76_QUST_TYPE_PUBLIC_EVENT || case.eid == Some("LookoutTowerQuest") {
                assert_eq!(flags & QUST_DNAM_FLAG_START_GAME_ENABLED, 0, "{case:?}");
            }
            if case.quest_type == FO76_QUST_TYPE_PUBLIC_EVENT {
                assert_eq!(quest_type, FO4_QUST_TYPE_SIDE_QUESTS, "{case:?}");
            }
        }

        let mut record = make_record("QUST", &interner);
        record.form_key.local = 0x0058_3D14;
        record.form_key.plugin = interner.intern("seVENTYsix.EsM");
        let mut dnam = vec![0x01, 0xA5, 73, 0xCC, 1, 2, 3, 4, 0, 5, 6, 7];
        dnam[8] = FO76_QUST_TYPE_PUBLIC_EVENT;
        let mut expected = dnam.clone();
        let expected_flags = u16::from_le_bytes([expected[0], expected[1]])
            & !QUST_DNAM_FLAG_RUN_ONCE
            & !QUST_DNAM_FLAG_START_GAME_ENABLED;
        expected[0..2].copy_from_slice(&expected_flags.to_le_bytes());
        push_field(&mut record, "DNAM", raw_bytes(&dnam));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .expect("DNAM");
        assert_eq!(
            dnam.value,
            raw_bytes(&expected),
            "existing DNAM keeps its other bytes; plugin match is case-insensitive"
        );
    }

    #[test]
    fn convert_qust_data_to_fo4_dnam_only_for_qust_without_dnam() {
        let interner = StringInterner::new();
        let mut record = make_record("QUST", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8311_u64.to_le_bytes());
        data[8] = 5;
        data[16] = 2;
        push_field(&mut record, "DATA", raw_bytes(&data));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"DATA"), "FO76 DATA renamed away");
        let FieldValue::Bytes(bytes) = &record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .expect("FO4 DNAM emitted")
            .value
        else {
            panic!("DNAM should be raw bytes");
        };
        assert_eq!(bytes.len(), FO4_QUST_DNAM_LEN);
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]), 0x8311);

        let mut record = make_record("QUST", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "DNAM",
            raw_bytes(&[0x01, 0x00, 9, 0, 0, 0, 0, 0, 1, 0, 0, 0]),
        );
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let FieldValue::Bytes(bytes) = &record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap()
            .value
        else {
            panic!("DNAM bytes");
        };
        assert_eq!(bytes[2], 9, "existing DNAM priority preserved");

        let mut record = make_record("WEAP", &interner);
        push_field(
            &mut record,
            "DATA",
            raw_bytes(&[0u8; FO76_QUST_DATA_FLAGS64_LEN]),
        );
        Fo76Fo4Hook::convert_qust_data_to_fo4_dnam(&interner, &mut record);
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["DATA"], "non-QUST DATA left untouched");
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
            0x00, 0x85, 0x80, 0x02, 0x00, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00,
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

        let mut ctx = crate::translator::pair_hook::PairCtx::new(&interner);
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
    fn real_decode_preserves_autostart_for_en_dialogue_controllers() {
        use crate::source_read::decode_record_from_parsed_relayout;
        use crate::struct_relayout::StructRelayoutCtx;
        use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
        use smol_str::SmolStr;

        let cases = [
            (
                0x0039_DD4,
                "EN05_DialogueGutsyDrillSergeant",
                0x0000_0000_0401_8111_u64,
            ),
            (
                0x0052_803,
                "EN05_DialogueCommieBots",
                0x0000_0000_0000_8111_u64,
            ),
            (
                0x008C_892,
                "EN05_DialoguePatriotismMannequins",
                0x0000_0000_0000_8111_u64,
            ),
        ];
        let fo76 = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let fo4 = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let translator = crate::translator::Translator::new(
            crate::translator::Game::Fo76,
            crate::translator::Game::Fo4,
        )
        .expect("translator");
        let relayout = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: false,
        };

        for (form_id, editor_id, source_flags) in cases {
            let mut edid = editor_id.as_bytes().to_vec();
            edid.push(0);
            let mut data = vec![0_u8; FO76_QUST_DATA_FLAGS64_LEN];
            data[0..8].copy_from_slice(&source_flags.to_le_bytes());
            data[8] = 40;
            data[16] = 1;
            let subrecord = |signature: &str, data: Vec<u8>| ParsedSubrecord {
                signature: SmolStr::new(signature),
                data: bytes::Bytes::from(data),
                semantic_type: None,
            };
            let raw = ParsedRecord {
                signature: SmolStr::new("QUST"),
                form_id,
                flags: 0,
                version_control: 0,
                form_version: Some(208),
                version2: Some(1),
                subrecords: vec![subrecord("EDID", edid), subrecord("DATA", data)],
                raw_payload: None,
                parse_error: None,
            };
            let form_key = FormKey {
                local: form_id,
                plugin: interner.intern("SeventySix.esm"),
            };
            let mut record = decode_record_from_parsed_relayout(
                &raw,
                &form_key,
                &fo76,
                &[],
                "SeventySix.esm",
                None,
                false,
                &interner,
                Some(&relayout),
            )
            .expect("decode");
            let mut ctx = crate::translator::pair_hook::PairCtx::new(&interner);
            translator
                .pre_translate(&mut ctx, &mut record)
                .expect("pre_translate");
            let translated = match translator.translate(&record, &interner) {
                crate::translator::TranslateResult::Translated(record) => record,
                crate::translator::TranslateResult::Dropped { .. } => panic!("translate Dropped"),
                crate::translator::TranslateResult::Deferred(_) => panic!("translate Deferred"),
            };
            let dnam = translated
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "DNAM")
                .expect("DNAM");
            let FieldValue::Bytes(bytes) = &dnam.value else {
                panic!("DNAM bytes")
            };
            assert_eq!(
                u16::from_le_bytes([bytes[0], bytes[1]]) & QUST_DNAM_FLAG_START_GAME_ENABLED,
                QUST_DNAM_FLAG_START_GAME_ENABLED,
                "{editor_id} must preserve source Start-Game-Enabled"
            );
        }
    }

    #[test]
    fn pre_translate_start_game_enabled_policy() {
        let interner = StringInterner::new();
        let sge = QUST_DNAM_FLAG_START_GAME_ENABLED;
        let named = |eid: &'static str, flags: u64| QustCase {
            eid: Some(eid),
            flags,
            ..QUST_CASE
        };
        // FO76 holotape_only hides a quest from the Pip-Boy but keeps it running;
        // only a flagged quest that also carries objectives is a QA harness.
        let holotape = QustCase {
            flags: 0x0008_8111,
            ..QUST_CASE
        };
        let no_warnings = Some("");
        for (case, expected_sge, expected_type, reason) in [
            (QustCase { objectives: true, ..holotape }, 0, None, None),
            (holotape, sge, None, None),
            (QustCase { flags: 0x8111, objectives: true, ..QUST_CASE }, sge, None, None),
            (named("XPD_Dialogue_WhitespringGreeter", 0x8500), 0, None, None),
            (named("RE_SceneKMK01", 0x8500), 0, None, None),
            (named("test_VHarbison_Dialogue_Someone", 0x8500), 0, None, None),
            (named("W05_MQ_003P_Radio", 0x8500), 0, None, None),
            (
                QustCase { enam: Some(b"ILOC"), ..named("WhitespringQuest", 0x0001_8111) },
                sge,
                Some(FO4_QUST_TYPE_NONE),
                no_warnings,
            ),
            (
                QustCase {
                    enam: Some(b"ILOC"),
                    ..named("W05_MQ_001P_Wayward_PenningtonScene", 0x0401_8511)
                },
                sge,
                Some(FO4_QUST_TYPE_NONE),
                None,
            ),
            (named("RE_SceneKMK01", 0x8501), sge, None, None),
            (named("SQ_RadioAppalachia", 0x0401_8511), sge, None, None),
            (named("BoS_Radio", 0x0001_8119), sge, None, None),
            (named("CB_RegionPatrol", 0x0001_8111), sge, None, no_warnings),
            (
                QustCase { flags: 0x8501, quest_type: FO76_QUST_TYPE_PUBLIC_EVENT, ..QUST_CASE },
                0,
                Some(FO4_QUST_TYPE_SIDE_QUESTS),
                None,
            ),
            (
                QustCase {
                    quest_type: FO76_QUST_TYPE_EVENT,
                    ..named("Dialogue_EventActivity", 0x8501)
                },
                0,
                Some(FO4_QUST_TYPE_SIDE_QUESTS),
                Some("reason=quest_type_event"),
            ),
            (named("TestDialogueExpressions", 0x8501), 0, None, None),
            (named("DebugCorrieQuest", 0x8319), 0, None, Some("reason=test_or_dev_editor_id")),
            (
                named("CB_HighSchoolPASystem_RadioScenes", 0x0400_8111),
                0,
                None,
                Some("reason=explicit_high_school_pa_exclusion"),
            ),
            (
                QustCase {
                    flags: 0x8501,
                    quest_type: FO76_QUST_TYPE_PUBLIC_EVENT,
                    existing_dnam: true,
                    ..QUST_CASE
                },
                0,
                None,
                None,
            ),
            (
                QustCase {
                    flags: 0x8501,
                    quest_type: FO76_QUST_TYPE_EVENT,
                    existing_dnam: true,
                    ..QUST_CASE
                },
                0,
                None,
                None,
            ),
        ] {
            let mut record = qust_case_record(&interner, case);
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
            let (flags, quest_type) = translated_qust_dnam(&record);
            assert_eq!(flags & sge, expected_sge, "{case:?}");
            if let Some(expected_type) = expected_type {
                assert_eq!(quest_type, expected_type, "{case:?}");
            }
            match reason {
                Some("") => assert!(record.warnings.is_empty(), "{case:?}"),
                Some(reason) => assert!(
                    record.warnings.iter().any(|warning| interner
                        .resolve(*warning)
                        .is_some_and(|message| message.contains(reason))),
                    "{case:?} should warn {reason}"
                ),
                None => {}
            }
        }

        let mut record = qust_case_record(&interner, named("EN07_MQ_Nuke_Master", 0x0001_8111));
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        assert_eq!(translated_qust_dnam(&record).0 & sge, 0);
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
    fn pre_translate_strips_fo76_only_quest_events_but_keeps_scpt() {
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
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "DNAM", "LNAM"]);
        let FieldValue::Bytes(dnam) = &record.fields[1].value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([dnam[0], dnam[1]]) & 1, 1);

        for event in [b"ADBO", b"CBGN", b"ILOC", b"LCPG", b"PCON", b"QPMT", b"SCPT"] {
            let mut record = make_record("QUST", &mut interner);
            push_field(&mut record, "ENAM", raw_bytes(event));
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
            assert_eq!(
                record.fields.iter().any(|field| field.sig.as_str() == "ENAM"),
                event == b"SCPT",
                "{}",
                String::from_utf8_lossy(event)
            );
        }
    }

    #[test]
    fn pre_translate_player_connect_and_standing_radio_autostart_fallback() {
        let interner = StringInterner::new();
        let sge = QUST_DNAM_FLAG_START_GAME_ENABLED;
        // Vanilla standing stations pair both bits (DN067_Radio 0x0011,
        // DN125_Radio 0x0111); StartGameEnabled alone never enables the
        // transmitter alias.
        let station_bits = sge | QUST_DNAM_FLAG_STARTS_ENABLED;
        let quest = |form_id: u32, eid: &'static str, flags: u64, enam: &'static [u8; 4]| QustCase {
            form_id,
            eid: Some(eid),
            flags,
            enam: Some(enam),
            ..QUST_CASE
        };
        let mut cases = vec![
            // FF06_Feed's transmitter is the event's distress beacon; starting it
            // at load blocks the public-event scheduler.
            (
                QustCase {
                    quest_type: FO76_QUST_TYPE_PUBLIC_EVENT,
                    ..quest(0x0009_210E, "FF06_Feed", 0x0315_8500, b"SCPT")
                },
                false,
                station_bits,
                0,
            ),
            (
                QustCase {
                    plugin: Some("UnsafeCopy.esm"),
                    ..quest(0x0072_A2A7, "Storm_MQ01_Breadcrumb_OnConnect", 0, b"PCON")
                },
                false,
                sge,
                0,
            ),
        ];
        for (form_id, eid) in [
            (0x005E_AD3B, "BS01_MQ00_Breadcrumb_OnConnect"),
            (0x0072_A2A7, "Storm_MQ01_Breadcrumb_OnConnect"),
            (0x007F_79A9, "BURN_SQ01_OnConnect"),
        ] {
            let both = sge | QUST_DNAM_FLAG_RUN_ONCE;
            cases.push((quest(form_id, eid, 0x8500, b"PCON"), true, both, both));
        }
        // Storm_MQ01_Breadcrumb_Radio: FO76 started it from a server story event
        // FO4 cannot fire. W05_MQ_101P_Radio fills its transmitter by
        // location-ref-type, not a forced ref.
        for (form_id, eid) in [
            (0x0069_9466, "Storm_MQ01_Breadcrumb_Radio"),
            (0x003F_BBB3, "W05_MQ_101P_Radio"),
            (0x0005_2DBF, "SFM04_Organic_Radio"),
            (0x0001_8ECF, "SFL02_Track_RadioQuest"),
        ] {
            cases.push((quest(form_id, eid, 0x0690_8500, b"SCPT"), true, station_bits, station_bits));
        }
        // BS00_Maxson*/BS00_Paladin* share one transmitter per five quests.
        for (form_id, eid) in [
            (0x005A_DC25, "BS00_MaxsonRadioQuest_01"),
            (0x005B_2BA1, "BS00_MaxsonRadioQuest_05"),
            (0x005A_DC24, "BS00_PaladinRadioQuest_01"),
            (0x005B_2BA6, "BS00_PaladinRadioQuest_05"),
            (0x0069_9466, "Storm_MQ01_Breadcrumb_Radio_Copy"),
            (0x0069_9467, "Storm_MQ01_Breadcrumb_Radio"),
        ] {
            cases.push((quest(form_id, eid, 0, b"SCPT"), false, sge, 0));
        }
        for (form_id, eid) in [
            (0x005E_AD3C, "BS01_MQ00_Breadcrumb_OnConnect"),
            (0x005E_AD3B, "BS01_MQ00_Breadcrumb_OnConnect_Copy"),
            (0x0072_A2A8, "Storm_MQ01_Breadcrumb_OnConnect"),
            (0x0072_A2A7, "Storm_MQ01_Breadcrumb_OnConnect_Copy"),
            (0x0072_A2A7, "SQ_OtherPlayerConnect"),
            (0x007F_79AA, "BURN_SQ01_OnConnect"),
            (0x007F_79A9, "BURN_SQ01_OnConnect_Copy"),
        ] {
            cases.push((quest(form_id, eid, 0, b"PCON"), false, sge, 0));
        }

        for (case, fallback, mask, expected) in cases {
            let mut record = qust_case_record(&interner, case);
            assert_eq!(
                qust_uses_player_connect_autostart_fallback(&interner, &record),
                fallback,
                "{case:?}"
            );
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
            assert_eq!(translated_qust_dnam(&record).0 & mask, expected, "{case:?}");
            if fallback && case.enam == Some(b"PCON") {
                assert!(
                    record.fields.iter().all(|field| field.sig.as_str() != "ENAM"),
                    "{case:?}"
                );
            }
        }
    }

    #[test]
    fn pre_translate_keeps_qust_objective_targets_and_alias_chain() {
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
        assert_eq!(
            sigs,
            vec![
                "EDID", "CTDA", "INDX", "QSDT", "CTDA", "QOBJ", "FNAM", "NNAM", "QSTA", "CTDA", "CIS1",
                "CIS2", "QSTA", "QOBJ", "ANAM", "ALST",
            ]
        );
    }

    #[test]
    fn full_pipeline_preserves_qust_objective_target_and_conditions() {
        use crate::source_read::decode_record_from_parsed_relayout;
        use crate::struct_relayout::StructRelayoutCtx;
        use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
        use smol_str::SmolStr;

        let interner = StringInterner::new();
        let mut qsta = Vec::new();
        qsta.extend_from_slice(&3_i32.to_le_bytes());
        qsta.extend_from_slice(&512_u16.to_le_bytes());
        qsta.extend_from_slice(&0x0000_1234_u32.to_le_bytes());
        qsta.extend_from_slice(&3000_u32.to_le_bytes());
        let mut ctda = vec![0u8; 32];
        ctda[8..10].copy_from_slice(&300_u16.to_le_bytes());
        let fo76 = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let fo4 = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let fk = FormKey {
            local: 0x0000_1234,
            plugin: interner.intern("SeventySix.esm"),
        };
        let mk = |sig: &str, data: Vec<u8>| ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: bytes::Bytes::from(data),
            semantic_type: None,
        };
        let raw = ParsedRecord {
            signature: SmolStr::new("QUST"),
            form_id: fk.local,
            flags: 0,
            version_control: 0,
            form_version: Some(208),
            version2: None,
            subrecords: vec![
                mk("EDID", b"QSTARegression\0".to_vec()),
                mk("QOBJ", 10_u16.to_le_bytes().to_vec()),
                mk("QSTA", qsta),
                mk("CTDA", ctda),
                mk("CIS1", b"first\0".to_vec()),
                mk("CIS2", b"second\0".to_vec()),
                mk("ANAM", 5_u32.to_le_bytes().to_vec()),
                mk("ALST", 7_u32.to_le_bytes().to_vec()),
            ],
            raw_payload: None,
            parse_error: None,
        };
        let relayout_ctx = StructRelayoutCtx {
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
            Some(&relayout_ctx),
        )
        .expect("decode");

        let translator = Translator::new(Game::Fo76, Game::Fo4).expect("translator");
        translator
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("pre_translate");
        let translated = match translator.translate(&record, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated QUST, got {other:?}"),
        };

        let normalizer = crate::target_normalize::TargetRecordNormalizer {
            target_schema: &fo4,
            source_record_def: fo76.record_def("QUST"),
            interner: Some(&interner),
        };
        let crate::target_normalize::TargetRecordNormalization::Keep(record) =
            normalizer.normalize(translated)
        else {
            panic!("QUST should be supported");
        };

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec!["EDID", "QOBJ", "QSTA", "CTDA", "CIS1", "CIS2", "ANAM", "ALST"]
        );
        let qsta = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "QSTA")
            .expect("QSTA");
        let FieldValue::Bytes(qsta) = &qsta.value else {
            panic!("QSTA should be bytes");
        };
        assert_eq!(qsta.len(), 12);
        assert_eq!(i32::from_le_bytes(qsta[0..4].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(qsta[4..8].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(qsta[8..12].try_into().unwrap()),
            0x0000_1234
        );
    }

    #[test]
    fn full_pipeline_preserves_mq_overseer_target_conditions_and_alias_vmad() {
        use crate::source_read::decode_record_from_parsed_relayout;
        use crate::struct_relayout::StructRelayoutCtx;
        use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
        use smol_str::SmolStr;

        const TARGETS: [(i32, u16, u32, u32); 17] = [
            (75, 0, 0x0013_894C, 18),
            (4, 0, 0x004E_62C7, 20),
            (6, 0, 0x004E_62C8, 30),
            (8, 0, 0x004E_62C9, 40),
            (10, 0, 0x004E_62CA, 50),
            (11, 0, 0x004E_62CB, 60),
            (12, 0, 0x004E_62CC, 70),
            (13, 0, 0x004E_62CD, 80),
            (14, 512, 0x004E_62CE, 90),
            (15, 0, 0x004E_62CF, 100),
            (38, 0, 0x004E_62D0, 110),
            (42, 0, 0x004E_C63E, 500),
            (43, 0, 0x004E_C63F, 510),
            (44, 0, 0x004E_C640, 520),
            (51, 0, 0x0052_8084, 530),
            (53, 0, 0x0052_808A, 540),
            (73, 0, 0x003D_113E, 120),
        ];

        fn target_bytes(alias: i32, flags: u16) -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&alias.to_le_bytes());
            bytes.extend_from_slice(&flags.to_le_bytes());
            bytes.extend_from_slice(&0_u32.to_le_bytes());
            bytes.extend_from_slice(&0.0_f32.to_le_bytes());
            bytes
        }

        fn global_condition(global: u32) -> Vec<u8> {
            let mut bytes = vec![0_u8; 32];
            bytes[8..10].copy_from_slice(&14_u16.to_le_bytes());
            bytes[12..16].copy_from_slice(&global.to_le_bytes());
            bytes[20..24].copy_from_slice(&5_u32.to_le_bytes());
            bytes[28..32].copy_from_slice(&2_u32.to_le_bytes());
            bytes
        }

        fn stage_condition(stage: u32) -> Vec<u8> {
            let mut bytes = vec![0_u8; 32];
            bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
            bytes[8..10].copy_from_slice(&59_u16.to_le_bytes());
            bytes[12..16].copy_from_slice(&stage.to_le_bytes());
            bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
            bytes
        }

        let interner = StringInterner::new();
        let fo76 = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let fo4 = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let fk = FormKey {
            local: 0x004E_49D9,
            plugin: interner.intern("SeventySix.esm"),
        };
        let vmad = qust_vmad_fixture(&[(2, &["MQ_OverseerPlayerScript"])]).bytes;
        let mk = |sig: &str, data: Vec<u8>| ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: bytes::Bytes::from(data),
            semantic_type: None,
        };
        let mut subrecords = vec![
            mk("VMAD", vmad.clone()),
            mk("EDID", b"MQ_Overseer\0".to_vec()),
            mk("INDX", 9000_u16.to_le_bytes().to_vec()),
            mk("QSDT", vec![0]),
            mk("CTDA", global_condition(TARGETS[0].2)),
            mk("CTDA", stage_condition(TARGETS[0].3)),
            mk("QOBJ", 10_u16.to_le_bytes().to_vec()),
        ];
        for (alias, flags, global, stage) in TARGETS {
            subrecords.push(mk("QSTA", target_bytes(alias, flags)));
            subrecords.push(mk("CTDA", global_condition(global)));
            subrecords.push(mk("CTDA", stage_condition(stage)));
        }
        subrecords.push(mk("QOBJ", 1000_u16.to_le_bytes().to_vec()));
        subrecords.push(mk("ANAM", 80_u32.to_le_bytes().to_vec()));
        let raw = ParsedRecord {
            signature: SmolStr::new("QUST"),
            form_id: fk.local,
            flags: 0,
            version_control: 0,
            form_version: Some(208),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        };
        let relayout_ctx = StructRelayoutCtx {
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
            Some(&relayout_ctx),
        )
        .expect("decode MQ_Overseer");
        let translator = Translator::new(Game::Fo76, Game::Fo4).expect("translator");
        translator
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("pre-translate MQ_Overseer");
        let translated = match translator.translate(&record, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated MQ_Overseer, got {other:?}"),
        };
        let normalizer = crate::target_normalize::TargetRecordNormalizer {
            target_schema: &fo4,
            source_record_def: fo76.record_def("QUST"),
            interner: Some(&interner),
        };
        let crate::target_normalize::TargetRecordNormalization::Keep(record) =
            normalizer.normalize(translated)
        else {
            panic!("MQ_Overseer should be supported");
        };

        let objective_start = record
            .fields
            .iter()
            .position(|field| {
                field.sig.as_str() == "QOBJ"
                    && matches!(
                        &field.value,
                        FieldValue::Bytes(bytes)
                            if bytes.len() >= 2
                                && u16::from_le_bytes([bytes[0], bytes[1]]) == 10
                    )
            })
            .expect("objective 10");
        let objective_end = record.fields[objective_start + 1..]
            .iter()
            .position(|field| field.sig.as_str() == "QOBJ")
            .map(|offset| objective_start + 1 + offset)
            .expect("following objective");
        let objective = &record.fields[objective_start + 1..objective_end];
        assert_eq!(
            objective
                .iter()
                .filter(|field| field.sig.as_str() == "QSTA")
                .count(),
            17,
        );
        assert_eq!(
            objective
                .iter()
                .filter(|field| field.sig.as_str() == "CTDA")
                .count(),
            34,
        );
        let FieldValue::Bytes(converted_vmad) = &record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "VMAD")
            .expect("MQ_Overseer VMAD")
            .value
        else {
            panic!("VMAD should remain opaque bytes");
        };
        assert_eq!(converted_vmad.as_slice(), vmad.as_slice());
        assert!(converted_vmad
            .windows(b"MQ_OverseerPlayerScript".len())
            .any(|window| window == b"MQ_OverseerPlayerScript"));
    }

    #[test]
    fn full_pipeline_preserves_w05_mq_102p_completion_and_cleanup_stage_flags() {
        use crate::source_read::decode_record_from_parsed_relayout;
        use crate::struct_relayout::StructRelayoutCtx;
        use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
        use smol_str::SmolStr;

        let fo76 = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let fo4 = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let quest = FormKey {
            local: 0x003F_FACF,
            plugin: interner.intern("SeventySix.esm"),
        };
        let reward = 0x0059_18DE_u32;
        let vmad = match qust_vmad_with_top_level_scripts(&["B21:QuestRewards"]) {
            FieldValue::Bytes(bytes) => bytes.to_vec(),
            other => panic!("fixture VMAD must be bytes, got {other:?}"),
        };
        let stage = |index: u16, flags: u8| {
            let mut bytes = index.to_le_bytes().to_vec();
            bytes.extend_from_slice(&[flags, 0]);
            bytes
        };
        let subrecord = |signature: &str, data: Vec<u8>| ParsedSubrecord {
            signature: SmolStr::new(signature),
            data: bytes::Bytes::from(data),
            semantic_type: None,
        };
        let raw = ParsedRecord {
            signature: SmolStr::new("QUST"),
            form_id: quest.local,
            flags: 0,
            version_control: 3_486_857,
            form_version: Some(208),
            version2: Some(1),
            subrecords: vec![
                subrecord("EDID", b"W05_MQ_102P\0".to_vec()),
                subrecord("VMAD", vmad.clone()),
                subrecord("XNAM", reward.to_le_bytes().to_vec()),
                subrecord("INDX", stage(9000, 0)),
                subrecord("QSDT", vec![0x01]),
                subrecord("INDX", stage(10000, 0x04)),
                subrecord("QSDT", vec![0x00]),
            ],
            raw_payload: None,
            parse_error: None,
        };
        let relayout_ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: false,
        };
        let mut record = decode_record_from_parsed_relayout(
            &raw,
            &quest,
            &fo76,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
            Some(&relayout_ctx),
        )
        .expect("decode W05_MQ_102P fixture");

        let translator = Translator::new(Game::Fo76, Game::Fo4).expect("translator");
        translator
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("pre_translate");
        let translated = match translator.translate(&record, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated QUST, got {other:?}"),
        };
        let normalizer = crate::target_normalize::TargetRecordNormalizer {
            target_schema: &fo4,
            source_record_def: fo76.record_def("QUST"),
            interner: Some(&interner),
        };
        let crate::target_normalize::TargetRecordNormalization::Keep(record) =
            normalizer.normalize(translated)
        else {
            panic!("W05_MQ_102P should be supported");
        };

        let mut current_stage = None;
        let mut stages = Vec::new();
        for field in &record.fields {
            let encoded =
                crate::target_write::encode_field_pub(field, fo4.record_def("QUST"), &interner)
                    .expect("encode translated QUST field");
            match field.sig.as_str() {
                "VMAD" => assert_eq!(encoded, vmad, "reward VMAD changed"),
                "XNAM" => assert_eq!(
                    encoded,
                    reward.to_le_bytes(),
                    "completion reward XNAM changed"
                ),
                "INDX" => {
                    assert_eq!(encoded.len(), 4);
                    current_stage = Some((
                        u16::from_le_bytes(encoded[0..2].try_into().unwrap()),
                        encoded[2],
                    ));
                }
                "QSDT" => {
                    let (index, flags) = current_stage.expect("QSDT after INDX");
                    stages.push((index, flags, encoded[0]));
                }
                _ => {}
            }
        }

        assert_eq!(stages, vec![(9000, 0x00, 0x01), (10000, 0x04, 0x00)]);
    }

    #[test]
    fn full_pipeline_preserves_w05_mqr_204_lou_alias_faction() {
        use crate::source_read::decode_record_from_parsed_relayout;
        use crate::struct_relayout::StructRelayoutCtx;
        use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
        use smol_str::SmolStr;

        let fo76 = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let fo4 = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let mut interner = StringInterner::new();
        let source_quest = FormKey {
            local: 0x0053_5E55,
            plugin: interner.intern("SeventySix.esm"),
        };
        let source_faction_bytes = [0x10, 0x86, 0x05, 0x00];
        let source_faction = FormKey {
            local: u32::from_le_bytes(source_faction_bytes),
            plugin: interner.intern("SeventySix.esm"),
        };
        let mk = |sig: &str, data: Vec<u8>| ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: bytes::Bytes::from(data),
            semantic_type: None,
        };
        let raw = ParsedRecord {
            signature: SmolStr::new("QUST"),
            form_id: source_quest.local,
            flags: 0,
            version_control: 3_486_857,
            form_version: Some(208),
            version2: Some(1),
            subrecords: vec![
                mk("EDID", b"W05_MQR_204P\0".to_vec()),
                mk("ANAM", 43_u32.to_le_bytes().to_vec()),
                mk("ALST", 9_u32.to_le_bytes().to_vec()),
                mk("ALID", b"Lou\0".to_vec()),
                mk("ALFC", source_faction_bytes.to_vec()),
                mk("ALED", Vec::new()),
            ],
            raw_payload: None,
            parse_error: None,
        };
        let relayout_ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
            legacy_bptd_only: false,
        };
        let mut record = decode_record_from_parsed_relayout(
            &raw,
            &source_quest,
            &fo76,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
            Some(&relayout_ctx),
        )
        .expect("decode");
        assert!(record.fields.iter().any(|field| {
            field.sig.as_str() == "ALFC" && field.value == FieldValue::FormKey(source_faction)
        }));

        let translator = Translator::new(Game::Fo76, Game::Fo4).expect("translator");
        translator
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("pre_translate");
        let translated = match translator.translate(&record, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated QUST, got {other:?}"),
        };
        let normalizer = crate::target_normalize::TargetRecordNormalizer {
            target_schema: &fo4,
            source_record_def: fo76.record_def("QUST"),
            interner: Some(&interner),
        };
        let crate::target_normalize::TargetRecordNormalization::Keep(mut record) =
            normalizer.normalize(translated)
        else {
            panic!("QUST should be supported");
        };
        let target_faction = FormKey {
            local: source_faction.local,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_faction, target_faction);
        mapper
            .rewrite_record(&mut record)
            .expect("rewrite FormKeys");
        drop(mapper);
        let faction = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ALFC")
            .expect("Lou alias faction");
        assert_eq!(faction.value, FieldValue::FormKey(target_faction));
        assert_eq!(
            crate::target_write::encode_field_pub(faction, fo4.record_def("QUST"), &interner)
                .expect("encode ALFC"),
            source_faction_bytes
        );
    }


    #[test]
    fn pre_translate_drops_unscoped_and_invalid_qust_alias_factions() {
        let interner = StringInterner::new();
        let faction = form_key_value(&interner, 0x0005_8610);
        for (label, fields) in [
            ("unscoped", vec![("EDID", FieldValue::None), ("ALFC", faction.clone())]),
            (
                "after ALED",
                vec![
                    ("ANAM", FieldValue::Uint(10)),
                    ("ALST", FieldValue::Uint(9)),
                    ("ALED", FieldValue::None),
                    ("ALFC", faction.clone()),
                ],
            ),
            (
                "ALLS row",
                vec![
                    ("ANAM", FieldValue::Uint(10)),
                    ("ALLS", FieldValue::Uint(9)),
                    ("ALFC", faction.clone()),
                    ("ALED", FieldValue::None),
                ],
            ),
            (
                "ALCS row",
                vec![
                    ("ANAM", FieldValue::Uint(10)),
                    ("ALCS", FieldValue::Uint(9)),
                    ("ALFC", faction.clone()),
                    ("ALED", FieldValue::None),
                ],
            ),
            (
                "high-byte-only form id",
                vec![
                    ("ANAM", FieldValue::Uint(10)),
                    ("ALST", FieldValue::Uint(9)),
                    ("ALFC", form_key_value(&interner, 0x0100_0000)),
                    ("ALED", FieldValue::None),
                ],
            ),
        ] {
            let mut record = make_record("QUST", &interner);
            for (sig, value) in fields {
                push_field(&mut record, sig, value);
            }
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .expect("pre_translate");
            assert!(
                !record.fields.iter().any(|field| field.sig.as_str() == "ALFC"),
                "{label}"
            );
        }
    }

    // Fallout4.esm never authors a null ALFR (0 of 11,573 aliases); carried
    // through, the always-failing forced fill aborts quest start (EN02_MQ_Us
    // alias 55 then never protects MODUSHolotape).
    #[test]
    fn pre_translate_drops_only_null_qust_alias_forced_references() {
        let interner = StringInterner::new();
        let fnam = |flags: u32| FieldValue::Bytes(SmallVec::from_vec(flags.to_le_bytes().to_vec()));
        for (label, fields, null_alfr) in [
            (
                "null ALFR",
                vec![
                    ("ANAM", FieldValue::Uint(56)),
                    ("ALST", FieldValue::Uint(55)),
                    ("ALID", raw_bytes(b"ResourceDropContainerActual\0")),
                    ("FNAM", fnam(0x0000_0A80)),
                    ("ALFR", form_key_value(&interner, 0)),
                    ("ALED", FieldValue::None),
                ],
                true,
            ),
            (
                "populated ALFR with quest object flag",
                vec![
                    ("ANAM", FieldValue::Uint(41)),
                    ("ALST", FieldValue::Uint(40)),
                    ("ALID", raw_bytes(b"WhitespringHolotape\0")),
                    ("FNAM", fnam(0x0000_0006)),
                    ("ALFR", form_key_value(&interner, 0x0028_3EC8)),
                    ("ALED", FieldValue::None),
                ],
                false,
            ),
            (
                "empty ALFR beside a live ALFA/ALRT fill",
                vec![
                    ("ANAM", FieldValue::Uint(5)),
                    ("ALST", FieldValue::Uint(4)),
                    ("ALID", raw_bytes(b"HolotapeQuestTarget02\0")),
                    ("FNAM", fnam(0x0000_0002)),
                    ("ALFR", FieldValue::None),
                    ("ALFA", FieldValue::Uint(5)),
                    ("ALRT", form_key_value(&interner, 0x004E_49E6)),
                    ("ALED", FieldValue::None),
                ],
                true,
            ),
        ] {
            let mut record = make_record("QUST", &interner);
            push_field(&mut record, "EDID", FieldValue::None);
            for (sig, value) in fields {
                push_field(&mut record, sig, value);
            }
            let expected: Vec<FieldEntry> = record
                .fields
                .iter()
                .filter(|entry| !(null_alfr && entry.sig.as_str() == "ALFR"))
                .cloned()
                .collect();
            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .expect("pre_translate");
            assert_eq!(record.fields.to_vec(), expected, "{label}");
        }
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

    fn mile_caravan_intro_record(interner: &StringInterner) -> Record {
        let mut record = make_record("QUST", interner);
        record.form_key.local = MILE_CARAVAN_INTRO_QUEST_FORM_ID;
        record.eid = Some(interner.intern("MILE_CaravanIntro"));
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("MILE_CaravanIntro")),
        );
        push_field(
            &mut record,
            "VMAD",
            qust_vmad_with_alias_scripts(&[(
                MILE_CARAVAN_CARGO_ALIAS_ID as i16,
                &["MILECaravanCargoAliasScript"],
            )]),
        );
        push_field(&mut record, "INDX", raw_bytes(&4500_u16.to_le_bytes()));
        push_field(&mut record, "QSDT", raw_bytes(&[0x01]));
        push_field(&mut record, "ANAM", FieldValue::Uint(13));

        push_field(&mut record, "ALST", FieldValue::Uint(10));
        push_field(&mut record, "ALID", raw_bytes(b"DeadBrahmin\0"));
        push_field(
            &mut record,
            "FNAM",
            raw_bytes(&0x0000_0A80_u32.to_le_bytes()),
        );
        push_field(&mut record, "ALFR", form_key_value(interner, 0x0076_B13B));
        push_field(&mut record, "CTDA", raw_bytes(&[0x11; 32]));
        push_field(&mut record, "ALED", FieldValue::None);

        push_field(&mut record, "ALST", FieldValue::Uint(11));
        push_field(&mut record, "ALID", raw_bytes(b"Cargo\0"));
        push_field(
            &mut record,
            "FNAM",
            raw_bytes(&0x0000_0104_u32.to_le_bytes()),
        );
        push_field(
            &mut record,
            "ALFR",
            form_key_value(interner, MILE_CARAVAN_BAD_CARGO_REF_FORM_ID),
        );
        push_field(&mut record, "ALED", FieldValue::None);

        push_field(&mut record, "ALST", FieldValue::Uint(12));
        push_field(&mut record, "ALID", raw_bytes(b"CaravanLeader\0"));
        push_field(
            &mut record,
            "FNAM",
            raw_bytes(&0x0000_0002_u32.to_le_bytes()),
        );
        push_field(&mut record, "ALFR", form_key_value(interner, 0x0076_B120));
        push_field(&mut record, "ALED", FieldValue::None);
        record
    }

    fn qust_alias_row(record: &Record, alias_id: u32) -> &[FieldEntry] {
        let start = record
            .fields
            .iter()
            .position(|entry| {
                entry.sig.0 == *b"ALST" && field_value_to_u32(&entry.value) == Some(alias_id)
            })
            .expect("alias anchor");
        let end = record.fields[start..]
            .iter()
            .position(|entry| entry.sig.0 == *b"ALED")
            .map(|offset| start + offset + 1)
            .expect("alias end");
        &record.fields[start..end]
    }

    fn nuke_edge_marker_record(interner: &StringInterner) -> Record {
        let mut record = make_record("QUST", interner);
        record.form_key.local = 0x002D_0F69;
        record.eid = Some(interner.intern("EN07_MQ_FleeBlast"));
        push_field(&mut record, "ANAM", FieldValue::Uint(52));
        push_field(&mut record, "ALCS", FieldValue::Uint(13));
        push_field(&mut record, "ALMI", FieldValue::Uint(0));
        push_field(&mut record, "ALST", FieldValue::Uint(13));
        push_field(&mut record, "ALID", raw_bytes(b"AffectedEdgeMarkers\0"));
        push_field(&mut record, "FNAM", raw_bytes(&0x128A_u32.to_le_bytes()));
        push_field(&mut record, "ALCC", FieldValue::Uint(0));
        push_field(
            &mut record,
            "CTDA",
            raw_bytes(&hex::decode("A600000060112D000100000000000000000000000000000000000000FFFFFFFF").unwrap()),
        );
        push_field(&mut record, "ALDN", form_key_value(interner, 0));
        push_field(&mut record, "ALFF", form_key_value(interner, 0x002D_116A));
        push_field(&mut record, "ALED", FieldValue::None);
        push_field(&mut record, "ALST", FieldValue::Uint(14));
        push_field(&mut record, "ALID", raw_bytes(b"LaunchingPlayer\0"));
        push_field(&mut record, "ALFR", form_key_value(interner, 0x14));
        push_field(&mut record, "ALED", FieldValue::None);
        record
    }

    #[test]
    fn nuke_edge_marker_filter_survives_translation_without_widening_alias_fill() {
        let interner = StringInterner::new();
        let mut record = nuke_edge_marker_record(&interner);
        let before = qust_alias_row(&record, 13).to_vec();
        let neighbor = qust_alias_row(&record, 14).to_vec();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        translator.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
        let alias = qust_alias_row(&record, 13);
        let filter_index = alias.iter().position(|entry| entry.sig.0 == *b"CTDA").unwrap();
        assert_eq!(
            alias.iter().enumerate().filter(|(index, _)| *index != filter_index).map(|(_, entry)| entry.clone()).collect::<Vec<_>>(),
            before.into_iter().filter(|entry| entry.sig.0 != *b"ALFF").collect::<Vec<_>>(),
        );
        assert_eq!(qust_alias_row(&record, 14), neighbor);
        let once = record.fields.clone();
        translator.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
        assert_eq!(record.fields, once, "repair must be idempotent");
        let TranslateResult::Translated(record) = translator.translate(&record, &interner) else {
            panic!("nuke quest must translate");
        };
        let fo76 = AuthoringSchema::for_game("fo76").unwrap();
        let fo4 = AuthoringSchema::for_game("fo4").unwrap();
        let normalizer = crate::target_normalize::TargetRecordNormalizer {
            target_schema: &fo4,
            source_record_def: fo76.record_def("QUST"),
            interner: Some(&interner),
        };
        let crate::target_normalize::TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("nuke quest must survive normalization");
        };
        let alias = qust_alias_row(&record, 13);
        let conditions: Vec<_> = alias.iter().filter(|entry| entry.sig.0 == *b"CTDA").collect();
        assert_eq!(conditions.len(), 2, "distance alone captures unrelated transmitters");
        let FieldValue::Bytes(filter) = &conditions[0].value else { panic!("raw CTDA"); };
        assert_eq!(filter.len(), 32);
        assert_eq!(filter[0], 0, "AND, equal, subject; no alias/global flags");
        assert_eq!(f32::from_le_bytes(filter[4..8].try_into().unwrap()), 1.0);
        assert_eq!(u16::from_le_bytes(filter[8..10].try_into().unwrap()), 561);
        assert_eq!(u32::from_le_bytes(filter[12..16].try_into().unwrap()), 0x002D_116A);
        assert_eq!(&filter[16..28], &[0; 12]);
        assert_eq!(u32::from_le_bytes(filter[28..32].try_into().unwrap()), u32::MAX);
        assert_eq!(conditions[1].value, raw_bytes(&hex::decode("A600000060112D000100000000000000000000000000000000000000FFFFFFFF").unwrap()));

        for mismatch in 0..4 {
            let mut record = nuke_edge_marker_record(&interner);
            match mismatch {
                0 => record.form_key.local += 1,
                1 => record.form_key.plugin = interner.intern("Other.esm"),
                2 => record.eid = Some(interner.intern("EN07_MQ_FleeBlast_Copy")),
                _ => record.fields.iter_mut().find(|entry| entry.sig.0 == *b"ALFF").unwrap().value = form_key_value(&interner, 0x002D_116B),
            }
            Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
            assert_eq!(qust_alias_row(&record, 13).iter().filter(|entry| entry.sig.0 == *b"CTDA").count(), 1);
        }
    }

    fn tw043_record(interner: &StringInterner) -> Record {
        let mut record = make_record("QUST", interner);
        record.form_key.local = TW043_FORM_ID;
        record.eid = Some(interner.intern("TW043"));
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("TW043")),
        );
        push_field(&mut record, "ANAM", FieldValue::Uint(8));

        push_field(&mut record, "ALST", FieldValue::Uint(TW043_GUARD_ALIAS_ID.into()));
        push_field(&mut record, "ALID", raw_bytes(b"TW043Guard\0"));
        push_field(
            &mut record,
            "ALCO",
            form_key_value(interner, TW043_GUARD_BASE_FORM_ID),
        );
        let mut create_in_pod = Vec::new();
        create_in_pod.extend_from_slice(&(TW043_GUARD_POD_ALIAS_ID as i16).to_le_bytes());
        create_in_pod.extend_from_slice(&QUST_CREATE_REFERENCE_IN_ALIAS.to_le_bytes());
        push_field(&mut record, "ALCA", raw_bytes(&create_in_pod));
        push_field(
            &mut record,
            "ALCL",
            FieldValue::Uint(QUST_CREATE_REFERENCE_DEFAULT_LEVEL),
        );
        let mut unsafe_link = Vec::new();
        unsafe_link.extend_from_slice(&LINK_PROTECTRON_POD_FORM_ID.to_le_bytes());
        unsafe_link.extend_from_slice(&TW043_GUARD_POD_ALIAS_ID.to_le_bytes());
        push_field(&mut record, "ALLA", raw_bytes(&unsafe_link));
        push_field(&mut record, "ALED", FieldValue::None);

        push_field(&mut record, "ALST", FieldValue::Uint(3));
        push_field(&mut record, "ALID", raw_bytes(b"AdditionalGuards\0"));
        let mut unrelated_link = Vec::new();
        unrelated_link.extend_from_slice(&0x003C_BFF8_u32.to_le_bytes());
        unrelated_link.extend_from_slice(&0_i32.to_le_bytes());
        push_field(&mut record, "ALLA", raw_bytes(&unrelated_link));
        push_field(&mut record, "ALED", FieldValue::None);
        record
    }

    #[test]
    fn pre_translate_removes_only_tw043_guard_startup_link() {
        let interner = StringInterner::new();
        let mut record = tw043_record(&interner);
        let guard_before = qust_alias_row(&record, TW043_GUARD_ALIAS_ID).to_vec();
        let additional_guards_before = qust_alias_row(&record, 3).to_vec();

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("pre_translate");

        let guard = qust_alias_row(&record, TW043_GUARD_ALIAS_ID);
        assert_eq!(guard.len(), guard_before.len() - 1);
        assert!(!guard.iter().any(|entry| entry.sig.0 == *b"ALLA"));
        assert_eq!(
            guard,
            guard_before
                .iter()
                .filter(|entry| entry.sig.0 != *b"ALLA")
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(qust_alias_row(&record, 3), additional_guards_before);

        let after_first_pass = record.fields.clone();
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("second pre_translate");
        assert_eq!(record.fields, after_first_pass);

        let mut wrong_plugin = tw043_record(&interner);
        wrong_plugin.form_key.plugin = interner.intern("UnsafeCopy.esm");
        let before = wrong_plugin.fields.clone();
        Fo76Fo4Hook::remove_tw043_guard_startup_link(&interner, &mut wrong_plugin);
        assert_eq!(wrong_plugin.fields, before, "wrong plugin");

        let mut wrong_link = tw043_record(&interner);
        let link = wrong_link
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"ALLA")
            .expect("guard ALLA");
        let FieldValue::Bytes(payload) = &mut link.value else {
            panic!("ALLA must be raw bytes");
        };
        payload[0..4].copy_from_slice(&(LINK_PROTECTRON_POD_FORM_ID + 1).to_le_bytes());
        let before = wrong_link.fields.clone();
        Fo76Fo4Hook::remove_tw043_guard_startup_link(&interner, &mut wrong_link);
        assert_eq!(wrong_link.fields, before, "wrong linked-ref keyword");
    }

    #[test]
    fn pre_translate_repairs_mile_caravan_intro_cargo_alias_create_reference_fill() {
        let interner = StringInterner::new();
        let mut record = mile_caravan_intro_record(&interner);
        let vmad_before = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"VMAD")
            .expect("VMAD")
            .clone();
        let stage_before: Vec<FieldEntry> = record.fields[2..4].to_vec();
        let dead_brahmin_before = qust_alias_row(&record, 10).to_vec();
        let caravan_leader_before = qust_alias_row(&record, 12).to_vec();

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("pre_translate");

        let cargo = qust_alias_row(&record, MILE_CARAVAN_CARGO_ALIAS_ID);
        assert_eq!(
            cargo
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["ALST", "ALID", "FNAM", "ALCO", "ALCA", "ALCL", "ALED"]
        );
        assert!(matches!(
            &cargo[3].value,
            FieldValue::FormKey(form_key)
                if form_key.local == MILE_CARAVAN_CARGO_MISC_FORM_ID
                    && form_key.plugin == record.form_key.plugin
        ));
        let FieldValue::Bytes(alca) = &cargo[4].value else {
            panic!("ALCA must be raw bytes");
        };
        assert_eq!(
            i16::from_le_bytes(alca[0..2].try_into().unwrap()),
            MILE_CARAVAN_DEAD_BRAHMIN_ALIAS_ID
        );
        assert_eq!(
            u16::from_le_bytes(alca[2..4].try_into().unwrap()),
            QUST_CREATE_REFERENCE_IN_ALIAS
        );
        assert_eq!(
            cargo[5].value,
            FieldValue::Uint(QUST_CREATE_REFERENCE_DEFAULT_LEVEL)
        );
        assert_eq!(
            field_value_to_u32(&cargo[2].value),
            Some(0x0000_0104),
            "Cargo alias flags changed"
        );
        assert_eq!(qust_alias_row(&record, 10), dead_brahmin_before);
        assert_eq!(qust_alias_row(&record, 12), caravan_leader_before);
        assert_eq!(record.fields[2..4], stage_before);
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"VMAD")
                .expect("VMAD"),
            &vmad_before
        );
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"ALED")
                .count(),
            3
        );

        let fo4 = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let qust = fo4.record_def("QUST");
        assert_eq!(
            crate::target_write::encode_field_pub(&cargo[3], qust, &interner).expect("encode ALCO"),
            MILE_CARAVAN_CARGO_MISC_FORM_ID.to_le_bytes()
        );
        assert_eq!(
            crate::target_write::encode_field_pub(&cargo[4], qust, &interner).expect("encode ALCA"),
            [10, 0, 0, 0x80]
        );
        assert_eq!(
            crate::target_write::encode_field_pub(&cargo[5], qust, &interner).expect("encode ALCL"),
            [0, 0, 0, 0]
        );

        let after_first_pass = record.fields.clone();
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("second pre_translate");
        assert_eq!(record.fields, after_first_pass);

        let mut wrong_plugin = mile_caravan_intro_record(&interner);
        wrong_plugin.form_key.plugin = interner.intern("UnsafeCopy.esm");
        let before = wrong_plugin.fields.clone();
        Fo76Fo4Hook::repair_mile_caravan_intro_cargo_alias(&interner, &mut wrong_plugin);
        assert_eq!(wrong_plugin.fields, before, "wrong plugin");

        let mut wrong_quest = mile_caravan_intro_record(&interner);
        wrong_quest.form_key.local += 1;
        let before = wrong_quest.fields.clone();
        Fo76Fo4Hook::repair_mile_caravan_intro_cargo_alias(&interner, &mut wrong_quest);
        assert_eq!(wrong_quest.fields, before, "wrong QUST ID");

        let mut wrong_eid = mile_caravan_intro_record(&interner);
        wrong_eid.eid = Some(interner.intern("MILE_CaravanIntro_Copy"));
        let before = wrong_eid.fields.clone();
        Fo76Fo4Hook::repair_mile_caravan_intro_cargo_alias(&interner, &mut wrong_eid);
        assert_eq!(wrong_eid.fields, before, "wrong EID");

        let mut wrong_alias_id = mile_caravan_intro_record(&interner);
        let alias_id = wrong_alias_id
            .fields
            .iter_mut()
            .find(|entry| {
                entry.sig.0 == *b"ALST"
                    && field_value_to_u32(&entry.value) == Some(MILE_CARAVAN_CARGO_ALIAS_ID)
            })
            .expect("Cargo alias ID");
        alias_id.value = FieldValue::Uint(13);
        let before = wrong_alias_id.fields.clone();
        Fo76Fo4Hook::repair_mile_caravan_intro_cargo_alias(&interner, &mut wrong_alias_id);
        assert_eq!(wrong_alias_id.fields, before, "wrong alias ID");

        let mut wrong_alias_name = mile_caravan_intro_record(&interner);
        let cargo_start = wrong_alias_name
            .fields
            .iter()
            .position(|entry| {
                entry.sig.0 == *b"ALST"
                    && field_value_to_u32(&entry.value) == Some(MILE_CARAVAN_CARGO_ALIAS_ID)
            })
            .expect("Cargo alias ID");
        wrong_alias_name.fields[cargo_start + 1].value = raw_bytes(b"CargoCopy\0");
        let before = wrong_alias_name.fields.clone();
        Fo76Fo4Hook::repair_mile_caravan_intro_cargo_alias(&interner, &mut wrong_alias_name);
        assert_eq!(wrong_alias_name.fields, before, "wrong alias name");

        let mut wrong_alfr = mile_caravan_intro_record(&interner);
        let cargo = qust_alias_row(&wrong_alfr, MILE_CARAVAN_CARGO_ALIAS_ID);
        let alfr_offset = cargo
            .iter()
            .position(|entry| entry.sig.0 == *b"ALFR")
            .expect("Cargo ALFR");
        let cargo_start = wrong_alfr
            .fields
            .iter()
            .position(|entry| {
                entry.sig.0 == *b"ALST"
                    && field_value_to_u32(&entry.value) == Some(MILE_CARAVAN_CARGO_ALIAS_ID)
            })
            .expect("Cargo alias ID");
        wrong_alfr.fields[cargo_start + alfr_offset].value =
            form_key_value(&interner, MILE_CARAVAN_BAD_CARGO_REF_FORM_ID + 1);
        let before = wrong_alfr.fields.clone();
        Fo76Fo4Hook::repair_mile_caravan_intro_cargo_alias(&interner, &mut wrong_alfr);
        assert_eq!(wrong_alfr.fields, before, "wrong ALFR");

        let mut record = make_record("REFR", &interner);
        record.form_key.local = MILE_CARAVAN_BAD_CARGO_REF_FORM_ID;
        push_field(&mut record, "NAME", form_key_value(&interner, 0x0003_94D5));
        let before = record.fields.clone();

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .expect("pre_translate REFR");

        assert_eq!(record.form_key.local, MILE_CARAVAN_BAD_CARGO_REF_FORM_ID);
        assert_eq!(record.fields, before);
    }
