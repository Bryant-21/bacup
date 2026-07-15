//! FO76->FO4 Story Manager subset carry.
//!
//! The global translation map still skips SMBN/SMEN/SMQN. This phase restores a
//! narrow, classified subset for radio and dialogue quest startup paths.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldValue, Record};
use crate::run::{ConversionRun, RunError, TranslateStats};
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fo76_fo4::{
    fo76_qust_type_disables_start_game, qust_eid_is_dialogue_conversation,
};

const SCPT_EVENT_TYPE: u32 = 0x5450_4353;
const QUST_FLAG_START_GAME_ENABLED: u16 = 0x0001;
const QUST_FLAG_HAS_DIALOGUE_DATA: u16 = 0x8000;

const FO4_SM_EVENT_ROOTS: [(u32, u32); 10] = [
    (fourcc(b"ADIA"), 0x021E65),
    (fourcc(b"CLOC"), 0x0238DB),
    (fourcc(b"HACK"), 0x1244D0),
    (fourcc(b"KILL"), 0x034A37),
    (fourcc(b"LCLD"), 0x05FD8C),
    (fourcc(b"LEVL"), 0x16556F),
    (fourcc(b"LOCK"), 0x0A51B5),
    (fourcc(b"REMP"), 0x07956B),
    (SCPT_EVENT_TYPE, 0x029152),
    (fourcc(b"TMEE"), 0x02A68B),
];

const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_le_bytes([code[0], code[1], code[2], code[3]])
}

/// Story Manager event types (`SMEN.ENAM`) that exist in FO4. A node rooted at one
/// of these is carried with its event type intact; any other (FO76-only) type —
/// e.g. ADBO/CBGN/ILOC/LCPG/PCON/QPMT — is remapped to SCPT so FO4's CK does not
/// crash instantiating an unknown event dispatcher at load.
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
    pub selected_dialogue_quests: Vec<FormKey>,
    pub diagnostics: Vec<StoryManagerDiagnostic>,
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
        if let Some(quest) = story_manager_quest(record) {
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
        let Some(quest_fk) = story_manager_quest(smqn) else {
            push_skip(
                &mut selection,
                *smqn_fk,
                StoryManagerSkipReason::MissingQuest,
                "SMQN has no NNAM quest",
            );
            continue;
        };
        if !translated_quests.contains(&quest_fk) {
            push_skip(
                &mut selection,
                *smqn_fk,
                StoryManagerSkipReason::QuestNotTranslated,
                format!("quest {:06X} is not translated", quest_fk.local),
            );
            continue;
        }
        let Some(quest) = graph.quests.get(&quest_fk) else {
            push_skip(
                &mut selection,
                *smqn_fk,
                StoryManagerSkipReason::MissingQuestRecord,
                format!("quest {:06X} could not be read", quest_fk.local),
            );
            continue;
        };
        let Some(quest_kind) = classify_quest(quest, interner) else {
            push_skip(
                &mut selection,
                *smqn_fk,
                StoryManagerSkipReason::UnsupportedQuest,
                format!("quest {:06X} is not radio/dialogue safe", quest_fk.local),
            );
            continue;
        };
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

        for fk in chain.into_iter().rev().chain(std::iter::once(*smqn_fk)) {
            if selection.selected_nodes.insert(fk) {
                selection.ordered_nodes.push(fk);
            }
        }
        if quest_kind == StoryQuestKind::Dialogue
            && !selection.selected_dialogue_quests.contains(&quest_fk)
        {
            selection.selected_dialogue_quests.push(quest_fk);
        }
        selection.diagnostics.push(StoryManagerDiagnostic {
            form_key: *smqn_fk,
            kind: StoryManagerDiagnosticKind::Selected,
            reason: None,
            message: format!(
                "selected SMQN {:06X} quest={:06X} kind={quest_kind:?}",
                smqn_fk.local, quest_fk.local
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
    if qust_dnam_type(value, interner).is_some_and(fo76_qust_type_disables_start_game) {
        return false;
    }
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

fn qust_dnam_type(value: &FieldValue, interner: &StringInterner) -> Option<u8> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() > 8 => Some(bytes[8]),
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| field_name_is(interner, *name, "type"))
            .and_then(|(_, value)| field_value_u32(value))
            .and_then(|value| u8::try_from(value).ok()),
        _ => None,
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
            // Every SMEN is a valid event root: FO4-native types are kept as-is and
            // FO76-only types are neutralized to SCPT at translation time, so no
            // event root is unsafe anymore. Its PNAM may point to an inherited
            // base-game root that is intentionally outside the source graph.
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
    if qust_eid_is_test_or_dev(record, interner)
        || qust_eid_is_disabled_local_broadcast(record, interner)
    {
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

fn qust_eid_is_disabled_local_broadcast(record: &Record, interner: &StringInterner) -> bool {
    quest_editor_id_lower(record, interner)
        .is_some_and(|eid| eid == "cb_highschoolpasystem_radioscenes")
}

fn qust_has_dialogue_data(record: &Record, interner: &StringInterner) -> bool {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"DATA" || entry.sig.0 == *b"DNAM")
        .and_then(|entry| quest_flags(&entry.value, interner))
        .is_some_and(|flags| flags & QUST_FLAG_HAS_DIALOGUE_DATA != 0)
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

fn quest_flags(value: &FieldValue, interner: &StringInterner) -> Option<u16> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes([bytes[0], bytes[1]]))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| field_name_is(interner, *name, "flags"))
            .and_then(|(_, value)| field_value_u16(value)),
        _ => field_value_u16(value),
    }
}

