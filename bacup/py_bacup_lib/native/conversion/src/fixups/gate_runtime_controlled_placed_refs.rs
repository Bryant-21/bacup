//! Fixup: gate FO76 runtime-controlled placed refs after cell-copy insertion.
//!
//! FO76 locations and quests can leave event visuals in the plugin while a quest
//! script enables them at runtime. The FO4 conversion does not port those
//! controllers yet, so copied placed refs must remain present but initially
//! disabled.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec};

use esp_authoring_core::plugin_runtime::{ParsedItem, ParsedRecord, WriteEffect};

use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::{HandleRawScan, PluginSession};

const RECORD_FLAG_PERSISTENT: u32 = 0x0000_0400;
const RECORD_FLAG_INITIALLY_DISABLED: u32 = 0x0000_0800;

const PLACED_SIGS: &[&str] = &["REFR", "ACHR", "PGRE", "PHZD", "PGRD"];
const QUEST_RUNTIME_GATE_TOKENS: &[&str] = &[
    "nuke",
    "nuked",
    "blast",
    "storm",
    "distantcloud",
    "weathercloud",
    "76trailer",
    "chargentrailer",
    "mpscripttest",
    "testserver",
];
const RUNTIME_LAYER_TOKENS: &[&str] = &[
    "76trailer",
    "babylon",
    "chargentrailer",
    "mpscripttest",
    "testserver",
    "donotuse",
];
const EDITOR_ONLY_BASE_TOKENS: &[&str] = &["mpscripttest", "testserver", "donotuse"];
const MANUAL_DISABLED_REF_FORM_IDS: &[u32] = &[
    0x0779_B387,
    0x0779_B388,
    0x0743_CF1C,
    0x073C_006D,
    0x0752_04E4,
    0x0762_6788,
];
const MANUAL_ENABLED_REF_FORM_IDS: &[u32] = &[0x0785_AD03];
const ALWAYS_GATED_BASE_EDIDS: &[&str] = &[
    "WorkshopCapturePointBorderCylinderHalf512",
    "WorkshopCapturePointBorderCylinderHalf512Trigger",
    "WorkshopCapturePointBorderCylinder512Trigger",
];
const NUKED_FLORA_BASE_MARKER: &str = "FloraRad";
const CHALKLETTER_BASE_PREFIX: &str = "ChalkLetter_";
const CHALKLETTER_GATED_CELL: (i32, i32) = (-26, 22);
const EXTERIOR_CELL_SIZE: f32 = 4096.0;

