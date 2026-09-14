use std::collections::HashMap;

use smallvec::SmallVec;
use thiserror::Error;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::{StringInterner, Sym};
use crate::translator::pair_hooks::fnv_conditions::{
    ConditionNormalizeError, LegacyConditionFamily, normalize_legacy_condition_scope,
};

use super::InfoFragmentPhase;
use crate::fnv_legacy_scripting::voice::FnvVoiceManifestEntry;

const FALLOUT4_MASTER: &str = "Fallout4.esm";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DialogueLowerError {
    #[error("DIAL {topic:06X} has no quest owner")]
    MissingTopicOwner { topic: u32 },
    #[error("DIAL {topic:06X} has {owners} owners but {clone_targets} clone targets")]
    AmbiguousTopicOwner {
        topic: u32,
        owners: usize,
        clone_targets: usize,
    },
    #[error("DIAL {topic:06X} quest owner {owner:06X} has no target mapping")]
    UnmappedTopicOwner { topic: u32, owner: u32 },
    #[error("INFO {info:06X} has no speaker")]
    MissingSpeaker { info: u32 },
    #[error("INFO {info:06X} speaker {speaker:06X} does not match voice resolution {resolved:06X}")]
    SpeakerVoiceMismatch {
        info: u32,
        speaker: u32,
        resolved: u32,
    },
    #[error("INFO {info:06X} speaker {speaker:06X} has no target mapping")]
    UnmappedSpeaker { info: u32, speaker: u32 },
    #[error("INFO {info:06X} has an incomplete response row at index {response}")]
    IncompleteResponse { info: u32, response: usize },
    #[error("INFO {info:06X} contains orphan response field {signature}")]
    OrphanResponseField { info: u32, signature: String },
    #[error("INFO {info:06X} response {response} has unsupported TRDT data")]
    UnsupportedResponseData { info: u32, response: usize },
    #[error("INFO {info:06X} response {response} references unsupported raw sound {sound:08X}")]
    UnsupportedRawSound {
        info: u32,
        response: usize,
        sound: u32,
    },
    #[error("INFO response sound {sound:06X} has no target mapping")]
    UnmappedResponseSound { sound: u32 },
    #[error("INFO {info:06X} condition {condition} failed: {reason:?}")]
    Condition {
        info: u32,
        condition: usize,
        reason: ConditionNormalizeError,
    },
    #[error("hostage GREETING projection failed: {0}")]
    ExactHostageGreeting(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceResolution {
    pub source_speaker: FormKey,
    pub target_voice_type: FormKey,
    pub source_plugin: String,
    pub source_voice_type: String,
    pub target_plugin: String,
    pub target_voice_type_edid: String,
}

pub type VoiceManifestRequest = FnvVoiceManifestEntry;

#[derive(Debug, Clone, PartialEq)]
pub struct DialogueResponseIr {
    pub emotion_type: u32,
    pub emotion_value: i32,
    pub response_number: u8,
    pub sound: Option<FormKey>,
    pub use_emotion_animation: bool,
    pub text: FieldValue,
    pub script_notes: FieldValue,
    pub edits: FieldValue,
}

#[derive(Debug, Clone)]
pub struct LoweredInfo {
    pub record: Record,
    pub source_speaker: FormKey,
    pub responses: Vec<DialogueResponseIr>,
    pub condition_count: usize,
    pub embedded_script_sources: Vec<Vec<u8>>,
    pub fragment_phases: Vec<InfoFragmentPhase>,
    pub target_voice_type: FormKey,
    pub voice_requests: Vec<VoiceManifestRequest>,
}

#[derive(Debug, Clone)]
pub struct LoweredTopic {
    pub source_topic: FormKey,
    pub source_owner: FormKey,
    pub target_owner: FormKey,
    pub record: Record,
}

#[derive(Debug, Clone)]
pub struct HostageGreetingProjection {
    pub keyword: Record,
    pub topic: LoweredTopic,
    pub infos: Vec<LoweredInfo>,
    pub topic_children: Vec<FormKey>,
}

pub fn lower_dial_topic(
    source: &Record,
    target_form_key: FormKey,
    target_owner_by_source: &HashMap<FormKey, FormKey>,
    clone_targets: &[FormKey],
    info_count: u32,
    interner: &StringInterner,
) -> Result<Vec<LoweredTopic>, DialogueLowerError> {
    let scene_topic = [0x13015B, 0x134B9A]
        .into_iter()
        .any(|local| exact_form_key(source.form_key, local, interner));
    let mut owners = source
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"QSTI")
        .filter_map(|field| first_form_key(&field.value))
        .collect::<Vec<_>>();
    owners.sort_by(|left, right| {
        form_key_sort_key(*left, interner).cmp(&form_key_sort_key(*right, interner))
    });
    owners.dedup();
    if owners.is_empty() {
        return Err(DialogueLowerError::MissingTopicOwner {
            topic: source.form_key.local,
        });
    }
    if clone_targets.len() != owners.len().saturating_sub(1) {
        return Err(DialogueLowerError::AmbiguousTopicOwner {
            topic: source.form_key.local,
            owners: owners.len(),
            clone_targets: clone_targets.len(),
        });
    }

    owners
        .into_iter()
        .enumerate()
        .map(|(index, owner)| {
            let target_owner = target_owner_by_source.get(&owner).copied().ok_or(
                DialogueLowerError::UnmappedTopicOwner {
                    topic: source.form_key.local,
                    owner: owner.local,
                },
            )?;
            let form_key = if index == 0 {
                target_form_key
            } else {
                clone_targets[index - 1]
            };
            let mut record = Record::new(SigCode(*b"DIAL"), form_key);
            record.flags = source.flags;
            record.eid = source.eid.map(|eid| {
                if index == 0 {
                    eid
                } else {
                    let original = interner.resolve(eid).unwrap_or("FNVTopic");
                    interner.intern(&format!("{original}_Q{:06X}", owner.local))
                }
            });
            if let Some(editor_id) = record.eid {
                record.fields.push(FieldEntry {
                    sig: SubrecordSig(*b"EDID"),
                    value: FieldValue::String(editor_id),
                });
            }
            for field in &source.fields {
                match field.sig.0 {
                    sig if sig == *b"FULL" || sig == *b"PNAM" => record.fields.push(field.clone()),
                    sig if sig == *b"DATA" => record.fields.push(FieldEntry {
                        sig: SubrecordSig(*b"DATA"),
                        value: if scene_topic {
                            lower_scene_topic_data(&field.value, interner)
                        } else {
                            lower_topic_data(&field.value, interner)
                        },
                    }),
                    _ => {}
                }
            }
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"QNAM"),
                value: FieldValue::FormKey(target_owner),
            });
            if scene_topic {
                record.fields.push(FieldEntry {
                    sig: SubrecordSig(*b"SNAM"),
                    value: FieldValue::Bytes(SmallVec::from_slice(b"SCEN")),
                });
            }
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"TIFC"),
                value: FieldValue::Uint(info_count as u64),
            });
            Ok(LoweredTopic {
                source_topic: source.form_key,
                source_owner: owner,
                target_owner,
                record,
            })
        })
        .collect()
}

