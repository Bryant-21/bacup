use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::quest_runtime::*;

pub(crate) const REQUIRED_FNV_QUEST_SLICE: &[(&str, &[u32])] = &[
    ("ACHR", &[0x12319B, 0x12319C, 0x134B9C]),
    ("ACTI", &[0x133F41]),
    ("DIAL", &[0x13015B, 0x134B9A, 0x138A74]),
    ("INFO", &[0x130161, 0x134B9B, 0x15734B, 0x15734C, 0x15734D]),
    ("MGEF", &[0x0CB05D]),
    ("PACK", &[0x1231B6, 0x1231B7, 0x13289E, 0x133F3E]),
    ("PERK", &[0x058FDF]),
    ("QUST", &[0x06136D, 0x11F935]),
    ("REFR", &[0x133F42]),
    ("SCPT", &[0x11FC64, 0x123191, 0x134491, 0x166305]),
    ("SPEL", &[0x172091]),
];

pub(crate) const REQUIRED_FNV_SCPT_BINDINGS: &[(u32, &str, u32)] = &[
    (0x11FC64, "QUST", 0x11F935),
    (0x123191, "NPC_", 0x123193),
    (0x134491, "ACTI", 0x133F41),
    (0x166305, "NPC_", 0x1300F0),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FnvQuestTopologyEvidence {
    pub source: BTreeMap<QuestRecordKey, Vec<String>>,
    pub target: BTreeMap<QuestRecordKey, Vec<String>>,
}

const FREEFORM_POWER_ARMOR_QUEST: u32 = 0x06136D;
const VTECHATTICUP_QUEST: u32 = 0x11F935;
const VTECH_SCENE_INTENT: &str = "intent:fnv:11f935:scene";
const VTECH_DIALOGUE_BRANCH_INTENT: &str = "intent:fnv:11f935:dialogue_branch";
const DEDICATED_GREETING_KEYWORD_INTENT: &str = "intent:fnv:11f935:dedicated_greeting_keyword";
const DEDICATED_GREETING_TOPIC_INTENT: &str = "intent:fnv:11f935:dedicated_greeting_topic";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FnvQuestSyntheticIntent {
    Scene,
    DialogueBranch,
    DedicatedGreetingKeyword,
    DedicatedGreetingTopic,
}

impl FnvQuestSyntheticIntent {
    pub(crate) fn record_key(self) -> QuestRecordKey {
        let (signature, intent) = match self {
            Self::Scene => ("SCEN", VTECH_SCENE_INTENT),
            Self::DialogueBranch => ("DLBR", VTECH_DIALOGUE_BRANCH_INTENT),
            Self::DedicatedGreetingKeyword => ("KYWD", DEDICATED_GREETING_KEYWORD_INTENT),
            Self::DedicatedGreetingTopic => ("DIAL", DEDICATED_GREETING_TOPIC_INTENT),
        };
        QuestRecordKey::new(signature, intent)
    }

    pub(crate) fn target_key(self, form_key: impl Into<String>) -> QuestRecordKey {
        QuestRecordKey::new(self.record_key().signature, form_key)
    }
}

pub(crate) fn admit_exact_fnv_quest_slice(
    selected: &HashMap<String, Vec<u32>>,
    source_plugin: &str,
    target_mappings: &BTreeMap<QuestRecordKey, QuestRecordKey>,
    topology: &FnvQuestTopologyEvidence,
) -> Result<Vec<QuestRuntimeComponentPlan>, String> {
    let mut plans =
        build_exact_fnv_quest_slice_plans(selected, source_plugin, target_mappings, topology);
    apply_fnv_start_routes(&mut plans)?;
    let rejected = plans
        .iter()
        .filter_map(|plan| match &plan.admission {
            QuestRuntimeAdmission::Supported => None,
            QuestRuntimeAdmission::Rejected { reasons } => Some(format!(
                "{}:{}",
                plan.component_id,
                reasons
                    .iter()
                    .map(rejection_reason_name)
                    .collect::<Vec<_>>()
                    .join(",")
            )),
        })
        .collect::<Vec<_>>();
    if !rejected.is_empty() {
        return Err(format!(
            "fnv_exact_slice_component_rejected:{}",
            rejected.join(";")
        ));
    }
    let issues = validate_component_plans(&plans);
    if !issues.is_empty() {
        return Err(format_plan_issues(&issues));
    }
    Ok(plans)
}

pub(crate) fn apply_fnv_start_routes(
    plans: &mut [QuestRuntimeComponentPlan],
) -> Result<(), String> {
    for plan in plans.iter_mut() {
        if matches!(plan.admission, QuestRuntimeAdmission::Supported) {
            super::start_route::apply_fnv_start_route_policy(
                plan,
                &super::start_route::FnvStartRouteEvidence::default(),
            )
            .map_err(|error| {
                format!(
                    "{} start-route policy rejected exact slice: {error:?}",
                    plan.component_id
                )
            })?;
        }
    }
    let issues = validate_component_plans(&plans);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(format_plan_issues(&issues))
    }
}

