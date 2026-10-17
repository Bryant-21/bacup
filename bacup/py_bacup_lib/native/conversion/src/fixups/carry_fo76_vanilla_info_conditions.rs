//! Carries FO76's extra conditions on dialogue it inherited from Fallout 4 into Fallout4.esm overrides.
//!
//! FO76's SeventySix.esm keeps many FO4 INFOs under their FO4 object ids and gates them for
//! Appalachia: it adds rows that silence a stock bark for an FO76 faction, role, quest NPC or
//! location. One example is the generic Mr. Handy "Hello there." group `1CD029`, gated off in the
//! Vault 76 character-generation locations. The translator maps those INFOs onto Fallout4.esm and
//! drops them, so in FO4 the ungated vanilla line plays.
//!
//! For each such INFO, this writes FO4's own record with the rows FO76 added prepended. FO4's
//! rows and responses stay as they are. FO76 only ever ANDs these rows on, so an override can
//! only silence a line, never expose FO4-world text in Appalachia. FO4 places an INFO override
//! under its DIAL, inside the QUST's quest-child group, so both parents are written as unchanged
//! overrides.
//!
//! An INFO is left alone when an added row would loosen or lose meaning in FO4: an OR row, a row
//! with script strings, a function FO4 lacks, or a FormID with no target. Runs after the
//! raw-FormID rewrite passes, which would otherwise read FO4's own rows as FO76 FormIDs.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::rewrite_raw_object_template_formids::{
    RawConditionRewrite, encoded_targets_by_source_object_id, rewrite_raw_condition_bytes,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{PluginSession, SessionError};
use crate::translator::pair_hooks::fo76_fo4::{
    is_fo4_incompatible_condition_function_id, normalize_fo76_raw_condition_rows,
};
use esp_authoring_core::plugin_runtime::{ParsedItem, effective_subrecords_for_record};

const FO4_MASTER_NAME: &str = "Fallout4.esm";
const TOPIC_CHILD_GROUP: i32 = 7;
const OBJECT_ID_MASK: u32 = 0x00FF_FFFF;
const CTDA_LEN: usize = 32;
const CTDA_OR_FLAG: u8 = 0x01;

pub struct CarryFo76VanillaInfoConditionsFixup;

impl Fixup for CarryFo76VanillaInfoConditionsFixup {
    fn name(&self) -> &'static str {
        "carry_fo76_vanilla_info_conditions"
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
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let handle_error = |error: SessionError| FixupError::HandleError(error.to_string());
        let sig = |name: &str| SigCode::from_str(name).map_err(FixupError::SchemaError);
        let schema = session.schema().map_err(handle_error)?;
        let target_masters = session.target_masters().to_vec();
        // Raw rows are copied between files, so FO4's load index must be 0 in the target as it is
        // in Fallout4.esm itself.
        let (Some(first_master), Some(&fallout4_handle)) =
            (target_masters.first(), config.target_master_handle_ids.first())
        else {
            return Ok(report);
        };
        if !first_master.eq_ignore_ascii_case(FO4_MASTER_NAME) {
            return Ok(report);
        }
        let fallout4 = mapper.interner.intern(FO4_MASTER_NAME);
        let output_plugin = mapper.output_plugin_sym();
        let master_key = |local| FormKey {
            local,
            plugin: fallout4,
        };

        // Masters are index-only handles with no group tree, so topic nesting comes from the
        // FO76 source, which inherited FO4's topics under the same ids.
        let mut fallout4_locals = |name: &str| -> Result<FxHashSet<u32>, FixupError> {
            Ok(session
                .form_keys_of_sig_in_handle(fallout4_handle, sig(name)?, mapper.interner)
                .map_err(handle_error)?
                .into_iter()
                .filter(|form_key| form_key.plugin == fallout4)
                .map(|form_key| form_key.local)
                .collect())
        };
        let fallout4_infos = fallout4_locals("INFO")?;
        let fallout4_topics = fallout4_locals("DIAL")?;
        let Some(source_slot) = session.source_slot_opt() else {
            return Ok(report);
        };
        let mut source_infos = Vec::new();
        collect_source_info_conditions(
            &source_slot.parsed.root_items,
            &fallout4_infos,
            &fallout4_topics,
            &mut source_infos,
        );

        let existing: FxHashSet<FormKey> = session
            .form_keys_of_sig(sig("INFO")?, mapper.interner)
            .map_err(handle_error)?
            .into_iter()
            .collect();
        let fallout4_object_ids = session
            .local_object_ids_in_handle(fallout4_handle)
            .map_err(handle_error)?;
        let encoded_targets = encoded_targets_by_source_object_id(mapper, &target_masters);