pub fn lower_hostage_greeting_topic(
    source: &Record,
    target_form_key: FormKey,
    target_owner: FormKey,
    target_keyword: FormKey,
    interner: &StringInterner,
) -> Result<LoweredTopic, DialogueLowerError> {
    if !exact_form_key(source.form_key, 0x0000C8, interner) {
        return Err(DialogueLowerError::ExactHostageGreeting(format!(
            "expected DIAL 0000C8:FalloutNV.esm, got {:06X}",
            source.form_key.local
        )));
    }
    if target_form_key.local < 0x800 || target_form_key == source.form_key {
        return Err(DialogueLowerError::ExactHostageGreeting(
            "target DIAL must use a generated non-source identity".into(),
        ));
    }
    let owner = source
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"QSTI")
        .filter_map(|field| first_form_key(&field.value))
        .find(|owner| exact_form_key(*owner, 0x11F935, interner))
        .ok_or_else(|| {
            DialogueLowerError::ExactHostageGreeting(
                "global GREETING lacks audited QSTI 11F935 owner".into(),
            )
        })?;
    let mut record = Record::new(SigCode(*b"DIAL"), target_form_key);
    record.flags = source.flags;
    let editor_id = interner.intern("FNV_FO3_TecMineHostageGreeting");
    record.eid = Some(editor_id);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"EDID"),
        value: FieldValue::String(editor_id),
    });
    for field in &source.fields {
        match field.sig.0 {
            sig if sig == *b"FULL" || sig == *b"PNAM" => record.fields.push(field.clone()),
            sig if sig == *b"DATA" => record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: lower_topic_data(&field.value, interner),
            }),
            _ => {}
        }
    }
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"QNAM"),
        value: FieldValue::FormKey(target_owner),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"KNAM"),
        value: FieldValue::FormKey(target_keyword),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"SNAM"),
        value: FieldValue::Bytes(SmallVec::from_slice(b"CUST")),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"TIFC"),
        value: FieldValue::Uint(3),
    });
    Ok(LoweredTopic {
        source_topic: source.form_key,
        source_owner: owner,
        target_owner,
        record,
    })
}

pub fn lower_hostage_greeting_info_record(
    source: &Record,
    target_form_key: FormKey,
    source_voice_type_form_key: FormKey,
    voice: VoiceResolution,
    mapper: &mut FormKeyMapper<'_>,
    interner: &StringInterner,
) -> Result<LoweredInfo, DialogueLowerError> {
    if !matches!(source.form_key.local, 0x15734B..=0x15734D)
        || !exact_form_key(source.form_key, source.form_key.local, interner)
    {
        return Err(DialogueLowerError::ExactHostageGreeting(format!(
            "INFO {:06X} is outside audited 15734B/C/D set",
            source.form_key.local
        )));
    }
    if source
        .fields
        .iter()
        .any(|field| matches!(field.sig.0, sig if sig == *b"PNAM" || sig == *b"ANAM"))
    {
        return Err(DialogueLowerError::ExactHostageGreeting(format!(
            "INFO {:06X} unexpectedly has PNAM/ANAM",
            source.form_key.local
        )));
    }
    validate_exact_hostage_info_payload(source, interner)?;
    if !source.fields.iter().any(|field| {
        field.sig.0 == *b"QSTI"
            && first_form_key(&field.value)
                .is_some_and(|owner| exact_form_key(owner, 0x11F935, interner))
    }) {
        return Err(DialogueLowerError::ExactHostageGreeting(format!(
            "INFO {:06X} lacks QSTI 11F935",
            source.form_key.local
        )));
    }
    let conditions = source
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"CTDA")
        .collect::<Vec<_>>();
    if conditions.len() != 1 || !exact_hostage_speaker_condition(&conditions[0].value) {
        return Err(DialogueLowerError::ExactHostageGreeting(format!(
            "INFO {:06X} must have exactly one GetIsID 123193 condition",
            source.form_key.local
        )));
    }
    let source_speaker = FormKey {
        local: 0x123193,
        plugin: source.form_key.plugin,
    };
    if voice.source_speaker != source_speaker {
        return Err(DialogueLowerError::SpeakerVoiceMismatch {
            info: source.form_key.local,
            speaker: source_speaker.local,
            resolved: voice.source_speaker.local,
        });
    }
    if !exact_form_key(source_voice_type_form_key, 0x02AB62, interner) {
        return Err(DialogueLowerError::ExactHostageGreeting(
            "hostage GREETING requires source VTYP 02AB62".into(),
        ));
    }
    let mut adapted = source.clone();
    adapted.fields.push(FieldEntry {
        sig: SubrecordSig(*b"ANAM"),
        value: FieldValue::FormKey(source_speaker),
    });
    lower_info_record(&adapted, target_form_key, voice, mapper, interner)
}

pub fn lower_hostage_greeting_projection(
    source_topic: &Record,
    source_infos: &[Record],
    target_keyword: FormKey,
    target_topic: FormKey,
    target_owner: FormKey,
    target_infos: &[FormKey],
    source_voice_type_form_key: FormKey,
    voices: &[VoiceResolution],
    mapper: &mut FormKeyMapper<'_>,
    interner: &StringInterner,
) -> Result<HostageGreetingProjection, DialogueLowerError> {
    if source_infos.len() != 3 || target_infos.len() != 3 || voices.len() != 3 {
        return Err(DialogueLowerError::ExactHostageGreeting(format!(
            "projection requires exactly three source INFOs, targets, and voice resolutions; got {}/{}/{}",
            source_infos.len(),
            target_infos.len(),
            voices.len()
        )));
    }
    for (index, source) in source_infos.iter().enumerate() {
        if source.form_key.local != 0x15734B + index as u32
            || !exact_form_key(source.form_key, 0x15734B + index as u32, interner)
        {
            return Err(DialogueLowerError::ExactHostageGreeting(format!(
                "source INFO order changed at index {index}: expected {:06X}, got {:06X}",
                0x15734B + index as u32,
                source.form_key.local
            )));
        }
    }
    let keyword = lower_hostage_greeting_keyword(target_keyword, interner)?;
    let topic = lower_hostage_greeting_topic(
        source_topic,
        target_topic,
        target_owner,
        target_keyword,
        interner,
    )?;
    let infos = source_infos
        .iter()
        .zip(target_infos)
        .zip(voices)
        .map(|((source, target), voice)| {
            lower_hostage_greeting_info_record(
                source,
                *target,
                source_voice_type_form_key,
                voice.clone(),
                mapper,
                interner,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HostageGreetingProjection {
        keyword,
        topic,
        infos,
        topic_children: target_infos.to_vec(),
    })
}

fn lower_hostage_greeting_keyword(
    target_form_key: FormKey,
    interner: &StringInterner,
) -> Result<Record, DialogueLowerError> {
    if target_form_key.local < 0x800 {
        return Err(DialogueLowerError::ExactHostageGreeting(
            "generated hostage greeting KYWD identity is inside the reserved source range".into(),
        ));
    }
    let mut record = Record::new(SigCode(*b"KYWD"), target_form_key);
    let editor_id = interner.intern("FNV_FO3_TecMineHostageFreedGreeting");
    record.eid = Some(editor_id);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"EDID"),
        value: FieldValue::String(editor_id),
    });
    Ok(record)
}

fn exact_hostage_speaker_condition(value: &FieldValue) -> bool {
    let FieldValue::Bytes(bytes) = value else {
        return false;
    };
    bytes.len() == 28
        && u16::from_le_bytes([bytes[8], bytes[9]]) == 72
        && u32::from_le_bytes(bytes[12..16].try_into().unwrap()) & 0x00FF_FFFF == 0x123193
}

fn validate_exact_hostage_info_payload(
    source: &Record,
    interner: &StringInterner,
) -> Result<(), DialogueLowerError> {
    let index = (source.form_key.local - 0x15734B) as usize;
    let expected_text = [
        "I'm getting out of here.",
        "Fuck this place.",
        "The Legion will pay for this.",
    ][index];
    let expected_emotion = [3, 1, 1][index];
    let expected_flags = [0x03, 0x03, 0x23][index];
    let responses = parse_response_rows(source, interner)?;
    let exact_response = matches!(responses.as_slice(), [response]
        if response.emotion_type == expected_emotion
            && response.emotion_value == 20
            && response.response_number == 1
            && response.sound.is_none()
            && response.use_emotion_animation
            && response_text(&response.text, interner).as_deref() == Some(expected_text));
    if !exact_response {
        return Err(DialogueLowerError::ExactHostageGreeting(format!(
            "INFO {:06X} response text/emotion payload changed",
            source.form_key.local
        )));
    }
    let flags = source
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DATA")
        .and_then(|field| info_flags_1(&field.value, interner));
    if flags != Some(expected_flags) {
        return Err(DialogueLowerError::ExactHostageGreeting(format!(
            "INFO {:06X} expected exact Goodbye/Random flags 0x{expected_flags:02X}, got {flags:?}",
            source.form_key.local
        )));
    }
    Ok(())
}