#[derive(Debug, Default, PartialEq, Eq)]
struct GateSourceIndex {
    lctns_scanned: u32,
    layers_scanned: u32,
    runtime_layers: u32,
    quests_scanned: u32,
    quest_aliases_scanned: u32,
    placed_refs_scanned: u32,
    explicit_disabled_refs: u32,
    quest_gated_refs: u32,
    layer_gated_refs: u32,
    editor_only_base_gated_refs: u32,
    always_gated_base_refs: u32,
    nuked_flora_base_gated_refs: u32,
    chalkletter_cell_gated_refs: u32,
    manual_disabled_refs: u32,
    manual_enabled_refs: u32,
    manual_enabled_ref_locals: FxHashSet<u32>,
    gated_refs: FxHashSet<u32>,
    special_refs_by_location: FxHashMap<u32, Vec<LctnSpecialRef>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LctnSpecialRef {
    loc_ref_type: u32,
    placed_ref: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AliasKind {
    Reference,
    Location,
    Collection,
}

#[derive(Debug, Default, Clone)]
struct QuestAlias {
    kind: Option<AliasKind>,
    id: u32,
    name: Option<String>,
    location_alias: Option<u32>,
    specific_location: Option<u32>,
    ref_type: Option<u32>,
}

#[derive(Debug, Default)]
struct QuestAliases {
    edid: Option<String>,
    has_vmad: bool,
    aliases: Vec<QuestAlias>,
}

#[derive(Debug, Default)]
struct GateApplyStats {
    changed: u32,
    already_disabled: u32,
    enabled_changed: u32,
    already_enabled: u32,
    skipped_missing: u32,
    skipped_nonplaced: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateEvidence {
    ExplicitInitiallyDisabled,
    QuestRuntimeAlias,
    RuntimeLayer,
    EditorOnlyBase,
    AlwaysGatedBase,
    NukedFloraBase,
    ChalkLetterCell,
}

pub fn gate_runtime_controlled_placed_refs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    _config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let interner = mapper.interner;

    let source_index = {
        let Some(source_id) = session.source_id() else {
            return Ok(report);
        };
        let source_scan = session
            .handle_raw_scan(source_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        GateSourceIndex::from_source_scan(&source_scan)?
    };
    if source_index.gated_refs.is_empty() && source_index.manual_enabled_ref_locals.is_empty() {
        return Ok(report);
    }

    let master_count = session.target_masters().len();
    if master_count > u8::MAX as usize {
        return Ok(report);
    }
    let own_load_index = master_count as u8;
    let own_sym = interner.intern(&session.target_slot().parsed.plugin_name);
    let source_plugin_name = session
        .source_slot_opt()
        .map(|slot| slot.parsed.plugin_name.clone())
        .unwrap_or_default();
    let source_sym = interner.intern(&source_plugin_name);
    let target_masters = session.target_masters().to_vec();
    let mapped_targets =
        mapped_target_raws_by_source_local(mapper, source_sym, target_masters.as_slice());
    let own_placed_ids = own_placed_object_ids(session, interner, own_sym)?;

    let mut stats = GateApplyStats::default();
    let mut changed_form_ids = smallvec![];
    for source_local in &source_index.gated_refs {
        let Some(target_raw) = resolve_target_raw_for_source_local(
            *source_local,
            &mapped_targets,
            &own_placed_ids,
            own_load_index,
        ) else {
            stats.skipped_missing = stats.skipped_missing.saturating_add(1);
            continue;
        };

        let Ok(record) = session.record_mut(target_raw) else {
            stats.skipped_missing = stats.skipped_missing.saturating_add(1);
            continue;
        };
        if !is_gateable_placed_sig(record.signature.as_str()) {
            stats.skipped_nonplaced = stats.skipped_nonplaced.saturating_add(1);
            continue;
        }

        if mark_record_initially_disabled(record) {
            stats.changed = stats.changed.saturating_add(1);
            changed_form_ids.push(target_raw);
        } else {
            stats.already_disabled = stats.already_disabled.saturating_add(1);
        }
    }
    for source_local in &source_index.manual_enabled_ref_locals {
        let Some(target_raw) = resolve_target_raw_for_source_local(
            *source_local,
            &mapped_targets,
            &own_placed_ids,
            own_load_index,
        ) else {
            stats.skipped_missing = stats.skipped_missing.saturating_add(1);
            continue;
        };

        let Ok(record) = session.record_mut(target_raw) else {
            stats.skipped_missing = stats.skipped_missing.saturating_add(1);
            continue;
        };
        if !is_gateable_placed_sig(record.signature.as_str()) {
            stats.skipped_nonplaced = stats.skipped_nonplaced.saturating_add(1);
            continue;
        }

        if clear_record_initially_disabled(record) {
            stats.enabled_changed = stats.enabled_changed.saturating_add(1);
            changed_form_ids.push(target_raw);
        } else {
            stats.already_enabled = stats.already_enabled.saturating_add(1);
        }
    }

    if stats.changed > 0 || stats.enabled_changed > 0 {
        session.target_slot_mut().clear_record_count_cache();
        session.record_effect(WriteEffect::RecordContents {
            form_ids: changed_form_ids,
        });
    }

    report.records_changed = stats.changed.saturating_add(stats.enabled_changed);
    push_summary_warning(&mut report, interner, &source_index, &stats);
    Ok(report)
}

impl GateSourceIndex {
    fn from_source_scan(scan: &HandleRawScan<'_>) -> Result<Self, FixupError> {
        use rayon::prelude::*;

        let mut index = Self::default();
        let source_metadata = SourceMetadata {
            edids: scan.own_editor_ids(),
            layer_locals: scan
                .raw_form_ids_of_sig(
                    SigCode::from_str("LAYR")
                        .map_err(|e| FixupError::SchemaError(e.to_string()))?,
                )
                .into_iter()
                .filter_map(source_own_local)
                .collect(),
        };

        let lctn_ids = scan.raw_form_ids_of_sig(
            SigCode::from_str("LCTN").map_err(|e| FixupError::SchemaError(e.to_string()))?,
        );
        for raw_form_id in lctn_ids {
            let _ = scan.with_record(raw_form_id, |record| {
                index.lctns_scanned = index.lctns_scanned.saturating_add(1);
                collect_lctn_record_gates(record, &mut index);
            });
        }

        let runtime_layers: FxHashSet<u32> = source_metadata
            .edids
            .iter()
            .filter_map(|(local, edid)| {
                (source_metadata.layer_locals.contains(local) && is_runtime_layer_edid(edid))
                    .then_some(*local)
            })
            .collect();
        index.layers_scanned = source_metadata.layer_locals.len() as u32;
        index.runtime_layers = runtime_layers.len() as u32;

        let mut placed_ids = Vec::new();
        for sig in PLACED_SIGS {
            let sig = SigCode::from_str(sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            placed_ids.extend(scan.raw_form_ids_of_sig(sig));
        }
        let placed_evidence: Vec<(u32, SmallVec<[GateEvidence; 3]>)> = placed_ids
            .par_iter()
            .filter_map(|raw_form_id| {
                scan.with_record(*raw_form_id, |record| {
                    (
                        record.form_id,
                        placed_record_gate_evidence(record, &source_metadata, &runtime_layers),
                    )
                })
            })
            .collect();
        for (raw_form_id, evidence) in placed_evidence {
            index.placed_refs_scanned = index.placed_refs_scanned.saturating_add(1);
            for kind in evidence {
                index.insert_gated_ref(raw_form_id, kind);
            }
        }

        let quest_ids = scan.raw_form_ids_of_sig(
            SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?,
        );
        let quests: Vec<QuestAliases> = quest_ids
            .par_iter()
            .filter_map(|raw_form_id| scan.with_record(*raw_form_id, parse_quest_aliases))
            .collect();
        for quest in quests {
            collect_parsed_quest_gates(&quest, &mut index);
        }

        index.apply_manual_overrides();
        Ok(index)
    }

    fn from_source_items(items: &[ParsedItem]) -> Self {
        let mut index = Self::default();
        collect_lctn_gates(items, &mut index);
        let source_metadata = collect_source_metadata(items);
        collect_placed_metadata_gates(items, &source_metadata, &mut index);
        collect_quest_gates(items, &mut index);
        index.apply_manual_overrides();
        index
    }

    fn insert_gated_ref(&mut self, raw: u32, evidence: GateEvidence) {
        let Some(local) = source_own_local(raw) else {
            return;
        };
        if self.gated_refs.insert(local) {
            match evidence {
                GateEvidence::ExplicitInitiallyDisabled => {
                    self.explicit_disabled_refs = self.explicit_disabled_refs.saturating_add(1);
                }
                GateEvidence::QuestRuntimeAlias => {
                    self.quest_gated_refs = self.quest_gated_refs.saturating_add(1);
                }
                GateEvidence::RuntimeLayer => {
                    self.layer_gated_refs = self.layer_gated_refs.saturating_add(1);
                }
                GateEvidence::EditorOnlyBase => {
                    self.editor_only_base_gated_refs =
                        self.editor_only_base_gated_refs.saturating_add(1);
                }
                GateEvidence::AlwaysGatedBase => {
                    self.always_gated_base_refs = self.always_gated_base_refs.saturating_add(1);
                }
                GateEvidence::NukedFloraBase => {
                    self.nuked_flora_base_gated_refs =
                        self.nuked_flora_base_gated_refs.saturating_add(1);
                }
                GateEvidence::ChalkLetterCell => {
                    self.chalkletter_cell_gated_refs =
                        self.chalkletter_cell_gated_refs.saturating_add(1);
                }
            }
        }
    }

    fn apply_manual_overrides(&mut self) {
        for form_id in MANUAL_DISABLED_REF_FORM_IDS {
            if self.gated_refs.insert(form_id & 0x00FF_FFFF) {
                self.manual_disabled_refs = self.manual_disabled_refs.saturating_add(1);
            }
        }
        for form_id in MANUAL_ENABLED_REF_FORM_IDS {
            let local = form_id & 0x00FF_FFFF;
            if self.manual_enabled_ref_locals.insert(local) {
                self.manual_enabled_refs = self.manual_enabled_refs.saturating_add(1);
            }
            self.gated_refs.remove(&local);
        }
    }
}

fn collect_lctn_gates(items: &[ParsedItem], index: &mut GateSourceIndex) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "LCTN" => {
                index.lctns_scanned = index.lctns_scanned.saturating_add(1);
                collect_lctn_record_gates(record, index);
            }
            ParsedItem::Group(group) => collect_lctn_gates(&group.children, index),
            _ => {}
        }
    }
}

fn collect_lctn_record_gates(record: &ParsedRecord, index: &mut GateSourceIndex) {
    let Some(location_local) = source_own_local(record.form_id) else {
        return;
    };
    for subrecord in &record.subrecords {
        match subrecord.signature.as_str() {
            "ACID" | "LCID" => {
                for raw in subrecord.data.chunks_exact(4).map(read_u32_at_zero) {
                    index.insert_gated_ref(raw, GateEvidence::ExplicitInitiallyDisabled);
                }
            }
            "ACSR" | "LCSR" => {
                for row in subrecord.data.chunks_exact(16) {
                    let Some(loc_ref_type) = source_own_local(read_u32(row, 0)) else {
                        continue;
                    };
                    let Some(placed_ref) = source_own_local(read_u32(row, 4)) else {
                        continue;
                    };
                    index
                        .special_refs_by_location
                        .entry(location_local)
                        .or_default()
                        .push(LctnSpecialRef {
                            loc_ref_type,
                            placed_ref,
                        });
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Default)]
struct SourceMetadata {
    edids: FxHashMap<u32, String>,
    layer_locals: FxHashSet<u32>,
}

fn collect_source_metadata(items: &[ParsedItem]) -> SourceMetadata {
    let mut metadata = SourceMetadata::default();
    collect_source_metadata_in_items(items, &mut metadata);
    metadata
}

fn collect_source_metadata_in_items(items: &[ParsedItem], metadata: &mut SourceMetadata) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                if let Some(local) = source_own_local(record.form_id) {
                    if record.signature.as_str() == "LAYR" {
                        metadata.layer_locals.insert(local);
                    }
                    if let Some(edid) = raw_record_edid(record) {
                        metadata.edids.insert(local, edid);
                    }
                }
            }
            ParsedItem::Group(group) => collect_source_metadata_in_items(&group.children, metadata),
        }
    }
}