pub(crate) fn apply_runtime_origin_provenance(
    plans: &mut [QuestRuntimeComponentPlan],
    origins: &[crate::merge_sources::LegacyRuntimeOriginRow],
) -> Result<(), String> {
    if origins.is_empty() {
        return Ok(());
    }
    let mut by_merged_record = BTreeMap::new();
    for origin in origins {
        let key = (
            origin.signature.trim().to_ascii_uppercase(),
            canonical_form_key_text(&origin.merged_form_key)?,
        );
        if by_merged_record.insert(key.clone(), origin).is_some() {
            return Err(format!(
                "duplicate quest-runtime origin for {} {}",
                key.0, origin.merged_form_key
            ));
        }
    }

    for plan in plans.iter_mut() {
        let root_origin =
            origin_for_record(&by_merged_record, &plan.root_quest)?.ok_or_else(|| {
                format!(
                    "{} has no runtime origin for root {}",
                    plan.component_id, plan.root_quest.form_key
                )
            })?;
        let game = runtime_origin_game(&root_origin.source_game)?;
        for record in plan
            .owned_records
            .iter()
            .chain(&plan.shared_records)
            .filter(|record| !record.form_key.starts_with("intent:"))
        {
            let origin = origin_for_record(&by_merged_record, record)?.ok_or_else(|| {
                format!(
                    "{} has no runtime origin for {} {}",
                    plan.component_id, record.signature, record.form_key
                )
            })?;
            if runtime_origin_game(&origin.source_game)? != game {
                return Err(format!(
                    "{} crosses FNV/FO3 provenance at {} {}",
                    plan.component_id, record.signature, record.form_key
                ));
            }
        }
        plan.provenance = SourceProvenance {
            game,
            source_plugin: root_origin.source_plugin.clone(),
            graft: provenance_requires_graft(root_origin).then(|| GraftProvenance {
                source_plugin: root_origin.contributing_plugin.clone(),
                source_form_key: root_origin.source_form_key.clone(),
                graft_form_key: root_origin.merged_form_key.clone(),
            }),
        };
    }
    let issues = validate_component_plans(plans);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(format_plan_issues(&issues))
    }
}

fn origin_for_record<'a>(
    origins: &'a BTreeMap<(String, String), &crate::merge_sources::LegacyRuntimeOriginRow>,
    record: &QuestRecordKey,
) -> Result<Option<&'a crate::merge_sources::LegacyRuntimeOriginRow>, String> {
    Ok(origins
        .get(&(
            record.signature.to_ascii_uppercase(),
            canonical_form_key_text(&record.form_key)?,
        ))
        .copied())
}

fn runtime_origin_game(value: &str) -> Result<QuestSourceGame, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fnv" => Ok(QuestSourceGame::Fnv),
        "fo3" => Ok(QuestSourceGame::Fo3),
        other => Err(format!("unsupported quest-runtime origin game {other:?}")),
    }
}

fn provenance_requires_graft(origin: &crate::merge_sources::LegacyRuntimeOriginRow) -> bool {
    canonical_form_key_text(&origin.source_form_key).ok()
        != canonical_form_key_text(&origin.merged_form_key).ok()
        || !origin
            .contributing_plugin
            .eq_ignore_ascii_case(&origin.source_plugin)
}

fn canonical_form_key_text(value: &str) -> Result<String, String> {
    let value = value.trim();
    let delimiter = value
        .rfind('@')
        .or_else(|| value.rfind(':'))
        .ok_or_else(|| format!("invalid quest-runtime FormKey {value:?}"))?;
    let local = u32::from_str_radix(
        value[..delimiter]
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X"),
        16,
    )
    .map_err(|_| format!("invalid quest-runtime FormKey {value:?}"))?;
    let plugin = value[delimiter + 1..].trim();
    if plugin.is_empty() {
        return Err(format!("invalid quest-runtime FormKey {value:?}"));
    }
    Ok(format!(
        "{:06x}@{}",
        local & 0x00FF_FFFF,
        plugin.to_ascii_lowercase()
    ))
}

pub(crate) fn build_exact_fnv_quest_slice_plans(
    selected: &HashMap<String, Vec<u32>>,
    source_plugin: &str,
    target_mappings: &BTreeMap<QuestRecordKey, QuestRecordKey>,
    topology: &FnvQuestTopologyEvidence,
) -> Vec<QuestRuntimeComponentPlan> {
    let selected_keys = normalized_selected_keys(selected, source_plugin);
    let expected_keys = required_slice_keys(source_plugin);
    let selection_is_exact = selected_keys == expected_keys;

    let freeform_root = key("QUST", FREEFORM_POWER_ARMOR_QUEST, source_plugin);
    let vtech_root = key("QUST", VTECHATTICUP_QUEST, source_plugin);
    let mut freeform_owned = Vec::new();
    let mut vtech_owned = Vec::new();
    for record in &selected_keys {
        if belongs_to_freeform(record) {
            freeform_owned.push(record.clone());
        } else {
            vtech_owned.push(record.clone());
        }
    }
    let vtech_synthetic = vtech_synthetic_records();
    vtech_owned.extend(vtech_synthetic.iter().cloned());

    vec![
        build_plan(
            "fnv:falloutnv.esm:06136d",
            freeform_root,
            freeform_owned,
            BTreeSet::new(),
            BTreeSet::new(),
            source_plugin,
            target_mappings,
            topology,
            selection_is_exact,
            &[],
            &["QF_FNV_FO3_06136D", "FNV_FO3_FnvSliceCompat"],
            0x1D,
        ),
        build_plan(
            "fnv:falloutnv.esm:11f935",
            vtech_root,
            vtech_owned,
            vtech_synthetic,
            standalone_external_owner_keys(source_plugin),
            source_plugin,
            target_mappings,
            topology,
            selection_is_exact,
            &[0x11FC64, 0x123191, 0x134491, 0x166305],
            &[
                "QF_FNV_FO3_11F935",
                "TIF__130161",
                "TIF__134B9B",
                "FNV_FO3_FnvSliceCompat",
            ],
            0x11,
        ),
    ]
}

