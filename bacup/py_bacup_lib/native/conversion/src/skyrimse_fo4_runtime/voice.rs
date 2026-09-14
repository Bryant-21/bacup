//! Deterministic evidence contract for Skyrim dialogue voice assets.
//!
//! This is intentionally an admission model, not an audio converter.  A caller
//! supplies source and converted-artifact evidence; the model either records a
//! complete FO4 voice intent or a stable rejection that can keep the owning
//! dialogue component out of production emission.

use std::collections::HashSet;
use std::fmt;

use crate::ids::FormKey;

use super::dialogue::SkyrimDialogueVoiceRequirement;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimVoiceAudioFormat {
    Xwm,
    Wav,
}

impl SkyrimVoiceAudioFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Xwm => "xwm",
            Self::Wav => "wav",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimVoiceAssetProvenance {
    SourceArchive {
        archive: String,
    },
    LooseFile,
    Converted {
        source_blake3: String,
        converter: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimVoiceAssetEvidence {
    /// Relative to the source or target Data root.  The planner canonicalizes
    /// it to lower-case backslash form before putting it in a receipt.
    pub relative_path: String,
    pub exists: bool,
    pub byte_len: u64,
    pub blake3: String,
    pub provenance: SkyrimVoiceAssetProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimVoiceTypeDependency {
    pub source_voice_type: FormKey,
    pub target_voice_type: FormKey,
    /// Target VTYP editor ID, used as the FO4 Sound\\Voice directory identity.
    pub target_identity: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimSequencePlaybackKind {
    Scene,
    StartGameEnabledQuest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimSequenceAssetInput {
    pub kind: SkyrimSequencePlaybackKind,
    pub source_seq: Option<SkyrimVoiceAssetEvidence>,
    pub target_seq: Option<SkyrimVoiceAssetEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimSequenceAssetIntent {
    pub kind: SkyrimSequencePlaybackKind,
    pub source_seq: SkyrimVoiceAssetEvidence,
    pub target_seq: SkyrimVoiceAssetEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueVoiceAssetInput {
    pub source_info: FormKey,
    pub target_info: FormKey,
    pub source_speaker: FormKey,
    pub target_speaker: FormKey,
    pub response_number: u8,
    pub transcript: String,
    pub voice_type: Option<SkyrimVoiceTypeDependency>,
    pub source_fuz: Option<SkyrimVoiceAssetEvidence>,
    pub source_lip: Option<SkyrimVoiceAssetEvidence>,
    pub target_audio: Option<(SkyrimVoiceAudioFormat, SkyrimVoiceAssetEvidence)>,
    pub target_lip: Option<SkyrimVoiceAssetEvidence>,
    /// Kept independent from the line assets because a `.seq` is a playback
    /// scheduling requirement, not speech or lip data.
    pub sequence: Option<SkyrimSequenceAssetInput>,
}

impl SkyrimDialogueVoiceAssetInput {
    /// Preserves the existing INFO/speaker/response transcript identity while
    /// attaching the evidence gathered by the asset conversion stage.
    pub fn from_dialogue_requirement(
        requirement: &SkyrimDialogueVoiceRequirement,
        voice_type: Option<SkyrimVoiceTypeDependency>,
        source_fuz: Option<SkyrimVoiceAssetEvidence>,
        source_lip: Option<SkyrimVoiceAssetEvidence>,
        target_audio: Option<(SkyrimVoiceAudioFormat, SkyrimVoiceAssetEvidence)>,
        target_lip: Option<SkyrimVoiceAssetEvidence>,
        sequence: Option<SkyrimSequenceAssetInput>,
    ) -> Self {
        Self {
            source_info: requirement.source_info,
            target_info: requirement.target_info,
            source_speaker: requirement.source_speaker,
            target_speaker: requirement.target_speaker,
            response_number: requirement.response_number,
            transcript: requirement.transcript.clone(),
            voice_type,
            source_fuz,
            source_lip,
            target_audio,
            target_lip,
            sequence,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimVoiceRejection {
    MissingVoiceType,
    InvalidVoiceIdentity,
    EmptyTranscript,
    MissingSourceFuz,
    MissingSourceLip,
    InvalidSourceFuz(String),
    InvalidSourceLip(String),
    MissingConvertedAudio,
    InvalidConvertedAudio(String),
    MissingConvertedLip,
    InvalidConvertedLip(String),
    MissingSourceSequence(SkyrimSequencePlaybackKind),
    InvalidSourceSequence(String),
    MissingConvertedSequence(SkyrimSequencePlaybackKind),
    InvalidConvertedSequence(String),
}

impl fmt::Display for SkyrimVoiceRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVoiceType => formatter.write_str("missing target voice type"),
            Self::InvalidVoiceIdentity => formatter.write_str("invalid target voice identity"),
            Self::EmptyTranscript => formatter.write_str("empty voiced response transcript"),
            Self::MissingSourceFuz => formatter.write_str("missing source FUZ evidence"),
            Self::MissingSourceLip => formatter.write_str("missing source LIP evidence"),
            Self::InvalidSourceFuz(reason) => {
                write!(formatter, "invalid source FUZ evidence: {reason}")
            }
            Self::InvalidSourceLip(reason) => {
                write!(formatter, "invalid source LIP evidence: {reason}")
            }
            Self::MissingConvertedAudio => {
                formatter.write_str("missing converted XWM/WAV evidence")
            }
            Self::InvalidConvertedAudio(reason) => {
                write!(formatter, "invalid converted XWM/WAV evidence: {reason}")
            }
            Self::MissingConvertedLip => formatter.write_str("missing converted LIP evidence"),
            Self::InvalidConvertedLip(reason) => {
                write!(formatter, "invalid converted LIP evidence: {reason}")
            }
            Self::MissingSourceSequence(kind) => {
                write!(formatter, "missing source {kind:?} .seq evidence")
            }
            Self::InvalidSourceSequence(reason) => {
                write!(formatter, "invalid source .seq evidence: {reason}")
            }
            Self::MissingConvertedSequence(kind) => {
                write!(formatter, "missing converted {kind:?} .seq evidence")
            }
            Self::InvalidConvertedSequence(reason) => {
                write!(formatter, "invalid converted .seq evidence: {reason}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimVoiceAdmission {
    Ready,
    Rejected(SkyrimVoiceRejection),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueVoiceAssetIntent {
    pub source_info: FormKey,
    pub target_info: FormKey,
    pub source_speaker: FormKey,
    pub target_speaker: FormKey,
    pub response_number: u8,
    pub transcript: String,
    pub voice_type: Option<SkyrimVoiceTypeDependency>,
    pub target_voice_path: Option<String>,
    pub source_fuz: Option<SkyrimVoiceAssetEvidence>,
    pub source_lip: Option<SkyrimVoiceAssetEvidence>,
    pub target_audio: Option<(SkyrimVoiceAudioFormat, SkyrimVoiceAssetEvidence)>,
    pub target_lip: Option<SkyrimVoiceAssetEvidence>,
    pub sequence: Option<SkyrimSequenceAssetIntent>,
    pub admission: SkyrimVoiceAdmission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueVoiceAssetReceipt {
    pub output_plugin: String,
    pub intents: Vec<SkyrimDialogueVoiceAssetIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimVoiceContractError {
    InvalidOutputPlugin,
    DuplicateResponseIdentity {
        target_info: FormKey,
        response_number: u8,
    },
    TargetAssetCollision(String),
    ReceiptNotCanonical,
}

impl fmt::Display for SkyrimVoiceContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOutputPlugin => {
                formatter.write_str("voice output plugin has no usable namespace")
            }
            Self::DuplicateResponseIdentity {
                target_info,
                response_number,
            } => write!(
                formatter,
                "duplicate target INFO {:06X} response {response_number}",
                target_info.local
            ),
            Self::TargetAssetCollision(path) => {
                write!(formatter, "voice target asset collision at {path:?}")
            }
            Self::ReceiptNotCanonical => {
                formatter.write_str("voice receipt is not in deterministic canonical form")
            }
        }
    }
}

impl std::error::Error for SkyrimVoiceContractError {}

/// Builds an evidence-only FO4 voice receipt.  This never copies, decodes, or
/// synthesizes FUZ, XWM/WAV, LIP, or SEQ content.
pub fn plan_dialogue_voice_assets(
    output_plugin: &str,
    inputs: &[SkyrimDialogueVoiceAssetInput],
) -> Result<SkyrimDialogueVoiceAssetReceipt, SkyrimVoiceContractError> {
    let output_plugin = output_plugin_namespace(output_plugin)?;
    let mut seen_response_ids = HashSet::with_capacity(inputs.len());
    let mut intents = Vec::with_capacity(inputs.len());

    for input in inputs {
        if !seen_response_ids.insert((input.target_info, input.response_number)) {
            return Err(SkyrimVoiceContractError::DuplicateResponseIdentity {
                target_info: input.target_info,
                response_number: input.response_number,
            });
        }
        intents.push(plan_one(&output_plugin, input));
    }
    intents.sort_by_key(intent_sort_key);
    let receipt = SkyrimDialogueVoiceAssetReceipt {
        output_plugin,
        intents,
    };
    receipt.validate_shape()?;
    Ok(receipt)
}

impl SkyrimDialogueVoiceAssetReceipt {
    /// Validates deterministic ordering, target-path uniqueness, and that an
    /// admitted line has all source and converted artifact evidence.
    pub fn validate_shape(&self) -> Result<(), SkyrimVoiceContractError> {
        if output_plugin_namespace(&self.output_plugin).is_err() {
            return Err(SkyrimVoiceContractError::InvalidOutputPlugin);
        }
        if self
            .intents
            .windows(2)
            .any(|pair| intent_sort_key(&pair[0]) > intent_sort_key(&pair[1]))
        {
            return Err(SkyrimVoiceContractError::ReceiptNotCanonical);
        }

        let mut response_ids = HashSet::with_capacity(self.intents.len());
        let mut target_paths = HashSet::new();
        for intent in &self.intents {
            if !response_ids.insert((intent.target_info, intent.response_number)) {
                return Err(SkyrimVoiceContractError::DuplicateResponseIdentity {
                    target_info: intent.target_info,
                    response_number: intent.response_number,
                });
            }
            if let SkyrimVoiceAdmission::Ready = &intent.admission {
                validate_ready_intent(intent, &self.output_plugin)?;
                for path in ready_target_paths(intent) {
                    if !target_paths.insert(path.clone()) {
                        return Err(SkyrimVoiceContractError::TargetAssetCollision(path));
                    }
                }
            }
        }
        Ok(())
    }
}

fn plan_one(
    output_plugin: &str,
    input: &SkyrimDialogueVoiceAssetInput,
) -> SkyrimDialogueVoiceAssetIntent {
    let mut intent = SkyrimDialogueVoiceAssetIntent {
        source_info: input.source_info,
        target_info: input.target_info,
        source_speaker: input.source_speaker,
        target_speaker: input.target_speaker,
        response_number: input.response_number,
        transcript: input.transcript.trim().to_owned(),
        voice_type: input.voice_type.clone(),
        target_voice_path: None,
        source_fuz: input.source_fuz.clone().map(canonical_evidence),
        source_lip: input.source_lip.clone().map(canonical_evidence),
        target_audio: input
            .target_audio
            .clone()
            .map(|(format, evidence)| (format, canonical_evidence(evidence))),
        target_lip: input.target_lip.clone().map(canonical_evidence),
        sequence: None,
        admission: SkyrimVoiceAdmission::Ready,
    };

    let Some(voice_type) = intent.voice_type.as_ref() else {
        intent.admission = SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::MissingVoiceType);
        return intent;
    };
    let Some(identity) = canonical_path_component(&voice_type.target_identity) else {
        intent.admission =
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::InvalidVoiceIdentity);
        return intent;
    };
    let stem = format!(
        "{:06x}_{:02}",
        intent.target_info.local, intent.response_number
    );
    let target_directory = format!(r"sound\voice\{output_plugin}\{identity}");
    intent.target_voice_path = Some(target_directory.clone());

    if intent.transcript.is_empty() {
        intent.admission = SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::EmptyTranscript);
        return intent;
    }
    let Some(source_fuz) = intent.source_fuz.as_ref() else {
        intent.admission = SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::MissingSourceFuz);
        return intent;
    };
    if let Err(reason) = validate_source_asset(source_fuz, "fuz") {
        intent.admission =
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::InvalidSourceFuz(reason));
        return intent;
    }
    let Some(source_lip) = intent.source_lip.as_ref() else {
        intent.admission = SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::MissingSourceLip);
        return intent;
    };
    if let Err(reason) = validate_source_asset(source_lip, "lip") {
        intent.admission =
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::InvalidSourceLip(reason));
        return intent;
    }
    let Some((audio_format, target_audio)) = intent.target_audio.as_ref() else {
        intent.admission =
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::MissingConvertedAudio);
        return intent;
    };
    let expected_audio_path = format!(r"{target_directory}\{stem}.{}", audio_format.extension());
    if let Err(reason) = validate_converted_asset(
        target_audio,
        audio_format.extension(),
        &source_fuz.blake3,
        &expected_audio_path,
    ) {
        intent.admission =
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::InvalidConvertedAudio(reason));
        return intent;
    }
    let Some(target_lip) = intent.target_lip.as_ref() else {
        intent.admission =
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::MissingConvertedLip);
        return intent;
    };
    let expected_lip_path = format!(r"{target_directory}\{stem}.lip");
    if let Err(reason) =
        validate_converted_asset(target_lip, "lip", &source_lip.blake3, &expected_lip_path)
    {
        intent.admission =
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::InvalidConvertedLip(reason));
        return intent;
    }

    if let Some(sequence) = &input.sequence {
        match plan_sequence(sequence) {
            Ok(sequence) => intent.sequence = Some(sequence),
            Err(rejection) => intent.admission = SkyrimVoiceAdmission::Rejected(rejection),
        }
    }
    intent
}

