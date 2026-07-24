

    fn workshop_cobj_form_key(record: &Record, sig: &[u8; 4]) -> FormKey {
        let field = record
            .fields
            .iter()
            .find(|field| &field.sig.0 == sig)
            .unwrap_or_else(|| panic!("missing {}", std::str::from_utf8(sig).unwrap()));
        match &field.value {
            FieldValue::FormKey(form_key) => *form_key,
            FieldValue::List(values) => match values.first() {
                Some(FieldValue::FormKey(form_key)) => *form_key,
                other => panic!("expected FormKey list, got {other:?}"),
            },
            other => panic!("expected FormKey, got {other:?}"),
        }
    }

    #[test]
    fn workshop_cobj_scope_accepts_only_intended_editor_ids() {
        let interner = StringInterner::new();
        for eid in [
            "workshop_co_Wall",
            "ATX_workshop_co_Lights_TrainHeadlight",
            "SCORE_S24_Workshop_CO_DriveInStatue",
        ] {
            let record = workshop_cobj(&interner, eid, FO76_WORKSHOP_CATEGORY_WALLS);
            assert!(
                Fo76Fo4Hook::is_convertible_workshop_cobj(&interner, &record),
                "{eid}"
            );
        }
        for eid in [
            "zzz_ATX_workshop_co_Wall",
            "ZZZworkshop_co_Wall",
            "co_mod_Weapon_Rifle",
            "ATX_co_mod_Weapon_Rifle",
            "SCORE_co_modScrapRecipe",
            "ATX_workshop_co_mod_Weapon_Rifle",
            "co_Clothes_Outfit",
            "ATX_co_Clothes_Outfit",
            "SCORE_co_Cloths_Outfit",
            "SCORE_workshop_co_clothes_Outfit",
            "ATX_workbench_co_NotWorkshop",
        ] {
            let record = workshop_cobj(&interner, eid, FO76_WORKSHOP_CATEGORY_WALLS);
            assert!(
                !Fo76Fo4Hook::is_convertible_workshop_cobj(&interner, &record),
                "{eid}"
            );
        }
    }

    #[test]
    fn post_translate_moves_fo76_workshop_category_to_fnam_and_sets_matching_bnam() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "ATX_workshop_co_Lights_TrainHeadlight",
            FO76_WORKSHOP_CATEGORY_LIGHTS,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let bench = workshop_cobj_form_key(&record, b"BNAM");
        assert_eq!(bench.local, FO4_WORKSHOP_WORKBENCH_POWER);
        assert_eq!(interner.resolve(bench.plugin), Some(FO4_MASTER_NAME));
        let category = workshop_cobj_form_key(&record, b"FNAM");
        assert_eq!(category.local, FO76_WORKSHOP_CATEGORY_LIGHTS);
        assert_eq!(interner.resolve(category.plugin), Some(FO76_MASTER_NAME));
    }

    #[test]
    fn post_translate_handles_fo76_main_workshop_category_keyword() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "ATX_workshop_co_Furniture_Generic",
            FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").local,
            FO4_WORKSHOP_WORKBENCH_FURNITURE
        );
        assert_eq!(
            workshop_cobj_form_key(&record, b"FNAM").local,
            FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE
        );
    }

    #[test]
    fn post_translate_infers_category_for_workshop_all_type_recipe() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "SCORE_S25_workshop_co_Structure_VinesJailCell_WallFull",
            FO76_WORKSHOP_WORKBENCH_ALL_TYPE,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").local,
            FO4_WORKSHOP_WORKBENCH_EXTERIOR
        );
        assert_eq!(
            workshop_cobj_form_key(&record, b"FNAM").local,
            FO76_WORKSHOP_CATEGORY_WALLS
        );
    }

    #[test]
    fn post_translate_keeps_non_powered_fire_lights_on_furniture_workbench() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "workshop_co_Lights_CampFire01",
            FO76_WORKSHOP_WORKBENCH_ALL_TYPE,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").local,
            FO4_WORKSHOP_WORKBENCH_FURNITURE
        );
        assert_eq!(
            workshop_cobj_form_key(&record, b"FNAM").local,
            FO76_WORKSHOP_CATEGORY_LIGHTS
        );
    }

    #[test]
    fn post_translate_does_not_touch_excluded_workshop_recipe_family() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "ATX_workshop_co_mod_Weapon",
            FO76_WORKSHOP_CATEGORY_LIGHTS,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").local,
            FO76_WORKSHOP_CATEGORY_LIGHTS
        );
        assert!(record.fields.iter().all(|field| field.sig.0 != *b"FNAM"));
    }

    #[test]
    fn post_translate_does_not_expose_workshop_recipe_without_created_object() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "SCORE_S25_workshop_co_Structure_VinesJailCell_WallFull",
            FO76_WORKSHOP_CATEGORY_WALLS,
        );
        record.fields.retain(|field| field.sig.0 != *b"CNAM");
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").plugin,
            interner.intern(FO76_MASTER_NAME)
        );
        assert!(record.fields.iter().all(|field| field.sig.0 != *b"FNAM"));
    }

    #[test]
    fn post_translate_drops_cobj_raw_ctda_with_ck_rejected_cell_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("COBJ", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID,
                0x0000_DC58,
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }
