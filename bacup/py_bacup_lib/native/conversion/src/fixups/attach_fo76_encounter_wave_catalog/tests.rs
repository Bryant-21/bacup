use super::*;
use crate::formkey_mapper::MapperOptions;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::{
    build_vmad_bytes_from_payload, plugin_handle_close_native, plugin_handle_new_native,
};
use serde_json::json;

const PLUGIN: &str = "SeventySix.esm";
const FEED_QUEST: u32 = 0x09210E;
const SCORCHED_TEST_QUEST: u32 = 0x9000A1;
const UNRESOLVED_TEST_QUEST: u32 = 0x9000A2;
const SUBCLASS_WAVE_QUEST: u32 = 0x634B0B;
const LIBERATOR: u32 = 0x002ECE;
const MOLE_MINER_BOSS: u32 = 0x08EC17;
const MOLE_MINER: u32 = 0x0342C9;
const FERAL_GHOUL: u32 = 0x075337;
const SCORCHED_A: u32 = 0x08E624;
const SCORCHED_B: u32 = 0x31B1FF;
const UNEMITTED_SPAWN: u32 = 0x9000F0;

fn fk(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        plugin: interner.intern(PLUGIN),
        local,
    }
}

fn object(local: u32, alias: i32) -> Value {
    json!({"Alias": alias, "FormID": {"reference": {
        "plugin": PLUGIN, "object_id": format!("{local:06X}")
    }}})
}

fn member(name: &str, kind: &str, value: Value) -> Value {
    json!({"memberName": name, "Type": kind, "Flags": 1, "Value": value})
}

fn feed_wave(id: &str, keyword: u32, secondary_alias: i32, difficulty: i32) -> Value {
    json!([
        member("IDString", "String", json!(id)),
        member("WaveType", "Int32", json!(0)),
        member("WaveRefCollection", "Object", object(FEED_QUEST, 38)),
        member(
            "WaveRefCollectionSecondary",
            "Object",
            object(FEED_QUEST, secondary_alias),
        ),
        member("SpawnArea", "Object", object(FEED_QUEST, 7)),
        member("Difficulty", "Int32", json!(difficulty)),
        member("PreferSpawnmarker", "Bool", json!(true)),
        member("EncounterTypeKeyword", "Object", object(keyword, -1)),
        member("ClusterSpawns", "Bool", json!(false)),
    ])
}

fn quest_vmad(quest: u32, extra_scripts: Vec<Value>, waves: Vec<Value>) -> Vec<u8> {
    wave_script_quest_vmad(
        quest,
        "defaultquestencounterwavescript",
        extra_scripts,
        waves,
    )
}

fn wave_script_quest_vmad(
    quest: u32,
    wave_script: &str,
    extra_scripts: Vec<Value>,
    waves: Vec<Value>,
) -> Vec<u8> {
    let mut scripts = extra_scripts;
    scripts.push(json!({
        "ScriptName": wave_script,
        "Flags": 0,
        "Properties": [
            {"propertyName": "EMS", "Type": "Object", "Flags": 1, "Value": object(0x12D5B8, -1)},
            {"propertyName": "EncounterWaves", "Type": "Array of Struct", "Flags": 1, "Value": waves},
        ],
    }));
    let payload = json!({
        "Version": 6,
        "Object Format": 2,
        "semantic_type": "QUST",
        "Scripts": scripts,
        "Script Fragments": {
            "Version": 4,
            "FragmentCount": 1,
            "Script": {"ScriptName": format!("Fragments:Quests:QF_Test_{quest:08X}"), "Properties": []},
            "Fragments": [{
                "Quest Stage": 30,
                "Unknown": 0,
                "Quest Stage Index": 0,
                "Unknown1": 1,
                "ScriptName": format!("Fragments:Quests:QF_Test_{quest:08X}"),
                "FragmentName": "Fragment_Stage_0030_Item_00",
            }],
            "Aliases": [],
        },
    });
    build_vmad_bytes_from_payload(&payload, &[], PLUGIN).expect("quest VMAD")
}