fn collect_placed_metadata_gates(
    items: &[ParsedItem],
    source_metadata: &SourceMetadata,
    index: &mut GateSourceIndex,
) {
    let runtime_layers: FxHashSet<u32> = source_metadata
        .edids
        .iter()
        .filter_map(|(local, edid)| {
            if source_metadata.layer_locals.contains(local) && is_runtime_layer_edid(edid) {
                Some(*local)
            } else {
                None
            }
        })
        .collect();
    index.layers_scanned = source_metadata.layer_locals.len() as u32;
    index.runtime_layers = runtime_layers.len() as u32;
    collect_placed_metadata_gates_in_items(items, source_metadata, &runtime_layers, index);
}

fn collect_placed_metadata_gates_in_items(
    items: &[ParsedItem],
    source_metadata: &SourceMetadata,
    runtime_layers: &FxHashSet<u32>,
    index: &mut GateSourceIndex,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) if is_gateable_placed_sig(record.signature.as_str()) => {
                index.placed_refs_scanned = index.placed_refs_scanned.saturating_add(1);
                collect_placed_record_metadata_gates(
                    record,
                    source_metadata,
                    runtime_layers,
                    index,
                );
            }
            ParsedItem::Group(group) => {
                collect_placed_metadata_gates_in_items(
                    &group.children,
                    source_metadata,
                    runtime_layers,
                    index,
                );
            }
            _ => {}
        }
    }
}

fn collect_placed_record_metadata_gates(
    record: &ParsedRecord,
    source_metadata: &SourceMetadata,
    runtime_layers: &FxHashSet<u32>,
    index: &mut GateSourceIndex,
) {
    for evidence in placed_record_gate_evidence(record, source_metadata, runtime_layers) {
        index.insert_gated_ref(record.form_id, evidence);
    }
}

