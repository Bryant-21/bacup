use super::*;
use crate::formkey_mapper::MapperOptions;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use esp_authoring_core::plugin_runtime::{
    build_vmad_bytes_from_payload, plugin_handle_close_native, plugin_handle_new_native,
};
use serde_json::{Value, json};

const PLUGIN: &str = "SeventySix.esm";
const FEED: u32 = 0x09210E;
const ONE_VIOLENT_NIGHT: u32 = 0x10BAE1;
const UNDEFINED_EXPIRY: u32 = 0x9000C1;
const TIMER_END_ONLY: u32 = 0x9000C2;
const MISSING_TIMER_GLOBAL: u32 = 0x9000C3;
const RUN_ON_START: u8 = 0x02;
const RUN_ON_STOP: u8 = 0x04;

fn fk(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        plugin: interner.intern(PLUGIN),
        local,
    }
}

fn field(sig: &[u8; 4], data: Vec<u8>) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value: FieldValue::Bytes(data.into()),
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

fn stage(index: u16, flags: u8) -> FieldEntry {
    let [low, high] = index.to_le_bytes();
    field(b"INDX", vec![low, high, flags, 0])
}

/// `(objective, uses_timer, timer, expiry_stage)`
fn objective(fields: &mut Vec<FieldEntry>, spec: (u16, bool, Option<u32>, Option<u16>)) {
    let (index, uses_timer, timer, expiry_stage) = spec;
    fields.push(field(b"QOBJ", index.to_le_bytes().to_vec()));
    let flags: u32 = if uses_timer { 0x8 } else { 0 };
    fields.push(field(b"FNAM", flags.to_le_bytes().to_vec()));
    if let Some(timer) = timer {
        fields.push(field(b"QOTM", timer.to_le_bytes().to_vec()));
    }
    if let Some(expiry_stage) = expiry_stage {
        fields.push(field(b"SNAM", expiry_stage.to_le_bytes().to_vec()));
    }
}

fn source_quest(
    local: u32,
    eid: &str,
    quest_timer: Option<u32>,
    stages: &[(u16, u8)],
    objectives: &[(u16, bool, Option<u32>, Option<u16>)],
    interner: &StringInterner,
) -> Record {
    let mut quest = record(b"QUST", local, eid, interner);
    let mut fields = Vec::new();
    if let Some(timer) = quest_timer {
        fields.push(field(b"QTLM", timer.to_le_bytes().to_vec()));
    }
    fields.extend(stages.iter().map(|(index, flags)| stage(*index, *flags)));
    for spec in objectives {
        objective(&mut fields, *spec);
    }
    fields.push(field(b"ANAM", 2u32.to_le_bytes().to_vec()));
    fields.push(field(b"ALST", 0u32.to_le_bytes().to_vec()));
    // Alias flags reuse FNAM; bit 0x8 here must not read as an objective timer.
    fields.push(field(b"FNAM", 0x8u32.to_le_bytes().to_vec()));
    quest.fields.extend(fields);
    quest
}

