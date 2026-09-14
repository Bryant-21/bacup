use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::ids::{FormKey, SigCode};
use crate::quest_runtime::{
    AliasFillKind, CompilerEvidenceIntent, ConditionIntent, DependencyClosure, FragmentIntent,
    LocalizedTextIntent, PscIntent, QuestAliasIntent, QuestObjectiveIntent, QuestRuntimeAdmission,
    QuestRuntimeComponentPlan, QuestRuntimeExpectedReceipt, QuestSourceGame, QuestStageIntent,
    QuestStartFlag, ScriptIntent, SourceProvenance, SourceTargetFormMapping, StartDisposition,
    StartProducerIntent, TopologyPlacement, VmadIntent,
};
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;
use serde::{Deserialize, Serialize};

use super::capability::{SkyrimRecordCapability, SkyrimRuntimeFamily, classify_record_capability};
use super::receipt::{
    RuntimeComponentDecision, RuntimeComponentReceipt, RuntimeRecordIdentity, RuntimeTopologyEdge,
    RuntimeTopologyKind, SignatureAdaptationReceipt,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTopologyEdge {
    pub parent: FormKey,
    pub child: FormKey,
    pub kind: RuntimeTopologyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimComponentDisposition {
    pub component_ids: Vec<String>,
    pub supported: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SkyrimCapabilityPlan {
    pub by_record: HashMap<FormKey, SkyrimComponentDisposition>,
    pub record_signatures: HashMap<FormKey, String>,
    pub target_signatures: HashMap<FormKey, String>,
    pub components: Vec<RuntimeComponentReceipt>,
    pub source_topology: Vec<RuntimeTopologyEdge>,
    pub minimal_quests: HashMap<FormKey, SkyrimMinimalQuestAdmission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkyrimMinimalQuestCandidateManifestRow {
    pub component_id: String,
    pub source_quest: String,
    pub source_script_class: String,
    pub source_fragment_function: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SkyrimMinimalQuestActionSpec {
    SetObjectiveDisplayed { index: u16, displayed: bool },
    SetObjectiveCompleted { index: u16, completed: bool },
    SetStage { index: u16 },
    CompleteQuest,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimMinimalQuestActionRequest {
    pub component_id: String,
    pub actions: Vec<SkyrimMinimalQuestActionSpec>,
}

#[derive(Debug, Clone)]
pub struct SkyrimMinimalQuestAdmission {
    pub plan: QuestRuntimeComponentPlan,
    pub editor_id: String,
    pub priority: u8,
    pub title: LocalizedTextIntent,
    pub fragment_id: String,
    pub stage_index: u16,
    pub stage_item_index: u16,
    pub actions: Vec<SkyrimMinimalQuestActionSpec>,
    pub source_script_class: String,
    pub source_fragment_function: String,
    pub alias_evidence: Vec<super::alias::SkyrimSourceAliasEvidence>,
}

#[derive(Debug, Clone)]
struct SkyrimMinimalQuestDraft {
    source_quest: FormKey,
    editor_id: String,
    priority: u8,
    title: LocalizedTextIntent,
    semantics: crate::quest_runtime::QuestSemanticPlan,
    fragment_id: String,
    stage_index: u16,
    stage_item_index: u16,
    source_script_class: String,
    source_fragment_function: String,
    dependency_form_keys: Vec<FormKey>,
    alias_evidence: Vec<super::alias::SkyrimSourceAliasEvidence>,
}

pub fn minimal_quest_candidate_manifest(
    record: &Record,
    interner: &StringInterner,
) -> Result<SkyrimMinimalQuestCandidateManifestRow, String> {
    let draft = derive_minimal_quest_draft(record, interner)?;
    Ok(SkyrimMinimalQuestCandidateManifestRow {
        component_id: minimal_quest_component_id(draft.source_quest, interner)?,
        source_quest: render_form_key(draft.source_quest, interner)?,
        source_script_class: draft.source_script_class,
        source_fragment_function: draft.source_fragment_function,
    })
}

pub fn minimal_quest_dependency_form_keys(
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<FormKey>, String> {
    Ok(derive_minimal_quest_draft(record, interner)?.dependency_form_keys)
}

pub fn admit_minimal_quest(
    record: &Record,
    target_by_source: &HashMap<FormKey, (FormKey, SigCode)>,
    class_prefix: &str,
    actions: Vec<SkyrimMinimalQuestActionSpec>,
    interner: &StringInterner,
) -> Result<SkyrimMinimalQuestAdmission, String> {
    let draft = derive_minimal_quest_draft(record, interner)?;
    if actions.is_empty() {
        return Err("minimal Skyrim QUST fragment has no verified actions".to_string());
    }
    validate_action_targets(&draft.semantics, &actions)?;
    let (target_quest, target_sig) = target_by_source
        .get(&draft.source_quest)
        .copied()
        .ok_or_else(|| "minimal Skyrim QUST has no preallocated target identity".to_string())?;
    if target_sig.as_str() != "QUST" {
        return Err("minimal Skyrim QUST target identity is not QUST".to_string());
    }
    let source_key = quest_key("QUST", draft.source_quest, interner)?;
    let target_key = quest_key("QUST", target_quest, interner)?;
    let projected_fragment = super::quest::SkyrimQuestFragmentSource {
        fragment_id: draft.fragment_id.clone(),
        stage_index: draft.stage_index,
        stage_item_index: draft.stage_item_index,
        actions: actions.iter().cloned().map(projected_action).collect(),
    };
    let (_, psc_manifest) = super::quest::minimal_quest_psc_artifact(
        class_prefix,
        target_quest.local,
        &projected_fragment,
    )?;
    let class_name = psc_manifest.class_name.clone();
    let compiler_evidence = CompilerEvidenceIntent {
        manifest_id: psc_manifest.manifest_id.clone(),
        source_digest: psc_manifest.source_blake3.clone(),
        require_fresh_output: true,
    };
    let mut mappings = Vec::new();
    for (source, (target, signature)) in target_by_source {
        mappings.push(SourceTargetFormMapping {
            source: quest_key(signature.as_str(), *source, interner)?,
            target: quest_key(signature.as_str(), *target, interner)?,
        });
    }
    mappings.sort();
    let shared_records = draft
        .dependency_form_keys
        .iter()
        .map(|source| {
            let (_, signature) = target_by_source.get(source).ok_or_else(|| {
                format!(
                    "minimal Skyrim QUST dependency {:06X} is not preallocated",
                    source.local
                )
            })?;
            quest_key(signature.as_str(), *source, interner)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let producer_id = format!("skyrim-qust-autostart:{:06X}", draft.source_quest.local);
    let evidence_id = format!("skyrim-qust-data-flags:{:06X}", draft.source_quest.local);
    let fragment = FragmentIntent {
        owner: source_key.clone(),
        fragment_id: draft.fragment_id.clone(),
        entrypoint: format!(
            "Fragment_Stage_{:04}_Item_{:02}",
            draft.stage_index, draft.stage_item_index
        ),
        target_class: class_name.clone(),
    };
    let psc = PscIntent {
        class_name: class_name.clone(),
        source_artifact: format!("Scripts/Source/User/{class_name}.psc"),
        compiler_evidence_required: true,
        compiler_evidence: Some(compiler_evidence.clone()),
    };
    let vmad = VmadIntent {
        owner: target_key.clone(),
        script_class: class_name.clone(),
        properties: BTreeSet::new(),
        compiler_evidence_required: true,
        compiler_evidence: Some(compiler_evidence),
    };
    let expected_receipt = QuestRuntimeExpectedReceipt {
        emitted_records: BTreeSet::from([target_key.clone()]),
        placements: BTreeSet::from([TopologyPlacement {
            record: target_key.clone(),
            group_path: vec!["GRUP:QUST".to_string()],
        }]),
        scripts: BTreeSet::from([crate::quest_runtime::ScriptArtifactReceipt {
            class_name: class_name.clone(),
            psc_path: format!("Scripts/Source/User/{class_name}.psc"),
            pex_path: format!("data/Scripts/{class_name}.pex"),
            compiler_evidence_id: psc_manifest.manifest_id.clone(),
        }]),
        vmad_attachments: BTreeSet::from([crate::quest_runtime::VmadAttachmentReceipt {
            owner: target_key.clone(),
            script_class: class_name.clone(),
            property_names: BTreeSet::new(),
            compiler_evidence_id: psc_manifest.manifest_id.clone(),
        }]),
        routes: BTreeSet::from([crate::quest_runtime::StartRouteReceipt {
            route_id: producer_id.clone(),
            quest: target_key.clone(),
            producer_evidence_id: evidence_id.clone(),
            node_chain: Vec::new(),
        }]),
        ..QuestRuntimeExpectedReceipt::default()
    };
    let plan = QuestRuntimeComponentPlan {
        component_id: minimal_quest_component_id(draft.source_quest, interner)?,
        root_quest: source_key.clone(),
        provenance: SourceProvenance {
            game: QuestSourceGame::SkyrimSe,
            source_plugin: interner
                .resolve(draft.source_quest.plugin)
                .unwrap_or_default()
                .to_string(),
            graft: None,
        },
        owned_records: vec![source_key.clone()],
        shared_records: shared_records.clone(),
        dependencies: DependencyClosure {
            direct: shared_records.clone(),
            recursive: shared_records,
        },
        source_topology: BTreeSet::from([TopologyPlacement {
            record: source_key.clone(),
            group_path: vec!["GRUP:QUST".to_string()],
        }]),
        target_topology: BTreeSet::from([TopologyPlacement {
            record: target_key.clone(),
            group_path: vec!["GRUP:QUST".to_string()],
        }]),
        mappings,
        semantics: draft.semantics.clone(),
        scripts: BTreeSet::from([ScriptIntent {
            owner: source_key.clone(),
            source_name: draft.source_script_class.clone(),
            target_class: class_name.clone(),
            required: true,
        }]),
        fragments: BTreeSet::from([fragment]),
        psc: BTreeSet::from([psc]),
        vmad: BTreeSet::from([vmad]),
        assets: BTreeSet::new(),
        inbound_producers: BTreeSet::from([StartProducerIntent {
            producer_id: producer_id.clone(),
            carrier: source_key,
            producer_kind: "qust_start_game_enabled".to_string(),
            evidence_id,
            proven: true,
        }]),
        start_disposition: Some(StartDisposition::Autostart),
        admission: QuestRuntimeAdmission::Supported,
        expected_receipt,
    };
    let issues = plan.validation_issues();
    if !issues.is_empty() {
        return Err(format!(
            "minimal Skyrim QUST component contract is invalid: {}",
            serde_json::to_string(&issues).unwrap_or_default()
        ));
    }
    Ok(SkyrimMinimalQuestAdmission {
        plan,
        editor_id: draft.editor_id,
        priority: draft.priority,
        title: draft.title,
        fragment_id: draft.fragment_id,
        stage_index: draft.stage_index,
        stage_item_index: draft.stage_item_index,
        actions,
        source_script_class: draft.source_script_class,
        source_fragment_function: draft.source_fragment_function,
        alias_evidence: draft.alias_evidence,
    })
}

pub fn is_managed_signature(signature: &str) -> bool {
    matches!(
        signature,
        "ACHR"
            | "AMMO"
            | "ARMA"
            | "ARMO"
            | "BPTD"
            | "CLFM"
            | "CSTY"
            | "DIAL"
            | "DLBR"
            | "DLVW"
            | "ENCH"
            | "EYES"
            | "FURN"
            | "HDPT"
            | "INFO"
            | "LSCR"
            | "LVLN"
            | "MGEF"
            | "MOVT"
            | "MUSC"
            | "MUST"
            | "NPC_"
            | "PACK"
            | "PERK"
            | "PROJ"
            | "QUST"
            | "RACE"
            | "SCEN"
            | "SCRL"
            | "SHOU"
            | "SMBN"
            | "SMEN"
            | "SMQN"
            | "SPEL"
            | "VTYP"
            | "WEAP"
    )
}

pub fn plan_components(
    records: Vec<Record>,
    topology_edges: &[SourceTopologyEdge],
    interner: &StringInterner,
) -> Result<SkyrimCapabilityPlan, String> {
    let mut by_key = HashMap::new();
    for record in records {
        if !is_managed_signature(record.sig.as_str())
            && !(record.sig.as_str() == "STAT"
                && record.form_key.local == 0x020E27
                && interner
                    .resolve(record.form_key.plugin)
                    .is_some_and(|plugin| plugin.eq_ignore_ascii_case("Skyrim.esm")))
        {
            continue;
        }
        if by_key.insert(record.form_key, record).is_some() {
            return Err("duplicate Skyrim runtime record in component preflight".to_string());
        }
    }

    let managed = by_key.keys().copied().collect::<HashSet<_>>();
    let supported_actor_races = by_key
        .values()
        .filter(|record| record.sig.as_str() == "RACE")
        .map(|record| record.form_key)
        .collect::<HashSet<_>>();
    let supported_actor_npcs = by_key
        .values()
        .filter(|record| record.sig.as_str() == "NPC_")
        .filter(|record| {
            exact_form_key(record, "RNAM").is_some_and(|race| supported_actor_races.contains(&race))
        })
        .map(|record| record.form_key)
        .collect::<HashSet<_>>();
    let supported_actors = by_key
        .values()
        .filter(|record| record.sig.as_str() == "ACHR")
        .filter(|record| {
            exact_form_key(record, "NAME").is_some_and(|npc| supported_actor_npcs.contains(&npc))
        })
        .map(|record| record.form_key)
        .collect::<HashSet<_>>();
    validate_total_actor_closure(
        &by_key,
        &supported_actor_races,
        &supported_actor_npcs,
        &supported_actors,
        interner,
    )?;
    let supported_simple_melee_weapons = by_key
        .values()
        .filter(|record| record.sig.as_str() == "WEAP")
        .filter(|record| super::weapon::simple_melee_weapon_source(record, interner).is_some())
        .map(|record| record.form_key)
        .collect::<HashSet<_>>();
    let mut dependencies = managed
        .iter()
        .copied()
        .map(|form_key| (form_key, HashSet::new()))
        .collect::<HashMap<_, _>>();
    let mut owned_children = HashMap::<FormKey, HashSet<FormKey>>::new();
    let mut referenced = HashSet::new();
    let mut source_topology = Vec::with_capacity(topology_edges.len());
    for edge in topology_edges {
        source_topology.push(RuntimeTopologyEdge {
            parent_form_key: render_form_key(edge.parent, interner)?,
            child_form_key: render_form_key(edge.child, interner)?,
            kind: edge.kind,
        });
        if managed.contains(&edge.parent) && managed.contains(&edge.child) {
            owned_children
                .entry(edge.parent)
                .or_default()
                .insert(edge.child);
            referenced.insert(edge.child);
        }
    }
    for record in by_key.values() {
        if record.sig.as_str() == "WEAP"
            && !supported_simple_melee_weapons.contains(&record.form_key)
        {
            continue;
        }
        let mut references = HashSet::new();
        for field in &record.fields {
            if is_donor_normalized_equipment_reference(record.sig.as_str(), field.sig.as_str()) {
                continue;
            }
            if supported_actor_npcs.contains(&record.form_key) && field.sig.as_str() != "RNAM" {
                continue;
            }
            if supported_actor_races.contains(&record.form_key) {
                continue;
            }
            if supported_actors.contains(&record.form_key) && field.sig.as_str() != "NAME" {
                continue;
            }
            collect_form_keys(&field.value, &mut references);
        }
        for target in references {
            if target == record.form_key || !managed.contains(&target) {
                continue;
            }
            if !should_link_component_dependency(record.sig.as_str(), by_key[&target].sig.as_str())
            {
                continue;
            }
            dependencies
                .entry(record.form_key)
                .or_default()
                .insert(target);
            referenced.insert(target);
            if matches!(record.sig.as_str(), "DIAL" | "DLBR" | "SCEN")
                && by_key[&target].sig.as_str() == "QUST"
            {
                owned_children
                    .entry(target)
                    .or_default()
                    .insert(record.form_key);
                source_topology.push(RuntimeTopologyEdge {
                    parent_form_key: render_form_key(target, interner)?,
                    child_form_key: render_form_key(record.form_key, interner)?,
                    kind: RuntimeTopologyKind::QuestChild,
                });
            }
        }
    }

    let mut roots = managed
        .iter()
        .copied()
        .filter(|form_key| is_component_root(by_key[form_key].sig.as_str()))
        .collect::<HashSet<_>>();
    roots.extend(
        managed
            .iter()
            .copied()
            .filter(|form_key| !referenced.contains(form_key))
            .filter(|form_key| {
                !owned_children
                    .values()
                    .any(|children| children.contains(form_key))
            }),
    );
    let mut ordered_roots = roots.into_iter().collect::<Vec<_>>();
    ordered_roots.sort_by_key(|form_key| sort_key(*form_key, interner));

    let mut plan = SkyrimCapabilityPlan {
        record_signatures: by_key
            .iter()
            .map(|(form_key, record)| (*form_key, record.sig.as_str().to_string()))
            .collect(),
        target_signatures: by_key
            .iter()
            .map(|(form_key, record)| {
                (*form_key, target_signature(record.sig.as_str()).to_string())
            })
            .collect(),
        source_topology,
        ..Default::default()
    };
    let mut covered = HashSet::new();
    for start in ordered_roots {
        let mut queue = VecDeque::from([start]);
        let mut members = HashSet::new();
        while let Some(current) = queue.pop_front() {
            if !members.insert(current) {
                continue;
            }
            if let Some(children) = owned_children.get(&current) {
                queue.extend(children.iter().copied());
            }
            if let Some(required) = dependencies.get(&current) {
                queue.extend(required.iter().copied());
            }
        }
        covered.extend(members.iter().copied());
        let mut ordered_members = members.iter().copied().collect::<Vec<_>>();
        ordered_members.sort_by_key(|form_key| sort_key(*form_key, interner));

        let mut reasons = BTreeSet::new();
        let mut families = BTreeSet::new();
        let mut supported = true;
        for member in &ordered_members {
            let record = &by_key[member];
            let capability = classify_record_capability(record, interner);
            if let Some(family) = capability.family() {
                families.insert(family_name(family));
            }
            if let Some(reason) = planned_drop_reason(
                record,
                &supported_actor_races,
                &supported_actor_npcs,
                &supported_actors,
                &capability,
            ) {
                supported = false;
                reasons.insert(reason);
            }
        }
        let component_id = format!("skyrim-runtime:{}", render_form_key(start, interner)?);
        let reason = (!supported).then(|| reasons.into_iter().collect::<Vec<_>>().join("; "));
        for member in &ordered_members {
            let disposition =
                plan.by_record
                    .entry(*member)
                    .or_insert_with(|| SkyrimComponentDisposition {
                        component_ids: Vec::new(),
                        supported: false,
                        reason: None,
                    });
            disposition.component_ids.push(component_id.clone());
            if supported {
                disposition.supported = true;
                disposition.reason = None;
            } else if !disposition.supported {
                let reason = reason.as_ref().expect("unsupported component has a reason");
                let reasons = disposition.reason.get_or_insert_with(String::new);
                if !reasons.is_empty() {
                    reasons.push_str("; ");
                }
                reasons.push_str(reason);
            }
        }
        plan.components.push(RuntimeComponentReceipt {
            component_id,
            family: if families.len() == 1 {
                families.into_iter().next().unwrap().to_string()
            } else {
                "mixed".to_string()
            },
            roots: vec![record_identity(&by_key[&start], interner)?],
            members: ordered_members
                .iter()
                .map(|form_key| record_identity(&by_key[form_key], interner))
                .collect::<Result<_, _>>()?,
            decision: if supported {
                RuntimeComponentDecision::Supported
            } else {
                RuntimeComponentDecision::Unsupported
            },
            reason,
            adaptations: ordered_members
                .iter()
                .filter_map(|form_key| {
                    signature_adaptation(&by_key[form_key], interner).transpose()
                })
                .collect::<Result<_, _>>()?,
        });
    }
    for missing in managed.difference(&covered).copied().collect::<Vec<_>>() {
        let record = &by_key[&missing];
        let capability = classify_record_capability(record, interner);
        let reason = planned_drop_reason(
            record,
            &supported_actor_races,
            &supported_actor_npcs,
            &supported_actors,
            &capability,
        );
        let supported = reason.is_none();
        let reason = reason.map(str::to_string);
        let component_id = format!("skyrim-runtime:{}", render_form_key(missing, interner)?);
        plan.by_record.insert(
            missing,
            SkyrimComponentDisposition {
                component_ids: vec![component_id.clone()],
                supported,
                reason: reason.clone(),
            },
        );
        let identity = record_identity(record, interner)?;
        plan.components.push(RuntimeComponentReceipt {
            component_id,
            family: capability
                .family()
                .map(family_name)
                .unwrap_or("unmanaged")
                .to_string(),
            roots: vec![identity.clone()],
            members: vec![identity],
            decision: if supported {
                RuntimeComponentDecision::Supported
            } else {
                RuntimeComponentDecision::Unsupported
            },
            reason,
            adaptations: signature_adaptation(record, interner)?
                .into_iter()
                .collect(),
        });
    }
    plan.components
        .sort_by(|left, right| left.component_id.cmp(&right.component_id));
    Ok(plan)
}

fn exact_form_key(record: &Record, signature: &str) -> Option<FormKey> {
    let mut values = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .filter_map(|field| match &field.value {
            FieldValue::FormKey(form_key) => Some(*form_key),
            _ => None,
        });
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

fn derive_minimal_quest_draft(
    record: &Record,
    interner: &StringInterner,
) -> Result<SkyrimMinimalQuestDraft, String> {
    if record.sig.as_str() != "QUST" {
        return Err("minimal Skyrim quest candidate is not QUST".to_string());
    }
    let editor_id = record
        .eid
        .and_then(|value| interner.resolve(value))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "minimal Skyrim QUST has no EditorID".to_string())?
        .to_string();
    let title = localized_text_field(record, "FULL", "STRINGS", interner)?;
    let (source_flags, priority) = quest_general_data(record, interner)?;
    if source_flags & 0x01 == 0 {
        return Err("minimal Skyrim QUST is not Start Game Enabled".to_string());
    }
    let (source_script_class, fragments) = quest_fragment_metadata(record)?;
    if fragments.len() != 1 {
        return Err("minimal Skyrim QUST requires exactly one VMAD stage fragment".to_string());
    }
    let fragment = &fragments[0];
    let stages = quest_stages(record, fragment, interner)?;
    let objectives = quest_objectives(record, interner)?;
    let (aliases, alias_evidence) = quest_aliases(record, interner)?;
    if stages.is_empty() || objectives.is_empty() || aliases.len() != 1 {
        return Err(
            "minimal Skyrim QUST requires stages, objectives, and one forced-reference alias"
                .to_string(),
        );
    }
    let conditions = quest_conditions(record, interner)?;
    let mut dependency_form_keys = Vec::new();
    for alias in &aliases {
        if let AliasFillKind::ForcedReference(value) = &alias.fill {
            dependency_form_keys.push(parse_rendered_form_key(value, interner)?);
        }
    }
    for condition in &conditions {
        let source = parse_rendered_form_key(&condition.parameters[0], interner)?;
        if source != record.form_key {
            dependency_form_keys.push(source);
        }
    }
    dependency_form_keys.sort_by_key(|form_key| sort_key(*form_key, interner));
    dependency_form_keys.dedup();
    let mut start_flags = BTreeSet::from([QuestStartFlag::StartGameEnabled]);
    if source_flags & 0x04 != 0 {
        start_flags.insert(QuestStartFlag::RunOnce);
    }
    Ok(SkyrimMinimalQuestDraft {
        source_quest: record.form_key,
        editor_id,
        priority,
        title,
        semantics: crate::quest_runtime::QuestSemanticPlan {
            stages,
            objectives,
            aliases,
            conditions,
            start_flags,
        },
        fragment_id: fragment.fragment_id.clone(),
        stage_index: fragment.stage_index,
        stage_item_index: fragment.stage_item_index,
        source_script_class,
        source_fragment_function: fragment.function_name.clone(),
        dependency_form_keys,
        alias_evidence,
    })
}

#[derive(Debug, Clone)]
struct SourceQuestFragment {
    fragment_id: String,
    stage_index: u16,
    stage_item_index: u16,
    function_name: String,
}

fn quest_fragment_metadata(record: &Record) -> Result<(String, Vec<SourceQuestFragment>), String> {
    let bytes = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "VMAD")
        .and_then(|field| match &field.value {
            FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .ok_or_else(|| "minimal Skyrim QUST has no raw VMAD fragment metadata".to_string())?;
    let object_format = read_u16(bytes, 2)?;
    if !matches!(object_format, 1 | 2) {
        return Err("minimal Skyrim QUST VMAD object format is unsupported".to_string());
    }
    let script_count = read_u16(bytes, 4)? as usize;
    let mut offset = 6;
    for _ in 0..script_count {
        skip_vmad_script(bytes, &mut offset, object_format)?;
    }
    read_byte(bytes, &mut offset)?;
    let fragment_count = read_u16_advance(bytes, &mut offset)? as usize;
    let source_script_class = read_vmad_string(bytes, &mut offset)?;
    if source_script_class.is_empty() {
        return Err("minimal Skyrim QUST VMAD fragment script is empty".to_string());
    }
    read_byte(bytes, &mut offset)?;
    let property_count = read_u16_advance(bytes, &mut offset)? as usize;
    if property_count != 0 {
        return Err("minimal Skyrim QUST fragment script properties are unsupported".to_string());
    }
    let mut fragments = Vec::with_capacity(fragment_count);
    for index in 0..fragment_count {
        let stage_index = read_u16_advance(bytes, &mut offset)?;
        advance(bytes, &mut offset, 2)?;
        let stage_item_index = read_i32_advance(bytes, &mut offset)?;
        if !(0..=u16::MAX as i32).contains(&stage_item_index) {
            return Err("minimal Skyrim QUST fragment stage-item index is invalid".to_string());
        }
        read_byte(bytes, &mut offset)?;
        let entry_script = read_vmad_string(bytes, &mut offset)?;
        let function_name = read_vmad_string(bytes, &mut offset)?;
        if !entry_script.is_empty() && !entry_script.eq_ignore_ascii_case(&source_script_class) {
            return Err("minimal Skyrim QUST fragment names another script".to_string());
        }
        if function_name.is_empty() {
            return Err("minimal Skyrim QUST fragment function is empty".to_string());
        }
        fragments.push(SourceQuestFragment {
            fragment_id: format!("vmad-fragment-{index}"),
            stage_index,
            stage_item_index: stage_item_index as u16,
            function_name,
        });
    }
    let remaining = &bytes[offset..];
    if !remaining.is_empty() && remaining != [0, 0] {
        return Err("minimal Skyrim QUST alias VMAD is unsupported".to_string());
    }
    Ok((source_script_class, fragments))
}

fn skip_vmad_script(bytes: &[u8], offset: &mut usize, object_format: u16) -> Result<(), String> {
    read_vmad_string(bytes, offset)?;
    read_byte(bytes, offset)?;
    let property_count = read_u16_advance(bytes, offset)? as usize;
    for _ in 0..property_count {
        read_vmad_string(bytes, offset)?;
        let kind = read_byte(bytes, offset)?;
        read_byte(bytes, offset)?;
        skip_vmad_property(bytes, offset, kind, object_format)?;
    }
    Ok(())
}

fn skip_vmad_property(
    bytes: &[u8],
    offset: &mut usize,
    kind: u8,
    object_format: u16,
) -> Result<(), String> {
    match kind {
        1 => advance(bytes, offset, 8),
        2 | 4 | 6 => advance(bytes, offset, 4),
        3 => read_vmad_string(bytes, offset).map(|_| ()),
        5 => advance(bytes, offset, 1),
        11..=15 => {
            let count = read_u32_advance(bytes, offset)? as usize;
            for _ in 0..count {
                skip_vmad_property(bytes, offset, kind - 10, object_format)?;
            }
            Ok(())
        }
        _ => Err(format!(
            "minimal Skyrim QUST VMAD property type {kind} is unsupported"
        )),
    }
}

fn quest_stages(
    record: &Record,
    fragment: &SourceQuestFragment,
    interner: &StringInterner,
) -> Result<Vec<QuestStageIntent>, String> {
    let mut stages = Vec::new();
    let mut index = 0;
    while index < record.fields.len() {
        if record.fields[index].sig.as_str() != "INDX" {
            index += 1;
            continue;
        }
        let stage_index = scalar_value(&record.fields[index].value, Some("stage_index"), interner)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| "minimal Skyrim QUST stage index is malformed".to_string())?;
        index += 1;
        let mut log_entries = Vec::new();
        while index < record.fields.len() && record.fields[index].sig.as_str() != "INDX" {
            if record.fields[index].sig.as_str() == "CNAM" {
                log_entries.push(localized_text_value(
                    &record.fields[index].value,
                    "CNAM",
                    "DLSTRINGS",
                    interner,
                )?);
            }
            if matches!(record.fields[index].sig.as_str(), "QOBJ" | "ANAM" | "ALST") {
                break;
            }
            index += 1;
        }
        if log_entries.is_empty() {
            return Err("minimal Skyrim QUST stage has no localized log entry".to_string());
        }
        let fragment_ids = if stage_index == fragment.stage_index {
            BTreeSet::from([fragment.fragment_id.clone()])
        } else {
            BTreeSet::new()
        };
        stages.push(QuestStageIntent {
            index: stage_index,
            log_entries,
            fragment_ids,
        });
    }
    if !stages
        .iter()
        .any(|stage| stage.index == fragment.stage_index)
    {
        return Err("minimal Skyrim QUST VMAD fragment has no source stage".to_string());
    }
    let stage = stages
        .iter()
        .find(|stage| stage.index == fragment.stage_index)
        .unwrap();
    if usize::from(fragment.stage_item_index) >= stage.log_entries.len() {
        return Err("minimal Skyrim QUST VMAD fragment stage-item is missing".to_string());
    }
    Ok(stages)
}

fn quest_objectives(
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<QuestObjectiveIntent>, String> {
    let mut objectives = Vec::new();
    let mut index = 0;
    while index < record.fields.len() {
        if record.fields[index].sig.as_str() != "QOBJ" {
            index += 1;
            continue;
        }
        let objective_index = scalar_value(&record.fields[index].value, None, interner)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| "minimal Skyrim QUST objective index is malformed".to_string())?;
        index += 1;
        let mut display_text = None;
        let mut target_aliases = BTreeSet::new();
        while index < record.fields.len() && record.fields[index].sig.as_str() != "QOBJ" {
            match record.fields[index].sig.as_str() {
                "NNAM" => {
                    display_text = Some(localized_text_value(
                        &record.fields[index].value,
                        "NNAM",
                        "STRINGS",
                        interner,
                    )?);
                }
                "QSTA" => {
                    let alias = scalar_value(&record.fields[index].value, Some("alias"), interner)
                        .and_then(|value| u32::try_from(value).ok())
                        .ok_or_else(|| {
                            "minimal Skyrim QUST objective alias is malformed".to_string()
                        })?;
                    target_aliases.insert(alias);
                }
                "ANAM" | "ALST" => break,
                _ => {}
            }
            index += 1;
        }
        objectives.push(QuestObjectiveIntent {
            index: objective_index,
            display_text: display_text
                .ok_or_else(|| "minimal Skyrim QUST objective has no display text".to_string())?,
            target_aliases,
        });
    }
    Ok(objectives)
}

fn quest_aliases(
    record: &Record,
    interner: &StringInterner,
) -> Result<
    (
        Vec<QuestAliasIntent>,
        Vec<super::alias::SkyrimSourceAliasEvidence>,
    ),
    String,
> {
    let mut aliases = Vec::new();
    let mut evidence = Vec::new();
    let owner = crate::quest_runtime::QuestRecordKey::new(
        "QUST",
        render_form_key(record.form_key, interner)?,
    );
    let mut index = 0;
    while index < record.fields.len() {
        if record.fields[index].sig.as_str() != "ALST" {
            index += 1;
            continue;
        }
        let alias_id = scalar_value(&record.fields[index].value, None, interner)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| "minimal Skyrim QUST alias id is malformed".to_string())?;
        index += 1;
        let mut name = None;
        let mut forced_reference = None;
        let mut flags = None;
        while index < record.fields.len() && record.fields[index].sig.as_str() != "ALED" {
            match record.fields[index].sig.as_str() {
                "ALID" => name = text_value(&record.fields[index].value, interner),
                "ALFR" => forced_reference = first_form_key(&record.fields[index].value),
                "FNAM" => {
                    flags = scalar_value(&record.fields[index].value, None, interner)
                        .and_then(|value| u32::try_from(value).ok())
                }
                "CTDA" | "CTDT" => {
                    return Err("minimal Skyrim QUST alias conditions are unsupported".to_string());
                }
                "ALUA" | "ALCO" | "ALLA" | "ALFE" | "ALEQ" | "ALDN" | "ALSP" | "ALRT" => {
                    return Err("minimal Skyrim QUST alias fill type is unsupported".to_string());
                }
                _ => {}
            }
            index += 1;
        }
        if index >= record.fields.len() {
            return Err("minimal Skyrim QUST alias has no ALED terminator".to_string());
        }
        let alias = QuestAliasIntent {
            id: alias_id,
            name: name
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "minimal Skyrim QUST alias has no name".to_string())?,
            fill: AliasFillKind::ForcedReference(render_form_key(
                forced_reference.ok_or_else(|| {
                    "minimal Skyrim QUST alias is not forced-reference".to_string()
                })?,
                interner,
            )?),
            conditions: Vec::new(),
        };
        evidence.push(super::alias::SkyrimSourceAliasEvidence {
            owner: owner.clone(),
            alias_id,
            name: alias.name.clone(),
            flags: flags
                .ok_or_else(|| "minimal Skyrim QUST alias has no exact FNAM flags".to_string())?,
            fill: alias.fill.clone(),
            conditions: Vec::new(),
            vmad: super::alias::SkyrimAliasVmadEvidence::default(),
        });
        aliases.push(alias);
        index += 1;
    }
    Ok((aliases, evidence))
}

fn quest_conditions(
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<ConditionIntent>, String> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "CTDA")
        .map(|field| condition_intent(&field.value, interner))
        .collect()
}

fn condition_intent(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<ConditionIntent, String> {
    let function = scalar_value(value, Some("function"), interner)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| "minimal Skyrim QUST condition function is malformed".to_string())?;
    let function_name = match function {
        72 => "GetIsID",
        59 => "GetStageDone",
        _ => {
            return Err(format!(
                "unsupported required Skyrim quest condition {function}"
            ));
        }
    };
    let parameter_1 = named_form_key(value, "parameter_1", interner)
        .ok_or_else(|| "minimal Skyrim QUST condition parameter 1 is malformed".to_string())?;
    let mut parameters = vec![render_form_key(parameter_1, interner)?];
    if function == 59 {
        let stage = scalar_value(value, Some("parameter_2"), interner)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| "minimal Skyrim QUST GetStageDone stage is malformed".to_string())?;
        parameters.push(stage.to_string());
    }
    let comparison = float_value(value, "comparison_value", interner)
        .ok_or_else(|| "minimal Skyrim QUST condition comparison is malformed".to_string())?;
    if (comparison - 1.0).abs() > f32::EPSILON
        || scalar_value(value, Some("run_on"), interner).unwrap_or(0) != 0
    {
        return Err("minimal Skyrim QUST condition operator/run-on is unsupported".to_string());
    }
    Ok(ConditionIntent {
        function: function_name.to_string(),
        operator: "==".to_string(),
        comparison_value: "1".to_string(),
        parameters,
        run_on: "Subject".to_string(),
        cis1: None,
        cis2: None,
    })
}

fn localized_text_field(
    record: &Record,
    field: &str,
    table: &str,
    interner: &StringInterner,
) -> Result<LocalizedTextIntent, String> {
    let value = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == field)
        .ok_or_else(|| format!("minimal Skyrim QUST has no {field}"))?;
    localized_text_value(&value.value, field, table, interner)
}

fn localized_text_value(
    value: &FieldValue,
    field: &str,
    table: &str,
    interner: &StringInterner,
) -> Result<LocalizedTextIntent, String> {
    let text = text_value(value, interner)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("minimal Skyrim QUST {field} text is empty"))?;
    Ok(LocalizedTextIntent {
        field: field.to_string(),
        source_text_id: None,
        text,
        target_table: table.to_string(),
    })
}

fn quest_general_data(record: &Record, interner: &StringInterner) -> Result<(u64, u8), String> {
    let value = &record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "DNAM")
        .ok_or_else(|| "minimal Skyrim QUST has no DNAM".to_string())?
        .value;
    let flags = scalar_value(value, Some("flags"), interner)
        .ok_or_else(|| "minimal Skyrim QUST DNAM flags are malformed".to_string())?;
    let priority = scalar_value(value, Some("priority"), interner)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| "minimal Skyrim QUST DNAM priority is malformed".to_string())?;
    Ok((flags, priority))
}

fn validate_action_targets(
    semantics: &crate::quest_runtime::QuestSemanticPlan,
    actions: &[SkyrimMinimalQuestActionSpec],
) -> Result<(), String> {
    let objective_ids = semantics
        .objectives
        .iter()
        .map(|objective| objective.index)
        .collect::<BTreeSet<_>>();
    let stage_ids = semantics
        .stages
        .iter()
        .map(|stage| stage.index)
        .collect::<BTreeSet<_>>();
    for action in actions {
        match action {
            SkyrimMinimalQuestActionSpec::SetObjectiveDisplayed { index, .. }
            | SkyrimMinimalQuestActionSpec::SetObjectiveCompleted { index, .. }
                if !objective_ids.contains(index) =>
            {
                return Err(format!(
                    "minimal Skyrim QUST action references missing objective {index}"
                ));
            }
            SkyrimMinimalQuestActionSpec::SetStage { index } if !stage_ids.contains(index) => {
                return Err(format!(
                    "minimal Skyrim QUST action references missing stage {index}"
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn projected_action(
    action: SkyrimMinimalQuestActionSpec,
) -> super::quest::SkyrimQuestFragmentAction {
    match action {
        SkyrimMinimalQuestActionSpec::SetObjectiveDisplayed { index, displayed } => {
            super::quest::SkyrimQuestFragmentAction::SetObjectiveDisplayed { index, displayed }
        }
        SkyrimMinimalQuestActionSpec::SetObjectiveCompleted { index, completed } => {
            super::quest::SkyrimQuestFragmentAction::SetObjectiveCompleted { index, completed }
        }
        SkyrimMinimalQuestActionSpec::SetStage { index } => {
            super::quest::SkyrimQuestFragmentAction::SetStage(index)
        }
        SkyrimMinimalQuestActionSpec::CompleteQuest => {
            super::quest::SkyrimQuestFragmentAction::CompleteQuest
        }
        SkyrimMinimalQuestActionSpec::Stop => super::quest::SkyrimQuestFragmentAction::Stop,
    }
}

fn minimal_quest_component_id(
    form_key: FormKey,
    interner: &StringInterner,
) -> Result<String, String> {
    Ok(format!(
        "skyrim-minimal-quest:{}",
        render_form_key(form_key, interner)?
    ))
}

fn quest_key(
    signature: &str,
    form_key: FormKey,
    interner: &StringInterner,
) -> Result<crate::quest_runtime::QuestRecordKey, String> {
    Ok(crate::quest_runtime::QuestRecordKey::new(
        signature,
        render_form_key(form_key, interner)?,
    ))
}

fn parse_rendered_form_key(value: &str, interner: &StringInterner) -> Result<FormKey, String> {
    FormKey::parse(value, interner)
}

fn named_form_key(value: &FieldValue, name: &str, interner: &StringInterner) -> Option<FormKey> {
    match value {
        FieldValue::Struct(fields) => fields.iter().find_map(|(field_name, value)| {
            interner
                .resolve(*field_name)
                .is_some_and(|field_name| canonical(field_name) == canonical(name))
                .then(|| first_form_key(value))
                .flatten()
        }),
        _ => None,
    }
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(value) => Some(*value),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        FieldValue::List(values) => values.iter().find_map(first_form_key),
        _ => None,
    }
}

fn scalar_value(value: &FieldValue, name: Option<&str>, interner: &StringInterner) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) if *value >= 0 => Some(*value as u64),
        FieldValue::Bool(value) => Some(u64::from(*value)),
        FieldValue::Bytes(bytes) => match bytes.len() {
            1 => Some(bytes[0] as u64),
            2 => Some(u16::from_le_bytes(bytes[..2].try_into().ok()?) as u64),
            4.. => Some(u32::from_le_bytes(bytes[..4].try_into().ok()?) as u64),
            _ => None,
        },
        FieldValue::Struct(fields) => {
            let selected = name
                .and_then(|name| {
                    fields.iter().find_map(|(field_name, value)| {
                        interner
                            .resolve(*field_name)
                            .is_some_and(|field_name| canonical(field_name) == canonical(name))
                            .then_some(value)
                    })
                })
                .or_else(|| fields.first().map(|(_, value)| value))?;
            scalar_value(selected, None, interner)
        }
        FieldValue::List(values) => values
            .first()
            .and_then(|value| scalar_value(value, None, interner)),
        _ => None,
    }
}

fn float_value(value: &FieldValue, name: &str, interner: &StringInterner) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value as f32),
        FieldValue::Struct(fields) => fields.iter().find_map(|(field_name, value)| {
            interner
                .resolve(*field_name)
                .is_some_and(|field_name| canonical(field_name) == canonical(name))
                .then(|| float_value(value, name, interner))
                .flatten()
        }),
        _ => None,
    }
}

