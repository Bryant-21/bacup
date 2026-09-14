use super::*;
use crate::formkey_mapper::MapperOptions;
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::{plugin_handle_close_native, plugin_handle_new_native};

fn fk(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        plugin: interner.intern("SeventySix.esm"),
        local,
    }
}

fn object(local: u32) -> Value {
    json!({"Alias": -1, "FormID": {"reference": {
        "plugin": "SeventySix.esm", "object_id": format!("{local:06X}")
    }}})
}

fn row(keyword: u32, spell: u32, wait_time: Option<f64>) -> Value {
    let mut members = vec![
        json!({"memberName": "FurnitureTypeKeyword", "Type": "Object", "Flags": 1, "Value": object(keyword)}),
        json!({"memberName": "SpellToCast", "Type": "Object", "Flags": 1, "Value": object(spell)}),
    ];
    if let Some(wait_time) = wait_time {
        members.push(
            json!({"memberName": "WaitTime", "Type": "Float", "Flags": 1, "Value": wait_time}),
        );
    }
    json!(members)
}

fn player_payload(rows: Vec<Value>) -> Value {
    json!({"Version": 6, "Object Format": 2, "Scripts": [{
        "ScriptName": SOURCE_SCRIPT, "Properties": [{
            "propertyName": "FurnitureTypeData", "Type": "Array of Struct", "Flags": 1, "Value": rows
        }]
    }]})
}

#[test]
fn source_table_preserves_delays_and_omits_bed_and_companion_logic() {
    let interner = StringInterner::new();
    let mut bed = row(0x3CD037, 0x05C528, None);
    bed.as_array_mut().unwrap().push(
        json!({"memberName": "SpellToCast_InfatuatedAllyPresent", "Value": object(0x59E3F0)}),
    );
    let mut companion = row(0x3CD038, 0x05C528, None);
    companion
        .as_array_mut()
        .unwrap()
        .push(json!({"memberName": "SpellToCast_RomanticAllyPresent", "Value": object(0x59E3F1)}));
    let mut disease = row(0x3CD038, 0x05C528, Some(5.0));
    disease
        .as_array_mut()
        .unwrap()
        .push(json!({"memberName": "IsDiseaseRisk", "Value": true}));
    let mut strength = row(0x5B359F, 0x5B519D, Some(15.0));
    strength.as_array_mut().unwrap().push(json!({"memberName": "AbilityToAddWhileInFurniture", "Value": {"Alias": -1, "FormID": null}}));
    let payload = player_payload(vec![
        row(0x5EDEE0, 0x5EDEE5, Some(15.0)),
        row(0x50CD11, 0x50CD15, None),
        bed,
        companion,
        disease,
        strength,
        row(0x65015B, 0x650155, Some(-1.0)),
    ]);
    let specs = source_buff_specs(&payload, &interner);
    assert_eq!(
        specs
            .iter()
            .map(|spec| (spec.keyword.local, spec.spell.local, spec.wait_time))
            .collect::<Vec<_>>(),
        vec![
            (0x5EDEE0, 0x5EDEE5, 15.0),
            (0x50CD11, 0x50CD15, 30.0),
            (0x5B359F, 0x5B519D, 15.0)
        ]
    );
}

