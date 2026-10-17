use super::*;

#[test]
fn skyrim_script_properties_remap_forward_master_and_dlc_visual_effects() {
    use serde_json::json;
    let vmad = esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload(&json!({
        "Version": 5, "Object Format": 2,
        "Scripts": [{"ScriptName": "magicAbsorbFXScript", "Flags": 0, "Properties": [
            {"propertyName": "TargetVFX", "Type": "Object", "Flags": 1, "Value": {"Alias": -1, "FormID": {"reference": {"plugin": "Skyrim.esm", "object_id": "001234"}}}},
            {"propertyName": "CasterVFX", "Type": "Object", "Flags": 1, "Value": {"Alias": -1, "FormID": {"reference": {"plugin": "Dawnguard.esm", "object_id": "001234"}}}}
        ]}]
    }), &["Skyrim.esm".to_string()], "Dawnguard.esm").unwrap();
    let mut data = vec![0_u8; 152];
    for (offset, value) in [
        (12, 20_u32),
        (16, u32::MAX),
        (64, 1),
        (68, u32::MAX),
        (80, 1),
        (84, 1),
        (88, u32::MAX),
    ] {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let mut effect = subrecord(b"EDID", b"B21_ForwardAbsorbScript\0");
    effect.extend(subrecord(b"VMAD", &vmad));
    effect.extend(subrecord(b"DATA", &data));
    let fixture = plugin(
        &["Skyrim.esm"],
        &[
            group(b"MGEF", 0, &[record(b"MGEF", 0x0100_2000, 0, &effect)]),
            group(
                b"RFCT",
                0,
                &[
                    record(
                        b"RFCT",
                        0x0000_1234,
                        0,
                        &subrecord(b"EDID", b"B21_MasterVisualEffect\0"),
                    ),
                    record(
                        b"RFCT",
                        0x0100_1234,
                        0,
                        &subrecord(b"EDID", b"B21_DlcVisualEffect\0"),
                    ),
                ],
            ),
        ],
    );
    for use_v2 in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("Dawnguard.esm");
        std::fs::write(&source_path, &fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "skyrimse");
        let target_handle = plugin_handle_new_native("Converted.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Converted.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                preserve_source_ids: true,
                is_whole_plugin: false,
                skip_record_signatures: vec!["RFCT".into()],
                ..Default::default()
            },
        })
        .unwrap();
        let stats = with_run(id, |run| {
            run.install_creature_dependency_admissions(
                ["Skyrim.esm", "Dawnguard.esm"]
                    .into_iter()
                    .map(|plugin| CreatureDependencyAdmission {
                        source_form_key: FormKey {
                            plugin: run.interner.intern(plugin),
                            local: 0x1234,
                        },
                        source_signature: SigCode(*b"RFCT"),
                        owner_family_ids: vec!["vampire".into()],
                        legacy_idle: None,
                    })
                    .collect(),
            )?;
            if use_v2 {
                run.translate_all_v2(&source_path)
            } else {
                run.translate_all()
            }
        })
        .unwrap();
        assert_eq!(stats.records_failed, 0, "v2={use_v2}: {stats:?}");
        let records = handle_records(target_handle);
        assert_eq!(records.len(), 3, "v2={use_v2}: {stats:?}");
        let effect = record_with_edid(&records, "B21_ForwardAbsorbScript");
        let master_visual = record_with_edid(&records, "B21_MasterVisualEffect");
        let dlc_visual = record_with_edid(&records, "B21_DlcVisualEffect");
        assert_ne!(master_visual.form_id, dlc_visual.form_id);
        let output_vmad = subrecord_data(effect, "VMAD");
        let decoded = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(output_vmad, &["Fallout4.esm".into()], "Converted.esm", Some("MGEF")).unwrap();
        let script = &decoded["Scripts"][0];
        assert_eq!(script["ScriptName"], "B21_SkyrimAbsorbFXEffect");
        for (index, expected) in [(0, master_visual.form_id), (1, dlc_visual.form_id)] {
            let reference = &script["Properties"][index]["Value"]["FormID"]["reference"];
            assert_eq!(
                reference["plugin"], "Converted.esm",
                "v2={use_v2}: {decoded}"
            );
            assert_eq!(
                reference["object_id"],
                format!("{:06X}", expected & 0x00FF_FFFF),
                "v2={use_v2}: {decoded}"
            );
        }
        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }
}
