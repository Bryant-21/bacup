#[test]
fn post_translate_adds_anio_unload_event_once() {
    let interner = StringInterner::new();
    let mut record = make_record("ANIO", &interner);
    push_field(
        &mut record,
        "MODL",
        FieldValue::String(interner.intern("AnimObjects\\Duster.nif")),
    );

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.post_translate(&mut ctx, &mut record).unwrap();
    hook.post_translate(&mut ctx, &mut record).unwrap();

    let unload_events: Vec<_> = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"BNAM")
        .collect();
    assert_eq!(unload_events.len(), 1);
    let FieldValue::String(unload_event) = unload_events[0].value else {
        panic!("expected BNAM string");
    };
    assert_eq!(interner.resolve(unload_event), Some("AnimObjUnequip"));
    assert_eq!(record.fields.last().unwrap().sig.as_str(), "BNAM");
}

#[test]
fn pre_translate_routes_only_skip_havok_misc_to_static_model_variant() {
    let interner = StringInterner::new();
    let source_model = "ATX\\backpack_flair\\Flair_FilmReel\\ATX_FilmReel_Flair.nif";
    let mut record = make_record("MISC", &interner);
    record.eid = Some(interner.intern("zzz_BURN_SQ04_FilmPiece"));
    push_field(
        &mut record,
        "MODL",
        FieldValue::String(interner.intern(source_model)),
    );
    push_field(
        &mut record,
        "XALG",
        FieldValue::Uint(FO76_XALG_SKIP_HAVOK_ON_LOAD | 0x10),
    );

    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let variant = fo76_misc_static_model_variant(&record, &interner).expect("static model variant");
    assert_eq!(
        variant.source_path,
        "Meshes/ATX/backpack_flair/Flair_FilmReel/ATX_FilmReel_Flair.nif"
    );
    assert_eq!(
        variant.output_subpath,
        "Meshes/BACUP_Static/ATX/backpack_flair/Flair_FilmReel/ATX_FilmReel_Flair.nif"
    );
    let decision: serde_json::Value =
        serde_json::from_str(&variant.decision_message()).expect("variant decision JSON");
    assert_eq!(
        decision["source_path"],
        "Meshes/ATX/backpack_flair/Flair_FilmReel/ATX_FilmReel_Flair.nif"
    );
    assert_eq!(
        decision["output_subpath"],
        "Meshes/BACUP_Static/ATX/backpack_flair/Flair_FilmReel/ATX_FilmReel_Flair.nif"
    );
    let FieldValue::String(model) = &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"MODL")
        .expect("MODL")
        .value
    else {
        panic!("expected MODL string");
    };
    assert_eq!(
        interner.resolve(*model),
        Some("BACUP_Static\\ATX\\backpack_flair\\Flair_FilmReel\\ATX_FilmReel_Flair.nif")
    );

    let mut record = make_record("MISC", &interner);
    push_field(
        &mut record,
        "MODL",
        FieldValue::String(interner.intern(source_model)),
    );
    push_field(&mut record, "XALG", FieldValue::Uint(0x10));
    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();
    assert!(fo76_misc_static_model_variant(&record, &interner).is_none());
    let FieldValue::String(model) = record.fields[0].value else {
        panic!("expected MODL string");
    };
    assert_eq!(interner.resolve(model), Some(source_model));
}