fn placed_record_gate_evidence(
    record: &ParsedRecord,
    source_metadata: &SourceMetadata,
    runtime_layers: &FxHashSet<u32>,
) -> SmallVec<[GateEvidence; 3]> {
    let mut evidence = SmallVec::new();
    for subrecord in &record.subrecords {
        match subrecord.signature.as_str() {
            "XLYR" => {
                let Some(layer_local) = source_own_local(read_u32_at_zero(&subrecord.data)) else {
                    continue;
                };
                if runtime_layers.contains(&layer_local) {
                    evidence.push(GateEvidence::RuntimeLayer);
                }
            }
            "NAME" => {
                let Some(base_local) = source_own_local(read_u32_at_zero(&subrecord.data)) else {
                    continue;
                };
                if source_metadata
                    .edids
                    .get(&base_local)
                    .is_some_and(|edid| is_editor_only_base_edid(edid))
                {
                    evidence.push(GateEvidence::EditorOnlyBase);
                }
                if source_metadata
                    .edids
                    .get(&base_local)
                    .is_some_and(|edid| is_always_gated_base_edid(edid))
                {
                    evidence.push(GateEvidence::AlwaysGatedBase);
                }
                if source_metadata
                    .edids
                    .get(&base_local)
                    .is_some_and(|edid| is_nuked_flora_base_edid(edid))
                {
                    evidence.push(GateEvidence::NukedFloraBase);
                }
                if source_metadata
                    .edids
                    .get(&base_local)
                    .is_some_and(|edid| is_chalkletter_base_edid(edid))
                    && placed_record_grid(record) == Some(CHALKLETTER_GATED_CELL)
                {
                    evidence.push(GateEvidence::ChalkLetterCell);
                }
            }
            _ => {}
        }
    }
    evidence
}

fn collect_quest_gates(items: &[ParsedItem], index: &mut GateSourceIndex) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "QUST" => {
                collect_quest_record_gates(record, index);
            }
            ParsedItem::Group(group) => collect_quest_gates(&group.children, index),
            _ => {}
        }
    }
}

fn collect_quest_record_gates(record: &ParsedRecord, index: &mut GateSourceIndex) {
    let quest = parse_quest_aliases(record);
    collect_parsed_quest_gates(&quest, index);
}

fn collect_parsed_quest_gates(quest: &QuestAliases, index: &mut GateSourceIndex) {
    index.quests_scanned = index.quests_scanned.saturating_add(1);
    index.quest_aliases_scanned = index
        .quest_aliases_scanned
        .saturating_add(quest.aliases.len() as u32);
    if !quest.has_vmad {
        return;
    }

    let quest_has_runtime_token = quest
        .edid
        .as_deref()
        .is_some_and(contains_quest_runtime_gate_token);
    let locations_by_alias: FxHashMap<u32, u32> = quest
        .aliases
        .iter()
        .filter(|alias| alias.kind == Some(AliasKind::Location))
        .filter_map(|alias| Some((alias.id, source_own_local(alias.specific_location?)?)))
        .collect();

    for alias in quest
        .aliases
        .iter()
        .filter(|alias| alias.kind == Some(AliasKind::Reference))
    {
        if !quest_has_runtime_token
            && !alias
                .name
                .as_deref()
                .is_some_and(contains_quest_runtime_gate_token)
        {
            continue;
        }
        let Some(location_alias) = alias.location_alias else {
            continue;
        };
        let Some(location_local) = locations_by_alias.get(&location_alias).copied() else {
            continue;
        };
        let Some(ref_type_local) = alias.ref_type.and_then(source_own_local) else {
            continue;
        };
        let Some(special_refs) = index.special_refs_by_location.get(&location_local) else {
            continue;
        };
        let matched: Vec<u32> = special_refs
            .iter()
            .filter(|special| special.loc_ref_type == ref_type_local)
            .map(|special| special.placed_ref)
            .collect();
        for placed_ref in matched {
            index.insert_gated_ref(placed_ref, GateEvidence::QuestRuntimeAlias);
        }
    }
}

fn parse_quest_aliases(record: &ParsedRecord) -> QuestAliases {
    let mut quest = QuestAliases::default();
    let mut current: Option<QuestAlias> = None;

    for subrecord in &record.subrecords {
        match subrecord.signature.as_str() {
            "EDID" => quest.edid = decode_zstring(&subrecord.data),
            "VMAD" => quest.has_vmad = true,
            "ALST" | "ALLS" | "ALCS" => {
                if let Some(alias) = current.take() {
                    quest.aliases.push(alias);
                }
                current = Some(QuestAlias {
                    kind: alias_kind(subrecord.signature.as_str()),
                    id: read_u32(subrecord.data.as_ref(), 0),
                    ..QuestAlias::default()
                });
            }
            "ALID" => {
                if let Some(alias) = current.as_mut() {
                    alias.name = decode_zstring(&subrecord.data);
                }
            }
            "ALFA" => {
                if let Some(alias) = current.as_mut() {
                    alias.location_alias = Some(read_u32(subrecord.data.as_ref(), 0));
                }
            }
            "ALFL" => {
                if let Some(alias) = current.as_mut() {
                    alias.specific_location = Some(read_u32(subrecord.data.as_ref(), 0));
                }
            }
            "ALRT" => {
                if let Some(alias) = current.as_mut() {
                    alias.ref_type = Some(read_u32(subrecord.data.as_ref(), 0));
                }
            }
            _ => {}
        }
    }

    if let Some(alias) = current {
        quest.aliases.push(alias);
    }
    quest
}

fn alias_kind(sig: &str) -> Option<AliasKind> {
    match sig {
        "ALST" => Some(AliasKind::Reference),
        "ALLS" => Some(AliasKind::Location),
        "ALCS" => Some(AliasKind::Collection),
        _ => None,
    }
}

fn mapped_target_raws_by_source_local(
    mapper: &FormKeyMapper,
    source_plugin: crate::sym::Sym,
    target_masters: &[String],
) -> FxHashMap<u32, u32> {
    mapper
        .source_to_target_iter()
        .filter(|(source, _)| source.plugin == source_plugin)
        .filter_map(|(source, target)| {
            encode_target_form_id(target, mapper.interner, target_masters)
                .map(|raw| (source.local & 0x00FF_FFFF, raw))
        })
        .collect()
}

