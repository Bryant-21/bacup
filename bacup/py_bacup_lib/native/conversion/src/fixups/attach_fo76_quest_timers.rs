//! Carry FO76 quest and objective timers onto the converted quests.
//!
//! FO76 timers are record data FO4 has no field for: `QTLM` names the quest
//! timer's length global, stage `INDX` flags mark the stages that start it
//! (`StartTimer`) and run when it expires (`TimerEnd`), and objectives flagged
//! `UsesTimer` carry their own `QOTM` length global and `SNAM` expiry stage.
//! They are attached as `B21:QuestTimer` and `B21:ObjectiveTimers` properties.

use esp_authoring_core::plugin_runtime::ParsedSubrecord;
use rustc_hash::FxHashSet;

use crate::fixups::quest_script_binding::{
    SourcePlugin, attach_quest_script, emitted_target_form, int_array_property,
    object_array_property, object_property, script_vmad,
};
use crate::fixups::quest_script_vmad::AttachResult;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const QUEST_TIMER_SCRIPT: &str = "B21:QuestTimer";
const OBJECTIVE_TIMERS_SCRIPT: &str = "B21:ObjectiveTimers";
const STAGE_FLAG_TIMER_END: u8 = 0x10;
const STAGE_FLAG_START_TIMER: u8 = 0x20;
const OBJECTIVE_FLAG_USES_TIMER: u32 = 0x8;
const NO_EXPIRY_STAGE: i32 = -1;
const OBJECTIVE_SCOPE_END_SIGS: &[&str] = &["QOBJ", "ANAM", "ALST", "ALLS", "ALCS"];

pub struct AttachFo76QuestTimersFixup;

#[derive(Debug, Default, PartialEq, Eq)]
struct SourceTimers {
    quest_timer: Option<FormKey>,
    stages: Vec<i32>,
    start_timer_stages: Vec<i32>,
    timer_end_stages: Vec<i32>,
    objectives: Vec<SourceObjectiveTimer>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SourceObjectiveTimer {
    objective: i32,
    uses_timer: bool,
    timer: Option<FormKey>,
    expiry_stage: Option<i32>,
}

impl SourceTimers {
    fn has_quest_timer(&self) -> bool {
        self.quest_timer.is_some()
            || !self.start_timer_stages.is_empty()
            || !self.timer_end_stages.is_empty()
    }

