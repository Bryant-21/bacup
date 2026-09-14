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

fn raw_hex(value: &str) -> FieldValue {
    FieldValue::Bytes(SmallVec::from_vec(hex::decode(value).unwrap()))
}

#[test]
fn fo76_term_menu_item_condition_survives_translation() {
    let interner = StringInterner::new();
    let source_plugin = interner.intern("SeventySix.esm");
    let mut terminal = Record::new(
        SigCode::from_str("TERM").unwrap(),
        FormKey {
            local: 0x0011_B1F3,
            plugin: source_plugin,
        },
    );
    terminal.eid = Some(interner.intern("FF05_Balance_TerminalSubEnviroMonitoring"));
    terminal.fields.extend([
        field(
            "PNAM",
            FieldValue::FormKey(FormKey {
                local: 0x0001_C035,
                plugin: source_plugin,
            }),
        ),
        field("ISIZ", FieldValue::Uint(9)),
        field(
            "ITXT",
            FieldValue::String(interner.intern("Upload Air Data")),
        ),
        field("ANAM", FieldValue::Uint(0)),
        field("ITID", FieldValue::Uint(3)),
        field(
            "UNAM",
            FieldValue::String(interner.intern("Commencing data transfer...")),
        ),
        field(
            "CTDA",
            raw_hex("000000000000803F3B0000002C010000000000000000000000000000FFFFFFFF"),
        ),
    ]);

    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    assert!(
        translator.maps.record_map("TERM").is_none(),
        "TERM has no translation-map drop rules"
    );

    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut terminal)
        .unwrap();
    let mut translated = match translator.translate(&terminal, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated TERM, got {other:?}"),
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

    let condition = translated
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "CTDA")
        .expect("terminal menu condition survives");
    assert_eq!(
        condition.value,
        raw_hex("000000000000803F3B0000002C010000000000000000000000000000FFFFFFFF")
    );
}
