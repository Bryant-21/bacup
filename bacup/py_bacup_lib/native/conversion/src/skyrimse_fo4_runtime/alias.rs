use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{FormKey, SubrecordSig};
use crate::quest_runtime::{
    AliasFillKind, ConditionIntent, QuestAliasIntent, QuestRecordKey, QuestRuntimeAdmission,
    QuestRuntimeComponentPlan, QuestSourceGame,
};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const SHARED_ALIAS_FLAGS: u32 = 0x0002_FFFF;
const FO4_EXTERNAL_ALIAS_LINKED: u32 = 0x0010_0000;
const FORCED_REFERENCE_TARGETS: &[&str] = &[
    "ACHR", "PARW", "PBAR", "PCON", "PFLA", "PGRE", "PHZD", "PLYR", "PMIS", "REFR",
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SkyrimAliasPropertyEvidence {
    pub name: String,
    pub property_type: String,
    pub source_value: String,
    pub required: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SkyrimAliasVmadEvidence {
    pub source_vmad_present: bool,
    pub source_script_class: Option<String>,
    pub properties: BTreeSet<SkyrimAliasPropertyEvidence>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimSourceAliasEvidence {
    pub owner: QuestRecordKey,
    pub alias_id: u32,
    pub name: String,
    pub flags: u32,
    pub fill: AliasFillKind,
    pub conditions: Vec<ConditionIntent>,
    pub vmad: SkyrimAliasVmadEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SkyrimExternalAliasIdentity {
    pub owner: QuestRecordKey,
    pub alias_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum SkyrimAliasFieldIntent {
    ReferenceAliasStart(u32),
    LocationAliasStart(u32),
    AliasName(String),
    Flags(u32),
    ForcedReference(QuestRecordKey),
    UniqueActor(QuestRecordKey),
    SpecificLocation(QuestRecordKey),
    ExternalQuest(QuestRecordKey),
    ExternalAlias(i32),
    Condition(ConditionIntent),
    AliasEnd,
}

impl SkyrimAliasFieldIntent {
    pub const fn signature(&self) -> &'static str {
        match self {
            Self::ReferenceAliasStart(_) => "ALST",
            Self::LocationAliasStart(_) => "ALLS",
            Self::AliasName(_) => "ALID",
            Self::Flags(_) => "FNAM",
            Self::ForcedReference(_) => "ALFR",
            Self::UniqueActor(_) => "ALUA",
            Self::SpecificLocation(_) => "ALFL",
            Self::ExternalQuest(_) => "ALEQ",
            Self::ExternalAlias(_) => "ALEA",
            Self::Condition(_) => "CTDA",
            Self::AliasEnd => "ALED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimAliasFillReceiptKind {
    ForcedReference,
    UniqueActor,
    Location,
    ExternalAlias,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SkyrimAliasDependencyReceipt {
    pub source: QuestRecordKey,
    pub target: QuestRecordKey,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SkyrimAliasReceiptRow {
    pub source_owner: QuestRecordKey,
    pub target_owner: QuestRecordKey,
    pub alias_id: u32,
    pub name: String,
    pub source_flags: u32,
    pub target_flags: u32,
    pub fill_kind: SkyrimAliasFillReceiptKind,
    pub field_signatures: Vec<String>,
    pub condition_count: usize,
    pub source_vmad_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimAliasComponentReceipt {
    pub component_id: String,
    pub source_owner: QuestRecordKey,
    pub target_owner: QuestRecordKey,
    pub next_alias_id: u32,
    pub aliases: Vec<SkyrimAliasReceiptRow>,
    pub dependencies: BTreeSet<SkyrimAliasDependencyReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimProjectedAlias {
    pub alias_id: u32,
    pub name: String,
    pub fill_kind: SkyrimAliasFillReceiptKind,
    pub fields: Vec<SkyrimAliasFieldIntent>,
    pub dependencies: BTreeSet<SkyrimAliasDependencyReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimAliasProjection {
    pub component_id: String,
    pub source_owner: QuestRecordKey,
    pub target_owner: QuestRecordKey,
    pub next_alias_id: u32,
    pub aliases: Vec<SkyrimProjectedAlias>,
    pub receipt: SkyrimAliasComponentReceipt,
}

#[derive(Debug, Clone)]
pub struct SkyrimAliasMaterialization {
    pub next_alias: FieldEntry,
    pub alias_fields: Vec<FieldEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimAliasApplyDisposition {
    Inserted,
    AlreadyExact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimAliasRecordReceiptRow {
    pub alias_id: u32,
    pub name: String,
    pub flags: u32,
    pub fill_kind: SkyrimAliasFillReceiptKind,
    pub field_signatures: Vec<String>,
    pub condition_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimAliasRecordReceipt {
    pub component_id: String,
    pub target_owner: QuestRecordKey,
    pub next_alias_id: u32,
    pub aliases: Vec<SkyrimAliasRecordReceiptRow>,
    pub dependencies: BTreeSet<SkyrimAliasDependencyReceipt>,
    pub preserved_quest_vmad_count: usize,
    pub alias_vmad_attachment_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimAliasApplication {
    pub disposition: SkyrimAliasApplyDisposition,
    pub receipt: SkyrimAliasRecordReceipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimAliasRejectionCode {
    WrongSourceGame,
    ComponentNotAdmitted,
    InvalidComponent,
    DuplicateOwnership,
    DuplicateAliasId,
    DuplicateAliasName,
    AliasEvidenceMismatch,
    InvalidAliasIdentity,
    UnsupportedFlags,
    UnsupportedFill,
    UnsupportedCondition,
    MissingMapping,
    AmbiguousMapping,
    WrongTargetSignature,
    MissingExternalAlias,
    DuplicateExternalAlias,
    RequiredAliasScriptUnsupported,
    AliasVmadUnsupported,
    InvalidAliasVmadEvidence,
    AliasIdOverflow,
    WrongTargetRecord,
    ExistingAliasConflict,
    MalformedExistingAlias,
    FieldEncoding,
    ReceiptMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimAliasProjectionError {
    pub code: SkyrimAliasRejectionCode,
    pub alias_id: Option<u32>,
    pub detail: String,
}

impl std::fmt::Display for SkyrimAliasProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.detail)
    }
}

impl std::error::Error for SkyrimAliasProjectionError {}

pub fn project_quest_aliases(
    plan: &QuestRuntimeComponentPlan,
    source_evidence: &[SkyrimSourceAliasEvidence],
    external_alias_inventory: &[SkyrimExternalAliasIdentity],
) -> Result<SkyrimAliasProjection, SkyrimAliasProjectionError> {
    validate_component(plan)?;
    let target_owner = map_record(plan, &plan.root_quest, &["QUST"], None)?;
    let evidence_by_id = validate_alias_evidence(plan, source_evidence)?;
    let external_aliases = validate_external_inventory(external_alias_inventory)?;

    let mut aliases = Vec::with_capacity(plan.semantics.aliases.len());
    for alias in &plan.semantics.aliases {
        let evidence = evidence_by_id
            .get(&alias.id)
            .expect("evidence set validated");
        aliases.push(project_alias(plan, alias, evidence, &external_aliases)?);
    }
    aliases.sort_by_key(|alias| alias.alias_id);
    let next_alias_id = aliases.last().map_or(Ok(0), |alias| {
        alias.alias_id.checked_add(1).ok_or_else(|| {
            rejection(
                SkyrimAliasRejectionCode::AliasIdOverflow,
                Some(alias.alias_id),
                "Skyrim quest alias ID cannot be represented by target ANAM",
            )
        })
    })?;
    let dependencies = aliases
        .iter()
        .flat_map(|alias| alias.dependencies.iter().cloned())
        .collect::<BTreeSet<_>>();
    let receipt = build_receipt(
        plan,
        &target_owner,
        next_alias_id,
        source_evidence,
        &aliases,
        dependencies,
    );
    let projection = SkyrimAliasProjection {
        component_id: plan.component_id.clone(),
        source_owner: plan.root_quest.clone(),
        target_owner,
        next_alias_id,
        aliases,
        receipt,
    };
    validate_alias_projection(&projection)?;
    Ok(projection)
}

pub fn validate_alias_projection(
    projection: &SkyrimAliasProjection,
) -> Result<(), SkyrimAliasProjectionError> {
    if projection.component_id != projection.receipt.component_id
        || projection.source_owner != projection.receipt.source_owner
        || projection.target_owner != projection.receipt.target_owner
        || projection.next_alias_id != projection.receipt.next_alias_id
        || projection.aliases.len() != projection.receipt.aliases.len()
    {
        return Err(receipt_mismatch(
            "Skyrim alias receipt header is inconsistent",
        ));
    }
    let mut alias_ids = BTreeSet::new();
    let mut dependency_union = BTreeSet::new();
    for (alias, receipt) in projection.aliases.iter().zip(&projection.receipt.aliases) {
        if !alias_ids.insert(alias.alias_id)
            || alias.alias_id != receipt.alias_id
            || alias.name != receipt.name
            || alias.fill_kind != receipt.fill_kind
            || alias
                .fields
                .iter()
                .map(|field| field.signature().to_string())
                .collect::<Vec<_>>()
                != receipt.field_signatures
            || alias
                .fields
                .iter()
                .filter(|field| matches!(field, SkyrimAliasFieldIntent::Condition(_)))
                .count()
                != receipt.condition_count
            || receipt.source_vmad_present
        {
            return Err(receipt_mismatch("Skyrim alias receipt row is inconsistent"));
        }
        dependency_union.extend(alias.dependencies.iter().cloned());
    }
    if dependency_union != projection.receipt.dependencies {
        return Err(receipt_mismatch(
            "Skyrim alias dependency receipt is inconsistent",
        ));
    }
    let expected_next = projection.aliases.last().map_or(Ok(0), |alias| {
        alias
            .alias_id
            .checked_add(1)
            .ok_or_else(|| receipt_mismatch("Skyrim alias receipt overflows ANAM"))
    })?;
    if projection.next_alias_id != expected_next {
        return Err(receipt_mismatch(
            "Skyrim alias ANAM receipt is inconsistent",
        ));
    }
    Ok(())
}

pub fn materialize_alias_projection(
    projection: &SkyrimAliasProjection,
    interner: &StringInterner,
) -> Result<SkyrimAliasMaterialization, SkyrimAliasProjectionError> {
    validate_alias_projection(projection)?;
    let next_alias = field("ANAM", FieldValue::Uint(projection.next_alias_id.into()))?;
    let mut alias_fields = Vec::new();
    for alias in &projection.aliases {
        for intent in &alias.fields {
            alias_fields.push(materialize_field(intent, interner)?);
        }
    }
    if alias_fields
        .iter()
        .any(|entry| entry.sig.as_str() == "VMAD")
    {
        return Err(rejection(
            SkyrimAliasRejectionCode::AliasVmadUnsupported,
            None,
            "Skyrim alias materialization attempted to cross the alias VMAD boundary",
        ));
    }
    Ok(SkyrimAliasMaterialization {
        next_alias,
        alias_fields,
    })
}

pub fn apply_alias_projection_to_qust(
    record: &mut Record,
    projection: &SkyrimAliasProjection,
    interner: &StringInterner,
) -> Result<SkyrimAliasApplication, SkyrimAliasProjectionError> {
    validate_target_record(record, projection, interner)?;
    let materialized = materialize_alias_projection(projection, interner)?;
    let before_vmad = count_signature(record, "VMAD");
    let mut candidate = record.clone();
    let alias_starts = candidate
        .fields
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches!(entry.sig.as_str(), "ALST" | "ALLS"))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let alias_ends = candidate
        .fields
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.sig.as_str() == "ALED")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let anam = candidate
        .fields
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.sig.as_str() == "ANAM")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if anam.len() > 1 || alias_starts.len() != alias_ends.len() {
        return Err(rejection(
            SkyrimAliasRejectionCode::MalformedExistingAlias,
            None,
            "target QUST has malformed ANAM or alias block boundaries",
        ));
    }

    let disposition = if alias_starts.is_empty() {
        if !alias_ends.is_empty() {
            return Err(rejection(
                SkyrimAliasRejectionCode::MalformedExistingAlias,
                None,
                "target QUST has an ALED without an alias start",
            ));
        }
        if let Some(index) = anam.first().copied() {
            if index + 1 != candidate.fields.len() {
                return Err(rejection(
                    SkyrimAliasRejectionCode::ExistingAliasConflict,
                    None,
                    "target QUST ANAM is not the final field before alias insertion",
                ));
            }
            candidate.fields[index] = materialized.next_alias.clone();
        } else {
            candidate.fields.push(materialized.next_alias.clone());
        }
        candidate
            .fields
            .extend(materialized.alias_fields.iter().cloned());
        SkyrimAliasApplyDisposition::Inserted
    } else {
        let Some(first_alias) = alias_starts.first().copied() else {
            unreachable!();
        };
        if anam.len() != 1
            || anam[0] + 1 != first_alias
            || alias_ends.last().copied() != Some(candidate.fields.len() - 1)
            || candidate.fields[first_alias..] != materialized.alias_fields
            || candidate.fields[anam[0]] != materialized.next_alias
        {
            return Err(rejection(
                SkyrimAliasRejectionCode::ExistingAliasConflict,
                None,
                "target QUST already contains a different or partial alias block",
            ));
        }
        SkyrimAliasApplyDisposition::AlreadyExact
    };

    if count_signature(&candidate, "VMAD") != before_vmad {
        return Err(rejection(
            SkyrimAliasRejectionCode::AliasVmadUnsupported,
            None,
            "Skyrim alias application changed the QUST VMAD boundary",
        ));
    }
    let receipt = verify_alias_projection_in_record(&candidate, projection, interner)?;
    *record = candidate;
    Ok(SkyrimAliasApplication {
        disposition,
        receipt,
    })
}

pub fn verify_alias_projection_in_record(
    record: &Record,
    projection: &SkyrimAliasProjection,
    interner: &StringInterner,
) -> Result<SkyrimAliasRecordReceipt, SkyrimAliasProjectionError> {
    validate_target_record(record, projection, interner)?;
    let materialized = materialize_alias_projection(projection, interner)?;
    let anam = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.sig.as_str() == "ANAM")
        .collect::<Vec<_>>();
    if anam.len() != 1 || anam[0].1 != &materialized.next_alias {
        return Err(receipt_mismatch(
            "target QUST ANAM does not match the alias projection",
        ));
    }
    let Some(first_alias) = record
        .fields
        .iter()
        .position(|entry| matches!(entry.sig.as_str(), "ALST" | "ALLS"))
    else {
        return Err(receipt_mismatch(
            "target QUST is missing its projected alias block",
        ));
    };
    if anam[0].0 + 1 != first_alias || record.fields[first_alias..] != materialized.alias_fields {
        return Err(receipt_mismatch(
            "target QUST alias fields differ from the strict materialization",
        ));
    }

    let mut offset = first_alias;
    let mut rows = Vec::with_capacity(projection.aliases.len());
    let mut dependencies = BTreeSet::new();
    for projected in &projection.aliases {
        let length = projected.fields.len();
        let actual = &record.fields[offset..offset + length];
        let alias_id = scalar_u32(&actual[0].value).ok_or_else(|| {
            receipt_mismatch("target QUST alias start does not contain an alias ID")
        })?;
        let name = actual
            .iter()
            .find(|entry| entry.sig.as_str() == "ALID")
            .and_then(|entry| match entry.value {
                FieldValue::String(value) => interner.resolve(value),
                _ => None,
            })
            .ok_or_else(|| receipt_mismatch("target QUST alias name is malformed"))?
            .to_string();
        let flags = actual
            .iter()
            .find(|entry| entry.sig.as_str() == "FNAM")
            .and_then(|entry| scalar_u32(&entry.value))
            .ok_or_else(|| receipt_mismatch("target QUST alias flags are malformed"))?;
        collect_record_dependencies(actual, projection, interner, &mut dependencies)?;
        rows.push(SkyrimAliasRecordReceiptRow {
            alias_id,
            name,
            flags,
            fill_kind: projected.fill_kind,
            field_signatures: actual
                .iter()
                .map(|entry| entry.sig.as_str().to_string())
                .collect(),
            condition_count: actual
                .iter()
                .filter(|entry| entry.sig.as_str() == "CTDA")
                .count(),
        });
        offset += length;
    }
    if offset != record.fields.len()
        || rows.len() != projection.receipt.aliases.len()
        || dependencies != projection.receipt.dependencies
        || rows
            .iter()
            .zip(&projection.receipt.aliases)
            .any(|(actual, expected)| {
                actual.alias_id != expected.alias_id
                    || actual.name != expected.name
                    || actual.flags != expected.target_flags
                    || actual.fill_kind != expected.fill_kind
                    || actual.field_signatures != expected.field_signatures
                    || actual.condition_count != expected.condition_count
            })
    {
        return Err(receipt_mismatch(
            "target QUST alias receipt does not match the admitted component",
        ));
    }
    Ok(SkyrimAliasRecordReceipt {
        component_id: projection.component_id.clone(),
        target_owner: projection.target_owner.clone(),
        next_alias_id: projection.next_alias_id,
        aliases: rows,
        dependencies,
        preserved_quest_vmad_count: count_signature(record, "VMAD"),
        alias_vmad_attachment_count: 0,
    })
}

fn materialize_field(
    intent: &SkyrimAliasFieldIntent,
    interner: &StringInterner,
) -> Result<FieldEntry, SkyrimAliasProjectionError> {
    match intent {
        SkyrimAliasFieldIntent::ReferenceAliasStart(id) => {
            field("ALST", FieldValue::Uint((*id).into()))
        }
        SkyrimAliasFieldIntent::LocationAliasStart(id) => {
            field("ALLS", FieldValue::Uint((*id).into()))
        }
        SkyrimAliasFieldIntent::AliasName(name) => {
            field("ALID", FieldValue::String(interner.intern(name)))
        }
        SkyrimAliasFieldIntent::Flags(flags) => field("FNAM", FieldValue::Uint((*flags).into())),
        SkyrimAliasFieldIntent::ForcedReference(target) => field(
            "ALFR",
            FieldValue::FormKey(parse_quest_record_key(target, interner)?),
        ),
        SkyrimAliasFieldIntent::UniqueActor(target) => field(
            "ALUA",
            FieldValue::FormKey(parse_quest_record_key(target, interner)?),
        ),
        SkyrimAliasFieldIntent::SpecificLocation(target) => field(
            "ALFL",
            FieldValue::FormKey(parse_quest_record_key(target, interner)?),
        ),
        SkyrimAliasFieldIntent::ExternalQuest(target) => field(
            "ALEQ",
            FieldValue::FormKey(parse_quest_record_key(target, interner)?),
        ),
        SkyrimAliasFieldIntent::ExternalAlias(id) => field("ALEA", FieldValue::Int((*id).into())),
        SkyrimAliasFieldIntent::Condition(condition) => materialize_condition(condition, interner),
        SkyrimAliasFieldIntent::AliasEnd => field("ALED", FieldValue::None),
    }
}

fn materialize_condition(
    condition: &ConditionIntent,
    interner: &StringInterner,
) -> Result<FieldEntry, SkyrimAliasProjectionError> {
    let function = match condition.function.as_str() {
        "GetIsID" => 72_u64,
        "GetStageDone" => 59_u64,
        _ => {
            return Err(rejection(
                SkyrimAliasRejectionCode::UnsupportedCondition,
                None,
                "projected Skyrim alias condition is not materializable",
            ));
        }
    };
    if condition.parameters.is_empty() {
        return Err(rejection(
            SkyrimAliasRejectionCode::UnsupportedCondition,
            None,
            "projected Skyrim alias condition has no target parameter",
        ));
    }
    let target = parse_form_key_text(&condition.parameters[0], interner)?;
    let parameter_2 = condition
        .parameters
        .get(1)
        .map(|value| value.parse::<u16>())
        .transpose()
        .map_err(|_| {
            rejection(
                SkyrimAliasRejectionCode::UnsupportedCondition,
                None,
                "projected Skyrim alias condition stage is invalid",
            )
        })?
        .unwrap_or(0);
    field(
        "CTDA",
        FieldValue::Struct(vec![
            (interner.intern("type"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_1"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
            (interner.intern("comparison_value"), FieldValue::Float(1.0)),
            (interner.intern("function"), FieldValue::Uint(function)),
            (interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_7"), FieldValue::Uint(0)),
            (interner.intern("parameter_1"), FieldValue::FormKey(target)),
            (
                interner.intern("parameter_2"),
                FieldValue::Uint(parameter_2.into()),
            ),
            (interner.intern("run_on"), FieldValue::Uint(0)),
            (interner.intern("reference"), FieldValue::Uint(0)),
            (interner.intern("parameter_3"), FieldValue::Int(-1)),
        ]),
    )
}

fn validate_target_record(
    record: &Record,
    projection: &SkyrimAliasProjection,
    interner: &StringInterner,
) -> Result<(), SkyrimAliasProjectionError> {
    if record.sig.as_str() != "QUST"
        || record.form_key != parse_quest_record_key(&projection.target_owner, interner)?
    {
        return Err(rejection(
            SkyrimAliasRejectionCode::WrongTargetRecord,
            None,
            "Skyrim alias projection target is not the mapped Fallout 4 QUST",
        ));
    }
    Ok(())
}

fn collect_record_dependencies(
    fields: &[FieldEntry],
    projection: &SkyrimAliasProjection,
    interner: &StringInterner,
    dependencies: &mut BTreeSet<SkyrimAliasDependencyReceipt>,
) -> Result<(), SkyrimAliasProjectionError> {
    for entry in fields {
        let (field_name, form_key) = match (entry.sig.as_str(), &entry.value) {
            ("ALFR" | "ALUA" | "ALFL" | "ALEQ", FieldValue::FormKey(form_key)) => {
                (entry.sig.as_str(), *form_key)
            }
            ("CTDA", FieldValue::Struct(values)) => {
                let form_key = values
                    .iter()
                    .find(|(name, _)| interner.resolve(*name) == Some("parameter_1"))
                    .and_then(|(_, value)| match value {
                        FieldValue::FormKey(form_key) => Some(*form_key),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        receipt_mismatch("target QUST alias CTDA parameter_1 is malformed")
                    })?;
                ("CTDA.parameter_1", form_key)
            }
            _ => continue,
        };
        let matching = projection
            .receipt
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.field == field_name
                    && parse_quest_record_key(&dependency.target, interner) == Ok(form_key)
            })
            .cloned()
            .collect::<Vec<_>>();
        match matching.as_slice() {
            [dependency] => {
                dependencies.insert(dependency.clone());
            }
            _ => {
                return Err(receipt_mismatch(
                    "target QUST alias dependency does not match one receipt row",
                ));
            }
        }
    }
    Ok(())
}

fn parse_quest_record_key(
    key: &QuestRecordKey,
    interner: &StringInterner,
) -> Result<FormKey, SkyrimAliasProjectionError> {
    parse_form_key_text(&key.form_key, interner)
}

fn parse_form_key_text(
    value: &str,
    interner: &StringInterner,
) -> Result<FormKey, SkyrimAliasProjectionError> {
    let normalized = if value.contains('@') {
        value.to_string()
    } else {
        let (local, plugin) = value.split_once(':').ok_or_else(|| {
            rejection(
                SkyrimAliasRejectionCode::FieldEncoding,
                None,
                format!("invalid Skyrim alias target FormKey {value}"),
            )
        })?;
        format!("{local}@{plugin}")
    };
    FormKey::parse(&normalized, interner)
        .map_err(|error| rejection(SkyrimAliasRejectionCode::FieldEncoding, None, error))
}

fn field(signature: &str, value: FieldValue) -> Result<FieldEntry, SkyrimAliasProjectionError> {
    Ok(FieldEntry {
        sig: SubrecordSig::from_str(signature)
            .map_err(|error| rejection(SkyrimAliasRejectionCode::FieldEncoding, None, error))?,
        value,
    })
}

fn scalar_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Struct(values) => values.first().and_then(|(_, value)| scalar_u32(value)),
        _ => None,
    }
}

fn count_signature(record: &Record, signature: &str) -> usize {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == signature)
        .count()
}

fn validate_component(plan: &QuestRuntimeComponentPlan) -> Result<(), SkyrimAliasProjectionError> {
    if plan.provenance.game != QuestSourceGame::SkyrimSe {
        return Err(rejection(
            SkyrimAliasRejectionCode::WrongSourceGame,
            None,
            "Skyrim alias projector received another source game",
        ));
    }
    if !matches!(plan.admission, QuestRuntimeAdmission::Supported) {
        return Err(rejection(
            SkyrimAliasRejectionCode::ComponentNotAdmitted,
            None,
            "Skyrim alias component is not admitted",
        ));
    }
    if !plan.validation_issues().is_empty() {
        return Err(rejection(
            SkyrimAliasRejectionCode::InvalidComponent,
            None,
            "Skyrim alias component contract is invalid",
        ));
    }
    if plan
        .owned_records
        .iter()
        .filter(|record| **record == plan.root_quest)
        .count()
        != 1
    {
        return Err(rejection(
            SkyrimAliasRejectionCode::DuplicateOwnership,
            None,
            "Skyrim alias owner must be owned exactly once",
        ));
    }
    Ok(())
}

fn validate_alias_evidence<'a>(
    plan: &QuestRuntimeComponentPlan,
    evidence: &'a [SkyrimSourceAliasEvidence],
) -> Result<BTreeMap<u32, &'a SkyrimSourceAliasEvidence>, SkyrimAliasProjectionError> {
    let mut plan_by_id = BTreeMap::new();
    let mut plan_names = BTreeSet::new();
    for alias in &plan.semantics.aliases {
        if plan_by_id.insert(alias.id, alias).is_some() {
            return Err(rejection(
                SkyrimAliasRejectionCode::DuplicateAliasId,
                Some(alias.id),
                "Skyrim component declares a duplicate alias ID",
            ));
        }
        if alias.name.trim().is_empty() {
            return Err(rejection(
                SkyrimAliasRejectionCode::InvalidAliasIdentity,
                Some(alias.id),
                "Skyrim component alias name is empty",
            ));
        }
        if !plan_names.insert(alias.name.to_ascii_lowercase()) {
            return Err(rejection(
                SkyrimAliasRejectionCode::DuplicateAliasName,
                Some(alias.id),
                "Skyrim component declares a duplicate alias name",
            ));
        }
    }

    let mut evidence_by_id = BTreeMap::new();
    let mut evidence_names = BTreeSet::new();
    for row in evidence {
        if row.owner != plan.root_quest {
            return Err(rejection(
                SkyrimAliasRejectionCode::DuplicateOwnership,
                Some(row.alias_id),
                "Skyrim alias evidence names a different owning quest",
            ));
        }
        if evidence_by_id.insert(row.alias_id, row).is_some() {
            return Err(rejection(
                SkyrimAliasRejectionCode::DuplicateAliasId,
                Some(row.alias_id),
                "Skyrim source evidence contains a duplicate alias ID",
            ));
        }
        if !evidence_names.insert(row.name.to_ascii_lowercase()) {
            return Err(rejection(
                SkyrimAliasRejectionCode::DuplicateAliasName,
                Some(row.alias_id),
                "Skyrim source evidence contains a duplicate alias name",
            ));
        }
        validate_vmad_evidence(row.alias_id, &row.vmad)?;
        let Some(alias) = plan_by_id.get(&row.alias_id) else {
            return Err(rejection(
                SkyrimAliasRejectionCode::AliasEvidenceMismatch,
                Some(row.alias_id),
                "Skyrim source alias evidence is not declared by the component",
            ));
        };
        if alias.name != row.name || alias.fill != row.fill || alias.conditions != row.conditions {
            return Err(rejection(
                SkyrimAliasRejectionCode::AliasEvidenceMismatch,
                Some(row.alias_id),
                "Skyrim source alias evidence differs from component semantics",
            ));
        }
    }
    if evidence_by_id.len() != plan_by_id.len() {
        return Err(rejection(
            SkyrimAliasRejectionCode::AliasEvidenceMismatch,
            None,
            "Skyrim component does not have exact source evidence for every alias",
        ));
    }
    Ok(evidence_by_id)
}

fn validate_vmad_evidence(
    alias_id: u32,
    evidence: &SkyrimAliasVmadEvidence,
) -> Result<(), SkyrimAliasProjectionError> {
    let has_script = evidence
        .source_script_class
        .as_ref()
        .is_some_and(|class| !class.trim().is_empty());
    if !evidence.source_vmad_present {
        if evidence.required || has_script || !evidence.properties.is_empty() {
            return Err(rejection(
                SkyrimAliasRejectionCode::InvalidAliasVmadEvidence,
                Some(alias_id),
                "Skyrim alias VMAD evidence is internally inconsistent",
            ));
        }
        return Ok(());
    }
    if evidence.required || evidence.properties.iter().any(|property| property.required) {
        return Err(rejection(
            SkyrimAliasRejectionCode::RequiredAliasScriptUnsupported,
            Some(alias_id),
            "required Skyrim alias script/property semantics cannot be attached by the current alias model",
        ));
    }
    Err(rejection(
        SkyrimAliasRejectionCode::AliasVmadUnsupported,
        Some(alias_id),
        "Skyrim alias VMAD is present but the current model has no alias-ID-specific target attachment intent",
    ))
}

fn validate_external_inventory(
    inventory: &[SkyrimExternalAliasIdentity],
) -> Result<BTreeSet<SkyrimExternalAliasIdentity>, SkyrimAliasProjectionError> {
    let mut unique = BTreeSet::new();
    for identity in inventory {
        if identity.owner.signature != "QUST" {
            return Err(rejection(
                SkyrimAliasRejectionCode::MissingExternalAlias,
                Some(identity.alias_id),
                "external Skyrim alias inventory owner is not QUST",
            ));
        }
        if !unique.insert(identity.clone()) {
            return Err(rejection(
                SkyrimAliasRejectionCode::DuplicateExternalAlias,
                Some(identity.alias_id),
                "external Skyrim alias inventory contains a duplicate identity",
            ));
        }
    }
    Ok(unique)
}

fn project_alias(
    plan: &QuestRuntimeComponentPlan,
    alias: &QuestAliasIntent,
    evidence: &SkyrimSourceAliasEvidence,
    external_aliases: &BTreeSet<SkyrimExternalAliasIdentity>,
) -> Result<SkyrimProjectedAlias, SkyrimAliasProjectionError> {
    if evidence.flags & !SHARED_ALIAS_FLAGS != 0 {
        return Err(rejection(
            SkyrimAliasRejectionCode::UnsupportedFlags,
            Some(alias.id),
            "Skyrim alias has flags whose target semantics are not shared with Fallout 4",
        ));
    }
    let mut dependencies = BTreeSet::new();
    let (fill_kind, mut fields, target_flags) = match &alias.fill {
        AliasFillKind::ForcedReference(source) => {
            let dependency = mapped_dependency(
                plan,
                source,
                FORCED_REFERENCE_TARGETS,
                "ALFR",
                Some(alias.id),
            )?;
            dependencies.insert(dependency.clone());
            (
                SkyrimAliasFillReceiptKind::ForcedReference,
                vec![
                    SkyrimAliasFieldIntent::ReferenceAliasStart(alias.id),
                    SkyrimAliasFieldIntent::AliasName(alias.name.clone()),
                    SkyrimAliasFieldIntent::Flags(evidence.flags),
                    SkyrimAliasFieldIntent::ForcedReference(dependency.target),
                ],
                evidence.flags,
            )
        }
        AliasFillKind::UniqueActor(source) => {
            let dependency = mapped_dependency(plan, source, &["NPC_"], "ALUA", Some(alias.id))?;
            dependencies.insert(dependency.clone());
            (
                SkyrimAliasFillReceiptKind::UniqueActor,
                vec![
                    SkyrimAliasFieldIntent::ReferenceAliasStart(alias.id),
                    SkyrimAliasFieldIntent::AliasName(alias.name.clone()),
                    SkyrimAliasFieldIntent::Flags(evidence.flags),
                    SkyrimAliasFieldIntent::UniqueActor(dependency.target),
                ],
                evidence.flags,
            )
        }
        AliasFillKind::Location(source) => {
            let dependency = mapped_dependency(plan, source, &["LCTN"], "ALFL", Some(alias.id))?;
            dependencies.insert(dependency.clone());
            (
                SkyrimAliasFillReceiptKind::Location,
                vec![
                    SkyrimAliasFieldIntent::LocationAliasStart(alias.id),
                    SkyrimAliasFieldIntent::AliasName(alias.name.clone()),
                    SkyrimAliasFieldIntent::Flags(evidence.flags),
                    SkyrimAliasFieldIntent::SpecificLocation(dependency.target),
                ],
                evidence.flags,
            )
        }
        AliasFillKind::ExternalAlias {
            quest,
            alias_id: external_alias_id,
        } => {
            let source_quest = source_record_for_form_key(plan, quest, Some(alias.id))?;
            if source_quest.signature != "QUST"
                || !external_aliases.contains(&SkyrimExternalAliasIdentity {
                    owner: source_quest.clone(),
                    alias_id: *external_alias_id,
                })
            {
                return Err(rejection(
                    SkyrimAliasRejectionCode::MissingExternalAlias,
                    Some(alias.id),
                    "Skyrim external alias does not resolve to one inventoried source alias",
                ));
            }
            let target_quest = map_record(plan, &source_quest, &["QUST"], Some(alias.id))?;
            let dependency = SkyrimAliasDependencyReceipt {
                source: source_quest,
                target: target_quest.clone(),
                field: "ALEQ".to_string(),
            };
            dependencies.insert(dependency);
            let external_alias_id = i32::try_from(*external_alias_id).map_err(|_| {
                rejection(
                    SkyrimAliasRejectionCode::UnsupportedFill,
                    Some(alias.id),
                    "Skyrim external alias ID cannot be represented by target ALEA",
                )
            })?;
            let target_flags = evidence.flags | FO4_EXTERNAL_ALIAS_LINKED;
            (
                SkyrimAliasFillReceiptKind::ExternalAlias,
                vec![
                    SkyrimAliasFieldIntent::ReferenceAliasStart(alias.id),
                    SkyrimAliasFieldIntent::AliasName(alias.name.clone()),
                    SkyrimAliasFieldIntent::Flags(target_flags),
                    SkyrimAliasFieldIntent::ExternalQuest(target_quest),
                    SkyrimAliasFieldIntent::ExternalAlias(external_alias_id),
                ],
                target_flags,
            )
        }
        AliasFillKind::CreatedReference(_) => {
            return Err(rejection(
                SkyrimAliasRejectionCode::UnsupportedFill,
                Some(alias.id),
                "CreatedReference is not projectable because the shared alias model lacks source ALCA create mode and ALCL level",
            ));
        }
        AliasFillKind::PairSpecific(_) => {
            return Err(rejection(
                SkyrimAliasRejectionCode::UnsupportedFill,
                Some(alias.id),
                "pair-specific Skyrim alias fill has no proven Fallout 4 shape",
            ));
        }
    };
    for condition in &alias.conditions {
        let projected = project_condition(plan, condition, alias.id, &mut dependencies)?;
        fields.push(SkyrimAliasFieldIntent::Condition(projected));
    }
    fields.push(SkyrimAliasFieldIntent::AliasEnd);
    debug_assert_eq!(
        fields.iter().find_map(|field| match field {
            SkyrimAliasFieldIntent::Flags(flags) => Some(*flags),
            _ => None,
        }),
        Some(target_flags)
    );
    Ok(SkyrimProjectedAlias {
        alias_id: alias.id,
        name: alias.name.clone(),
        fill_kind,
        fields,
        dependencies,
    })
}

fn project_condition(
    plan: &QuestRuntimeComponentPlan,
    condition: &ConditionIntent,
    alias_id: u32,
    dependencies: &mut BTreeSet<SkyrimAliasDependencyReceipt>,
) -> Result<ConditionIntent, SkyrimAliasProjectionError> {
    if !matches!(condition.operator.as_str(), "==" | "EqualTo")
        || condition.comparison_value != "1"
        || condition.run_on != "Subject"
        || condition.cis1.is_some()
        || condition.cis2.is_some()
    {
        return Err(rejection(
            SkyrimAliasRejectionCode::UnsupportedCondition,
            Some(alias_id),
            "Skyrim alias condition has an unsupported operator, run-on, comparison, or CIS shape",
        ));
    }
    let (target_signatures, expected_parameters) = match condition.function.as_str() {
        "GetIsID" => (&[][..], 1),
        "GetStageDone" => (&["QUST"][..], 2),
        _ => {
            return Err(rejection(
                SkyrimAliasRejectionCode::UnsupportedCondition,
                Some(alias_id),
                "Skyrim alias condition function has no exact target projection",
            ));
        }
    };
    if condition.parameters.len() != expected_parameters
        || (condition.function == "GetStageDone" && condition.parameters[1].parse::<u16>().is_err())
    {
        return Err(rejection(
            SkyrimAliasRejectionCode::UnsupportedCondition,
            Some(alias_id),
            "Skyrim alias condition parameter shape is unsupported",
        ));
    }
    let source = source_record_for_form_key(plan, &condition.parameters[0], Some(alias_id))?;
    let target = map_record(plan, &source, target_signatures, Some(alias_id))?;
    dependencies.insert(SkyrimAliasDependencyReceipt {
        source,
        target: target.clone(),
        field: "CTDA.parameter_1".to_string(),
    });
    let mut projected = condition.clone();
    projected.parameters[0] = target.form_key;
    Ok(projected)
}

fn mapped_dependency(
    plan: &QuestRuntimeComponentPlan,
    source_form_key: &str,
    target_signatures: &[&str],
    field: &str,
    alias_id: Option<u32>,
) -> Result<SkyrimAliasDependencyReceipt, SkyrimAliasProjectionError> {
    let source = source_record_for_form_key(plan, source_form_key, alias_id)?;
    let target = map_record(plan, &source, target_signatures, alias_id)?;
    Ok(SkyrimAliasDependencyReceipt {
        source,
        target,
        field: field.to_string(),
    })
}

fn source_record_for_form_key(
    plan: &QuestRuntimeComponentPlan,
    source_form_key: &str,
    alias_id: Option<u32>,
) -> Result<QuestRecordKey, SkyrimAliasProjectionError> {
    let matching = plan
        .mappings
        .iter()
        .filter(|mapping| {
            mapping
                .source
                .form_key
                .eq_ignore_ascii_case(source_form_key)
        })
        .map(|mapping| mapping.source.clone())
        .collect::<BTreeSet<_>>();
    match matching.into_iter().collect::<Vec<_>>().as_slice() {
        [source] => Ok(source.clone()),
        [] => Err(rejection(
            SkyrimAliasRejectionCode::MissingMapping,
            alias_id,
            "Skyrim alias dependency has no source-to-target mapping",
        )),
        _ => Err(rejection(
            SkyrimAliasRejectionCode::AmbiguousMapping,
            alias_id,
            "Skyrim alias dependency has ambiguous source identities",
        )),
    }
}

fn map_record(
    plan: &QuestRuntimeComponentPlan,
    source: &QuestRecordKey,
    target_signatures: &[&str],
    alias_id: Option<u32>,
) -> Result<QuestRecordKey, SkyrimAliasProjectionError> {
    let matching = plan
        .mappings
        .iter()
        .filter(|mapping| mapping.source == *source)
        .collect::<Vec<_>>();
    let mapping = match matching.as_slice() {
        [mapping] => *mapping,
        [] => {
            return Err(rejection(
                SkyrimAliasRejectionCode::MissingMapping,
                alias_id,
                "Skyrim alias record has no source-to-target mapping",
            ));
        }
        _ => {
            return Err(rejection(
                SkyrimAliasRejectionCode::AmbiguousMapping,
                alias_id,
                "Skyrim alias record has more than one source-to-target mapping",
            ));
        }
    };
    if !target_signatures.is_empty()
        && !target_signatures.contains(&mapping.target.signature.as_str())
    {
        return Err(rejection(
            SkyrimAliasRejectionCode::WrongTargetSignature,
            alias_id,
            "Skyrim alias mapping targets the wrong Fallout 4 record signature",
        ));
    }
    Ok(mapping.target.clone())
}

fn build_receipt(
    plan: &QuestRuntimeComponentPlan,
    target_owner: &QuestRecordKey,
    next_alias_id: u32,
    evidence: &[SkyrimSourceAliasEvidence],
    aliases: &[SkyrimProjectedAlias],
    dependencies: BTreeSet<SkyrimAliasDependencyReceipt>,
) -> SkyrimAliasComponentReceipt {
    let evidence_by_id = evidence
        .iter()
        .map(|row| (row.alias_id, row))
        .collect::<BTreeMap<_, _>>();
    let rows = aliases
        .iter()
        .map(|alias| {
            let source = evidence_by_id[&alias.alias_id];
            let target_flags = alias
                .fields
                .iter()
                .find_map(|field| match field {
                    SkyrimAliasFieldIntent::Flags(flags) => Some(*flags),
                    _ => None,
                })
                .expect("every alias has target flags");
            SkyrimAliasReceiptRow {
                source_owner: plan.root_quest.clone(),
                target_owner: target_owner.clone(),
                alias_id: alias.alias_id,
                name: alias.name.clone(),
                source_flags: source.flags,
                target_flags,
                fill_kind: alias.fill_kind,
                field_signatures: alias
                    .fields
                    .iter()
                    .map(|field| field.signature().to_string())
                    .collect(),
                condition_count: alias
                    .fields
                    .iter()
                    .filter(|field| matches!(field, SkyrimAliasFieldIntent::Condition(_)))
                    .count(),
                source_vmad_present: source.vmad.source_vmad_present,
            }
        })
        .collect();
    SkyrimAliasComponentReceipt {
        component_id: plan.component_id.clone(),
        source_owner: plan.root_quest.clone(),
        target_owner: target_owner.clone(),
        next_alias_id,
        aliases: rows,
        dependencies,
    }
}

fn receipt_mismatch(detail: &str) -> SkyrimAliasProjectionError {
    rejection(SkyrimAliasRejectionCode::ReceiptMismatch, None, detail)
}

fn rejection(
    code: SkyrimAliasRejectionCode,
    alias_id: Option<u32>,
    detail: impl Into<String>,
) -> SkyrimAliasProjectionError {
    SkyrimAliasProjectionError {
        code,
        alias_id,
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quest_runtime::{
        DependencyClosure, QuestRuntimeExpectedReceipt, QuestSemanticPlan, QuestStartFlag,
        SourceProvenance, SourceTargetFormMapping, StartDisposition,
    };

    fn key(signature: &str, local: u32) -> QuestRecordKey {
        QuestRecordKey::new(signature, format!("{local:06X}:Skyrim.esm"))
    }

    fn target_key(signature: &str, local: u32) -> QuestRecordKey {
        QuestRecordKey::new(signature, format!("{local:06X}:Skyrim.esp"))
    }

    fn alias(id: u32, name: &str, fill: AliasFillKind) -> QuestAliasIntent {
        QuestAliasIntent {
            id,
            name: name.to_string(),
            fill,
            conditions: Vec::new(),
        }
    }

    fn evidence(
        owner: &QuestRecordKey,
        alias: &QuestAliasIntent,
        flags: u32,
    ) -> SkyrimSourceAliasEvidence {
        SkyrimSourceAliasEvidence {
            owner: owner.clone(),
            alias_id: alias.id,
            name: alias.name.clone(),
            flags,
            fill: alias.fill.clone(),
            conditions: alias.conditions.clone(),
            vmad: SkyrimAliasVmadEvidence::default(),
        }
    }

    fn fixture_plan(aliases: Vec<QuestAliasIntent>) -> QuestRuntimeComponentPlan {
        let root = key("QUST", 0x100);
        let dependencies = [
            key("REFR", 0x200),
            key("NPC_", 0x201),
            key("LCTN", 0x202),
            key("QUST", 0x203),
        ];
        let mut mappings = vec![SourceTargetFormMapping {
            source: root.clone(),
            target: target_key("QUST", 0x1100),
        }];
        mappings.extend(dependencies.iter().enumerate().map(|(index, source)| {
            SourceTargetFormMapping {
                source: source.clone(),
                target: target_key(&source.signature, 0x1200 + index as u32),
            }
        }));
        QuestRuntimeComponentPlan {
            component_id: "skyrim-alias-fixture".to_string(),
            root_quest: root.clone(),
            provenance: SourceProvenance {
                game: QuestSourceGame::SkyrimSe,
                source_plugin: "Skyrim.esm".to_string(),
                graft: None,
            },
            owned_records: vec![root],
            shared_records: BTreeSet::new(),
            dependencies: DependencyClosure {
                direct: dependencies.into_iter().collect(),
                recursive: BTreeSet::new(),
            },
            source_topology: BTreeSet::new(),
            target_topology: BTreeSet::new(),
            mappings,
            semantics: QuestSemanticPlan {
                aliases,
                start_flags: BTreeSet::from([QuestStartFlag::StartGameEnabled]),
                ..QuestSemanticPlan::default()
            },
            scripts: BTreeSet::new(),
            fragments: BTreeSet::new(),
            psc: BTreeSet::new(),
            vmad: BTreeSet::new(),
            assets: BTreeSet::new(),
            inbound_producers: BTreeSet::new(),
            start_disposition: Some(StartDisposition::Autostart),
            admission: QuestRuntimeAdmission::Supported,
            expected_receipt: QuestRuntimeExpectedReceipt::default(),
        }
    }

    fn full_projection() -> SkyrimAliasProjection {
        let forced = alias(
            0,
            "Marker",
            AliasFillKind::ForcedReference(key("REFR", 0x200).form_key),
        );
        let mut unique = alias(
            1,
            "Actor",
            AliasFillKind::UniqueActor(key("NPC_", 0x201).form_key),
        );
        unique.conditions.push(ConditionIntent {
            function: "GetIsID".to_string(),
            operator: "==".to_string(),
            comparison_value: "1".to_string(),
            parameters: vec![key("NPC_", 0x201).form_key],
            run_on: "Subject".to_string(),
            cis1: None,
            cis2: None,
        });
        let location = alias(
            4,
            "Home",
            AliasFillKind::Location(key("LCTN", 0x202).form_key),
        );
        let external = alias(
            8,
            "External",
            AliasFillKind::ExternalAlias {
                quest: key("QUST", 0x203).form_key,
                alias_id: 12,
            },
        );
        let plan = fixture_plan(vec![
            forced.clone(),
            unique.clone(),
            location.clone(),
            external.clone(),
        ]);
        project_quest_aliases(
            &plan,
            &[
                evidence(&plan.root_quest, &forced, 0x02),
                evidence(&plan.root_quest, &unique, 0x10),
                evidence(&plan.root_quest, &location, 0x01),
                evidence(&plan.root_quest, &external, 0x02),
            ],
            &[SkyrimExternalAliasIdentity {
                owner: key("QUST", 0x203),
                alias_id: 12,
            }],
        )
        .unwrap()
    }

    fn target_record(projection: &SkyrimAliasProjection, interner: &StringInterner) -> Record {
        Record::new(
            crate::ids::SigCode::from_str("QUST").unwrap(),
            parse_quest_record_key(&projection.target_owner, interner).unwrap(),
        )
    }

    #[test]
    fn projects_proven_alias_shapes_with_exact_fields_and_receipt() {
        let forced = alias(
            0,
            "Marker",
            AliasFillKind::ForcedReference(key("REFR", 0x200).form_key),
        );
        let mut unique = alias(
            1,
            "Actor",
            AliasFillKind::UniqueActor(key("NPC_", 0x201).form_key),
        );
        unique.conditions.push(ConditionIntent {
            function: "GetIsID".to_string(),
            operator: "==".to_string(),
            comparison_value: "1".to_string(),
            parameters: vec![key("NPC_", 0x201).form_key],
            run_on: "Subject".to_string(),
            cis1: None,
            cis2: None,
        });
        let location = alias(
            4,
            "Home",
            AliasFillKind::Location(key("LCTN", 0x202).form_key),
        );
        let external = alias(
            8,
            "External",
            AliasFillKind::ExternalAlias {
                quest: key("QUST", 0x203).form_key,
                alias_id: 12,
            },
        );
        let plan = fixture_plan(vec![
            forced.clone(),
            unique.clone(),
            location.clone(),
            external.clone(),
        ]);
        let rows = vec![
            evidence(&plan.root_quest, &forced, 0x02),
            evidence(&plan.root_quest, &unique, 0x10),
            evidence(&plan.root_quest, &location, 0x01),
            evidence(&plan.root_quest, &external, 0x02),
        ];
        let projection = project_quest_aliases(
            &plan,
            &rows,
            &[SkyrimExternalAliasIdentity {
                owner: key("QUST", 0x203),
                alias_id: 12,
            }],
        )
        .unwrap();

        assert_eq!(projection.next_alias_id, 9);
        assert_eq!(
            projection.aliases[0]
                .fields
                .iter()
                .map(SkyrimAliasFieldIntent::signature)
                .collect::<Vec<_>>(),
            ["ALST", "ALID", "FNAM", "ALFR", "ALED"]
        );
        assert_eq!(
            projection.aliases[1]
                .fields
                .iter()
                .map(SkyrimAliasFieldIntent::signature)
                .collect::<Vec<_>>(),
            ["ALST", "ALID", "FNAM", "ALUA", "CTDA", "ALED"]
        );
        assert_eq!(
            projection.aliases[2]
                .fields
                .iter()
                .map(SkyrimAliasFieldIntent::signature)
                .collect::<Vec<_>>(),
            ["ALLS", "ALID", "FNAM", "ALFL", "ALED"]
        );
        assert_eq!(
            projection.aliases[3]
                .fields
                .iter()
                .map(SkyrimAliasFieldIntent::signature)
                .collect::<Vec<_>>(),
            ["ALST", "ALID", "FNAM", "ALEQ", "ALEA", "ALED"]
        );
        assert!(matches!(
            projection.aliases[3].fields[2],
            SkyrimAliasFieldIntent::Flags(flags) if flags == 0x0010_0002
        ));
        assert_eq!(projection.receipt.dependencies.len(), 5);
        assert_eq!(validate_alias_projection(&projection), Ok(()));
    }

    #[test]
    fn created_reference_and_required_alias_vmad_fail_closed() {
        let created = alias(
            0,
            "Created",
            AliasFillKind::CreatedReference(key("NPC_", 0x201).form_key),
        );
        let plan = fixture_plan(vec![created.clone()]);
        let error = project_quest_aliases(&plan, &[evidence(&plan.root_quest, &created, 0)], &[])
            .unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::UnsupportedFill);
        assert!(error.detail.contains("ALCA"));

        let forced = alias(
            0,
            "Marker",
            AliasFillKind::ForcedReference(key("REFR", 0x200).form_key),
        );
        let plan = fixture_plan(vec![forced.clone()]);
        let mut row = evidence(&plan.root_quest, &forced, 0);
        row.vmad = SkyrimAliasVmadEvidence {
            source_vmad_present: true,
            source_script_class: Some("AliasScript".to_string()),
            properties: BTreeSet::from([SkyrimAliasPropertyEvidence {
                name: "Target".to_string(),
                property_type: "ObjectReference".to_string(),
                source_value: key("REFR", 0x200).form_key,
                required: true,
            }]),
            required: true,
        };
        let error = project_quest_aliases(&plan, &[row], &[]).unwrap_err();
        assert_eq!(
            error.code,
            SkyrimAliasRejectionCode::RequiredAliasScriptUnsupported
        );
    }

    #[test]
    fn duplicate_aliases_and_wrong_owner_are_rejected() {
        let one = alias(
            0,
            "One",
            AliasFillKind::ForcedReference(key("REFR", 0x200).form_key),
        );
        let duplicate = alias(
            0,
            "Two",
            AliasFillKind::UniqueActor(key("NPC_", 0x201).form_key),
        );
        let plan = fixture_plan(vec![one.clone(), duplicate]);
        let error =
            project_quest_aliases(&plan, &[evidence(&plan.root_quest, &one, 0)], &[]).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::DuplicateAliasId);

        let plan = fixture_plan(vec![one.clone()]);
        let mut row = evidence(&plan.root_quest, &one, 0);
        row.owner = key("QUST", 0x999);
        let error = project_quest_aliases(&plan, &[row], &[]).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::DuplicateOwnership);
    }

    #[test]
    fn external_alias_requires_exact_inventory_and_mapping() {
        let external = alias(
            0,
            "External",
            AliasFillKind::ExternalAlias {
                quest: key("QUST", 0x203).form_key,
                alias_id: 12,
            },
        );
        let plan = fixture_plan(vec![external.clone()]);
        let row = evidence(&plan.root_quest, &external, 0);
        let error = project_quest_aliases(&plan, std::slice::from_ref(&row), &[]).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::MissingExternalAlias);

        let mut wrong_target = plan.clone();
        wrong_target
            .mappings
            .iter_mut()
            .find(|mapping| mapping.source == key("QUST", 0x203))
            .unwrap()
            .target
            .signature = "SCEN".to_string();
        let error = project_quest_aliases(
            &wrong_target,
            &[row],
            &[SkyrimExternalAliasIdentity {
                owner: key("QUST", 0x203),
                alias_id: 12,
            }],
        )
        .unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::WrongTargetSignature);
    }

    #[test]
    fn unsupported_source_flag_and_receipt_damage_are_rejected() {
        let forced = alias(
            0,
            "Marker",
            AliasFillKind::ForcedReference(key("REFR", 0x200).form_key),
        );
        let plan = fixture_plan(vec![forced.clone()]);
        let error = project_quest_aliases(
            &plan,
            &[evidence(&plan.root_quest, &forced, 0x0001_0000)],
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::UnsupportedFlags);

        let mut projection =
            project_quest_aliases(&plan, &[evidence(&plan.root_quest, &forced, 0)], &[]).unwrap();
        projection.receipt.aliases[0].field_signatures.pop();
        let error = validate_alias_projection(&projection).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::ReceiptMismatch);
    }

    #[test]
    fn materializes_and_applies_exact_alias_block_idempotently() {
        let interner = StringInterner::new();
        let projection = full_projection();
        let materialized = materialize_alias_projection(&projection, &interner).unwrap();
        assert_eq!(materialized.next_alias.sig.as_str(), "ANAM");
        assert_eq!(scalar_u32(&materialized.next_alias.value), Some(9));
        assert_eq!(
            materialized
                .alias_fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>(),
            [
                "ALST", "ALID", "FNAM", "ALFR", "ALED", "ALST", "ALID", "FNAM", "ALUA", "CTDA",
                "ALED", "ALLS", "ALID", "FNAM", "ALFL", "ALED", "ALST", "ALID", "FNAM", "ALEQ",
                "ALEA", "ALED",
            ]
        );
        let condition = materialized
            .alias_fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CTDA")
            .unwrap();
        let FieldValue::Struct(condition_fields) = &condition.value else {
            panic!("CTDA must materialize as a struct");
        };
        assert!(condition_fields.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("function") && *value == FieldValue::Uint(72)
        }));
        let expected_actor =
            parse_quest_record_key(&target_key("NPC_", 0x1201), &interner).unwrap();
        assert!(condition_fields.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("parameter_1")
                && *value == FieldValue::FormKey(expected_actor)
        }));

        let mut record = target_record(&projection, &interner);
        record
            .fields
            .push(field("EDID", FieldValue::String(interner.intern("TargetQuest"))).unwrap());
        record
            .fields
            .push(field("VMAD", FieldValue::Bytes(vec![1_u8, 2, 3].into())).unwrap());
        let application =
            apply_alias_projection_to_qust(&mut record, &projection, &interner).unwrap();
        assert_eq!(
            application.disposition,
            SkyrimAliasApplyDisposition::Inserted
        );
        assert_eq!(application.receipt.next_alias_id, 9);
        assert_eq!(application.receipt.aliases.len(), 4);
        assert_eq!(
            application.receipt.dependencies,
            projection.receipt.dependencies
        );
        assert_eq!(application.receipt.preserved_quest_vmad_count, 1);
        assert_eq!(application.receipt.alias_vmad_attachment_count, 0);
        assert_eq!(count_signature(&record, "VMAD"), 1);

        let exact_fields = record.fields.clone();
        let second = apply_alias_projection_to_qust(&mut record, &projection, &interner).unwrap();
        assert_eq!(
            second.disposition,
            SkyrimAliasApplyDisposition::AlreadyExact
        );
        assert_eq!(record.fields, exact_fields);
        assert_eq!(
            verify_alias_projection_in_record(&record, &projection, &interner).unwrap(),
            second.receipt
        );
    }

    #[test]
    fn applicator_rejects_wrong_conflicting_or_malformed_targets_transactionally() {
        let interner = StringInterner::new();
        let projection = full_projection();

        let mut wrong = Record::new(
            crate::ids::SigCode::from_str("QUST").unwrap(),
            parse_quest_record_key(&target_key("QUST", 0x9999), &interner).unwrap(),
        );
        let error = apply_alias_projection_to_qust(&mut wrong, &projection, &interner).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::WrongTargetRecord);
        assert!(wrong.fields.is_empty());

        let mut conflict = target_record(&projection, &interner);
        conflict
            .fields
            .push(field("ANAM", FieldValue::Uint(1)).unwrap());
        conflict
            .fields
            .push(field("ALST", FieldValue::Uint(0)).unwrap());
        conflict.fields.push(
            field(
                "ALID",
                FieldValue::String(interner.intern("DifferentAlias")),
            )
            .unwrap(),
        );
        conflict
            .fields
            .push(field("FNAM", FieldValue::Uint(0)).unwrap());
        conflict
            .fields
            .push(field("ALED", FieldValue::None).unwrap());
        let original = conflict.fields.clone();
        let error =
            apply_alias_projection_to_qust(&mut conflict, &projection, &interner).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::ExistingAliasConflict);
        assert_eq!(conflict.fields, original);

        let mut malformed = target_record(&projection, &interner);
        malformed
            .fields
            .push(field("ALED", FieldValue::None).unwrap());
        let original = malformed.fields.clone();
        let error =
            apply_alias_projection_to_qust(&mut malformed, &projection, &interner).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::MalformedExistingAlias);
        assert_eq!(malformed.fields, original);
    }

    #[test]
    fn strict_record_receipt_rejects_alias_damage_and_ambiguous_anam_position() {
        let interner = StringInterner::new();
        let projection = full_projection();
        let mut record = target_record(&projection, &interner);
        apply_alias_projection_to_qust(&mut record, &projection, &interner).unwrap();
        let alias_name = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.as_str() == "ALID")
            .unwrap();
        alias_name.value = FieldValue::String(interner.intern("Damaged"));
        let error = verify_alias_projection_in_record(&record, &projection, &interner).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::ReceiptMismatch);

        let mut ambiguous = target_record(&projection, &interner);
        ambiguous
            .fields
            .push(field("ANAM", FieldValue::Uint(9)).unwrap());
        ambiguous
            .fields
            .push(field("FULL", FieldValue::String(interner.intern("Trailing"))).unwrap());
        let original = ambiguous.fields.clone();
        let error =
            apply_alias_projection_to_qust(&mut ambiguous, &projection, &interner).unwrap_err();
        assert_eq!(error.code, SkyrimAliasRejectionCode::ExistingAliasConflict);
        assert_eq!(ambiguous.fields, original);
    }
}