#[test]
fn session_roundtrip_binds_cosmic_capture_and_dual_buffs_without_replacing_scripts() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
    let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
    let payload = player_payload(vec![
        row(0x5EDEE0, 0x5EDEE5, Some(15.0)),
        row(0x65015D, 0x650156, Some(15.0)),
        row(0x65015B, 0x650155, Some(15.0)),
    ]);
    let mut player = Record::new(SigCode(*b"NPC_"), fk(7, &interner));
    player.fields.push(FieldEntry {
        sig: SubrecordSig(*b"VMAD"),
        value: FieldValue::Bytes(
            build_vmad_bytes_from_payload(&payload, &[], "SeventySix.esm")
                .unwrap()
                .into(),
        ),
    });
    {
        let mut session = open_session(source, None).unwrap();
        let schema = session.schema().unwrap();
        session.add_record(player, &schema, &interner).unwrap();
    }
    let mut session = open_session(target, Some(source)).unwrap();
    session
        .target_slot_mut()
        .parsed
        .header
        .masters
        .push("Fallout4.esm".into());
    let schema = session.schema().unwrap();
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            source_plugin_name: "SeventySix.esm".into(),
            ..MapperOptions::default()
        },
        &interner,
    );
    for (sig, source_local, target_local) in [
        (b"KYWD", 0x5EDEE0, 0x900001),
        (b"SPEL", 0x5EDEE5, 0x900002),
        (b"KYWD", 0x65015D, 0x65015D),
        (b"SPEL", 0x650156, 0x650156),
        (b"KYWD", 0x65015B, 0x65015B),
    ] {
        mapper.add_mapping(fk(source_local, &interner), fk(target_local, &interner));
        session
            .add_record(
                Record::new(SigCode(*sig), fk(target_local, &interner)),
                &schema,
                &interner,
            )
            .unwrap();
    }
    mapper.add_mapping(fk(0x650155, &interner), fk(0x650155, &interner));
    let existing = build_vmad_bytes_from_payload(
        &json!({
            "Version": 6, "Object Format": 2,
            "Scripts": [{"ScriptName": "B21MusicInstrumentScript", "Properties": []}]
        }),
        session.target_masters(),
        "SeventySix.esm",
    )
    .unwrap();
    for (local, keywords) in [
        (0x7CF731, vec![0x01900001u32]),
        (0x76A6C0, vec![0x01900001, 0x0165015D]),
        (0x900003, vec![0x00900001]),
        (0x900004, vec![0x0165015B]),
    ] {
        let mut furniture = Record::new(SigCode(*b"FURN"), fk(local, &interner));
        furniture.fields.push(FieldEntry {
            sig: SubrecordSig(*b"KSIZ"),
            value: FieldValue::Uint(keywords.len() as u64),
        });
        furniture.fields.push(FieldEntry {
            sig: SubrecordSig(*b"KWDA"),
            value: FieldValue::Bytes(
                keywords
                    .iter()
                    .flat_map(|raw| raw.to_le_bytes())
                    .collect::<Vec<_>>()
                    .into(),
            ),
        });
        furniture.fields.push(FieldEntry {
            sig: SubrecordSig(*b"VMAD"),
            value: FieldValue::Bytes(existing.clone().into()),
        });
        session.add_record(furniture, &schema, &interner).unwrap();
    }
    assert_eq!(
        session
            .first_subrecord_bytes(&fk(0x7CF731, &interner), "KWDA")
            .unwrap(),
        Some(0x01900001u32.to_le_bytes().to_vec())
    );
    let fixup = AttachFo76FurnitureBuffsFixup;
    let source_schema = session.source_schema().unwrap();
    let player_decoded = session
        .source_record_decoded(&fk(7, &interner), &source_schema, &interner)
        .unwrap();
    assert!(
        matches!(field(&player_decoded, b"VMAD"), Some(FieldValue::Bytes(_))),
        "{player_decoded:?}"
    );
    assert!(fixup.applies_to_session(&session, &FixupConfig::default()));
    let report = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(
        report.records_changed,
        2,
        "warnings={:?}, diagnostics={:?}",
        report
            .warnings
            .iter()
            .map(|s| interner.resolve(*s))
            .collect::<Vec<_>>(),
        report
            .diagnostics
            .iter()
            .map(|s| interner.resolve(*s))
            .collect::<Vec<_>>()
    );
    assert_eq!(report.warnings.len(), 1);
    for (local, count) in [(0x7CF731, 1), (0x76A6C0, 2)] {
        let bytes = session
            .first_subrecord_bytes(&fk(local, &interner), "VMAD")
            .unwrap()
            .unwrap();
        let decoded = compact_vmad_payload_json(
            &bytes,
            session.target_masters(),
            "SeventySix.esm",
            Some("FURN"),
        )
        .unwrap();
        let scripts = decoded["Scripts"].as_array().unwrap();
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0]["ScriptName"], "B21MusicInstrumentScript");
        assert_eq!(scripts[1]["ScriptName"], SCRIPT_NAME);
        let buffs = &scripts[1]["Properties"][0]["Value"];
        assert_eq!(buffs.as_array().unwrap().len(), count);
        assert_eq!(
            object_form(
                named_value(&buffs[0], "memberName", "BuffSpell").unwrap(),
                &interner
            ),
            Some(fk(0x900002, &interner))
        );
        assert_eq!(
            named_value(&buffs[0], "memberName", "WaitTime"),
            Some(&json!(15.0))
        );
    }
    assert_eq!(
        fixup
            .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
            .unwrap()
            .records_changed,
        0
    );
    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(source));
}

#[test]
fn attachment_creates_missing_vmad_and_refuses_conflicting_bindings() {
    let interner = StringInterner::new();
    let spec = BuffSpec {
        keyword: fk(0x5EDEE0, &interner),
        spell: fk(0x5EDEE5, &interner),
        wait_time: 15.0,
    };
    let vmad = buff_vmad(&[spec.clone()], &[], "SeventySix.esm", &interner).unwrap();
    let mut record = Record::new(SigCode(*b"FURN"), fk(0x7CF731, &interner));
    assert_eq!(
        attach_buff_script(&mut record, &vmad),
        AttachResult::Changed
    );
    assert_eq!(
        attach_buff_script(&mut record, &vmad),
        AttachResult::AlreadyPresent
    );
    let original = record.clone();
    let changed = buff_vmad(
        &[BuffSpec {
            wait_time: 30.0,
            ..spec
        }],
        &[],
        "SeventySix.esm",
        &interner,
    )
    .unwrap();
    assert_eq!(
        attach_buff_script(&mut record, &changed),
        AttachResult::Conflict("same_script_different_binding")
    );
    assert_eq!(field(&record, b"VMAD"), field(&original, b"VMAD"));
}