fn bytes_field(sig: &[u8; 4], bytes: Vec<u8>) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value: FieldValue::Bytes(bytes.into()),
    }
}

fn record(sig: &[u8; 4], local: u32, eid: &str, interner: &StringInterner) -> Record {
    let mut record = Record::new(SigCode(*sig), fk(local, interner));
    let eid = interner.intern(eid);
    record.eid = Some(eid);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"EDID"),
        value: FieldValue::String(eid),
    });
    record
}

fn wave_record(
    local: u32,
    eid: &str,
    keywords: &[u32],
    entries: &[(u32, u32, u32)],
    interner: &StringInterner,
) -> Record {
    let mut wave = record(b"WAVE", local, eid, interner);
    wave.fields.push(FieldEntry {
        sig: SubrecordSig(*b"KSIZ"),
        value: FieldValue::Uint(keywords.len() as u64),
    });
    wave.fields.push(bytes_field(
        b"KWDA",
        keywords.iter().flat_map(|raw| raw.to_le_bytes()).collect(),
    ));
    wave.fields.push(bytes_field(
        b"WAVD",
        entries
            .iter()
            .flat_map(|(slot, spawn, unknown)| {
                [
                    slot.to_le_bytes(),
                    spawn.to_le_bytes(),
                    unknown.to_le_bytes(),
                ]
                .concat()
            })
            .collect(),
    ));
    wave.fields
        .push(bytes_field(b"DNAM", vec![0, 0, 0, 0, 0x0C, 0, 0, 0]));
    wave
}

fn repeated(count: usize, spawn: u32, unknown: u32) -> Vec<(u32, u32, u32)> {
    vec![(0, spawn, unknown); count]
}

/// WaveTypeScorched 529DF0: the first entry carries no spawn order (slot 0).
fn scorched_entries() -> Vec<(u32, u32, u32)> {
    vec![
        (0, SCORCHED_A, 15),
        (1, SCORCHED_B, 15),
        (2, SCORCHED_A, 15),
        (2, SCORCHED_B, 15),
        (3, SCORCHED_B, 15),
    ]
}

/// WaveTypeScorchedBoss 5307EF.
fn scorched_boss_entries() -> Vec<(u32, u32, u32)> {
    let mut entries = vec![(0, SCORCHED_B, 16), (0, SCORCHED_A, 16), (1, SCORCHED_A, 6)];
    for slot in 2..=11u32 {
        if !matches!(slot, 5 | 9) {
            entries.push((slot, SCORCHED_B, 6));
        }
        entries.push((slot, SCORCHED_A, 6));
    }
    entries
}

