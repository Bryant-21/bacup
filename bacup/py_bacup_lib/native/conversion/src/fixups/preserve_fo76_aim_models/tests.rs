use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::session::open_session;
use crate::sym::StringInterner;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
};

fn aim_data(seed: f32) -> Vec<u8> {
    (0..16)
        .flat_map(|index| {
            if matches!(index, 4 | 11) {
                (index as u32 + 1).to_le_bytes()
            } else {
                (seed + index as f32 * 0.25).to_le_bytes()
            }
        })
        .collect()
}

fn aim_model(form_key: FormKey, data: &[u8], interner: &StringInterner) -> Record {
    let mut record = Record::new(SigCode(*b"AMDL"), form_key);
    let eid = interner.intern(&format!("B21_TestAim{:06X}", form_key.local));
    record.eid = Some(eid);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"EDID"),
        value: FieldValue::String(eid),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"DNAM"),
        value: FieldValue::Bytes(data.to_vec().into()),
    });
    record
}

#[test]
fn shared_aim_model_keeps_all_source_values_and_new_models_stay_intact() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native("Source.esm", Some("fo76")).unwrap();
    let target = plugin_handle_new_native("Output.esm", Some("fo4")).unwrap();
    plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
    let source_key = |local| FormKey {
        local,
        plugin: interner.intern("Source.esm"),
    };
    let shared = FormKey {
        local: 0x800,
        plugin: interner.intern("Fallout4.esm"),
    };
    let new = FormKey {
        local: 0x801,
        plugin: interner.intern("Output.esm"),
    };
    let source_data = aim_data(1.25);
    let new_data = aim_data(2.5);
    {
        let mut session = open_session(source, None).unwrap();
        let schema = session.schema().unwrap();
        for (local, data) in [
            (0x800, &source_data),
            (0x801, &new_data),
            (0x802, &source_data),
            (0x803, &source_data),
        ] {
            let mut record = aim_model(source_key(local), data, &interner);
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"AMMR"),
                value: FieldValue::Bytes(0f32.to_le_bytes().to_vec().into()),
            });
            if local == 0x803 {
                record.flags |= RecordFlags::DELETED;
            }
            session.add_record(record, &schema, &interner).unwrap();
        }
    }
    let mut session = open_session(target, Some(source)).unwrap();
    let schema = session.schema().unwrap();
    session
        .add_record(aim_model(new, &new_data, &interner), &schema, &interner)
        .unwrap();
    let mut state = MapperState::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: "Output.esm".into(),
            ..Default::default()
        },
    );
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    mapper.add_mapping(source_key(0x800), shared);
    mapper.add_mapping(source_key(0x801), new);
    mapper.add_mapping(
        source_key(0x803),
        FormKey {
            local: 0x803,
            plugin: shared.plugin,
        },
    );

    let report = PreserveFo76AimModelsFixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(report.records_added, 1);
    let read_data = |session: &mut PluginSession, raw| {
        session
            .handle_raw_scan(target)
            .unwrap()
            .with_record_subrecords(raw, |fields| {
                assert!(fields
                    .iter()
                    .all(|field| matches!(field.signature.as_str(), "EDID" | "DNAM")));
                fields
                    .iter()
                    .find(|field| field.signature.as_str() == "DNAM")
                    .unwrap()
                    .data
                    .clone()
            })
            .unwrap()
    };
    assert_eq!(read_data(&mut session, 0x800), source_data);
    assert_eq!(read_data(&mut session, 0x01000801), new_data);
    assert_eq!(
        session
            .form_keys_of_sig(SigCode(*b"AMDL"), &interner)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(mapper.lookup(source_key(0x800)), Some(shared));
    assert!(PreserveFo76AimModelsFixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap()
        .is_no_op());

    session
        .replace_records_contents(
            vec![aim_model(shared, &aim_data(8.0), &interner)],
            &schema,
            &interner,
        )
        .unwrap();
    let report = PreserveFo76AimModelsFixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(report.records_added, 0);
    assert_eq!(report.records_changed, 1);
    assert_eq!(read_data(&mut session, 0x800), source_data);
    drop(session);
    assert!(plugin_handle_close_native(source));
    assert!(plugin_handle_close_native(target));
}

#[test]
fn aim_model_preservation_is_only_for_fo76_to_fo4() {
    for (source_game, target_game, expected) in [
        ("fo76", "fo4", true),
        ("fo4", "fo4", false),
        ("fo76", "fo76", false),
    ] {
        let source = plugin_handle_new_native("Source.esm", Some(source_game)).unwrap();
        let target = plugin_handle_new_native("Output.esm", Some(target_game)).unwrap();
        let session = open_session(target, Some(source)).unwrap();
        assert_eq!(
            PreserveFo76AimModelsFixup.applies_to_session(&session, &FixupConfig::default()),
            expected
        );
        drop(session);
        assert!(plugin_handle_close_native(source));
        assert!(plugin_handle_close_native(target));
    }
}
