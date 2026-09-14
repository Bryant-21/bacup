use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{QuestRecordKey, TopologyPlacement};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LocalizedStringReceipt {
    pub owner: QuestRecordKey,
    pub field: String,
    pub table: String,
    pub string_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScriptArtifactReceipt {
    pub class_name: String,
    pub psc_path: String,
    pub pex_path: String,
    pub compiler_evidence_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VmadAttachmentReceipt {
    pub owner: QuestRecordKey,
    pub script_class: String,
    pub property_names: BTreeSet<String>,
    pub compiler_evidence_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StartRouteReceipt {
    pub route_id: String,
    pub quest: QuestRecordKey,
    pub producer_evidence_id: String,
    pub node_chain: Vec<QuestRecordKey>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestRuntimeExpectedReceipt {
    pub emitted_records: BTreeSet<QuestRecordKey>,
    pub placements: BTreeSet<TopologyPlacement>,
    pub localized_strings: BTreeSet<LocalizedStringReceipt>,
    pub scripts: BTreeSet<ScriptArtifactReceipt>,
    pub vmad_attachments: BTreeSet<VmadAttachmentReceipt>,
    pub routes: BTreeSet<StartRouteReceipt>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestRuntimePostFixupReceipt {
    pub emitted_records: Vec<QuestRecordKey>,
    pub placements: Vec<TopologyPlacement>,
    pub localized_strings: Vec<LocalizedStringReceipt>,
    pub scripts: Vec<ScriptArtifactReceipt>,
    pub vmad_attachments: Vec<VmadAttachmentReceipt>,
    pub routes: Vec<StartRouteReceipt>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestRuntimeReceiptAllowances {
    pub emitted_records: BTreeSet<QuestRecordKey>,
    pub placements: BTreeSet<TopologyPlacement>,
    pub localized_strings: BTreeSet<LocalizedStringReceipt>,
    pub scripts: BTreeSet<ScriptArtifactReceipt>,
    pub vmad_attachments: BTreeSet<VmadAttachmentReceipt>,
    pub routes: BTreeSet<StartRouteReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "required")]
pub enum ReceiptDamage {
    MissingRecord(QuestRecordKey),
    MissingPlacement(TopologyPlacement),
    MissingLocalizedString(LocalizedStringReceipt),
    MissingScript(ScriptArtifactReceipt),
    MissingVmadAttachment(VmadAttachmentReceipt),
    MissingStartRoute(StartRouteReceipt),
    UnexpectedRecord(QuestRecordKey),
    UnexpectedPlacement(TopologyPlacement),
    UnexpectedLocalizedString(LocalizedStringReceipt),
    UnexpectedScript(ScriptArtifactReceipt),
    UnexpectedVmadAttachment(VmadAttachmentReceipt),
    UnexpectedStartRoute(StartRouteReceipt),
    DuplicateRecord {
        record: QuestRecordKey,
        occurrences: usize,
    },
    DuplicatePlacement {
        placement: TopologyPlacement,
        occurrences: usize,
    },
    DuplicateLocalizedString {
        localized_string: LocalizedStringReceipt,
        occurrences: usize,
    },
    DuplicateScript {
        script: ScriptArtifactReceipt,
        occurrences: usize,
    },
    DuplicateVmadAttachment {
        attachment: VmadAttachmentReceipt,
        occurrences: usize,
    },
    DuplicateStartRoute {
        route: StartRouteReceipt,
        occurrences: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptComparison {
    pub intact: bool,
    pub required_damage: Vec<ReceiptDamage>,
}

pub fn compare_post_fixup_receipt(
    expected: &QuestRuntimeExpectedReceipt,
    post_fixup: &QuestRuntimePostFixupReceipt,
) -> ReceiptComparison {
    compare_post_fixup_receipt_with_allowances(
        expected,
        post_fixup,
        &QuestRuntimeReceiptAllowances::default(),
    )
}

pub fn compare_post_fixup_receipt_with_allowances(
    expected: &QuestRuntimeExpectedReceipt,
    post_fixup: &QuestRuntimePostFixupReceipt,
    allowed: &QuestRuntimeReceiptAllowances,
) -> ReceiptComparison {
    let mut damage = BTreeSet::new();

    compare_occurrences(
        &expected.emitted_records,
        &post_fixup.emitted_records,
        &allowed.emitted_records,
        ReceiptDamage::MissingRecord,
        ReceiptDamage::UnexpectedRecord,
        |record, occurrences| ReceiptDamage::DuplicateRecord {
            record,
            occurrences,
        },
        &mut damage,
    );
    compare_occurrences(
        &expected.placements,
        &post_fixup.placements,
        &allowed.placements,
        ReceiptDamage::MissingPlacement,
        ReceiptDamage::UnexpectedPlacement,
        |placement, occurrences| ReceiptDamage::DuplicatePlacement {
            placement,
            occurrences,
        },
        &mut damage,
    );
    compare_occurrences(
        &expected.localized_strings,
        &post_fixup.localized_strings,
        &allowed.localized_strings,
        ReceiptDamage::MissingLocalizedString,
        ReceiptDamage::UnexpectedLocalizedString,
        |localized_string, occurrences| ReceiptDamage::DuplicateLocalizedString {
            localized_string,
            occurrences,
        },
        &mut damage,
    );
    compare_occurrences(
        &expected.scripts,
        &post_fixup.scripts,
        &allowed.scripts,
        ReceiptDamage::MissingScript,
        ReceiptDamage::UnexpectedScript,
        |script, occurrences| ReceiptDamage::DuplicateScript {
            script,
            occurrences,
        },
        &mut damage,
    );
    compare_occurrences(
        &expected.vmad_attachments,
        &post_fixup.vmad_attachments,
        &allowed.vmad_attachments,
        ReceiptDamage::MissingVmadAttachment,
        ReceiptDamage::UnexpectedVmadAttachment,
        |attachment, occurrences| ReceiptDamage::DuplicateVmadAttachment {
            attachment,
            occurrences,
        },
        &mut damage,
    );
    compare_occurrences(
        &expected.routes,
        &post_fixup.routes,
        &allowed.routes,
        ReceiptDamage::MissingStartRoute,
        ReceiptDamage::UnexpectedStartRoute,
        |route, occurrences| ReceiptDamage::DuplicateStartRoute { route, occurrences },
        &mut damage,
    );

    let required_damage = damage.into_iter().collect::<Vec<_>>();
    ReceiptComparison {
        intact: required_damage.is_empty(),
        required_damage,
    }
}

fn compare_occurrences<T, Missing, Unexpected, Duplicate>(
    expected: &BTreeSet<T>,
    actual: &[T],
    allowed: &BTreeSet<T>,
    missing: Missing,
    unexpected: Unexpected,
    duplicate: Duplicate,
    damage: &mut BTreeSet<ReceiptDamage>,
) where
    T: Clone + Ord,
    Missing: Fn(T) -> ReceiptDamage,
    Unexpected: Fn(T) -> ReceiptDamage,
    Duplicate: Fn(T, usize) -> ReceiptDamage,
{
    let mut occurrences = std::collections::BTreeMap::new();
    for item in actual {
        *occurrences.entry(item.clone()).or_insert(0usize) += 1;
    }
    let actual_unique = occurrences.keys().cloned().collect::<BTreeSet<_>>();
    damage.extend(expected.difference(&actual_unique).cloned().map(missing));
    damage.extend(
        actual_unique
            .difference(expected)
            .filter(|item| !allowed.contains(*item))
            .cloned()
            .map(unexpected),
    );
    damage.extend(
        occurrences
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(item, count)| duplicate(item, count)),
    );
}