fn seed_source(source: u64, interner: &StringInterner) {
    let mut session = open_session(source, None).unwrap();
    let schema = session.schema().unwrap();
    let mut moles = repeated(1, MOLE_MINER_BOSS, 30);
    moles.extend(repeated(11, MOLE_MINER, 30));
    let mut records = vec![
        wave_record(
            0x8998AF,
            "FF06_Feed_WaveTypeLiberators",
            &[0x8998A8],
            &repeated(10, LIBERATOR, 30),
            interner,
        ),
        wave_record(
            0x8998AE,
            "FF06_Feed_WaveTypeMoles",
            &[0x8998A9],
            &moles,
            interner,
        ),
        wave_record(
            0x8998AD,
            "FF06_Feed_WaveTypeGhouls",
            &[0x8998AA],
            &repeated(11, FERAL_GHOUL, 30),
            interner,
        ),
        wave_record(
            0x529DF0,
            "WaveTypeScorched",
            &[0x0067D1, 0x05CB01, 0x0067ED, 0x2D3EBF],
            &scorched_entries(),
            interner,
        ),
        wave_record(
            0x5307EF,
            "WaveTypeScorchedBoss",
            &[0x0067D1, 0x05CB01, 0x0067ED, 0x2D3EBF],
            &scorched_boss_entries(),
            interner,
        ),
        wave_record(
            0x9000B0,
            "B21Test_WaveTypeUnemitted",
            &[0x9000B1],
            &repeated(3, UNEMITTED_SPAWN, 30),
            interner,
        ),
    ];

    let mut feed = record(b"QUST", FEED_QUEST, "FF06_Feed", interner);
    feed.fields.push(bytes_field(
        b"VMAD",
        quest_vmad(
            FEED_QUEST,
            vec![json!({"ScriptName": "defaultgamedayspassedglobalsscript", "Flags": 0, "Properties": []})],
            vec![
                feed_wave("Wave00", 0x8998A8, 33, 0),
                feed_wave("Wave01", 0x8998A9, 8, 3),
                feed_wave("Wave02", 0x8998AA, 9, 4),
                feed_wave("Wave03", 0x8998AA, 40, 4),
            ],
        ),
    ));
    records.push(feed);

    let mut scorched = record(
        b"QUST",
        SCORCHED_TEST_QUEST,
        "B21Test_ScorchedWaves",
        interner,
    );
    scorched.fields.push(bytes_field(
        b"VMAD",
        quest_vmad(
            SCORCHED_TEST_QUEST,
            Vec::new(),
            vec![
                json!([
                    member("IDString", "String", json!("Stored")),
                    member("StoredEncounterWave", "Object", object(0x529DF0, -1)),
                    member("EncounterTypeKeyword", "Object", object(0x8998A8, -1)),
                ]),
                json!([
                    member("IDString", "String", json!("Keyword")),
                    member(
                        "StoredEncounterWave",
                        "Object",
                        json!({"Alias": -1, "FormID": null})
                    ),
                    member("EncounterTypeKeyword", "Object", object(0x2D3EBF, -1)),
                ]),
            ],
        ),
    ));
    records.push(scorched);

    let mut unresolved = record(b"QUST", UNRESOLVED_TEST_QUEST, "B21Test_NoWaves", interner);
    unresolved.fields.push(bytes_field(
        b"VMAD",
        quest_vmad(
            UNRESOLVED_TEST_QUEST,
            Vec::new(),
            vec![
                json!([member(
                    "EncounterTypeKeyword",
                    "Object",
                    object(0x9000C0, -1)
                )]),
                json!([member(
                    "EncounterTypeKeyword",
                    "Object",
                    object(0x9000B1, -1)
                )]),
                json!([member("IDString", "String", json!("NoSelector"))]),
            ],
        ),
    ));
    records.push(unresolved);

    // Spin the Wheel: an EncounterWaveParentScript subclass, not DefaultQuestEncounterWaveScript.
    let mut wheel = record(b"QUST", SUBCLASS_WAVE_QUEST, "E09B_Wheel", interner);
    wheel.fields.push(bytes_field(
        b"VMAD",
        wave_script_quest_vmad(
            SUBCLASS_WAVE_QUEST,
            "E09B_MobWaves",
            vec![json!({"ScriptName": "E09B_Script", "Flags": 0, "Properties": []})],
            vec![json!([
                member("IDString", "String", json!("MobWave")),
                member("EncounterTypeKeyword", "Object", object(0x2D3EBF, -1)),
            ])],
        ),
    ));
    records.push(wheel);

    for record in records {
        session.add_record(record, &schema, interner).unwrap();
    }
}

fn target_quest_vmad(target_masters: &[String]) -> Vec<u8> {
    build_vmad_bytes_from_payload(
        &json!({
            "Version": 6,
            "Object Format": 2,
            "Scripts": [
                {"ScriptName": "DefaultQuestEncounterWaveScript", "Flags": 0, "Properties": []},
                {"ScriptName": "B21:LocalEncounterMaterializer", "Flags": 0, "Properties": [
                    {"propertyName": "WaveActorCounts", "Type": "Array of Int32", "Flags": 1, "Value": [3]},
                ]},
            ],
        }),
        target_masters,
        PLUGIN,
    )
    .unwrap()
}

