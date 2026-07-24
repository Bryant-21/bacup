//! FO76->FO4 Story Manager subset carry.
//!
//! The global translation map still skips SMBN/SMEN/SMQN. This phase restores a
//! narrow, classified subset for radio and dialogue quest startup paths.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::run::{ConversionRun, RunError, TranslateStats};
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fo76_fo4::{
    qust_eid_is_dialogue_conversation, qust_has_untranslatable_event_alias,
};

const SCPT_EVENT_TYPE: u32 = 0x5450_4353;
pub(crate) const FO4_SCRIPT_EVENT_ROOT_LOCAL: u32 = 0x029152;
const QUST_FLAG_START_GAME_ENABLED: u16 = 0x0001;
const QUST_FLAG_HAS_DIALOGUE_DATA: u16 = 0x8000;
const QUST_FLAG_UNIQUE_INSTANCE: u64 = 0x0001_0000;
const QUST_STAGE_FLAG_RUN_ON_START: u16 = 0x0002;

const FO4_SM_EVENT_ROOTS: [(u32, u32); 10] = [
    (fourcc(b"ADIA"), 0x021E65),
    (fourcc(b"CLOC"), 0x0238DB),
    (fourcc(b"HACK"), 0x1244D0),
    (fourcc(b"KILL"), 0x034A37),
    (fourcc(b"LCLD"), 0x05FD8C),
    (fourcc(b"LEVL"), 0x16556F),
    (fourcc(b"LOCK"), 0x0A51B5),
    (fourcc(b"REMP"), 0x07956B),
    (SCPT_EVENT_TYPE, FO4_SCRIPT_EVENT_ROOT_LOCAL),
    (fourcc(b"TMEE"), 0x02A68B),
];

const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_le_bytes([code[0], code[1], code[2], code[3]])
}

/// Story Manager event types (`SMEN.ENAM`) that exist in FO4. Their source roots
/// map to the corresponding FO4 master records. Other event roots are lowered to
/// keyword-gated branches beneath FO4's Script Event root.
const FO4_VALID_SM_EVENT_TYPES: [u32; 10] = [
    fourcc(b"ADIA"),
    fourcc(b"CLOC"),
    fourcc(b"HACK"),
    fourcc(b"KILL"),
    fourcc(b"LCLD"),
    fourcc(b"LEVL"),
    fourcc(b"LOCK"),
    fourcc(b"REMP"),
    fourcc(b"SCPT"),
    fourcc(b"TMEE"),
];

pub struct EmitStoryManagerSubsetPhase;

impl Phase for EmitStoryManagerSubsetPhase {
    fn name(&self) -> &'static str {
        "emit_story_manager_subset"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let started = std::time::Instant::now();
        let stats = ctx
            .run
            .emit_story_manager_subset()
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "story_manager: selected={} skipped={} added={} quests_changed={}",
                stats.selected_nodes,
                stats.skipped_nodes,
                stats.translate.records_translated,
                stats.quests_changed
            ),
        });
        Ok(PhaseReport {
            records_changed: stats.quests_changed,
            records_added: stats.translate.records_translated,
            records_vanilla_remapped: stats.translate.records_vanilla_remapped,
            records_dropped: stats.skipped_nodes,
            records_deferred: stats.translate.records_deferred,
            assets_written: 0,
            warnings: stats.translate.records_failed,
            elapsed_ms: started.elapsed().as_millis() as u64,
            items_failed: 0,
        })
    }
}

