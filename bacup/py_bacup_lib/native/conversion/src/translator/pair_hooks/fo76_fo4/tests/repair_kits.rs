#[test]
fn utility_items_lower_to_fo4_misc_or_alch_only_when_recognised() {
    let interner = StringInterner::new();
    for (name, local) in [("ATX_Utility_RepairKit_Basic", 0x41adeb), ("Utility_RepairKit_Improved", 0x41adec)] {
        let mut record = make_record("UTIL", &interner);
        record.form_key.local = local;
        record.eid = Some(interner.intern(name));
        push_field(&mut record, "MODL", FieldValue::String(interner.intern("props/repairkit/repairkitpristine.nif")));
        push_field(&mut record, "DATA", FieldValue::Bytes(SmallVec::from_slice(&[0, 0, 0, 0, 0xcd, 0xcc, 0xcc, 0x3d])));
        push_field(&mut record, "UITE", FieldValue::Bytes(SmallVec::from_slice(&[0; 16])));
        push_field(&mut record, "AQIC", FieldValue::Bytes(SmallVec::from_slice(&[0; 8])));
        push_field(&mut record, "XALG", FieldValue::Uint(1));
        Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
        assert_eq!(record.sig.as_str(), "MISC");
        assert_eq!(record.form_key.local, local);
        assert_eq!(interner.resolve(record.eid.unwrap()), Some(name));
        assert_eq!(record.fields.len(), 2);
        assert_eq!(record.fields[0].sig.as_str(), "MODL");
        assert_eq!(record.fields[1].sig.as_str(), "DATA");
    }

    {
        let mut record = make_record("UTIL", &interner);
        record.form_key.local = 0x54b4ec;
        record.eid = Some(interner.intern("ATX_Utility_ScrapToStash"));
        let use_sound = FormKey::parse("54FFEF@SeventySix.esm", &interner).unwrap();
        push_field(&mut record, "MODL", FieldValue::String(interner.intern("atx/utility/atx_scraptostash/atx_scraptostash.nif")));
        push_field(&mut record, "DATA", FieldValue::Bytes(SmallVec::from_slice(&[7, 0, 0, 0, 0xcd, 0xcc, 0xcc, 0x3d])));
        push_field(&mut record, "AQIC", FieldValue::Bytes(SmallVec::from_slice(&[0; 8])));
        push_field(&mut record, "UITE", FieldValue::Uint(0));
        push_field(&mut record, "UIUS", FieldValue::FormKey(use_sound));
        Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
        assert_eq!(record.sig.as_str(), "ALCH");
        assert_eq!(record.form_key.local, 0x54b4ec);
        let sigs: Vec<_> = record.fields.iter().map(|field| field.sig.as_str()).collect();
        assert_eq!(sigs, ["MODL", "DATA", "ENIT"]);
        assert_eq!(record.fields[1].value, FieldValue::Bytes(SmallVec::from_slice(&0.1f32.to_le_bytes())));
        let FieldValue::Struct(enit) = &record.fields[2].value else { panic!("ENIT must be a struct") };
        let names: Vec<_> = enit.iter().map(|(name, _)| interner.resolve(*name).unwrap()).collect();
        assert_eq!(names, ["value", "flags", "addiction", "addiction_chance", "sound_consume"]);
        assert_eq!(enit[0].1, FieldValue::Bytes(SmallVec::from_slice(&7u32.to_le_bytes())));
        assert_eq!(enit[1].1, FieldValue::Bytes(SmallVec::from_slice(&1u32.to_le_bytes())));
        assert_eq!(enit[4].1, FieldValue::FormKey(use_sound));
    }

    {
        let mut record = make_record("UTIL", &interner);
        record.eid = Some(interner.intern("Utility_ScrapKit"));
        Fo76Fo4Hook.pre_translate(&mut make_ctx(&interner), &mut record).unwrap();
        assert_eq!(record.sig.as_str(), "UTIL");
    }
}
