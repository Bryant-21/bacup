

    #[test]
    fn pre_translate_converts_only_nif_backed_empty_scol_to_stat() {
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

        let mut record = make_record("SCOL", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("SCOL\\SeventySix.esm\\CM00001234.NIF")),
        );
        push_field(
            &mut record,
            "ONAM",
            FieldValue::Bytes(SmallVec::from_slice(&[0x12, 0x58, 0x03, 0x00, 0, 0, 0, 0])),
        );
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();
        assert_eq!(record.sig, SigCode::from_str("SCOL").unwrap(), "usable part");
        assert!(record.fields.iter().any(|entry| entry.sig.0 == *b"ONAM"));

        for model in [None, Some(" \t\0 ")] {
            let mut record = make_record("SCOL", &interner);
            if let Some(model) = model {
                push_field(
                    &mut record,
                    "MODL",
                    FieldValue::String(interner.intern(model)),
                );
            }
            push_field(&mut record, "ONAM", FieldValue::None);

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(
                record.sig,
                SigCode::from_str("SCOL").unwrap(),
                "model {model:?}"
            );
        }
    }

    #[test]
    fn pre_translate_stat_signature_drives_mapper_and_flst_rewrite() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let mut source = make_record("SCOL", &interner);
        let source_form_key = source.form_key;
        let eid = interner.intern("EmptyCombinedStatic");
        source.eid = Some(eid);
        push_field(&mut source, "EDID", FieldValue::String(eid));
        push_field(
            &mut source,
            "MODL",
            FieldValue::String(interner.intern("SCOL\\SeventySix.esm\\CM00000800.NIF")),
        );

        translator
            .pre_translate(&mut make_ctx(&interner), &mut source)
            .unwrap();
        let mut translated = match translator.translate(&source, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated record, got {other:?}"),
        };
        assert_eq!(translated.sig, SigCode::from_str("STAT").unwrap());
        assert_eq!(translated.form_key, source_form_key);

        let stat_target = FormKey::parse("001234@Fallout4.esm", &interner).unwrap();
        let scol_target = FormKey::parse("005678@Fallout4.esm", &interner).unwrap();
        let mut mapper = FormKeyMapper::new(
            [
                (eid, scol_target, SigCode::from_str("SCOL").unwrap()),
                (eid, stat_target, SigCode::from_str("STAT").unwrap()),
            ],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                use_base_game_assets: true,
                ..Default::default()
            },
            &interner,
        );
        let target = mapper.allocate_or_resolve(source_form_key, Some(eid), translated.sig);
        translated.form_key = target;
        assert_eq!(target, stat_target, "mapper must select the STAT EID entry");

        let mut flst = make_record("FLST", &interner);
        push_field(&mut flst, "LNAM", FieldValue::FormKey(source_form_key));
        mapper.rewrite_record(&mut flst).unwrap();
        assert!(matches!(
            flst.fields.first().map(|entry| &entry.value),
            Some(FieldValue::FormKey(form_key)) if *form_key == stat_target
        ));
    }

    fn raw_bytes(bytes: &[u8]) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_slice(bytes))
    }

    #[test]
    fn strips_only_zero_health_cont_destructibles_across_fo76_interstitials() {
        let interner = StringInterner::new();
        for (health, expected) in [
            (0, vec!["EDID", "DATA"]),
            (
                50,
                vec![
                    "EDID", "DEST", "HGLB", "DSTD", "DMDL", "DMDT", "ENLT", "ENLS", "AUUV", "DSTF",
                    "DATA",
                ],
            ),
        ] {
            let mut record = make_record("CONT", &interner);
            push_field(&mut record, "EDID", FieldValue::None);
            push_field(&mut record, "DEST", raw_bytes(&i32::to_le_bytes(health)));
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
            assert_eq!(sigs, expected, "health {health}");
        }
    }

    #[test]
    fn drop_incompatible_condition_reconciles_citc() {
        // CITC=2 with one compatible condition (fn 74) and one FO76-only
        // condition (fn 875 > FO4 max 817). Dropping the incompatible CTDA
        // must also decrement CITC to match, or FO4's audio update
        // null-derefs on the phantom condition.
        let interner = StringInterner::new();
        let mut record = make_record("MUST", &interner);
        push_field(&mut record, "CNAM", raw_bytes(&0u32.to_le_bytes()));
        push_field(&mut record, "CITC", raw_bytes(&2u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(74));
        push_field(&mut record, "CTDA", raw_ctda(875));

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        let ctda_count = record
            .fields
            .iter()
            .filter(|f| f.sig.as_str() == "CTDA")
            .count();
        assert_eq!(ctda_count, 1, "fn 875 condition dropped");
        let citc = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CITC")
            .expect("CITC remains");
        assert_eq!(
            citc.value,
            raw_bytes(&1u32.to_le_bytes()),
            "CITC reconciled to surviving CTDA count"
        );
    }

    // The FO76 XCRI `reference_count` header field counts u32 *words* (2x the
    // logical reference-row count), not rows. `SeventySix.esm` CELL 0x0062781C
    // has XCRI length `79144 = 16 + 409*8 + 4741*16` with
    // `reference_count=9482` (2x 4741). Read as a row count, the size check
    // rejects every dense FO76 CELL and XCRI is dropped.
    #[test]
    fn cell_xcri_converts_fo76_layouts_to_fo4() {
        let mesh_count: u32 = 3;
        let row_count: u32 = 5;
        let reference_count_field: u64 = u64::from(row_count) * 2;

        let mut raw = Vec::new();
        raw.extend_from_slice(&u64::from(mesh_count).to_le_bytes());
        raw.extend_from_slice(&reference_count_field.to_le_bytes());
        for i in 0..mesh_count {
            raw.extend_from_slice(&(0x1000 + i).to_le_bytes()); // mesh_id
            raw.extend_from_slice(&0u32.to_le_bytes()); // unknown "count", discarded
        }
        for i in 0..row_count {
            raw.extend_from_slice(&(0x01_000800 + i).to_le_bytes()); // reference
            raw.extend_from_slice(&0xDEAD_BEEF_u32.to_le_bytes()); // unknown, discarded
            raw.extend_from_slice(&(0x1000 + (i % mesh_count)).to_le_bytes()); // mesh_id
            raw.extend_from_slice(&0xCAFE_BABE_u32.to_le_bytes()); // unknown, discarded
        }
        assert_eq!(
            raw.len(),
            16 + (mesh_count as usize) * 8 + (row_count as usize) * 16
        );

        let converted = convert_cell_xcri_raw_to_fo4(&raw)
            .expect("dense FO76 XCRI with a 2x reference_count header must convert to FO4");

        let mut expected = Vec::new();
        expected.extend_from_slice(&mesh_count.to_le_bytes());
        expected.extend_from_slice(&(row_count * 2).to_le_bytes());
        for i in 0..mesh_count {
            expected.extend_from_slice(&(0x1000 + i).to_le_bytes());
        }
        for i in 0..row_count {
            expected.extend_from_slice(&(0x01_000800 + i).to_le_bytes());
            expected.extend_from_slice(&(0x1000 + (i % mesh_count)).to_le_bytes());
        }
        assert_eq!(converted, expected);

        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        let mut record = make_record("CELL", &interner);
        let mut raw = Vec::new();
        raw.extend_from_slice(&2_u64.to_le_bytes());
        raw.extend_from_slice(&2_u64.to_le_bytes());
        for (mesh_or_reference, unknown) in [
            (0x1111_1111_u32, [1, 2, 3, 4]),
            (0x2222_2222, [5, 6, 7, 8]),
            (0xAABB_CCDD, [9, 10, 11, 12]),
            (0x3333_3333, [13, 14, 15, 16]),
        ] {
            raw.extend_from_slice(&mesh_or_reference.to_le_bytes());
            raw.extend_from_slice(&unknown);
        }
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "XCRI", raw_bytes(&raw));
        push_field(&mut record, "XCLC", FieldValue::None);
        hook.pre_translate(&mut ctx, &mut record).unwrap();
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "XCRI", "XCLC"]);
        let expected: Vec<u8> = [2_u32, 2, 0x1111_1111, 0x2222_2222, 0xAABB_CCDD, 0x3333_3333]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        assert_eq!(record.fields[1].value, raw_bytes(&expected), "raw XCRI");

        let mut record = make_record("CELL", &interner);
        let xcri = FieldValue::Struct(vec![
            (interner.intern("meshes_count"), FieldValue::Uint(1)),
            (interner.intern("references_count"), FieldValue::Uint(2)),
            (
                interner.intern("meshes"),
                FieldValue::List(vec![FieldValue::Struct(vec![
                    (interner.intern("combined_mesh"), FieldValue::Uint(7)),
                    (interner.intern("unknown_u8_1"), FieldValue::Uint(255)),
                ])]),
            ),
            (
                interner.intern("references"),
                FieldValue::List(vec![FieldValue::Struct(vec![
                    (
                        interner.intern("reference"),
                        raw_bytes(&0x1234_5678_u32.to_le_bytes()),
                    ),
                    (interner.intern("unknown_u8_1"), FieldValue::Uint(255)),
                    (interner.intern("combined_mesh"), FieldValue::Uint(7)),
                ])]),
            ),
        ]);
        push_field(&mut record, "XCRI", xcri);
        hook.pre_translate(&mut ctx, &mut record).unwrap();
        assert_eq!(record.fields.len(), 1);
        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded = crate::target_write::encode_field_pub(
            &record.fields[0],
            schema.record_def("CELL"),
            &interner,
        )
        .expect("converted XCRI encodes");
        let expected: Vec<u8> = [1_u32, 2, 7, 0x1234_5678, 7]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        assert_eq!(encoded, expected, "structured XCRI");

        for (name, sig, xcri, expected) in [
            ("malformed_cell_xcri_dropped", "CELL", vec![1, 2, 3], vec!["EDID"]),
            ("non_cell_xcri_kept", "STAT", vec![0; 32], vec!["EDID", "XCRI"]),
        ] {
            let mut record = make_record(sig, &interner);
            push_field(&mut record, "EDID", FieldValue::None);
            push_field(&mut record, "XCRI", raw_bytes(&xcri));
            hook.pre_translate(&mut ctx, &mut record).unwrap();
            let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
            assert_eq!(sigs, expected, "{name}");
        }
    }

    /// `CELL.XCLL` reaches a pair hook as raw bytes, never a typed
    /// `FieldValue::Struct` — `source_read::decode_subrecord` emits bytes for
    /// every `struct:` codec. Tests must use this shape or they validate a
    /// record layout the pipeline never produces.
    const XCLL_LEN: usize = 136;

    fn xcll_bytes(inherits: u32, ambient: u8, directional_ambient: u8) -> FieldValue {
        let mut bytes = vec![0u8; XCLL_LEN];
        // Ambient RGB @ +0, Directional RGB @ +4.
        bytes[0..3].fill(ambient);
        // The six directional-ambient RGB quads @ +40..64.
        for offset in (40..64).filter(|offset| offset % 4 != 3) {
            bytes[offset] = directional_ambient;
        }
        bytes[88..92].copy_from_slice(&inherits.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn lit_cell(
        interner: &StringInterner,
        editor_id: &str,
        cell_flags: u32,
        xcll: FieldValue,
        lighting_template: Option<u32>,
    ) -> Record {
        let mut record = make_record("CELL", interner);
        record.eid = Some(interner.intern(editor_id));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(cell_flags.to_le_bytes().to_vec())),
        );
        if let Some(template) = lighting_template {
            push_field(&mut record, "LTMP", form_key_value(interner, template));
        }
        push_field(&mut record, "XCLL", xcll);
        record
    }

    fn cell_inherits(record: &Record) -> u32 {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"XCLL")
            .and_then(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => Some(u32::from_le_bytes(
                    bytes[88..92].try_into().expect("inherits slot"),
                )),
                _ => None,
            })
            .expect("CELL.XCLL inherits")
    }

    fn cell_ambient_byte(record: &Record) -> u8 {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"XCLL")
            .and_then(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => bytes.first().copied(),
                _ => None,
            })
            .expect("CELL.XCLL ambient")
    }

    #[test]
    fn pre_translate_cell_lighting_inherits_table() {
        let interner = StringInterner::new();
        let interior = CELL_INTERIOR_FLAG;
        for (name, editor_id, flags, xcll, template, expected) in [
            ("storm_interior_inherits_template_fog", "StormStolzManor", interior, xcll_bytes(0x04E0, 90, 90), Some(0x002FEB), 0x07FC),
            ("storm_lab_inherits_template_fog", "StormWeatherLab01", interior, xcll_bytes(0x04E0, 90, 90), Some(0x002FEB), 0x07FC),
            ("black_interior_inherits_ambient", "FortAtlas01", interior, xcll_bytes(0, 0, 0), Some(0x002FE3), 0x0003),
            ("near_black_interior_inherits_ambient", "FortAtlas01", interior, xcll_bytes(0, 47, 47), Some(0x002FE3), 0x0003),
            // Bit 0 clear, every other bit authored: only ambient and directional may be added.
            ("unlit_interior_keeps_authored_fog_bits", "DebugZachW", interior, xcll_bytes(0x079E, 29, 29), Some(0x000DD38D), 0x079F),
            ("lit_interior_untouched", "SomeCell", interior, xcll_bytes(0, 48, 48), Some(0x002FE3), 0x0000),
            ("already_inheriting_untouched", "SomeCell", interior, xcll_bytes(0x0001, 0, 0), Some(0x002FE3), 0x0001),
            ("templateless_untouched", "SomeCell", interior, xcll_bytes(0, 0, 0), None, 0x0000),
            ("exterior_untouched", "SomeExterior", 0, xcll_bytes(0, 0, 0), Some(0x002FE3), 0x0000),
            ("non_storm_fog_kept", "Vault63Meteorology", interior, xcll_bytes(0x04E0, 90, 90), Some(0x002FEB), 0x04E0),
            ("storm_exterior_fog_kept", "StormExterior", 0, xcll_bytes(0x04E0, 90, 90), Some(0x002FEB), 0x04E0),
        ] {
            let FieldValue::Bytes(source) = &xcll else {
                unreachable!("xcll_bytes builds raw XCLL");
            };
            let ambient = source[0];
            let mut record = lit_cell(&interner, editor_id, flags, xcll, template);

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(cell_inherits(&record), expected, "{name}");
            // The inherit rewrite must not scribble over the ambient colour at +0.
            assert_eq!(cell_ambient_byte(&record), ambient, "{name}");
        }
    }

    #[test]
    fn pre_translate_keeps_qust_vmad_and_full_alias_chain() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3])),
        );
        push_field(&mut record, "FULL", FieldValue::None);
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 4])),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(34_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALID", FieldValue::Bytes(SmallVec::new()));
        // FO76-only alias keyword/faction-rank fields: dropped even though they
        // appear inside the alias chain.
        push_field(&mut record, "KNAM", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "ALFC", FieldValue::Bytes(SmallVec::new()));
        // FO76 event alias-fill data is unsafe once the FO76 event scope is
        // stripped, so the alias row survives without these fields.
        push_field(&mut record, "ALFE", FieldValue::Bytes(SmallVec::new()));
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(0x00003152_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALLS",
            FieldValue::Bytes(SmallVec::from_vec(35_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALCS",
            FieldValue::Bytes(SmallVec::from_vec(36_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "KSIZ",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 0, 0, 0])),
        );
        push_field(&mut record, "ALRT", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "SNAM", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(
            sigs,
            vec![
                "EDID", "VMAD", "FULL", "FNAM", "ANAM", "ALST", "ALID", "ALLS", "ALCS", "KSIZ",
                "KWDA", "ALRT", "SNAM"
            ]
        );
        match &record.fields[4].value {
            FieldValue::Bytes(bytes) => assert_eq!(&bytes[..4], &34_u32.to_le_bytes()),
            other => panic!("ANAM should retain next alias id bytes, got {other:?}"),
        }
    }

    #[test]
    fn post_translate_border_region_flag_follows_only_the_source_flag() {
        let interner = StringInterner::new();
        for (name, editor_id, source_flag, rcbn, expected) in [
            ("named_border_without_flag", "BurningSpringsBorderRegion01", false, false, false),
            ("source_flag_preserved", "BurningSpringsRegion", true, false, true),
            ("rcbn_does_not_mark_border", "ForestObjectRegion", false, true, false),
        ] {
            let mut record = make_record("REGN", &interner);
            record.eid = Some(interner.intern(editor_id));
            if source_flag {
                record.flags.insert(RecordFlags::BORDER_REGION);
            }
            if rcbn {
                push_field(&mut record, "RCBN", raw_bytes(&[1]));
            }

            Fo76Fo4Hook
                .post_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(
                record.flags.contains(RecordFlags::BORDER_REGION),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn post_translate_adds_ingredient_production_only_to_harvestable_flora_without_one() {
        let interner = StringInterner::new();
        let mut record = make_record("FLOR", &interner);
        record.eid = Some(interner.intern("FloraRadDecayVine01"));
        push_field(&mut record, "PFIG", form_key_value(&interner, 0x2D_DD4D));
        push_field(&mut record, "SNAM", form_key_value(&interner, 0x22_3D06));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let production: Vec<&FieldValue> = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"PFPC")
            .map(|field| &field.value)
            .collect();
        assert_eq!(production, vec![&raw_bytes(&[100, 100, 100, 100])]);

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded = crate::target_write::encode_field_pub(
            record
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"PFPC")
                .expect("synthesized PFPC"),
            schema.record_def("FLOR"),
            &interner,
        )
        .expect("synthesized PFPC encodes");
        assert_eq!(encoded, vec![100, 100, 100, 100]);

        let mut record = make_record("FLOR", &interner);
        push_field(&mut record, "PFIG", form_key_value(&interner, 0x2D_DD4D));
        push_field(&mut record, "PFPC", raw_bytes(&[25, 50, 75, 100]));
        hook.post_translate(&mut ctx, &mut record).unwrap();
        let production: Vec<&FieldValue> = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"PFPC")
            .map(|field| &field.value)
            .collect();
        assert_eq!(production, vec![&raw_bytes(&[25, 50, 75, 100])], "source PFPC kept");

        let mut record = make_record("FLOR", &interner);
        push_field(&mut record, "SNAM", form_key_value(&interner, 0x22_3D06));
        hook.post_translate(&mut ctx, &mut record).unwrap();
        assert!(
            !record.fields.iter().any(|field| field.sig.0 == *b"PFPC"),
            "no ingredient, no PFPC"
        );
    }

    #[test]
    fn post_translate_converts_regn_rdot_and_drops_raw_rows() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let masters = vec!["SeventySix.esm".to_string()];
        let mut payload = vec![0_u8; 76];
        payload[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        payload[44..48].copy_from_slice(&64.0f32.to_le_bytes());
        payload[48..52].copy_from_slice(&(-200000.0f32).to_le_bytes());
        payload[52..56].copy_from_slice(&200000.0f32.to_le_bytes());
        payload[64] = 10;
        payload[65] = 20;
        payload[66] = 30;
        payload[67] = 1;
        payload[68..72].copy_from_slice(&0x0000_0800u32.to_le_bytes());
        payload[72..74].copy_from_slice(&0xFFFFu16.to_le_bytes());

        let decoded =
            crate::fo76_rdot::decode_fo76_regn_rdot(&payload, &masters, "Source.esm", &interner)
                .expect("FO76 RDOT decodes");
        let mut record = make_record("REGN", &interner);
        push_field(&mut record, "RDAT", FieldValue::None);
        push_field(&mut record, "RDOT", decoded);
        push_field(&mut record, "RDWT", FieldValue::None);

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let rdot = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "RDOT")
            .expect("converted RDOT is preserved");
        let FieldValue::List(rows) = &rdot.value else {
            panic!("expected FO4 RDOT row list");
        };
        assert_eq!(rows.len(), 1);

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded =
            crate::target_write::encode_field_pub(rdot, schema.record_def("REGN"), &interner)
                .expect("converted RDOT encodes");
        assert_eq!(encoded.len(), 52);

        for (name, sig, expected) in [
            ("raw_region_rdot_dropped", "REGN", vec!["RDAT", "RDWT"]),
            ("non_region_rdot_kept", "STAT", vec!["RDAT", "RDOT", "RDWT"]),
        ] {
            let mut record = make_record(sig, &interner);
            push_field(&mut record, "RDAT", FieldValue::None);
            push_field(&mut record, "RDOT", raw_bytes(&[0_u8; 456]));
            push_field(&mut record, "RDWT", FieldValue::None);
            hook.post_translate(&mut ctx, &mut record).unwrap();
            let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
            assert_eq!(sigs, expected, "{name}");
        }
    }

    #[test]
    fn post_translate_leaves_unprefixed_model_paths_unprefixed() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Landscape\\Trees\\Tree.nif")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields.len(), 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(interner.resolve(sym), Some("Landscape\\Trees\\Tree.nif"));
    }