    fn timed_objectives(&self) -> impl Iterator<Item = &SourceObjectiveTimer> {
        self.objectives
            .iter()
            .filter(|objective| objective.uses_timer)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ObjectiveTimerRow {
    objective: i32,
    timer: FormKey,
    expiry_stage: i32,
}

impl Fixup for AttachFo76QuestTimersFixup {
    fn name(&self) -> &'static str {
        "attach_fo76_quest_timers"
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
                    let timers = scan.with_record_subrecords(raw_form_id, |subrecords| {
                        parse_source_timers(subrecords, &source, mapper.interner)
                    })?;
                    (timers.has_quest_timer() || timers.timed_objectives().next().is_some())
                        .then_some((quest, timers))
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
        let mut skipped_objectives = 0u32;
        for (source_quest, timers) in source_quests {
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

            let mut bindings = Vec::new();
            if timers.has_quest_timer() {
                let timer_length = match timers.quest_timer {
                    Some(timer) => {
                        let emitted = emitted_target_form(session, mapper, timer)?;
                        if emitted.is_none() {
                            warn(
                                &mut report,
                                mapper.interner,
                                quest_local,
                                "quest_timer_global_missing",
                            );
                        }
                        emitted
                    }
                    None => None,
                };
                bindings.push((
                    QUEST_TIMER_SCRIPT,
                    quest_timer_vmad(
                        timer_length,
                        &timers,
                        &target_masters,
                        &target_plugin,
                        mapper.interner,
                    ),
                ));
            }

            let mut rows = Vec::new();
            for objective in timers.timed_objectives() {
                // FO76 writes SNAM 65535 for a countdown that sets no stage
                // (MTR05 objective 60, MTR10_Battle objective 182). The row still
                // owns the countdown the objective reads, so only a missing length
                // global makes it unusable.
                let expiry_stage = objective
                    .expiry_stage
                    .filter(|stage| timers.stages.contains(stage))
                    .unwrap_or(NO_EXPIRY_STAGE);
                let timer = match objective.timer {
                    Some(timer) => emitted_target_form(session, mapper, timer)?,
                    None => None,
                };
                let Some(timer) = timer else {
                    skipped_objectives += 1;
                    warn(
                        &mut report,
                        mapper.interner,
                        quest_local,
                        &format!("objective_timer_skipped={}", objective.objective),
                    );
                    continue;
                };
                rows.push(ObjectiveTimerRow {
                    objective: objective.objective,
                    timer,
                    expiry_stage,
                });
            }
            if !rows.is_empty() {
                bindings.push((
                    OBJECTIVE_TIMERS_SCRIPT,
                    objective_timers_vmad(&rows, &target_masters, &target_plugin, mapper.interner),
                ));
            }

            for (script_name, vmad) in bindings {
                let Some(vmad) = vmad else {
                    unresolved += 1;
                    warn(
                        &mut report,
                        mapper.interner,
                        quest_local,
                        &format!("{script_name}:vmad_encode_failed"),
                    );
                    continue;
                };
                match attach_quest_script(session, &target_quest, script_name, &vmad)? {
                    Some(AttachResult::Changed) => {
                        report.records_changed += 1;
                        attached += 1;
                    }
                    Some(AttachResult::AlreadyPresent) => present += 1,
                    Some(AttachResult::Conflict(reason)) => {
                        unresolved += 1;
                        warn(
                            &mut report,
                            mapper.interner,
                            quest_local,
                            &format!("{script_name}:{reason}"),
                        );
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
        }

        report.message = Some(mapper.interner.intern(&format!(
            "fo76_quest_timers:attached={attached};present={present};unresolved={unresolved};skipped_objectives={skipped_objectives}"
        )));
        Ok(report)
    }
}

fn parse_source_timers(
    subrecords: &[ParsedSubrecord],
    source: &SourcePlugin,
    interner: &StringInterner,
) -> SourceTimers {
    let mut timers = SourceTimers::default();
    let mut objective: Option<SourceObjectiveTimer> = None;
    for subrecord in subrecords {
        let sig = subrecord.signature.as_str();
        let data = subrecord.data.as_ref();
        if OBJECTIVE_SCOPE_END_SIGS.contains(&sig) {
            timers.objectives.extend(objective.take());
        }
        match (sig, objective.as_mut()) {
            ("QTLM", None) => timers.quest_timer = source.form_key_at(data, 0, interner),
            ("INDX", None) if data.len() >= 3 => {
                let stage = i32::from(u16::from_le_bytes([data[0], data[1]]));
                timers.stages.push(stage);
                if data[2] & STAGE_FLAG_START_TIMER != 0 {
                    timers.start_timer_stages.push(stage);
                }
                if data[2] & STAGE_FLAG_TIMER_END != 0 {
                    timers.timer_end_stages.push(stage);
                }
            }
            ("QOBJ", None) if data.len() >= 2 => {
                objective = Some(SourceObjectiveTimer {
                    objective: i32::from(u16::from_le_bytes([data[0], data[1]])),
                    ..SourceObjectiveTimer::default()
                });
            }
            ("FNAM", Some(objective)) if data.len() >= 4 => {
                let flags = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                objective.uses_timer = flags & OBJECTIVE_FLAG_USES_TIMER != 0;
            }
            ("QOTM", Some(objective)) => objective.timer = source.form_key_at(data, 0, interner),
            ("SNAM", Some(objective)) if data.len() >= 2 => {
                objective.expiry_stage = Some(i32::from(u16::from_le_bytes([data[0], data[1]])));
            }
            _ => {}
        }
    }
    timers.objectives.extend(objective);
    timers
}

fn quest_timer_vmad(
    timer_length: Option<FormKey>,
    timers: &SourceTimers,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let properties = vec![
        object_property("TimerLength", timer_length, interner)?,
        int_array_property("StartTimerStages", &timers.start_timer_stages),
        int_array_property("TimerEndStages", &timers.timer_end_stages),
    ];
    script_vmad(
        QUEST_TIMER_SCRIPT,
        properties,
        target_masters,
        target_plugin,
    )
}

fn objective_timers_vmad(
    rows: &[ObjectiveTimerRow],
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let properties = vec![
        int_array_property(
            "TimedObjectives",
            &rows.iter().map(|row| row.objective).collect::<Vec<_>>(),
        ),
        object_array_property(
            "TimerLengths",
            &rows.iter().map(|row| row.timer).collect::<Vec<_>>(),
            interner,
        )?,
        int_array_property(
            "ExpiryStages",
            &rows.iter().map(|row| row.expiry_stage).collect::<Vec<_>>(),
        ),
    ];
    script_vmad(
        OBJECTIVE_TIMERS_SCRIPT,
        properties,
        target_masters,
        target_plugin,
    )
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn warn(report: &mut FixupReport, interner: &StringInterner, source_quest: u32, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "attach_fo76_quest_timers:{source_quest:06X}:{reason}"
    )));
}

#[cfg(test)]
mod tests;
