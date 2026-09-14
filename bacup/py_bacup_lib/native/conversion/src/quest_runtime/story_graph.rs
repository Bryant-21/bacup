use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    ConditionIntent, QuestRecordKey, SourceTargetFormMapping, StartDisposition,
    StartProducerIntent, StartRouteReceipt,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryNodeKind {
    EventRoot,
    Branch,
    Quest,
}

impl StoryNodeKind {
    fn signature(self) -> &'static str {
        match self {
            Self::EventRoot => "SMEN",
            Self::Branch => "SMBN",
            Self::Quest => "SMQN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StoryEventRootIntent {
    pub event_type: String,
    pub target_event_root: QuestRecordKey,
    pub supported: bool,
    pub producer_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryGraphNode {
    pub source: QuestRecordKey,
    pub target: QuestRecordKey,
    pub kind: StoryNodeKind,
    pub parent: Option<QuestRecordKey>,
    pub children: BTreeSet<QuestRecordKey>,
    pub event_root: Option<StoryEventRootIntent>,
    pub quest: Option<QuestRecordKey>,
    pub conditions: BTreeSet<ConditionIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryRouteIntent {
    pub route_id: String,
    pub quest: QuestRecordKey,
    pub disposition: StartDisposition,
    pub quest_node: Option<QuestRecordKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryGraphPlan {
    pub graph_id: String,
    pub root_quest: QuestRecordKey,
    pub target_quest: QuestRecordKey,
    pub nodes: Vec<StoryGraphNode>,
    pub producers: BTreeSet<StartProducerIntent>,
    pub routes: Vec<StoryRouteIntent>,
}

impl StoryGraphPlan {
    pub fn validation_issues(&self) -> Vec<StoryGraphValidationIssue> {
        validate_story_graph_plans(std::slice::from_ref(self))
    }

    pub const fn production_emission_enabled(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryGraphValidationCode {
    InvalidQuest,
    DuplicateQuestOwnership,
    DuplicateSourceNode,
    DuplicateTargetAllocation,
    UnsupportedNode,
    MissingParent,
    ParentChildMismatch,
    ParentCycle,
    UnsupportedRoot,
    UnsupportedEvent,
    InvalidQuestOwnership,
    MissingRoute,
    MultipleRoutes,
    RouteQuestMismatch,
    InconsistentStartDisposition,
    MissingProvenProducer,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StoryGraphValidationIssue {
    pub code: StoryGraphValidationCode,
    pub graph_id: String,
    pub node: Option<QuestRecordKey>,
    pub quest: Option<QuestRecordKey>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedStoryGraph {
    pub ordered_nodes: Vec<QuestRecordKey>,
    pub allocations: Vec<SourceTargetFormMapping>,
    pub route_receipt: StoryGraphRouteReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StoryGraphRouteReceipt {
    pub route: StartRouteReceipt,
    pub node_kinds: BTreeMap<QuestRecordKey, StoryNodeKind>,
    pub conditions: BTreeMap<QuestRecordKey, BTreeSet<ConditionIntent>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StoryRouteReceiptDamage {
    Missing {
        route: StoryGraphRouteReceipt,
        occurrences: usize,
    },
    Unexpected {
        route: StoryGraphRouteReceipt,
        occurrences: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryRouteReceiptComparison {
    pub intact: bool,
    pub damage: Vec<StoryRouteReceiptDamage>,
}

pub fn validate_story_graph_plans(plans: &[StoryGraphPlan]) -> Vec<StoryGraphValidationIssue> {
    let mut issues = BTreeSet::new();
    let mut owners: BTreeMap<&QuestRecordKey, &str> = BTreeMap::new();

    for plan in plans {
        validate_plan(plan, &mut issues);
        for node in &plan.nodes {
            let Some(quest) = &node.quest else {
                continue;
            };
            if let Some(first_owner) = owners.get(quest) {
                insert_issue(
                    &mut issues,
                    plan,
                    StoryGraphValidationCode::DuplicateQuestOwnership,
                    Some(node.source.clone()),
                    Some(quest.clone()),
                    Some((*first_owner).to_owned()),
                );
            } else {
                owners.insert(quest, &plan.graph_id);
            }
        }
    }

    issues.into_iter().collect()
}

pub fn validate_story_graph(
    plan: &StoryGraphPlan,
) -> Result<ValidatedStoryGraph, Vec<StoryGraphValidationIssue>> {
    let issues = plan.validation_issues();
    if !issues.is_empty() {
        return Err(issues);
    }

    let ordered_nodes = deterministic_node_order(plan);
    let nodes_by_source: BTreeMap<_, _> =
        plan.nodes.iter().map(|node| (&node.source, node)).collect();
    let allocations = ordered_nodes
        .iter()
        .map(|source| {
            let node = nodes_by_source[source];
            SourceTargetFormMapping {
                source: node.source.clone(),
                target: node.target.clone(),
            }
        })
        .collect();
    let route_receipt = build_route_receipt(plan, &nodes_by_source);

    Ok(ValidatedStoryGraph {
        ordered_nodes,
        allocations,
        route_receipt,
    })
}

pub fn compare_story_route_receipts(
    expected: &[StoryGraphRouteReceipt],
    actual: &[StoryGraphRouteReceipt],
) -> StoryRouteReceiptComparison {
    let expected_counts = occurrence_counts(expected);
    let actual_counts = occurrence_counts(actual);
    let mut routes: BTreeSet<&StoryGraphRouteReceipt> = expected_counts.keys().copied().collect();
    routes.extend(actual_counts.keys().copied());

    let mut damage = Vec::new();
    for route in routes {
        let expected_count = expected_counts.get(route).copied().unwrap_or(0);
        let actual_count = actual_counts.get(route).copied().unwrap_or(0);
        if actual_count < expected_count {
            damage.push(StoryRouteReceiptDamage::Missing {
                route: route.clone(),
                occurrences: expected_count - actual_count,
            });
        } else if actual_count > expected_count {
            damage.push(StoryRouteReceiptDamage::Unexpected {
                route: route.clone(),
                occurrences: actual_count - expected_count,
            });
        }
    }

    StoryRouteReceiptComparison {
        intact: damage.is_empty(),
        damage,
    }
}

fn validate_plan(plan: &StoryGraphPlan, issues: &mut BTreeSet<StoryGraphValidationIssue>) {
    if plan.root_quest.signature != "QUST" || plan.target_quest.signature != "QUST" {
        insert_issue(
            issues,
            plan,
            StoryGraphValidationCode::InvalidQuest,
            None,
            Some(plan.root_quest.clone()),
            None,
        );
    }

    let mut nodes_by_source = BTreeMap::new();
    let mut allocated_targets = BTreeSet::new();
    for node in &plan.nodes {
        if nodes_by_source.insert(node.source.clone(), node).is_some() {
            insert_issue(
                issues,
                plan,
                StoryGraphValidationCode::DuplicateSourceNode,
                Some(node.source.clone()),
                None,
                None,
            );
        }
        if !allocated_targets.insert(node.target.clone()) {
            insert_issue(
                issues,
                plan,
                StoryGraphValidationCode::DuplicateTargetAllocation,
                Some(node.source.clone()),
                None,
                Some(node.target.form_key.clone()),
            );
        }
        validate_node_shape(plan, node, issues);
    }

    for node in &plan.nodes {
        validate_links(plan, node, &nodes_by_source, issues);
        validate_parent_cycle(plan, node, &nodes_by_source, issues);
    }
    validate_routes(plan, &nodes_by_source, issues);
}

fn validate_node_shape(
    plan: &StoryGraphPlan,
    node: &StoryGraphNode,
    issues: &mut BTreeSet<StoryGraphValidationIssue>,
) {
    if node.source.signature != node.kind.signature()
        || node.target.signature != node.kind.signature()
    {
        insert_issue(
            issues,
            plan,
            StoryGraphValidationCode::UnsupportedNode,
            Some(node.source.clone()),
            None,
            Some(node.kind.signature().to_owned()),
        );
    }

    match node.kind {
        StoryNodeKind::EventRoot => {
            if node.parent.is_some() || node.quest.is_some() {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::UnsupportedRoot,
                    Some(node.source.clone()),
                    None,
                    None,
                );
            }
            match &node.event_root {
                Some(event)
                    if event.supported
                        && !event.event_type.trim().is_empty()
                        && event.target_event_root == node.target => {}
                _ => insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::UnsupportedEvent,
                    Some(node.source.clone()),
                    None,
                    None,
                ),
            }
        }
        StoryNodeKind::Branch => {
            if node.event_root.is_some() || node.quest.is_some() || node.parent.is_none() {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::UnsupportedNode,
                    Some(node.source.clone()),
                    None,
                    Some("branch_shape".to_owned()),
                );
            }
        }
        StoryNodeKind::Quest => {
            if node.event_root.is_some() || node.parent.is_none() || !node.children.is_empty() {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::UnsupportedNode,
                    Some(node.source.clone()),
                    None,
                    Some("quest_node_shape".to_owned()),
                );
            }
            if node.quest.as_ref() != Some(&plan.root_quest) {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::InvalidQuestOwnership,
                    Some(node.source.clone()),
                    node.quest.clone(),
                    None,
                );
            }
        }
    }
}

fn validate_links(
    plan: &StoryGraphPlan,
    node: &StoryGraphNode,
    nodes: &BTreeMap<QuestRecordKey, &StoryGraphNode>,
    issues: &mut BTreeSet<StoryGraphValidationIssue>,
) {
    if let Some(parent) = &node.parent {
        match nodes.get(parent) {
            None => insert_issue(
                issues,
                plan,
                StoryGraphValidationCode::MissingParent,
                Some(node.source.clone()),
                None,
                Some(parent.form_key.clone()),
            ),
            Some(parent_node) if !parent_node.children.contains(&node.source) => insert_issue(
                issues,
                plan,
                StoryGraphValidationCode::ParentChildMismatch,
                Some(node.source.clone()),
                None,
                Some(parent.form_key.clone()),
            ),
            Some(_) => {}
        }
    }

    for child in &node.children {
        match nodes.get(child) {
            None => insert_issue(
                issues,
                plan,
                StoryGraphValidationCode::ParentChildMismatch,
                Some(node.source.clone()),
                None,
                Some(child.form_key.clone()),
            ),
            Some(child_node) if child_node.parent.as_ref() != Some(&node.source) => insert_issue(
                issues,
                plan,
                StoryGraphValidationCode::ParentChildMismatch,
                Some(node.source.clone()),
                None,
                Some(child.form_key.clone()),
            ),
            Some(_) => {}
        }
    }
}

fn validate_parent_cycle(
    plan: &StoryGraphPlan,
    node: &StoryGraphNode,
    nodes: &BTreeMap<QuestRecordKey, &StoryGraphNode>,
    issues: &mut BTreeSet<StoryGraphValidationIssue>,
) {
    let mut visited = BTreeSet::new();
    let mut current = Some(&node.source);
    while let Some(source) = current {
        if !visited.insert(source) {
            insert_issue(
                issues,
                plan,
                StoryGraphValidationCode::ParentCycle,
                Some(node.source.clone()),
                None,
                None,
            );
            return;
        }
        current = nodes
            .get(source)
            .and_then(|candidate| candidate.parent.as_ref());
    }
}

fn validate_routes(
    plan: &StoryGraphPlan,
    nodes: &BTreeMap<QuestRecordKey, &StoryGraphNode>,
    issues: &mut BTreeSet<StoryGraphValidationIssue>,
) {
    if plan.routes.is_empty() {
        insert_issue(
            issues,
            plan,
            StoryGraphValidationCode::MissingRoute,
            None,
            Some(plan.root_quest.clone()),
            None,
        );
        return;
    }
    if plan.routes.len() != 1 {
        insert_issue(
            issues,
            plan,
            StoryGraphValidationCode::MultipleRoutes,
            None,
            Some(plan.root_quest.clone()),
            Some(plan.routes.len().to_string()),
        );
        return;
    }

    let route = &plan.routes[0];
    if route.quest != plan.root_quest {
        insert_issue(
            issues,
            plan,
            StoryGraphValidationCode::RouteQuestMismatch,
            route.quest_node.clone(),
            Some(route.quest.clone()),
            None,
        );
    }

    match &route.disposition {
        StartDisposition::Autostart => {
            if !plan.nodes.is_empty() || route.quest_node.is_some() {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::InconsistentStartDisposition,
                    route.quest_node.clone(),
                    Some(route.quest.clone()),
                    Some("autostart".to_owned()),
                );
            }
        }
        StartDisposition::ExplicitScript { producer_id }
        | StartDisposition::InfoResultScript { producer_id }
        | StartDisposition::Controller { producer_id } => {
            if !plan.nodes.is_empty()
                || route.quest_node.is_some()
                || !has_one_proven_producer(plan, producer_id)
            {
                insert_issue(
                    issues,
                    plan,
                    if has_one_proven_producer(plan, producer_id) {
                        StoryGraphValidationCode::InconsistentStartDisposition
                    } else {
                        StoryGraphValidationCode::MissingProvenProducer
                    },
                    route.quest_node.clone(),
                    Some(route.quest.clone()),
                    Some(producer_id.clone()),
                );
            }
        }
        StartDisposition::StoryManager { route_id } => {
            let Some(quest_node_key) = &route.quest_node else {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::InconsistentStartDisposition,
                    None,
                    Some(route.quest.clone()),
                    Some("story_manager_without_quest_node".to_owned()),
                );
                return;
            };
            let Some(quest_node) = nodes.get(quest_node_key) else {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::InconsistentStartDisposition,
                    Some(quest_node_key.clone()),
                    Some(route.quest.clone()),
                    Some("missing_story_quest_node".to_owned()),
                );
                return;
            };
            if route_id != &route.route_id
                || quest_node.kind != StoryNodeKind::Quest
                || quest_node.quest.as_ref() != Some(&route.quest)
            {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::InconsistentStartDisposition,
                    Some(quest_node_key.clone()),
                    Some(route.quest.clone()),
                    Some(route.route_id.clone()),
                );
                return;
            }
            let mut routed_nodes = BTreeSet::new();
            let mut current = Some(quest_node_key);
            while let Some(source) = current {
                if !routed_nodes.insert(source.clone()) {
                    break;
                }
                current = nodes.get(source).and_then(|node| node.parent.as_ref());
            }
            for source in nodes.keys() {
                if !routed_nodes.contains(source) {
                    insert_issue(
                        issues,
                        plan,
                        StoryGraphValidationCode::UnsupportedNode,
                        Some(source.clone()),
                        Some(route.quest.clone()),
                        Some("not_on_proven_route".to_owned()),
                    );
                }
            }
            let Some(root) = route_root(quest_node_key, nodes) else {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::UnsupportedRoot,
                    Some(quest_node_key.clone()),
                    Some(route.quest.clone()),
                    Some("route_has_no_event_root".to_owned()),
                );
                return;
            };
            let producer_id = root
                .event_root
                .as_ref()
                .map(|event| event.producer_id.as_str())
                .unwrap_or_default();
            if !has_one_proven_producer(plan, producer_id) {
                insert_issue(
                    issues,
                    plan,
                    StoryGraphValidationCode::MissingProvenProducer,
                    Some(root.source.clone()),
                    Some(route.quest.clone()),
                    Some(producer_id.to_owned()),
                );
            }
        }
        StartDisposition::OrphanStartUnknown => insert_issue(
            issues,
            plan,
            StoryGraphValidationCode::InconsistentStartDisposition,
            route.quest_node.clone(),
            Some(route.quest.clone()),
            Some("orphan_start_unknown".to_owned()),
        ),
    }
}

fn route_root<'a>(
    quest_node: &QuestRecordKey,
    nodes: &'a BTreeMap<QuestRecordKey, &StoryGraphNode>,
) -> Option<&'a StoryGraphNode> {
    let mut current = nodes.get(quest_node).copied()?;
    let mut visited = BTreeSet::new();
    while let Some(parent) = &current.parent {
        if !visited.insert(current.source.clone()) {
            return None;
        }
        current = nodes.get(parent).copied()?;
    }
    (current.kind == StoryNodeKind::EventRoot).then_some(current)
}