fn own_placed_object_ids(
    session: &mut PluginSession,
    interner: &crate::sym::StringInterner,
    own_sym: crate::sym::Sym,
) -> Result<FxHashSet<u32>, FixupError> {
    let mut ids = FxHashSet::default();
    for sig in PLACED_SIGS {
        let sig_code =
            SigCode::from_str(sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
        for fk in session
            .form_keys_of_sig(sig_code, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            if fk.plugin == own_sym {
                ids.insert(fk.local & 0x00FF_FFFF);
            }
        }
    }
    Ok(ids)
}

fn resolve_target_raw_for_source_local(
    source_local: u32,
    mapped_targets: &FxHashMap<u32, u32>,
    own_placed_ids: &FxHashSet<u32>,
    own_load_index: u8,
) -> Option<u32> {
    if let Some(mapped) = mapped_targets.get(&source_local).copied() {
        if (mapped >> 24) == own_load_index as u32 {
            return Some(mapped);
        }
        return None;
    }
    own_placed_ids
        .contains(&source_local)
        .then_some(((own_load_index as u32) << 24) | source_local)
}

fn mark_record_initially_disabled(record: &mut ParsedRecord) -> bool {
    if record.flags & RECORD_FLAG_INITIALLY_DISABLED != 0 {
        return false;
    }
    record.flags |= RECORD_FLAG_INITIALLY_DISABLED;
    true
}

fn clear_record_initially_disabled(record: &mut ParsedRecord) -> bool {
    if record.flags & RECORD_FLAG_INITIALLY_DISABLED == 0 {
        return false;
    }
    record.flags &= !RECORD_FLAG_INITIALLY_DISABLED;
    true
}

fn is_gateable_placed_sig(sig: &str) -> bool {
    PLACED_SIGS.contains(&sig)
}

fn source_own_local(raw: u32) -> Option<u32> {
    if raw == 0 || raw >> 24 != 0 {
        return None;
    }
    Some(raw & 0x00FF_FFFF)
}

fn contains_quest_runtime_gate_token(text: &str) -> bool {
    contains_any_runtime_token(text, QUEST_RUNTIME_GATE_TOKENS)
}

fn is_runtime_layer_edid(text: &str) -> bool {
    contains_any_runtime_token(text, RUNTIME_LAYER_TOKENS)
}

fn is_editor_only_base_edid(text: &str) -> bool {
    contains_any_runtime_token(text, EDITOR_ONLY_BASE_TOKENS)
}

fn is_always_gated_base_edid(text: &str) -> bool {
    ALWAYS_GATED_BASE_EDIDS
        .iter()
        .any(|edid| text.eq_ignore_ascii_case(edid))
}

fn is_nuked_flora_base_edid(text: &str) -> bool {
    text.as_bytes()
        .windows(NUKED_FLORA_BASE_MARKER.len())
        .any(|window| window.eq_ignore_ascii_case(NUKED_FLORA_BASE_MARKER.as_bytes()))
}

fn is_chalkletter_base_edid(text: &str) -> bool {
    text.starts_with(CHALKLETTER_BASE_PREFIX)
}

fn contains_any_runtime_token(text: &str, tokens: &[&str]) -> bool {
    let normalized = text.to_ascii_lowercase();
    tokens.iter().any(|token| normalized.contains(token))
}

fn placed_record_grid(record: &ParsedRecord) -> Option<(i32, i32)> {
    let data = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "DATA")?
        .data
        .as_ref();
    let x = read_f32(data, 0)?;
    let y = read_f32(data, 4)?;
    Some(position_to_grid(x, y))
}

fn position_to_grid(x: f32, y: f32) -> (i32, i32) {
    (
        (x / EXTERIOR_CELL_SIZE).floor() as i32,
        (y / EXTERIOR_CELL_SIZE).floor() as i32,
    )
}

