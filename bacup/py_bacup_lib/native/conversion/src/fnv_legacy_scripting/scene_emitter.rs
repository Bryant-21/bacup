use std::collections::{HashMap, HashSet};

use smallvec::SmallVec;
use thiserror::Error;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

use super::dialogue_graph::SceneBranchPlan;

const MAX_VANILLA_PLAYER_CHOICES: usize = 4;
const SCENE_FLAGS: u32 = 0x0000_1024;
const PLAYER_DIALOGUE_ACTION: u16 = 3;
const PLAYER_DIALOGUE_ACTION_FLAGS: u32 = 0x0022_8000;
const RUN_ONLY_SCENE_PACKAGES: u32 = 4;
const DEATH_END_COMBAT_END: u32 = 10;
const DIALOGUE_BRANCH_PLAYER_CATEGORY: u32 = 0;
const DIALOGUE_BRANCH_TOP_LEVEL: u32 = 1;

#[derive(Debug, Clone)]
pub struct SceneRuntimeOptions {
    pub scene_editor_id: String,
    pub branch_editor_id: String,
    pub quest_start_game_enabled: bool,
    pub output_is_master: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneTopicRuntime {
    pub topic: FormKey,
    pub info_count: u32,
    pub condition_count: usize,
    pub actor_alias: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct SceneRuntimeManifest {
    pub quest: FormKey,
    pub scene: Record,
    pub branch: Record,
    pub topics: Vec<Record>,
    pub action_topics: Vec<FormKey>,
    pub zero_info_topics: Vec<FormKey>,
    pub topic_runtime: Vec<SceneTopicRuntime>,
    pub requires_seq: bool,
}

impl SceneRuntimeManifest {
    pub fn into_quest_child_write_order(self) -> Vec<Record> {
        let mut records = Vec::with_capacity(self.topics.len() + 2);
        records.push(self.branch);
        records.extend(self.topics);
        records.push(self.scene);
        records
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SceneEmitError {
    #[error("scene plan references missing DIAL {topic:06X}")]
    MissingTopic { topic: u32 },
    #[error("scene input contains DIAL {topic:06X} outside the bounded plan")]
    UnexpectedTopic { topic: u32 },
    #[error("DIAL {topic:06X} does not belong to quest {quest:06X}")]
    WrongQuestOwner { topic: u32, quest: u32 },
    #[error("DIAL {topic:06X} declares {declared} INFOs but topology contains {actual}")]
    InfoCountMismatch {
        topic: u32,
        declared: u32,
        actual: u32,
    },
    #[error("INFO {info:06X} under DIAL {topic:06X} has no concrete speaker")]
    MissingInfoSpeaker { topic: u32, info: u32 },
    #[error("INFO {info:06X} speaker {speaker:06X} has no quest alias")]
    UnresolvedSpeakerAlias { info: u32, speaker: u32 },
    #[error("DIAL {topic:06X} resolves to multiple actor aliases")]
    AmbiguousTopicAlias { topic: u32 },
    #[error("scene choices resolve to multiple dialogue-target aliases")]
    MixedActionAliases,
    #[error("DIAL {topic:06X} scene subtype does not match runnable state {runnable}")]
    SceneSubtypeMismatch { topic: u32, runnable: bool },
    #[error("scene has no topic with a runnable INFO")]
    NoRunnableTopics,
    #[error("scene requires {choices} player choices; vanilla FO4 supports at most four")]
    ChoiceLimit { choices: usize },
    #[error("StartGameEnabled quest in a non-master output requires an external SEQ artifact")]
    SeqRequiredForPlugin,
}

pub fn emit_fo4_scene_runtime(
    plan: &SceneBranchPlan,
    mut topics: Vec<Record>,
    infos_by_parent: &[(FormKey, Record)],
    alias_by_speaker: &HashMap<FormKey, i32>,
    options: &SceneRuntimeOptions,
    interner: &StringInterner,
) -> Result<SceneRuntimeManifest, SceneEmitError> {
    if options.quest_start_game_enabled && !options.output_is_master {
        return Err(SceneEmitError::SeqRequiredForPlugin);
    }

    let planned = plan.ordered_topics.iter().copied().collect::<HashSet<_>>();
    for topic in &topics {
        if !planned.contains(&topic.form_key) {
            return Err(SceneEmitError::UnexpectedTopic {
                topic: topic.form_key.local,
            });
        }
    }
    let mut topic_index = topics
        .iter()
        .enumerate()
        .map(|(index, topic)| (topic.form_key, index))
        .collect::<HashMap<_, _>>();
    let mut ordered_topic_indices = Vec::with_capacity(plan.ordered_topics.len());
    for topic in &plan.ordered_topics {
        ordered_topic_indices.push(
            topic_index
                .remove(topic)
                .ok_or(SceneEmitError::MissingTopic { topic: topic.local })?,
        );
    }

    let mut runtime = Vec::with_capacity(plan.ordered_topics.len());
    let mut action_topics = Vec::new();
    let mut zero_info_topics = Vec::new();
    let mut action_aliases = HashSet::new();
    for (topic, index) in plan
        .ordered_topics
        .iter()
        .copied()
        .zip(ordered_topic_indices.iter().copied())
    {
        let record = &topics[index];
        if record_quest(record) != Some(plan.quest) {
            return Err(SceneEmitError::WrongQuestOwner {
                topic: topic.local,
                quest: plan.quest.local,
            });
        }
        let infos = infos_by_parent
            .iter()
            .filter(|(parent, _)| *parent == topic)
            .map(|(_, info)| info)
            .collect::<Vec<_>>();
        let declared = record_info_count(record).unwrap_or(0);
        let actual = infos.len() as u32;
        if declared != actual {
            return Err(SceneEmitError::InfoCountMismatch {
                topic: topic.local,
                declared,
                actual,
            });
        }
        let condition_count = infos
            .iter()
            .flat_map(|info| info.fields.iter())
            .filter(|field| field.sig.0 == *b"CTDA")
            .count();
        let actor_alias = if infos.is_empty() {
            zero_info_topics.push(topic);
            None
        } else {
            let mut aliases = HashSet::new();
            for info in infos {
                let speaker = info
                    .fields
                    .iter()
                    .rev()
                    .find(|field| field.sig.0 == *b"ANAM")
                    .and_then(|field| first_form_key(&field.value))
                    .ok_or(SceneEmitError::MissingInfoSpeaker {
                        topic: topic.local,
                        info: info.form_key.local,
                    })?;
                aliases.insert(*alias_by_speaker.get(&speaker).ok_or(
                    SceneEmitError::UnresolvedSpeakerAlias {
                        info: info.form_key.local,
                        speaker: speaker.local,
                    },
                )?);
            }
            if aliases.len() != 1 {
                return Err(SceneEmitError::AmbiguousTopicAlias { topic: topic.local });
            }
            let alias = aliases.into_iter().next().unwrap();
            action_aliases.insert(alias);
            action_topics.push(topic);
            Some(alias)
        };
        runtime.push(SceneTopicRuntime {
            topic,
            info_count: actual,
            condition_count,
            actor_alias,
        });
    }

    if action_topics.is_empty() {
        return Err(SceneEmitError::NoRunnableTopics);
    }
    if action_topics.len() > MAX_VANILLA_PLAYER_CHOICES {
        return Err(SceneEmitError::ChoiceLimit {
            choices: action_topics.len(),
        });
    }
    if action_aliases.len() != 1 {
        return Err(SceneEmitError::MixedActionAliases);
    }
    for topic in &topics {
        let runnable = action_topics.contains(&topic.form_key);
        if !topic_scene_projection_matches(topic, runnable) {
            return Err(SceneEmitError::SceneSubtypeMismatch {
                topic: topic.form_key.local,
                runnable,
            });
        }
    }
    let actor_alias = action_aliases.into_iter().next().unwrap();
    let starting_topic = if action_topics.contains(&plan.starting_topic) {
        plan.starting_topic
    } else {
        action_topics[0]
    };

    for topic in &mut topics {
        attach_branch(topic, plan.branch);
    }
    topics.sort_by_key(|topic| {
        plan.ordered_topics
            .iter()
            .position(|planned| *planned == topic.form_key)
            .unwrap_or(usize::MAX)
    });

    let branch = build_branch(plan, starting_topic, &options.branch_editor_id, interner);
    let scene = build_scene(
        plan,
        &action_topics,
        actor_alias,
        &options.scene_editor_id,
        interner,
    );
    Ok(SceneRuntimeManifest {
        quest: plan.quest,
        scene,
        branch,
        topics,
        action_topics,
        zero_info_topics,
        topic_runtime: runtime,
        requires_seq: false,
    })
}

fn build_branch(
    plan: &SceneBranchPlan,
    starting_topic: FormKey,
    editor_id: &str,
    interner: &StringInterner,
) -> Record {
    let mut branch = Record::new(SigCode(*b"DLBR"), plan.branch);
    let editor_id = interner.intern(editor_id);
    branch.eid = Some(editor_id);
    branch.fields.extend([
        field("EDID", FieldValue::String(editor_id)),
        field("QNAM", FieldValue::FormKey(plan.quest)),
        field(
            "TNAM",
            FieldValue::Uint(u64::from(DIALOGUE_BRANCH_PLAYER_CATEGORY)),
        ),
        field(
            "DNAM",
            FieldValue::Uint(u64::from(DIALOGUE_BRANCH_TOP_LEVEL)),
        ),
        field("SNAM", FieldValue::FormKey(starting_topic)),
    ]);
    branch
}

fn build_scene(
    plan: &SceneBranchPlan,
    action_topics: &[FormKey],
    actor_alias: i32,
    editor_id: &str,
    interner: &StringInterner,
) -> Record {
    let mut scene = Record::new(SigCode(*b"SCEN"), plan.scene);
    let editor_id = interner.intern(editor_id);
    scene.eid = Some(editor_id);
    scene.fields.extend([
        field("EDID", FieldValue::String(editor_id)),
        field("FNAM", FieldValue::Uint(u64::from(SCENE_FLAGS))),
        field("HNAM", FieldValue::None),
        field(
            "NAM0",
            FieldValue::String(interner.intern("Player Decides")),
        ),
        field("NEXT", FieldValue::None),
        field("NEXT", FieldValue::None),
        field("WNAM", FieldValue::Uint(491)),
        field("HNAM", FieldValue::None),
        field("ALID", FieldValue::Int(i64::from(actor_alias))),
        field("LNAM", FieldValue::Uint(u64::from(RUN_ONLY_SCENE_PACKAGES))),
        field("DNAM", FieldValue::Uint(u64::from(DEATH_END_COMBAT_END))),
        field("ANAM", FieldValue::Uint(u64::from(PLAYER_DIALOGUE_ACTION))),
        field("NAM0", FieldValue::String(interner.intern(""))),
        field("ALID", FieldValue::Int(i64::from(actor_alias))),
        field("INAM", FieldValue::Uint(1)),
        field(
            "FNAM",
            FieldValue::Uint(u64::from(PLAYER_DIALOGUE_ACTION_FLAGS)),
        ),
        field("SNAM", FieldValue::Uint(0)),
        field("ENAM", FieldValue::Uint(0)),
    ]);
    for (signature, topic) in ["PTOP", "NTOP", "NETO", "QTOP"]
        .into_iter()
        .zip(action_topics.iter().copied())
    {
        scene
            .fields
            .push(field(signature, FieldValue::FormKey(topic)));
    }
    for (signature, topic) in ["NPOT", "NNGT", "NNUT", "NQUT"]
        .into_iter()
        .zip(action_topics.iter().copied())
    {
        scene
            .fields
            .push(field(signature, FieldValue::FormKey(topic)));
    }
    scene.fields.extend([
        field("DTGT", FieldValue::Int(i64::from(actor_alias))),
        field("ANAM", FieldValue::None),
        field("PNAM", FieldValue::FormKey(plan.quest)),
        field("INAM", FieldValue::Uint(2)),
        field(
            "VNAM",
            FieldValue::Bytes(SmallVec::from_slice(&[
                3, 0, 0, 0, 3, 0, 0, 0, 3, 0, 0, 0, 3, 0, 0, 0,
            ])),
        ),
        field("XNAM", FieldValue::Uint(0)),
    ]);
    scene
}

fn attach_branch(topic: &mut Record, branch: FormKey) {
    let mut qnam = Vec::new();
    let mut data = Vec::new();
    let mut snam = Vec::new();
    let mut tifc = Vec::new();
    topic.fields.retain(|entry| match entry.sig.0 {
        sig if sig == *b"BNAM" => false,
        sig if sig == *b"QNAM" => {
            qnam.push(entry.clone());
            false
        }
        sig if sig == *b"DATA" => {
            data.push(entry.clone());
            false
        }
        sig if sig == *b"SNAM" => {
            snam.push(entry.clone());
            false
        }
        sig if sig == *b"TIFC" => {
            tifc.push(entry.clone());
            false
        }
        _ => true,
    });
    topic
        .fields
        .push(field("BNAM", FieldValue::FormKey(branch)));
    topic.fields.extend(qnam);
    topic.fields.extend(data);
    topic.fields.extend(snam);
    topic.fields.extend(tifc);
}

fn record_quest(record: &Record) -> Option<FormKey> {
    record
        .fields
        .iter()
        .rev()
        .find(|field| field.sig.0 == *b"QNAM")
        .and_then(|field| first_form_key(&field.value))
}

fn record_info_count(record: &Record) -> Option<u32> {
    record
        .fields
        .iter()
        .rev()
        .find(|field| field.sig.0 == *b"TIFC")
        .and_then(|field| first_u32(&field.value))
}

fn topic_scene_projection_matches(record: &Record, runnable: bool) -> bool {
    let scene_data = record.fields.iter().rev().any(|field| {
        field.sig.0 == *b"DATA"
            && matches!(&field.value, FieldValue::Bytes(bytes)
                if bytes.len() >= 4 && bytes[1] == 2 && bytes[2..4] == 14_u16.to_le_bytes())
    });
    let scene_name = record.fields.iter().rev().any(|field| {
        field.sig.0 == *b"SNAM"
            && matches!(&field.value, FieldValue::Bytes(bytes) if bytes.as_slice() == b"SCEN")
    });
    if runnable {
        scene_data && scene_name
    } else {
        !scene_data && !scene_name
    }
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::List(values) => values.iter().find_map(first_form_key),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        _ => None,
    }
}

fn first_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[..4].try_into().unwrap()))
        }
        FieldValue::List(values) => values.iter().find_map(first_u32),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_u32(value)),
        _ => None,
    }
}

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).expect("literal subrecord signature"),
        value,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::record::RecordFlags;
    use crate::schema::AuthoringSchema;
    use crate::target_write::{
        add_quest_child_record_native, add_record_native, add_topic_child_record_native,
    };
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, plugin_handle_close_native, plugin_handle_load_no_py, plugin_handle_new_no_py,
        plugin_handle_save_no_py, plugin_handle_store_ref,
    };

    const QUEST: u32 = 0x11F935;
    const ACCEPTANCE: u32 = 0x13015B;
    const ACCEPTANCE_INFO: u32 = 0x130161;
    const COMPLETION: u32 = 0x134B9A;
    const COMPLETION_INFO: u32 = 0x134B9B;
    const ZERO_INFO: u32 = 0x138A74;
    const SCENE: u32 = 0x13A100;
    const BRANCH: u32 = 0x13A101;
    const SPEAKER: u32 = 0x1300F0;

    fn fk(local: u32, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("Output.esm"),
        }
    }

    fn topic(local: u32, count: u32, interner: &StringInterner) -> Record {
        let mut record = Record::new(SigCode(*b"DIAL"), fk(local, interner));
        record.eid = Some(interner.intern(&format!("Topic_{local:06X}")));
        let flags = if local == COMPLETION { 0x20 } else { 0 };
        let data = if count > 0 {
            [flags, 2, 14, 0]
        } else {
            [flags, 0, 0, 0]
        };
        record.fields.extend([
            field("FULL", FieldValue::String(interner.intern("Choice"))),
            field("PNAM", FieldValue::Float(50.0)),
            field("DATA", FieldValue::Bytes(SmallVec::from_slice(&data))),
            field("QNAM", FieldValue::FormKey(fk(QUEST, interner))),
        ]);
        if count > 0 {
            record.fields.push(field(
                "SNAM",
                FieldValue::Bytes(SmallVec::from_slice(b"SCEN")),
            ));
        }
        record
            .fields
            .push(field("TIFC", FieldValue::Uint(u64::from(count))));
        record
    }

    fn info(local: u32, conditions: usize, interner: &StringInterner) -> Record {
        let mut record = Record::new(SigCode(*b"INFO"), fk(local, interner));
        for _ in 0..conditions {
            record.fields.push(field(
                "CTDA",
                FieldValue::Bytes(SmallVec::from_vec(vec![0; 32])),
            ));
        }
        record
            .fields
            .push(field("ANAM", FieldValue::FormKey(fk(SPEAKER, interner))));
        record
    }

    fn plan(interner: &StringInterner) -> SceneBranchPlan {
        SceneBranchPlan {
            quest: fk(QUEST, interner),
            scene: fk(SCENE, interner),
            branch: fk(BRANCH, interner),
            starting_topic: fk(COMPLETION, interner),
            ordered_topics: vec![
                fk(COMPLETION, interner),
                fk(ZERO_INFO, interner),
                fk(ACCEPTANCE, interner),
            ],
            edges: Vec::new(),
        }
    }

    fn emit_topics(
        topics: Vec<Record>,
        interner: &StringInterner,
    ) -> Result<SceneRuntimeManifest, SceneEmitError> {
        emit_fo4_scene_runtime(
            &plan(interner),
            topics,
            &[
                (fk(ACCEPTANCE, interner), info(ACCEPTANCE_INFO, 1, interner)),
                (fk(COMPLETION, interner), info(COMPLETION_INFO, 4, interner)),
            ],
            &HashMap::from([(fk(SPEAKER, interner), 2)]),
            &SceneRuntimeOptions {
                scene_editor_id: "B21_VTechatticupDialogueScene".into(),
                branch_editor_id: "B21_VTechatticupDialogueBranch".into(),
                quest_start_game_enabled: true,
                output_is_master: true,
            },
            interner,
        )
    }

    fn emit(interner: &StringInterner) -> SceneRuntimeManifest {
        emit_topics(
            vec![
                topic(ACCEPTANCE, 1, interner),
                topic(COMPLETION, 1, interner),
                topic(ZERO_INFO, 0, interner),
            ],
            interner,
        )
        .unwrap()
    }

    fn form_keys(record: &Record, signatures: &[[u8; 4]]) -> Vec<FormKey> {
        record
            .fields
            .iter()
            .filter(|field| signatures.contains(&field.sig.0))
            .filter_map(|field| first_form_key(&field.value))
            .collect()
    }

    #[test]
    fn vtechatticup_manifest_covers_acceptance_completion_and_zero_info_topic() {
        let interner = StringInterner::new();
        let manifest = emit(&interner);

        assert_eq!(
            manifest.action_topics,
            [fk(COMPLETION, &interner), fk(ACCEPTANCE, &interner)]
        );
        assert_eq!(manifest.zero_info_topics, [fk(ZERO_INFO, &interner)]);
        assert!(!manifest.requires_seq);
        assert_eq!(
            form_keys(&manifest.scene, &[*b"PTOP", *b"NTOP", *b"NETO", *b"QTOP"]),
            manifest.action_topics
        );
        assert_eq!(
            form_keys(&manifest.scene, &[*b"NPOT", *b"NNGT", *b"NNUT", *b"NQUT"]),
            manifest.action_topics
        );
        assert_eq!(
            form_keys(&manifest.branch, &[*b"SNAM"]),
            [fk(COMPLETION, &interner)]
        );
        assert!(
            manifest
                .topics
                .iter()
                .all(|topic| { form_keys(topic, &[*b"BNAM"]) == [fk(BRANCH, &interner)] })
        );
        let counts = manifest
            .topic_runtime
            .iter()
            .map(|topic| {
                (
                    topic.topic.local,
                    (topic.info_count, topic.condition_count, topic.actor_alias),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(counts[&ACCEPTANCE], (1, 1, Some(2)));
        assert_eq!(counts[&COMPLETION], (1, 4, Some(2)));
        assert_eq!(counts[&ZERO_INFO], (0, 0, None));
    }

    #[test]
    fn unresolved_speaker_alias_fails_closed() {
        let interner = StringInterner::new();
        let error = emit_fo4_scene_runtime(
            &SceneBranchPlan {
                quest: fk(QUEST, &interner),
                scene: fk(SCENE, &interner),
                branch: fk(BRANCH, &interner),
                starting_topic: fk(ACCEPTANCE, &interner),
                ordered_topics: vec![fk(ACCEPTANCE, &interner)],
                edges: Vec::new(),
            },
            vec![topic(ACCEPTANCE, 1, &interner)],
            &[(
                fk(ACCEPTANCE, &interner),
                info(ACCEPTANCE_INFO, 1, &interner),
            )],
            &HashMap::new(),
            &SceneRuntimeOptions {
                scene_editor_id: "Scene".into(),
                branch_editor_id: "Branch".into(),
                quest_start_game_enabled: false,
                output_is_master: true,
            },
            &interner,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            SceneEmitError::UnresolvedSpeakerAlias {
                info: ACCEPTANCE_INFO,
                speaker: SPEAKER
            }
        ));
    }

    #[test]
    fn runnable_and_zero_info_topic_subtypes_must_be_disjoint() {
        let interner = StringInterner::new();
        let mut missing_scene = vec![
            topic(ACCEPTANCE, 1, &interner),
            topic(COMPLETION, 1, &interner),
            topic(ZERO_INFO, 0, &interner),
        ];
        missing_scene[0]
            .fields
            .retain(|field| field.sig.0 != *b"SNAM");
        assert_eq!(
            emit_topics(missing_scene, &interner).unwrap_err(),
            SceneEmitError::SceneSubtypeMismatch {
                topic: ACCEPTANCE,
                runnable: true,
            }
        );

        let mut zero_info_scene = vec![
            topic(ACCEPTANCE, 1, &interner),
            topic(COMPLETION, 1, &interner),
            topic(ZERO_INFO, 0, &interner),
        ];
        let snam_index = zero_info_scene[2].fields.len() - 1;
        zero_info_scene[2].fields.insert(
            snam_index,
            field("SNAM", FieldValue::Bytes(SmallVec::from_slice(b"SCEN"))),
        );
        assert_eq!(
            emit_topics(zero_info_scene, &interner).unwrap_err(),
            SceneEmitError::SceneSubtypeMismatch {
                topic: ZERO_INFO,
                runnable: false,
            }
        );
    }

    #[test]
    fn start_game_enabled_plugin_requires_seq_contract() {
        let interner = StringInterner::new();
        let mut options = SceneRuntimeOptions {
            scene_editor_id: "Scene".into(),
            branch_editor_id: "Branch".into(),
            quest_start_game_enabled: true,
            output_is_master: false,
        };
        let error = emit_fo4_scene_runtime(
            &plan(&interner),
            Vec::new(),
            &[],
            &HashMap::new(),
            &options,
            &interner,
        )
        .unwrap_err();
        assert_eq!(error, SceneEmitError::SeqRequiredForPlugin);

        options.output_is_master = true;
        let error = emit_fo4_scene_runtime(
            &plan(&interner),
            Vec::new(),
            &[],
            &HashMap::new(),
            &options,
            &interner,
        )
        .unwrap_err();
        assert!(matches!(error, SceneEmitError::MissingTopic { .. }));
    }

    #[test]
    fn emitted_scene_and_branch_survive_save_reopen_in_quest_topology() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let manifest = emit(&interner);
        let infos = [
            (
                fk(ACCEPTANCE, &interner),
                info(ACCEPTANCE_INFO, 1, &interner),
            ),
            (
                fk(COMPLETION, &interner),
                info(COMPLETION_INFO, 4, &interner),
            ),
        ];
        let handle = plugin_handle_new_no_py("Output.esm", Some("fo4"));
        let mut quest = Record::new(SigCode(*b"QUST"), fk(QUEST, &interner));
        quest.eid = Some(interner.intern("VTechatticup"));
        quest.flags = RecordFlags::empty();
        add_record_native(handle, quest, &schema, &interner).unwrap();
        for record in manifest.into_quest_child_write_order() {
            assert!(add_quest_child_record_native(handle, record, &schema, &interner).unwrap());
        }
        for (parent, info) in infos {
            assert!(
                add_topic_child_record_native(handle, info, parent.local, &schema, &interner)
                    .unwrap()
            );
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Output.esm");
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(handle);
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        let quest_group = slot
            .parsed
            .root_items
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"QUST" => {
                    Some(group)
                }
                _ => None,
            })
            .unwrap();
        let children = quest_group
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group)
                    if group.group_type == 10 && group.label == QUEST.to_le_bytes() =>
                {
                    Some(group)
                }
                _ => None,
            })
            .unwrap();
        let record_exists = |signature: &str, form_id: u32| {
            children.children.iter().any(|item| match item {
                ParsedItem::Record(record) => {
                    record.signature.as_str() == signature
                        && (record.form_id & 0x00FF_FFFF) == form_id
                }
                _ => false,
            })
        };
        let direct_record = |signature: &str, form_id: u32| {
            children.children.iter().find_map(move |item| match item {
                ParsedItem::Record(record)
                    if record.signature.as_str() == signature
                        && (record.form_id & 0x00FF_FFFF) == form_id =>
                {
                    Some(record)
                }
                _ => None,
            })
        };
        assert!(record_exists("DLBR", BRANCH));
        assert!(record_exists("SCEN", SCENE));
        for topic in [ACCEPTANCE, COMPLETION, ZERO_INFO] {
            assert!(record_exists("DIAL", topic));
        }
        for (topic, expected_data) in [
            (ACCEPTANCE, vec![0x00, 0x02, 0x0E, 0x00]),
            (COMPLETION, vec![0x20, 0x02, 0x0E, 0x00]),
        ] {
            let topic = direct_record("DIAL", topic).unwrap();
            assert_eq!(raw_payloads(topic, "DATA"), [expected_data]);
            assert_eq!(raw_payloads(topic, "SNAM"), [b"SCEN".to_vec()]);
        }
        assert!(raw_payloads(direct_record("DIAL", ZERO_INFO).unwrap(), "SNAM").is_empty());
        let branch = direct_record("DLBR", BRANCH).unwrap();
        assert_eq!(
            branch
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "EDID")
                .unwrap()
                .data
                .as_ref(),
            b"B21_VTechatticupDialogueBranch\0"
        );
        assert_eq!(raw_form_id(branch, "QNAM") & 0x00FF_FFFF, QUEST);
        assert_eq!(raw_form_id(branch, "SNAM") & 0x00FF_FFFF, COMPLETION);
        assert_eq!(
            raw_signatures(branch),
            ["EDID", "QNAM", "TNAM", "DNAM", "SNAM"]
        );
        let scene = direct_record("SCEN", SCENE).unwrap();
        assert_eq!(
            scene
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "EDID")
                .unwrap()
                .data
                .as_ref(),
            b"B21_VTechatticupDialogueScene\0"
        );
        assert_eq!(raw_u32(scene, "FNAM"), SCENE_FLAGS);
        assert_eq!(raw_u32(scene, "LNAM"), RUN_ONLY_SCENE_PACKAGES);
        assert_eq!(raw_form_id(scene, "PNAM") & 0x00FF_FFFF, QUEST);
        assert_eq!(raw_payloads(scene, "ANAM"), [vec![3, 0], Vec::new()]);
        assert_eq!(
            raw_payloads(scene, "INAM"),
            [1_u32.to_le_bytes().to_vec(), 2_u32.to_le_bytes().to_vec()]
        );
        assert_eq!(
            raw_signatures(scene),
            [
                "EDID", "FNAM", "HNAM", "NAM0", "NEXT", "NEXT", "WNAM", "HNAM", "ALID", "LNAM",
                "DNAM", "ANAM", "NAM0", "ALID", "INAM", "FNAM", "SNAM", "ENAM", "PTOP", "NTOP",
                "NPOT", "NNGT", "DTGT", "ANAM", "PNAM", "INAM", "VNAM", "XNAM",
            ]
        );
        assert_eq!(
            raw_form_ids(scene, &[*b"PTOP", *b"NTOP", *b"NETO", *b"QTOP"]),
            [COMPLETION, ACCEPTANCE]
        );
        assert_eq!(
            raw_form_ids(scene, &[*b"NPOT", *b"NNGT", *b"NNUT", *b"NQUT"]),
            [COMPLETION, ACCEPTANCE]
        );
        assert_eq!(raw_i32(scene, "DTGT"), 2);
        let topic_group_exists = |topic: u32| {
            children.children.iter().any(|item| match item {
                ParsedItem::Group(group) => {
                    group.group_type == 7 && group.label == topic.to_le_bytes()
                }
                _ => false,
            })
        };
        assert!(topic_group_exists(ACCEPTANCE));
        assert!(topic_group_exists(COMPLETION));
        assert!(!topic_group_exists(ZERO_INFO));
        assert_eq!(topic_info_count(children, ACCEPTANCE), 1);
        assert_eq!(topic_info_count(children, COMPLETION), 1);
        drop(store);
        plugin_handle_close_native(reopened);
    }

    fn raw_u32(record: &esp_authoring_core::plugin_runtime::ParsedRecord, signature: &str) -> u32 {
        let data = &record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == signature)
            .unwrap()
            .data;
        u32::from_le_bytes(data[..4].try_into().unwrap())
    }

    fn raw_payloads(
        record: &esp_authoring_core::plugin_runtime::ParsedRecord,
        signature: &str,
    ) -> Vec<Vec<u8>> {
        record
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == signature)
            .map(|subrecord| subrecord.data.to_vec())
            .collect()
    }

    fn raw_signatures(record: &esp_authoring_core::plugin_runtime::ParsedRecord) -> Vec<&str> {
        record
            .subrecords
            .iter()
            .map(|subrecord| subrecord.signature.as_str())
            .collect()
    }

    fn raw_i32(record: &esp_authoring_core::plugin_runtime::ParsedRecord, signature: &str) -> i32 {
        raw_u32(record, signature) as i32
    }

    fn raw_form_id(
        record: &esp_authoring_core::plugin_runtime::ParsedRecord,
        signature: &str,
    ) -> u32 {
        raw_u32(record, signature)
    }

    fn raw_form_ids(
        record: &esp_authoring_core::plugin_runtime::ParsedRecord,
        signatures: &[[u8; 4]],
    ) -> Vec<u32> {
        record
            .subrecords
            .iter()
            .filter(|subrecord| {
                signatures
                    .iter()
                    .any(|signature| subrecord.signature.as_str().as_bytes() == signature)
            })
            .map(|subrecord| u32::from_le_bytes(subrecord.data[..4].try_into().unwrap()))
            .map(|form_id| form_id & 0x00FF_FFFF)
            .collect()
    }

    fn topic_info_count(
        quest_children: &esp_authoring_core::plugin_runtime::ParsedGroup,
        topic: u32,
    ) -> usize {
        quest_children
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group)
                    if group.group_type == 7 && group.label == topic.to_le_bytes() =>
                {
                    Some(
                        group
                            .children
                            .iter()
                            .filter(|item| {
                                matches!(item, ParsedItem::Record(record) if record.signature.as_str() == "INFO")
                            })
                            .count(),
                    )
                }
                _ => None,
            })
            .unwrap_or(0)
    }
}