fn has_one_proven_producer(plan: &StoryGraphPlan, producer_id: &str) -> bool {
    !producer_id.trim().is_empty()
        && plan
            .producers
            .iter()
            .filter(|producer| {
                producer.producer_id == producer_id
                    && producer.proven
                    && !producer.producer_kind.trim().is_empty()
                    && !producer.evidence_id.trim().is_empty()
            })
            .count()
            == 1
}

fn deterministic_node_order(plan: &StoryGraphPlan) -> Vec<QuestRecordKey> {
    let nodes: BTreeMap<_, _> = plan.nodes.iter().map(|node| (&node.source, node)).collect();
    let mut ordered: Vec<_> = plan
        .nodes
        .iter()
        .map(|node| (node_depth(node, &nodes), node.source.clone()))
        .collect();
    ordered.sort();
    ordered.into_iter().map(|(_, source)| source).collect()
}

fn node_depth(node: &StoryGraphNode, nodes: &BTreeMap<&QuestRecordKey, &StoryGraphNode>) -> usize {
    let mut depth = 0;
    let mut current = node.parent.as_ref();
    while let Some(parent) = current {
        depth += 1;
        current = nodes
            .get(parent)
            .and_then(|candidate| candidate.parent.as_ref());
    }
    depth
}

fn build_route_receipt(
    plan: &StoryGraphPlan,
    nodes: &BTreeMap<&QuestRecordKey, &StoryGraphNode>,
) -> StoryGraphRouteReceipt {
    let route = &plan.routes[0];
    let (producer_evidence_id, node_chain) = match &route.disposition {
        StartDisposition::Autostart => (
            format!("autostart:{}", plan.target_quest.form_key),
            Vec::new(),
        ),
        StartDisposition::ExplicitScript { producer_id }
        | StartDisposition::InfoResultScript { producer_id }
        | StartDisposition::Controller { producer_id } => {
            (producer_evidence_id(plan, producer_id), Vec::new())
        }
        StartDisposition::StoryManager { .. } => {
            let mut source_chain = Vec::new();
            let mut current = route.quest_node.as_ref();
            while let Some(source) = current {
                let node = nodes[source];
                source_chain.push(node.target.clone());
                current = node.parent.as_ref();
            }
            source_chain.reverse();
            let mut root = nodes[route.quest_node.as_ref().expect("validated quest node")];
            while let Some(parent) = &root.parent {
                root = nodes[parent];
            }
            let producer_id = &root
                .event_root
                .as_ref()
                .expect("validated event root")
                .producer_id;
            (producer_evidence_id(plan, producer_id), source_chain)
        }
        StartDisposition::OrphanStartUnknown => unreachable!("validated disposition"),
    };

    let node_kinds = node_chain
        .iter()
        .map(|target| {
            let node = plan
                .nodes
                .iter()
                .find(|node| &node.target == target)
                .expect("validated target allocation");
            (target.clone(), node.kind)
        })
        .collect();
    let conditions = node_chain
        .iter()
        .map(|target| {
            let node = plan
                .nodes
                .iter()
                .find(|node| &node.target == target)
                .expect("validated target allocation");
            (target.clone(), node.conditions.clone())
        })
        .collect();

    StoryGraphRouteReceipt {
        route: StartRouteReceipt {
            route_id: route.route_id.clone(),
            quest: plan.target_quest.clone(),
            producer_evidence_id,
            node_chain,
        },
        node_kinds,
        conditions,
    }
}