#[test]
fn post_translate_preserves_radio_frequencies_idempotently() {
    let interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);

    let mut receiver = make_record("ACTI", &interner);
    let mut raw_receiver = vec![0_u8; 14];
    raw_receiver[4..8].copy_from_slice(&98.2_f32.to_le_bytes());
    push_field(&mut receiver, "RADR", raw_bytes(&raw_receiver));
    hook.post_translate(&mut ctx, &mut receiver).unwrap();
    let once = receiver.fields.clone();
    hook.post_translate(&mut ctx, &mut receiver).unwrap();
    let FieldValue::Bytes(raw_receiver) = &receiver.fields[0].value else {
        panic!("expected raw RADR");
    };
    assert_eq!(f32::from_le_bytes(raw_receiver[4..8].try_into().unwrap()), 98.2);
    assert_eq!(receiver.fields, once);

    let mut receiver = make_record("ACTI", &interner);
    push_field(
        &mut receiver,
        "RADR",
        FieldValue::Struct(vec![
            (interner.intern("SoundModel"), FieldValue::Uint(0x0B5183)),
            (interner.intern("Frequency"), FieldValue::Float(80.5)),
        ]),
    );
    hook.post_translate(&mut ctx, &mut receiver).unwrap();
    let FieldValue::Struct(fields) = &receiver.fields[0].value else {
        panic!("expected structured RADR");
    };
    assert_eq!(fields[1].1, FieldValue::Float(80.5));

    let mut scene = make_record("SCEN", &interner);
    push_field(
        &mut scene,
        "CTDA",
        raw_bytes(
            &hex::decode("000000000000803F650200000000000000000000050000000000000003000000")
                .unwrap(),
        ),
    );
    push_field(
        &mut scene,
        "CTDA",
        raw_bytes(
            &hex::decode("000000009A99A642660200000000000000000000050000000000000003000000")
                .unwrap(),
        ),
    );
    hook.post_translate(&mut ctx, &mut scene).unwrap();
    let once = scene.fields.clone();
    hook.post_translate(&mut ctx, &mut scene).unwrap();
    let comparisons = scene
        .fields
        .iter()
        .map(|field| match &field.value {
            FieldValue::Bytes(bytes) => f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            other => panic!("expected raw CTDA, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(comparisons, vec![1.0, 83.300_003]);
    assert_eq!(scene.fields, once);
}

#[test]
fn post_translate_rewrites_fo76_font_aliases_in_localized_text() {
    let interner = StringInterner::new();
    let mut record = make_record("BOOK", &interner);
    push_field(
            &mut record,
            "DESC",
            FieldValue::String(interner.intern(
                "<font face='$Typewriter_Font'>typed</font> <font face='$76HandwrittenNeat_Font'>neat</font> <font face='$76HandwrittenIlliterate'>rough</font>",
            )),
        );

    Fo76Fo4Hook
        .post_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::String(text) = record.fields[0].value else {
        panic!("expected localized text");
    };
    assert_eq!(
        interner.resolve(text),
        Some(
            "<font face='$Terminal_Font'>typed</font> <font face='$HandwrittenFont'>neat</font> <font face='$HandwrittenFont'>rough</font>"
        )
    );
}

fn none_fields<'a>(sigs: &[&'a str]) -> Vec<(&'a str, FieldValue)> {
    sigs.iter().map(|sig| (*sig, FieldValue::None)).collect()
}

