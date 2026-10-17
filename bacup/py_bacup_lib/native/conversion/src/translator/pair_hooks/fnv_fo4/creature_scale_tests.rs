#[test]
fn source_capsule_scaling_requires_typed_keyword_and_unit_npc_height() {
    use crate::record::FieldValue;

    let input = adapter_fixture(LegacyCreatureGame::Fnv, false);
    let family = input.ledger.families[0].clone();
    let source_capsule = family
        .race_data
        .as_ref()
        .unwrap()
        .measured_evidence
        .capsule
        .as_ref()
        .unwrap();
    assert_eq!(input.rig.capsule.height, source_capsule.total_height);
    assert_eq!(input.rig.capsule.radius, source_capsule.radius);
    validate_family_contract(&input, &family, "fnv").unwrap();

    let mut missing_keyword = input.clone();
    missing_keyword.projection.race_keywords.clear();
    assert!(
        validate_family_contract(&missing_keyword, &family, "fnv")
            .unwrap_err()
            .to_string()
            .contains("controller_scale_keyword_missing")
    );
    let mut baked_scale = input.clone();
    baked_scale.rig.capsule.height *= 0.3;
    baked_scale.rig.capsule.radius *= 0.3;
    assert!(
        validate_family_contract(&baked_scale, &family, "fnv")
            .unwrap_err()
            .to_string()
            .contains("capsule_mismatch")
    );

    let keyword = super::super::creature_profile_race_data::controller_scale_keyword();
    let interner = StringInterner::new();
    let recipe = build_fnv_fo3_source_rig_executable_recipe(input).unwrap();
    let batch = recipe.rebuild_record_family_batch(&interner).unwrap();
    let closure = &batch.closures[0];
    assert!(closure.required_target_records.contains(&keyword));
    let race = closure.closure.record("RACE").unwrap();
    let keywords = &race
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "KWDA")
        .unwrap()
        .value;
    assert_eq!(
        keywords,
        &FieldValue::List(vec![FieldValue::FormKey(FormKey {
            local: 0x249F53,
            plugin: interner.intern("Fallout4.esm"),
        })])
    );
    assert_eq!(
        race.fields
            .iter()
            .find(|field| field.sig.as_str() == "KSIZ")
            .unwrap()
            .value,
        FieldValue::Uint(1)
    );
    // TESNPC::InitializeData defaults omitted NAM6/NAM7 to 1, leaving RACE height as the sole base-size factor.
    assert!(
        closure
            .closure
            .record("NPC_")
            .unwrap()
            .fields
            .iter()
            .all(|field| !matches!(field.sig.as_str(), "NAM6" | "NAM7"))
    );
}

#[test]
#[ignore = "requires installed FalloutNV.esm and full source creature evidence"]
fn real_ant_profiles_share_unscaled_capsule_with_distinct_runtime_sizes() {
    let workspace =
        std::path::PathBuf::from(std::env::var("B21_FNV_CREATURE_ACTION_CORPUS").unwrap());
    let ledger: CreatureFamilyRecipeLedger = serde_json::from_slice(
        &fs::read(workspace.join("tmp/fnv_fo3_full_source_action_evidence.json")).unwrap(),
    )
    .unwrap();
    let family = ledger
        .families
        .iter()
        .find(|family| {
            family.rig.game == LegacyCreatureGame::Fnv
                && family.rig.skeleton_path == "creatures/giantant/skeleton.nif"
        })
        .unwrap();
    let evidence = &family.race_data.as_ref().unwrap().measured_evidence;
    let capsule = evidence.capsule.as_ref().unwrap().clone();
    assert_eq!(capsule.total_height, 330.0);
    assert_eq!(capsule.radius, 165.0);
    let inventory: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("tmp/creature_body_part_plugins.json")).unwrap(),
    )
    .unwrap();
    let path = inventory
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["game"] == "fnv"
                && entry["path"]
                    .as_str()
                    .unwrap()
                    .to_ascii_lowercase()
                    .ends_with("/falloutnv.esm")
        })
        .unwrap()["path"]
        .as_str()
        .unwrap();
    let handle = crate::run::OwnedPluginHandle::load_projection(
        std::path::Path::new(path),
        "fnv",
        &[*b"CREA"],
    )
    .unwrap();
    let keys =
        esp_authoring_core::plugin_runtime::plugin_handle_record_form_keys_by_signature_no_py(
            handle.id(),
            "CREA",
        )
        .unwrap();
    let schema = crate::schema::AuthoringSchema::for_game("fnv").unwrap();
    let interner = StringInterner::new();
    let mut heights = Vec::new();
    for (local, expected_scale, expected_height, expected_radius) in [
        (0x01CF81, 0.1_f32, 33.0_f32, 16.5_f32),
        (0x01CF80, 0.3_f32, 99.0_f32, 49.5_f32),
    ] {
        let key = keys
            .iter()
            .find(|key| {
                u32::from_str_radix(key.rsplit(':').next().unwrap(), 16).ok() == Some(local)
            })
            .unwrap();
        let source = crate::source_read::read_record(handle.id(), key, &schema, &interner).unwrap();
        let owner = super::super::creature_catalog::StableFormKey {
            local,
            plugin: "FalloutNV.esm".into(),
        };
        let profile =
            super::super::creature_profile_race_data::prepare(family, &owner, &source, &interner)
                .unwrap();
        assert_eq!(profile.base_scale_bits, expected_scale.to_bits());
        let RaceDataMapping::Mapped { target } = &profile.derivation.mapping else {
            panic!("mapped ant profile")
        };
        assert_eq!(target.male_height, expected_scale);
        assert_eq!(target.female_height, expected_scale);
        assert!((capsule.total_height * target.male_height - expected_height).abs() < 0.0001);
        assert!((capsule.radius * target.male_height - expected_radius).abs() < 0.0001);
        assert_eq!(
            family
                .race_data
                .as_ref()
                .unwrap()
                .measured_evidence
                .capsule
                .as_ref()
                .unwrap(),
            &capsule
        );
        heights.push(target.male_height);
    }
    assert!(heights[1] > heights[0]);
}