        let mut stats = CarryStats {
            candidates: source_infos.len(),
            ..Default::default()
        };
        let mut overrides_by_topic: FxHashMap<u32, Vec<Record>> = FxHashMap::default();
        for source_info in source_infos {
            let (local, topic_local) = (source_info.local, source_info.topic);
            if existing.contains(&FormKey {
                local,
                plugin: output_plugin,
            }) || existing.contains(&master_key(local))
            {
                stats.already_present += 1;
                continue;
            }
            let mut master_info = session
                .record_decoded_in_handle(
                    fallout4_handle,
                    &master_key(local),
                    schema.as_ref(),
                    mapper.interner,
                )
                .map_err(handle_error)?;
            let added = match added_rows(source_info, &master_info, output_plugin) {
                Ok(rows) if rows.is_empty() => {
                    stats.unchanged += 1;
                    continue;
                }
                Ok(rows) => rows,
                Err(skip) => {
                    stats.count_skip(skip);
                    continue;
                }
            };
            let Some(encoded) = encode_rows(added, &encoded_targets, &fallout4_object_ids) else {
                stats.unencodable += 1;
                continue;
            };
            prepend_conditions(&mut master_info, encoded);
            overrides_by_topic
                .entry(topic_local)
                .or_default()
                .push(master_info);
        }

        let mut quests = Vec::new();
        let mut topics = Vec::new();
        let mut seen_quests = FxHashSet::default();
        for &topic_local in overrides_by_topic.keys() {
            let topic = session
                .record_decoded_in_handle(
                    fallout4_handle,
                    &master_key(topic_local),
                    schema.as_ref(),
                    mapper.interner,
                )
                .map_err(handle_error)?;
            let Some(quest) = topic_quest(&topic).filter(|quest| quest.plugin == fallout4) else {
                continue;
            };
            if seen_quests.insert(quest)
                && session
                    .raw_form_id_of_signature(&quest, sig("QUST")?, mapper.interner)
                    .map_err(handle_error)?
                    .is_none()
            {
                quests.push(
                    session
                        .record_decoded_in_handle(
                            fallout4_handle,
                            &quest,
                            schema.as_ref(),
                            mapper.interner,
                        )
                        .map_err(handle_error)?,
                );
            }
            if session
                .raw_form_id_of_signature(&topic.form_key, sig("DIAL")?, mapper.interner)
                .map_err(handle_error)?
                .is_none()
            {
                topics.push(topic);
            }
        }
        stats.quests = quests.len();
        stats.topics = topics.len();
        session
            .add_records(quests, schema.as_ref(), mapper.interner)
            .map_err(handle_error)?;
        let placed_topics = session
            .add_quest_child_records(topics, schema.as_ref(), mapper.interner)
            .map_err(handle_error)?;
        if placed_topics != stats.topics {
            return Err(FixupError::HandleError(format!(
                "carry_fo76_vanilla_info_conditions placed {placed_topics} of {} topics",
                stats.topics
            )));
        }
        for (topic_local, infos) in overrides_by_topic {
            for info in infos {
                if session
                    .add_topic_child_record(info, topic_local, schema.as_ref(), mapper.interner)
                    .map_err(handle_error)?
                {
                    stats.emitted += 1;
                }
            }
        }

        report.records_added =
            u32::try_from(stats.emitted + stats.topics + stats.quests).unwrap_or(u32::MAX);
        // Always reported: a regen log line is the only place these counts show up.
        report.diagnostics.push(mapper.interner.intern(&stats.to_string()));
        Ok(report)
    }
}

struct SourceInfoConditions {
    local: u32,
    topic: u32,
    rows: Vec<Vec<u8>>,
    has_condition_strings: bool,
}

#[derive(Clone, Copy)]
enum Skip {
    OrRow,
    ConditionStrings,
    Untranslatable,
}

#[derive(Default)]
struct CarryStats {
    candidates: usize,
    already_present: usize,
    unchanged: usize,
    or_rows: usize,
    condition_strings: usize,
    untranslatable: usize,
    unencodable: usize,
    emitted: usize,
    topics: usize,
    quests: usize,
}

impl CarryStats {
    fn count_skip(&mut self, skip: Skip) {
        match skip {
            Skip::OrRow => self.or_rows += 1,
            Skip::ConditionStrings => self.condition_strings += 1,
            Skip::Untranslatable => self.untranslatable += 1,
        }
    }
}