#[derive(Default, Debug, Clone)]
pub(crate) struct StoryManagerEmitStats {
    pub translate: TranslateStats,
    pub quests_changed: u32,
    pub selected_nodes: u32,
    pub skipped_nodes: u32,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct StoryManagerSourceGraph {
    pub nodes: FxHashMap<FormKey, Record>,
    pub quests: FxHashMap<FormKey, Record>,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct StoryManagerSelection {
    pub ordered_nodes: Vec<FormKey>,
    pub selected_nodes: FxHashSet<FormKey>,
    pub selected_quests_by_node: FxHashMap<FormKey, FxHashSet<FormKey>>,
    pub selected_event_roots_by_quest: FxHashMap<FormKey, FxHashSet<FormKey>>,
    pub fallback_dialogue_quests: Vec<FormKey>,
    pub diagnostics: Vec<StoryManagerDiagnostic>,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct StoryManagerQuestEventPlan {
    pub rewrites: FxHashMap<FormKey, u32>,
    pub unresolved: Vec<(FormKey, Vec<u32>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoryManagerDiagnostic {
    pub form_key: FormKey,
    pub kind: StoryManagerDiagnosticKind,
    pub reason: Option<StoryManagerSkipReason>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoryManagerDiagnosticKind {
    Selected,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoryManagerSkipReason {
    MissingQuest,
    QuestNotTranslated,
    MissingQuestRecord,
    UnsupportedQuest,
    MissingParent,
    UnsupportedParent,
    ParentCycle,
    MissingEventRoot,
}

impl StoryManagerSkipReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MissingQuest => "missing_quest",
            Self::QuestNotTranslated => "quest_not_translated",
            Self::MissingQuestRecord => "missing_quest_record",
            Self::UnsupportedQuest => "unsupported_quest",
            Self::MissingParent => "missing_parent",
            Self::UnsupportedParent => "unsupported_parent",
            Self::ParentCycle => "parent_cycle",
            Self::MissingEventRoot => "missing_event_root",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoryQuestKind {
    Radio,
    Dialogue,
    Generic,
}

pub(crate) fn load_source_graph(
    run: &mut ConversionRun,
) -> Result<StoryManagerSourceGraph, RunError> {
    let mut graph = StoryManagerSourceGraph::default();
    for sig in ["SMEN", "SMBN", "SMQN"] {
        let sig_code = SigCode::from_str(sig)
            .map_err(|e| RunError::InvalidConfig(format!("{sig} signature: {e}")))?;
        let fks = iter_form_keys_of_sig(run.source_handle_id, sig_code, &run.interner)?;
        for fk in fks {
            match read_record_relayout_by_form_key(
                run.source_handle_id,
                &fk,
                &run.schema_source,
                &run.interner,
                None,
            ) {
                Ok(record) => {
                    graph.nodes.insert(fk, record);
                }
                Err(e) => {
                    let warning = run
                        .interner
                        .intern(&format!("story_manager_read:{sig}:{:06X}:{e}", fk.local));
                    run.warnings.push(warning);
                }
            }
        }
    }

    let quest_sig = SigCode::from_str("QUST")
        .map_err(|e| RunError::InvalidConfig(format!("QUST signature: {e}")))?;
    let mut quest_fks = FxHashSet::default();
    for record in graph
        .nodes
        .values()
        .filter(|record| record.sig == smqn_sig())
    {
        for quest in story_manager_quests(record) {
            quest_fks.insert(quest);
        }
    }
    let mut quest_fks: Vec<_> = quest_fks.into_iter().collect();
    quest_fks.sort_by_key(|fk| fk.local);
    for fk in quest_fks {
        match read_record_relayout_by_form_key(
            run.source_handle_id,
            &fk,
            &run.schema_source,
            &run.interner,
            None,
        ) {
            Ok(record) if record.sig == quest_sig => {
                graph.quests.insert(fk, record);
            }
            Ok(_) => {}
            Err(e) => {
                let warning = run
                    .interner
                    .intern(&format!("story_manager_quest_read:{:06X}:{e}", fk.local));
                run.warnings.push(warning);
            }
        }
    }

    Ok(graph)
}

pub(crate) fn npc_referenced_quest_local_ids(
    source_handle_id: u64,
) -> Result<FxHashSet<u32>, RunError> {
    use esp_authoring_core::plugin_runtime::{
        ensure_core_section, ensure_refs_section, plugin_handle_store_ref,
    };

    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|e| RunError::InvalidConfig(format!("plugin handle store poisoned: {e}")))?;
    let slot = store.get_mut(&source_handle_id).ok_or_else(|| {
        RunError::InvalidConfig(format!("unknown plugin handle {source_handle_id}"))
    })?;
    let core = ensure_core_section(slot);
    let refs = ensure_refs_section(slot);
    let mut quests = FxHashSet::default();
    for (target, incoming) in &refs.reverse_refs_by_form_key {
        let Some(target_entry) = core.by_form_key.get(target) else {
            continue;
        };
        if !target_entry.signature.eq_ignore_ascii_case("QUST") {
            continue;
        }
        if incoming.iter().any(|source| {
            core.by_form_key
                .get(source)
                .is_some_and(|entry| entry.signature.eq_ignore_ascii_case("NPC_"))
        }) {
            quests.insert(target.object_id);
        }
    }
    Ok(quests)
}

pub(crate) fn classify_story_manager_records(
    graph: &StoryManagerSourceGraph,
    translated_quests: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> StoryManagerSelection {
    let mut selection = StoryManagerSelection::default();
    let mut smqns: Vec<_> = graph
        .nodes
        .iter()
        .filter(|(_, record)| record.sig == smqn_sig())
        .collect();
    smqns.sort_by_key(|(fk, _)| fk.local);

    for (smqn_fk, smqn) in smqns {
        let quest_fks = story_manager_quests(smqn);
        if quest_fks.is_empty() {
            push_skip(
                &mut selection,
                *smqn_fk,
                StoryManagerSkipReason::MissingQuest,
                "SMQN has no NNAM quest",
            );
            continue;
        }
        let mut selected_quests = Vec::new();
        let mut first_rejection = None;
        for quest_fk in quest_fks {
            let rejected = if !translated_quests.contains(&quest_fk) {
                Some((
                    StoryManagerSkipReason::QuestNotTranslated,
                    format!("quest {:06X} is not translated", quest_fk.local),
                ))
            } else if let Some(quest) = graph.quests.get(&quest_fk) {
                match classify_quest(quest, interner) {
                    Some(kind) => {
                        selected_quests.push((quest_fk, kind));
                        None
                    }
                    None => Some((
                        StoryManagerSkipReason::UnsupportedQuest,
                        format!("quest {:06X} is not runtime safe", quest_fk.local),
                    )),
                }
            } else {
                Some((
                    StoryManagerSkipReason::MissingQuestRecord,
                    format!("quest {:06X} could not be read", quest_fk.local),
                ))
            };
            if first_rejection.is_none() {
                first_rejection = rejected;
            }
        }
        if selected_quests.is_empty() {
            let (reason, message) = first_rejection.unwrap_or((
                StoryManagerSkipReason::MissingQuest,
                "SMQN has no usable NNAM quest".to_string(),
            ));
            push_skip(&mut selection, *smqn_fk, reason, message);
            continue;
        }
        let chain = match safe_parent_chain(smqn, graph) {
            Ok(chain) => chain,
            Err(reason) => {
                push_skip(
                    &mut selection,
                    *smqn_fk,
                    reason,
                    format!("unsafe parent chain: {}", reason.as_str()),
                );
                continue;
            }
        };
        let root_uses_fo4_event = chain
            .last()
            .and_then(|root_fk| graph.nodes.get(root_fk))
            .and_then(story_manager_event_type)
            .is_some_and(is_fo4_valid_sm_event);
        let event_root = *chain
            .last()
            .expect("safe Story Manager parent chain ends at an event root");

        for fk in chain.iter().rev().copied().chain(std::iter::once(*smqn_fk)) {
            if selection.selected_nodes.insert(fk) {
                selection.ordered_nodes.push(fk);
            }
        }
        let mut selected_ids = Vec::with_capacity(selected_quests.len());
        let allowed = selection
            .selected_quests_by_node
            .entry(*smqn_fk)
            .or_default();
        for (quest_fk, quest_kind) in selected_quests {
            allowed.insert(quest_fk);
            selection
                .selected_event_roots_by_quest
                .entry(quest_fk)
                .or_default()
                .insert(event_root);
            selected_ids.push(format!("{:06X}:{quest_kind:?}", quest_fk.local));
            let is_unique = graph
                .quests
                .get(&quest_fk)
                .is_some_and(|quest| qust_is_unique_instance(quest, interner));
            if quest_kind == StoryQuestKind::Dialogue
                && !root_uses_fo4_event
                && !is_unique
                && !selection.fallback_dialogue_quests.contains(&quest_fk)
            {
                selection.fallback_dialogue_quests.push(quest_fk);
            }
        }
        selection.diagnostics.push(StoryManagerDiagnostic {
            form_key: *smqn_fk,
            kind: StoryManagerDiagnosticKind::Selected,
            reason: None,
            message: format!(
                "selected SMQN {:06X} quests={}",
                smqn_fk.local,
                selected_ids.join(",")
            ),
        });
    }

    selection
}

pub(crate) fn sanitize_story_manager_previous_node(
    record: &mut Record,
    selected_nodes: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    let mut changed = false;
    for entry in &mut record.fields {
        if entry.sig != snam_sig() {
            continue;
        }
        if let Some(previous) = first_form_key(&entry.value, record.form_key.plugin)
            && previous.local != 0
            && !selected_nodes.contains(&previous)
        {
            entry.value = FieldValue::Bytes(SmallVec::from_slice(&0u32.to_le_bytes()));
            let warning = interner.intern("story_manager_previous_node_nulled");
            record.warnings.push(warning);
            changed = true;
        }
    }
    changed
}

pub(crate) fn force_qust_autostart(record: &mut Record, interner: &StringInterner) -> bool {
    for entry in &mut record.fields {
        if entry.sig.0 != *b"DNAM" {
            continue;
        }
        return force_qust_autostart_value(&mut entry.value, interner);
    }
    false
}

fn force_qust_autostart_value(value: &mut FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            let mut flags = u16::from_le_bytes([bytes[0], bytes[1]]);
            if flags & QUST_FLAG_HAS_DIALOGUE_DATA == 0 {
                return false;
            }
            let old = flags;
            flags |= QUST_FLAG_START_GAME_ENABLED;
            bytes[0..2].copy_from_slice(&flags.to_le_bytes());
            old != flags
        }
        FieldValue::Struct(fields) => {
            let Some((_, flags)) = fields
                .iter_mut()
                .find(|(name, _)| field_name_is(interner, *name, "flags"))
            else {
                return false;
            };
            let Some(mut raw) = field_value_u16(flags) else {
                return false;
            };
            if raw & QUST_FLAG_HAS_DIALOGUE_DATA == 0 {
                return false;
            }
            let old = raw;
            raw |= QUST_FLAG_START_GAME_ENABLED;
            write_u16_value(flags, raw);
            old != raw
        }
        _ => false,
    }
}

fn push_skip(
    selection: &mut StoryManagerSelection,
    form_key: FormKey,
    reason: StoryManagerSkipReason,
    message: impl Into<String>,
) {
    selection.diagnostics.push(StoryManagerDiagnostic {
        form_key,
        kind: StoryManagerDiagnosticKind::Skipped,
        reason: Some(reason),
        message: message.into(),
    });
}

fn safe_parent_chain(
    child: &Record,
    graph: &StoryManagerSourceGraph,
) -> Result<Vec<FormKey>, StoryManagerSkipReason> {
    let mut chain = Vec::new();
    let Some(mut current) = story_manager_parent(child) else {
        return Err(StoryManagerSkipReason::MissingParent);
    };
    let mut seen = FxHashSet::default();
    let mut found_event_root = false;

    while current.local != 0 {
        if !seen.insert(current) {
            return Err(StoryManagerSkipReason::ParentCycle);
        }
        let Some(record) = graph.nodes.get(&current) else {
            return Err(StoryManagerSkipReason::MissingParent);
        };
        if record.sig == smen_sig() {
            // Its PNAM may point to an inherited base-game root that is
            // intentionally outside the source graph.
            found_event_root = true;
            chain.push(current);
            break;
        }
        if record.sig == smbn_sig() {
            chain.push(current);
            if let Some(parent) = story_manager_parent(record) {
                current = parent;
                continue;
            }
            break;
        }
        return Err(StoryManagerSkipReason::UnsupportedParent);
    }

    if found_event_root {
        Ok(chain)
    } else {
        Err(StoryManagerSkipReason::MissingEventRoot)
    }
}

fn classify_quest(record: &Record, interner: &StringInterner) -> Option<StoryQuestKind> {
    // Never make a source event path runnable after translation discarded the
    // alias data it needs to start. Proven player aliases are rewritten safely.
    if qust_eid_is_test_or_dev(record, interner) || qust_has_untranslatable_event_alias(record) {
        return None;
    }
    if qust_eid_is_radio(record, interner) {
        return Some(StoryQuestKind::Radio);
    }
    if qust_has_dialogue_data(record, interner)
        && qust_eid_is_dialogue_conversation(interner, record)
    {
        return Some(StoryQuestKind::Dialogue);
    }
    // Generic gameplay quests remain in the Story Manager graph for later
    // events, but are never added to the force-autostart list.
    Some(StoryQuestKind::Generic)
}

fn qust_eid_is_test_or_dev(record: &Record, interner: &StringInterner) -> bool {
    quest_editor_id_lower(record, interner).is_some_and(|eid| {
        eid.starts_with("test") || eid.starts_with("zz") || eid.starts_with("zzz")
    })
}

fn qust_eid_is_radio(record: &Record, interner: &StringInterner) -> bool {
    quest_editor_id_lower(record, interner).is_some_and(|eid| eid.contains("radio"))
}

fn qust_has_dialogue_data(record: &Record, interner: &StringInterner) -> bool {
    quest_flags(record, interner)
        .is_some_and(|flags| flags & u64::from(QUST_FLAG_HAS_DIALOGUE_DATA) != 0)
}

fn qust_is_unique_instance(record: &Record, interner: &StringInterner) -> bool {
    quest_flags(record, interner).is_some_and(|flags| flags & QUST_FLAG_UNIQUE_INSTANCE != 0)
}

pub(crate) fn is_passive_dialogue_controller(
    record: &Record,
    has_incoming_npc_reference: bool,
    interner: &StringInterner,
) -> bool {
    let Some(flags) = quest_flags(record, interner) else {
        return false;
    };
    record.sig == SigCode::from_str("QUST").expect("literal sig")
        && flags & u64::from(QUST_FLAG_START_GAME_ENABLED) != 0
        && flags & u64::from(QUST_FLAG_HAS_DIALOGUE_DATA) != 0
        && flags & QUST_FLAG_UNIQUE_INSTANCE != 0
        && !record.fields.iter().any(|entry| entry.sig.0 == *b"ENAM")
        && record.fields.iter().any(|entry| entry.sig.0 == *b"VMAD")
        && record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"INDX")
            .any(|entry| stage_runs_on_start(&entry.value, interner))
        && has_incoming_npc_reference
}

fn stage_runs_on_start(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u16::from_le_bytes([bytes[2], bytes[3]]) & QUST_STAGE_FLAG_RUN_ON_START != 0
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| field_name_is(interner, *name, "flags"))
            .and_then(|(_, value)| field_value_u16(value))
            .is_some_and(|flags| flags & QUST_STAGE_FLAG_RUN_ON_START != 0),
        _ => false,
    }
}

fn quest_editor_id_lower(record: &Record, interner: &StringInterner) -> Option<String> {
    if let Some(eid) = record.eid.and_then(|sym| interner.resolve(sym)) {
        return Some(eid.to_ascii_lowercase());
    }
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"EDID")
        .and_then(|entry| match &entry.value {
            FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_ascii_lowercase()),
            FieldValue::Bytes(bytes) => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                std::str::from_utf8(&bytes[..end])
                    .ok()
                    .map(|s| s.to_ascii_lowercase())
            }
            _ => None,
        })
}

