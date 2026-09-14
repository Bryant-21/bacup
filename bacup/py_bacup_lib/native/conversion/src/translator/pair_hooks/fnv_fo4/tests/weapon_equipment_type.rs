// -------------------------------------------------------------------------
// Behavior: legacy ETYP equipment-type ordinal -> FO4 EQUP reference
// -------------------------------------------------------------------------

fn etyp_form_key(record: &Record, interner: &StringInterner) -> (String, u32) {
    let fields: Vec<_> = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "ETYP")
        .collect();
    assert_eq!(fields.len(), 1, "expected exactly one ETYP");
    match &fields[0].value {
        FieldValue::FormKey(form_key) => (
            interner.resolve(form_key.plugin).unwrap().to_string(),
            form_key.local,
        ),
        other => panic!("ETYP must carry a FormKey, got {other:?}"),
    }
}

fn weap_with_equipment_type(equipment_type: i64, interner: &StringInterner) -> Record {
    let mut record = make_record("WEAP", interner);
    push_field(&mut record, "EDID", FieldValue::None);
    push_field(&mut record, "ETYP", FieldValue::Int(equipment_type));
    record
}

#[test]
fn fnv_weap_melee_equipment_type_becomes_the_fo4_right_hand_equip_slot() {
    let interner = StringInterner::new();
    let mut record = weap_with_equipment_type(3, &interner);

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        etyp_form_key(&record, &interner),
        (
            "Fallout4.esm".to_string(),
            crate::target_fo4_melee::FO4_RIGHT_HAND_EQUIP_LOCAL
        )
    );
}

#[test]
fn fnv_weap_unarmed_equipment_type_becomes_the_fo4_unarmed_equip_slot() {
    let interner = StringInterner::new();
    let mut record = weap_with_equipment_type(4, &interner);

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        etyp_form_key(&record, &interner),
        (
            "Fallout4.esm".to_string(),
            crate::target_fo4_melee::FO4_UNARMED_WEAPON_EQUIP_LOCAL
        )
    );
}

/// Every remaining `equip_type_enum` ordinal is a two-handed gun class in FO4
/// terms. Carried verbatim they resolve as `Fallout4.esm:00000N` marker STATs
/// (1 = DoorMarker, 2 = TravelMarker, 5 = DivineMarker, 6 = TempleMarker), which
/// the engine type-checks into a null equip slot.
#[test]
fn fnv_weap_gun_equipment_types_become_the_fo4_both_hands_equip_slot() {
    for equipment_type in [0, 1, 2, 5, 6, 7, 13] {
        let interner = StringInterner::new();
        let mut record = weap_with_equipment_type(equipment_type, &interner);

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            etyp_form_key(&record, &interner),
            (
                "Fallout4.esm".to_string(),
                crate::target_fo4_melee::FO4_BOTH_HANDS_EQUIP_LOCAL
            ),
            "equipment type {equipment_type}"
        );
    }
}

#[test]
fn fnv_weap_equipment_type_already_resolved_to_an_equp_is_left_alone() {
    let interner = StringInterner::new();
    let fallout4 = interner.intern("Fallout4.esm");
    let mut record = make_record("WEAP", &interner);
    push_field(&mut record, "EDID", FieldValue::None);
    push_field(
        &mut record,
        "ETYP",
        FieldValue::FormKey(FormKey {
            local: crate::target_fo4_melee::FO4_RIGHT_HAND_EQUIP_LOCAL,
            plugin: fallout4,
        }),
    );

    FnvFo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    assert_eq!(
        etyp_form_key(&record, &interner),
        (
            "Fallout4.esm".to_string(),
            crate::target_fo4_melee::FO4_RIGHT_HAND_EQUIP_LOCAL
        )
    );
}

/// Vanilla FO4 carries no ETYP on a consumable, so the legacy ordinal has no
/// target meaning; carried verbatim it resolves to BobbyPin (10) or dangles
/// entirely (11-13).
#[test]
fn fnv_consumable_legacy_equipment_type_is_dropped() {
    for sig in ["ALCH", "INGR"] {
        let interner = StringInterner::new();
        let mut record = make_record(sig, &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "ETYP", FieldValue::Int(12));

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert!(
            !record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "ETYP"),
            "{sig} kept a legacy ETYP"
        );
    }
}

#[test]
fn fo3_weap_and_consumable_equipment_types_get_the_same_treatment() {
    let interner = StringInterner::new();
    let mut weapon = weap_with_equipment_type(2, &interner);
    let mut consumable = make_record("ALCH", &interner);
    push_field(&mut consumable, "EDID", FieldValue::None);
    push_field(&mut consumable, "ETYP", FieldValue::Int(11));

    Fo3Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut weapon)
        .unwrap();
    Fo3Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut consumable)
        .unwrap();

    assert_eq!(
        etyp_form_key(&weapon, &interner),
        (
            "Fallout4.esm".to_string(),
            crate::target_fo4_melee::FO4_BOTH_HANDS_EQUIP_LOCAL
        )
    );
    assert!(
        !consumable
            .fields
            .iter()
            .any(|entry| entry.sig.as_str() == "ETYP")
    );
}
