use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use thiserror::Error;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DialogueEdgeKind {
    LinkTo,
    LinkFrom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogueEdge {
    pub from_topic: FormKey,
    pub to_topic: FormKey,
    pub kind: DialogueEdgeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueGraph {
    pub roots: Vec<FormKey>,
    pub topics: Vec<FormKey>,
    pub edges: Vec<DialogueEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneBranchPlan {
    pub quest: FormKey,
    pub scene: FormKey,
    pub branch: FormKey,
    pub starting_topic: FormKey,
    pub ordered_topics: Vec<FormKey>,
    pub edges: Vec<DialogueEdge>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DialogueGraphError {
    #[error("dialogue graph has no top-level topic")]
    MissingRoot,
    #[error("dialogue graph references topic {topic:06X} outside the quest")]
    ExternalTopic { topic: u32 },
    #[error("dialogue graph exceeded {max_topics} topics")]
    TopicLimit { max_topics: usize },
    #[error("dialogue graph exceeded depth {max_depth}")]
    DepthLimit { max_depth: usize },
    #[error("dialogue graph has a cycle through topic {topic:06X}")]
    Cycle { topic: u32 },
    #[error("dialogue graph topic {topic:06X} has no target identity")]
    UnmappedTopic { topic: u32 },
}

pub fn build_legacy_dialogue_graph(
    topics: &[Record],
    infos_by_parent: &[(FormKey, Record)],
    interner: &StringInterner,
) -> DialogueGraph {
    let mut roots = topics
        .iter()
        .filter(|topic| is_top_level_topic(topic, interner))
        .map(|topic| topic.form_key)
        .collect::<Vec<_>>();
    let mut topic_keys = topics
        .iter()
        .map(|topic| topic.form_key)
        .collect::<Vec<_>>();
    let mut edges = Vec::new();
    for (parent, info) in infos_by_parent {
        for field in &info.fields {
            let kind = match field.sig.0 {
                sig if sig == *b"TCLT" => Some(DialogueEdgeKind::LinkTo),
                sig if sig == *b"TCLF" => Some(DialogueEdgeKind::LinkFrom),
                _ => None,
            };
            if let (Some(kind), Some(target)) = (kind, first_form_key(&field.value)) {
                edges.push(DialogueEdge {
                    from_topic: *parent,
                    to_topic: target,
                    kind,
                });
            }
        }
    }
    roots.sort_by_key(|topic| topic.local);
    roots.dedup();
    topic_keys.sort_by_key(|topic| topic.local);
    topic_keys.dedup();
    edges.sort_by_key(|edge| {
        (
            edge.from_topic.local,
            edge.to_topic.local,
            match edge.kind {
                DialogueEdgeKind::LinkTo => 0,
                DialogueEdgeKind::LinkFrom => 1,
            },
        )
    });
    edges.dedup();
    DialogueGraph {
        roots,
        topics: topic_keys,
        edges,
    }
}

pub fn bounded_scene_branch_from_graph(
    quest: FormKey,
    scene: FormKey,
    branch: FormKey,
    graph: &DialogueGraph,
    max_topics: usize,
    max_depth: usize,
) -> Result<SceneBranchPlan, DialogueGraphError> {
    build_bounded_scene_branch_plan(
        quest,
        scene,
        branch,
        &graph.roots,
        &graph.topics,
        &graph.edges,
        max_topics,
        max_depth,
    )
}

pub fn build_bounded_scene_branch_plan(
    quest: FormKey,
    scene: FormKey,
    branch: FormKey,
    top_level_topics: &[FormKey],
    all_topics: &[FormKey],
    links: &[DialogueEdge],
    max_topics: usize,
    max_depth: usize,
) -> Result<SceneBranchPlan, DialogueGraphError> {
    if top_level_topics.is_empty() {
        return Err(DialogueGraphError::MissingRoot);
    }
    let topic_set = all_topics.iter().copied().collect::<HashSet<_>>();
    for root in top_level_topics {
        if !topic_set.contains(root) {
            return Err(DialogueGraphError::ExternalTopic { topic: root.local });
        }
    }
    for edge in links {
        for topic in [edge.from_topic, edge.to_topic] {
            if !topic_set.contains(&topic) {
                return Err(DialogueGraphError::ExternalTopic { topic: topic.local });
            }
        }
    }
    let mut roots = top_level_topics.to_vec();
    roots.sort_by_key(|topic| topic.local);
    roots.dedup();
    let mut adjacency: BTreeMap<u32, Vec<DialogueEdge>> = BTreeMap::new();
    for edge in links {
        adjacency
            .entry(edge.from_topic.local)
            .or_default()
            .push(*edge);
    }
    for edges in adjacency.values_mut() {
        edges.sort_by_key(|edge| {
            (
                edge.to_topic.local,
                match edge.kind {
                    DialogueEdgeKind::LinkTo => 0,
                    DialogueEdgeKind::LinkFrom => 1,
                },
            )
        });
        edges.dedup();
    }

    let mut remaining = all_topics.to_vec();
    remaining.sort_by_key(|topic| topic.local);
    remaining.dedup();

    let mut visiting = HashSet::new();
    let mut completed = HashSet::new();
    for topic in roots.iter().chain(remaining.iter()) {
        validate_acyclic_component(*topic, &adjacency, &mut visiting, &mut completed)?;
    }
    let mut component_depths = HashMap::new();
    for topic in roots.iter().chain(remaining.iter()) {
        if maximum_component_depth(*topic, &adjacency, &mut component_depths) > max_depth {
            return Err(DialogueGraphError::DepthLimit { max_depth });
        }
    }

    let mut ordered = Vec::new();
    let mut discovered = HashSet::new();
    let mut queue = VecDeque::new();
    for root in &roots {
        if discovered.insert(*root) {
            queue.push_back(*root);
        }
    }
    loop {
        while let Some(topic) = queue.pop_front() {
            ordered.push(topic);
            if ordered.len() > max_topics {
                return Err(DialogueGraphError::TopicLimit { max_topics });
            }
            for edge in adjacency.get(&topic.local).into_iter().flatten() {
                if discovered.insert(edge.to_topic) {
                    queue.push_back(edge.to_topic);
                }
            }
        }
        let Some(next) = remaining
            .iter()
            .copied()
            .find(|topic| !discovered.contains(topic))
        else {
            break;
        };
        discovered.insert(next);
        queue.push_back(next);
    }
    Ok(SceneBranchPlan {
        quest,
        scene,
        branch,
        starting_topic: roots[0],
        ordered_topics: ordered,
        edges: links.to_vec(),
    })
}

fn validate_acyclic_component(
    topic: FormKey,
    adjacency: &BTreeMap<u32, Vec<DialogueEdge>>,
    visiting: &mut HashSet<FormKey>,
    completed: &mut HashSet<FormKey>,
) -> Result<(), DialogueGraphError> {
    if completed.contains(&topic) {
        return Ok(());
    }
    if !visiting.insert(topic) {
        return Err(DialogueGraphError::Cycle { topic: topic.local });
    }
    for edge in adjacency.get(&topic.local).into_iter().flatten() {
        validate_acyclic_component(edge.to_topic, adjacency, visiting, completed)?;
    }
    visiting.remove(&topic);
    completed.insert(topic);
    Ok(())
}

fn maximum_component_depth(
    topic: FormKey,
    adjacency: &BTreeMap<u32, Vec<DialogueEdge>>,
    depths: &mut HashMap<FormKey, usize>,
) -> usize {
    if let Some(depth) = depths.get(&topic) {
        return *depth;
    }
    let depth = adjacency
        .get(&topic.local)
        .into_iter()
        .flatten()
        .map(|edge| maximum_component_depth(edge.to_topic, adjacency, depths) + 1)
        .max()
        .unwrap_or(0);
    depths.insert(topic, depth);
    depth
}

pub fn remap_scene_branch_plan_topics(
    plan: &SceneBranchPlan,
    target_topic_by_source: &HashMap<FormKey, FormKey>,
) -> Result<SceneBranchPlan, DialogueGraphError> {
    let map = |source: FormKey| {
        target_topic_by_source
            .get(&source)
            .copied()
            .ok_or(DialogueGraphError::UnmappedTopic {
                topic: source.local,
            })
    };
    Ok(SceneBranchPlan {
        quest: plan.quest,
        scene: plan.scene,
        branch: plan.branch,
        starting_topic: map(plan.starting_topic)?,
        ordered_topics: plan
            .ordered_topics
            .iter()
            .copied()
            .map(map)
            .collect::<Result<Vec<_>, _>>()?,
        edges: plan
            .edges
            .iter()
            .map(|edge| {
                Ok(DialogueEdge {
                    from_topic: map(edge.from_topic)?,
                    to_topic: map(edge.to_topic)?,
                    kind: edge.kind,
                })
            })
            .collect::<Result<Vec<_>, DialogueGraphError>>()?,
    })
}

fn is_top_level_topic(topic: &Record, interner: &StringInterner) -> bool {
    topic
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DATA")
        .is_some_and(|field| match &field.value {
            FieldValue::Bytes(bytes) => bytes.get(1).is_some_and(|flags| flags & 0x02 != 0),
            value => contains_named_flag(value, "TopLevel", interner),
        })
}

fn contains_named_flag(value: &FieldValue, expected: &str, interner: &StringInterner) -> bool {
    match value {
        FieldValue::String(name) => interner
            .resolve(*name)
            .is_some_and(|name| name.eq_ignore_ascii_case(expected)),
        FieldValue::List(values) => values
            .iter()
            .any(|value| contains_named_flag(value, expected, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| contains_named_flag(value, expected, interner)),
        _ => false,
    }
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::List(values) => values.iter().find_map(first_form_key),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;

    fn fk(local: u32, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("Converted.esm"),
        }
    }

    #[test]
    fn vtechatticup_topics_form_a_bounded_deterministic_scene_branch() {
        let interner = StringInterner::new();
        let first = fk(0x13015B, &interner);
        let second = fk(0x134B9A, &interner);
        let empty = fk(0x138A74, &interner);
        let mut first_record = Record::new(SigCode(*b"DIAL"), first);
        first_record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0, 0])),
        });
        let mut second_record = Record::new(SigCode(*b"DIAL"), second);
        second_record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0, 2])),
        });
        let mut empty_record = Record::new(SigCode(*b"DIAL"), empty);
        empty_record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0, 0])),
        });
        let mut first_info = Record::new(SigCode(*b"INFO"), fk(0x130161, &interner));
        first_info.fields.push(FieldEntry {
            sig: SubrecordSig(*b"TCLT"),
            value: FieldValue::FormKey(empty),
        });
        let graph = build_legacy_dialogue_graph(
            &[first_record, second_record, empty_record],
            &[(second, first_info)],
            &interner,
        );
        let plan = bounded_scene_branch_from_graph(
            fk(0x11F935, &interner),
            fk(0x13A100, &interner),
            fk(0x13A101, &interner),
            &graph,
            16,
            8,
        )
        .unwrap();
        assert_eq!(plan.starting_topic, second);
        assert_eq!(plan.ordered_topics, [second, empty, first]);
    }

    #[test]
    fn graph_cycle_fails_closed() {
        let interner = StringInterner::new();
        let first = fk(1, &interner);
        let second = fk(2, &interner);
        let error = build_bounded_scene_branch_plan(
            fk(10, &interner),
            fk(11, &interner),
            fk(12, &interner),
            &[first],
            &[first, second],
            &[
                DialogueEdge {
                    from_topic: first,
                    to_topic: second,
                    kind: DialogueEdgeKind::LinkTo,
                },
                DialogueEdge {
                    from_topic: second,
                    to_topic: first,
                    kind: DialogueEdgeKind::LinkFrom,
                },
            ],
            16,
            8,
        )
        .expect_err("cycles must not synthesize unbounded scenes");
        assert!(matches!(error, DialogueGraphError::Cycle { .. }));
    }

    #[test]
    fn longest_converging_path_respects_depth_limit() {
        let interner = StringInterner::new();
        let root = fk(1, &interner);
        let shared = fk(2, &interner);
        let intermediate = fk(3, &interner);
        let leaf = fk(4, &interner);
        let error = build_bounded_scene_branch_plan(
            fk(10, &interner),
            fk(11, &interner),
            fk(12, &interner),
            &[root],
            &[root, shared, intermediate, leaf],
            &[
                DialogueEdge {
                    from_topic: root,
                    to_topic: shared,
                    kind: DialogueEdgeKind::LinkTo,
                },
                DialogueEdge {
                    from_topic: root,
                    to_topic: intermediate,
                    kind: DialogueEdgeKind::LinkTo,
                },
                DialogueEdge {
                    from_topic: intermediate,
                    to_topic: shared,
                    kind: DialogueEdgeKind::LinkTo,
                },
                DialogueEdge {
                    from_topic: shared,
                    to_topic: leaf,
                    kind: DialogueEdgeKind::LinkTo,
                },
            ],
            16,
            2,
        )
        .expect_err("the longest path, not the first visited path, sets the bound");

        assert_eq!(error, DialogueGraphError::DepthLimit { max_depth: 2 });
    }

    #[test]
    fn target_topic_remap_is_total_and_deterministic() {
        let interner = StringInterner::new();
        let source_first = fk(0x13015B, &interner);
        let source_second = fk(0x134B9A, &interner);
        let target_plugin = interner.intern("Output.esm");
        let target_first = FormKey {
            local: 0x23015B,
            plugin: target_plugin,
        };
        let target_second = FormKey {
            local: 0x234B9A,
            plugin: target_plugin,
        };
        let source_plan = SceneBranchPlan {
            quest: FormKey {
                local: 0x21F935,
                plugin: target_plugin,
            },
            scene: FormKey {
                local: 0x2A1000,
                plugin: target_plugin,
            },
            branch: FormKey {
                local: 0x2A1001,
                plugin: target_plugin,
            },
            starting_topic: source_second,
            ordered_topics: vec![source_second, source_first],
            edges: vec![DialogueEdge {
                from_topic: source_second,
                to_topic: source_first,
                kind: DialogueEdgeKind::LinkTo,
            }],
        };

        let remapped = remap_scene_branch_plan_topics(
            &source_plan,
            &HashMap::from([(source_first, target_first), (source_second, target_second)]),
        )
        .unwrap();

        assert_eq!(remapped.starting_topic, target_second);
        assert_eq!(remapped.ordered_topics, [target_second, target_first]);
        assert_eq!(remapped.edges[0].to_topic, target_first);
    }

    #[test]
    fn target_topic_remap_fails_closed_when_any_topic_is_unmapped() {
        let interner = StringInterner::new();
        let source = fk(0x13015B, &interner);
        let plan = SceneBranchPlan {
            quest: fk(1, &interner),
            scene: fk(2, &interner),
            branch: fk(3, &interner),
            starting_topic: source,
            ordered_topics: vec![source],
            edges: Vec::new(),
        };

        assert!(matches!(
            remap_scene_branch_plan_topics(&plan, &HashMap::new()),
            Err(DialogueGraphError::UnmappedTopic { topic: 0x13015B })
        ));
    }
}
