use super::*;
use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_no_py,
};

fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value,
    }
}

#[test]
fn legacy_actor_stats_catalog_preserves_selected_template_stats_and_marker() {
    let source_handle = plugin_handle_new_no_py("FalloutNV.esm", Some("fnv"));
    let target_handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
    plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
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
    with_run(id, |run| -> Result<(), RunError> {
        let source_plugin = run.interner.intern("FalloutNV.esm");
        let target_plugin = run.interner.intern("Converted.esm");
        let source = |local| FormKey {
            local,
            plugin: source_plugin,
        };
        let target = |local| FormKey {
            local,
            plugin: target_plugin,
        };
        for (local, health, level, agility) in [
            (0x800, 999_u16, 1_u16, 1_u8),
            (0x801, 40, 1500, 7),
            (0x802, 125, 12, 3),
        ] {
            let mut record = Record::new(SigCode(*b"CREA"), source(local));
            record.eid = Some(run.interner.intern(&format!("Source{local:X}")));
            let mut acbs = vec![0; 24];
            acbs[..4].copy_from_slice(&(if local == 0x801 { 0x80_u32 } else { 0 }).to_le_bytes());
            acbs[8..10].copy_from_slice(&level.to_le_bytes());
            acbs[10..12].copy_from_slice(&2_u16.to_le_bytes());
            acbs[12..14].copy_from_slice(&30_u16.to_le_bytes());
            if local == 0x800 {
                acbs[18..20].copy_from_slice(&2_u16.to_le_bytes());
            }
            let mut data = vec![0; 17];
            data[4..6].copy_from_slice(&health.to_le_bytes());
            data[10..17].copy_from_slice(&[5, 6, 7, 8, 9, agility, 4]);
            record.fields.extend([
                field(b"ACBS", FieldValue::Bytes(acbs.into())),
                field(b"DATA", FieldValue::Bytes(data.into())),
            ]);
            if local == 0x800 {
                record
                    .fields
                    .push(field(b"TPLT", FieldValue::FormKey(source(0x900))));
            }
            crate::target_write::add_record_native(
                source_handle,
                record,
                &run.schema_source,
                &run.interner,
            )
            .unwrap();
        }
        let mut list = Record::new(SigCode(*b"LVLC"), source(0x900));
        list.eid = Some(run.interner.intern("SourceVariants"));
        for (level, key) in [(1_u16, source(0x801)), (10, source(0x802))] {
            let mut bytes = vec![0; 12];
            bytes[..2].copy_from_slice(&level.to_le_bytes());
            bytes[4..8].copy_from_slice(&key.local.to_le_bytes());
            bytes[8..10].copy_from_slice(&1_u16.to_le_bytes());
            bytes[10] = 8;
            list.fields
                .push(field(b"LVLO", FieldValue::Bytes(bytes.into())));
        }
        crate::target_write::add_record_native(
            source_handle,
            list,
            &run.schema_source,
            &run.interner,
        )
        .unwrap();
        for (local, name, value) in [
            (0xA00, "fAVDActionPointsBase", 65.0),
            (0xA01, "fAVDActionPointsMult", 3.0),
        ] {
            let mut setting = Record::new(SigCode(*b"GMST"), source(local));
            setting.eid = Some(run.interner.intern(name));
            setting
                .fields
                .push(field(b"DATA", FieldValue::Float(value)));
            crate::target_write::add_record_native(
                source_handle,
                setting,
                &run.schema_source,
                &run.interner,
            )
            .unwrap();
        }
        for local in [0x2000, 0x2001] {
            let mut race = Record::new(SigCode(*b"RACE"), target(local));
            race.eid = Some(run.interner.intern(&format!("TargetRace{local:X}")));
            crate::target_write::add_record_native(
                target_handle,
                race,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();
        }
        for (local, race) in [(0x2100, 0x2000), (0x2101, 0x2000), (0x2102, 0x2001)] {
            let mut npc = Record::new(SigCode(*b"NPC_"), target(local));
            npc.eid = Some(run.interner.intern(&format!("TargetNpc{local:X}")));
            npc.fields
                .push(field(b"RNAM", FieldValue::FormKey(target(race))));
            if local == 0x2100 {
                npc.fields
                    .push(field(b"TPLT", FieldValue::FormKey(target(0x2200))));
            }
            crate::target_write::add_record_native(
                target_handle,
                npc,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();
        }
        let mut list = Record::new(SigCode(*b"LVLN"), target(0x2200));
        list.eid = Some(run.interner.intern("TargetVariants"));
        for (level, key) in [(1, target(0x2101)), (10, target(0x2102))] {
            list.fields.push(field(
                b"LVLO",
                FieldValue::Struct(vec![
                    (run.interner.intern("level"), FieldValue::Uint(level)),
                    (run.interner.intern("unknown_u8_1"), FieldValue::Uint(0)),
                    (run.interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
                    (run.interner.intern("npc"), FieldValue::FormKey(key)),
                    (run.interner.intern("count"), FieldValue::Uint(1)),
                    (run.interner.intern("chance_none"), FieldValue::Uint(0)),
                    (run.interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
                ]),
            ));
        }
        crate::target_write::add_record_native(
            target_handle,
            list,
            &run.schema_target,
            &run.interner,
        )
        .unwrap();
        run.init_mapper_state().unwrap();
        run.mapper_state
            .as_mut()
            .unwrap()
            .source_to_target
            .insert(source(0x800), target(0x2100));
        let output = prepare(run).map_err(RunError::InvalidConfig)?.unwrap();
        assert_eq!(output.actors.len(), 2);
        assert_eq!(output.actors[0].npc, "002101:Converted.esm");
        assert_eq!(output.actors[0].race, "002000:Converted.esm");
        assert_eq!(output.actors[0].health, 40);
        assert_eq!(output.actors[0].level, 1500);
        assert!(output.actors[0].pc_level_mult);
        assert_eq!(output.actors[0].special, [5, 6, 7, 8, 9, 7, 4]);
        assert_eq!(output.actors[1].health, 125);
        assert_eq!(output.actors[1].race, "002001:Converted.esm");
        assert!(!output.actors[1].pc_level_mult);
        assert_eq!(output.actors[1].ap_mult, 3.0);
        let repeated = prepare(run).map_err(RunError::InvalidConfig)?.unwrap();
        assert_eq!(repeated.actor_level, output.actor_level);
        assert_eq!(repeated.actors, output.actors);
        let marker = FormKey::parse(&output.actor_level.replace(':', "@"), &run.interner).unwrap();
        let marker = emitted(run, marker).unwrap();
        assert_eq!(marker.sig.0, *b"AVIF");
        Ok(())
    })
    .unwrap();
    drop_run(id);
    assert!(plugin_handle_close_native(source_handle));
    assert!(plugin_handle_close_native(target_handle));
}
