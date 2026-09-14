use std::collections::BTreeSet;

use super::*;

fn key(signature: &str, form_key: &str) -> QuestRecordKey {
    QuestRecordKey::new(signature, form_key)
}

fn plan(component_id: &str, root_form_key: &str) -> QuestRuntimeComponentPlan {
    let root_quest = key("QUST", root_form_key);
    let target_quest = key("QUST", "000100:Fallout4.esm");
    let source_placement = TopologyPlacement {
        record: root_quest.clone(),
        group_path: vec!["QUST".to_owned()],
    };
    let target_placement = TopologyPlacement {
        record: target_quest.clone(),
        group_path: vec!["QUST".to_owned()],
    };
    QuestRuntimeComponentPlan {
        component_id: component_id.to_owned(),
        root_quest: root_quest.clone(),
        provenance: SourceProvenance {
            game: QuestSourceGame::Fnv,
            source_plugin: "FalloutNV.esm".to_owned(),
            graft: None,
        },
        owned_records: vec![root_quest.clone()],
        shared_records: BTreeSet::new(),
        dependencies: DependencyClosure::default(),
        source_topology: BTreeSet::from([source_placement]),
        target_topology: BTreeSet::from([target_placement.clone()]),
        mappings: vec![SourceTargetFormMapping {
            source: root_quest,
            target: target_quest.clone(),
        }],
        semantics: QuestSemanticPlan {
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
        expected_receipt: QuestRuntimeExpectedReceipt {
            emitted_records: BTreeSet::from([target_quest]),
            placements: BTreeSet::from([target_placement]),
            ..QuestRuntimeExpectedReceipt::default()
        },
    }
}

fn issue_codes(plan: &QuestRuntimeComponentPlan) -> BTreeSet<PlanValidationCode> {
    plan.validation_issues()
        .into_iter()
        .map(|issue| issue.code)
        .collect()
}

fn add_mapping(
    plan: &mut QuestRuntimeComponentPlan,
    source: QuestRecordKey,
    target_form_key: &str,
) {
    let target = key(&source.signature, target_form_key);
    plan.mappings
        .push(SourceTargetFormMapping { source, target });
}

#[test]
fn supported_fixture_is_phase_zero_complete() {
    assert!(
        plan("complete", "000100:FalloutNV.esm")
            .validation_issues()
            .is_empty()
    );
}

#[test]
fn component_owned_records_have_one_owner() {
    let shared_owned = key("DIAL", "001234:FalloutNV.esm");
    let mut first = plan("first", "000100:FalloutNV.esm");
    first.owned_records.push(shared_owned.clone());
    add_mapping(&mut first, shared_owned.clone(), "001234:Fallout4.esm");
    let mut second = plan("second", "000200:FalloutNV.esm");
    second.owned_records.push(shared_owned.clone());
    add_mapping(&mut second, shared_owned.clone(), "001234:Fallout4.esm");

    let issues = validate_component_plans(&[first, second]);

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].code, PlanValidationCode::DuplicateOwnership);
    assert_eq!(issues[0].record.as_ref(), Some(&shared_owned));
    assert_eq!(issues[0].conflicting_component_id.as_deref(), Some("first"));
}

#[test]
fn shared_dependencies_can_be_used_by_multiple_components() {
    let shared = key("PACK", "004321:FalloutNV.esm");
    let mut first = plan("first", "000100:FalloutNV.esm");
    first.shared_records.insert(shared.clone());
    first.dependencies.direct.insert(shared.clone());
    add_mapping(&mut first, shared.clone(), "004321:Fallout4.esm");
    let mut second = plan("second", "000200:FalloutNV.esm");
    second.shared_records.insert(shared.clone());
    second.dependencies.recursive.insert(shared.clone());
    add_mapping(&mut second, shared, "004321:Fallout4.esm");

    assert!(validate_component_plans(&[first, second]).is_empty());
}

#[test]
fn a_component_requires_exactly_one_terminal_start_disposition() {
    let mut missing = plan("missing", "000100:FalloutNV.esm");
    missing.start_disposition = None;
    assert_eq!(
        missing.validation_issues()[0].code,
        PlanValidationCode::MissingStartDisposition
    );

    let terminal = plan("terminal", "000200:FalloutNV.esm");
    assert!(terminal.validation_issues().is_empty());

    let mut orphan = plan("orphan", "000300:FalloutNV.esm");
    orphan.start_disposition = Some(StartDisposition::OrphanStartUnknown);
    assert_eq!(
        orphan.validation_issues()[0].code,
        PlanValidationCode::SupportedOrphanStart
    );
}