fn text_value(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(value) => interner.resolve(*value).map(str::to_string),
        FieldValue::Bytes(bytes) => std::str::from_utf8(bytes)
            .ok()
            .map(|value| value.trim_end_matches('\0').to_string()),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| text_value(value, interner)),
        FieldValue::List(values) => values.iter().find_map(|value| text_value(value, interner)),
        _ => None,
    }
}

fn canonical(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| "minimal Skyrim QUST VMAD is truncated".to_string())
}

fn read_u16_advance(bytes: &[u8], offset: &mut usize) -> Result<u16, String> {
    let value = read_u16(bytes, *offset)?;
    *offset += 2;
    Ok(value)
}

fn read_u32_advance(bytes: &[u8], offset: &mut usize) -> Result<u32, String> {
    let value = bytes
        .get(*offset..*offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| "minimal Skyrim QUST VMAD is truncated".to_string())?;
    *offset += 4;
    Ok(value)
}

fn read_i32_advance(bytes: &[u8], offset: &mut usize) -> Result<i32, String> {
    read_u32_advance(bytes, offset).map(|value| i32::from_le_bytes(value.to_le_bytes()))
}

fn read_byte(bytes: &[u8], offset: &mut usize) -> Result<u8, String> {
    let value = bytes
        .get(*offset)
        .copied()
        .ok_or_else(|| "minimal Skyrim QUST VMAD is truncated".to_string())?;
    *offset += 1;
    Ok(value)
}

