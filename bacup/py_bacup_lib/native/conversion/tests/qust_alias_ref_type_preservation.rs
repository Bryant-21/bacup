use conversion_native::ids::{FormKey, SigCode, SubrecordSig};
use conversion_native::record::{FieldEntry, FieldValue, Record};
use conversion_native::sym::StringInterner;
use conversion_native::translator::pair_hook::PairCtx;
use conversion_native::translator::target_hook::TargetCtx;
use conversion_native::translator::{Game, TranslateResult, Translator};
use smallvec::SmallVec;

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).unwrap(),
        value,
    }
}

fn condition(function_id: u16, parameter_1: u32) -> FieldEntry {
    let mut bytes = SmallVec::<[u8; 32]>::new();
    bytes.resize(32, 0);
    bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
    bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    field("CTDA", FieldValue::Bytes(bytes))
}

fn alias_fields(record: &Record, alias_id: u64) -> &[FieldEntry] {
    let start = record
        .fields
        .iter()
        .position(|entry| entry.sig.as_str() == "ALST" && entry.value == FieldValue::Uint(alias_id))
        .expect("reference alias exists");
    let end = record.fields[start..]
        .iter()
        .position(|entry| entry.sig.as_str() == "ALED")
        .map(|offset| start + offset + 1)
        .expect("reference alias ends");
    &record.fields[start..end]
}

fn location_alias_fields(record: &Record, alias_id: u64) -> &[FieldEntry] {
    let start = record
        .fields
        .iter()
        .position(|entry| entry.sig.as_str() == "ALLS" && entry.value == FieldValue::Uint(alias_id))
        .expect("location alias exists");
    let end = record.fields[start..]
        .iter()
        .position(|entry| entry.sig.as_str() == "ALED")
        .map(|offset| start + offset + 1)
        .expect("location alias ends");
    &record.fields[start..end]
}

fn raw_condition_function_and_parameter(entry: &FieldEntry) -> Option<(u16, u32)> {
    let FieldValue::Bytes(bytes) = &entry.value else {
        return None;
    };
    (entry.sig.as_str() == "CTDA" && bytes.len() >= 16).then(|| {
        (
            u16::from_le_bytes([bytes[8], bytes[9]]),
            u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        )
    })
}

#[test]
fn fo76_gq_horde_alias_location_ref_type_is_not_reinterpreted_as_fo4_alrt() {
    let interner = StringInterner::new();
    let source_plugin = interner.intern("SeventySix.esm");
    let mut quest = Record::new(
        SigCode::from_str("QUST").unwrap(),
        FormKey {
            local: 0x0000_123F,
            plugin: source_plugin,
        },
    );
    quest.eid = Some(interner.intern("GQ_Horde"));
    quest.fields.extend([
        field("ANAM", FieldValue::Uint(37)),
        field("ALST", FieldValue::Uint(16)),
        field(
            "ALID",
            FieldValue::String(interner.intern("AlphaSpawnMarker")),
        ),
        field("FNAM", FieldValue::Uint(0)),
        field(
            "ALFF",
            FieldValue::Bytes(SmallVec::from_slice(&0x0001_F40F_u32.to_le_bytes())),
        ),
        field("ALED", FieldValue::None),
    ]);

    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut quest)
        .unwrap();
    let mut translated = match translator.translate(&quest, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated QUST, got {other:?}"),
    };
    translator
        .post_translate(&mut PairCtx::new(&interner), &mut translated)
        .unwrap();
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut translated,
        )
        .unwrap();

    assert!(
        translated
            .fields
            .iter()
            .all(|entry| entry.sig.as_str() != "ALRT"),
        "standalone FO76 ALFF must not become FO4 ALRT"
    );
    assert!(
        translated
            .fields
            .iter()
            .all(|entry| entry.sig.as_str() != "ALFF"),
        "FO76-only ALFF must not survive FO4 translation"
    );
}

