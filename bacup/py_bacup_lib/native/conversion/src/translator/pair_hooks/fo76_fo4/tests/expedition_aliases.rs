fn make_expedition_qust(
    interner: &StringInterner,
    local_form_id: u32,
    editor_id: &str,
    anchor: &str,
    alias_id: u32,
    alias_name: &str,
    flags: u64,
    event_data: u32,
) -> Record {
    let mut record = Record::new(
        SigCode(*b"QUST"),
        FormKey {
            local: local_form_id,
            plugin: interner.intern(FO76_MASTER_NAME),
        },
    );
    push_field(
        &mut record,
        "EDID",
        FieldValue::String(interner.intern(editor_id)),
    );
    push_field(
        &mut record,
        "ANAM",
        FieldValue::Bytes(SmallVec::from_vec((alias_id + 1).to_le_bytes().to_vec())),
    );
    push_field(
        &mut record,
        anchor,
        FieldValue::Bytes(SmallVec::from_vec(alias_id.to_le_bytes().to_vec())),
    );
    push_field(
        &mut record,
        "ALID",
        FieldValue::String(interner.intern(alias_name)),
    );
    push_field(
        &mut record,
        "FNAM",
        FieldValue::Bytes(SmallVec::from_vec(flags.to_le_bytes().to_vec())),
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
        FieldValue::Bytes(SmallVec::from_vec(event_data.to_le_bytes().to_vec())),
    );
    push_field(&mut record, "ALED", FieldValue::None);
    record
}

fn forced_player_alias(interner: &StringInterner, record: &Record) -> bool {
    record.fields.iter().any(|entry| {
        entry.sig.0 == *b"ALFR"
            && matches!(
                entry.value,
                FieldValue::FormKey(form_key)
                    if form_key.local & 0x00FF_FFFF == FO4_PLAYER_REF_FORM_ID
                        && interner.resolve(form_key.plugin) == Some(FO4_MASTER_NAME)
            )
    })
}

fn expedition_alias_event_pair(record: &Record) -> Vec<([u8; 4], u32)> {
    record
        .fields
        .iter()
        .filter(|entry| matches!(&entry.sig.0, b"ALFE" | b"ALFD"))
        .map(|entry| {
            (
                entry.sig.0,
                field_value_to_u32(&entry.value).expect("u32 event value"),
            )
        })
        .collect()
}

const EXPEDITION_PLAYER_ALIAS_CASES: &[(u32, &str, u32, &str, u64)] = &[
    (
        0x006B_AA3D,
        "XPD_AC01_Mission_Tax",
        19,
        "ExpeditionLeader",
        0x18,
    ),
    (
        0x006B_231C,
        "XPD_AC02_Mission_Sensation",
        19,
        "ExpeditionLeader",
        0x18,
    ),
    (
        0x0062_74EC,
        "XPD_Pitt01_Mission",
        19,
        "ExpeditionLeader",
        0x18,
    ),
    (
        0x0064_8280,
        "XPD_Pitt02_Mission",
        19,
        "ExpeditionLeader",
        0x18,
    ),
    (
        0x0064_6E8B,
        "XPD_HubRE_TakePhoto_CameraShy",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_22F9,
        "XPD_HubRE_LostAndFound_ChessProblem",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_53B9,
        "XPD_HubRE_RepairRobot_ConversionError",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_7FC1,
        "XPD_HubRE_RepairRobot_DogExMachina",
        1,
        "Player",
        0x210,
    ),
    (
        0x0063_272C,
        "XPD_HubRE_SettleDispute_GarbageDay",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_5790,
        "XPD_HubRE_LostAndFound_HonorRoll",
        1,
        "Player",
        0x210,
    ),
    (
        0x0063_3790,
        "XPD_HubRE_TakePhoto_HotelMakeover",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_9ED9,
        "XPD_HubRE_TakePhoto_ImpostorSyndrome",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_BAD0,
        "XPD_HubRE_TakePhoto_LettersToHome",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_A88E,
        "XPD_HubRE_SettleDispute_LoversQuarrel",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_ACD7,
        "XPD_HubRE_RepairRobot_MrMakeover",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_54DF,
        "XPD_HubRE_TakePhoto_NoSurprises",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_3EB9,
        "XPD_HubRE_LostAndFound_ReadingMaterial",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_A14E,
        "XPD_HubRE_SettleDispute_ResponderRecruitment",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_5D90,
        "XPD_HubRE_RepairRobot_Fussfungle",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_7A3E,
        "XPD_HubRE_RepairRobot_Toothache",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_78A4,
        "XPD_HubRE_SettleDispute_UltimateShowdown",
        1,
        "Player",
        0x210,
    ),
    (
        0x0064_DBEF,
        "XPD_HubRE_SettleDispute_WeaponOfChoice",
        1,
        "Player",
        0x210,
    ),
    (0x006D_5082, "XPD_AC_Dialogue_Billy", 2, "Player", 0x98),
    (0x006C_3517, "XPD_AC_Dialogue_Sal", 2, "Player", 0x98),
    (
        0x006F_A1DF,
        "XPD_AC_Dialogue_MotherCharlotte",
        2,
        "Player",
        0x10,
    ),
    (
        0x006F_AA69,
        "XPD_AC_Dialogue_VeracioCruz",
        2,
        "Player",
        0x10,
    ),
];

