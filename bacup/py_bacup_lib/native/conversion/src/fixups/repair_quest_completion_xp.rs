use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

const FO76_XP_NONE_LOCAL_ID: u32 = 0x098952;
const COMPLETE_QUEST_FLAG: u8 = 0x01;
/// FO76 pays ordinary completion XP from a level-scaled curve table, while
/// FO4's `QuestCompletionXP` is a flat `GLOB`. The curve has to collapse to one
/// representative value, sampled at a mid-game player level.
const XP_CURVE_SAMPLE_LEVEL: f32 = 50.0;
const XP_CURVE_ROOT: &str = "misc/curvetables/json";
const SYNTHETIC_XP_GLOBAL_PREFIX: &str = "B21_QuestCompletionXP_";

pub struct RepairQuestCompletionXpFixup;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompletionXpReason {
    Source,
    Reward,
    Curve,
    Fallback,
}

/// Where a quest's ordinary completion XP comes from in the source data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SourceCompletionXp {
    /// A `GLOB` named directly by an unconditional reward group's `NAM7`.
    Global(FormKey),
    /// A `CTDA`-free reward group's `XPCT` curve table.
    Curve(FormKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompletionXpChoice {
    source: SourceCompletionXp,
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
        config: &FixupConfig,
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
        let source_masters = session
            .source_slot_opt()
            .map(|slot| slot.parsed.header.masters.clone())
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
        let mut curve_globals: FxHashMap<u32, FormKey> = FxHashMap::default();
        let mut added_globals: Vec<Record> = Vec::new();
        let mut source_choices = 0u32;
        let mut reward_choices = 0u32;
        let mut curve_choices = 0u32;
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

            let choice = choose_completion_xp(
                &source_quest,
                source_xp_none,
                |value| {
                    source_form_key(value, &source_masters, &source_plugin_name, mapper.interner)
                },
                |reward_fk| {
                    source_reward_completion_xp(
                        session,
                        source_schema.as_ref(),
                        mapper.interner,
                        reward_fk,
                        &source_masters,
                        &source_plugin_name,
                        &mut reward_xp_cache,
                    )
                },
            );
            let Some(choice) = choice else {
                unresolved += 1;
                continue;
            };
            let target_xp_global = match choice.source {
                SourceCompletionXp::Global(source_global) => target_global_for_source(
                    session,
                    target_schema.as_ref(),
                    mapper,
                    source_global,
                    output_plugin,
                    glob_sig,
                ),
                SourceCompletionXp::Curve(curve_fk) => curve_table_xp(
                    session,
                    source_schema.as_ref(),
                    mapper.interner,
                    curve_fk,
                    config.source_extracted_dir.as_deref(),
                )
                .map(|xp| {
                    let rounded = xp.round().max(0.0) as u32;
                    *curve_globals.entry(rounded).or_insert_with(|| {
                        let synthetic = FormKey {
                            local: 1,
                            plugin: mapper
                                .interner
                                .intern(&format!("{SYNTHETIC_XP_GLOBAL_PREFIX}{rounded}")),
                        };
                        let fk = mapper.allocate_or_resolve(synthetic, None, glob_sig);
                        added_globals.push(completion_xp_global(fk, rounded, mapper.interner));
                        fk
                    })
                }),
            };
            let Some(target_xp_global) = target_xp_global else {
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
                CompletionXpReason::Curve => curve_choices += 1,
                CompletionXpReason::Fallback => fallback_choices += 1,
            }
        }

        report.records_added = added_globals.len() as u32;
        if !added_globals.is_empty() {
            session
                .add_records(added_globals, target_schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
        }
        report.message = Some(mapper.interner.intern(&format!(
            "quest_completion_xp:source={source_choices};reward={reward_choices};curve={curve_choices};fallback={fallback_choices};unresolved={unresolved}"
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

fn source_reward_completion_xp(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    interner: &crate::sym::StringInterner,
    reward_fk: FormKey,
    source_masters: &[String],
    source_plugin_name: &str,
    cache: &mut FxHashMap<FormKey, Option<SourceCompletionXp>>,
) -> Option<SourceCompletionXp> {
    if let Some(cached) = cache.get(&reward_fk) {
        return *cached;
    }
    let resolved = session
        .source_record_decoded(&reward_fk, source_schema, interner)
        .ok()
        .filter(|record| record.sig.0 == *b"GMRW")
        .and_then(|record| {
            unconditional_group_xp(&record, source_masters, source_plugin_name, interner)
        });
    cache.insert(reward_fk, resolved);
    resolved
}

/// The XP payout of the first reward group that carries no condition.
///
/// A `GMRW` holds several groups separated by `ITME`, and only the
/// condition-free one is paid on an ordinary completion. Taking the first
/// `NAM7` in the record regardless of its group handed every mutated-event
/// bonus out unconditionally -- thirteen public events were awarding
/// `XP_MutatedEvents` for a normal run.
fn unconditional_group_xp(
    record: &Record,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<SourceCompletionXp> {
    let mut groups = Vec::<Vec<&FieldEntry>>::new();
    let mut current = Vec::<&FieldEntry>::new();
    for entry in &record.fields {
        if entry.sig.0 == *b"ITME" {
            if !current.is_empty() {
                groups.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(entry);
    }
    if !current.is_empty() {
        groups.push(current);
    }

    for group in groups {
        if group
            .iter()
            .any(|entry| matches!(&entry.sig.0, b"CTDA" | b"CIS1" | b"CIS2"))
        {
            continue;
        }
        let field = |sig: [u8; 4]| {
            group
                .iter()
                .find(|entry| entry.sig.0 == sig)
                .and_then(|entry| {
                    source_form_key(&entry.value, source_masters, source_plugin_name, interner)
                })
        };
        if let Some(global) = field(*b"NAM7") {
            return Some(SourceCompletionXp::Global(global));
        }
        if let Some(curve) = field(*b"XPCT") {
            return Some(SourceCompletionXp::Curve(curve));
        }
    }
    None
}

/// The XP a FO76 curve table pays at `XP_CURVE_SAMPLE_LEVEL`.
fn curve_table_xp(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    interner: &crate::sym::StringInterner,
    curve_fk: FormKey,
    extracted_dir: Option<&std::path::Path>,
) -> Option<f32> {
    let record = session
        .source_record_decoded(&curve_fk, source_schema, interner)
        .ok()?;
    let relative = record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"JASF")
        .and_then(|entry| match &entry.value {
            FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .map(|bytes| {
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            String::from_utf8_lossy(&bytes[..end]).replace('\\', "/")
        })?;
    let path = extracted_dir?.join(XP_CURVE_ROOT).join(relative);
    sample_curve(&std::fs::read_to_string(path).ok()?, XP_CURVE_SAMPLE_LEVEL)
}

/// Linear interpolation over a `{"curve":[{"x":level,"y":xp}]}` table, clamped
/// to the first and last point outside the sampled range.
fn sample_curve(json: &str, level: f32) -> Option<f32> {
    let parsed: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut points: Vec<(f32, f32)> = parsed
        .get("curve")?
        .as_array()?
        .iter()
        .filter_map(|point| {
            Some((
                point.get("x")?.as_f64()? as f32,
                point.get("y")?.as_f64()? as f32,
            ))
        })
        .collect();
    points.sort_by(|left, right| left.0.total_cmp(&right.0));
    let first = *points.first()?;
    let last = *points.last()?;
    if level <= first.0 {
        return Some(first.1);
    }
    if level >= last.0 {
        return Some(last.1);
    }
    let upper = points.iter().position(|point| point.0 >= level)?;
    let (x0, y0) = points[upper.saturating_sub(1)];
    let (x1, y1) = points[upper];
    if (x1 - x0).abs() < f32::EPSILON {
        return Some(y1);
    }
    Some(y0 + (y1 - y0) * (level - x0) / (x1 - x0))
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
    mut reward_ref: impl FnMut(&FieldValue) -> Option<FormKey>,
    mut reward_xp: impl FnMut(FormKey) -> Option<SourceCompletionXp>,
) -> Option<CompletionXpChoice> {
    if let Some(source_global) = first_form_key(source_quest, *b"XNAM") {
        return Some(CompletionXpChoice {
            source: SourceCompletionXp::Global(source_global),
            reason: CompletionXpReason::Source,
        });
    }

    let rewards = completion_reward_refs(source_quest, &mut reward_ref)
        .into_iter()
        .filter_map(&mut reward_xp)
        .collect::<FxHashSet<_>>();
    if rewards.len() == 1 {
        return rewards.into_iter().next().map(|source| CompletionXpChoice {
            source,
            reason: match source {
                SourceCompletionXp::Global(_) => CompletionXpReason::Reward,
                SourceCompletionXp::Curve(_) => CompletionXpReason::Curve,
            },
        });
    }

    source_xp_none.map(|source_global| CompletionXpChoice {
        source: SourceCompletionXp::Global(source_global),
        reason: CompletionXpReason::Fallback,
    })
}

fn completion_reward_refs(
    record: &Record,
    mut resolve: impl FnMut(&FieldValue) -> Option<FormKey>,
) -> Vec<FormKey> {
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
            b"QRWD" if in_stage && stage_completes => {
                if let Some(form_key) = resolve(&entry.value) {
                    rewards.push(form_key);
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

fn first_source_form_key(
    record: &Record,
    sig: [u8; 4],
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<FormKey> {
    record.fields.iter().find_map(|entry| {
        (entry.sig.0 == sig)
            .then_some(&entry.value)
            .and_then(|value| source_form_key(value, source_masters, source_plugin_name, interner))
    })
}

fn source_form_key(
    value: &FieldValue,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<FormKey> {
    if let FieldValue::FormKey(form_key) = value {
        return (form_key.local != 0).then_some(*form_key);
    }
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    let raw = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?);
    if raw == 0 {
        return None;
    }
    let plugin = source_masters
        .get((raw >> 24) as usize)
        .map(String::as_str)
        .unwrap_or(source_plugin_name);
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
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

    fn raw_form(local: u32) -> FieldValue {
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&local.to_le_bytes()))
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

    fn reward(interner: &StringInterner, local: u32, fields: Vec<FieldEntry>) -> Record {
        Record {
            sig: SigCode(*b"GMRW"),
            form_key: fk(interner, local),
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec![],
        }
    }

    /// `QuestReward_E01C_Tales_Dark_Stage9000_01` (6311F8): an unconditional
    /// group paying an XP curve table, then a condition-gated group paying
    /// `XP_MutatedEvents`. Only the first is earned by an ordinary completion.
    #[test]
    fn unconditional_reward_group_supplies_completion_xp() {
        let interner = StringInterner::new();
        let curve = fk(&interner, 0x876403);
        let mutated_bonus = fk(&interner, 0x69FA1D);
        let record = reward(
            &interner,
            0x6311F8,
            vec![
                field(b"XPCT", raw_form(curve.local)),
                field(b"QRLR", raw_form(1)),
                field(b"ITME", FieldValue::Bytes(smallvec::SmallVec::new())),
                field(b"NAM7", raw_form(mutated_bonus.local)),
                field(b"QRLR", raw_form(3)),
                field(
                    b"CTDA",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 32])),
                ),
                field(b"ITME", FieldValue::Bytes(smallvec::SmallVec::new())),
            ],
        );

        assert_eq!(
            unconditional_group_xp(&record, &[], "SeventySix.esm", &interner),
            Some(SourceCompletionXp::Curve(curve))
        );

        let global = fk(&interner, 0x005918EA);
        let record = reward(
            &interner,
            0x600000,
            vec![
                field(b"NAM7", raw_form(global.local)),
                field(b"XPCT", raw_form(0x876403)),
                field(b"ITME", FieldValue::Bytes(smallvec::SmallVec::new())),
            ],
        );

        assert_eq!(
            unconditional_group_xp(&record, &[], "SeventySix.esm", &interner),
            Some(SourceCompletionXp::Global(global))
        );

        let record = reward(
            &interner,
            0x600001,
            vec![
                field(b"NAM7", raw_form(0x69FA1D)),
                field(
                    b"CTDA",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 32])),
                ),
                field(b"ITME", FieldValue::Bytes(smallvec::SmallVec::new())),
            ],
        );

        assert_eq!(
            unconditional_group_xp(&record, &[], "SeventySix.esm", &interner),
            None
        );

        let xp = fk(&interner, 0x556D52);
        let reward = Record {
            sig: SigCode(*b"GMRW"),
            form_key: fk(&interner, 0x6313B1),
            eid: None,
            flags: RecordFlags::empty(),
            fields: vec![field(b"NAM7", raw_form(xp.local))]
                .into_iter()
                .collect(),
            warnings: smallvec![],
        };

        assert_eq!(
            first_source_form_key(&reward, *b"NAM7", &[], "SeventySix.esm", &interner,),
            Some(xp)
        );
    }

    /// `XP_Universal_Tier25` runs 140 XP at level 1 to 823 at level 100.
    #[test]
    fn curve_is_sampled_by_interpolating_between_surrounding_points() {
        let json =
            r#"{"curve":[{"x":1,"y":140},{"x":45,"y":443},{"x":56,"y":519},{"x":100,"y":823}]}"#;

        assert_eq!(sample_curve(json, 45.0), Some(443.0));
        // Half way between the level-45 and level-56 points.
        let midpoint = sample_curve(json, 50.5).expect("sampled");
        assert!((midpoint - 481.0).abs() < 0.5, "got {midpoint}");
        // Outside the table the end points clamp rather than extrapolate.
        assert_eq!(sample_curve(json, 0.0), Some(140.0));
        assert_eq!(sample_curve(json, 999.0), Some(823.0));
        assert_eq!(sample_curve("{}", 50.0), None);
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
                field(b"QRWD", raw_form(ignored.local)),
                field(b"INDX", FieldValue::Uint(200)),
                field(b"QSDT", FieldValue::Uint(1)),
                field(b"DNAM", FieldValue::FormKey(ignored)),
                field(b"QRWD", raw_form(completion.local)),
            ],
        );

        assert_eq!(
            completion_reward_refs(&record, |value| {
                source_form_key(value, &[], "SeventySix.esm", &interner)
            }),
            vec![completion]
        );
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
                field(b"QRWD", raw_form(reward_a.local)),
                field(b"QRWD", raw_form(reward_b.local)),
            ],
        );

        let unique = choose_completion_xp(
            &record,
            Some(xp_none),
            |value| source_form_key(value, &[], "SeventySix.esm", &interner),
            |_| Some(SourceCompletionXp::Global(xp)),
        )
        .unwrap();
        assert_eq!(
            unique,
            CompletionXpChoice {
                source: SourceCompletionXp::Global(xp),
                reason: CompletionXpReason::Reward,
            }
        );

        let conflict = choose_completion_xp(
            &record,
            Some(xp_none),
            |value| source_form_key(value, &[], "SeventySix.esm", &interner),
            |reward| {
                Some(SourceCompletionXp::Global(if reward == reward_a {
                    xp
                } else {
                    other_xp
                }))
            },
        )
        .unwrap();
        assert_eq!(
            conflict,
            CompletionXpChoice {
                source: SourceCompletionXp::Global(xp_none),
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
        let choice = choose_completion_xp(&source, None, |_| None, |_| None).unwrap();
        assert_eq!(choice.reason, CompletionXpReason::Source);
        assert_eq!(choice.source, SourceCompletionXp::Global(source_xp));

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

/// A flat `GLOB` standing in for one sampled point of a FO76 XP curve table.
fn completion_xp_global(fk: FormKey, xp: u32, interner: &crate::sym::StringInterner) -> Record {
    let mut record = Record::new(SigCode(*b"GLOB"), fk);
    let eid = interner.intern(&format!("{SYNTHETIC_XP_GLOBAL_PREFIX}{xp}"));
    record.eid = Some(eid);
    record.fields.extend([
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(eid),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FNAM"),
            value: FieldValue::Bytes(vec![b'f'].into()),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FLTV"),
            value: FieldValue::Float(xp as f32),
        },
    ]);
    record
}