#[test]
fn supported_component_rejects_empty_or_invalid_source_plugin() {
    for invalid in ["", "   ", "FalloutNV", "folder/FalloutNV.esm"] {
        let mut component = plan("invalid_provenance", "000100:FalloutNV.esm");
        component.provenance.source_plugin = invalid.to_owned();
        assert!(
            issue_codes(&component).contains(&PlanValidationCode::InvalidSourcePlugin),
            "expected invalid source plugin issue for {invalid:?}"
        );
    }
}

#[test]
fn supported_component_rejects_incomplete_declared_mapping_and_topology_closure() {
    let mut component = plan("incomplete_closure", "000100:FalloutNV.esm");
    let unmapped_dependency = key("PACK", "000200:FalloutNV.esm");
    component
        .dependencies
        .direct
        .insert(unmapped_dependency.clone());
    let undeclared_source = key("INFO", "000300:FalloutNV.esm");
    component.mappings.push(SourceTargetFormMapping {
        source: undeclared_source,
        target: key("INFO", "000300:Fallout4.esm"),
    });
    component.target_topology.insert(TopologyPlacement {
        record: key("DIAL", "000400:Fallout4.esm"),
        group_path: vec!["DIAL".to_owned()],
    });

    let codes = issue_codes(&component);

    assert!(codes.contains(&PlanValidationCode::UnmappedDeclaredRecord));
    assert!(codes.contains(&PlanValidationCode::UndeclaredRecordReference));
    assert!(codes.contains(&PlanValidationCode::UnmappedTargetTopology));
}

#[test]
fn supported_component_rejects_required_psc_and_vmad_without_fresh_evidence_intent() {
    let mut component = plan("missing_compile_evidence", "000100:FalloutNV.esm");
    let owner = component.root_quest.clone();
    let target_owner = component.mappings[0].target.clone();
    component.scripts.insert(ScriptIntent {
        owner,
        source_name: "SourceQuestScript".to_owned(),
        target_class: "B21_TargetQuestScript".to_owned(),
        required: true,
    });
    component.psc.insert(PscIntent {
        class_name: "B21_TargetQuestScript".to_owned(),
        source_artifact: "Scripts/Source/User/B21_TargetQuestScript.psc".to_owned(),
        compiler_evidence_required: true,
        compiler_evidence: None,
    });
    component.vmad.insert(VmadIntent {
        owner: target_owner,
        script_class: "B21_TargetQuestScript".to_owned(),
        properties: BTreeSet::new(),
        compiler_evidence_required: true,
        compiler_evidence: None,
    });

    let issues = component.validation_issues();

    assert!(issues.iter().any(|issue| {
        issue.code == PlanValidationCode::MissingFreshCompilerEvidence
            && issue.subject.as_deref() == Some("psc:B21_TargetQuestScript")
    }));
    assert!(issues.iter().any(|issue| {
        issue.code == PlanValidationCode::MissingFreshCompilerEvidence
            && issue.subject.as_deref() == Some("vmad:B21_TargetQuestScript")
    }));
}

#[test]
fn supported_non_autostart_requires_matching_proven_inbound_producer() {
    let mut component = plan("missing_producer", "000100:FalloutNV.esm");
    component.semantics.start_flags.clear();
    component.start_disposition = Some(StartDisposition::ExplicitScript {
        producer_id: "start-script".to_owned(),
    });
    component.inbound_producers.insert(StartProducerIntent {
        producer_id: "different-script".to_owned(),
        carrier: component.root_quest.clone(),
        producer_kind: "explicit_script".to_owned(),
        evidence_id: "compile:producer".to_owned(),
        proven: true,
    });
    assert!(issue_codes(&component).contains(&PlanValidationCode::MissingProvenStartProducer));

    component.inbound_producers.insert(StartProducerIntent {
        producer_id: "start-script".to_owned(),
        carrier: component.root_quest.clone(),
        producer_kind: "explicit_script".to_owned(),
        evidence_id: "compile:producer".to_owned(),
        proven: true,
    });
    assert!(component.validation_issues().is_empty());
}

#[test]
fn rejection_reasons_and_validation_issues_serialize_deterministically() {
    let reasons = BTreeSet::from([
        QuestRuntimeRejectionReason::UnsupportedScript,
        QuestRuntimeRejectionReason::IncompleteClosure,
        QuestRuntimeRejectionReason::MissingCompilerEvidence,
    ]);
    let admission = QuestRuntimeAdmission::Rejected { reasons };

    let first = serde_json::to_string(&admission).unwrap();
    let second = serde_json::to_string(&admission).unwrap();

    assert_eq!(first, second);
    assert!(first.find("incomplete_closure").unwrap() < first.find("unsupported_script").unwrap());
}