#[test]
fn nuke_launch_card_patrol_synthesizes_event_location_ref_type_fills() {
    let interner = StringInterner::new();
    let source_plugin = interner.intern("SeventySix.esm");
    let mut quest = Record::new(
        SigCode::from_str("QUST").unwrap(),
        FormKey {
            local: 0x003E_133F,
            plugin: source_plugin,
        },
    );
    quest.eid = Some(interner.intern("Nuke_LaunchCardPatrol"));
    quest.fields.push(field("ANAM", FieldValue::Uint(35)));
    let marker_ref_types = [0x0001_F40F, 0x0001_9479, 0x0001_F40F];
    for ((alias_id, ref_type), existing_condition) in [1_u64, 23, 29]
        .into_iter()
        .zip(marker_ref_types)
        .zip([(310, 0x0025_DA15), (561, 0x0001_9479), (359, 0x0009_5004)])
    {
        quest.fields.extend([
            field("ALST", FieldValue::Uint(alias_id)),
            field("FNAM", FieldValue::Uint(0)),
            condition(existing_condition.0, existing_condition.1),
            field(
                "ALFF",
                FieldValue::FormKey(FormKey {
                    local: ref_type,
                    plugin: source_plugin,
                }),
            ),
            field("ALED", FieldValue::None),
        ]);
    }

    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut quest)
        .unwrap();
    let mut translated = match translator.translate(&quest, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated QUST, got {other:?}"),
    };
    translator
        .post_translate(&mut PairCtx::new(&interner), &mut translated)
        .unwrap();
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut translated,
        )
        .unwrap();

    assert!(
        translated
            .fields
            .iter()
            .all(|entry| entry.sig.as_str() != "ALFF")
    );
    assert!(
        translated
            .fields
            .iter()
            .any(|entry| { entry.sig.as_str() == "ANAM" && entry.value == FieldValue::Uint(36) })
    );

    let event_location = location_alias_fields(&translated, 35);
    assert!(event_location.iter().any(|entry| {
        entry.sig.as_str() == "ALID"
            && matches!(entry.value, FieldValue::String(value) if interner.resolve(value) == Some("EventLocation"))
    }));
    assert!(event_location.iter().any(|entry| {
        entry.sig.as_str() == "ALFE"
            && entry.value == FieldValue::Uint(u64::from(u32::from_le_bytes(*b"SCPT")))
    }));
    assert!(
        event_location.iter().any(|entry| {
            entry.sig.as_str() == "ALFD" && entry.value == FieldValue::Uint(12_620)
        })
    );

    let existing_conditions = [(310, 0x0025_DA15), (561, 0x0001_9479), (359, 0x0009_5004)];
    for ((alias_id, ref_type), existing_condition) in [1_u64, 23, 29]
        .into_iter()
        .zip(marker_ref_types)
        .zip(existing_conditions)
    {
        let fields = alias_fields(&translated, alias_id);
        let conditions = fields
            .iter()
            .filter_map(raw_condition_function_and_parameter)
            .collect::<Vec<_>>();
        assert_eq!(conditions, vec![existing_condition], "alias {alias_id}");
        let alfa_index = fields
            .iter()
            .position(|entry| entry.sig.as_str() == "ALFA")
            .expect("reference alias uses a location alias fill");
        assert_eq!(fields[alfa_index].value, FieldValue::Uint(35));
        let alrt = &fields[alfa_index + 1];
        assert_eq!(alrt.sig.as_str(), "ALRT");
        assert!(matches!(
            alrt.value,
            FieldValue::FormKey(form_key) if form_key.local & 0x00FF_FFFF == ref_type
        ));
    }
}

#[test]
fn fo76_qust_genuine_location_alias_ref_type_survives_translation() {
    let interner = StringInterner::new();
    let source_plugin = interner.intern("SeventySix.esm");
    let mut quest = Record::new(
        SigCode::from_str("QUST").unwrap(),
        FormKey {
            local: 0x0012_3456,
            plugin: source_plugin,
        },
    );
    quest.eid = Some(interner.intern("EventQuest"));
    quest.fields.extend([
        field("ANAM", FieldValue::Uint(3)),
        field("ALLS", FieldValue::Uint(1)),
        field("ALID", FieldValue::String(interner.intern("EventLocation"))),
        field("FNAM", FieldValue::Uint(0)),
        field("ALED", FieldValue::None),
        field("ALST", FieldValue::Uint(2)),
        field("ALID", FieldValue::String(interner.intern("MapMarker"))),
        field("FNAM", FieldValue::Uint(0)),
        field("ALFA", FieldValue::Uint(1)),
        field(
            "ALRT",
            FieldValue::FormKey(FormKey {
                local: 0x0002_271F,
                plugin: source_plugin,
            }),
        ),
        field("ALED", FieldValue::None),
    ]);

    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    let quest_map = translator.maps.record_map("QUST").expect("QUST map");
    assert!(
        !quest_map.drop_fields.iter().any(|field| field == "ALRT"),
        "ALRT must not be dropped from FO4-compatible QUST aliases"
    );

    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut quest)
        .unwrap();
    let mut translated = match translator.translate(&quest, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated QUST, got {other:?}"),
    };
    translator
        .post_translate(&mut PairCtx::new(&interner), &mut translated)
        .unwrap();
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut translated,
        )
        .unwrap();

    let alias_ref_type = translated
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "ALRT")
        .expect("genuine ALFA + ALRT alias fill survives");
    assert_eq!(
        alias_ref_type.value,
        FieldValue::FormKey(FormKey {
            local: 0x0002_271F,
            plugin: source_plugin,
        })
    );
}

#[test]
fn fo76_qust_unproven_event_alias_fill_is_dropped_and_marked_optional() {
    let interner = StringInterner::new();
    let source_plugin = interner.intern("SeventySix.esm");
    let mut quest = Record::new(
        SigCode::from_str("QUST").unwrap(),
        FormKey {
            local: 0x0011_F0,
            plugin: source_plugin,
        },
    );
    quest.eid = Some(interner.intern("GQ_WorkshopReclaim"));
    quest.fields.extend([
        field("ANAM", FieldValue::Uint(2)),
        field("ALST", FieldValue::Uint(1)),
        field("ALID", FieldValue::String(interner.intern("Workshop"))),
        field("FNAM", FieldValue::Uint(0)),
        field(
            "ALFE",
            FieldValue::Uint(u64::from(u32::from_le_bytes(*b"SCPT"))),
        ),
        field(
            "ALFD",
            FieldValue::Uint(u64::from(u16::from_le_bytes(*b"R1"))),
        ),
        field("ALED", FieldValue::None),
    ]);

    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut quest)
        .unwrap();
    let mut translated = match translator.translate(&quest, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated QUST, got {other:?}"),
    };
    translator
        .post_translate(&mut PairCtx::new(&interner), &mut translated)
        .unwrap();
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut translated,
        )
        .unwrap();

    let alias_fields = translated
        .fields
        .iter()
        .filter(|entry| matches!(&entry.sig.0, b"ALST" | b"FNAM" | b"ALFE" | b"ALFD"))
        .map(|entry| entry.sig.as_str())
        .collect::<Vec<_>>();
    assert_eq!(alias_fields, ["ALST", "FNAM"]);
    assert_eq!(
        translated
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "FNAM")
            .map(|entry| &entry.value),
        Some(&FieldValue::Uint(0x2))
    );
}