impl std::fmt::Display for CarryStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "carry_fo76_vanilla_info_conditions: candidates={} already_present={} unchanged={} \
             skipped_or={} skipped_condition_strings={} skipped_untranslatable={} \
             skipped_unencodable={} emitted_infos={} topics={} quests={}",
            self.candidates,
            self.already_present,
            self.unchanged,
            self.or_rows,
            self.condition_strings,
            self.untranslatable,
            self.unencodable,
            self.emitted,
            self.topics,
            self.quests,
        )
    }
}

/// Raw condition rows of every source INFO that sits, under the same ids, in a Fallout4.esm topic.
fn collect_source_info_conditions(
    items: &[ParsedItem],
    fallout4_infos: &FxHashSet<u32>,
    fallout4_topics: &FxHashSet<u32>,
    out: &mut Vec<SourceInfoConditions>,
) {
    for item in items {
        let ParsedItem::Group(group) = item else {
            continue;
        };
        let topic = u32::from_le_bytes(group.label) & OBJECT_ID_MASK;
        if group.group_type == TOPIC_CHILD_GROUP && fallout4_topics.contains(&topic) {
            for child in &group.children {
                let ParsedItem::Record(record) = child else {
                    continue;
                };
                let local = record.form_id & OBJECT_ID_MASK;
                if !record.signature.eq_ignore_ascii_case("INFO")
                    || !fallout4_infos.contains(&local)
                {
                    continue;
                }
                let subrecords = effective_subrecords_for_record(record);
                out.push(SourceInfoConditions {
                    local,
                    topic,
                    rows: subrecords
                        .iter()
                        .filter(|subrecord| subrecord.signature.eq_ignore_ascii_case("CTDA"))
                        .map(|subrecord| subrecord.data.to_vec())
                        .collect(),
                    has_condition_strings: subrecords.iter().any(|subrecord| {
                        subrecord.signature.eq_ignore_ascii_case("CIS1")
                            || subrecord.signature.eq_ignore_ascii_case("CIS2")
                    }),
                });
            }
        }
        collect_source_info_conditions(&group.children, fallout4_infos, fallout4_topics, out);
    }
}

/// A row's meaning without its padding bytes (1..4, 10..12), which differ between files.
fn row_key(row: &[u8]) -> Option<[u8; 27]> {
    (row.len() >= CTDA_LEN).then(|| {
        let mut key = [0u8; 27];
        key[0] = row[0];
        key[1..7].copy_from_slice(&row[4..10]);
        key[7..27].copy_from_slice(&row[12..32]);
        key
    })
}

fn condition_rows(record: &Record) -> impl Iterator<Item = &[u8]> {
    record.fields.iter().filter_map(|entry| match &entry.value {
        FieldValue::Bytes(bytes) if entry.sig.0 == *b"CTDA" => Some(bytes.as_slice()),
        _ => None,
    })
}

/// FO76's rows that FO4's record lacks, after the translator's function rewrites.
fn added_rows(
    source: SourceInfoConditions,
    master: &Record,
    output_plugin: crate::sym::Sym,
) -> Result<Vec<Vec<u8>>, Skip> {
    let mut normalized = Record::new(
        SigCode(*b"INFO"),
        FormKey {
            local: source.local,
            plugin: output_plugin,
        },
    );
    normalized.fields.extend(source.rows.into_iter().map(|row| FieldEntry {
        sig: SubrecordSig(*b"CTDA"),
        value: FieldValue::Bytes(SmallVec::from_vec(row)),
    }));
    normalize_fo76_raw_condition_rows(&mut normalized);

    let master_keys: FxHashSet<[u8; 27]> = condition_rows(master).filter_map(row_key).collect();
    let added: Vec<Vec<u8>> = condition_rows(&normalized)
        .filter(|row| row_key(row).is_some_and(|key| !master_keys.contains(&key)))
        .map(<[u8]>::to_vec)
        .collect();
    if added.is_empty() {
        return Ok(added);
    }
    if source.has_condition_strings {
        return Err(Skip::ConditionStrings);
    }
    if added.iter().any(|row| row[0] & CTDA_OR_FLAG != 0) {
        return Err(Skip::OrRow);
    }
    if added.iter().any(|row| {
        is_fo4_incompatible_condition_function_id(u16::from_le_bytes([row[8], row[9]]))
    }) {
        return Err(Skip::Untranslatable);
    }
    Ok(added)
}

