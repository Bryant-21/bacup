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
/// `FF05_Balance` "Daily: Ecological Balance", start keyword `FF05_Balance_QuestStart`.
const BALANCE: u32 = 0x01C035;
const BALANCE_KEYWORD: u32 = 0x044CE2;
/// `SFZ04_Waste` "Daily: Waste Not", start keyword `SFZ04_Waste_QuestStartKeyword`.
const WASTE: u32 = 0x124487;
const WASTE_KEYWORD: u32 = 0x36C246;
/// A quest whose start keyword did not survive conversion.
const DROPPED_KEYWORD_QUEST: u32 = 0x9000D1;
const DROPPED_KEYWORD: u32 = 0x9000D2;
/// A quest with no `QSSK` at all.
const NO_KEYWORD: u32 = 0x9000D3;

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

fn source_quest(
    local: u32,
    eid: &str,
    start_keyword: Option<u32>,
    interner: &StringInterner,
) -> Record {
    let mut quest = record(b"QUST", local, eid, interner);
    quest
        .fields
        .push(field(b"ENAM", 1414546259u32.to_le_bytes().to_vec()));
    if let Some(keyword) = start_keyword {
        quest
            .fields
            .push(field(b"QSSK", keyword.to_le_bytes().to_vec()));
    }
    quest.fields.push(field(
        b"INDX",
        10u16.to_le_bytes().into_iter().chain([2, 0]).collect(),
    ));
    quest
        .fields
        .push(field(b"ANAM", 1u32.to_le_bytes().to_vec()));
    quest
        .fields
        .push(field(b"ALST", 0u32.to_le_bytes().to_vec()));
    quest
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

fn start_keyword_local(scripts: &[Value]) -> u32 {
    let script = scripts
        .iter()
        .find(|script| script["ScriptName"] == START_KEYWORD_SCRIPT)
        .unwrap();
    let value = &script["Properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|property| property["propertyName"] == "StartKeyword")
        .unwrap()["Value"];
    u32::from_str_radix(
        value["FormID"]["reference"]["object_id"].as_str().unwrap(),
        16,
    )
    .unwrap()
}

#[test]
fn daily_start_keywords_attach_and_unresolved_ones_are_skipped() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native(PLUGIN, Some("fo76")).unwrap();
    let target = plugin_handle_new_native(PLUGIN, Some("fo4")).unwrap();

    {
        let mut source_session = open_session(source, None).unwrap();
        let schema = source_session.schema().unwrap();
        for quest in [
            source_quest(BALANCE, "FF05_Balance", Some(BALANCE_KEYWORD), &interner),
            source_quest(WASTE, "SFZ04_Waste", Some(WASTE_KEYWORD), &interner),
            source_quest(
                DROPPED_KEYWORD_QUEST,
                "B21Test_DroppedStartKeyword",
                Some(DROPPED_KEYWORD),
                &interner,
            ),
            source_quest(NO_KEYWORD, "B21Test_NoStartKeyword", None, &interner),
        ] {
            source_session
                .add_record(quest, &schema, &interner)
                .unwrap();
        }
    }

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
            "Scripts": [{"ScriptName": "DefaultDailyQuestScript", "Flags": 0, "Properties": []}],
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
    for quest in [BALANCE, WASTE, DROPPED_KEYWORD_QUEST, NO_KEYWORD] {
        mapper.add_mapping(fk(quest, &interner), fk(quest, &interner));
        let mut target_quest = record(b"QUST", quest, "Quest", &interner);
        target_quest
            .fields
            .push(field(b"VMAD", existing_vmad.clone()));
        session
            .add_record(target_quest, &schema, &interner)
            .unwrap();
    }
    // The dropped keyword is mapped but was never emitted, so it must not bind.
    for keyword in [BALANCE_KEYWORD, WASTE_KEYWORD, DROPPED_KEYWORD] {
        mapper.add_mapping(fk(keyword, &interner), fk(keyword, &interner));
    }
    for keyword in [BALANCE_KEYWORD, WASTE_KEYWORD] {
        session
            .add_record(
                record(b"KYWD", keyword, "QuestStart", &interner),
                &schema,
                &interner,
            )
            .unwrap();
    }

    let fixup = AttachFo76QuestStartKeywordFixup;
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
        vec!["attach_fo76_quest_start_keyword:9000D1:start_keyword_missing"]
    );
    assert_eq!(report.records_changed, 2);

    assert_eq!(
        start_keyword_local(&scripts(&mut session, fk(BALANCE, &interner))),
        BALANCE_KEYWORD
    );
    assert_eq!(
        start_keyword_local(&scripts(&mut session, fk(WASTE, &interner))),
        WASTE_KEYWORD
    );
    // The existing binding is preserved, not replaced.
    let balance = scripts(&mut session, fk(BALANCE, &interner));
    assert_eq!(
        balance
            .iter()
            .map(|script| script["ScriptName"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["DefaultDailyQuestScript", START_KEYWORD_SCRIPT]
    );

    for quest in [DROPPED_KEYWORD_QUEST, NO_KEYWORD] {
        let bound = scripts(&mut session, fk(quest, &interner));
        assert_eq!(bound.len(), 1, "{quest:06X} must keep only its own script");
    }

    let rerun = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(rerun.records_changed, 0);
    assert_eq!(
        interner.resolve(rerun.message.unwrap()),
        Some("fo76_quest_start_keyword:attached=0;present=2;unresolved=0;keyword_missing=1")
    );

    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(source));
}
