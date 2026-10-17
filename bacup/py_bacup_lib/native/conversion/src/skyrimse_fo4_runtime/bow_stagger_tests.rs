use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::ids::SubrecordSig;
use crate::record::FieldEntry;
use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
use crate::schema::AuthoringSchema;
use crate::target_write::{add_record_native, replace_record_native};
use crate::translator::Game;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_load_no_py,
    plugin_handle_new_no_py,
};

fn entry(sig: [u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(sig),
        value,
    }
}

#[test]
fn source_stagger_readers_preserve_native_units_and_finite_signed_values() {
    let interner = StringInterner::new();
    let key = FormKey {
        local: 0x800,
        plugin: interner.intern("Skyrim.esm"),
    };
    let mut armor = Record::new(SigCode(*b"ARMO"), key);
    armor.fields.push(entry(*b"DNAM", FieldValue::Int(2000)));
    assert_eq!(armor_rating(&armor, &interner).unwrap(), 20.0);
    armor.fields[0].value = FieldValue::Int(0x10000 + 2000);
    assert_eq!(armor_rating(&armor, &interner).unwrap(), 20.0);
    let mut race = Record::new(SigCode(*b"RACE"), key);
    let mut data = vec![0; 164];
    data[52..56].copy_from_slice(&3.5f32.to_le_bytes());
    race.fields
        .push(entry(*b"DATA", FieldValue::Bytes(data.into())));
    assert_eq!(race_mass(&race, &interner).unwrap(), 3.5);
    let mut weapon = Record::new(SigCode(*b"WEAP"), key);
    weapon.fields.push(entry(
        *b"DNAM",
        FieldValue::Struct(vec![
            (interner.intern("AnimationType"), FieldValue::Uint(9)),
            (interner.intern("Stagger"), FieldValue::Float(0.75)),
        ]),
    ));
    assert_eq!(weapon_stagger(&weapon, &interner).unwrap(), (9, 0.75));
    assert_eq!(
        scalar_float(Some(&FieldValue::Float(-0.005)), "shield multiplier").unwrap(),
        -0.005
    );
    assert!(scalar_float(Some(&FieldValue::Float(f32::NAN)), "shield multiplier").is_err());
    assert!(resolve_weapon_template(weapon.clone(), |_| Ok(weapon.clone())).is_ok());
    weapon
        .fields
        .push(entry(*b"CNAM", FieldValue::FormKey(key)));
    assert!(
        resolve_weapon_template(weapon.clone(), |_| Ok(weapon.clone()))
            .unwrap_err()
            .contains("cyclic")
    );
}

