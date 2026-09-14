use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::quest_runtime::{
    ConditionIntent, QuestRecordKey, QuestRuntimeAdmission, QuestRuntimeComponentPlan,
    QuestSourceGame, TopologyPlacement, VmadIntent,
};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::sym::{StringInterner, Sym};

const QUEST_CHILD_GROUP_TYPE: u32 = 10;
const SHARED_SCENE_FLAGS: u32 = 0x1f;
const SHARED_ACTOR_FLAGS: u32 = 0x03;
const SHARED_ACTOR_BEHAVIOR_FLAGS: u32 = 0xff;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkyrimSceneRejectionCode {
    InvalidSourceRecord,
    UnsupportedSourceField,
    MissingSourceEvidence,
    WrongSourceGame,
    ComponentNotAdmitted,
    InvalidComponentContract,
    InvalidSceneIdentity,
    SceneNotOwned,
    MissingTargetMapping,
    UnsupportedTopology,
    UnsupportedSceneFlags,
    InvalidPhaseLayout,
    UnsupportedCondition,
    InvalidActorBinding,
    InvalidActionLayout,
    UnsupportedAction,
    IncompleteDependencyClosure,
    UnsupportedFragmentBinding,
}

impl SkyrimSceneRejectionCode {
    pub const fn stable_name(self) -> &'static str {
        match self {
            Self::InvalidSourceRecord => "invalid_source_record",
            Self::UnsupportedSourceField => "unsupported_source_field",
            Self::MissingSourceEvidence => "missing_source_evidence",
            Self::WrongSourceGame => "wrong_source_game",
            Self::ComponentNotAdmitted => "component_not_admitted",
            Self::InvalidComponentContract => "invalid_component_contract",
            Self::InvalidSceneIdentity => "invalid_scene_identity",
            Self::SceneNotOwned => "scene_not_owned",
            Self::MissingTargetMapping => "missing_target_mapping",
            Self::UnsupportedTopology => "unsupported_topology",
            Self::UnsupportedSceneFlags => "unsupported_scene_flags",
            Self::InvalidPhaseLayout => "invalid_phase_layout",
            Self::UnsupportedCondition => "unsupported_condition",
            Self::InvalidActorBinding => "invalid_actor_binding",
            Self::InvalidActionLayout => "invalid_action_layout",
            Self::UnsupportedAction => "unsupported_action",
            Self::IncompleteDependencyClosure => "incomplete_dependency_closure",
            Self::UnsupportedFragmentBinding => "unsupported_fragment_binding",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimSceneRejection {
    pub code: SkyrimSceneRejectionCode,
    pub detail: String,
}

impl SkyrimSceneRejection {
    fn new(code: SkyrimSceneRejectionCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for SkyrimSceneRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "skyrim_scene.{}: {}",
            self.code.stable_name(),
            self.detail
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkyrimSceneFragmentPhase {
    SceneBegin,
    SceneEnd,
    PhaseStart(u32),
    PhaseCompletion(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimSceneFragmentBinding {
    pub fragment_id: String,
    pub phase: SkyrimSceneFragmentPhase,
    pub entrypoint: String,
    pub target_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimScenePhaseSource {
    pub index: u32,
    pub name: String,
    pub start_conditions: Vec<ConditionIntent>,
    pub completion_conditions: Vec<ConditionIntent>,
    pub editor_width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimSceneActorSource {
    pub alias_id: i32,
    pub flags: u32,
    pub behavior_flags: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkyrimSceneActionKind {
    Dialogue {
        topic: QuestRecordKey,
        headtrack_alias: Option<i32>,
        looping_min: f32,
        looping_max: f32,
        emotion_type: u32,
        emotion_value: u32,
    },
    Package {
        package: QuestRecordKey,
    },
    Timer {
        duration_seconds: f32,
    },
    Unsupported {
        source_type: u16,
        required_semantics: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimSceneActionSource {
    pub index: u32,
    pub name: String,
    pub actor_alias: Option<i32>,
    pub flags: u32,
    pub start_phase: u32,
    pub end_phase: u32,
    pub kind: SkyrimSceneActionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimSceneSource {
    pub scene: QuestRecordKey,
    pub parent_quest: QuestRecordKey,
    pub editor_id: String,
    pub flags: u32,
    pub phases: Vec<SkyrimScenePhaseSource>,
    pub actors: Vec<SkyrimSceneActorSource>,
    pub actions: Vec<SkyrimSceneActionSource>,
    pub start_conditions: Vec<ConditionIntent>,
    pub fragments: Vec<SkyrimSceneFragmentBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimSceneConditionAdmissionEvidence {
    pub source_payload: Vec<u8>,
    pub intent: ConditionIntent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimSceneFragmentAdmissionEvidence {
    pub source_script: String,
    pub source_entrypoint: String,
    pub binding: SkyrimSceneFragmentBinding,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkyrimSceneAdmissionEvidence {
    pub conditions: Vec<SkyrimSceneConditionAdmissionEvidence>,
    pub fragments: Vec<SkyrimSceneFragmentAdmissionEvidence>,
}

#[derive(Debug, Clone)]
pub struct SkyrimSceneAdmission {
    pub source: SkyrimSceneSource,
    pub plan: SkyrimScenePlan,
}

#[derive(Debug, Clone, PartialEq)]
struct SkyrimSceneRecordReceipt {
    signature: SigCode,
    form_key: FormKey,
    editor_id: Option<Sym>,
    flags: RecordFlags,
    fields: Vec<FieldEntry>,
    warnings: Vec<Sym>,
}

impl SkyrimSceneRecordReceipt {
    fn from_record(record: &Record) -> Self {
        Self {
            signature: record.sig,
            form_key: record.form_key,
            editor_id: record.eid,
            flags: record.flags,
            fields: record.fields.iter().cloned().collect(),
            warnings: record.warnings.iter().copied().collect(),
        }
    }

    fn matches(&self, record: &Record) -> bool {
        self.signature == record.sig
            && self.form_key == record.form_key
            && self.editor_id == record.eid
            && self.flags == record.flags
            && self.fields.as_slice() == record.fields.as_slice()
            && self.warnings.as_slice() == record.warnings.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimSceneReceipt {
    pub component_id: String,
    pub source_scene: QuestRecordKey,
    pub target_scene: QuestRecordKey,
    pub source_parent_quest: QuestRecordKey,
    pub target_parent_quest: QuestRecordKey,
    pub group_type: u32,
    pub dependencies: BTreeSet<QuestRecordKey>,
    pub fragments: Vec<SkyrimSceneFragmentBinding>,
    pub pending_vmad: Vec<VmadIntent>,
    expected_record: SkyrimSceneRecordReceipt,
}

impl SkyrimSceneReceipt {
    pub fn validate_materialized(
        &self,
        record: &Record,
        parent_quest: &QuestRecordKey,
        group_type: u32,
        pending_vmad: &[VmadIntent],
    ) -> Result<(), String> {
        if !self.expected_record.matches(record) {
            return Err(format!(
                "materialized Skyrim SCEN {} changed after projection",
                self.target_scene.form_key
            ));
        }
        if parent_quest != &self.target_parent_quest || group_type != self.group_type {
            return Err(format!(
                "materialized Skyrim SCEN {} left its QUST child group",
                self.target_scene.form_key
            ));
        }
        if pending_vmad != self.pending_vmad.as_slice() {
            return Err(format!(
                "materialized Skyrim SCEN {} VMAD intent changed",
                self.target_scene.form_key
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SkyrimScenePlan {
    component_id: String,
    source: SkyrimSceneSource,
    target_scene: Option<QuestRecordKey>,
    target_parent_quest: Option<QuestRecordKey>,
    target_by_source_dependency: BTreeMap<QuestRecordKey, QuestRecordKey>,
    target_dependencies: BTreeSet<QuestRecordKey>,
    pending_vmad: Vec<VmadIntent>,
    rejection: Option<SkyrimSceneRejection>,
}

impl SkyrimScenePlan {
    pub fn derive(component: &QuestRuntimeComponentPlan, source: SkyrimSceneSource) -> Self {
        match validate_source(component, &source) {
            Ok(validated) => Self {
                component_id: component.component_id.clone(),
                source,
                target_scene: Some(validated.target_scene),
                target_parent_quest: Some(validated.target_parent_quest),
                target_by_source_dependency: validated.target_by_source_dependency,
                target_dependencies: validated.target_dependencies,
                pending_vmad: validated.pending_vmad,
                rejection: None,
            },
            Err(rejection) => Self {
                component_id: component.component_id.clone(),
                source,
                target_scene: None,
                target_parent_quest: None,
                target_by_source_dependency: BTreeMap::new(),
                target_dependencies: BTreeSet::new(),
                pending_vmad: Vec::new(),
                rejection: Some(rejection),
            },
        }
    }

    pub fn rejection(&self) -> Option<&SkyrimSceneRejection> {
        self.rejection.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct SkyrimSceneProjection {
    pub parent_quest: QuestRecordKey,
    pub group_type: u32,
    pub record: Record,
    pub pending_vmad: Vec<VmadIntent>,
    pub receipt: SkyrimSceneReceipt,
}

pub fn classify_scene_record(
    component: &QuestRuntimeComponentPlan,
    record: &Record,
    evidence: &SkyrimSceneAdmissionEvidence,
    interner: &StringInterner,
) -> Result<SkyrimSceneSource, SkyrimSceneRejection> {
    if record.sig.0 != *b"SCEN" {
        return reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!("source record is {}, not SCEN", record.sig.as_str()),
        );
    }
    if record.flags.bits() & !RecordFlags::COMPRESSED.bits() != 0 {
        return reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!(
                "SCEN source header flags 0x{:08X} carry unsupported semantics",
                record.flags.bits()
            ),
        );
    }
    for warning in &record.warnings {
        if interner.resolve(*warning) != Some("unknown_codec:empty") {
            return reject(
                SkyrimSceneRejectionCode::InvalidSourceRecord,
                "SCEN source has a non-marker decode warning",
            );
        }
    }

    let scene = record_key("SCEN", record.form_key, interner)?;
    let editor_id = record
        .eid
        .and_then(|value| interner.resolve(value))
        .ok_or_else(|| {
            SkyrimSceneRejection::new(
                SkyrimSceneRejectionCode::InvalidSourceRecord,
                "SCEN source has no decoded EditorID",
            )
        })?
        .to_string();
    let mut cursor = SceneFieldCursor::new(&record.fields);
    let decoded_edid = field_string(cursor.take(b"EDID")?, interner, "EDID")?;
    if decoded_edid != editor_id {
        return reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            "SCEN EDID field and record EditorID disagree",
        );
    }

    let fragments = if cursor.peek_is(b"VMAD") {
        let payload = field_bytes(cursor.take(b"VMAD")?, "VMAD")?;
        classify_scene_vmad(payload, &evidence.fragments)?
    } else if evidence.fragments.is_empty() {
        Vec::new()
    } else {
        return reject(
            SkyrimSceneRejectionCode::MissingSourceEvidence,
            "SCEN fragment evidence exists without a source VMAD",
        );
    };
    let flags = field_u32(cursor.take(b"FNAM")?, "record FNAM")?;

    let mut condition_evidence_index = 0usize;
    let mut phases = Vec::new();
    while cursor.peek_is(b"HNAM") {
        cursor.take_marker(b"HNAM")?;
        let name = field_string(cursor.take(b"NAM0")?, interner, "phase NAM0")?;
        let start_conditions = classify_conditions_until(
            &mut cursor,
            b"NEXT",
            &component.root_quest,
            &evidence.conditions,
            &mut condition_evidence_index,
        )?;
        cursor.take_marker(b"NEXT")?;
        let completion_conditions = classify_conditions_until(
            &mut cursor,
            b"NEXT",
            &component.root_quest,
            &evidence.conditions,
            &mut condition_evidence_index,
        )?;
        cursor.take_marker(b"NEXT")?;
        let editor_width = field_u32(cursor.take(b"WNAM")?, "phase WNAM")?;
        cursor.take_marker(b"HNAM")?;
        phases.push(SkyrimScenePhaseSource {
            index: phases.len() as u32,
            name,
            start_conditions,
            completion_conditions,
            editor_width,
        });
    }

    let mut actors = Vec::new();
    while cursor.peek_is(b"ALID") {
        actors.push(SkyrimSceneActorSource {
            alias_id: field_i32(cursor.take(b"ALID")?, "actor ALID")?,
            flags: field_u32(cursor.take(b"LNAM")?, "actor LNAM")?,
            behavior_flags: field_u32(cursor.take(b"DNAM")?, "actor DNAM")?,
        });
    }

    let mut actions = Vec::new();
    while cursor.peek_is_nonempty(b"ANAM") {
        let source_type = field_u16(cursor.take(b"ANAM")?, "action ANAM")?;
        let name = field_string(cursor.take(b"NAM0")?, interner, "action NAM0")?;
        let actor_alias = match field_i32(cursor.take(b"ALID")?, "action ALID")? {
            -1 => None,
            value => Some(value),
        };
        if cursor.peek_is(b"LNAM") {
            cursor.take_marker(b"LNAM").map_err(|_| {
                SkyrimSceneRejection::new(
                    SkyrimSceneRejectionCode::UnsupportedSourceField,
                    "SCEN action LNAM has unknown nonempty semantics",
                )
            })?;
        }
        let index = field_u32(cursor.take(b"INAM")?, "action INAM")?;
        let action_flags = if cursor.peek_is(b"FNAM") {
            field_u32(cursor.take(b"FNAM")?, "action FNAM")?
        } else {
            0
        };
        let start_phase = field_u32(cursor.take(b"SNAM")?, "action start SNAM")?;
        let end_phase = field_u32(cursor.take(b"ENAM")?, "action ENAM")?;
        let kind = match source_type {
            0 => {
                let topic = record_key(
                    "DIAL",
                    field_form_key(cursor.take(b"DATA")?, "dialogue DATA")?,
                    interner,
                )?;
                let headtrack_alias = match field_i32(cursor.take(b"HTID")?, "dialogue HTID")? {
                    -1 => None,
                    value => Some(value),
                };
                let looping_max = field_f32(cursor.take(b"DMAX")?, "dialogue DMAX")?;
                let looping_min = field_f32(cursor.take(b"DMIN")?, "dialogue DMIN")?;
                let emotion_type = field_u32(cursor.take(b"DEMO")?, "dialogue DEMO")?;
                let emotion_value = field_u32(cursor.take(b"DEVA")?, "dialogue DEVA")?;
                SkyrimSceneActionKind::Dialogue {
                    topic,
                    headtrack_alias,
                    looping_min,
                    looping_max,
                    emotion_type,
                    emotion_value,
                }
            }
            1 => {
                let package = record_key(
                    "PACK",
                    field_form_key(cursor.take(b"PNAM")?, "package PNAM")?,
                    interner,
                )?;
                if cursor.peek_is(b"PNAM") {
                    return reject(
                        SkyrimSceneRejectionCode::UnsupportedAction,
                        format!("package action {index} contains multiple packages"),
                    );
                }
                SkyrimSceneActionKind::Package { package }
            }
            2 => SkyrimSceneActionKind::Timer {
                duration_seconds: field_f32(cursor.take(b"SNAM")?, "timer SNAM")?,
            },
            unsupported => {
                return reject(
                    SkyrimSceneRejectionCode::UnsupportedAction,
                    format!("SCEN source action type {unsupported} is not lowerable"),
                );
            }
        };
        cursor.take_marker(b"ANAM")?;
        actions.push(SkyrimSceneActionSource {
            index,
            name,
            actor_alias,
            flags: action_flags,
            start_phase,
            end_phase,
            kind,
        });
    }

    cursor.take_marker(b"NEXT")?;
    let parent_quest = record_key(
        "QUST",
        field_form_key(cursor.take(b"PNAM")?, "parent PNAM")?,
        interner,
    )?;
    if cursor.peek_is(b"INAM") {
        let last_action = field_u32(cursor.take(b"INAM")?, "last action INAM")?;
        let expected = actions.last().map(|action| action.index).unwrap_or(0);
        if last_action != expected {
            return reject(
                SkyrimSceneRejectionCode::InvalidActionLayout,
                format!("SCEN last action index {last_action} does not equal {expected}"),
            );
        }
    }
    if cursor.peek_is(b"VNAM") {
        require_zero_vnam(cursor.take(b"VNAM")?, interner)?;
    }
    let start_conditions = classify_conditions_until(
        &mut cursor,
        b"\0\0\0\0",
        &parent_quest,
        &evidence.conditions,
        &mut condition_evidence_index,
    )?;
    if !cursor.is_done() {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedSourceField,
            format!(
                "unsupported SCEN source subrecord {} at field {}",
                cursor.peek_signature(),
                cursor.index
            ),
        );
    }
    if condition_evidence_index != evidence.conditions.len() {
        return reject(
            SkyrimSceneRejectionCode::MissingSourceEvidence,
            "SCEN condition evidence contains unused rows",
        );
    }

    Ok(SkyrimSceneSource {
        scene,
        parent_quest,
        editor_id,
        flags,
        phases,
        actors,
        actions,
        start_conditions,
        fragments,
    })
}

pub fn admit_scene_record(
    component: &QuestRuntimeComponentPlan,
    record: &Record,
    evidence: &SkyrimSceneAdmissionEvidence,
    interner: &StringInterner,
) -> Result<SkyrimSceneAdmission, SkyrimSceneRejection> {
    let source = classify_scene_record(component, record, evidence, interner)?;
    let plan = SkyrimScenePlan::derive(component, source.clone());
    if let Some(rejection) = plan.rejection() {
        return Err(rejection.clone());
    }
    Ok(SkyrimSceneAdmission { source, plan })
}

struct SceneFieldCursor<'a> {
    fields: &'a [FieldEntry],
    index: usize,
}

impl<'a> SceneFieldCursor<'a> {
    fn new(fields: &'a [FieldEntry]) -> Self {
        Self { fields, index: 0 }
    }

    fn is_done(&self) -> bool {
        self.index == self.fields.len()
    }

    fn peek_is(&self, signature: &[u8; 4]) -> bool {
        self.fields
            .get(self.index)
            .is_some_and(|field| field.sig.0 == *signature)
    }

    fn peek_is_nonempty(&self, signature: &[u8; 4]) -> bool {
        self.fields
            .get(self.index)
            .is_some_and(|field| field.sig.0 == *signature && !is_empty_marker(&field.value))
    }

    fn peek_signature(&self) -> String {
        self.fields
            .get(self.index)
            .map(|field| field.sig.as_str().to_string())
            .unwrap_or_else(|| "<end>".to_string())
    }

    fn take(&mut self, signature: &[u8; 4]) -> Result<&'a FieldValue, SkyrimSceneRejection> {
        let Some(field) = self.fields.get(self.index) else {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedSourceField,
                format!(
                    "SCEN source ended before required {}",
                    String::from_utf8_lossy(signature)
                ),
            );
        };
        if field.sig.0 != *signature {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedSourceField,
                format!(
                    "SCEN source field {} is not expected {} at position {}",
                    field.sig.as_str(),
                    String::from_utf8_lossy(signature),
                    self.index
                ),
            );
        }
        self.index += 1;
        Ok(&field.value)
    }

    fn take_marker(&mut self, signature: &[u8; 4]) -> Result<(), SkyrimSceneRejection> {
        let value = self.take(signature)?;
        if !is_empty_marker(value) {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedSourceField,
                format!(
                    "SCEN marker {} has a nonempty payload",
                    String::from_utf8_lossy(signature)
                ),
            );
        }
        Ok(())
    }
}

fn classify_conditions_until(
    cursor: &mut SceneFieldCursor<'_>,
    stop: &[u8; 4],
    parent_quest: &QuestRecordKey,
    evidence: &[SkyrimSceneConditionAdmissionEvidence],
    evidence_index: &mut usize,
) -> Result<Vec<ConditionIntent>, SkyrimSceneRejection> {
    let mut conditions = Vec::new();
    while !cursor.is_done() && !cursor.peek_is(stop) {
        if !cursor.peek_is(b"CTDA") {
            if cursor.peek_is(b"CIS1") || cursor.peek_is(b"CIS2") {
                return reject(
                    SkyrimSceneRejectionCode::UnsupportedCondition,
                    "SCEN string-parameter conditions are not lowerable",
                );
            }
            break;
        }
        let payload = field_bytes(cursor.take(b"CTDA")?, "CTDA")?;
        let row = evidence.get(*evidence_index).ok_or_else(|| {
            SkyrimSceneRejection::new(
                SkyrimSceneRejectionCode::MissingSourceEvidence,
                format!("SCEN CTDA {} has no admission evidence", *evidence_index),
            )
        })?;
        *evidence_index += 1;
        if payload != row.source_payload.as_slice() {
            return reject(
                SkyrimSceneRejectionCode::MissingSourceEvidence,
                format!(
                    "SCEN CTDA {} evidence payload does not match",
                    *evidence_index - 1
                ),
            );
        }
        validate_source_ctda(payload, &row.intent, parent_quest)?;
        conditions.push(row.intent.clone());
    }
    Ok(conditions)
}

fn validate_source_ctda(
    payload: &[u8],
    intent: &ConditionIntent,
    parent_quest: &QuestRecordKey,
) -> Result<(), SkyrimSceneRejection> {
    if payload.len() != 32
        || payload[0..4] != [0, 0, 0, 0]
        || f32::from_le_bytes(payload[4..8].try_into().unwrap()) != 1.0
        || u16::from_le_bytes(payload[8..10].try_into().unwrap()) != 59
        || payload[10..12] != [0, 0]
        || u32::from_le_bytes(payload[20..24].try_into().unwrap()) != 0
        || u32::from_le_bytes(payload[24..28].try_into().unwrap()) != 0
        || i32::from_le_bytes(payload[28..32].try_into().unwrap()) != -1
    {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedCondition,
            "SCEN CTDA is not the supported GetStageDone == 1 Subject shape",
        );
    }
    let stage = u32::from_le_bytes(payload[16..20].try_into().unwrap());
    if stage > u16::MAX as u32
        || intent
            .parameters
            .get(1)
            .and_then(|value| value.parse().ok())
            != Some(stage as u16)
    {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedCondition,
            "SCEN CTDA stage and semantic evidence disagree",
        );
    }
    validate_conditions(std::slice::from_ref(intent), parent_quest)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawSceneFragment {
    phase: SkyrimSceneFragmentPhase,
    script: String,
    entrypoint: String,
}

fn classify_scene_vmad(
    payload: &[u8],
    evidence: &[SkyrimSceneFragmentAdmissionEvidence],
) -> Result<Vec<SkyrimSceneFragmentBinding>, SkyrimSceneRejection> {
    let raw = parse_scene_vmad_fragments(payload)?;
    if raw.len() != evidence.len() {
        return reject(
            SkyrimSceneRejectionCode::MissingSourceEvidence,
            format!(
                "SCEN VMAD has {} fragments but {} evidence rows",
                raw.len(),
                evidence.len()
            ),
        );
    }
    let mut bindings = Vec::with_capacity(raw.len());
    let mut seen = BTreeSet::new();
    for (index, (source, row)) in raw.iter().zip(evidence).enumerate() {
        if source.script != row.source_script
            || source.entrypoint != row.source_entrypoint
            || source.phase != row.binding.phase
            || !seen.insert((
                row.source_script.clone(),
                row.source_entrypoint.clone(),
                row.binding.phase,
            ))
        {
            return reject(
                SkyrimSceneRejectionCode::MissingSourceEvidence,
                format!("SCEN VMAD fragment evidence row {index} is not exact"),
            );
        }
        bindings.push(row.binding.clone());
    }
    Ok(bindings)
}

fn parse_scene_vmad_fragments(
    payload: &[u8],
) -> Result<Vec<RawSceneFragment>, SkyrimSceneRejection> {
    let mut offset = 0usize;
    let version = vmad_u16(payload, &mut offset)?;
    let object_format = vmad_u16(payload, &mut offset)?;
    let scripts = vmad_u16(payload, &mut offset)?;
    if !matches!(version, 5 | 6) || !matches!(object_format, 1 | 2) || scripts != 0 {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            format!(
                "SCEN VMAD header {version}/{object_format}/{scripts} carries unsupported script semantics"
            ),
        );
    }
    if offset == payload.len() {
        return Ok(Vec::new());
    }
    let fragment_version = vmad_u8(payload, &mut offset)?;
    let flags = vmad_u8(payload, &mut offset)?;
    if fragment_version != 3 || flags & !0x03 != 0 {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD fragment version or flags are unsupported",
        );
    }
    let fragment_script = vmad_string(payload, &mut offset)?;
    let script_flags = vmad_u8(payload, &mut offset)?;
    let property_count = vmad_u16(payload, &mut offset)?;
    if fragment_script.is_empty() || script_flags != 0 || property_count != 0 {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD fragment script flags or properties are not lowerable",
        );
    }

    let mut fragments = Vec::new();
    for (bit, phase) in [
        (0x01, SkyrimSceneFragmentPhase::SceneBegin),
        (0x02, SkyrimSceneFragmentPhase::SceneEnd),
    ] {
        if flags & bit == 0 {
            continue;
        }
        if vmad_u8(payload, &mut offset)? != 0 {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
                "SCEN VMAD scene fragment has nonzero unknown metadata",
            );
        }
        let script = vmad_string(payload, &mut offset)?;
        let entrypoint = vmad_string(payload, &mut offset)?;
        if script != fragment_script || entrypoint.is_empty() {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
                "SCEN VMAD scene fragment script metadata is inconsistent",
            );
        }
        fragments.push(RawSceneFragment {
            phase,
            script,
            entrypoint,
        });
    }

    let phase_count = vmad_u16(payload, &mut offset)?;
    for _ in 0..phase_count {
        let phase_flag = vmad_u8(payload, &mut offset)?;
        let phase_index = u32::from(vmad_u8(payload, &mut offset)?);
        let unknown_i16 = vmad_u16(payload, &mut offset)?;
        let unknown_a = vmad_u8(payload, &mut offset)?;
        let unknown_b = vmad_u8(payload, &mut offset)?;
        if unknown_i16 != 0 || unknown_a != 0 || unknown_b != 0 {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
                "SCEN VMAD phase fragment has nonzero unknown metadata",
            );
        }
        let phase = match phase_flag {
            1 => SkyrimSceneFragmentPhase::PhaseStart(phase_index),
            2 => SkyrimSceneFragmentPhase::PhaseCompletion(phase_index),
            _ => {
                return reject(
                    SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
                    format!("SCEN VMAD phase flag {phase_flag} is unsupported"),
                );
            }
        };
        let script = vmad_string(payload, &mut offset)?;
        let entrypoint = vmad_string(payload, &mut offset)?;
        if script != fragment_script || entrypoint.is_empty() {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
                "SCEN VMAD phase fragment script metadata is inconsistent",
            );
        }
        fragments.push(RawSceneFragment {
            phase,
            script,
            entrypoint,
        });
    }
    if offset != payload.len() {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD has an unclassified trailing payload",
        );
    }
    Ok(fragments)
}

fn vmad_u8(payload: &[u8], offset: &mut usize) -> Result<u8, SkyrimSceneRejection> {
    let value = payload.get(*offset).copied().ok_or_else(|| {
        SkyrimSceneRejection::new(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD is truncated",
        )
    })?;
    *offset += 1;
    Ok(value)
}

fn vmad_u16(payload: &[u8], offset: &mut usize) -> Result<u16, SkyrimSceneRejection> {
    let end = offset.checked_add(2).ok_or_else(|| {
        SkyrimSceneRejection::new(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD offset overflow",
        )
    })?;
    let bytes = payload.get(*offset..end).ok_or_else(|| {
        SkyrimSceneRejection::new(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD is truncated",
        )
    })?;
    *offset = end;
    Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
}

fn vmad_string(payload: &[u8], offset: &mut usize) -> Result<String, SkyrimSceneRejection> {
    let length = usize::from(vmad_u16(payload, offset)?);
    let end = offset.checked_add(length).ok_or_else(|| {
        SkyrimSceneRejection::new(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD string offset overflow",
        )
    })?;
    let bytes = payload.get(*offset..end).ok_or_else(|| {
        SkyrimSceneRejection::new(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD string is truncated",
        )
    })?;
    *offset = end;
    std::str::from_utf8(bytes).map(str::to_string).map_err(|_| {
        SkyrimSceneRejection::new(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN VMAD script metadata is not UTF-8",
        )
    })
}

fn record_key(
    signature: &str,
    form_key: FormKey,
    interner: &StringInterner,
) -> Result<QuestRecordKey, SkyrimSceneRejection> {
    let plugin = interner.resolve(form_key.plugin).ok_or_else(|| {
        SkyrimSceneRejection::new(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            "SCEN source FormKey plugin is not interned",
        )
    })?;
    Ok(QuestRecordKey::new(
        signature,
        format!("{:06X}@{plugin}", form_key.local),
    ))
}

fn field_bytes<'a>(value: &'a FieldValue, label: &str) -> Result<&'a [u8], SkyrimSceneRejection> {
    match value {
        FieldValue::Bytes(bytes) => Ok(bytes),
        _ => reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!("SCEN {label} is not a raw byte payload"),
        ),
    }
}

fn field_string(
    value: &FieldValue,
    interner: &StringInterner,
    label: &str,
) -> Result<String, SkyrimSceneRejection> {
    match value {
        FieldValue::String(value) => {
            interner.resolve(*value).map(str::to_string).ok_or_else(|| {
                SkyrimSceneRejection::new(
                    SkyrimSceneRejectionCode::InvalidSourceRecord,
                    format!("SCEN {label} string is not interned"),
                )
            })
        }
        _ => reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!("SCEN {label} is not a string"),
        ),
    }
}

fn field_u16(value: &FieldValue, label: &str) -> Result<u16, SkyrimSceneRejection> {
    match value {
        FieldValue::Uint(value) => u16::try_from(*value).map_err(|_| {
            SkyrimSceneRejection::new(
                SkyrimSceneRejectionCode::InvalidSourceRecord,
                format!("SCEN {label} is out of range"),
            )
        }),
        _ => reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!("SCEN {label} is not unsigned"),
        ),
    }
}

fn field_u32(value: &FieldValue, label: &str) -> Result<u32, SkyrimSceneRejection> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).map_err(|_| {
            SkyrimSceneRejection::new(
                SkyrimSceneRejectionCode::InvalidSourceRecord,
                format!("SCEN {label} is out of range"),
            )
        }),
        _ => reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!("SCEN {label} is not unsigned"),
        ),
    }
}

