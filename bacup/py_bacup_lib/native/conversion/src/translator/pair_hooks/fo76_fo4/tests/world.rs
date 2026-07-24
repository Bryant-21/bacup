

    #[test]
    fn pre_translate_keeps_scol_with_a_usable_part() {
        let interner = StringInterner::new();
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

        assert_eq!(record.sig, SigCode::from_str("SCOL").unwrap());
        assert!(record.fields.iter().any(|entry| entry.sig.0 == *b"ONAM"));
    }

    #[test]
    fn pre_translate_keeps_empty_scol_without_a_non_empty_model() {
        let interner = StringInterner::new();
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

            assert_eq!(record.sig, SigCode::from_str("SCOL").unwrap());
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
    fn post_translate_strips_wsbunker_intercom_radio() {
        let interner = StringInterner::new();
        let mut record = make_record("ACTI", &interner);
        record.eid = Some(interner.intern(WSBUNKER_INTERCOM_EDITOR_ID));
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("SetDressing\\WallPanels\\Intercom_Panel.nif")),
        );
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "RADR", raw_bytes(&[0_u8; 14]));

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["MODL"]
        );
    }

    #[test]
    fn pre_translate_does_not_rewrite_non_refr_tnam() {
        let interner = StringInterner::new();
        let mut record = make_record("TERM", &interner);
        push_field(&mut record, "TNAM", raw_bytes(&64_u32.to_le_bytes()));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        match &record.fields[0].value {
            FieldValue::Bytes(bytes) => assert_eq!(bytes.as_slice(), &64_u32.to_le_bytes()),
            value => panic!("expected TNAM bytes, got {value:?}"),
        }
    }

    #[test]
    fn zero_health_cont_destructible_strip_spans_fo76_interstitials() {
        let interner = StringInterner::new();
        let mut record = make_record("CONT", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "DEST", raw_bytes(&0_i32.to_le_bytes()));
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
        assert_eq!(sigs, vec!["EDID", "DATA"]);
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

    // Regression for the FO76 XCRI `reference_count` header field: it counts
    // u32 *words* (2x the logical reference-row count), not rows. Ground
    // truth: `SeventySix.esm` CELL 0x0062781C has XCRI length
    // `79144 = 16 + 409*8 + 4741*16` with `reference_count=9482` (2x 4741).
    // The pre-fix `convert_cell_xcri_raw_to_fo4` read `reference_count` as a
    // literal row count, so its size check rejected every real dense FO76
    // CELL and silently dropped XCRI from the FO4 output.
    #[test]
    fn convert_cell_xcri_raw_to_fo4_accepts_fo76_2x_reference_count_layout() {
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
    }

    #[test]
    fn pre_translate_converts_structured_cell_xcri() {
        let mut interner = StringInterner::new();
        let mut record = make_record("CELL", &mut interner);
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
                        FieldValue::Bytes(SmallVec::from_vec(
                            0x1234_5678_u32.to_le_bytes().to_vec(),
                        )),
                    ),
                    (interner.intern("unknown_u8_1"), FieldValue::Uint(255)),
                    (interner.intern("combined_mesh"), FieldValue::Uint(7)),
                ])]),
            ),
        ]);
        push_field(&mut record, "XCRI", xcri);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields.len(), 1);
        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded = crate::target_write::encode_field_pub(
            &record.fields[0],
            schema.record_def("CELL"),
            &interner,
        )
        .expect("converted XCRI encodes");
        let mut expected = Vec::new();
        expected.extend_from_slice(&1_u32.to_le_bytes());
        expected.extend_from_slice(&2_u32.to_le_bytes());
        expected.extend_from_slice(&7_u32.to_le_bytes());
        expected.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
        expected.extend_from_slice(&7_u32.to_le_bytes());
        assert_eq!(encoded, expected);
    }

    #[test]
    fn pre_translate_drops_malformed_cell_xcri() {
        let mut interner = StringInterner::new();
        let mut record = make_record("CELL", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "XCRI",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3])),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID"]);
    }

    #[test]
    fn pre_translate_keeps_xcri_on_non_cell_records() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "XCRI",
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 32])),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["XCRI"]);
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
    fn post_translate_does_not_mark_named_border_region_when_flag_is_missing() {
        let interner = StringInterner::new();
        let mut record = make_record("REGN", &interner);
        record.eid = Some(interner.intern("BurningSpringsBorderRegion01"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(!record.flags.contains(RecordFlags::BORDER_REGION));
    }

    #[test]
    fn post_translate_preserves_source_border_region_flag() {
        let interner = StringInterner::new();
        let mut record = make_record("REGN", &interner);
        record.eid = Some(interner.intern("BurningSpringsRegion"));
        record.flags.insert(RecordFlags::BORDER_REGION);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(record.flags.contains(RecordFlags::BORDER_REGION));
    }

    #[test]
    fn post_translate_does_not_use_rcbn_to_mark_region_border() {
        let interner = StringInterner::new();
        let mut record = make_record("REGN", &interner);
        record.eid = Some(interner.intern("ForestObjectRegion"));
        push_field(&mut record, "RCBN", raw_bytes(&[1]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(!record.flags.contains(RecordFlags::BORDER_REGION));
    }

    #[test]
    fn post_translate_converts_structured_regn_rdot_to_fo4_rows() {
        let mut interner = StringInterner::new();
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

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
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
    }

    #[test]
    fn post_translate_keeps_rdot_on_non_region_records() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "RDOT",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 456])),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"RDOT"));
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
