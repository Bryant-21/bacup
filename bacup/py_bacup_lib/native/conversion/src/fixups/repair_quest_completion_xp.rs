use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

const FO76_XP_NONE_LOCAL_ID: u32 = 0x098952;
const COMPLETE_QUEST_FLAG: u8 = 0x01;

pub struct RepairQuestCompletionXpFixup;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompletionXpReason {
    Source,
    Reward,
    Fallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompletionXpChoice {
    source_global: FormKey,
    reason: CompletionXpReason,
}

impl Fixup for RepairQuestCompletionXpFixup {
    fn name(&self) -> &'static str {
        "repair_quest_completion_xp"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.source_id().is_some()
            && session
                .source_schema()
                .is_ok_and(|schema| schema.record_def("GMRW").is_some())
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let source_schema = session
            .source_schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let qust_sig = SigCode::from_str("QUST")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let glob_sig = SigCode::from_str("GLOB")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;

        let source_plugin_name = session
            .source_slot_opt()
            .map(|slot| slot.parsed.plugin_name.clone())
            .ok_or_else(|| FixupError::HandleError("source plugin missing".into()))?;
        let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let source_plugin = mapper.interner.intern(&source_plugin_name);
        let output_plugin = mapper.interner.intern(&output_plugin_name);
        let source_xp_none = verified_source_xp_none(
            session,
            source_schema.as_ref(),
            mapper.interner,
            FormKey {
                local: FO76_XP_NONE_LOCAL_ID,
                plugin: source_plugin,
            },
            glob_sig,
        );

