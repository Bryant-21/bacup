use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::ids::SubrecordSig;
use crate::record::FieldEntry;
use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
use crate::target_write::{add_record_native, replace_record_native};
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_no_py,
};

#[test]
fn source_weapon_catalog_publishes_without_horses_and_validates_target_ammunition() {
    let source_handle = plugin_handle_new_no_py("Skyrim.esm", Some("skyrimse"));
    let target_handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
    plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
    let id = create_run(RunParams {
        source: Game::SkyrimSe,
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
        let interner = &run.interner;
        let key = |local, plugin: &str| FormKey { local, plugin: interner.intern(plugin) };
        let source = |local| key(local, "Skyrim.esm");
        let target = |local| key(local, "Converted.esm");
        let field = |sig, value| FieldEntry { sig: SubrecordSig(sig), value };
        let mut mapper = MapperState::new([], MapperOptions {
            output_plugin_name: "Converted.esm".into(),
            source_plugin_name: "Skyrim.esm".into(),
            target_master_names: vec!["Fallout4.esm".into()],
            ..Default::default()
        });
        for (local, animation_type) in [(0x1000, 7u8), (0x1001, 9)] {
            let mut record = Record::new(SigCode(*b"WEAP"), source(local));
            let mut data = vec![0; 100];
            data[0] = animation_type;
            record.fields.push(field(*b"DNAM", FieldValue::Bytes(data.into())));
            add_record_native(source_handle, record, &run.schema_source, interner).unwrap();
            add_record_native(target_handle, Record::new(SigCode(*b"WEAP"), target(local)), &run.schema_target, interner).unwrap();
            mapper.source_to_target.insert(source(local), target(local));
        }
        let mut source_ammo = Record::new(SigCode(*b"AMMO"), source(0x1002));
        let mut data = vec![0; 16];
        data[..4].copy_from_slice(&0x1003u32.to_le_bytes());
        data[4..8].copy_from_slice(&4u32.to_le_bytes());
        data[8..12].copy_from_slice(&8.0f32.to_le_bytes());
        source_ammo.fields.push(field(*b"DATA", FieldValue::Bytes(data.into())));
        add_record_native(source_handle, source_ammo, &run.schema_source, interner).unwrap();
        let mut target_ammo = Record::new(SigCode(*b"AMMO"), target(0x1002));
        target_ammo.fields.push(field(*b"DNAM", FieldValue::Struct(vec![
            (interner.intern("projectile"), FieldValue::FormKey(target(0x1003))),
            (interner.intern("flags"), FieldValue::Uint(0)),
            (interner.intern("damage"), FieldValue::Float(8.0)),
            (interner.intern("health"), FieldValue::Uint(0)),
        ])));
        add_record_native(target_handle, target_ammo.clone(), &run.schema_target, interner).unwrap();
        add_record_native(target_handle, Record::new(SigCode(*b"PROJ"), target(0x1003)), &run.schema_target, interner).unwrap();
        mapper.source_to_target.insert(source(0x1002), target(0x1002));
        run.mapper_state = Some(mapper);
        let output = prepare_weapon_runtime_output(run).unwrap().unwrap();
        assert_eq!(output.write(directory.path()).unwrap(), 1);
        let path = directory.path().join("F4SE/Plugins/B21_ConversionRuntime/Weapons/Converted.esm.json");
        let first = std::fs::read(&path).unwrap();
        let catalog: serde_json::Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(catalog["source_game"], "skyrimse");
        assert_eq!(catalog["weapons"], serde_json::json!([
            {"weapon":"001000:Converted.esm", "animation_type":7, "dwarven_crossbow":false},
            {"weapon":"001001:Converted.esm", "animation_type":9, "dwarven_crossbow":false},
        ]));
        assert_eq!(catalog["ammunition"], serde_json::json!([
            {"ammo":"001002:Converted.esm", "non_bolt":true, "damage":8.0, "projectile":"001003:Converted.esm"},
        ]));
        assert_eq!(catalog["bow_stagger"]["weapons"].as_array().unwrap().len(), 2);
        assert_eq!(catalog["bow_stagger"]["weapons"][0]["weapon"], "001000:Converted.esm");
        assert_eq!(catalog["bow_stagger"]["weapons"][1]["animation_type"], 9);
        assert!(catalog["bow_stagger"]["settings"].as_object().unwrap().contains_key("fStaggerMassMult"));
        assert!(!directory.path().join("F4SE/Plugins/B21_ConversionRuntime/Riding").exists());
        prepare_weapon_runtime_output(run).unwrap().unwrap().write(directory.path()).unwrap();
        assert_eq!(first, std::fs::read(path).unwrap());
        run.source = Game::Fnv;
        assert!(prepare_weapon_runtime_output(run).unwrap().is_none());
        run.source = Game::SkyrimSe;

        let FieldValue::Struct(fields) = &mut target_ammo.fields[0].value else { panic!() };
        fields[2].1 = FieldValue::Float(9.0);
        replace_record_native(target_handle, target_ammo.clone(), &run.schema_target, interner).unwrap();
        assert!(prepare_weapon_runtime_output(run).err().unwrap().contains("changed source damage"));
        let FieldValue::Struct(fields) = &mut target_ammo.fields[0].value else { panic!() };
        fields[2].1 = FieldValue::Float(8.0);
        fields[0].1 = FieldValue::FormKey(target(0x1000));
        replace_record_native(target_handle, target_ammo, &run.schema_target, interner).unwrap();
        assert!(prepare_weapon_runtime_output(run).err().unwrap().contains("lost its projectile"));
        run.mapper_state.as_mut().unwrap().source_to_target.insert(source(0x1000), target(0x1002));
        assert!(prepare_weapon_runtime_output(run).err().unwrap().contains("mapped to a non-WEAP"));
        Ok(())
    }).unwrap();
    drop_run(id).unwrap();
    assert!(plugin_handle_close_native(source_handle));
    assert!(plugin_handle_close_native(target_handle));
}

#[test]
fn empty_weapon_catalog_supersedes_its_own_previous_publication() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory
        .path()
        .join("F4SE/Plugins/B21_ConversionRuntime/Weapons");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Converted.esm.json"), b"stale").unwrap();
    std::fs::write(root.join("Other.esm.json"), b"other").unwrap();
    let output = WeaponRuntimeOutput {
        version: 1,
        source_game: "skyrimse",
        weapons: vec![],
        ammunition: vec![],
        bow_stagger: Default::default(),
        plugin: "Converted.esm".into(),
    };
    assert_eq!(output.write(directory.path()).unwrap(), 1);
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("Converted.esm.json")).unwrap()).unwrap();
    assert_eq!(
        catalog,
        serde_json::json!({"version":1,"source_game":"skyrimse","weapons":[],"ammunition":[]})
    );
    assert_eq!(
        std::fs::read(root.join("Other.esm.json")).unwrap(),
        b"other"
    );
}
