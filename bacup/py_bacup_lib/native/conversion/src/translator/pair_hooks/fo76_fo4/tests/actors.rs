
    #[test]
    fn pre_translate_strips_only_bee_swarm_ant_limb_replacements() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let mut bee_parts = Record::new(
            SigCode::from_str("BPTD").unwrap(),
            FormKey {
                local: HONEY_BEAST_BEE_SWARM_BPTD_FORM_ID,
                plugin,
            },
        );
        bee_parts.eid = Some(interner.intern(HONEY_BEAST_BEE_SWARM_BPTD_EDITOR_ID));
        push_field(
            &mut bee_parts,
            "NAM1",
            FieldValue::String(
                interner.intern("Actors\\DLC04\\Swarm\\CharacterAssets\\ReplaceSwarm01.nif"),
            ),
        );
        push_field(&mut bee_parts, "BPND", raw_bytes(&[0; 112]));
        push_field(
            &mut bee_parts,
            "NAM1",
            FieldValue::String(
                interner.intern("Actors\\DLC04\\Swarm\\CharacterAssets\\ReplaceSwarm02.nif"),
            ),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut bee_parts)
            .unwrap();

        assert!(bee_parts.fields.iter().all(|field| field.sig.0 != *b"NAM1"));
        assert!(bee_parts.fields.iter().any(|field| field.sig.0 == *b"BPND"));

        let mut other_parts = make_record("BPTD", &interner);
        other_parts.eid = Some(interner.intern("OtherCreatureBodyPartData"));
        push_field(
            &mut other_parts,
            "NAM1",
            FieldValue::String(interner.intern("Actors\\Other\\Replacement.nif")),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut other_parts)
            .unwrap();

        assert!(
            other_parts
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"NAM1")
        );
    }

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

    #[test]
    fn pre_translate_maps_child_underarmor_slots_to_body() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMO", &interner);
        push_field(
            &mut record,
            "RNAM",
            FieldValue::FormKey(FormKey {
                local: FO4_HUMAN_CHILD_RACE_FORM_ID,
                plugin: interner.intern("SeventySix.esm"),
            }),
        );
        push_field(
            &mut record,
            "BOD2",
            FieldValue::Uint(FO76_UPPER_BODY_SKIN_BIPED_MASK),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let mask = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .and_then(|entry| match entry.value {
                FieldValue::Uint(mask) => Some(mask),
                _ => None,
            })
            .expect("ARMO BOD2 mask");
        assert_eq!(mask, FO4_BODY_BIPED_MASK);
    }

    #[test]
    fn pre_translate_preserves_full_body_child_armor_slots() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMO", &interner);
        push_field(
            &mut record,
            "RNAM",
            FieldValue::FormKey(FormKey {
                local: FO4_HUMAN_CHILD_RACE_FORM_ID,
                plugin: interner.intern("SeventySix.esm"),
            }),
        );
        let full_body_mask = FO4_BODY_BIPED_MASK | FO76_UPPER_BODY_SKIN_BIPED_MASK;
        push_field(&mut record, "BOD2", FieldValue::Uint(full_body_mask));

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let mask = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .and_then(|entry| match entry.value {
                FieldValue::Uint(mask) => Some(mask),
                _ => None,
            })
            .expect("ARMO BOD2 mask");
        assert_eq!(mask, full_body_mask);
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

    fn raw_fo76_destruction_stage(health: u8, flags: u8, explosion: u32) -> FieldValue {
        let mut bytes = vec![0_u8; 28];
        bytes[0] = health;
        bytes[3] = flags;
        bytes[8..12].copy_from_slice(&explosion.to_le_bytes());
        raw_bytes(&bytes)
    }

    #[test]
    fn pre_translate_normalizes_fo76_explosive_vehicle_destruction_flags() {
        let interner = StringInterner::new();
        let mut record = make_record("MSTT", &interner);
        push_field(
            &mut record,
            "DSTD",
            raw_fo76_destruction_stage(85, 0, 0x0022_496),
        );
        push_field(
            &mut record,
            "DSTD",
            raw_fo76_destruction_stage(50, 0x05, 0x0023_D3B),
        );
        push_field(
            &mut record,
            "DSTD",
            raw_fo76_destruction_stage(0, 0x08, 0),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let flags: Vec<u8> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"DSTD")
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => bytes[3],
                _ => panic!("DSTD should remain raw before struct relayout"),
            })
            .collect();
        assert_eq!(flags, vec![0, 0x04, 0]);
    }

    #[test]
    fn pre_translate_preserves_nonexplosive_mstt_destruction_flags() {
        let interner = StringInterner::new();
        let mut record = make_record("MSTT", &interner);
        push_field(
            &mut record,
            "DSTD",
            raw_fo76_destruction_stage(50, 0x05, 0),
        );
        push_field(
            &mut record,
            "DSTD",
            raw_fo76_destruction_stage(0, 0x08, 0),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let flags: Vec<u8> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"DSTD")
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => bytes[3],
                _ => panic!("DSTD should remain raw before struct relayout"),
            })
            .collect();
        assert_eq!(flags, vec![0x05, 0x08]);
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

    /// Translation carries `KWDA` across verbatim; nothing here special-cases a grip.
    /// The AlienRifle DOES end up without `AnimsGripRifleStraight`, but that happens
    /// later and generically, in `strip_generic_grips_from_owned_weapons`, which can
    /// see whether an additive third-person block was actually emitted for the weapon.
    /// Deciding it per-record here cannot: the block set does not exist yet.
    #[test]
    fn pre_translate_carries_weapon_keywords_across_untouched() {
        let interner = StringInterner::new();
        let mut record = make_record("WEAP", &interner);
        push_field(&mut record, "KSIZ", FieldValue::Uint(3));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::List(vec![
                form_key_value(&interner, FO76_ANIMS_GRIP_RIFLE_STRAIGHT_FORM_ID),
                form_key_value(&interner, FO76_ANIMS_ALIEN_RIFLE_FORM_ID),
                form_key_value(&interner, 0x0F4AEA),
            ]),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let keywords = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"KWDA")
            .expect("KWDA remains");
        let FieldValue::List(keywords) = &keywords.value else {
            panic!("KWDA should remain a decoded keyword list");
        };
        let keyword_locals: Vec<u32> = keywords
            .iter()
            .filter_map(|value| match value {
                FieldValue::FormKey(keyword) => Some(keyword.local),
                _ => None,
            })
            .collect();
        assert_eq!(
            keyword_locals,
            vec![
                FO76_ANIMS_GRIP_RIFLE_STRAIGHT_FORM_ID,
                FO76_ANIMS_ALIEN_RIFLE_FORM_ID,
                0x0F4AEA
            ]
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"KSIZ")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(3))
        );
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

    fn w05_nuke_reaction_vmad(base_outfit: Option<u32>) -> FieldValue {
        let masters = [FO4_MASTER_NAME.to_string()];
        let mut properties = Vec::new();
        if let Some(form_id) = base_outfit {
            properties.push(serde_json::json!({
                "propertyName": W05_ACTOR_NUKE_BASE_OUTFIT_PROPERTY,
                "Type": "Object",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": {
                    "Alias": -1,
                    "FormID": {
                        "reference": {
                            "plugin": FO76_MASTER_NAME,
                            "object_id": format!("{form_id:06X}"),
                        },
                    },
                },
            }));
        }
        let payload = serde_json::json!({
            "Version": FO4_VMAD_VERSION,
            "Object Format": FO4_VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": W05_ACTOR_NUKE_REACTION_SCRIPT,
                "Properties": properties,
            }],
        });
        raw_bytes(
            &build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME)
                .expect("W05 actor VMAD fixture must encode"),
        )
    }

    fn w05_nuke_reaction_base_outfit(record: &Record) -> u32 {
        let FieldValue::Bytes(bytes) = &record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"VMAD")
            .expect("W05 actor VMAD remains")
            .value
        else {
            panic!("W05 actor VMAD should remain bytes");
        };
        let masters = [FO4_MASTER_NAME.to_string()];
        let payload = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
                bytes,
                &masters,
                FO76_MASTER_NAME,
                Some("NPC_"),
            )
            .expect("W05 actor VMAD must decode");
        let property = payload["Scripts"][0]["Properties"]
            .as_array()
            .expect("properties")
            .iter()
            .find(|property| {
                property["propertyName"].as_str() == Some(W05_ACTOR_NUKE_BASE_OUTFIT_PROPERTY)
            })
            .expect("BaseOutfit property");
        u32::from_str_radix(
            property["Value"]["FormID"]["reference"]["object_id"]
                .as_str()
                .expect("BaseOutfit object id"),
            16,
        )
        .expect("hex BaseOutfit object id")
    }

    #[test]
    fn post_translate_backfills_w05_nuke_reaction_base_outfit_from_doft() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(&mut record, "VMAD", w05_nuke_reaction_vmad(None));
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x58E6C7));
        let target_masters = [FO4_MASTER_NAME.to_string()];
        let source_masters: [String; 0] = [];
        let mut ctx = PairCtx::with_source_and_target(
            &interner,
            FO76_MASTER_NAME,
            &source_masters,
            &target_masters,
        );

        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(w05_nuke_reaction_base_outfit(&record), 0x58E6C7);
    }

    #[test]
    fn post_translate_preserves_explicit_w05_nuke_base_outfit_and_is_idempotent() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(&mut record, "VMAD", w05_nuke_reaction_vmad(Some(0x01D984)));
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x58E6C7));
        let target_masters = [FO4_MASTER_NAME.to_string()];
        let source_masters: [String; 0] = [];
        let mut ctx = PairCtx::with_source_and_target(
            &interner,
            FO76_MASTER_NAME,
            &source_masters,
            &target_masters,
        );

        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();
        let first_vmad = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"VMAD")
            .expect("VMAD")
            .value
            .clone();
        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(w05_nuke_reaction_base_outfit(&record), 0x01D984);
        assert_eq!(
            record
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"VMAD")
                .expect("VMAD")
                .value,
            first_vmad
        );
    }

    #[test]
    fn post_translate_w05_nuke_base_outfit_fails_closed_on_duplicate_doft() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(&mut record, "VMAD", w05_nuke_reaction_vmad(None));
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x58E6C7));
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x01D984));
        let original_fields = record.fields.clone();
        let target_masters = [FO4_MASTER_NAME.to_string()];
        let source_masters: [String; 0] = [];
        let mut ctx = PairCtx::with_source_and_target(
            &interner,
            FO76_MASTER_NAME,
            &source_masters,
            &target_masters,
        );

        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields, original_fields);
    }

    #[test]
    fn post_translate_w05_nuke_base_outfit_fails_closed_on_duplicate_vmad() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        let vmad = w05_nuke_reaction_vmad(None);
        push_field(&mut record, "VMAD", vmad.clone());
        push_field(&mut record, "VMAD", vmad);
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x58E6C7));
        let original_fields = record.fields.clone();
        let target_masters = [FO4_MASTER_NAME.to_string()];
        let source_masters: [String; 0] = [];
        let mut ctx = PairCtx::with_source_and_target(
            &interner,
            FO76_MASTER_NAME,
            &source_masters,
            &target_masters,
        );

        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields, original_fields);
    }

    #[test]
    fn post_translate_w05_nuke_base_outfit_requires_one_matching_script() {
        let interner = StringInterner::new();
        let masters = [FO4_MASTER_NAME.to_string()];
        let payload = serde_json::json!({
            "Version": FO4_VMAD_VERSION,
            "Object Format": FO4_VMAD_OBJECT_FORMAT,
            "Scripts": [
                {
                    "ScriptName": W05_ACTOR_NUKE_REACTION_SCRIPT,
                    "Properties": [],
                },
                {
                    "ScriptName": W05_ACTOR_NUKE_REACTION_SCRIPT,
                    "Properties": [],
                },
            ],
        });
        let vmad = raw_bytes(
            &build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME)
                .expect("duplicate-script VMAD fixture must encode"),
        );
        let mut record = make_record("NPC_", &interner);
        push_field(&mut record, "VMAD", vmad);
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x58E6C7));
        let original_fields = record.fields.clone();
        let source_masters: [String; 0] = [];
        let mut ctx = PairCtx::with_source_and_target(
            &interner,
            FO76_MASTER_NAME,
            &source_masters,
            &masters,
        );

        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields, original_fields);
    }

    #[test]
    fn post_translate_w05_nuke_base_outfit_fails_closed_on_malformed_vmad() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "VMAD",
            raw_bytes(b"W05_ActorNukeReactionScript"),
        );
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x58E6C7));
        let original_fields = record.fields.clone();
        let target_masters = [FO4_MASTER_NAME.to_string()];
        let source_masters: [String; 0] = [];
        let mut ctx = PairCtx::with_source_and_target(
            &interner,
            FO76_MASTER_NAME,
            &source_masters,
            &target_masters,
        );

        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields, original_fields);
    }