fn plan_sequence(
    input: &SkyrimSequenceAssetInput,
) -> Result<SkyrimSequenceAssetIntent, SkyrimVoiceRejection> {
    let source = input
        .source_seq
        .clone()
        .map(canonical_evidence)
        .ok_or(SkyrimVoiceRejection::MissingSourceSequence(input.kind))?;
    validate_source_asset(&source, "seq").map_err(SkyrimVoiceRejection::InvalidSourceSequence)?;
    let target = input
        .target_seq
        .clone()
        .map(canonical_evidence)
        .ok_or(SkyrimVoiceRejection::MissingConvertedSequence(input.kind))?;
    validate_converted_asset(&target, "seq", &source.blake3, &target.relative_path)
        .map_err(SkyrimVoiceRejection::InvalidConvertedSequence)?;
    Ok(SkyrimSequenceAssetIntent {
        kind: input.kind,
        source_seq: source,
        target_seq: target,
    })
}

fn validate_ready_intent(
    intent: &SkyrimDialogueVoiceAssetIntent,
    output_plugin: &str,
) -> Result<(), SkyrimVoiceContractError> {
    let Some(voice_type) = intent.voice_type.as_ref() else {
        return Err(SkyrimVoiceContractError::ReceiptNotCanonical);
    };
    let Some(identity) = canonical_path_component(&voice_type.target_identity) else {
        return Err(SkyrimVoiceContractError::ReceiptNotCanonical);
    };
    let expected_directory = format!(r"sound\voice\{output_plugin}\{identity}");
    if intent.target_voice_path.as_deref() != Some(expected_directory.as_str())
        || intent.transcript.trim().is_empty()
    {
        return Err(SkyrimVoiceContractError::ReceiptNotCanonical);
    }
    let (audio_format, audio) = intent
        .target_audio
        .as_ref()
        .ok_or(SkyrimVoiceContractError::ReceiptNotCanonical)?;
    let fuz = intent
        .source_fuz
        .as_ref()
        .ok_or(SkyrimVoiceContractError::ReceiptNotCanonical)?;
    let source_lip = intent
        .source_lip
        .as_ref()
        .ok_or(SkyrimVoiceContractError::ReceiptNotCanonical)?;
    let target_lip = intent
        .target_lip
        .as_ref()
        .ok_or(SkyrimVoiceContractError::ReceiptNotCanonical)?;
    let stem = format!(
        "{:06x}_{:02}",
        intent.target_info.local, intent.response_number
    );
    validate_source_asset(fuz, "fuz").map_err(|_| SkyrimVoiceContractError::ReceiptNotCanonical)?;
    validate_source_asset(source_lip, "lip")
        .map_err(|_| SkyrimVoiceContractError::ReceiptNotCanonical)?;
    validate_converted_asset(
        audio,
        audio_format.extension(),
        &fuz.blake3,
        &format!(r"{expected_directory}\{stem}.{}", audio_format.extension()),
    )
    .map_err(|_| SkyrimVoiceContractError::ReceiptNotCanonical)?;
    validate_converted_asset(
        target_lip,
        "lip",
        &source_lip.blake3,
        &format!(r"{expected_directory}\{stem}.lip"),
    )
    .map_err(|_| SkyrimVoiceContractError::ReceiptNotCanonical)?;
    if let Some(sequence) = &intent.sequence {
        validate_source_asset(&sequence.source_seq, "seq")
            .map_err(|_| SkyrimVoiceContractError::ReceiptNotCanonical)?;
        validate_converted_asset(
            &sequence.target_seq,
            "seq",
            &sequence.source_seq.blake3,
            &sequence.target_seq.relative_path,
        )
        .map_err(|_| SkyrimVoiceContractError::ReceiptNotCanonical)?;
    }
    Ok(())
}