fn advance(bytes: &[u8], offset: &mut usize, count: usize) -> Result<(), String> {
    if bytes.get(*offset..*offset + count).is_none() {
        return Err("minimal Skyrim QUST VMAD is truncated".to_string());
    }
    *offset += count;
    Ok(())
}

fn read_vmad_string(bytes: &[u8], offset: &mut usize) -> Result<String, String> {
    let length = read_u16_advance(bytes, offset)? as usize;
    let value = bytes
        .get(*offset..*offset + length)
        .ok_or_else(|| "minimal Skyrim QUST VMAD string is truncated".to_string())?;
    *offset += length;
    std::str::from_utf8(value)
        .map(str::to_string)
        .map_err(|_| "minimal Skyrim QUST VMAD string is not UTF-8".to_string())
}

fn should_link_component_dependency(owner: &str, target: &str) -> bool {
    is_quest_runtime_signature(owner) == is_quest_runtime_signature(target)
}

fn is_quest_runtime_signature(signature: &str) -> bool {
    matches!(
        signature,
        "QUST" | "SMBN" | "SMEN" | "SMQN" | "DLVW" | "DLBR" | "DIAL" | "INFO" | "SCEN" | "PACK"
    )
}

fn target_signature(source: &str) -> &str {
    match source {
        "SCRL" => "ALCH",
        "SHOU" => "SPEL",
        other => other,
    }
}