fn build_plan(
    component_id: &str,
    root_quest: QuestRecordKey,
    mut owned_records: Vec<QuestRecordKey>,
    synthetic_records: BTreeSet<QuestRecordKey>,
    external_dependencies: BTreeSet<QuestRecordKey>,
    source_plugin: &str,
    target_mappings: &BTreeMap<QuestRecordKey, QuestRecordKey>,
    topology: &FnvQuestTopologyEvidence,
    selection_is_exact: bool,
    script_locals: &[u32],
    component_classes: &[&str],
    source_flags: u8,
) -> QuestRuntimeComponentPlan {
    owned_records.sort();
    let mut dependencies = owned_records
        .iter()
        .filter(|record| !synthetic_records.contains(*record))
        .filter(|record| **record != root_quest)
        .cloned()
        .collect::<BTreeSet<_>>();
    dependencies.extend(external_dependencies.iter().cloned());
    let mut mappings = owned_records
        .iter()
        .filter(|source| !synthetic_records.contains(*source))
        .chain(external_dependencies.iter())
        .filter_map(|source| {
            target_mappings
                .get(source)
                .cloned()
                .map(|target| SourceTargetFormMapping {
                    source: source.clone(),
                    target,
                })
        })
        .collect::<Vec<_>>();
    mappings.extend(
        synthetic_records
            .iter()
            .cloned()
            .map(|intent| SourceTargetFormMapping {
                source: intent.clone(),
                target: intent,
            }),
    );
    let source_topology = owned_records
        .iter()
        .filter(|record| !synthetic_records.contains(*record))
        .filter_map(|record| source_topology_placement(record, topology))
        .collect::<BTreeSet<_>>();
    let mut target_topology = owned_records
        .iter()
        .filter(|record| !synthetic_records.contains(*record))
        .filter(|record| record.signature != "SCPT")
        .filter_map(|source| {
            target_mappings.get(source).and_then(|target| {
                target_topology_placement(source, target, target_mappings, topology)
            })
        })
        .collect::<BTreeSet<_>>();
    let target_root = target_mappings
        .get(&root_quest)
        .cloned()
        .unwrap_or_else(|| root_quest.clone());
    target_topology.extend(
        synthetic_records
            .iter()
            .map(|record| synthetic_topology_placement(record, &target_root)),
    );
    let scripts = script_locals
        .iter()
        .map(|local| ScriptIntent {
            owner: key("SCPT", *local, source_plugin),
            source_name: format!("{local:06X}:{source_plugin}"),
            target_class: format!("FNV_FO3_S_{local:06X}"),
            required: true,
        })
        .collect::<BTreeSet<_>>();
    let psc = component_classes
        .iter()
        .map(|class_name| PscIntent {
            class_name: (*class_name).to_owned(),
            source_artifact: format!("Scripts/Source/User/{class_name}.psc"),
            compiler_evidence_required: true,
            compiler_evidence: Some(compiler_evidence_intent(class_name)),
        })
        .chain(script_locals.iter().map(|local| {
            let class_name = format!("FNV_FO3_S_{local:06X}");
            PscIntent {
                source_artifact: format!("Scripts/Source/User/{class_name}.psc"),
                compiler_evidence: Some(compiler_evidence_intent(&class_name)),
                class_name,
                compiler_evidence_required: true,
            }
        }))
        .collect::<BTreeSet<_>>();
    let fragments = fragment_intents(&root_quest, source_plugin);
    let vmad = vmad_intents(&root_quest, source_plugin, target_mappings);
    let mut reasons = BTreeSet::new();
    if source_plugin.trim().is_empty() {
        reasons.insert(QuestRuntimeRejectionReason::MissingProvenance);
    }
    if !selection_is_exact
        || !owned_records
            .iter()
            .filter(|record| !synthetic_records.contains(*record))
            .chain(external_dependencies.iter())
            .all(|record| target_mappings.contains_key(record))
        || !owned_records.contains(&root_quest)
    {
        reasons.insert(QuestRuntimeRejectionReason::IncompleteClosure);
    }
    if owned_records.iter().any(|record| {
        is_placed_record(record)
            && (!topology.source.contains_key(record)
                || target_mappings
                    .get(record)
                    .is_none_or(|target| !topology.target.contains_key(target)))
    }) {
        reasons.insert(QuestRuntimeRejectionReason::UnsupportedTopology);
    }
    let admission = if reasons.is_empty() {
        QuestRuntimeAdmission::Supported
    } else {
        QuestRuntimeAdmission::Rejected { reasons }
    };
    let producer_id = format!("qust_autostart:{component_id}");
    let evidence_id = format!("fnv_qust_data_flags:{source_flags:02X}");
    let inbound_producers = BTreeSet::from([StartProducerIntent {
        producer_id: producer_id.clone(),
        carrier: root_quest.clone(),
        producer_kind: "qust_start_game_enabled".to_owned(),
        evidence_id: evidence_id.clone(),
        proven: source_flags & 0x01 != 0,
    }]);
    let expected_receipt = expected_receipt(
        &owned_records,
        &target_topology,
        &psc,
        &vmad,
        &root_quest,
        &producer_id,
        &evidence_id,
        target_mappings,
        &synthetic_records,
    );

    QuestRuntimeComponentPlan {
        component_id: component_id.to_owned(),
        root_quest,
        provenance: SourceProvenance {
            game: QuestSourceGame::Fnv,
            source_plugin: source_plugin.to_owned(),
            graft: None,
        },
        owned_records,
        shared_records: external_dependencies,
        dependencies: DependencyClosure {
            direct: dependencies.clone(),
            recursive: dependencies,
        },
        source_topology,
        target_topology,
        mappings,
        semantics: QuestSemanticPlan {
            start_flags: BTreeSet::from([QuestStartFlag::StartGameEnabled]),
            ..QuestSemanticPlan::default()
        },
        scripts,
        fragments,
        psc,
        vmad,
        assets: BTreeSet::new(),
        inbound_producers,
        start_disposition: Some(StartDisposition::Autostart),
        admission,
        expected_receipt,
    }
}

