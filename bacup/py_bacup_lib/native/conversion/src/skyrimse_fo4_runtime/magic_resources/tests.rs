use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::schema::AuthoringSchema;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_load_no_py,
    plugin_handle_new_no_py, plugin_handle_save_no_py, plugin_handle_store_ref,
};

#[test]
fn magic_resources_keep_magicka_separate_and_roundtrip_without_virtual_master() {
    let interner = StringInterner::new();
    let options = MapperOptions {
        output_plugin_name: "Converted.esm".into(),
        source_plugin_name: "Skyrim.esm".into(),
        target_master_names: vec!["Fallout4.esm".into()],
        ..Default::default()
    };
    let mut state = MapperState::new([], options.clone());
    state.reserved_generated_object_ids.extend(0x800..0x810);
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let (keys, records) = allocate_magic_resource_records(&mut mapper, None, &interner);
    assert!(keys.iter().all(|key| key.local >= 0x810));
    for value in [24, 26, 29, 157] {
        assert_eq!(
            magic_resource_source_key(&FieldValue::Uint(value), &interner),
            None
        );
    }
    for (index, (number, name, ..)) in RESOURCES
        .iter()
        .enumerate()
        .filter(|(_, (number, ..))| *number != u32::MAX)
    {
        assert_eq!(
            magic_resource_source_key(&FieldValue::Uint((*number).into()), &interner),
            Some(source_key(index, &interner))
        );
        assert_eq!(
            magic_resource_source_key(&FieldValue::String(interner.intern(name)), &interner),
            Some(source_key(index, &interner))
        );
    }
    let mut reset = MapperState::new([], options);
    let mut reset_mapper = FormKeyMapper::from_state(&mut reset, &interner);
    let (restored, duplicates) =
        allocate_magic_resource_records(&mut reset_mapper, Some(keys), &interner);
    assert_eq!(keys, restored);
    assert!(duplicates.is_empty());
    for _ in 0..32 {
        assert!(!keys.contains(&reset_mapper.allocate_generated()));
    }

    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    for record in records {
        crate::target_write::add_record_native(handle, record, &schema, &interner).unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Converted.esm");
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        assert_eq!(slot.parsed.header.masters.as_slice(), ["Fallout4.esm"]);
    }
    for (index, key) in keys.into_iter().enumerate() {
        let record = crate::source_read::read_record_relayout_by_form_key(
            reopened, &key, &schema, &interner, None,
        )
        .unwrap();
        let flags = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"AVFL")
            .unwrap();
        assert_eq!(flags.value, FieldValue::Uint(RESOURCES[index].3.into()));
        let default = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"NAM0")
            .unwrap();
        assert_eq!(default.value, FieldValue::Float(RESOURCES[index].2));
    }
    assert!(plugin_handle_close_native(reopened));
}