fn field_i32(value: &FieldValue, label: &str) -> Result<i32, SkyrimSceneRejection> {
    match value {
        FieldValue::Int(value) => i32::try_from(*value).map_err(|_| {
            SkyrimSceneRejection::new(
                SkyrimSceneRejectionCode::InvalidSourceRecord,
                format!("SCEN {label} is out of range"),
            )
        }),
        _ => reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!("SCEN {label} is not signed"),
        ),
    }
}

fn field_f32(value: &FieldValue, label: &str) -> Result<f32, SkyrimSceneRejection> {
    match value {
        FieldValue::Float(value) => Ok(*value),
        _ => reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!("SCEN {label} is not a float"),
        ),
    }
}

fn field_form_key(value: &FieldValue, label: &str) -> Result<FormKey, SkyrimSceneRejection> {
    match value {
        FieldValue::FormKey(value) => Ok(*value),
        _ => reject(
            SkyrimSceneRejectionCode::InvalidSourceRecord,
            format!("SCEN {label} is not a FormKey"),
        ),
    }
}

fn require_zero_vnam(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<(), SkyrimSceneRejection> {
    let is_zero = match value {
        FieldValue::Bytes(bytes) => bytes.len() == 16 && bytes.iter().all(|byte| *byte == 0),
        FieldValue::Struct(fields) => fields.iter().all(|(_, value)| match value {
            FieldValue::Uint(value) => *value == 0,
            FieldValue::Int(value) => *value == 0,
            _ => false,
        }),
        _ => false,
    };
    if !is_zero {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedSourceField,
            "SCEN VNAM actor behavior settings are not the shared zero shape",
        );
    }
    if let FieldValue::Struct(fields) = value {
        if fields
            .iter()
            .any(|(name, _)| interner.resolve(*name).is_none())
        {
            return reject(
                SkyrimSceneRejectionCode::InvalidSourceRecord,
                "SCEN VNAM has an unresolved field name",
            );
        }
    }
    Ok(())
}

