use super::*;
use crate::formkey_mapper::MapperOptions;
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::{plugin_handle_close_native, plugin_handle_new_native};

fn fk(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        local,
        plugin: interner.intern("SeventySix.esm"),
    }
}

fn record(
    sig: &[u8; 4],
    local: u32,
    fields: Vec<(&[u8; 4], FieldValue)>,
    interner: &StringInterner,
) -> Record {
    let mut record = Record::new(SigCode(*sig), fk(local, interner));
    record
        .fields
        .extend(fields.into_iter().map(|(sig, value)| FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }));
    record
}

fn source_recipe(local: u32, book: u32, method: u64, interner: &StringInterner) -> Record {
    record(
        b"COBJ",
        local,
        vec![
            (b"LRNM", FieldValue::Uint(method)),
            (b"GNAM", FieldValue::FormKey(fk(book, interner))),
        ],
        interner,
    )
}

#[test]
fn source_plan_links_exclude_scrapping_and_default_recipes() {
    let interner = StringInterner::new();
    assert_eq!(
        plan_book(&source_recipe(0x4709D6, 0x470A3F, 4, &interner), &interner),
        Some(fk(0x470A3F, &interner))
    );
    for method in [0, 1, 2, 3, 5] {
        assert_eq!(
            plan_book(
                &source_recipe(0x4709D6, 0x470A3F, method, &interner),
                &interner
            ),
            None
        );
    }
    assert_eq!(
        plan_book(&source_recipe(0x4709D6, 0, 4, &interner), &interner),
        None
    );
}

#[test]
fn global_gate_preserves_categories_costs_and_or_chains() {
    let interner = StringInterner::new();
    let mut recipe = record(
        b"COBJ",
        0x40F66C,
        vec![
            (b"FVPA", FieldValue::Bytes(vec![8; 8].into())),
            (b"CNAM", FieldValue::FormKey(fk(0x40F663, &interner))),
            (
                b"FNAM",
                FieldValue::List(vec![FieldValue::FormKey(fk(0x8229EA, &interner))]),
            ),
        ],
        &interner,
    );
    let mut existing = learning_condition(0x085A20E9);
    if let FieldValue::Bytes(bytes) = &mut existing.value {
        bytes[0] = 1;
    }
    recipe.fields.insert(1, existing.clone());
    recipe.fields.insert(2, learning_condition(0x08001000));
    let before = recipe.fields.clone();
    assert!(add_learning_condition(&mut recipe, 0x08F00234));
    assert_eq!(recipe.fields[1], learning_condition(0x08F00234));
    recipe.fields.remove(1);
    assert_eq!(recipe.fields, before);
    assert!(add_learning_condition(&mut recipe, 0x08F00234));
    assert!(!add_learning_condition(&mut recipe, 0x08F00234));
}

#[test]
fn readable_text_is_added_only_to_empty_books() {
    let interner = StringInterner::new();
    let mut book = record(
        b"BOOK",
        0x40F66A,
        vec![
            (
                b"FULL",
                FieldValue::String(interner.intern("Plan: Crane Treasure Hunting Sign")),
            ),
            (
                b"DESC",
                FieldValue::Struct(vec![(
                    interner.intern("TargetLanguage"),
                    FieldValue::String(interner.intern("English")),
                )]),
            ),
            (b"DATA", FieldValue::Bytes(vec![0; 8].into())),
        ],
        &interner,
    );
    assert!(ensure_readable(&mut book, &interner));
    assert!(!ensure_readable(&mut book, &interner));
    assert_eq!(
        field(&book, b"FULL"),
        Some(&FieldValue::String(
            interner.intern("Plan: Crane Treasure Hunting Sign")
        ))
    );
    let original = FieldValue::Struct(vec![(
        interner.intern("Value"),
        FieldValue::String(interner.intern("Existing instructions")),
    )]);
    book.fields[1].value = original.clone();
    assert!(!ensure_readable(&mut book, &interner));
    assert_eq!(field(&book, b"DESC"), Some(&original));
}

#[test]
fn attaching_the_reader_preserves_other_scripts_and_is_idempotent() {
    let interner = StringInterner::new();
    let original = build_vmad_bytes_from_payload(
        &serde_json::json!({"Version":6,"Object Format":2,
        "Scripts":[{"ScriptName":"ExistingBookScript","Flags":0,"Properties":[]}]}),
        &[],
        "SeventySix.esm",
    )
    .unwrap();
    let mut book = record(
        b"BOOK",
        0x40F66A,
        vec![(b"VMAD", FieldValue::Bytes(original.into()))],
        &interner,
    );
    let vmad = plan_vmad(
        Some(fk(0xF00001, &interner)),
        Some(fk(0x40F5BE, &interner)),
        &[],
        "SeventySix.esm",
        &interner,
    )
    .unwrap();
    assert!(attach_plan_script(&mut book, &vmad).unwrap());
    assert!(!attach_plan_script(&mut book, &vmad).unwrap());
    let FieldValue::Bytes(bytes) = field(&book, b"VMAD").unwrap() else {
        panic!()
    };
    assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), 2);
    assert!(bytes.windows(18).any(|part| part == b"ExistingBookScript"));
    assert!(bytes.windows(9).any(|part| part == b"ReadQuest"));
}

