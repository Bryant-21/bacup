use super::*;
use crate::formkey_mapper::MapperOptions;
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use esp_authoring_core::plugin_runtime::{
    build_vmad_bytes_from_payload, plugin_handle_close_native, plugin_handle_new_native,
};
use serde_json::{Value, json};

const PLUGIN: &str = "SeventySix.esm";
const FEED: u32 = 0x09210E;
const LIGHT: u32 = 0x187531;
const HABITAT: u32 = 0x5109AF;
const LIGHT_TIMER: u32 = 0x18AEB8;
const LAMP_TEXT: u32 = 0x1001;
const COMMUNE_TEXT: u32 = 0x1002;
const DEPOSIT_TEXT: u32 = 0x1003;
const LIGHT_LOG: u32 = 0x2001;
const MOVED_DEPOSIT_TEXT: u32 = 0x2002;

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

fn seed_strings(session: &mut PluginSession) {
    session.target_slot_mut().parsed.header.flags |= TES4_FLAG_LOCALIZED;
    let strings = session.target_slot_mut().strings_mut();
    strings.default_language = "en".into();
    for (id, table, en, de) in [
        (
            LAMP_TEXT,
            "strings",
            "Deposit Bioluminescent Fluids in lamp (<Variable=CurrentFluid>/<Variable=MaxFluid>)",
            "Fülle Biolumineszenz-Flüssigkeit in die Lampe. (<variable=currentFluid>/<Variable=MaxFluid>)",
        ),
        (
            COMMUNE_TEXT,
            "strings",
            "(Optional) Commune with the Wise Mothman",
            "(Optional) Kommuniziere mit dem Weisen Mottenmann.",
        ),
        (
            DEPOSIT_TEXT,
            "strings",
            "Deposit food (<Variable=ItemDepositsCompleted>/<Variable=ItemDepositsToCompleteTotal>)",
            "Lege Essen ab (<Variable=ItemDepositsCompleted>/<Variable=ItemDepositsToCompleteTotal>)",
        ),
        (
            LIGHT_LOG,
            "dlstrings",
            "The lamp holds <Variable=CurrentFluid> fluid.",
            "Die Lampe enthält <Variable=CurrentFluid> Flüssigkeit.",
        ),
    ] {
        for (language, text) in [("en", en), ("de", de)] {
            strings
                .by_language
                .entry(language.into())
                .or_default()
                .insert(id, text.into());
        }
        strings.table_types.insert(id, table.into());
    }
}

fn quest(
    local: u32,
    eid: &str,
    vmad: Option<&[u8]>,
    globals: &[u32],
    log: Option<u32>,
    objectives: &[(u16, u32)],
    interner: &StringInterner,
) -> Record {
    let mut quest = record(b"QUST", local, eid, interner);
    if let Some(vmad) = vmad {
        quest.fields.push(field(b"VMAD", vmad.to_vec()));
    }
    for global in globals {
        quest
            .fields
            .push(field(b"QTGL", global.to_le_bytes().to_vec()));
    }
    quest.fields.push(field(b"NEXT", Vec::new()));
    quest.fields.push(field(b"INDX", vec![100, 0, 0, 0]));
    if let Some(log) = log {
        quest.fields.push(field(b"QSDT", vec![0]));
        quest
            .fields
            .push(field(b"CNAM", log.to_le_bytes().to_vec()));
    }
    for (objective, text) in objectives {
        quest
            .fields
            .push(field(b"QOBJ", objective.to_le_bytes().to_vec()));
        quest.fields.push(field(b"FNAM", vec![0; 4]));
        quest
            .fields
            .push(field(b"NNAM", text.to_le_bytes().to_vec()));
    }
    quest
}

fn subrecords(session: &mut PluginSession, quest: FormKey) -> Vec<(String, Vec<u8>)> {
    let raw_form_id = session.raw_form_id_for_form_key(&quest).unwrap();
    session
        .record(raw_form_id)
        .unwrap()
        .subrecords
        .iter()
        .map(|subrecord| (subrecord.signature.to_string(), subrecord.data.to_vec()))
        .collect()
}

fn signatures(subrecords: &[(String, Vec<u8>)]) -> Vec<&str> {
    subrecords.iter().map(|(sig, _)| sig.as_str()).collect()
}

