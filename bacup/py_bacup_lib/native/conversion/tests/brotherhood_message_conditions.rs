use conversion_native::ids::{FormKey, SigCode, SubrecordSig};
use conversion_native::record::{FieldEntry, FieldValue, Record};
use conversion_native::sym::StringInterner;
use conversion_native::translator::pair_hook::PairCtx;
use conversion_native::translator::target_hook::TargetCtx;
use conversion_native::translator::{Game, TranslateResult, Translator};
use smallvec::SmallVec;

#[test]
fn penance_rockslide_keeps_button_conditions_and_instance_stage_gate() {
    let interner = StringInterner::new();
    let plugin = interner.intern("SeventySix.esm");
    let mut record = Record::new(
        SigCode::from_str("MESG").unwrap(),
        FormKey {
            local: 0x5F5DF1,
            plugin,
        },
    );
    let fixtures = [
        (
            "[Strength] Dig through with your shovel",
            vec![
                "60000000000080400E000000C2020000000000000000000000000000FFFFFFFF",
                "000000000000803F95030000F1385F003C0500000000000000000000FFFFFFFF",
                "600000000000803F2F000000202E4E00000000000000000000000000FFFFFFFF",
            ],
        ),
        (
            "Plant a Dynamite Bundle",
            vec![
                "000000000000803F95030000F1385F003C0500000000000000000000FFFFFFFF",
                "610000000000803F2F000000EBEE6000000000000000000000000000FFFFFFFF",
                "600000000000803F2F0000005AC15500000000000000000000000000FFFFFFFF",
            ],
        ),
    ];
    let mut expected = Vec::new();
    for (label, conditions) in fixtures {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ITXT"),
            value: FieldValue::String(interner.intern(label)),
        });
        for condition in conditions {
            let bytes = hex::decode(condition).unwrap();
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"CTDA"),
                value: FieldValue::Bytes(SmallVec::from_vec(bytes.clone())),
            });
            let mut converted = bytes;
            if converted[8..10] == 917_u16.to_le_bytes() {
                converted[8..10].copy_from_slice(&59_u16.to_le_bytes());
            }
            expected.push(FieldValue::Bytes(SmallVec::from_vec(converted)));
        }
    }
    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut record)
        .unwrap();
    let mut converted = match translator.translate(&record, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated MESG, got {other:?}"),
    };
    translator
        .post_translate(&mut PairCtx::new(&interner), &mut converted)
        .unwrap();
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut converted,
        )
        .unwrap();
    let conditions: Vec<_> = converted
        .fields
        .iter()
        .filter(|f| f.sig.0 == *b"CTDA")
        .map(|f| f.value.clone())
        .collect();
    assert_eq!(conditions, expected);
    let order: Vec<_> = converted
        .fields
        .iter()
        .filter(|f| matches!(&f.sig.0, b"ITXT" | b"CTDA"))
        .map(|f| f.sig.0)
        .collect();
    assert_eq!(
        order,
        vec![
            *b"ITXT", *b"CTDA", *b"CTDA", *b"CTDA", *b"ITXT", *b"CTDA", *b"CTDA", *b"CTDA"
        ]
    );
}