fn expected_receipt(
    owned_records: &[QuestRecordKey],
    target_topology: &BTreeSet<TopologyPlacement>,
    psc: &BTreeSet<PscIntent>,
    vmad: &BTreeSet<VmadIntent>,
    root_quest: &QuestRecordKey,
    producer_id: &str,
    evidence_id: &str,
    target_mappings: &BTreeMap<QuestRecordKey, QuestRecordKey>,
    synthetic_records: &BTreeSet<QuestRecordKey>,
) -> QuestRuntimeExpectedReceipt {
    let mut emitted_records: BTreeSet<QuestRecordKey> = owned_records
        .iter()
        .filter(|record| !synthetic_records.contains(*record))
        .filter(|record| record.signature != "SCPT")
        .filter_map(|source| target_mappings.get(source).cloned())
        .collect();
    emitted_records.extend(synthetic_records.iter().cloned());
    let scripts = psc
        .iter()
        .map(|intent| ScriptArtifactReceipt {
            class_name: intent.class_name.clone(),
            psc_path: intent.source_artifact.clone(),
            pex_path: format!("Scripts/{}.pex", intent.class_name),
            compiler_evidence_id: intent
                .compiler_evidence
                .as_ref()
                .map(|evidence| evidence.manifest_id.clone())
                .unwrap_or_default(),
        })
        .collect();
    let vmad_attachments = vmad
        .iter()
        .filter_map(|intent| {
            let owner = target_mappings.get(&intent.owner)?.clone();
            Some(VmadAttachmentReceipt {
                owner,
                script_class: intent.script_class.clone(),
                property_names: intent
                    .properties
                    .iter()
                    .map(|property| property.property_name.clone())
                    .collect(),
                compiler_evidence_id: intent
                    .compiler_evidence
                    .as_ref()
                    .map(|evidence| evidence.manifest_id.clone())
                    .unwrap_or_default(),
            })
        })
        .collect();
    let target_root = target_mappings
        .get(root_quest)
        .cloned()
        .unwrap_or_else(|| root_quest.clone());

    QuestRuntimeExpectedReceipt {
        emitted_records,
        placements: target_topology.clone(),
        localized_strings: BTreeSet::new(),
        scripts,
        vmad_attachments,
        routes: BTreeSet::from([StartRouteReceipt {
            route_id: producer_id.to_owned(),
            quest: target_root,
            producer_evidence_id: evidence_id.to_owned(),
            node_chain: Vec::new(),
        }]),
    }
}

fn fragment_intents(root: &QuestRecordKey, source_plugin: &str) -> BTreeSet<FragmentIntent> {
    let local = local_id(root);
    let mut fragments = BTreeSet::from([FragmentIntent {
        owner: root.clone(),
        fragment_id: format!("quest_stage_fragments:{local:06X}"),
        entrypoint: "stage_fragments".to_owned(),
        target_class: format!("QF_FNV_FO3_{local:06X}"),
    }]);
    if local == VTECHATTICUP_QUEST {
        fragments.extend([0x130161, 0x134B9B].map(|info| FragmentIntent {
            owner: key("INFO", info, source_plugin),
            fragment_id: format!("info_result_fragment:{info:06X}"),
            entrypoint: "Fragment_Begin".to_owned(),
            target_class: format!("TIF__{info:06X}"),
        }));
    }
    fragments
}

fn vmad_intents(
    root: &QuestRecordKey,
    source_plugin: &str,
    target_mappings: &BTreeMap<QuestRecordKey, QuestRecordKey>,
) -> BTreeSet<VmadIntent> {
    let local = local_id(root);
    let compat_property = VmadPropertyIntent {
        property_name: "FNVSliceCompat".to_owned(),
        property_type: "FNV_FO3_FnvSliceCompat".to_owned(),
        target_value: target_mappings
            .get(root)
            .map(|target| target.form_key.clone())
            .unwrap_or_default(),
        required: true,
    };
    let mut intents = BTreeSet::from([
        VmadIntent {
            owner: root.clone(),
            script_class: format!("QF_FNV_FO3_{local:06X}"),
            properties: BTreeSet::from([compat_property]),
            compiler_evidence_required: true,
            compiler_evidence: Some(compiler_evidence_intent(&format!("QF_FNV_FO3_{local:06X}"))),
        },
        VmadIntent {
            owner: root.clone(),
            script_class: "FNV_FO3_FnvSliceCompat".to_owned(),
            properties: BTreeSet::new(),
            compiler_evidence_required: true,
            compiler_evidence: Some(compiler_evidence_intent("FNV_FO3_FnvSliceCompat")),
        },
    ]);
    if local == VTECHATTICUP_QUEST {
        intents.extend(REQUIRED_FNV_SCPT_BINDINGS.iter().map(
            |(script_local, owner_signature, owner_local)| {
                let script_class = format!("FNV_FO3_S_{script_local:06X}");
                VmadIntent {
                    owner: key(owner_signature, *owner_local, source_plugin),
                    properties: standalone_script_properties(*script_local),
                    compiler_evidence_required: true,
                    compiler_evidence: Some(compiler_evidence_intent(&script_class)),
                    script_class,
                }
            },
        ));
        for info in [0x130161, 0x134B9B] {
            intents.insert(VmadIntent {
                owner: key("INFO", info, source_plugin),
                script_class: format!("TIF__{info:06X}"),
                properties: BTreeSet::from([VmadPropertyIntent {
                    property_name: "VTechatticup".to_owned(),
                    property_type: "FNV_FO3_S_11FC64".to_owned(),
                    target_value: target_mappings
                        .get(root)
                        .map(|target| target.form_key.clone())
                        .unwrap_or_default(),
                    required: true,
                }]),
                compiler_evidence_required: true,
                compiler_evidence: Some(compiler_evidence_intent(&format!("TIF__{info:06X}"))),
            });
        }
    }
    intents
}

