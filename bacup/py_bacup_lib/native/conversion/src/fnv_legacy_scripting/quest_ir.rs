use serde_json::Value;

const SCRIPT_FIELD_SIGNATURES: &[&str] = &[
    "SCRI", "SCHR", "SCDA", "SCTX", "SLSD", "SCVR", "SCRO", "SCRV",
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LegacyQuestIr {
    pub editor_id: String,
    pub name: Option<Value>,
    pub general: LegacyQuestGeneral,
    pub start_conditions: Vec<LegacyConditionScope>,
    pub stages: Vec<LegacyQuestStage>,
    pub objectives: Vec<LegacyQuestObjective>,
    pub fragments: Vec<QuestFragmentMetadata>,
    pub losses: QuestLossAccounting,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LegacyQuestGeneral {
    pub flags: u8,
    pub priority: u8,
    pub delay_time: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LegacyQuestStage {
    pub index: u16,
    pub entries: Vec<LegacyQuestStageEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LegacyQuestStageEntry {
    pub flags: u8,
    pub conditions: Vec<LegacyConditionScope>,
    pub log_entry: Option<Value>,
    pub note: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LegacyQuestObjective {
    pub index: u16,
    pub flags: u32,
    pub display_text: Option<Value>,
    pub targets: Vec<LegacyQuestTarget>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LegacyQuestTarget {
    pub identity: LegacyTargetIdentity,
    pub flags: u8,
    pub conditions: Vec<LegacyConditionScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyTargetIdentity {
    RawFormId(u32),
    FormKeyText(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LegacyConditionScope {
    pub fields: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestFragmentMetadata {
    pub stage_index: u16,
    pub stage_item_index: u16,
    pub function_name: String,
    pub source_hex: Option<String>,
    pub source_text: Option<String>,
    pub compiled_hex: Option<String>,
    pub global_references: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestLossDisposition {
    Externalized,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestLoss {
    pub scope: String,
    pub signature: String,
    pub disposition: QuestLossDisposition,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestLossAccounting {
    pub entries: Vec<QuestLoss>,
}

impl QuestLossAccounting {
    pub fn terminal_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|loss| loss.disposition == QuestLossDisposition::Terminal)
            .count()
    }

    pub(crate) fn externalized(&mut self, scope: impl Into<String>, signature: &str, reason: &str) {
        self.entries.push(QuestLoss {
            scope: scope.into(),
            signature: signature.to_string(),
            disposition: QuestLossDisposition::Externalized,
            reason: reason.to_string(),
        });
    }

    pub(crate) fn terminal(&mut self, scope: impl Into<String>, signature: &str, reason: &str) {
        self.entries.push(QuestLoss {
            scope: scope.into(),
            signature: signature.to_string(),
            disposition: QuestLossDisposition::Terminal,
            reason: reason.to_string(),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestLoweringError {
    MissingFields,
    MalformedField { signature: String, reason: String },
    MissingStageEntry { stage: u16 },
    OrphanConditionCompanion { signature: String },
    ConditionWithoutTarget { objective: u16 },
    ObjectiveOutOfRange { value: i64 },
    StageOutOfRange { value: i64 },
    MissingRequiredTarget { objective: u16, reason: String },
    ConditionLowering { scope: String, reason: String },
    Vmad(String),
}

impl std::fmt::Display for QuestLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFields => write!(f, "QUST payload has no fields array"),
            Self::MalformedField { signature, reason } => {
                write!(f, "malformed QUST {signature}: {reason}")
            }
            Self::MissingStageEntry { stage } => {
                write!(f, "QUST stage {stage} has no QSDT log entry")
            }
            Self::OrphanConditionCompanion { signature } => {
                write!(f, "orphan QUST condition companion {signature}")
            }
            Self::ConditionWithoutTarget { objective } => {
                write!(
                    f,
                    "QUST objective {objective} condition has no preceding target"
                )
            }
            Self::ObjectiveOutOfRange { value } => {
                write!(f, "QUST objective index {value} does not fit FO4 uint16")
            }
            Self::StageOutOfRange { value } => {
                write!(f, "QUST stage index {value} does not fit FO4 uint16")
            }
            Self::MissingRequiredTarget { objective, reason } => {
                write!(
                    f,
                    "QUST objective {objective} target cannot be lowered: {reason}"
                )
            }
            Self::ConditionLowering { scope, reason } => {
                write!(f, "QUST {scope} condition cannot be lowered: {reason}")
            }
            Self::Vmad(reason) => write!(f, "QUST VMAD cannot be synthesized: {reason}"),
        }
    }
}

impl std::error::Error for QuestLoweringError {}

impl LegacyQuestIr {
    pub(crate) fn parse(record: &Value) -> Result<Self, QuestLoweringError> {
        let fields = record
            .get("fields")
            .and_then(Value::as_array)
            .ok_or(QuestLoweringError::MissingFields)?;
        let editor_id = record
            .get("eid")
            .and_then(Value::as_str)
            .unwrap_or("Unnamed")
            .to_string();
        let mut name = None;
        let mut general = LegacyQuestGeneral {
            flags: 0,
            priority: 0,
            delay_time: 0.0,
        };
        let mut start_conditions = Vec::new();
        let mut stages = Vec::new();
        let mut objectives = Vec::new();
        let mut fragments = Vec::new();
        let mut losses = QuestLossAccounting::default();
        let mut index = 0;

        while index < fields.len() {
            let (signature, value) = field_parts(&fields[index])?;
            match signature {
                "INDX" => {
                    let (stage, next) = parse_stage(fields, index, &mut fragments, &mut losses)?;
                    stages.push(stage);
                    index = next;
                }
                "QOBJ" => {
                    let (objective, next) = parse_objective(fields, index, &mut losses)?;
                    objectives.push(objective);
                    index = next;
                }
                "CTDA" => {
                    let (scope, next) = parse_condition_scope(fields, index)?;
                    start_conditions.push(scope);
                    index = next;
                }
                "CIS1" | "CIS2" => {
                    return Err(QuestLoweringError::OrphanConditionCompanion {
                        signature: signature.to_string(),
                    });
                }
                "EDID" => index += 1,
                "FULL" => {
                    name = Some(value.clone());
                    index += 1;
                }
                "DATA" => {
                    general = parse_general(value)?;
                    index += 1;
                }
                signature if SCRIPT_FIELD_SIGNATURES.contains(&signature) => {
                    losses.externalized("record", signature, "owned by Papyrus/VMAD lowering");
                    index += 1;
                }
                signature => {
                    losses.terminal("record", signature, "no verified FO4 QUST field mapping");
                    index += 1;
                }
            }
        }

        Ok(Self {
            editor_id,
            name,
            general,
            start_conditions,
            stages,
            objectives,
            fragments,
            losses,
        })
    }
}

fn parse_stage(
    fields: &[Value],
    start: usize,
    fragments: &mut Vec<QuestFragmentMetadata>,
    losses: &mut QuestLossAccounting,
) -> Result<(LegacyQuestStage, usize), QuestLoweringError> {
    let (_, index_value) = field_parts(&fields[start])?;
    let stage_value =
        integer_value(index_value).ok_or_else(|| QuestLoweringError::MalformedField {
            signature: "INDX".to_string(),
            reason: "expected int16 stage index".to_string(),
        })?;
    let stage_index = u16::try_from(stage_value)
        .map_err(|_| QuestLoweringError::StageOutOfRange { value: stage_value })?;
    let mut entries = Vec::new();
    let mut index = start + 1;

    while index < fields.len() {
        let (signature, _) = field_parts(&fields[index])?;
        if matches!(
            signature,
            "INDX" | "QOBJ" | "ANAM" | "ALST" | "ALLS" | "ALCS"
        ) {
            break;
        }
        if signature != "QSDT" {
            if matches!(signature, "CTDA" | "CIS1" | "CIS2") {
                return Err(QuestLoweringError::MalformedField {
                    signature: signature.to_string(),
                    reason: format!(
                        "stage {stage_index} condition appeared before a QSDT stage entry"
                    ),
                });
            }
            losses.terminal(
                format!("stage:{stage_index}"),
                signature,
                "field appeared before a QSDT stage entry",
            );
            index += 1;
            continue;
        }
        let (entry, fragment, next) =
            parse_stage_entry(fields, index, stage_index, entries.len(), losses)?;
        entries.push(entry);
        if let Some(fragment) = fragment {
            fragments.push(fragment);
        }
        index = next;
    }

    if entries.is_empty() {
        return Err(QuestLoweringError::MissingStageEntry { stage: stage_index });
    }
    Ok((
        LegacyQuestStage {
            index: stage_index,
            entries,
        },
        index,
    ))
}

fn parse_stage_entry(
    fields: &[Value],
    start: usize,
    stage_index: u16,
    item_index: usize,
    losses: &mut QuestLossAccounting,
) -> Result<(LegacyQuestStageEntry, Option<QuestFragmentMetadata>, usize), QuestLoweringError> {
    let (_, flags_value) = field_parts(&fields[start])?;
    let flags = unsigned_value(flags_value)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| QuestLoweringError::MalformedField {
            signature: "QSDT".to_string(),
            reason: "expected uint8 stage flags".to_string(),
        })?;
    let mut conditions = Vec::new();
    let mut log_entry = None;
    let mut note = None;
    let mut source_hex = None;
    let mut source_text = None;
    let mut compiled_hex = None;
    let mut global_references = Vec::new();
    let mut saw_script = false;
    let mut index = start + 1;

    while index < fields.len() {
        let (signature, value) = field_parts(&fields[index])?;
        if matches!(
            signature,
            "QSDT" | "INDX" | "QOBJ" | "ANAM" | "ALST" | "ALLS" | "ALCS"
        ) {
            break;
        }
        match signature {
            "CTDA" => {
                let (scope, next) = parse_condition_scope(fields, index)?;
                conditions.push(scope);
                index = next;
            }
            "CIS1" | "CIS2" => {
                return Err(QuestLoweringError::OrphanConditionCompanion {
                    signature: signature.to_string(),
                });
            }
            "CNAM" => {
                log_entry = Some(value.clone());
                index += 1;
            }
            "NAM2" => {
                note = Some(value.clone());
                index += 1;
            }
            "SCTX" => {
                saw_script = true;
                losses.externalized(
                    format!("stage:{stage_index}:item:{item_index}"),
                    signature,
                    "source retained in fragment metadata",
                );
                source_hex = raw_hex(value).map(str::to_string);
                source_text = source_hex
                    .as_deref()
                    .and_then(|hex| hex::decode(hex).ok())
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
                index += 1;
            }
            "SCDA" => {
                saw_script = true;
                losses.externalized(
                    format!("stage:{stage_index}:item:{item_index}"),
                    signature,
                    "compiled legacy bytecode retained as evidence only",
                );
                compiled_hex = raw_hex(value).map(str::to_string);
                index += 1;
            }
            "SCRO" => {
                saw_script = true;
                losses.externalized(
                    format!("stage:{stage_index}:item:{item_index}"),
                    signature,
                    "reference retained in fragment metadata",
                );
                if let Some(reference) = form_key_text(value) {
                    global_references.push(reference);
                }
                index += 1;
            }
            signature if SCRIPT_FIELD_SIGNATURES.contains(&signature) => {
                saw_script = true;
                losses.externalized(
                    format!("stage:{stage_index}:item:{item_index}"),
                    signature,
                    "owned by Papyrus/VMAD lowering",
                );
                index += 1;
            }
            signature => {
                losses.terminal(
                    format!("stage:{stage_index}:item:{item_index}"),
                    signature,
                    "no verified FO4 stage-entry mapping",
                );
                index += 1;
            }
        }
    }

    let fragment = saw_script.then(|| QuestFragmentMetadata {
        stage_index,
        stage_item_index: item_index as u16,
        function_name: format!("Fragment_Stage_{stage_index:04}_Item_{item_index:02}"),
        source_hex,
        source_text,
        compiled_hex,
        global_references,
    });
    Ok((
        LegacyQuestStageEntry {
            flags,
            conditions,
            log_entry,
            note,
        },
        fragment,
        index,
    ))
}

fn parse_objective(
    fields: &[Value],
    start: usize,
    losses: &mut QuestLossAccounting,
) -> Result<(LegacyQuestObjective, usize), QuestLoweringError> {
    let (_, index_value) = field_parts(&fields[start])?;
    let objective_value =
        integer_value(index_value).ok_or_else(|| QuestLoweringError::MalformedField {
            signature: "QOBJ".to_string(),
            reason: "expected int32 objective index".to_string(),
        })?;
    let objective_index =
        u16::try_from(objective_value).map_err(|_| QuestLoweringError::ObjectiveOutOfRange {
            value: objective_value,
        })?;
    let mut flags = 0;
    let mut display_text = None;
    let mut targets: Vec<LegacyQuestTarget> = Vec::new();
    let mut index = start + 1;

    while index < fields.len() {
        let (signature, value) = field_parts(&fields[index])?;
        if matches!(
            signature,
            "QOBJ" | "INDX" | "ANAM" | "ALST" | "ALLS" | "ALCS"
        ) {
            break;
        }
        match signature {
            "FNAM" => {
                flags = unsigned_value(value)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| QuestLoweringError::MalformedField {
                        signature: "FNAM".to_string(),
                        reason: "expected objective uint32 flags".to_string(),
                    })?;
                index += 1;
            }
            "NNAM" => {
                display_text = Some(value.clone());
                index += 1;
            }
            "QSTA" => {
                targets.push(parse_target(value)?);
                index += 1;
            }
            "CTDA" => {
                let Some(target) = targets.last_mut() else {
                    return Err(QuestLoweringError::ConditionWithoutTarget {
                        objective: objective_index,
                    });
                };
                let (scope, next) = parse_condition_scope(fields, index)?;
                target.conditions.push(scope);
                index = next;
            }
            "CIS1" | "CIS2" => {
                return Err(QuestLoweringError::OrphanConditionCompanion {
                    signature: signature.to_string(),
                });
            }
            signature => {
                losses.terminal(
                    format!("objective:{objective_index}"),
                    signature,
                    "no verified FO4 objective mapping",
                );
                index += 1;
            }
        }
    }

    Ok((
        LegacyQuestObjective {
            index: objective_index,
            flags,
            display_text,
            targets,
        },
        index,
    ))
}

fn parse_target(value: &Value) -> Result<LegacyQuestTarget, QuestLoweringError> {
    if let Some(bytes) = raw_bytes(value) {
        if bytes.len() != 8 {
            return Err(QuestLoweringError::MalformedField {
                signature: "QSTA".to_string(),
                reason: format!("expected 8 legacy bytes, found {}", bytes.len()),
            });
        }
        let raw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if raw == 0 {
            return Err(QuestLoweringError::MalformedField {
                signature: "QSTA".to_string(),
                reason: "direct target is null".to_string(),
            });
        }
        return Ok(LegacyQuestTarget {
            identity: LegacyTargetIdentity::RawFormId(raw),
            flags: bytes[4],
            conditions: Vec::new(),
        });
    }
    let target = value
        .get("Target")
        .and_then(form_key_text)
        .or_else(|| form_key_text(value))
        .ok_or_else(|| QuestLoweringError::MalformedField {
            signature: "QSTA".to_string(),
            reason: "missing direct target FormKey".to_string(),
        })?;
    let flags = value
        .get("Flags")
        .and_then(unsigned_value)
        .and_then(|value| u8::try_from(value).ok())
        .unwrap_or(0);
    Ok(LegacyQuestTarget {
        identity: LegacyTargetIdentity::FormKeyText(target),
        flags,
        conditions: Vec::new(),
    })
}

fn parse_condition_scope(
    fields: &[Value],
    start: usize,
) -> Result<(LegacyConditionScope, usize), QuestLoweringError> {
    let mut scope = Vec::new();
    let (signature, value) = field_parts(&fields[start])?;
    if signature != "CTDA" {
        return Err(QuestLoweringError::MalformedField {
            signature: signature.to_string(),
            reason: "condition scope must begin with CTDA".to_string(),
        });
    }
    scope.push((signature.to_string(), value.clone()));
    let mut index = start + 1;
    while index < fields.len() {
        let (signature, value) = field_parts(&fields[index])?;
        if !matches!(signature, "CIS1" | "CIS2") {
            break;
        }
        scope.push((signature.to_string(), value.clone()));
        index += 1;
    }
    Ok((LegacyConditionScope { fields: scope }, index))
}

fn parse_general(value: &Value) -> Result<LegacyQuestGeneral, QuestLoweringError> {
    if let Some(bytes) = raw_bytes(value) {
        if bytes.len() != 8 {
            return Err(QuestLoweringError::MalformedField {
                signature: "DATA".to_string(),
                reason: format!("expected 8 legacy bytes, found {}", bytes.len()),
            });
        }
        return Ok(LegacyQuestGeneral {
            flags: bytes[0],
            priority: bytes[1],
            delay_time: f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        });
    }
    let object = value
        .as_object()
        .ok_or_else(|| QuestLoweringError::MalformedField {
            signature: "DATA".to_string(),
            reason: "expected legacy bytes or General object".to_string(),
        })?;
    Ok(LegacyQuestGeneral {
        flags: object
            .get("Flags")
            .and_then(unsigned_value)
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(0),
        priority: object
            .get("Priority")
            .and_then(unsigned_value)
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(0),
        delay_time: object
            .get("QuestDelay")
            .or_else(|| object.get("Quest Delay"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
    })
}

pub(crate) fn field_parts(field: &Value) -> Result<(&str, &Value), QuestLoweringError> {
    let object = field
        .as_object()
        .ok_or_else(|| QuestLoweringError::MalformedField {
            signature: "<field>".to_string(),
            reason: "field is not an object".to_string(),
        })?;
    if object.len() != 1 {
        return Err(QuestLoweringError::MalformedField {
            signature: "<field>".to_string(),
            reason: "field must contain exactly one signature".to_string(),
        });
    }
    Ok(object
        .iter()
        .next()
        .map(|(key, value)| (key.as_str(), value))
        .unwrap())
}

pub(crate) fn raw_hex(value: &Value) -> Option<&str> {
    value
        .as_object()?
        .get("hex")
        .and_then(Value::as_str)
        .or_else(|| value.as_object()?.get("raw_hex").and_then(Value::as_str))
}

pub(crate) fn raw_bytes(value: &Value) -> Option<Vec<u8>> {
    hex::decode(raw_hex(value)?).ok()
}

fn integer_value(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.get("StageIndex").and_then(integer_value))
        .or_else(|| value.get("ObjectiveIndex").and_then(integer_value))
}

fn unsigned_value(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
}

pub(crate) fn form_key_text(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }
    if let Some(target) = value.get("Target") {
        return form_key_text(target);
    }
    let reference = value.get("reference").unwrap_or(value);
    let plugin = reference.get("plugin")?.as_str()?;
    let object_id = reference.get("object_id")?.as_str()?;
    Some(format!("{}:{}", object_id.to_uppercase(), plugin))
}
