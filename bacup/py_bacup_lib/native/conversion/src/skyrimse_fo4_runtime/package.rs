use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const CURRENT_LOCATION_TYPES: [i32; 3] = [2, 7, 12];
const FO4_TRAVEL_INPUT_INDICES: [u8; 4] = [1, 3, 5, 7];
const FO4_PATROL_INPUT_INDICES: [u8; 7] = [0, 1, 2, 4, 6, 8, 10];
const SKYRIM_PATROL_INPUT_INDICES: [u8; 6] = [0, 1, 2, 4, 6, 8];
const PROCEDURE_PHASE_SIGS: [&str; 8] = [
    "PRCB", "PNAM", "FNAM", "PKC2", "PFO2", "PFOR", "BNAM", "UNAM",
];
const LEGACY_SCRIPT_SIGS: [&str; 5] = ["SCHR", "SCDA", "SCTX", "QNAM", "TNAM"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimPackProcedureFamily {
    Travel,
    Patrol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimPackBlueprintKind {
    ExactSemanticBlueprint,
    GenericSafeTravelFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackProcedureEvidence {
    pub family: SkyrimPackProcedureFamily,
    pub source_template: String,
    pub target_template: String,
    pub kind: SkyrimPackBlueprintKind,
    pub evidence_id: String,
    pub source_blueprint_blake3: String,
    pub target_blueprint_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackFlags {
    pub general: u32,
    pub package_type: u8,
    pub interrupt_override: u8,
    pub preferred_speed: u8,
    pub interrupt_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackSchedule {
    pub month: i8,
    pub day_of_week: i8,
    pub date: i8,
    pub hour: i8,
    pub minute: i8,
    pub duration_hours: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimPackCurrentLocation {
    NearPackageStartLocation,
    AtPackageLocation,
    NearSelf,
}

impl SkyrimPackCurrentLocation {
    fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            2 => Self::NearPackageStartLocation,
            7 => Self::AtPackageLocation,
            12 => Self::NearSelf,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackLocationIntent {
    pub selector: SkyrimPackCurrentLocation,
    pub radius: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackPatrolIntent {
    pub source_path_start: String,
    pub radius_f32_bits: u32,
    pub repeatable: bool,
    pub start_at_nearest: bool,
    pub static_pathing: bool,
    pub ride_horse_if_possible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackTargetInventory {
    pub selector_type: i32,
    pub value: String,
    pub count_or_distance: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimPackScriptEvent {
    RecordScript,
    OnBegin,
    OnEnd,
    OnChange,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackScriptSurface {
    pub events: BTreeSet<SkyrimPackScriptEvent>,
    pub source_vmad_present: bool,
    pub legacy_payload_signatures: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackClassification {
    pub source_record: String,
    pub editor_id: Option<String>,
    pub owner_quest: Option<String>,
    pub family: SkyrimPackProcedureFamily,
    pub procedure_evidence: SkyrimPackProcedureEvidence,
    pub flags: SkyrimPackFlags,
    pub schedule: SkyrimPackSchedule,
    pub locations: Vec<SkyrimPackLocationIntent>,
    pub targets: Vec<SkyrimPackTargetInventory>,
    pub patrol: Option<SkyrimPackPatrolIntent>,
    pub condition_count: usize,
    pub procedure_phase_signatures: BTreeSet<String>,
    pub scripts: SkyrimPackScriptSurface,
    pub dependencies: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackCompilerEvidence {
    pub manifest_id: String,
    pub class_name: String,
    pub source_blake3: String,
    pub relative_pex_path: String,
    pub pex_artifact_path: PathBuf,
    pub pex_blake3: String,
    pub compile_run_id: String,
    pub target_game: String,
    pub compiled_success: bool,
    pub fresh_output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackScriptProjection {
    pub event: SkyrimPackScriptEvent,
    pub entrypoint: String,
    pub semantics_evidence_id: String,
    pub vmad_payload_blake3: String,
    pub compiler: SkyrimPackCompilerEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackLoweringRequest {
    pub component_id: String,
    pub quest_component_admitted: bool,
    pub source_owned_by_component: bool,
    pub target_record: String,
    pub script_projections: Vec<SkyrimPackScriptProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackTargetIntent {
    pub component_id: String,
    pub source_record: String,
    pub target_record: String,
    pub editor_id: Option<String>,
    pub owner_quest: Option<String>,
    pub family: SkyrimPackProcedureFamily,
    pub target_template: String,
    pub flags: SkyrimPackFlags,
    pub schedule: SkyrimPackSchedule,
    pub locations: Vec<SkyrimPackLocationIntent>,
    pub patrol: Option<SkyrimPackPatrolIntent>,
    pub dependencies: BTreeSet<String>,
    pub script_projections: Vec<SkyrimPackScriptProjection>,
    pub universal_fallback_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackStrictReceipt {
    pub component_id: String,
    pub source_record: String,
    pub target_record: String,
    pub procedure_evidence_id: String,
    pub source_blueprint_blake3: String,
    pub target_blueprint_blake3: String,
    pub family: SkyrimPackProcedureFamily,
    pub flags: SkyrimPackFlags,
    pub schedule: SkyrimPackSchedule,
    pub locations: Vec<SkyrimPackLocationIntent>,
    pub patrol: Option<SkyrimPackPatrolIntent>,
    pub dependencies: BTreeSet<String>,
    pub compiler_evidence_ids: BTreeSet<String>,
    pub vmad_payload_blake3: BTreeSet<String>,
    pub universal_fallback_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimPackLowering {
    pub target: SkyrimPackTargetIntent,
    pub receipt: SkyrimPackStrictReceipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimPackFo4TemplateBlueprint {
    TravelV1,
    PatrolV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackTemplateMaterializationEvidence {
    pub family: SkyrimPackProcedureFamily,
    pub blueprint: SkyrimPackFo4TemplateBlueprint,
    pub evidence_id: String,
    pub target_template: String,
    pub target_blueprint_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPackRecordReceipt {
    pub component_id: String,
    pub source_record: String,
    pub target_record: String,
    pub target_template: String,
    pub family: SkyrimPackProcedureFamily,
    pub flags: SkyrimPackFlags,
    pub schedule: SkyrimPackSchedule,
    pub locations: Vec<SkyrimPackLocationIntent>,
    pub patrol: Option<SkyrimPackPatrolIntent>,
    pub mapped_references: BTreeMap<String, String>,
    pub field_sequence: Vec<String>,
    pub pending_script_events: BTreeSet<SkyrimPackScriptEvent>,
    pub pending_vmad_payload_blake3: BTreeSet<String>,
    pub template_evidence_id: String,
    pub target_blueprint_blake3: String,
    pub vmad_attached: bool,
}

#[derive(Debug, Clone)]
pub struct SkyrimPackMaterialization {
    pub record: Record,
    pub pending_script_projections: Vec<SkyrimPackScriptProjection>,
    pub receipt: SkyrimPackRecordReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum SkyrimPackRejectionReason {
    NotPack,
    MissingField { signature: String },
    DuplicateField { signature: String },
    MalformedField { signature: String },
    UnsupportedPackageKind { value: u8 },
    UnknownProcedureTemplate { source_template: String },
    GenericFallbackForbidden,
    InvalidBlueprintEvidence,
    UnsupportedLocation { selector_type: i32 },
    UnsupportedTarget { selector_type: i32 },
    ConditionsRequireSemanticLowering { count: usize },
    ProcedurePhasesRequireSemanticLowering { signatures: BTreeSet<String> },
    UnsupportedPackageData { signatures: BTreeSet<String> },
    QuestComponentNotAdmitted,
    PackNotOwnedByComponent,
    InvalidTargetIdentity,
    MissingCompilerEvidence { event: SkyrimPackScriptEvent },
    InvalidCompilerEvidence { event: SkyrimPackScriptEvent },
    MissingVmadEvidence { event: SkyrimPackScriptEvent },
    UnexpectedScriptProjection { event: SkyrimPackScriptEvent },
    InvalidMaterializationEvidence,
    UnsupportedPatrolShape,
    MissingReferenceMapping { source_form_key: String },
    InvalidMaterializedRecord { field: String },
    ReceiptMismatch { field: String },
}

pub fn classify_skyrim_pack(
    record: &Record,
    evidence: &SkyrimPackProcedureEvidence,
    interner: &StringInterner,
) -> Result<SkyrimPackClassification, SkyrimPackRejectionReason> {
    if record.sig.as_str() != "PACK" {
        return Err(SkyrimPackRejectionReason::NotPack);
    }
    validate_blueprint_evidence(evidence)?;
    let flags = parse_flags(required_unique(record, "PKDT")?, interner)?;
    if flags.package_type != 18 {
        return Err(SkyrimPackRejectionReason::UnsupportedPackageKind {
            value: flags.package_type,
        });
    }
    let schedule = parse_schedule(required_unique(record, "PSDT")?, interner)?;
    let source_template = parse_source_template(required_unique(record, "PKCU")?, interner)?;
    if !same_form_key(&source_template, &evidence.source_template) {
        return Err(SkyrimPackRejectionReason::UnknownProcedureTemplate { source_template });
    }

    let condition_count = record
        .fields
        .iter()
        .filter(|field| matches!(field.sig.as_str(), "CTDA" | "CTDT"))
        .count();
    if condition_count != 0 {
        return Err(
            SkyrimPackRejectionReason::ConditionsRequireSemanticLowering {
                count: condition_count,
            },
        );
    }

    let procedure_phase_signatures = procedure_phase_signatures(record);
    if !procedure_phase_signatures.is_empty() {
        return Err(
            SkyrimPackRejectionReason::ProcedurePhasesRequireSemanticLowering {
                signatures: procedure_phase_signatures,
            },
        );
    }

    let mut locations = Vec::new();
    for field in record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "PLDT")
    {
        locations.push(parse_current_location(&field.value, interner)?);
    }
    let mut targets = Vec::new();
    for field in record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "PTDA")
    {
        targets.push(parse_target(&field.value, interner)?);
    }
    let patrol = match evidence.family {
        SkyrimPackProcedureFamily::Travel => {
            if locations.len() != 1 {
                return Err(SkyrimPackRejectionReason::MalformedField {
                    signature: "PLDT".to_string(),
                });
            }
            if let Some(target) = targets.first() {
                return Err(SkyrimPackRejectionReason::UnsupportedTarget {
                    selector_type: target.selector_type,
                });
            }
            None
        }
        SkyrimPackProcedureFamily::Patrol => {
            if !locations.is_empty() || targets.len() != 1 {
                return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
            }
            Some(parse_exact_skyrim_patrol(record, &targets[0], interner)?)
        }
    };

    let scripts = script_surface(record);
    let allowed = BTreeSet::from([
        "EDID", "PKDT", "PSDT", "PKCU", "PLDT", "PTDA", "ANAM", "CNAM", "UNAM", "XNAM", "VMAD",
        "POBA", "POEA", "POCA",
    ]);
    let script_owned = BTreeSet::from(["SCHR", "SCDA", "SCTX", "QNAM", "TNAM", "INAM", "PDTO"]);
    let unsupported = record
        .fields
        .iter()
        .map(|field| field.sig.as_str())
        .filter(|signature| !allowed.contains(signature) && !script_owned.contains(signature))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if !unsupported.is_empty() {
        return Err(SkyrimPackRejectionReason::UnsupportedPackageData {
            signatures: unsupported,
        });
    }

    let mut dependencies = BTreeSet::new();
    collect_form_keys_from_record(record, interner, &mut dependencies);
    dependencies.insert(evidence.target_template.clone());
    let owner_quest = owner_quest(record, interner)?;
    Ok(SkyrimPackClassification {
        source_record: record.form_key.format(interner),
        editor_id: record
            .eid
            .and_then(|editor_id| interner.resolve(editor_id).map(str::to_owned)),
        owner_quest,
        family: evidence.family,
        procedure_evidence: evidence.clone(),
        flags,
        schedule,
        locations,
        targets,
        patrol,
        condition_count,
        procedure_phase_signatures: BTreeSet::new(),
        scripts,
        dependencies,
    })
}

pub fn lower_skyrim_pack(
    classification: &SkyrimPackClassification,
    request: &SkyrimPackLoweringRequest,
) -> Result<SkyrimPackLowering, SkyrimPackRejectionReason> {
    if !request.quest_component_admitted {
        return Err(SkyrimPackRejectionReason::QuestComponentNotAdmitted);
    }
    if !request.source_owned_by_component {
        return Err(SkyrimPackRejectionReason::PackNotOwnedByComponent);
    }
    if request.component_id.trim().is_empty() || !valid_form_key_text(&request.target_record) {
        return Err(SkyrimPackRejectionReason::InvalidTargetIdentity);
    }
    if matches!(
        classification.procedure_evidence.kind,
        SkyrimPackBlueprintKind::GenericSafeTravelFallback
    ) {
        return Err(SkyrimPackRejectionReason::GenericFallbackForbidden);
    }
    validate_script_projections(&classification.scripts, &request.script_projections)?;

    let target = SkyrimPackTargetIntent {
        component_id: request.component_id.clone(),
        source_record: classification.source_record.clone(),
        target_record: request.target_record.clone(),
        editor_id: classification.editor_id.clone(),
        owner_quest: classification.owner_quest.clone(),
        family: classification.family,
        target_template: classification.procedure_evidence.target_template.clone(),
        flags: classification.flags.clone(),
        schedule: classification.schedule.clone(),
        locations: classification.locations.clone(),
        patrol: classification.patrol.clone(),
        dependencies: classification.dependencies.clone(),
        script_projections: request.script_projections.clone(),
        universal_fallback_used: false,
    };
    let receipt = receipt_for(classification, &target);
    verify_skyrim_pack_receipt(classification, &target, &receipt)?;
    Ok(SkyrimPackLowering { target, receipt })
}

pub fn verify_skyrim_pack_receipt(
    classification: &SkyrimPackClassification,
    target: &SkyrimPackTargetIntent,
    receipt: &SkyrimPackStrictReceipt,
) -> Result<(), SkyrimPackRejectionReason> {
    let target_mismatch = if target.source_record != classification.source_record {
        Some("target_source_record")
    } else if target.editor_id != classification.editor_id {
        Some("target_editor_id")
    } else if target.owner_quest != classification.owner_quest {
        Some("target_owner_quest")
    } else if target.family != classification.family {
        Some("target_family")
    } else if target.flags != classification.flags {
        Some("target_flags")
    } else if target.schedule != classification.schedule {
        Some("target_schedule")
    } else if target.locations != classification.locations {
        Some("target_locations")
    } else if target.patrol != classification.patrol {
        Some("target_patrol")
    } else if target.dependencies != classification.dependencies {
        Some("target_dependencies")
    } else if !target
        .target_template
        .eq_ignore_ascii_case(&classification.procedure_evidence.target_template)
    {
        Some("target_template")
    } else {
        None
    };
    if let Some(field) = target_mismatch {
        return Err(SkyrimPackRejectionReason::ReceiptMismatch {
            field: field.to_string(),
        });
    }
    let expected = receipt_for(classification, target);
    let mismatch = if receipt.component_id != expected.component_id {
        Some("component_id")
    } else if receipt.source_record != expected.source_record {
        Some("source_record")
    } else if receipt.target_record != expected.target_record {
        Some("target_record")
    } else if receipt.procedure_evidence_id != expected.procedure_evidence_id {
        Some("procedure_evidence_id")
    } else if receipt.source_blueprint_blake3 != expected.source_blueprint_blake3 {
        Some("source_blueprint_blake3")
    } else if receipt.target_blueprint_blake3 != expected.target_blueprint_blake3 {
        Some("target_blueprint_blake3")
    } else if receipt.family != expected.family {
        Some("family")
    } else if receipt.flags != expected.flags {
        Some("flags")
    } else if receipt.schedule != expected.schedule {
        Some("schedule")
    } else if receipt.locations != expected.locations {
        Some("locations")
    } else if receipt.dependencies != expected.dependencies {
        Some("dependencies")
    } else if receipt.compiler_evidence_ids != expected.compiler_evidence_ids {
        Some("compiler_evidence_ids")
    } else if receipt.vmad_payload_blake3 != expected.vmad_payload_blake3 {
        Some("vmad_payload_blake3")
    } else if receipt.universal_fallback_used || target.universal_fallback_used {
        Some("universal_fallback_used")
    } else {
        None
    };
    mismatch.map_or(Ok(()), |field| {
        Err(SkyrimPackRejectionReason::ReceiptMismatch {
            field: field.to_string(),
        })
    })
}

pub fn materialize_skyrim_pack(
    lowering: &SkyrimPackLowering,
    source_form_key: FormKey,
    target_form_key: FormKey,
    target_template: FormKey,
    reference_mappings: &BTreeMap<String, FormKey>,
    template_evidence: &SkyrimPackTemplateMaterializationEvidence,
    interner: &StringInterner,
) -> Result<SkyrimPackMaterialization, SkyrimPackRejectionReason> {
    validate_materialization_inputs(
        lowering,
        source_form_key,
        target_form_key,
        target_template,
        template_evidence,
        interner,
    )?;
    let mut record = Record::new(SigCode(*b"PACK"), target_form_key);
    if let Some(editor_id) = &lowering.target.editor_id {
        let editor_id = interner.intern(editor_id);
        record.eid = Some(editor_id);
        push_pack_field(&mut record, b"EDID", FieldValue::String(editor_id));
    }
    push_pack_field(
        &mut record,
        b"PKDT",
        pack_struct(
            interner,
            [
                (
                    "general_flags",
                    FieldValue::Uint(u64::from(lowering.target.flags.general)),
                ),
                (
                    "type",
                    FieldValue::Uint(u64::from(lowering.target.flags.package_type)),
                ),
                (
                    "interrupt_override",
                    FieldValue::Uint(u64::from(lowering.target.flags.interrupt_override)),
                ),
                (
                    "preferred_speed",
                    FieldValue::Uint(u64::from(lowering.target.flags.preferred_speed)),
                ),
                ("unknown_u8_4", FieldValue::Uint(0)),
                (
                    "interrupt_flags",
                    FieldValue::Uint(u64::from(lowering.target.flags.interrupt_flags)),
                ),
                ("unknown_u8_6", FieldValue::Uint(0)),
                ("unknown_u8_7", FieldValue::Uint(0)),
            ],
        ),
    );
    push_pack_field(
        &mut record,
        b"PSDT",
        pack_struct(
            interner,
            [
                (
                    "month",
                    FieldValue::Int(i64::from(lowering.target.schedule.month)),
                ),
                (
                    "day_of_week",
                    FieldValue::Int(i64::from(lowering.target.schedule.day_of_week)),
                ),
                (
                    "date",
                    FieldValue::Int(i64::from(lowering.target.schedule.date)),
                ),
                (
                    "hour",
                    FieldValue::Int(i64::from(lowering.target.schedule.hour)),
                ),
                (
                    "minute",
                    FieldValue::Int(i64::from(lowering.target.schedule.minute)),
                ),
                ("unknown_u8_5", FieldValue::Uint(0)),
                ("unknown_u8_6", FieldValue::Uint(0)),
                ("unknown_u8_7", FieldValue::Uint(0)),
                (
                    "duration_hours",
                    FieldValue::Uint(u64::from(lowering.target.schedule.duration_hours)),
                ),
            ],
        ),
    );
    if let Some(owner) = &lowering.target.owner_quest {
        push_pack_field(
            &mut record,
            b"QNAM",
            FieldValue::FormKey(mapped_reference(owner, reference_mappings)?),
        );
    }
    match lowering.target.family {
        SkyrimPackProcedureFamily::Travel => {
            materialize_travel_fields(&mut record, lowering, target_template, interner)?
        }
        SkyrimPackProcedureFamily::Patrol => materialize_patrol_fields(
            &mut record,
            lowering,
            target_template,
            reference_mappings,
            interner,
        )?,
    }
    push_empty_event_fields(&mut record);
    let receipt = receipt_from_materialized_record(
        lowering,
        &record,
        source_form_key,
        target_form_key,
        target_template,
        reference_mappings,
        template_evidence,
        interner,
    )?;
    Ok(SkyrimPackMaterialization {
        record,
        pending_script_projections: lowering.target.script_projections.clone(),
        receipt,
    })
}

pub fn receipt_from_materialized_record(
    lowering: &SkyrimPackLowering,
    record: &Record,
    source_form_key: FormKey,
    target_form_key: FormKey,
    target_template: FormKey,
    reference_mappings: &BTreeMap<String, FormKey>,
    template_evidence: &SkyrimPackTemplateMaterializationEvidence,
    interner: &StringInterner,
) -> Result<SkyrimPackRecordReceipt, SkyrimPackRejectionReason> {
    validate_materialization_inputs(
        lowering,
        source_form_key,
        target_form_key,
        target_template,
        template_evidence,
        interner,
    )?;
    if record.sig.as_str() != "PACK" || record.form_key != target_form_key {
        return Err(invalid_materialized("record_identity"));
    }
    if record.eid.and_then(|eid| interner.resolve(eid)) != lowering.target.editor_id.as_deref() {
        return Err(invalid_materialized("editor_id"));
    }
    if parse_flags(required_unique(record, "PKDT")?, interner)? != lowering.target.flags {
        return Err(invalid_materialized("flags"));
    }
    if parse_schedule(required_unique(record, "PSDT")?, interner)? != lowering.target.schedule {
        return Err(invalid_materialized("schedule"));
    }
    let (expected_count, expected_version) = match template_evidence.blueprint {
        SkyrimPackFo4TemplateBlueprint::TravelV1 => (4, 1),
        SkyrimPackFo4TemplateBlueprint::PatrolV2 => (7, 2),
    };
    if !counter_matches(
        required_unique(record, "PKCU")?,
        expected_count,
        expected_version,
        target_template,
        interner,
    )? {
        return Err(invalid_materialized("target_template"));
    }
    let field_sequence = record
        .fields
        .iter()
        .map(|field| field.sig.as_str().to_string())
        .collect::<Vec<_>>();
    if field_sequence != expected_materialized_field_sequence(lowering) {
        return Err(invalid_materialized("field_sequence"));
    }
    if record
        .fields
        .iter()
        .any(|field| field.sig.as_str() == "VMAD")
    {
        return Err(invalid_materialized("vmad_boundary"));
    }
    verify_empty_event_fields(record)?;

    let mut mapped_references = BTreeMap::new();
    if let Some(source_owner) = &lowering.target.owner_quest {
        let expected = mapped_reference(source_owner, reference_mappings)?;
        let owner = required_unique(record, "QNAM")?;
        if !matches!(owner, FieldValue::FormKey(actual) if *actual == expected) {
            return Err(invalid_materialized("owner_quest"));
        }
        mapped_references.insert(source_owner.clone(), expected.format(interner));
    }
    match lowering.target.family {
        SkyrimPackProcedureFamily::Travel => {
            let actual = parse_current_location(required_unique(record, "PLDT")?, interner)?;
            if lowering.target.locations.as_slice() != [actual] || lowering.target.patrol.is_some()
            {
                return Err(invalid_materialized("travel_location"));
            }
            let package_values = package_cnam_values(record);
            if package_values.len() != 3
                || package_values
                    .iter()
                    .any(|value| bool_value(value) != Some(false))
                || pack_byte_values(record, "UNAM") != Some(FO4_TRAVEL_INPUT_INDICES.to_vec())
                || pack_byte_values(record, "XNAM") != Some(vec![0x08])
            {
                return Err(invalid_materialized("travel_inputs"));
            }
        }
        SkyrimPackProcedureFamily::Patrol => {
            let patrol = lowering
                .target
                .patrol
                .as_ref()
                .ok_or(SkyrimPackRejectionReason::UnsupportedPatrolShape)?;
            let mapped_path = mapped_reference(&patrol.source_path_start, reference_mappings)?;
            if !patrol_target_matches(required_unique(record, "PTDA")?, mapped_path, interner)? {
                return Err(invalid_materialized("patrol_path_start"));
            }
            let values = package_cnam_values(record);
            let [
                radius,
                repeatable,
                start_at_nearest,
                static_pathing,
                ride_horse,
                wait_time,
            ] = values.as_slice()
            else {
                return Err(invalid_materialized("patrol_inputs"));
            };
            if float_bits(radius) != Some(patrol.radius_f32_bits)
                || bool_value(repeatable) != Some(patrol.repeatable)
                || bool_value(start_at_nearest) != Some(patrol.start_at_nearest)
                || bool_value(static_pathing) != Some(patrol.static_pathing)
                || bool_value(ride_horse) != Some(patrol.ride_horse_if_possible)
                || float_bits(wait_time) != Some(0.0_f32.to_bits())
                || pack_byte_values(record, "UNAM") != Some(FO4_PATROL_INPUT_INDICES.to_vec())
                || pack_byte_values(record, "XNAM") != Some(vec![0x0B])
            {
                return Err(invalid_materialized("patrol_inputs"));
            }
            mapped_references.insert(
                patrol.source_path_start.clone(),
                mapped_path.format(interner),
            );
        }
    }
    Ok(SkyrimPackRecordReceipt {
        component_id: lowering.target.component_id.clone(),
        source_record: source_form_key.format(interner),
        target_record: record.form_key.format(interner),
        target_template: target_template.format(interner),
        family: lowering.target.family,
        flags: lowering.target.flags.clone(),
        schedule: lowering.target.schedule.clone(),
        locations: lowering.target.locations.clone(),
        patrol: lowering.target.patrol.clone(),
        mapped_references,
        field_sequence,
        pending_script_events: lowering
            .target
            .script_projections
            .iter()
            .map(|projection| projection.event)
            .collect(),
        pending_vmad_payload_blake3: lowering
            .target
            .script_projections
            .iter()
            .map(|projection| projection.vmad_payload_blake3.clone())
            .collect(),
        template_evidence_id: template_evidence.evidence_id.clone(),
        target_blueprint_blake3: template_evidence.target_blueprint_blake3.clone(),
        vmad_attached: false,
    })
}

pub fn verify_skyrim_pack_record_receipt(
    lowering: &SkyrimPackLowering,
    record: &Record,
    source_form_key: FormKey,
    target_form_key: FormKey,
    target_template: FormKey,
    reference_mappings: &BTreeMap<String, FormKey>,
    template_evidence: &SkyrimPackTemplateMaterializationEvidence,
    receipt: &SkyrimPackRecordReceipt,
    interner: &StringInterner,
) -> Result<(), SkyrimPackRejectionReason> {
    let expected = receipt_from_materialized_record(
        lowering,
        record,
        source_form_key,
        target_form_key,
        target_template,
        reference_mappings,
        template_evidence,
        interner,
    )?;
    if receipt == &expected {
        Ok(())
    } else {
        Err(SkyrimPackRejectionReason::ReceiptMismatch {
            field: "materialized_record_receipt".to_string(),
        })
    }
}

fn validate_materialization_inputs(
    lowering: &SkyrimPackLowering,
    source_form_key: FormKey,
    target_form_key: FormKey,
    target_template: FormKey,
    evidence: &SkyrimPackTemplateMaterializationEvidence,
    interner: &StringInterner,
) -> Result<(), SkyrimPackRejectionReason> {
    let expected_blueprint = match lowering.target.family {
        SkyrimPackProcedureFamily::Travel => SkyrimPackFo4TemplateBlueprint::TravelV1,
        SkyrimPackProcedureFamily::Patrol => SkyrimPackFo4TemplateBlueprint::PatrolV2,
    };
    if source_form_key.format(interner) != lowering.target.source_record
        || target_form_key.format(interner) != lowering.target.target_record
    {
        return Err(SkyrimPackRejectionReason::InvalidTargetIdentity);
    }
    if evidence.family != lowering.target.family
        || evidence.blueprint != expected_blueprint
        || evidence.evidence_id.trim().is_empty()
        || !same_form_key(&evidence.target_template, &lowering.target.target_template)
        || !same_form_key(&evidence.target_template, &target_template.format(interner))
        || !is_blake3(&evidence.target_blueprint_blake3)
        || !evidence
            .target_blueprint_blake3
            .eq_ignore_ascii_case(&lowering.receipt.target_blueprint_blake3)
        || lowering.target.universal_fallback_used
        || lowering.receipt.universal_fallback_used
    {
        return Err(SkyrimPackRejectionReason::InvalidMaterializationEvidence);
    }
    Ok(())
}

fn materialize_travel_fields(
    record: &mut Record,
    lowering: &SkyrimPackLowering,
    target_template: FormKey,
    interner: &StringInterner,
) -> Result<(), SkyrimPackRejectionReason> {
    let [location] = lowering.target.locations.as_slice() else {
        return Err(invalid_materialized("travel_location"));
    };
    if lowering.target.patrol.is_some() {
        return Err(invalid_materialized("travel_patrol_intent"));
    }
    push_pack_counter(record, 4, target_template, 1, interner);
    push_pack_string(record, b"ANAM", "Location", interner);
    push_pack_field(
        record,
        b"PLDT",
        pack_struct(
            interner,
            [
                (
                    "type",
                    FieldValue::Int(current_location_code(location.selector).into()),
                ),
                ("location_value", FieldValue::Uint(0)),
                ("radius", FieldValue::Int(i64::from(location.radius))),
                ("collection_index", FieldValue::Uint(0)),
            ],
        ),
    );
    for _ in 0..3 {
        push_pack_string(record, b"ANAM", "Bool", interner);
        push_pack_field(record, b"CNAM", FieldValue::Bool(false));
    }
    for index in FO4_TRAVEL_INPUT_INDICES {
        push_pack_bytes(record, b"UNAM", &[index]);
    }
    push_pack_bytes(record, b"XNAM", &[0x08]);
    Ok(())
}

fn materialize_patrol_fields(
    record: &mut Record,
    lowering: &SkyrimPackLowering,
    target_template: FormKey,
    reference_mappings: &BTreeMap<String, FormKey>,
    interner: &StringInterner,
) -> Result<(), SkyrimPackRejectionReason> {
    let patrol = lowering
        .target
        .patrol
        .as_ref()
        .ok_or(SkyrimPackRejectionReason::UnsupportedPatrolShape)?;
    if !lowering.target.locations.is_empty() {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    }
    let path_start = mapped_reference(&patrol.source_path_start, reference_mappings)?;
    push_pack_counter(record, 7, target_template, 2, interner);
    push_pack_string(record, b"ANAM", "SingleRef", interner);
    push_pack_field(
        record,
        b"PTDA",
        pack_struct(
            interner,
            [
                ("target_data_type", FieldValue::Int(0)),
                ("target_data_target", FieldValue::FormKey(path_start)),
                ("target_data_count_distance", FieldValue::Int(0)),
            ],
        ),
    );
    push_pack_string(record, b"ANAM", "Float", interner);
    push_pack_field(
        record,
        b"CNAM",
        FieldValue::Float(f32::from_bits(patrol.radius_f32_bits)),
    );
    for value in [
        patrol.repeatable,
        patrol.start_at_nearest,
        patrol.static_pathing,
        patrol.ride_horse_if_possible,
    ] {
        push_pack_string(record, b"ANAM", "Bool", interner);
        push_pack_field(record, b"CNAM", FieldValue::Bool(value));
    }
    push_pack_string(record, b"ANAM", "Float", interner);
    push_pack_field(record, b"CNAM", FieldValue::Float(0.0));
    for index in FO4_PATROL_INPUT_INDICES {
        push_pack_bytes(record, b"UNAM", &[index]);
    }
    push_pack_bytes(record, b"XNAM", &[0x0B]);
    Ok(())
}

fn expected_materialized_field_sequence(lowering: &SkyrimPackLowering) -> Vec<String> {
    let mut fields = Vec::new();
    if lowering.target.editor_id.is_some() {
        fields.push("EDID");
    }
    fields.extend(["PKDT", "PSDT"]);
    if lowering.target.owner_quest.is_some() {
        fields.push("QNAM");
    }
    match lowering.target.family {
        SkyrimPackProcedureFamily::Travel => fields.extend([
            "PKCU", "ANAM", "PLDT", "ANAM", "CNAM", "ANAM", "CNAM", "ANAM", "CNAM", "UNAM", "UNAM",
            "UNAM", "UNAM", "XNAM",
        ]),
        SkyrimPackProcedureFamily::Patrol => fields.extend([
            "PKCU", "ANAM", "PTDA", "ANAM", "CNAM", "ANAM", "CNAM", "ANAM", "CNAM", "ANAM", "CNAM",
            "ANAM", "CNAM", "ANAM", "CNAM", "UNAM", "UNAM", "UNAM", "UNAM", "UNAM", "UNAM", "UNAM",
            "XNAM",
        ]),
    }
    fields.extend([
        "POBA", "INAM", "PDTO", "POEA", "INAM", "PDTO", "POCA", "INAM", "PDTO",
    ]);
    fields.into_iter().map(str::to_string).collect()
}

fn mapped_reference(
    source: &str,
    mappings: &BTreeMap<String, FormKey>,
) -> Result<FormKey, SkyrimPackRejectionReason> {
    mappings
        .iter()
        .find_map(|(candidate, target)| candidate.eq_ignore_ascii_case(source).then_some(*target))
        .ok_or_else(|| SkyrimPackRejectionReason::MissingReferenceMapping {
            source_form_key: source.to_string(),
        })
}

fn current_location_code(location: SkyrimPackCurrentLocation) -> i32 {
    match location {
        SkyrimPackCurrentLocation::NearPackageStartLocation => 2,
        SkyrimPackCurrentLocation::AtPackageLocation => 7,
        SkyrimPackCurrentLocation::NearSelf => 12,
    }
}

fn counter_matches(
    value: &FieldValue,
    expected_count: u64,
    expected_version: u64,
    target_template: FormKey,
    interner: &StringInterner,
) -> Result<bool, SkyrimPackRejectionReason> {
    match value {
        FieldValue::Struct(fields) => Ok(integer(fields, "data_input_count", interner, "PKCU")?
            == expected_count
            && integer(fields, "version_counter_autoincremented", interner, "PKCU")?
                == expected_version
            && same_form_key(
                &parse_source_template(value, interner)?,
                &target_template.format(interner),
            )),
        FieldValue::Bytes(bytes) if bytes.len() == 12 => {
            let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            let encoded_template = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
            let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
            Ok(u64::from(count) == expected_count
                && u64::from(version) == expected_version
                && encoded_template >> 24 == 0
                && encoded_template & 0x00FF_FFFF == target_template.local)
        }
        _ => Err(SkyrimPackRejectionReason::MalformedField {
            signature: "PKCU".to_string(),
        }),
    }
}

fn patrol_target_matches(
    value: &FieldValue,
    expected: FormKey,
    interner: &StringInterner,
) -> Result<bool, SkyrimPackRejectionReason> {
    match value {
        FieldValue::Struct(_) => {
            let target = parse_target(value, interner)?;
            Ok(target.selector_type == 0
                && target.count_or_distance == 0
                && same_form_key(&target.value, &expected.format(interner)))
        }
        FieldValue::Bytes(bytes) if bytes.len() == 12 => {
            let target_type = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
            let encoded_target = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
            let distance = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
            Ok(target_type == 0
                && distance == 0
                && encoded_target != 0
                && encoded_target & 0x00FF_FFFF == expected.local)
        }
        _ => Err(SkyrimPackRejectionReason::MalformedField {
            signature: "PTDA".to_string(),
        }),
    }
}

fn pack_struct<const N: usize>(
    interner: &StringInterner,
    fields: [(&str, FieldValue); N],
) -> FieldValue {
    FieldValue::Struct(
        fields
            .into_iter()
            .map(|(name, value)| (interner.intern(name), value))
            .collect(),
    )
}

fn push_pack_counter(
    record: &mut Record,
    count: u32,
    template: FormKey,
    version: u32,
    interner: &StringInterner,
) {
    push_pack_field(
        record,
        b"PKCU",
        pack_struct(
            interner,
            [
                ("data_input_count", FieldValue::Uint(u64::from(count))),
                ("package_template", FieldValue::FormKey(template)),
                (
                    "version_counter_autoincremented",
                    FieldValue::Uint(u64::from(version)),
                ),
            ],
        ),
    );
}

fn push_pack_string(
    record: &mut Record,
    signature: &[u8; 4],
    value: &str,
    interner: &StringInterner,
) {
    push_pack_field(
        record,
        signature,
        FieldValue::String(interner.intern(value)),
    );
}

fn push_pack_bytes(record: &mut Record, signature: &[u8; 4], value: &[u8]) {
    push_pack_field(
        record,
        signature,
        FieldValue::Bytes(SmallVec::from_slice(value)),
    );
}

fn push_pack_field(record: &mut Record, signature: &[u8; 4], value: FieldValue) {
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*signature),
        value,
    });
}

fn push_empty_event_fields(record: &mut Record) {
    for marker in [b"POBA", b"POEA", b"POCA"] {
        push_pack_field(record, marker, FieldValue::None);
        push_pack_bytes(record, b"INAM", &[0; 4]);
        push_pack_bytes(record, b"PDTO", &[0; 8]);
    }
}

fn verify_empty_event_fields(record: &Record) -> Result<(), SkyrimPackRejectionReason> {
    for marker in ["POBA", "POEA", "POCA"] {
        let index = record
            .fields
            .iter()
            .position(|field| field.sig.as_str() == marker)
            .ok_or_else(|| invalid_materialized("event_boundary"))?;
        if !matches!(&record.fields[index].value, FieldValue::None)
            && !matches!(&record.fields[index].value, FieldValue::Bytes(bytes) if bytes.is_empty())
        {
            return Err(invalid_materialized("event_boundary"));
        }
        if !record
            .fields
            .get(index + 1)
            .is_some_and(|field| field.sig.as_str() == "INAM" && null_value(&field.value))
            || !record
                .fields
                .get(index + 2)
                .is_some_and(|field| field.sig.as_str() == "PDTO" && null_value(&field.value))
        {
            return Err(invalid_materialized("event_boundary"));
        }
    }
    Ok(())
}

fn null_value(value: &FieldValue) -> bool {
    match value {
        FieldValue::None | FieldValue::Int(0) | FieldValue::Uint(0) => true,
        FieldValue::FormKey(value) => value.local == 0,
        FieldValue::Bytes(bytes) => bytes.iter().all(|byte| *byte == 0),
        FieldValue::Struct(fields) => fields.iter().all(|(_, value)| null_value(value)),
        _ => false,
    }
}

fn package_cnam_values(record: &Record) -> Vec<&FieldValue> {
    let Some(start) = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "PKCU")
    else {
        return Vec::new();
    };
    record.fields[start + 1..]
        .iter()
        .take_while(|field| field.sig.as_str() != "XNAM")
        .filter(|field| field.sig.as_str() == "CNAM")
        .map(|field| &field.value)
        .collect()
}

fn pack_byte_values(record: &Record, signature: &str) -> Option<Vec<u8>> {
    let start = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "PKCU")?;
    record.fields[start + 1..]
        .iter()
        .take_while(|field| field.sig.as_str() != "XNAM" || signature == "XNAM")
        .filter(|field| field.sig.as_str() == signature)
        .map(|field| byte_value(&field.value))
        .collect()
}

fn invalid_materialized(field: &str) -> SkyrimPackRejectionReason {
    SkyrimPackRejectionReason::InvalidMaterializedRecord {
        field: field.to_string(),
    }
}

fn receipt_for(
    classification: &SkyrimPackClassification,
    target: &SkyrimPackTargetIntent,
) -> SkyrimPackStrictReceipt {
    SkyrimPackStrictReceipt {
        component_id: target.component_id.clone(),
        source_record: classification.source_record.clone(),
        target_record: target.target_record.clone(),
        procedure_evidence_id: classification.procedure_evidence.evidence_id.clone(),
        source_blueprint_blake3: classification
            .procedure_evidence
            .source_blueprint_blake3
            .clone(),
        target_blueprint_blake3: classification
            .procedure_evidence
            .target_blueprint_blake3
            .clone(),
        family: classification.family,
        flags: classification.flags.clone(),
        schedule: classification.schedule.clone(),
        locations: classification.locations.clone(),
        patrol: classification.patrol.clone(),
        dependencies: classification.dependencies.clone(),
        compiler_evidence_ids: target
            .script_projections
            .iter()
            .map(|projection| projection.compiler.compile_run_id.clone())
            .collect(),
        vmad_payload_blake3: target
            .script_projections
            .iter()
            .map(|projection| projection.vmad_payload_blake3.clone())
            .collect(),
        universal_fallback_used: false,
    }
}

fn validate_blueprint_evidence(
    evidence: &SkyrimPackProcedureEvidence,
) -> Result<(), SkyrimPackRejectionReason> {
    if matches!(
        evidence.kind,
        SkyrimPackBlueprintKind::GenericSafeTravelFallback
    ) {
        return Err(SkyrimPackRejectionReason::GenericFallbackForbidden);
    }
    if evidence.evidence_id.trim().is_empty()
        || !valid_form_key_text(&evidence.source_template)
        || !valid_form_key_text(&evidence.target_template)
        || !is_blake3(&evidence.source_blueprint_blake3)
        || !is_blake3(&evidence.target_blueprint_blake3)
    {
        return Err(SkyrimPackRejectionReason::InvalidBlueprintEvidence);
    }
    Ok(())
}

fn validate_script_projections(
    source: &SkyrimPackScriptSurface,
    projections: &[SkyrimPackScriptProjection],
) -> Result<(), SkyrimPackRejectionReason> {
    let projected = projections
        .iter()
        .map(|projection| projection.event)
        .collect::<BTreeSet<_>>();
    for event in source.events.difference(&projected) {
        return Err(SkyrimPackRejectionReason::MissingCompilerEvidence { event: *event });
    }
    for event in projected.difference(&source.events) {
        return Err(SkyrimPackRejectionReason::UnexpectedScriptProjection { event: *event });
    }
    if projected.len() != projections.len() {
        let event = projections
            .first()
            .map(|projection| projection.event)
            .unwrap_or(SkyrimPackScriptEvent::RecordScript);
        return Err(SkyrimPackRejectionReason::UnexpectedScriptProjection { event });
    }
    for projection in projections {
        let evidence = &projection.compiler;
        if !evidence.compiled_success
            || !evidence.fresh_output
            || !evidence.target_game.eq_ignore_ascii_case("fo4")
            || evidence.manifest_id.trim().is_empty()
            || evidence.class_name.trim().is_empty()
            || evidence.compile_run_id.trim().is_empty()
            || !is_blake3(&evidence.source_blake3)
            || !is_blake3(&evidence.pex_blake3)
            || !evidence
                .relative_pex_path
                .eq_ignore_ascii_case(&format!("data/Scripts/{}.pex", evidence.class_name))
            || projection.semantics_evidence_id.trim().is_empty()
        {
            return Err(SkyrimPackRejectionReason::InvalidCompilerEvidence {
                event: projection.event,
            });
        }
        validate_compiled_package(evidence, &projection.entrypoint).map_err(|()| {
            SkyrimPackRejectionReason::InvalidCompilerEvidence {
                event: projection.event,
            }
        })?;
        if !is_blake3(&projection.vmad_payload_blake3) {
            return Err(SkyrimPackRejectionReason::MissingVmadEvidence {
                event: projection.event,
            });
        }
    }
    Ok(())
}

fn validate_compiled_package(
    evidence: &SkyrimPackCompilerEvidence,
    entrypoint: &str,
) -> Result<(), ()> {
    if !evidence.pex_artifact_path.is_absolute() {
        return Err(());
    }
    let bytes = std::fs::read(&evidence.pex_artifact_path).map_err(|_| ())?;
    if bytes.is_empty()
        || !blake3::hash(&bytes)
            .to_hex()
            .to_string()
            .eq_ignore_ascii_case(&evidence.pex_blake3)
    {
        return Err(());
    }
    let pex = papyrus_core::pex::parse_pex_bytes(&bytes).map_err(|_| ())?;
    if pex.game_id != 2 {
        return Err(());
    }
    let matching = pex
        .objects
        .iter()
        .filter(|object| object.name.eq_ignore_ascii_case(&evidence.class_name))
        .collect::<Vec<_>>();
    if matching.len() != 1 || !matching[0].parent.eq_ignore_ascii_case("Package") {
        return Err(());
    }
    matching[0]
        .states
        .iter()
        .flat_map(|state| &state.functions)
        .any(|function| function.name.eq_ignore_ascii_case(entrypoint))
        .then_some(())
        .ok_or(())
}

fn parse_flags(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<SkyrimPackFlags, SkyrimPackRejectionReason> {
    if let FieldValue::Bytes(bytes) = value {
        if bytes.len() != 12 || bytes[7] != 0 || bytes[10] != 0 || bytes[11] != 0 {
            return Err(SkyrimPackRejectionReason::MalformedField {
                signature: "PKDT".to_string(),
            });
        }
        return Ok(SkyrimPackFlags {
            general: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            package_type: bytes[4],
            interrupt_override: bytes[5],
            preferred_speed: bytes[6],
            interrupt_flags: u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
        });
    }
    let fields = struct_fields(value, "PKDT")?;
    let unknown_4 = integer(fields, "unknown_u8_4", interner, "PKDT")?;
    let unknown_6 = integer(fields, "unknown_u8_6", interner, "PKDT")?;
    let unknown_7 = integer(fields, "unknown_u8_7", interner, "PKDT")?;
    if unknown_4 != 0 || unknown_6 != 0 || unknown_7 != 0 {
        return Err(SkyrimPackRejectionReason::MalformedField {
            signature: "PKDT".to_string(),
        });
    }
    Ok(SkyrimPackFlags {
        general: integer(fields, "general_flags", interner, "PKDT")? as u32,
        package_type: integer(fields, "type", interner, "PKDT")? as u8,
        interrupt_override: integer(fields, "interrupt_override", interner, "PKDT")? as u8,
        preferred_speed: integer(fields, "preferred_speed", interner, "PKDT")? as u8,
        interrupt_flags: integer(fields, "interrupt_flags", interner, "PKDT")? as u16,
    })
}

fn parse_schedule(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<SkyrimPackSchedule, SkyrimPackRejectionReason> {
    if let FieldValue::Bytes(bytes) = value {
        if bytes.len() != 12 || bytes[5..8].iter().any(|byte| *byte != 0) {
            return Err(SkyrimPackRejectionReason::MalformedField {
                signature: "PSDT".to_string(),
            });
        }
        return Ok(SkyrimPackSchedule {
            month: bytes[0] as i8,
            day_of_week: bytes[1] as i8,
            date: bytes[2] as i8,
            hour: bytes[3] as i8,
            minute: bytes[4] as i8,
            duration_hours: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        });
    }
    let fields = struct_fields(value, "PSDT")?;
    for field in ["unknown_u8_5", "unknown_u8_6", "unknown_u8_7"] {
        if integer(fields, field, interner, "PSDT")? != 0 {
            return Err(SkyrimPackRejectionReason::MalformedField {
                signature: "PSDT".to_string(),
            });
        }
    }
    Ok(SkyrimPackSchedule {
        month: signed_integer(fields, "month", interner, "PSDT")? as i8,
        day_of_week: signed_integer(fields, "day_of_week", interner, "PSDT")? as i8,
        date: signed_integer(fields, "date", interner, "PSDT")? as i8,
        hour: signed_integer(fields, "hour", interner, "PSDT")? as i8,
        minute: signed_integer(fields, "minute", interner, "PSDT")? as i8,
        duration_hours: integer(fields, "duration_hours", interner, "PSDT")? as u32,
    })
}

fn parse_source_template(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<String, SkyrimPackRejectionReason> {
    let fields = struct_fields(value, "PKCU")?;
    let value = named(fields, "package_template", interner).ok_or_else(|| {
        SkyrimPackRejectionReason::MalformedField {
            signature: "PKCU".to_string(),
        }
    })?;
    match value {
        FieldValue::FormKey(form_key) => Ok(form_key.format(interner)),
        _ => Err(SkyrimPackRejectionReason::MalformedField {
            signature: "PKCU".to_string(),
        }),
    }
}

fn parse_current_location(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<SkyrimPackLocationIntent, SkyrimPackRejectionReason> {
    if let FieldValue::Bytes(bytes) = value {
        if bytes.len() != 16 || bytes[4..8].iter().any(|byte| *byte != 0) {
            return Err(SkyrimPackRejectionReason::MalformedField {
                signature: "PLDT".to_string(),
            });
        }
        let selector_type = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
        return Ok(SkyrimPackLocationIntent {
            selector: SkyrimPackCurrentLocation::from_code(selector_type)
                .ok_or(SkyrimPackRejectionReason::UnsupportedLocation { selector_type })?,
            radius: i32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        });
    }
    let fields = struct_fields(value, "PLDT")?;
    let selector_type = signed_integer(fields, "type", interner, "PLDT")? as i32;
    if !CURRENT_LOCATION_TYPES.contains(&selector_type) {
        return Err(SkyrimPackRejectionReason::UnsupportedLocation { selector_type });
    }
    let selector = SkyrimPackCurrentLocation::from_code(selector_type)
        .ok_or(SkyrimPackRejectionReason::UnsupportedLocation { selector_type })?;
    let location = named(fields, "location_value", interner).ok_or_else(|| {
        SkyrimPackRejectionReason::MalformedField {
            signature: "PLDT".to_string(),
        }
    })?;
    if !matches!(
        location,
        FieldValue::Uint(0) | FieldValue::Int(0) | FieldValue::None
    ) {
        return Err(SkyrimPackRejectionReason::MalformedField {
            signature: "PLDT".to_string(),
        });
    }
    Ok(SkyrimPackLocationIntent {
        selector,
        radius: signed_integer(fields, "radius", interner, "PLDT")? as i32,
    })
}

fn parse_target(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<SkyrimPackTargetInventory, SkyrimPackRejectionReason> {
    let fields = struct_fields(value, "PTDA")?;
    let selector_type =
        signed_integer_any(fields, &["type", "target_data_type"], interner, "PTDA")? as i32;
    let target =
        named_any(fields, &["target", "target_data_target"], interner).ok_or_else(|| {
            SkyrimPackRejectionReason::MalformedField {
                signature: "PTDA".to_string(),
            }
        })?;
    Ok(SkyrimPackTargetInventory {
        selector_type,
        value: describe_value(target, interner),
        count_or_distance: signed_integer_any(
            fields,
            &["count_distance", "target_data_count_distance"],
            interner,
            "PTDA",
        )? as i32,
    })
}

fn parse_exact_skyrim_patrol(
    record: &Record,
    target: &SkyrimPackTargetInventory,
    interner: &StringInterner,
) -> Result<SkyrimPackPatrolIntent, SkyrimPackRejectionReason> {
    if target.selector_type != 0 || !valid_form_key_text(&target.value) {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    }
    let pkcu = struct_fields(required_unique(record, "PKCU")?, "PKCU")?;
    if integer(pkcu, "data_input_count", interner, "PKCU")? != 6 {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    }
    let Some(pkcu_index) = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "PKCU")
    else {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    };
    let Some(xnam_index) = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "XNAM")
    else {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    };
    if xnam_index <= pkcu_index || byte_value(&record.fields[xnam_index].value) != Some(0x09) {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    }
    let package_data = &record.fields[pkcu_index + 1..xnam_index];
    let expected = [
        "ANAM", "PTDA", "ANAM", "CNAM", "ANAM", "CNAM", "ANAM", "CNAM", "ANAM", "CNAM", "ANAM",
        "CNAM", "UNAM", "UNAM", "UNAM", "UNAM", "UNAM", "UNAM",
    ];
    if package_data
        .iter()
        .map(|field| field.sig.as_str())
        .ne(expected)
    {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    }
    let names = package_data
        .iter()
        .filter(|field| field.sig.as_str() == "ANAM")
        .map(|field| string_value(&field.value, interner))
        .collect::<Option<Vec<_>>>();
    if names.as_deref() != Some(&["SingleRef", "Float", "Bool", "Bool", "Bool", "Bool"]) {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    }
    let values = package_data
        .iter()
        .filter(|field| field.sig.as_str() == "CNAM")
        .map(|field| &field.value)
        .collect::<Vec<_>>();
    let [
        radius,
        repeatable,
        start_at_nearest,
        static_pathing,
        ride_horse_if_possible,
    ] = values.as_slice()
    else {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    };
    let indices = package_data
        .iter()
        .filter(|field| field.sig.as_str() == "UNAM")
        .map(|field| byte_value(&field.value))
        .collect::<Option<Vec<_>>>();
    if indices.as_deref() != Some(SKYRIM_PATROL_INPUT_INDICES.as_slice()) {
        return Err(SkyrimPackRejectionReason::UnsupportedPatrolShape);
    }
    Ok(SkyrimPackPatrolIntent {
        source_path_start: target.value.clone(),
        radius_f32_bits: float_bits(radius)
            .ok_or(SkyrimPackRejectionReason::UnsupportedPatrolShape)?,
        repeatable: bool_value(repeatable)
            .ok_or(SkyrimPackRejectionReason::UnsupportedPatrolShape)?,
        start_at_nearest: bool_value(start_at_nearest)
            .ok_or(SkyrimPackRejectionReason::UnsupportedPatrolShape)?,
        static_pathing: bool_value(static_pathing)
            .ok_or(SkyrimPackRejectionReason::UnsupportedPatrolShape)?,
        ride_horse_if_possible: bool_value(ride_horse_if_possible)
            .ok_or(SkyrimPackRejectionReason::UnsupportedPatrolShape)?,
    })
}

fn owner_quest(
    record: &Record,
    interner: &StringInterner,
) -> Result<Option<String>, SkyrimPackRejectionReason> {
    let pkcu_index = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "PKCU")
        .unwrap_or(record.fields.len());
    let owners = record.fields[..pkcu_index]
        .iter()
        .filter(|field| field.sig.as_str() == "QNAM")
        .collect::<Vec<_>>();
    match owners.as_slice() {
        [] => Ok(None),
        [owner] => match owner.value {
            FieldValue::FormKey(form_key) => Ok(Some(form_key.format(interner))),
            _ => Err(SkyrimPackRejectionReason::MalformedField {
                signature: "QNAM".to_string(),
            }),
        },
        _ => Err(SkyrimPackRejectionReason::DuplicateField {
            signature: "QNAM".to_string(),
        }),
    }
}

fn script_surface(record: &Record) -> SkyrimPackScriptSurface {
    let mut surface = SkyrimPackScriptSurface::default();
    for (index, field) in record.fields.iter().enumerate() {
        match field.sig.as_str() {
            "VMAD" => {
                surface.source_vmad_present = true;
                surface.events.insert(SkyrimPackScriptEvent::RecordScript);
            }
            "POBA" if event_has_payload(record, index) => {
                surface.events.insert(SkyrimPackScriptEvent::OnBegin);
            }
            "POEA" if event_has_payload(record, index) => {
                surface.events.insert(SkyrimPackScriptEvent::OnEnd);
            }
            "POCA" if event_has_payload(record, index) => {
                surface.events.insert(SkyrimPackScriptEvent::OnChange);
            }
            signature if LEGACY_SCRIPT_SIGS.contains(&signature) => {
                surface
                    .legacy_payload_signatures
                    .insert(signature.to_string());
            }
            _ => {}
        }
    }
    surface
}

fn event_has_payload(record: &Record, marker_index: usize) -> bool {
    if !null_value(&record.fields[marker_index].value) {
        return true;
    }
    record.fields[marker_index + 1..]
        .iter()
        .take_while(|field| !matches!(field.sig.as_str(), "POBA" | "POEA" | "POCA"))
        .any(|field| match field.sig.as_str() {
            "INAM" | "PDTO" => !null_value(&field.value),
            "SCHR" | "SCDA" | "SCTX" | "QNAM" | "TNAM" | "VMAD" => true,
            _ => false,
        })
}

fn required_unique<'a>(
    record: &'a Record,
    signature: &str,
) -> Result<&'a FieldValue, SkyrimPackRejectionReason> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature);
    let Some(field) = fields.next() else {
        return Err(SkyrimPackRejectionReason::MissingField {
            signature: signature.to_string(),
        });
    };
    if fields.next().is_some() {
        return Err(SkyrimPackRejectionReason::DuplicateField {
            signature: signature.to_string(),
        });
    }
    Ok(&field.value)
}

fn struct_fields<'a>(
    value: &'a FieldValue,
    signature: &str,
) -> Result<&'a [(crate::sym::Sym, FieldValue)], SkyrimPackRejectionReason> {
    match value {
        FieldValue::Struct(fields) => Ok(fields),
        _ => Err(SkyrimPackRejectionReason::MalformedField {
            signature: signature.to_string(),
        }),
    }
}

fn named<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields.iter().find_map(|(field, value)| {
        interner
            .resolve(*field)
            .is_some_and(|field| field.eq_ignore_ascii_case(name))
            .then_some(value)
    })
}

fn named_any<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    names: &[&str],
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    names.iter().find_map(|name| named(fields, name, interner))
}

fn integer(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
    signature: &str,
) -> Result<u64, SkyrimPackRejectionReason> {
    match named(fields, name, interner) {
        Some(FieldValue::Uint(value)) => Ok(*value),
        Some(FieldValue::Int(value)) if *value >= 0 => Ok(*value as u64),
        _ => Err(SkyrimPackRejectionReason::MalformedField {
            signature: signature.to_string(),
        }),
    }
}

fn signed_integer(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
    signature: &str,
) -> Result<i64, SkyrimPackRejectionReason> {
    match named(fields, name, interner) {
        Some(FieldValue::Int(value)) => Ok(*value),
        Some(FieldValue::Uint(value)) => Ok(*value as i64),
        _ => Err(SkyrimPackRejectionReason::MalformedField {
            signature: signature.to_string(),
        }),
    }
}

fn signed_integer_any(
    fields: &[(crate::sym::Sym, FieldValue)],
    names: &[&str],
    interner: &StringInterner,
    signature: &str,
) -> Result<i64, SkyrimPackRejectionReason> {
    for name in names {
        if let Some(value) = named(fields, name, interner) {
            return match value {
                FieldValue::Int(value) => Ok(*value),
                FieldValue::Uint(value) => Ok(*value as i64),
                _ => Err(SkyrimPackRejectionReason::MalformedField {
                    signature: signature.to_string(),
                }),
            };
        }
    }
    Err(SkyrimPackRejectionReason::MalformedField {
        signature: signature.to_string(),
    })
}

fn string_value<'a>(value: &'a FieldValue, interner: &'a StringInterner) -> Option<&'a str> {
    match value {
        FieldValue::String(value) => interner.resolve(*value),
        FieldValue::Bytes(bytes) => {
            std::str::from_utf8(bytes.strip_suffix(&[0]).unwrap_or(bytes)).ok()
        }
        _ => None,
    }
}

fn byte_value(value: &FieldValue) -> Option<u8> {
    match value {
        FieldValue::Bool(value) => Some(u8::from(*value)),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Bytes(bytes) if bytes.len() == 1 => Some(bytes[0]),
        _ => None,
    }
}

fn bool_value(value: &FieldValue) -> Option<bool> {
    byte_value(value).and_then(|value| match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    })
}

fn float_bits(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Float(value) if value.is_finite() => Some(value.to_bits()),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            let value = f32::from_le_bytes(bytes.as_slice().try_into().ok()?);
            value.is_finite().then_some(value.to_bits())
        }
        _ => None,
    }
}

fn procedure_phase_signatures(record: &Record) -> BTreeSet<String> {
    let start = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "XNAM")
        .map_or(0, |index| index + 1);
    record.fields[start..]
        .iter()
        .take_while(|field| !matches!(field.sig.as_str(), "POBA" | "POEA" | "POCA"))
        .map(|field| field.sig.as_str())
        .filter(|signature| PROCEDURE_PHASE_SIGS.contains(signature))
        .map(str::to_string)
        .collect()
}

fn collect_form_keys_from_record(
    record: &Record,
    interner: &StringInterner,
    output: &mut BTreeSet<String>,
) {
    for field in &record.fields {
        collect_form_keys(&field.value, interner, output);
    }
}

fn collect_form_keys(value: &FieldValue, interner: &StringInterner, output: &mut BTreeSet<String>) {
    match value {
        FieldValue::FormKey(form_key) => {
            output.insert(form_key.format(interner));
        }
        FieldValue::List(values) => {
            for value in values {
                collect_form_keys(value, interner, output);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_form_keys(value, interner, output);
            }
        }
        _ => {}
    }
}

fn describe_value(value: &FieldValue, interner: &StringInterner) -> String {
    match value {
        FieldValue::None => "none".to_string(),
        FieldValue::Int(value) => value.to_string(),
        FieldValue::Uint(value) => value.to_string(),
        FieldValue::FormKey(form_key) => form_key.format(interner),
        _ => "structured".to_string(),
    }
}

fn valid_form_key_text(value: &str) -> bool {
    let Some((local, plugin)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 8
        && local.bytes().all(|byte| byte.is_ascii_hexdigit())
        && !plugin.trim().is_empty()
        && !plugin.contains(['@', '/', '\\'])
}

fn same_form_key(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn structure(interner: &StringInterner, fields: &[(&str, FieldValue)]) -> FieldValue {
        FieldValue::Struct(
            fields
                .iter()
                .map(|(name, value)| (interner.intern(name), value.clone()))
                .collect(),
        )
    }

    fn evidence(kind: SkyrimPackBlueprintKind) -> SkyrimPackProcedureEvidence {
        SkyrimPackProcedureEvidence {
            family: SkyrimPackProcedureFamily::Travel,
            source_template: "012345@Skyrim.esm".to_string(),
            target_template: "002CB0@Fallout4.esm".to_string(),
            kind,
            evidence_id: "pack-blueprint-1".to_string(),
            source_blueprint_blake3: "1".repeat(64),
            target_blueprint_blake3: "2".repeat(64),
        }
    }

    fn patrol_evidence() -> SkyrimPackProcedureEvidence {
        SkyrimPackProcedureEvidence {
            family: SkyrimPackProcedureFamily::Patrol,
            source_template: "012345@Skyrim.esm".to_string(),
            target_template: "002CE0@Fallout4.esm".to_string(),
            kind: SkyrimPackBlueprintKind::ExactSemanticBlueprint,
            evidence_id: "pack-patrol-blueprint-1".to_string(),
            source_blueprint_blake3: "5".repeat(64),
            target_blueprint_blake3: "6".repeat(64),
        }
    }

    fn materialization_evidence(
        procedure: &SkyrimPackProcedureEvidence,
    ) -> SkyrimPackTemplateMaterializationEvidence {
        SkyrimPackTemplateMaterializationEvidence {
            family: procedure.family,
            blueprint: match procedure.family {
                SkyrimPackProcedureFamily::Travel => SkyrimPackFo4TemplateBlueprint::TravelV1,
                SkyrimPackProcedureFamily::Patrol => SkyrimPackFo4TemplateBlueprint::PatrolV2,
            },
            evidence_id: format!("{}-materialization", procedure.evidence_id),
            target_template: procedure.target_template.clone(),
            target_blueprint_blake3: procedure.target_blueprint_blake3.clone(),
        }
    }

    fn source_pack(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode(*b"PACK"),
            FormKey::parse("000111@Skyrim.esm", interner).unwrap(),
        );
        record.eid = Some(interner.intern("B21_TestTravel"));
        record.fields = SmallVec::from_vec(vec![
            field(
                "PKDT",
                structure(
                    interner,
                    &[
                        ("general_flags", FieldValue::Uint(0x120)),
                        ("type", FieldValue::Uint(18)),
                        ("interrupt_override", FieldValue::Uint(2)),
                        ("preferred_speed", FieldValue::Uint(1)),
                        ("unknown_u8_4", FieldValue::Uint(0)),
                        ("interrupt_flags", FieldValue::Uint(0x41)),
                        ("unknown_u8_6", FieldValue::Uint(0)),
                        ("unknown_u8_7", FieldValue::Uint(0)),
                    ],
                ),
            ),
            field(
                "PSDT",
                structure(
                    interner,
                    &[
                        ("month", FieldValue::Int(-1)),
                        ("day_of_week", FieldValue::Int(-1)),
                        ("date", FieldValue::Int(-1)),
                        ("hour", FieldValue::Int(8)),
                        ("minute", FieldValue::Int(30)),
                        ("unknown_u8_5", FieldValue::Uint(0)),
                        ("unknown_u8_6", FieldValue::Uint(0)),
                        ("unknown_u8_7", FieldValue::Uint(0)),
                        ("duration_hours", FieldValue::Uint(4)),
                    ],
                ),
            ),
            field(
                "PKCU",
                structure(
                    interner,
                    &[
                        ("data_input_count", FieldValue::Uint(1)),
                        (
                            "package_template",
                            FieldValue::FormKey(
                                FormKey::parse("012345@Skyrim.esm", interner).unwrap(),
                            ),
                        ),
                        ("version_counter_autoincremented", FieldValue::Uint(7)),
                    ],
                ),
            ),
            field(
                "PLDT",
                structure(
                    interner,
                    &[
                        ("type", FieldValue::Int(12)),
                        ("location_value", FieldValue::Uint(0)),
                        ("radius", FieldValue::Int(384)),
                    ],
                ),
            ),
        ]);
        record
    }

    fn request() -> SkyrimPackLoweringRequest {
        SkyrimPackLoweringRequest {
            component_id: "sky-quest-1".to_string(),
            quest_component_admitted: true,
            source_owned_by_component: true,
            target_record: "000800@Output.esp".to_string(),
            script_projections: Vec::new(),
        }
    }

    fn source_patrol(interner: &StringInterner) -> Record {
        let mut record = source_pack(interner);
        record.eid = Some(interner.intern("B21_TestPatrol"));
        record.fields.truncate(3);
        let counter = struct_fields(&record.fields[2].value, "PKCU").unwrap();
        let template = named(counter, "package_template", interner)
            .unwrap()
            .clone();
        record.fields[2].value = structure(
            interner,
            &[
                ("data_input_count", FieldValue::Uint(6)),
                ("package_template", template),
                ("version_counter_autoincremented", FieldValue::Uint(3)),
            ],
        );
        record.fields.extend([
            field("ANAM", FieldValue::String(interner.intern("SingleRef"))),
            field(
                "PTDA",
                structure(
                    interner,
                    &[
                        ("target_data_type", FieldValue::Int(0)),
                        (
                            "target_data_target",
                            FieldValue::FormKey(
                                FormKey::parse("000222@Skyrim.esm", interner).unwrap(),
                            ),
                        ),
                        ("target_data_count_distance", FieldValue::Int(0)),
                    ],
                ),
            ),
            field("ANAM", FieldValue::String(interner.intern("Float"))),
            field("CNAM", FieldValue::Float(768.0)),
            field("ANAM", FieldValue::String(interner.intern("Bool"))),
            field("CNAM", FieldValue::Bool(true)),
            field("ANAM", FieldValue::String(interner.intern("Bool"))),
            field("CNAM", FieldValue::Bool(true)),
            field("ANAM", FieldValue::String(interner.intern("Bool"))),
            field("CNAM", FieldValue::Bool(false)),
            field("ANAM", FieldValue::String(interner.intern("Bool"))),
            field("CNAM", FieldValue::Bool(false)),
        ]);
        for index in SKYRIM_PATROL_INPUT_INDICES {
            record.fields.push(field(
                "UNAM",
                FieldValue::Bytes(SmallVec::from_slice(&[index])),
            ));
        }
        record.fields.push(field(
            "XNAM",
            FieldValue::Bytes(SmallVec::from_slice(&[0x09])),
        ));
        record
    }

    fn save_reopen(record: Record, target: FormKey, interner: &StringInterner) -> Record {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_load_no_py, plugin_handle_new_native,
            plugin_handle_save_no_py, plugin_handle_store_ref,
        };

        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let handle = plugin_handle_new_native("Output.esp", Some("fo4")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store.get_mut(&handle).unwrap().parsed.header.masters = vec!["Fallout4.esm".into()];
        }
        crate::target_write::add_record_native(handle, record, &schema, interner).unwrap();
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Output.esp");
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        assert!(plugin_handle_close_native(handle));
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let decoded = crate::source_read::read_record_relayout_by_form_key(
            reopened, &target, &schema, interner, None,
        )
        .unwrap();
        assert!(plugin_handle_close_native(reopened));
        decoded
    }

    #[test]
    fn exact_current_location_travel_preserves_semantics_and_receipt() {
        let interner = StringInterner::new();
        let classification = classify_skyrim_pack(
            &source_pack(&interner),
            &evidence(SkyrimPackBlueprintKind::ExactSemanticBlueprint),
            &interner,
        )
        .unwrap();
        let lowering = lower_skyrim_pack(&classification, &request()).unwrap();
        assert_eq!(lowering.target.flags.general, 0x120);
        assert_eq!(lowering.target.schedule.hour, 8);
        assert_eq!(lowering.target.schedule.minute, 30);
        assert_eq!(
            lowering.target.locations[0].selector,
            SkyrimPackCurrentLocation::NearSelf
        );
        assert_eq!(lowering.target.locations[0].radius, 384);
        assert!(!lowering.target.universal_fallback_used);
        verify_skyrim_pack_receipt(&classification, &lowering.target, &lowering.receipt).unwrap();
    }

    #[test]
    fn travel_materialization_save_reopen_has_strict_record_receipt() {
        let interner = StringInterner::new();
        let procedure = evidence(SkyrimPackBlueprintKind::ExactSemanticBlueprint);
        let classification =
            classify_skyrim_pack(&source_pack(&interner), &procedure, &interner).unwrap();
        let lowering = lower_skyrim_pack(&classification, &request()).unwrap();
        let source = FormKey::parse("000111@Skyrim.esm", &interner).unwrap();
        let target = FormKey::parse("000800@Output.esp", &interner).unwrap();
        let template = FormKey::parse("002CB0@Fallout4.esm", &interner).unwrap();
        let template_evidence = materialization_evidence(&procedure);
        let materialized = materialize_skyrim_pack(
            &lowering,
            source,
            target,
            template,
            &BTreeMap::new(),
            &template_evidence,
            &interner,
        )
        .unwrap();
        assert!(!materialized.receipt.vmad_attached);
        let reopened = save_reopen(materialized.record, target, &interner);
        let reopened_receipt = receipt_from_materialized_record(
            &lowering,
            &reopened,
            source,
            target,
            template,
            &BTreeMap::new(),
            &template_evidence,
            &interner,
        )
        .unwrap();
        assert_eq!(reopened_receipt, materialized.receipt);
    }

    #[test]
    fn exact_patrol_materializes_mapped_path_and_tamper_fails_closed() {
        let interner = StringInterner::new();
        let procedure = patrol_evidence();
        let classification =
            classify_skyrim_pack(&source_patrol(&interner), &procedure, &interner).unwrap();
        let lowering = lower_skyrim_pack(&classification, &request()).unwrap();
        let source = FormKey::parse("000111@Skyrim.esm", &interner).unwrap();
        let target = FormKey::parse("000800@Output.esp", &interner).unwrap();
        let template = FormKey::parse("002CE0@Fallout4.esm", &interner).unwrap();
        let mapped_path = FormKey::parse("000900@Output.esp", &interner).unwrap();
        let mappings = BTreeMap::from([("000222@Skyrim.esm".to_string(), mapped_path)]);
        let template_evidence = materialization_evidence(&procedure);
        let materialized = materialize_skyrim_pack(
            &lowering,
            source,
            target,
            template,
            &mappings,
            &template_evidence,
            &interner,
        )
        .unwrap();
        assert_eq!(
            materialized.receipt.mapped_references,
            BTreeMap::from([(
                "000222@Skyrim.esm".to_string(),
                "000900@Output.esp".to_string()
            )])
        );
        let mut reopened = save_reopen(materialized.record, target, &interner);
        verify_skyrim_pack_record_receipt(
            &lowering,
            &reopened,
            source,
            target,
            template,
            &mappings,
            &template_evidence,
            &materialized.receipt,
            &interner,
        )
        .unwrap();
        let ptda = reopened
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "PTDA")
            .unwrap();
        ptda.value = pack_struct(
            &interner,
            [
                ("target_data_type", FieldValue::Int(0)),
                (
                    "target_data_target",
                    FieldValue::FormKey(FormKey::parse("000901@Output.esp", &interner).unwrap()),
                ),
                ("target_data_count_distance", FieldValue::Int(0)),
            ],
        );
        assert!(matches!(
            receipt_from_materialized_record(
                &lowering,
                &reopened,
                source,
                target,
                template,
                &mappings,
                &template_evidence,
                &interner,
            ),
            Err(SkyrimPackRejectionReason::InvalidMaterializedRecord { .. })
        ));
    }

    #[test]
    fn patrol_requires_reference_mapping_and_exact_source_abi() {
        let interner = StringInterner::new();
        let procedure = patrol_evidence();
        let mut source_record = source_patrol(&interner);
        let classification = classify_skyrim_pack(&source_record, &procedure, &interner).unwrap();
        let lowering = lower_skyrim_pack(&classification, &request()).unwrap();
        assert_eq!(
            materialize_skyrim_pack(
                &lowering,
                FormKey::parse("000111@Skyrim.esm", &interner).unwrap(),
                FormKey::parse("000800@Output.esp", &interner).unwrap(),
                FormKey::parse("002CE0@Fallout4.esm", &interner).unwrap(),
                &BTreeMap::new(),
                &materialization_evidence(&procedure),
                &interner,
            )
            .unwrap_err(),
            SkyrimPackRejectionReason::MissingReferenceMapping {
                source_form_key: "000222@Skyrim.esm".to_string()
            }
        );
        source_record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "XNAM")
            .unwrap()
            .value = FieldValue::Bytes(SmallVec::from_slice(&[0x08]));
        assert_eq!(
            classify_skyrim_pack(&source_record, &procedure, &interner).unwrap_err(),
            SkyrimPackRejectionReason::UnsupportedPatrolShape
        );
    }

    #[test]
    fn unsupported_shape_rejects_conditions_targets_and_phases() {
        let interner = StringInterner::new();
        let mut conditioned = source_pack(&interner);
        conditioned
            .fields
            .push(field("CTDA", FieldValue::Bytes(SmallVec::new())));
        assert!(matches!(
            classify_skyrim_pack(
                &conditioned,
                &evidence(SkyrimPackBlueprintKind::ExactSemanticBlueprint),
                &interner
            ),
            Err(SkyrimPackRejectionReason::ConditionsRequireSemanticLowering { count: 1 })
        ));

        let mut targeted = source_pack(&interner);
        targeted.fields.push(field(
            "PTDA",
            structure(
                &interner,
                &[
                    ("type", FieldValue::Int(6)),
                    ("target", FieldValue::Uint(0)),
                    ("count_distance", FieldValue::Int(0)),
                ],
            ),
        ));
        assert!(matches!(
            classify_skyrim_pack(
                &targeted,
                &evidence(SkyrimPackBlueprintKind::ExactSemanticBlueprint),
                &interner
            ),
            Err(SkyrimPackRejectionReason::UnsupportedTarget { selector_type: 6 })
        ));

        let mut phased = source_pack(&interner);
        phased.fields.push(field("PRCB", FieldValue::Uint(1)));
        assert!(matches!(
            classify_skyrim_pack(
                &phased,
                &evidence(SkyrimPackBlueprintKind::ExactSemanticBlueprint),
                &interner
            ),
            Err(SkyrimPackRejectionReason::ProcedurePhasesRequireSemanticLowering { .. })
        ));
    }

    #[test]
    fn quest_owned_pack_cannot_use_the_universal_safe_travel_fallback() {
        let interner = StringInterner::new();
        assert_eq!(
            classify_skyrim_pack(
                &source_pack(&interner),
                &evidence(SkyrimPackBlueprintKind::GenericSafeTravelFallback),
                &interner,
            )
            .unwrap_err(),
            SkyrimPackRejectionReason::GenericFallbackForbidden
        );
    }

    #[test]
    fn scripts_require_fresh_compiler_entrypoint_and_vmad_evidence() {
        let interner = StringInterner::new();
        let mut source = source_pack(&interner);
        source.fields.push(field("POBA", FieldValue::None));
        source.fields.push(field(
            "SCTX",
            FieldValue::Bytes(SmallVec::from_slice(b"begin")),
        ));
        let classification = classify_skyrim_pack(
            &source,
            &evidence(SkyrimPackBlueprintKind::ExactSemanticBlueprint),
            &interner,
        )
        .unwrap();
        assert_eq!(
            lower_skyrim_pack(&classification, &request()).unwrap_err(),
            SkyrimPackRejectionReason::MissingCompilerEvidence {
                event: SkyrimPackScriptEvent::OnBegin
            }
        );

        let root = tempfile::tempdir().unwrap();
        let compiled = papyrus_core::compiler::compile_source(
            "ScriptName B21_PF_000111 Extends Package\nFunction Fragment_0()\nEndFunction\n",
            &[],
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(
            compiled.ok,
            "compiler diagnostics: {:?}",
            compiled.diagnostics
        );
        let pex_bytes = compiled.pex_bytes.unwrap();
        let pex_artifact_path = root.path().join("data/Scripts/B21_PF_000111.pex");
        std::fs::create_dir_all(pex_artifact_path.parent().unwrap()).unwrap();
        std::fs::write(&pex_artifact_path, &pex_bytes).unwrap();

        let mut scripted = request();
        scripted
            .script_projections
            .push(SkyrimPackScriptProjection {
                event: SkyrimPackScriptEvent::OnBegin,
                entrypoint: "Fragment_0".to_string(),
                semantics_evidence_id: "semantic-port-1".to_string(),
                vmad_payload_blake3: "3".repeat(64),
                compiler: SkyrimPackCompilerEvidence {
                    manifest_id: "manifest-1".to_string(),
                    class_name: "B21_PF_000111".to_string(),
                    source_blake3: "4".repeat(64),
                    relative_pex_path: "data/Scripts/B21_PF_000111.pex".to_string(),
                    pex_artifact_path,
                    pex_blake3: blake3::hash(&pex_bytes).to_hex().to_string(),
                    compile_run_id: "compile-1".to_string(),
                    target_game: "fo4".to_string(),
                    compiled_success: true,
                    fresh_output: true,
                },
            });
        let lowering = lower_skyrim_pack(&classification, &scripted).unwrap();
        assert_eq!(
            lowering.receipt.compiler_evidence_ids,
            BTreeSet::from(["compile-1".to_string()])
        );
        let procedure = evidence(SkyrimPackBlueprintKind::ExactSemanticBlueprint);
        let materialized = materialize_skyrim_pack(
            &lowering,
            FormKey::parse("000111@Skyrim.esm", &interner).unwrap(),
            FormKey::parse("000800@Output.esp", &interner).unwrap(),
            FormKey::parse("002CB0@Fallout4.esm", &interner).unwrap(),
            &BTreeMap::new(),
            &materialization_evidence(&procedure),
            &interner,
        )
        .unwrap();
        assert_eq!(materialized.pending_script_projections.len(), 1);
        assert_eq!(
            materialized.receipt.pending_script_events,
            BTreeSet::from([SkyrimPackScriptEvent::OnBegin])
        );
        assert_eq!(
            materialized.receipt.pending_vmad_payload_blake3,
            BTreeSet::from(["3".repeat(64)])
        );
        assert!(
            materialized
                .record
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "VMAD")
        );

        let mut stale = scripted;
        stale.script_projections[0].compiler.fresh_output = false;
        assert!(matches!(
            lower_skyrim_pack(&classification, &stale),
            Err(SkyrimPackRejectionReason::InvalidCompilerEvidence {
                event: SkyrimPackScriptEvent::OnBegin
            })
        ));
    }

    #[test]
    fn strict_receipt_detects_semantic_substitution() {
        let interner = StringInterner::new();
        let classification = classify_skyrim_pack(
            &source_pack(&interner),
            &evidence(SkyrimPackBlueprintKind::ExactSemanticBlueprint),
            &interner,
        )
        .unwrap();
        let mut lowering = lower_skyrim_pack(&classification, &request()).unwrap();
        lowering.receipt.flags.general = 0;
        assert_eq!(
            verify_skyrim_pack_receipt(&classification, &lowering.target, &lowering.receipt),
            Err(SkyrimPackRejectionReason::ReceiptMismatch {
                field: "flags".to_string()
            })
        );
    }
}