#[test]
fn frozen_receipt_comparison_surfaces_required_post_fixup_damage() {
    let quest = key("QUST", "000100:Fallout4.esm");
    let placement = TopologyPlacement {
        record: quest.clone(),
        group_path: vec!["QUST".to_owned()],
    };
    let vmad = VmadAttachmentReceipt {
        owner: quest.clone(),
        script_class: "B21_TestQuest".to_owned(),
        property_names: BTreeSet::from(["TargetAlias".to_owned()]),
        compiler_evidence_id: "compile:B21_TestQuest".to_owned(),
    };
    let expected = QuestRuntimeExpectedReceipt {
        emitted_records: BTreeSet::from([quest.clone()]),
        placements: BTreeSet::from([placement]),
        localized_strings: BTreeSet::new(),
        scripts: BTreeSet::new(),
        vmad_attachments: BTreeSet::from([vmad.clone()]),
        routes: BTreeSet::new(),
    };
    let post_fixup = QuestRuntimePostFixupReceipt {
        emitted_records: vec![quest],
        ..QuestRuntimePostFixupReceipt::default()
    };

    let comparison = compare_post_fixup_receipt(&expected, &post_fixup);

    assert!(!comparison.intact);
    assert_eq!(comparison.required_damage.len(), 2);
    assert!(
        comparison
            .required_damage
            .contains(&ReceiptDamage::MissingVmadAttachment(vmad))
    );
}

#[test]
fn strict_receipt_rejects_generic_translation_readdition() {
    let required = key("QUST", "000100:Fallout4.esm");
    let readded = key("INFO", "000200:Fallout4.esm");
    let expected = QuestRuntimeExpectedReceipt {
        emitted_records: BTreeSet::from([required.clone()]),
        ..QuestRuntimeExpectedReceipt::default()
    };
    let actual = QuestRuntimePostFixupReceipt {
        emitted_records: vec![required, readded.clone()],
        ..QuestRuntimePostFixupReceipt::default()
    };

    let comparison = compare_post_fixup_receipt(&expected, &actual);

    assert_eq!(
        comparison.required_damage,
        vec![ReceiptDamage::UnexpectedRecord(readded)]
    );
}

#[test]
fn strict_receipt_rejects_duplicate_required_output() {
    let required = key("QUST", "000100:Fallout4.esm");
    let expected = QuestRuntimeExpectedReceipt {
        emitted_records: BTreeSet::from([required.clone()]),
        ..QuestRuntimeExpectedReceipt::default()
    };
    let actual = QuestRuntimePostFixupReceipt {
        emitted_records: vec![required.clone(), required.clone()],
        ..QuestRuntimePostFixupReceipt::default()
    };

    let comparison = compare_post_fixup_receipt(&expected, &actual);

    assert_eq!(
        comparison.required_damage,
        vec![ReceiptDamage::DuplicateRecord {
            record: required,
            occurrences: 2,
        }]
    );
}

#[test]
fn receipt_allows_only_explicit_optional_or_shared_extras() {
    let required = key("QUST", "000100:Fallout4.esm");
    let shared = key("PACK", "000200:Fallout4.esm");
    let expected = QuestRuntimeExpectedReceipt {
        emitted_records: BTreeSet::from([required.clone()]),
        ..QuestRuntimeExpectedReceipt::default()
    };
    let actual = QuestRuntimePostFixupReceipt {
        emitted_records: vec![required, shared.clone()],
        ..QuestRuntimePostFixupReceipt::default()
    };
    let allowances = QuestRuntimeReceiptAllowances {
        emitted_records: BTreeSet::from([shared]),
        ..QuestRuntimeReceiptAllowances::default()
    };

    assert!(compare_post_fixup_receipt_with_allowances(&expected, &actual, &allowances).intact);
}

#[test]
fn component_contract_cannot_enable_production_emission() {
    let component = plan("contract_only", "000100:FalloutNV.esm");

    assert!(!PRODUCTION_EMISSION_ENABLED);
    assert!(!component.production_emission_enabled());
    let json = serde_json::to_string(&component).unwrap();
    assert!(!json.contains("record_payload"));
    assert!(!json.contains("writer"));
}
