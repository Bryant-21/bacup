

    #[test]
    fn post_translate_preserves_pack_location_and_target_references() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        push_field(&mut record, "PLDT", raw_bytes(&[0, 0x52, 0x7D, 0x52, 0]));
        push_field(&mut record, "PTDA", raw_bytes(&[0, 0x52, 0x7D, 0x52, 0]));
        push_field(&mut record, "CNAM", raw_bytes(&[1, 2, 3, 4]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"PKCU"));
        assert!(sigs.contains(&"CNAM"));
        assert!(sigs.contains(&"PLDT"));
        assert!(sigs.contains(&"PTDA"));
    }

    fn pldt_bytes(type_value: i32, location_value: u32) -> FieldValue {
        let mut raw = Vec::with_capacity(16);
        raw.extend_from_slice(&type_value.to_le_bytes());
        raw.extend_from_slice(&location_value.to_le_bytes());
        raw.extend_from_slice(&(-1i32).to_le_bytes()); // Radius
        raw.extend_from_slice(&0u32.to_le_bytes()); // Collection Index
        raw_bytes(&raw)
    }

    fn ptda_bytes(type_value: i32, target_value: u32) -> FieldValue {
        let mut raw = Vec::with_capacity(12);
        raw.extend_from_slice(&type_value.to_le_bytes());
        raw.extend_from_slice(&target_value.to_le_bytes());
        raw.extend_from_slice(&1i32.to_le_bytes()); // Count / Distance
        raw_bytes(&raw)
    }

    #[test]
    fn post_translate_leaves_non_alias_pack_location_types_untouched() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        // PLDT Type 0 (Reference) carries a FormID, not an alias — must survive verbatim.
        push_field(&mut record, "PLDT", pldt_bytes(0, 0x0001_2345));
        // PTDA Type 1 (Object ID) — not an alias.
        push_field(&mut record, "PTDA", ptda_bytes(1, 0x0006_789A));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let pldt = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "PLDT")
            .unwrap();
        assert_eq!(
            pldt.value,
            pldt_bytes(0, 0x0001_2345),
            "non-alias PLDT untouched"
        );
        let ptda = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "PTDA")
            .unwrap();
        assert_eq!(
            ptda.value,
            ptda_bytes(1, 0x0006_789A),
            "non-alias PTDA untouched"
        );
    }

    #[test]
    fn post_translate_maps_pack_fallback_procedure_to_sequence() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        push_field(&mut record, "ANAM", raw_bytes(b"Fallback\0"));
        push_field(&mut record, "XNAM", raw_bytes(&[0]));
        push_field(&mut record, "ANAM", raw_bytes(b"Fallback\0"));
        push_field(
            &mut record,
            "ANAM",
            FieldValue::String(interner.intern("Fallback")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let anams: Vec<String> = record
            .fields
            .iter()
            .filter(|field| matches!(field.sig.as_str(), "ANAM" | "PNAM"))
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => {
                    let value = bytes
                        .as_slice()
                        .strip_suffix(&[0])
                        .unwrap_or(bytes.as_slice());
                    String::from_utf8(value.to_vec()).unwrap()
                }
                FieldValue::String(sym) => interner.resolve(*sym).unwrap().to_string(),
                other => panic!("expected procedure string-ish value, got {other:?}"),
            })
            .collect();
        assert_eq!(anams, vec!["Fallback", "Sequence", "Sequence"]);
    }

    #[test]
    fn post_translate_rewrites_fo76_pack_procedure_tree_for_fo4() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        push_field(&mut record, "XNAM", raw_bytes(&[0x0D]));
        push_field(&mut record, "ANAM", raw_bytes(b"Stacked\0"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PRCB", raw_bytes(&[1, 0, 0, 0, 0, 0, 0, 0]));
        push_field(&mut record, "PNAM", raw_bytes(b"Foll"));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "PKC2", FieldValue::Uint(0));
        push_field(&mut record, "ANAM", raw_bytes(b"Procedure\0"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PNAM", raw_bytes(&[1, 0, 0, 0]));
        push_field(&mut record, "PKC2", FieldValue::Uint(1));
        push_field(&mut record, "UNAM", FieldValue::Uint(0));
        push_field(&mut record, "BNAM", raw_bytes(b"target\0"));
        push_field(&mut record, "PNAM", raw_bytes(&[1, 0, 0, 0]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec![
                "PKCU", "XNAM", "ANAM", "CITC", "PRCB", "ANAM", "CITC", "PNAM", "FNAM", "PKC2",
                "PKC2", "UNAM", "BNAM", "PNAM"
            ]
        );
        let procedure_names: Vec<String> = record
            .fields
            .iter()
            .take_while(|field| field.sig.as_str() != "UNAM")
            .filter(|field| field.sig.as_str() == "PNAM")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => {
                    String::from_utf8(trim_nul_suffix(bytes.as_slice()).to_vec()).unwrap()
                }
                FieldValue::String(sym) => interner.resolve(*sym).unwrap().to_string(),
                other => panic!("expected procedure name, got {other:?}"),
            })
            .collect();
        assert_eq!(procedure_names, vec!["Follow"]);
    }

    #[test]
    fn post_translate_preserves_valid_fo4_procedures_with_long_names() {
        // FO76 packages carrying procedures whose names are valid FO4 procedures
        // (GuardArea, Hover, KeepAnEyeOn, LockDoors, UseMagic, Acquire, FollowTo)
        // must survive the procedure-tree rewrite, not be dropped.
        // Values are interned strings, matching the schema-decoded production form.
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        let s = |v: &str| FieldValue::String(interner.intern(v));
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        push_field(&mut record, "XNAM", raw_bytes(&[0x0D]));
        push_field(&mut record, "ANAM", s("Simultaneous"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PRCB", raw_bytes(&[1, 0, 0, 0, 0, 0, 0, 0]));
        push_field(&mut record, "ANAM", s("Procedure"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PNAM", s("GuardArea"));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "PKC2", FieldValue::Uint(0));
        push_field(&mut record, "ANAM", s("Procedure"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PNAM", s("Sandbox"));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "PKC2", FieldValue::Uint(0));
        push_field(&mut record, "UNAM", FieldValue::Uint(0));
        push_field(&mut record, "BNAM", s("target"));
        push_field(&mut record, "PNAM", raw_bytes(&[1, 0, 0, 0]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let proc_names: Vec<String> = record
            .fields
            .iter()
            .take_while(|f| f.sig.as_str() != "UNAM")
            .filter(|f| f.sig.as_str() == "PNAM")
            .map(|f| match &f.value {
                FieldValue::Bytes(b) => {
                    String::from_utf8(trim_nul_suffix(b.as_slice()).to_vec()).unwrap()
                }
                FieldValue::String(sym) => interner.resolve(*sym).unwrap().to_string(),
                other => panic!("expected procedure name, got {other:?}"),
            })
            .collect();
        assert_eq!(
            proc_names,
            vec!["GuardArea".to_string(), "Sandbox".to_string()],
            "GuardArea is a valid FO4 procedure and must be preserved, not dropped"
        );

        // Each long-name procedure maps to itself.
        for name in [
            "GuardArea",
            "Hover",
            "KeepAnEyeOn",
            "LockDoors",
            "UseMagic",
            "Acquire",
            "FollowTo",
        ] {
            let entry = FieldEntry {
                sig: SubrecordSig::from_str("PNAM").unwrap(),
                value: FieldValue::String(interner.intern(name)),
            };
            assert_eq!(
                Fo76Fo4Hook::fo76_pack_procedure_name(&interner, &entry),
                Some(name),
                "{name} must map to itself"
            );
        }
    }

    #[test]
    fn post_translate_maps_observed_fo76_pack_procedure_codes() {
        let interner = StringInterner::new();
        let names = [
            (b"Trav".as_slice(), "Travel"),
            (b"Rang".as_slice(), "Range"),
            (b"Unlo".as_slice(), "UnlockDoors"),
            (b"Hold".as_slice(), "HoldPosition"),
            (b"Say\0".as_slice(), "ForceGreet"),
            (b"UseI".as_slice(), "UseIdleMarker"),
        ];

        for (raw, expected) in names {
            let mut entry = FieldEntry {
                sig: SubrecordSig::from_str("PNAM").unwrap(),
                value: raw_bytes(raw),
            };
            let mapped =
                Fo76Fo4Hook::fo76_pack_procedure_name(&interner, &entry).expect("mapped procedure");
            assert_eq!(mapped, expected);
            Fo76Fo4Hook::set_pack_tree_text_value(&interner, &mut entry, mapped);
            match entry.value {
                FieldValue::String(sym) => {
                    assert_eq!(interner.resolve(sym), Some(expected));
                }
                other => panic!("expected interned string, got {other:?}"),
            }
        }
    }

    #[test]
    fn pre_translate_converts_fo76_mgef_data_to_fo4_layout() {
        let interner = StringInterner::new();
        let mut record = make_record("MGEF", &interner);
        let mut data = vec![0_u8; FO76_MGEF_DATA_LEN];
        data[0..4].copy_from_slice(&0xAABBCCDD_u32.to_le_bytes());
        data[4..8].copy_from_slice(&0x11223344_u32.to_le_bytes());
        data[68..72].copy_from_slice(&36_u32.to_le_bytes());
        data[72..76].copy_from_slice(&0x00000823_u32.to_le_bytes());
        data[140..144].copy_from_slice(&0x00110839_u32.to_le_bytes());
        data[156..160].copy_from_slice(&0xDEADBEEF_u32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(data)),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(bytes.len(), FO4_MGEF_DATA_LEN);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0xAABBCCDD
        );
        assert_eq!(u32::from_le_bytes(bytes[64..68].try_into().unwrap()), 36);
        assert_eq!(
            u32::from_le_bytes(bytes[68..72].try_into().unwrap()),
            0x00000823
        );
        assert_eq!(
            u32::from_le_bytes(bytes[136..140].try_into().unwrap()),
            0x00110839
        );
    }
