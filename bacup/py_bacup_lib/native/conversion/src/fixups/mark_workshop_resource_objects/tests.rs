use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use esp_authoring_core::plugin_runtime::plugin_handle_close_native;
use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
use serde_json::Value;

use super::*;
use crate::formkey_mapper::MapperOptions;
use crate::session::open_session;
use crate::sym::StringInterner;

const EXTERIOR: u32 = 0x05A0C8;
const POWER: u32 = 0x05A0CA;
const SETTLEMENT: u32 = 0x246F85;
const DECORATIONS: u32 = 0x08280B;
const CHAIR_KEYWORD: u32 = 0x030BA3;
const GUARD_KEYWORD: u32 = 0x069548;
const POWER_GENERATED: u32 = 0x00032E;
const POWER_REQUIRED: u32 = 0x000330;
const WATER: u32 = 0x000332;
const SAFETY: u32 = 0x000333;

fn fk(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        plugin: interner.intern("SeventySix.esm"),
        local,
    }
}

fn fo4(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        plugin: interner.intern("Fallout4.esm"),
        local,
    }
}

fn entry(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value,
    }
}

fn recipe(local: u32, workbench: u32, created: FormKey, interner: &StringInterner) -> Record {
    let mut record = Record::new(SigCode(*b"COBJ"), fk(local, interner));
    record.fields.push(entry(b"CNAM", FieldValue::FormKey(created)));
    record
        .fields
        .push(entry(b"BNAM", FieldValue::FormKey(fo4(workbench, interner))));
    record
}

fn object(
    sig: &[u8; 4],
    local: u32,
    keyword: Option<u32>,
    properties: &[(u32, f32)],
    vmad: Option<Vec<u8>>,
    interner: &StringInterner,
) -> Record {
    let mut record = Record::new(SigCode(*sig), fk(local, interner));
    if let Some(vmad) = vmad {
        record.fields.push(entry(b"VMAD", FieldValue::Bytes(vmad.into())));
    }
    if let Some(keyword) = keyword {
        record.fields.push(entry(b"KSIZ", FieldValue::Uint(1)));
        record.fields.push(entry(
            b"KWDA",
            FieldValue::Bytes(keyword.to_le_bytes().to_vec().into()),
        ));
    }
    // Fallout4.esm is master 0, so its raw form ids are its local ids.
    if !properties.is_empty() {
        let rows = properties
            .iter()
            .flat_map(|(local, value)| [local.to_le_bytes(), value.to_le_bytes()])
            .flatten()
            .collect::<Vec<_>>();
        record.fields.push(entry(b"PRPS", FieldValue::Bytes(rows.into())));
    }
    record
}

fn names(interner: &StringInterner) -> PropertyNames {
    PropertyNames {
        actor_value: interner.intern("properties_actor_value"),
        value: interner.intern("properties_value"),
    }
}