fn info_flags_1(value: &FieldValue, interner: &StringInterner) -> Option<u8> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 3 => Some(bytes[2]),
        FieldValue::Struct(fields) => match named(fields, "flags_1", interner)? {
            FieldValue::Uint(value) => u8::try_from(*value).ok(),
            FieldValue::Int(value) => u8::try_from(*value).ok(),
            FieldValue::List(values) => values.iter().try_fold(0_u8, |flags, value| {
                let FieldValue::String(name) = value else {
                    return None;
                };
                let bit = match interner.resolve(*name)? {
                    "Goodbye" => 0x01,
                    "Random" => 0x02,
                    "RandomEnd" => 0x20,
                    _ => return None,
                };
                Some(flags | bit)
            }),
            _ => None,
        },
        _ => None,
    }
}

fn exact_form_key(form_key: FormKey, local: u32, interner: &StringInterner) -> bool {
    form_key.local == local
        && interner
            .resolve(form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case("FalloutNV.esm"))
}

pub fn lower_info_record(
    source: &Record,
    target_form_key: FormKey,
    voice: VoiceResolution,
    mapper: &mut FormKeyMapper<'_>,
    interner: &StringInterner,
) -> Result<LoweredInfo, DialogueLowerError> {
    let speaker = source
        .fields
        .iter()
        .rev()
        .find(|field| matches!(field.sig.0, sig if sig == *b"PNAM" || sig == *b"ANAM"))
        .and_then(|field| first_form_key(&field.value))
        .ok_or(DialogueLowerError::MissingSpeaker {
            info: source.form_key.local,
        })?;
    if speaker != voice.source_speaker {
        return Err(DialogueLowerError::SpeakerVoiceMismatch {
            info: source.form_key.local,
            speaker: speaker.local,
            resolved: voice.source_speaker.local,
        });
    }
    let target_speaker = mapper
        .lookup(speaker)
        .ok_or(DialogueLowerError::UnmappedSpeaker {
            info: source.form_key.local,
            speaker: speaker.local,
        })?;
    let responses = parse_response_rows(source, interner)?;
    let mut record = Record::new(SigCode(*b"INFO"), target_form_key);
    record.eid = source.eid;
    record.flags = source.flags;

    if let Some(flags) = source.fields.iter().find(|field| field.sig.0 == *b"DATA") {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ENAM"),
            value: lower_info_flags(&flags.value, interner),
        });
    }
    for response in &responses {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"TRDA"),
            value: lower_response_data(response, mapper, interner)?,
        });
        for (sig, value) in [
            (*b"NAM1", response.text.clone()),
            (*b"NAM2", response.script_notes.clone()),
            (*b"NAM3", response.edits.clone()),
            (*b"NAM4", empty_string(interner)),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(sig),
                value,
            });
        }
    }

    let mut condition_count = 0;
    let mut index = 0;
    while index < source.fields.len() {
        if source.fields[index].sig.0 != *b"CTDA" {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < source.fields.len()
            && matches!(source.fields[index].sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2")
        {
            index += 1;
        }
        let (lowered, _) = normalize_legacy_condition_scope(
            &source.fields[start..index],
            LegacyConditionFamily::Fnv,
            mapper,
        )
        .map_err(|reason| DialogueLowerError::Condition {
            info: source.form_key.local,
            condition: condition_count,
            reason,
        })?;
        record.fields.extend(lowered);
        condition_count += 1;
    }
    for sig in [*b"TPIC", *b"RNAM", *b"KNAM", *b"DNAM"] {
        record.fields.extend(
            source
                .fields
                .iter()
                .filter(|field| field.sig.0 == sig)
                .cloned(),
        );
    }
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"ANAM"),
        value: FieldValue::FormKey(target_speaker),
    });

    let embedded_scripts = extract_embedded_scripts(source, interner);
    let embedded_script_sources = embedded_scripts
        .iter()
        .map(|(_, source)| source.clone())
        .collect();
    let fragment_phases = embedded_scripts.iter().map(|(phase, _)| *phase).collect();
    let info_form_id = format!("{:06X}", target_form_key.local);
    let voice_requests = responses
        .iter()
        .enumerate()
        .map(|(response_index, response)| {
            FnvVoiceManifestEntry::new(
                voice.source_plugin.clone(),
                voice.source_voice_type.clone(),
                voice.target_plugin.clone(),
                voice.target_voice_type_edid.clone(),
                &info_form_id,
                response_index as u32,
                response_text(&response.text, interner),
            )
        })
        .collect();
    Ok(LoweredInfo {
        record,
        source_speaker: speaker,
        responses,
        condition_count,
        embedded_script_sources,
        fragment_phases,
        target_voice_type: voice.target_voice_type,
        voice_requests,
    })
}

fn extract_embedded_scripts(
    source: &Record,
    interner: &StringInterner,
) -> Vec<(InfoFragmentPhase, Vec<u8>)> {
    let mut scripts = Vec::new();
    let mut flat_phase = InfoFragmentPhase::Begin;
    for field in &source.fields {
        if field.sig.0 == *b"NEXT" {
            collect_nested_sctx(&field.value, InfoFragmentPhase::End, interner, &mut scripts);
            flat_phase = InfoFragmentPhase::End;
        } else if field.sig.0 == *b"SCHR" {
            collect_nested_sctx(&field.value, flat_phase, interner, &mut scripts);
        } else if field.sig.0 == *b"SCTX" {
            if let Some(bytes) = bytes_from_value(&field.value) {
                scripts.push((flat_phase, bytes));
            }
        }
    }
    scripts
}

fn collect_nested_sctx(
    value: &FieldValue,
    phase: InfoFragmentPhase,
    interner: &StringInterner,
    output: &mut Vec<(InfoFragmentPhase, Vec<u8>)>,
) {
    match value {
        FieldValue::List(values) => {
            for value in values {
                collect_nested_sctx(value, phase, interner, output);
            }
        }
        FieldValue::Struct(fields) => {
            for (name, value) in fields {
                if interner
                    .resolve(*name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("SCTX"))
                {
                    if let Some(bytes) = bytes_from_value(value) {
                        output.push((phase, bytes));
                    }
                } else {
                    collect_nested_sctx(value, phase, interner, output);
                }
            }
        }
        _ => {}
    }
}

fn parse_response_rows(
    source: &Record,
    interner: &StringInterner,
) -> Result<Vec<DialogueResponseIr>, DialogueLowerError> {
    let mut rows = Vec::new();
    let mut index = 0;
    while index < source.fields.len() {
        if source.fields[index].sig.0 != *b"TRDT" {
            if matches!(source.fields[index].sig.0, sig if sig == *b"NAM1" || sig == *b"NAM2" || sig == *b"NAM3")
            {
                return Err(DialogueLowerError::OrphanResponseField {
                    info: source.form_key.local,
                    signature: source.fields[index].sig.as_str().to_string(),
                });
            }
            index += 1;
            continue;
        }
        let response_index = rows.len();
        let data = parse_response_data(
            &source.fields[index].value,
            source.form_key.local,
            response_index,
            interner,
        )?;
        index += 1;
        let mut values: [Option<FieldValue>; 3] = [None, None, None];
        while index < source.fields.len() {
            let slot = match source.fields[index].sig.0 {
                sig if sig == *b"NAM1" => Some(0),
                sig if sig == *b"NAM2" => Some(1),
                sig if sig == *b"NAM3" => Some(2),
                _ => None,
            };
            let Some(slot) = slot else { break };
            values[slot] = Some(source.fields[index].value.clone());
            index += 1;
        }
        if values.iter().any(Option::is_none) {
            return Err(DialogueLowerError::IncompleteResponse {
                info: source.form_key.local,
                response: response_index,
            });
        }
        rows.push(DialogueResponseIr {
            emotion_type: data.0,
            emotion_value: data.1,
            response_number: data.2,
            sound: data.3,
            use_emotion_animation: data.4,
            text: values[0].take().unwrap(),
            script_notes: values[1].take().unwrap(),
            edits: values[2].take().unwrap(),
        });
    }
    Ok(rows)
}

