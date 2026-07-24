

    #[test]
    fn pre_translate_chinese_stealth_arma_keeps_pipboy_visible() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMA", &interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("AA_ArmorChineseStealth")),
        );
        push_field(
            &mut record,
            "BOD2",
            FieldValue::Uint((1 << (33 - 30)) | (1 << (60 - 30))),
        );

        let hook = Fo76Fo4Hook;
        hook.pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let mask = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .and_then(|entry| match entry.value {
                FieldValue::Uint(mask) => Some(mask),
                _ => None,
            })
            .expect("ARMA BOD2 mask");
        assert_eq!(mask, 1 << (33 - 30));
    }

    fn raw_ctda(function_id: u16) -> FieldValue {
        raw_ctda_with_parameter_1(function_id, 0)
    }

    fn raw_ctda_with_parameter_1(function_id: u16, parameter_1: u32) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn raw_ctda_with_run_on(function_id: u16, parameter_1: u32, run_on: u32) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[20..24].copy_from_slice(&run_on.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    #[test]
    fn pre_translate_converts_nif_backed_empty_scol_to_stat() {
        let interner = StringInterner::new();
        let mut record = make_record("SCOL", &interner);
        let original_form_key = record.form_key;
        let eid = interner.intern("ToxicCreeperSC01_Copy02");
        record.eid = Some(eid);
        push_field(&mut record, "EDID", FieldValue::String(eid));
        push_field(
            &mut record,
            "OBND",
            FieldValue::Bytes(SmallVec::from_slice(&[0; 12])),
        );
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("SCOL\\SeventySix.esm\\CM007D2AB2.NIF")),
        );
        push_field(
            &mut record,
            "MODT",
            FieldValue::Bytes(SmallVec::from_slice(&[1, 2, 3, 4])),
        );
        push_field(
            &mut record,
            "ONAM",
            FieldValue::Bytes(SmallVec::from_slice(&[0; 8])),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_slice(&[0; 28])),
        );
        push_field(&mut record, "DEFL", FieldValue::Uint(0xA4E1));

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.sig, SigCode::from_str("STAT").unwrap());
        assert_eq!(record.form_key, original_form_key);
        assert_eq!(record.eid, Some(eid));
        assert_eq!(
            record
                .fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID", "OBND", "MODL", "MODT"]
        );
    }

    #[test]
    fn nonzero_health_cont_destructible_is_preserved() {
        let interner = StringInterner::new();
        let mut record = make_record("CONT", &interner);
        push_field(&mut record, "DEST", raw_bytes(&50_i32.to_le_bytes()));
        push_field(&mut record, "HGLB", raw_bytes(&[1, 0, 0, 0]));
        push_field(&mut record, "DSTD", raw_bytes(&[0; 28]));
        push_field(&mut record, "DMDL", raw_bytes(b"destroyed.nif\0"));
        push_field(&mut record, "DMDT", raw_bytes(&[0; 20]));
        push_field(&mut record, "ENLT", raw_bytes(&[0; 4]));
        push_field(&mut record, "ENLS", raw_bytes(&[0; 4]));
        push_field(&mut record, "AUUV", raw_bytes(&[0; 32]));
        push_field(&mut record, "DSTF", FieldValue::None);
        push_field(&mut record, "DATA", raw_bytes(&[1]));

        Fo76Fo4Hook::strip_zero_health_cont_destructibles(&interner, &mut record);

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec![
                "DEST", "HGLB", "DSTD", "DMDL", "DMDT", "ENLT", "ENLS", "AUUV", "DSTF", "DATA",
            ]
        );
    }

    fn read_vmad_string(bytes: &[u8], offset: &mut usize) -> String {
        let length = u16::from_le_bytes(bytes[*offset..*offset + 2].try_into().unwrap()) as usize;
        *offset += 2;
        let value = std::str::from_utf8(&bytes[*offset..*offset + length])
            .unwrap()
            .to_string();
        *offset += length;
        value
    }

    fn read_power_armor_vmad(bytes: &[u8]) -> (String, Vec<(String, u32)>) {
        let mut offset = 0;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            FO4_VMAD_VERSION
        );
        offset += 2;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            FO4_VMAD_OBJECT_FORMAT
        );
        offset += 2;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            1
        );
        offset += 2;

        let script_name = read_vmad_string(bytes, &mut offset);
        assert_eq!(bytes[offset], 0);
        offset += 1;
        let property_count =
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize;
        offset += 2;

        let mut properties = Vec::with_capacity(property_count);
        for _ in 0..property_count {
            let name = read_vmad_string(bytes, &mut offset);
            assert_eq!(bytes[offset], 1);
            offset += 1;
            assert_eq!(bytes[offset], VMAD_PROPERTY_FLAG_EDITED);
            offset += 1;
            assert_eq!(
                u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                0
            );
            offset += 2;
            assert_eq!(
                i16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                -1
            );
            offset += 2;
            let form_id = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            offset += 4;
            properties.push((name, form_id));
        }
        assert_eq!(offset, bytes.len());
        (script_name, properties)
    }

    #[test]
    fn pre_translate_preserves_scorched_statue_activation_conditions() {
        let interner = StringInterner::new();
        let mut record = make_record("ACTI", &interner);
        record.eid = Some(interner.intern("ScorchedStatue05"));
        push_field(&mut record, "FULL", FieldValue::None);
        push_field(&mut record, "CNDC", raw_bytes(&0_u32.to_le_bytes()));
        push_field(&mut record, "CITC", raw_bytes(&3_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(203));
        push_field(&mut record, "CTDA", raw_ctda(77));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda(FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID),
        );
        push_field(&mut record, "CNDC", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "CITC", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(203));
        push_field(&mut record, "FNAM", FieldValue::None);

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec![
                "FULL", "CNDC", "CITC", "CTDA", "CTDA", "CTDA", "CNDC", "CITC",
                "CTDA", "FNAM",
            ]
        );
    }

    #[test]
    fn pre_translate_converts_note_snam_scene_to_typed_formkey() {
        let interner = StringInterner::new();
        let mut record = make_record("NOTE", &interner);
        push_field(&mut record, "SNAM", FieldValue::Uint(0x0053_4F51));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        let FieldValue::FormKey(scene) = &snam.value else {
            panic!("SNAM should be a FormKey");
        };
        assert_eq!(scene.local, 0x534F51);
        assert_eq!(interner.resolve(scene.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn normalized_note_snam_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NOTE", &interner);
        push_field(&mut record, "SNAM", FieldValue::Uint(0x0053_4F51));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x534F51,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x534F51,
            plugin: interner.intern("Output.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        assert_eq!(snam.value, FieldValue::FormKey(target_fk));
    }

    #[test]
    fn normalized_npc_snam_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0x04, 0x83, 0x05, 0x00, 0x00]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x058304,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x058304,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        let FieldValue::Struct(fields) = &snam.value else {
            panic!("SNAM should be structured");
        };
        assert_eq!(
            named_value(fields, "faction", &interner).expect("faction"),
            &FieldValue::FormKey(target_fk)
        );
    }

    #[test]
    fn normalized_npc_cnto_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "CNTO",
            raw_bytes(&[0x3B, 0x33, 0x11, 0x00, 1, 0, 0, 0]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x11333B,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x11333B,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let cnto = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CNTO")
            .expect("CNTO remains");
        let FieldValue::Struct(fields) = &cnto.value else {
            panic!("CNTO should be structured");
        };
        assert_eq!(
            named_value(fields, "item", &interner).expect("item"),
            &FieldValue::FormKey(target_fk)
        );
    }

    #[test]
    fn normalized_cont_cnto_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("CONT", &interner);
        push_field(
            &mut record,
            "CNTO",
            raw_bytes(&[0xB5, 0x73, 0x06, 0x00, 1, 0, 0, 0]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x0673B5,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x0673B5,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let cnto = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CNTO")
            .expect("CNTO remains");
        let FieldValue::Struct(fields) = &cnto.value else {
            panic!("CNTO should be structured");
        };
        assert_eq!(
            named_value(fields, "item", &interner).expect("item"),
            &FieldValue::FormKey(target_fk)
        );
    }

    #[test]
    fn normalized_npc_prkr_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "PRKR",
            raw_bytes(&[0xF5, 0x64, 0x84, 0x00, 0x00]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x8464F5,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x8464F5,
            plugin: interner.intern("Output.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let prkr = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "PRKR")
            .expect("PRKR remains");
        let FieldValue::Struct(fields) = &prkr.value else {
            panic!("PRKR should be structured");
        };
        assert_eq!(
            named_value(fields, "Perk", &interner).expect("perk reference"),
            &FieldValue::FormKey(target_fk)
        );
    }

    // -------------------------------------------------------------------------
    // Behavior 1: global field drop
    // -------------------------------------------------------------------------

    #[test]
    fn pre_translate_drops_magf_subrecord() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(&mut record, "MAGF", FieldValue::None);
        push_field(&mut record, "EDID", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"MAGF"), "MAGF should be dropped");
        assert!(sigs.contains(&"EDID"), "EDID should be preserved");
    }

    #[test]
    fn post_translate_normalizes_npc_raw_form_refs() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0x08, 0xC0, 0x3F, 0x00, 0xFE]),
        );
        push_field(
            &mut record,
            "CNTO",
            raw_bytes(&[0x84, 0xAB, 0x33, 0x00, 1, 0, 0, 0]),
        );
        push_field(&mut record, "INAM", raw_bytes(&[0x50, 0xE3, 0x04, 0x00]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        let FieldValue::Struct(snam_fields) = &snam.value else {
            panic!("SNAM should be structured");
        };
        let FieldValue::FormKey(faction) =
            named_value(snam_fields, "faction", &interner).expect("faction")
        else {
            panic!("faction should be a FormKey");
        };
        assert_eq!(faction.local, 0x3FC008);
        assert_eq!(interner.resolve(faction.plugin), Some("SeventySix.esm"));
        assert_eq!(
            named_value(snam_fields, "rank", &interner).expect("rank"),
            &raw_bytes(&[0xFE])
        );

        let cnto = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CNTO")
            .expect("CNTO remains");
        let FieldValue::Struct(cnto_fields) = &cnto.value else {
            panic!("CNTO should be structured");
        };
        let FieldValue::FormKey(item) = named_value(cnto_fields, "item", &interner).expect("item")
        else {
            panic!("item should be a FormKey");
        };
        assert_eq!(item.local, 0x33AB84);
        assert_eq!(interner.resolve(item.plugin), Some("SeventySix.esm"));
        assert_eq!(
            named_value(cnto_fields, "count", &interner).expect("count"),
            &raw_bytes(&[1, 0, 0, 0])
        );

        let inam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "INAM")
            .expect("INAM remains");
        let FieldValue::FormKey(death_item) = &inam.value else {
            panic!("INAM should be a FormKey");
        };
        assert_eq!(death_item.local, 0x04E350);
        assert_eq!(interner.resolve(death_item.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn post_translate_remaps_rd01_assassin_combat_style_to_ranged() {
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("78BD9B@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("NPC_").unwrap(), fk);
        let eid = interner.intern("RD01_Enc04_Assassin");
        let source_plugin = interner.intern("SeventySix.esm");
        record.eid = Some(eid);
        push_field(&mut record, "EDID", FieldValue::String(eid));
        push_field(
            &mut record,
            "ZNAM",
            FieldValue::FormKey(FormKey {
                local: CS_RAIDER_01_MELEE_FORM_ID,
                plugin: source_plugin,
            }),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let combat_style = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "ZNAM")
            .expect("combat style remains");
        let FieldValue::FormKey(fk) = &combat_style.value else {
            panic!("combat style should be a FormKey");
        };
        assert_eq!(fk.local, CS_RAIDER_RANGED_FORM_ID);
        assert_eq!(fk.plugin, source_plugin);
    }

    #[test]
    fn pre_translate_converts_raw_cell_xcri() {
        let mut interner = StringInterner::new();
        let mut record = make_record("CELL", &mut interner);
        let mut raw = Vec::new();
        raw.extend_from_slice(&2_u64.to_le_bytes()); // mesh_count = 2 (literal)
        raw.extend_from_slice(&2_u64.to_le_bytes()); // reference_count field = 2x1 row
        raw.extend_from_slice(&0x1111_1111_u32.to_le_bytes());
        raw.extend_from_slice(&[1, 2, 3, 4]);
        raw.extend_from_slice(&0x2222_2222_u32.to_le_bytes());
        raw.extend_from_slice(&[5, 6, 7, 8]);
        raw.extend_from_slice(&0xAABB_CCDD_u32.to_le_bytes());
        raw.extend_from_slice(&[9, 10, 11, 12]);
        raw.extend_from_slice(&0x3333_3333_u32.to_le_bytes());
        raw.extend_from_slice(&[13, 14, 15, 16]);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "XCRI",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );
        push_field(&mut record, "XCLC", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "XCRI", "XCLC"]);
        let FieldValue::Bytes(bytes) = &record.fields[1].value else {
            panic!("expected raw XCRI bytes");
        };
        let mut expected = Vec::new();
        expected.extend_from_slice(&2_u32.to_le_bytes());
        expected.extend_from_slice(&2_u32.to_le_bytes());
        expected.extend_from_slice(&0x1111_1111_u32.to_le_bytes());
        expected.extend_from_slice(&0x2222_2222_u32.to_le_bytes());
        expected.extend_from_slice(&0xAABB_CCDD_u32.to_le_bytes());
        expected.extend_from_slice(&0x3333_3333_u32.to_le_bytes());
        assert_eq!(bytes.as_slice(), expected.as_slice());
    }

    #[test]
    fn post_translate_masks_only_fo76_idlm_unknown_5_flag() {
        let interner = StringInterner::new();
        let mut record = make_record("IDLM", &interner);
        push_field(&mut record, "IDLF", FieldValue::Uint(0x3f));
        push_field(&mut record, "IDLF", FieldValue::Int(0x28));
        push_field(&mut record, "IDLF", raw_bytes(&[0x28]));
        push_field(&mut record, "IDLF", raw_bytes(&[0x28, 0xff]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields[0].value, FieldValue::Uint(0x1f));
        assert_eq!(record.fields[1].value, FieldValue::Int(0x08));
        assert_eq!(record.fields[2].value, raw_bytes(&[0x08]));
        assert_eq!(record.fields[3].value, raw_bytes(&[0x28, 0xff]));
    }