fn quest_flags(record: &Record, interner: &StringInterner) -> Option<u64> {
    let entry = record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"DATA" || entry.sig.0 == *b"DNAM")?;
    match &entry.value {
        FieldValue::Bytes(bytes) if entry.sig.0 == *b"DATA" && bytes.len() >= 20 => {
            Some(u64::from_le_bytes(bytes[0..8].try_into().ok()?))
        }
        FieldValue::Bytes(bytes) if entry.sig.0 == *b"DATA" && bytes.len() >= 16 => {
            Some(u64::from(u32::from_le_bytes(bytes[0..4].try_into().ok()?)))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u64::from(u16::from_le_bytes([bytes[0], bytes[1]])))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| field_name_is(interner, *name, "flags"))
            .and_then(|(_, value)| field_value_u64(value)),
        value => field_value_u64(value),
    }
}

fn story_manager_parent(record: &Record) -> Option<FormKey> {
    story_manager_fk(record, pnam_sig())
}

fn story_manager_quests(record: &Record) -> Vec<FormKey> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == nnam_sig())
        .filter_map(|entry| first_form_key(&entry.value, record.form_key.plugin))
        .filter(|fk| fk.local != 0)
        .collect()
}

pub(crate) fn retain_story_manager_quests(
    record: &mut Record,
    allowed_quests: &FxHashSet<FormKey>,
) {
    let fallback_plugin = record.form_key.plugin;
    record.fields.retain(|entry| {
        entry.sig != nnam_sig()
            || first_form_key(&entry.value, fallback_plugin)
                .is_some_and(|quest| allowed_quests.contains(&quest))
    });
}

