
    fn sunny_vendor_values() -> FieldValue {
        FieldValue::Bytes(SmallVec::from_slice(&[0, 0, 24, 0, 0, 0, 0, 0, 1, 0, 1, 0]))
    }

    fn faction_flags_value(flags: u32) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_slice(&flags.to_le_bytes()))
    }

    fn venv_bytes(record: &Record) -> &[u8] {
        let Some(FieldValue::Bytes(bytes)) = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"VENV")
            .map(|field| &field.value)
        else {
            panic!("expected raw VENV bytes");
        };
        bytes
    }

    #[test]
    fn post_translate_sells_everything_only_for_vendor_without_buy_sell_list() {
        let interner = StringInterner::new();
        for (name, faction_flags, buy_sell_list, sells_everything) in [
            ("vendor without buy/sell list", 0x4000, false, 1),
            ("vendor with buy/sell list whitelist", 0x4000, true, 0),
            ("non-vendor faction", 0, false, 0),
        ] {
            let mut record = make_record("FACT", &interner);
            push_field(&mut record, "DATA", faction_flags_value(faction_flags));
            if buy_sell_list {
                push_field(&mut record, "VEND", form_key_value(&interner, 0x359661));
            }
            push_field(&mut record, "VENV", sunny_vendor_values());

            Fo76Fo4Hook
                .post_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(
                venv_bytes(&record),
                &[0, 0, 24, 0, 0, 0, 0, 0, 1, sells_everything, 1, 0],
                "{name}"
            );
        }
    }