fn standalone_script_properties(script_local: u32) -> BTreeSet<VmadPropertyIntent> {
    match script_local {
        0x123191 => BTreeSet::from([
            VmadPropertyIntent {
                property_name: "TecMineHostageEscapeData".to_owned(),
                property_type: "ReferenceAlias".to_owned(),
                target_value: "alias:11F935:TecMineHostageEscapeData".to_owned(),
                required: true,
            },
            VmadPropertyIntent {
                property_name: "TecMineHostageFreedGreeting".to_owned(),
                property_type: "Keyword".to_owned(),
                target_value: DEDICATED_GREETING_KEYWORD_INTENT.to_owned(),
                required: true,
            },
        ]),
        0x134491 => BTreeSet::from([VmadPropertyIntent {
            property_name: "TechaticupNCRRenoldsDialoguePackageData".to_owned(),
            property_type: "ReferenceAlias".to_owned(),
            target_value: "alias:11F935:TechaticupNCRRenoldsDialoguePackageData".to_owned(),
            required: true,
        }]),
        _ => BTreeSet::new(),
    }
}

fn compiler_evidence_intent(class_name: &str) -> CompilerEvidenceIntent {
    CompilerEvidenceIntent {
        manifest_id: format!("fnv_exact_slice:{class_name}"),
        source_digest: format!("psc_source:{class_name}"),
        require_fresh_output: true,
    }
}

fn vtech_synthetic_records() -> BTreeSet<QuestRecordKey> {
    BTreeSet::from([
        FnvQuestSyntheticIntent::Scene.record_key(),
        FnvQuestSyntheticIntent::DialogueBranch.record_key(),
        FnvQuestSyntheticIntent::DedicatedGreetingKeyword.record_key(),
        FnvQuestSyntheticIntent::DedicatedGreetingTopic.record_key(),
    ])
}

fn is_synthetic_intent(record: &QuestRecordKey) -> bool {
    record.form_key.starts_with("intent:fnv:")
}

fn synthetic_topology_placement(
    record: &QuestRecordKey,
    target_root: &QuestRecordKey,
) -> TopologyPlacement {
    let group_path = match record.signature.as_str() {
        "SCEN" | "DLBR" | "DIAL" => vec![
            "GRUP:QUST".to_owned(),
            format!("GRUP:type=10:owner={}", target_root.form_key),
        ],
        signature => vec![format!("GRUP:{signature}")],
    };
    TopologyPlacement {
        record: record.clone(),
        group_path,
    }
}

fn source_topology_placement(
    record: &QuestRecordKey,
    topology: &FnvQuestTopologyEvidence,
) -> Option<TopologyPlacement> {
    let group_path = if is_placed_record(record) {
        topology.source.get(record)?.clone()
    } else {
        topology_path(record, false, &BTreeMap::new())
    };
    Some(TopologyPlacement {
        record: record.clone(),
        group_path,
    })
}

fn target_topology_placement(
    source: &QuestRecordKey,
    target: &QuestRecordKey,
    mappings: &BTreeMap<QuestRecordKey, QuestRecordKey>,
    topology: &FnvQuestTopologyEvidence,
) -> Option<TopologyPlacement> {
    let group_path = if is_placed_record(source) {
        topology.target.get(target)?.clone()
    } else {
        topology_path(source, true, mappings)
    };
    Some(TopologyPlacement {
        record: target.clone(),
        group_path,
    })
}

fn topology_path(
    record: &QuestRecordKey,
    target: bool,
    mappings: &BTreeMap<QuestRecordKey, QuestRecordKey>,
) -> Vec<String> {
    let plugin = record
        .form_key
        .split_once(':')
        .map(|(_, plugin)| plugin)
        .unwrap_or("FalloutNV.esm");
    let mapped = |signature: &str, local: u32| {
        let source = key(signature, local, plugin);
        mappings
            .get(&source)
            .map(|record| record.form_key.clone())
            .unwrap_or(source.form_key)
    };
    match (record.signature.as_str(), local_id(record)) {
        ("DIAL", _) => vec![
            "GRUP:QUST".to_owned(),
            format!("GRUP:type=10:owner={}", mapped("QUST", VTECHATTICUP_QUEST)),
        ],
        ("INFO", 0x130161) => vec![
            "GRUP:DIAL".to_owned(),
            format!("GRUP:type=7:owner={}", mapped("DIAL", 0x13015B)),
        ],
        ("INFO", 0x134B9B) => vec![
            "GRUP:DIAL".to_owned(),
            format!("GRUP:type=7:owner={}", mapped("DIAL", 0x134B9A)),
        ],
        ("INFO", 0x15734B | 0x15734C | 0x15734D) if target => vec![
            "GRUP:DIAL".to_owned(),
            format!("GRUP:type=7:owner={DEDICATED_GREETING_TOPIC_INTENT}"),
        ],
        ("INFO", 0x15734B | 0x15734C | 0x15734D) => vec![
            "GRUP:DIAL".to_owned(),
            format!(
                "GRUP:type=7:owner={}",
                key("DIAL", 0x0000C8, plugin).form_key
            ),
        ],
        (signature, _) => vec![format!("GRUP:{signature}")],
    }
}

fn normalized_selected_keys(
    selected: &HashMap<String, Vec<u32>>,
    source_plugin: &str,
) -> BTreeSet<QuestRecordKey> {
    selected
        .iter()
        .flat_map(|(signature, locals)| {
            locals
                .iter()
                .map(move |local| key(signature, *local, source_plugin))
        })
        .collect()
}

fn required_slice_keys(source_plugin: &str) -> BTreeSet<QuestRecordKey> {
    REQUIRED_FNV_QUEST_SLICE
        .iter()
        .flat_map(|(signature, locals)| {
            locals
                .iter()
                .map(move |local| key(signature, *local, source_plugin))
        })
        .collect()
}

fn belongs_to_freeform(record: &QuestRecordKey) -> bool {
    matches!(
        (record.signature.as_str(), local_id(record)),
        ("QUST", FREEFORM_POWER_ARMOR_QUEST)
            | ("MGEF", 0x0CB05D)
            | ("PERK", 0x058FDF)
            | ("SPEL", 0x172091)
    )
}