fn ready_target_paths(intent: &SkyrimDialogueVoiceAssetIntent) -> Vec<String> {
    let mut paths = Vec::with_capacity(3);
    if let Some((_, audio)) = &intent.target_audio {
        paths.push(audio.relative_path.clone());
    }
    if let Some(lip) = &intent.target_lip {
        paths.push(lip.relative_path.clone());
    }
    if let Some(sequence) = &intent.sequence {
        paths.push(sequence.target_seq.relative_path.clone());
    }
    paths
}

fn validate_source_asset(
    evidence: &SkyrimVoiceAssetEvidence,
    extension: &str,
) -> Result<(), String> {
    validate_evidence_basics(evidence, extension)?;
    match &evidence.provenance {
        SkyrimVoiceAssetProvenance::SourceArchive { archive } if !archive.trim().is_empty() => {
            Ok(())
        }
        SkyrimVoiceAssetProvenance::LooseFile => Ok(()),
        SkyrimVoiceAssetProvenance::SourceArchive { .. } => {
            Err("source archive is empty".to_string())
        }
        SkyrimVoiceAssetProvenance::Converted { .. } => {
            Err("source asset is marked converted".to_string())
        }
    }
}

fn validate_converted_asset(
    evidence: &SkyrimVoiceAssetEvidence,
    extension: &str,
    expected_source_blake3: &str,
    expected_path: &str,
) -> Result<(), String> {
    validate_evidence_basics(evidence, extension)?;
    if evidence.relative_path != expected_path {
        return Err(format!(
            "target path is {:?}, expected {:?}",
            evidence.relative_path, expected_path
        ));
    }
    match &evidence.provenance {
        SkyrimVoiceAssetProvenance::Converted {
            source_blake3,
            converter,
        } if source_blake3.eq_ignore_ascii_case(expected_source_blake3)
            && !converter.trim().is_empty() =>
        {
            Ok(())
        }
        SkyrimVoiceAssetProvenance::Converted { source_blake3, .. }
            if !source_blake3.eq_ignore_ascii_case(expected_source_blake3) =>
        {
            Err("converted artifact is not proven from the required source hash".to_string())
        }
        SkyrimVoiceAssetProvenance::Converted { .. } => {
            Err("converted artifact has no converter provenance".to_string())
        }
        _ => Err("target asset is not marked converted".to_string()),
    }
}