fn producer_evidence_id(plan: &StoryGraphPlan, producer_id: &str) -> String {
    plan.producers
        .iter()
        .find(|producer| producer.producer_id == producer_id && producer.proven)
        .expect("validated producer")
        .evidence_id
        .clone()
}

fn occurrence_counts<T: Ord>(items: &[T]) -> BTreeMap<&T, usize> {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
    }
    counts
}

fn insert_issue(
    issues: &mut BTreeSet<StoryGraphValidationIssue>,
    plan: &StoryGraphPlan,
    code: StoryGraphValidationCode,
    node: Option<QuestRecordKey>,
    quest: Option<QuestRecordKey>,
    subject: Option<String>,
) {
    issues.insert(StoryGraphValidationIssue {
        code,
        graph_id: plan.graph_id.clone(),
        node,
        quest,
        subject,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(signature: &str, form_key: &str) -> QuestRecordKey {
        QuestRecordKey::new(signature, form_key)
    }

    fn valid_story_plan(graph_id: &str, source_quest: &str) -> StoryGraphPlan {
        let quest = key("QUST", source_quest);
        let target_quest = key("QUST", "000400:Fallout4.esm");
        let root = key("SMEN", "000100:Skyrim.esm");
        let branch = key("SMBN", "000200:Skyrim.esm");
        let quest_node = key("SMQN", "000300:Skyrim.esm");
        let target_root = key("SMEN", "000100:Fallout4.esm");
        let target_branch = key("SMBN", "000200:Port.esp");
        let target_node = key("SMQN", "000300:Port.esp");
        StoryGraphPlan {
            graph_id: graph_id.to_owned(),
            root_quest: quest.clone(),
            target_quest,
            nodes: vec![
                StoryGraphNode {
                    source: quest_node.clone(),
                    target: target_node,
                    kind: StoryNodeKind::Quest,
                    parent: Some(branch.clone()),
                    children: BTreeSet::new(),
                    event_root: None,
                    quest: Some(quest.clone()),
                    conditions: BTreeSet::new(),
                },
                StoryGraphNode {
                    source: root.clone(),
                    target: target_root.clone(),
                    kind: StoryNodeKind::EventRoot,
                    parent: None,
                    children: BTreeSet::from([branch.clone()]),
                    event_root: Some(StoryEventRootIntent {
                        event_type: "script_event".to_owned(),
                        target_event_root: target_root,
                        supported: true,
                        producer_id: "story_event_producer".to_owned(),
                    }),
                    quest: None,
                    conditions: BTreeSet::new(),
                },
                StoryGraphNode {
                    source: branch.clone(),
                    target: target_branch,
                    kind: StoryNodeKind::Branch,
                    parent: Some(root.clone()),
                    children: BTreeSet::from([quest_node.clone()]),
                    event_root: None,
                    quest: None,
                    conditions: BTreeSet::new(),
                },
            ],
            producers: BTreeSet::from([StartProducerIntent {
                producer_id: "story_event_producer".to_owned(),
                carrier: root,
                producer_kind: "papyrus_event".to_owned(),
                evidence_id: "pex:story_event_producer".to_owned(),
                proven: true,
            }]),
            routes: vec![StoryRouteIntent {
                route_id: "route:story".to_owned(),
                quest,
                disposition: StartDisposition::StoryManager {
                    route_id: "route:story".to_owned(),
                },
                quest_node: Some(quest_node),
            }],
        }
    }

    fn issue_codes(plan: &StoryGraphPlan) -> BTreeSet<StoryGraphValidationCode> {
        plan.validation_issues()
            .into_iter()
            .map(|issue| issue.code)
            .collect()
    }

    #[test]
    fn valid_chain_has_deterministic_allocations_and_route_receipt() {
        let plan = valid_story_plan("valid", "000400:Skyrim.esm");
        let validated = validate_story_graph(&plan).unwrap();

        assert_eq!(
            validated.ordered_nodes,
            vec![
                key("SMEN", "000100:Skyrim.esm"),
                key("SMBN", "000200:Skyrim.esm"),
                key("SMQN", "000300:Skyrim.esm"),
            ]
        );
        assert_eq!(
            validated
                .allocations
                .iter()
                .map(|mapping| mapping.target.clone())
                .collect::<Vec<_>>(),
            vec![
                key("SMEN", "000100:Fallout4.esm"),
                key("SMBN", "000200:Port.esp"),
                key("SMQN", "000300:Port.esp"),
            ]
        );
        assert_eq!(
            validated.route_receipt.route.node_chain,
            vec![
                key("SMEN", "000100:Fallout4.esm"),
                key("SMBN", "000200:Port.esp"),
                key("SMQN", "000300:Port.esp"),
            ]
        );
        assert_eq!(
            validated.route_receipt.route.producer_evidence_id,
            "pex:story_event_producer"
        );
    }

    #[test]
    fn missing_parent_is_rejected() {
        let mut plan = valid_story_plan("missing_parent", "000400:Skyrim.esm");
        plan.nodes[0].parent = Some(key("SMBN", "00DEAD:Skyrim.esm"));
        assert!(issue_codes(&plan).contains(&StoryGraphValidationCode::MissingParent));
    }

    #[test]
    fn parent_cycle_is_rejected() {
        let mut plan = valid_story_plan("cycle", "000400:Skyrim.esm");
        let branch = key("SMBN", "000200:Skyrim.esm");
        let quest_node = key("SMQN", "000300:Skyrim.esm");
        plan.nodes[0].children.insert(branch.clone());
        plan.nodes[2].parent = Some(quest_node);
        assert!(issue_codes(&plan).contains(&StoryGraphValidationCode::ParentCycle));
    }

    #[test]
    fn unsupported_root_and_event_are_rejected() {
        let mut unsupported_event = valid_story_plan("event", "000400:Skyrim.esm");
        unsupported_event.nodes[1]
            .event_root
            .as_mut()
            .unwrap()
            .supported = false;
        assert!(
            issue_codes(&unsupported_event).contains(&StoryGraphValidationCode::UnsupportedEvent)
        );

        let mut unsupported_root = valid_story_plan("root", "000400:Skyrim.esm");
        unsupported_root.nodes[1].parent = Some(key("SMEN", "00DEAD:Skyrim.esm"));
        assert!(
            issue_codes(&unsupported_root).contains(&StoryGraphValidationCode::UnsupportedRoot)
        );
    }

    #[test]
    fn duplicate_quest_ownership_is_rejected() {
        let first = valid_story_plan("first", "000400:Skyrim.esm");
        let second = valid_story_plan("second", "000400:Skyrim.esm");
        let issues = validate_story_graph_plans(&[first, second]);
        assert!(issues.iter().any(|issue| {
            issue.code == StoryGraphValidationCode::DuplicateQuestOwnership
                && issue.graph_id == "second"
                && issue.subject.as_deref() == Some("first")
        }));
    }

    #[test]
    fn absent_producer_is_rejected() {
        let mut plan = valid_story_plan("absent_producer", "000400:Skyrim.esm");
        plan.producers.clear();
        assert!(issue_codes(&plan).contains(&StoryGraphValidationCode::MissingProvenProducer));
    }

    #[test]
    fn autostart_without_story_nodes_is_valid() {
        let source_quest = key("QUST", "000400:FalloutNV.esm");
        let plan = StoryGraphPlan {
            graph_id: "autostart".to_owned(),
            root_quest: source_quest.clone(),
            target_quest: key("QUST", "000500:Port.esp"),
            nodes: Vec::new(),
            producers: BTreeSet::new(),
            routes: vec![StoryRouteIntent {
                route_id: "route:autostart".to_owned(),
                quest: source_quest,
                disposition: StartDisposition::Autostart,
                quest_node: None,
            }],
        };

        let validated = validate_story_graph(&plan).unwrap();
        assert!(validated.ordered_nodes.is_empty());
        assert!(validated.route_receipt.route.node_chain.is_empty());
        assert_eq!(
            validated.route_receipt.route.producer_evidence_id,
            "autostart:000500:Port.esp"
        );
        assert!(!plan.production_emission_enabled());
    }

    #[test]
    fn ordering_and_route_receipt_are_input_order_independent_and_multiplicity_aware() {
        let first = valid_story_plan("deterministic", "000400:Skyrim.esm");
        let mut second = first.clone();
        second.nodes.reverse();
        let first = validate_story_graph(&first).unwrap();
        let second = validate_story_graph(&second).unwrap();
        assert_eq!(first, second);

        let expected = vec![first.route_receipt.clone()];
        let actual = vec![first.route_receipt.clone(), first.route_receipt.clone()];
        assert_eq!(
            compare_story_route_receipts(&expected, &actual).damage,
            vec![StoryRouteReceiptDamage::Unexpected {
                route: first.route_receipt,
                occurrences: 1,
            }]
        );
    }
}