/// Re-encode FO76 FormIDs for the target, or `None` when any row names a form with no target.
fn encode_rows(
    rows: Vec<Vec<u8>>,
    encoded_targets: &FxHashMap<u32, u32>,
    fallout4_object_ids: &FxHashSet<u32>,
) -> Option<Vec<FieldEntry>> {
    let mut is_fallout4_form = |raw: u32| fallout4_object_ids.contains(&(raw & OBJECT_ID_MASK));
    rows.into_iter()
        .map(|mut row| {
            match rewrite_raw_condition_bytes(&mut row, encoded_targets, &mut is_fallout4_form) {
                RawConditionRewrite::Drop => None,
                RawConditionRewrite::Unchanged | RawConditionRewrite::Changed => Some(FieldEntry {
                    sig: SubrecordSig(*b"CTDA"),
                    value: FieldValue::Bytes(SmallVec::from_vec(row)),
                }),
            }
        })
        .collect()
}

/// Leading AND rows cannot join an OR group further down the list.
fn prepend_conditions(record: &mut Record, conditions: Vec<FieldEntry>) {
    const RESPONSE_ANCHORS: &[[u8; 4]] = &[*b"TRDA", *b"NAM1", *b"NAM0"];
    let at = record
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"CTDA")
        .or_else(|| {
            record
                .fields
                .iter()
                .position(|entry| RESPONSE_ANCHORS.contains(&entry.sig.0))
        })
        .unwrap_or(record.fields.len());
    record.fields.insert_many(at, conditions);
    if record.fields.iter().any(|entry| entry.sig.0 == *b"CITC") {
        record.sync_condition_count();
    }
}