fn validate_evidence_basics(
    evidence: &SkyrimVoiceAssetEvidence,
    extension: &str,
) -> Result<(), String> {
    if !evidence.exists || evidence.byte_len == 0 {
        return Err("file is absent or empty".to_string());
    }
    if !is_blake3(&evidence.blake3) {
        return Err("BLAKE3 is not a 64-digit hexadecimal value".to_string());
    }
    let path = normalize_asset_path(&evidence.relative_path)?;
    if path != evidence.relative_path {
        return Err("path is not canonical".to_string());
    }
    if !path.ends_with(&format!(".{extension}")) {
        return Err(format!("path does not end in .{extension}"));
    }
    Ok(())
}

fn canonical_evidence(mut evidence: SkyrimVoiceAssetEvidence) -> SkyrimVoiceAssetEvidence {
    evidence.relative_path = normalize_asset_path(&evidence.relative_path).unwrap_or_else(|_| {
        evidence
            .relative_path
            .trim()
            .replace('/', r"\")
            .to_ascii_lowercase()
    });
    evidence.blake3 = evidence.blake3.to_ascii_lowercase();
    if let SkyrimVoiceAssetProvenance::Converted { source_blake3, .. } = &mut evidence.provenance {
        *source_blake3 = source_blake3.to_ascii_lowercase();
    }
    evidence
}

fn normalize_asset_path(path: &str) -> Result<String, String> {
    let mut normalized = path.trim().replace('/', r"\");
    normalized = normalized.trim_matches('\\').to_owned();
    let lower = normalized.to_ascii_lowercase();
    if lower.starts_with(r"data\") {
        normalized = normalized[5..].to_owned();
    }
    if normalized.is_empty()
        || normalized
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == ".." || part.contains(':'))
    {
        return Err("path is absolute or contains traversal".to_string());
    }
    Ok(normalized.to_ascii_lowercase())
}

fn canonical_path_component(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let component: String = value
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                Some(character.to_ascii_lowercase())
            } else if character.is_ascii_whitespace() {
                Some('_')
            } else {
                None
            }
        })
        .collect();
    (!component.is_empty() && component != "." && component != "..").then_some(component)
}