fn catalog(session: &mut PluginSession, quest: FormKey) -> Option<(Vec<String>, Value)> {
    let bytes = session.first_subrecord_bytes(&quest, "VMAD").unwrap()?;
    let decoded =
        compact_vmad_payload_json(&bytes, session.target_masters(), PLUGIN, Some("QUST")).unwrap();
    let scripts = decoded["Scripts"].as_array().unwrap();
    let names = scripts
        .iter()
        .map(|script| script["ScriptName"].as_str().unwrap().to_string())
        .collect();
    let catalog = scripts
        .iter()
        .find(|script| script["ScriptName"] == CATALOG_SCRIPT_NAME)?;
    Some((names, catalog["Properties"].clone()))
}

fn catalog_rows(properties: &Value, interner: &StringInterner) -> Vec<(i64, i64, i64, FormKey)> {
    let ints = |name| {
        named_value(properties, "propertyName", name)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_i64().unwrap())
            .collect::<Vec<_>>()
    };
    let forms = named_value(properties, "propertyName", "CandidateForms")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| object_form(value, interner).unwrap())
        .collect::<Vec<_>>();
    let waves = ints("CandidateWaveIndices");
    let variants = ints("CandidateVariants");
    let slots = ints("CandidateSpawnSlots");
    assert_eq!(waves.len(), forms.len());
    assert_eq!(variants.len(), forms.len());
    assert_eq!(slots.len(), forms.len());
    (0..forms.len())
        .map(|row| (waves[row], variants[row], slots[row], forms[row]))
        .collect()
}

fn expected(
    wave: i64,
    variant: i64,
    entries: &[(u32, u32, u32)],
    target: impl Fn(u32) -> FormKey,
) -> Vec<(i64, i64, i64, FormKey)> {
    entries
        .iter()
        .map(|(slot, spawn, _)| (wave, variant, i64::from(*slot), target(*spawn)))
        .collect()
}