#[test]
fn session_roundtrip_links_remapped_books_shared_plans_and_expanded_variants() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
    let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
    let mut source_book = record(
        b"BOOK",
        0x40F66A,
        vec![(
            b"DNAM",
            FieldValue::Bytes(vec![0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].into()),
        )],
        &interner,
    );
    source_book.eid =
        Some(interner.intern("W05_MQ_002P_Radical_Recipe_Workshop_CraneRadioTransmitter"));
    source_book.fields.insert(
        0,
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(source_book.eid.unwrap()),
        },
    );
    {
        let mut session = open_session(source, None).unwrap();
        let schema = session.schema().unwrap();
        session
            .add_records(
                vec![
                    source_book,
                    source_recipe(0x40F66C, 0x40F66A, 4, &interner),
                    source_recipe(0x40F66D, 0x40F66A, 4, &interner),
                    record(
                        b"BOOK",
                        0x900010,
                        vec![(b"DNAM", FieldValue::Bytes(vec![0x20; 13].into()))],
                        &interner,
                    ),
                    record(
                        b"BOOK",
                        0x900011,
                        vec![(b"DNAM", FieldValue::Bytes(vec![0; 13].into()))],
                        &interner,
                    ),
                    record(
                        b"BOOK",
                        0x2B8BCE,
                        vec![(b"DNAM", FieldValue::Bytes(vec![0x20; 13].into()))],
                        &interner,
                    ),
                    source_recipe(0x101291, 0x2B8BCE, 4, &interner),
                ],
                &schema,
                &interner,
            )
            .unwrap();
    }
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            source_plugin_name: "SeventySix.esm".into(),
            generated_object_id_floor: 0xF00000,
            ..MapperOptions::default()
        },
        &interner,
    );
    let mut session = open_session(target, Some(source)).unwrap();
    session
        .target_slot_mut()
        .parsed
        .header
        .masters
        .push("Fallout4.esm".into());
    mapper.add_mapping(
        fk(0x101291, &interner),
        FormKey {
            local: 0x101291,
            plugin: interner.intern("Fallout4.esm"),
        },
    );
    let schema = session.schema().unwrap();
    let mut seeds = Vec::new();
    for (sig, local) in [
        (b"BOOK", 0x40F66A),
        (b"BOOK", 0x900010),
        (b"BOOK", 0x900011),
        (b"BOOK", 0x2B8BCE),
        (b"QUST", 0x40F5BE),
        (b"COBJ", 0x40F66C),
        (b"COBJ", 0x40F66D),
        (b"COBJ", 0xE00001),
    ] {
        let target_local = if local == 0x40F66A { 0x800008 } else { local };
        mapper.add_mapping(fk(local, &interner), fk(target_local, &interner));
        let mut seed = Record::new(SigCode(*sig), fk(target_local, &interner));
        if sig == b"COBJ" {
            seed.fields.push(FieldEntry {
                sig: SubrecordSig(*b"CNAM"),
                value: FieldValue::FormKey(fk(0x40F663, &interner)),
            });
        }
        if local == 0xE00001 {
            let eid = interner.intern("B21_FO76Workshop_40F66C_40F663_001");
            seed.eid = Some(eid);
            seed.fields.insert(
                0,
                FieldEntry {
                    sig: SubrecordSig(*b"EDID"),
                    value: FieldValue::String(eid),
                },
            );
        }
        seeds.push(seed);
    }
    session.add_records(seeds, &schema, &interner).unwrap();
    let fixup = RestoreFo76PlanLearningFixup;
    let report = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(report.records_added, 2);
    assert_eq!(report.records_changed, 6);
    let globals = session
        .form_keys_of_sig(SigCode(*b"GLOB"), &interner)
        .unwrap();
    let global = *globals
        .iter()
        .find(|fk| {
            let record = session.record_decoded(fk, &schema, &interner).unwrap();
            record.eid.and_then(|eid| interner.resolve(eid)) == Some("B21_PlanLearned_800008")
        })
        .unwrap();
    let decoded = session.record_decoded(&global, &schema, &interner).unwrap();
    assert_eq!(field(&decoded, b"FLTV"), Some(&FieldValue::Float(0.0)));
    for local in [0x40F66C, 0x40F66D, 0xE00001] {
        let recipe = session
            .record_decoded(&fk(local, &interner), &schema, &interner)
            .unwrap();
        assert!(
            recipe
                .fields
                .contains(&learning_condition(0x01000000 | global.local))
        );
    }
    let book = session
        .record_decoded(&fk(0x800008, &interner), &schema, &interner)
        .unwrap();
    assert!(has_text(field(&book, b"DESC").unwrap(), &interner));
    let ordinary = session
        .record_decoded(&fk(0x900011, &interner), &schema, &interner)
        .unwrap();
    assert!(field(&ordinary, b"VMAD").is_none());
    assert!(field(&ordinary, b"DESC").is_none());
    let unavailable = session
        .record_decoded(&fk(0x900010, &interner), &schema, &interner)
        .unwrap();
    let FieldValue::Bytes(vmad) = field(&unavailable, b"VMAD").unwrap() else {
        panic!()
    };
    assert!(!vmad.windows(7).any(|part| part == b"Learned"));
    let vanilla_global = globals.iter().find(|fk| **fk != global).unwrap();
    let vanilla_global = session
        .record_decoded(vanilla_global, &schema, &interner)
        .unwrap();
    assert_eq!(
        field(&vanilla_global, b"FLTV"),
        Some(&FieldValue::Float(1.0))
    );
    let repeat = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!((repeat.records_added, repeat.records_changed), (0, 0));
    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(source));
}
