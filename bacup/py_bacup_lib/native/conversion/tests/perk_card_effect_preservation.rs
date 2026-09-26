use conversion_native::ids::{FormKey, SigCode, SubrecordSig};
use conversion_native::record::{FieldEntry, FieldValue, Record};
use conversion_native::schema::AuthoringSchema;
use conversion_native::sym::StringInterner;
use conversion_native::translator::class_a_normalize::normalize_flags_and_enums;
use conversion_native::translator::pair_hook::PairCtx;
use conversion_native::translator::{Game, TranslateResult, Translator};
use smallvec::SmallVec;

fn raw(signature: &[u8; 4], bytes: &[u8]) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*signature),
        value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
    }
}

#[test]
fn perk_effect_data_is_not_normalized_as_the_top_level_trait_flag() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let mut perk = Record::new(
        SigCode(*b"PERK"),
        FormKey {
            local: 0x800,
            plugin: interner.intern("Cards.esm"),
        },
    );
    perk.fields.extend([
        raw(b"DATA", &[0, 0, 1, 0, 1]),
        raw(b"PRKE", &[1, 0, 0]),
        raw(b"DATA", &[0x53, 0xBE, 0x31, 1]),
        raw(b"PRKF", &[]),
        raw(b"PRKE", &[0, 0, 0]),
        raw(b"DATA", &[0xA3, 0x12, 0, 1, 20, 0, 0, 0]),
        raw(b"PRKF", &[]),
        raw(b"PRKE", &[2, 0, 0]),
        raw(b"DATA", &[14, 9, 2]),
        raw(b"EPFT", &[1]),
        raw(b"EPFD", &1.25_f32.to_le_bytes()),
        raw(b"PRKF", &[]),
    ]);
    let expected = perk.fields.clone();
    normalize_flags_and_enums(&mut perk, &schema, &interner);
    assert_eq!(perk.fields, expected);
}

#[test]
fn fo76_perk_float_payload_survives_the_translation_map() {
    let interner = StringInterner::new();
    let mut perk = Record::new(
        SigCode(*b"PERK"),
        FormKey {
            local: 0x800,
            plugin: interner.intern("SeventySix.esm"),
        },
    );
    perk.fields.extend([
        raw(b"PRKE", &[2, 0]),
        raw(b"DATA", &[14, 1, 2, 0]),
        raw(b"EPFT", &[1]),
        raw(b"EPFD", &1.25_f32.to_le_bytes()),
        raw(b"PRKF", &[]),
    ]);
    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut perk)
        .unwrap();
    let TranslateResult::Translated(mut result) = translator.translate(&perk, &interner) else {
        panic!("expected translated perk");
    };
    translator
        .post_translate(&mut PairCtx::new(&interner), &mut result)
        .unwrap();
    assert!(
        result
            .fields
            .contains(&raw(b"EPFD", &1.25_f32.to_le_bytes()))
    );
}