#[test]
fn pre_translate_subrecord_layout_table() {
    let interner = StringInterner::new();
    let marker_row = || raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN]);
    let count = |n: u32| raw_bytes(&n.to_le_bytes());
    let race_input = [
        "EDID", "ATKD", "CTDA", "CIS1", "CIS2", "HEAD", "CTDA", "CIS1", "CIS2", "TINL", "TTGP",
        "TETI", "TTEF", "CTDA", "CIS1", "CIS2", "TTET", "TTEB", "TTEC", "TTED", "TTGE", "MPGN",
        "MPPC", "MPPI", "MPPN", "MPPM", "MPPT", "MPPF", "MPPK", "MPGS",
    ];
    let cases: Vec<(&str, &str, Option<&str>, Vec<(&str, FieldValue)>, Vec<&str>)> = vec![
        (
            "strips_info_editor_id",
            "INFO",
            None,
            vec![
                ("ENAM", FieldValue::None),
                ("EDID", raw_bytes(b"FO76OnlyInfoEditorId\0")),
            ],
            vec!["ENAM"],
        ),
        (
            "drops_global_fo76_sigs",
            "NPC_",
            None,
            none_fields(&["VCTX", "FVER", "FL76", "FLWR", "MIID", "MAGF", "CODV", "OPDS", "EDID"]),
            vec!["EDID"],
        ),
        (
            "drops_only_unanchored_term_condition_groups",
            "TERM",
            None,
            vec![
                ("BSIZ", count(1)),
                ("BTXT", FieldValue::None),
                ("CTDA", raw_ctda(1)),
                ("CIS1", FieldValue::None),
                ("ISIZ", count(1)),
                ("CTDA", raw_ctda(2)),
                ("CIS1", FieldValue::None),
                ("CIS2", FieldValue::None),
                ("ITXT", FieldValue::None),
                ("ANAM", FieldValue::None),
                ("ITID", FieldValue::None),
                ("CTDA", raw_ctda(3)),
                ("CIS2", FieldValue::None),
                ("UNAM", FieldValue::None),
                ("CTDA", raw_ctda(4)),
                ("CIS1", FieldValue::None),
            ],
            vec![
                "BSIZ", "BTXT", "CTDA", "CIS1", "ISIZ", "ITXT", "ANAM", "ITID", "CTDA", "CIS2",
                "UNAM",
            ],
        ),
        (
            "keeps_unrelated_acti_activation_conditions",
            "ACTI",
            Some("OtherSearchActivator"),
            vec![("CITC", count(1)), ("CTDA", raw_ctda(203))],
            vec!["CITC", "CTDA"],
        ),
        (
            "keeps_scorched_statue_activation_conditions",
            "ACTI",
            Some("ScorchedStatue05"),
            vec![
                ("FULL", FieldValue::None),
                ("CNDC", count(0)),
                ("CITC", count(3)),
                ("CTDA", raw_ctda(203)),
                ("CTDA", raw_ctda(77)),
                ("CTDA", raw_ctda(FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID)),
                ("CNDC", count(1)),
                ("CITC", count(1)),
                ("CTDA", raw_ctda(203)),
                ("FNAM", FieldValue::None),
            ],
            vec![
                "FULL", "CNDC", "CITC", "CTDA", "CTDA", "CTDA", "CNDC", "CITC", "CTDA", "FNAM",
            ],
        ),
        (
            "renames_furniture_marker_parameters",
            "FURN",
            None,
            vec![("ZNAM", marker_row())],
            vec!["SNAM"],
        ),
        (
            "keeps_terminal_znam",
            "TERM",
            None,
            vec![("SNAM", count(1)), ("ZNAM", marker_row())],
            vec!["SNAM", "ZNAM"],
        ),
        (
            "strips_race_tints_keeping_conditions_and_morphs",
            "RACE",
            None,
            none_fields(&race_input),
            vec![
                "EDID", "ATKD", "CTDA", "CIS1", "CIS2", "HEAD", "CTDA", "CIS1", "CIS2", "MPGN",
                "MPPC", "MPPI", "MPPN", "MPPM", "MPPT", "MPPF", "MPPK", "MPGS",
            ],
        ),
        ("keeps_npc_qnam", "NPC_", None, none_fields(&["QNAM"]), vec!["QNAM"]),
    ];
    for (name, sig, editor_id, fields, expected) in cases {
        let mut record = make_record(sig, &interner);
        record.eid = editor_id.map(|editor_id| interner.intern(editor_id));
        for (field, value) in fields {
            push_field(&mut record, field, value);
        }

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, expected, "{name}");
    }
}

#[test]
fn post_translate_subrecord_layout_table() {
    let interner = StringInterner::new();
    let cases: Vec<(&str, &str, Option<&str>, Vec<(&str, FieldValue)>, Vec<&str>)> = vec![
        (
            "strips_wsbunker_intercom_radio",
            "ACTI",
            Some(WSBUNKER_INTERCOM_EDITOR_ID),
            vec![
                (
                    "MODL",
                    FieldValue::String(
                        interner.intern("SetDressing\\WallPanels\\Intercom_Panel.nif"),
                    ),
                ),
                ("FNAM", FieldValue::None),
                ("RADR", raw_bytes(&[0_u8; 14])),
            ],
            vec!["MODL"],
        ),
        (
            "preserves_perk_vmad",
            "PERK",
            None,
            vec![
                ("EDID", FieldValue::None),
                ("VMAD", raw_bytes(&[1, 2, 3, 4])),
                ("FULL", FieldValue::String(interner.intern("Perk"))),
            ],
            vec!["EDID", "VMAD", "FULL"],
        ),
    ];
    for (name, sig, editor_id, fields, expected) in cases {
        let mut record = make_record(sig, &interner);
        record.eid = editor_id.map(|editor_id| interner.intern(editor_id));
        for (field, value) in fields {
            push_field(&mut record, field, value);
        }

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, expected, "{name}");
    }
}