fn story_manager_parent(record: &Record) -> Option<FormKey> {
    story_manager_fk(record, pnam_sig())
}

fn story_manager_quest(record: &Record) -> Option<FormKey> {
    story_manager_fk(record, nnam_sig())
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
    let target_event = if is_fo4_valid_sm_event(source_event) {
        source_event
    } else {
        SCPT_EVENT_TYPE
    };
    FO4_SM_EVENT_ROOTS
        .iter()
        .find_map(|(event_type, local)| (*event_type == target_event).then_some(*local))
}

/// If `record` is an SMEN whose event type has no FO4 equivalent, remap it to SCPT
/// (an inert, CK-valid event) so FO4 loads the node without crashing on an unknown
/// dispatcher. Returns true when the type was changed.
pub(crate) fn neutralize_fo76_only_event_type(record: &mut Record) -> bool {
    if record.sig != smen_sig() {
        return false;
    }
    match story_manager_event_type(record) {
        Some(current) if !is_fo4_valid_sm_event(current) => {}
        _ => return false,
    }
    for entry in &mut record.fields {
        if entry.sig == enam_sig() {
            write_u32_value(&mut entry.value, SCPT_EVENT_TYPE);
            return true;
        }
    }
    false
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

    fn quest_data(flags: u16) -> FieldValue {
        let mut data = vec![0u8; 20];
        data[0..2].copy_from_slice(&flags.to_le_bytes());
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
    fn story_manager_carries_fo76_only_event_root() {
        let interner = StringInterner::new();
        let (mut graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        // Re-root the chain on a FO76-only event type (ILOC). It must now be
        // carried, not skipped — the event type is neutralized to SCPT at
        // translation time so FO4's CK loads it without crashing.
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
    fn neutralize_remaps_fo76_only_event_type_to_scpt() {
        let interner = StringInterner::new();
        // FO76-only event (ILOC) → remapped to SCPT.
        let mut smen = record(&interner, "SMEN", 0x1000, Some("ILocEvent"));
        smen.fields.push(field("ENAM", bytes(b"ILOC")));
        assert!(neutralize_fo76_only_event_type(&mut smen));
        assert_eq!(story_manager_event_type(&smen), Some(SCPT_EVENT_TYPE));

        // FO4-native event (TMEE) → untouched.
        let mut valid = record(&interner, "SMEN", 0x1001, Some("MineEvent"));
        valid.fields.push(field("ENAM", bytes(b"TMEE")));
        assert!(!neutralize_fo76_only_event_type(&mut valid));
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
        assert_eq!(fo4_story_manager_event_root(&fo76_only), Some(0x029152));
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
    fn story_manager_skips_high_school_pa_broadcast() {
        let interner = StringInterner::new();
        let quest = record(
            &interner,
            "QUST",
            0x024442,
            Some("CB_HighSchoolPASystem_RadioScenes"),
        );

        assert_eq!(classify_quest(&quest, &interner), None);
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

        assert_eq!(selection.selected_dialogue_quests, vec![quest_fk]);
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
        assert!(selection.selected_dialogue_quests.is_empty());
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

    #[test]
    fn story_manager_force_autostart_keeps_event_types_disabled() {
        let interner = StringInterner::new();
        for quest_type in [6, 8] {
            let mut record = record(&interner, "QUST", 0x0100, Some("Dialogue_EventActivity"));
            let mut dnam = vec![0u8; 12];
            dnam[0..2].copy_from_slice(&QUST_FLAG_HAS_DIALOGUE_DATA.to_le_bytes());
            dnam[8] = quest_type;
            record.fields.push(field("DNAM", bytes(&dnam)));

            assert!(!force_qust_autostart(&mut record, &interner));
            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("DNAM bytes expected");
            };
            assert_eq!(
                u16::from_le_bytes([bytes[0], bytes[1]]) & QUST_FLAG_START_GAME_ENABLED,
                0
            );
        }
    }
}