fn read_u32_at_zero(bytes: &[u8]) -> u32 {
    read_u32(bytes, 0)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    bytes
        .get(offset..offset + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(0)
}

fn read_f32(bytes: &[u8], offset: usize) -> Option<f32> {
    bytes
        .get(offset..offset + 4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn decode_zstring(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    if end == 0 {
        return None;
    }
    std::str::from_utf8(&bytes[..end]).ok().map(str::to_owned)
}

fn raw_record_edid(record: &ParsedRecord) -> Option<String> {
    record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "EDID")
        .and_then(|subrecord| decode_zstring(&subrecord.data))
}

fn push_summary_warning(
    report: &mut FixupReport,
    interner: &crate::sym::StringInterner,
    source_index: &GateSourceIndex,
    stats: &GateApplyStats,
) {
    if source_index.gated_refs.is_empty()
        && stats.changed == 0
        && stats.skipped_missing == 0
        && stats.skipped_nonplaced == 0
    {
        return;
    }
    let message = format!(
        "gate_runtime_controlled_placed_refs:lctns={} layers={} runtime_layers={} quests={} quest_aliases={} placed_refs={} candidates={} explicit={} quest_gated={} layer_gated={} editor_only_base_gated={} always_gated_base={} nuked_flora_base_gated={} chalkletter_cell_gated={} manual_disabled={} manual_enabled={} changed={} already_disabled={} enabled_changed={} already_enabled={} skipped_missing={} skipped_nonplaced={}",
        source_index.lctns_scanned,
        source_index.layers_scanned,
        source_index.runtime_layers,
        source_index.quests_scanned,
        source_index.quest_aliases_scanned,
        source_index.placed_refs_scanned,
        source_index.gated_refs.len(),
        source_index.explicit_disabled_refs,
        source_index.quest_gated_refs,
        source_index.layer_gated_refs,
        source_index.editor_only_base_gated_refs,
        source_index.always_gated_base_refs,
        source_index.nuked_flora_base_gated_refs,
        source_index.chalkletter_cell_gated_refs,
        source_index.manual_disabled_refs,
        source_index.manual_enabled_refs,
        stats.changed,
        stats.already_disabled,
        stats.enabled_changed,
        stats.already_enabled,
        stats.skipped_missing,
        stats.skipped_nonplaced,
    );
    report.warnings.push(interner.intern(&message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, ParsedSubrecord, plugin_handle_new_native, plugin_handle_store_ref,
    };

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: sig.into(),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn record(
        sig: &str,
        form_id: u32,
        flags: u32,
        subrecords: Vec<ParsedSubrecord>,
    ) -> ParsedRecord {
        ParsedRecord {
            signature: sig.into(),
            form_id,
            flags,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn item(record: ParsedRecord) -> ParsedItem {
        ParsedItem::Record(record)
    }

    fn group(children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label: *b"GRUP",
            group_type: 0,
            tail: Bytes::new(),
            children,
        })
    }

    fn z(text: &str) -> Vec<u8> {
        let mut out = text.as_bytes().to_vec();
        out.push(0);
        out
    }

    fn formid_array(ids: &[u32]) -> Vec<u8> {
        ids.iter().flat_map(|id| id.to_le_bytes()).collect()
    }

    #[test]
    fn indexed_source_scan_matches_tree_walk() {
        let layer = record(
            "LAYR",
            0x000100,
            0,
            vec![sub("EDID", z("TestServerRuntimeLayer"))],
        );
        let placed = record(
            "REFR",
            0x000200,
            0,
            vec![sub("XLYR", 0x000100u32.to_le_bytes().to_vec())],
        );
        let items = vec![group(vec![item(layer), item(placed)])];
        let expected = GateSourceIndex::from_source_items(&items);
        let handle = plugin_handle_new_native("GateScan.esm", Some("fo76")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store.get_mut(&handle).unwrap().parsed.root_items = items;
        }
        let mut session = crate::session::open_session(handle, None).unwrap();
        let scan = session.handle_raw_scan(handle).unwrap();
        let actual = GateSourceIndex::from_source_scan(&scan).unwrap();

        assert_eq!(actual, expected);
    }

    fn placed_data(x: f32, y: f32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
        out.extend_from_slice(&0.0_f32.to_le_bytes());
        out.extend_from_slice(&0.0_f32.to_le_bytes());
        out.extend_from_slice(&0.0_f32.to_le_bytes());
        out.extend_from_slice(&0.0_f32.to_le_bytes());
        out
    }

    fn lcsr_row(loc_ref_type: u32, placed_ref: u32, world_cell: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&loc_ref_type.to_le_bytes());
        out.extend_from_slice(&placed_ref.to_le_bytes());
        out.extend_from_slice(&world_cell.to_le_bytes());
        out.extend_from_slice(&(-18_i16).to_le_bytes());
        out.extend_from_slice(&(-37_i16).to_le_bytes());
        out
    }

    #[test]
    fn explicit_master_initially_disabled_refs_are_gated() {
        let lctn = record(
            "LCTN",
            0x000A0078,
            0,
            vec![sub("LCID", formid_array(&[0x0085647F]))],
        );

        let index = GateSourceIndex::from_source_items(&[item(lctn)]);

        assert!(index.gated_refs.contains(&0x85647F));
        assert_eq!(index.explicit_disabled_refs, 1);
        assert_eq!(index.quest_gated_refs, 0);
    }

    #[test]
    fn manual_disabled_refs_are_gated() {
        let index = GateSourceIndex::from_source_items(&[]);

        assert!(index.gated_refs.contains(&0x79B387));
        assert!(index.gated_refs.contains(&0x79B388));
        assert!(index.gated_refs.contains(&0x43CF1C));
        assert!(index.gated_refs.contains(&0x3C006D));
        assert!(index.gated_refs.contains(&0x5204E4));
        assert!(index.gated_refs.contains(&0x626788));
        assert!(index.manual_enabled_ref_locals.contains(&0x85AD03));
        assert_eq!(index.manual_disabled_refs, 6);
        assert_eq!(index.manual_enabled_refs, 1);
    }

    #[test]
    fn manual_enabled_refs_override_source_gates() {
        let lctn = record(
            "LCTN",
            0x000A0078,
            0,
            vec![sub("LCID", formid_array(&[0x0085AD03]))],
        );

        let index = GateSourceIndex::from_source_items(&[item(lctn)]);

        assert!(!index.gated_refs.contains(&0x85AD03));
        assert!(index.manual_enabled_ref_locals.contains(&0x85AD03));
        assert_eq!(index.manual_enabled_refs, 1);
    }

    #[test]
    fn scripted_nuke_quest_alias_gates_matching_lctn_special_ref() {
        let lctn = record(
            "LCTN",
            0x000A0078,
            0,
            vec![sub("LCSR", lcsr_row(0x00856473, 0x0085647F, 0x0025DA15))],
        );
        let qust = record(
            "QUST",
            0x002D0F69,
            0,
            vec![
                sub("EDID", z("EN07_MQ_FleeBlast")),
                sub("VMAD", vec![1, 2, 3]),
                sub("ALLS", 4_u32.to_le_bytes().to_vec()),
                sub("ALID", z("IncomingNukeLoc")),
                sub("ALFL", 0x000A0078_u32.to_le_bytes().to_vec()),
                sub("ALST", 51_u32.to_le_bytes().to_vec()),
                sub("ALID", z("BurningSpringsDistantCloud")),
                sub("ALFA", 4_u32.to_le_bytes().to_vec()),
                sub("ALRT", 0x00856473_u32.to_le_bytes().to_vec()),
            ],
        );

        let index = GateSourceIndex::from_source_items(&[group(vec![item(lctn), item(qust)])]);

        assert!(index.gated_refs.contains(&0x85647F));
        assert_eq!(index.explicit_disabled_refs, 0);
        assert_eq!(index.quest_gated_refs, 1);
    }

    #[test]
    fn unrelated_special_refs_stay_enabled_without_runtime_token() {
        let lctn = record(
            "LCTN",
            0x000A0078,
            0,
            vec![sub("LCSR", lcsr_row(0x00856473, 0x0085647F, 0x0025DA15))],
        );
        let qust = record(
            "QUST",
            0x002D0F69,
            0,
            vec![
                sub("EDID", z("WorkshopQuest")),
                sub("VMAD", vec![1, 2, 3]),
                sub("ALLS", 4_u32.to_le_bytes().to_vec()),
                sub("ALID", z("WorkshopLoc")),
                sub("ALFL", 0x000A0078_u32.to_le_bytes().to_vec()),
                sub("ALST", 51_u32.to_le_bytes().to_vec()),
                sub("ALID", z("BossMarker")),
                sub("ALFA", 4_u32.to_le_bytes().to_vec()),
                sub("ALRT", 0x00856473_u32.to_le_bytes().to_vec()),
            ],
        );

        let index = GateSourceIndex::from_source_items(&[item(lctn), item(qust)]);

        assert!(!index.gated_refs.contains(&0x85647F));
        assert_eq!(index.quest_gated_refs, 0);
    }

    #[test]
    fn placed_ref_on_runtime_trailer_layer_is_gated() {
        let layer = record(
            "LAYR",
            0x004DFF93,
            0,
            vec![sub("EDID", z("76Trailer_CharGenTrailer"))],
        );
        let placed = record(
            "REFR",
            0x00856480,
            0,
            vec![sub("XLYR", 0x004DFF93_u32.to_le_bytes().to_vec())],
        );

        let index = GateSourceIndex::from_source_items(&[item(layer), item(placed)]);

        assert!(index.gated_refs.contains(&0x856480));
        assert_eq!(index.layers_scanned, 1);
        assert_eq!(index.runtime_layers, 1);
        assert_eq!(index.layer_gated_refs, 1);
    }

    #[test]
    fn placed_ref_on_babylon_layer_is_gated() {
        let layer = record("LAYR", 0x004DFF94, 0, vec![sub("EDID", z("Babylon"))]);
        let placed = record(
            "REFR",
            0x00856480,
            0,
            vec![sub("XLYR", 0x004DFF94_u32.to_le_bytes().to_vec())],
        );

        let index = GateSourceIndex::from_source_items(&[item(layer), item(placed)]);

        assert!(index.gated_refs.contains(&0x856480));
        assert_eq!(index.layers_scanned, 1);
        assert_eq!(index.runtime_layers, 1);
        assert_eq!(index.layer_gated_refs, 1);
    }

    #[test]
    fn ordinary_world_trailer_layer_stays_enabled() {
        let layer = record(
            "LAYR",
            0x00369D11,
            0,
            vec![sub("EDID", z("Huntersville_Trailer_Clutter"))],
        );
        let placed = record(
            "REFR",
            0x00856480,
            0,
            vec![sub("XLYR", 0x00369D11_u32.to_le_bytes().to_vec())],
        );

        let index = GateSourceIndex::from_source_items(&[item(layer), item(placed)]);

        assert!(!index.gated_refs.contains(&0x856480));
        assert_eq!(index.layers_scanned, 1);
        assert_eq!(index.runtime_layers, 0);
        assert_eq!(index.layer_gated_refs, 0);
    }

    #[test]
    fn placed_ref_with_test_server_base_is_gated() {
        let base = record(
            "ACTI",
            0x0010D467,
            0,
            vec![sub("EDID", z("test_MPScriptTestServertEventKeywordButton"))],
        );
        let placed = record(
            "REFR",
            0x00856481,
            0,
            vec![sub("NAME", 0x0010D467_u32.to_le_bytes().to_vec())],
        );

        let index = GateSourceIndex::from_source_items(&[item(base), item(placed)]);

        assert!(index.gated_refs.contains(&0x856481));
        assert_eq!(index.editor_only_base_gated_refs, 1);
    }

    #[test]
    fn placed_ref_with_nuked_flora_base_is_gated() {
        let nuked_base = record(
            "FLOR",
            0x00525646,
            0,
            vec![sub("EDID", z("UseLPI_FloraRadThistle01"))],
        );
        let ordinary_base = record(
            "FLOR",
            0x003411CB,
            0,
            vec![sub("EDID", z("UseLPI_FloraThistle01"))],
        );
        let nuked_placed = record(
            "REFR",
            0x00311776,
            0,
            vec![sub("NAME", 0x00525646_u32.to_le_bytes().to_vec())],
        );
        let ordinary_placed = record(
            "REFR",
            0x00311777,
            0,
            vec![sub("NAME", 0x003411CB_u32.to_le_bytes().to_vec())],
        );

        let handle = plugin_handle_new_native("NukedFloraGate.esm", Some("fo76")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store.get_mut(&handle).unwrap().parsed.root_items = vec![
                item(nuked_base),
                item(ordinary_base),
                item(nuked_placed),
                item(ordinary_placed),
            ];
        }
        let mut session = crate::session::open_session(handle, None).unwrap();
        let scan = session.handle_raw_scan(handle).unwrap();
        let index = GateSourceIndex::from_source_scan(&scan).unwrap();

        assert!(index.gated_refs.contains(&0x311776));
        assert!(!index.gated_refs.contains(&0x311777));
        assert_eq!(index.nuked_flora_base_gated_refs, 1);
    }

    #[test]
    fn placed_refs_with_always_gated_bases_are_gated_in_any_cell() {
        let cases: [(u32, &str, u32); 3] = [
            (
                0x0038_1E2D,
                "WorkshopCapturePointBorderCylinderHalf512",
                0x003A_1AEC,
            ),
            (
                0x003A_673B,
                "WorkshopCapturePointBorderCylinderHalf512Trigger",
                0x003A_1AEE,
            ),
            (
                0x003A_6880,
                "WorkshopCapturePointBorderCylinder512Trigger",
                0x003A_1AF0,
            ),
        ];
        let mut items = Vec::new();
        for (base_form_id, edid, placed_form_id) in cases {
            items.push(item(record(
                "STAT",
                base_form_id,
                0,
                vec![sub("EDID", z(edid))],
            )));
            items.push(item(record(
                "REFR",
                placed_form_id,
                0,
                vec![
                    sub("NAME", base_form_id.to_le_bytes().to_vec()),
                    sub("DATA", placed_data(0.0, 0.0)),
                ],
            )));
        }

        let index = GateSourceIndex::from_source_items(&items);

        for (_, _, placed_form_id) in cases {
            assert!(index.gated_refs.contains(&placed_form_id));
        }
        assert_eq!(index.always_gated_base_refs, 3);
    }

    #[test]
    fn chalkletter_refs_in_configured_cell_are_gated() {
        let base = record(
            "STAT",
            0x003A24DB,
            0,
            vec![sub("EDID", z("ChalkLetter_Drawing01"))],
        );
        let placed = record(
            "REFR",
            0x0042BEEF,
            0,
            vec![
                sub("NAME", 0x003A24DB_u32.to_le_bytes().to_vec()),
                sub("DATA", placed_data(-103_725.0, 94_159.0)),
            ],
        );

        let index = GateSourceIndex::from_source_items(&[item(base), item(placed)]);

        assert!(index.gated_refs.contains(&0x42BEEF));
        assert_eq!(index.chalkletter_cell_gated_refs, 1);
    }

    #[test]
    fn chalkletter_refs_outside_configured_cell_stay_enabled() {
        let base = record(
            "STAT",
            0x003A24DB,
            0,
            vec![sub("EDID", z("ChalkLetter_Drawing01"))],
        );
        let placed = record(
            "REFR",
            0x0042BEEF,
            0,
            vec![
                sub("NAME", 0x003A24DB_u32.to_le_bytes().to_vec()),
                sub("DATA", placed_data(-99_000.0, 94_159.0)),
            ],
        );

        let index = GateSourceIndex::from_source_items(&[item(base), item(placed)]);

        assert!(!index.gated_refs.contains(&0x42BEEF));
        assert_eq!(index.chalkletter_cell_gated_refs, 0);
    }

    #[test]
    fn atx_chalkletterkit_refs_do_not_match_chalkletter_prefix_rule() {
        let base = record(
            "STAT",
            0x006653D4,
            0,
            vec![sub("EDID", z("ATX_ChalkLetterKit_Math_0"))],
        );
        let placed = record(
            "REFR",
            0x0042BEEF,
            0,
            vec![
                sub("NAME", 0x006653D4_u32.to_le_bytes().to_vec()),
                sub("DATA", placed_data(-103_725.0, 94_159.0)),
            ],
        );

        let index = GateSourceIndex::from_source_items(&[item(base), item(placed)]);

        assert!(!index.gated_refs.contains(&0x42BEEF));
        assert_eq!(index.chalkletter_cell_gated_refs, 0);
    }

    #[test]
    fn marking_temporary_target_record_is_idempotent_without_making_it_persistent() {
        let mut placed = record("REFR", 0x0785647F, 0, Vec::new());

        assert!(mark_record_initially_disabled(&mut placed));
        assert_eq!(placed.flags, RECORD_FLAG_INITIALLY_DISABLED);
        assert!(!mark_record_initially_disabled(&mut placed));
        assert_eq!(placed.flags, RECORD_FLAG_INITIALLY_DISABLED);
    }

    #[test]
    fn marking_persistent_target_record_preserves_its_persistence() {
        let mut placed = record("REFR", 0x0785647F, RECORD_FLAG_PERSISTENT, Vec::new());

        assert!(mark_record_initially_disabled(&mut placed));
        assert_eq!(
            placed.flags,
            RECORD_FLAG_PERSISTENT | RECORD_FLAG_INITIALLY_DISABLED
        );
    }

    #[test]
    fn manual_enabled_ref_clears_initially_disabled_flag_only() {
        let mut placed = record(
            "REFR",
            0x0785AD03,
            RECORD_FLAG_PERSISTENT | RECORD_FLAG_INITIALLY_DISABLED,
            Vec::new(),
        );

        assert!(clear_record_initially_disabled(&mut placed));
        assert_eq!(placed.flags, RECORD_FLAG_PERSISTENT);
        assert!(!clear_record_initially_disabled(&mut placed));
        assert_eq!(placed.flags, RECORD_FLAG_PERSISTENT);
    }

    #[test]
    fn target_resolution_uses_mapping_then_own_fallback() {
        let mut mapped = FxHashMap::default();
        mapped.insert(0x85647F, 0x07012345);
        let mut own = FxHashSet::default();
        own.insert(0x85647F);
        own.insert(0x856480);

        assert_eq!(
            resolve_target_raw_for_source_local(0x85647F, &mapped, &own, 7),
            Some(0x07012345)
        );
        assert_eq!(
            resolve_target_raw_for_source_local(0x856480, &mapped, &own, 7),
            Some(0x07856480)
        );
        assert_eq!(
            resolve_target_raw_for_source_local(0x856481, &mapped, &own, 7),
            None
        );
    }

    #[test]
    fn target_resolution_skips_master_mappings() {
        let mut mapped = FxHashMap::default();
        mapped.insert(0x85647F, 0x00012345);
        let own = FxHashSet::default();

        assert_eq!(
            resolve_target_raw_for_source_local(0x85647F, &mapped, &own, 7),
            None
        );
    }
}