fn seed_source(source: u64, interner: &StringInterner) {
    let mut session = open_session(source, None).unwrap();
    let schema = session.schema().unwrap();
    let records = vec![
        source_quest(
            FEED,
            "FF06_Feed",
            Some(0x43C5F5),
            &[
                (10, RUN_ON_START),
                (20, 0),
                (30, 0),
                (100, STAGE_FLAG_START_TIMER),
                (200, 0),
                (1000, 0),
                (1500, 0),
                (1550, RUN_ON_STOP),
                (9991, 0),
                (10000, RUN_ON_STOP),
            ],
            &[
                (1, true, Some(0x63616D), Some(9991)),
                (5, false, None, None),
                (40, false, None, None),
                (70, true, Some(0x17864D), Some(1500)),
                (75, true, Some(0x17864D), Some(1500)),
                (80, true, Some(0x17864D), Some(1500)),
                (85, true, Some(0x17864D), Some(1500)),
            ],
            interner,
        ),
        source_quest(
            ONE_VIOLENT_NIGHT,
            "MTNS04_Night",
            Some(0x59F29C),
            &[
                (100, RUN_ON_START),
                (110, 0),
                (200, 0),
                (300, 0),
                (301, 0),
                (400, 0),
                (450, 0),
                (500, RUN_ON_STOP),
                (9990, 0),
                (9991, STAGE_FLAG_TIMER_END),
            ],
            &[
                (100, true, Some(0x58B688), Some(9990)),
                (200, true, Some(0x5902D5), Some(9991)),
                (250, false, None, None),
                (260, false, None, None),
                (300, true, Some(0x5902D6), Some(9991)),
            ],
            interner,
        ),
        source_quest(
            UNDEFINED_EXPIRY,
            "B21Test_UndefinedExpiryStage",
            None,
            &[(10, 0)],
            &[
                (10, true, Some(0x58B688), Some(4022)),
                (20, false, None, None),
            ],
            interner,
        ),
        source_quest(
            TIMER_END_ONLY,
            "B21Test_TimerEndStageOnly",
            None,
            &[(10, 0), (210, STAGE_FLAG_TIMER_END)],
            &[(10, false, None, None)],
            interner,
        ),
        source_quest(
            MISSING_TIMER_GLOBAL,
            "B21Test_MissingObjectiveTimerGlobal",
            None,
            &[(10, 0), (9991, 0)],
            &[(10, true, None, Some(9991))],
            interner,
        ),
    ];
    for record in records {
        session.add_record(record, &schema, interner).unwrap();
    }
}

fn scripts(session: &mut PluginSession, quest: FormKey) -> Vec<Value> {
    let bytes = session
        .first_subrecord_bytes(&quest, "VMAD")
        .unwrap()
        .unwrap();
    let decoded =
        compact_vmad_payload_json(&bytes, session.target_masters(), PLUGIN, Some("QUST")).unwrap();
    decoded["Scripts"].as_array().unwrap().clone()
}

fn property<'a>(script: &'a Value, name: &str) -> &'a Value {
    &script["Properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|property| property["propertyName"] == name)
        .unwrap()["Value"]
}

fn form_local(value: &Value) -> u32 {
    u32::from_str_radix(
        value["FormID"]["reference"]["object_id"].as_str().unwrap(),
        16,
    )
    .unwrap()
}

fn ints(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_i64().unwrap())
        .collect()
}

fn assert_timers(
    scripts: &[Value],
    quest_timer: (u32, Vec<i64>, Vec<i64>),
    objective_rows: Vec<(i64, u32, i64)>,
) {
    let names = scripts
        .iter()
        .map(|script| script["ScriptName"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "DefaultEventQuest",
            QUEST_TIMER_SCRIPT,
            OBJECTIVE_TIMERS_SCRIPT
        ]
    );
    let quest = &scripts[1];
    assert_eq!(form_local(property(quest, "TimerLength")), quest_timer.0);
    assert_eq!(ints(property(quest, "StartTimerStages")), quest_timer.1);
    assert_eq!(ints(property(quest, "TimerEndStages")), quest_timer.2);

    let objectives = &scripts[2];
    let indices = ints(property(objectives, "TimedObjectives"));
    let timers = property(objectives, "TimerLengths")
        .as_array()
        .unwrap()
        .iter()
        .map(form_local)
        .collect::<Vec<_>>();
    let stages = ints(property(objectives, "ExpiryStages"));
    let rows = (0..indices.len())
        .map(|row| (indices[row], timers[row], stages[row]))
        .collect::<Vec<_>>();
    assert_eq!(rows, objective_rows);
}