#[test]
fn magic_resources_output_preserves_source_pools_and_idempotent_catalog() {
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;
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
        let source = interner.intern("Skyrim.esm");
        let target = interner.intern("Converted.esm");
        let race_key = FormKey {
            local: 0x1000,
            plugin: source,
        };
        let npc_key = FormKey {
            local: 0x1001,
            plugin: source,
        };
        let class_key = FormKey {
            local: 0x1002,
            plugin: source,
        };
        let target_race = FormKey {
            local: 0x2000,
            plugin: target,
        };
        let target_npc = FormKey {
            local: 0x2001,
            plugin: target,
        };
        let mut race = Record::new(SigCode(*b"RACE"), race_key);
        race.eid = Some(interner.intern("TestCreatureRace"));
        let mut race_data = vec![0; 164];
        for (offset, value) in [
            (36, 100.0_f32),
            (40, 50.0),
            (44, 80.0),
            (84, 0.7),
            (88, 3.0),
            (92, 5.0),
        ] {
            race_data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        race.fields.push(field(
            *b"DATA",
            FieldValue::Bytes(race_data.into_iter().collect()),
        ));
        let mut class = Record::new(SigCode(*b"CLAS"), class_key);
        class.eid = Some(interner.intern("TestCreatureClass"));
        let mut class_data = vec![0; 36];
        class_data[32..35].copy_from_slice(&[1, 2, 3]);
        class.fields.push(field(
            *b"DATA",
            FieldValue::Bytes(class_data.into_iter().collect()),
        ));
        let mut npc = Record::new(SigCode(*b"NPC_"), npc_key);
        npc.eid = Some(interner.intern("TestCreatureNPC"));
        let mut config = vec![0; 24];
        config[..4].copy_from_slice(&0x10_u32.to_le_bytes());
        config[4..6].copy_from_slice(&5_i16.to_le_bytes());
        config[6..8].copy_from_slice(&10_i16.to_le_bytes());
        config[8..10].copy_from_slice(&3_u16.to_le_bytes());
        config[20..22].copy_from_slice(&20_i16.to_le_bytes());
        npc.fields.extend([
            field(*b"ACBS", FieldValue::Bytes(config.into_iter().collect())),
            field(*b"RNAM", FieldValue::FormKey(race_key)),
            field(*b"CNAM", FieldValue::FormKey(class_key)),
        ]);
        let mut cost_setting = Record::new(
            SigCode(*b"GMST"),
            FormKey {
                local: 0x1003,
                plugin: source,
            },
        );
        cost_setting.eid = Some(interner.intern("fMagicCasterPCSkillCostBase"));
        cost_setting
            .fields
            .push(field(*b"DATA", FieldValue::Float(0.0025)));
        for record in [race, class, npc, cost_setting] {
            crate::target_write::add_record_native(
                source_handle,
                record,
                &run.schema_source,
                interner,
            )
            .unwrap();
        }
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: "Skyrim.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, interner);
        let (resources, records) = allocate_magic_resource_records(&mut mapper, None, interner);
        mapper.add_mapping(race_key, target_race);
        mapper.add_mapping(npc_key, target_npc);
        let second_npc = FormKey {
            local: 0x1004,
            plugin: source,
        };
        let source_list = FormKey {
            local: 0x1005,
            plugin: source,
        };
        let child_npc = FormKey {
            local: 0x1006,
            plugin: source,
        };
        let target_second = FormKey {
            local: 0x2004,
            plugin: target,
        };
        let target_list = FormKey {
            local: 0x2005,
            plugin: target,
        };
        let target_child = FormKey {
            local: 0x2006,
            plugin: target,
        };
        for (source, target) in [
            (second_npc, target_second),
            (source_list, target_list),
            (child_npc, target_child),
        ] {
            mapper.add_mapping(source, target);
        }
        for (key, inherited) in [(second_npc, false), (child_npc, true)] {
            let mut record = Record::new(SigCode(*b"NPC_"), key);
            record.eid = Some(interner.intern(&format!("TemplateNPC{:X}", key.local)));
            let mut config = vec![0; 24];
            config[..4].copy_from_slice(&(if inherited { 0x310_u32 } else { 0x10 }).to_le_bytes());
            config[4..6].copy_from_slice(&75_i16.to_le_bytes());
            config[8..10].copy_from_slice(&3_u16.to_le_bytes());
            config[18..20].copy_from_slice(&(if inherited { 0xA_u16 } else { 0 }).to_le_bytes());
            record.fields.extend([
                field(*b"ACBS", FieldValue::Bytes(config.into_iter().collect())),
                field(*b"RNAM", FieldValue::FormKey(race_key)),
                field(*b"CNAM", FieldValue::FormKey(class_key)),
            ]);
            if inherited {
                record
                    .fields
                    .push(field(*b"TPLT", FieldValue::FormKey(source_list)));
            }
            crate::target_write::add_record_native(
                source_handle,
                record,
                &run.schema_source,
                interner,
            )
            .unwrap();
        }
        for (handle, key, first, second, schema) in [
            (
                source_handle,
                source_list,
                npc_key,
                second_npc,
                &run.schema_source,
            ),
            (
                target_handle,
                target_list,
                target_npc,
                target_second,
                &run.schema_target,
            ),
        ] {
            let mut list = Record::new(SigCode(*b"LVLN"), key);
            list.eid = Some(interner.intern("ExactTemplateChoice"));
            for (level, npc) in [(1, first), (12, second)] {
                list.fields.push(field(
                    *b"LVLO",
                    FieldValue::Struct(vec![
                        (interner.intern("level"), FieldValue::Uint(level)),
                        (interner.intern("unknown_u8_1"), FieldValue::Uint(0)),
                        (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
                        (interner.intern("npc"), FieldValue::FormKey(npc)),
                        (interner.intern("count"), FieldValue::Uint(1)),
                        (
                            interner.intern(if handle == source_handle {
                                "unknown_u8_5"
                            } else {
                                "chance_none"
                            }),
                            FieldValue::Uint(0),
                        ),
                        (interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
                    ]),
                ));
            }
            crate::target_write::add_record_native(handle, list, schema, interner).unwrap();
        }
        let source_effect = FormKey {
            local: 0x1100,
            plugin: source,
        };
        let target_effect = FormKey {
            local: 0x2100,
            plugin: target,
        };
        mapper.add_mapping(source_effect, target_effect);
        for (handle, key, schema) in [
            (source_handle, source_effect, &run.schema_source),
            (target_handle, target_effect, &run.schema_target),
        ] {
            let length = schema
                .struct_field_layout_versioned("MGEF", "DATA", None)
                .iter()
                .map(|field| field.offset + field.width)
                .max()
                .unwrap();
            let mut data = vec![0; length];
            data[..4].copy_from_slice(&0x400_u32.to_le_bytes());
            data[64..68].copy_from_slice(&11_u32.to_le_bytes());
            data[4..8].copy_from_slice(&2.5_f32.to_le_bytes());
            let mut record = Record::new(SigCode(*b"MGEF"), key);
            record.eid = Some(interner.intern("TestResourceEffect"));
            record.fields.push(field(
                *b"DATA",
                FieldValue::Bytes(data.into_iter().collect()),
            ));
            crate::target_write::add_record_native(handle, record, schema, interner).unwrap();
        }
        for (ordinal, spell_type, zero_target_cost) in
            [(1, 0_u32, false), (2, 1, false), (3, 0, true)]
        {
            let source_spell = FormKey {
                local: 0x1100 + ordinal,
                plugin: source,
            };
            let target_spell = FormKey {
                local: 0x2100 + ordinal,
                plugin: target,
            };
            mapper.add_mapping(source_spell, target_spell);
            for (handle, key, effect, schema) in [
                (
                    source_handle,
                    source_spell,
                    source_effect,
                    &run.schema_source,
                ),
                (
                    target_handle,
                    target_spell,
                    target_effect,
                    &run.schema_target,
                ),
            ] {
                let mut data = vec![0; 36];
                let cost = if zero_target_cost && handle == target_handle {
                    0_u32
                } else {
                    40
                };
                data[..4].copy_from_slice(&cost.to_le_bytes());
                data[4..8].copy_from_slice(&1_u32.to_le_bytes());
                data[8..12].copy_from_slice(&spell_type.to_le_bytes());
                data[12..16].copy_from_slice(&0.5_f32.to_le_bytes());
                data[16..20].copy_from_slice(&1_u32.to_le_bytes());
                data[20..24].copy_from_slice(&2_u32.to_le_bytes());
                let mut parameters = vec![0; 12];
                parameters[..4].copy_from_slice(&10_f32.to_le_bytes());
                let mut record = Record::new(SigCode(*b"SPEL"), key);
                record.eid = Some(interner.intern(&format!("TestResourceSpell{ordinal}")));
                record.fields.extend([
                    field(*b"SPIT", FieldValue::Bytes(data.into_iter().collect())),
                    field(*b"EFID", FieldValue::FormKey(effect)),
                    field(
                        *b"EFIT",
                        FieldValue::Bytes(parameters.into_iter().collect()),
                    ),
                ]);
                crate::target_write::add_record_native(handle, record, schema, interner).unwrap();
            }
        }
        run.skyrim_magic_resource_records = Some(resources);
        for record in records {
            crate::target_write::add_record_native(
                target_handle,
                record,
                &run.schema_target,
                interner,
            )
            .unwrap();
        }
        let mut race = Record::new(SigCode(*b"RACE"), target_race);
        race.eid = Some(interner.intern("ConvertedRace"));
        let mut npc = Record::new(SigCode(*b"NPC_"), target_npc);
        npc.eid = Some(interner.intern("ConvertedNPC"));
        npc.fields.extend([
            field(
                *b"ACBS",
                FieldValue::Bytes(vec![0; 20].into_iter().collect()),
            ),
            field(
                *b"DNAM",
                FieldValue::Bytes(vec![0; 8].into_iter().collect()),
            ),
            field(*b"RNAM", FieldValue::FormKey(target_race)),
        ]);
        let mut second = npc.clone();
        second.form_key = target_second;
        second.eid = Some(interner.intern("ConvertedSecondNPC"));
        let mut child = npc.clone();
        child.form_key = target_child;
        child.eid = Some(interner.intern("ConvertedChildNPC"));
        for record in [race, npc, second, child] {
            crate::target_write::add_record_native(
                target_handle,
                record,
                &run.schema_target,
                interner,
            )
            .unwrap();
        }
        run.mapper_state = Some(state);
        let output = prepare_magic_runtime_output(run).unwrap().unwrap();
        assert_eq!(output.records_changed, 4);
        output.write(directory.path()).unwrap();
        let catalog_path = directory
            .path()
            .join("F4SE/Plugins/B21_ConversionRuntime/Magic/Converted.esm.json");
        let first = std::fs::read(&catalog_path).unwrap();
        let catalog: serde_json::Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(catalog["actors"].as_array().unwrap().len(), 2);
        assert_eq!(catalog["actors"][0]["npc"], "002001:Converted.esm");
        assert_eq!(catalog["actors"][1]["npc"], "002004:Converted.esm");
        assert_eq!(catalog["actors"][0]["race"], "002000:Converted.esm");
        assert_eq!(
            catalog["actors"][1]["formula"]["base"],
            serde_json::json!([100.0, 125.0, 80.0])
        );
        assert_eq!(
            catalog["invisibility_effects"],
            serde_json::json!(["002100:Converted.esm"])
        );
        assert_eq!(catalog["actors"][0]["baseline_magicka"], 62.0);
        assert_eq!(
            catalog["actors"][0]["skill_formula"]["base"]
                .as_array()
                .unwrap()
                .len(),
            18
        );
        assert_eq!(
            catalog["cost_settings"]["player"]["base"],
            serde_json::json!(0.0025_f32)
        );
        assert_eq!(
            catalog["actors"][0]["formula"]["base"],
            serde_json::json!([120.0, 55.0, 90.0])
        );
        assert_eq!(
            catalog["spells"],
            serde_json::json!(["002101:Converted.esm"])
        );
        assert_eq!(
            catalog["disease_spells"],
            serde_json::json!(["002102:Converted.esm"])
        );
        assert!(
            catalog["resource"]["magicka"]
                .as_str()
                .unwrap()
                .ends_with(":Converted.esm")
        );
        prepare_magic_runtime_output(run)
            .unwrap()
            .unwrap()
            .write(directory.path())
            .unwrap();
        assert_eq!(std::fs::read(catalog_path).unwrap(), first);
        let target = run
            .read_target_record_if_available(target_npc)
            .unwrap()
            .unwrap();
        assert_eq!(
            target
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"PRPS")
                .count(),
            1
        );
        let FieldValue::Bytes(stats) = &target
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DNAM")
            .unwrap()
            .value
        else {
            panic!("DNAM was not serialized");
        };
        assert_eq!(u16::from_le_bytes(stats[..2].try_into().unwrap()), 133);
        assert_eq!(u16::from_le_bytes(stats[2..4].try_into().unwrap()), 100);
        let child = run
            .read_target_record_if_available(target_child)
            .unwrap()
            .unwrap();
        let template = child
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"TPLT")
            .unwrap();
        assert_eq!(template.value, FieldValue::FormKey(target_list));
        let FieldValue::Bytes(config) = &child
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"ACBS")
            .unwrap()
            .value
        else {
            panic!("raw target ACBS")
        };
        assert_eq!(u16::from_le_bytes(config[14..16].try_into().unwrap()), 0xA);
        assert_eq!(
            u32::from_le_bytes(config[..4].try_into().unwrap()) & 0x300,
            0x300
        );
        Ok(())
    })
    .unwrap();
    drop_run(id);
    assert!(plugin_handle_close_native(source_handle));
    assert!(plugin_handle_close_native(target_handle));
}
