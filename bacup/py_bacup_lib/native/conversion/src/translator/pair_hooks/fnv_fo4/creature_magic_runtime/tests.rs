use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::ids::SubrecordSig;
use crate::record::FieldEntry;
use crate::run::{
    CreatureDependencyAdmission, RunConfig, RunError, RunParams, create_run, drop_run, with_run,
};
use esp_authoring_core::plugin_runtime::{plugin_handle_close_native, plugin_handle_new_no_py};

fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(sig),
        value,
    }
}

#[test]
fn fnv_native_catalog_cleanup_removes_only_absent_current_output_catalogs() {
    let source_handle = plugin_handle_new_no_py("FalloutNV.esm", Some("fnv"));
    let target_handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
    let id = create_run(RunParams {
        source: Game::Fnv,
        target: Game::Fo4,
        source_handle_id: source_handle,
        target_handle_id: target_handle,
        master_handle_ids: vec![],
        config: RunConfig {
            output_plugin_name: "Converted.esm".into(),
            ..Default::default()
        },
    })
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let catalogs = directory.path().join("F4SE/Plugins/B21_ConversionRuntime");
    for runtime in ["FnvRadiation", "FnvImageSpace", "Magic"] {
        std::fs::create_dir_all(catalogs.join(runtime)).unwrap();
        for plugin in ["Converted.esm", "Unrelated.esm"] {
            std::fs::write(
                catalogs.join(runtime).join(format!("{plugin}.json")),
                b"retained",
            )
            .unwrap();
        }
    }
    with_run(id, |run| -> Result<(), RunError> {
        remove_stale_native_outputs(run, directory.path(), false, true).unwrap();
        assert!(!catalogs.join("FnvRadiation/Converted.esm.json").exists());
        assert!(catalogs.join("FnvImageSpace/Converted.esm.json").exists());
        remove_stale_native_outputs(run, directory.path(), false, false).unwrap();
        assert!(!catalogs.join("FnvImageSpace/Converted.esm.json").exists());
        for runtime in ["FnvRadiation", "FnvImageSpace", "Magic"] {
            assert_eq!(
                std::fs::read(catalogs.join(runtime).join("Unrelated.esm.json")).unwrap(),
                b"retained"
            );
        }
        assert!(catalogs.join("Magic/Converted.esm.json").exists());
        std::fs::write(
            catalogs.join("FnvRadiation/Converted.esm.json"),
            b"retained",
        )
        .unwrap();
        let source = run.source;
        run.source = Game::SkyrimSe;
        remove_stale_native_outputs(run, directory.path(), false, false).unwrap();
        assert!(catalogs.join("FnvRadiation/Converted.esm.json").exists());
        run.source = source;
        run.source_handle_id = 0;
        remove_stale_native_outputs(run, directory.path(), false, false).unwrap();
        assert!(catalogs.join("FnvRadiation/Converted.esm.json").exists());
        run.source_handle_id = source_handle;
        for plugin in ["../Unrelated.esm", "C:Unrelated.esm", "C:\\Unrelated.esm"] {
            run.config.output_plugin_name = plugin.into();
            assert!(remove_stale_native_outputs(run, directory.path(), false, false).is_err());
        }
        Ok(())
    })
    .unwrap();
    drop_run(id).unwrap();
    assert!(plugin_handle_close_native(source_handle));
    assert!(plugin_handle_close_native(target_handle));
}

fn explosion(key: FormKey, bs_units: bool) -> Record {
    let mut record = Record::new(SigCode(*b"EXPL"), key);
    let mut data = vec![0; 52];
    data[20..24].copy_from_slice(&u32::from(bs_units).to_le_bytes());
    data[36..40].copy_from_slice(&5.0_f32.to_le_bytes());
    data[40..44].copy_from_slice(&3.0_f32.to_le_bytes());
    data[44..48].copy_from_slice(&1024.0_f32.to_le_bytes());
    record
        .fields
        .push(field(*b"DATA", FieldValue::Bytes(data.into())));
    record
}