        let target_quests = session
            .form_keys_of_sig(qust_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if target_quests.is_empty() {
            return Ok(report);
        }
        let target_quest_set = target_quests.iter().copied().collect::<FxHashSet<_>>();
        let source_by_target = mapper
            .source_to_target_iter()
            .filter(|(_, target)| target_quest_set.contains(target))
            .map(|(source, target)| (target, source))
            .collect::<FxHashMap<_, _>>();
        let mut reward_xp_cache = FxHashMap::default();
        let mut source_choices = 0u32;
        let mut reward_choices = 0u32;
        let mut fallback_choices = 0u32;
        let mut unresolved = 0u32;

        for target_quest_fk in target_quests {
            let Some(source_quest_fk) = source_by_target.get(&target_quest_fk).copied() else {
                continue;
            };
            let mut target_quest = match session.record_decoded(
                &target_quest_fk,
                target_schema.as_ref(),
                mapper.interner,
            ) {
                Ok(record) => record,
                Err(error) => {
                    report.warnings.push(mapper.interner.intern(&format!(
                        "repair_quest_completion_xp:target_read:{:06X}:{error}",
                        target_quest_fk.local
                    )));
                    continue;
                }
            };
            if has_non_null_xnam(&target_quest) || !has_complete_stage(&target_quest) {
                continue;
            }
            let source_quest = match session.source_record_decoded(
                &source_quest_fk,
                source_schema.as_ref(),
                mapper.interner,
            ) {
                Ok(record) => record,
                Err(error) => {
                    report.warnings.push(mapper.interner.intern(&format!(
                        "repair_quest_completion_xp:source_read:{:06X}:{error}",
                        source_quest_fk.local
                    )));
                    continue;
                }
            };

            let choice = choose_completion_xp(&source_quest, source_xp_none, |reward_fk| {
                source_reward_xp_global(
                    session,
                    source_schema.as_ref(),
                    mapper.interner,
                    reward_fk,
                    &mut reward_xp_cache,
                )
            });
            let Some(choice) = choice else {
                unresolved += 1;
                continue;
            };
            let Some(target_xp_global) = target_global_for_source(
                session,
                target_schema.as_ref(),
                mapper,
                choice.source_global,
                output_plugin,
                glob_sig,
            ) else {
                unresolved += 1;
                continue;
            };

            set_xnam(&mut target_quest, target_xp_global);
            let replaced = session
                .replace_record_contents(target_quest, target_schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            if !replaced {
                unresolved += 1;
                continue;
            }
            report.records_changed += 1;
            match choice.reason {
                CompletionXpReason::Source => source_choices += 1,
                CompletionXpReason::Reward => reward_choices += 1,
                CompletionXpReason::Fallback => fallback_choices += 1,
            }
        }

        report.message = Some(mapper.interner.intern(&format!(
            "quest_completion_xp:source={source_choices};reward={reward_choices};fallback={fallback_choices};unresolved={unresolved}"
        )));
        Ok(report)
    }
}

fn verified_source_xp_none(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    interner: &crate::sym::StringInterner,
    source_fk: FormKey,
    glob_sig: SigCode,
) -> Option<FormKey> {
    let record = session
        .source_record_decoded(&source_fk, source_schema, interner)
        .ok()?;
    let editor_id = record.eid.and_then(|eid| interner.resolve(eid));
    (record.sig == glob_sig && editor_id == Some("XPNone")).then_some(source_fk)
}

fn source_reward_xp_global(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    interner: &crate::sym::StringInterner,
    reward_fk: FormKey,
    cache: &mut FxHashMap<FormKey, Option<FormKey>>,
) -> Option<FormKey> {
    if let Some(cached) = cache.get(&reward_fk) {
        return *cached;
    }
    let resolved = session
        .source_record_decoded(&reward_fk, source_schema, interner)
        .ok()
        .filter(|record| record.sig.0 == *b"GMRW")
        .and_then(|record| first_form_key(&record, *b"NAM7"));
    cache.insert(reward_fk, resolved);
    resolved
}

fn target_global_for_source(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    mapper: &FormKeyMapper,
    source_global: FormKey,
    output_plugin: crate::sym::Sym,
    glob_sig: SigCode,
) -> Option<FormKey> {
    let target_global = mapper.lookup(source_global)?;
    if target_global.plugin != output_plugin {
        return Some(target_global);
    }
    session
        .record_decoded(&target_global, target_schema, mapper.interner)
        .ok()
        .filter(|record| record.sig == glob_sig)
        .map(|_| target_global)
}

fn choose_completion_xp(
    source_quest: &Record,
    source_xp_none: Option<FormKey>,
    mut reward_xp: impl FnMut(FormKey) -> Option<FormKey>,
) -> Option<CompletionXpChoice> {
    if let Some(source_global) = first_form_key(source_quest, *b"XNAM") {
        return Some(CompletionXpChoice {
            source_global,
            reason: CompletionXpReason::Source,
        });
    }

    let reward_globals = completion_reward_refs(source_quest)
        .into_iter()
        .filter_map(&mut reward_xp)
        .collect::<FxHashSet<_>>();
    if reward_globals.len() == 1 {
        return reward_globals
            .into_iter()
            .next()
            .map(|source_global| CompletionXpChoice {
                source_global,
                reason: CompletionXpReason::Reward,
            });
    }

    source_xp_none.map(|source_global| CompletionXpChoice {
        source_global,
        reason: CompletionXpReason::Fallback,
    })
}

fn completion_reward_refs(record: &Record) -> Vec<FormKey> {
    let mut in_stage = false;
    let mut stage_completes = false;
    let mut rewards = Vec::new();
    for entry in &record.fields {
        match &entry.sig.0 {
            b"INDX" => {
                in_stage = true;
                stage_completes = false;
            }
            b"QSDT" if in_stage => {
                stage_completes = field_value_u8(&entry.value)
                    .is_some_and(|flags| flags & COMPLETE_QUEST_FLAG != 0);
            }
            b"DNAM" if in_stage && stage_completes => {
                if let FieldValue::FormKey(form_key) = &entry.value
                    && form_key.local != 0
                {
                    rewards.push(*form_key);
                }
            }
            _ => {}
        }
    }
    rewards
}

fn has_complete_stage(record: &Record) -> bool {
    record.fields.iter().any(|entry| {
        entry.sig.0 == *b"QSDT"
            && field_value_u8(&entry.value).is_some_and(|flags| flags & COMPLETE_QUEST_FLAG != 0)
    })
}

fn has_non_null_xnam(record: &Record) -> bool {
    record.fields.iter().any(|entry| {
        entry.sig.0 == *b"XNAM"
            && matches!(&entry.value, FieldValue::FormKey(form_key) if form_key.local != 0)
    })
}

fn first_form_key(record: &Record, sig: [u8; 4]) -> Option<FormKey> {
    record.fields.iter().find_map(|entry| {
        (entry.sig.0 == sig)
            .then_some(&entry.value)
            .and_then(|value| match value {
                FieldValue::FormKey(form_key) if form_key.local != 0 => Some(*form_key),
                _ => None,
            })
    })
}

fn field_value_u8(value: &FieldValue) -> Option<u8> {
    match value {
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Bytes(bytes) => bytes.first().copied(),
        FieldValue::Struct(fields) => fields.first().and_then(|(_, value)| field_value_u8(value)),
        _ => None,
    }
}

fn set_xnam(record: &mut Record, target_global: FormKey) {
    if let Some(existing) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"XNAM")
    {
        existing.value = FieldValue::FormKey(target_global);
        return;
    }

    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(&entry.sig.0, b"QTGL" | b"INDX" | b"QOBJ" | b"ANAM"))
        .unwrap_or(record.fields.len());
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: SubrecordSig(*b"XNAM"),
            value: FieldValue::FormKey(target_global),
        },
    );
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use super::*;
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;

    fn fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }
    }

    fn quest(interner: &StringInterner, local: u32, fields: Vec<FieldEntry>) -> Record {
        Record {
            sig: SigCode(*b"QUST"),
            form_key: fk(interner, local),
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec![],
        }
    }

    #[test]
    fn completion_rewards_only_come_from_complete_stages() {
        let interner = StringInterner::new();
        let ignored = fk(&interner, 0x100);
        let completion = fk(&interner, 0x200);
        let record = quest(
            &interner,
            1,
            vec![
                field(b"INDX", FieldValue::Uint(100)),
                field(b"QSDT", FieldValue::Uint(0)),
                field(b"DNAM", FieldValue::FormKey(ignored)),
                field(b"INDX", FieldValue::Uint(200)),
                field(b"QSDT", FieldValue::Uint(1)),
                field(b"DNAM", FieldValue::FormKey(completion)),
            ],
        );

        assert_eq!(completion_reward_refs(&record), vec![completion]);
    }

    #[test]
    fn unique_reward_xp_wins_and_conflicts_use_xp_none() {
        let interner = StringInterner::new();
        let reward_a = fk(&interner, 0x100);
        let reward_b = fk(&interner, 0x101);
        let xp = fk(&interner, 0x200);
        let other_xp = fk(&interner, 0x201);
        let xp_none = fk(&interner, FO76_XP_NONE_LOCAL_ID);
        let record = quest(
            &interner,
            1,
            vec![
                field(b"INDX", FieldValue::Uint(200)),
                field(b"QSDT", FieldValue::Uint(1)),
                field(b"DNAM", FieldValue::FormKey(reward_a)),
                field(b"DNAM", FieldValue::FormKey(reward_b)),
            ],
        );

        let unique = choose_completion_xp(&record, Some(xp_none), |_| Some(xp)).unwrap();
        assert_eq!(
            unique,
            CompletionXpChoice {
                source_global: xp,
                reason: CompletionXpReason::Reward,
            }
        );

        let conflict = choose_completion_xp(&record, Some(xp_none), |reward| {
            (reward == reward_a).then_some(xp).or(Some(other_xp))
        })
        .unwrap();
        assert_eq!(
            conflict,
            CompletionXpChoice {
                source_global: xp_none,
                reason: CompletionXpReason::Fallback,
            }
        );
    }

    #[test]
    fn source_xnam_is_preserved_and_target_insertion_precedes_stages() {
        let interner = StringInterner::new();
        let source_xp = fk(&interner, 0x300);
        let target_xp = fk(&interner, 0x400);
        let source = quest(
            &interner,
            1,
            vec![field(b"XNAM", FieldValue::FormKey(source_xp))],
        );
        let choice = choose_completion_xp(&source, None, |_| None).unwrap();
        assert_eq!(choice.reason, CompletionXpReason::Source);
        assert_eq!(choice.source_global, source_xp);

        let mut target = quest(
            &interner,
            2,
            vec![
                field(b"DNAM", FieldValue::Bytes(smallvec![0; 12])),
                field(b"INDX", FieldValue::Uint(200)),
                field(b"QSDT", FieldValue::Uint(1)),
            ],
        );
        set_xnam(&mut target, target_xp);
        assert_eq!(
            target
                .fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["DNAM", "XNAM", "INDX", "QSDT"]
        );

        let mut target_with_null_xnam = quest(
            &interner,
            3,
            vec![
                field(b"XNAM", FieldValue::FormKey(fk(&interner, 0))),
                field(b"INDX", FieldValue::Uint(200)),
            ],
        );
        set_xnam(&mut target_with_null_xnam, target_xp);
        assert_eq!(
            target_with_null_xnam
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"XNAM")
                .map(|entry| &entry.value)
                .collect::<Vec<_>>(),
            vec![&FieldValue::FormKey(target_xp)]
        );
    }
}
