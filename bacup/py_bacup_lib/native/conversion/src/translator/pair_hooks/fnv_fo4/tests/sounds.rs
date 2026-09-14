use super::sounds::{
    AUDIO_CATEGORY_SFX, AUDIO_CATEGORY_UI, LOOP, SOM_UI_DEFAULT,
    legacy_sound_descriptor_substitution_mappings, target_form_key,
};

fn field<'a>(record: &'a Record, signature: &[u8; 4]) -> &'a FieldValue {
    &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *signature)
        .unwrap_or_else(|| panic!("missing {}", String::from_utf8_lossy(signature)))
        .value
}

fn form_key_value(record: &Record, signature: &[u8; 4]) -> FormKey {
    let FieldValue::FormKey(form_key) = field(record, signature) else {
        panic!("{} must be a FormKey", String::from_utf8_lossy(signature));
    };
    *form_key
}

#[test]
fn pre_translate_lowers_pipboy_soun_to_standard_fo4_sndr() {
    let interner = StringInterner::new();
    let mut record = make_record("SOUN", &interner);
    record.eid = Some(interner.intern("UIPipBoyHumLP"));
    push_field(
        &mut record,
        "FNAM",
        FieldValue::String(interner.intern("fx\\ui\\pipboy\\ui_pipboy_hum_lp.wav")),
    );
    let mut sndx = vec![70, 10, 0, 0];
    sndx.extend_from_slice(&LOOP.to_le_bytes());
    sndx.extend_from_slice(&1542_i16.to_le_bytes());
    sndx.extend_from_slice(&[0, 0]);
    push_field(
        &mut record,
        "SNDX",
        FieldValue::Bytes(smallvec::SmallVec::from_vec(sndx)),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(record.sig.as_str(), "SNDR");
    assert_eq!(
        form_key_value(&record, b"GNAM"),
        target_form_key(AUDIO_CATEGORY_UI, &interner)
    );
    assert_eq!(
        form_key_value(&record, b"ONAM"),
        target_form_key(SOM_UI_DEFAULT, &interner)
    );
    let FieldValue::String(sound_path) = field(&record, b"ANAM") else {
        panic!("ANAM must be a string");
    };
    assert_eq!(
        interner.resolve(*sound_path),
        Some("data\\Sound\\fx\\ui\\pipboy\\ui_pipboy_hum_lp.wav")
    );
    assert_eq!(
        field(&record, b"LNAM"),
        &FieldValue::Bytes(smallvec::smallvec![0, 8, 0, 0])
    );
    assert_eq!(
        field(&record, b"BNAM"),
        &FieldValue::Bytes(smallvec::smallvec![0, 0, 128, 0, 0x06, 0x06])
    );
    assert!(record.fields.iter().all(|field| !matches!(
        field.sig.0,
        sig if sig == *b"FNAM" || sig == *b"SNDX" || sig == *b"SNDD"
    )));
}

#[test]
fn pre_translate_preserves_extended_sound_playback_values() {
    let interner = StringInterner::new();
    let mut record = make_record("SOUN", &interner);
    record.eid = Some(interner.intern("WPNConvertedFire"));
    push_field(
        &mut record,
        "FNAM",
        FieldValue::String(interner.intern("Sound/fx/wpn/converted/fire/")),
    );
    let mut sndd = vec![20, 15, (-7_i8) as u8, 0];
    sndd.extend_from_slice(&1_u32.to_le_bytes());
    sndd.extend_from_slice(&881_i16.to_le_bytes());
    sndd.extend_from_slice(&[0, 0]);
    sndd.extend_from_slice(&[100, 0, 50, 0, 20, 0, 5, 0, 0, 0]);
    sndd.extend_from_slice(&0_i16.to_le_bytes());
    sndd.extend_from_slice(&200_i32.to_le_bytes());
    sndd.extend_from_slice(&0_i32.to_le_bytes());
    sndd.extend_from_slice(&0_i32.to_le_bytes());
    assert_eq!(sndd.len(), 36);
    push_field(
        &mut record,
        "SNDD",
        FieldValue::Bytes(smallvec::SmallVec::from_vec(sndd)),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(record.sig.as_str(), "SNDR");
    assert_eq!(
        form_key_value(&record, b"GNAM"),
        target_form_key(AUDIO_CATEGORY_SFX, &interner)
    );
    assert_eq!(
        form_key_value(&record, b"ONAM"),
        target_form_key(0x074808, &interner)
    );
    assert_eq!(
        field(&record, b"BNAM"),
        &FieldValue::Bytes(smallvec::smallvec![0, 7, 200, 0, 0x71, 0x03])
    );
}

#[test]
fn fo3_hook_uses_the_same_legacy_sound_lowering_contract() {
    let interner = StringInterner::new();
    let mut record = make_record("SOUN", &interner);
    push_field(
        &mut record,
        "FNAM",
        FieldValue::String(interner.intern("fx\\amb\\wind.wav")),
    );

    Fo3Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(record.sig.as_str(), "SNDR");
    assert!(record.fields.iter().any(|field| field.sig.0 == *b"ANAM"));
}

#[test]
fn lowered_legacy_soun_reaches_the_translated_terminal() {
    let interner = StringInterner::new();
    let mut record = make_record("SOUN", &interner);
    push_field(
        &mut record,
        "FNAM",
        FieldValue::String(interner.intern("fx\\amb\\wind.wav")),
    );
    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();
    let translator = crate::translator::Translator::new(Game::Fnv, Game::Fo4).unwrap();

    let crate::translator::TranslateResult::Translated(translated) =
        translator.translate_ignoring_skip(&record, &interner, "SOUN")
    else {
        panic!("lowered SOUN must not be dropped after it becomes SNDR");
    };

    assert_eq!(translated.sig.as_str(), "SNDR");
}

#[test]
fn legacy_sound_collision_substitutes_the_fo4_sndr_across_signature_change() {
    let interner = StringInterner::new();
    let source = FormKey::parse("018E9C@FalloutNV.esm", &interner).unwrap();
    let target = FormKey::parse("021B17@Fallout4.esm", &interner).unwrap();
    let unrelated = FormKey::parse("012345@FalloutNV.esm", &interner).unwrap();
    let source_entries = [
        (interner.intern("UIPipBoyHumLP"), source, SigCode(*b"SOUN")),
        (interner.intern("NotLegacy"), unrelated, SigCode(*b"SNDR")),
    ];
    let target_entries = rustc_hash::FxHashMap::from_iter([(
        interner.intern("uipipboyhumlp"),
        vec![(target, SigCode(*b"SNDR"))],
    )]);

    assert_eq!(
        legacy_sound_descriptor_substitution_mappings(&source_entries, &target_entries, &interner),
        vec![(source, target)]
    );
}

#[test]
fn mapped_legacy_sound_reference_resolves_to_sndr_target() {
    let interner = StringInterner::new();
    let source = FormKey::parse("018E9C@FalloutNV.esm", &interner).unwrap();
    let target = FormKey::parse("021B17@Fallout4.esm", &interner).unwrap();
    let mut mapper = legacy_magic_mapper(&interner);
    mapper.add_mapping(source, target);
    let mut weapon = make_record("WEAP", &interner);
    push_field(&mut weapon, "SNAM", FieldValue::FormKey(source));

    mapper.rewrite_record(&mut weapon).unwrap();

    assert_eq!(field(&weapon, b"SNAM"), &FieldValue::FormKey(target));
}