fn story_manager_fk(record: &Record, sig: SubrecordSig) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig == sig)
        .and_then(|entry| first_form_key(&entry.value, record.form_key.plugin))
        .filter(|fk| fk.local != 0)
}

fn story_manager_event_type(record: &Record) -> Option<u32> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig == enam_sig())
        .and_then(|entry| field_value_u32(&entry.value))
}

fn is_fo4_valid_sm_event(event_type: u32) -> bool {
    FO4_VALID_SM_EVENT_TYPES.contains(&event_type)
}

pub(crate) fn fo4_story_manager_event_root(record: &Record) -> Option<u32> {
    if record.sig != smen_sig() {
        return None;
    }
    let source_event = story_manager_event_type(record)?;
    FO4_SM_EVENT_ROOTS
        .iter()
        .find_map(|(event_type, local)| (*event_type == source_event).then_some(*local))
}

pub(crate) fn incompatible_story_manager_event_roots(
    selected_nodes: &FxHashSet<FormKey>,
    graph: &StoryManagerSourceGraph,
) -> Vec<(FormKey, u32)> {
    let mut roots = selected_nodes
        .iter()
        .filter_map(|form_key| {
            let record = graph.nodes.get(form_key)?;
            let event_type = story_manager_event_type(record)?;
            (record.sig == smen_sig() && !is_fo4_valid_sm_event(event_type))
                .then_some((*form_key, event_type))
        })
        .collect::<Vec<_>>();
    roots.sort_by_key(|(form_key, _)| form_key.local);
    roots
}

pub(crate) fn plan_story_manager_quest_events(
    selection: &StoryManagerSelection,
    graph: &StoryManagerSourceGraph,
    event_bridges: &FxHashMap<FormKey, u32>,
    emitted_nodes: &FxHashSet<FormKey>,
) -> StoryManagerQuestEventPlan {
    let mut plan = StoryManagerQuestEventPlan::default();
    let mut quests = selection
        .selected_event_roots_by_quest
        .iter()
        .collect::<Vec<_>>();
    quests.sort_by_key(|(quest, _)| quest.local);

    for (quest, roots) in quests {
        let Some(source_event) = graph.quests.get(quest).and_then(story_manager_event_type) else {
            continue;
        };
        if is_fo4_valid_sm_event(source_event) {
            continue;
        }

        let mut complete = true;
        let mut final_events = FxHashSet::default();
        for root in roots {
            if !emitted_nodes.contains(root) {
                complete = false;
                continue;
            }
            if event_bridges.contains_key(root) {
                final_events.insert(SCPT_EVENT_TYPE);
                continue;
            }
            let Some(event_type) = graph
                .nodes
                .get(root)
                .and_then(story_manager_event_type)
                .filter(|event_type| is_fo4_valid_sm_event(*event_type))
            else {
                complete = false;
                continue;
            };
            final_events.insert(event_type);
        }

        if complete && final_events.len() == 1 && final_events.contains(&SCPT_EVENT_TYPE) {
            plan.rewrites.insert(*quest, SCPT_EVENT_TYPE);
        } else {
            let mut final_events = final_events.into_iter().collect::<Vec<_>>();
            final_events.sort_unstable();
            plan.unresolved.push((*quest, final_events));
        }
    }

    plan
}

pub(crate) fn set_qust_event_type(record: &mut Record, event_type: u32) -> bool {
    if record.sig != SigCode::from_str("QUST").expect("literal sig") {
        return false;
    }
    if let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig == enam_sig())
    {
        if field_value_u32(&entry.value) == Some(event_type) {
            return false;
        }
        write_u32_value(&mut entry.value, event_type);
        return true;
    }

    let insert_at = record
        .fields
        .iter()
        .rposition(|entry| entry.sig.0 == *b"DNAM")
        .map_or(record.fields.len(), |index| index + 1);
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: enam_sig(),
            value: FieldValue::Uint(u64::from(event_type)),
        },
    );
    true
}