#[test]
fn fnv_radiation_catalog_requires_emitted_explosions_and_uses_source_creature_races() {
    let source_handle = plugin_handle_new_no_py("FalloutNV.esm", Some("fnv"));
    let target_handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
    let id = create_run(RunParams {
        source: Game::Fnv,
        target: Game::Fo4,
        source_handle_id: source_handle,
        target_handle_id: target_handle,
        master_handle_ids: vec![],
        config: RunConfig {
            output_plugin_name: "Converted.esm".into(),
            ..Default::default()
        },
    })
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    with_run(id, |run| -> Result<(), RunError> {
        let source = run.interner.intern("FalloutNV.esm");
        let target = run.interner.intern("Converted.esm");
        let source_explosion = FormKey {
            local: 0x1000,
            plugin: source,
        };
        let unadmitted_explosion = FormKey {
            local: 0x1001,
            plugin: source,
        };
        let source_creature = FormKey {
            local: 0x1002,
            plugin: source,
        };
        let source_npc = FormKey {
            local: 0x1003,
            plugin: source,
        };
        let target_explosion = FormKey {
            local: 0x2000,
            plugin: target,
        };
        let target_creature = FormKey {
            local: 0x2002,
            plugin: target,
        };
        let target_npc = FormKey {
            local: 0x2003,
            plugin: target,
        };
        let creature_race = FormKey {
            local: 0x3002,
            plugin: target,
        };
        let human_race = FormKey {
            local: 0x3003,
            plugin: target,
        };
        for record in [
            explosion(source_explosion, false),
            explosion(unadmitted_explosion, true),
            Record::new(SigCode(*b"CREA"), source_creature),
            Record::new(SigCode(*b"NPC_"), source_npc),
        ] {
            crate::target_write::add_record_native(
                source_handle,
                record,
                &run.schema_source,
                &run.interner,
            )
            .unwrap();
        }
        for (offset, name, scalar) in [
            (0, "fRadiationAccumulationRate", 1.25_f32),
            (1, "fRadiationDecayRate", 0.02),
            (2, "fRadiationReevaluationRate", 1.5),
            (3, "fPlayerMaxResistance", 90.0),
            (4, "fRadiationAvoidThreshold", 45.0),
            (5, "fBSUnitsPerFoot", 24.0),
        ] {
            let mut record = Record::new(
                SigCode(*b"GMST"),
                FormKey {
                    local: 0x1100 + offset,
                    plugin: source,
                },
            );
            record.eid = Some(run.interner.intern(name));
            record
                .fields
                .push(field(*b"DATA", FieldValue::Float(scalar)));
            crate::target_write::add_record_native(
                source_handle,
                record,
                &run.schema_source,
                &run.interner,
            )
            .unwrap();
        }
        for (npc, race) in [(target_creature, creature_race), (target_npc, human_race)] {
            let mut record = Record::new(SigCode(*b"NPC_"), npc);
            record
                .fields
                .push(field(*b"RNAM", FieldValue::FormKey(race)));
            crate::target_write::add_record_native(
                target_handle,
                record,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();
            crate::target_write::add_record_native(
                target_handle,
                Record::new(SigCode(*b"RACE"), race),
                &run.schema_target,
                &run.interner,
            )
            .unwrap();
        }
        let mut state = MapperState::new(
            [],
            MapperOptions {
                source_plugin_name: "FalloutNV.esm".into(),
                output_plugin_name: "Converted.esm".into(),
                ..Default::default()
            },
        );
        state.source_to_target.extend([
            (source_explosion, target_explosion),
            (source_creature, target_creature),
            (source_npc, target_npc),
        ]);
        run.mapper_state = Some(state);
        assert!(prepare_fnv_radiation_output(run).unwrap().is_none());
        run.install_creature_dependency_admissions(vec![CreatureDependencyAdmission {
            source_form_key: source_explosion,
            source_signature: SigCode(*b"EXPL"),
            owner_family_ids: vec!["glowing_one".into()],
            legacy_idle: None,
        }])?;
        assert!(
            prepare_fnv_radiation_output(run)
                .err()
                .unwrap()
                .contains("was not emitted")
        );
        let mut converted = explosion(target_explosion, false);
        super::super::legacy_explosions::lower_legacy_explosion(&mut converted);
        crate::target_write::add_record_native(
            target_handle,
            converted,
            &run.schema_target,
            &run.interner,
        )
        .unwrap();
        let output = prepare_fnv_radiation_output(run).unwrap().unwrap();
        output.write(directory.path()).unwrap();
        let path = directory
            .path()
            .join("F4SE/Plugins/B21_ConversionRuntime/FnvRadiation/Converted.esm.json");
        let first = std::fs::read(&path).unwrap();
        let catalog: serde_json::Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(catalog["version"], 1);
        assert_eq!(catalog["explosions"].as_array().unwrap().len(), 1);
        assert_eq!(
            catalog["explosions"][0],
            serde_json::json!({
                "explosion": "002000:Converted.esm", "level": 5.0,
                "dissipation": 3.0, "radius_bs": 24576.0
            })
        );
        assert_eq!(
            catalog["creature_races"],
            serde_json::json!(["003002:Converted.esm"])
        );
        assert_eq!(catalog["settings"]["accumulation_rate"], 1.25);
        assert_eq!(catalog["settings"]["reevaluation_rate"], 1.5);
        assert_eq!(catalog["settings"]["player_max_resistance"], 90.0);
        assert_eq!(catalog["settings"]["avoid_threshold"], 45.0);
        assert!((catalog["settings"]["decay_rate"].as_f64().unwrap() - 0.02).abs() < 1e-8);
        prepare_fnv_radiation_output(run)
            .unwrap()
            .unwrap()
            .write(directory.path())
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), first);
        Ok(())
    })
    .unwrap();
    drop_run(id).unwrap();
    assert!(plugin_handle_close_native(source_handle));
    assert!(plugin_handle_close_native(target_handle));
}

#[test]
#[ignore = "requires MODKIT_FNV_CREATURE_MAGIC_PLUGIN pointing to FalloutNV.esm"]
fn source_glowing_one_radiation_preserves_source_envelope() {
    let path = std::env::var("MODKIT_FNV_CREATURE_MAGIC_PLUGIN").expect("FNV source fixture");
    let handle = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
        &path,
        Some("fnv"),
        None,
        None,
        true,
    )
    .unwrap();
    let interner = StringInterner::new();
    let schema = crate::schema::AuthoringSchema::for_game("fnv").unwrap();
    let key = FormKey {
        local: 0x133032,
        plugin: interner.intern("FalloutNV.esm"),
    };
    let record = read_record_relayout_by_form_key(handle, &key, &schema, &interner, None).unwrap();
    assert_eq!(
        radiation_scalars(&record).unwrap(),
        Some((5.0, 3.0, 1024.0, true))
    );
    assert!(plugin_handle_close_native(handle));
}
