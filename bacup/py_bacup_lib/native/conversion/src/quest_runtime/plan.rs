use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::QuestRuntimeExpectedReceipt;

pub const PRODUCTION_EMISSION_ENABLED: bool = false;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QuestRecordKey {
    pub signature: String,
    pub form_key: String,
}

impl QuestRecordKey {
    pub fn new(signature: impl Into<String>, form_key: impl Into<String>) -> Self {
        Self {
            signature: signature.into().to_ascii_uppercase(),
            form_key: form_key.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestSourceGame {
    SkyrimSe,
    Fnv,
    Fo3,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GraftProvenance {
    pub source_plugin: String,
    pub source_form_key: String,
    pub graft_form_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub game: QuestSourceGame,
    pub source_plugin: String,
    pub graft: Option<GraftProvenance>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyClosure {
    pub direct: BTreeSet<QuestRecordKey>,
    pub recursive: BTreeSet<QuestRecordKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TopologyPlacement {
    pub record: QuestRecordKey,
    pub group_path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceTargetFormMapping {
    pub source: QuestRecordKey,
    pub target: QuestRecordKey,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LocalizedTextIntent {
    pub field: String,
    pub source_text_id: Option<u32>,
    pub text: String,
    pub target_table: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConditionIntent {
    pub function: String,
    pub operator: String,
    pub comparison_value: String,
    pub parameters: Vec<String>,
    pub run_on: String,
    pub cis1: Option<String>,
    pub cis2: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QuestStageIntent {
    pub index: u16,
    pub log_entries: Vec<LocalizedTextIntent>,
    pub fragment_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QuestObjectiveIntent {
    pub index: u16,
    pub display_text: LocalizedTextIntent,
    pub target_aliases: BTreeSet<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum AliasFillKind {
    ForcedReference(String),
    UniqueActor(String),
    CreatedReference(String),
    Location(String),
    ExternalAlias { quest: String, alias_id: u32 },
    PairSpecific(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QuestAliasIntent {
    pub id: u32,
    pub name: String,
    pub fill: AliasFillKind,
    pub conditions: Vec<ConditionIntent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestStartFlag {
    StartGameEnabled,
    StartsEnabled,
    RunOnce,
    RepeatableStages,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestSemanticPlan {
    pub stages: Vec<QuestStageIntent>,
    pub objectives: Vec<QuestObjectiveIntent>,
    pub aliases: Vec<QuestAliasIntent>,
    pub conditions: Vec<ConditionIntent>,
    pub start_flags: BTreeSet<QuestStartFlag>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScriptIntent {
    pub owner: QuestRecordKey,
    pub source_name: String,
    pub target_class: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FragmentIntent {
    pub owner: QuestRecordKey,
    pub fragment_id: String,
    pub entrypoint: String,
    pub target_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompilerEvidenceIntent {
    pub manifest_id: String,
    pub source_digest: String,
    pub require_fresh_output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PscIntent {
    pub class_name: String,
    pub source_artifact: String,
    pub compiler_evidence_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_evidence: Option<CompilerEvidenceIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VmadPropertyIntent {
    pub property_name: String,
    pub property_type: String,
    pub target_value: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VmadIntent {
    pub owner: QuestRecordKey,
    pub script_class: String,
    pub properties: BTreeSet<VmadPropertyIntent>,
    pub compiler_evidence_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_evidence: Option<CompilerEvidenceIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AssetIntent {
    pub kind: String,
    pub source_path: String,
    pub target_path: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StartProducerIntent {
    pub producer_id: String,
    pub carrier: QuestRecordKey,
    pub producer_kind: String,
    pub evidence_id: String,
    pub proven: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StartDisposition {
    Autostart,
    ExplicitScript { producer_id: String },
    InfoResultScript { producer_id: String },
    StoryManager { route_id: String },
    Controller { producer_id: String },
    OrphanStartUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestRuntimeRejectionReason {
    MissingProvenance,
    IncompleteClosure,
    DuplicateOwnership,
    MissingStartDisposition,
    OrphanStartUnknown,
    UnsupportedCondition,
    UnsupportedAliasFill,
    UnsupportedScript,
    MissingCompilerEvidence,
    MissingStartProducer,
    UnsupportedTopology,
    RequiredAssetUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "result")]
pub enum QuestRuntimeAdmission {
    Supported,
    Rejected {
        reasons: BTreeSet<QuestRuntimeRejectionReason>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestRuntimeComponentPlan {
    pub component_id: String,
    pub root_quest: QuestRecordKey,
    pub provenance: SourceProvenance,
    pub owned_records: Vec<QuestRecordKey>,
    pub shared_records: BTreeSet<QuestRecordKey>,
    pub dependencies: DependencyClosure,
    pub source_topology: BTreeSet<TopologyPlacement>,
    pub target_topology: BTreeSet<TopologyPlacement>,
    pub mappings: Vec<SourceTargetFormMapping>,
    pub semantics: QuestSemanticPlan,
    pub scripts: BTreeSet<ScriptIntent>,
    pub fragments: BTreeSet<FragmentIntent>,
    pub psc: BTreeSet<PscIntent>,
    pub vmad: BTreeSet<VmadIntent>,
    pub assets: BTreeSet<AssetIntent>,
    pub inbound_producers: BTreeSet<StartProducerIntent>,
    pub start_disposition: Option<StartDisposition>,
    pub admission: QuestRuntimeAdmission,
    pub expected_receipt: QuestRuntimeExpectedReceipt,
}

impl QuestRuntimeComponentPlan {
    pub const fn production_emission_enabled(&self) -> bool {
        PRODUCTION_EMISSION_ENABLED
    }

    pub fn validation_issues(&self) -> Vec<PlanValidationIssue> {
        validate_component_plans(std::slice::from_ref(self))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanValidationCode {
    InvalidRootQuest,
    InvalidSourcePlugin,
    InvalidGraftProvenance,
    RootQuestNotOwned,
    DuplicateOwnership,
    OwnedAndShared,
    DuplicateSourceMapping,
    UndeclaredRecordReference,
    UnmappedDeclaredRecord,
    UnmappedTargetTopology,
    UnmappedExpectedRecord,
    MissingRequiredPscIntent,
    MissingFreshCompilerEvidence,
    MissingProvenStartProducer,
    InconsistentStartDisposition,
    MissingStartDisposition,
    EmptyRejectionReasons,
    SupportedOrphanStart,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlanValidationIssue {
    pub code: PlanValidationCode,
    pub component_id: String,
    pub record: Option<QuestRecordKey>,
    pub conflicting_component_id: Option<String>,
    pub subject: Option<String>,
}

pub fn validate_component_plans(plans: &[QuestRuntimeComponentPlan]) -> Vec<PlanValidationIssue> {
    let mut issues = BTreeSet::new();
    let mut owners: BTreeMap<&QuestRecordKey, &str> = BTreeMap::new();

    for plan in plans {
        if plan.root_quest.signature != "QUST" {
            issues.insert(PlanValidationIssue {
                code: PlanValidationCode::InvalidRootQuest,
                component_id: plan.component_id.clone(),
                record: Some(plan.root_quest.clone()),
                conflicting_component_id: None,
                subject: None,
            });
        }
        if !plan.owned_records.contains(&plan.root_quest) {
            issues.insert(PlanValidationIssue {
                code: PlanValidationCode::RootQuestNotOwned,
                component_id: plan.component_id.clone(),
                record: Some(plan.root_quest.clone()),
                conflicting_component_id: None,
                subject: None,
            });
        }
        if plan.start_disposition.is_none() {
            issues.insert(PlanValidationIssue {
                code: PlanValidationCode::MissingStartDisposition,
                component_id: plan.component_id.clone(),
                record: Some(plan.root_quest.clone()),
                conflicting_component_id: None,
                subject: None,
            });
        }
        if matches!(
            plan.start_disposition,
            Some(StartDisposition::OrphanStartUnknown)
        ) && matches!(plan.admission, QuestRuntimeAdmission::Supported)
        {
            issues.insert(PlanValidationIssue {
                code: PlanValidationCode::SupportedOrphanStart,
                component_id: plan.component_id.clone(),
                record: Some(plan.root_quest.clone()),
                conflicting_component_id: None,
                subject: None,
            });
        }
        if matches!(
            &plan.admission,
            QuestRuntimeAdmission::Rejected { reasons } if reasons.is_empty()
        ) {
            issues.insert(PlanValidationIssue {
                code: PlanValidationCode::EmptyRejectionReasons,
                component_id: plan.component_id.clone(),
                record: Some(plan.root_quest.clone()),
                conflicting_component_id: None,
                subject: None,
            });
        }

        for record in &plan.owned_records {
            if plan.shared_records.contains(record) {
                issues.insert(PlanValidationIssue {
                    code: PlanValidationCode::OwnedAndShared,
                    component_id: plan.component_id.clone(),
                    record: Some(record.clone()),
                    conflicting_component_id: None,
                    subject: None,
                });
            }
            if let Some(existing) = owners.insert(record, &plan.component_id) {
                issues.insert(PlanValidationIssue {
                    code: PlanValidationCode::DuplicateOwnership,
                    component_id: plan.component_id.clone(),
                    record: Some(record.clone()),
                    conflicting_component_id: Some(existing.to_owned()),
                    subject: None,
                });
            }
        }

        if !matches!(plan.admission, QuestRuntimeAdmission::Supported) {
            continue;
        }

        validate_supported_plan(plan, &mut issues);
    }

    issues.into_iter().collect()
}

fn validate_supported_plan(
    plan: &QuestRuntimeComponentPlan,
    issues: &mut BTreeSet<PlanValidationIssue>,
) {
    if !valid_plugin_name(&plan.provenance.source_plugin) {
        insert_issue(
            issues,
            plan,
            PlanValidationCode::InvalidSourcePlugin,
            None,
            Some(plan.provenance.source_plugin.clone()),
        );
    }
    if plan.provenance.graft.as_ref().is_some_and(|graft| {
        !valid_plugin_name(&graft.source_plugin)
            || graft.source_form_key.trim().is_empty()
            || graft.graft_form_key.trim().is_empty()
    }) {
        insert_issue(
            issues,
            plan,
            PlanValidationCode::InvalidGraftProvenance,
            None,
            Some("graft".to_owned()),
        );
    }

    let declared = plan
        .owned_records
        .iter()
        .chain(&plan.shared_records)
        .chain(&plan.dependencies.direct)
        .chain(&plan.dependencies.recursive)
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut mapped_sources = BTreeSet::new();
    let mut mapped_targets = BTreeSet::new();
    for mapping in &plan.mappings {
        if !mapped_sources.insert(mapping.source.clone()) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::DuplicateSourceMapping,
                Some(mapping.source.clone()),
                None,
            );
        }
        mapped_targets.insert(mapping.target.clone());
        if !declared.contains(&mapping.source) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UndeclaredRecordReference,
                Some(mapping.source.clone()),
                Some("mapping_source".to_owned()),
            );
        }
    }
    for record in declared.difference(&mapped_sources) {
        insert_issue(
            issues,
            plan,
            PlanValidationCode::UnmappedDeclaredRecord,
            Some(record.clone()),
            None,
        );
    }
    for placement in &plan.source_topology {
        if !declared.contains(&placement.record) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UndeclaredRecordReference,
                Some(placement.record.clone()),
                Some("source_topology".to_owned()),
            );
        }
    }
    for placement in &plan.target_topology {
        if !mapped_targets.contains(&placement.record) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UnmappedTargetTopology,
                Some(placement.record.clone()),
                None,
            );
        }
    }
    for record in &plan.expected_receipt.emitted_records {
        if !mapped_targets.contains(record) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UnmappedExpectedRecord,
                Some(record.clone()),
                None,
            );
        }
    }
    for placement in &plan.expected_receipt.placements {
        if !mapped_targets.contains(&placement.record) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UnmappedExpectedRecord,
                Some(placement.record.clone()),
                Some("expected_placement".to_owned()),
            );
        }
    }
    for localized in &plan.expected_receipt.localized_strings {
        if !mapped_targets.contains(&localized.owner) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UnmappedExpectedRecord,
                Some(localized.owner.clone()),
                Some("expected_localized_string".to_owned()),
            );
        }
    }
    for attachment in &plan.expected_receipt.vmad_attachments {
        if !mapped_targets.contains(&attachment.owner) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UnmappedExpectedRecord,
                Some(attachment.owner.clone()),
                Some("expected_vmad_attachment".to_owned()),
            );
        }
    }
    for route in &plan.expected_receipt.routes {
        for record in std::iter::once(&route.quest).chain(&route.node_chain) {
            if !mapped_targets.contains(record) {
                insert_issue(
                    issues,
                    plan,
                    PlanValidationCode::UnmappedExpectedRecord,
                    Some(record.clone()),
                    Some("expected_start_route".to_owned()),
                );
            }
        }
    }

    for (record, subject) in plan
        .scripts
        .iter()
        .map(|intent| (&intent.owner, "script_owner"))
        .chain(
            plan.fragments
                .iter()
                .map(|intent| (&intent.owner, "fragment_owner")),
        )
        .chain(
            plan.inbound_producers
                .iter()
                .map(|intent| (&intent.carrier, "start_producer_carrier")),
        )
    {
        if !declared.contains(record) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UndeclaredRecordReference,
                Some(record.clone()),
                Some(subject.to_owned()),
            );
        }
    }
    for intent in &plan.vmad {
        if !declared.contains(&intent.owner) && !mapped_targets.contains(&intent.owner) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::UndeclaredRecordReference,
                Some(intent.owner.clone()),
                Some("vmad_owner".to_owned()),
            );
        }
    }

    for intent in &plan.psc {
        if intent.compiler_evidence_required && !fresh_evidence(&intent.compiler_evidence) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::MissingFreshCompilerEvidence,
                None,
                Some(format!("psc:{}", intent.class_name)),
            );
        }
    }
    for intent in &plan.vmad {
        if intent.compiler_evidence_required && !fresh_evidence(&intent.compiler_evidence) {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::MissingFreshCompilerEvidence,
                Some(intent.owner.clone()),
                Some(format!("vmad:{}", intent.script_class)),
            );
        }
        if intent.compiler_evidence_required
            && !plan
                .psc
                .iter()
                .any(|psc| psc.class_name == intent.script_class && fresh_required_psc(psc))
        {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::MissingRequiredPscIntent,
                Some(intent.owner.clone()),
                Some(intent.script_class.clone()),
            );
        }
    }
    for (class_name, owner) in plan
        .scripts
        .iter()
        .filter(|intent| intent.required)
        .map(|intent| (&intent.target_class, &intent.owner))
        .chain(
            plan.fragments
                .iter()
                .map(|intent| (&intent.target_class, &intent.owner)),
        )
    {
        if !plan
            .psc
            .iter()
            .any(|psc| &psc.class_name == class_name && fresh_required_psc(psc))
        {
            insert_issue(
                issues,
                plan,
                PlanValidationCode::MissingRequiredPscIntent,
                Some(owner.clone()),
                Some(class_name.clone()),
            );
        }
    }

    validate_start_route(plan, issues);
}

fn validate_start_route(
    plan: &QuestRuntimeComponentPlan,
    issues: &mut BTreeSet<PlanValidationIssue>,
) {
    let start_game_enabled = plan
        .semantics
        .start_flags
        .contains(&QuestStartFlag::StartGameEnabled);
    let Some(disposition) = &plan.start_disposition else {
        return;
    };
    let producer_id = match disposition {
        StartDisposition::Autostart => {
            if !start_game_enabled {
                insert_issue(
                    issues,
                    plan,
                    PlanValidationCode::InconsistentStartDisposition,
                    Some(plan.root_quest.clone()),
                    Some("autostart_without_start_game_enabled".to_owned()),
                );
            }
            return;
        }
        StartDisposition::ExplicitScript { producer_id }
        | StartDisposition::InfoResultScript { producer_id }
        | StartDisposition::Controller { producer_id } => producer_id,
        StartDisposition::StoryManager { route_id } => route_id,
        StartDisposition::OrphanStartUnknown => return,
    };

    if start_game_enabled {
        insert_issue(
            issues,
            plan,
            PlanValidationCode::InconsistentStartDisposition,
            Some(plan.root_quest.clone()),
            Some("non_autostart_with_start_game_enabled".to_owned()),
        );
    }
    let proven = plan.inbound_producers.iter().any(|producer| {
        producer.producer_id == *producer_id
            && producer.proven
            && !producer.producer_kind.trim().is_empty()
            && !producer.evidence_id.trim().is_empty()
    });
    if !proven {
        insert_issue(
            issues,
            plan,
            PlanValidationCode::MissingProvenStartProducer,
            Some(plan.root_quest.clone()),
            Some(producer_id.clone()),
        );
    }
}

fn valid_plugin_name(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains(['/', '\\', ':']) {
        return false;
    }
    let lowercase = trimmed.to_ascii_lowercase();
    [".esm", ".esp", ".esl"].iter().any(|extension| {
        lowercase
            .strip_suffix(extension)
            .is_some_and(|stem| !stem.is_empty())
    })
}

fn fresh_required_psc(intent: &PscIntent) -> bool {
    intent.compiler_evidence_required && fresh_evidence(&intent.compiler_evidence)
}

fn fresh_evidence(intent: &Option<CompilerEvidenceIntent>) -> bool {
    intent.as_ref().is_some_and(|intent| {
        intent.require_fresh_output
            && !intent.manifest_id.trim().is_empty()
            && !intent.source_digest.trim().is_empty()
    })
}

fn insert_issue(
    issues: &mut BTreeSet<PlanValidationIssue>,
    plan: &QuestRuntimeComponentPlan,
    code: PlanValidationCode,
    record: Option<QuestRecordKey>,
    subject: Option<String>,
) {
    issues.insert(PlanValidationIssue {
        code,
        component_id: plan.component_id.clone(),
        record,
        conflicting_component_id: None,
        subject,
    });
}
