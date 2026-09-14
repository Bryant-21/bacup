use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::quest_runtime::{
    ConditionIntent, QuestRecordKey, QuestRuntimeAdmission, QuestRuntimeComponentPlan,
    StartDisposition, StoryEventRootIntent, StoryGraphNode, StoryGraphPlan, StoryGraphRouteReceipt,
    StoryNodeKind, StoryRouteIntent, validate_story_graph,
};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::{StringInterner, Sym};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimFo4StoryEvent {
    HackComputer,
    KillActor,
}

impl SkyrimFo4StoryEvent {
    pub(crate) fn from_code(code: u32) -> Result<Self, String> {
        match code.to_le_bytes() {
            value if value == *b"HACK" => Ok(Self::HackComputer),
            value if value == *b"KILL" => Ok(Self::KillActor),
            _ => Err(format!(
                "unsupported Skyrim Story Manager event type {code:08X}"
            )),
        }
    }

    pub(crate) fn code(self) -> u32 {
        let code = match self {
            Self::HackComputer => *b"HACK",
            Self::KillActor => *b"KILL",
        };
        u32::from_le_bytes(code)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::HackComputer => "HACK",
            Self::KillActor => "KILL",
        }
    }

    pub(crate) fn producer_kind(self) -> &'static str {
        "native_story_event"
    }

    pub(crate) fn canonical_fo4_root(self) -> Option<(u32, &'static str)> {
        match self {
            Self::HackComputer => Some((0x1244D0, "HackComputer")),
            Self::KillActor => None,
        }
    }

    pub(crate) fn unsupported_reason(self) -> String {
        format!(
            "unsupported Skyrim Story Manager event {}: Fallout 4 has no canonical SMEN root",
            self.name()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimStoryEventMapping {
    pub event: SkyrimFo4StoryEvent,
    pub target_event_root: QuestRecordKey,
    pub producer_id: String,
}

#[derive(Debug, Clone)]
pub struct SkyrimStoryManagerProjection {
    pub graph: StoryGraphPlan,
    pub records: Vec<Record>,
    pub route_receipt: StoryGraphRouteReceipt,
}

#[derive(Debug, Clone)]
struct ClassifiedNode {
    source: QuestRecordKey,
    target: QuestRecordKey,
    source_record: Record,
    parent: Option<QuestRecordKey>,
    children: BTreeSet<QuestRecordKey>,
    conditions: Vec<LoweredCondition>,
}

#[derive(Debug, Clone)]
struct LoweredCondition {
    intent: ConditionIntent,
    target_value: FieldValue,
    cis1: Option<String>,
    cis2: Option<String>,
}

pub fn project_story_manager_chain(
    source_records: &[Record],
    component: &QuestRuntimeComponentPlan,
    event_mapping: &SkyrimStoryEventMapping,
    interner: &StringInterner,
) -> Result<SkyrimStoryManagerProjection, String> {
    let (canonical_root_local, _) = event_mapping
        .event
        .canonical_fo4_root()
        .ok_or_else(|| event_mapping.event.unsupported_reason())?;
    if event_mapping.target_event_root.signature != "SMEN"
        || event_mapping.target_event_root.form_key
            != format!("{canonical_root_local:06X}@Fallout4.esm")
    {
        return Err(format!(
            "Skyrim Story Manager event {} is not mapped to its canonical loaded FO4 SMEN root",
            event_mapping.event.name()
        ));
    }
    validate_component_gate(component)?;
    let source_by_form = unique_source_records(source_records)?;
    let source_quest = component.root_quest.clone();
    let source_quest_record = find_component_record(&source_by_form, &source_quest, interner)?;
    if source_quest_record.sig.as_str() != "QUST" {
        return Err("Skyrim Story Manager component root is not QUST".to_string());
    }
    let target_quest = mapped_key(component, &source_quest)?;
    require_component_topology(component, &source_quest, &target_quest)?;

    let (source_root, source_chain) = classify_linear_chain(
        &source_by_form,
        &source_quest,
        component,
        event_mapping,
        interner,
    )?;
    validate_script_and_vmad_gate(component, &source_quest, &target_quest)?;
    let producer = component
        .inbound_producers
        .iter()
        .find(|producer| producer.producer_id == event_mapping.producer_id)
        .ok_or_else(|| "Skyrim Story Manager route has no matching producer".to_string())?;
    if !producer.proven
        || producer.carrier != source_root
        || producer.producer_kind != event_mapping.event.producer_kind()
        || producer.evidence_id.trim().is_empty()
    {
        return Err("Skyrim Story Manager route producer is not proven".to_string());
    }

    let route_id = match &component.start_disposition {
        Some(StartDisposition::StoryManager { route_id })
            if route_id == &event_mapping.producer_id =>
        {
            route_id.clone()
        }
        _ => {
            return Err("Skyrim Story Manager component has another start disposition".to_string());
        }
    };

    let mut nodes = Vec::with_capacity(source_chain.len());
    for node in &source_chain {
        let kind = match node.source.signature.as_str() {
            "SMEN" => StoryNodeKind::EventRoot,
            "SMBN" => StoryNodeKind::Branch,
            "SMQN" => StoryNodeKind::Quest,
            signature => return Err(format!("unsupported Skyrim Story Manager node {signature}")),
        };
        nodes.push(StoryGraphNode {
            source: node.source.clone(),
            target: node.target.clone(),
            kind,
            parent: node.parent.clone(),
            children: node.children.clone(),
            event_root: (kind == StoryNodeKind::EventRoot).then(|| StoryEventRootIntent {
                event_type: event_mapping.event.name().to_string(),
                target_event_root: event_mapping.target_event_root.clone(),
                supported: true,
                producer_id: event_mapping.producer_id.clone(),
            }),
            quest: (kind == StoryNodeKind::Quest).then(|| source_quest.clone()),
            conditions: node
                .conditions
                .iter()
                .map(|condition| condition.intent.clone())
                .collect(),
        });
    }
    let quest_node = source_chain
        .last()
        .ok_or_else(|| "Skyrim Story Manager chain is empty".to_string())?;
    let graph = StoryGraphPlan {
        graph_id: format!("skyrim-story:{route_id}"),
        root_quest: source_quest.clone(),
        target_quest: target_quest.clone(),
        nodes,
        producers: component.inbound_producers.clone(),
        routes: vec![StoryRouteIntent {
            route_id: route_id.clone(),
            quest: source_quest,
            disposition: StartDisposition::StoryManager {
                route_id: route_id.clone(),
            },
            quest_node: Some(quest_node.source.clone()),
        }],
    };

    let validated = validate_story_graph(&graph).map_err(|issues| {
        let codes = issues
            .iter()
            .map(|issue| format!("{:?}", issue.code))
            .collect::<Vec<_>>()
            .join(",");
        format!("Skyrim Story Manager graph contract rejected the component: {codes}")
    })?;
    let expected_order = source_chain
        .iter()
        .map(|node| node.source.clone())
        .collect::<Vec<_>>();
    if validated.ordered_nodes != expected_order {
        return Err("Skyrim Story Manager graph order is not deterministic root-first".to_string());
    }
    let records = project_target_records(&source_chain, component, interner)?;
    Ok(SkyrimStoryManagerProjection {
        graph,
        records,
        route_receipt: validated.route_receipt,
    })
}

fn validate_component_gate(component: &QuestRuntimeComponentPlan) -> Result<(), String> {
    if !matches!(component.admission, QuestRuntimeAdmission::Supported) {
        return Err("Skyrim Story Manager component is not admitted".to_string());
    }
    if !component.validation_issues().is_empty() {
        return Err("Skyrim Story Manager component contract is invalid".to_string());
    }
    if component.provenance.game != crate::quest_runtime::QuestSourceGame::SkyrimSe {
        return Err("Story Manager source policy received another source game".to_string());
    }
    Ok(())
}

fn validate_script_and_vmad_gate(
    component: &QuestRuntimeComponentPlan,
    source_quest: &QuestRecordKey,
    target_quest: &QuestRecordKey,
) -> Result<(), String> {
    let required_classes = component
        .scripts
        .iter()
        .filter(|script| script.required && script.owner == *source_quest)
        .map(|script| script.target_class.as_str())
        .collect::<BTreeSet<_>>();
    if required_classes.is_empty() {
        return Err("Skyrim Story Manager quest has no required target script".to_string());
    }
    for class_name in required_classes {
        let psc = component
            .psc
            .iter()
            .find(|psc| psc.class_name == class_name)
            .ok_or_else(|| format!("Skyrim Story Manager script {class_name} has no PSC intent"))?;
        let evidence = psc.compiler_evidence.as_ref().filter(|evidence| {
            psc.compiler_evidence_required
                && evidence.require_fresh_output
                && !evidence.manifest_id.trim().is_empty()
                && !evidence.source_digest.trim().is_empty()
        });
        let vmad = component.vmad.iter().find(|vmad| {
            vmad.owner == *target_quest
                && vmad.script_class == class_name
                && vmad.compiler_evidence_required
                && vmad.compiler_evidence.as_ref() == evidence
        });
        if evidence.is_none() || vmad.is_none() {
            return Err(format!(
                "Skyrim Story Manager script {class_name} lacks fresh PSC/VMAD evidence"
            ));
        }
    }
    Ok(())
}

fn unique_source_records(records: &[Record]) -> Result<HashMap<FormKey, &Record>, String> {
    let mut by_form = HashMap::new();
    for record in records {
        if by_form.insert(record.form_key, record).is_some() {
            return Err(format!(
                "duplicate Skyrim Story Manager source {:06X}",
                record.form_key.local
            ));
        }
    }
    Ok(by_form)
}

fn find_component_record<'a>(
    records: &'a HashMap<FormKey, &Record>,
    key: &QuestRecordKey,
    interner: &StringInterner,
) -> Result<&'a Record, String> {
    records
        .values()
        .copied()
        .find(|record| record_key(record, interner) == *key)
        .ok_or_else(|| format!("missing Skyrim Story Manager record {}", key.form_key))
}