fn signature_adaptation(
    record: &Record,
    interner: &StringInterner,
) -> Result<Option<SignatureAdaptationReceipt>, String> {
    let source_signature = record.sig.as_str();
    let target_signature = target_signature(source_signature);
    if source_signature == target_signature {
        return Ok(None);
    }
    Ok(Some(SignatureAdaptationReceipt {
        source_form_key: render_form_key(record.form_key, interner)?,
        source_signature: source_signature.to_string(),
        target_signature: target_signature.to_string(),
        semantic_role: match source_signature {
            "SCRL" => "scroll-consumable",
            "SHOU" => "shout-spell-fallback",
            _ => unreachable!("only adapted signatures reach this branch"),
        }
        .to_string(),
    }))
}

fn validate_total_actor_closure(
    by_key: &HashMap<FormKey, Record>,
    races: &HashSet<FormKey>,
    npcs: &HashSet<FormKey>,
    actors: &HashSet<FormKey>,
    interner: &StringInterner,
) -> Result<(), String> {
    for record in by_key.values() {
        let (dependency_signature, supported) = match record.sig.as_str() {
            "NPC_" => ("RNAM", npcs.contains(&record.form_key)),
            "ACHR" => ("NAME", actors.contains(&record.form_key)),
            _ => continue,
        };
        if supported {
            continue;
        }
        let dependency = exact_form_key(record, dependency_signature)
            .map(|form_key| render_form_key(form_key, interner))
            .transpose()?
            .unwrap_or_else(|| "missing-or-repeated".to_string());
        let expected = if record.sig.as_str() == "NPC_" {
            "RACE"
        } else {
            "NPC_"
        };
        return Err(format!(
            "Skyrim {} {} has no resolvable {} {} dependency ({dependency})",
            record.sig.as_str(),
            render_form_key(record.form_key, interner)?,
            expected,
            dependency_signature,
        ));
    }
    if races.len()
        != by_key
            .values()
            .filter(|record| record.sig.as_str() == "RACE")
            .count()
    {
        return Err("Skyrim actor preflight did not admit every RACE record".to_string());
    }
    Ok(())
}