fn parse_response_data(
    value: &FieldValue,
    info: u32,
    response: usize,
    interner: &StringInterner,
) -> Result<(u32, i32, u8, Option<FormKey>, bool), DialogueLowerError> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 24 => {
            let raw_sound = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
            if raw_sound != 0 {
                return Err(DialogueLowerError::UnsupportedRawSound {
                    info,
                    response,
                    sound: raw_sound,
                });
            }
            Ok((
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                i32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                bytes[12],
                None,
                bytes[20] != 0,
            ))
        }
        FieldValue::Struct(fields) => {
            let emotion_type = emotion_type_named(fields, interner).unwrap_or(0);
            let emotion_value = int_named(fields, "emotion_value", interner).unwrap_or(0) as i32;
            let response_number =
                uint_named(fields, "response_number", interner).unwrap_or(1) as u8;
            let sound = form_key_named(fields, "sound", interner);
            let use_emotion_animation =
                bool_named(fields, "use_emotion_animation", interner).unwrap_or(true);
            Ok((
                emotion_type,
                emotion_value,
                response_number,
                sound,
                use_emotion_animation,
            ))
        }
        _ => Err(DialogueLowerError::UnsupportedResponseData { info, response }),
    }
}

fn lower_response_data(
    response: &DialogueResponseIr,
    mapper: &FormKeyMapper<'_>,
    interner: &StringInterner,
) -> Result<FieldValue, DialogueLowerError> {
    let emotion = FormKey {
        local: emotion_keyword(response.emotion_type),
        plugin: interner.intern(FALLOUT4_MASTER),
    };
    let sound = response
        .sound
        .map(|source| {
            mapper
                .lookup(source)
                .ok_or(DialogueLowerError::UnmappedResponseSound {
                    sound: source.local,
                })
        })
        .transpose()?;
    Ok(FieldValue::Struct(vec![
        (interner.intern("emotion"), FieldValue::FormKey(emotion)),
        (
            interner.intern("response_number"),
            FieldValue::Uint(response.response_number as u64),
        ),
        (
            interner.intern("sound_file"),
            sound
                .map(FieldValue::FormKey)
                .unwrap_or(FieldValue::Uint(0)),
        ),
        (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
        (interner.intern("interrupt_percentage"), FieldValue::Uint(0)),
        (interner.intern("camera_target_alias"), FieldValue::Int(-1)),
        (
            interner.intern("camera_location_alias"),
            FieldValue::Int(-1),
        ),
    ]))
}

fn emotion_keyword(source: u32) -> u32 {
    match source {
        1 => 0x0FA84A, // Angry
        2 => 0x0C8674, // Disgust
        3 => 0x0FA84B, // Afraid
        4 => 0x0FA848, // Sad
        5 => 0x0FA847, // Happy
        6 => 0x0C8672, // Surprised
        7 => 0x0C8673, // Puzzled
        8 => 0x100286, // In pain
        _ => 0x0D755D, // Neutral
    }
}

fn lower_topic_data(value: &FieldValue, interner: &StringInterner) -> FieldValue {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            FieldValue::Bytes(SmallVec::from_slice(&[bytes[1], 0, bytes[0], 0]))
        }
        FieldValue::Struct(fields) => {
            let topic_type = uint_named(fields, "type", interner).unwrap_or(0) as u16;
            let flags = uint_named(fields, "flags", interner).unwrap_or(0) as u8;
            FieldValue::Bytes(SmallVec::from_slice(&[
                flags,
                0,
                topic_type as u8,
                (topic_type >> 8) as u8,
            ]))
        }
        _ => FieldValue::Bytes(SmallVec::from_slice(&[0, 0, 0, 0])),
    }
}

fn lower_scene_topic_data(value: &FieldValue, interner: &StringInterner) -> FieldValue {
    let FieldValue::Bytes(mut bytes) = lower_topic_data(value, interner) else {
        unreachable!();
    };
    bytes.resize(4, 0);
    bytes[1] = 2;
    bytes[2..4].copy_from_slice(&14_u16.to_le_bytes());
    FieldValue::Bytes(bytes)
}

fn lower_info_flags(value: &FieldValue, interner: &StringInterner) -> FieldValue {
    let flags = match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 3 => {
            bytes[2] as u16 | ((bytes.get(3).copied().unwrap_or(0) as u16) << 8)
        }
        FieldValue::Struct(_) => info_flags_1(value, interner).unwrap_or(0) as u16,
        _ => 0,
    };
    FieldValue::Bytes(SmallVec::from_slice(&[
        flags as u8,
        (flags >> 8) as u8,
        0,
        0,
    ]))
}

fn empty_string(interner: &StringInterner) -> FieldValue {
    FieldValue::String(interner.intern(""))
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::List(values) => values.iter().find_map(first_form_key),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        _ => None,
    }
}

fn bytes_from_value(value: &FieldValue) -> Option<Vec<u8>> {
    match value {
        FieldValue::Bytes(bytes) => Some(bytes.to_vec()),
        FieldValue::List(values) => values.iter().find_map(bytes_from_value),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| bytes_from_value(value)),
        _ => None,
    }
}

fn response_text(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(text) => interner.resolve(*text).map(str::to_string),
        FieldValue::Bytes(bytes) => std::str::from_utf8(bytes)
            .ok()
            .map(|text| text.trim_end_matches('\0').to_string()),
        _ => None,
    }
}

fn form_key_sort_key(form_key: FormKey, interner: &StringInterner) -> (String, u32) {
    (
        interner
            .resolve(form_key.plugin)
            .unwrap_or("")
            .to_ascii_lowercase(),
        form_key.local,
    )
}

fn named<'a>(
    fields: &'a [(Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields.iter().find_map(|(key, value)| {
        interner
            .resolve(*key)
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            .then_some(value)
    })
}

