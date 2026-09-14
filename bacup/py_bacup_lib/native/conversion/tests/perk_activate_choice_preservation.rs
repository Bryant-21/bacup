use conversion_native::ids::{FormKey, SigCode, SubrecordSig};
use conversion_native::record::{FieldEntry, FieldValue, Record};
use conversion_native::sym::StringInterner;
use conversion_native::translator::pair_hook::PairCtx;
use conversion_native::translator::target_hook::TargetCtx;
use conversion_native::translator::{Game, TranslateResult, Translator};
use smallvec::SmallVec;

fn field(signature: &str, bytes: &[u8]) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).unwrap(),
        value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
    }
}

fn condition(function: u16, parameter: u32) -> FieldEntry {
    let mut bytes = [0_u8; 32];
    bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    field("CTDA", &bytes)
}

#[test]
fn fo76_perk_activate_choice_entries_survive_translation_in_source_order() {
    let interner = StringInterner::new();
    let source_plugin = interner.intern("SeventySix.esm");
    let mut perk = Record::new(
        SigCode::from_str("PERK").unwrap(),
        FormKey {
            local: 0x0011_D76B,
            plugin: source_plugin,
        },
    );
    perk.eid = Some(interner.intern("TW004ActivateChoices"));
    perk.fields.extend([
        field("VMAD", &[6, 0, 2, 0, 0, 0]),
        field("PRKE", &[0x02, 0x00]),
        field("DATA", &[0x0E, 0x09, 0x02, 0x00]),
        field("PRKC", &[0]),
        condition(59, 100),
        field("CIS1", b"first\0"),
        field("CIS2", b"first-value\0"),
        field("EPFT", &[2]),
        field("EPFB", &[7]),
        field("EPF2", b"First choice\0"),
        field("EPF3", &[1]),
        field("PRKF", &[]),
        field("PRKE", &[0x02, 0x00]),
        field("DATA", &[0x0E, 0x09, 0x02, 0x00]),
        field("PRKC", &[1]),
        condition(59, 200),
        field("CIS1", b"second\0"),
        field("CIS2", b"second-value\0"),
        field("EPFT", &[2]),
        field("EPFB", &[8]),
        field("EPF2", b"Second choice\0"),
        field("EPF3", &[2]),
        field("PRKF", &[]),
    ]);

    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    let perk_map = translator.maps.record_map("PERK").expect("PERK map");
    for compatible_field in [
        "VirtualMachineAdapter",
        "VMAD",
        "PRKE",
        "RunOnTabIndex",
        "PRKC",
        "CTDA",
        "Type",
        "EPFT",
        "PerkEntryIDUnique",
        "EPFB",
        "ButtonLabel",
        "EPF2",
        "ScriptFlags",
        "EPF3",
        "EndMarker",
        "PRKF",
        "CIS1",
        "CIS2",
    ] {
        assert!(
            !perk_map
                .drop_fields
                .iter()
                .any(|field| field == compatible_field),
            "{compatible_field} must not be dropped from FO4-compatible PERK entries"
        );
    }

    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut perk)
        .unwrap();
    let mut translated = match translator.translate(&perk, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated PERK, got {other:?}"),
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

    assert_eq!(
        translated
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect::<Vec<_>>(),
        vec![
            "VMAD", "PRKE", "DATA", "PRKC", "CTDA", "CIS1", "CIS2", "EPFT", "EPFB", "EPF2", "EPF3",
            "PRKF", "PRKE", "DATA", "PRKC", "CTDA", "CIS1", "CIS2", "EPFT", "EPFB", "EPF2", "EPF3",
            "PRKF",
        ]
    );

    let prke_values = translated
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "PRKE")
        .map(|entry| &entry.value)
        .collect::<Vec<_>>();
    assert_eq!(
        prke_values,
        vec![
            &FieldValue::Bytes(SmallVec::from_slice(&[0x02, 0x00, 0x00])),
            &FieldValue::Bytes(SmallVec::from_slice(&[0x02, 0x00, 0x00])),
        ]
    );

    let entry_data_values = translated
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "DATA")
        .map(|entry| &entry.value)
        .collect::<Vec<_>>();
    assert_eq!(
        entry_data_values,
        vec![
            &FieldValue::Bytes(SmallVec::from_slice(&[0x0E, 0x09, 0x02])),
            &FieldValue::Bytes(SmallVec::from_slice(&[0x0E, 0x09, 0x02])),
        ]
    );

    let labels = translated
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "EPF2")
        .map(|entry| &entry.value)
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        vec![
            &FieldValue::Bytes(SmallVec::from_slice(b"First choice\0")),
            &FieldValue::Bytes(SmallVec::from_slice(b"Second choice\0")),
        ]
    );
}