fn standalone_external_owner_keys(source_plugin: &str) -> BTreeSet<QuestRecordKey> {
    REQUIRED_FNV_SCPT_BINDINGS
        .iter()
        .filter(|(_, signature, local)| {
            !REQUIRED_FNV_QUEST_SLICE
                .iter()
                .any(|(selected, locals)| selected == signature && locals.contains(local))
        })
        .map(|(_, signature, local)| key(signature, *local, source_plugin))
        .collect()
}

fn is_placed_record(record: &QuestRecordKey) -> bool {
    matches!(record.signature.as_str(), "ACHR" | "REFR")
}

fn key(signature: &str, local: u32, plugin: &str) -> QuestRecordKey {
    QuestRecordKey::new(signature, format!("{:06X}:{}", local & 0x00FF_FFFF, plugin))
}

fn local_id(record: &QuestRecordKey) -> u32 {
    record
        .form_key
        .split_once(':')
        .and_then(|(local, _)| u32::from_str_radix(local, 16).ok())
        .unwrap_or(0)
}

fn format_plan_issues(issues: &[PlanValidationIssue]) -> String {
    let details = issues
        .iter()
        .map(|issue| {
            format!(
                "{}:{:?}:{}",
                issue.component_id,
                issue.code,
                issue
                    .record
                    .as_ref()
                    .map(|record| format!("{}:{}", record.signature, record.form_key))
                    .unwrap_or_else(|| "none".to_owned())
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    format!("fnv_exact_slice_component_invalid:{details}")
}

fn rejection_reason_name(reason: &QuestRuntimeRejectionReason) -> &'static str {
    match reason {
        QuestRuntimeRejectionReason::MissingProvenance => "missing_provenance",
        QuestRuntimeRejectionReason::IncompleteClosure => "incomplete_closure",
        QuestRuntimeRejectionReason::DuplicateOwnership => "duplicate_ownership",
        QuestRuntimeRejectionReason::MissingStartDisposition => "missing_start_disposition",
        QuestRuntimeRejectionReason::OrphanStartUnknown => "orphan_start_unknown",
        QuestRuntimeRejectionReason::UnsupportedCondition => "unsupported_condition",
        QuestRuntimeRejectionReason::UnsupportedAliasFill => "unsupported_alias_fill",
        QuestRuntimeRejectionReason::UnsupportedScript => "unsupported_script",
        QuestRuntimeRejectionReason::MissingCompilerEvidence => "missing_compiler_evidence",
        QuestRuntimeRejectionReason::MissingStartProducer => "missing_start_producer",
        QuestRuntimeRejectionReason::UnsupportedTopology => "unsupported_topology",
        QuestRuntimeRejectionReason::RequiredAssetUnavailable => "required_asset_unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact_selection() -> HashMap<String, Vec<u32>> {
        REQUIRED_FNV_QUEST_SLICE
            .iter()
            .map(|(signature, locals)| ((*signature).to_owned(), locals.to_vec()))
            .collect()
    }

    fn exact_mappings() -> BTreeMap<QuestRecordKey, QuestRecordKey> {
        required_slice_keys("FalloutNV.esm")
            .into_iter()
            .chain(standalone_external_owner_keys("FalloutNV.esm"))
            .map(|source| {
                let target = QuestRecordKey::new(
                    &source.signature,
                    format!(
                        "{:06X}:Converted.esm",
                        (local_id(&source) + 0x200000) & 0x00FF_FFFF
                    ),
                );
                (source, target)
            })
            .collect()
    }

    fn exact_topology(
        mappings: &BTreeMap<QuestRecordKey, QuestRecordKey>,
    ) -> FnvQuestTopologyEvidence {
        let mut evidence = FnvQuestTopologyEvidence::default();
        for (signature, local, child_type) in [
            ("ACHR", 0x12319B, 9),
            ("ACHR", 0x12319C, 9),
            ("ACHR", 0x134B9C, 9),
            ("REFR", 0x133F42, 8),
        ] {
            let source = key(signature, local, "FalloutNV.esm");
            let target = mappings[&source].clone();
            evidence.source.insert(
                source,
                vec![
                    "GRUP:CELL".to_owned(),
                    "GRUP:type=6:label=00000100".to_owned(),
                    format!("GRUP:type={child_type}:label=00000100"),
                ],
            );
            evidence.target.insert(
                target,
                vec![
                    "GRUP:CELL".to_owned(),
                    "GRUP:type=6:label=00000200".to_owned(),
                    format!("GRUP:type={child_type}:label=00000200"),
                ],
            );
        }
        evidence
    }

    #[test]
    fn exact_slice_has_unique_two_three_five_four_four_ownership() {
        let mappings = exact_mappings();
        let plans = admit_exact_fnv_quest_slice(
            &exact_selection(),
            "FalloutNV.esm",
            &mappings,
            &exact_topology(&mappings),
        )
        .unwrap();

        let counts = plans
            .iter()
            .flat_map(|plan| &plan.owned_records)
            .filter(|record| !is_synthetic_intent(record))
            .fold(BTreeMap::<&str, usize>::new(), |mut counts, record| {
                *counts.entry(&record.signature).or_default() += 1;
                counts
            });
        assert_eq!(counts.get("QUST"), Some(&2));
        assert_eq!(counts.get("DIAL"), Some(&3));
        assert_eq!(counts.get("INFO"), Some(&5));
        assert_eq!(counts.get("SCPT"), Some(&4));
        assert_eq!(counts.get("PACK"), Some(&4));
        assert!(plans.iter().all(|plan| {
            plan.scripts
                .iter()
                .all(|script| plan.owned_records.contains(&script.owner))
        }));
        assert!(plans.iter().all(|plan| {
            matches!(plan.start_disposition, Some(StartDisposition::Autostart))
                && plan.inbound_producers.is_empty()
                && plan.expected_receipt.routes.iter().all(|route| {
                    route.route_id.starts_with("fnvfo3:start:fnv:")
                        && route.producer_evidence_id.starts_with("autostart:")
                        && route.node_chain.is_empty()
                })
        }));
        assert!(validate_component_plans(&plans).is_empty());
    }

    #[test]
    fn exact_slice_admission_and_receipts_are_deterministic() {
        let selected = exact_selection();
        let mappings = exact_mappings();
        let topology = exact_topology(&mappings);
        let first =
            admit_exact_fnv_quest_slice(&selected, "FalloutNV.esm", &mappings, &topology).unwrap();
        let second =
            admit_exact_fnv_quest_slice(&selected, "FalloutNV.esm", &mappings, &topology).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
    }

    #[test]
    fn collision_remapped_fo3_provenance_routes_with_canonical_owner() {
        let mappings = exact_mappings();
        let mut plans = admit_exact_fnv_quest_slice(
            &exact_selection(),
            "FalloutNV.esm",
            &mappings,
            &exact_topology(&mappings),
        )
        .unwrap();
        let origins = mappings
            .keys()
            .map(|record| crate::merge_sources::LegacyRuntimeOriginRow {
                signature: record.signature.clone(),
                merged_form_key: record.form_key.clone(),
                source_game: "fo3".to_string(),
                source_plugin: "Fallout3.esm".to_string(),
                source_form_key: format!("{:08X}@Fallout3.esm", local_id(record)),
                contributing_plugin: "ThePitt.esm".to_string(),
                source_parent_form_key: None,
                merged_parent_form_key: None,
                child_group_type: None,
            })
            .collect::<Vec<_>>();

        apply_runtime_origin_provenance(&mut plans, &origins).unwrap();
        apply_fnv_start_routes(&mut plans).unwrap();

        assert!(plans.iter().all(|plan| {
            plan.provenance.game == QuestSourceGame::Fo3
                && plan.provenance.source_plugin == "Fallout3.esm"
                && plan.provenance.graft.as_ref().is_some_and(|graft| {
                    graft.source_plugin == "ThePitt.esm"
                        && graft
                            .graft_form_key
                            .to_ascii_lowercase()
                            .ends_with("falloutnv.esm")
                })
                && plan.expected_receipt.routes.iter().all(|route| {
                    route.route_id.starts_with("fnvfo3:start:fo3:fallout3.esm:")
                        && route.node_chain.is_empty()
                })
        }));
        assert!(validate_component_plans(&plans).is_empty());
    }

    #[test]
    fn missing_mapping_rejects_with_a_stable_reason() {
        let mut mappings = exact_mappings();
        mappings.remove(&key("INFO", 0x130161, "FalloutNV.esm"));
        let topology = exact_topology(&mappings);

        let error =
            admit_exact_fnv_quest_slice(&exact_selection(), "FalloutNV.esm", &mappings, &topology)
                .unwrap_err();

        assert_eq!(
            error,
            "fnv_exact_slice_component_rejected:fnv:falloutnv.esm:11f935:incomplete_closure"
        );
    }

    #[test]
    fn selected_identity_and_expected_receipt_match_legacy_contract() {
        let mappings = exact_mappings();
        let plans = admit_exact_fnv_quest_slice(
            &exact_selection(),
            "FalloutNV.esm",
            &mappings,
            &exact_topology(&mappings),
        )
        .unwrap();
        let mapped_sources = plans
            .iter()
            .flat_map(|plan| plan.mappings.iter().map(|mapping| mapping.source.clone()))
            .collect::<BTreeSet<_>>();
        let mut expected_sources = required_slice_keys("FalloutNV.esm");
        expected_sources.extend(standalone_external_owner_keys("FalloutNV.esm"));
        expected_sources.extend(vtech_synthetic_records());
        assert_eq!(mapped_sources, expected_sources);

        let emitted = plans
            .iter()
            .flat_map(|plan| plan.expected_receipt.emitted_records.iter().cloned())
            .collect::<BTreeSet<_>>();
        let mut expected_emitted = required_slice_keys("FalloutNV.esm")
            .iter()
            .filter(|source| source.signature != "SCPT")
            .map(|source| mappings[source].clone())
            .collect::<BTreeSet<_>>();
        expected_emitted.extend(vtech_synthetic_records());
        assert_eq!(emitted, expected_emitted);
        assert_eq!(
            plans
                .iter()
                .flat_map(|plan| plan.expected_receipt.scripts.iter())
                .map(|receipt| receipt.class_name.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "FNV_FO3_FnvSliceCompat",
                "FNV_FO3_S_11FC64",
                "FNV_FO3_S_123191",
                "FNV_FO3_S_134491",
                "FNV_FO3_S_166305",
                "QF_FNV_FO3_06136D",
                "QF_FNV_FO3_11F935",
                "TIF__130161",
                "TIF__134B9B",
            ])
        );
    }

    #[test]
    fn placed_receipts_preserve_exact_cell_child_topology() {
        let mappings = exact_mappings();
        let topology = exact_topology(&mappings);
        let plans =
            admit_exact_fnv_quest_slice(&exact_selection(), "FalloutNV.esm", &mappings, &topology)
                .unwrap();

        let source_placements = plans
            .iter()
            .flat_map(|plan| plan.source_topology.iter())
            .filter(|placement| is_placed_record(&placement.record))
            .map(|placement| (placement.record.clone(), placement.group_path.clone()))
            .collect::<BTreeMap<_, _>>();
        let target_placements = plans
            .iter()
            .flat_map(|plan| plan.expected_receipt.placements.iter())
            .filter(|placement| is_placed_record(&placement.record))
            .map(|placement| (placement.record.clone(), placement.group_path.clone()))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(source_placements, topology.source);
        assert_eq!(target_placements, topology.target);
        assert_eq!(source_placements.len(), 4);
        assert_eq!(target_placements.len(), 4);
        for path in source_placements.values().chain(target_placements.values()) {
            assert_eq!(path.first().map(String::as_str), Some("GRUP:CELL"));
            assert!(
                path.iter()
                    .any(|segment| segment.starts_with("GRUP:type=6:"))
            );
            assert!(path.iter().any(|segment| {
                segment.starts_with("GRUP:type=8:") || segment.starts_with("GRUP:type=9:")
            }));
            assert!(
                !path
                    .iter()
                    .any(|segment| { segment == "GRUP:ACHR" || segment == "GRUP:REFR" })
            );
        }
    }

    #[test]
    fn expected_vmad_receipts_cover_the_full_live_attachment_inventory() {
        let mappings = exact_mappings();
        let plans = admit_exact_fnv_quest_slice(
            &exact_selection(),
            "FalloutNV.esm",
            &mappings,
            &exact_topology(&mappings),
        )
        .unwrap();
        let actual = plans
            .iter()
            .flat_map(|plan| plan.expected_receipt.vmad_attachments.iter())
            .map(|attachment| {
                (
                    attachment.owner.clone(),
                    attachment.script_class.clone(),
                    attachment.property_names.clone(),
                )
            })
            .collect::<BTreeSet<_>>();
        let mapped = |signature, local| mappings[&key(signature, local, "FalloutNV.esm")].clone();
        let expected = BTreeSet::from([
            (
                mapped("QUST", 0x06136D),
                "QF_FNV_FO3_06136D".to_owned(),
                BTreeSet::from(["FNVSliceCompat".to_owned()]),
            ),
            (
                mapped("QUST", 0x06136D),
                "FNV_FO3_FnvSliceCompat".to_owned(),
                BTreeSet::new(),
            ),
            (
                mapped("QUST", 0x11F935),
                "QF_FNV_FO3_11F935".to_owned(),
                BTreeSet::from(["FNVSliceCompat".to_owned()]),
            ),
            (
                mapped("QUST", 0x11F935),
                "FNV_FO3_FnvSliceCompat".to_owned(),
                BTreeSet::new(),
            ),
            (
                mapped("QUST", 0x11F935),
                "FNV_FO3_S_11FC64".to_owned(),
                BTreeSet::new(),
            ),
            (
                mapped("NPC_", 0x123193),
                "FNV_FO3_S_123191".to_owned(),
                BTreeSet::from([
                    "TecMineHostageEscapeData".to_owned(),
                    "TecMineHostageFreedGreeting".to_owned(),
                ]),
            ),
            (
                mapped("ACTI", 0x133F41),
                "FNV_FO3_S_134491".to_owned(),
                BTreeSet::from(["TechaticupNCRRenoldsDialoguePackageData".to_owned()]),
            ),
            (
                mapped("NPC_", 0x1300F0),
                "FNV_FO3_S_166305".to_owned(),
                BTreeSet::new(),
            ),
            (
                mapped("INFO", 0x130161),
                "TIF__130161".to_owned(),
                BTreeSet::from(["VTechatticup".to_owned()]),
            ),
            (
                mapped("INFO", 0x134B9B),
                "TIF__134B9B".to_owned(),
                BTreeSet::from(["VTechatticup".to_owned()]),
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn generated_scene_and_dedicated_runtime_records_are_not_omitted() {
        let mappings = exact_mappings();
        let plans = admit_exact_fnv_quest_slice(
            &exact_selection(),
            "FalloutNV.esm",
            &mappings,
            &exact_topology(&mappings),
        )
        .unwrap();
        let vtech = plans
            .iter()
            .find(|plan| local_id(&plan.root_quest) == VTECHATTICUP_QUEST)
            .unwrap();
        let emitted = &vtech.expected_receipt.emitted_records;

        assert!(vtech_synthetic_records().is_subset(emitted));
        assert!(
            vtech
                .expected_receipt
                .placements
                .iter()
                .filter(|placement| is_synthetic_intent(&placement.record))
                .map(|placement| placement.record.clone())
                .collect::<BTreeSet<_>>()
                == vtech_synthetic_records()
        );

        let dedicated_runtime = BTreeSet::from([
            QuestRecordKey::new("KYWD", DEDICATED_GREETING_KEYWORD_INTENT),
            QuestRecordKey::new("DIAL", DEDICATED_GREETING_TOPIC_INTENT),
            mappings[&key("INFO", 0x15734B, "FalloutNV.esm")].clone(),
            mappings[&key("INFO", 0x15734C, "FalloutNV.esm")].clone(),
            mappings[&key("INFO", 0x15734D, "FalloutNV.esm")].clone(),
        ]);
        assert_eq!(dedicated_runtime.len(), 5);
        assert!(dedicated_runtime.is_subset(emitted));

        let scene_runtime = BTreeSet::from([
            QuestRecordKey::new("SCEN", VTECH_SCENE_INTENT),
            QuestRecordKey::new("DLBR", VTECH_DIALOGUE_BRANCH_INTENT),
            mappings[&key("DIAL", 0x13015B, "FalloutNV.esm")].clone(),
            mappings[&key("DIAL", 0x134B9A, "FalloutNV.esm")].clone(),
            mappings[&key("DIAL", 0x138A74, "FalloutNV.esm")].clone(),
        ]);
        assert_eq!(scene_runtime.len(), 5);
        assert!(scene_runtime.is_subset(emitted));

        let resolved = [
            FnvQuestSyntheticIntent::Scene,
            FnvQuestSyntheticIntent::DialogueBranch,
            FnvQuestSyntheticIntent::DedicatedGreetingKeyword,
            FnvQuestSyntheticIntent::DedicatedGreetingTopic,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, intent)| {
            (
                intent.record_key(),
                intent.target_key(format!("{:06X}:Converted.esm", 0x700000 + index)),
            )
        })
        .collect::<BTreeMap<_, _>>();
        assert_eq!(
            resolved.keys().cloned().collect::<BTreeSet<_>>(),
            vtech_synthetic_records()
        );
        assert!(resolved.iter().all(|(intent, target)| {
            intent.signature == target.signature && !is_synthetic_intent(target)
        }));
    }
}