fn uint_named(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<u64> {
    match named(fields, name, interner) {
        Some(FieldValue::Uint(value)) => Some(*value),
        Some(FieldValue::Int(value)) if *value >= 0 => Some(*value as u64),
        Some(FieldValue::Bool(value)) => Some(u64::from(*value)),
        _ => None,
    }
}

fn int_named(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<i64> {
    match named(fields, name, interner) {
        Some(FieldValue::Int(value)) => Some(*value),
        Some(FieldValue::Uint(value)) => Some(*value as i64),
        _ => None,
    }
}

fn form_key_named(
    fields: &[(Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    named(fields, name, interner).and_then(first_form_key)
}

fn bool_named(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<bool> {
    match named(fields, name, interner) {
        Some(FieldValue::Bool(value)) => Some(*value),
        Some(FieldValue::Uint(value)) => Some(*value != 0),
        Some(FieldValue::Int(value)) => Some(*value != 0),
        _ => None,
    }
}

fn emotion_type_named(fields: &[(Sym, FieldValue)], interner: &StringInterner) -> Option<u32> {
    match named(fields, "emotion_type", interner) {
        Some(FieldValue::Uint(value)) => Some(*value as u32),
        Some(FieldValue::Int(value)) if *value >= 0 => Some(*value as u32),
        Some(FieldValue::String(value)) => interner.resolve(*value).and_then(|name| match name {
            "Anger" | "Angry" => Some(1),
            "Disgust" => Some(2),
            "Fear" | "Afraid" => Some(3),
            "Sad" => Some(4),
            "Happy" => Some(5),
            "Surprise" | "Surprised" => Some(6),
            "Puzzled" => Some(7),
            "Pain" | "InPain" => Some(8),
            "Neutral" => Some(0),
            _ => None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::schema::AuthoringSchema;
    use crate::target_write::{add_quest_child_record_native, add_record_native};
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, plugin_handle_close_native, plugin_handle_load_no_py,
        plugin_handle_new_no_py, plugin_handle_save_no_py, plugin_handle_store_ref,
    };

    fn find_parsed_record<'a>(
        items: &'a [ParsedItem],
        signature: &str,
        local: u32,
    ) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record)
                    if record.signature.as_str() == signature
                        && (record.form_id & 0x00FF_FFFF) == local =>
                {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(record) = find_parsed_record(&group.children, signature, local) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn parsed_editor_id(record: &ParsedRecord) -> Option<&str> {
        let bytes = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "EDID")?
            .data
            .as_ref();
        std::str::from_utf8(bytes.strip_suffix(&[0]).unwrap_or(bytes)).ok()
    }

    fn parsed_subrecords<'a>(record: &'a ParsedRecord, signature: &str) -> Vec<&'a [u8]> {
        record
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == signature)
            .map(|subrecord| subrecord.data.as_ref())
            .collect()
    }

    fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }
    }

    fn source_fk(local: u32, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("FalloutNV.esm"),
        }
    }

    fn target_fk(local: u32, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("Converted.esm"),
        }
    }

    fn mapper<'a>(interner: &'a StringInterner) -> FormKeyMapper<'a> {
        FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: "FalloutNV.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            interner,
        )
    }

    fn response_data(number: u8) -> FieldValue {
        let mut bytes = vec![0; 24];
        bytes[0..4].copy_from_slice(&5_u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&20_i32.to_le_bytes());
        bytes[12] = number;
        bytes[20] = 1;
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn response_fields(interner: &StringInterner, number: u8) -> Vec<FieldEntry> {
        vec![
            field(b"TRDT", response_data(number)),
            field(
                b"NAM1",
                FieldValue::String(interner.intern(if number == 1 {
                    "The hostages are safe."
                } else {
                    "The NCR could use someone like you."
                })),
            ),
            field(b"NAM2", FieldValue::String(interner.intern(""))),
            field(b"NAM3", FieldValue::String(interner.intern(""))),
        ]
    }

    fn condition(function: u16, parameter_1: u32) -> FieldEntry {
        let mut bytes = vec![0; 28];
        bytes[4..8].copy_from_slice(&1_f32.to_le_bytes());
        bytes[8..10].copy_from_slice(&function.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        field(b"CTDA", FieldValue::Bytes(SmallVec::from_vec(bytes)))
    }

    fn topic(topic_local: u32, owners: &[FormKey], interner: &StringInterner) -> Record {
        let mut record = Record::new(SigCode(*b"DIAL"), source_fk(topic_local, interner));
        record.eid = Some(interner.intern(&format!("VDialogue{topic_local:06X}")));
        record.fields.extend(
            owners
                .iter()
                .copied()
                .map(|owner| field(b"QSTI", FieldValue::FormKey(owner))),
        );
        record.fields.push(field(
            b"FULL",
            FieldValue::String(interner.intern("I'll take a look.")),
        ));
        record.fields.push(field(
            b"DATA",
            FieldValue::Bytes(SmallVec::from_slice(&[0, 0])),
        ));
        record
    }

    fn info(
        info_local: u32,
        speaker: FormKey,
        response_count: usize,
        condition_count: usize,
        interner: &StringInterner,
    ) -> Record {
        let mut record = Record::new(SigCode(*b"INFO"), source_fk(info_local, interner));
        record.fields.push(field(
            b"DATA",
            FieldValue::Bytes(SmallVec::from_slice(&[0, 0, 1, 0])),
        ));
        for number in 1..=response_count {
            record
                .fields
                .extend(response_fields(interner, number as u8));
        }
        for index in 0..condition_count {
            record.fields.push(if index == 0 {
                condition(72, speaker.local)
            } else {
                condition(0, 0)
            });
        }
        record
            .fields
            .push(field(b"PNAM", FieldValue::FormKey(speaker)));
        record
    }

    fn voice(speaker: FormKey, interner: &StringInterner) -> VoiceResolution {
        VoiceResolution {
            source_speaker: speaker,
            target_voice_type: source_fk(0x02AB62, interner),
            source_plugin: "FalloutNV.esm".into(),
            source_voice_type: "MaleAdult".into(),
            target_plugin: "B21_nv_FalloutNV.esm".into(),
            target_voice_type_edid: "MaleAdult01DefaultB".into(),
        }
    }

    #[test]
    fn golden_13015b_to_130161_has_one_condition_and_response() {
        let interner = StringInterner::new();
        let quest = source_fk(0x11F935, &interner);
        let speaker = source_fk(0x1300F0, &interner);
        let target_quest = target_fk(0x11F935, &interner);
        let target_speaker = target_fk(0x1300F0, &interner);
        let topic = topic(0x13015B, &[quest], &interner);
        let lowered_topic = lower_dial_topic(
            &topic,
            target_fk(0x13015B, &interner),
            &HashMap::from([(quest, target_quest)]),
            &[],
            1,
            &interner,
        )
        .unwrap();
        assert_eq!(lowered_topic.len(), 1);
        assert_eq!(lowered_topic[0].record.fields[0].sig.0, *b"EDID");
        assert_eq!(
            lowered_topic[0].record.fields[0].value,
            FieldValue::String(interner.intern("VDialogue13015B"))
        );
        assert_eq!(
            lowered_topic[0]
                .record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"TIFC")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(1))
        );

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
        add_record_native(
            handle,
            Record::new(SigCode(*b"QUST"), target_quest),
            &schema,
            &interner,
        )
        .unwrap();
        assert!(
            add_quest_child_record_native(
                handle,
                lowered_topic[0].record.clone(),
                &schema,
                &interner,
            )
            .unwrap()
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Converted.esm");
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(handle);
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        assert_eq!(
            find_parsed_record(&slot.parsed.root_items, "DIAL", 0x13015B)
                .and_then(parsed_editor_id),
            Some("VDialogue13015B")
        );
        drop(store);
        plugin_handle_close_native(reopened);

        let mut mapper = mapper(&interner);
        mapper.add_mapping(speaker, target_speaker);
        let lowered = lower_info_record(
            &info(0x130161, speaker, 1, 1, &interner),
            target_fk(0x130161, &interner),
            voice(speaker, &interner),
            &mut mapper,
            &interner,
        )
        .unwrap();
        assert_eq!(lowered.responses.len(), 1);
        assert_eq!(lowered.condition_count, 1);
        assert_eq!(lowered.target_voice_type.local, 0x02AB62);
        assert_eq!(lowered.voice_requests.len(), 1);
        assert_eq!(
            lowered.voice_requests[0].target_voice_type_edid,
            "MaleAdult01DefaultB"
        );
        assert!(
            !lowered
                .record
                .fields
                .iter()
                .any(|entry| { matches!(entry.sig.0, sig if sig == *b"PNAM" || sig == *b"TRDT") })
        );
        assert!(
            lowered
                .record
                .fields
                .iter()
                .any(|entry| entry.sig.0 == *b"ANAM")
        );
        assert!(
            lowered
                .record
                .fields
                .iter()
                .any(|entry| entry.sig.0 == *b"TRDA")
        );
        let condition = lowered
            .record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CTDA")
            .unwrap();
        assert!(matches!(&condition.value, FieldValue::Bytes(bytes) if bytes.len() == 32));
    }

    #[test]
    fn golden_134b9a_to_134b9b_preserves_four_conditions_two_responses() {
        let interner = StringInterner::new();
        let speaker = source_fk(0x1300F0, &interner);
        let mut mapper = mapper(&interner);
        mapper.add_mapping(speaker, target_fk(0x1300F0, &interner));
        let lowered = lower_info_record(
            &info(0x134B9B, speaker, 2, 4, &interner),
            target_fk(0x134B9B, &interner),
            voice(speaker, &interner),
            &mut mapper,
            &interner,
        )
        .unwrap();
        assert_eq!(lowered.responses.len(), 2);
        assert_eq!(lowered.condition_count, 4);
        assert_eq!(
            lowered
                .record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"TRDA")
                .count(),
            2
        );
        assert_eq!(
            lowered
                .record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"NAM4")
                .count(),
            2
        );
    }

    #[test]
    fn selected_infos_save_and_reopen_with_canonical_response_rows() {
        let interner = StringInterner::new();
        let renolds = source_fk(0x1300F0, &interner);
        let hostage = source_fk(0x123193, &interner);
        let mut mapper = mapper(&interner);
        mapper.add_mapping(renolds, target_fk(0x1300F0, &interner));
        mapper.add_mapping(hostage, target_fk(0x123193, &interner));
        let selected = [
            (0x130161, renolds, 1, 1),
            (0x134B9B, renolds, 2, 4),
            (0x15734B, hostage, 1, 1),
            (0x15734C, hostage, 1, 1),
            (0x15734D, hostage, 1, 1),
        ];
        let lowered = selected
            .iter()
            .map(|(local, speaker, responses, conditions)| {
                lower_info_record(
                    &info(*local, *speaker, *responses, *conditions, &interner),
                    target_fk(*local, &interner),
                    voice(*speaker, &interner),
                    &mut mapper,
                    &interner,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
        for info in &lowered {
            add_record_native(handle, info.record.clone(), &schema, &interner).unwrap();
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Converted.esm");
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(handle);

        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        for (local, _, response_count, _) in selected {
            let record = find_parsed_record(&slot.parsed.root_items, "INFO", local).unwrap();
            assert!(
                record.parse_error.is_none(),
                "INFO {local:06X}: {:?}",
                record.parse_error
            );
            let responses = parsed_subrecords(record, "TRDA");
            assert_eq!(responses.len(), response_count, "INFO {local:06X}");
            for response in responses {
                assert_eq!(response.len(), 20, "INFO {local:06X} TRDA");
                assert_eq!(&response[5..9], &[0; 4], "INFO {local:06X} sound file");
                assert_eq!(&response[12..20], &[0xFF; 8], "INFO {local:06X} aliases");
            }
            for signature in ["NAM2", "NAM3", "NAM4"] {
                let strings = parsed_subrecords(record, signature);
                assert_eq!(
                    strings.len(),
                    response_count,
                    "INFO {local:06X} {signature}"
                );
                assert!(
                    strings.iter().all(|bytes| *bytes == [0]),
                    "INFO {local:06X} {signature}"
                );
            }
        }
        drop(store);
        plugin_handle_close_native(reopened);
    }

    #[test]
    fn selected_info_emotions_survive_seven_master_postflight_as_fallout4_keywords() {
        use crate::fixups::remap_struct_internal_formids::RemapStructInternalFormIdsFixup;
        use crate::fixups::rewrite_raw_object_template_formids::RewriteRawObjectTemplateFormIdsFixup;
        use crate::fixups::{Fixup, FixupConfig};
        use crate::formkey_mapper::MapperState;
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_no_py, plugin_handle_new_native,
            plugin_handle_save_preserving_identity_no_py,
        };

        let interner = StringInterner::new();
        let renolds = source_fk(0x1300F0, &interner);
        let hostage = source_fk(0x123193, &interner);
        let target_masters = vec![
            "Fallout4.esm".into(),
            "DLCRobot.esm".into(),
            "DLCworkshop01.esm".into(),
            "DLCCoast.esm".into(),
            "DLCworkshop02.esm".into(),
            "DLCworkshop03.esm".into(),
            "DLCNukaWorld.esm".into(),
        ];
        let mut mapper_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "FalloutNV.esm".into(),
                source_plugin_name: "FalloutNV.esm".into(),
                target_master_names: target_masters.clone(),
                ..MapperOptions::default()
            },
        );
        mapper_state
            .source_to_target
            .insert(renolds, source_fk(0x1300F0, &interner));
        mapper_state
            .source_to_target
            .insert(hostage, source_fk(0x123193, &interner));
        for local in [0x0FA847, 0x0FA84A, 0x0FA84B] {
            mapper_state
                .source_to_target
                .insert(source_fk(local, &interner), source_fk(local, &interner));
        }

        let mut source_infos = vec![
            info(0x130161, renolds, 1, 1, &interner),
            info(0x134B9B, renolds, 2, 4, &interner),
            info(0x15734B, hostage, 1, 1, &interner),
            info(0x15734C, hostage, 1, 1, &interner),
            info(0x15734D, hostage, 1, 1, &interner),
        ];
        for (record, emotion) in source_infos[2..].iter_mut().zip([3_u32, 1, 1]) {
            let FieldValue::Bytes(bytes) = &mut record
                .fields
                .iter_mut()
                .find(|field| field.sig.0 == *b"TRDT")
                .unwrap()
                .value
            else {
                unreachable!();
            };
            bytes[0..4].copy_from_slice(&emotion.to_le_bytes());
        }
        let lowered = {
            let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
            source_infos
                .iter()
                .map(|source| {
                    lower_info_record(
                        source,
                        source_fk(source.form_key.local, &interner),
                        voice(
                            if source.form_key.local >= 0x15734B {
                                hostage
                            } else {
                                renolds
                            },
                            &interner,
                        ),
                        &mut mapper,
                        &interner,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>()
        };
        assert!(lowered.iter().all(|info| {
            info.record
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"TRDA")
                .all(|field| {
                    let FieldValue::Struct(fields) = &field.value else {
                        return false;
                    };
                    form_key_named(fields, "emotion", &interner).is_some_and(|emotion| {
                        interner.resolve(emotion.plugin) == Some("Fallout4.esm")
                    })
                })
        }));

        let schema = std::sync::Arc::new(AuthoringSchema::for_game("fo4").unwrap());
        let source = plugin_handle_new_native("FalloutNV.esm", Some("fnv")).unwrap();
        let target = plugin_handle_new_native("FalloutNV.esm", Some("fo4")).unwrap();
        let target_master_handles = target_masters
            .iter()
            .map(|master| plugin_handle_new_native(master, Some("fo4")).unwrap())
            .collect::<Vec<_>>();
        for local in [0x0FA847, 0x0FA84A, 0x0FA84B] {
            add_record_native(
                target_master_handles[0],
                Record::new(
                    SigCode(*b"KYWD"),
                    FormKey {
                        local,
                        plugin: interner.intern("Fallout4.esm"),
                    },
                ),
                &schema,
                &interner,
            )
            .unwrap();
        }
        let fixup_config = FixupConfig {
            target_master_handle_ids: target_master_handles.clone(),
            target_schema: Some(std::sync::Arc::clone(&schema)),
            ..FixupConfig::default()
        };
        for master in &target_masters {
            plugin_handle_add_master_no_py(target, master, None).unwrap();
        }
        for local in [0x0FA847, 0x0FA84A, 0x0FA84B] {
            add_record_native(
                target,
                Record::new(SigCode(*b"REFR"), source_fk(local, &interner)),
                &schema,
                &interner,
            )
            .unwrap();
        }
        for info in &lowered {
            add_record_native(target, info.record.clone(), &schema, &interner).unwrap();
        }
        {
            let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
            let mut session = crate::session::open_session(target, Some(source)).unwrap();
            RewriteRawObjectTemplateFormIdsFixup
                .run_with_session(&mut session, &mut mapper, &fixup_config)
                .unwrap();
            RemapStructInternalFormIdsFixup
                .run_with_session(&mut session, &mut mapper, &fixup_config)
                .unwrap();
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("FalloutNV.esm");
        plugin_handle_save_preserving_identity_no_py(target, path.to_str().unwrap()).unwrap();
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let mut session = crate::session::open_session(reopened, None).unwrap();
        let masters = session.target_masters().to_vec();
        assert_eq!(masters, target_masters);
        let expected = [
            (0x130161, vec![0x0FA847]),
            (0x134B9B, vec![0x0FA847, 0x0FA847]),
            (0x15734B, vec![0x0FA84B]),
            (0x15734C, vec![0x0FA84A]),
            (0x15734D, vec![0x0FA84A]),
        ];
        for (local, expected_emotions) in expected {
            let record = session
                .record_decoded(&source_fk(local, &interner), &schema, &interner)
                .unwrap();
            let actual = record
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"TRDA")
                .map(|field| {
                    let FieldValue::Bytes(bytes) = &field.value else {
                        panic!("INFO {local:06X} TRDA did not preserve raw layout");
                    };
                    u32::from_le_bytes(bytes[0..4].try_into().unwrap())
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual
                    .iter()
                    .map(|emotion| emotion & 0x00FF_FFFF)
                    .collect::<Vec<_>>(),
                expected_emotions,
                "INFO {local:06X}"
            );
            assert!(
                actual.iter().all(|emotion| (*emotion >> 24) == 0),
                "INFO {local:06X} emotion was remapped to an output-owned collision"
            );
        }
        drop(session);
        plugin_handle_close_native(reopened);
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
        for handle in target_master_handles {
            plugin_handle_close_native(handle);
        }
    }

    #[test]
    fn native_row_groups_keep_begin_and_end_result_script_phases() {
        let interner = StringInterner::new();
        let speaker = source_fk(0x1300F0, &interner);
        let mut source = info(0x134B9B, speaker, 1, 0, &interner);
        let sctx = interner.intern("SCTX");
        source.fields.insert(
            1,
            field(
                b"SCHR",
                FieldValue::List(vec![FieldValue::Struct(vec![(
                    sctx,
                    FieldValue::Bytes(SmallVec::from_slice(b"SetStage VTechatticup 10")),
                )])]),
            ),
        );
        source.fields.insert(
            2,
            field(
                b"NEXT",
                FieldValue::List(vec![FieldValue::Struct(vec![(
                    sctx,
                    FieldValue::Bytes(SmallVec::from_slice(b"SetStage VTechatticup 100")),
                )])]),
            ),
        );
        let mut mapper = mapper(&interner);
        mapper.add_mapping(speaker, target_fk(0x1300F0, &interner));
        let lowered = lower_info_record(
            &source,
            target_fk(0x134B9B, &interner),
            voice(speaker, &interner),
            &mut mapper,
            &interner,
        )
        .unwrap();
        assert_eq!(
            lowered.fragment_phases,
            [InfoFragmentPhase::Begin, InfoFragmentPhase::End]
        );
        assert_eq!(
            lowered.embedded_script_sources,
            [
                b"SetStage VTechatticup 10".to_vec(),
                b"SetStage VTechatticup 100".to_vec()
            ]
        );
    }

    #[test]
    fn flat_next_marker_assigns_trailing_schr_and_sctx_to_end_phase() {
        let interner = StringInterner::new();
        let speaker = source_fk(0x1300F0, &interner);
        let mut source = info(0x134B9B, speaker, 1, 0, &interner);
        source.fields.insert(1, field(b"NEXT", FieldValue::None));
        source
            .fields
            .insert(2, field(b"SCHR", FieldValue::Bytes(SmallVec::new())));
        source.fields.insert(
            3,
            field(
                b"SCTX",
                FieldValue::Bytes(SmallVec::from_slice(b"SetStage VTechatticup 100")),
            ),
        );
        let mut mapper = mapper(&interner);
        mapper.add_mapping(speaker, target_fk(0x1300F0, &interner));
        let lowered = lower_info_record(
            &source,
            target_fk(0x134B9B, &interner),
            voice(speaker, &interner),
            &mut mapper,
            &interner,
        )
        .unwrap();
        assert_eq!(lowered.fragment_phases, [InfoFragmentPhase::End]);
        assert_eq!(
            lowered.embedded_script_sources,
            [b"SetStage VTechatticup 100".to_vec()]
        );
    }

    #[test]
    fn exact_hostage_greeting_projection_builds_keyword_topic_and_three_children() {
        let interner = StringInterner::new();
        let owner = source_fk(0x11F935, &interner);
        let speaker = source_fk(0x123193, &interner);
        let mut infos = (0..3)
            .map(|index| {
                let mut record = info(0x15734B + index, speaker, 1, 0, &interner);
                if let FieldValue::Bytes(bytes) = &mut record
                    .fields
                    .iter_mut()
                    .find(|field| field.sig.0 == *b"DATA")
                    .unwrap()
                    .value
                {
                    bytes[2] = if index == 2 { 0x23 } else { 0x03 };
                }
                if let FieldValue::Bytes(bytes) = &mut record
                    .fields
                    .iter_mut()
                    .find(|field| field.sig.0 == *b"TRDT")
                    .unwrap()
                    .value
                {
                    bytes[0..4].copy_from_slice(&[3_u32, 1, 1][index as usize].to_le_bytes());
                }
                record
                    .fields
                    .iter_mut()
                    .find(|field| field.sig.0 == *b"NAM1")
                    .unwrap()
                    .value = FieldValue::String(interner.intern(
                    [
                        "I'm getting out of here.",
                        "Fuck this place.",
                        "The Legion will pay for this.",
                    ][index as usize],
                ));
                record.fields.retain(|field| field.sig.0 != *b"PNAM");
                record
                    .fields
                    .push(field(b"QSTI", FieldValue::FormKey(owner)));
                let mut bytes = vec![0; 28];
                bytes[4..8].copy_from_slice(&1_f32.to_le_bytes());
                bytes[8..10].copy_from_slice(&72_u16.to_le_bytes());
                bytes[12..16].copy_from_slice(&0x123193_u32.to_le_bytes());
                record
                    .fields
                    .push(field(b"CTDA", FieldValue::Bytes(SmallVec::from_vec(bytes))));
                record
            })
            .collect::<Vec<_>>();
        let mut mapper = mapper(&interner);
        mapper.add_mapping(speaker, target_fk(0x123193, &interner));
        let target_infos = [
            target_fk(0x900101, &interner),
            target_fk(0x900102, &interner),
            target_fk(0x900103, &interner),
        ];
        let voices = vec![voice(speaker, &interner); 3];
        let projection = lower_hostage_greeting_projection(
            &topic(0x0000C8, &[owner], &interner),
            &infos,
            target_fk(0x900001, &interner),
            target_fk(0x900002, &interner),
            target_fk(0x11F935, &interner),
            &target_infos,
            source_fk(0x02AB62, &interner),
            &voices,
            &mut mapper,
            &interner,
        )
        .unwrap();
        assert_eq!(projection.keyword.sig.0, *b"KYWD");
        assert_eq!(projection.topic_children, target_infos);
        assert_eq!(projection.infos.len(), 3);
        assert!(projection.topic.record.fields.iter().any(|field| {
            field.sig.0 == *b"KNAM"
                && field.value == FieldValue::FormKey(target_fk(0x900001, &interner))
        }));
        assert!(projection.topic.record.fields.iter().any(|field| {
            field.sig.0 == *b"SNAM"
                && field.value == FieldValue::Bytes(SmallVec::from_slice(b"CUST"))
        }));
        assert!(
            projection
                .topic
                .record
                .fields
                .iter()
                .any(|field| { field.sig.0 == *b"TIFC" && field.value == FieldValue::Uint(3) })
        );
        assert!(projection.infos.iter().all(|info| {
            info.record.fields.iter().any(|field| {
                field.sig.0 == *b"ANAM"
                    && field.value == FieldValue::FormKey(target_fk(0x123193, &interner))
            })
        }));

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
        add_record_native(
            handle,
            Record::new(SigCode(*b"QUST"), target_fk(0x11F935, &interner)),
            &schema,
            &interner,
        )
        .unwrap();
        add_record_native(handle, projection.keyword.clone(), &schema, &interner).unwrap();
        assert!(
            add_quest_child_record_native(
                handle,
                projection.topic.record.clone(),
                &schema,
                &interner,
            )
            .unwrap()
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Converted.esm");
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(handle);
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        assert_eq!(
            find_parsed_record(&slot.parsed.root_items, "KYWD", 0x900001)
                .and_then(parsed_editor_id),
            Some("FNV_FO3_TecMineHostageFreedGreeting")
        );
        assert_eq!(
            find_parsed_record(&slot.parsed.root_items, "DIAL", 0x900002)
                .and_then(parsed_editor_id),
            Some("FNV_FO3_TecMineHostageGreeting")
        );
        let greeting = find_parsed_record(&slot.parsed.root_items, "DIAL", 0x900002).unwrap();
        assert!(greeting.parse_error.is_none());
        assert_eq!(
            parsed_subrecords(greeting, "DATA"),
            [b"\0\0\0\0".as_slice()]
        );
        assert_eq!(parsed_subrecords(greeting, "SNAM"), [b"CUST".as_slice()]);
        drop(store);
        plugin_handle_close_native(reopened);

        if let FieldValue::Bytes(bytes) = &mut infos[0]
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"CTDA")
            .unwrap()
            .value
        {
            bytes[12..16].copy_from_slice(&0x123194_u32.to_le_bytes());
        }
        let error = lower_hostage_greeting_info_record(
            &infos[0],
            target_infos[0],
            source_fk(0x02AB62, &interner),
            voice(speaker, &interner),
            &mut mapper,
            &interner,
        )
        .unwrap_err();
        assert!(error.to_string().contains("GetIsID 123193"));
    }

    #[test]
    fn golden_138a74_emits_valid_zero_info_topic() {
        let interner = StringInterner::new();
        let quest = source_fk(0x11F935, &interner);
        let lowered = lower_dial_topic(
            &topic(0x138A74, &[quest], &interner),
            target_fk(0x138A74, &interner),
            &HashMap::from([(quest, target_fk(0x11F935, &interner))]),
            &[],
            0,
            &interner,
        )
        .unwrap();
        assert!(
            lowered[0]
                .record
                .fields
                .iter()
                .any(|entry| { entry.sig.0 == *b"TIFC" && entry.value == FieldValue::Uint(0) })
        );
    }

    #[test]
    fn runnable_scene_topics_save_with_scene_subtype_only() {
        let interner = StringInterner::new();
        let owner = source_fk(0x11F935, &interner);
        let target_owner = target_fk(0x11F935, &interner);
        let owners = HashMap::from([(owner, target_owner)]);
        let scene_130 = lower_dial_topic(
            &topic(0x13015B, &[owner], &interner),
            target_fk(0x13015B, &interner),
            &owners,
            &[],
            1,
            &interner,
        )
        .unwrap()
        .remove(0);
        let mut source_134 = topic(0x134B9A, &[owner], &interner);
        let FieldValue::Bytes(data) = &mut source_134
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"DATA")
            .unwrap()
            .value
        else {
            unreachable!();
        };
        data[1] = 0x20;
        let scene_134 = lower_dial_topic(
            &source_134,
            target_fk(0x134B9A, &interner),
            &owners,
            &[],
            1,
            &interner,
        )
        .unwrap()
        .remove(0);
        let disconnected = lower_dial_topic(
            &topic(0x138A74, &[owner], &interner),
            target_fk(0x138A74, &interner),
            &owners,
            &[],
            0,
            &interner,
        )
        .unwrap()
        .remove(0);
        let greeting = lower_hostage_greeting_topic(
            &topic(0x0000C8, &[owner], &interner),
            target_fk(0x900002, &interner),
            target_owner,
            target_fk(0x900001, &interner),
            &interner,
        )
        .unwrap();

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
        for topic in [&scene_130, &scene_134, &disconnected, &greeting] {
            add_record_native(handle, topic.record.clone(), &schema, &interner).unwrap();
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Converted.esm");
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(handle);

        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        for (local, expected_data) in [
            (0x13015B, [0x00, 0x02, 0x0E, 0x00]),
            (0x134B9A, [0x20, 0x02, 0x0E, 0x00]),
        ] {
            let record = find_parsed_record(&slot.parsed.root_items, "DIAL", local).unwrap();
            assert!(record.parse_error.is_none(), "DIAL {local:06X}");
            assert_eq!(
                parsed_subrecords(record, "DATA"),
                [expected_data.as_slice()]
            );
            assert_eq!(parsed_subrecords(record, "SNAM"), [b"SCEN".as_slice()]);
        }
        let disconnected = find_parsed_record(&slot.parsed.root_items, "DIAL", 0x138A74).unwrap();
        assert!(parsed_subrecords(disconnected, "SNAM").is_empty());
        let greeting = find_parsed_record(&slot.parsed.root_items, "DIAL", 0x900002).unwrap();
        assert_eq!(
            parsed_subrecords(greeting, "DATA"),
            [b"\0\0\0\0".as_slice()]
        );
        assert_eq!(parsed_subrecords(greeting, "SNAM"), [b"CUST".as_slice()]);
        drop(store);
        plugin_handle_close_native(reopened);
    }

    #[test]
    fn repeated_qsti_requires_deterministic_clone_targets() {
        let interner = StringInterner::new();
        let owners = [
            source_fk(0x220000, &interner),
            source_fk(0x110000, &interner),
        ];
        let mappings = HashMap::from([
            (owners[0], target_fk(0x220000, &interner)),
            (owners[1], target_fk(0x110000, &interner)),
        ]);
        let error = lower_dial_topic(
            &topic(0x13015B, &owners, &interner),
            target_fk(0x13015B, &interner),
            &mappings,
            &[],
            1,
            &interner,
        )
        .expect_err("missing clone target must fail closed");
        assert!(matches!(
            error,
            DialogueLowerError::AmbiguousTopicOwner { .. }
        ));
        let clones = lower_dial_topic(
            &topic(0x13015B, &owners, &interner),
            target_fk(0x13015B, &interner),
            &mappings,
            &[target_fk(0x200001, &interner)],
            1,
            &interner,
        )
        .unwrap();
        assert_eq!(clones[0].source_owner.local, 0x110000);
        assert_eq!(clones[1].source_owner.local, 0x220000);
        assert_eq!(clones[0].record.fields[0].sig.0, *b"EDID");
        assert_eq!(
            clones[0].record.fields[0].value,
            FieldValue::String(interner.intern("VDialogue13015B"))
        );
        assert_eq!(clones[1].record.fields[0].sig.0, *b"EDID");
        assert_eq!(
            clones[1].record.fields[0].value,
            FieldValue::String(interner.intern("VDialogue13015B_Q220000"))
        );
    }

    #[test]
    fn speaker_voice_resolution_mismatch_fails_closed() {
        let interner = StringInterner::new();
        let speaker = source_fk(0x1300F0, &interner);
        let other = source_fk(0x1300F1, &interner);
        let mut mapper = mapper(&interner);
        mapper.add_mapping(speaker, target_fk(0x1300F0, &interner));
        let error = lower_info_record(
            &info(0x130161, speaker, 1, 1, &interner),
            target_fk(0x130161, &interner),
            voice(other, &interner),
            &mut mapper,
            &interner,
        )
        .expect_err("wrong speaker metadata must fail closed");
        assert!(matches!(
            error,
            DialogueLowerError::SpeakerVoiceMismatch { .. }
        ));
    }
}