fn output_plugin_namespace(plugin: &str) -> Result<String, SkyrimVoiceContractError> {
    let stem = plugin
        .trim()
        .rsplit_once('.')
        .map_or(plugin.trim(), |(stem, _)| stem);
    canonical_path_component(stem).ok_or(SkyrimVoiceContractError::InvalidOutputPlugin)
}

fn intent_sort_key(intent: &SkyrimDialogueVoiceAssetIntent) -> (u32, u8, u32, u32) {
    (
        intent.target_info.local,
        intent.response_number,
        intent.source_info.local,
        intent.target_speaker.local,
    )
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    fn key(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("Skyrim.esm"),
        }
    }

    fn hash(letter: char) -> String {
        std::iter::repeat_n(letter, 64).collect()
    }

    fn source(path: &str, hash: &str) -> SkyrimVoiceAssetEvidence {
        SkyrimVoiceAssetEvidence {
            relative_path: path.to_owned(),
            exists: true,
            byte_len: 1,
            blake3: hash.to_owned(),
            provenance: SkyrimVoiceAssetProvenance::SourceArchive {
                archive: "Skyrim - Voices_en0.bsa".to_string(),
            },
        }
    }

    fn converted(path: &str, source_hash: &str, hash: &str) -> SkyrimVoiceAssetEvidence {
        SkyrimVoiceAssetEvidence {
            relative_path: path.to_owned(),
            exists: true,
            byte_len: 1,
            blake3: hash.to_owned(),
            provenance: SkyrimVoiceAssetProvenance::Converted {
                source_blake3: source_hash.to_owned(),
                converter: "voice-transcode-v1".to_string(),
            },
        }
    }

    fn valid_input(interner: &StringInterner, local: u32) -> SkyrimDialogueVoiceAssetInput {
        let fuz_hash = hash('a');
        let lip_hash = hash('b');
        SkyrimDialogueVoiceAssetInput {
            source_info: key(interner, 0x123),
            target_info: key(interner, local),
            source_speaker: key(interner, 0x456),
            target_speaker: key(interner, 0x789),
            response_number: 1,
            transcript: "  A spoken line.  ".to_string(),
            voice_type: Some(SkyrimVoiceTypeDependency {
                source_voice_type: key(interner, 0x10),
                target_voice_type: key(interner, 0x20),
                target_identity: "Nord Male".to_string(),
            }),
            source_fuz: Some(source(
                "Data/Sound/Voice/Skyrim.esm/nord/fuz.fuz",
                &fuz_hash,
            )),
            source_lip: Some(source("Sound/Voice/Skyrim.esm/nord/lip.lip", &lip_hash)),
            target_audio: Some((
                SkyrimVoiceAudioFormat::Xwm,
                converted(
                    &format!(r"sound\voice\converted\nord_male\{:06x}_01.xwm", local),
                    &fuz_hash,
                    &hash('c'),
                ),
            )),
            target_lip: Some(converted(
                &format!(r"sound\voice\converted\nord_male\{:06x}_01.lip", local),
                &lip_hash,
                &hash('d'),
            )),
            sequence: None,
        }
    }

    #[test]
    fn builds_canonical_voice_identity_filename_and_receipt() {
        let interner = StringInterner::default();
        let input = valid_input(&interner, 0x4567);
        let receipt = plan_dialogue_voice_assets("Converted.esp", &[input]).unwrap();
        assert_eq!(receipt.output_plugin, "converted");
        let intent = &receipt.intents[0];
        assert_eq!(intent.transcript, "A spoken line.");
        assert_eq!(
            intent.target_voice_path.as_deref(),
            Some(r"sound\voice\converted\nord_male")
        );
        assert_eq!(intent.admission, SkyrimVoiceAdmission::Ready);
        assert_eq!(receipt.validate_shape(), Ok(()));
    }

    #[test]
    fn missing_voice_type_and_fuz_or_lip_are_stable_rejections() {
        let interner = StringInterner::default();
        let mut missing_voice = valid_input(&interner, 0x4567);
        missing_voice.voice_type = None;
        let receipt = plan_dialogue_voice_assets("Converted.esp", &[missing_voice]).unwrap();
        assert_eq!(
            receipt.intents[0].admission,
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::MissingVoiceType)
        );

        let mut missing_fuz = valid_input(&interner, 0x4567);
        missing_fuz.source_fuz = None;
        let receipt = plan_dialogue_voice_assets("Converted.esp", &[missing_fuz]).unwrap();
        assert_eq!(
            receipt.intents[0].admission,
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::MissingSourceFuz)
        );

        let mut missing_lip = valid_input(&interner, 0x4567);
        missing_lip.source_lip = None;
        let receipt = plan_dialogue_voice_assets("Converted.esp", &[missing_lip]).unwrap();
        assert_eq!(
            receipt.intents[0].admission,
            SkyrimVoiceAdmission::Rejected(SkyrimVoiceRejection::MissingSourceLip)
        );
    }

    #[test]
    fn path_normalization_and_collisions_are_enforced() {
        assert_eq!(
            normalize_asset_path(r" Data/Sound/Voice/Test.FUZ ").unwrap(),
            r"sound\voice\test.fuz"
        );
        assert!(normalize_asset_path(r"sound\voice\..\bad.fuz").is_err());

        let interner = StringInterner::default();
        let mut first = valid_input(&interner, 0x4567);
        let mut duplicate = valid_input(&interner, 0x4568);
        let seq_hash = hash('e');
        let target_seq = converted("Sound/Sequences/shared.seq", &seq_hash, &hash('f'));
        first.sequence = Some(SkyrimSequenceAssetInput {
            kind: SkyrimSequencePlaybackKind::Scene,
            source_seq: Some(source("Sound/Sequences/first.seq", &seq_hash)),
            target_seq: Some(target_seq.clone()),
        });
        duplicate.sequence = Some(SkyrimSequenceAssetInput {
            kind: SkyrimSequencePlaybackKind::StartGameEnabledQuest,
            source_seq: Some(source("Sound/Sequences/second.seq", &seq_hash)),
            target_seq: Some(target_seq),
        });
        assert!(matches!(
            plan_dialogue_voice_assets("Converted.esp", &[first, duplicate]),
            Err(SkyrimVoiceContractError::TargetAssetCollision(path)) if path == r"sound\sequences\shared.seq"
        ));
    }

    #[test]
    fn receipt_is_deterministic_and_seq_is_validated_independently() {
        let interner = StringInterner::default();
        let mut later = valid_input(&interner, 0x4568);
        let seq_hash = hash('e');
        later.sequence = Some(SkyrimSequenceAssetInput {
            kind: SkyrimSequencePlaybackKind::Scene,
            source_seq: Some(source("Sound/Sequences/scene.seq", &seq_hash)),
            target_seq: Some(converted(
                "Sound/Sequences/converted_scene.seq",
                &seq_hash,
                &hash('f'),
            )),
        });
        let earlier = valid_input(&interner, 0x4567);
        let receipt =
            plan_dialogue_voice_assets("Converted.esp", &[later.clone(), earlier.clone()]).unwrap();
        let repeat = plan_dialogue_voice_assets("Converted.esp", &[earlier, later]).unwrap();
        assert_eq!(receipt, repeat);
        assert_eq!(receipt.intents[1].admission, SkyrimVoiceAdmission::Ready);
        assert!(receipt.intents[1].sequence.is_some());
    }
}
