//! Carry the FO76 quest start keyword onto the converted quest.
//!
//! FO76 `QUST` records name the keyword that offers the quest in `QSSK`, and
//! the server's content manager sent that keyword. FO4 has no such field and no
//! such sender, so the Story Manager node gated on it can never fire. Most
//! region dailies pair that initial node with a repeat node gated on
//! `GetQuestCompleted == 1`, which makes the whole quest unreachable: the first
//! run is the only way to satisfy the repeat gate.
//!
//! The keyword is attached as `B21:QuestStartKeyword` so a scheduler can send it
//! for a quest that has never been completed. Quests whose keyword did not
//! survive conversion are skipped rather than bound to a null property.

use esp_authoring_core::plugin_runtime::ParsedSubrecord;
use rustc_hash::FxHashSet;

use crate::fixups::quest_script_binding::{
    SourcePlugin, attach_quest_script, emitted_target_form, object_property, script_vmad,
};
use crate::fixups::quest_script_vmad::AttachResult;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const START_KEYWORD_SCRIPT: &str = "B21:QuestStartKeyword";

pub struct AttachFo76QuestStartKeywordFixup;

impl Fixup for AttachFo76QuestStartKeywordFixup {
    fn name(&self) -> &'static str {
        "attach_fo76_quest_start_keyword"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let Some(source) = SourcePlugin::of(session, mapper.interner) else {
            return Ok(report);
        };
        let source_quests = {
            let scan = session.handle_raw_scan(source.id).map_err(handle_error)?;
            source
                .own_raw_form_ids(&scan, SigCode(*b"QUST"))
                .into_iter()
                .filter_map(|raw_form_id| {
                    let quest = source.form_key(raw_form_id, mapper.interner)?;
                    let keyword = scan.with_record_subrecords(raw_form_id, |subrecords| {
                        parse_start_keyword(subrecords, &source, mapper.interner)
                    })??;
                    Some((quest, keyword))
                })
                .collect::<Vec<_>>()
        };

        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let target_quests = session
            .form_keys_of_sig(SigCode(*b"QUST"), mapper.interner)
            .map_err(handle_error)?
            .into_iter()
            .collect::<FxHashSet<_>>();

        let mut attached = 0u32;
        let mut present = 0u32;
        let mut unresolved = 0u32;
        let mut keyword_missing = 0u32;
        for (source_quest, source_keyword) in source_quests {
            let quest_local = source_quest.local;
            let Some(target_quest) = mapper
                .lookup(source_quest)
                .filter(|target| target_quests.contains(target))
            else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest_local,
                    "target_quest_missing",
                );
                continue;
            };
            let Some(keyword) = emitted_target_form(session, mapper, source_keyword)? else {
                keyword_missing += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest_local,
                    "start_keyword_missing",
                );
                continue;
            };
            let vmad = script_vmad(
                START_KEYWORD_SCRIPT,
                vec![
                    object_property("StartKeyword", Some(keyword), mapper.interner).ok_or_else(
                        || FixupError::HandleError("start keyword property encode".into()),
                    )?,
                ],
                &target_masters,
                &target_plugin,
            );
            let Some(vmad) = vmad else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest_local,
                    "vmad_encode_failed",
                );
                continue;
            };
            match attach_quest_script(session, &target_quest, START_KEYWORD_SCRIPT, &vmad)? {
                Some(AttachResult::Changed) => {
                    report.records_changed += 1;
                    attached += 1;
                }
                Some(AttachResult::AlreadyPresent) => present += 1,
                Some(AttachResult::Conflict(reason)) => {
                    unresolved += 1;
                    warn(&mut report, mapper.interner, quest_local, reason);
                }
                None => {
                    unresolved += 1;
                    warn(
                        &mut report,
                        mapper.interner,
                        quest_local,
                        "quest_vmad_missing",
                    );
                }
            }
        }

        report.message = Some(mapper.interner.intern(&format!(
            "fo76_quest_start_keyword:attached={attached};present={present};unresolved={unresolved};keyword_missing={keyword_missing}"
        )));
        Ok(report)
    }
}

fn parse_start_keyword(
    subrecords: &[ParsedSubrecord],
    source: &SourcePlugin,
    interner: &StringInterner,
) -> Option<FormKey> {
    subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "QSSK")
        .and_then(|subrecord| source.form_key_at(subrecord.data.as_ref(), 0, interner))
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn warn(report: &mut FixupReport, interner: &StringInterner, source_quest: u32, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "attach_fo76_quest_start_keyword:{source_quest:06X}:{reason}"
    )));
}

#[cfg(test)]
mod tests;