#[test]
fn bow_stagger_publication_uses_source_values_and_emitted_target_boundaries() {
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
    with_run(id, |run| -> Result<(), RunError> {
        let interner = &run.interner;
        let key = |local, plugin: &str| FormKey { local, plugin: interner.intern(plugin) };
        let source = |local| key(local, "Skyrim.esm");
        let target = |local| key(local, "Converted.esm");
        let mut mapper = MapperState::new([], MapperOptions {
            output_plugin_name: "Converted.esm".into(), source_plugin_name: "Skyrim.esm".into(),
            target_master_names: vec!["Fallout4.esm".into()], ..Default::default()
        });
        let mut weapon = Record::new(SigCode(*b"WEAP"), source(0x800));
        let mut data = vec![0; 100];
        data[0] = 9;
        data[96..100].copy_from_slice(&0.75f32.to_le_bytes());
        weapon.fields.push(entry(*b"DNAM", FieldValue::Bytes(data.into())));
        add_record_native(source_handle, weapon, &run.schema_source, interner).unwrap();
        let mut enchanted = Record::new(SigCode(*b"WEAP"), source(0x801));
        enchanted.fields.push(entry(*b"CNAM", FieldValue::FormKey(source(0x800))));
        add_record_native(source_handle, enchanted, &run.schema_source, interner).unwrap();
        add_record_native(target_handle, Record::new(SigCode(*b"WEAP"), target(0x901)), &run.schema_target, interner).unwrap();
        mapper.source_to_target.insert(source(0x801), target(0x901));
        // Donor replacements never become source-owned native combat catalog entries.
        mapper.source_to_target.insert(source(0x800), key(0x123, "Fallout4.esm"));
        let mut armor = Record::new(SigCode(*b"ARMO"), source(0x802));
        armor.fields.push(entry(*b"DNAM", FieldValue::Int(2000)));
        add_record_native(source_handle, armor, &run.schema_source, interner).unwrap();
        add_record_native(target_handle, Record::new(SigCode(*b"ARMO"), target(0x902)), &run.schema_target, interner).unwrap();
        mapper.source_to_target.insert(source(0x802), target(0x902));
        let mut setting = Record::new(SigCode(*b"GMST"), source(0x803));
        setting.eid = Some(interner.intern("fStaggerBlockAttackShieldMult"));
        setting.fields.push(entry(*b"EDID", FieldValue::String(setting.eid.unwrap())));
        setting.fields.push(entry(*b"DATA", FieldValue::Float(-0.007)));
        add_record_native(source_handle, setting.clone(), &run.schema_source, interner).unwrap();
        run.mapper_state = Some(mapper);
        let output = serde_json::to_value(prepare_bow_stagger_output(run).unwrap()).unwrap();
        assert_eq!(output["weapons"], serde_json::json!([{"weapon":"000901:Converted.esm", "source_weapon":"000801:Skyrim.esm", "animation_type":9, "stagger":0.75}]));
        assert_eq!(output["armor"], serde_json::json!([{"armor":"000902:Converted.esm", "source_armor":"000802:Skyrim.esm", "armor_rating":20.0}]));
        assert_eq!(output["races"], serde_json::json!([]));
        assert_eq!(output["settings"]["fStaggerMassOffsetBase"], 3.0);
        assert_eq!(output["settings"]["fStaggerBlockAttackShieldMult"].as_f64().unwrap() as f32, -0.007);
        assert_eq!(output["settings"].as_object().unwrap().len(), 13);
        assert_eq!(output, serde_json::to_value(prepare_bow_stagger_output(run).unwrap()).unwrap());
        setting.fields[1].value = FieldValue::Float(f32::NAN);
        replace_record_native(source_handle, setting, &run.schema_source, interner).unwrap();
        assert!(prepare_bow_stagger_output(run).err().unwrap().contains("not finite"));
        Ok(())
    }).unwrap();
    drop_run(id).unwrap();
    assert!(plugin_handle_close_native(source_handle));
    assert!(plugin_handle_close_native(target_handle));
}

#[test]
#[ignore = "requires MODKIT_SKYRIM_DATA pointing to the installed official source plugins"]
fn actual_source_bow_stagger_values_match_native_layout_and_official_corpus() {
    let directory = std::path::PathBuf::from(std::env::var("MODKIT_SKYRIM_DATA").unwrap());
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let mut summary = Vec::new();
    for (plugin, expected, expected_nonzero) in [
        ("Skyrim.esm", 257, 0),
        ("Dawnguard.esm", 51, 36),
        ("Dragonborn.esm", 69, 0),
    ] {
        let handle = plugin_handle_load_no_py(
            directory.join(plugin).to_str().unwrap(),
            Some("skyrimse"),
            None,
            None,
            true,
        )
        .unwrap();
        let mut count = 0;
        let mut nonzero = 0;
        for key in iter_form_keys_of_sig(handle, SigCode(*b"WEAP"), &interner).unwrap() {
            let record =
                read_record_relayout_by_form_key(handle, &key, &schema, &interner, None).unwrap();
            if record.flags.contains(RecordFlags::DELETED) {
                continue;
            }
            if field(&record, b"DNAM").is_none() {
                continue;
            }
            let (kind, stagger) = weapon_stagger(&record, &interner).unwrap();
            if matches!(kind, 7 | 9) {
                count += 1;
                if stagger != 0.0 {
                    nonzero += 1;
                    assert_eq!((plugin, kind, stagger), ("Dawnguard.esm", 9, 0.75));
                }
            }
        }
        assert_eq!((count, nonzero), (expected, expected_nonzero), "{plugin}");
        if plugin == "Skyrim.esm" {
            let read = |local| {
                read_record_relayout_by_form_key(
                    handle,
                    &FormKey {
                        local,
                        plugin: interner.intern(plugin),
                    },
                    &schema,
                    &interner,
                    None,
                )
                .unwrap()
            };
            assert_eq!(race_mass(&read(0x13746), &interner).unwrap(), 1.0);
            assert_eq!(armor_rating(&read(0x12EB6), &interner).unwrap(), 20.0);
        }
        summary.push(serde_json::json!({"plugin":plugin,"projectile_weapon_count":count,"nonzero_stagger_count":nonzero}));
        assert!(plugin_handle_close_native(handle));
    }
    eprintln!("{}", serde_json::to_string(&summary).unwrap());
}

