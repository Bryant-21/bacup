

    fn property_row_with_function_type(
        interner: &StringInterner,
        property_id: u16,
        function_type: u64,
    ) -> FieldValue {
        FieldValue::Struct(vec![
            (interner.intern("value_type"), FieldValue::Uint(4)),
            (
                interner.intern("function_type"),
                FieldValue::Uint(function_type),
            ),
            (
                interner.intern("property"),
                FieldValue::Uint(property_id as u64),
            ),
            (interner.intern("value_1"), FieldValue::Uint(0)),
            (interner.intern("value_2"), FieldValue::Uint(0)),
            (interner.intern("step"), FieldValue::Float(0.0)),
        ])
    }

    fn property_ids(value: &FieldValue, interner: &StringInterner) -> Vec<u16> {
        let FieldValue::Struct(fields) = value else {
            panic!("expected struct");
        };
        let Some(FieldValue::List(properties)) = named_value(fields, "properties", interner) else {
            panic!("expected properties list");
        };
        properties
            .iter()
            .map(|property| {
                let FieldValue::Struct(row_fields) = property else {
                    panic!("expected property row struct");
                };
                field_value_to_u16(named_value(row_fields, "property", interner).unwrap())
                    .expect("property id")
            })
            .collect()
    }

    fn property_function_types(value: &FieldValue, interner: &StringInterner) -> Vec<u16> {
        let FieldValue::Struct(fields) = value else {
            panic!("expected struct");
        };
        let Some(FieldValue::List(properties)) = named_value(fields, "properties", interner) else {
            panic!("expected properties list");
        };
        properties
            .iter()
            .map(|property| {
                let FieldValue::Struct(row_fields) = property else {
                    panic!("expected property row struct");
                };
                named_value(row_fields, "function_type", interner)
                    .and_then(field_value_to_u16)
                    .unwrap_or(0)
            })
            .collect()
    }

    fn raw_property_row(property_id: u16) -> [u8; 24] {
        raw_property_row_with_function_type(property_id, 2)
    }

    fn raw_property_row_with_function_type(property_id: u16, function_type: u8) -> [u8; 24] {
        let mut row = [0; 24];
        row[0] = 4;
        row[4] = function_type;
        row[8..10].copy_from_slice(&property_id.to_le_bytes());
        row
    }

    fn raw_obts(property_ids: &[u16]) -> FieldValue {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&(property_ids.len() as u32).to_le_bytes());
        raw.extend_from_slice(&[0, 0, 0, 0]);
        raw.extend_from_slice(&(-1_i16).to_le_bytes());
        raw.push(1);
        raw.push(0);
        raw.push(0);
        raw.push(0);
        for property_id in property_ids {
            raw.extend_from_slice(&raw_property_row(*property_id));
        }
        FieldValue::Bytes(smallvec::SmallVec::from_vec(raw))
    }

    fn raw_obts_property_ids(value: &FieldValue) -> Vec<u16> {
        let FieldValue::Bytes(bytes) = value else {
            panic!("expected raw OBTS bytes");
        };
        let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let property_start = 18;
        (0..property_count)
            .map(|index| {
                let offset = property_start + index * 24;
                u16::from_le_bytes(bytes[offset + 8..offset + 10].try_into().unwrap())
            })
            .collect()
    }

    fn raw_omod_data(form_type: &[u8; 4], property_ids: &[u16]) -> FieldValue {
        let rows: Vec<[u8; 24]> = property_ids
            .iter()
            .map(|property_id| raw_property_row(*property_id))
            .collect();
        raw_omod_data_rows(form_type, &rows)
    }

    fn raw_omod_data_rows(form_type: &[u8; 4], rows: &[[u8; 24]]) -> FieldValue {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        raw.push(0);
        raw.push(0);
        raw.extend_from_slice(form_type);
        raw.push(0);
        raw.push(0);
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        for row in rows {
            raw.extend_from_slice(row);
        }
        FieldValue::Bytes(smallvec::SmallVec::from_vec(raw))
    }

    fn raw_omod_property_ids(value: &FieldValue) -> Vec<u16> {
        let FieldValue::Bytes(bytes) = value else {
            panic!("expected raw OMOD DATA bytes");
        };
        let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let property_start = 28;
        (0..property_count)
            .map(|index| {
                let offset = property_start + index * 24;
                u16::from_le_bytes(bytes[offset + 8..offset + 10].try_into().unwrap())
            })
            .collect()
    }

    fn raw_omod_property_function_types(value: &FieldValue) -> Vec<u8> {
        let FieldValue::Bytes(bytes) = value else {
            panic!("expected raw OMOD DATA bytes");
        };
        let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let property_start = 28;
        (0..property_count)
            .map(|index| {
                let offset = property_start + index * 24;
                bytes[offset + OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET]
            })
            .collect()
    }

    #[test]
    fn post_translate_adds_liberator_shell_robot_attach_point() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        record.eid = Some(interner.intern(LIBERATOR_BODY_ARMOR_OMOD_EDITOR_ID));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"NPC_") as u64),
                ),
                (interner.intern("items"), FieldValue::List(Vec::new())),
            ]),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"DATA")
            .unwrap();
        let FieldValue::Struct(fields) = &data.value else {
            panic!("expected structured OMOD DATA");
        };
        assert!(matches!(
            named_value_canonical(fields, "attach_point", &interner),
            Some(FieldValue::FormKey(form_key))
                if form_key.local == FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID
                    && interner.resolve(form_key.plugin) == Some(FO4_MASTER_NAME)
        ));
    }

    #[test]
    fn post_translate_adds_liberator_shell_robot_attach_point_to_raw_data() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        record.eid = Some(interner.intern(LIBERATOR_BODY_ARMOR_OMOD_EDITOR_ID));
        push_field(&mut record, "DATA", raw_omod_data(b"NPC_", &[]));

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"DATA")
            .unwrap();
        let FieldValue::Bytes(bytes) = &data.value else {
            panic!("expected raw OMOD DATA");
        };
        assert_eq!(
            read_u32_le_at(bytes, OMOD_DATA_ATTACH_POINT_OFFSET),
            Some(FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID)
        );
    }

    #[test]
    fn post_translate_exposes_liberator_race_armor_attach_slot() {
        let interner = StringInterner::new();
        let mut record = make_record("RACE", &interner);
        record.eid = Some(interner.intern(LIBERATOR_RACE_EDITOR_ID));
        push_field(&mut record, "APPR", FieldValue::List(Vec::new()));

        // Run twice: the slot must be added once, not appended per pass.
        for _ in 0..2 {
            Fo76Fo4Hook
                .post_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();
        }

        let appr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"APPR")
            .unwrap();
        let FieldValue::List(items) = &appr.value else {
            panic!("expected APPR formid list");
        };
        assert_eq!(
            items
                .iter()
                .filter(|item| matches!(
                    item,
                    FieldValue::FormKey(form_key)
                        if form_key.local == FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID
                            && interner.resolve(form_key.plugin) == Some(FO4_MASTER_NAME)
                ))
                .count(),
            1
        );
    }

    #[test]
    fn post_translate_exposes_liberator_race_armor_attach_slot_in_raw_appr() {
        let interner = StringInterner::new();
        let mut record = make_record("RACE", &interner);
        record.eid = Some(interner.intern(LIBERATOR_RACE_EDITOR_ID));
        // ap_customName, the only slot the FO76 race carries.
        push_field(
            &mut record,
            "APPR",
            FieldValue::Bytes(SmallVec::from_slice(&0x0047_A264_u32.to_le_bytes())),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let appr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"APPR")
            .unwrap();
        let FieldValue::Bytes(bytes) = &appr.value else {
            panic!("expected raw APPR");
        };
        let slots: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert_eq!(slots, vec![0x0047_A264, FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID]);
    }

    #[test]
    fn post_translate_leaves_other_race_attach_slots_alone() {
        let interner = StringInterner::new();
        let mut record = make_record("RACE", &interner);
        record.eid = Some(interner.intern("SomeOtherRace"));
        push_field(&mut record, "APPR", FieldValue::List(Vec::new()));

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let appr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"APPR")
            .unwrap();
        assert_eq!(appr.value, FieldValue::List(Vec::new()));
    }

    #[test]
    fn pre_translate_strips_tesla_cannon_receiver_base_model() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Weapons\\TeslaCannon\\Weapon_TeslaCannon.nif")),
        );
        push_field(&mut record, "MODT", raw_bytes(&[0; 20]));
        push_field(&mut record, "ENLT", raw_bytes(&[0; 4]));
        push_field(&mut record, "INDX", FieldValue::Uint(0));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"WEAP") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 34)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert!(!sigs.contains(&"MODL"));
        assert!(!sigs.contains(&"MODT"));
        assert!(!sigs.contains(&"ENLT"));
        assert!(!sigs.contains(&"INDX"));
        assert!(sigs.contains(&"DATA"));
    }

    #[test]
    fn pre_translate_keeps_other_indexed_omod_models() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Weapons\\Other\\Receiver.nif")),
        );
        push_field(&mut record, "INDX", FieldValue::Uint(0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "MODL")
        );
    }

    #[test]
    fn pre_translate_strips_model_fields_from_material_omod() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("ATX/BackPacks/Backpack_HoldAll.nif")),
        );
        push_field(
            &mut record,
            "MODT",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 20])),
        );
        push_field(&mut record, "MODB", FieldValue::Float(0.0));
        push_field(&mut record, "MODF", FieldValue::Uint(0));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 13)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert!(!sigs.contains(&"MODL"));
        assert!(!sigs.contains(&"MODT"));
        assert!(!sigs.contains(&"MODB"));
        assert!(!sigs.contains(&"MODF"));
        assert!(sigs.contains(&"DATA"));
    }

    // FO76 hangs the Enclave tint off the RECEIVER rather than a separate paint mod, so the
    // material-swap heuristic deleted the geometry the whole gun is built from. `WEAP.MODL`
    // is a geometry-free dummy receiver, so the weapon rendered as nothing at all.
    #[test]
    fn pre_translate_keeps_model_fields_for_structural_weapon_omod() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Weapons\\Plasma\\PlasmaReceiver.nif")),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"WEAP") as u64),
                ),
                (
                    interner.intern("attach_point"),
                    FieldValue::Uint(0x0002_4004), // ap_gun_receiver
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 89)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert!(sigs.contains(&"MODL"), "structural receiver lost its model");
    }

    // The narrowing must not turn the strip off: an OMOD that really does sit on the weapon
    // material slot is still the case this heuristic was written for.
    #[test]
    fn pre_translate_still_strips_model_fields_for_weapon_material_omod() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Weapons\\Plasma\\PlasmaReceiver.nif")),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"WEAP") as u64),
                ),
                (
                    interner.intern("attach_point"),
                    FieldValue::Uint(0x0024_A0D8), // ap_WeaponMaterial
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 89)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert!(!sigs.contains(&"MODL"));
    }

    #[test]
    fn pre_translate_keeps_power_armor_model_fields_from_material_omod() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(
                interner.intern("actors/powerarmor/characterassets/mods/PA_Hellfire_LArm.nif"),
            ),
        );
        push_field(
            &mut record,
            "MODT",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 20])),
        );
        push_field(&mut record, "MODB", FieldValue::Float(0.0));
        push_field(&mut record, "MODF", FieldValue::Uint(0));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 13)]),
                ),
            ]),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert!(sigs.contains(&"MODL"));
        assert!(sigs.contains(&"MODT"));
        assert!(sigs.contains(&"MODB"));
        assert!(sigs.contains(&"MODF"));
    }

    #[test]
    fn pre_translate_drops_redundant_omod_target_keyword_keeps_others() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        let ma_gun = FormKey::parse("37D0B2@SeventySix.esm", &interner).unwrap();
        let keeper = FormKey::parse("0ABCDE@SeventySix.esm", &interner).unwrap();
        push_field(
            &mut record,
            "MNAM",
            FieldValue::List(vec![
                FieldValue::FormKey(ma_gun),
                FieldValue::FormKey(keeper),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let mnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MNAM")
            .expect("MNAM must survive while it still has a keeper entry");
        let FieldValue::List(items) = &mnam.value else {
            panic!("expected MNAM list");
        };
        let locals: Vec<u32> = items
            .iter()
            .map(|item| match item {
                FieldValue::FormKey(fk) => fk.local,
                other => panic!("expected FormKey, got {other:?}"),
            })
            .collect();
        assert_eq!(
            locals,
            vec![0xABCDE],
            "ma_Gun_Appearance must be dropped from MNAM; the keeper must remain"
        );
    }

    #[test]
    fn pre_translate_removes_emptied_omod_mnam_subrecord() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        let ma_gun = FormKey::parse("37D0B2@SeventySix.esm", &interner).unwrap();
        push_field(
            &mut record,
            "MNAM",
            FieldValue::List(vec![FieldValue::FormKey(ma_gun)]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "MNAM"),
            "an MNAM array emptied by the filter must be removed entirely"
        );
    }

    #[test]
    fn pre_translate_drops_redundant_omod_target_keyword_in_raw_bytes() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        // On-disk FO76 MNAM array: ma_Gun_Appearance (0737D0B2, high byte
        // retained in raw bytes) followed by an unrelated keeper keyword.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x0737_D0B2_u32.to_le_bytes());
        bytes.extend_from_slice(&0x0700_ABCD_u32.to_le_bytes());
        push_field(&mut record, "MNAM", raw_bytes(&bytes));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let mnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MNAM")
            .expect("MNAM must survive while it still has a keeper entry");
        let FieldValue::Bytes(out) = &mnam.value else {
            panic!("expected MNAM bytes");
        };
        assert_eq!(
            out.as_slice(),
            &0x0700_ABCD_u32.to_le_bytes(),
            "only the ma_Gun_Appearance row should be removed from the raw array"
        );
    }

    #[test]
    fn pre_translate_keeps_material_omod_appearance_target_keyword() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        let ma_gun = FormKey::parse("37D0B2@SeventySix.esm", &interner).unwrap();
        push_field(
            &mut record,
            "MNAM",
            FieldValue::List(vec![FieldValue::FormKey(ma_gun)]),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"WEAP") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 89)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let mnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MNAM")
            .expect("material OMOD appearance target keyword must survive for FO4 mapping");
        let FieldValue::List(items) = &mnam.value else {
            panic!("expected MNAM list");
        };
        assert_eq!(items.len(), 1);
        let FieldValue::FormKey(fk) = &items[0] else {
            panic!("expected FormKey");
        };
        assert_eq!(fk.local, 0x0037_D0B2);
    }

    #[test]
    fn post_translate_dedupes_mapped_omod_target_keywords() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        let pa_material = FormKey::parse("18DFCB@Fallout4.esm", &interner).unwrap();
        let pa_helmet = FormKey::parse("182CD5@Fallout4.esm", &interner).unwrap();
        push_field(
            &mut record,
            "MNAM",
            FieldValue::List(vec![
                FieldValue::FormKey(pa_material),
                FieldValue::FormKey(pa_material),
                FieldValue::FormKey(pa_helmet),
            ]),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let mnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MNAM")
            .expect("MNAM");
        assert_eq!(
            mnam.value,
            FieldValue::List(vec![
                FieldValue::FormKey(pa_material),
                FieldValue::FormKey(pa_helmet),
            ])
        );
    }

    #[test]
    fn post_translate_dedupes_raw_armor_keywords_and_syncs_count() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMO", &interner);
        push_field(
            &mut record,
            "KSIZ",
            raw_bytes(&3_u32.to_le_bytes()),
        );
        let mut keywords = Vec::new();
        keywords.extend_from_slice(&0x0018_DFCB_u32.to_le_bytes());
        keywords.extend_from_slice(&0x0018_DFCB_u32.to_le_bytes());
        keywords.extend_from_slice(&0x0018_2CD5_u32.to_le_bytes());
        push_field(&mut record, "KWDA", raw_bytes(&keywords));

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let kwda = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "KWDA")
            .expect("KWDA");
        let FieldValue::Bytes(keywords) = &kwda.value else {
            panic!("expected raw KWDA");
        };
        let expected = [
            0x0018_DFCB_u32.to_le_bytes(),
            0x0018_2CD5_u32.to_le_bytes(),
        ]
        .concat();
        assert_eq!(keywords.as_slice(), expected.as_slice());

        let ksiz = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "KSIZ")
            .expect("KSIZ");
        let FieldValue::Bytes(count) = &ksiz.value else {
            panic!("expected raw KSIZ");
        };
        assert_eq!(u32::from_le_bytes(count[..4].try_into().unwrap()), 2);
    }

    #[test]
    fn pre_translate_keeps_model_fields_for_non_material_omod() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Armor/BackPack.nif")),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 3)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "MODL")
        );
    }

    #[test]
    fn post_translate_drops_unknown_weap_object_template_property() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(
            &mut record,
            "OBTS",
            FieldValue::Struct(vec![
                (interner.intern("property_count"), FieldValue::Uint(2)),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![
                        property_row(&interner, 31),
                        property_row(&interner, 103),
                    ]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let obts = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "OBTS")
            .expect("OBTS remains");
        assert_eq!(property_ids(&obts.value, &interner), vec![31]);
        let FieldValue::Struct(fields) = &obts.value else {
            panic!("expected OBTS struct");
        };
        assert_eq!(
            field_value_to_u16(named_value(fields, "property_count", &interner).unwrap()),
            Some(1),
        );
    }

    #[test]
    fn post_translate_drops_unknown_raw_weap_object_template_property() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(&mut record, "OBTS", raw_obts(&[31, 103]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let obts = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "OBTS")
            .expect("OBTS remains");
        assert_eq!(raw_obts_property_ids(&obts.value), vec![31]);
    }

    #[test]
    fn post_translate_drops_unknown_omod_property_for_form_type() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (interner.intern("property_count"), FieldValue::Uint(2)),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![
                        property_row(&interner, 3),
                        property_row(&interner, 31),
                    ]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(property_ids(&data.value, &interner), vec![3]);
        let FieldValue::Struct(fields) = &data.value else {
            panic!("expected DATA struct");
        };
        assert_eq!(
            field_value_to_u16(named_value(fields, "property_count", &interner).unwrap()),
            Some(1),
        );
    }

    #[test]
    fn post_translate_drops_unknown_raw_omod_property_for_form_type() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(&mut record, "DATA", raw_omod_data(b"ARMO", &[3, 31]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(raw_omod_property_ids(&data.value), vec![3]);
    }

    #[test]
    fn post_translate_drops_mstt_omod_data() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![(
                interner.intern("form_type"),
                FieldValue::Uint(u32::from_le_bytes(*b"MSTT") as u64),
            )]),
        );
        push_field(
            &mut record,
            "FULL",
            FieldValue::String(interner.intern("Nuka Victory Wallpaper")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "DATA"),
            "MSTT OMOD DATA must be dropped"
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "FULL"),
            "non-DATA fields remain"
        );
    }

    #[test]
    fn post_translate_drops_raw_mstt_omod_data() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(&mut record, "DATA", raw_omod_data(b"MSTT", &[3]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "DATA"),
            "raw MSTT OMOD DATA must be dropped"
        );
    }

    #[test]
    fn post_translate_sets_material_swap_function_type() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"WEAP") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![
                        property_row_with_function_type(&interner, 89, 0),
                        property_row_with_function_type(&interner, 31, 0),
                    ]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(property_ids(&data.value, &interner), vec![89, 31]);
        assert_eq!(property_function_types(&data.value, &interner), vec![2, 0]);
    }

    #[test]
    fn post_translate_sets_raw_material_swap_function_type() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "DATA",
            raw_omod_data_rows(
                b"WEAP",
                &[
                    raw_property_row_with_function_type(89, 0),
                    raw_property_row_with_function_type(31, 0),
                ],
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(raw_omod_property_ids(&data.value), vec![89, 31]);
        assert_eq!(raw_omod_property_function_types(&data.value), vec![2, 0]);
    }

    #[test]
    fn post_translate_drops_raw_ctda_with_fo76_only_function_id() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MGEF", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(10017));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"EDID"));
        assert!(!sigs.contains(&"CTDA"));
    }

    /// `ap_PowerArmor_HeadMod` — the helmet's FO76 paint slot.
    const FO76_AP_POWER_ARMOR_HEAD_MOD: u32 = 0x0003_DAFC;
    /// `ap_PowerArmor_BodyMod` — the torso's.
    const FO76_AP_POWER_ARMOR_BODY_MOD: u32 = 0x0005_5F8C;
    /// `ap_PowerArmor_Lining` — a child slot the paint grants, not a part root.
    const FO4_AP_POWER_ARMOR_LINING: u32 = 0x0017_DAA9;

    #[test]
    fn post_translate_moves_power_armor_paint_to_fo4_paint_slot() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (
                    interner.intern("attach_point"),
                    form_key_value(&interner, FO76_AP_POWER_ARMOR_HEAD_MOD),
                ),
                (interner.intern("items"), FieldValue::List(Vec::new())),
            ]),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"DATA")
            .unwrap();
        let FieldValue::Struct(fields) = &data.value else {
            panic!("expected structured OMOD DATA");
        };
        assert!(matches!(
            named_value_canonical(fields, "attach_point", &interner),
            Some(FieldValue::FormKey(form_key))
                if form_key.local == FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID
                    && interner.resolve(form_key.plugin) == Some(FO4_MASTER_NAME)
        ));
    }

    #[test]
    fn post_translate_moves_power_armor_paint_to_fo4_paint_slot_in_raw_data() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        let mut data = raw_omod_data(b"ARMO", &[]);
        let FieldValue::Bytes(bytes) = &mut data else {
            panic!("expected raw OMOD DATA");
        };
        set_u32_le_at(
            bytes,
            OMOD_DATA_ATTACH_POINT_OFFSET,
            FO76_AP_POWER_ARMOR_BODY_MOD,
        );
        push_field(&mut record, "DATA", data);

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"DATA")
            .unwrap();
        let FieldValue::Bytes(bytes) = &data.value else {
            panic!("expected raw OMOD DATA");
        };
        assert_eq!(
            read_u32_le_at(bytes, OMOD_DATA_ATTACH_POINT_OFFSET),
            Some(FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID)
        );
    }

    #[test]
    fn post_translate_exposes_fo4_paint_slot_on_power_armor_piece() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMO", &interner);
        push_field(
            &mut record,
            "APPR",
            FieldValue::List(vec![
                form_key_value(&interner, FO76_AP_POWER_ARMOR_HEAD_MOD),
                // ap_Armor_SURV_Description, which must survive untouched.
                form_key_value(&interner, 0x003B_EEFD),
            ]),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let appr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"APPR")
            .unwrap();
        let FieldValue::List(items) = &appr.value else {
            panic!("expected APPR formid list");
        };
        assert!(matches!(
            &items[0],
            FieldValue::FormKey(form_key)
                if form_key.local == FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID
                    && interner.resolve(form_key.plugin) == Some(FO4_MASTER_NAME)
        ));
        assert_eq!(items[1], form_key_value(&interner, 0x003B_EEFD));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn post_translate_collapses_duplicate_paint_slots_in_raw_appr() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMO", &interner);
        let mut raw = Vec::new();
        raw.extend_from_slice(&FO76_AP_POWER_ARMOR_HEAD_MOD.to_le_bytes());
        raw.extend_from_slice(&FO76_AP_POWER_ARMOR_BODY_MOD.to_le_bytes());
        raw.extend_from_slice(&FO4_AP_POWER_ARMOR_LINING.to_le_bytes());
        push_field(
            &mut record,
            "APPR",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let appr = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"APPR")
            .unwrap();
        let FieldValue::Bytes(bytes) = &appr.value else {
            panic!("expected raw APPR");
        };
        let slots: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert_eq!(
            slots,
            vec![FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID, FO4_AP_POWER_ARMOR_LINING]
        );
    }

    #[test]
    fn post_translate_leaves_non_part_root_attach_points_alone() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (
                    interner.intern("attach_point"),
                    form_key_value(&interner, FO4_AP_POWER_ARMOR_LINING),
                ),
            ]),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"DATA")
            .unwrap();
        let FieldValue::Struct(fields) = &data.value else {
            panic!("expected structured OMOD DATA");
        };
        assert_eq!(
            named_value_canonical(fields, "attach_point", &interner),
            Some(&form_key_value(&interner, FO4_AP_POWER_ARMOR_LINING))
        );
    }