fn classify_linear_chain(
    records: &HashMap<FormKey, &Record>,
    source_quest: &QuestRecordKey,
    component: &QuestRuntimeComponentPlan,
    event_mapping: &SkyrimStoryEventMapping,
    interner: &StringInterner,
) -> Result<(QuestRecordKey, Vec<ClassifiedNode>), String> {
    let quest_nodes = records
        .values()
        .copied()
        .filter(|record| record.sig.as_str() == "SMQN")
        .filter_map(|record| match exact_form_key(record, "NNAM") {
            Ok(quest) if record_key_for_form("QUST", quest, interner) == *source_quest => {
                Some(Ok(record))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let [quest_node] = quest_nodes.as_slice() else {
        return Err(format!(
            "Skyrim Story Manager component requires exactly one SMQN owner for {}",
            source_quest.form_key
        ));
    };

    let mut reversed = vec![*quest_node];
    let mut seen = HashSet::from([quest_node.form_key]);
    let mut current = *quest_node;
    loop {
        let parent = exact_form_key(current, "PNAM")?;
        if !seen.insert(parent) {
            return Err("Skyrim Story Manager parent cycle".to_string());
        }
        let parent_record = records.get(&parent).copied().ok_or_else(|| {
            format!(
                "Skyrim Story Manager parent {:06X} is missing",
                parent.local
            )
        })?;
        match parent_record.sig.as_str() {
            "SMBN" => {
                reversed.push(parent_record);
                current = parent_record;
            }
            "SMEN" => {
                reversed.push(parent_record);
                break;
            }
            signature => {
                return Err(format!(
                    "Skyrim Story Manager parent {:06X} has unsupported signature {signature}",
                    parent.local
                ));
            }
        }
    }
    reversed.reverse();
    if reversed.len() < 3 {
        return Err("Skyrim Story Manager chain requires SMEN, SMBN, and SMQN".to_string());
    }
    let story_record_count = records
        .values()
        .filter(|record| matches!(record.sig.as_str(), "SMEN" | "SMBN" | "SMQN"))
        .count();
    if story_record_count != reversed.len() {
        return Err(
            "Skyrim Story Manager component contains nodes outside the owned linear chain"
                .to_string(),
        );
    }

    let root = reversed[0];
    reject_present_form_key(root, "PNAM")?;
    reject_present_form_key(root, "SNAM")?;
    let event_type = exact_u32(root, "ENAM")?;
    if event_type != event_mapping.event.code() {
        return Err(format!(
            "unsupported Skyrim Story Manager event type {event_type:08X}"
        ));
    }
    if !condition_scopes(root)?.is_empty() {
        return Err(
            "Skyrim Story Manager event-root conditions cannot be attached to a target master root"
                .to_string(),
        );
    }
    if optional_u32(root, "CITC")?.unwrap_or(0) != 0 {
        return Err("Skyrim Story Manager event root has an invalid condition count".to_string());
    }

    let source_root = record_key(root, interner);
    let mapped_root = mapped_key(component, &source_root)?;
    if mapped_root != event_mapping.target_event_root
        || mapped_root.signature != "SMEN"
        || event_mapping.producer_id.trim().is_empty()
    {
        return Err("Skyrim Story Manager event mapping is not the allocated FO4 root".to_string());
    }

    let mut classified = Vec::with_capacity(reversed.len());
    for (index, record) in reversed.iter().enumerate() {
        validate_node_fields(record)?;
        let source = record_key(record, interner);
        let target = mapped_key(component, &source)?;
        require_component_topology(component, &source, &target)?;
        if target.signature != source.signature {
            return Err(format!(
                "Skyrim Story Manager mapping changed {} into {}",
                source.signature, target.signature
            ));
        }
        reject_present_form_key(record, "SNAM")?;
        let parent = index
            .checked_sub(1)
            .map(|parent| record_key(reversed[parent], interner));
        if let Some(parent_key) = &parent {
            let declared = exact_form_key(record, "PNAM")?;
            if record_key_for_form(&parent_key.signature, declared, interner) != *parent_key {
                return Err(format!(
                    "Skyrim Story Manager {} PNAM does not own its chain parent",
                    source.form_key
                ));
            }
        }
        let children = reversed
            .get(index + 1)
            .map(|child| BTreeSet::from([record_key(child, interner)]))
            .unwrap_or_default();
        let conditions = if record.sig.as_str() == "SMEN" {
            Vec::new()
        } else {
            lower_conditions(record, component, interner)?
        };
        classified.push(ClassifiedNode {
            source,
            target,
            source_record: (*record).clone(),
            parent,
            children,
            conditions,
        });
    }
    Ok((source_root, classified))
}

fn project_target_records(
    nodes: &[ClassifiedNode],
    component: &QuestRuntimeComponentPlan,
    interner: &StringInterner,
) -> Result<Vec<Record>, String> {
    let mut projected = Vec::with_capacity(nodes.len().saturating_sub(1));
    for node in nodes.iter().filter(|node| node.source.signature != "SMEN") {
        let mut target = Record::new(
            SigCode::from_str(&node.target.signature)?,
            parse_key(&node.target, interner)?,
        );
        target.eid = node.source_record.eid;
        target.flags = node.source_record.flags;
        let parent = node
            .parent
            .as_ref()
            .ok_or_else(|| "non-root Skyrim Story Manager node has no parent".to_string())?;
        push_field(
            &mut target,
            "PNAM",
            FieldValue::FormKey(parse_key(&mapped_key(component, parent)?, interner)?),
        )?;
        if !node.conditions.is_empty() {
            push_field(
                &mut target,
                "CITC",
                FieldValue::Uint(node.conditions.len() as u64),
            )?;
            for condition in &node.conditions {
                push_field(&mut target, "CTDA", condition.target_value.clone())?;
                if let Some(value) = &condition.cis1 {
                    push_field(
                        &mut target,
                        "CIS1",
                        FieldValue::String(interner.intern(value)),
                    )?;
                }
                if let Some(value) = &condition.cis2 {
                    push_field(
                        &mut target,
                        "CIS2",
                        FieldValue::String(interner.intern(value)),
                    )?;
                }
            }
        }
        copy_optional_scalar(&node.source_record, &mut target, "DNAM")?;
        copy_optional_scalar(&node.source_record, &mut target, "XNAM")?;
        if node.source.signature == "SMQN" {
            copy_optional_scalar(&node.source_record, &mut target, "MNAM")?;
            push_field(&mut target, "QNAM", FieldValue::Uint(1))?;
            push_field(
                &mut target,
                "NNAM",
                FieldValue::FormKey(parse_key(
                    &mapped_key(component, &component.root_quest)?,
                    interner,
                )?),
            )?;
            copy_optional_scalar(&node.source_record, &mut target, "FNAM")?;
            copy_optional_scalar(&node.source_record, &mut target, "RNAM")?;
        }
        projected.push(target);
    }
    Ok(projected)
}

struct ConditionScope<'a> {
    ctda: &'a FieldEntry,
    companions: &'a [FieldEntry],
}

fn lower_conditions(
    record: &Record,
    component: &QuestRuntimeComponentPlan,
    interner: &StringInterner,
) -> Result<Vec<LoweredCondition>, String> {
    let scopes = condition_scopes(record)?;
    let declared_count = optional_u32(record, "CITC")?;
    if declared_count.unwrap_or(0) != scopes.len() as u32
        || (!scopes.is_empty() && declared_count.is_none())
    {
        return Err(format!(
            "Skyrim Story Manager {:06X} CITC does not own its condition rows",
            record.form_key.local
        ));
    }
    scopes
        .iter()
        .map(|scope| lower_condition(scope, record.form_key.plugin, component, interner))
        .collect()
}

fn condition_scopes(record: &Record) -> Result<Vec<ConditionScope<'_>>, String> {
    let mut scopes = Vec::new();
    let mut index = 0;
    while index < record.fields.len() {
        if record.fields[index].sig.0 != *b"CTDA" {
            if matches!(record.fields[index].sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2") {
                return Err(format!(
                    "Skyrim Story Manager {:06X} has an orphan condition string",
                    record.form_key.local
                ));
            }
            index += 1;
            continue;
        }
        let ctda = &record.fields[index];
        index += 1;
        let start = index;
        while index < record.fields.len()
            && matches!(record.fields[index].sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2")
        {
            index += 1;
        }
        scopes.push(ConditionScope {
            ctda,
            companions: &record.fields[start..index],
        });
    }
    Ok(scopes)
}

fn lower_condition(
    scope: &ConditionScope<'_>,
    source_plugin: Sym,
    component: &QuestRuntimeComponentPlan,
    interner: &StringInterner,
) -> Result<LoweredCondition, String> {
    let FieldValue::Bytes(bytes) = &scope.ctda.value else {
        return Err("Skyrim Story Manager CTDA is not a raw 32-byte row".to_string());
    };
    if bytes.len() != 32
        || bytes[1..4] != [0, 0, 0]
        || bytes[10..12] != [0, 0]
        || i32::from_le_bytes(bytes[28..32].try_into().unwrap()) != -1
    {
        return Err("Skyrim Story Manager CTDA has an unsupported payload shape".to_string());
    }
    let source_function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    let (function_name, parameter_kind) = match source_function {
        47 => ("GetItemCount", ConditionParameterKind::Form),
        58 => ("GetStage", ConditionParameterKind::FormAndInteger),
        59 => ("GetStageDone", ConditionParameterKind::FormAndInteger),
        72 => ("GetIsID", ConditionParameterKind::Form),
        566 => ("GetIsAliasRef", ConditionParameterKind::Alias),
        629 => ("GetVMQuestVariable", ConditionParameterKind::VmVariable),
        _ => {
            return Err(format!(
                "unsupported Skyrim Story Manager condition function {source_function}"
            ));
        }
    };
    let parameter_1 = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let parameter_2 = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let run_on_value = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let reference = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let (run_on, target_reference) = match (run_on_value, reference) {
        (0, 0) => ("Subject".to_string(), FieldValue::Uint(0)),
        (1, 0) => ("Target".to_string(), FieldValue::Uint(0)),
        (2, reference) if reference != 0 => {
            let target = mapped_raw_form(component, reference, source_plugin, interner)?;
            (
                format!("Reference:{}", target.form_key),
                FieldValue::FormKey(parse_key(&target, interner)?),
            )
        }
        _ => {
            return Err(format!(
                "unsupported Skyrim Story Manager condition run-on {run_on_value}"
            ));
        }
    };

    let mut parameters = Vec::new();
    let target_parameter_1 = match parameter_kind {
        ConditionParameterKind::Alias => {
            if parameter_2 != 0 {
                return Err("Skyrim Story Manager alias condition has parameter 2".to_string());
            }
            parameters.push(parameter_1.to_string());
            FieldValue::Uint(parameter_1 as u64)
        }
        ConditionParameterKind::Form | ConditionParameterKind::FormAndInteger => {
            let target = mapped_raw_form(component, parameter_1, source_plugin, interner)?;
            parameters.push(target.form_key.clone());
            if parameter_kind == ConditionParameterKind::FormAndInteger {
                parameters.push(parameter_2.to_string());
            } else if parameter_2 != 0 {
                return Err("Skyrim Story Manager form condition has parameter 2".to_string());
            }
            FieldValue::FormKey(parse_key(&target, interner)?)
        }
        ConditionParameterKind::VmVariable => {
            if parameter_2 == 0 {
                return Err(
                    "Skyrim Story Manager VM condition has no variable selector".to_string()
                );
            }
            let target = mapped_raw_form(component, parameter_1, source_plugin, interner)?;
            parameters.push(target.form_key.clone());
            FieldValue::FormKey(parse_key(&target, interner)?)
        }
    };

    let cis1 = exact_companion(scope, "CIS1", interner)?;
    let cis2 = exact_companion(scope, "CIS2", interner)?;
    match parameter_kind {
        ConditionParameterKind::VmVariable
            if cis1.is_none() && cis2.as_deref().is_some_and(|v| !v.is_empty()) => {}
        ConditionParameterKind::VmVariable => {
            return Err("Skyrim Story Manager VM condition has invalid CIS companions".to_string());
        }
        _ if cis1.is_some() || cis2.is_some() => {
            return Err(
                "Skyrim Story Manager condition has unsupported CIS companions".to_string(),
            );
        }
        _ => {}
    }

    let condition_type = bytes[0];
    let comparison = if condition_type & 0x04 != 0 {
        let target = mapped_raw_form(
            component,
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            source_plugin,
            interner,
        )?;
        (
            target.form_key.clone(),
            FieldValue::FormKey(parse_key(&target, interner)?),
        )
    } else {
        let value = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if !value.is_finite() {
            return Err("Skyrim Story Manager condition comparison is not finite".to_string());
        }
        (value.to_string(), FieldValue::Float(value))
    };
    let operator = match condition_type >> 5 {
        0 => "EqualTo",
        1 => "NotEqualTo",
        2 => "GreaterThan",
        3 => "GreaterThanOrEqualTo",
        4 => "LessThan",
        5 => "LessThanOrEqualTo",
        value => return Err(format!("unsupported Skyrim Story Manager operator {value}")),
    };
    let target_value = FieldValue::Struct(vec![
        (
            interner.intern("type"),
            FieldValue::Uint(condition_type as u64),
        ),
        (interner.intern("unknown_u8_1"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
        (interner.intern("comparison_value"), comparison.1),
        (
            interner.intern("function"),
            FieldValue::Uint(source_function as u64),
        ),
        (interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_7"), FieldValue::Uint(0)),
        (interner.intern("parameter_1"), target_parameter_1),
        (
            interner.intern("parameter_2"),
            FieldValue::Uint(if parameter_kind == ConditionParameterKind::VmVariable {
                0
            } else {
                parameter_2 as u64
            }),
        ),
        (
            interner.intern("run_on"),
            FieldValue::Uint(run_on_value as u64),
        ),
        (interner.intern("reference"), target_reference),
        (interner.intern("parameter_3"), FieldValue::Int(-1)),
    ]);
    Ok(LoweredCondition {
        intent: ConditionIntent {
            function: function_name.to_string(),
            operator: operator.to_string(),
            comparison_value: comparison.0,
            parameters,
            run_on,
            cis1: cis1.clone(),
            cis2: cis2.clone(),
        },
        target_value,
        cis1,
        cis2,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionParameterKind {
    Alias,
    Form,
    FormAndInteger,
    VmVariable,
}

fn validate_node_fields(record: &Record) -> Result<(), String> {
    let allowed: &[&[u8; 4]] = match record.sig.as_str() {
        "SMEN" => &[b"EDID", b"CITC", b"ENAM"],
        "SMBN" => &[
            b"EDID", b"PNAM", b"SNAM", b"CITC", b"CTDA", b"CIS1", b"CIS2", b"DNAM", b"XNAM",
        ],
        "SMQN" => &[
            b"EDID", b"PNAM", b"SNAM", b"CITC", b"CTDA", b"CIS1", b"CIS2", b"DNAM", b"XNAM",
            b"MNAM", b"QNAM", b"NNAM", b"FNAM", b"RNAM",
        ],
        signature => return Err(format!("unsupported Skyrim Story Manager node {signature}")),
    };
    for field in &record.fields {
        if !allowed.contains(&&field.sig.0) {
            return Err(format!(
                "Skyrim Story Manager {} has unsupported mandatory field {}",
                record.sig.as_str(),
                field.sig.as_str()
            ));
        }
    }
    for signature in ["DNAM", "XNAM", "MNAM"] {
        if count_fields(record, signature) > 1 {
            return Err(format!(
                "Skyrim Story Manager {} has repeated {signature}",
                record.sig.as_str()
            ));
        }
    }
    if record.sig.as_str() == "SMQN" {
        if exact_u32(record, "QNAM")? != 1 || count_fields(record, "NNAM") != 1 {
            return Err("Skyrim Story Manager SMQN must own exactly one QUST".to_string());
        }
        if count_fields(record, "FNAM") > 1 || count_fields(record, "RNAM") > 1 {
            return Err("Skyrim Story Manager SMQN quest metadata is ambiguous".to_string());
        }
    }
    Ok(())
}

fn exact_companion(
    scope: &ConditionScope<'_>,
    signature: &str,
    interner: &StringInterner,
) -> Result<Option<String>, String> {
    let values = scope
        .companions
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .map(|field| text_value(&field.value, interner))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| format!("Skyrim Story Manager {signature} is not text"))?;
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value.clone())),
        _ => Err(format!(
            "Skyrim Story Manager condition repeats {signature}"
        )),
    }
}

fn mapped_raw_form(
    component: &QuestRuntimeComponentPlan,
    raw: u32,
    source_plugin: Sym,
    interner: &StringInterner,
) -> Result<QuestRecordKey, String> {
    if raw == 0 {
        return Err("Skyrim Story Manager condition contains a NULL FormKey".to_string());
    }
    let source = FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: source_plugin,
    }
    .format(interner);
    let matches = component
        .mappings
        .iter()
        .filter(|mapping| mapping.source.form_key.eq_ignore_ascii_case(&source))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [mapping] => Ok(mapping.target.clone()),
        _ => Err(format!(
            "Skyrim Story Manager condition FormKey {source} is not mapped exactly once"
        )),
    }
}

fn mapped_key(
    component: &QuestRuntimeComponentPlan,
    source: &QuestRecordKey,
) -> Result<QuestRecordKey, String> {
    let matches = component
        .mappings
        .iter()
        .filter(|mapping| mapping.source == *source)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [mapping] => Ok(mapping.target.clone()),
        _ => Err(format!(
            "Skyrim Story Manager {} {} is not mapped exactly once",
            source.signature, source.form_key
        )),
    }
}

fn require_component_topology(
    component: &QuestRuntimeComponentPlan,
    source: &QuestRecordKey,
    target: &QuestRecordKey,
) -> Result<(), String> {
    if !component
        .source_topology
        .iter()
        .any(|placement| placement.record == *source && !placement.group_path.is_empty())
        || !component
            .target_topology
            .iter()
            .any(|placement| placement.record == *target && !placement.group_path.is_empty())
    {
        return Err(format!(
            "Skyrim Story Manager topology is incomplete for {}",
            source.form_key
        ));
    }
    Ok(())
}

fn record_key(record: &Record, interner: &StringInterner) -> QuestRecordKey {
    record_key_for_form(record.sig.as_str(), record.form_key, interner)
}

fn record_key_for_form(
    signature: &str,
    form_key: FormKey,
    interner: &StringInterner,
) -> QuestRecordKey {
    QuestRecordKey::new(signature, form_key.format(interner))
}

fn parse_key(key: &QuestRecordKey, interner: &StringInterner) -> Result<FormKey, String> {
    if key.form_key.contains('@') {
        return FormKey::parse(&key.form_key, interner);
    }
    let (local, plugin) = key
        .form_key
        .split_once(':')
        .ok_or_else(|| format!("invalid Story Manager FormKey {}", key.form_key))?;
    FormKey::parse(&format!("{local}@{plugin}"), interner)
}

fn exact_form_key(record: &Record, signature: &str) -> Result<FormKey, String> {
    let values = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .map(|field| match &field.value {
            FieldValue::FormKey(value) if value.local != 0 => Some(*value),
            _ => None,
        })
        .collect::<Vec<_>>();
    match values.as_slice() {
        [Some(value)] => Ok(*value),
        _ => Err(format!(
            "Skyrim Story Manager {} requires exactly one typed {signature}",
            record.sig.as_str()
        )),
    }
}

fn reject_present_form_key(record: &Record, signature: &str) -> Result<(), String> {
    if count_fields(record, signature) != 0 {
        return Err(format!(
            "Skyrim Story Manager {} has unsupported {signature} ordering/parent semantics",
            record.sig.as_str()
        ));
    }
    Ok(())
}

fn exact_u32(record: &Record, signature: &str) -> Result<u32, String> {
    optional_u32(record, signature)?.ok_or_else(|| {
        format!(
            "Skyrim Story Manager {} requires {signature}",
            record.sig.as_str()
        )
    })
}

fn optional_u32(record: &Record, signature: &str) -> Result<Option<u32>, String> {
    let values = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .map(|field| match &field.value {
            FieldValue::Uint(value) => u32::try_from(*value).ok(),
            FieldValue::Int(value) => u32::try_from(*value).ok(),
            FieldValue::Bytes(bytes) if bytes.len() == 4 => {
                Some(u32::from_le_bytes(bytes.as_slice().try_into().ok()?))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(None),
        [Some(value)] => Ok(Some(*value)),
        _ => Err(format!(
            "Skyrim Story Manager {} has invalid {signature}",
            record.sig.as_str()
        )),
    }
}

fn count_fields(record: &Record, signature: &str) -> usize {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .count()
}

fn copy_optional_scalar(
    source: &Record,
    target: &mut Record,
    signature: &str,
) -> Result<(), String> {
    let values = source
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(()),
        [field] => push_field(target, signature, field.value.clone()),
        _ => Err(format!(
            "Skyrim Story Manager {} repeats {signature}",
            source.sig.as_str()
        )),
    }
}

fn text_value(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(value) => interner.resolve(*value).map(str::to_string),
        FieldValue::Bytes(bytes) => {
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            std::str::from_utf8(&bytes[..end]).ok().map(str::to_string)
        }
        _ => None,
    }
}

fn push_field(record: &mut Record, signature: &str, value: FieldValue) -> Result<(), String> {
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str(signature)?,
        value,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use crate::quest_runtime::{
        CompilerEvidenceIntent, DependencyClosure, PscIntent, QuestRuntimeExpectedReceipt,
        QuestSemanticPlan, QuestSourceGame, ScriptIntent, SourceProvenance,
        SourceTargetFormMapping, StartProducerIntent, TopologyPlacement, VmadIntent,
    };

    use super::*;

    struct Fixture {
        records: Vec<Record>,
        component: QuestRuntimeComponentPlan,
        mapping: SkyrimStoryEventMapping,
        source_root: QuestRecordKey,
        source_branch: QuestRecordKey,
        source_node: QuestRecordKey,
        target_root: QuestRecordKey,
        target_branch: QuestRecordKey,
        target_node: QuestRecordKey,
        target_quest: QuestRecordKey,
    }

    fn fixture(interner: &StringInterner, locals: [u32; 4]) -> Fixture {
        let plugin = interner.intern("Skyrim.esm");
        let output = interner.intern("Port.esp");
        let fallout4 = interner.intern("Fallout4.esm");
        let source_root_fk = FormKey {
            local: locals[0],
            plugin,
        };
        let source_branch_fk = FormKey {
            local: locals[1],
            plugin,
        };
        let source_node_fk = FormKey {
            local: locals[2],
            plugin,
        };
        let source_quest_fk = FormKey {
            local: locals[3],
            plugin,
        };
        let source_reference_fk = FormKey {
            local: locals[3] + 1,
            plugin,
        };
        let target_root_fk = FormKey {
            local: 0x1244D0,
            plugin: fallout4,
        };
        let target_branch_fk = FormKey {
            local: 0x080222,
            plugin: output,
        };
        let target_node_fk = FormKey {
            local: 0x080223,
            plugin: output,
        };
        let target_quest_fk = FormKey {
            local: 0x080224,
            plugin: output,
        };
        let target_reference_fk = FormKey {
            local: 0x080225,
            plugin: output,
        };

        let mut root = record("SMEN", source_root_fk, "Shape_EventRoot", interner);
        root.fields.push(field(
            "ENAM",
            FieldValue::Uint(SkyrimFo4StoryEvent::HackComputer.code() as u64),
        ));
        let mut branch = record("SMBN", source_branch_fk, "Shape_Branch", interner);
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(source_root_fk)));
        branch.fields.push(field("CITC", FieldValue::Uint(1)));
        branch.fields.push(field(
            "CTDA",
            raw_condition(
                72,
                source_reference_fk.local,
                0,
                2,
                source_reference_fk.local,
            ),
        ));
        let mut node = record("SMQN", source_node_fk, "Shape_QuestNode", interner);
        node.fields
            .push(field("PNAM", FieldValue::FormKey(source_branch_fk)));
        node.fields.push(field("CITC", FieldValue::Uint(1)));
        node.fields.push(field(
            "CTDA",
            raw_condition(629, source_quest_fk.local, 1, 0, 0),
        ));
        node.fields.push(field(
            "CIS2",
            FieldValue::String(interner.intern("::State")),
        ));
        node.fields.push(field("MNAM", FieldValue::Uint(1)));
        node.fields.push(field("QNAM", FieldValue::Uint(1)));
        node.fields
            .push(field("NNAM", FieldValue::FormKey(source_quest_fk)));
        let quest = record("QUST", source_quest_fk, "Shape_Quest", interner);

        let source_root = record_key(&root, interner);
        let source_branch = record_key(&branch, interner);
        let source_node = record_key(&node, interner);
        let source_quest = record_key(&quest, interner);
        let source_reference = record_key_for_form("REFR", source_reference_fk, interner);
        let target_root = record_key_for_form("SMEN", target_root_fk, interner);
        let target_branch = record_key_for_form("SMBN", target_branch_fk, interner);
        let target_node = record_key_for_form("SMQN", target_node_fk, interner);
        let target_quest = record_key_for_form("QUST", target_quest_fk, interner);
        let target_reference = record_key_for_form("REFR", target_reference_fk, interner);
        let evidence = CompilerEvidenceIntent {
            manifest_id: "manifest-story-fragment".to_string(),
            source_digest: blake3::hash(b"story fragment").to_hex().to_string(),
            require_fresh_output: true,
        };
        let route_id = format!("skyrim-story-route:{:06X}", source_quest_fk.local);
        let mappings = vec![
            SourceTargetFormMapping {
                source: source_root.clone(),
                target: target_root.clone(),
            },
            SourceTargetFormMapping {
                source: source_branch.clone(),
                target: target_branch.clone(),
            },
            SourceTargetFormMapping {
                source: source_node.clone(),
                target: target_node.clone(),
            },
            SourceTargetFormMapping {
                source: source_quest.clone(),
                target: target_quest.clone(),
            },
            SourceTargetFormMapping {
                source: source_reference.clone(),
                target: target_reference,
            },
        ];
        let source_topology = mappings
            .iter()
            .map(|mapping| TopologyPlacement {
                record: mapping.source.clone(),
                group_path: vec![format!("GRUP:{}", mapping.source.signature)],
            })
            .collect();
        let target_topology = mappings
            .iter()
            .map(|mapping| TopologyPlacement {
                record: mapping.target.clone(),
                group_path: vec![format!("GRUP:{}", mapping.target.signature)],
            })
            .collect();
        let component = QuestRuntimeComponentPlan {
            component_id: format!("skyrim-story:{:06X}", source_quest_fk.local),
            root_quest: source_quest.clone(),
            provenance: SourceProvenance {
                game: QuestSourceGame::SkyrimSe,
                source_plugin: "Skyrim.esm".to_string(),
                graft: None,
            },
            owned_records: vec![
                source_root.clone(),
                source_branch.clone(),
                source_node.clone(),
                source_quest.clone(),
            ],
            shared_records: BTreeSet::from([source_reference.clone()]),
            dependencies: DependencyClosure {
                direct: BTreeSet::from([source_reference.clone()]),
                recursive: BTreeSet::from([source_reference]),
            },
            source_topology,
            target_topology,
            mappings,
            semantics: QuestSemanticPlan::default(),
            scripts: BTreeSet::from([ScriptIntent {
                owner: source_quest.clone(),
                source_name: "QF_Shape".to_string(),
                target_class: "B21_SkyQF_080224".to_string(),
                required: true,
            }]),
            fragments: BTreeSet::new(),
            psc: BTreeSet::from([PscIntent {
                class_name: "B21_SkyQF_080224".to_string(),
                source_artifact: "Scripts/Source/User/B21_SkyQF_080224.psc".to_string(),
                compiler_evidence_required: true,
                compiler_evidence: Some(evidence.clone()),
            }]),
            vmad: BTreeSet::from([VmadIntent {
                owner: target_quest.clone(),
                script_class: "B21_SkyQF_080224".to_string(),
                properties: BTreeSet::new(),
                compiler_evidence_required: true,
                compiler_evidence: Some(evidence),
            }]),
            assets: BTreeSet::new(),
            inbound_producers: BTreeSet::from([StartProducerIntent {
                producer_id: route_id.clone(),
                carrier: source_root.clone(),
                producer_kind: "native_story_event".to_string(),
                evidence_id: "fo4-native-event:HACK".to_string(),
                proven: true,
            }]),
            start_disposition: Some(StartDisposition::StoryManager {
                route_id: route_id.clone(),
            }),
            admission: QuestRuntimeAdmission::Supported,
            expected_receipt: QuestRuntimeExpectedReceipt::default(),
        };
        Fixture {
            records: vec![root, branch, node, quest],
            component,
            mapping: SkyrimStoryEventMapping {
                event: SkyrimFo4StoryEvent::HackComputer,
                target_event_root: target_root.clone(),
                producer_id: route_id,
            },
            source_root,
            source_branch,
            source_node,
            target_root,
            target_branch,
            target_node,
            target_quest,
        }
    }

    fn record(
        signature: &str,
        form_key: FormKey,
        editor_id: &str,
        interner: &StringInterner,
    ) -> Record {
        let mut record = Record::new(SigCode::from_str(signature).unwrap(), form_key);
        record.eid = Some(interner.intern(editor_id));
        record
    }

    fn field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn raw_condition(
        function: u16,
        parameter_1: u32,
        parameter_2: u32,
        run_on: u32,
        reference: u32,
    ) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
        bytes[8..10].copy_from_slice(&function.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[16..20].copy_from_slice(&parameter_2.to_le_bytes());
        bytes[20..24].copy_from_slice(&run_on.to_le_bytes());
        bytes[24..28].copy_from_slice(&reference.to_le_bytes());
        bytes[28..32].copy_from_slice(&(-1_i32).to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    #[test]
    fn synthetic_complete_chain_projects_with_lowered_conditions() {
        let interner = StringInterner::new();
        let fixture = fixture(&interner, [0x001001, 0x001002, 0x001003, 0x001004]);
        let projection = project_story_manager_chain(
            &fixture.records,
            &fixture.component,
            &fixture.mapping,
            &interner,
        )
        .unwrap();
        assert_eq!(
            projection.route_receipt.route.node_chain,
            vec![
                fixture.target_root.clone(),
                fixture.target_branch.clone(),
                fixture.target_node.clone(),
            ]
        );
        assert_eq!(projection.route_receipt.route.quest, fixture.target_quest);
        assert_eq!(projection.records.len(), 2);
        assert_eq!(
            projection.records[0].form_key,
            parse_key(&fixture.target_branch, &interner).unwrap()
        );
        assert_eq!(
            projection.records[1].form_key,
            parse_key(&fixture.target_node, &interner).unwrap()
        );
        assert!(projection.records[1].fields.iter().any(|field| {
            field.sig.0 == *b"CIS2"
                && text_value(&field.value, &interner).as_deref() == Some("::State")
        }));
        let expected_root = parse_key(&fixture.target_root, &interner).unwrap();
        let expected_branch = parse_key(&fixture.target_branch, &interner).unwrap();
        let expected_quest = parse_key(&fixture.target_quest, &interner).unwrap();
        assert!(projection.records[0].fields.iter().any(|field| {
            field.sig.0 == *b"PNAM" && field.value == FieldValue::FormKey(expected_root)
        }));
        assert!(projection.records[1].fields.iter().any(|field| {
            field.sig.0 == *b"PNAM" && field.value == FieldValue::FormKey(expected_branch)
        }));
        assert!(projection.records[1].fields.iter().any(|field| {
            field.sig.0 == *b"NNAM" && field.value == FieldValue::FormKey(expected_quest)
        }));
        assert_eq!(projection.graph.nodes[1].conditions.len(), 1);
        assert_eq!(projection.graph.nodes[2].conditions.len(), 1);
    }

    #[test]
    fn documented_070221_through_070224_shape_is_root_first_and_deterministic() {
        let interner = StringInterner::new();
        let mut fixture = fixture(&interner, [0x070221, 0x070222, 0x070223, 0x070224]);
        fixture.records.reverse();
        let projection = project_story_manager_chain(
            &fixture.records,
            &fixture.component,
            &fixture.mapping,
            &interner,
        )
        .unwrap();
        assert_eq!(
            projection
                .graph
                .nodes
                .iter()
                .map(|node| node.source.clone())
                .collect::<Vec<_>>(),
            vec![
                fixture.source_root,
                fixture.source_branch,
                fixture.source_node
            ]
        );
        assert_eq!(
            projection.route_receipt.route.producer_evidence_id,
            "fo4-native-event:HACK"
        );
    }

    #[test]
    fn noncanonical_or_unavailable_fo4_event_roots_fail_closed() {
        let interner = StringInterner::new();
        let mut noncanonical = fixture(&interner, [0x009001, 0x009002, 0x009003, 0x009004]);
        noncanonical.mapping.target_event_root.form_key = "000ABC@Fallout4.esm".to_string();
        let error = project_story_manager_chain(
            &noncanonical.records,
            &noncanonical.component,
            &noncanonical.mapping,
            &interner,
        )
        .unwrap_err();
        assert!(error.contains("canonical loaded FO4 SMEN root"), "{error}");

        let mut kill = fixture(&interner, [0x009101, 0x009102, 0x009103, 0x009104]);
        kill.mapping.event = SkyrimFo4StoryEvent::KillActor;
        let root = kill
            .records
            .iter_mut()
            .find(|record| record.sig.as_str() == "SMEN")
            .unwrap();
        root.fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "ENAM")
            .unwrap()
            .value = FieldValue::Uint(SkyrimFo4StoryEvent::KillActor.code() as u64);
        let error =
            project_story_manager_chain(&kill.records, &kill.component, &kill.mapping, &interner)
                .unwrap_err();
        assert!(
            error.contains("Fallout 4 has no canonical SMEN root"),
            "{error}"
        );
    }

    #[test]
    fn missing_parent_cycle_and_unsupported_event_reject_the_component() {
        let interner = StringInterner::new();

        let mut missing = fixture(&interner, [0x002001, 0x002002, 0x002003, 0x002004]);
        missing
            .records
            .retain(|record| record.sig.as_str() != "SMBN");
        assert!(
            project_story_manager_chain(
                &missing.records,
                &missing.component,
                &missing.mapping,
                &interner,
            )
            .unwrap_err()
            .contains("missing")
        );

        let mut cycle = fixture(&interner, [0x003001, 0x003002, 0x003003, 0x003004]);
        let source_node_fk = parse_key(&cycle.source_node, &interner).unwrap();
        let branch = cycle
            .records
            .iter_mut()
            .find(|record| record.sig.as_str() == "SMBN")
            .unwrap();
        branch.fields[0].value = FieldValue::FormKey(source_node_fk);
        assert!(
            project_story_manager_chain(
                &cycle.records,
                &cycle.component,
                &cycle.mapping,
                &interner,
            )
            .unwrap_err()
            .contains("cycle")
        );

        let mut event = fixture(&interner, [0x004001, 0x004002, 0x004003, 0x004004]);
        let root = event
            .records
            .iter_mut()
            .find(|record| record.sig.as_str() == "SMEN")
            .unwrap();
        root.fields[0].value = FieldValue::Uint(u32::from_le_bytes(*b"NONE") as u64);
        assert!(
            project_story_manager_chain(
                &event.records,
                &event.component,
                &event.mapping,
                &interner,
            )
            .unwrap_err()
            .contains("unsupported")
        );
    }

    #[test]
    fn graph_requires_component_script_vmad_producer_and_topology_gates() {
        let interner = StringInterner::new();

        let mut scripts = fixture(&interner, [0x005001, 0x005002, 0x005003, 0x005004]);
        scripts.component.vmad.clear();
        assert!(
            project_story_manager_chain(
                &scripts.records,
                &scripts.component,
                &scripts.mapping,
                &interner,
            )
            .is_err()
        );

        let mut quest = fixture(&interner, [0x005101, 0x005102, 0x005103, 0x005104]);
        quest.records.retain(|record| record.sig.as_str() != "QUST");
        assert!(
            project_story_manager_chain(
                &quest.records,
                &quest.component,
                &quest.mapping,
                &interner,
            )
            .unwrap_err()
            .contains("missing Skyrim Story Manager record")
        );

        let mut producer = fixture(&interner, [0x006001, 0x006002, 0x006003, 0x006004]);
        producer.component.inbound_producers = producer
            .component
            .inbound_producers
            .iter()
            .cloned()
            .map(|mut producer| {
                producer.proven = false;
                producer
            })
            .collect();
        assert!(
            project_story_manager_chain(
                &producer.records,
                &producer.component,
                &producer.mapping,
                &interner,
            )
            .is_err()
        );

        let mut topology = fixture(&interner, [0x007001, 0x007002, 0x007003, 0x007004]);
        topology
            .component
            .target_topology
            .retain(|placement| placement.record != topology.target_node);
        assert!(
            project_story_manager_chain(
                &topology.records,
                &topology.component,
                &topology.mapping,
                &interner,
            )
            .unwrap_err()
            .contains("topology")
        );
    }

    #[test]
    fn unsupported_condition_shape_rejects_the_whole_graph() {
        let interner = StringInterner::new();
        let mut fixture = fixture(&interner, [0x008001, 0x008002, 0x008003, 0x008004]);
        let branch = fixture
            .records
            .iter_mut()
            .find(|record| record.sig.as_str() == "SMBN")
            .unwrap();
        let FieldValue::Bytes(bytes) = &mut branch
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"CTDA")
            .unwrap()
            .value
        else {
            unreachable!()
        };
        bytes[8..10].copy_from_slice(&999_u16.to_le_bytes());
        assert!(
            project_story_manager_chain(
                &fixture.records,
                &fixture.component,
                &fixture.mapping,
                &interner,
            )
            .unwrap_err()
            .contains("unsupported Skyrim Story Manager condition")
        );
    }
}