fn is_empty_marker(value: &FieldValue) -> bool {
    matches!(value, FieldValue::None)
        || matches!(value, FieldValue::Bytes(bytes) if bytes.is_empty())
}

pub fn project_scene(
    plan: &SkyrimScenePlan,
    interner: &StringInterner,
) -> Result<SkyrimSceneProjection, String> {
    if let Some(rejection) = &plan.rejection {
        return Err(rejection.to_string());
    }
    let target_scene = plan
        .target_scene
        .as_ref()
        .ok_or("validated Skyrim scene is missing its target scene")?;
    let target_parent_quest = plan
        .target_parent_quest
        .as_ref()
        .ok_or("validated Skyrim scene is missing its target parent quest")?;
    let mut record = Record::new(
        SigCode(*b"SCEN"),
        parse_form_key(&target_scene.form_key, interner)?,
    );
    let editor_id = interner.intern(&plan.source.editor_id);
    record.eid = Some(editor_id);
    push_field(&mut record, b"EDID", FieldValue::String(editor_id));
    push_field(
        &mut record,
        b"FNAM",
        FieldValue::Uint(plan.source.flags.into()),
    );

    for phase in &plan.source.phases {
        push_field(&mut record, b"HNAM", FieldValue::None);
        push_field(
            &mut record,
            b"NAM0",
            FieldValue::String(interner.intern(&phase.name)),
        );
        push_conditions(
            &mut record,
            &phase.start_conditions,
            target_parent_quest,
            interner,
        )?;
        push_field(&mut record, b"NEXT", FieldValue::None);
        push_conditions(
            &mut record,
            &phase.completion_conditions,
            target_parent_quest,
            interner,
        )?;
        push_field(&mut record, b"NEXT", FieldValue::None);
        push_field(
            &mut record,
            b"WNAM",
            FieldValue::Uint(phase.editor_width.into()),
        );
        push_field(&mut record, b"HNAM", FieldValue::None);
    }

    for actor in &plan.source.actors {
        push_field(&mut record, b"ALID", FieldValue::Int(actor.alias_id.into()));
        push_field(&mut record, b"LNAM", FieldValue::Uint(actor.flags.into()));
        push_field(
            &mut record,
            b"DNAM",
            FieldValue::Uint(actor.behavior_flags.into()),
        );
    }

    for action in &plan.source.actions {
        push_field(
            &mut record,
            b"ANAM",
            FieldValue::Uint(action_type(&action.kind).into()),
        );
        push_field(
            &mut record,
            b"NAM0",
            FieldValue::String(interner.intern(&action.name)),
        );
        push_field(
            &mut record,
            b"ALID",
            FieldValue::Int(i64::from(action.actor_alias.unwrap_or(-1))),
        );
        push_field(&mut record, b"INAM", FieldValue::Uint(action.index.into()));
        push_field(&mut record, b"FNAM", FieldValue::Uint(action.flags.into()));
        push_field(
            &mut record,
            b"SNAM",
            FieldValue::Uint(action.start_phase.into()),
        );
        push_field(
            &mut record,
            b"ENAM",
            FieldValue::Uint(action.end_phase.into()),
        );
        match &action.kind {
            SkyrimSceneActionKind::Dialogue {
                topic,
                headtrack_alias,
                looping_min,
                looping_max,
                emotion_type,
                emotion_value,
            } => {
                let target_topic =
                    plan.target_by_source_dependency.get(topic).ok_or_else(|| {
                        format!("SCEN topic {} lost its target mapping", topic.form_key)
                    })?;
                push_field(
                    &mut record,
                    b"DATA",
                    FieldValue::FormKey(parse_form_key(&target_topic.form_key, interner)?),
                );
                push_field(&mut record, b"DMAX", FieldValue::Float(*looping_max));
                push_field(&mut record, b"DMIN", FieldValue::Float(*looping_min));
                push_field(
                    &mut record,
                    b"DEMO",
                    FieldValue::Uint((*emotion_type).into()),
                );
                push_field(
                    &mut record,
                    b"DEVA",
                    FieldValue::Uint((*emotion_value).into()),
                );
                push_field(
                    &mut record,
                    b"HTID",
                    FieldValue::List(
                        headtrack_alias
                            .iter()
                            .map(|alias| FieldValue::Int(i64::from(*alias)))
                            .collect(),
                    ),
                );
            }
            SkyrimSceneActionKind::Package { package } => {
                let target_package =
                    plan.target_by_source_dependency
                        .get(package)
                        .ok_or_else(|| {
                            format!("SCEN package {} lost its target mapping", package.form_key)
                        })?;
                push_field(
                    &mut record,
                    b"PNAM",
                    FieldValue::FormKey(parse_form_key(&target_package.form_key, interner)?),
                )
            }
            SkyrimSceneActionKind::Timer { duration_seconds } => {
                push_field(&mut record, b"SNAM", FieldValue::Float(*duration_seconds));
                push_field(&mut record, b"TNAM", FieldValue::Float(*duration_seconds));
            }
            SkyrimSceneActionKind::Unsupported { .. } => unreachable!("validated action kind"),
        }
        push_field(&mut record, b"ANAM", FieldValue::None);
    }

    push_field(
        &mut record,
        b"PNAM",
        FieldValue::FormKey(parse_form_key(&target_parent_quest.form_key, interner)?),
    );
    push_field(
        &mut record,
        b"INAM",
        FieldValue::Uint(
            plan.source
                .actions
                .last()
                .map(|action| action.index)
                .unwrap_or(0)
                .into(),
        ),
    );
    push_field(
        &mut record,
        b"VNAM",
        FieldValue::Struct(vec![
            (interner.intern("death"), FieldValue::Uint(0)),
            (interner.intern("combat"), FieldValue::Uint(0)),
            (interner.intern("player_dialogue"), FieldValue::Uint(0)),
            (interner.intern("observe_combat"), FieldValue::Uint(0)),
        ]),
    );
    push_conditions(
        &mut record,
        &plan.source.start_conditions,
        target_parent_quest,
        interner,
    )?;

    let receipt = SkyrimSceneReceipt {
        component_id: plan.component_id.clone(),
        source_scene: plan.source.scene.clone(),
        target_scene: target_scene.clone(),
        source_parent_quest: plan.source.parent_quest.clone(),
        target_parent_quest: target_parent_quest.clone(),
        group_type: QUEST_CHILD_GROUP_TYPE,
        dependencies: plan.target_dependencies.clone(),
        fragments: plan.source.fragments.clone(),
        pending_vmad: plan.pending_vmad.clone(),
        expected_record: SkyrimSceneRecordReceipt::from_record(&record),
    };
    Ok(SkyrimSceneProjection {
        parent_quest: target_parent_quest.clone(),
        group_type: QUEST_CHILD_GROUP_TYPE,
        record,
        pending_vmad: plan.pending_vmad.clone(),
        receipt,
    })
}