#[test]
fn feed_the_people_and_slot_ordered_waves_attach_catalog_rows() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native(PLUGIN, Some("fo76")).unwrap();
    let target = plugin_handle_new_native(PLUGIN, Some("fo4")).unwrap();
    seed_source(source, &interner);

    let mut session = open_session(target, Some(source)).unwrap();
    session
        .target_slot_mut()
        .parsed
        .header
        .masters
        .push("Fallout4.esm".into());
    let schema = session.schema().unwrap();
    let target_masters = session.target_masters().to_vec();
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: PLUGIN.into(),
            source_plugin_name: PLUGIN.into(),
            ..MapperOptions::default()
        },
        &interner,
    );
    let fallout4 = interner.intern("Fallout4.esm");
    for quest in [
        FEED_QUEST,
        SCORCHED_TEST_QUEST,
        UNRESOLVED_TEST_QUEST,
        SUBCLASS_WAVE_QUEST,
    ] {
        mapper.add_mapping(fk(quest, &interner), fk(quest, &interner));
        let mut target_quest = record(b"QUST", quest, "Quest", &interner);
        target_quest
            .fields
            .push(bytes_field(b"VMAD", target_quest_vmad(&target_masters)));
        session
            .add_record(target_quest, &schema, &interner)
            .unwrap();
    }
    for spawn in [
        LIBERATOR,
        MOLE_MINER_BOSS,
        MOLE_MINER,
        SCORCHED_A,
        SCORCHED_B,
    ] {
        mapper.add_mapping(fk(spawn, &interner), fk(spawn, &interner));
        session
            .add_record(
                record(b"NPC_", spawn, "Spawn", &interner),
                &schema,
                &interner,
            )
            .unwrap();
    }
    // Resolved to a master record, which is not in the output plugin.
    let ghoul_target = FormKey {
        plugin: fallout4,
        local: FERAL_GHOUL,
    };
    mapper.add_mapping(fk(FERAL_GHOUL, &interner), ghoul_target);
    // Mapped, but never emitted.
    mapper.add_mapping(
        fk(UNEMITTED_SPAWN, &interner),
        fk(UNEMITTED_SPAWN, &interner),
    );

    let fixup = AttachFo76EncounterWaveCatalogFixup;
    assert!(fixup.applies_to_session(&session, &FixupConfig::default()));
    let report = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    let warnings = report
        .warnings
        .iter()
        .filter_map(|warning| interner.resolve(*warning))
        .collect::<Vec<_>>();
    assert_eq!(report.records_changed, 3, "{warnings:?}");
    assert_eq!(
        warnings,
        vec!["attach_fo76_encounter_wave_catalog:9000A2:dropped_unmapped_spawns=3"]
    );
    assert_eq!(
        interner.resolve(report.message.unwrap()),
        Some(
            "fo76_encounter_wave_catalog:attached=3;present=0;unresolved=0;without_rows=1;rows=101;dropped_rows=3"
        )
    );

    let local = |spawn| fk(spawn, &interner);
    let (names, properties) = catalog(&mut session, local(FEED_QUEST)).expect("Feed catalog");
    assert_eq!(
        names,
        vec![
            "DefaultQuestEncounterWaveScript",
            "B21:LocalEncounterMaterializer",
            CATALOG_SCRIPT_NAME
        ]
    );
    let mut feed_rows = expected(0, 0, &repeated(10, LIBERATOR, 30), local);
    feed_rows.extend(expected(1, 0, &repeated(1, MOLE_MINER_BOSS, 30), local));
    feed_rows.extend(expected(1, 0, &repeated(11, MOLE_MINER, 30), local));
    feed_rows.extend(expected(2, 0, &repeated(11, FERAL_GHOUL, 30), |_| {
        ghoul_target
    }));
    feed_rows.extend(expected(3, 0, &repeated(11, FERAL_GHOUL, 30), |_| {
        ghoul_target
    }));
    assert_eq!(catalog_rows(&properties, &interner), feed_rows);

    let (_, properties) =
        catalog(&mut session, local(SCORCHED_TEST_QUEST)).expect("Scorched catalog");
    let mut scorched_rows = expected(0, 0, &scorched_entries(), local);
    scorched_rows.extend(expected(1, 0, &scorched_entries(), local));
    scorched_rows.extend(expected(1, 1, &scorched_boss_entries(), local));
    assert_eq!(catalog_rows(&properties, &interner), scorched_rows);

    assert!(catalog(&mut session, local(UNRESOLVED_TEST_QUEST)).is_none());

    let (_, properties) =
        catalog(&mut session, local(SUBCLASS_WAVE_QUEST)).expect("Spin the Wheel catalog");
    let mut wheel_rows = expected(0, 0, &scorched_entries(), local);
    wheel_rows.extend(expected(0, 1, &scorched_boss_entries(), local));
    assert_eq!(catalog_rows(&properties, &interner), wheel_rows);

    let rerun = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(rerun.records_changed, 0);
    assert_eq!(
        interner.resolve(rerun.message.unwrap()),
        Some(
            "fo76_encounter_wave_catalog:attached=0;present=3;unresolved=0;without_rows=1;rows=101;dropped_rows=3"
        )
    );

    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(source));
}

#[test]
fn variants_stay_dense_when_a_wave_record_loses_every_spawn() {
    let interner = StringInterner::new();
    let wave = |local| fk(local, &interner);
    let rows = [
        SourceRow {
            wave_index: 0,
            source_variant: 0,
            spawn_slot: 0,
            spawn: wave(UNEMITTED_SPAWN),
        },
        SourceRow {
            wave_index: 0,
            source_variant: 1,
            spawn_slot: 2,
            spawn: wave(SCORCHED_A),
        },
        SourceRow {
            wave_index: 1,
            source_variant: 0,
            spawn_slot: 0,
            spawn: wave(SCORCHED_B),
        },
    ];
    let (mapped, dropped) = map_catalog_rows(&rows, |spawn| {
        (spawn.local != UNEMITTED_SPAWN).then_some(spawn)
    });
    assert_eq!(dropped, 1);
    assert_eq!(
        mapped,
        vec![
            CatalogRow {
                wave_index: 0,
                variant: 0,
                spawn_slot: 2,
                form: wave(SCORCHED_A),
            },
            CatalogRow {
                wave_index: 1,
                variant: 0,
                spawn_slot: 0,
                form: wave(SCORCHED_B),
            },
        ]
    );
}