fn u32_values(subrecords: &[(String, Vec<u8>)], sig: &str) -> Vec<u32> {
    subrecords
        .iter()
        .filter(|(signature, _)| signature == sig)
        .map(|(_, data)| u32::from_le_bytes(data.as_slice().try_into().unwrap()))
        .collect()
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

/// `(VariableNames, VariableGlobals object ids)` of the quest's binding.
fn variable_rows(scripts: &[Value]) -> (Vec<String>, Vec<u32>) {
    let script = scripts
        .iter()
        .find(|script| script["ScriptName"] == SCRIPT_NAME)
        .unwrap();
    let names = property(script, "VariableNames")
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_string())
        .collect();
    let globals = property(script, "VariableGlobals")
        .as_array()
        .unwrap()
        .iter()
        .map(|global| {
            u32::from_str_radix(
                global["FormID"]["reference"]["object_id"].as_str().unwrap(),
                16,
            )
            .unwrap()
        })
        .collect();
    (names, globals)
}

fn text(session: &PluginSession, language: &str, id: u32) -> String {
    session
        .target_slot()
        .strings_ref()
        .resolve(language, id)
        .unwrap()
}

fn resolve_all(interner: &StringInterner, symbols: &[crate::sym::Sym]) -> Vec<String> {
    symbols
        .iter()
        .filter_map(|symbol| interner.resolve(*symbol))
        .map(str::to_string)
        .collect()
}