fn planned_drop_reason(
    record: &Record,
    supported_races: &HashSet<FormKey>,
    supported_npcs: &HashSet<FormKey>,
    supported_actors: &HashSet<FormKey>,
    capability: &SkyrimRecordCapability,
) -> Option<&'static str> {
    if supported_races.contains(&record.form_key)
        || supported_npcs.contains(&record.form_key)
        || supported_actors.contains(&record.form_key)
    {
        None
    } else {
        capability.drop_reason()
    }
}

fn is_donor_normalized_equipment_reference(record_signature: &str, field_signature: &str) -> bool {
    matches!(
        (record_signature, field_signature),
        ("ARMO", "RNAM") | ("ARMA", "RNAM" | "MODL")
    )
}

fn is_component_root(signature: &str) -> bool {
    matches!(
        signature,
        "QUST" | "MUSC" | "NPC_" | "ACHR" | "SPEL" | "WEAP"
    )
}

fn collect_form_keys(value: &FieldValue, output: &mut HashSet<FormKey>) {
    match value {
        FieldValue::FormKey(form_key) if form_key.local != 0 => {
            output.insert(*form_key);
        }
        FieldValue::List(values) => {
            for value in values {
                collect_form_keys(value, output);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_form_keys(value, output);
            }
        }
        _ => {}
    }
}

