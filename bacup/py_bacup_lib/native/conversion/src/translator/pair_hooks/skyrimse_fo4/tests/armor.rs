#[test]
fn armor_addon_uses_human_race_fo4_slots_and_no_source_additional_races() {
    let interner = StringInterner::new();
    let plugin = interner.intern("Skyrim.esm");
    let source_race = FormKey {
        local: 0x000019,
        plugin,
    };
    let mut record = Record::new(
        SigCode::from_str("ARMA").unwrap(),
        FormKey { local: 1, plugin },
    );
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"RNAM"),
        value: FieldValue::FormKey(source_race),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"BODT"),
        value: FieldValue::List(vec![
            FieldValue::String(interner.intern("32Body")),
            FieldValue::String(interner.intern("34Forearms")),
            FieldValue::String(interner.intern("38Calves")),
        ]),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"MODL"),
        value: FieldValue::FormKey(source_race),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"MOD2"),
        value: FieldValue::String(interner.intern("Armor\\Iron\\Male\\CuirassLight_1.nif")),
    });

    armor::normalize_skyrim_armor(&mut record, &interner);

    assert!(matches!(
        record.fields.iter().find(|field| field.sig.0 == *b"RNAM").map(|field| &field.value),
        Some(FieldValue::FormKey(form_key))
            if form_key.local == 0x013746
                && interner.resolve(form_key.plugin) == Some("Fallout4.esm")
    ));
    assert!(!record.fields.iter().any(|field| field.sig.0 == *b"MODL"));
    assert!(record.fields.iter().any(|field| field.sig.0 == *b"MOD2"));
    assert!(!record.fields.iter().any(|field| field.sig.0 == *b"BODT"));

    let FieldValue::List(slots) = &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"BOD2")
        .unwrap()
        .value
    else {
        panic!("expected biped slot list");
    };
    let slots = slots
        .iter()
        .filter_map(|value| match value {
            FieldValue::String(token) => interner.resolve(*token),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        slots,
        [
            "33BODY", "36UTorso", "37ULArm", "38URArm", "39ULLeg", "40URLLeg"
        ]
    );
}

#[test]
fn armor_raw_biped_mask_maps_hands_to_both_fo4_hand_slots() {
    let interner = StringInterner::new();
    let plugin = interner.intern("Skyrim.esm");
    let mut record = Record::new(
        SigCode::from_str("ARMO").unwrap(),
        FormKey { local: 1, plugin },
    );
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"BOD2"),
        value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[
            0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        ])),
    });

    armor::normalize_skyrim_armor(&mut record, &interner);

    let FieldValue::Uint(mask) = &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"BOD2")
        .unwrap()
        .value
    else {
        panic!("expected normalized biped mask");
    };
    assert_eq!(*mask, 0x30);
}
