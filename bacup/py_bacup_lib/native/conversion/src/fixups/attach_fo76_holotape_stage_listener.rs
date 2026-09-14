use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;

use crate::fixups::quest_script_vmad::{AttachResult, attach_script_bytes};
use crate::fixups::{FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::source_read::form_key_to_read_str;
use crate::sym::{StringInterner, Sym};
use crate::translator::Game;

const SOURCE_PLUGIN: &str = "SeventySix.esm";
const SCRIPT_NAME: &str = "B21:HolotapeStageOnPlay";
const GOVERNMENT_DROP_SCRIPT_NAME: &str = "GQ_DropGovt01HolotapeScript";
const GOVERNMENT_DROP_QUEST_LOCAL: u32 = 0x006F3C;
const GOVERNMENT_DROP_TAPE_LOCAL: u32 = 0x015B14;
const GOVERNMENT_DROP_FLAG_LOCAL: u32 = 0x006F41;
const GOVERNMENT_DROP_KEYWORD_LOCAL: u32 = 0x0072C0;
const GOVERNMENT_DROP_QUEST_EDITOR_ID: &str = "GQ_DropGovtIntro";
const GOVERNMENT_DROP_TAPE_EDITOR_ID: &str = "GQ_DropGovtHolotape";
const GOVERNMENT_DROP_FLAG_EDITOR_ID: &str = "GQ_DropGovt01Flag";
const GOVERNMENT_DROP_KEYWORD_EDITOR_ID: &str = "GQ_DropGovtIntroKeyword";
const QUEST_LOCAL: u32 = 0x73BAFC;
const TAPE_LOCAL: u32 = 0x73BB19;
const QUEST_EDITOR_ID: &str = "AC_SQ01_HellsEagles";
const TAPE_EDITOR_ID: &str = "AC_SQ01_HellsEagles_Investigation01_Holotape";
const PREREQ_STAGE: i32 = 200;
const STAGE_TO_SET: i32 = 300;
const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;

#[derive(Clone, Copy)]
struct ListenerSpec {
    slug: &'static str,
    quest_local: u32,
    tape_local: u32,
    quest_editor_id: &'static str,
    tape_editor_id: &'static str,
    prereq_stage: i32,
    stage_to_set: i32,
}

const LISTENER_SPECS: &[ListenerSpec] = &[
    ListenerSpec {
        slug: "ac_sq01_hells_eagles",
        quest_local: QUEST_LOCAL,
        tape_local: TAPE_LOCAL,
        quest_editor_id: QUEST_EDITOR_ID,
        tape_editor_id: TAPE_EDITOR_ID,
        prereq_stage: PREREQ_STAGE,
        stage_to_set: STAGE_TO_SET,
    },
    ListenerSpec {
        slug: "tw007_coldcase_otis_mail",
        quest_local: 0x2C460C,
        tape_local: 0x07D8BD,
        quest_editor_id: "TW007_ColdCase",
        tape_editor_id: "TW007_SecurityHolotape_OtisMail",
        prereq_stage: 300,
        stage_to_set: 375,
    },
    ListenerSpec {
        slug: "tw007_coldcase_tracker_removed",
        quest_local: 0x2C460C,
        tape_local: 0x07D8BE,
        quest_editor_id: "TW007_ColdCase",
        tape_editor_id: "TW007_SecurityHolotape_TrackerRemoved",
        prereq_stage: 300,
        stage_to_set: 350,
    },
];

pub fn apply(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    source: Game,
    target: Game,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    if (source, target) != (Game::Fo76, Game::Fo4) {
        return Ok(report);
    }

    let source_plugin = mapper.interner.intern(SOURCE_PLUGIN);
    for spec in LISTENER_SPECS {
        apply_listener_spec(session, mapper, source_plugin, *spec, &mut report)?;
    }
    apply_government_drop_sender(session, mapper, source_plugin, &mut report)?;
    Ok(report)
}

fn apply_government_drop_sender(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    source_plugin: Sym,
    report: &mut FixupReport,
) -> Result<(), FixupError> {
    let map = |local| {
        mapper.lookup(FormKey {
            local,
            plugin: source_plugin,
        })
    };
    let Some(target_quest) = map(GOVERNMENT_DROP_QUEST_LOCAL) else {
        warn_government_drop(report, mapper.interner, "quest_unmapped");
        return Ok(());
    };
    let Some(target_tape) = map(GOVERNMENT_DROP_TAPE_LOCAL) else {
        warn_government_drop(report, mapper.interner, "holotape_unmapped");
        return Ok(());
    };
    let Some(target_flag) = map(GOVERNMENT_DROP_FLAG_LOCAL) else {
        warn_government_drop(report, mapper.interner, "flag_unmapped");
        return Ok(());
    };
    let Some(target_keyword) = map(GOVERNMENT_DROP_KEYWORD_LOCAL) else {
        warn_government_drop(report, mapper.interner, "keyword_unmapped");
        return Ok(());
    };

    for (target, reason) in [
        (target_quest, "target_quest_missing"),
        (target_tape, "target_holotape_missing"),
        (target_flag, "target_flag_missing"),
        (target_keyword, "target_keyword_missing"),
    ] {
        if !target_record_exists(session, target, mapper.interner)? {
            warn_government_drop(report, mapper.interner, reason);
            return Ok(());
        }
    }

    let schema = session
        .schema()
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let quest = session
        .record_decoded(&target_quest, schema.as_ref(), mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let mut tape = session
        .record_decoded(&target_tape, schema.as_ref(), mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let flag = session
        .record_decoded(&target_flag, schema.as_ref(), mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let keyword = session
        .record_decoded(&target_keyword, schema.as_ref(), mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    if !record_matches(
        &quest,
        "QUST",
        GOVERNMENT_DROP_QUEST_EDITOR_ID,
        mapper.interner,
    ) || !record_matches(
        &tape,
        "NOTE",
        GOVERNMENT_DROP_TAPE_EDITOR_ID,
        mapper.interner,
    ) || !record_matches(
        &flag,
        "AVIF",
        GOVERNMENT_DROP_FLAG_EDITOR_ID,
        mapper.interner,
    ) || !record_matches(
        &keyword,
        "KYWD",
        GOVERNMENT_DROP_KEYWORD_EDITOR_ID,
        mapper.interner,
    ) {
        warn_government_drop(report, mapper.interner, "target_contract_mismatch");
        return Ok(());
    }

    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let Some(vmad) = government_drop_vmad(
        target_flag,
        target_keyword,
        session.target_masters(),
        &target_plugin,
        mapper.interner,
    ) else {
        return Err(FixupError::SchemaError(
            "government drop holotape sender VMAD encode failed".into(),
        ));
    };

    match attach_named_script(&mut tape, &vmad, GOVERNMENT_DROP_SCRIPT_NAME)? {
        AttachResult::Changed => {
            if session
                .replace_record_contents(tape, schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?
            {
                report.records_changed += 1;
                report.diagnostics.push(
                    mapper
                        .interner
                        .intern("attach_fo76_holotape_stage_listener:gq_drop_govt:attached"),
                );
            } else {
                warn_government_drop(report, mapper.interner, "target_replace_failed");
            }
        }
        AttachResult::AlreadyPresent => report.diagnostics.push(
            mapper
                .interner
                .intern("attach_fo76_holotape_stage_listener:gq_drop_govt:already_present"),
        ),
        AttachResult::Conflict(reason) => warn_government_drop(report, mapper.interner, reason),
    }
    Ok(())
}

fn apply_listener_spec(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    source_plugin: Sym,
    spec: ListenerSpec,
    report: &mut FixupReport,
) -> Result<(), FixupError> {
    let source_quest = FormKey {
        local: spec.quest_local,
        plugin: source_plugin,
    };
    let source_tape = FormKey {
        local: spec.tape_local,
        plugin: source_plugin,
    };
    let Some(target_quest) = mapper.lookup(source_quest) else {
        warn(report, mapper.interner, spec.slug, "quest_unmapped");
        return Ok(());
    };
    let Some(target_tape) = mapper.lookup(source_tape) else {
        warn(report, mapper.interner, spec.slug, "holotape_unmapped");
        return Ok(());
    };
    if !target_record_exists(session, target_quest, mapper.interner)? {
        warn(report, mapper.interner, spec.slug, "target_quest_missing");
        return Ok(());
    }
    if !target_record_exists(session, target_tape, mapper.interner)? {
        warn(
            report,
            mapper.interner,
            spec.slug,
            "target_holotape_missing",
        );
        return Ok(());
    }

    let schema = session
        .schema()
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let quest = session
        .record_decoded(&target_quest, schema.as_ref(), mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let mut tape = session
        .record_decoded(&target_tape, schema.as_ref(), mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    if !record_matches(&quest, "QUST", spec.quest_editor_id, mapper.interner)
        || !record_matches(&tape, "NOTE", spec.tape_editor_id, mapper.interner)
    {
        warn(
            report,
            mapper.interner,
            spec.slug,
            "target_contract_mismatch",
        );
        return Ok(());
    }

    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let vmad = listener_vmad(
        target_quest,
        session.target_masters(),
        &target_plugin,
        mapper.interner,
        spec.prereq_stage,
        spec.stage_to_set,
    )
    .ok_or_else(|| FixupError::SchemaError("holotape listener VMAD encode failed".into()))?;

    match attach_listener(&mut tape, &vmad)? {
        AttachResult::Changed => {
            if session
                .replace_record_contents(tape, schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?
            {
                report.records_changed += 1;
                report.diagnostics.push(mapper.interner.intern(&format!(
                    "attach_fo76_holotape_stage_listener:{}:attached",
                    spec.slug
                )));
            } else {
                warn(report, mapper.interner, spec.slug, "target_replace_failed");
            }
        }
        AttachResult::AlreadyPresent => {
            report.diagnostics.push(mapper.interner.intern(&format!(
                "attach_fo76_holotape_stage_listener:{}:already_present",
                spec.slug
            )));
        }
        AttachResult::Conflict(reason) => warn(report, mapper.interner, spec.slug, reason),
    }
    Ok(())
}

fn target_record_exists(
    session: &mut PluginSession,
    target: FormKey,
    interner: &StringInterner,
) -> Result<bool, FixupError> {
    let target_id = session.target_id();
    let target_key = form_key_to_read_str(&target, interner);
    session
        .record_exists_in_handle(target_id, &target_key)
        .map_err(|error| FixupError::HandleError(error.to_string()))
}

fn record_matches(
    record: &Record,
    signature: &str,
    editor_id: &str,
    interner: &StringInterner,
) -> bool {
    SigCode::from_str(signature).is_ok_and(|expected| {
        record.sig == expected
            && record
                .eid
                .and_then(|eid| interner.resolve(eid))
                .is_some_and(|actual| actual.eq_ignore_ascii_case(editor_id))
    })
}

fn listener_vmad(
    target_quest: FormKey,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
    prereq_stage: i32,
    stage_to_set: i32,
) -> Option<Vec<u8>> {
    let quest_plugin = interner.resolve(target_quest.plugin)?;
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": SCRIPT_NAME,
                "Flags": 0,
                "Properties": [
                    {
                        "propertyName": "TargetQuest",
                        "Type": "Object",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": {
                            "Alias": -1,
                            "FormID": {"reference": {
                                "plugin": quest_plugin,
                                "object_id": format!("{:06X}", target_quest.local),
                            }},
                        },
                    },
                    {"propertyName": "PrereqStage", "Type": "Int32", "Flags": VMAD_PROPERTY_FLAG_EDITED, "Value": prereq_stage},
                    {"propertyName": "StageToSet", "Type": "Int32", "Flags": VMAD_PROPERTY_FLAG_EDITED, "Value": stage_to_set},
                ],
            }],
        }),
        target_masters,
        target_plugin,
    )
}

fn government_drop_vmad(
    target_flag: FormKey,
    target_keyword: FormKey,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let flag_plugin = interner.resolve(target_flag.plugin)?;
    let keyword_plugin = interner.resolve(target_keyword.plugin)?;
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": GOVERNMENT_DROP_SCRIPT_NAME,
                "Flags": 0,
                "Properties": [
                    {
                        "propertyName": "GQ_DropGovt01Flag",
                        "Type": "Object",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": {
                            "Alias": -1,
                            "FormID": {"reference": {
                                "plugin": flag_plugin,
                                "object_id": format!("{:06X}", target_flag.local),
                            }},
                        },
                    },
                    {
                        "propertyName": "GQ_DropGovtIntroKeyword",
                        "Type": "Object",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": {
                            "Alias": -1,
                            "FormID": {"reference": {
                                "plugin": keyword_plugin,
                                "object_id": format!("{:06X}", target_keyword.local),
                            }},
                        },
                    },
                ],
            }],
        }),
        target_masters,
        target_plugin,
    )
}