struct ValidatedScene {
    target_scene: QuestRecordKey,
    target_parent_quest: QuestRecordKey,
    target_by_source_dependency: BTreeMap<QuestRecordKey, QuestRecordKey>,
    target_dependencies: BTreeSet<QuestRecordKey>,
    pending_vmad: Vec<VmadIntent>,
}

fn validate_source(
    component: &QuestRuntimeComponentPlan,
    source: &SkyrimSceneSource,
) -> Result<ValidatedScene, SkyrimSceneRejection> {
    if component.provenance.game != QuestSourceGame::SkyrimSe {
        return reject(
            SkyrimSceneRejectionCode::WrongSourceGame,
            "SCEN projector only accepts Skyrim SE components",
        );
    }
    if !matches!(component.admission, QuestRuntimeAdmission::Supported) {
        return reject(
            SkyrimSceneRejectionCode::ComponentNotAdmitted,
            "quest-runtime component is rejected",
        );
    }
    if !component.validation_issues().is_empty() {
        return reject(
            SkyrimSceneRejectionCode::InvalidComponentContract,
            "quest-runtime component contract has validation issues",
        );
    }
    if source.scene.signature != "SCEN"
        || source.parent_quest.signature != "QUST"
        || source.parent_quest != component.root_quest
        || !valid_editor_id(&source.editor_id)
    {
        return reject(
            SkyrimSceneRejectionCode::InvalidSceneIdentity,
            "scene signature, parent QUST, or EditorID is invalid",
        );
    }
    if !component.owned_records.contains(&source.scene) {
        return reject(
            SkyrimSceneRejectionCode::SceneNotOwned,
            format!("SCEN {} is not component-owned", source.scene.form_key),
        );
    }
    let target_scene = mapped_record(component, &source.scene)?;
    let target_parent_quest = mapped_record(component, &source.parent_quest)?;
    let target_placement = scene_placement(&target_scene, &target_parent_quest);
    if !component.target_topology.contains(&target_placement)
        || !component
            .expected_receipt
            .emitted_records
            .contains(&target_scene)
        || !component
            .expected_receipt
            .placements
            .contains(&target_placement)
    {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedTopology,
            "SCEN must be frozen in its target QUST child group type 10",
        );
    }
    if source.flags & !SHARED_SCENE_FLAGS != 0 {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedSceneFlags,
            format!("SCEN flags 0x{:08X} exceed the shared mask", source.flags),
        );
    }
    validate_phases(source)?;
    validate_actors(component, source)?;
    let source_dependencies = validate_actions(component, source)?;
    validate_conditions(&source.start_conditions, &source.parent_quest)?;
    let pending_vmad = validate_fragments(component, source)?;

    let declared = component
        .owned_records
        .iter()
        .chain(&component.shared_records)
        .chain(&component.dependencies.direct)
        .chain(&component.dependencies.recursive)
        .cloned()
        .collect::<BTreeSet<_>>();
    if source_dependencies
        .iter()
        .any(|dependency| !declared.contains(dependency))
    {
        return reject(
            SkyrimSceneRejectionCode::IncompleteDependencyClosure,
            "SCEN action dependency is absent from the atomic component closure",
        );
    }
    let mut target_dependencies = BTreeSet::from([target_parent_quest.clone()]);
    let mut target_by_source_dependency = BTreeMap::new();
    for dependency in source_dependencies {
        let target = mapped_record(component, &dependency)?;
        target_dependencies.insert(target.clone());
        target_by_source_dependency.insert(dependency, target);
    }
    Ok(ValidatedScene {
        target_scene,
        target_parent_quest,
        target_by_source_dependency,
        target_dependencies,
        pending_vmad,
    })
}

