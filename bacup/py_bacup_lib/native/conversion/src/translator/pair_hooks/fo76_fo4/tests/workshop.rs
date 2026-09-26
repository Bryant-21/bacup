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
fn radshield_recipe_uses_the_replacement_chemistry_bench_keywords() {
    let interner = StringInterner::new();
    let mut record = workshop_cobj(&interner, "SFM04_Organic_co_chem_RadShield", 0x102158);
    record.form_key.local = 0x081FDD;
    push_field(&mut record, "FNAM", FieldValue::List(vec![form_key_value(&interner, 0x102150)]));
    let output = workshop_cobj_form_key(&record, b"CNAM");
    Fo76Fo4Hook::repair_radshield_recipe_workbench(&interner, &mut record);
    for sig in [b"BNAM", b"FNAM"] {
        assert_eq!(interner.resolve(workshop_cobj_form_key(&record, sig).plugin), Some(FO4_MASTER_NAME));
    }
    assert_eq!(workshop_cobj_form_key(&record, b"CNAM"), output);
    let once = format!("{:?}", record.fields);
    Fo76Fo4Hook::repair_radshield_recipe_workbench(&interner, &mut record);
    assert_eq!(format!("{:?}", record.fields), once);

    for mismatch in 0..5 {
        let mut record = workshop_cobj(&interner, "SFM04_Organic_co_chem_RadShield", 0x102158);
        record.form_key.local = 0x081FDD;
        match mismatch {
            0 => record.form_key.local = 0x081FDC,
            1 => record.form_key.plugin = interner.intern("Other.esm"),
            2 => record.eid = Some(interner.intern("OtherRecipe")),
            3 => record.sig = SigCode(*b"KYWD"),
            _ => {
                record.fields.iter_mut().find(|f| f.sig.0 == *b"BNAM").unwrap().value =
                    form_key_value(&interner, 0x102159);
            }
        }
        let before = format!("{:?}", record.fields);
        Fo76Fo4Hook::repair_radshield_recipe_workbench(&interner, &mut record);
        assert_eq!(format!("{:?}", record.fields), before, "mismatch case {mismatch}");
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
fn post_translate_routes_workshop_recipes_to_fo4_workbenches_and_drops_recipe_filter() {
    let interner = StringInterner::new();
    for (eid, category, has_created_object, bench, bench_plugin) in [
        (
            "ATX_workshop_co_Lights_TrainHeadlight",
            FO76_WORKSHOP_CATEGORY_LIGHTS,
            true,
            FO4_WORKSHOP_WORKBENCH_POWER,
            FO4_MASTER_NAME,
        ),
        (
            "ATX_workshop_co_Furniture_Generic",
            FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE,
            true,
            FO4_WORKSHOP_WORKBENCH_FURNITURE,
            FO4_MASTER_NAME,
        ),
        (
            "SCORE_S25_workshop_co_Structure_VinesJailCell_WallFull",
            FO76_WORKSHOP_WORKBENCH_ALL_TYPE,
            true,
            FO4_WORKSHOP_WORKBENCH_EXTERIOR,
            FO4_MASTER_NAME,
        ),
        (
            "workshop_co_Lights_CampFire01",
            FO76_WORKSHOP_WORKBENCH_ALL_TYPE,
            true,
            FO4_WORKSHOP_WORKBENCH_FURNITURE,
            FO4_MASTER_NAME,
        ),
        (
            "ATX_workshop_co_mod_Weapon",
            FO76_WORKSHOP_CATEGORY_LIGHTS,
            true,
            FO76_WORKSHOP_CATEGORY_LIGHTS,
            FO76_MASTER_NAME,
        ),
        (
            "SCORE_S25_workshop_co_Structure_VinesJailCell_WallFull",
            FO76_WORKSHOP_CATEGORY_WALLS,
            false,
            FO76_WORKSHOP_CATEGORY_WALLS,
            FO76_MASTER_NAME,
        ),
    ] {
        let mut record = workshop_cobj(&interner, eid, category);
        if !has_created_object {
            record.fields.retain(|field| field.sig.0 != *b"CNAM");
        }
        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let label = format!("{eid} (created object: {has_created_object})");
        let actual = workshop_cobj_form_key(&record, b"BNAM");
        assert_eq!(actual.local, bench, "{label}");
        assert_eq!(interner.resolve(actual.plugin), Some(bench_plugin), "{label}");
        assert!(record.fields.iter().all(|field| field.sig.0 != *b"FNAM"), "{label}");
    }
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
#[test]
fn pre_translate_renames_fo76_workshop_power_connection_for_vanilla_reuse() {
    let interner = StringInterner::new();
    let mut record = make_record("KYWD", &interner);
    let source_form_key = record.form_key;
    let source_editor_id = interner.intern(FO76_WORKSHOP_POWER_CONNECTION_EID);
    record.eid = Some(source_editor_id);
    push_field(&mut record, "EDID", FieldValue::String(source_editor_id));

    Fo76Fo4Hook
        .pre_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let editor_id = record.eid.and_then(|eid| interner.resolve(eid));
    assert_eq!(editor_id, Some(FO4_WORKSHOP_POWER_CONNECTION_EID));
    assert!(record.fields.iter().any(|field| {
        field.sig.0 == *b"EDID"
            && matches!(
                field.value,
                FieldValue::String(value)
                    if interner.resolve(value) == Some(FO4_WORKSHOP_POWER_CONNECTION_EID)
            )
    }));

    let target_form_key = FormKey::parse("054BA4@Fallout4.esm", &interner).unwrap();
    let mut mapper = FormKeyMapper::new(
        [(
            record.eid.unwrap(),
            target_form_key,
            SigCode::from_str("KYWD").unwrap(),
        )],
        MapperOptions {
            output_plugin_name: "SeventySix.esm".to_string(),
            use_base_game_assets: true,
            ..Default::default()
        },
        &interner,
    );
    assert_eq!(
        mapper.allocate_or_resolve(source_form_key, record.eid, record.sig),
        target_form_key
    );
}