#[test]
fn pre_translate_maps_only_refr_marker_types_to_two_byte_fo4_layout() {
    let interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);

    for (source_type, target_type) in [(2_u16, 3_u8), (7, 8), (54, 58), (64, 6), (65, 4), (66, 8)] {
        let mut record = make_record("REFR", &interner);
        push_field(
            &mut record,
            "TNAM",
            raw_bytes(&u32::from(source_type).to_le_bytes()),
        );

        hook.pre_translate(&mut ctx, &mut record).unwrap();

        match &record.fields[0].value {
            FieldValue::Bytes(bytes) => assert_eq!(bytes.as_slice(), &[target_type, 0]),
            value => panic!("expected TNAM bytes, got {value:?}"),
        }
    }

    let mut record = make_record("TERM", &interner);
    push_field(&mut record, "TNAM", raw_bytes(&64_u32.to_le_bytes()));
    hook.pre_translate(&mut ctx, &mut record).unwrap();
    assert_eq!(record.fields[0].value, raw_bytes(&64_u32.to_le_bytes()));
}

#[test]
fn pre_translate_converts_fo76_lvli_split_rows_to_fo4_lvlo() {
    let mut interner = StringInterner::new();
    let mut record = make_record("LVLI", &mut interner);
    let variant_sym = interner.intern("variant");
    let value_sym = interner.intern("value");
    let reference_variant = interner.intern("reference");
    push_field(&mut record, "LLCT", FieldValue::Uint(2));
    push_field(
        &mut record,
        "LVLO",
        FieldValue::Struct(vec![
            (variant_sym, FieldValue::String(reference_variant)),
            (value_sym, FieldValue::Uint(0x08E3A8)),
        ]),
    );
    push_field(&mut record, "LVIV", FieldValue::Float(2.0));
    push_field(&mut record, "LVLV", FieldValue::Float(3.0));
    push_field(&mut record, "LVLO", FieldValue::Uint(0x02C59E));
    push_field(&mut record, "LVIV", FieldValue::Float(1.0));
    push_field(&mut record, "LVLV", FieldValue::Float(1.0));

    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    hook.pre_translate(&mut ctx, &mut record).unwrap();

    let count = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "LLCT")
        .map(|entry| &entry.value);
    assert_eq!(count, Some(&FieldValue::Uint(2)));
    let lvlo_entries: Vec<_> = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "LVLO")
        .collect();
    assert_eq!(lvlo_entries.len(), 2);

    let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
    let record = match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
        &schema, &interner,
    )
    .normalize(record)
    {
        crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
        crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
            panic!("LVLI is supported by FO4 schema")
        }
    };
    let lvlo_entries: Vec<_> = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "LVLO")
        .collect();
    assert_eq!(lvlo_entries.len(), 2);
    let first = crate::target_write::encode_field_pub(
        lvlo_entries[0],
        schema.record_def("LVLI"),
        &interner,
    )
    .expect("first converted LVLO encodes");
    assert_eq!(first, vec![3, 0, 0, 0, 0xA8, 0xE3, 0x08, 0, 2, 0, 0, 0]);
    let second = crate::target_write::encode_field_pub(
        lvlo_entries[1],
        schema.record_def("LVLI"),
        &interner,
    )
    .expect("second converted LVLO encodes");
    assert_eq!(second, vec![1, 0, 0, 0, 0x9E, 0xC5, 0x02, 0, 1, 0, 0, 0]);
}

#[test]
fn post_translate_trims_fo76_movement_speed_data_to_fo4_ck_size() {
    let interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    let mut record = make_record("MOVT", &interner);
    let raw = (0_u8..124).collect::<Vec<_>>();
    push_field(
        &mut record,
        "SPED",
        FieldValue::Bytes(SmallVec::from_vec(raw)),
    );

    hook.post_translate(&mut ctx, &mut record).unwrap();

    let FieldValue::Bytes(bytes) = &record.fields[0].value else {
        panic!("expected SPED bytes");
    };
    assert_eq!(bytes.len(), FO4_MOVEMENT_SPEED_DATA_LEN);
    assert_eq!(bytes.as_slice(), (0_u8..112).collect::<Vec<_>>().as_slice());
}