fn validate_phases(source: &SkyrimSceneSource) -> Result<(), SkyrimSceneRejection> {
    if source.phases.is_empty() {
        return reject(
            SkyrimSceneRejectionCode::InvalidPhaseLayout,
            "SCEN has no phases",
        );
    }
    for (expected, phase) in source.phases.iter().enumerate() {
        if phase.index != expected as u32 || phase.name.trim().is_empty() {
            return reject(
                SkyrimSceneRejectionCode::InvalidPhaseLayout,
                "SCEN phases must be named and indexed contiguously from zero",
            );
        }
        validate_conditions(&phase.start_conditions, &source.parent_quest)?;
        validate_conditions(&phase.completion_conditions, &source.parent_quest)?;
    }
    Ok(())
}

fn validate_actors(
    component: &QuestRuntimeComponentPlan,
    source: &SkyrimSceneSource,
) -> Result<(), SkyrimSceneRejection> {
    let quest_aliases = component
        .semantics
        .aliases
        .iter()
        .filter_map(|alias| i32::try_from(alias.id).ok())
        .collect::<BTreeSet<_>>();
    let mut scene_aliases = BTreeSet::new();
    for actor in &source.actors {
        if actor.alias_id < 0
            || !quest_aliases.contains(&actor.alias_id)
            || !scene_aliases.insert(actor.alias_id)
            || actor.flags & !SHARED_ACTOR_FLAGS != 0
            || actor.behavior_flags & !SHARED_ACTOR_BEHAVIOR_FLAGS != 0
        {
            return reject(
                SkyrimSceneRejectionCode::InvalidActorBinding,
                format!("SCEN actor alias {} is unsupported", actor.alias_id),
            );
        }
    }
    Ok(())
}

fn validate_actions(
    component: &QuestRuntimeComponentPlan,
    source: &SkyrimSceneSource,
) -> Result<BTreeSet<QuestRecordKey>, SkyrimSceneRejection> {
    let aliases = source
        .actors
        .iter()
        .map(|actor| actor.alias_id)
        .collect::<BTreeSet<_>>();
    let phase_count = source.phases.len() as u32;
    let mut dependencies = BTreeSet::new();
    for (expected, action) in source.actions.iter().enumerate() {
        if action.index != expected as u32
            || action.name.trim().is_empty()
            || action.start_phase > action.end_phase
            || action.end_phase >= phase_count
            || action.flags != 0
        {
            return reject(
                SkyrimSceneRejectionCode::InvalidActionLayout,
                format!("SCEN action {} has an unsupported layout", action.index),
            );
        }
        match &action.kind {
            SkyrimSceneActionKind::Dialogue {
                topic,
                headtrack_alias,
                looping_min,
                looping_max,
                emotion_type,
                emotion_value,
            } => {
                if topic.signature != "DIAL"
                    || action
                        .actor_alias
                        .is_none_or(|alias| !aliases.contains(&alias))
                    || headtrack_alias.is_some_and(|alias| !aliases.contains(&alias))
                    || !looping_min.is_finite()
                    || !looping_max.is_finite()
                    || *looping_min < 0.0
                    || looping_max < looping_min
                    || *emotion_type > 7
                    || *emotion_value > 100
                {
                    return reject(
                        SkyrimSceneRejectionCode::UnsupportedAction,
                        format!("dialogue action {} has unsupported semantics", action.index),
                    );
                }
                dependencies.insert(topic.clone());
            }
            SkyrimSceneActionKind::Package { package } => {
                if package.signature != "PACK"
                    || action
                        .actor_alias
                        .is_none_or(|alias| !aliases.contains(&alias))
                {
                    return reject(
                        SkyrimSceneRejectionCode::UnsupportedAction,
                        format!("package action {} has unsupported semantics", action.index),
                    );
                }
                dependencies.insert(package.clone());
            }
            SkyrimSceneActionKind::Timer { duration_seconds } => {
                if action.actor_alias.is_some()
                    || !duration_seconds.is_finite()
                    || *duration_seconds <= 0.0
                {
                    return reject(
                        SkyrimSceneRejectionCode::UnsupportedAction,
                        format!("timer action {} has unsupported semantics", action.index),
                    );
                }
            }
            SkyrimSceneActionKind::Unsupported {
                source_type,
                required_semantics,
            } => {
                return reject(
                    SkyrimSceneRejectionCode::UnsupportedAction,
                    format!(
                        "action {} source type {} requires {}",
                        action.index, source_type, required_semantics
                    ),
                );
            }
        }
    }
    for dependency in &dependencies {
        mapped_record(component, dependency)?;
    }
    Ok(dependencies)
}

