use std::collections::BTreeSet;

use crate::quest_runtime::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FnvStartRouteEvidence {
    pub source_story_manager_records: BTreeSet<QuestRecordKey>,
    pub candidates: Vec<FnvStartCandidate>,
}

impl Default for FnvStartRouteEvidence {
    fn default() -> Self {
        Self {
            source_story_manager_records: BTreeSet::new(),
            candidates: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FnvStartCandidate {
    ExplicitScript {
        producer: StartProducerIntent,
        compiled: bool,
        vmad_attached: bool,
    },
    InfoResultScript {
        producer: StartProducerIntent,
        compiled: bool,
        vmad_attached: bool,
    },
    VerifiedTargetEvent {
        producer: StartProducerIntent,
        event_type: String,
        event_supported: bool,
        event_root_source: QuestRecordKey,
        event_root_target: QuestRecordKey,
        branch_source: QuestRecordKey,
        branch_target: QuestRecordKey,
        quest_node_source: QuestRecordKey,
        quest_node_target: QuestRecordKey,
    },
    Controller {
        producer: StartProducerIntent,
        contract_id: String,
        compiled: bool,
        vmad_attached: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FnvStartRoutePolicyError {
    UnsupportedProvenance,
    SourceStoryManagerUnexpected,
    DuplicateStart,
    MissingProducerEvidence,
    MissingTargetQuest,
    SharedGraphRejected,
}

pub(crate) fn apply_fnv_start_route_policy(
    component: &mut QuestRuntimeComponentPlan,
    evidence: &FnvStartRouteEvidence,
) -> Result<StoryGraphPlan, FnvStartRoutePolicyError> {
    if !matches!(
        component.provenance.game,
        QuestSourceGame::Fnv | QuestSourceGame::Fo3
    ) {
        return reject(
            component,
            FnvStartRoutePolicyError::UnsupportedProvenance,
            false,
        );
    }
    if !evidence.source_story_manager_records.is_empty() {
        return reject(
            component,
            FnvStartRoutePolicyError::SourceStoryManagerUnexpected,
            false,
        );
    }

    let autostart = component
        .semantics
        .start_flags
        .contains(&QuestStartFlag::StartGameEnabled);
    if (autostart && !evidence.candidates.is_empty()) || evidence.candidates.len() > 1 {
        return reject(component, FnvStartRoutePolicyError::DuplicateStart, false);
    }

    let Some(target_quest) = mapped_target(component, &component.root_quest) else {
        return reject(
            component,
            FnvStartRoutePolicyError::MissingTargetQuest,
            false,
        );
    };
    let route_id = route_id(component, if autostart { "autostart" } else { "producer" });
    let graph = if autostart {
        StoryGraphPlan {
            graph_id: route_id.clone(),
            root_quest: component.root_quest.clone(),
            target_quest,
            nodes: Vec::new(),
            producers: BTreeSet::new(),
            routes: vec![StoryRouteIntent {
                route_id,
                quest: component.root_quest.clone(),
                disposition: StartDisposition::Autostart,
                quest_node: None,
            }],
        }
    } else {
        let Some(candidate) = evidence.candidates.first() else {
            return reject(
                component,
                FnvStartRoutePolicyError::MissingProducerEvidence,
                true,
            );
        };
        graph_for_candidate(component, target_quest, candidate)?
    };

    let validated = validate_story_graph(&graph).map_err(|_| {
        mark_rejected(component, false);
        FnvStartRoutePolicyError::SharedGraphRejected
    })?;
    let route = graph.routes.first().expect("FNV policy emits one route");
    component.inbound_producers = graph.producers.clone();
    component.start_disposition = Some(route.disposition.clone());
    component.expected_receipt.routes = BTreeSet::from([validated.route_receipt.route]);
    if !autostart {
        component
            .semantics
            .start_flags
            .remove(&QuestStartFlag::StartGameEnabled);
    }
    Ok(graph)
}

fn graph_for_candidate(
    component: &mut QuestRuntimeComponentPlan,
    target_quest: QuestRecordKey,
    candidate: &FnvStartCandidate,
) -> Result<StoryGraphPlan, FnvStartRoutePolicyError> {
    let (producer, disposition, nodes, quest_node) = match candidate {
        FnvStartCandidate::ExplicitScript {
            producer,
            compiled,
            vmad_attached,
        } if producer_is_ready(producer, *compiled, *vmad_attached) => (
            producer,
            StartDisposition::ExplicitScript {
                producer_id: producer.producer_id.clone(),
            },
            Vec::new(),
            None,
        ),
        FnvStartCandidate::InfoResultScript {
            producer,
            compiled,
            vmad_attached,
        } if producer_is_ready(producer, *compiled, *vmad_attached) => (
            producer,
            StartDisposition::InfoResultScript {
                producer_id: producer.producer_id.clone(),
            },
            Vec::new(),
            None,
        ),
        FnvStartCandidate::Controller {
            producer,
            contract_id,
            compiled,
            vmad_attached,
        } if !contract_id.trim().is_empty()
            && producer_is_ready(producer, *compiled, *vmad_attached) =>
        {
            (
                producer,
                StartDisposition::Controller {
                    producer_id: producer.producer_id.clone(),
                },
                Vec::new(),
                None,
            )
        }
        FnvStartCandidate::VerifiedTargetEvent {
            producer,
            event_type,
            event_supported,
            event_root_source,
            event_root_target,
            branch_source,
            branch_target,
            quest_node_source,
            quest_node_target,
        } if *event_supported && producer.proven && producer.carrier.signature != "SCPT" => {
            let nodes = vec![
                StoryGraphNode {
                    source: event_root_source.clone(),
                    target: event_root_target.clone(),
                    kind: StoryNodeKind::EventRoot,
                    parent: None,
                    children: BTreeSet::from([branch_source.clone()]),
                    event_root: Some(StoryEventRootIntent {
                        event_type: event_type.clone(),
                        target_event_root: event_root_target.clone(),
                        supported: true,
                        producer_id: producer.producer_id.clone(),
                    }),
                    quest: None,
                    conditions: BTreeSet::new(),
                },
                StoryGraphNode {
                    source: branch_source.clone(),
                    target: branch_target.clone(),
                    kind: StoryNodeKind::Branch,
                    parent: Some(event_root_source.clone()),
                    children: BTreeSet::from([quest_node_source.clone()]),
                    event_root: None,
                    quest: None,
                    conditions: BTreeSet::new(),
                },
                StoryGraphNode {
                    source: quest_node_source.clone(),
                    target: quest_node_target.clone(),
                    kind: StoryNodeKind::Quest,
                    parent: Some(branch_source.clone()),
                    children: BTreeSet::new(),
                    event_root: None,
                    quest: Some(component.root_quest.clone()),
                    conditions: BTreeSet::new(),
                },
            ];
            (
                producer,
                StartDisposition::StoryManager {
                    route_id: producer.producer_id.clone(),
                },
                nodes,
                Some(quest_node_source.clone()),
            )
        }
        _ => {
            return reject(
                component,
                FnvStartRoutePolicyError::MissingProducerEvidence,
                true,
            );
        }
    };
    let route_id = route_id(component, &producer.producer_id);
    let disposition = if matches!(disposition, StartDisposition::StoryManager { .. }) {
        StartDisposition::StoryManager {
            route_id: route_id.clone(),
        }
    } else {
        disposition
    };
    Ok(StoryGraphPlan {
        graph_id: route_id.clone(),
        root_quest: component.root_quest.clone(),
        target_quest,
        nodes,
        producers: BTreeSet::from([producer.clone()]),
        routes: vec![StoryRouteIntent {
            route_id,
            quest: component.root_quest.clone(),
            disposition,
            quest_node,
        }],
    })
}

fn producer_is_ready(producer: &StartProducerIntent, compiled: bool, vmad_attached: bool) -> bool {
    producer.proven
        && compiled
        && vmad_attached
        && !producer.producer_id.trim().is_empty()
        && !producer.evidence_id.trim().is_empty()
}

fn mapped_target(
    component: &QuestRuntimeComponentPlan,
    source: &QuestRecordKey,
) -> Option<QuestRecordKey> {
    component
        .mappings
        .iter()
        .find(|mapping| &mapping.source == source)
        .map(|mapping| mapping.target.clone())
}

fn route_id(component: &QuestRuntimeComponentPlan, suffix: &str) -> String {
    let game = match component.provenance.game {
        QuestSourceGame::Fnv => "fnv",
        QuestSourceGame::Fo3 => "fo3",
        QuestSourceGame::SkyrimSe => "skyrimse",
    };
    format!(
        "fnvfo3:start:{game}:{}:{}:{suffix}",
        component.provenance.source_plugin.to_ascii_lowercase(),
        component.root_quest.form_key.to_ascii_lowercase()
    )
}

fn reject<T>(
    component: &mut QuestRuntimeComponentPlan,
    error: FnvStartRoutePolicyError,
    orphan: bool,
) -> Result<T, FnvStartRoutePolicyError> {
    mark_rejected(component, orphan);
    Err(error)
}

fn mark_rejected(component: &mut QuestRuntimeComponentPlan, orphan: bool) {
    let mut reasons = match &component.admission {
        QuestRuntimeAdmission::Supported => BTreeSet::new(),
        QuestRuntimeAdmission::Rejected { reasons } => reasons.clone(),
    };
    reasons.insert(QuestRuntimeRejectionReason::MissingStartProducer);
    if orphan {
        reasons.insert(QuestRuntimeRejectionReason::OrphanStartUnknown);
        component.start_disposition = Some(StartDisposition::OrphanStartUnknown);
    }
    component.admission = QuestRuntimeAdmission::Rejected { reasons };
    component.inbound_producers.clear();
    component.expected_receipt.routes.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(signature: &str, local: &str, plugin: &str) -> QuestRecordKey {
        QuestRecordKey::new(signature, format!("{local}:{plugin}"))
    }

    fn component(game: QuestSourceGame, start_game_enabled: bool) -> QuestRuntimeComponentPlan {
        let source_plugin = match game {
            QuestSourceGame::Fo3 => "Fallout3.esm",
            QuestSourceGame::Fnv => "FalloutNV.esm",
            QuestSourceGame::SkyrimSe => "Skyrim.esm",
        };
        let root = key("QUST", "11F935", source_plugin);
        let target = key("QUST", "200100", "Converted.esp");
        QuestRuntimeComponentPlan {
            component_id: "fnvfo3:start-policy:11f935".to_owned(),
            root_quest: root.clone(),
            provenance: SourceProvenance {
                game,
                source_plugin: source_plugin.to_owned(),
                graft: None,
            },
            owned_records: vec![root.clone()],
            shared_records: BTreeSet::new(),
            dependencies: DependencyClosure::default(),
            source_topology: BTreeSet::new(),
            target_topology: BTreeSet::new(),
            mappings: vec![SourceTargetFormMapping {
                source: root,
                target,
            }],
            semantics: QuestSemanticPlan {
                start_flags: start_game_enabled
                    .then_some(QuestStartFlag::StartGameEnabled)
                    .into_iter()
                    .collect(),
                ..QuestSemanticPlan::default()
            },
            scripts: BTreeSet::new(),
            fragments: BTreeSet::new(),
            psc: BTreeSet::new(),
            vmad: BTreeSet::new(),
            assets: BTreeSet::new(),
            inbound_producers: BTreeSet::new(),
            start_disposition: None,
            admission: QuestRuntimeAdmission::Supported,
            expected_receipt: QuestRuntimeExpectedReceipt::default(),
        }
    }

    fn producer(
        component: &QuestRuntimeComponentPlan,
        id: &str,
        kind: &str,
    ) -> StartProducerIntent {
        StartProducerIntent {
            producer_id: id.to_owned(),
            carrier: component.root_quest.clone(),
            producer_kind: kind.to_owned(),
            evidence_id: format!("pex:{id}"),
            proven: true,
        }
    }

    fn explicit(component: &QuestRuntimeComponentPlan, id: &str) -> FnvStartCandidate {
        FnvStartCandidate::ExplicitScript {
            producer: producer(component, id, "compiled_startquest"),
            compiled: true,
            vmad_attached: true,
        }
    }

    #[test]
    fn autostart_preserves_flag_without_story_manager_nodes() {
        let mut component = component(QuestSourceGame::Fnv, true);
        let graph = apply_fnv_start_route_policy(&mut component, &FnvStartRouteEvidence::default())
            .unwrap();

        assert!(graph.nodes.is_empty());
        assert!(graph.producers.is_empty());
        assert_eq!(
            component.start_disposition,
            Some(StartDisposition::Autostart)
        );
        assert!(
            component
                .semantics
                .start_flags
                .contains(&QuestStartFlag::StartGameEnabled)
        );
        assert_eq!(component.expected_receipt.routes.len(), 1);
        assert!(
            component
                .expected_receipt
                .routes
                .iter()
                .all(|route| route.node_chain.is_empty())
        );
    }

    #[test]
    fn explicit_compiled_producer_is_preserved_with_fo3_provenance() {
        let mut component = component(QuestSourceGame::Fo3, false);
        let candidate = explicit(&component, "fo3_startquest_11f935");
        let graph = apply_fnv_start_route_policy(
            &mut component,
            &FnvStartRouteEvidence {
                source_story_manager_records: BTreeSet::new(),
                candidates: vec![candidate],
            },
        )
        .unwrap();

        assert!(graph.graph_id.contains(":fo3:fallout3.esm:"));
        assert!(graph.nodes.is_empty());
        assert!(matches!(
            component.start_disposition,
            Some(StartDisposition::ExplicitScript { ref producer_id })
                if producer_id == "fo3_startquest_11f935"
        ));
        assert_eq!(
            component
                .expected_receipt
                .routes
                .iter()
                .next()
                .unwrap()
                .producer_evidence_id,
            "pex:fo3_startquest_11f935"
        );
    }

    #[test]
    fn info_result_producer_requires_compiled_vmad_evidence() {
        let mut component = component(QuestSourceGame::Fnv, false);
        let info = FnvStartCandidate::InfoResultScript {
            producer: producer(&component, "info_130161_setstage", "compiled_info_result"),
            compiled: true,
            vmad_attached: true,
        };
        let graph = apply_fnv_start_route_policy(
            &mut component,
            &FnvStartRouteEvidence {
                source_story_manager_records: BTreeSet::new(),
                candidates: vec![info],
            },
        )
        .unwrap();

        assert!(graph.nodes.is_empty());
        assert!(matches!(
            component.start_disposition,
            Some(StartDisposition::InfoResultScript { .. })
        ));
    }

    #[test]
    fn missing_producer_rejects_orphaned_start() {
        let mut component = component(QuestSourceGame::Fnv, false);
        let error = apply_fnv_start_route_policy(&mut component, &FnvStartRouteEvidence::default())
            .unwrap_err();

        assert_eq!(error, FnvStartRoutePolicyError::MissingProducerEvidence);
        assert_eq!(
            component.start_disposition,
            Some(StartDisposition::OrphanStartUnknown)
        );
        assert!(matches!(
            component.admission,
            QuestRuntimeAdmission::Rejected { ref reasons }
                if reasons.contains(&QuestRuntimeRejectionReason::OrphanStartUnknown)
                    && reasons.contains(&QuestRuntimeRejectionReason::MissingStartProducer)
        ));
    }

    #[test]
    fn duplicate_start_paths_are_rejected() {
        let mut component = component(QuestSourceGame::Fnv, false);
        let first = explicit(&component, "start_one");
        let second = explicit(&component, "start_two");
        let error = apply_fnv_start_route_policy(
            &mut component,
            &FnvStartRouteEvidence {
                source_story_manager_records: BTreeSet::new(),
                candidates: vec![first, second],
            },
        )
        .unwrap_err();

        assert_eq!(error, FnvStartRoutePolicyError::DuplicateStart);
        assert!(component.inbound_producers.is_empty());
        assert!(component.expected_receipt.routes.is_empty());
    }

    #[test]
    fn absent_source_story_manager_does_not_create_decorative_scpt_nodes() {
        let mut component = component(QuestSourceGame::Fnv, false);
        let candidate = explicit(&component, "placed_actor_startquest");
        let graph = apply_fnv_start_route_policy(
            &mut component,
            &FnvStartRouteEvidence {
                source_story_manager_records: BTreeSet::new(),
                candidates: vec![candidate],
            },
        )
        .unwrap();

        assert!(graph.nodes.is_empty());
        assert!(
            graph
                .producers
                .iter()
                .all(|producer| producer.carrier.signature != "SCPT")
        );
    }

    #[test]
    fn verified_target_event_is_the_only_story_manager_synthesis_path() {
        let mut component = component(QuestSourceGame::Fnv, false);
        let event_root_source = key("SMEN", "intent_event", "FalloutNV.esm");
        let event = FnvStartCandidate::VerifiedTargetEvent {
            producer: producer(&component, "route:verified_event", "verified_target_event"),
            event_type: "OnLocationChange".to_owned(),
            event_supported: true,
            event_root_source: event_root_source.clone(),
            event_root_target: key("SMEN", "300100", "Converted.esp"),
            branch_source: key("SMBN", "intent_branch", "FalloutNV.esm"),
            branch_target: key("SMBN", "300101", "Converted.esp"),
            quest_node_source: key("SMQN", "intent_quest", "FalloutNV.esm"),
            quest_node_target: key("SMQN", "300102", "Converted.esp"),
        };
        let graph = apply_fnv_start_route_policy(
            &mut component,
            &FnvStartRouteEvidence {
                source_story_manager_records: BTreeSet::new(),
                candidates: vec![event],
            },
        )
        .unwrap();

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].source, event_root_source);
        assert!(matches!(
            component.start_disposition,
            Some(StartDisposition::StoryManager { .. })
        ));
        assert_eq!(
            component
                .expected_receipt
                .routes
                .iter()
                .next()
                .unwrap()
                .node_chain
                .len(),
            3
        );
    }

    #[test]
    fn controller_route_requires_an_explicit_evidence_contract() {
        let mut component = component(QuestSourceGame::Fnv, false);
        let controller = FnvStartCandidate::Controller {
            producer: producer(&component, "controller_bootstrap", "compiled_controller"),
            contract_id: "fnv:controller:world_event".to_owned(),
            compiled: true,
            vmad_attached: true,
        };
        let graph = apply_fnv_start_route_policy(
            &mut component,
            &FnvStartRouteEvidence {
                source_story_manager_records: BTreeSet::new(),
                candidates: vec![controller],
            },
        )
        .unwrap();

        assert!(graph.nodes.is_empty());
        assert!(matches!(
            component.start_disposition,
            Some(StartDisposition::Controller { .. })
        ));
    }
}