fn record_identity(
    record: &Record,
    interner: &StringInterner,
) -> Result<RuntimeRecordIdentity, String> {
    Ok(RuntimeRecordIdentity {
        form_key: render_form_key(record.form_key, interner)?,
        signature: record.sig.as_str().to_string(),
    })
}

fn render_form_key(form_key: FormKey, interner: &StringInterner) -> Result<String, String> {
    let plugin = interner
        .resolve(form_key.plugin)
        .ok_or_else(|| format!("unresolved plugin for {:06X}", form_key.local))?;
    Ok(format!("{:06X}@{plugin}", form_key.local))
}

fn sort_key(form_key: FormKey, interner: &StringInterner) -> (String, u32) {
    (
        interner
            .resolve(form_key.plugin)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        form_key.local,
    )
}

fn family_name(family: SkyrimRuntimeFamily) -> &'static str {
    match family {
        SkyrimRuntimeFamily::Quest => "quest",
        SkyrimRuntimeFamily::Dialogue => "dialogue",
        SkyrimRuntimeFamily::Scene => "scene",
        SkyrimRuntimeFamily::Actor => "actor",
        SkyrimRuntimeFamily::WorldObject => "world_object",
        SkyrimRuntimeFamily::Appearance => "appearance",
        SkyrimRuntimeFamily::Ai => "ai",
        SkyrimRuntimeFamily::Equipment => "equipment",
        SkyrimRuntimeFamily::Magic => "magic",
        SkyrimRuntimeFamily::Music => "music",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn audited_steel_battleaxe(interner: &StringInterner) -> Record {
        let plugin = interner.intern("Skyrim.esm");
        let mut record = Record::new(
            SigCode::from_str("WEAP").unwrap(),
            FormKey {
                local: 0x013984,
                plugin,
            },
        );
        record.eid = Some(interner.intern("SteelBattleaxe"));
        let mut dnam = [0_u8; 100];
        dnam[0] = 6;
        record.fields = SmallVec::from_vec(vec![
            field(
                "EDID",
                FieldValue::String(interner.intern("SteelBattleaxe")),
            ),
            field("OBND", FieldValue::Bytes(SmallVec::from_slice(&[0; 12]))),
            field(
                "FULL",
                FieldValue::String(interner.intern("Steel Battleaxe")),
            ),
            field(
                "MODL",
                FieldValue::String(interner.intern("Weapons\\Steel\\SteelBattleAxe.nif")),
            ),
            field(
                "ETYP",
                FieldValue::FormKey(FormKey {
                    local: 0x013F45,
                    plugin,
                }),
            ),
            field(
                "WNAM",
                FieldValue::FormKey(FormKey {
                    local: 0x020E27,
                    plugin,
                }),
            ),
            field(
                "DATA",
                FieldValue::Bytes(SmallVec::from_slice(&[100, 0, 0, 0, 0, 0, 168, 65, 18, 0])),
            ),
            field("DNAM", FieldValue::Bytes(SmallVec::from_slice(&dnam))),
        ]);
        record
    }

    #[test]
    fn unsupported_quest_component_does_not_poison_supported_magic_dependency() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fixture.esm");
        let quest_key = FormKey { local: 1, plugin };
        let spell_key = FormKey { local: 2, plugin };
        let mut quest = Record::new(SigCode::from_str("QUST").unwrap(), quest_key);
        quest.fields.push(FieldEntry {
            sig: crate::ids::SubrecordSig(*b"QTGL"),
            value: FieldValue::FormKey(spell_key),
        });
        let spell = Record::new(SigCode::from_str("SPEL").unwrap(), spell_key);
        let plan = plan_components(vec![spell, quest], &[], &interner).unwrap();
        assert!(plan.components.iter().any(|component| {
            component.roots[0].form_key.starts_with("000001@")
                && component.members.len() == 1
                && matches!(component.decision, RuntimeComponentDecision::Unsupported)
        }));
        assert!(plan.components.iter().any(|component| {
            component.roots[0].form_key.starts_with("000002@")
                && component.members.len() == 1
                && matches!(component.decision, RuntimeComponentDecision::Supported)
        }));
        assert!(!plan.by_record[&quest_key].supported);
        assert!(plan.by_record[&spell_key].supported);
        assert_eq!(plan.by_record[&spell_key].component_ids.len(), 1);
    }

    #[test]
    fn quest_owned_package_is_in_the_atomic_blocked_component() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fixture.esm");
        let quest_key = FormKey { local: 1, plugin };
        let package_key = FormKey { local: 2, plugin };
        let mut quest = Record::new(SigCode::from_str("QUST").unwrap(), quest_key);
        quest
            .fields
            .push(field("PKID", FieldValue::FormKey(package_key)));
        let package = Record::new(SigCode::from_str("PACK").unwrap(), package_key);

        let plan = plan_components(vec![quest, package], &[], &interner).unwrap();

        assert!(!plan.by_record[&quest_key].supported);
        assert!(!plan.by_record[&package_key].supported);
        assert_eq!(
            plan.by_record[&quest_key].component_ids,
            plan.by_record[&package_key].component_ids
        );
        assert!(plan.components.iter().any(|component| {
            component.members.len() == 2
                && matches!(component.decision, RuntimeComponentDecision::Unsupported)
                && component
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("target-native component projection"))
        }));
    }

    #[test]
    fn dialogue_view_quest_reference_does_not_create_quest_child_topology() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fixture.esm");
        let quest_key = FormKey { local: 1, plugin };
        let view_key = FormKey { local: 2, plugin };
        let quest = Record::new(SigCode::from_str("QUST").unwrap(), quest_key);
        let mut view = Record::new(SigCode::from_str("DLVW").unwrap(), view_key);
        view.fields
            .push(field("QNAM", FieldValue::FormKey(quest_key)));

        let plan = plan_components(vec![quest, view], &[], &interner).unwrap();

        assert!(!plan.source_topology.iter().any(|edge| {
            edge.child_form_key.starts_with("000002@")
                && edge.kind == RuntimeTopologyKind::QuestChild
        }));
    }

    #[test]
    fn unsupported_component_does_not_poison_supported_actor_dependencies() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let quest_key = FormKey { local: 1, plugin };
        let race_key = FormKey {
            local: 0x013746,
            plugin,
        };
        let npc_key = FormKey {
            local: 0x013290,
            plugin,
        };
        let actor_key = FormKey {
            local: 0x0198D9,
            plugin,
        };

        let mut quest = Record::new(SigCode::from_str("QUST").unwrap(), quest_key);
        quest
            .fields
            .push(field("QTGL", FieldValue::FormKey(npc_key)));
        let mut race = Record::new(SigCode::from_str("RACE").unwrap(), race_key);
        race.eid = Some(interner.intern("NordRace"));
        let mut npc = Record::new(SigCode::from_str("NPC_").unwrap(), npc_key);
        npc.fields
            .push(field("RNAM", FieldValue::FormKey(race_key)));
        let mut actor = Record::new(SigCode::from_str("ACHR").unwrap(), actor_key);
        actor
            .fields
            .push(field("NAME", FieldValue::FormKey(npc_key)));

        let plan = plan_components(vec![quest, race, npc, actor], &[], &interner).unwrap();

        assert!(!plan.by_record[&quest_key].supported);
        assert!(plan.by_record[&race_key].supported);
        assert!(plan.by_record[&npc_key].supported);
        assert!(plan.by_record[&actor_key].supported);
        assert_eq!(plan.by_record[&npc_key].component_ids.len(), 2);
    }

    #[test]
    fn translated_non_weapon_world_object_components_are_supported() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fixture.esm");
        let armor_key = FormKey { local: 1, plugin };
        let armor_addon_key = FormKey { local: 2, plugin };
        let furniture_key = FormKey { local: 3, plugin };
        let mut armor = Record::new(SigCode::from_str("ARMO").unwrap(), armor_key);
        armor.fields.push(FieldEntry {
            sig: SubrecordSig(*b"MODL"),
            value: FieldValue::FormKey(armor_addon_key),
        });
        let armor_addon = Record::new(SigCode::from_str("ARMA").unwrap(), armor_addon_key);
        let furniture = Record::new(SigCode::from_str("FURN").unwrap(), furniture_key);

        let plan = plan_components(vec![armor, armor_addon, furniture], &[], &interner).unwrap();

        for form_key in [armor_key, armor_addon_key, furniture_key] {
            assert!(plan.by_record[&form_key].supported);
        }
    }

    #[test]
    fn armor_race_links_do_not_pull_actor_races_into_the_equipment_component() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fixture.esm");
        let armor_key = FormKey { local: 1, plugin };
        let armor_addon_key = FormKey { local: 2, plugin };
        let race_key = FormKey { local: 3, plugin };

        let mut armor = Record::new(SigCode::from_str("ARMO").unwrap(), armor_key);
        armor.fields.push(FieldEntry {
            sig: SubrecordSig(*b"MODL"),
            value: FieldValue::FormKey(armor_addon_key),
        });
        armor.fields.push(FieldEntry {
            sig: SubrecordSig(*b"RNAM"),
            value: FieldValue::FormKey(race_key),
        });

        let mut armor_addon = Record::new(SigCode::from_str("ARMA").unwrap(), armor_addon_key);
        for sig in [*b"RNAM", *b"MODL"] {
            armor_addon.fields.push(FieldEntry {
                sig: SubrecordSig(sig),
                value: FieldValue::FormKey(race_key),
            });
        }
        let race = Record::new(SigCode::from_str("RACE").unwrap(), race_key);

        let plan = plan_components(vec![armor, armor_addon, race], &[], &interner).unwrap();

        assert!(plan.by_record[&armor_key].supported);
        assert!(plan.by_record[&armor_addon_key].supported);
        assert!(plan.by_record[&race_key].supported);
        let armor_component = plan
            .components
            .iter()
            .find(|component| component.roots[0].form_key.starts_with("000001@"))
            .unwrap();
        assert_eq!(armor_component.members.len(), 2);
    }

    #[test]
    fn audited_humanoid_npc_and_placements_form_supported_actor_components() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let race_key = FormKey {
            local: 0x013746,
            plugin,
        };
        let npc_key = FormKey {
            local: 0x0A2C8E,
            plugin,
        };
        let actor_key = FormKey {
            local: 0x0A2C94,
            plugin,
        };
        let voice_key = FormKey {
            local: 0x013ADD,
            plugin,
        };
        let mut race = Record::new(SigCode::from_str("RACE").unwrap(), race_key);
        race.eid = Some(interner.intern("NordRace"));
        let mut npc = Record::new(SigCode::from_str("NPC_").unwrap(), npc_key);
        npc.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"RNAM"),
                value: FieldValue::FormKey(race_key),
            },
            FieldEntry {
                sig: SubrecordSig(*b"VTCK"),
                value: FieldValue::FormKey(voice_key),
            },
        ]);
        let mut actor = Record::new(SigCode::from_str("ACHR").unwrap(), actor_key);
        actor.fields.push(FieldEntry {
            sig: SubrecordSig(*b"NAME"),
            value: FieldValue::FormKey(npc_key),
        });
        let voice = Record::new(SigCode::from_str("VTYP").unwrap(), voice_key);

        let plan = plan_components(vec![race, npc, actor, voice], &[], &interner).unwrap();

        assert!(plan.by_record[&race_key].supported);
        assert!(plan.by_record[&npc_key].supported);
        assert!(plan.by_record[&actor_key].supported);
        assert!(!plan.by_record[&voice_key].supported);
        assert!(plan.components.iter().any(|component| {
            component.roots[0].form_key.starts_with("0A2C94@")
                && component.members.len() == 3
                && matches!(component.decision, RuntimeComponentDecision::Supported)
        }));
    }

    #[test]
    fn audited_beast_race_npcs_are_supported() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let race_key = FormKey {
            local: 0x013740,
            plugin,
        };
        let npc_key = FormKey {
            local: 0x800,
            plugin,
        };
        let mut race = Record::new(SigCode::from_str("RACE").unwrap(), race_key);
        race.eid = Some(interner.intern("ArgonianRace"));
        let mut npc = Record::new(SigCode::from_str("NPC_").unwrap(), npc_key);
        npc.fields.push(FieldEntry {
            sig: SubrecordSig(*b"RNAM"),
            value: FieldValue::FormKey(race_key),
        });

        let plan = plan_components(vec![race, npc], &[], &interner).unwrap();

        assert!(plan.by_record[&race_key].supported);
        assert!(plan.by_record[&npc_key].supported);
    }

    #[test]
    fn every_resolved_race_npc_and_actor_is_supported_without_an_allowlist() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let race_key = FormKey {
            local: 0x0131F9,
            plugin,
        };
        let npc_key = FormKey {
            local: 0x02B10E,
            plugin,
        };
        let actor_key = FormKey {
            local: 0x07E94D,
            plugin,
        };
        let mut race = Record::new(SigCode::from_str("RACE").unwrap(), race_key);
        race.eid = Some(interner.intern("FalmerRace"));
        let mut npc = Record::new(SigCode::from_str("NPC_").unwrap(), npc_key);
        npc.fields
            .push(field("RNAM", FieldValue::FormKey(race_key)));
        let mut actor = Record::new(SigCode::from_str("ACHR").unwrap(), actor_key);
        actor
            .fields
            .push(field("NAME", FieldValue::FormKey(npc_key)));

        let plan = plan_components(vec![race, npc, actor], &[], &interner).unwrap();

        for form_key in [race_key, npc_key, actor_key] {
            assert!(plan.by_record[&form_key].supported);
        }
    }

    #[test]
    fn unresolved_npc_race_fails_total_actor_preflight() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let npc_key = FormKey { local: 1, plugin };
        let missing_race = FormKey { local: 2, plugin };
        let mut npc = Record::new(SigCode::from_str("NPC_").unwrap(), npc_key);
        npc.fields
            .push(field("RNAM", FieldValue::FormKey(missing_race)));

        let error = plan_components(vec![npc], &[], &interner).unwrap_err();

        assert!(error.contains("no resolvable RACE RNAM dependency"));
    }

    #[test]
    fn unresolved_actor_base_fails_total_actor_preflight() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let actor_key = FormKey { local: 1, plugin };
        let missing_npc = FormKey { local: 2, plugin };
        let mut actor = Record::new(SigCode::from_str("ACHR").unwrap(), actor_key);
        actor
            .fields
            .push(field("NAME", FieldValue::FormKey(missing_npc)));

        let error = plan_components(vec![actor], &[], &interner).unwrap_err();

        assert!(error.contains("no resolvable NPC_ NAME dependency"));
    }

    #[test]
    fn non_melee_weapons_are_routed_without_poisoning_audited_melee() {
        let interner = StringInterner::new();
        let steel = audited_steel_battleaxe(&interner);
        let steel_key = steel.form_key;
        let plugin = steel_key.plugin;

        let enchanted_key = FormKey {
            local: 0x0A56D0,
            plugin,
        };
        let mut enchanted = Record::new(SigCode::from_str("WEAP").unwrap(), enchanted_key);
        enchanted.eid = Some(interner.intern("EnchSteelBattleaxeFire3"));
        enchanted.fields.extend([
            field("CNAM", FieldValue::FormKey(steel_key)),
            field(
                "EITM",
                FieldValue::FormKey(FormKey {
                    local: 0x000800,
                    plugin,
                }),
            ),
        ]);

        let ranged_key = FormKey {
            local: 0x000801,
            plugin,
        };
        let mut ranged = audited_steel_battleaxe(&interner);
        ranged.form_key = ranged_key;
        ranged.eid = Some(interner.intern("FixtureBow"));

        let plan = plan_components(vec![steel, enchanted, ranged], &[], &interner).unwrap();

        assert!(plan.by_record[&steel_key].supported);
        assert!(plan.by_record[&enchanted_key].supported);
        assert!(plan.by_record[&ranged_key].supported);
        assert!(plan.components.iter().any(|component| {
            component.roots[0].form_key.starts_with("013984@")
                && component.members.len() == 1
                && matches!(component.decision, RuntimeComponentDecision::Supported)
        }));
    }

    #[test]
    fn audited_melee_weapon_component_can_include_its_exact_first_person_static() {
        let interner = StringInterner::new();
        let steel = audited_steel_battleaxe(&interner);
        let steel_key = steel.form_key;
        let stat_key = FormKey {
            local: 0x020E27,
            plugin: steel_key.plugin,
        };
        let mut first_person = Record::new(SigCode::from_str("STAT").unwrap(), stat_key);
        first_person.eid = Some(interner.intern("1stPersonSteelBattleaxe"));

        let plan = plan_components(vec![steel, first_person], &[], &interner).unwrap();

        assert!(plan.by_record[&steel_key].supported);
        assert!(plan.by_record[&stat_key].supported);
        assert_eq!(plan.components.len(), 1);
        assert_eq!(plan.components[0].members.len(), 2);
    }

    #[test]
    fn scroll_and_shout_receipts_record_target_signature_adaptations() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let scroll_key = FormKey {
            local: 0x010000,
            plugin,
        };
        let shout_key = FormKey {
            local: 0x010001,
            plugin,
        };
        let scroll = Record::new(SigCode::from_str("SCRL").unwrap(), scroll_key);
        let shout = Record::new(SigCode::from_str("SHOU").unwrap(), shout_key);

        let plan = plan_components(vec![scroll, shout], &[], &interner).unwrap();

        assert_eq!(plan.target_signatures[&scroll_key], "ALCH");
        assert_eq!(plan.target_signatures[&shout_key], "SPEL");
        let adaptations = plan
            .components
            .iter()
            .flat_map(|component| &component.adaptations)
            .collect::<Vec<_>>();
        assert!(adaptations.iter().any(|adaptation| {
            adaptation.source_signature == "SCRL"
                && adaptation.target_signature == "ALCH"
                && adaptation.semantic_role == "scroll-consumable"
        }));
        assert!(adaptations.iter().any(|adaptation| {
            adaptation.source_signature == "SHOU"
                && adaptation.target_signature == "SPEL"
                && adaptation.semantic_role == "shout-spell-fallback"
        }));
    }
}