const EXPEDITION_MODULE_LOCATION_CASES: &[(u32, &str)] = &[
    (0x0064_BC52, "XPD_Module_Assassination"),
    (0x0064_D26C, "XPD_Module_FreePrisoners"),
    (0x006B_ABB6, "XPD_Module_Race"),
    (0x0065_07B5, "XPD_Module_ProximityTracker"),
    (0x0064_DD30, "XPD_Module_GatherAndDeposit"),
    (0x0064_BC50, "XPD_Module_SolveLocks"),
    (0x0064_CD4E, "XPD_Module_RepelEnemies"),
    (0x0064_EA14, "XPD_Module_DefendNPC"),
    (0x0064_CD4D, "XPD_Module_CarryAndThrow"),
    (0x0064_C2D0, "XPD_Module_ObjectDestruction"),
];

#[test]
fn direct_start_allowlist_forces_only_the_exact_player_aliases() {
    assert_eq!(EXPEDITION_PLAYER_ALIAS_CASES.len(), 26);
    let interner = StringInterner::new();
    for &(form_id, editor_id, alias_id, alias_name, flags) in EXPEDITION_PLAYER_ALIAS_CASES {
        let mut record = make_expedition_qust(
            &interner,
            form_id,
            editor_id,
            "ALST",
            alias_id,
            alias_name,
            flags,
            FO76_QUEST_EVENT_REFERENCE3,
        );
        Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut record);

        assert!(forced_player_alias(&interner, &record), "{editor_id}");
        assert!(
            expedition_alias_event_pair(&record).is_empty(),
            "{editor_id}"
        );
        assert_eq!(qust_alias_flags(&record), vec![flags as u32], "{editor_id}");
        assert!(!qust_has_untranslatable_event_alias(&record), "{editor_id}");
    }
}

#[test]
fn direct_start_allowlist_optionalizes_exact_module_location_aliases() {
    assert_eq!(EXPEDITION_MODULE_LOCATION_CASES.len(), 10);
    let interner = StringInterner::new();
    for &(form_id, editor_id) in EXPEDITION_MODULE_LOCATION_CASES {
        let mut record = make_expedition_qust(
            &interner,
            form_id,
            editor_id,
            "ALLS",
            3,
            "ModuleLocation",
            0x0000_0020_0001_0308,
            FO4_QUEST_EVENT_LOCATION1,
        );
        Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut record);

        assert_eq!(
            qust_alias_flags(&record),
            vec![0x0001_0308 | QUST_ALIAS_OPTIONAL_FLAG],
            "{editor_id}"
        );
        assert_eq!(
            expedition_alias_event_pair(&record),
            vec![
                (*b"ALFE", FO76_QUEST_EVENT_SCPT),
                (*b"ALFD", FO4_QUEST_EVENT_LOCATION1),
            ],
            "{editor_id}"
        );
        assert!(!qust_has_untranslatable_event_alias(&record), "{editor_id}");
    }
}

#[test]
fn direct_start_alias_adaptation_is_idempotent() {
    let interner = StringInterner::new();
    let mut player = make_expedition_qust(
        &interner,
        0x006B_AA3D,
        "XPD_AC01_Mission_Tax",
        "ALST",
        19,
        "ExpeditionLeader",
        0x18,
        FO76_QUEST_EVENT_REFERENCE3,
    );
    Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut player);
    let once = format!("{:?}", player.fields);
    Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut player);
    assert_eq!(format!("{:?}", player.fields), once);

    let mut module = make_expedition_qust(
        &interner,
        0x0064_BC52,
        "XPD_Module_Assassination",
        "ALLS",
        3,
        "ModuleLocation",
        0x0000_0020_0001_0308,
        FO4_QUEST_EVENT_LOCATION1,
    );
    Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut module);
    let once = format!("{:?}", module.fields);
    Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut module);
    assert_eq!(format!("{:?}", module.fields), once);
}