pub(crate) fn lower_incompatible_event_root(
    record: &mut Record,
    script_event_root: FormKey,
    keyword_raw: u32,
) -> bool {
    if record.sig != smen_sig()
        || story_manager_event_type(record).is_none_or(is_fo4_valid_sm_event)
    {
        return false;
    }

    record.sig = smbn_sig();
    record
        .fields
        .retain(|entry| !matches!(&entry.sig.0, b"PNAM" | b"SNAM" | b"CITC" | b"ENAM"));

    let parent = FieldEntry {
        sig: pnam_sig(),
        value: FieldValue::FormKey(script_event_root),
    };
    let condition_count = record
        .fields
        .iter()
        .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
        .count()
        .saturating_add(1) as u32;
    let count = FieldEntry {
        sig: SubrecordSig(*b"CITC"),
        value: FieldValue::Uint(u64::from(condition_count)),
    };
    let gate = FieldEntry {
        sig: SubrecordSig(*b"CTDA"),
        value: FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
            keyword_raw,
        ))),
    };

    let body_start = record
        .fields
        .iter()
        .position(|entry| entry.sig.0 != *b"EDID")
        .unwrap_or(record.fields.len());
    record.fields.insert(body_start, parent);
    record.fields.insert(body_start + 1, count);
    record.fields.insert(body_start + 2, gate);
    true
}

fn script_event_keyword_condition(keyword_raw: u32) -> [u8; 32] {
    let mut condition = [
        0x00, 0x04, 0xD5, 0xDC, 0x00, 0x00, 0x80, 0x3F, 0x40, 0x02, 0x11, 0xDB, 0x00, 0x00, 0x4B,
        0x31, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0xFF,
    ];
    condition[16..20].copy_from_slice(&keyword_raw.to_le_bytes());
    condition
}

fn first_form_key(value: &FieldValue, fallback_plugin: crate::sym::Sym) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Uint(raw) => u32::try_from(*raw)
            .ok()
            .map(|raw| raw_form_key(raw, fallback_plugin)),
        FieldValue::Int(raw) => u32::try_from(*raw)
            .ok()
            .map(|raw| raw_form_key(raw, fallback_plugin)),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => Some(raw_form_key(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            fallback_plugin,
        )),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| first_form_key(value, fallback_plugin)),
        FieldValue::List(values) => values
            .iter()
            .find_map(|value| first_form_key(value, fallback_plugin)),
        _ => None,
    }
}

fn raw_form_key(raw: u32, fallback_plugin: crate::sym::Sym) -> FormKey {
    FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: fallback_plugin,
    }
}

fn field_value_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u32::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u32)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| field_value_u32(value)),
        _ => None,
    }
}

fn field_value_u64(value: &FieldValue) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) => u64::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u64::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u64)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
            Some(u64::from_le_bytes(bytes[0..8].try_into().ok()?))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u64::from(u32::from_le_bytes(bytes[0..4].try_into().ok()?)))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u64::from(u16::from_le_bytes([bytes[0], bytes[1]])))
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| field_value_u64(value)),
        _ => None,
    }
}

fn field_value_u16(value: &FieldValue) -> Option<u16> {
    match value {
        FieldValue::Uint(value) => u16::try_from(*value).ok(),
        FieldValue::Int(value) => u16::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u16::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u16)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes([bytes[0], bytes[1]]))
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| field_value_u16(value)),
        _ => None,
    }
}

fn write_u16_value(value: &mut FieldValue, new_value: u16) {
    match value {
        FieldValue::Uint(value) => *value = u64::from(new_value),
        FieldValue::Int(value) => *value = i64::from(new_value),
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            bytes[0..2].copy_from_slice(&new_value.to_le_bytes());
        }
        other => *other = FieldValue::Uint(u64::from(new_value)),
    }
}

fn write_u32_value(value: &mut FieldValue, new_value: u32) {
    match value {
        FieldValue::Uint(value) => *value = u64::from(new_value),
        FieldValue::Int(value) => *value = i64::from(new_value),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            bytes[0..4].copy_from_slice(&new_value.to_le_bytes());
        }
        other => *other = FieldValue::Uint(u64::from(new_value)),
    }
}

fn field_name_is(interner: &StringInterner, name: crate::sym::Sym, expected: &str) -> bool {
    interner.resolve(name).is_some_and(|actual| {
        actual.eq_ignore_ascii_case(expected)
            || actual.replace('_', "").eq_ignore_ascii_case(expected)
    })
}

fn smen_sig() -> SigCode {
    SigCode::from_str("SMEN").expect("literal sig")
}

fn smbn_sig() -> SigCode {
    SigCode::from_str("SMBN").expect("literal sig")
}

fn smqn_sig() -> SigCode {
    SigCode::from_str("SMQN").expect("literal sig")
}

fn pnam_sig() -> SubrecordSig {
    SubrecordSig::from_str("PNAM").expect("literal sig")
}

fn snam_sig() -> SubrecordSig {
    SubrecordSig::from_str("SNAM").expect("literal sig")
}

fn enam_sig() -> SubrecordSig {
    SubrecordSig::from_str("ENAM").expect("literal sig")
}