fn topic_quest(topic: &Record) -> Option<FormKey> {
    topic
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"QNAM")
        .and_then(|entry| match entry.value {
            FieldValue::FormKey(quest) => Some(quest),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::record::RecordFlags;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

    const QUEST_CHILD_GROUP: i32 = 10;
    const INFO_GROUP_FLAG: u32 = 0x40;
    const CREATURE_DIALOGUE_MR_HANDY: u32 = 0x04_64FB;
    const MR_HANDY_HELLO_TOPIC: u32 = 0x04_64FC;
    const MR_HANDY_GENERIC_HELLO_GROUP: u32 = 0x1C_D029;
    const UNCHANGED_INFO: u32 = 0x04_6508;
    const OR_GATED_INFO: u32 = 0x04_6509;
    const FO4_CODSWORTH: u32 = 0x01_79FF;
    const ROBOT_MR_HANDY_VOICE: u32 = 0x01_9FE3;
    const CHARGEN_LOCATION_KEYWORD: u32 = 0x59_12D5;
    const GOODBYE_KIOSK_MR_HANDY: u32 = 0x4F_79ED;
    const LOCATION_HAS_KEYWORD: u16 = 562;
    const GET_IS_ID: u16 = 72;
    const GET_IS_VOICE_TYPE: u16 = 426;
    const GET_QUEST_RUNNING_UNIQUE: u16 = 856;
    const GET_QUEST_RUNNING: u16 = 56;

    fn ctda(operator: u8, function_id: u16, parameter_1: u32, value: f32, padding: u8) -> Vec<u8> {
        let mut bytes = vec![padding; CTDA_LEN];
        bytes[0] = operator;
        bytes[4..8].copy_from_slice(&value.to_le_bytes());
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[16..20].fill(0);
        bytes[20..24].fill(0);
        bytes[24..28].fill(0);
        bytes[28..32].copy_from_slice(&(-1i32).to_le_bytes());
        bytes
    }

    fn record(sig: &str, form_key: FormKey, fields: Vec<(&[u8; 4], FieldValue)>) -> Record {
        let mut record = Record::new(SigCode::from_str(sig).unwrap(), form_key);
        record.fields.extend(fields.into_iter().map(|(sig, value)| FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }));
        record
    }

    fn bytes(data: Vec<u8>) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(data))
    }

    fn info(form_key: FormKey, rows: Vec<Vec<u8>>, group: bool) -> Record {
        let mut fields = vec![(b"ENAM", bytes(vec![0, 3, 0, 0]))];
        fields.extend(rows.into_iter().map(|row| (b"CTDA", bytes(row))));
        let mut info = record("INFO", form_key, fields);
        if group {
            info.flags = RecordFlags::from_bits_retain(INFO_GROUP_FLAG);
        }
        info
    }

    /// Writes CreatureDialogueMrHandy → its Hello topic → `infos` into `handle`.
    fn write_mr_handy_dialogue(
        handle: u64,
        plugin: crate::sym::Sym,
        infos: Vec<Record>,
        game: &str,
        interner: &StringInterner,
    ) {
        let key = |local| FormKey { local, plugin };
        let schema = crate::schema::AuthoringSchema::for_game(game).unwrap();
        let mut session = open_session(handle, None).unwrap();
        session
            .add_record(
                record(
                    "QUST",
                    key(CREATURE_DIALOGUE_MR_HANDY),
                    vec![(b"DNAM", bytes(vec![0x11, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0]))],
                ),
                schema.as_ref(),
                interner,
            )
            .unwrap();
        session
            .add_quest_child_records(
                vec![record(
                    "DIAL",
                    key(MR_HANDY_HELLO_TOPIC),
                    vec![
                        (b"PNAM", FieldValue::Float(50.0)),
                        (b"QNAM", FieldValue::FormKey(key(CREATURE_DIALOGUE_MR_HANDY))),
                        (b"SNAM", bytes(b"HELO".to_vec())),
                        (b"TIFC", FieldValue::Uint(3)),
                    ],
                )],
                schema.as_ref(),
                interner,
            )
            .unwrap();
        for info in infos {
            assert!(
                session
                    .add_topic_child_record(info, MR_HANDY_HELLO_TOPIC, schema.as_ref(), interner)
                    .unwrap()
            );
        }
    }

    fn condition_summary(record: &Record) -> Vec<(u16, u32, f32)> {
        condition_rows(record)
            .map(|row| {
                (
                    u16::from_le_bytes([row[8], row[9]]),
                    u32::from_le_bytes(row[12..16].try_into().unwrap()),
                    f32::from_le_bytes(row[4..8].try_into().unwrap()),
                )
            })
            .collect()
    }

    fn info_parents(items: &[ParsedItem], form_id: u32) -> Option<(u32, u32)> {
        for item in items {
            let ParsedItem::Group(group) = item else {
                continue;
            };
            if group.group_type == QUEST_CHILD_GROUP {
                for child in &group.children {
                    if let ParsedItem::Group(topic) = child
                        && topic.group_type == TOPIC_CHILD_GROUP
                        && topic.children.iter().any(|info| {
                            matches!(info, ParsedItem::Record(info) if info.form_id == form_id)
                        })
                    {
                        return Some((
                            u32::from_le_bytes(group.label),
                            u32::from_le_bytes(topic.label),
                        ));
                    }
                }
            }
            if let Some(found) = info_parents(&group.children, form_id) {
                return Some(found);
            }
        }
        None
    }

    /// The Vault 76 Mr. Handy case: FO76 gates FO4's generic hello off in CharGen, drops the
    /// Codsworth row (no Codsworth in Appalachia) and moves a row to an FO76-only function id.
    #[test]
    fn carries_fo76_gates_onto_fallout4_info_overrides() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern(FO4_MASTER_NAME);
        let source_plugin = interner.intern("SeventySix.esm");
        let output_plugin = source_plugin;
        let master_key = |local| FormKey {
            local,
            plugin: fallout4,
        };
        let source_key = |local| FormKey {
            local,
            plugin: source_plugin,
        };
        let fo4_schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let source_handle = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let fallout4_handle = plugin_handle_new_native(FO4_MASTER_NAME, Some("fo4")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, FO4_MASTER_NAME, None).unwrap();

        let voice = ctda(0, GET_IS_VOICE_TYPE, ROBOT_MR_HANDY_VOICE, 1.0, 0xFF);
        write_mr_handy_dialogue(
            fallout4_handle,
            fallout4,
            vec![
                info(
                    master_key(MR_HANDY_GENERIC_HELLO_GROUP),
                    vec![ctda(0, GET_IS_ID, FO4_CODSWORTH, 0.0, 0xFF), voice.clone()],
                    true,
                ),
                info(master_key(UNCHANGED_INFO), vec![voice.clone()], false),
                info(master_key(OR_GATED_INFO), vec![voice.clone()], false),
            ],
            "fo4",
            &interner,
        );
        // Same rows with other padding bytes: still "unchanged".
        let source_voice = ctda(0, GET_IS_VOICE_TYPE, ROBOT_MR_HANDY_VOICE, 1.0, 0x44);
        write_mr_handy_dialogue(
            source_handle,
            source_plugin,
            vec![
                info(
                    source_key(MR_HANDY_GENERIC_HELLO_GROUP),
                    vec![
                        ctda(0, LOCATION_HAS_KEYWORD, CHARGEN_LOCATION_KEYWORD, 0.0, 0x44),
                        ctda(0, GET_IS_ID, GOODBYE_KIOSK_MR_HANDY, 0.0, 0x44),
                        ctda(0, GET_QUEST_RUNNING_UNIQUE, CREATURE_DIALOGUE_MR_HANDY, 1.0, 0x44),
                        source_voice.clone(),
                    ],
                    true,
                ),
                info(source_key(UNCHANGED_INFO), vec![source_voice.clone()], false),
                info(
                    source_key(OR_GATED_INFO),
                    vec![
                        ctda(CTDA_OR_FLAG, GET_IS_ID, GOODBYE_KIOSK_MR_HANDY, 0.0, 0x44),
                        source_voice,
                    ],
                    false,
                ),
            ],
            "fo76",
            &interner,
        );

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                ..Default::default()
            },
        );
        for (source, target) in [
            (CHARGEN_LOCATION_KEYWORD, FormKey { local: CHARGEN_LOCATION_KEYWORD, plugin: output_plugin }),
            (GOODBYE_KIOSK_MR_HANDY, FormKey { local: GOODBYE_KIOSK_MR_HANDY, plugin: output_plugin }),
            (CREATURE_DIALOGUE_MR_HANDY, master_key(CREATURE_DIALOGUE_MR_HANDY)),
            (ROBOT_MR_HANDY_VOICE, master_key(ROBOT_MR_HANDY_VOICE)),
            // FO76's 0179FF is an unrelated REFR: FO4's Codsworth row must not follow it.
            (FO4_CODSWORTH, FormKey { local: FO4_CODSWORTH, plugin: output_plugin }),
        ] {
            state.source_to_target.insert(source_key(source), target);
        }
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let config = FixupConfig {
            target_master_handle_ids: vec![fallout4_handle],
            ..Default::default()
        };
        let mut session = open_session(target_handle, Some(source_handle)).unwrap();
        let report = CarryFo76VanillaInfoConditionsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        let diagnostic = interner.resolve(report.diagnostics[0]).unwrap().to_string();
        assert!(
            diagnostic.contains("candidates=3")
                && diagnostic.contains("unchanged=1")
                && diagnostic.contains("skipped_or=1")
                && diagnostic.contains("emitted_infos=1 topics=1 quests=1"),
            "{diagnostic}"
        );
        assert_eq!(report.records_added, 3);

        let override_group = session
            .record_decoded(&master_key(MR_HANDY_GENERIC_HELLO_GROUP), fo4_schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(override_group.flags.bits() & INFO_GROUP_FLAG, INFO_GROUP_FLAG);
        // Output-plugin forms take load index 1; FO4's rows keep their Fallout4.esm ids.
        assert_eq!(
            condition_summary(&override_group),
            vec![
                (LOCATION_HAS_KEYWORD, 0x0100_0000 | CHARGEN_LOCATION_KEYWORD, 0.0),
                (GET_IS_ID, 0x0100_0000 | GOODBYE_KIOSK_MR_HANDY, 0.0),
                (GET_QUEST_RUNNING, CREATURE_DIALOGUE_MR_HANDY, 1.0),
                (GET_IS_ID, FO4_CODSWORTH, 0.0),
                (GET_IS_VOICE_TYPE, ROBOT_MR_HANDY_VOICE, 1.0),
            ]
        );
        assert_eq!(
            info_parents(&session.target_slot().parsed.root_items, MR_HANDY_GENERIC_HELLO_GROUP),
            Some((CREATURE_DIALOGUE_MR_HANDY, MR_HANDY_HELLO_TOPIC)),
            "the override must sit under its master QUST and DIAL"
        );
        for skipped in [UNCHANGED_INFO, OR_GATED_INFO] {
            assert!(
                session
                    .record_decoded(&master_key(skipped), fo4_schema.as_ref(), &interner)
                    .is_err(),
                "{skipped:06X} must not be overridden"
            );
        }

        let rerun = CarryFo76VanillaInfoConditionsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(rerun.records_added, 0);

        drop(session);
        assert!(plugin_handle_close_native(target_handle));
        assert!(plugin_handle_close_native(fallout4_handle));
        assert!(plugin_handle_close_native(source_handle));
    }
}
