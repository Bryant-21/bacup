const RD01_MODULE_CASES: &[(u32, &str, u32)] = &[
    (0x0077_2A47, "RD01_Enc01_Bot", 1),
    (0x0078_F7A1, "RD01_Enc02_Drill", 0),
    (0x0078_B59E, "RD01_Enc04_EnclaveSquad", 1),
    (0x0078_8127, "RD01_Enc05_ResearchLab", 0),
    (0x0078_6D41, "RD01_Enc06_Scorchtongue", 1),
];
const RD01_MODULE_LOCATION_FNAM: u64 = 0x0000_0020_0000_0000;

fn rd01_module_qust(interner: &StringInterner, form_id: u32, editor_id: &str, alias_id: u32) -> Record {
    make_expedition_qust(
        interner,
        form_id,
        editor_id,
        "ALLS",
        alias_id,
        "ModuleLocation",
        RD01_MODULE_LOCATION_FNAM,
        FO4_QUEST_EVENT_LOCATION1,
    )
}

fn rd01_forced_location(interner: &StringInterner, record: &Record) -> Vec<(u32, String)> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"ALFL")
        .filter_map(|entry| match entry.value {
            FieldValue::FormKey(form_key) => Some((
                form_key.local,
                interner.resolve(form_key.plugin).unwrap_or_default().to_string(),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn rd01_module_location_is_forced_to_gleaming_depths() {
    let interner = StringInterner::new();
    for &(form_id, editor_id, alias_id) in RD01_MODULE_CASES {
        let mut record = rd01_module_qust(&interner, form_id, editor_id, alias_id);
        Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut record);

        assert_eq!(
            rd01_forced_location(&interner, &record),
            vec![(0x0077_0117, FO76_MASTER_NAME.to_string())],
            "{editor_id}"
        );
        assert!(expedition_alias_event_pair(&record).is_empty(), "{editor_id}");
        assert_eq!(
            qust_alias_flags(&record),
            vec![RD01_MODULE_LOCATION_FNAM as u32],
            "{editor_id} must stay non-optional"
        );
        assert!(!qust_has_untranslatable_event_alias(&record), "{editor_id}");

        let once = format!("{:?}", record.fields);
        Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut record);
        assert_eq!(format!("{:?}", record.fields), once, "{editor_id} idempotent");
    }
}

#[test]
fn rd01_module_location_rewrite_rejects_near_matches() {
    let interner = StringInterner::new();
    let fixtures = [
        (0x0077_2A48, "RD01_Enc01_Bot", 1, FO4_QUEST_EVENT_LOCATION1),
        (0x0077_2A47, "RD01_Enc01_BotCopy", 1, FO4_QUEST_EVENT_LOCATION1),
        (0x0077_2A47, "RD01_Enc01_Bot", 0, FO4_QUEST_EVENT_LOCATION1),
        (0x0077_2A47, "RD01_Enc01_Bot", 1, FO4_QUEST_EVENT_REFERENCE1),
    ];
    for (form_id, editor_id, alias_id, event_data) in fixtures {
        let mut record = make_expedition_qust(
            &interner,
            form_id,
            editor_id,
            "ALLS",
            alias_id,
            "ModuleLocation",
            RD01_MODULE_LOCATION_FNAM,
            event_data,
        );
        Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut record);
        assert!(rd01_forced_location(&interner, &record).is_empty(), "{editor_id} {alias_id}");
        assert_eq!(expedition_alias_event_pair(&record).len(), 2);
    }

    let mut wrong_plugin = rd01_module_qust(&interner, 0x0077_2A47, "RD01_Enc01_Bot", 1);
    wrong_plugin.form_key.plugin = interner.intern("Other.esm");
    Fo76Fo4Hook::adapt_direct_start_quest_aliases(&interner, &mut wrong_plugin);
    assert!(rd01_forced_location(&interner, &wrong_plugin).is_empty());
}

#[test]
fn rd01_module_location_survives_full_translation() {
    let interner = StringInterner::new();
    let mut record = rd01_module_qust(&interner, 0x0078_6D41, "RD01_Enc06_Scorchtongue", 1);
    let translator = Translator::new(Game::Fo76, Game::Fo4).expect("translator");
    translator
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .expect("pre-translation");
    let translated = match translator.translate(&record, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated RD01 module QUST, got {other:?}"),
    };

    assert_eq!(
        rd01_forced_location(&interner, &translated),
        vec![(0x0077_0117, FO76_MASTER_NAME.to_string())]
    );
    assert!(expedition_alias_event_pair(&translated).is_empty());
    assert!(qust_alias_flags(&translated)
        .iter()
        .all(|flags| flags & QUST_ALIAS_OPTIONAL_FLAG == 0));
}

fn rd01_run_once_flag(interner: &StringInterner, form_id: u32, editor_id: &str, existing_dnam: bool) -> u16 {
    let mut record = make_record("QUST", interner);
    record.form_key.local = form_id;
    record.eid = Some(interner.intern(editor_id));
    let mut bytes = vec![0u8; if existing_dnam { FO4_QUST_DNAM_LEN } else { FO76_QUST_DATA_FLAGS64_LEN }];
    // FO76 source DATA flags for every RD01 quest carry RunOnce + WarnOnAliasFillFailure.
    bytes[0..2].copy_from_slice(&0x8100_u16.to_le_bytes());
    push_field(&mut record, if existing_dnam { "DNAM" } else { "DATA" }, raw_bytes(&bytes));
    Fo76Fo4Hook.pre_translate(&mut make_ctx(interner), &mut record).unwrap();
    let dnam = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "DNAM")
        .expect("DNAM");
    let FieldValue::Bytes(bytes) = &dnam.value else {
        panic!("DNAM bytes")
    };
    u16::from_le_bytes([bytes[0], bytes[1]]) & QUST_DNAM_FLAG_RUN_ONCE
}

#[test]
fn rd01_raid_quests_are_repeatable() {
    let interner = StringInterner::new();
    let quests = RD01_MODULE_CASES
        .iter()
        .map(|&(form_id, editor_id, _)| (form_id, editor_id))
        .chain([(0x0078_DA2A, "RD01_GleamingDepths")]);
    for (form_id, editor_id) in quests {
        for existing_dnam in [false, true] {
            assert_eq!(rd01_run_once_flag(&interner, form_id, editor_id, existing_dnam), 0, "{editor_id}");
            let lookalike = format!("{editor_id}_Copy");
            assert_eq!(
                rd01_run_once_flag(&interner, form_id, &lookalike, existing_dnam),
                QUST_DNAM_FLAG_RUN_ONCE,
                "{lookalike}"
            );
        }
    }
}