#[test]
fn feed_the_people_and_one_violent_night_timers_attach_to_their_quests() {
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
    let existing_vmad = build_vmad_bytes_from_payload(
        &json!({
            "Version": 6,
            "Object Format": 2,
            "Scripts": [{"ScriptName": "DefaultEventQuest", "Flags": 0, "Properties": []}],
        }),
        session.target_masters(),
        PLUGIN,
    )
    .unwrap();
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: PLUGIN.into(),
            source_plugin_name: PLUGIN.into(),
            ..MapperOptions::default()
        },
        &interner,
    );
    for quest in [
        FEED,
        ONE_VIOLENT_NIGHT,
        UNDEFINED_EXPIRY,
        TIMER_END_ONLY,
        MISSING_TIMER_GLOBAL,
    ] {
        mapper.add_mapping(fk(quest, &interner), fk(quest, &interner));
        let mut target_quest = record(b"QUST", quest, "Quest", &interner);
        target_quest
            .fields
            .push(field(b"VMAD", existing_vmad.clone()));
        session
            .add_record(target_quest, &schema, &interner)
            .unwrap();
    }
    for global in [
        0x43C5F5, 0x63616D, 0x17864D, 0x59F29C, 0x58B688, 0x5902D5, 0x5902D6,
    ] {
        mapper.add_mapping(fk(global, &interner), fk(global, &interner));
        session
            .add_record(
                record(b"GLOB", global, "Timer", &interner),
                &schema,
                &interner,
            )
            .unwrap();
    }

    let fixup = AttachFo76QuestTimersFixup;
    assert!(fixup.applies_to_session(&session, &FixupConfig::default()));
    let report = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    let warnings = report
        .warnings
        .iter()
        .filter_map(|warning| interner.resolve(*warning))
        .collect::<Vec<_>>();
    assert_eq!(
        warnings,
        vec!["attach_fo76_quest_timers:9000C3:objective_timer_skipped=10"]
    );
    assert_eq!(report.records_changed, 6);

    assert_timers(
        &scripts(&mut session, fk(FEED, &interner)),
        (0x43C5F5, vec![100], vec![]),
        vec![
            (1, 0x63616D, 9991),
            (70, 0x17864D, 1500),
            (75, 0x17864D, 1500),
            (80, 0x17864D, 1500),
            (85, 0x17864D, 1500),
        ],
    );
    assert_timers(
        &scripts(&mut session, fk(ONE_VIOLENT_NIGHT, &interner)),
        (0x59F29C, vec![], vec![9991]),
        vec![
            (100, 0x58B688, 9990),
            (200, 0x5902D5, 9991),
            (300, 0x5902D6, 9991),
        ],
    );
    // FO76's SNAM 65535 sentinel: the countdown survives with no stage to set.
    let undefined_expiry = scripts(&mut session, fk(UNDEFINED_EXPIRY, &interner));
    assert_eq!(undefined_expiry.len(), 2);
    assert_eq!(undefined_expiry[1]["ScriptName"], OBJECTIVE_TIMERS_SCRIPT);
    assert_eq!(
        ints(property(&undefined_expiry[1], "TimedObjectives")),
        vec![10]
    );
    assert_eq!(
        ints(property(&undefined_expiry[1], "ExpiryStages")),
        vec![i64::from(NO_EXPIRY_STAGE)]
    );
    assert_eq!(
        scripts(&mut session, fk(MISSING_TIMER_GLOBAL, &interner)).len(),
        1
    );
    let stage_only = scripts(&mut session, fk(TIMER_END_ONLY, &interner));
    assert_eq!(stage_only.len(), 2);
    assert_eq!(stage_only[1]["ScriptName"], QUEST_TIMER_SCRIPT);
    assert!(property(&stage_only[1], "TimerLength")["FormID"].is_null());
    assert_eq!(
        ints(property(&stage_only[1], "StartTimerStages")),
        Vec::<i64>::new()
    );
    assert_eq!(ints(property(&stage_only[1], "TimerEndStages")), vec![210]);

    let rerun = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(rerun.records_changed, 0);
    assert_eq!(
        interner.resolve(rerun.message.unwrap()),
        Some("fo76_quest_timers:attached=0;present=6;unresolved=0;skipped_objectives=1")
    );

    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(source));
}