fn nnam_sig() -> SubrecordSig {
    SubrecordSig::from_str("NNAM").expect("literal sig")
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::record::{FieldEntry, Record};

    fn fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    fn record(interner: &StringInterner, sig: &str, local: u32, eid: Option<&str>) -> Record {
        let mut record = Record::new(SigCode::from_str(sig).unwrap(), fk(interner, local));
        if let Some(eid) = eid {
            record.eid = Some(interner.intern(eid));
        }
        record
    }

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn bytes(bytes: &[u8]) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_slice(bytes))
    }

    fn quest_data(flags: impl Into<u64>) -> FieldValue {
        let mut data = vec![0u8; 20];
        data[0..8].copy_from_slice(&flags.into().to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(data))
    }

    fn graph_with_radio(interner: &StringInterner) -> (StoryManagerSourceGraph, FormKey, FormKey) {
        let root_fk = fk(interner, 0x029152);
        let branch_fk = fk(interner, 0x4E34B9);
        let smqn_fk = fk(interner, 0x4E2A02);
        let prev_fk = fk(interner, 0x4E34BA);
        let quest_fk = fk(interner, 0x1948B4);

        let mut root = record(interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));

        let mut branch = record(
            interner,
            "SMBN",
            branch_fk.local,
            Some("PublicRadios_Branch"),
        );
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));

        let mut smqn = record(
            interner,
            "SMQN",
            smqn_fk.local,
            Some("GeneralRadio_AppalachiaRadio"),
        );
        smqn.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        smqn.fields
            .push(field("SNAM", FieldValue::FormKey(prev_fk)));
        smqn.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let quest = record(interner, "QUST", quest_fk.local, Some("SQ_RadioAppalachia"));

        let mut graph = StoryManagerSourceGraph::default();
        graph.nodes.insert(root_fk, root);
        graph.nodes.insert(branch_fk, branch);
        graph.nodes.insert(smqn_fk, smqn);
        graph.quests.insert(quest_fk, quest);
        (graph, smqn_fk, quest_fk)
    }

    #[test]
    fn story_manager_selects_appalachia_style_scpt_radio_chain() {
        let interner = StringInterner::new();
        let (graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.selected_nodes.contains(&fk(&interner, 0x029152)));
        assert!(selection.selected_nodes.contains(&fk(&interner, 0x4E34B9)));
        assert!(selection.selected_nodes.contains(&smqn_fk));
        assert_eq!(selection.ordered_nodes[0], fk(&interner, 0x029152));
        assert_eq!(selection.ordered_nodes.last().copied(), Some(smqn_fk));
        assert_eq!(
            selection
                .diagnostics
                .iter()
                .filter(|d| d.kind == StoryManagerDiagnosticKind::Selected)
                .count(),
            1
        );
    }

    #[test]
    fn story_manager_selects_and_filters_every_nnam_quest() {
        let interner = StringInterner::new();
        let (mut graph, smqn_fk, first_quest_fk) = graph_with_radio(&interner);
        let second_quest_fk = fk(&interner, 0x1948B5);
        graph
            .nodes
            .get_mut(&smqn_fk)
            .unwrap()
            .fields
            .push(field("NNAM", FieldValue::FormKey(second_quest_fk)));
        graph.quests.insert(
            second_quest_fk,
            record(
                &interner,
                "QUST",
                second_quest_fk.local,
                Some("SQ_SecondRadio"),
            ),
        );
        let translated = FxHashSet::from_iter([first_quest_fk, second_quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);
        let allowed = selection.selected_quests_by_node.get(&smqn_fk).unwrap();
        assert_eq!(allowed.len(), 2);
        assert!(allowed.contains(&first_quest_fk));
        assert!(allowed.contains(&second_quest_fk));

        let mut smqn = graph.nodes.get(&smqn_fk).unwrap().clone();
        retain_story_manager_quests(&mut smqn, &FxHashSet::from_iter([second_quest_fk]));
        assert_eq!(story_manager_quests(&smqn), vec![second_quest_fk]);
    }

    #[test]
    fn story_manager_carries_fo76_only_event_root() {
        let interner = StringInterner::new();
        let (mut graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        // Re-root the chain on a FO76-only event type (ILOC). It is carried so
        // the emit phase can lower it to an isolated Script Event branch.
        graph
            .nodes
            .get_mut(&fk(&interner, 0x029152))
            .unwrap()
            .fields[0]
            .value = bytes(b"ILOC");
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.selected_nodes.contains(&smqn_fk));
        assert!(selection.selected_nodes.contains(&fk(&interner, 0x029152)));
        // The whole chain is selected; nothing is skipped for the event type.
        assert!(
            selection
                .diagnostics
                .iter()
                .all(|d| d.kind == StoryManagerDiagnosticKind::Selected)
        );
    }

    #[test]
    fn story_manager_stops_at_event_root_with_inherited_parent() {
        let interner = StringInterner::new();
        let (mut graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        let inherited_root = FormKey {
            local: 0x00005B,
            plugin: interner.intern("Fallout76.esm"),
        };
        graph
            .nodes
            .get_mut(&fk(&interner, 0x029152))
            .unwrap()
            .fields
            .push(field("PNAM", FieldValue::FormKey(inherited_root)));
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.selected_nodes.contains(&smqn_fk));
        assert!(selection.selected_nodes.contains(&fk(&interner, 0x029152)));
        assert!(
            selection
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.kind == StoryManagerDiagnosticKind::Selected)
        );
    }

    #[test]
    fn incompatible_event_root_lowers_to_keyword_gated_script_branch() {
        let interner = StringInterner::new();
        let mut smen = record(&interner, "SMEN", 0x1000, Some("ILocEvent"));
        smen.fields.push(field("ENAM", bytes(b"ILOC")));
        let script_event_root = FormKey {
            local: FO4_SCRIPT_EVENT_ROOT_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert!(lower_incompatible_event_root(
            &mut smen,
            script_event_root,
            0x0110_00AB,
        ));
        assert_eq!(smen.sig, smbn_sig());
        assert_eq!(story_manager_parent(&smen), Some(script_event_root));
        assert!(story_manager_event_type(&smen).is_none());
        let count = smen
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CITC")
            .and_then(|entry| field_value_u32(&entry.value));
        assert_eq!(count, Some(1));
        let FieldValue::Bytes(gate) = &smen
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CTDA")
            .unwrap()
            .value
        else {
            panic!("raw GetEventData condition expected");
        };
        assert_eq!(
            u32::from_le_bytes(gate[16..20].try_into().unwrap()),
            0x0110_00AB
        );

        let mut valid = record(&interner, "SMEN", 0x1001, Some("MineEvent"));
        valid.fields.push(field("ENAM", bytes(b"TMEE")));
        assert!(!lower_incompatible_event_root(
            &mut valid,
            script_event_root,
            0x0110_00AB,
        ));
        assert_eq!(story_manager_event_type(&valid), Some(fourcc(b"TMEE")));
    }

    #[test]
    fn story_manager_maps_event_roots_to_fo4_native_records() {
        let interner = StringInterner::new();
        let mut script_event = record(&interner, "SMEN", 0x1000, Some("ScriptEvent"));
        script_event.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut mine_event = record(&interner, "SMEN", 0x1001, Some("MineEvent"));
        mine_event.fields.push(field("ENAM", bytes(b"TMEE")));
        let mut fo76_only = record(&interner, "SMEN", 0x1002, Some("InteriorEvent"));
        fo76_only.fields.push(field("ENAM", bytes(b"ILOC")));

        assert_eq!(fo4_story_manager_event_root(&script_event), Some(0x029152));
        assert_eq!(fo4_story_manager_event_root(&mine_event), Some(0x02A68B));
        assert_eq!(fo4_story_manager_event_root(&fo76_only), None);
    }

    #[test]
    fn incompatible_event_roots_include_every_fo76_only_event_type() {
        let interner = StringInterner::new();
        let mut graph = StoryManagerSourceGraph::default();
        let mut selected = FxHashSet::default();
        let incompatible = [b"ADBO", b"CBGN", b"ILOC", b"LCPG", b"PCON", b"QPMT"];
        let compatible = [b"HACK", b"LEVL", b"TMEE"];

        for (index, event_type) in incompatible.iter().enumerate() {
            let form_key = fk(&interner, 0x2000 + index as u32);
            let mut event = record(&interner, "SMEN", form_key.local, None);
            event.fields.push(field("ENAM", bytes(*event_type)));
            graph.nodes.insert(form_key, event);
            selected.insert(form_key);
        }
        for (index, event_type) in compatible.iter().enumerate() {
            let form_key = fk(&interner, 0x3000 + index as u32);
            let mut event = record(&interner, "SMEN", form_key.local, None);
            event.fields.push(field("ENAM", bytes(*event_type)));
            graph.nodes.insert(form_key, event);
            selected.insert(form_key);
        }

        let roots = incompatible_story_manager_event_roots(&selected, &graph);
        assert_eq!(roots.len(), incompatible.len());
        assert_eq!(
            roots
                .iter()
                .map(|(_, event_type)| *event_type)
                .collect::<FxHashSet<_>>(),
            incompatible
                .iter()
                .map(|event_type| fourcc(*event_type))
                .collect::<FxHashSet<_>>()
        );
    }

    #[test]
    fn bridged_quest_event_rewrites_to_scpt_after_root_emits() {
        let interner = StringInterner::new();
        let (mut graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        let root_fk = fk(&interner, 0x029152);
        graph.nodes.get_mut(&root_fk).unwrap().fields[0].value = bytes(b"ILOC");
        graph
            .quests
            .get_mut(&quest_fk)
            .unwrap()
            .fields
            .push(field("ENAM", bytes(b"ILOC")));
        let mut selection =
            classify_story_manager_records(&graph, &FxHashSet::from_iter([quest_fk]), &interner);
        assert_eq!(
            selection.selected_event_roots_by_quest.get(&quest_fk),
            Some(&FxHashSet::from_iter([root_fk]))
        );
        let native_scpt_root = fk(&interner, 0x029153);
        let mut native_scpt = record(&interner, "SMEN", native_scpt_root.local, None);
        native_scpt.fields.push(field("ENAM", bytes(b"SCPT")));
        graph.nodes.insert(native_scpt_root, native_scpt);
        selection
            .selected_event_roots_by_quest
            .get_mut(&quest_fk)
            .unwrap()
            .insert(native_scpt_root);
        let emitted_nodes = FxHashSet::from_iter(
            selection
                .selected_nodes
                .iter()
                .copied()
                .chain([native_scpt_root]),
        );

        let plan = plan_story_manager_quest_events(
            &selection,
            &graph,
            &FxHashMap::from_iter([(root_fk, 0x0110_00AB)]),
            &emitted_nodes,
        );

        assert_eq!(plan.rewrites.get(&quest_fk), Some(&SCPT_EVENT_TYPE));
        assert!(plan.unresolved.is_empty());

        let mut target_quest = record(&interner, "QUST", quest_fk.local, Some("SQ_Radio"));
        target_quest.fields.push(field("DNAM", bytes(&[0; 12])));
        target_quest.fields.push(field("LNAM", FieldValue::None));
        assert!(set_qust_event_type(&mut target_quest, SCPT_EVENT_TYPE));
        assert_eq!(
            story_manager_event_type(&target_quest),
            Some(SCPT_EVENT_TYPE)
        );
        assert_eq!(
            target_quest
                .fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["DNAM", "ENAM", "LNAM"]
        );
        assert!(selection.selected_quests_by_node.contains_key(&smqn_fk));
    }

    #[test]
    fn quest_event_plan_reports_conflicting_final_event_types() {
        let interner = StringInterner::new();
        let quest_fk = fk(&interner, 0x2000);
        let incompatible_root = fk(&interner, 0x3000);
        let kill_root = fk(&interner, 0x3001);
        let mut graph = StoryManagerSourceGraph::default();
        let mut quest = record(&interner, "QUST", quest_fk.local, Some("SQ_Conflict"));
        quest.fields.push(field("ENAM", bytes(b"ILOC")));
        graph.quests.insert(quest_fk, quest);
        let mut incompatible = record(&interner, "SMEN", incompatible_root.local, None);
        incompatible.fields.push(field("ENAM", bytes(b"ILOC")));
        graph.nodes.insert(incompatible_root, incompatible);
        let mut kill = record(&interner, "SMEN", kill_root.local, None);
        kill.fields.push(field("ENAM", bytes(b"KILL")));
        graph.nodes.insert(kill_root, kill);
        let mut selection = StoryManagerSelection::default();
        selection.selected_event_roots_by_quest.insert(
            quest_fk,
            FxHashSet::from_iter([incompatible_root, kill_root]),
        );

        let plan = plan_story_manager_quest_events(
            &selection,
            &graph,
            &FxHashMap::from_iter([(incompatible_root, 0x0110_00AB)]),
            &FxHashSet::from_iter([incompatible_root, kill_root]),
        );

        assert!(plan.rewrites.is_empty());
        assert_eq!(plan.unresolved.len(), 1);
        assert_eq!(plan.unresolved[0].0, quest_fk);
        assert_eq!(
            plan.unresolved[0]
                .1
                .iter()
                .copied()
                .collect::<FxHashSet<_>>(),
            FxHashSet::from_iter([SCPT_EVENT_TYPE, fourcc(b"KILL")])
        );
    }

    #[test]
    fn classify_quest_keeps_generic_gameplay_for_story_manager() {
        let interner = StringInterner::new();
        let quest = record(&interner, "QUST", 0x2000, Some("SQ_SomeSideQuest"));
        assert_eq!(
            classify_quest(&quest, &interner),
            Some(StoryQuestKind::Generic)
        );

        let dev = record(&interner, "QUST", 0x2001, Some("ZZTestQuest"));
        assert_eq!(classify_quest(&dev, &interner), None);
    }

    #[test]
    fn story_manager_skips_quest_with_discarded_event_alias() {
        let interner = StringInterner::new();
        let (mut graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        let quest = graph.quests.get_mut(&quest_fk).unwrap();
        quest
            .fields
            .push(field("ALST", bytes(&0_u32.to_le_bytes())));
        quest.fields.push(field(
            "ALFE",
            bytes(&u32::from_le_bytes(*b"CLOC").to_le_bytes()),
        ));
        quest
            .fields
            .push(field("ALFD", bytes(&1_u32.to_le_bytes())));
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(!selection.selected_nodes.contains(&smqn_fk));
        assert_eq!(
            selection.diagnostics[0].reason,
            Some(StoryManagerSkipReason::UnsupportedQuest)
        );
    }

    #[test]
    fn high_school_pa_broadcast_is_story_manager_driven_not_name_blocked() {
        let interner = StringInterner::new();
        let mut quest = record(
            &interner,
            "QUST",
            0x024442,
            Some("CB_HighSchoolPASystem_RadioScenes"),
        );
        quest
            .fields
            .push(field("DATA", quest_data(0x0401_8111_u64)));

        assert_eq!(
            classify_quest(&quest, &interner),
            Some(StoryQuestKind::Radio)
        );
        assert!(!is_passive_dialogue_controller(&quest, true, &interner));
    }

    #[test]
    fn whitespring_live_shape_is_passive_dialogue_controller() {
        let interner = StringInterner::new();
        let mut quest = record(&interner, "QUST", 0x37D8DD, Some("WhitespringQuest"));
        quest
            .fields
            .push(field("DATA", quest_data(0x0001_8111_u64)));
        quest.fields.push(field("VMAD", FieldValue::None));
        quest.fields.push(field("INDX", bytes(&[10, 0, 2, 0])));

        assert!(is_passive_dialogue_controller(&quest, true, &interner));
        assert!(!is_passive_dialogue_controller(&quest, false, &interner));
    }

    #[test]
    fn story_manager_nuls_unselected_previous_sibling() {
        let interner = StringInterner::new();
        let (graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        let translated = FxHashSet::from_iter([quest_fk]);
        let selection = classify_story_manager_records(&graph, &translated, &interner);
        let mut smqn = graph.nodes.get(&smqn_fk).unwrap().clone();

        assert!(sanitize_story_manager_previous_node(
            &mut smqn,
            &selection.selected_nodes,
            &interner
        ));
        let previous = story_manager_fk(&smqn, snam_sig()).unwrap_or(FormKey {
            local: 0,
            plugin: smqn.form_key.plugin,
        });
        assert_eq!(previous.local, 0);
    }

    #[test]
    fn story_manager_selects_explicit_npc_conversation_quest() {
        let interner = StringInterner::new();
        let (mut graph, _smqn_fk, quest_fk) = graph_with_radio(&interner);
        let quest = graph.quests.get_mut(&quest_fk).unwrap();
        quest.eid = Some(interner.intern("NPCConversation_Biv"));
        quest
            .fields
            .push(field("DATA", quest_data(QUST_FLAG_HAS_DIALOGUE_DATA)));
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.fallback_dialogue_quests.is_empty());
    }

    #[test]
    fn story_manager_falls_back_only_for_unsupported_dialogue_event_root() {
        let interner = StringInterner::new();
        let (mut graph, _smqn_fk, quest_fk) = graph_with_radio(&interner);
        graph
            .nodes
            .get_mut(&fk(&interner, 0x029152))
            .unwrap()
            .fields[0]
            .value = bytes(b"ILOC");
        let quest = graph.quests.get_mut(&quest_fk).unwrap();
        quest.eid = Some(interner.intern("NPCConversation_Biv"));
        quest
            .fields
            .push(field("DATA", quest_data(QUST_FLAG_HAS_DIALOGUE_DATA)));
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert_eq!(selection.fallback_dialogue_quests, vec![quest_fk]);
    }

    #[test]
    fn story_manager_does_not_force_unique_dialogue_fallback() {
        let interner = StringInterner::new();
        let (mut graph, _smqn_fk, quest_fk) = graph_with_radio(&interner);
        graph
            .nodes
            .get_mut(&fk(&interner, 0x029152))
            .unwrap()
            .fields[0]
            .value = bytes(b"ILOC");
        let quest = graph.quests.get_mut(&quest_fk).unwrap();
        quest.eid = Some(interner.intern("NPCConversation_Unique"));
        quest.fields.push(field(
            "DATA",
            quest_data(u64::from(QUST_FLAG_HAS_DIALOGUE_DATA) | QUST_FLAG_UNIQUE_INSTANCE),
        ));
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.fallback_dialogue_quests.is_empty());
    }

    #[test]
    fn story_manager_does_not_autostart_tw043_with_dialogue_data() {
        let interner = StringInterner::new();
        let (mut graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        let quest = graph.quests.get_mut(&quest_fk).unwrap();
        quest.eid = Some(interner.intern("TW043"));
        quest
            .fields
            .push(field("DATA", quest_data(QUST_FLAG_HAS_DIALOGUE_DATA)));
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.selected_nodes.contains(&smqn_fk));
        assert!(selection.fallback_dialogue_quests.is_empty());
    }

    #[test]
    fn story_manager_skips_test_dialogue_quest() {
        let interner = StringInterner::new();
        let (mut graph, _smqn_fk, quest_fk) = graph_with_radio(&interner);
        let quest = graph.quests.get_mut(&quest_fk).unwrap();
        quest.eid = Some(interner.intern("test_VHarbison_Dialogue_Someone"));
        quest
            .fields
            .push(field("DATA", quest_data(QUST_FLAG_HAS_DIALOGUE_DATA)));
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.selected_nodes.is_empty());
        assert_eq!(
            selection.diagnostics[0].reason,
            Some(StoryManagerSkipReason::UnsupportedQuest)
        );
    }

    #[test]
    fn story_manager_force_autostart_sets_sge_when_dialogue_data_present() {
        let interner = StringInterner::new();
        let mut record = record(&interner, "QUST", 0x0100, Some("NPCConversation_Biv"));
        record.fields.push(field(
            "DNAM",
            bytes(&QUST_FLAG_HAS_DIALOGUE_DATA.to_le_bytes()),
        ));

        assert!(force_qust_autostart(&mut record, &interner));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("DNAM bytes expected");
        };
        let flags = u16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(
            flags & (QUST_FLAG_HAS_DIALOGUE_DATA | QUST_FLAG_START_GAME_ENABLED),
            QUST_FLAG_HAS_DIALOGUE_DATA | QUST_FLAG_START_GAME_ENABLED
        );
    }
}