#[test]
fn post_translate_masks_only_idlm_unknown_5_flag_idempotently() {
    let interner = StringInterner::new();
    let hook = Fo76Fo4Hook;
    let mut ctx = make_ctx(&interner);
    let mut record = make_record("IDLM", &interner);
    push_field(&mut record, "IDLF", FieldValue::Uint(0x3f));
    push_field(&mut record, "IDLF", FieldValue::Int(0x28));
    push_field(&mut record, "IDLF", raw_bytes(&[0x28]));
    push_field(&mut record, "IDLF", raw_bytes(&[0x28, 0xff]));

    hook.post_translate(&mut ctx, &mut record).unwrap();
    let once = record.fields.clone();
    hook.post_translate(&mut ctx, &mut record).unwrap();

    assert_eq!(record.fields, once);
    assert_eq!(record.fields[0].value, FieldValue::Uint(0x1f));
    assert_eq!(record.fields[1].value, FieldValue::Int(0x08));
    assert_eq!(record.fields[2].value, raw_bytes(&[0x08]));
    assert_eq!(record.fields[3].value, raw_bytes(&[0x28, 0xff]));

    let mut record = make_record("PACK", &interner);
    push_field(&mut record, "IDLF", FieldValue::Uint(0x28));
    hook.post_translate(&mut ctx, &mut record).unwrap();
    assert_eq!(record.fields[0].value, FieldValue::Uint(0x28));
}

/// Build a 72-byte FO76-style LIGH DATA blob: flags @ +12, near clip @ +24,
/// flicker intensity amplitude @ +32.
fn fo76_ligh_data(flags: u32, near_clip: f32, flicker_intensity_amp: f32) -> Vec<u8> {
    let mut bytes = vec![0_u8; 72];
    bytes[12..16].copy_from_slice(&flags.to_le_bytes());
    bytes[24..28].copy_from_slice(&near_clip.to_le_bytes());
    bytes[32..36].copy_from_slice(&flicker_intensity_amp.to_le_bytes());
    bytes
}

/// Build an FO76 INNR carrying `filter` plus one minimal ruleset.
fn fo76_innr(filter: FieldValue, interner: &StringInterner) -> Record {
    let mut record = make_record("INNR", interner);
    push_field(&mut record, "INRF", filter);
    push_field(
        &mut record,
        "ZNAM",
        FieldValue::String(interner.intern("Legendary")),
    );
    push_field(&mut record, "VNAM", FieldValue::Uint(1));
    record
}

fn innr_target(record: &Record) -> Option<u64> {
    record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"UNAM")
        .and_then(|field| match field.value {
            FieldValue::Uint(value) => Some(value),
            _ => None,
        })
}

#[test]
fn pre_translate_converts_innr_filter_to_fo4_target() {
    let interner = StringInterner::new();
    let string = |filter: &str| FieldValue::String(interner.intern(filter));
    for (name, sig, filter, expected) in [
        ("armor", "INNR", string("ARMO"), Some(FO4_INNR_TARGET_ARMOR)),
        ("weapon", "INNR", string("WEAP"), Some(FO4_INNR_TARGET_WEAPON)),
        ("furniture", "INNR", string("FURN"), Some(FO4_INNR_TARGET_FURNITURE)),
        ("actor", "INNR", string("NPC_"), Some(FO4_INNR_TARGET_ACTOR)),
        (
            "raw_bytes_filter",
            "INNR",
            FieldValue::Bytes(SmallVec::from_slice(b"WEAP")),
            Some(FO4_INNR_TARGET_WEAPON),
        ),
        ("unknown_filter", "INNR", string("SPEL"), None),
        ("non_innr_record", "WEAP", string("WEAP"), None),
    ] {
        let mut record = if sig == "INNR" {
            fo76_innr(filter, &interner)
        } else {
            let mut record = make_record(sig, &interner);
            push_field(&mut record, "INRF", filter);
            record
        };

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(innr_target(&record), expected.map(u64::from), "{name}");
        if expected.is_none() {
            continue;
        }
        // INRF has no FO4 counterpart: it becomes UNAM rather than lingering.
        assert!(
            !record.fields.iter().any(|field| field.sig.0 == *b"INRF"),
            "{name} should not retain INRF"
        );
        // UNAM must precede the first ruleset VNAM to match FO4 subrecord order.
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        let unam = sigs.iter().position(|s| *s == "UNAM").expect("UNAM");
        let vnam = sigs.iter().position(|s| *s == "VNAM").expect("VNAM");
        assert!(unam < vnam, "{name}: UNAM must come before VNAM");
    }
}
