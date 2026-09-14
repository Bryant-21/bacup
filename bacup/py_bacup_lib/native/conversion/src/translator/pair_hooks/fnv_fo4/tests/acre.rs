fn acre_fixture(interner: &StringInterner) -> (Record, FormKey, FormKey) {
    let source_placed = FormKey::parse("017A0A@FalloutNV.esm", interner).unwrap();
    let source_base = FormKey::parse("017A03@FalloutNV.esm", interner).unwrap();
    let mut record = Record::new(SigCode::from_str("ACRE").unwrap(), source_placed);
    record.flags = crate::record::RecordFlags::PERSISTENT;
    record.eid = Some(interner.intern("ShakesRef"));
    push_field(&mut record, "NAME", FieldValue::FormKey(source_base));
    push_field(
        &mut record,
        "DATA",
        FieldValue::Struct(vec![
            (interner.intern("PositionRotationPositionX"), FieldValue::Float(3404.2976)),
            (interner.intern("PositionRotationPositionY"), FieldValue::Float(993.3981)),
            (interner.intern("PositionRotationPositionZ"), FieldValue::Float(2800.0)),
            (interner.intern("PositionRotationRotationZ"), FieldValue::Float(0.0)),
            (interner.intern("scale"), FieldValue::Float(1.0)),
        ]),
    );
    (record, source_placed, source_base)
}

#[test]
fn acre_lowering_preserves_persistent_placed_payload_and_registers_alias_target() {
    let interner = StringInterner::new();
    let (mut record, source_placed, source_base) = acre_fixture(&interner);
    let source_data = record.fields[1].value.clone();
    let target_base = FormKey::parse("010123@Out.esm", &interner).unwrap();
    let target_placed = FormKey::parse("017A0A@Out.esm", &interner).unwrap();
    let mut mapper = legacy_magic_mapper(&interner);
    mapper.add_mapping(source_base, target_base);
    let mut aliases = PlacedActorAliasResolver::default();

    lower_acre_signature(&mut record).unwrap();
    let target = finalize_lowered_acre(
        &mut record,
        source_placed,
        target_placed,
        &mut mapper,
        &mut aliases,
        &interner,
    )
    .unwrap();

    assert_eq!(record.sig.as_str(), "ACHR");
    assert!(record.flags.contains(crate::record::RecordFlags::PERSISTENT));
    assert_eq!(record.fields[1].value, source_data, "DATA position/rotation/scale survives");
    assert_eq!(placed_actor_base_formkey(&record, &interner), Some(target_base));
    assert_eq!(mapper.lookup(source_placed), Some(target_placed));
    assert_eq!(aliases.resolve_direct(source_placed), Some(target));
    assert_eq!(target.source_base, source_base);
    assert_eq!(target.target_base, target_base);
}

#[test]
fn acre_lowering_rejects_an_unconverted_creature_base() {
    let interner = StringInterner::new();
    let (mut record, source_placed, source_base) = acre_fixture(&interner);
    let target_placed = FormKey::parse("017A0A@Out.esm", &interner).unwrap();
    let mut mapper = legacy_magic_mapper(&interner);
    let mut aliases = PlacedActorAliasResolver::default();

    lower_acre_signature(&mut record).unwrap();
    let err = finalize_lowered_acre(
        &mut record,
        source_placed,
        target_placed,
        &mut mapper,
        &mut aliases,
        &interner,
    )
    .unwrap_err();

    assert_eq!(
        err,
        super::acre::AcreLoweringError::UnmappedBase {
            placed: source_placed,
            base: source_base,
        }
    );
    assert_eq!(placed_actor_base_formkey(&record, &interner), Some(source_base));
    assert_eq!(aliases.resolve_direct(source_placed), None);
}

#[test]
fn acre_fixture_documents_cell_group_ownership_outside_the_record() {
    let cell_group = 9_u32;
    let (_, source_placed, _) = acre_fixture(&StringInterner::new());

    assert_eq!(cell_group, 9, "placed ACHR must remain in the CELL temporary group");
    assert_eq!(source_placed.local, 0x017A0A);
}