fn attach_listener(record: &mut Record, vmad: &[u8]) -> Result<AttachResult, FixupError> {
    attach_named_script(record, vmad, SCRIPT_NAME)
}

fn attach_named_script(
    record: &mut Record,
    vmad: &[u8],
    script_name: &str,
) -> Result<AttachResult, FixupError> {
    let vmad_sig = SubrecordSig::from_str("VMAD")
        .map_err(|error| FixupError::SchemaError(error.to_string()))?;
    for field in &mut record.fields {
        if field.sig != vmad_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut field.value else {
            return Ok(AttachResult::Conflict("existing_vmad_not_bytes"));
        };
        let mut patched_vmad = bytes.to_vec();
        let result = attach_script_bytes(&mut patched_vmad, script_name, vmad);
        if result == AttachResult::Changed {
            *bytes = patched_vmad.into();
        }
        return Ok(result);
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: vmad_sig,
            value: FieldValue::Bytes(vmad.into()),
        },
    );
    Ok(AttachResult::Changed)
}

fn warn(report: &mut FixupReport, interner: &StringInterner, slug: &str, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "attach_fo76_holotape_stage_listener:{slug}:{reason}"
    )));
}

fn warn_government_drop(report: &mut FixupReport, interner: &StringInterner, reason: &str) {
    warn(report, interner, "gq_drop_govt", reason);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::record::RecordFlags;
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_new_native,
    };

    fn target_record(
        signature: &str,
        local: u32,
        editor_id: &str,
        target_plugin: &str,
        interner: &StringInterner,
    ) -> Record {
        let editor_id = interner.intern(editor_id);
        Record {
            sig: SigCode::from_str(signature).expect("record signature"),
            form_key: FormKey {
                local,
                plugin: interner.intern(target_plugin),
            },
            eid: Some(editor_id),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("EDID").expect("EDID signature"),
                value: FieldValue::String(editor_id),
            }],
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn target_handle(target_plugin: &str, records: Vec<Record>, interner: &StringInterner) -> u64 {
        let handle = plugin_handle_new_native(target_plugin, Some("fo4")).expect("target handle");
        {
            let mut session = open_session(handle, None).expect("target session");
            let schema = session.schema().expect("FO4 schema");
            session
                .add_records(records, schema.as_ref(), interner)
                .expect("seed target records");
        }
        handle
    }

    fn mapped_holotape_fixture<'a>(
        target_plugin: &str,
        interner: &'a StringInterner,
    ) -> FormKeyMapper<'a> {
        mapped_listener_specs_fixture(target_plugin, interner, &LISTENER_SPECS[..1])
    }

    fn mapped_listener_specs_fixture<'a>(
        target_plugin: &str,
        interner: &'a StringInterner,
        specs: &[ListenerSpec],
    ) -> FormKeyMapper<'a> {
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: target_plugin.to_string(),
                source_plugin_name: SOURCE_PLUGIN.to_string(),
                ..MapperOptions::default()
            },
            interner,
        );
        let source_plugin = interner.intern(SOURCE_PLUGIN);
        let target_plugin = interner.intern(target_plugin);
        for spec in specs {
            mapper.add_mapping(
                FormKey {
                    local: spec.quest_local,
                    plugin: source_plugin,
                },
                FormKey {
                    local: spec.quest_local,
                    plugin: target_plugin,
                },
            );
            mapper.add_mapping(
                FormKey {
                    local: spec.tape_local,
                    plugin: source_plugin,
                },
                FormKey {
                    local: spec.tape_local,
                    plugin: target_plugin,
                },
            );
        }
        mapper
    }

    fn mapped_government_drop_fixture<'a>(
        target_plugin: &str,
        interner: &'a StringInterner,
    ) -> FormKeyMapper<'a> {
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: target_plugin.to_string(),
                source_plugin_name: SOURCE_PLUGIN.to_string(),
                ..MapperOptions::default()
            },
            interner,
        );
        let source_plugin = interner.intern(SOURCE_PLUGIN);
        let target_plugin = interner.intern(target_plugin);
        for local in [
            GOVERNMENT_DROP_QUEST_LOCAL,
            GOVERNMENT_DROP_TAPE_LOCAL,
            GOVERNMENT_DROP_FLAG_LOCAL,
            GOVERNMENT_DROP_KEYWORD_LOCAL,
        ] {
            mapper.add_mapping(
                FormKey {
                    local,
                    plugin: source_plugin,
                },
                FormKey {
                    local,
                    plugin: target_plugin,
                },
            );
        }
        mapper
    }

    fn has_message(symbols: &[Sym], expected: &str, interner: &StringInterner) -> bool {
        symbols
            .iter()
            .any(|symbol| interner.resolve(*symbol) == Some(expected))
    }

    fn record_vmad(record: &Record) -> Option<&[u8]> {
        record.fields.iter().find_map(|field| {
            if field.sig.as_str() == "VMAD" {
                match &field.value {
                    FieldValue::Bytes(bytes) => Some(bytes.as_ref()),
                    _ => None,
                }
            } else {
                None
            }
        })
    }

    #[test]
    fn listener_vmad_names_the_exact_script_quest_and_stages() {
        let interner = StringInterner::new();
        let quest = FormKey {
            local: QUEST_LOCAL,
            plugin: interner.intern(SOURCE_PLUGIN),
        };
        let vmad = listener_vmad(
            quest,
            &[],
            SOURCE_PLUGIN,
            &interner,
            PREREQ_STAGE,
            STAGE_TO_SET,
        )
        .expect("VMAD");
        let text = String::from_utf8_lossy(&vmad);

        assert!(text.contains(SCRIPT_NAME));
        assert!(text.contains("TargetQuest"));
        assert!(text.contains("PrereqStage"));
        assert!(text.contains("StageToSet"));
    }

    #[test]
    fn listener_attachment_is_idempotent_on_the_holotape_record() {
        let interner = StringInterner::new();
        let plugin = interner.intern(SOURCE_PLUGIN);
        let quest = FormKey {
            local: QUEST_LOCAL,
            plugin,
        };
        let mut tape = Record::new(
            SigCode::from_str("NOTE").expect("NOTE signature"),
            FormKey {
                local: TAPE_LOCAL,
                plugin,
            },
        );
        let vmad = listener_vmad(
            quest,
            &[],
            SOURCE_PLUGIN,
            &interner,
            PREREQ_STAGE,
            STAGE_TO_SET,
        )
        .expect("VMAD");

        assert_eq!(
            attach_listener(&mut tape, &vmad).expect("attach"),
            AttachResult::Changed
        );
        assert_eq!(
            attach_listener(&mut tape, &vmad).expect("reattach"),
            AttachResult::AlreadyPresent
        );
        assert_eq!(
            tape.fields
                .iter()
                .filter(|field| field.sig.0 == *b"VMAD")
                .count(),
            1
        );
    }

    #[test]
    fn government_drop_vmad_names_the_source_script_and_exact_properties() {
        let interner = StringInterner::new();
        let plugin = interner.intern(SOURCE_PLUGIN);
        let vmad = government_drop_vmad(
            FormKey {
                local: GOVERNMENT_DROP_FLAG_LOCAL,
                plugin,
            },
            FormKey {
                local: GOVERNMENT_DROP_KEYWORD_LOCAL,
                plugin,
            },
            &[],
            SOURCE_PLUGIN,
            &interner,
        )
        .expect("VMAD");
        let text = String::from_utf8_lossy(&vmad);

        assert!(text.contains(GOVERNMENT_DROP_SCRIPT_NAME));
        assert!(text.contains("GQ_DropGovt01Flag"));
        assert!(text.contains("GQ_DropGovtIntroKeyword"));
    }

    #[test]
    fn apply_attaches_the_exact_government_drop_sender_once() {
        let interner = StringInterner::new();
        let target_plugin = "GovernmentDropHappyPath.esp";
        let target_plugin_sym = interner.intern(target_plugin);
        let records = vec![
            target_record(
                "QUST",
                GOVERNMENT_DROP_QUEST_LOCAL,
                GOVERNMENT_DROP_QUEST_EDITOR_ID,
                target_plugin,
                &interner,
            ),
            target_record(
                "NOTE",
                GOVERNMENT_DROP_TAPE_LOCAL,
                GOVERNMENT_DROP_TAPE_EDITOR_ID,
                target_plugin,
                &interner,
            ),
            target_record(
                "AVIF",
                GOVERNMENT_DROP_FLAG_LOCAL,
                GOVERNMENT_DROP_FLAG_EDITOR_ID,
                target_plugin,
                &interner,
            ),
            target_record(
                "KYWD",
                GOVERNMENT_DROP_KEYWORD_LOCAL,
                GOVERNMENT_DROP_KEYWORD_EDITOR_ID,
                target_plugin,
                &interner,
            ),
        ];
        let handle = target_handle(target_plugin, records, &interner);
        let mut mapper = mapped_government_drop_fixture(target_plugin, &interner);

        let (first, second) = {
            let mut session = open_session(handle, None).expect("target session");
            let first =
                apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("first apply");
            let second =
                apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("second apply");
            let schema = session.schema().expect("FO4 schema");
            let tape = session
                .record_decoded(
                    &FormKey {
                        local: GOVERNMENT_DROP_TAPE_LOCAL,
                        plugin: target_plugin_sym,
                    },
                    schema.as_ref(),
                    &interner,
                )
                .expect("read target tape");
            let expected = government_drop_vmad(
                FormKey {
                    local: GOVERNMENT_DROP_FLAG_LOCAL,
                    plugin: target_plugin_sym,
                },
                FormKey {
                    local: GOVERNMENT_DROP_KEYWORD_LOCAL,
                    plugin: target_plugin_sym,
                },
                &[],
                target_plugin,
                &interner,
            )
            .expect("expected VMAD");
            assert_eq!(record_vmad(&tape), Some(expected.as_slice()));
            (first, second)
        };

        assert_eq!(first.records_changed, 1);
        assert_eq!(second.records_changed, 0);
        assert!(has_message(
            &second.diagnostics,
            "attach_fo76_holotape_stage_listener:gq_drop_govt:already_present",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn government_drop_sender_rejects_a_wrong_selector_identity() {
        let interner = StringInterner::new();
        let target_plugin = "GovernmentDropWrongSelector.esp";
        let records = vec![
            target_record(
                "QUST",
                GOVERNMENT_DROP_QUEST_LOCAL,
                GOVERNMENT_DROP_QUEST_EDITOR_ID,
                target_plugin,
                &interner,
            ),
            target_record(
                "NOTE",
                GOVERNMENT_DROP_TAPE_LOCAL,
                GOVERNMENT_DROP_TAPE_EDITOR_ID,
                target_plugin,
                &interner,
            ),
            target_record(
                "AVIF",
                GOVERNMENT_DROP_FLAG_LOCAL,
                GOVERNMENT_DROP_FLAG_EDITOR_ID,
                target_plugin,
                &interner,
            ),
            target_record(
                "KYWD",
                GOVERNMENT_DROP_KEYWORD_LOCAL,
                "WrongSelector",
                target_plugin,
                &interner,
            ),
        ];
        let handle = target_handle(target_plugin, records, &interner);
        let mut mapper = mapped_government_drop_fixture(target_plugin, &interner);

        let report = {
            let mut session = open_session(handle, None).expect("target session");
            apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("identity apply")
        };

        assert_eq!(report.records_changed, 0);
        assert!(has_message(
            &report.warnings,
            "attach_fo76_holotape_stage_listener:gq_drop_govt:target_contract_mismatch",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn apply_is_fail_soft_when_mapped_quest_record_is_missing() {
        let interner = StringInterner::new();
        let target_plugin = "HellsMissingQuest.esp";
        let tape = target_record("NOTE", TAPE_LOCAL, TAPE_EDITOR_ID, target_plugin, &interner);
        let handle = target_handle(target_plugin, vec![tape], &interner);
        let mut mapper = mapped_holotape_fixture(target_plugin, &interner);

        let report = {
            let mut session = open_session(handle, None).expect("target session");
            apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("fail-soft apply")
        };

        assert_eq!(report.records_changed, 0);
        assert!(has_message(
            &report.warnings,
            "attach_fo76_holotape_stage_listener:ac_sq01_hells_eagles:target_quest_missing",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn apply_is_fail_soft_when_mapped_holotape_record_is_missing() {
        let interner = StringInterner::new();
        let target_plugin = "HellsMissingTape.esp";
        let quest = target_record(
            "QUST",
            QUEST_LOCAL,
            QUEST_EDITOR_ID,
            target_plugin,
            &interner,
        );
        let handle = target_handle(target_plugin, vec![quest], &interner);
        let mut mapper = mapped_holotape_fixture(target_plugin, &interner);

        let report = {
            let mut session = open_session(handle, None).expect("target session");
            apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("fail-soft apply")
        };

        assert_eq!(report.records_changed, 0);
        assert!(has_message(
            &report.warnings,
            "attach_fo76_holotape_stage_listener:ac_sq01_hells_eagles:target_holotape_missing",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn apply_rejects_wrong_target_record_identity() {
        let interner = StringInterner::new();
        let target_plugin = "HellsWrongIdentity.esp";
        let quest = target_record(
            "QUST",
            QUEST_LOCAL,
            "DifferentQuest",
            target_plugin,
            &interner,
        );
        let tape = target_record("NOTE", TAPE_LOCAL, TAPE_EDITOR_ID, target_plugin, &interner);
        let handle = target_handle(target_plugin, vec![quest, tape], &interner);
        let mut mapper = mapped_holotape_fixture(target_plugin, &interner);

        let report = {
            let mut session = open_session(handle, None).expect("target session");
            apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("identity apply")
        };

        assert_eq!(report.records_changed, 0);
        assert!(has_message(
            &report.warnings,
            "attach_fo76_holotape_stage_listener:ac_sq01_hells_eagles:target_contract_mismatch",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn apply_reports_conflicting_binding_without_mutation() {
        let interner = StringInterner::new();
        let target_plugin = "HellsConflict.esp";
        let target_plugin_sym = interner.intern(target_plugin);
        let quest = target_record(
            "QUST",
            QUEST_LOCAL,
            QUEST_EDITOR_ID,
            target_plugin,
            &interner,
        );
        let mut tape = target_record("NOTE", TAPE_LOCAL, TAPE_EDITOR_ID, target_plugin, &interner);
        let conflicting_vmad = listener_vmad(
            FormKey {
                local: QUEST_LOCAL + 1,
                plugin: target_plugin_sym,
            },
            &[],
            target_plugin,
            &interner,
            PREREQ_STAGE,
            STAGE_TO_SET,
        )
        .expect("conflicting VMAD");
        tape.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig::from_str("VMAD").expect("VMAD signature"),
                value: FieldValue::Bytes(conflicting_vmad.clone().into()),
            },
        );
        let handle = target_handle(target_plugin, vec![quest, tape], &interner);
        let mut mapper = mapped_holotape_fixture(target_plugin, &interner);

        let report = {
            let mut session = open_session(handle, None).expect("target session");
            let report =
                apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("conflict apply");
            let schema = session.schema().expect("FO4 schema");
            let tape = session
                .record_decoded(
                    &FormKey {
                        local: TAPE_LOCAL,
                        plugin: target_plugin_sym,
                    },
                    schema.as_ref(),
                    &interner,
                )
                .expect("read target tape");
            assert_eq!(record_vmad(&tape), Some(conflicting_vmad.as_slice()));
            report
        };

        assert_eq!(report.records_changed, 0);
        assert!(has_message(
            &report.warnings,
            "attach_fo76_holotape_stage_listener:ac_sq01_hells_eagles:same_script_different_binding",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn apply_attaches_once_and_is_idempotent() {
        let interner = StringInterner::new();
        let target_plugin = "HellsHappyPath.esp";
        let quest = target_record(
            "QUST",
            QUEST_LOCAL,
            QUEST_EDITOR_ID,
            target_plugin,
            &interner,
        );
        let tape = target_record("NOTE", TAPE_LOCAL, TAPE_EDITOR_ID, target_plugin, &interner);
        let handle = target_handle(target_plugin, vec![quest, tape], &interner);
        let mut mapper = mapped_holotape_fixture(target_plugin, &interner);

        let (first, second) = {
            let mut session = open_session(handle, None).expect("target session");
            let first =
                apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("first apply");
            let second =
                apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("second apply");
            (first, second)
        };

        assert_eq!(first.records_changed, 1);
        assert_eq!(second.records_changed, 0);
        assert!(has_message(
            &second.diagnostics,
            "attach_fo76_holotape_stage_listener:ac_sq01_hells_eagles:already_present",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn apply_attaches_both_exact_tw007_holotape_listeners() {
        let interner = StringInterner::new();
        let target_plugin = "ColdCaseHappyPath.esp";
        let target_plugin_sym = interner.intern(target_plugin);
        let specs = &LISTENER_SPECS[1..];
        let quest = target_record(
            "QUST",
            specs[0].quest_local,
            specs[0].quest_editor_id,
            target_plugin,
            &interner,
        );
        let first_tape = target_record(
            "NOTE",
            specs[0].tape_local,
            specs[0].tape_editor_id,
            target_plugin,
            &interner,
        );
        let second_tape = target_record(
            "NOTE",
            specs[1].tape_local,
            specs[1].tape_editor_id,
            target_plugin,
            &interner,
        );
        let handle = target_handle(
            target_plugin,
            vec![quest, first_tape, second_tape],
            &interner,
        );
        let mut mapper = mapped_listener_specs_fixture(target_plugin, &interner, specs);

        let (first, second) = {
            let mut session = open_session(handle, None).expect("target session");
            let first =
                apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("first apply");
            let second =
                apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4).expect("second apply");
            let schema = session.schema().expect("FO4 schema");
            for spec in specs {
                let tape = session
                    .record_decoded(
                        &FormKey {
                            local: spec.tape_local,
                            plugin: target_plugin_sym,
                        },
                        schema.as_ref(),
                        &interner,
                    )
                    .expect("read target tape");
                let expected = listener_vmad(
                    FormKey {
                        local: spec.quest_local,
                        plugin: target_plugin_sym,
                    },
                    &[],
                    target_plugin,
                    &interner,
                    spec.prereq_stage,
                    spec.stage_to_set,
                )
                .expect("expected listener VMAD");
                assert_eq!(record_vmad(&tape), Some(expected.as_slice()));
            }
            (first, second)
        };

        assert_eq!(first.records_changed, 2);
        assert_eq!(second.records_changed, 0);
        for spec in specs {
            assert!(has_message(
                &second.diagnostics,
                &format!(
                    "attach_fo76_holotape_stage_listener:{}:already_present",
                    spec.slug
                ),
                &interner,
            ));
        }
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn apply_rejects_a_wrong_source_local_id() {
        let interner = StringInterner::new();
        let target_plugin = "HellsWrongSourceId.esp";
        let target_plugin_sym = interner.intern(target_plugin);
        let quest = target_record(
            "QUST",
            QUEST_LOCAL,
            QUEST_EDITOR_ID,
            target_plugin,
            &interner,
        );
        let tape = target_record("NOTE", TAPE_LOCAL, TAPE_EDITOR_ID, target_plugin, &interner);
        let handle = target_handle(target_plugin, vec![quest, tape], &interner);
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: target_plugin.to_string(),
                source_plugin_name: SOURCE_PLUGIN.to_string(),
                ..MapperOptions::default()
            },
            &interner,
        );
        let source_plugin = interner.intern(SOURCE_PLUGIN);
        mapper.add_mapping(
            FormKey {
                local: QUEST_LOCAL,
                plugin: source_plugin,
            },
            FormKey {
                local: QUEST_LOCAL,
                plugin: target_plugin_sym,
            },
        );
        mapper.add_mapping(
            FormKey {
                local: TAPE_LOCAL + 1,
                plugin: source_plugin,
            },
            FormKey {
                local: TAPE_LOCAL,
                plugin: target_plugin_sym,
            },
        );

        let report = {
            let mut session = open_session(handle, None).expect("target session");
            let report = apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4)
                .expect("wrong source ID apply");
            let schema = session.schema().expect("FO4 schema");
            let tape = session
                .record_decoded(
                    &FormKey {
                        local: TAPE_LOCAL,
                        plugin: target_plugin_sym,
                    },
                    schema.as_ref(),
                    &interner,
                )
                .expect("read target tape");
            assert_eq!(record_vmad(&tape), None);
            report
        };

        assert_eq!(report.records_changed, 0);
        assert!(has_message(
            &report.warnings,
            "attach_fo76_holotape_stage_listener:ac_sq01_hells_eagles:holotape_unmapped",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn apply_rejects_exact_ids_mapped_from_a_foreign_source_plugin() {
        let interner = StringInterner::new();
        let target_plugin = "HellsWrongSourcePlugin.esp";
        let target_plugin_sym = interner.intern(target_plugin);
        let quest = target_record(
            "QUST",
            QUEST_LOCAL,
            QUEST_EDITOR_ID,
            target_plugin,
            &interner,
        );
        let tape = target_record("NOTE", TAPE_LOCAL, TAPE_EDITOR_ID, target_plugin, &interner);
        let handle = target_handle(target_plugin, vec![quest, tape], &interner);
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: target_plugin.to_string(),
                source_plugin_name: SOURCE_PLUGIN.to_string(),
                ..MapperOptions::default()
            },
            &interner,
        );
        let foreign_plugin = interner.intern("Foreign.esm");
        for local in [QUEST_LOCAL, TAPE_LOCAL] {
            mapper.add_mapping(
                FormKey {
                    local,
                    plugin: foreign_plugin,
                },
                FormKey {
                    local,
                    plugin: target_plugin_sym,
                },
            );
        }

        let report = {
            let mut session = open_session(handle, None).expect("target session");
            let report = apply(&mut session, &mut mapper, Game::Fo76, Game::Fo4)
                .expect("wrong source plugin apply");
            let schema = session.schema().expect("FO4 schema");
            let tape = session
                .record_decoded(
                    &FormKey {
                        local: TAPE_LOCAL,
                        plugin: target_plugin_sym,
                    },
                    schema.as_ref(),
                    &interner,
                )
                .expect("read target tape");
            assert_eq!(record_vmad(&tape), None);
            report
        };

        assert_eq!(report.records_changed, 0);
        assert!(has_message(
            &report.warnings,
            "attach_fo76_holotape_stage_listener:ac_sq01_hells_eagles:quest_unmapped",
            &interner,
        ));
        assert!(plugin_handle_close_native(handle));
    }
}
