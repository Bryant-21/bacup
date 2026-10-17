use super::*;
use crate::record::FieldEntry;

#[test]
fn fnv_imagespace_luminance_contract_rejects_unrepresented_additive_pivot() {
    let interner = StringInterner::new();
    let mut record = Record::new(
        crate::ids::SigCode(*b"IMAD"),
        FormKey {
            plugin: interner.intern("FalloutNV.esm"),
            local: NELLIS_IMAD,
        },
    );
    for (index, values) in [(19, [1.0_f32, 0.7]), (83, [0.0, 0.0])] {
        record.fields.push(FieldEntry {
            sig: SubrecordSig([index, b'I', b'A', b'D']),
            value: FieldValue::Bytes(
                [0.0, values[0], 1.0, values[1]]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect(),
            ),
        });
    }
    assert_eq!(
        average_luminance_curve(&record, &interner).unwrap()[1].value,
        0.7
    );
    let FieldValue::Bytes(bytes) = &mut record.fields[1].value else {
        unreachable!()
    };
    bytes[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    assert!(average_luminance_curve(&record, &interner).is_err());
}

#[test]
#[ignore = "requires installed FalloutNV.esm"]
fn source_nellis_imagespace_preserves_curves_through_relayout_and_serialization() {
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use crate::translator::pair_hook::PairCtx;
    use crate::translator::target_hook::TargetCtx;
    use crate::translator::{TranslateResult, Translator};
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_load_no_py, plugin_handle_new_no_py,
        plugin_handle_save_no_py,
    };

    let path =
        std::env::var("MODKIT_FNV_CREATURE_MAGIC_PLUGIN").expect("FalloutNV.esm source path");
    let source_handle = plugin_handle_load_no_py(&path, Some("fnv"), None, None, true).unwrap();
    let interner = StringInterner::new();
    let source_schema = AuthoringSchema::for_game("fnv").unwrap();
    let target_schema = AuthoringSchema::for_game("fo4").unwrap();
    let relayout = crate::struct_relayout::StructRelayoutCtx {
        target_schema: &target_schema,
        target_form_version: 131,
        legacy_bptd_only: false,
    };
    let mut record = crate::source_read::read_record_relayout(
        source_handle,
        "019CED@FalloutNV.esm",
        &source_schema,
        &interner,
        Some(&relayout),
    )
    .unwrap();
    assert!(plugin_handle_close_native(source_handle));
    let curve = average_luminance_curve(&record, &interner).unwrap();
    assert_eq!(
        curve,
        vec![
            CurvePoint {
                time: 0.0,
                value: 1.0
            },
            CurvePoint {
                time: 0.04,
                value: 0.7
            },
            CurvePoint {
                time: 1.0,
                value: 1.0
            }
        ]
    );
    let blur = float_rows(&record, *b"BNAM", 2, &interner).unwrap();
    assert_eq!(blur.len(), 4);
    let contrast = float_rows(&record, [18, b'I', b'A', b'D'], 2, &interner).unwrap();
    let brightness = float_rows(&record, [20, b'I', b'A', b'D'], 2, &interner).unwrap();
    let translator = Translator::new(Game::Fnv, Game::Fo4).unwrap();
    let mut context = PairCtx::with_source(&interner, "FalloutNV.esm", &[]);
    translator.pre_translate(&mut context, &mut record).unwrap();
    let once = record.fields.clone();
    translator.pre_translate(&mut context, &mut record).unwrap();
    assert_eq!(record.fields, once);
    let TranslateResult::Translated(mut record) =
        translator.translate_ignoring_skip(&record, &interner, "IMAD")
    else {
        panic!("Nellis IMAD translated")
    };
    translator
        .post_translate(&mut context, &mut record)
        .unwrap();
    assert!(is_lowered(&record, &interner));
    const OUTPUT: &str = "B21_NellisImageSpace.esp";
    record.form_key = FormKey {
        plugin: interner.intern(OUTPUT),
        local: 0x800,
    };
    translator
        .run_target_hook(
            &mut TargetCtx {
                interner: &interner,
            },
            &mut record,
        )
        .unwrap();
    let TargetRecordNormalization::Keep(record) =
        TargetRecordNormalizer::target_only_with_interner(&target_schema, &interner)
            .normalize(record)
    else {
        panic!("target IMAD retained")
    };
    let output = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    crate::target_write::add_record_native(output, record, &target_schema, &interner).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(output, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(output));
    let output =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    let record = crate::source_read::read_record(
        output,
        "000800@B21_NellisImageSpace.esp",
        &target_schema,
        &interner,
    )
    .unwrap();
    assert_eq!(float_rows(&record, *b"BNAM", 2, &interner).unwrap(), blur);
    assert_eq!(
        float_rows(&record, [19, b'I', b'A', b'D'], 2, &interner).unwrap(),
        contrast
    );
    assert_eq!(
        float_rows(&record, [18, b'I', b'A', b'D'], 2, &interner).unwrap(),
        brightness
    );
    let FieldValue::Bytes(data) = value(&record, *b"DNAM").unwrap() else {
        panic!("target raw DNAM")
    };
    assert_eq!(data.len(), 252);
    assert_eq!(f32::from_le_bytes(data[4..8].try_into().unwrap()), 1.5);
    for (offset, count) in [(0, 1), (8 + 18 * 8, 3), (8 + 19 * 8, 3), (0xB4, 4)] {
        assert_eq!(
            u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()),
            count
        );
    }
    assert!(plugin_handle_close_native(output));
}