fn validate_conditions(
    conditions: &[ConditionIntent],
    parent_quest: &QuestRecordKey,
) -> Result<(), SkyrimSceneRejection> {
    for condition in conditions {
        if condition.function != "GetStageDone"
            || !matches!(condition.operator.as_str(), "==" | "EqualTo")
            || condition.comparison_value != "1"
            || condition.parameters.len() != 2
            || !condition.parameters[0].eq_ignore_ascii_case(&parent_quest.form_key)
            || condition.parameters[1].parse::<u16>().is_err()
            || condition.run_on != "Subject"
            || condition.cis1.is_some()
            || condition.cis2.is_some()
        {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedCondition,
                format!("unsupported required condition {}", condition.function),
            );
        }
    }
    Ok(())
}

fn validate_fragments(
    component: &QuestRuntimeComponentPlan,
    source: &SkyrimSceneSource,
) -> Result<Vec<VmadIntent>, SkyrimSceneRejection> {
    let phase_count = source.phases.len() as u32;
    let mut seen = BTreeSet::new();
    let mut classes = BTreeSet::new();
    for binding in &source.fragments {
        if binding.fragment_id.trim().is_empty()
            || binding.entrypoint.trim().is_empty()
            || binding.target_class.trim().is_empty()
            || !seen.insert((binding.fragment_id.clone(), binding.phase))
            || matches!(
                binding.phase,
                SkyrimSceneFragmentPhase::PhaseStart(index)
                    | SkyrimSceneFragmentPhase::PhaseCompletion(index)
                    if index >= phase_count
            )
            || !component.fragments.iter().any(|intent| {
                intent.owner == source.scene
                    && intent.fragment_id == binding.fragment_id
                    && intent.entrypoint == binding.entrypoint
                    && intent.target_class == binding.target_class
            })
            || !component.scripts.iter().any(|intent| {
                intent.owner == source.scene
                    && intent.required
                    && intent.target_class == binding.target_class
            })
        {
            return reject(
                SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
                format!(
                    "SCEN fragment {} lacks an exact script intent",
                    binding.fragment_id
                ),
            );
        }
        classes.insert(binding.target_class.clone());
    }

    let target_scene = mapped_record(component, &source.scene)?;
    let mut pending_vmad = component
        .vmad
        .iter()
        .filter(|intent| intent.owner == target_scene && classes.contains(&intent.script_class))
        .cloned()
        .collect::<Vec<_>>();
    pending_vmad.sort_by(|left, right| left.script_class.cmp(&right.script_class));
    if pending_vmad.len() != classes.len()
        || pending_vmad.iter().any(|intent| {
            !intent.compiler_evidence_required
                || intent.compiler_evidence.is_none()
                || !component.psc.iter().any(|psc| {
                    psc.class_name == intent.script_class
                        && psc.compiler_evidence_required
                        && psc.compiler_evidence == intent.compiler_evidence
                })
        })
    {
        return reject(
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding,
            "SCEN fragments require matching fresh PSC and pending VMAD intents",
        );
    }
    Ok(pending_vmad)
}

fn push_conditions(
    record: &mut Record,
    conditions: &[ConditionIntent],
    target_parent_quest: &QuestRecordKey,
    interner: &StringInterner,
) -> Result<(), String> {
    for condition in conditions {
        let quest = parse_form_key(&target_parent_quest.form_key, interner)?;
        let stage = condition.parameters[1]
            .parse::<u16>()
            .map_err(|_| "invalid Skyrim SCEN condition stage")?;
        push_field(
            record,
            b"CTDA",
            FieldValue::Struct(vec![
                (interner.intern("type"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_1"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
                (interner.intern("comparison_value"), FieldValue::Float(1.0)),
                (interner.intern("function"), FieldValue::Uint(59)),
                (interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_7"), FieldValue::Uint(0)),
                (interner.intern("parameter_1"), FieldValue::FormKey(quest)),
                (
                    interner.intern("parameter_2"),
                    FieldValue::Uint(stage.into()),
                ),
                (interner.intern("run_on"), FieldValue::Uint(0)),
                (interner.intern("reference"), FieldValue::Uint(0)),
                (interner.intern("parameter_3"), FieldValue::Int(-1)),
            ]),
        );
    }
    Ok(())
}

fn mapped_record(
    component: &QuestRuntimeComponentPlan,
    source: &QuestRecordKey,
) -> Result<QuestRecordKey, SkyrimSceneRejection> {
    let matches = component
        .mappings
        .iter()
        .filter(|mapping| &mapping.source == source)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [mapping] => Ok(mapping.target.clone()),
        _ => reject(
            SkyrimSceneRejectionCode::MissingTargetMapping,
            format!(
                "{} {} is not mapped exactly once",
                source.signature, source.form_key
            ),
        ),
    }
}

pub fn scene_placement(scene: &QuestRecordKey, parent_quest: &QuestRecordKey) -> TopologyPlacement {
    TopologyPlacement {
        record: scene.clone(),
        group_path: vec![
            "QUST".to_string(),
            parent_quest.form_key.clone(),
            format!("children:{QUEST_CHILD_GROUP_TYPE}"),
        ],
    }
}

fn parse_form_key(value: &str, interner: &StringInterner) -> Result<FormKey, String> {
    if value.contains('@') {
        return FormKey::parse(value, interner);
    }
    let (local, plugin) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid Skyrim SCEN FormKey {value}"))?;
    FormKey::parse(&format!("{local}@{plugin}"), interner)
}

fn push_field(record: &mut Record, signature: &[u8; 4], value: FieldValue) {
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*signature),
        value,
    });
}

fn action_type(kind: &SkyrimSceneActionKind) -> u16 {
    match kind {
        SkyrimSceneActionKind::Dialogue { .. } => 0,
        SkyrimSceneActionKind::Package { .. } => 1,
        SkyrimSceneActionKind::Timer { .. } => 2,
        SkyrimSceneActionKind::Unsupported { source_type, .. } => *source_type,
    }
}

