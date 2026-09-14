#[test]
fn pair_hook_projects_audited_steel_battleaxe_before_generic_translation() {
    let interner = StringInterner::new();
    let plugin = interner.intern("Skyrim.esm");
    let mut record = Record::new(
        SigCode::from_str("WEAP").unwrap(),
        FormKey {
            local: 0x013984,
            plugin,
        },
    );
    record.eid = Some(interner.intern("SteelBattleaxe"));
    let mut dnam = [0_u8; 100];
    dnam[0] = 6;
    record.fields.extend([
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(interner.intern("SteelBattleaxe")),
        },
        FieldEntry {
            sig: SubrecordSig(*b"OBND"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0; 12])),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FULL"),
            value: FieldValue::String(interner.intern("Steel Battleaxe")),
        },
        FieldEntry {
            sig: SubrecordSig(*b"MODL"),
            value: FieldValue::String(interner.intern("Weapons\\Steel\\SteelBattleAxe.nif")),
        },
        FieldEntry {
            sig: SubrecordSig(*b"ETYP"),
            value: FieldValue::FormKey(FormKey {
                local: 0x013F45,
                plugin,
            }),
        },
        FieldEntry {
            sig: SubrecordSig(*b"WNAM"),
            value: FieldValue::FormKey(FormKey {
                local: 0x020E27,
                plugin,
            }),
        },
        FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[
                100, 0, 0, 0, 0, 0, 168, 65, 18, 0,
            ])),
        },
        FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&dnam)),
        },
    ]);

    let translator = Translator::new(Game::SkyrimSe, Game::Fo4).unwrap();
    translator
        .pre_translate(
            &mut PairCtx::new(&interner),
            &mut record,
        )
        .unwrap();
    let mut record = match translator.translate(&record, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("projected Steel Battleaxe must translate, got {other:?}"),
    };
    translator
        .post_translate(
            &mut PairCtx::new(&interner),
            &mut record,
        )
        .unwrap();

    assert!(record.fields.iter().any(|field| {
        field.sig.0 == *b"MOD4"
            && matches!(&field.value, FieldValue::String(path) if interner.resolve(*path) == Some("Weapons\\Steel\\1stPersonSteelBattleAxe.nif"))
    }));
    assert!(matches!(
        record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"ETYP")
            .map(|field| &field.value),
        Some(FieldValue::FormKey(form_key))
            if form_key.local == 0x013F42
                && interner.resolve(form_key.plugin) == Some("Fallout4.esm")
    ));
    let FieldValue::Struct(dnam) = &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DNAM")
        .unwrap()
        .value
    else {
        panic!("generic translation overwrote projected DNAM");
    };
    assert!(dnam.iter().any(|(name, value)| {
        interner.resolve(*name) == Some("animation_type") && *value == FieldValue::Uint(5)
    }));
    assert_eq!(
        record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CRDT")
            .count(),
        1
    );
}