#[test]
fn bow_stagger_races_require_source_owned_committed_attack_events() {
    use crate::run::{SkyrimCreatureReservationOwner, SkyrimCreatureReservationSource};
    let source_handle = plugin_handle_new_no_py("Skyrim.esm", Some("skyrimse"));
    let target_handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
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
    with_run(id, |run| -> Result<(), RunError> {
        let source_key = FormKey {
            local: 0x1000,
            plugin: run.interner.intern("Skyrim.esm"),
        };
        let mut source = Record::new(SigCode(*b"RACE"), source_key);
        let mut data = vec![0; 164];
        data[52..56].copy_from_slice(&3.0f32.to_le_bytes());
        source
            .fields
            .push(entry(*b"DATA", FieldValue::Bytes(data.into())));
        add_record_native(source_handle, source, &run.schema_source, &run.interner).unwrap();
        run.mapper_state = Some(MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: "Skyrim.esm".into(),
                ..Default::default()
            },
        ));
        run.reserve_skyrim_creature_records(
            vec![SkyrimCreatureReservationSource {
                source: source_key,
                kind: SkyrimCreatureReservedRecordKind::Race,
                owners: vec![SkyrimCreatureReservationOwner {
                    source_race: source_key,
                    motion_family_id: "wolf".into(),
                    normalized_project_path: "actors/wolf/wolfproject.hkx".into(),
                }],
            }],
            1,
        )?;
        let target_key = run.skyrim_creature_record_reservations()[0].target;
        let mut target = Record::new(SigCode(*b"RACE"), target_key);
        target.fields.push(entry(
            *b"ATKE",
            FieldValue::String(run.interner.intern("ConvertedBite")),
        ));
        add_record_native(target_handle, target, &run.schema_target, &run.interner).unwrap();
        run.skyrim_bow_stagger_races.push(ProjectedRaceStagger {
            race: TargetFormKey::new(target_key.local, "Converted.esm"),
            attacks: vec![ProjectedAttackStagger {
                event: "ConvertedBite".into(),
                source_event: "attackStartBite".into(),
                stagger: 0.65,
            }],
        });
        let output = prepare_bow_stagger_output(run).unwrap();
        assert_eq!(output.races.len(), 1);
        assert_eq!(output.races[0].source_race, "001000:Skyrim.esm");
        assert_eq!(output.races[0].mass, 3.0);
        assert_eq!(output.races[0].attacks[0].event, "ConvertedBite");
        assert_eq!(output.races[0].attacks[0].source_event, "attackStartBite");
        assert_eq!(
            output.races[0].attacks[0].stagger.to_bits(),
            0.65f32.to_bits()
        );
        run.skyrim_bow_stagger_races[0].attacks[0].event = "AbsentTargetAttack".into();
        assert!(
            prepare_bow_stagger_output(run)
                .err()
                .unwrap()
                .contains("not preserved")
        );
        run.skyrim_bow_stagger_races[0].race.plugin = "Fallout4.esm".into();
        assert!(
            prepare_bow_stagger_output(run)
                .err()
                .unwrap()
                .contains("donor plugin")
        );
        Ok(())
    })
    .unwrap();
    drop_run(id).unwrap();
    assert!(plugin_handle_close_native(source_handle));
    assert!(plugin_handle_close_native(target_handle));
}