fn valid_editor_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn reject<T>(
    code: SkyrimSceneRejectionCode,
    detail: impl Into<String>,
) -> Result<T, SkyrimSceneRejection> {
    Err(SkyrimSceneRejection::new(code, detail))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Documented070224SceneClosure {
    pub quest_node: QuestRecordKey,
    pub branch: QuestRecordKey,
    pub quest: QuestRecordKey,
    pub scene: SkyrimSceneSource,
    pub externally_supplied_dependencies: BTreeSet<QuestRecordKey>,
}

impl Documented070224SceneClosure {
    pub fn validate_documented_shape(&self) -> Result<(), String> {
        let expected = [
            (&self.quest_node, "SMQN", 0x070221),
            (&self.branch, "SMBN", 0x070222),
            (&self.quest, "QUST", 0x070223),
            (&self.scene.scene, "SCEN", 0x070224),
        ];
        for (record, signature, local) in expected {
            if record.signature != signature || record_local_id(record)? != local {
                return Err(format!(
                    "documented 070224 closure expected {signature} {local:06X}"
                ));
            }
        }
        if self.scene.parent_quest != self.quest {
            return Err("documented 070224 SCEN is not owned by its QUST".to_string());
        }
        Ok(())
    }
}

fn record_local_id(record: &QuestRecordKey) -> Result<u32, String> {
    let local = record
        .form_key
        .split(['@', ':'])
        .next()
        .ok_or_else(|| format!("invalid documented FormKey {}", record.form_key))?;
    u32::from_str_radix(local.trim_start_matches("0x"), 16)
        .map_err(|_| format!("invalid documented FormKey {}", record.form_key))
}

#[cfg(test)]
mod tests {
    use crate::quest_runtime::{
        AliasFillKind, CompilerEvidenceIntent, DependencyClosure, FragmentIntent, PscIntent,
        QuestAliasIntent, QuestRuntimeExpectedReceipt, QuestSemanticPlan, QuestStartFlag,
        ScriptIntent, SourceProvenance, SourceTargetFormMapping, StartDisposition, VmadIntent,
    };

    use super::*;

    fn key(signature: &str, local: u32, plugin: &str) -> QuestRecordKey {
        QuestRecordKey::new(signature, format!("{local:06X}@{plugin}"))
    }

    fn condition(quest: &QuestRecordKey, stage: u16) -> ConditionIntent {
        ConditionIntent {
            function: "GetStageDone".to_string(),
            operator: "==".to_string(),
            comparison_value: "1".to_string(),
            parameters: vec![quest.form_key.clone(), stage.to_string()],
            run_on: "Subject".to_string(),
            cis1: None,
            cis2: None,
        }
    }

    struct Fixture {
        component: QuestRuntimeComponentPlan,
        source: SkyrimSceneSource,
        target_quest: QuestRecordKey,
    }

    fn fixture() -> Fixture {
        let source_quest = key("QUST", 0x070223, "Synthetic070224.esm");
        let source_scene = key("SCEN", 0x070224, "Synthetic070224.esm");
        let source_topic = key("DIAL", 0x070225, "Synthetic070224.esm");
        let source_package = key("PACK", 0x070226, "Synthetic070224.esm");
        let target_quest = key("QUST", 0x170223, "Output.esm");
        let target_scene = key("SCEN", 0x170224, "Output.esm");
        let target_topic = key("DIAL", 0x170225, "Output.esm");
        let target_package = key("PACK", 0x170226, "Output.esm");
        let evidence = CompilerEvidenceIntent {
            manifest_id: "manifest:scene".to_string(),
            source_digest: "blake3:scene".to_string(),
            require_fresh_output: true,
        };
        let fragment = SkyrimSceneFragmentBinding {
            fragment_id: "scene_begin".to_string(),
            phase: SkyrimSceneFragmentPhase::SceneBegin,
            entrypoint: "Fragment_Begin".to_string(),
            target_class: "B21_SkySF_170224".to_string(),
        };
        let placement = scene_placement(&target_scene, &target_quest);
        let component = QuestRuntimeComponentPlan {
            component_id: "skyrim:070223:scene:070224".to_string(),
            root_quest: source_quest.clone(),
            provenance: SourceProvenance {
                game: QuestSourceGame::SkyrimSe,
                source_plugin: "Synthetic070224.esm".to_string(),
                graft: None,
            },
            owned_records: vec![source_quest.clone(), source_scene.clone()],
            shared_records: BTreeSet::new(),
            dependencies: DependencyClosure {
                direct: BTreeSet::from([source_topic.clone(), source_package.clone()]),
                recursive: BTreeSet::new(),
            },
            source_topology: BTreeSet::new(),
            target_topology: BTreeSet::from([placement.clone()]),
            mappings: vec![
                (source_quest.clone(), target_quest.clone()),
                (source_scene.clone(), target_scene.clone()),
                (source_topic.clone(), target_topic.clone()),
                (source_package.clone(), target_package.clone()),
            ]
            .into_iter()
            .map(|(source, target)| SourceTargetFormMapping { source, target })
            .collect(),
            semantics: QuestSemanticPlan {
                aliases: vec![
                    QuestAliasIntent {
                        id: 1,
                        name: "Speaker".to_string(),
                        fill: AliasFillKind::PairSpecific("fixture".to_string()),
                        conditions: Vec::new(),
                    },
                    QuestAliasIntent {
                        id: 2,
                        name: "Listener".to_string(),
                        fill: AliasFillKind::PairSpecific("fixture".to_string()),
                        conditions: Vec::new(),
                    },
                ],
                start_flags: BTreeSet::from([QuestStartFlag::StartGameEnabled]),
                ..QuestSemanticPlan::default()
            },
            scripts: BTreeSet::from([ScriptIntent {
                owner: source_scene.clone(),
                source_name: "SyntheticSceneFragment".to_string(),
                target_class: fragment.target_class.clone(),
                required: true,
            }]),
            fragments: BTreeSet::from([FragmentIntent {
                owner: source_scene.clone(),
                fragment_id: fragment.fragment_id.clone(),
                entrypoint: fragment.entrypoint.clone(),
                target_class: fragment.target_class.clone(),
            }]),
            psc: BTreeSet::from([PscIntent {
                class_name: fragment.target_class.clone(),
                source_artifact: format!("Scripts/Source/User/{}.psc", fragment.target_class),
                compiler_evidence_required: true,
                compiler_evidence: Some(evidence.clone()),
            }]),
            vmad: BTreeSet::from([VmadIntent {
                owner: target_scene.clone(),
                script_class: fragment.target_class.clone(),
                properties: BTreeSet::new(),
                compiler_evidence_required: true,
                compiler_evidence: Some(evidence),
            }]),
            assets: BTreeSet::new(),
            inbound_producers: BTreeSet::new(),
            start_disposition: Some(StartDisposition::Autostart),
            admission: QuestRuntimeAdmission::Supported,
            expected_receipt: QuestRuntimeExpectedReceipt {
                emitted_records: BTreeSet::from([target_scene]),
                placements: BTreeSet::from([placement]),
                ..QuestRuntimeExpectedReceipt::default()
            },
        };
        let source = SkyrimSceneSource {
            scene: source_scene,
            parent_quest: source_quest.clone(),
            editor_id: "B21_SyntheticScene070224".to_string(),
            flags: 1,
            phases: vec![
                SkyrimScenePhaseSource {
                    index: 0,
                    name: "Opening".to_string(),
                    start_conditions: vec![condition(&source_quest, 10)],
                    completion_conditions: Vec::new(),
                    editor_width: 320,
                },
                SkyrimScenePhaseSource {
                    index: 1,
                    name: "Closing".to_string(),
                    start_conditions: Vec::new(),
                    completion_conditions: vec![condition(&source_quest, 20)],
                    editor_width: 320,
                },
            ],
            actors: vec![
                SkyrimSceneActorSource {
                    alias_id: 1,
                    flags: 0,
                    behavior_flags: 0,
                },
                SkyrimSceneActorSource {
                    alias_id: 2,
                    flags: 2,
                    behavior_flags: 0x20,
                },
            ],
            actions: vec![
                SkyrimSceneActionSource {
                    index: 0,
                    name: "Greeting".to_string(),
                    actor_alias: Some(1),
                    flags: 0,
                    start_phase: 0,
                    end_phase: 0,
                    kind: SkyrimSceneActionKind::Dialogue {
                        topic: source_topic,
                        headtrack_alias: Some(2),
                        looping_min: 0.0,
                        looping_max: 0.0,
                        emotion_type: 0,
                        emotion_value: 0,
                    },
                },
                SkyrimSceneActionSource {
                    index: 1,
                    name: "Travel".to_string(),
                    actor_alias: Some(2),
                    flags: 0,
                    start_phase: 0,
                    end_phase: 1,
                    kind: SkyrimSceneActionKind::Package {
                        package: source_package,
                    },
                },
                SkyrimSceneActionSource {
                    index: 2,
                    name: "Pause".to_string(),
                    actor_alias: None,
                    flags: 0,
                    start_phase: 1,
                    end_phase: 1,
                    kind: SkyrimSceneActionKind::Timer {
                        duration_seconds: 1.5,
                    },
                },
            ],
            start_conditions: vec![condition(&source_quest, 10)],
            fragments: vec![fragment],
        };
        Fixture {
            component,
            source,
            target_quest,
        }
    }

    fn raw_bytes(value: Vec<u8>) -> FieldValue {
        FieldValue::Bytes(value.into_iter().collect())
    }

    fn vmad_string_bytes(value: &str, output: &mut Vec<u8>) {
        output.extend_from_slice(&(value.len() as u16).to_le_bytes());
        output.extend_from_slice(value.as_bytes());
    }

    fn source_scene_vmad() -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&6u16.to_le_bytes());
        payload.extend_from_slice(&2u16.to_le_bytes());
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.push(3);
        payload.push(1);
        vmad_string_bytes("SF_070224", &mut payload);
        payload.push(0);
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.push(0);
        vmad_string_bytes("SF_070224", &mut payload);
        vmad_string_bytes("Fragment_Begin", &mut payload);
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload
    }

    fn source_ctda(stage: u16) -> Vec<u8> {
        let mut payload = vec![0u8; 32];
        payload[4..8].copy_from_slice(&1.0f32.to_le_bytes());
        payload[8..10].copy_from_slice(&59u16.to_le_bytes());
        payload[16..20].copy_from_slice(&u32::from(stage).to_le_bytes());
        payload[28..32].copy_from_slice(&(-1i32).to_le_bytes());
        payload
    }

    fn push_source_condition(
        record: &mut Record,
        evidence: &mut SkyrimSceneAdmissionEvidence,
        intent: &ConditionIntent,
    ) {
        let stage = intent.parameters[1].parse::<u16>().unwrap();
        let payload = source_ctda(stage);
        push_field(record, b"CTDA", raw_bytes(payload.clone()));
        evidence
            .conditions
            .push(SkyrimSceneConditionAdmissionEvidence {
                source_payload: payload,
                intent: intent.clone(),
            });
    }

    fn raw_scene_record(
        fixture: &Fixture,
        interner: &StringInterner,
    ) -> (Record, SkyrimSceneAdmissionEvidence) {
        let mut record = Record::new(
            SigCode(*b"SCEN"),
            parse_form_key(&fixture.source.scene.form_key, interner).unwrap(),
        );
        record.flags = RecordFlags::COMPRESSED;
        let editor_id = interner.intern(&fixture.source.editor_id);
        record.eid = Some(editor_id);
        record.warnings.push(interner.intern("unknown_codec:empty"));
        push_field(&mut record, b"EDID", FieldValue::String(editor_id));
        push_field(&mut record, b"VMAD", raw_bytes(source_scene_vmad()));
        push_field(
            &mut record,
            b"FNAM",
            FieldValue::Uint(fixture.source.flags.into()),
        );

        let mut evidence = SkyrimSceneAdmissionEvidence {
            fragments: vec![SkyrimSceneFragmentAdmissionEvidence {
                source_script: "SF_070224".to_string(),
                source_entrypoint: "Fragment_Begin".to_string(),
                binding: fixture.source.fragments[0].clone(),
            }],
            ..SkyrimSceneAdmissionEvidence::default()
        };
        for phase in &fixture.source.phases {
            push_field(&mut record, b"HNAM", raw_bytes(Vec::new()));
            push_field(
                &mut record,
                b"NAM0",
                FieldValue::String(interner.intern(&phase.name)),
            );
            for intent in &phase.start_conditions {
                push_source_condition(&mut record, &mut evidence, intent);
            }
            push_field(&mut record, b"NEXT", raw_bytes(Vec::new()));
            for intent in &phase.completion_conditions {
                push_source_condition(&mut record, &mut evidence, intent);
            }
            push_field(&mut record, b"NEXT", raw_bytes(Vec::new()));
            push_field(
                &mut record,
                b"WNAM",
                FieldValue::Uint(phase.editor_width.into()),
            );
            push_field(&mut record, b"HNAM", raw_bytes(Vec::new()));
        }
        for actor in &fixture.source.actors {
            push_field(&mut record, b"ALID", FieldValue::Int(actor.alias_id.into()));
            push_field(&mut record, b"LNAM", FieldValue::Uint(actor.flags.into()));
            push_field(
                &mut record,
                b"DNAM",
                FieldValue::Uint(actor.behavior_flags.into()),
            );
        }
        for action in &fixture.source.actions {
            push_field(
                &mut record,
                b"ANAM",
                FieldValue::Uint(action_type(&action.kind).into()),
            );
            push_field(
                &mut record,
                b"NAM0",
                FieldValue::String(interner.intern(&action.name)),
            );
            push_field(
                &mut record,
                b"ALID",
                FieldValue::Int(i64::from(action.actor_alias.unwrap_or(-1))),
            );
            push_field(&mut record, b"INAM", FieldValue::Uint(action.index.into()));
            push_field(&mut record, b"FNAM", FieldValue::Uint(action.flags.into()));
            push_field(
                &mut record,
                b"SNAM",
                FieldValue::Uint(action.start_phase.into()),
            );
            push_field(
                &mut record,
                b"ENAM",
                FieldValue::Uint(action.end_phase.into()),
            );
            match &action.kind {
                SkyrimSceneActionKind::Dialogue {
                    topic,
                    headtrack_alias,
                    looping_min,
                    looping_max,
                    emotion_type,
                    emotion_value,
                } => {
                    push_field(
                        &mut record,
                        b"DATA",
                        FieldValue::FormKey(parse_form_key(&topic.form_key, interner).unwrap()),
                    );
                    push_field(
                        &mut record,
                        b"HTID",
                        FieldValue::Int(i64::from(headtrack_alias.unwrap_or(-1))),
                    );
                    push_field(&mut record, b"DMAX", FieldValue::Float(*looping_max));
                    push_field(&mut record, b"DMIN", FieldValue::Float(*looping_min));
                    push_field(
                        &mut record,
                        b"DEMO",
                        FieldValue::Uint((*emotion_type).into()),
                    );
                    push_field(
                        &mut record,
                        b"DEVA",
                        FieldValue::Uint((*emotion_value).into()),
                    );
                }
                SkyrimSceneActionKind::Package { package } => push_field(
                    &mut record,
                    b"PNAM",
                    FieldValue::FormKey(parse_form_key(&package.form_key, interner).unwrap()),
                ),
                SkyrimSceneActionKind::Timer { duration_seconds } => {
                    push_field(&mut record, b"SNAM", FieldValue::Float(*duration_seconds))
                }
                SkyrimSceneActionKind::Unsupported { .. } => unreachable!(),
            }
            push_field(&mut record, b"ANAM", raw_bytes(Vec::new()));
        }
        push_field(&mut record, b"NEXT", raw_bytes(Vec::new()));
        push_field(
            &mut record,
            b"PNAM",
            FieldValue::FormKey(
                parse_form_key(&fixture.source.parent_quest.form_key, interner).unwrap(),
            ),
        );
        push_field(
            &mut record,
            b"INAM",
            FieldValue::Uint(
                fixture
                    .source
                    .actions
                    .last()
                    .map(|action| action.index)
                    .unwrap_or(0)
                    .into(),
            ),
        );
        push_field(&mut record, b"VNAM", raw_bytes(vec![0; 16]));
        for intent in &fixture.source.start_conditions {
            push_source_condition(&mut record, &mut evidence, intent);
        }
        (record, evidence)
    }

    #[test]
    fn admits_raw_070224_scene_through_ir_projection_and_receipt() {
        let fixture = fixture();
        let interner = StringInterner::new();
        let (record, evidence) = raw_scene_record(&fixture, &interner);
        let admission =
            admit_scene_record(&fixture.component, &record, &evidence, &interner).unwrap();
        assert_eq!(admission.source, fixture.source);

        let closure = Documented070224SceneClosure {
            quest_node: key("SMQN", 0x070221, "Synthetic070224.esm"),
            branch: key("SMBN", 0x070222, "Synthetic070224.esm"),
            quest: key("QUST", 0x070223, "Synthetic070224.esm"),
            scene: admission.source.clone(),
            externally_supplied_dependencies: BTreeSet::new(),
        };
        closure.validate_documented_shape().unwrap();

        let projection = project_scene(&admission.plan, &interner).unwrap();
        assert_eq!(projection.record.sig.0, *b"SCEN");
        assert_eq!(projection.parent_quest, fixture.target_quest);
        assert_eq!(
            projection
                .record
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"NEXT")
                .count(),
            fixture.source.phases.len() * 2
        );
        projection
            .receipt
            .validate_materialized(
                &projection.record,
                &projection.parent_quest,
                projection.group_type,
                &projection.pending_vmad,
            )
            .unwrap();
    }

    #[test]
    fn raw_scene_unknown_subrecord_rejects_stably() {
        let fixture = fixture();
        let interner = StringInterner::new();
        let (mut record, evidence) = raw_scene_record(&fixture, &interner);
        record.fields.insert(
            3,
            FieldEntry {
                sig: SubrecordSig(*b"ZZZZ"),
                value: raw_bytes(vec![1]),
            },
        );
        let error =
            classify_scene_record(&fixture.component, &record, &evidence, &interner).unwrap_err();
        assert_eq!(error.code, SkyrimSceneRejectionCode::UnsupportedSourceField);
        assert_eq!(error.code.stable_name(), "unsupported_source_field");
    }

    #[test]
    fn raw_scene_unsupported_action_rejects_the_whole_scene() {
        let fixture = fixture();
        let interner = StringInterner::new();
        let (mut record, evidence) = raw_scene_record(&fixture, &interner);
        let action_type = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"ANAM" && !is_empty_marker(&field.value))
            .unwrap();
        action_type.value = FieldValue::Uint(3);
        let error =
            classify_scene_record(&fixture.component, &record, &evidence, &interner).unwrap_err();
        assert_eq!(error.code, SkyrimSceneRejectionCode::UnsupportedAction);
    }

    #[test]
    fn raw_scene_vmad_and_condition_evidence_fail_closed() {
        let fixture = fixture();
        let interner = StringInterner::new();
        let (mut record, evidence) = raw_scene_record(&fixture, &interner);
        let vmad = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"VMAD")
            .unwrap();
        let FieldValue::Bytes(bytes) = &mut vmad.value else {
            unreachable!()
        };
        bytes.push(0x7f);
        let error =
            classify_scene_record(&fixture.component, &record, &evidence, &interner).unwrap_err();
        assert_eq!(
            error.code,
            SkyrimSceneRejectionCode::UnsupportedFragmentBinding
        );

        let (record, mut evidence) = raw_scene_record(&fixture, &interner);
        evidence.conditions.remove(0);
        let error =
            classify_scene_record(&fixture.component, &record, &evidence, &interner).unwrap_err();
        assert_eq!(error.code, SkyrimSceneRejectionCode::MissingSourceEvidence);
    }

    #[test]
    fn projects_shared_scene_shapes_and_freezes_receipt() {
        let fixture = fixture();
        let issues = fixture.component.validation_issues();
        assert!(issues.is_empty(), "{issues:#?}");
        let plan = SkyrimScenePlan::derive(&fixture.component, fixture.source);
        assert_eq!(plan.rejection(), None);
        let interner = StringInterner::new();
        let projection = project_scene(&plan, &interner).unwrap();
        assert_eq!(projection.group_type, 10);
        assert_eq!(projection.parent_quest, fixture.target_quest);
        assert_eq!(projection.record.sig.0, *b"SCEN");
        assert_eq!(projection.pending_vmad.len(), 1);
        assert_eq!(
            projection
                .record
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"ANAM")
                .filter_map(|field| match &field.value {
                    FieldValue::Uint(value) => Some(*value),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        projection
            .receipt
            .validate_materialized(
                &projection.record,
                &projection.parent_quest,
                projection.group_type,
                &projection.pending_vmad,
            )
            .unwrap();

        let mut damaged = projection.record.clone();
        damaged.fields.push(FieldEntry {
            sig: SubrecordSig(*b"NNAM"),
            value: FieldValue::String(interner.intern("late mutation")),
        });
        assert!(
            projection
                .receipt
                .validate_materialized(
                    &damaged,
                    &projection.parent_quest,
                    projection.group_type,
                    &projection.pending_vmad,
                )
                .unwrap_err()
                .contains("changed after projection")
        );
    }

    #[test]
    fn unsupported_required_action_rejects_the_whole_scene() {
        let mut fixture = fixture();
        fixture.source.actions[0].flags = 0x8000;
        let plan = SkyrimScenePlan::derive(&fixture.component, fixture.source);
        assert_eq!(
            plan.rejection().map(|reason| reason.code),
            Some(SkyrimSceneRejectionCode::InvalidActionLayout)
        );
        assert!(
            project_scene(&plan, &StringInterner::new())
                .unwrap_err()
                .starts_with("skyrim_scene.invalid_action_layout:")
        );
    }

    #[test]
    fn missing_action_dependency_rejects_the_whole_scene() {
        let mut fixture = fixture();
        fixture.component.dependencies.direct.clear();
        fixture.component.mappings.truncate(2);
        let plan = SkyrimScenePlan::derive(&fixture.component, fixture.source);
        assert_eq!(
            plan.rejection().map(|reason| reason.code),
            Some(SkyrimSceneRejectionCode::MissingTargetMapping)
        );
    }

    #[test]
    fn documented_070224_interface_does_not_invent_external_dependencies() {
        let fixture = fixture();
        let closure = Documented070224SceneClosure {
            quest_node: key("SMQN", 0x070221, "Synthetic070224.esm"),
            branch: key("SMBN", 0x070222, "Synthetic070224.esm"),
            quest: key("QUST", 0x070223, "Synthetic070224.esm"),
            scene: fixture.source,
            externally_supplied_dependencies: BTreeSet::new(),
        };
        closure.validate_documented_shape().unwrap();
        assert!(closure.externally_supplied_dependencies.is_empty());
    }
}