#[test]
fn quest_text_variables_become_quest_display_globals() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native(PLUGIN, Some("fo76")).unwrap();
    let target = plugin_handle_new_native(PLUGIN, Some("fo4")).unwrap();
    let mut session = open_session(target, Some(source)).unwrap();
    session
        .target_slot_mut()
        .parsed
        .header
        .masters
        .push("Fallout4.esm".into());
    seed_strings(&mut session);
    let schema = session.schema().unwrap();
    let event_vmad = build_vmad_bytes_from_payload(
        &json!({
            "Version": 6,
            "Object Format": 2,
            "Scripts": [{"ScriptName": "DefaultEventQuest", "Flags": 0, "Properties": []}],
        }),
        session.target_masters(),
        PLUGIN,
    )
    .unwrap();
    let own = |local: u32| (1 << 24) | local;
    for record in [
        quest(
            FEED,
            "FF06_Feed",
            Some(&event_vmad),
            &[],
            None,
            &[(10, DEPOSIT_TEXT)],
            &interner,
        ),
        quest(
            LIGHT,
            "FFZ10_Light",
            Some(&event_vmad),
            &[own(LIGHT_TIMER)],
            Some(LIGHT_LOG),
            &[(10, LAMP_TEXT), (20, COMMUNE_TEXT)],
            &interner,
        ),
        quest(
            HABITAT,
            "SFS09_Habitat",
            None,
            &[],
            None,
            &[(5, DEPOSIT_TEXT)],
            &interner,
        ),
        record(b"GLOB", LIGHT_TIMER, "FFZ10_Timer", &interner),
    ] {
        session.add_record(record, &schema, &interner).unwrap();
    }
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: PLUGIN.into(),
            source_plugin_name: PLUGIN.into(),
            ..MapperOptions::default()
        },
        &interner,
    );

    let fixup = AttachFo76QuestVariablesFixup;
    assert!(fixup.applies_to_session(&session, &FixupConfig::default()));
    let report = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(
        resolve_all(&interner, &report.warnings),
        vec!["attach_fo76_quest_variables:5109AF:quest_vmad_missing"]
    );
    assert_eq!(
        interner.resolve(report.message.unwrap()),
        Some(
            "fo76_quest_variables:quests=3;variables=6;globals_added=6;strings_rewritten=3;strings_moved=1;attached=2;present=0;unresolved=1"
        )
    );
    assert_eq!(report.records_added, 6);
    assert_eq!(report.records_changed, 3);

    let light_scripts = scripts(&mut session, fk(LIGHT, &interner));
    assert_eq!(light_scripts[0]["ScriptName"], "DefaultEventQuest");
    let (light_names, light_globals) = variable_rows(&light_scripts);
    assert_eq!(light_names, vec!["CurrentFluid", "MaxFluid"]);
    let light = subrecords(&mut session, fk(LIGHT, &interner));
    assert_eq!(
        u32_values(&light, "QTGL"),
        [
            own(LIGHT_TIMER),
            own(light_globals[0]),
            own(light_globals[1])
        ]
    );
    assert_eq!(
        signatures(&light)[..6],
        ["EDID", "VMAD", "QTGL", "QTGL", "QTGL", "NEXT"]
    );
    for (global, editor_id) in light_globals.iter().zip([
        "B21_QuestVar_187531_CurrentFluid",
        "B21_QuestVar_187531_MaxFluid",
    ]) {
        let global = session
            .record_decoded(&fk(*global, &interner), &schema, &interner)
            .unwrap();
        assert_eq!(global.sig, SigCode(*b"GLOB"));
        assert_eq!(
            global.eid.and_then(|eid| interner.resolve(eid)),
            Some(editor_id)
        );
    }
    assert_eq!(
        text(&session, "en", LAMP_TEXT),
        "Deposit Bioluminescent Fluids in lamp (<Global=B21_QuestVar_187531_CurrentFluid>/<Global=B21_QuestVar_187531_MaxFluid>)"
    );
    assert_eq!(
        text(&session, "de", LAMP_TEXT),
        "Fülle Biolumineszenz-Flüssigkeit in die Lampe. (<Global=B21_QuestVar_187531_CurrentFluid>/<Global=B21_QuestVar_187531_MaxFluid>)"
    );
    assert_eq!(
        text(&session, "de", LIGHT_LOG),
        "Die Lampe enthält <Global=B21_QuestVar_187531_CurrentFluid> Flüssigkeit."
    );
    assert_eq!(
        text(&session, "en", COMMUNE_TEXT),
        "(Optional) Commune with the Wise Mothman"
    );

    let feed = subrecords(&mut session, fk(FEED, &interner));
    assert_eq!(
        signatures(&feed),
        [
            "EDID", "VMAD", "QTGL", "QTGL", "NEXT", "INDX", "QOBJ", "FNAM", "NNAM"
        ]
    );
    assert_eq!(u32_values(&feed, "NNAM"), [DEPOSIT_TEXT]);
    let (feed_names, _) = variable_rows(&scripts(&mut session, fk(FEED, &interner)));
    assert_eq!(
        feed_names,
        vec!["ItemDepositsCompleted", "ItemDepositsToCompleteTotal"]
    );
    assert_eq!(
        text(&session, "en", DEPOSIT_TEXT),
        "Deposit food (<Global=B21_QuestVar_09210E_ItemDepositsCompleted>/<Global=B21_QuestVar_09210E_ItemDepositsToCompleteTotal>)"
    );

    // Habitat shared Feed's string id, so its rewrite moved to a fresh one.
    let habitat = subrecords(&mut session, fk(HABITAT, &interner));
    assert_eq!(u32_values(&habitat, "QTGL").len(), 2);
    assert_eq!(u32_values(&habitat, "NNAM"), [MOVED_DEPOSIT_TEXT]);
    assert_eq!(
        text(&session, "de", MOVED_DEPOSIT_TEXT),
        "Lege Essen ab (<Global=B21_QuestVar_5109AF_ItemDepositsCompleted>/<Global=B21_QuestVar_5109AF_ItemDepositsToCompleteTotal>)"
    );
    assert_eq!(
        session
            .target_slot()
            .strings_ref()
            .table_types
            .get(&MOVED_DEPOSIT_TEXT)
            .map(String::as_str),
        Some("strings")
    );

    let rerun = fixup
        .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
        .unwrap();
    assert_eq!(rerun.records_added, 0);
    assert_eq!(rerun.records_changed, 0);
    assert_eq!(
        interner.resolve(rerun.message.unwrap()),
        Some(
            "fo76_quest_variables:quests=3;variables=6;globals_added=0;strings_rewritten=0;strings_moved=0;attached=0;present=2;unresolved=1"
        )
    );
    assert_eq!(subrecords(&mut session, fk(LIGHT, &interner)), light);

    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(source));
}

#[test]
fn referenced_variables_read_variable_tags_and_own_rewritten_globals() {
    let (names, invalid) = referenced_variables(
        [
            "(<Variable=Toys_Current>/<Global=b21_questvar_65B0A8_Toys_Max>)",
            "<variable=toys_current> <Global=B21_QuestVar_000001_Other> <Variable=Bad Name>",
        ],
        0x65B0A8,
    );
    assert_eq!(names, vec!["Toys_Current", "Toys_Max"]);
    assert_eq!(invalid, vec!["Bad Name"]);
    assert_eq!(
        rewrite_variable_tags(
            "<Variable=Bad Name> <variable=TOYS_CURRENT>",
            0x65B0A8,
            &names
        )
        .as_deref(),
        Some("<Variable=Bad Name> <Global=B21_QuestVar_65B0A8_Toys_Current>")
    );
    assert_eq!(rewrite_variable_tags("no tags", 0x65B0A8, &names), None);
}