#[test]
fn direct_start_alias_adaptation_rejects_near_matches() {
    let interner = StringInterner::new();
    let fixtures = [
        (
            0x006B_AA3E,
            "XPD_AC01_Mission_Tax",
            19,
            "ExpeditionLeader",
            FO76_QUEST_EVENT_REFERENCE3,
        ),
        (
            0x006B_AA3D,
            "XPD_AC01_Mission_Tax_Wrong",
            19,
            "ExpeditionLeader",
            FO76_QUEST_EVENT_REFERENCE3,
        ),
        (
            0x006B_AA3D,
            "XPD_AC01_Mission_Tax",
            18,
            "ExpeditionLeader",
            FO76_QUEST_EVENT_REFERENCE3,
        ),
        (
            0x006B_AA3D,
            "XPD_AC01_Mission_Tax",
            19,
            "Player",
            FO76_QUEST_EVENT_REFERENCE3,
        ),
        (
            0x006B_AA3D,
            "XPD_AC01_Mission_Tax",
            19,
            "ExpeditionLeader",
            FO4_QUEST_EVENT_REFERENCE1,
        ),
    ];
    for (form_id, editor_id, alias_id, alias_name, event_data) in fixtures {
        let mut record = make_expedition_qust(
            &interner, form_id, editor_id, "ALST", alias_id, alias_name, 0x18, event_data,
        );
        Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut record);
        assert!(!forced_player_alias(&interner, &record));
        assert_eq!(qust_alias_flags(&record), vec![0x18]);
        assert_eq!(expedition_alias_event_pair(&record).len(), 2);
    }

    let mut wrong_plugin = make_expedition_qust(
        &interner,
        0x006B_AA3D,
        "XPD_AC01_Mission_Tax",
        "ALST",
        19,
        "ExpeditionLeader",
        0x18,
        FO76_QUEST_EVENT_REFERENCE3,
    );
    wrong_plugin.form_key.plugin = interner.intern("Other.esm");
    Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut wrong_plugin);
    assert!(!forced_player_alias(&interner, &wrong_plugin));
    assert_eq!(expedition_alias_event_pair(&wrong_plugin).len(), 2);

    let mut wrong_event = make_expedition_qust(
        &interner,
        0x006B_AA3D,
        "XPD_AC01_Mission_Tax",
        "ALST",
        19,
        "ExpeditionLeader",
        0x18,
        FO76_QUEST_EVENT_REFERENCE3,
    );
    let event = wrong_event
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"ALFE")
        .expect("event fill");
    event.value = FieldValue::Uint(u64::from(FO76_QUEST_EVENT_SCPT - 1));
    Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut wrong_event);
    assert!(!forced_player_alias(&interner, &wrong_event));
    assert_eq!(expedition_alias_event_pair(&wrong_event).len(), 2);
}

#[test]
fn full_hook_keeps_direct_start_aliases_legal() {
    let interner = StringInterner::new();
    let mut mission = make_expedition_qust(
        &interner,
        0x0062_74EC,
        "XPD_Pitt01_Mission",
        "ALST",
        19,
        "ExpeditionLeader",
        0x18,
        FO76_QUEST_EVENT_REFERENCE3,
    );
    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut mission)
        .expect("mission pre-translation");
    assert!(forced_player_alias(&interner, &mission));
    assert!(!qust_has_untranslatable_event_alias(&mission));

    let mut module = make_expedition_qust(
        &interner,
        0x0064_C2D0,
        "XPD_Module_ObjectDestruction",
        "ALLS",
        3,
        "ModuleLocation",
        0x0000_0020_0001_0308,
        FO4_QUEST_EVENT_LOCATION1,
    );
    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut module)
        .expect("module pre-translation");
    assert_eq!(
        qust_alias_flags(&module),
        vec![0x0001_0308 | QUST_ALIAS_OPTIONAL_FLAG]
    );
    assert!(!qust_has_untranslatable_event_alias(&module));
}

#[test]
fn full_translation_preserves_the_forced_player_alias() {
    let interner = StringInterner::new();
    let mut record = make_expedition_qust(
        &interner,
        0x006B_231C,
        "XPD_AC02_Mission_Sensation",
        "ALST",
        19,
        "ExpeditionLeader",
        0x18,
        FO76_QUEST_EVENT_REFERENCE3,
    );
    let translator = Translator::new(Game::Fo76, Game::Fo4).expect("translator");
    translator
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .expect("pre-translation");
    let translated = match translator.translate(&record, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated expedition QUST, got {other:?}"),
    };

    assert!(forced_player_alias(&interner, &translated));
    assert!(!qust_has_untranslatable_event_alias(&translated));

    let mut module = make_expedition_qust(
        &interner,
        0x0064_D26C,
        "XPD_Module_FreePrisoners",
        "ALLS",
        3,
        "ModuleLocation",
        0x0000_0020_0001_0308,
        FO4_QUEST_EVENT_LOCATION1,
    );
    translator
        .pre_translate(&mut make_ctx(&interner), &mut module)
        .expect("module pre-translation");
    let translated_module = match translator.translate(&module, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated module QUST, got {other:?}"),
    };
    assert_eq!(
        qust_alias_flags(&translated_module),
        vec![0x0001_0308 | QUST_ALIAS_OPTIONAL_FLAG]
    );
    assert_eq!(
        expedition_alias_event_pair(&translated_module),
        vec![
            (*b"ALFE", FO76_QUEST_EVENT_SCPT),
            (*b"ALFD", FO4_QUEST_EVENT_LOCATION1),
        ]
    );
    assert!(!qust_has_untranslatable_event_alias(&translated_module));
}