fn properties(record: &Record, interner: &StringInterner) -> Vec<(u32, f32)> {
    let names = names(interner);
    let mut rows: Vec<(u32, f32)> = match field(record, b"PRPS") {
        Some(FieldValue::Bytes(rows)) => rows
            .chunks_exact(8)
            .map(|row| {
                (
                    u32::from_le_bytes(row[..4].try_into().unwrap()),
                    f32::from_le_bytes(row[4..].try_into().unwrap()),
                )
            })
            .collect(),
        Some(FieldValue::List(rows)) => rows
            .iter()
            .filter_map(|row| {
                let FieldValue::Struct(fields) = row else {
                    return None;
                };
                let member = |name| fields.iter().find(|(field, _)| *field == name).map(|(_, v)| v);
                match (member(names.actor_value), member(names.value)) {
                    (Some(FieldValue::FormKey(av)), Some(FieldValue::Float(value))) => {
                        Some((av.local, *value))
                    }
                    _ => None,
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    rows.sort_by_key(|(av, _)| *av);
    rows
}

fn scripts(session: &mut PluginSession, local: u32, sig: &str, interner: &StringInterner) -> Value {
    let bytes = session
        .first_subrecord_bytes(&fk(local, interner), "VMAD")
        .unwrap()
        .unwrap();
    compact_vmad_payload_json(&bytes, session.target_masters(), "SeventySix.esm", Some(sig))
        .unwrap()["Scripts"]
        .clone()
}

fn assert_bound_to_workshop_parent(script: &Value) {
    assert_eq!(script["ScriptName"], SCRIPT_NAME);
    let properties = script["Properties"].as_array().unwrap();
    assert_eq!(properties.len(), 1, "{script}");
    assert_eq!(properties[0]["propertyName"], PARENT_PROPERTY);
    let reference = &properties[0]["Value"]["FormID"]["reference"];
    assert_eq!(reference["plugin"], "Fallout4.esm");
    assert_eq!(reference["object_id"], "02058E");
}

#[test]
fn buildable_beds_and_rated_objects_get_vanilla_workshop_data() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
    let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
    let mut session = open_session(target, Some(source)).unwrap();
    session
        .target_slot_mut()
        .parsed
        .header
        .masters
        .push("Fallout4.esm".into());
    let schema = session.schema().unwrap();
    let masters = session.target_masters().to_vec();
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            source_plugin_name: "SeventySix.esm".into(),
            ..MapperOptions::default()
        },
        &interner,
    );
    let vmad = |scripts: Value| {
        build_vmad_bytes_from_payload(
            &json!({"Version": 6, "Object Format": 2, "Scripts": scripts}),
            &masters,
            "SeventySix.esm",
        )
        .unwrap()
    };
    let unbound = vmad(json!([
        {"ScriptName": "B21MusicInstrumentScript", "Properties": []},
        {"ScriptName": SCRIPT_NAME, "Properties": []}
    ]));
    let collector = vmad(json!([{"ScriptName": "B21:WorkshopCollector", "Properties": []}]));
    let bound = workshop_object_vmad(&masters, "SeventySix.esm").unwrap();
    let furniture = FO4_WORKSHOP_WORKBENCH_FURNITURE;
    let sleep = Some(IS_SLEEP_FURNITURE);
    let own = |local| fk(local, &interner);
    let records = [
        recipe(0x900010, furniture, own(0x900001), &interner),
        recipe(0x900011, furniture, own(0x900002), &interner),
        recipe(0x900012, furniture, own(0x900003), &interner),
        recipe(0x900013, EXTERIOR, own(0x900004), &interner),
        recipe(0x900014, POWER, own(0x900006), &interner),
        recipe(0x900015, SETTLEMENT, own(0x900007), &interner),
        recipe(0x900016, EXTERIOR, own(0x900008), &interner),
        recipe(0x900017, EXTERIOR, own(0x900009), &interner),
        recipe(0x900018, DECORATIONS, own(0x90000A), &interner),
        // A vanilla bed: master records are never edited.
        recipe(0x900019, furniture, fo4(0x118F4B, &interner), &interner),
        // Plain bed: nothing FO4 counts.
        object(b"FURN", 0x900001, sleep, &[], None, &interner),
        // FO76 sleeping bag: bed rating 2, script left without WorkshopParent.
        object(b"FURN", 0x900002, sleep, &[(RESOURCE_BED, 2.0)], Some(unbound), &interner),
        // A chair from the furniture category.
        object(b"FURN", 0x900003, Some(CHAIR_KEYWORD), &[], None, &interner),
        // A sleepable repair-bot pod from another category.
        object(b"FURN", 0x900004, sleep, &[], None, &interner),
        // A bed no recipe builds.
        object(b"FURN", 0x900005, sleep, &[], None, &interner),
        // Generator: already bound, only lacks the resource-object value.
        object(b"ACTI", 0x900006, None, &[(POWER_GENERATED, 5.0)], Some(bound), &interner),
        // FO76 water purifier: a collector container.
        object(b"CONT", 0x900007, None, &[(WATER, 1.0), (POWER_REQUIRED, 8.0)], Some(collector), &interner),
        // Trap.
        object(b"ACTI", 0x900008, None, &[(SAFETY, 2.0)], None, &interner),
        // Guard post.
        object(b"FURN", 0x900009, Some(GUARD_KEYWORD), &[(SAFETY, 2.0)], None, &interner),
        // A powered decoration: needing power is not a rating.
        object(b"ACTI", 0x90000A, None, &[(POWER_REQUIRED, 1.0)], None, &interner),
    ];
    for record in records {
        session.add_record(record, &schema, &interner).unwrap();
    }

    let fixup = MarkWorkshopResourceObjectsFixup;
    assert!(fixup.applies_to_session(&session, &FixupConfig::default()));
    let report = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    let text = |syms: &[Sym]| syms.iter().map(|s| interner.resolve(*s)).collect::<Vec<_>>();
    assert!(report.warnings.is_empty(), "{:?}", text(&report.warnings));
    assert_eq!(report.records_changed, 6, "{:?}", text(&report.diagnostics));

    let decoded = |session: &mut PluginSession, local| {
        session
            .record_decoded(&fk(local, &interner), &schema, &interner)
            .unwrap()
    };
    let keywords = |record: &Record| keyword_form_keys(record, &masters, "SeventySix.esm", &interner);
    let work_object = fo4(WORKSHOP_WORK_OBJECT, &interner);
    let expected = [
        (0x900001, vec![(RESOURCE_BED, 1.0), (WORKSHOP_RESOURCE_OBJECT, 1.0)], true),
        (0x900002, vec![(RESOURCE_BED, 2.0), (WORKSHOP_RESOURCE_OBJECT, 1.0)], true),
        (0x900006, vec![(POWER_GENERATED, 5.0), (WORKSHOP_RESOURCE_OBJECT, 1.0)], false),
        (0x900007, vec![(POWER_REQUIRED, 8.0), (WATER, 1.0), (WORKSHOP_RESOURCE_OBJECT, 1.0)], false),
        (0x900008, vec![(SAFETY, 2.0), (WORKSHOP_RESOURCE_OBJECT, 1.0)], false),
        (0x900009, vec![(SAFETY, 2.0), (WORKSHOP_RESOURCE_OBJECT, 1.0)], true),
    ];
    for (local, rows, needs_settler) in expected {
        let record = decoded(&mut session, local);
        assert_eq!(properties(&record, &interner), rows, "{local:06X}");
        assert_eq!(keywords(&record).contains(&work_object), needs_settler, "{local:06X}");
    }
    assert_eq!(
        keywords(&decoded(&mut session, 0x900001)),
        vec![fo4(IS_SLEEP_FURNITURE, &interner), work_object]
    );

    for (local, sig) in [(0x900001, "FURN"), (0x900006, "ACTI"), (0x900008, "ACTI"), (0x900009, "FURN")] {
        let scripts = scripts(&mut session, local, sig, &interner);
        assert_eq!(scripts.as_array().unwrap().len(), 1, "{local:06X}");
        assert_bound_to_workshop_parent(&scripts[0]);
    }
    let sleeping_bag = scripts(&mut session, 0x900002, "FURN", &interner);
    assert_eq!(sleeping_bag.as_array().unwrap().len(), 2);
    assert_eq!(sleeping_bag[0]["ScriptName"], "B21MusicInstrumentScript");
    assert_bound_to_workshop_parent(&sleeping_bag[1]);
    let purifier = scripts(&mut session, 0x900007, "CONT", &interner);
    assert_eq!(purifier.as_array().unwrap().len(), 2);
    assert_eq!(purifier[0]["ScriptName"], "B21:WorkshopCollector");
    assert_bound_to_workshop_parent(&purifier[1]);

    for local in [0x900003, 0x900004, 0x900005, 0x90000A] {
        let record = decoded(&mut session, local);
        assert!(
            !properties(&record, &interner)
                .iter()
                .any(|(av, _)| *av == WORKSHOP_RESOURCE_OBJECT),
            "{local:06X}"
        );
        assert!(field(&record, b"VMAD").is_none(), "{local:06X}");
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
fn structured_property_sheets_are_read_and_extended() {
    let interner = StringInterner::new();
    let names = names(&interner);
    let av = |local| Av {
        key: fo4(local, &interner),
        raw: local,
    };
    let mut record = Record::new(SigCode(*b"ACTI"), fk(0x900006, &interner));
    record.fields.push(entry(
        b"PRPS",
        FieldValue::List(vec![FieldValue::Struct(vec![
            (names.actor_value, FieldValue::FormKey(fo4(POWER_GENERATED, &interner))),
            (names.value, FieldValue::Float(5.0)),
        ])]),
    ));
    assert_eq!(property_value(&record, av(POWER_GENERATED), &names), Some(5.0));
    assert_eq!(add_property(&mut record, av(WORKSHOP_RESOURCE_OBJECT), &names), Some(true));
    assert_eq!(add_property(&mut record, av(WORKSHOP_RESOURCE_OBJECT), &names), Some(false));
    assert_eq!(
        properties(&record, &interner),
        vec![(POWER_GENERATED, 5.0), (WORKSHOP_RESOURCE_OBJECT, 1.0)]
    );
}
