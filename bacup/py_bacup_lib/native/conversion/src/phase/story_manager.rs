//! FO76->FO4 Story Manager subset carry.
//!
//! The global translation map still skips SMBN/SMEN/SMQN. This phase restores a
//! narrow, classified subset for radio and dialogue quest startup paths.

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use smallvec::SmallVec;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::run::{ConversionRun, RunError, TranslateStats};
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::translator::Game;
use crate::translator::pair_hooks::fo76_fo4::{
    Fo76Fo4Hook, qust_eid_is_dialogue_conversation, qust_has_untranslatable_event_alias_for_source,
    qust_uses_player_connect_autostart_fallback,
};

const SCPT_EVENT_TYPE: u32 = 0x5450_4353;
pub(crate) const FO4_SCRIPT_EVENT_ROOT_LOCAL: u32 = 0x029152;
const QUST_FLAG_START_GAME_ENABLED: u16 = 0x0001;
const QUST_FLAG_HAS_DIALOGUE_DATA: u16 = 0x8000;
const QUST_FLAG_UNIQUE_INSTANCE: u64 = 0x0001_0000;
const QUST_STAGE_FLAG_RUN_ON_START: u16 = 0x0002;
const BETTER_TOMORROW_CONTENT_MANAGER_LOCAL: u32 = 0x0073_99D6;
const BETTER_TOMORROW_CONTENT_MANAGER_EID: &str = "NPE_DQ_ContentManager";
const BETTER_TOMORROW_QUEST_LOCAL: u32 = 0x006F_D072;
const BETTER_TOMORROW_START_KEYWORD_LOCAL: u32 = 0x0072_C956;
const BETTER_TOMORROW_REGION_LOCAL: u32 = 0x0009_5004;
// These quests have exact single-player adapters for required FO76 event fills;
// any remaining stripped event aliases are optional in the converted record.
const SINGLE_PLAYER_EVENT_ALIAS_ADAPTED_QUESTS: [(u32, &str); 14] = [
    (0x0000_D783, "mtr06_physicalexam"),
    (0x002D_0F69, "en07_mq_fleeblast"),
    (0x002D_0F6A, "en07_mq_codehunt"),
    (0x003E_03AA, "msilopersonal"),
    (0x0056_BF31, "comp_quest_outtro_full_astronaut"),
    (0x0058_2172, "comp_quest_intro_lite_beggar"),
    (0x0058_2153, "comp_quest_intro_lite_hunter"),
    (0x0056_8CB2, "comp_quest_intro_lite_raiderpunk"),
    (0x0058_5BD2, "comp_quest_intro_lite_scavenger"),
    (0x0058_2173, "comp_quest_intro_lite_wanderer"),
    (0x0054_F1A4, "comp_rq_fetch"),
    (0x0056_FB76, "comp_rq_kill"),
    (0x0057_27AD, "comp_rq_rescue"),
    (0x0055_FD53, "comp_visitor"),
];
const BECKETT_SPECIFIC_ALIAS_QUESTS: [(u32, &str); 16] = [
    (
        0x0058_215B,
        "comp_rq_fetch_specificaliases_beckett_000_saddiary",
    ),
    (
        0x0058_2164,
        "comp_rq_rescue_specificaliases_beckett_001_cultistsage",
    ),
    (0x0058_2163, "comp_rq_fetch_specificaliases_beckett_002_key"),
    (
        0x0058_2160,
        "comp_rq_kill_specificaliases_beckett_003_bronx",
    ),
    (
        0x0058_2167,
        "comp_rq_fetch_specificaliases_beckett_004_cave",
    ),
    (
        0x0058_2165,
        "comp_rq_kill_specificaliases_beckett_005_blood",
    ),
    (
        0x0058_215A,
        "comp_rq_rescue_specificaliases_beckett_006_pet",
    ),
    (0x0058_215E, "comp_rq_kill_specificaliases_beckett_007_dj"),
    (
        0x0058_216A,
        "comp_rq_rescue_specificaliases_beckett_008_missnanny",
    ),
    (
        0x0058_2168,
        "comp_rq_fetch_specificaliases_beckett_009_holotapes",
    ),
    (
        0x0058_215F,
        "comp_rq_fetch_specificaliases_beckett_010_poisonedfood",
    ),
    (0x0058_215D, "comp_rq_kill_specificaliases_beckett_011_eye"),
    (
        0x005A_272F,
        "comp_rq_kill_specificaliases_beckett_bloodeaglerandomloc",
    ),
    (
        0x005A_2730,
        "comp_rq_kill_specificaliases_beckett_bloodeagledungeon",
    ),
    (0x005A_272D, "comp_rq_fetch_specificaliases_legendaryarmor"),
    (0x005A_272E, "comp_rq_fetch_specificaliases_legendaryweapon"),
];
const LITE_ALLY_INTRO_NODE_LOCAL: u32 = 0x0058_56CB;
const LITE_ALLY_INTRO_BRANCH_LOCAL: u32 = 0x003F_53A3;
const LITE_ALLY_INTRO_PREVIOUS_LOCAL: u32 = 0x0056_F719;
const LITE_ALLY_INTRO_SHARED_EVENT_FLAGS: u32 = 0x0002_0000;
const LITE_ALLY_INTRO_ROUTES: [(u32, &str, u32, &str); 5] = [
    (
        0x0058_2172,
        "comp_quest_intro_lite_beggar",
        0x0058_56CC,
        "comp_keyword_queststart_beggar",
    ),
    (
        0x0058_2153,
        "comp_quest_intro_lite_hunter",
        0x0058_5CCA,
        "comp_keyword_queststart_hunter",
    ),
    (
        0x0056_8CB2,
        "comp_quest_intro_lite_raiderpunk",
        0x0058_5D27,
        "comp_keyword_queststart_raiderpunk",
    ),
    (
        0x0058_5BD2,
        "comp_quest_intro_lite_scavenger",
        0x0058_5CD8,
        "comp_keyword_queststart_scavenger",
    ),
    (
        0x0058_2173,
        "comp_quest_intro_lite_wanderer",
        0x0058_5CBF,
        "comp_keyword_queststart_wanderer",
    ),
];
const ALWAYS_ON_RADIO_STATION_QUESTS: [u32; 3] = [0x0019_48B4, 0x0039_9C29, 0x0063_3375];
const SKYLINE_BREADCRUMB_NODE_LOCAL: u32 = 0x0072_A2BE;
const SKYLINE_BREADCRUMB_START_KEYWORD_LOCAL: u32 = 0x0072_A0A0;
const SKYLINE_BREADCRUMB_ACTIVE_KEYWORD_LOCAL: u32 = 0x0069_3DA9;
const SKYLINE_BREADCRUMB_QUEST_LOCAL: u32 = 0x0072_A2A8;
const BURN_SQ01_RADIO_NODE_LOCAL: u32 = 0x007F_79E0;
const BURN_SQ01_RADIO_START_KEYWORD_LOCAL: u32 = 0x007F_79E1;
const BURN_SQ01_RADIO_QUEST_LOCAL: u32 = 0x007F_79AB;
const MQ_OVERSEER_NODE_LOCAL: u32 = 0x004E_4A13;
const MQ_OVERSEER_START_KEYWORD_LOCAL: u32 = 0x004E_49E5;
const MQ_OVERSEER_QUEST_LOCAL: u32 = 0x004E_49D9;
const OVERSEER_PERSONAL_NODE_LOCAL: u32 = 0x0012_6717;
const OVERSEER_PERSONAL_START_KEYWORD_LOCAL: u32 = 0x0012_F8A2;
const OVERSEER_PERSONAL_QUEST_LOCAL: u32 = 0x0026_AA34;
const TW007_COLD_CASE_NODE_LOCAL: u32 = 0x003D_B98B;
const TW007_COLD_CASE_BRANCH_LOCAL: u32 = 0x003D_B98A;
const TW007_COLD_CASE_START_KEYWORD_LOCAL: u32 = 0x003D_B988;
const TW007_COLD_CASE_QUEST_LOCAL: u32 = 0x002C_460C;
const TW007_COLD_CASE_COMPLETION_CONDITION: [u8; 32] = [
    0x00, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x59, 0x03, 0x65, 0x43, 0x0C, 0x46, 0x2C, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x52, 0x33, 0x00, 0x00,
];
const TW007_COLD_CASE_START_CONDITION: [u8; 32] = [
    0x00, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x40, 0x02, 0x65, 0x43, 0x00, 0x00, 0x4B, 0x31,
    0x88, 0xB9, 0x3D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
];
const BURN_SQ01_RADIO_LCP_CONDITION: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x4A, 0x00, 0x00, 0x00, 0x53, 0x21, 0x86, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
];
const BURN_OUTRO_P2_NODE_LOCAL: u32 = 0x0084_4F49;
const BURN_OUTRO_P2_START_KEYWORD_LOCAL: u32 = 0x0084_4F44;
const BURN_OUTRO_P2_ACTIVE_KEYWORD_LOCAL: u32 = 0x0084_4F45;
const BURN_OUTRO_P2_QUEST_LOCAL: u32 = 0x0083_8DB1;
const AC_SQ03_NODE_LOCAL: u32 = 0x0072_9CD0;
const AC_SQ04_NODE_LOCAL: u32 = 0x0073_99EC;
const AC_SQ05_NODE_LOCAL: u32 = 0x0074_84FC;
const AC_SQ05_BRANCH_LOCAL: u32 = 0x0072_9CCF;
const AC_SQ05_START_KEYWORD_LOCAL: u32 = 0x006F_CFC0;
const AC_SQ05_ACTIVE_KEYWORD_LOCAL: u32 = 0x006F_CFBF;
const AC_SQ05_QUEST_LOCAL: u32 = 0x006F_CF87;

const FO4_SM_EVENT_ROOTS: [(u32, u32); 9] = [
    (fourcc(b"ADIA"), 0x021E65),
    (fourcc(b"CLOC"), 0x0238DB),
    (fourcc(b"HACK"), 0x1244D0),
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
const FO4_VALID_SM_EVENT_TYPES: [u32; 9] = [
    fourcc(b"ADIA"),
    fourcc(b"CLOC"),
    fourcc(b"HACK"),
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
    pub event_metadata_quests: FxHashMap<FormKey, Record>,
    pub keywords: FxHashMap<FormKey, Record>,
    pub condition_diagnostics: Vec<StoryManagerConditionDiagnostic>,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StoryManagerRouteSeedReport {
    pub schema_version: u32,
    pub source_game: String,
    pub target_game: String,
    pub routes: Vec<StoryManagerRouteSeed>,
}

impl StoryManagerRouteSeedReport {
    pub(crate) fn empty(source: Game, target: Game) -> Self {
        Self {
            schema_version: 1,
            source_game: source.as_str().to_string(),
            target_game: target.as_str().to_string(),
            routes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StoryManagerRouteSeed {
    pub route_id: String,
    pub source_quest: String,
    pub target_quest: Option<String>,
    pub quest_editor_id: Option<String>,
    pub source_node: Option<String>,
    pub target_node: Option<String>,
    pub source_root: Option<String>,
    pub target_root: Option<String>,
    pub target_event_root: Option<String>,
    pub source_quest_event: Option<String>,
    pub target_quest_event: Option<String>,
    pub source_root_event: Option<String>,
    pub target_event: Option<String>,
    pub source_start_keyword: Option<String>,
    pub source_metadata_fields: Vec<String>,
    pub source_selector_keywords: Vec<String>,
    pub target_selector_keywords: Vec<String>,
    pub bridge_keyword: Option<String>,
    pub emission_status: String,
    pub skip_reason: Option<String>,
    pub condition_valid: bool,
    pub structural_status: String,
    pub producer_status: String,
    pub fo76_only_event: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoryManagerDiagnostic {
    pub form_key: FormKey,
    pub kind: StoryManagerDiagnosticKind,
    pub reason: Option<StoryManagerSkipReason>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoryManagerConditionDiagnostic {
    pub form_key: FormKey,
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

    synthesize_better_tomorrow_story_manager_node(run, &mut graph)?;

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
    let referenced_quest_fks = quest_fks.clone();
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

    if story_manager_condition_normalization_applies(run.source, run.target) {
        let all_quest_fks = iter_form_keys_of_sig(run.source_handle_id, quest_sig, &run.interner)?;
        for fk in all_quest_fks {
            if referenced_quest_fks.contains(&fk)
                || !run
                    .mapper_state
                    .as_ref()
                    .is_some_and(|state| state.source_to_target.contains_key(&fk))
            {
                continue;
            }
            match read_record_relayout_by_form_key(
                run.source_handle_id,
                &fk,
                &run.schema_source,
                &run.interner,
                None,
            ) {
                Ok(record) if record.sig == quest_sig && quest_has_event_metadata(&record) => {
                    graph.event_metadata_quests.insert(fk, record);
                }
                Ok(_) => {}
                Err(e) => {
                    let warning = run.interner.intern(&format!(
                        "story_manager_event_metadata_quest_read:{:06X}:{e}",
                        fk.local
                    ));
                    run.warnings.push(warning);
                }
            }
        }
    }

    if story_manager_condition_normalization_applies(run.source, run.target) {
        let keyword_sig = SigCode::from_str("KYWD")
            .map_err(|e| RunError::InvalidConfig(format!("KYWD signature: {e}")))?;
        let mut keyword_fks = FxHashSet::default();
        for record in graph
            .nodes
            .values()
            .filter(|record| record.sig == smqn_sig())
        {
            for entry in &record.fields {
                let raw = Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
                    .or_else(|| {
                        has_lite_ally_intro_node_identity(record, &run.interner)
                            .then(|| lite_ally_shared_or_start_keyword(&entry.value))
                            .flatten()
                    })
                    .or_else(|| Fo76Fo4Hook::story_manager_active_keyword(&entry.value));
                if let Some(raw) = raw {
                    keyword_fks.insert(raw_form_key(raw, record.form_key.plugin));
                }
            }
        }
        let mut keyword_fks = keyword_fks.into_iter().collect::<Vec<_>>();
        keyword_fks.sort_by_key(|fk| fk.local);
        for fk in keyword_fks {
            match read_record_relayout_by_form_key(
                run.source_handle_id,
                &fk,
                &run.schema_source,
                &run.interner,
                None,
            ) {
                Ok(record) if record.sig == keyword_sig => {
                    graph.keywords.insert(fk, record);
                }
                Ok(_) => {}
                Err(e) => {
                    let warning = run
                        .interner
                        .intern(&format!("story_manager_keyword_read:{:06X}:{e}", fk.local));
                    run.warnings.push(warning);
                }
            }
        }

        let condition_diagnostics = normalize_story_manager_quest_start_conditions_for_pair(
            run.source,
            run.target,
            &mut graph,
            &run.interner,
        );
        for diagnostic in &condition_diagnostics {
            let warning = run.interner.intern(&format!(
                "story_manager_condition:{:06X}:{}",
                diagnostic.form_key.local, diagnostic.message
            ));
            run.warnings.push(warning);
        }
        graph.condition_diagnostics = condition_diagnostics;
    }

    Ok(graph)
}

fn synthesize_better_tomorrow_story_manager_node(
    run: &mut ConversionRun,
    graph: &mut StoryManagerSourceGraph,
) -> Result<(), RunError> {
    let dcgf_sig = SigCode::from_str("DCGF")
        .map_err(|e| RunError::InvalidConfig(format!("DCGF signature: {e}")))?;
    let source_form_keys = iter_form_keys_of_sig(run.source_handle_id, dcgf_sig, &run.interner)?;
    let Some(form_key) = source_form_keys.into_iter().find(|form_key| {
        form_key.local & 0x00FF_FFFF == BETTER_TOMORROW_CONTENT_MANAGER_LOCAL
            && run
                .interner
                .resolve(form_key.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case("SeventySix.esm"))
    }) else {
        return Ok(());
    };
    let content_manager = read_record_relayout_by_form_key(
        run.source_handle_id,
        &form_key,
        &run.schema_source,
        &run.interner,
        None,
    )?;
    if let Some(node) = better_tomorrow_story_manager_node(&content_manager, &run.interner) {
        graph.nodes.insert(node.form_key, node);
    }
    Ok(())
}

fn better_tomorrow_story_manager_node(
    content_manager: &Record,
    interner: &StringInterner,
) -> Option<Record> {
    if content_manager.sig != SigCode::from_str("DCGF").expect("literal sig")
        || content_manager.form_key.local & 0x00FF_FFFF != BETTER_TOMORROW_CONTENT_MANAGER_LOCAL
        || !interner
            .resolve(content_manager.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case("SeventySix.esm"))
        || record_editor_id(content_manager, interner)
            .is_none_or(|eid| !eid.eq_ignore_ascii_case(BETTER_TOMORROW_CONTENT_MANAGER_EID))
        || story_manager_field_form_key(content_manager, *b"DCGQ")?.local & 0x00FF_FFFF
            != BETTER_TOMORROW_QUEST_LOCAL
        || story_manager_field_form_key(content_manager, *b"DCGL")?.local & 0x00FF_FFFF
            != BETTER_TOMORROW_REGION_LOCAL
    {
        return None;
    }

    let plugin = content_manager.form_key.plugin;
    let mut node = Record::new(smqn_sig(), content_manager.form_key);
    node.eid = Some(interner.intern("NPE_DQ01_BetterTomorrow_QuestNode"));
    node.fields.push(FieldEntry {
        sig: pnam_sig(),
        value: FieldValue::FormKey(FormKey {
            local: FO4_SCRIPT_EVENT_ROOT_LOCAL,
            plugin,
        }),
    });
    node.fields.push(FieldEntry {
        sig: SubrecordSig(*b"CITC"),
        value: FieldValue::Uint(1),
    });
    node.fields.push(FieldEntry {
        sig: SubrecordSig(*b"CTDA"),
        value: FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
            BETTER_TOMORROW_START_KEYWORD_LOCAL,
        ))),
    });
    for (sig, value) in [(*b"DNAM", 0_u64), (*b"XNAM", 0), (*b"QNAM", 1)] {
        node.fields.push(FieldEntry {
            sig: SubrecordSig(sig),
            value: FieldValue::Uint(value),
        });
    }
    node.fields.push(FieldEntry {
        sig: nnam_sig(),
        value: FieldValue::FormKey(FormKey {
            local: BETTER_TOMORROW_QUEST_LOCAL,
            plugin,
        }),
    });
    Some(node)
}

fn story_manager_field_form_key(record: &Record, sig: [u8; 4]) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == sig)
        .and_then(|entry| first_form_key(&entry.value, record.form_key.plugin))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoryManagerKeywordRole {
    Start,
    Active,
}

fn story_manager_condition_normalization_applies(source: Game, target: Game) -> bool {
    source == Game::Fo76 && target == Game::Fo4
}

pub(crate) fn normalize_story_manager_quest_start_conditions_for_pair(
    source: Game,
    target: Game,
    graph: &mut StoryManagerSourceGraph,
    interner: &StringInterner,
) -> Vec<StoryManagerConditionDiagnostic> {
    if !story_manager_condition_normalization_applies(source, target) {
        return Vec::new();
    }
    normalize_story_manager_quest_start_conditions(graph, interner)
}

pub(crate) fn normalize_story_manager_quest_start_conditions(
    graph: &mut StoryManagerSourceGraph,
    interner: &StringInterner,
) -> Vec<StoryManagerConditionDiagnostic> {
    let alias_keyword_quests = story_manager_alias_keyword_quests(graph, interner);
    let mut diagnostics = Vec::new();
    let mut node_fks = graph
        .nodes
        .iter()
        .filter_map(|(form_key, record)| (record.sig == smqn_sig()).then_some(*form_key))
        .collect::<Vec<_>>();
    node_fks.sort_by_key(|form_key| form_key.local);

    for node_fk in node_fks {
        let Some(node) = graph.nodes.get(&node_fk) else {
            continue;
        };
        if !story_manager_has_scpt_root(node, graph) {
            continue;
        }

        let start_keywords = node
            .fields
            .iter()
            .filter_map(|entry| {
                Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
                    .or_else(|| {
                        has_lite_ally_intro_node_identity(node, interner)
                            .then(|| lite_ally_shared_or_start_keyword(&entry.value))
                            .flatten()
                    })
                    .map(|raw| raw_form_key(raw, node.form_key.plugin))
            })
            .collect::<Vec<_>>();
        if start_keywords.is_empty() {
            continue;
        }
        let condition_rows = node
            .fields
            .iter()
            .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
            .collect::<Vec<_>>();
        let has_unambiguous_untyped_start = start_keywords.len() == 1
            && condition_rows.len() == 1
            && Fo76Fo4Hook::story_manager_start_keyword(&condition_rows[0].value)
                .is_some_and(|raw| raw_form_key(raw, node.form_key.plugin) == start_keywords[0]);
        let valid_start_keywords = start_keywords
            .iter()
            .filter(|keyword| {
                graph.keywords.get(keyword).is_some_and(|record| {
                    match story_manager_keyword_role(record, interner) {
                        Some(StoryManagerKeywordRole::Start) => true,
                        Some(StoryManagerKeywordRole::Active) => false,
                        // A lone K1 gate is an unambiguous start selector even
                        // when its legacy EditorID lacks the usual role suffix.
                        None => has_unambiguous_untyped_start,
                    }
                })
            })
            .copied()
            .collect::<Vec<_>>();
        if start_keywords.len() != 1 || valid_start_keywords.len() != 1 {
            if exact_lite_ally_shared_start_relation(
                graph,
                node_fk,
                &valid_start_keywords,
                &condition_rows,
                interner,
            ) {
                continue;
            }
            diagnostics.push(StoryManagerConditionDiagnostic {
                form_key: node_fk,
                message: format!(
                    "quest_start_unqualified:start_keywords={} valid_start_keywords={}",
                    start_keywords.len(),
                    valid_start_keywords.len()
                ),
            });
            continue;
        }

        let mut active_gates = Vec::new();
        let mut unresolved_active_keyword_roles = false;
        for (index, entry) in node.fields.iter().enumerate() {
            let Some(raw) = Fo76Fo4Hook::story_manager_active_keyword(&entry.value) else {
                continue;
            };
            let keyword = raw_form_key(raw, node.form_key.plugin);
            match graph
                .keywords
                .get(&keyword)
                .and_then(|record| story_manager_keyword_role(record, interner))
            {
                Some(StoryManagerKeywordRole::Active) => active_gates.push((index, keyword)),
                Some(StoryManagerKeywordRole::Start) => {}
                None if graph.keywords.contains_key(&keyword) => {}
                None => {
                    unresolved_active_keyword_roles = true;
                    diagnostics.push(StoryManagerConditionDiagnostic {
                        form_key: node_fk,
                        message: format!("active_keyword_role_unresolved:{:06X}", keyword.local),
                    });
                }
            }
        }
        if unresolved_active_keyword_roles {
            continue;
        }
        if active_gates.is_empty() {
            if exact_tw007_cold_case_self_completion_start_relation(
                graph,
                node_fk,
                valid_start_keywords[0],
                &condition_rows,
                interner,
            ) {
                if let Some(node) = graph.nodes.get_mut(&node_fk) {
                    let changed = node.fields.iter_mut().fold(false, |changed, entry| {
                        Fo76Fo4Hook::lower_story_manager_completion_condition(&mut entry.value)
                            || changed
                    });
                    if changed {
                        node.sync_condition_count();
                    }
                }
                continue;
            }
            if exact_overseer_self_completion_start_relation(
                graph,
                node_fk,
                valid_start_keywords[0],
                &condition_rows,
                interner,
            ) {
                if let Some(node) = graph.nodes.get_mut(&node_fk) {
                    let changed = node.fields.iter_mut().fold(false, |changed, entry| {
                        Fo76Fo4Hook::lower_story_manager_completion_condition(&mut entry.value)
                            || changed
                    });
                    if changed {
                        node.sync_condition_count();
                    }
                }
                continue;
            }
            if (condition_rows.len() == 1
                && Fo76Fo4Hook::story_manager_start_keyword(&condition_rows[0].value).is_some_and(
                    |raw| raw_form_key(raw, node.form_key.plugin) == valid_start_keywords[0],
                ))
                || exact_burn_sq01_radio_start_relation(
                    graph,
                    node_fk,
                    valid_start_keywords[0],
                    &condition_rows,
                    interner,
                )
            {
                continue;
            }
            diagnostics.push(StoryManagerConditionDiagnostic {
                form_key: node_fk,
                message: "quest_start_unqualified:no_active_keyword_role".to_string(),
            });
            continue;
        }
        let nnam_quests = story_manager_quests(node)
            .into_iter()
            .collect::<FxHashSet<_>>();
        let matching_completion_quests = node
            .fields
            .iter()
            .filter_map(|entry| {
                let raw = Fo76Fo4Hook::story_manager_completion_quest(&entry.value)?;
                let quest = raw_form_key(raw, node.form_key.plugin);
                nnam_quests.contains(&quest).then_some(quest)
            })
            .collect::<FxHashSet<_>>();
        let unique_self_quest = (matching_completion_quests.len() == 1)
            .then(|| matching_completion_quests.iter().next().copied())
            .flatten();
        let unresolved_active_gate_count = active_gates
            .iter()
            .filter(|(_, keyword)| !alias_keyword_quests.contains_key(keyword))
            .count();
        let mut active_gate_quests = FxHashMap::default();
        let mut unsafe_active_gate = false;
        for (index, keyword) in active_gates {
            let alias_quests = alias_keyword_quests.get(&keyword);
            let bounded_skyline_relation = exact_skyline_breadcrumb_active_keyword_quest_relation(
                graph,
                node_fk,
                valid_start_keywords[0],
                keyword,
                &nnam_quests,
                interner,
            )
            .or_else(|| {
                exact_burning_springs_outro_p2_active_keyword_quest_relation(
                    graph,
                    node_fk,
                    valid_start_keywords[0],
                    keyword,
                    &nnam_quests,
                    interner,
                )
            })
            .or_else(|| {
                exact_ac_sq05_active_keyword_quest_relation(
                    graph,
                    node_fk,
                    valid_start_keywords[0],
                    keyword,
                    &nnam_quests,
                    interner,
                )
            });
            let quest = match alias_quests.map(FxHashSet::len) {
                Some(1) => alias_quests.and_then(|quests| quests.iter().next().copied()),
                Some(count) => {
                    unsafe_active_gate = true;
                    diagnostics.push(StoryManagerConditionDiagnostic {
                        form_key: node_fk,
                        message: format!(
                            "active_keyword_ambiguous:{:06X}:quest_count={count}",
                            keyword.local
                        ),
                    });
                    None
                }
                None if unresolved_active_gate_count == 1 => {
                    unique_self_quest.or(bounded_skyline_relation)
                }
                None => bounded_skyline_relation,
            };
            if let Some(quest) = quest {
                active_gate_quests.insert(index, quest);
            } else if alias_quests.is_none() {
                unsafe_active_gate = true;
                diagnostics.push(StoryManagerConditionDiagnostic {
                    form_key: node_fk,
                    message: format!("active_keyword_unmapped:{:06X}", keyword.local),
                });
            }
        }
        if unsafe_active_gate || active_gate_quests.is_empty() {
            diagnostics.push(StoryManagerConditionDiagnostic {
                form_key: node_fk,
                message: "quest_start_unqualified:no_unique_quest_relation".to_string(),
            });
            continue;
        }

        let Some(node) = graph.nodes.get_mut(&node_fk) else {
            continue;
        };
        let mut changed = false;
        for (index, entry) in node.fields.iter_mut().enumerate() {
            if let Some(quest) = active_gate_quests.get(&index) {
                changed |= Fo76Fo4Hook::lower_story_manager_active_keyword_condition(
                    &mut entry.value,
                    quest.local,
                );
            }
            changed |= Fo76Fo4Hook::lower_story_manager_completion_condition(&mut entry.value);
        }
        if changed {
            node.sync_condition_count();
        }
    }

    diagnostics
}

fn exact_lite_ally_shared_start_relation(
    graph: &StoryManagerSourceGraph,
    node_fk: FormKey,
    start_keywords: &[FormKey],
    condition_rows: &[&FieldEntry],
    interner: &StringInterner,
) -> bool {
    if interner.resolve(node_fk.plugin) != Some("SeventySix.esm")
        || node_fk.local != LITE_ALLY_INTRO_NODE_LOCAL
        || start_keywords.len() != LITE_ALLY_INTRO_ROUTES.len()
        || condition_rows.len() != LITE_ALLY_INTRO_ROUTES.len()
    {
        return false;
    }

    let Some(node) = graph.nodes.get(&node_fk) else {
        return false;
    };
    let branch_fk = FormKey {
        local: LITE_ALLY_INTRO_BRANCH_LOCAL,
        plugin: node_fk.plugin,
    };
    let previous_fk = FormKey {
        local: LITE_ALLY_INTRO_PREVIOUS_LOCAL,
        plugin: node_fk.plugin,
    };
    let root_fk = FormKey {
        local: FO4_SCRIPT_EVENT_ROOT_LOCAL,
        plugin: node_fk.plugin,
    };
    let Some(branch) = graph.nodes.get(&branch_fk) else {
        return false;
    };
    let Some(root) = graph.nodes.get(&root_fk) else {
        return false;
    };

    if node.sig != smqn_sig()
        || record_editor_id_lower(node, interner).as_deref() != Some("comp_lite_intro_questnodes")
        || story_manager_parent(node) != Some(branch_fk)
        || story_manager_fk(node, snam_sig()) != Some(previous_fk)
        || story_manager_u32(node, *b"DNAM") != Some(LITE_ALLY_INTRO_SHARED_EVENT_FLAGS)
        || story_manager_u32(node, *b"XNAM") != Some(0)
        || story_manager_u32(node, *b"QNAM") != Some(LITE_ALLY_INTRO_ROUTES.len() as u32)
        || branch.sig != smbn_sig()
        || record_editor_id_lower(branch, interner).as_deref() != Some("comp_quests_branch")
        || story_manager_parent(branch) != Some(root_fk)
        || branch
            .fields
            .iter()
            .any(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
        || root.sig != smen_sig()
        || story_manager_event_type(root) != Some(SCPT_EVENT_TYPE)
    {
        return false;
    }

    let expected_quests = LITE_ALLY_INTRO_ROUTES
        .iter()
        .map(|(quest, _, _, _)| FormKey {
            local: *quest,
            plugin: node_fk.plugin,
        })
        .collect::<Vec<_>>();
    if story_manager_quests(node) != expected_quests {
        return false;
    }

    LITE_ALLY_INTRO_ROUTES.iter().enumerate().all(
        |(index, (quest_local, quest_editor_id, keyword_local, keyword_editor_id))| {
            let quest_fk = FormKey {
                local: *quest_local,
                plugin: node_fk.plugin,
            };
            let keyword_fk = FormKey {
                local: *keyword_local,
                plugin: node_fk.plugin,
            };
            let Some(quest) = graph.quests.get(&quest_fk) else {
                return false;
            };
            let Some(keyword) = graph.keywords.get(&keyword_fk) else {
                return false;
            };
            let condition_keyword = condition_rows.get(index).and_then(|entry| {
                Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
                    .or_else(|| lite_ally_shared_or_start_keyword(&entry.value))
                    .map(|raw| raw_form_key(raw, node_fk.plugin))
            });

            start_keywords.get(index) == Some(&keyword_fk)
                && condition_keyword == Some(keyword_fk)
                && record_editor_id_lower(quest, interner).as_deref() == Some(*quest_editor_id)
                && story_manager_event_type(quest) == Some(SCPT_EVENT_TYPE)
                && record_editor_id_lower(keyword, interner).as_deref() == Some(*keyword_editor_id)
                && story_manager_keyword_role(keyword, interner)
                    == Some(StoryManagerKeywordRole::Start)
        },
    )
}

fn exact_lite_ally_intro_node(
    graph: &StoryManagerSourceGraph,
    node_fk: FormKey,
    interner: &StringInterner,
) -> bool {
    let Some(node) = graph.nodes.get(&node_fk) else {
        return false;
    };
    let start_keywords = node
        .fields
        .iter()
        .filter_map(|entry| {
            Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
                .or_else(|| lite_ally_shared_or_start_keyword(&entry.value))
                .map(|raw| raw_form_key(raw, node_fk.plugin))
        })
        .collect::<Vec<_>>();
    let condition_rows = node
        .fields
        .iter()
        .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
        .collect::<Vec<_>>();
    exact_lite_ally_shared_start_relation(
        graph,
        node_fk,
        &start_keywords,
        &condition_rows,
        interner,
    )
}

fn lite_ally_shared_or_start_keyword(value: &FieldValue) -> Option<u32> {
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    if bytes.first() != Some(&0x01) {
        return None;
    }
    let mut normalized = bytes.clone();
    normalized[0] = 0;
    Fo76Fo4Hook::story_manager_start_keyword(&FieldValue::Bytes(normalized))
}

fn has_lite_ally_intro_node_identity(record: &Record, interner: &StringInterner) -> bool {
    has_lite_ally_intro_node_form_key(record, interner)
        && record_editor_id_lower(record, interner).as_deref() == Some("comp_lite_intro_questnodes")
}

fn has_lite_ally_intro_node_form_key(record: &Record, interner: &StringInterner) -> bool {
    record.sig == smqn_sig()
        && interner.resolve(record.form_key.plugin) == Some("SeventySix.esm")
        && record.form_key.local == LITE_ALLY_INTRO_NODE_LOCAL
}

fn exact_burn_sq01_radio_start_relation(
    graph: &StoryManagerSourceGraph,
    node_fk: FormKey,
    start_keyword_fk: FormKey,
    condition_rows: &[&FieldEntry],
    interner: &StringInterner,
) -> bool {
    if interner.resolve(node_fk.plugin) != Some("SeventySix.esm")
        || node_fk.local != BURN_SQ01_RADIO_NODE_LOCAL
        || start_keyword_fk
            != (FormKey {
                local: BURN_SQ01_RADIO_START_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            })
        || condition_rows.len() != 2
    {
        return false;
    }

    let quest_fk = FormKey {
        local: BURN_SQ01_RADIO_QUEST_LOCAL,
        plugin: node_fk.plugin,
    };
    let Some(node) = graph.nodes.get(&node_fk) else {
        return false;
    };
    let Some(start_keyword) = graph.keywords.get(&start_keyword_fk) else {
        return false;
    };
    let Some(quest) = graph.quests.get(&quest_fk) else {
        return false;
    };
    let exact_lcp_gate = condition_rows.iter().any(|entry| {
        matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.as_slice() == BURN_SQ01_RADIO_LCP_CONDITION)
    });

    exact_lcp_gate
        && story_manager_quests(node) == vec![quest_fk]
        && node.sig == smqn_sig()
        && record_editor_id_lower(node, interner).as_deref() == Some("burn_sq01_radio_questnode")
        && record_editor_id_lower(start_keyword, interner).as_deref()
            == Some("burn_sq01_radio_queststartkeyword")
        && quest.sig == SigCode::from_str("QUST").expect("literal sig")
        && record_editor_id_lower(quest, interner).as_deref() == Some("burn_sq01_radio")
}

fn exact_overseer_self_completion_start_relation(
    graph: &StoryManagerSourceGraph,
    node_fk: FormKey,
    start_keyword_fk: FormKey,
    condition_rows: &[&FieldEntry],
    interner: &StringInterner,
) -> bool {
    let (quest_local, start_keyword_local, node_eid, quest_eid, start_keyword_eid) =
        match node_fk.local {
            MQ_OVERSEER_NODE_LOCAL => (
                MQ_OVERSEER_QUEST_LOCAL,
                MQ_OVERSEER_START_KEYWORD_LOCAL,
                "mqoverseernode",
                "mq_overseer",
                "mq_overseer_queststartkeyword",
            ),
            OVERSEER_PERSONAL_NODE_LOCAL => (
                OVERSEER_PERSONAL_QUEST_LOCAL,
                OVERSEER_PERSONAL_START_KEYWORD_LOCAL,
                "overseerpersonalnode",
                "overseerpersonal",
                "overseerpersonal_queststartkeyword",
            ),
            _ => return false,
        };
    if interner.resolve(node_fk.plugin) != Some("SeventySix.esm")
        || start_keyword_fk
            != (FormKey {
                local: start_keyword_local,
                plugin: node_fk.plugin,
            })
        || condition_rows.len() != 2
    {
        return false;
    }

    let quest_fk = FormKey {
        local: quest_local,
        plugin: node_fk.plugin,
    };
    let Some(node) = graph.nodes.get(&node_fk) else {
        return false;
    };
    let Some(quest) = graph.quests.get(&quest_fk) else {
        return false;
    };
    let Some(start_keyword) = graph.keywords.get(&start_keyword_fk) else {
        return false;
    };
    let completion_condition_count = condition_rows
        .iter()
        .filter(|entry| {
            Fo76Fo4Hook::story_manager_completion_quest(&entry.value) == Some(quest_local)
        })
        .count();
    let start_condition_count = condition_rows
        .iter()
        .filter(|entry| {
            Fo76Fo4Hook::story_manager_start_keyword(&entry.value) == Some(start_keyword_local)
        })
        .count();

    node.sig == smqn_sig()
        && story_manager_quests(node) == vec![quest_fk]
        && record_editor_id_lower(node, interner).as_deref() == Some(node_eid)
        && quest.sig == SigCode::from_str("QUST").expect("literal sig")
        && record_editor_id_lower(quest, interner).as_deref() == Some(quest_eid)
        && start_keyword.sig == SigCode::from_str("KYWD").expect("literal sig")
        && record_editor_id_lower(start_keyword, interner).as_deref() == Some(start_keyword_eid)
        && completion_condition_count == 1
        && start_condition_count == 1
}

fn exact_tw007_cold_case_self_completion_start_relation(
    graph: &StoryManagerSourceGraph,
    node_fk: FormKey,
    start_keyword_fk: FormKey,
    condition_rows: &[&FieldEntry],
    interner: &StringInterner,
) -> bool {
    if interner.resolve(node_fk.plugin) != Some("SeventySix.esm")
        || node_fk.local != TW007_COLD_CASE_NODE_LOCAL
        || start_keyword_fk
            != (FormKey {
                local: TW007_COLD_CASE_START_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            })
        || condition_rows.len() != 2
    {
        return false;
    }

    let quest_fk = FormKey {
        local: TW007_COLD_CASE_QUEST_LOCAL,
        plugin: node_fk.plugin,
    };
    let Some(node) = graph.nodes.get(&node_fk) else {
        return false;
    };
    let Some(quest) = graph.quests.get(&quest_fk) else {
        return false;
    };
    let Some(start_keyword) = graph.keywords.get(&start_keyword_fk) else {
        return false;
    };
    let exact_completion_condition_count = condition_rows
        .iter()
        .filter(|entry| {
            matches!(
                &entry.value,
                FieldValue::Bytes(bytes)
                    if bytes.as_slice() == TW007_COLD_CASE_COMPLETION_CONDITION
            )
        })
        .count();
    let exact_start_condition_count = condition_rows
        .iter()
        .filter(|entry| {
            matches!(
                &entry.value,
                FieldValue::Bytes(bytes) if bytes.as_slice() == TW007_COLD_CASE_START_CONDITION
            )
        })
        .count();

    node.sig == smqn_sig()
        && story_manager_fk(node, pnam_sig())
            == Some(FormKey {
                local: TW007_COLD_CASE_BRANCH_LOCAL,
                plugin: node_fk.plugin,
            })
        && story_manager_quests(node) == vec![quest_fk]
        && record_editor_id_lower(node, interner).as_deref() == Some("tw007_coldcase_questnode")
        && quest.sig == SigCode::from_str("QUST").expect("literal sig")
        && record_editor_id_lower(quest, interner).as_deref() == Some("tw007_coldcase")
        && start_keyword.sig == SigCode::from_str("KYWD").expect("literal sig")
        && record_editor_id_lower(start_keyword, interner).as_deref()
            == Some("tw007_queststartkeyword")
        && exact_completion_condition_count == 1
        && exact_start_condition_count == 1
}

fn exact_skyline_breadcrumb_active_keyword_quest_relation(
    graph: &StoryManagerSourceGraph,
    node_fk: FormKey,
    start_keyword_fk: FormKey,
    active_keyword_fk: FormKey,
    node_quests: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> Option<FormKey> {
    if interner.resolve(node_fk.plugin) != Some("SeventySix.esm")
        || node_fk.local != SKYLINE_BREADCRUMB_NODE_LOCAL
        || start_keyword_fk
            != (FormKey {
                local: SKYLINE_BREADCRUMB_START_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            })
        || active_keyword_fk
            != (FormKey {
                local: SKYLINE_BREADCRUMB_ACTIVE_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            })
    {
        return None;
    }

    let quest_fk = FormKey {
        local: SKYLINE_BREADCRUMB_QUEST_LOCAL,
        plugin: node_fk.plugin,
    };
    if !node_quests.contains(&quest_fk) {
        return None;
    }

    let node = graph.nodes.get(&node_fk)?;
    let start_keyword = graph.keywords.get(&start_keyword_fk)?;
    let active_keyword = graph.keywords.get(&active_keyword_fk)?;
    let quest = graph.quests.get(&quest_fk)?;
    (node.sig == smqn_sig()
        && record_editor_id_lower(node, interner).as_deref()
            == Some("storm_mq01_breadcrumb_questnode")
        && record_editor_id_lower(start_keyword, interner).as_deref()
            == Some("storm_mq01_breadcrumb_startkeyword")
        && record_editor_id_lower(active_keyword, interner).as_deref()
            == Some("storm_mq00_breadcrumb_activekeyword")
        && quest.sig == SigCode::from_str("QUST").expect("literal sig")
        && record_editor_id_lower(quest, interner).as_deref() == Some("storm_mq01_breadcrumb"))
    .then_some(quest_fk)
}

fn exact_burning_springs_outro_p2_active_keyword_quest_relation(
    graph: &StoryManagerSourceGraph,
    node_fk: FormKey,
    start_keyword_fk: FormKey,
    active_keyword_fk: FormKey,
    node_quests: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> Option<FormKey> {
    if interner.resolve(node_fk.plugin) != Some("SeventySix.esm")
        || node_fk.local != BURN_OUTRO_P2_NODE_LOCAL
        || start_keyword_fk
            != (FormKey {
                local: BURN_OUTRO_P2_START_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            })
        || active_keyword_fk
            != (FormKey {
                local: BURN_OUTRO_P2_ACTIVE_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            })
    {
        return None;
    }

    let quest_fk = FormKey {
        local: BURN_OUTRO_P2_QUEST_LOCAL,
        plugin: node_fk.plugin,
    };
    if !node_quests.contains(&quest_fk) {
        return None;
    }

    let node = graph.nodes.get(&node_fk)?;
    let start_keyword = graph.keywords.get(&start_keyword_fk)?;
    let active_keyword = graph.keywords.get(&active_keyword_fk)?;
    let quest = graph.quests.get(&quest_fk)?;
    (node.sig == smqn_sig()
        && record_editor_id_lower(node, interner).as_deref() == Some("burn_sq02_outrop2_questnode")
        && record_editor_id_lower(start_keyword, interner).as_deref()
            == Some("burn_sq02_outrop2_queststartkeyword")
        && record_editor_id_lower(active_keyword, interner).as_deref()
            == Some("burn_sq02_outrop2_questactivekeyword")
        && quest.sig == SigCode::from_str("QUST").expect("literal sig")
        && record_editor_id_lower(quest, interner).as_deref() == Some("burn_sq02_outrop2"))
    .then_some(quest_fk)
}

fn exact_ac_sq05_active_keyword_quest_relation(
    graph: &StoryManagerSourceGraph,
    node_fk: FormKey,
    start_keyword_fk: FormKey,
    active_keyword_fk: FormKey,
    node_quests: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> Option<FormKey> {
    if interner.resolve(node_fk.plugin) != Some("SeventySix.esm")
        || node_fk.local != AC_SQ05_NODE_LOCAL
        || start_keyword_fk
            != (FormKey {
                local: AC_SQ05_START_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            })
        || active_keyword_fk
            != (FormKey {
                local: AC_SQ05_ACTIVE_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            })
    {
        return None;
    }

    let quest_fk = FormKey {
        local: AC_SQ05_QUEST_LOCAL,
        plugin: node_fk.plugin,
    };
    if node_quests.len() != 1 || !node_quests.contains(&quest_fk) {
        return None;
    }

    let node = graph.nodes.get(&node_fk)?;
    let start_keyword = graph.keywords.get(&start_keyword_fk)?;
    let active_keyword = graph.keywords.get(&active_keyword_fk)?;
    let quest = graph.quests.get(&quest_fk)?;
    (node.sig == smqn_sig()
        && story_manager_fk(node, pnam_sig()).is_some_and(|parent| {
            parent
                == FormKey {
                    local: AC_SQ05_BRANCH_LOCAL,
                    plugin: node_fk.plugin,
                }
        })
        && story_manager_fk(node, snam_sig()).is_some_and(|previous| {
            previous
                == FormKey {
                    local: AC_SQ04_NODE_LOCAL,
                    plugin: node_fk.plugin,
                }
        })
        && record_editor_id_lower(node, interner).as_deref() == Some("ac_sq05_regent_questnote")
        && record_editor_id_lower(start_keyword, interner).as_deref()
            == Some("ac_sq05_regent_startkeyword")
        && record_editor_id_lower(active_keyword, interner).as_deref()
            == Some("ac_sq05_regent_questactivekeyword")
        && quest.sig == SigCode::from_str("QUST").expect("literal sig")
        && record_editor_id_lower(quest, interner).as_deref() == Some("ac_sq05_regent"))
    .then_some(quest_fk)
}

fn story_manager_has_scpt_root(node: &Record, graph: &StoryManagerSourceGraph) -> bool {
    safe_parent_chain(node, graph)
        .ok()
        .and_then(|chain| chain.last().and_then(|root| graph.nodes.get(root)))
        .and_then(story_manager_event_type)
        == Some(SCPT_EVENT_TYPE)
}

fn story_manager_alias_keyword_quests(
    graph: &StoryManagerSourceGraph,
    interner: &StringInterner,
) -> FxHashMap<FormKey, FxHashSet<FormKey>> {
    let mut associations: FxHashMap<FormKey, FxHashSet<FormKey>> = FxHashMap::default();
    for (quest_fk, quest) in &graph.quests {
        for entry in &quest.fields {
            if entry.sig.0 != *b"KWDA" {
                continue;
            }
            let mut keywords = Vec::new();
            collect_form_keys(&entry.value, quest.form_key.plugin, &mut keywords);
            for keyword in keywords {
                if graph.keywords.get(&keyword).is_some_and(|record| {
                    story_manager_keyword_role(record, interner)
                        == Some(StoryManagerKeywordRole::Active)
                }) {
                    associations.entry(keyword).or_default().insert(*quest_fk);
                }
            }
        }
    }
    associations
}

fn story_manager_keyword_role(
    record: &Record,
    interner: &StringInterner,
) -> Option<StoryManagerKeywordRole> {
    let editor_id = record_editor_id_lower(record, interner)?;
    let compact = editor_id.replace(['_', '-', ' '], "");
    let has_role_token = |expected| {
        editor_id
            .split(['_', '-', ' '])
            .any(|token| token == expected)
    };
    if compact.contains("startkeyword")
        || has_role_token("queststart")
        || compact.ends_with("startquestkeyword")
        || compact == "workshopvertibirdgrenadekw"
    {
        Some(StoryManagerKeywordRole::Start)
    } else if compact.contains("activekeyword") || has_role_token("questactive") {
        Some(StoryManagerKeywordRole::Active)
    } else {
        None
    }
}

fn collect_form_keys(
    value: &FieldValue,
    fallback_plugin: crate::sym::Sym,
    output: &mut Vec<FormKey>,
) {
    match value {
        FieldValue::FormKey(form_key) => output.push(*form_key),
        FieldValue::Uint(raw) => {
            if let Ok(raw) = u32::try_from(*raw) {
                output.push(raw_form_key(raw, fallback_plugin));
            }
        }
        FieldValue::Int(raw) => {
            if let Ok(raw) = u32::try_from(*raw) {
                output.push(raw_form_key(raw, fallback_plugin));
            }
        }
        FieldValue::Bytes(bytes) => {
            for raw in bytes
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            {
                if raw != 0 {
                    output.push(raw_form_key(raw, fallback_plugin));
                }
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_form_keys(value, fallback_plugin, output);
            }
        }
        FieldValue::List(values) => {
            for value in values {
                collect_form_keys(value, fallback_plugin, output);
            }
        }
        _ => {}
    }
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
        let exact_lite_ally_node = exact_lite_ally_intro_node(graph, *smqn_fk, interner);
        if has_lite_ally_intro_node_form_key(smqn, interner) && !exact_lite_ally_node {
            push_skip(
                &mut selection,
                *smqn_fk,
                StoryManagerSkipReason::UnsupportedQuest,
                "lite ally shared start relation does not match its exact source contract",
            );
            continue;
        }
        if exact_lite_ally_node
            && !quest_fks.iter().all(|quest_fk| {
                translated_quests.contains(quest_fk)
                    && graph
                        .quests
                        .get(quest_fk)
                        .is_some_and(|quest| classify_quest(quest, interner).is_some())
            })
        {
            push_skip(
                &mut selection,
                *smqn_fk,
                StoryManagerSkipReason::QuestNotTranslated,
                "lite ally shared start relation requires all five exact quests",
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

pub(crate) fn build_story_manager_route_seed_for_pair(
    source: Game,
    target: Game,
    graph: &StoryManagerSourceGraph,
    selection: &StoryManagerSelection,
    source_to_target: &FxHashMap<FormKey, FormKey>,
    target_records: &FxHashMap<FormKey, Record>,
    emitted_nodes: &FxHashSet<FormKey>,
    bridge_roots: &FxHashSet<FormKey>,
    output_plugin: crate::sym::Sym,
    interner: &StringInterner,
) -> StoryManagerRouteSeedReport {
    let mut report = StoryManagerRouteSeedReport::empty(source, target);
    if source != Game::Fo76 || target != Game::Fo4 {
        return report;
    }

    let mut nodes = graph
        .nodes
        .iter()
        .filter(|(_, record)| record.sig == smqn_sig())
        .collect::<Vec<_>>();
    nodes.sort_by_key(|(form_key, _)| form_key.format(interner));
    for (node_fk, node) in nodes {
        for quest_fk in story_manager_quests(node) {
            if !source_to_target.contains_key(&quest_fk) {
                continue;
            }
            report.routes.push(build_story_manager_route_seed(
                quest_fk,
                Some(*node_fk),
                graph.quests.get(&quest_fk),
                graph,
                selection,
                source_to_target,
                target_records,
                emitted_nodes,
                bridge_roots,
                output_plugin,
                interner,
            ));
        }
    }

    let mut orphan_quests = graph.event_metadata_quests.iter().collect::<Vec<_>>();
    orphan_quests.sort_by_key(|(form_key, _)| form_key.format(interner));
    for (quest_fk, quest) in orphan_quests {
        if source_to_target.contains_key(quest_fk) {
            report.routes.push(build_story_manager_route_seed(
                *quest_fk,
                None,
                Some(quest),
                graph,
                selection,
                source_to_target,
                target_records,
                emitted_nodes,
                bridge_roots,
                output_plugin,
                interner,
            ));
        }
    }

    report.routes.sort_by(|left, right| {
        (
            &left.source_quest,
            &left.source_node,
            &left.source_root_event,
            &left.route_id,
        )
            .cmp(&(
                &right.source_quest,
                &right.source_node,
                &right.source_root_event,
                &right.route_id,
            ))
    });
    report
}

pub(crate) fn story_manager_route_source_form_keys(
    graph: &StoryManagerSourceGraph,
) -> FxHashSet<FormKey> {
    let mut form_keys = graph.nodes.keys().copied().collect::<FxHashSet<_>>();
    form_keys.extend(graph.quests.keys().copied());
    form_keys.extend(graph.event_metadata_quests.keys().copied());
    for record in graph
        .nodes
        .values()
        .filter(|record| record.sig == smqn_sig())
    {
        form_keys.extend(story_manager_quests(record));
    }
    form_keys
}

#[allow(clippy::too_many_arguments)]
fn build_story_manager_route_seed(
    source_quest_fk: FormKey,
    source_node_fk: Option<FormKey>,
    source_quest: Option<&Record>,
    graph: &StoryManagerSourceGraph,
    selection: &StoryManagerSelection,
    source_to_target: &FxHashMap<FormKey, FormKey>,
    target_records: &FxHashMap<FormKey, Record>,
    emitted_nodes: &FxHashSet<FormKey>,
    bridge_roots: &FxHashSet<FormKey>,
    output_plugin: crate::sym::Sym,
    interner: &StringInterner,
) -> StoryManagerRouteSeed {
    let target_quest_fk = source_to_target.get(&source_quest_fk).copied();
    let target_quest = target_quest_fk.and_then(|form_key| target_records.get(&form_key));
    let source_chain = source_node_fk
        .and_then(|form_key| graph.nodes.get(&form_key))
        .and_then(|node| safe_parent_chain(node, graph).ok());
    let source_root_fk = source_chain
        .as_ref()
        .and_then(|chain| chain.last())
        .copied();
    let target_node_fk =
        source_node_fk.and_then(|form_key| source_to_target.get(&form_key).copied());
    let target_root_fk =
        source_root_fk.and_then(|form_key| source_to_target.get(&form_key).copied());
    let is_bridge = source_root_fk.is_some_and(|form_key| bridge_roots.contains(&form_key));
    let target_event_root_fk = if is_bridge {
        target_root_fk
            .and_then(|form_key| target_records.get(&form_key))
            .and_then(story_manager_parent)
    } else {
        target_root_fk
    };
    let source_quest_event = source_quest.and_then(story_manager_event_type);
    let target_quest_event = target_quest.and_then(story_manager_event_type);
    let source_root_event = source_root_fk
        .and_then(|form_key| graph.nodes.get(&form_key))
        .and_then(story_manager_event_type);
    let target_event = target_root_fk.and_then(|_| {
        if is_bridge {
            Some(SCPT_EVENT_TYPE)
        } else {
            source_root_event.filter(|event| is_fo4_valid_sm_event(*event))
        }
    });
    let source_selector_keywords = route_selector_keywords(
        source_node_fk,
        source_chain.as_deref(),
        source_quest,
        &graph.nodes,
        interner,
    );
    let target_selector_keywords = route_target_selector_keywords(
        source_node_fk,
        source_chain.as_deref(),
        target_quest,
        source_to_target,
        target_records,
        interner,
    );
    let bridge_keyword_fk = is_bridge.then(|| FormKey {
        local: source_root_fk
            .expect("bridge route has a source root")
            .local,
        plugin: output_plugin,
    });

    let mut issues = Vec::new();
    let mut skip_reason = None;
    let selected = source_node_fk.is_some_and(|node| {
        selection
            .selected_quests_by_node
            .get(&node)
            .is_some_and(|quests| quests.contains(&source_quest_fk))
    });
    let emission_status = if source_node_fk.is_none() {
        issues.push("orphan_no_story_manager_node".to_string());
        "orphan"
    } else if !selected {
        let reason = story_manager_route_skip_reason(
            source_node_fk.expect("referenced route has a source node"),
            source_quest,
            selection,
            interner,
        );
        skip_reason = Some(reason.clone());
        issues.push(format!("not_selected:{reason}"));
        "skipped"
    } else if !emitted_nodes.contains(&source_node_fk.expect("selected route has a source node")) {
        skip_reason = Some("node_not_emitted".to_string());
        issues.push("node_not_emitted".to_string());
        "skipped"
    } else if target_node_fk.is_none_or(|form_key| !target_records.contains_key(&form_key)) {
        skip_reason = Some("target_node_missing".to_string());
        issues.push("target_node_missing".to_string());
        "skipped"
    } else if target_quest_fk.is_none_or(|form_key| !target_records.contains_key(&form_key)) {
        skip_reason = Some("target_quest_missing".to_string());
        issues.push("target_quest_missing".to_string());
        "skipped"
    } else {
        "emitted"
    };

    if let Some(node_fk) = source_node_fk {
        for diagnostic in graph
            .condition_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.form_key == node_fk)
        {
            issues.push(format!("condition_normalization:{}", diagnostic.message));
        }
        if source_chain.is_none() {
            issues.push("invalid_parent_chain".to_string());
        }
        if source_root_fk.is_some() && target_root_fk.is_none() {
            issues.push("target_root_missing".to_string());
        }
        if is_bridge && target_event_root_fk.is_none() {
            issues.push("target_event_root_missing".to_string());
        }
    }
    let has_exact_shared_lite_ally_selectors =
        source_node_fk.is_some_and(|node_fk| exact_lite_ally_intro_node(graph, node_fk, interner));
    if target_selector_keywords.len() > 1 && !has_exact_shared_lite_ally_selectors {
        issues.push("conflicting_mandatory_k1_selectors".to_string());
    }
    if target_event == Some(SCPT_EVENT_TYPE) && target_selector_keywords.is_empty() {
        issues.push("script_event_missing_mandatory_k1_selector".to_string());
    }
    issues.sort();
    issues.dedup();

    let condition_valid = issues.is_empty();
    let structural_status = match emission_status {
        "orphan" => "orphan",
        "skipped" => "not_emitted",
        _ if condition_valid => "eligible",
        _ => "invalid",
    };
    let producer_status = match target_event {
        Some(SCPT_EVENT_TYPE) => "unproven",
        Some(_) => "not_required",
        None => "not_applicable",
    };
    let source_quest_text = source_quest_fk.format(interner);
    let source_node_text = source_node_fk.map(|form_key| form_key.format(interner));
    let source_root_event_text = source_root_event.map(event_type_name);
    let route_event_text = source_root_event_text
        .clone()
        .or_else(|| source_quest_event.map(event_type_name))
        .unwrap_or_else(|| "none".to_string());
    let route_id = format!(
        "{}|{}|{}",
        source_quest_text,
        source_node_text.as_deref().unwrap_or("orphan"),
        route_event_text
    );

    StoryManagerRouteSeed {
        route_id,
        source_quest: source_quest_text,
        target_quest: target_quest_fk.map(|form_key| form_key.format(interner)),
        quest_editor_id: source_quest.and_then(|record| record_editor_id(record, interner)),
        source_node: source_node_text,
        target_node: target_node_fk.map(|form_key| form_key.format(interner)),
        source_root: source_root_fk.map(|form_key| form_key.format(interner)),
        target_root: target_root_fk.map(|form_key| form_key.format(interner)),
        target_event_root: target_event_root_fk.map(|form_key| form_key.format(interner)),
        source_quest_event: source_quest_event.map(event_type_name),
        target_quest_event: target_quest_event.map(event_type_name),
        source_root_event: source_root_event_text,
        target_event: target_event.map(event_type_name),
        source_start_keyword: source_quest
            .and_then(quest_start_keyword)
            .map(|form_key| form_key.format(interner)),
        source_metadata_fields: source_quest.map_or_else(Vec::new, quest_event_metadata_fields),
        source_selector_keywords,
        target_selector_keywords,
        bridge_keyword: bridge_keyword_fk.map(|form_key| form_key.format(interner)),
        emission_status: emission_status.to_string(),
        skip_reason,
        condition_valid,
        structural_status: structural_status.to_string(),
        producer_status: producer_status.to_string(),
        fo76_only_event: source_root_event
            .is_some_and(|event| Fo76Fo4Hook::fo76_only_qust_event_types().contains(&event))
            || source_quest_event
                .is_some_and(|event| Fo76Fo4Hook::fo76_only_qust_event_types().contains(&event)),
        issues,
    }
}

fn story_manager_route_skip_reason(
    node_fk: FormKey,
    source_quest: Option<&Record>,
    selection: &StoryManagerSelection,
    interner: &StringInterner,
) -> String {
    if source_quest.is_none() {
        return StoryManagerSkipReason::MissingQuestRecord
            .as_str()
            .to_string();
    }
    if source_quest.is_some_and(|record| classify_quest(record, interner).is_none()) {
        return StoryManagerSkipReason::UnsupportedQuest
            .as_str()
            .to_string();
    }
    selection
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.form_key == node_fk && diagnostic.kind == StoryManagerDiagnosticKind::Skipped
        })
        .and_then(|diagnostic| diagnostic.reason)
        .map_or_else(
            || "not_selected".to_string(),
            |reason| reason.as_str().to_string(),
        )
}

fn route_selector_keywords(
    source_node_fk: Option<FormKey>,
    source_chain: Option<&[FormKey]>,
    source_quest: Option<&Record>,
    records: &FxHashMap<FormKey, Record>,
    interner: &StringInterner,
) -> Vec<String> {
    let mut selectors = Vec::new();
    if let Some(quest) = source_quest {
        collect_story_manager_start_keywords(quest, &mut selectors, interner);
    }
    for form_key in source_node_fk
        .into_iter()
        .chain(source_chain.into_iter().flatten().copied())
    {
        let Some(record) = records.get(&form_key) else {
            continue;
        };
        collect_story_manager_start_keywords(record, &mut selectors, interner);
    }
    formatted_form_keys(selectors, interner)
}

fn route_target_selector_keywords(
    source_node_fk: Option<FormKey>,
    source_chain: Option<&[FormKey]>,
    target_quest: Option<&Record>,
    source_to_target: &FxHashMap<FormKey, FormKey>,
    target_records: &FxHashMap<FormKey, Record>,
    interner: &StringInterner,
) -> Vec<String> {
    let mut selectors = Vec::new();
    if let Some(quest) = target_quest {
        collect_story_manager_start_keywords(quest, &mut selectors, interner);
    }
    for source_form_key in source_node_fk
        .into_iter()
        .chain(source_chain.into_iter().flatten().copied())
    {
        let Some(target_form_key) = source_to_target.get(&source_form_key) else {
            continue;
        };
        let Some(record) = target_records.get(target_form_key) else {
            continue;
        };
        collect_story_manager_start_keywords(record, &mut selectors, interner);
    }
    formatted_form_keys(selectors, interner)
}

fn collect_story_manager_start_keywords(
    record: &Record,
    output: &mut Vec<FormKey>,
    interner: &StringInterner,
) {
    output.extend(record.fields.iter().filter_map(|entry| {
        Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
            .or_else(|| {
                has_lite_ally_intro_node_identity(record, interner)
                    .then(|| lite_ally_shared_or_start_keyword(&entry.value))
                    .flatten()
            })
            .map(|raw| raw_form_key(raw, record.form_key.plugin))
    }));
}

fn formatted_form_keys(form_keys: Vec<FormKey>, interner: &StringInterner) -> Vec<String> {
    let mut form_keys = form_keys
        .into_iter()
        .map(|form_key| form_key.format(interner))
        .collect::<Vec<_>>();
    form_keys.sort();
    form_keys.dedup();
    form_keys
}

fn quest_event_metadata_fields(record: &Record) -> Vec<String> {
    const EVENT_METADATA_SIGS: [[u8; 4]; 6] =
        [*b"ENAM", *b"QETL", *b"QSSK", *b"QUCF", *b"QSDD", *b"QETE"];
    EVENT_METADATA_SIGS
        .iter()
        .filter(|sig| record.fields.iter().any(|entry| entry.sig.0 == **sig))
        .map(|sig| String::from_utf8_lossy(sig).into_owned())
        .collect()
}

fn quest_has_event_metadata(record: &Record) -> bool {
    !quest_event_metadata_fields(record).is_empty()
}

fn quest_start_keyword(record: &Record) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"QSSK")
        .and_then(|entry| first_form_key(&entry.value, record.form_key.plugin))
        .filter(|form_key| form_key.local != 0)
}

fn record_editor_id(record: &Record, interner: &StringInterner) -> Option<String> {
    if let Some(editor_id) = record.eid.and_then(|symbol| interner.resolve(symbol)) {
        return Some(editor_id.to_string());
    }
    record.fields.iter().find_map(|entry| {
        (entry.sig.0 == *b"EDID").then(|| match &entry.value {
            FieldValue::String(symbol) => interner.resolve(*symbol).map(str::to_string),
            FieldValue::Bytes(bytes) => {
                let end = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(bytes.len());
                std::str::from_utf8(&bytes[..end]).ok().map(str::to_string)
            }
            _ => None,
        })?
    })
}

fn event_type_name(event_type: u32) -> String {
    String::from_utf8_lossy(&event_type.to_le_bytes()).into_owned()
}

pub(crate) fn sanitize_story_manager_previous_node(
    record: &mut Record,
    selected_nodes: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    let surviving_previous =
        exact_ac_sq05_surviving_previous_node(record, selected_nodes, interner);
    let mut changed = false;
    for entry in &mut record.fields {
        if entry.sig != snam_sig() {
            continue;
        }
        if let Some(previous) = first_form_key(&entry.value, record.form_key.plugin)
            && previous.local != 0
            && !selected_nodes.contains(&previous)
        {
            let (replacement, warning) = if let Some(replacement) = surviving_previous {
                (
                    FieldValue::FormKey(replacement),
                    "story_manager_previous_node_rechained",
                )
            } else {
                (
                    FieldValue::Bytes(SmallVec::from_slice(&0u32.to_le_bytes())),
                    "story_manager_previous_node_nulled",
                )
            };
            entry.value = replacement;
            let warning = interner.intern(warning);
            record.warnings.push(warning);
            changed = true;
        }
    }
    changed
}

fn exact_ac_sq05_surviving_previous_node(
    record: &Record,
    selected_nodes: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> Option<FormKey> {
    if interner.resolve(record.form_key.plugin) != Some("SeventySix.esm")
        || record.form_key.local != AC_SQ05_NODE_LOCAL
        || record.sig != smqn_sig()
        || record_editor_id_lower(record, interner).as_deref() != Some("ac_sq05_regent_questnote")
        || story_manager_fk(record, pnam_sig())
            != Some(FormKey {
                local: AC_SQ05_BRANCH_LOCAL,
                plugin: record.form_key.plugin,
            })
        || story_manager_fk(record, snam_sig())
            != Some(FormKey {
                local: AC_SQ04_NODE_LOCAL,
                plugin: record.form_key.plugin,
            })
        || story_manager_quests(record)
            != vec![FormKey {
                local: AC_SQ05_QUEST_LOCAL,
                plugin: record.form_key.plugin,
            }]
        || !selected_nodes.contains(&record.form_key)
    {
        return None;
    }

    let predecessor = FormKey {
        local: AC_SQ03_NODE_LOCAL,
        plugin: record.form_key.plugin,
    };
    selected_nodes.contains(&predecessor).then_some(predecessor)
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

pub(crate) fn clear_qust_autostart_for_pair(
    source: Game,
    target: Game,
    record: &mut Record,
    interner: &StringInterner,
) -> bool {
    if source != Game::Fo76 || target != Game::Fo4 {
        return false;
    }
    clear_qust_autostart(record, interner)
}

fn clear_qust_autostart(record: &mut Record, interner: &StringInterner) -> bool {
    if record.sig != SigCode::from_str("QUST").expect("literal sig") {
        return false;
    }
    for entry in &mut record.fields {
        if entry.sig.0 != *b"DNAM" {
            continue;
        }
        return clear_qust_autostart_value(&mut entry.value, interner);
    }
    false
}

fn clear_qust_autostart_value(value: &mut FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            let old = u16::from_le_bytes([bytes[0], bytes[1]]);
            let flags = old & !QUST_FLAG_START_GAME_ENABLED;
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
            let Some(old) = field_value_u16(flags) else {
                return false;
            };
            let new = old & !QUST_FLAG_START_GAME_ENABLED;
            write_u16_value(flags, new);
            old != new
        }
        _ => false,
    }
}

/// Clearing a quest's start-game-enabled bit hands responsibility for starting it
/// to its Story Manager node. A node whose start condition failed normalization
/// (any `condition_diagnostics` row — the same rows that drive the route report's
/// `condition_valid`) can never select the quest at runtime, so clearing on its
/// behalf strands the quest permanently: it never starts, its aliases never fill.
/// FO76's always-on radio-station quests (`SQ_RadioAppalachia`,
/// `SQ_RadioClassical`, `SQ_RadioPirate`) also keep their source autostart bit.
/// Their emitted SCPT nodes have no runtime producer, so treating node emission
/// alone as ownership strands the station controllers before their scenes bind.
pub(crate) fn emitted_story_manager_quests_requiring_autostart_clear(
    selection: &StoryManagerSelection,
    emitted_nodes: &FxHashSet<FormKey>,
    graph: &StoryManagerSourceGraph,
) -> Vec<FormKey> {
    let fallback_quests = selection
        .fallback_dialogue_quests
        .iter()
        .copied()
        .collect::<FxHashSet<_>>();
    let condition_invalid_nodes = graph
        .condition_diagnostics
        .iter()
        .map(|diagnostic| diagnostic.form_key)
        .collect::<FxHashSet<_>>();
    let mut quests = selection
        .selected_quests_by_node
        .iter()
        .filter(|(node, _)| emitted_nodes.contains(node) && !condition_invalid_nodes.contains(node))
        .flat_map(|(_, quests)| quests.iter().copied())
        .filter(|quest| !fallback_quests.contains(quest))
        .filter(|quest| !ALWAYS_ON_RADIO_STATION_QUESTS.contains(&(quest.local & 0x00FF_FFFF)))
        .collect::<FxHashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    quests.sort_by_key(|quest| quest.local);
    quests
}

pub(crate) fn allows_passive_dialogue_autostart_restore(
    source_fk: FormKey,
    story_manager_owned_quests: &FxHashSet<FormKey>,
) -> bool {
    !story_manager_owned_quests.contains(&source_fk)
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
    // FO4 has no native PCON event producer. This one-stage connector is safe
    // to run at game load because its Papyrus fragment restores the source
    // level gate and forwards the real quest's Story Manager keyword.
    if qust_uses_player_connect_autostart_fallback(interner, record) {
        return None;
    }
    // Never make a source event path runnable when its alias fill uses an event
    // type FO4 cannot resolve. Compatible event fills survive translation.
    if qust_eid_is_test_or_dev(record, interner)
        || (qust_has_untranslatable_event_alias_for_source(interner, record)
            && !qust_has_single_player_event_alias_adapter(record, interner))
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

fn qust_has_single_player_event_alias_adapter(record: &Record, interner: &StringInterner) -> bool {
    if interner.resolve(record.form_key.plugin) != Some("SeventySix.esm") {
        return false;
    }
    let Some(editor_id) = quest_editor_id_lower(record, interner) else {
        return false;
    };
    SINGLE_PLAYER_EVENT_ALIAS_ADAPTED_QUESTS
        .iter()
        .chain(BECKETT_SPECIFIC_ALIAS_QUESTS.iter())
        .any(|(local, expected)| record.form_key.local == *local && editor_id == *expected)
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
    record_editor_id_lower(record, interner)
}

fn record_editor_id_lower(record: &Record, interner: &StringInterner) -> Option<String> {
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

fn story_manager_u32(record: &Record, sig: [u8; 4]) -> Option<u32> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == sig)
        .and_then(|entry| field_value_u32(&entry.value))
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

pub(crate) fn unproven_story_manager_event_roots(
    selected_nodes: &FxHashSet<FormKey>,
    graph: &StoryManagerSourceGraph,
) -> Vec<(FormKey, u32)> {
    let mut roots = selected_nodes
        .iter()
        .filter_map(|form_key| {
            let record = graph.nodes.get(form_key)?;
            let event_type = story_manager_event_type(record)?;
            (record.sig == smen_sig() && event_type == fourcc(b"KILL"))
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
    interner: &StringInterner,
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
        if source_event == SCPT_EVENT_TYPE
            && exact_lite_ally_intro_route_is_emitted(
                *quest,
                selection,
                graph,
                emitted_nodes,
                interner,
            )
            && roots.iter().all(|root| emitted_nodes.contains(root))
        {
            plan.rewrites.insert(*quest, source_event);
            continue;
        }
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

fn exact_lite_ally_intro_route_is_emitted(
    quest_fk: FormKey,
    selection: &StoryManagerSelection,
    graph: &StoryManagerSourceGraph,
    emitted_nodes: &FxHashSet<FormKey>,
    interner: &StringInterner,
) -> bool {
    let node_fk = FormKey {
        local: LITE_ALLY_INTRO_NODE_LOCAL,
        plugin: quest_fk.plugin,
    };
    if !emitted_nodes.contains(&node_fk)
        || !selection
            .selected_quests_by_node
            .get(&node_fk)
            .is_some_and(|quests| quests.contains(&quest_fk))
    {
        return false;
    }

    exact_lite_ally_intro_node(graph, node_fk, interner)
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
    use crate::translator::pair_hooks::fo76_fo4::qust_has_untranslatable_event_alias;

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

    fn raw_ctda(raw_hex: &str) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(
            hex::decode(raw_hex).expect("valid source CTDA fixture"),
        ))
    }

    fn condition_function_ids(record: &Record) -> Vec<u16> {
        record
            .fields
            .iter()
            .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => u16::from_le_bytes([bytes[8], bytes[9]]),
                _ => panic!("raw CTDA expected"),
            })
            .collect()
    }

    fn condition_parameter_1(record: &Record, condition_index: usize) -> u32 {
        let FieldValue::Bytes(bytes) = &record
            .fields
            .iter()
            .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
            .nth(condition_index)
            .expect("condition index")
            .value
        else {
            panic!("raw CTDA expected");
        };
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()) & 0x00FF_FFFF
    }

    fn condition_count(record: &Record) -> u32 {
        record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CITC")
            .and_then(|entry| field_value_u32(&entry.value))
            .expect("CITC")
    }

    fn keyword(interner: &StringInterner, local: u32, editor_id: &str) -> Record {
        record(interner, "KYWD", local, Some(editor_id))
    }

    fn append_event_alias(
        record: &mut Record,
        anchor: &str,
        alias_id: u32,
        flags: u32,
        event_data: u32,
    ) {
        record
            .fields
            .push(field(anchor, bytes(&alias_id.to_le_bytes())));
        record
            .fields
            .push(field("FNAM", bytes(&flags.to_le_bytes())));
        record
            .fields
            .push(field("ALFE", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        record
            .fields
            .push(field("ALFD", bytes(&event_data.to_le_bytes())));
    }

    fn mtr06_physical_exam_quest(interner: &StringInterner, local: u32, editor_id: &str) -> Record {
        let mut quest = record(interner, "QUST", local, Some(editor_id));
        quest
            .fields
            .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(fk(interner, 0x068345))));
        append_event_alias(&mut quest, "ALLS", 4, 0, 12_620);
        append_event_alias(&mut quest, "ALST", 0, 33_554_448, 13_138);
        append_event_alias(&mut quest, "ALST", 1, 8, 12_626);
        quest
    }

    fn skyline_breadcrumb_condition_graph(
        interner: &StringInterner,
        plugin: &str,
        node_local: u32,
        start_keyword_local: u32,
        active_keyword_local: u32,
        quest_local: u32,
    ) -> (StoryManagerSourceGraph, FormKey) {
        let plugin = interner.intern(plugin);
        let form_key = |local| FormKey { local, plugin };
        let make_record = |sig: &str, local: u32, editor_id: &str| {
            let mut record = Record::new(SigCode::from_str(sig).unwrap(), form_key(local));
            record.eid = Some(interner.intern(editor_id));
            record
        };
        let condition = |raw_hex: &str, parameter_offset: usize, parameter: u32| {
            let mut raw = hex::decode(raw_hex).expect("valid source CTDA fixture");
            raw[parameter_offset..parameter_offset + 4].copy_from_slice(&parameter.to_le_bytes());
            FieldValue::Bytes(SmallVec::from_vec(raw))
        };

        let root_fk = form_key(0x029152);
        let branch_fk = form_key(0x72A2BB);
        let node_fk = form_key(node_local);
        let quest_fk = form_key(quest_local);
        let start_keyword_fk = form_key(start_keyword_local);
        let active_keyword_fk = form_key(active_keyword_local);

        let mut root = make_record("SMEN", root_fk.local, "ScriptEvent");
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = make_record("SMBN", branch_fk.local, "Storm_MainQuest_Branch");
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = make_record("SMQN", node_fk.local, "Storm_MQ01_Breadcrumb_QuestNode");
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&2_u32.to_le_bytes())));
        node.fields.push(field(
            "CTDA",
            condition(
                "000000000000000030020000A93D690000000000070000000000000052330000",
                12,
                active_keyword_fk.local,
            ),
        ));
        node.fields.push(field(
            "CTDA",
            condition(
                "000000000000803F4002000000004B31A0A072000000000000000000FFFFFFFF",
                16,
                start_keyword_fk.local,
            ),
        ));
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let quest = make_record("QUST", quest_fk.local, "Storm_MQ01_Breadcrumb");
        let start_keyword = make_record(
            "KYWD",
            start_keyword_fk.local,
            "Storm_MQ01_Breadcrumb_StartKeyword",
        );
        let active_keyword = make_record(
            "KYWD",
            active_keyword_fk.local,
            "Storm_MQ00_Breadcrumb_ActiveKeyword",
        );

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest.form_key, quest);
        for record in [start_keyword, active_keyword] {
            graph.keywords.insert(record.form_key, record);
        }
        (graph, node_fk)
    }

    fn burn_sq01_radio_condition_graph(
        interner: &StringInterner,
    ) -> (StoryManagerSourceGraph, FormKey) {
        let root_fk = fk(interner, FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let branch_fk = fk(interner, 0x007D_F78A);
        let node_fk = fk(interner, BURN_SQ01_RADIO_NODE_LOCAL);
        let quest_fk = fk(interner, BURN_SQ01_RADIO_QUEST_LOCAL);
        let start_keyword_fk = fk(interner, BURN_SQ01_RADIO_START_KEYWORD_LOCAL);

        let mut root = record(interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(
            interner,
            "SMBN",
            branch_fk.local,
            Some("BURN_SQ01_BranchNode"),
        );
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = record(
            interner,
            "SMQN",
            node_fk.local,
            Some("BURN_SQ01_Radio_QuestNode"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&2_u32.to_le_bytes())));
        node.fields.push(field(
            "CTDA",
            raw_ctda("000000000000803F4002000000004B31E1797F000000000000000000FFFFFFFF"),
        ));
        node.fields.push(field(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_slice(&BURN_SQ01_RADIO_LCP_CONDITION)),
        ));
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let quest = record(interner, "QUST", quest_fk.local, Some("BURN_SQ01_Radio"));
        let start_keyword = keyword(
            interner,
            start_keyword_fk.local,
            "BURN_SQ01_Radio_QuestStartKeyword",
        );

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest.form_key, quest);
        graph.keywords.insert(start_keyword.form_key, start_keyword);
        (graph, node_fk)
    }

    fn overseer_self_completion_start_condition_graph(
        interner: &StringInterner,
        personal: bool,
    ) -> (StoryManagerSourceGraph, FormKey) {
        let root_fk = fk(interner, FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let branch_fk = fk(interner, 0x0002_9153);
        let (
            node_local,
            quest_local,
            start_keyword_local,
            node_eid,
            quest_eid,
            start_keyword_eid,
            completion_condition,
            start_condition,
        ) = if personal {
            (
                OVERSEER_PERSONAL_NODE_LOCAL,
                OVERSEER_PERSONAL_QUEST_LOCAL,
                OVERSEER_PERSONAL_START_KEYWORD_LOCAL,
                "OverseerPersonalNode",
                "OverseerPersonal",
                "OverseerPersonal_QuestStartKeyword",
                "000B0000000000005903944334AA260000000000070000000000000052330000",
                "000B00000000803F4002944300004B31A2F812000000000000000000FFFFFFFF",
            )
        } else {
            (
                MQ_OVERSEER_NODE_LOCAL,
                MQ_OVERSEER_QUEST_LOCAL,
                MQ_OVERSEER_START_KEYWORD_LOCAL,
                "MQOverseerNode",
                "MQ_Overseer",
                "MQ_Overseer_QuestStartKeyword",
                "000B00000000000059039443D9494E0000000000070000000000000052330000",
                "000B00000000803F4002944300004B31E5494E000000000000000000FFFFFFFF",
            )
        };
        let node_fk = fk(interner, node_local);
        let quest_fk = fk(interner, quest_local);
        let start_keyword_fk = fk(interner, start_keyword_local);

        let mut root = record(interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(
            interner,
            "SMBN",
            branch_fk.local,
            Some("ScriptEventBranchNode"),
        );
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = record(interner, "SMQN", node_fk.local, Some(node_eid));
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&2_u32.to_le_bytes())));
        node.fields
            .push(field("CTDA", raw_ctda(completion_condition)));
        node.fields.push(field("CTDA", raw_ctda(start_condition)));
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let quest = record(interner, "QUST", quest_fk.local, Some(quest_eid));
        let start_keyword = keyword(interner, start_keyword_fk.local, start_keyword_eid);
        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest.form_key, quest);
        graph.keywords.insert(start_keyword.form_key, start_keyword);
        (graph, node_fk)
    }

    fn tw007_cold_case_condition_graph(
        interner: &StringInterner,
        plugin_name: &str,
    ) -> (StoryManagerSourceGraph, FormKey) {
        let plugin = interner.intern(plugin_name);
        let form_key = |local| FormKey { local, plugin };
        let make_record = |sig: &str, local: u32, eid: Option<&str>| {
            let mut record = Record::new(SigCode::from_str(sig).unwrap(), form_key(local));
            if let Some(eid) = eid {
                record.eid = Some(interner.intern(eid));
            }
            record
        };
        let root_fk = form_key(FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let branch_fk = form_key(TW007_COLD_CASE_BRANCH_LOCAL);
        let node_fk = form_key(TW007_COLD_CASE_NODE_LOCAL);
        let quest_fk = form_key(TW007_COLD_CASE_QUEST_LOCAL);
        let start_keyword_fk = form_key(TW007_COLD_CASE_START_KEYWORD_LOCAL);

        let mut root = make_record("SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = make_record("SMBN", branch_fk.local, Some("TW007_ColdCase_BranchNode"));
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = make_record("SMQN", node_fk.local, Some("TW007_ColdCase_QuestNode"));
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&2_u32.to_le_bytes())));
        node.fields.push(field(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_slice(&TW007_COLD_CASE_COMPLETION_CONDITION)),
        ));
        node.fields.push(field(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_slice(&TW007_COLD_CASE_START_CONDITION)),
        ));
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let quest = make_record("QUST", quest_fk.local, Some("TW007_ColdCase"));
        let start_keyword = make_record(
            "KYWD",
            start_keyword_fk.local,
            Some("TW007_QuestStartKeyword"),
        );
        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest.form_key, quest);
        graph.keywords.insert(start_keyword.form_key, start_keyword);
        (graph, node_fk)
    }

    fn burn_outro_p2_condition_graph(
        interner: &StringInterner,
    ) -> (StoryManagerSourceGraph, FormKey) {
        let root_fk = fk(interner, FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let branch_fk = fk(interner, 0x0082_D589);
        let node_fk = fk(interner, BURN_OUTRO_P2_NODE_LOCAL);
        let quest_fk = fk(interner, BURN_OUTRO_P2_QUEST_LOCAL);
        let start_keyword_fk = fk(interner, BURN_OUTRO_P2_START_KEYWORD_LOCAL);
        let active_keyword_fk = fk(interner, BURN_OUTRO_P2_ACTIVE_KEYWORD_LOCAL);

        let mut root = record(interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(
            interner,
            "SMBN",
            branch_fk.local,
            Some("BURN_SQ02_Outro_BranchNode"),
        );
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = record(
            interner,
            "SMQN",
            node_fk.local,
            Some("BURN_SQ02_OutroP2_QuestNode"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&3_u32.to_le_bytes())));
        for condition in [
            "000000000000803F4002000000004B31444F84000000000000000000FFFFFFFF",
            "000000000000000030020000454F840000000000070000000000000052330000",
            "000000000000803F4A00000064258600000000000000000000000000FFFFFFFF",
        ] {
            node.fields.push(field("CTDA", raw_ctda(condition)));
        }
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let quest = record(interner, "QUST", quest_fk.local, Some("BURN_SQ02_OutroP2"));
        let start_keyword = keyword(
            interner,
            start_keyword_fk.local,
            "BURN_SQ02_OutroP2_QuestStartKeyword",
        );
        let active_keyword = keyword(
            interner,
            active_keyword_fk.local,
            "BURN_SQ02_OutroP2_QuestActiveKeyword",
        );

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest.form_key, quest);
        for keyword_record in [start_keyword, active_keyword] {
            graph
                .keywords
                .insert(keyword_record.form_key, keyword_record);
        }
        (graph, node_fk)
    }

    fn ac_sq05_condition_graph(interner: &StringInterner) -> (StoryManagerSourceGraph, FormKey) {
        let root_fk = fk(interner, FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let atlantic_city_branch_fk = fk(interner, 0x006C_1F52);
        let side_quest_branch_fk = fk(interner, AC_SQ05_BRANCH_LOCAL);
        let node_fk = fk(interner, AC_SQ05_NODE_LOCAL);
        let quest_fk = fk(interner, AC_SQ05_QUEST_LOCAL);
        let start_keyword_fk = fk(interner, AC_SQ05_START_KEYWORD_LOCAL);
        let active_keyword_fk = fk(interner, AC_SQ05_ACTIVE_KEYWORD_LOCAL);

        let mut root = record(interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut atlantic_city_branch = record(
            interner,
            "SMBN",
            atlantic_city_branch_fk.local,
            Some("AC_AtlanticCity_Branch"),
        );
        atlantic_city_branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut side_quest_branch = record(
            interner,
            "SMBN",
            side_quest_branch_fk.local,
            Some("AC_SQ_Branch"),
        );
        side_quest_branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(atlantic_city_branch_fk)));
        let mut node = record(
            interner,
            "SMQN",
            node_fk.local,
            Some("AC_SQ05_Regent_QuestNote"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(side_quest_branch_fk)));
        node.fields.push(field(
            "SNAM",
            FieldValue::FormKey(fk(interner, AC_SQ04_NODE_LOCAL)),
        ));
        node.fields.push(field("CITC", bytes(&2_u32.to_le_bytes())));
        for condition in [
            "000000000000000030020000BFCF6F0000000000070000000000000052330000",
            "000000000000803F4002000000004B31C0CF6F000000000000000000FFFFFFFF",
        ] {
            node.fields.push(field("CTDA", raw_ctda(condition)));
        }
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let quest = record(interner, "QUST", quest_fk.local, Some("AC_SQ05_Regent"));
        let start_keyword = keyword(
            interner,
            start_keyword_fk.local,
            "AC_SQ05_Regent_StartKeyword",
        );
        let active_keyword = keyword(
            interner,
            active_keyword_fk.local,
            "AC_SQ05_Regent_QuestActiveKeyword",
        );

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, atlantic_city_branch, side_quest_branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest.form_key, quest);
        for record in [start_keyword, active_keyword] {
            graph.keywords.insert(record.form_key, record);
        }
        (graph, node_fk)
    }

    fn scpt_quest_start_graph(
        interner: &StringInterner,
    ) -> (StoryManagerSourceGraph, FormKey, FormKey) {
        let root_fk = fk(interner, 0x029152);
        let branch_fk = fk(interner, 0x3FBBBD);
        let wayward_node_fk = fk(interner, 0x405ECD);
        let lacey_node_fk = fk(interner, 0x405ECE);
        let wayward_quest_fk = fk(interner, 0x405E14);
        let lacey_quest_fk = fk(interner, 0x405E15);

        let mut root = record(interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(interner, "SMBN", branch_fk.local, Some("W05_MQ_Branch"));
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));

        let mut wayward_node = record(
            interner,
            "SMQN",
            wayward_node_fk.local,
            Some("W05_MQ001P_QuestNode"),
        );
        wayward_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        wayward_node
            .fields
            .push(field("CITC", bytes(&3_u32.to_le_bytes())));
        for ctda in [
            "000B00000000803F4002944300004B31C65E40000000000000000000FFFFFFFF",
            "000000000000000030029443C85E400000000000070000000000000052330000",
            "A00000000000000059039443145E400000000000070000000000000052330000",
        ] {
            wayward_node.fields.push(field("CTDA", raw_ctda(ctda)));
        }
        wayward_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(wayward_quest_fk)));

        let mut lacey_node = record(
            interner,
            "SMQN",
            lacey_node_fk.local,
            Some("W05_MQ001P_LaceyIsela_QuestNode"),
        );
        lacey_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        lacey_node
            .fields
            .push(field("CITC", bytes(&3_u32.to_le_bytes())));
        for ctda in [
            "000B00000000803F4002944300004B31C75E40000000000000000000FFFFFFFF",
            "000000000000000030029443C95E400000000000070000000000000052330000",
            "A00000000000000059039443155E400000000000070000000000000052330000",
        ] {
            lacey_node.fields.push(field("CTDA", raw_ctda(ctda)));
        }
        lacey_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(lacey_quest_fk)));

        let mut wayward_quest = record(
            interner,
            "QUST",
            wayward_quest_fk.local,
            Some("W05_MQ_001P_Wayward"),
        );
        wayward_quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(fk(interner, 0x405EC8))));
        let lacey_quest = record(
            interner,
            "QUST",
            lacey_quest_fk.local,
            Some("W05_MQ_001P_Wayward_LaceyIselaScene"),
        );

        let mut graph = StoryManagerSourceGraph::default();
        graph.nodes.insert(root_fk, root);
        graph.nodes.insert(branch_fk, branch);
        graph.nodes.insert(wayward_node_fk, wayward_node);
        graph.nodes.insert(lacey_node_fk, lacey_node);
        graph.quests.insert(wayward_quest_fk, wayward_quest);
        graph.quests.insert(lacey_quest_fk, lacey_quest);
        for keyword_record in [
            keyword(interner, 0x405EC6, "W05_MQ_001P_Wayward_QuestStartKeyword"),
            keyword(
                interner,
                0x405EC7,
                "W05_MQ_001P_Wayward_LaceyIselaQuestStartKeyword",
            ),
            keyword(interner, 0x405EC8, "W05_MQ_001P_Wayward_QuestActiveKeyword"),
            keyword(
                interner,
                0x405EC9,
                "W05_MQ_001P_Wayward_LaceyIselaQuestActiveKeyword",
            ),
        ] {
            graph
                .keywords
                .insert(keyword_record.form_key, keyword_record);
        }
        (graph, wayward_node_fk, lacey_node_fk)
    }

    #[test]
    fn exact_skyline_breadcrumb_relation_lowers_the_active_keyword_gate() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = skyline_breadcrumb_condition_graph(
            &interner,
            "SeventySix.esm",
            SKYLINE_BREADCRUMB_NODE_LOCAL,
            SKYLINE_BREADCRUMB_START_KEYWORD_LOCAL,
            SKYLINE_BREADCRUMB_ACTIVE_KEYWORD_LOCAL,
            SKYLINE_BREADCRUMB_QUEST_LOCAL,
        );

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let node = &graph.nodes[&node_fk];
        assert_eq!(condition_function_ids(node), vec![56, 576]);
        assert_eq!(
            condition_parameter_1(node, 0),
            SKYLINE_BREADCRUMB_QUEST_LOCAL
        );
        assert_eq!(condition_count(node), 2);
    }

    #[test]
    fn exact_ac_sq05_relation_lowers_the_unowned_active_keyword_gate() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = ac_sq05_condition_graph(&interner);

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let node = &graph.nodes[&node_fk];
        assert_eq!(condition_function_ids(node), vec![56, 576]);
        assert_eq!(condition_parameter_1(node, 0), AC_SQ05_QUEST_LOCAL);
        assert_eq!(condition_count(node), 2);
    }

    #[test]
    fn ac_sq05_relation_rejects_an_active_keyword_identity_lookalike() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = ac_sq05_condition_graph(&interner);
        graph
            .keywords
            .get_mut(&fk(&interner, AC_SQ05_ACTIVE_KEYWORD_LOCAL))
            .unwrap()
            .eid = Some(interner.intern("AC_SQ05_Regent_QuestActiveKeyword_Copy"));

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.form_key == node_fk
                && diagnostic.message
                    == format!(
                        "active_keyword_unmapped:{:06X}",
                        AC_SQ05_ACTIVE_KEYWORD_LOCAL
                    )
        }));
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![560, 576]
        );
        assert_eq!(condition_count(&graph.nodes[&node_fk]), 2);
    }

    #[test]
    fn exact_ac_sq05_node_rechains_around_removed_sq04() {
        let interner = StringInterner::new();
        let (graph, node_fk) = ac_sq05_condition_graph(&interner);
        let sq03_fk = fk(&interner, AC_SQ03_NODE_LOCAL);
        let selected_nodes = FxHashSet::from_iter([node_fk, sq03_fk]);
        let mut node = graph.nodes[&node_fk].clone();

        assert!(sanitize_story_manager_previous_node(
            &mut node,
            &selected_nodes,
            &interner
        ));

        assert_eq!(story_manager_fk(&node, snam_sig()), Some(sq03_fk));
        assert!(node.warnings.iter().any(|warning| {
            interner.resolve(*warning) == Some("story_manager_previous_node_rechained")
        }));
    }

    #[test]
    fn ac_sq05_rechain_rejects_unsafe_lookalikes() {
        let cases = [
            "wrong_editor_id",
            "wrong_parent",
            "wrong_quest",
            "missing_sq03",
        ];
        for case in cases {
            let interner = StringInterner::new();
            let (graph, node_fk) = ac_sq05_condition_graph(&interner);
            let sq03_fk = fk(&interner, AC_SQ03_NODE_LOCAL);
            let mut selected_nodes = FxHashSet::from_iter([node_fk, sq03_fk]);
            let mut node = graph.nodes[&node_fk].clone();
            match case {
                "wrong_editor_id" => {
                    node.eid = Some(interner.intern("AC_SQ05_Regent_QuestNode_Copy"));
                }
                "wrong_parent" => {
                    node.fields
                        .iter_mut()
                        .find(|entry| entry.sig == pnam_sig())
                        .unwrap()
                        .value = FieldValue::FormKey(fk(&interner, AC_SQ05_BRANCH_LOCAL + 1));
                }
                "wrong_quest" => {
                    node.fields
                        .iter_mut()
                        .find(|entry| entry.sig == nnam_sig())
                        .unwrap()
                        .value = FieldValue::FormKey(fk(&interner, AC_SQ05_QUEST_LOCAL + 1));
                }
                "missing_sq03" => {
                    selected_nodes.remove(&sq03_fk);
                }
                _ => unreachable!(),
            }

            assert!(sanitize_story_manager_previous_node(
                &mut node,
                &selected_nodes,
                &interner
            ));
            assert_eq!(
                story_manager_fk(&node, snam_sig()).unwrap_or(FormKey {
                    local: 0,
                    plugin: node.form_key.plugin,
                }),
                FormKey {
                    local: 0,
                    plugin: node.form_key.plugin,
                },
                "{case}"
            );
            assert!(node.warnings.iter().any(|warning| {
                interner.resolve(*warning) == Some("story_manager_previous_node_nulled")
            }));
        }
    }

    #[test]
    fn exact_burn_sq01_radio_relation_preserves_the_lcp_gate() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = burn_sq01_radio_condition_graph(&interner);

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![576, 74]
        );
        assert_eq!(condition_count(&graph.nodes[&node_fk]), 2);
    }

    #[test]
    fn burn_sq01_radio_relation_rejects_an_identity_lookalike() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = burn_sq01_radio_condition_graph(&interner);
        graph.nodes.get_mut(&node_fk).unwrap().eid =
            Some(interner.intern("BURN_SQ01_Radio_QuestNode_Copy"));

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.form_key == node_fk
                && diagnostic.message == "quest_start_unqualified:no_active_keyword_role"
        }));
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![576, 74]
        );
    }

    #[test]
    fn exact_overseer_self_completion_start_relations_lower_only_the_self_completion_gate() {
        for personal in [false, true] {
            let interner = StringInterner::new();
            let (mut graph, node_fk) =
                overseer_self_completion_start_condition_graph(&interner, personal);
            let quest_local = if personal {
                OVERSEER_PERSONAL_QUEST_LOCAL
            } else {
                MQ_OVERSEER_QUEST_LOCAL
            };

            let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

            assert!(
                diagnostics.is_empty(),
                "personal={personal}: {diagnostics:?}"
            );
            assert_eq!(
                condition_function_ids(&graph.nodes[&node_fk]),
                vec![543, 576],
                "personal={personal}"
            );
            assert_eq!(
                condition_parameter_1(&graph.nodes[&node_fk], 0),
                quest_local,
                "personal={personal}"
            );
            assert_eq!(condition_count(&graph.nodes[&node_fk]), 2);
        }
    }

    #[test]
    fn overseer_self_completion_start_relations_reject_identity_and_shape_lookalikes() {
        for personal in [false, true] {
            for case in ["node_eid", "quest_eid", "keyword_eid", "quest", "condition"] {
                let interner = StringInterner::new();
                let (mut graph, node_fk) =
                    overseer_self_completion_start_condition_graph(&interner, personal);
                let quest_fk = story_manager_quests(&graph.nodes[&node_fk])[0];
                let start_keyword_fk = graph.nodes[&node_fk]
                    .fields
                    .iter()
                    .find_map(|entry| {
                        Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
                            .map(|raw| raw_form_key(raw, node_fk.plugin))
                    })
                    .unwrap();
                match case {
                    "node_eid" => {
                        graph.nodes.get_mut(&node_fk).unwrap().eid =
                            Some(interner.intern("OverseerNode_Copy"));
                    }
                    "quest_eid" => {
                        graph.quests.get_mut(&quest_fk).unwrap().eid =
                            Some(interner.intern("OverseerQuest_Copy"));
                    }
                    "keyword_eid" => {
                        graph.keywords.get_mut(&start_keyword_fk).unwrap().eid =
                            Some(interner.intern("Overseer_StartKeyword_Copy"));
                    }
                    "quest" => {
                        graph
                            .nodes
                            .get_mut(&node_fk)
                            .unwrap()
                            .fields
                            .iter_mut()
                            .find(|entry| entry.sig == nnam_sig())
                            .unwrap()
                            .value = FieldValue::FormKey(FormKey {
                            local: quest_fk.local + 1,
                            plugin: quest_fk.plugin,
                        });
                    }
                    "condition" => {
                        let FieldValue::Bytes(bytes) = &mut graph
                            .nodes
                            .get_mut(&node_fk)
                            .unwrap()
                            .fields
                            .iter_mut()
                            .find(|entry| {
                                Fo76Fo4Hook::story_manager_completion_quest(&entry.value).is_some()
                            })
                            .unwrap()
                            .value
                        else {
                            unreachable!()
                        };
                        bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
                    }
                    _ => unreachable!(),
                }

                let diagnostics =
                    normalize_story_manager_quest_start_conditions(&mut graph, &interner);

                assert!(!diagnostics.is_empty(), "personal={personal} case={case}");
                assert_eq!(
                    condition_function_ids(&graph.nodes[&node_fk]),
                    vec![857, 576],
                    "personal={personal} case={case}"
                );
            }
        }
    }

    #[test]
    fn exact_tw007_cold_case_relation_lowers_only_the_self_completion_guard() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = tw007_cold_case_condition_graph(&interner, "SeventySix.esm");

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![543, 576]
        );
        assert_eq!(
            condition_parameter_1(&graph.nodes[&node_fk], 0),
            TW007_COLD_CASE_QUEST_LOCAL
        );
        assert_eq!(
            graph.nodes[&node_fk]
                .fields
                .iter()
                .find_map(|entry| { Fo76Fo4Hook::story_manager_start_keyword(&entry.value) }),
            Some(TW007_COLD_CASE_START_KEYWORD_LOCAL)
        );
        assert_eq!(condition_count(&graph.nodes[&node_fk]), 2);
    }

    #[test]
    fn tw007_cold_case_relation_rejects_identity_and_shape_lookalikes() {
        for case in [
            "node_eid",
            "quest_eid",
            "keyword_eid",
            "parent",
            "quest",
            "condition",
        ] {
            let interner = StringInterner::new();
            let (mut graph, node_fk) = tw007_cold_case_condition_graph(&interner, "SeventySix.esm");
            let quest_fk = FormKey {
                local: TW007_COLD_CASE_QUEST_LOCAL,
                plugin: node_fk.plugin,
            };
            let start_keyword_fk = FormKey {
                local: TW007_COLD_CASE_START_KEYWORD_LOCAL,
                plugin: node_fk.plugin,
            };
            match case {
                "node_eid" => {
                    graph.nodes.get_mut(&node_fk).unwrap().eid =
                        Some(interner.intern("TW007_ColdCase_QuestNode_Copy"));
                }
                "quest_eid" => {
                    graph.quests.get_mut(&quest_fk).unwrap().eid =
                        Some(interner.intern("TW007_ColdCase_Copy"));
                }
                "keyword_eid" => {
                    graph.keywords.get_mut(&start_keyword_fk).unwrap().eid =
                        Some(interner.intern("TW007_QuestStartKeyword_Copy"));
                }
                "parent" => {
                    graph
                        .nodes
                        .get_mut(&node_fk)
                        .unwrap()
                        .fields
                        .iter_mut()
                        .find(|entry| entry.sig == pnam_sig())
                        .unwrap()
                        .value = FieldValue::FormKey(FormKey {
                        local: TW007_COLD_CASE_BRANCH_LOCAL + 1,
                        plugin: node_fk.plugin,
                    });
                }
                "quest" => {
                    graph
                        .nodes
                        .get_mut(&node_fk)
                        .unwrap()
                        .fields
                        .iter_mut()
                        .find(|entry| entry.sig == nnam_sig())
                        .unwrap()
                        .value = FieldValue::FormKey(FormKey {
                        local: TW007_COLD_CASE_QUEST_LOCAL + 1,
                        plugin: node_fk.plugin,
                    });
                }
                "condition" => {
                    let FieldValue::Bytes(bytes) = &mut graph
                        .nodes
                        .get_mut(&node_fk)
                        .unwrap()
                        .fields
                        .iter_mut()
                        .find(|entry| {
                            Fo76Fo4Hook::story_manager_completion_quest(&entry.value).is_some()
                        })
                        .unwrap()
                        .value
                    else {
                        unreachable!()
                    };
                    bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
                }
                _ => unreachable!(),
            }

            let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

            if case == "parent" {
                assert!(!story_manager_has_scpt_root(&graph.nodes[&node_fk], &graph));
            } else {
                assert!(!diagnostics.is_empty(), "case={case}");
            }
            assert_eq!(
                condition_function_ids(&graph.nodes[&node_fk]),
                vec![857, 576],
                "case={case}"
            );
        }
    }

    #[test]
    fn tw007_cold_case_relation_rejects_a_foreign_source_plugin() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = tw007_cold_case_condition_graph(&interner, "Lookalike.esm");

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.form_key == node_fk
                && diagnostic.message == "quest_start_unqualified:no_active_keyword_role"
        }));
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![857, 576]
        );
    }

    #[test]
    fn exact_burn_outro_p2_relation_lowers_the_missing_active_keyword_owner() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = burn_outro_p2_condition_graph(&interner);

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![576, 56, 74]
        );
        assert_eq!(
            condition_parameter_1(&graph.nodes[&node_fk], 1),
            BURN_OUTRO_P2_QUEST_LOCAL
        );
        assert_eq!(condition_count(&graph.nodes[&node_fk]), 3);
    }

    #[test]
    fn burn_outro_p2_relation_rejects_an_active_keyword_lookalike() {
        let interner = StringInterner::new();
        let (mut graph, node_fk) = burn_outro_p2_condition_graph(&interner);
        let active_keyword_fk = fk(&interner, BURN_OUTRO_P2_ACTIVE_KEYWORD_LOCAL);
        graph.keywords.get_mut(&active_keyword_fk).unwrap().eid =
            Some(interner.intern("BURN_SQ02_OutroP2_QuestActiveKeyword_Copy"));

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.form_key == node_fk
                && diagnostic.message
                    == format!(
                        "active_keyword_unmapped:{:06X}",
                        BURN_OUTRO_P2_ACTIVE_KEYWORD_LOCAL
                    )
        }));
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![576, 560, 74]
        );
    }

    #[test]
    fn skyline_breadcrumb_relation_rejects_every_identity_lookalike() {
        let cases = [
            (
                "wrong node",
                "SeventySix.esm",
                SKYLINE_BREADCRUMB_NODE_LOCAL + 1,
                SKYLINE_BREADCRUMB_START_KEYWORD_LOCAL,
                SKYLINE_BREADCRUMB_ACTIVE_KEYWORD_LOCAL,
                SKYLINE_BREADCRUMB_QUEST_LOCAL,
            ),
            (
                "wrong start keyword",
                "SeventySix.esm",
                SKYLINE_BREADCRUMB_NODE_LOCAL,
                SKYLINE_BREADCRUMB_START_KEYWORD_LOCAL + 1,
                SKYLINE_BREADCRUMB_ACTIVE_KEYWORD_LOCAL,
                SKYLINE_BREADCRUMB_QUEST_LOCAL,
            ),
            (
                "wrong active keyword",
                "SeventySix.esm",
                SKYLINE_BREADCRUMB_NODE_LOCAL,
                SKYLINE_BREADCRUMB_START_KEYWORD_LOCAL,
                SKYLINE_BREADCRUMB_ACTIVE_KEYWORD_LOCAL + 1,
                SKYLINE_BREADCRUMB_QUEST_LOCAL,
            ),
            (
                "wrong quest",
                "SeventySix.esm",
                SKYLINE_BREADCRUMB_NODE_LOCAL,
                SKYLINE_BREADCRUMB_START_KEYWORD_LOCAL,
                SKYLINE_BREADCRUMB_ACTIVE_KEYWORD_LOCAL,
                SKYLINE_BREADCRUMB_QUEST_LOCAL + 1,
            ),
            (
                "wrong plugin",
                "SeventySix_Lookalike.esm",
                SKYLINE_BREADCRUMB_NODE_LOCAL,
                SKYLINE_BREADCRUMB_START_KEYWORD_LOCAL,
                SKYLINE_BREADCRUMB_ACTIVE_KEYWORD_LOCAL,
                SKYLINE_BREADCRUMB_QUEST_LOCAL,
            ),
        ];

        for (label, plugin, node, start_keyword, active_keyword, quest) in cases {
            let interner = StringInterner::new();
            let (mut graph, node_fk) = skyline_breadcrumb_condition_graph(
                &interner,
                plugin,
                node,
                start_keyword,
                active_keyword,
                quest,
            );

            let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.form_key == node_fk
                        && diagnostic.message
                            == "quest_start_unqualified:no_unique_quest_relation"),
                "{label}: {diagnostics:?}"
            );
            assert_eq!(
                condition_function_ids(&graph.nodes[&node_fk]),
                vec![560, 576],
                "{label}"
            );
            assert_eq!(condition_count(&graph.nodes[&node_fk]), 2, "{label}");
        }
    }

    #[test]
    fn structural_start_condition_lowering_handles_exact_wayward_nodes() {
        let interner = StringInterner::new();
        let (mut graph, wayward_node_fk, lacey_node_fk) = scpt_quest_start_graph(&interner);
        let original_starts = [wayward_node_fk, lacey_node_fk].map(|node_fk| {
            graph.nodes[&node_fk]
                .fields
                .iter()
                .find(|entry| Fo76Fo4Hook::story_manager_start_keyword(&entry.value).is_some())
                .unwrap()
                .value
                .clone()
        });

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        for (index, node_fk) in [wayward_node_fk, lacey_node_fk].into_iter().enumerate() {
            let node = &graph.nodes[&node_fk];
            assert_eq!(condition_function_ids(node), vec![576, 56, 543]);
            assert_eq!(condition_count(node), 3);
            let start = node
                .fields
                .iter()
                .find(|entry| Fo76Fo4Hook::story_manager_start_keyword(&entry.value).is_some())
                .unwrap();
            assert_eq!(start.value, original_starts[index]);
            let expected_quest = if node_fk == wayward_node_fk {
                0x405E14
            } else {
                0x405E15
            };
            assert_eq!(condition_parameter_1(node, 1), expected_quest);
            assert_eq!(condition_parameter_1(node, 2), expected_quest);
            for condition in node
                .fields
                .iter()
                .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
                .skip(1)
            {
                let FieldValue::Bytes(bytes) = &condition.value else {
                    panic!("raw CTDA expected");
                };
                assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 0);
                assert_eq!(
                    u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
                    u32::MAX
                );
            }
        }
    }

    #[test]
    fn exact_k1_only_w05_nodes_are_valid_and_route_eligible() {
        for (
            node_local,
            quest_local,
            start_keyword_local,
            previous_local,
            node_editor_id,
            quest_editor_id,
            start_keyword_editor_id,
            condition_hex,
        ) in [
            (
                0x3FBC11,
                0x3FBC0D,
                0x3FBC0E,
                0x3FBBBF,
                "W05_MQ101P_A_QuestNode",
                "W05_MQ_101P_A",
                "W05_MQ_101P_A_QuestStartKeyword",
                "000B00000000803F4002944300004B310EBC3F000000000000000000FFFFFFFF",
            ),
            (
                0x3FBC12,
                0x3FBC10,
                0x3FBC0F,
                0x3FBC11,
                "W05_MQ101P_B_QuestNode",
                "W05_MQ_101P_B",
                "W05_MQ_101P_B_QuestStartKeyword",
                "000B00000000803F4002944300004B310FBC3F000000000000000000FFFFFFFF",
            ),
            (
                0x3FFAD4,
                0x3FFACF,
                0x3FFAD1,
                0x3FBC12,
                "W05_MQ102P_QuestNode",
                "W05_MQ_102P",
                "W05_MQ_102P_QuestStartKeyword",
                "000B00000000803F4002944300004B31D1FA3F000000000000000000FFFFFFFF",
            ),
            (
                0x40D4A7,
                0x40D28D,
                0x40D47C,
                0x3FBCA6,
                "W05_MQR_201P_QuestNode",
                "W05_MQR_201P",
                "W05_MQR_201P_QuestStart_Keyword",
                "000B00000000803F4002944300004B317CD440000000000000000000FFFFFFFF",
            ),
            (
                0x41CB60,
                0x41C9E6,
                0x41CB4B,
                0x40F6B6,
                "W05_MQR_202P_QuestNode",
                "W05_MQR_202P",
                "W05_MQR_202P_QuestStart_Keyword",
                "000B00000000803F4002944300004B314BCB41000000000000000000FFFFFFFF",
            ),
            (
                0x535E74,
                0x42F31B,
                0x42F546,
                0x41CC44,
                "W05_MQR_203P_QuestNode",
                "W05_MQR_203P",
                "W05_MQR_203P_QuestStart_Keyword",
                "000B00000000803F4002944300004B3146F542000000000000000000FFFFFFFF",
            ),
            (
                0x53C272,
                0x535E55,
                0x535E77,
                0x42692A,
                "W05_MQR_204P_QuestNode",
                "W05_MQR_204P",
                "W05_MQR_204P_QuestStart_Keyword",
                "000B00000000803F4002944300004B31775E53000000000000000000FFFFFFFF",
            ),
            (
                0x548D69,
                0x548B7A,
                0x548CFC,
                0x535E74,
                "W05_MQR_205P_QuestNode",
                "W05_MQR_205P",
                "W05_MQR_205P_QuestStart_Keyword",
                "000000000000803F4002000000004B31FC8C54000000000000000000FFFFFFFF",
            ),
        ] {
            let interner = StringInterner::new();
            let root_fk = fk(&interner, 0x029152);
            let branch_fk = fk(&interner, 0x3FBBBD);
            let node_fk = fk(&interner, node_local);
            let quest_fk = fk(&interner, quest_local);
            let start_keyword_fk = fk(&interner, start_keyword_local);

            let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
            root.fields.push(field("ENAM", bytes(b"SCPT")));
            let mut branch = record(&interner, "SMBN", branch_fk.local, Some("W05_MQ_Branch"));
            branch
                .fields
                .push(field("PNAM", FieldValue::FormKey(root_fk)));
            let mut node = record(&interner, "SMQN", node_fk.local, Some(node_editor_id));
            node.fields
                .push(field("PNAM", FieldValue::FormKey(branch_fk)));
            node.fields.push(field(
                "SNAM",
                FieldValue::FormKey(fk(&interner, previous_local)),
            ));
            node.fields.push(field("CITC", bytes(&1_u32.to_le_bytes())));
            node.fields.push(field("CTDA", raw_ctda(condition_hex)));
            node.fields
                .push(field("NNAM", FieldValue::FormKey(quest_fk)));
            let mut quest = record(&interner, "QUST", quest_fk.local, Some(quest_editor_id));
            quest
                .fields
                .push(field("ENAM", FieldValue::Uint(u64::from(SCPT_EVENT_TYPE))));

            let mut graph = StoryManagerSourceGraph::default();
            graph.nodes.insert(root_fk, root);
            graph.nodes.insert(branch_fk, branch);
            graph.nodes.insert(node_fk, node);
            graph.quests.insert(quest_fk, quest);
            graph.keywords.insert(
                start_keyword_fk,
                keyword(&interner, start_keyword_fk.local, start_keyword_editor_id),
            );

            let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

            assert!(diagnostics.is_empty(), "{node_local:06X}: {diagnostics:?}");
            assert_eq!(condition_function_ids(&graph.nodes[&node_fk]), vec![576]);
            assert_eq!(condition_count(&graph.nodes[&node_fk]), 1);
            graph.condition_diagnostics = diagnostics;

            let mut fixture = route_seed_fixture(&interner, SCPT_EVENT_TYPE, false);
            let target_root = fixture.source_to_target[&fixture.source_root];
            let target_quest = fixture.source_to_target[&fixture.source_quest];
            fixture
                .target_records
                .get_mut(&fixture.target_node)
                .unwrap()
                .fields
                .push(field("CTDA", raw_ctda(condition_hex)));
            fixture.graph = graph;
            fixture.selection = classify_story_manager_records(
                &fixture.graph,
                &FxHashSet::from_iter([quest_fk]),
                &interner,
            );
            fixture.source_to_target = FxHashMap::from_iter([
                (root_fk, target_root),
                (branch_fk, fixture.target_branch),
                (node_fk, fixture.target_node),
                (quest_fk, target_quest),
            ]);
            fixture.emitted_nodes = FxHashSet::from_iter([root_fk, branch_fk, node_fk]);
            fixture.source_root = root_fk;
            fixture.source_node = node_fk;
            fixture.source_quest = quest_fk;

            let report = route_seed_report(&interner, &fixture, Game::Fo76);
            assert_eq!(report.routes.len(), 1);
            let route = &report.routes[0];
            assert_eq!(route.source_node, Some(node_fk.format(&interner)));
            assert!(route.condition_valid);
            assert_eq!(route.structural_status, "eligible");
        }
    }

    #[test]
    fn exact_w05_mqr_choice_node_lowers_only_the_active_keyword_gate() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, 0x029152);
        let branch_fk = fk(&interner, 0x3FBBBD);
        let node_fk = fk(&interner, 0x5930BB);
        let quest_fk = fk(&interner, 0x5930B2);
        let start_keyword_fk = fk(&interner, 0x5930B9);
        let active_keyword_fk = fk(&interner, 0x5930BA);
        let condition_hexes = [
            "000B00000000803F4002944300004B31B93059000000000000000000FFFFFFFF",
            "000000000000000030029443BA30590000000000070000000000000052330000",
            "00000000000000000E0094430925590000000000070000000000000052330000",
        ];

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(&interner, "SMBN", branch_fk.local, Some("W05_MQ_Branch"));
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = record(
            &interner,
            "SMQN",
            node_fk.local,
            Some("W05_MQR_Choice_QuestNode"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields
            .push(field("SNAM", FieldValue::FormKey(fk(&interner, 0x563448))));
        node.fields.push(field("CITC", bytes(&3_u32.to_le_bytes())));
        for condition_hex in condition_hexes {
            node.fields.push(field("CTDA", raw_ctda(condition_hex)));
        }
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));
        let mut quest = record(&interner, "QUST", quest_fk.local, Some("W05_MQR_Choice"));
        quest
            .fields
            .push(field("ENAM", FieldValue::Uint(u64::from(SCPT_EVENT_TYPE))));
        quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(active_keyword_fk)));

        let mut graph = StoryManagerSourceGraph::default();
        graph.nodes.insert(root_fk, root);
        graph.nodes.insert(branch_fk, branch);
        graph.nodes.insert(node_fk, node);
        graph.quests.insert(quest_fk, quest);
        for keyword_record in [
            keyword(
                &interner,
                start_keyword_fk.local,
                "W05_MQR_Choice_QuestStartKeyword",
            ),
            keyword(
                &interner,
                active_keyword_fk.local,
                "W05_MQR_Choice_QuestActiveKeyword",
            ),
        ] {
            graph
                .keywords
                .insert(keyword_record.form_key, keyword_record);
        }

        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![576, 560, 14]
        );
        let original_actor_value_gate = graph.nodes[&node_fk]
            .fields
            .iter()
            .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
            .nth(2)
            .unwrap()
            .value
            .clone();

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let normalized_node = &graph.nodes[&node_fk];
        assert_eq!(condition_function_ids(normalized_node), vec![576, 56, 14]);
        assert_eq!(condition_count(normalized_node), 3);
        assert_eq!(condition_parameter_1(normalized_node, 1), quest_fk.local);
        assert_eq!(condition_parameter_1(normalized_node, 2), 0x592509);
        assert_eq!(
            normalized_node
                .fields
                .iter()
                .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
                .nth(2)
                .unwrap()
                .value,
            original_actor_value_gate
        );
        graph.condition_diagnostics = diagnostics;

        let mut fixture = route_seed_fixture(&interner, SCPT_EVENT_TYPE, false);
        let target_root = fixture.source_to_target[&fixture.source_root];
        let target_quest = fixture.source_to_target[&fixture.source_quest];
        fixture
            .target_records
            .get_mut(&fixture.target_node)
            .unwrap()
            .fields
            .extend(
                normalized_node
                    .fields
                    .iter()
                    .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
                    .cloned(),
            );
        fixture.graph = graph;
        fixture.selection = classify_story_manager_records(
            &fixture.graph,
            &FxHashSet::from_iter([quest_fk]),
            &interner,
        );
        fixture.source_to_target = FxHashMap::from_iter([
            (root_fk, target_root),
            (branch_fk, fixture.target_branch),
            (node_fk, fixture.target_node),
            (quest_fk, target_quest),
        ]);
        fixture.emitted_nodes = FxHashSet::from_iter([root_fk, branch_fk, node_fk]);
        fixture.source_root = root_fk;
        fixture.source_node = node_fk;
        fixture.source_quest = quest_fk;

        let report = route_seed_report(&interner, &fixture, Game::Fo76);
        assert_eq!(report.routes.len(), 1);
        let route = &report.routes[0];
        assert_eq!(route.source_node, Some(node_fk.format(&interner)));
        assert!(route.condition_valid);
        assert_eq!(route.structural_status, "eligible");
    }

    #[test]
    fn emitted_wayward_story_manager_quests_clear_source_sge_without_losing_selection() {
        let interner = StringInterner::new();
        let (mut graph, wayward_node_fk, lacey_node_fk) = scpt_quest_start_graph(&interner);
        let wayward_quest_fk = fk(&interner, 0x405E14);
        let lacey_quest_fk = fk(&interner, 0x405E15);
        let non_selected_quest_fk = fk(&interner, 0x123456);
        let source_flags = 0x0401_8511_u64;
        graph
            .quests
            .get_mut(&wayward_quest_fk)
            .unwrap()
            .fields
            .push(field("DATA", quest_data(0_u64)));
        let lacey_source = graph.quests.get_mut(&lacey_quest_fk).unwrap();
        lacey_source
            .fields
            .push(field("DATA", quest_data(source_flags)));
        lacey_source
            .fields
            .push(field("VMAD", bytes(&[6, 0, 2, 0])));
        lacey_source
            .fields
            .push(field("INDX", bytes(&[10, 0, 2, 0])));
        let mut non_selected = lacey_source.clone();
        non_selected.form_key = non_selected_quest_fk;
        non_selected.eid = Some(interner.intern("UnselectedPassiveController"));
        graph.quests.insert(non_selected_quest_fk, non_selected);
        let translated = FxHashSet::from_iter([wayward_quest_fk, lacey_quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.selected_nodes.contains(&wayward_node_fk));
        assert!(selection.selected_nodes.contains(&lacey_node_fk));
        assert!(is_passive_dialogue_controller(
            &graph.quests[&lacey_quest_fk],
            true,
            &interner,
        ));
        let emitted_nodes = selection.selected_nodes.clone();
        let quests = emitted_story_manager_quests_requiring_autostart_clear(
            &selection,
            &emitted_nodes,
            &graph,
        );
        assert_eq!(quests, vec![wayward_quest_fk, lacey_quest_fk]);
        let story_manager_owned_quests = quests.iter().copied().collect::<FxHashSet<_>>();
        assert!(story_manager_owned_quests.contains(&lacey_quest_fk));
        assert!(!story_manager_owned_quests.contains(&non_selected_quest_fk));
        assert!(!allows_passive_dialogue_autostart_restore(
            lacey_quest_fk,
            &story_manager_owned_quests,
        ));
        assert!(allows_passive_dialogue_autostart_restore(
            non_selected_quest_fk,
            &story_manager_owned_quests,
        ));
        assert!(is_passive_dialogue_controller(
            &graph.quests[&non_selected_quest_fk],
            true,
            &interner,
        ));

        let mut fallback_selection = selection.clone();
        fallback_selection
            .fallback_dialogue_quests
            .push(lacey_quest_fk);
        assert_eq!(
            emitted_story_manager_quests_requiring_autostart_clear(
                &fallback_selection,
                &emitted_nodes,
                &graph,
            ),
            vec![wayward_quest_fk],
            "intentionally autostarted dialogue fallbacks are excluded"
        );

        let mut target_quests = FxHashMap::default();
        for (quest_fk, flags) in [
            (wayward_quest_fk, 0_u16),
            (lacey_quest_fk, source_flags as u16),
            (non_selected_quest_fk, source_flags as u16),
        ] {
            let mut target = record(
                &interner,
                "QUST",
                quest_fk.local,
                Some("TranslatedTargetQuest"),
            );
            target
                .fields
                .push(field("DNAM", bytes(&flags.to_le_bytes())));
            target_quests.insert(quest_fk, target);
        }

        let mut other_pair_lacey = target_quests[&lacey_quest_fk].clone();
        assert!(!clear_qust_autostart_for_pair(
            Game::Fnv,
            Game::Fo4,
            &mut other_pair_lacey,
            &interner,
        ));
        assert_ne!(
            quest_flags(&other_pair_lacey, &interner).unwrap()
                & u64::from(QUST_FLAG_START_GAME_ENABLED),
            0
        );

        let mut changed = 0;
        for quest_fk in quests {
            changed += u32::from(clear_qust_autostart_for_pair(
                Game::Fo76,
                Game::Fo4,
                target_quests.get_mut(&quest_fk).unwrap(),
                &interner,
            ));
        }
        assert_eq!(changed, 1, "only the helper carried source SGE");
        assert_eq!(
            quest_flags(&target_quests[&wayward_quest_fk], &interner).unwrap()
                & u64::from(QUST_FLAG_START_GAME_ENABLED),
            0,
            "the main quest remains non-SGE"
        );
        assert_eq!(
            quest_flags(&target_quests[&lacey_quest_fk], &interner).unwrap()
                & u64::from(QUST_FLAG_START_GAME_ENABLED),
            0,
            "the Story Manager helper loses source SGE"
        );
        assert_ne!(
            quest_flags(&target_quests[&non_selected_quest_fk], &interner).unwrap()
                & u64::from(QUST_FLAG_START_GAME_ENABLED),
            0,
            "a non-selected quest is untouched"
        );
    }

    #[test]
    fn condition_invalid_story_manager_nodes_do_not_claim_quest_autostart() {
        let interner = StringInterner::new();
        let (mut graph, _wayward_node_fk, lacey_node_fk) = scpt_quest_start_graph(&interner);
        let wayward_quest_fk = fk(&interner, 0x405E14);
        let lacey_quest_fk = fk(&interner, 0x405E15);
        let translated = FxHashSet::from_iter([wayward_quest_fk, lacey_quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);
        let emitted_nodes = selection.selected_nodes.clone();
        assert_eq!(
            emitted_story_manager_quests_requiring_autostart_clear(
                &selection,
                &emitted_nodes,
                &graph,
            ),
            vec![wayward_quest_fk, lacey_quest_fk],
        );

        // The FO76 radio-station shape: the node is emitted, but its start
        // condition never normalized, so it can never select the quest.
        graph
            .condition_diagnostics
            .push(StoryManagerConditionDiagnostic {
                form_key: lacey_node_fk,
                message: "quest_start_unqualified:start_keywords=1 valid_start_keywords=0"
                    .to_string(),
            });

        assert_eq!(
            emitted_story_manager_quests_requiring_autostart_clear(
                &selection,
                &emitted_nodes,
                &graph,
            ),
            vec![wayward_quest_fk],
            "a quest whose only emitted node has an unnormalized start condition keeps its autostart bit",
        );
    }

    #[test]
    fn always_on_radio_station_quests_keep_source_autostart_when_nodes_emit() {
        let interner = StringInterner::new();
        let node = fk(&interner, 0x4E2A02);
        let radio_quests = ALWAYS_ON_RADIO_STATION_QUESTS
            .iter()
            .map(|local| fk(&interner, *local))
            .collect::<FxHashSet<_>>();
        let mut selection = StoryManagerSelection::default();
        selection.selected_quests_by_node.insert(node, radio_quests);

        assert!(
            emitted_story_manager_quests_requiring_autostart_clear(
                &selection,
                &FxHashSet::from_iter([node]),
                &StoryManagerSourceGraph::default(),
            )
            .is_empty(),
            "always-on station controllers cannot rely on unproduced SCPT routes",
        );
    }

    #[test]
    fn non_fo76_source_pairs_do_not_normalize_story_manager_conditions() {
        for source in [Game::SkyrimSe, Game::Fnv] {
            let interner = StringInterner::new();
            let (mut graph, wayward_node_fk, _) = scpt_quest_start_graph(&interner);

            let diagnostics = normalize_story_manager_quest_start_conditions_for_pair(
                source,
                Game::Fo4,
                &mut graph,
                &interner,
            );

            assert!(diagnostics.is_empty());
            assert_eq!(
                condition_function_ids(&graph.nodes[&wayward_node_fk]),
                vec![576, 560, 857],
                "{source:?} source conditions must not pass through FO76 lowering"
            );
            assert_eq!(condition_count(&graph.nodes[&wayward_node_fk]), 3);
        }
    }

    #[test]
    fn wrong_start_keyword_role_does_not_qualify_node() {
        let interner = StringInterner::new();
        let (mut graph, wayward_node_fk, _) = scpt_quest_start_graph(&interner);
        graph
            .keywords
            .get_mut(&fk(&interner, 0x405EC6))
            .unwrap()
            .eid = Some(interner.intern("W05_MQ_001P_Wayward_QuestActiveKeyword"));

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.form_key == wayward_node_fk
                && diagnostic.message.contains("valid_start_keywords=0")
        }));
        assert_eq!(
            condition_function_ids(&graph.nodes[&wayward_node_fk]),
            vec![576, 560, 857]
        );
        assert_eq!(condition_count(&graph.nodes[&wayward_node_fk]), 3);
    }

    #[test]
    fn sole_untyped_k1_selector_qualifies_quest_start_node() {
        let interner = StringInterner::new();
        let (mut graph, wayward_node_fk, _) = scpt_quest_start_graph(&interner);
        graph
            .keywords
            .get_mut(&fk(&interner, 0x405EC6))
            .unwrap()
            .eid = Some(interner.intern("LookoutTowerQuestKeyword"));
        let node = graph.nodes.get_mut(&wayward_node_fk).unwrap();
        let mut retained_start = false;
        node.fields.retain(|entry| {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                return true;
            }
            if retained_start {
                return false;
            }
            retained_start = true;
            true
        });
        node.sync_condition_count();

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            condition_function_ids(&graph.nodes[&wayward_node_fk]),
            vec![576]
        );
        assert_eq!(condition_count(&graph.nodes[&wayward_node_fk]), 1);
    }

    #[test]
    fn workshop_vertibird_grenade_keyword_is_a_start_selector() {
        let interner = StringInterner::new();
        let record = keyword(&interner, 0x04DFDE, "WorkshopVertibirdGrenadeKW");

        assert_eq!(
            story_manager_keyword_role(&record, &interner),
            Some(StoryManagerKeywordRole::Start)
        );
    }

    #[test]
    fn start_quest_keyword_suffix_is_a_bounded_start_selector() {
        let interner = StringInterner::new();
        let exact = keyword(&interner, 0x06EFC6, "MTR06_PhysicalExamStartQuestKeyword");
        assert_eq!(
            story_manager_keyword_role(&exact, &interner),
            Some(StoryManagerKeywordRole::Start)
        );

        for editor_id in [
            "MTR06_PhysicalExamStartQuestKeywordState",
            "MTR06_PhysicalExamStartQuestMarkerKeyword",
            "MTR06_PhysicalExamQuestKeyword",
        ] {
            let lookalike = keyword(&interner, 0x06EFC7, editor_id);
            assert_eq!(story_manager_keyword_role(&lookalike, &interner), None);
        }
    }

    #[test]
    fn quest_role_tokens_reject_substring_lookalikes() {
        let interner = StringInterner::new();
        for (local, editor_id) in [
            (0x56C830, "COMP_Keyword_QuestStarter_Astronaut_Outtro"),
            (0x56C831, "COMP_Keyword_QuestStartState_Astronaut_Outtro"),
            (0x57319A, "COMP_Keyword_QuestActiveState_Astronaut_Outtro"),
            (0x57319B, "COMP_Keyword_InQuestActive_Astronaut_Outtro"),
        ] {
            let lookalike = keyword(&interner, local, editor_id);
            assert_eq!(story_manager_keyword_role(&lookalike, &interner), None);
        }
    }

    #[test]
    fn unresolved_second_k1_start_row_disqualifies_the_whole_node() {
        let interner = StringInterner::new();
        let (mut graph, wayward_node_fk, _) = scpt_quest_start_graph(&interner);
        let node = graph.nodes.get_mut(&wayward_node_fk).unwrap();
        let insert_at = node
            .fields
            .iter()
            .position(|entry| entry.sig == nnam_sig())
            .unwrap();
        node.fields.insert(
            insert_at,
            field(
                "CTDA",
                raw_ctda("000B00000000803F4002944300004B31563412000000000000000000FFFFFFFF"),
            ),
        );
        node.sync_condition_count();

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.form_key == wayward_node_fk
                && diagnostic
                    .message
                    .contains("start_keywords=2 valid_start_keywords=1")
        }));
        assert_eq!(
            condition_function_ids(&graph.nodes[&wayward_node_fk]),
            vec![576, 560, 857, 576]
        );
        assert_eq!(condition_count(&graph.nodes[&wayward_node_fk]), 4);
    }

    #[test]
    fn ambiguous_active_keyword_relation_is_reported_and_left_unchanged() {
        let interner = StringInterner::new();
        let (mut graph, wayward_node_fk, _) = scpt_quest_start_graph(&interner);
        let other_quest_fk = fk(&interner, 0x123456);
        let mut other_quest = record(
            &interner,
            "QUST",
            other_quest_fk.local,
            Some("AnotherQuestWithTheSameActiveKeyword"),
        );
        other_quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(fk(&interner, 0x405EC8))));
        graph.quests.insert(other_quest_fk, other_quest);

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.form_key == wayward_node_fk
                && diagnostic
                    .message
                    .contains("active_keyword_ambiguous:405EC8:quest_count=2")
        }));
        assert_eq!(
            condition_function_ids(&graph.nodes[&wayward_node_fk]),
            vec![576, 560, 857]
        );
        assert_eq!(condition_count(&graph.nodes[&wayward_node_fk]), 3);
    }

    #[test]
    fn mixed_mapped_and_ambiguous_active_gates_leave_the_whole_node_unchanged() {
        let interner = StringInterner::new();
        let (mut graph, wayward_node_fk, _) = scpt_quest_start_graph(&interner);
        let ambiguous_keyword_fk = fk(&interner, 0x123456);
        let node = graph.nodes.get_mut(&wayward_node_fk).unwrap();
        let insert_at = node
            .fields
            .iter()
            .position(|entry| entry.sig == nnam_sig())
            .unwrap();
        node.fields.insert(
            insert_at,
            field(
                "CTDA",
                raw_ctda("0000000000000000300294435634120000000000070000000000000052330000"),
            ),
        );
        node.sync_condition_count();
        graph.keywords.insert(
            ambiguous_keyword_fk,
            keyword(
                &interner,
                ambiguous_keyword_fk.local,
                "AmbiguousQuestActiveKeyword",
            ),
        );
        for quest_local in [0x123457, 0x123458] {
            let quest_fk = fk(&interner, quest_local);
            let mut quest = record(
                &interner,
                "QUST",
                quest_local,
                Some("QuestSharingAmbiguousActiveKeyword"),
            );
            quest
                .fields
                .push(field("KWDA", FieldValue::FormKey(ambiguous_keyword_fk)));
            graph.quests.insert(quest_fk, quest);
        }

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.form_key == wayward_node_fk
                && diagnostic
                    .message
                    .contains("active_keyword_ambiguous:123456:quest_count=2")
        }));
        assert_eq!(
            condition_function_ids(&graph.nodes[&wayward_node_fk]),
            vec![576, 560, 857, 560],
            "a mixed-safe node must not be partially lowered"
        );
        assert_eq!(condition_count(&graph.nodes[&wayward_node_fk]), 4);
    }

    #[test]
    fn qualified_predecessor_node_lowers_every_exact_completion_gate() {
        let interner = StringInterner::new();
        let (mut graph, _, _) = scpt_quest_start_graph(&interner);
        let branch_fk = fk(&interner, 0x3FBBBD);
        let node_fk = fk(&interner, 0x594DFE);
        let quest_fk = fk(&interner, 0x594DFD);
        let active_keyword_fk = fk(&interner, 0x594DFA);
        let mut node = record(
            &interner,
            "SMQN",
            node_fk.local,
            Some("W05_MQ_001p_Wayward_MiscPointer_QuestNode"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&4_u32.to_le_bytes())));
        for ctda in [
            "000B00000000803F4002944300004B31FB4D59000000000000000000FFFFFFFF",
            "000000000000000030029443FA4D590000000000070000000000000052330000",
            "A00000000000000059039443145E400000000000070000000000000052330000",
            "A00000000000000059039443FD4D590000000000070000000000000052330000",
        ] {
            node.fields.push(field("CTDA", raw_ctda(ctda)));
        }
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));
        let mut quest = record(
            &interner,
            "QUST",
            quest_fk.local,
            Some("W05_MQ_001P_Wayward_MiscPointer"),
        );
        quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(active_keyword_fk)));
        graph.nodes.insert(node_fk, node);
        graph.quests.insert(quest_fk, quest);
        for keyword_record in [
            keyword(
                &interner,
                0x594DFB,
                "W05_Wayward_MiscPointer_QuestStartKeyword",
            ),
            keyword(
                &interner,
                0x594DFA,
                "W05_Wayward_MiscPointer_QuestActiveKeyword",
            ),
        ] {
            graph
                .keywords
                .insert(keyword_record.form_key, keyword_record);
        }

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let node = &graph.nodes[&node_fk];
        assert_eq!(condition_function_ids(node), vec![576, 56, 543, 543]);
        assert_eq!(condition_parameter_1(node, 1), quest_fk.local);
        assert_eq!(condition_parameter_1(node, 2), 0x405E14);
        assert_eq!(condition_parameter_1(node, 3), quest_fk.local);
        assert_eq!(condition_count(node), 4);
    }

    #[test]
    fn mtr01_alias_relations_lower_own_and_sibling_active_gates() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, 0x029152);
        let branch_fk = fk(&interner, 0x0616CF);
        let misc_node_fk = fk(&interner, 0x056F65);
        let intro_node_fk = fk(&interner, 0x056FB9);
        let misc_quest_fk = fk(&interner, 0x0554A5);
        let intro_quest_fk = fk(&interner, 0x056F63);
        let misc_active_fk = fk(&interner, 0x054E37);
        let intro_active_fk = fk(&interner, 0x055473);

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(&interner, "SMBN", branch_fk.local, Some("MTR01Branch"));
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut misc_node = record(
            &interner,
            "SMQN",
            misc_node_fk.local,
            Some("MTR01MiscQuestNode"),
        );
        misc_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        misc_node
            .fields
            .push(field("CITC", bytes(&5_u32.to_le_bytes())));
        for ctda in [
            "00000000000000004A000000006E0300000000000000000000000000FFFFFFFF",
            "000000000000000030020000374E050000000000070000000000000052330000",
            "0000000000000000300200007354050000000000070000000000000052330000",
            "00000000000000000E000000756F050000000000070000000000000052330000",
            "000000000000803F4002000000004B31C05205000000000000000000FFFFFFFF",
        ] {
            misc_node.fields.push(field("CTDA", raw_ctda(ctda)));
        }
        misc_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(misc_quest_fk)));
        let mut intro_node = record(
            &interner,
            "SMQN",
            intro_node_fk.local,
            Some("MTR01IntroQuestNode"),
        );
        intro_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        intro_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(intro_quest_fk)));

        let mut misc_quest = record(&interner, "QUST", misc_quest_fk.local, Some("MTR01_Misc"));
        misc_quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(misc_active_fk)));
        let mut intro_quest = record(&interner, "QUST", intro_quest_fk.local, Some("MTR01_Intro"));
        intro_quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(intro_active_fk)));

        let mut graph = StoryManagerSourceGraph::default();
        graph.nodes.insert(root_fk, root);
        graph.nodes.insert(branch_fk, branch);
        graph.nodes.insert(misc_node_fk, misc_node);
        graph.nodes.insert(intro_node_fk, intro_node);
        graph.quests.insert(misc_quest_fk, misc_quest);
        graph.quests.insert(intro_quest_fk, intro_quest);
        for keyword_record in [
            keyword(&interner, 0x0552C0, "MTR01_Misc_StartKeyword"),
            keyword(&interner, 0x054E37, "MTR01_Misc_ActiveKeyword"),
            keyword(&interner, 0x055473, "MTR01_Intro_ActiveKeyword"),
        ] {
            graph
                .keywords
                .insert(keyword_record.form_key, keyword_record);
        }

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let misc_node = &graph.nodes[&misc_node_fk];
        assert_eq!(condition_function_ids(misc_node), vec![74, 56, 56, 14, 576]);
        assert_eq!(condition_parameter_1(misc_node, 1), misc_quest_fk.local);
        assert_eq!(
            condition_parameter_1(misc_node, 2),
            intro_quest_fk.local,
            "the sibling active keyword must gate on the sibling quest"
        );
        assert_eq!(condition_count(misc_node), 5);
    }

    #[test]
    fn unrelated_r3_has_keyword_gate_remains_unchanged() {
        let interner = StringInterner::new();
        let (mut graph, wayward_node_fk, _) = scpt_quest_start_graph(&interner);
        let unrelated_keyword_fk = fk(&interner, 0x123456);
        let node = graph.nodes.get_mut(&wayward_node_fk).unwrap();
        let insert_at = node
            .fields
            .iter()
            .position(|entry| entry.sig == nnam_sig())
            .unwrap();
        node.fields.insert(
            insert_at,
            field(
                "CTDA",
                raw_ctda("0000000000000000300294435634120000000000070000000000000052330000"),
            ),
        );
        node.sync_condition_count();
        graph.keywords.insert(
            unrelated_keyword_fk,
            keyword(&interner, unrelated_keyword_fk.local, "SomeGameplayKeyword"),
        );

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let node = &graph.nodes[&wayward_node_fk];
        assert_eq!(condition_function_ids(node), vec![576, 56, 543, 560]);
        assert_eq!(condition_parameter_1(node, 3), unrelated_keyword_fk.local);
        assert_eq!(condition_count(node), 4);
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

    struct RouteSeedFixture {
        graph: StoryManagerSourceGraph,
        selection: StoryManagerSelection,
        source_to_target: FxHashMap<FormKey, FormKey>,
        target_records: FxHashMap<FormKey, Record>,
        emitted_nodes: FxHashSet<FormKey>,
        bridge_roots: FxHashSet<FormKey>,
        output_plugin: crate::sym::Sym,
        source_root: FormKey,
        source_node: FormKey,
        source_quest: FormKey,
        target_node: FormKey,
        target_branch: FormKey,
    }

    fn route_seed_fixture(
        interner: &StringInterner,
        event_type: u32,
        bridge: bool,
    ) -> RouteSeedFixture {
        let (mut graph, source_node, source_quest) = graph_with_radio(interner);
        let source_root = fk(interner, 0x029152);
        graph.nodes.get_mut(&source_root).unwrap().fields[0].value =
            FieldValue::Uint(u64::from(event_type));
        graph
            .quests
            .get_mut(&source_quest)
            .unwrap()
            .fields
            .push(field("ENAM", FieldValue::Uint(u64::from(event_type))));

        let selection =
            classify_story_manager_records(&graph, &FxHashSet::from_iter([source_quest]), interner);
        let output_plugin = interner.intern("B21_SeventySix.esp");
        let fallout4 = interner.intern("Fallout4.esm");
        let target_quest = FormKey {
            local: 0x900001,
            plugin: output_plugin,
        };
        let target_node = FormKey {
            local: 0x900002,
            plugin: output_plugin,
        };
        let target_branch = FormKey {
            local: 0x900003,
            plugin: output_plugin,
        };
        let target_root = if bridge {
            FormKey {
                local: 0x900004,
                plugin: output_plugin,
            }
        } else {
            FormKey {
                local: FO4_SM_EVENT_ROOTS
                    .iter()
                    .find_map(|(candidate, local)| (*candidate == event_type).then_some(*local))
                    .expect("native event root"),
                plugin: fallout4,
            }
        };
        let source_branch = fk(interner, 0x4E34B9);
        let mut source_to_target = FxHashMap::default();
        source_to_target.insert(source_quest, target_quest);
        source_to_target.insert(source_node, target_node);
        source_to_target.insert(source_branch, target_branch);
        source_to_target.insert(source_root, target_root);

        let target_event = if bridge { SCPT_EVENT_TYPE } else { event_type };
        let mut target_quest_record = Record::new(SigCode::from_str("QUST").unwrap(), target_quest);
        target_quest_record
            .fields
            .push(field("ENAM", FieldValue::Uint(u64::from(target_event))));
        let mut target_node_record = Record::new(SigCode::from_str("SMQN").unwrap(), target_node);
        target_node_record
            .fields
            .push(field("PNAM", FieldValue::FormKey(target_branch)));
        target_node_record
            .fields
            .push(field("NNAM", FieldValue::FormKey(target_quest)));
        let mut target_branch_record =
            Record::new(SigCode::from_str("SMBN").unwrap(), target_branch);
        target_branch_record
            .fields
            .push(field("PNAM", FieldValue::FormKey(target_root)));

        let mut target_records = FxHashMap::default();
        target_records.insert(target_quest, target_quest_record);
        target_records.insert(target_node, target_node_record);
        target_records.insert(target_branch, target_branch_record);
        let mut bridge_roots = FxHashSet::default();
        if bridge {
            bridge_roots.insert(source_root);
            let mut target_root_record =
                Record::new(SigCode::from_str("SMBN").unwrap(), target_root);
            target_root_record.fields.push(field(
                "PNAM",
                FieldValue::FormKey(FormKey {
                    local: FO4_SCRIPT_EVENT_ROOT_LOCAL,
                    plugin: fallout4,
                }),
            ));
            target_root_record.fields.push(field(
                "CTDA",
                FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
                    source_root.local,
                ))),
            ));
            target_records.insert(target_root, target_root_record);
        }

        RouteSeedFixture {
            graph,
            selection,
            source_to_target,
            target_records,
            emitted_nodes: FxHashSet::from_iter([source_root, source_branch, source_node]),
            bridge_roots,
            output_plugin,
            source_root,
            source_node,
            source_quest,
            target_node,
            target_branch,
        }
    }

    fn route_seed_report(
        interner: &StringInterner,
        fixture: &RouteSeedFixture,
        source: Game,
    ) -> StoryManagerRouteSeedReport {
        build_story_manager_route_seed_for_pair(
            source,
            Game::Fo4,
            &fixture.graph,
            &fixture.selection,
            &fixture.source_to_target,
            &fixture.target_records,
            &fixture.emitted_nodes,
            &fixture.bridge_roots,
            fixture.output_plugin,
            interner,
        )
    }

    #[test]
    fn route_seed_marks_all_fo76_only_roots_as_unproven_script_event_bridges() {
        for event_type in Fo76Fo4Hook::fo76_only_qust_event_types() {
            let interner = StringInterner::new();
            let fixture = route_seed_fixture(&interner, *event_type, true);

            let report = route_seed_report(&interner, &fixture, Game::Fo76);

            assert_eq!(report.routes.len(), 1);
            let route = &report.routes[0];
            assert_eq!(route.source_root_event, Some(event_type_name(*event_type)));
            assert_eq!(route.target_event.as_deref(), Some("SCPT"));
            assert!(route.fo76_only_event);
            assert_eq!(route.emission_status, "emitted");
            assert_eq!(route.structural_status, "eligible");
            assert_eq!(route.producer_status, "unproven");
            assert_eq!(
                route.bridge_keyword,
                Some(
                    FormKey {
                        local: fixture.source_root.local,
                        plugin: fixture.output_plugin,
                    }
                    .format(&interner)
                )
            );
            assert_eq!(
                route.target_event_root,
                Some(
                    FormKey {
                        local: FO4_SCRIPT_EVENT_ROOT_LOCAL,
                        plugin: interner.intern("Fallout4.esm"),
                    }
                    .format(&interner)
                )
            );
        }
    }

    #[test]
    fn route_seed_distinguishes_native_cloc_and_scpt_selector_routes() {
        let interner = StringInterner::new();
        let cloc = route_seed_fixture(&interner, fourcc(b"CLOC"), false);
        let cloc_report = route_seed_report(&interner, &cloc, Game::Fo76);
        let cloc_route = &cloc_report.routes[0];
        assert_eq!(cloc_route.target_event.as_deref(), Some("CLOC"));
        assert_eq!(cloc_route.structural_status, "eligible");
        assert_eq!(cloc_route.producer_status, "not_required");
        assert!(cloc_route.target_selector_keywords.is_empty());

        let mut scpt = route_seed_fixture(&interner, SCPT_EVENT_TYPE, false);
        let source_selector = 0x011111;
        let target_selector = 0x022222;
        scpt.graph
            .nodes
            .get_mut(&scpt.source_node)
            .unwrap()
            .fields
            .push(field(
                "CTDA",
                FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
                    source_selector,
                ))),
            ));
        scpt.target_records
            .get_mut(&scpt.target_node)
            .unwrap()
            .fields
            .push(field(
                "CTDA",
                FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
                    target_selector,
                ))),
            ));
        let scpt_report = route_seed_report(&interner, &scpt, Game::Fo76);
        let scpt_route = &scpt_report.routes[0];
        assert_eq!(
            scpt_route.source_selector_keywords,
            vec![fk(&interner, source_selector).format(&interner)]
        );
        assert_eq!(
            scpt_route.target_selector_keywords,
            vec![
                FormKey {
                    local: target_selector,
                    plugin: scpt.output_plugin,
                }
                .format(&interner)
            ]
        );
        assert!(scpt_route.condition_valid);
        assert_eq!(scpt_route.producer_status, "unproven");
    }

    #[test]
    fn route_seed_accepts_quest_level_scpt_selector() {
        let interner = StringInterner::new();
        let mut scpt = route_seed_fixture(&interner, SCPT_EVENT_TYPE, false);
        let source_selector = 0x011111;
        let target_selector = 0x022222;
        let target_quest = scpt.source_to_target[&scpt.source_quest];
        scpt.graph
            .quests
            .get_mut(&scpt.source_quest)
            .unwrap()
            .fields
            .push(field(
                "CTDA",
                FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
                    source_selector,
                ))),
            ));
        scpt.target_records
            .get_mut(&target_quest)
            .unwrap()
            .fields
            .push(field(
                "CTDA",
                FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
                    target_selector,
                ))),
            ));

        let report = route_seed_report(&interner, &scpt, Game::Fo76);
        let route = &report.routes[0];

        assert_eq!(
            route.source_selector_keywords,
            vec![fk(&interner, source_selector).format(&interner)]
        );
        assert_eq!(
            route.target_selector_keywords,
            vec![
                FormKey {
                    local: target_selector,
                    plugin: scpt.output_plugin,
                }
                .format(&interner)
            ]
        );
        assert!(route.condition_valid);
        assert_eq!(route.structural_status, "eligible");
    }

    #[test]
    fn route_seed_includes_orphans_and_is_deterministically_sorted() {
        let interner = StringInterner::new();
        let mut fixture = route_seed_fixture(&interner, fourcc(b"CLOC"), false);
        for local in [0x700002, 0x700001] {
            let source_quest = fk(&interner, local);
            let mut source_record = record(
                &interner,
                "QUST",
                local,
                Some(&format!("Orphan_{local:06X}")),
            );
            source_record
                .fields
                .push(field("ENAM", FieldValue::Uint(u64::from(fourcc(b"CBGN")))));
            source_record
                .fields
                .push(field("QETL", FieldValue::Uint(1)));
            source_record.fields.push(field(
                "QSSK",
                FieldValue::FormKey(fk(&interner, local + 0x10)),
            ));
            for sig in ["QUCF", "QSDD", "QETE"] {
                source_record.fields.push(field(sig, FieldValue::Uint(1)));
            }
            fixture
                .graph
                .event_metadata_quests
                .insert(source_quest, source_record);
            let target_quest = FormKey {
                local: local + 0x100000,
                plugin: fixture.output_plugin,
            };
            fixture.source_to_target.insert(source_quest, target_quest);
            fixture.target_records.insert(
                target_quest,
                Record::new(SigCode::from_str("QUST").unwrap(), target_quest),
            );
        }

        let first = route_seed_report(&interner, &fixture, Game::Fo76);
        let second = route_seed_report(&interner, &fixture, Game::Fo76);

        assert_eq!(first, second);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
        assert_eq!(
            first
                .routes
                .iter()
                .map(|route| route.source_quest.clone())
                .collect::<Vec<_>>(),
            vec![
                fixture.source_quest.format(&interner),
                fk(&interner, 0x700001).format(&interner),
                fk(&interner, 0x700002).format(&interner),
            ]
        );
        let orphan = first
            .routes
            .iter()
            .find(|route| route.source_quest.starts_with("700001"))
            .unwrap();
        assert_eq!(orphan.emission_status, "orphan");
        assert_eq!(orphan.structural_status, "orphan");
        assert_eq!(orphan.producer_status, "not_applicable");
        assert_eq!(
            orphan.source_metadata_fields,
            vec!["ENAM", "QETL", "QSSK", "QUCF", "QSDD", "QETE"]
        );
        assert_eq!(
            orphan.source_start_keyword,
            Some(fk(&interner, 0x700011).format(&interner))
        );
    }

    #[test]
    fn route_seed_records_skips_condition_failures_and_conflicting_k1_selectors() {
        let interner = StringInterner::new();
        let mut skipped = route_seed_fixture(&interner, SCPT_EVENT_TYPE, false);
        skipped
            .graph
            .quests
            .get_mut(&skipped.source_quest)
            .unwrap()
            .eid = Some(interner.intern("TestUnsafeQuest"));
        skipped.selection = classify_story_manager_records(
            &skipped.graph,
            &FxHashSet::from_iter([skipped.source_quest]),
            &interner,
        );
        skipped.emitted_nodes.clear();
        skipped
            .graph
            .condition_diagnostics
            .push(StoryManagerConditionDiagnostic {
                form_key: skipped.source_node,
                message: "quest_start_unqualified:no_active_keyword_role".to_string(),
            });

        let skipped_report = route_seed_report(&interner, &skipped, Game::Fo76);
        let skipped_route = &skipped_report.routes[0];
        assert_eq!(skipped_route.emission_status, "skipped");
        assert_eq!(
            skipped_route.skip_reason.as_deref(),
            Some("unsupported_quest")
        );
        assert!(!skipped_route.condition_valid);
        assert!(skipped_route.issues.iter().any(|issue| {
            issue == "condition_normalization:quest_start_unqualified:no_active_keyword_role"
        }));

        let mut conflict = route_seed_fixture(&interner, SCPT_EVENT_TYPE, false);
        for (target, selector) in [
            (conflict.target_node, 0x011111),
            (conflict.target_branch, 0x022222),
        ] {
            conflict
                .target_records
                .get_mut(&target)
                .unwrap()
                .fields
                .push(field(
                    "CTDA",
                    FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
                        selector,
                    ))),
                ));
        }
        let conflict_report = route_seed_report(&interner, &conflict, Game::Fo76);
        let conflict_route = &conflict_report.routes[0];
        assert!(!conflict_route.condition_valid);
        assert_eq!(conflict_route.structural_status, "invalid");
        assert!(
            conflict_route
                .issues
                .contains(&"conflicting_mandatory_k1_selectors".to_string())
        );
    }

    #[test]
    fn route_seed_is_pair_gated() {
        let interner = StringInterner::new();
        let fixture = route_seed_fixture(&interner, fourcc(b"CLOC"), false);

        let report = route_seed_report(&interner, &fixture, Game::Fnv);

        assert_eq!(report.source_game, "fnv");
        assert_eq!(report.target_game, "fo4");
        assert!(report.routes.is_empty());
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
    fn synthesizes_better_tomorrow_node_from_exact_content_manager_contract() {
        let interner = StringInterner::new();
        let mut content_manager = record(
            &interner,
            "DCGF",
            BETTER_TOMORROW_CONTENT_MANAGER_LOCAL,
            Some(BETTER_TOMORROW_CONTENT_MANAGER_EID),
        );
        content_manager.fields.push(field(
            "DCGQ",
            FieldValue::FormKey(fk(&interner, BETTER_TOMORROW_QUEST_LOCAL)),
        ));
        content_manager.fields.push(field(
            "DCGL",
            FieldValue::FormKey(fk(&interner, BETTER_TOMORROW_REGION_LOCAL)),
        ));

        let node = better_tomorrow_story_manager_node(&content_manager, &interner)
            .expect("exact DCGF contract should synthesize a node");

        assert_eq!(node.sig, smqn_sig());
        assert_eq!(node.form_key, content_manager.form_key);
        assert_eq!(
            record_editor_id(&node, &interner).as_deref(),
            Some("NPE_DQ01_BetterTomorrow_QuestNode")
        );
        assert_eq!(
            story_manager_parent(&node).unwrap().local,
            FO4_SCRIPT_EVENT_ROOT_LOCAL
        );
        assert_eq!(
            story_manager_quests(&node),
            vec![fk(&interner, BETTER_TOMORROW_QUEST_LOCAL)]
        );
        assert_eq!(condition_count(&node), 1);
        let FieldValue::Bytes(condition) = &node
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CTDA")
            .unwrap()
            .value
        else {
            panic!("CTDA bytes")
        };
        assert_eq!(
            u32::from_le_bytes(condition[16..20].try_into().unwrap()),
            BETTER_TOMORROW_START_KEYWORD_LOCAL
        );

        let root_fk = fk(&interner, FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let quest_fk = fk(&interner, BETTER_TOMORROW_QUEST_LOCAL);
        let keyword_fk = fk(&interner, BETTER_TOMORROW_START_KEYWORD_LOCAL);
        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut quest = record(
            &interner,
            "QUST",
            quest_fk.local,
            Some("NPE_DQ01_BetterTomorrow"),
        );
        quest
            .fields
            .push(field("ENAM", FieldValue::Uint(u64::from(SCPT_EVENT_TYPE))));
        let mut graph = StoryManagerSourceGraph::default();
        graph.nodes.insert(root_fk, root);
        graph.nodes.insert(node.form_key, node);
        graph.quests.insert(quest_fk, quest);
        graph.keywords.insert(
            keyword_fk,
            keyword(
                &interner,
                BETTER_TOMORROW_START_KEYWORD_LOCAL,
                "NPE_DQ01_BetterTomorrow_QuestStartKeyword",
            ),
        );

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let selection =
            classify_story_manager_records(&graph, &FxHashSet::from_iter([quest_fk]), &interner);
        assert!(selection.selected_nodes.contains(&content_manager.form_key));
    }

    #[test]
    fn does_not_synthesize_better_tomorrow_node_for_changed_region_contract() {
        let interner = StringInterner::new();
        let mut content_manager = record(
            &interner,
            "DCGF",
            BETTER_TOMORROW_CONTENT_MANAGER_LOCAL,
            Some(BETTER_TOMORROW_CONTENT_MANAGER_EID),
        );
        content_manager.fields.push(field(
            "DCGQ",
            FieldValue::FormKey(fk(&interner, BETTER_TOMORROW_QUEST_LOCAL)),
        ));
        content_manager.fields.push(field(
            "DCGL",
            FieldValue::FormKey(fk(&interner, BETTER_TOMORROW_REGION_LOCAL + 1)),
        ));

        assert!(better_tomorrow_story_manager_node(&content_manager, &interner).is_none());
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
        let mut kill_event = record(&interner, "SMEN", 0x1003, Some("KillEvent"));
        kill_event.fields.push(field("ENAM", bytes(b"KILL")));

        assert_eq!(fo4_story_manager_event_root(&script_event), Some(0x029152));
        assert_eq!(fo4_story_manager_event_root(&mine_event), Some(0x02A68B));
        assert_eq!(fo4_story_manager_event_root(&fo76_only), None);
        assert_eq!(fo4_story_manager_event_root(&kill_event), None);
    }

    #[test]
    fn kill_event_root_is_unproven_and_must_fail_before_bridge_emission() {
        let interner = StringInterner::new();
        let root = fk(&interner, 0x4100);
        let mut kill = record(&interner, "SMEN", root.local, Some("KillEvent"));
        kill.fields.push(field("ENAM", bytes(b"KILL")));
        let graph = StoryManagerSourceGraph {
            nodes: FxHashMap::from_iter([(root, kill)]),
            ..Default::default()
        };
        let selected = FxHashSet::from_iter([root]);

        assert_eq!(
            unproven_story_manager_event_roots(&selected, &graph),
            vec![(root, fourcc(b"KILL"))]
        );
        assert_eq!(
            incompatible_story_manager_event_roots(&selected, &graph),
            vec![(root, fourcc(b"KILL"))]
        );
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
            &interner,
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
        let hack_root = fk(&interner, 0x3001);
        let mut graph = StoryManagerSourceGraph::default();
        let mut quest = record(&interner, "QUST", quest_fk.local, Some("SQ_Conflict"));
        quest.fields.push(field("ENAM", bytes(b"ILOC")));
        graph.quests.insert(quest_fk, quest);
        let mut incompatible = record(&interner, "SMEN", incompatible_root.local, None);
        incompatible.fields.push(field("ENAM", bytes(b"ILOC")));
        graph.nodes.insert(incompatible_root, incompatible);
        let mut hack = record(&interner, "SMEN", hack_root.local, None);
        hack.fields.push(field("ENAM", bytes(b"HACK")));
        graph.nodes.insert(hack_root, hack);
        let mut selection = StoryManagerSelection::default();
        selection.selected_event_roots_by_quest.insert(
            quest_fk,
            FxHashSet::from_iter([incompatible_root, hack_root]),
        );

        let plan = plan_story_manager_quest_events(
            &selection,
            &graph,
            &FxHashMap::from_iter([(incompatible_root, 0x0110_00AB)]),
            &FxHashSet::from_iter([incompatible_root, hack_root]),
            &interner,
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
            FxHashSet::from_iter([SCPT_EVENT_TYPE, fourcc(b"HACK")])
        );
    }

    #[test]
    fn exact_mtr06_alias_adapter_restores_the_physical_exam_story_route() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, 0x029152);
        let branch_fk = fk(&interner, 0x0616CF);
        let node_fk = fk(&interner, 0x077B4C);
        let quest_fk = fk(&interner, 0x00D783);
        let active_keyword_fk = fk(&interner, 0x068345);
        let start_keyword_fk = fk(&interner, 0x06EFC6);

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(&interner, "SMBN", branch_fk.local, Some("MTRQuestBranch"));
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));

        let mut node = record(
            &interner,
            "SMQN",
            node_fk.local,
            Some("MTR06PhysicalExamNode"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&2_u32.to_le_bytes())));
        for ctda in [
            "000A000000000000300265434583060000000000070000000000000052330000",
            "000A00000000803F4002654300004B31C6EF06000000000000000000FFFFFFFF",
        ] {
            node.fields.push(field("CTDA", raw_ctda(ctda)));
        }
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let quest = mtr06_physical_exam_quest(&interner, quest_fk.local, "MTR06_PhysicalExam");
        assert!(qust_has_untranslatable_event_alias(&quest));
        assert_eq!(
            classify_quest(&quest, &interner),
            Some(StoryQuestKind::Generic)
        );

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest_fk, quest);
        for record in [
            keyword(
                &interner,
                active_keyword_fk.local,
                "MTR06_PhysicalExamPlayerActiveKeyword",
            ),
            keyword(
                &interner,
                start_keyword_fk.local,
                "MTR06_PhysicalExamStartQuestKeyword",
            ),
        ] {
            graph.keywords.insert(record.form_key, record);
        }

        let condition_diagnostics =
            normalize_story_manager_quest_start_conditions(&mut graph, &interner);
        assert!(
            condition_diagnostics.is_empty(),
            "{condition_diagnostics:?}"
        );
        assert_eq!(condition_count(&graph.nodes[&node_fk]), 2);
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![56, 576]
        );
        assert_eq!(
            condition_parameter_1(&graph.nodes[&node_fk], 0),
            quest_fk.local
        );
        assert!(graph.nodes[&node_fk].fields.iter().any(|entry| {
            Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
                .is_some_and(|raw| raw & 0x00FF_FFFF == start_keyword_fk.local)
        }));

        let translated = FxHashSet::from_iter([quest_fk]);
        let selection = classify_story_manager_records(&graph, &translated, &interner);
        for selected in [root_fk, branch_fk, node_fk] {
            assert!(selection.selected_nodes.contains(&selected));
        }
        assert_eq!(
            selection.selected_quests_by_node.get(&node_fk),
            Some(&FxHashSet::from_iter([quest_fk]))
        );
        assert_eq!(
            selection.selected_event_roots_by_quest.get(&quest_fk),
            Some(&FxHashSet::from_iter([root_fk]))
        );
        assert!(selection.diagnostics.iter().all(|diagnostic| {
            diagnostic.kind == StoryManagerDiagnosticKind::Selected && diagnostic.reason.is_none()
        }));
        assert_eq!(
            fo4_story_manager_event_root(&graph.nodes[&root_fk]),
            Some(FO4_SCRIPT_EVENT_ROOT_LOCAL)
        );

        let event_plan = plan_story_manager_quest_events(
            &selection,
            &graph,
            &FxHashMap::default(),
            &selection.selected_nodes,
            &interner,
        );
        assert!(event_plan.rewrites.is_empty());
        assert!(event_plan.unresolved.is_empty());

        let source_to_target = graph
            .nodes
            .keys()
            .chain(graph.quests.keys())
            .map(|form_key| (*form_key, *form_key))
            .collect::<FxHashMap<_, _>>();
        let target_records = graph
            .nodes
            .values()
            .chain(graph.quests.values())
            .cloned()
            .map(|record| (record.form_key, record))
            .collect::<FxHashMap<_, _>>();
        let report = build_story_manager_route_seed_for_pair(
            Game::Fo76,
            Game::Fo4,
            &graph,
            &selection,
            &source_to_target,
            &target_records,
            &selection.selected_nodes,
            &FxHashSet::default(),
            interner.intern("SeventySix.esm"),
            &interner,
        );
        let route = report
            .routes
            .iter()
            .find(|route| route.source_node.as_deref() == Some(node_fk.format(&interner).as_str()))
            .expect("MTR06 physical exam route");
        assert_eq!(route.emission_status, "emitted");
        assert_eq!(route.skip_reason, None);
        assert_eq!(route.target_quest, Some(quest_fk.format(&interner)));
        assert_eq!(route.target_event.as_deref(), Some("SCPT"));
        assert_eq!(
            route.source_selector_keywords,
            vec![start_keyword_fk.format(&interner)]
        );
        assert_eq!(
            route.target_selector_keywords,
            route.source_selector_keywords
        );
        assert!(route.condition_valid);
        assert!(route.issues.is_empty());
    }

    #[test]
    fn exact_astronaut_outtro_adapter_restores_the_story_route() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, 0x029152);
        let branch_fk = fk(&interner, 0x55F6CF);
        let node_fk = fk(&interner, 0x573198);
        let quest_fk = fk(&interner, 0x56BF31);
        let start_keyword_fk = fk(&interner, 0x56C82F);
        let active_keyword_fk = fk(&interner, 0x573199);

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(
            &interner,
            "SMBN",
            branch_fk.local,
            Some("Companions_Quests_Unique"),
        );
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = record(
            &interner,
            "SMQN",
            node_fk.local,
            Some("Astronaut_Outtro_Start"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&4_u32.to_le_bytes())));
        for ctda in [
            "000B00000000803F4A0094439C645A00000000000000000000000000FFFFFFFF",
            "000B00000000803F4002944300004B312FC856000000000000000000FFFFFFFF",
            "0000000000000000300294439931570000000000070000000000000052330000",
            "00000000000000005903944331BF560000000000070000000000000052330000",
        ] {
            node.fields.push(field("CTDA", raw_ctda(ctda)));
        }
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let mut quest = record(
            &interner,
            "QUST",
            quest_fk.local,
            Some("COMP_Quest_Outtro_Full_Astronaut"),
        );
        quest
            .fields
            .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(active_keyword_fk)));
        append_event_alias(&mut quest, "ALST", 0, 0, 0x3352);
        append_event_alias(&mut quest, "ALST", 1, 0, 0x3152);
        assert!(qust_has_untranslatable_event_alias(&quest));
        assert_eq!(
            classify_quest(&quest, &interner),
            Some(StoryQuestKind::Generic)
        );

        let start_keyword = keyword(
            &interner,
            start_keyword_fk.local,
            "COMP_Keyword_QuestStart_Astronaut_Outtro",
        );
        let active_keyword = keyword(
            &interner,
            active_keyword_fk.local,
            "COMP_Keyword_QuestActive_Astronaut_Outtro",
        );
        assert_eq!(
            story_manager_keyword_role(&start_keyword, &interner),
            Some(StoryManagerKeywordRole::Start)
        );
        assert_eq!(
            story_manager_keyword_role(&active_keyword, &interner),
            Some(StoryManagerKeywordRole::Active)
        );

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest_fk, quest);
        for record in [start_keyword, active_keyword] {
            graph.keywords.insert(record.form_key, record);
        }

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(condition_count(&graph.nodes[&node_fk]), 4);
        assert_eq!(
            condition_function_ids(&graph.nodes[&node_fk]),
            vec![74, 576, 56, 543]
        );
        assert_eq!(
            condition_parameter_1(&graph.nodes[&node_fk], 2),
            quest_fk.local
        );

        let selection =
            classify_story_manager_records(&graph, &FxHashSet::from_iter([quest_fk]), &interner);
        for selected in [root_fk, branch_fk, node_fk] {
            assert!(selection.selected_nodes.contains(&selected));
        }
        assert_eq!(
            selection.selected_quests_by_node.get(&node_fk),
            Some(&FxHashSet::from_iter([quest_fk]))
        );
        let source_to_target = graph
            .nodes
            .keys()
            .chain(graph.quests.keys())
            .map(|form_key| (*form_key, *form_key))
            .collect::<FxHashMap<_, _>>();
        let target_records = graph
            .nodes
            .values()
            .chain(graph.quests.values())
            .cloned()
            .map(|record| (record.form_key, record))
            .collect::<FxHashMap<_, _>>();
        let report = build_story_manager_route_seed_for_pair(
            Game::Fo76,
            Game::Fo4,
            &graph,
            &selection,
            &source_to_target,
            &target_records,
            &selection.selected_nodes,
            &FxHashSet::default(),
            interner.intern("SeventySix.esm"),
            &interner,
        );
        let route = report
            .routes
            .iter()
            .find(|route| route.source_node.as_deref() == Some(node_fk.format(&interner).as_str()))
            .expect("Astronaut outro route");
        assert_eq!(route.emission_status, "emitted");
        assert_eq!(route.target_event.as_deref(), Some("SCPT"));
        assert_eq!(
            route.target_selector_keywords,
            vec![start_keyword_fk.format(&interner)]
        );
        assert!(route.condition_valid);
        assert!(route.issues.is_empty());

        for (local, editor_id) in [
            (0x56BF32, "COMP_Quest_Outtro_Full_Astronaut"),
            (0x56BF31, "COMP_Quest_Outtro_Full_Astronaut_Copy"),
        ] {
            let mut lookalike = record(&interner, "QUST", local, Some(editor_id));
            append_event_alias(&mut lookalike, "ALST", 0, 0, 0x3352);
            assert!(qust_has_untranslatable_event_alias(&lookalike));
            assert_eq!(classify_quest(&lookalike, &interner), None);
        }
        let mut foreign_lookalike = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey {
                local: quest_fk.local,
                plugin: interner.intern("Foreign.esm"),
            },
        );
        foreign_lookalike.eid = Some(interner.intern("COMP_Quest_Outtro_Full_Astronaut"));
        append_event_alias(&mut foreign_lookalike, "ALST", 0, 0, 0x3352);
        assert!(qust_has_untranslatable_event_alias(&foreign_lookalike));
        assert_eq!(classify_quest(&foreign_lookalike, &interner), None);
    }

    #[test]
    fn mtr06_alias_adapter_rejects_unsafe_lookalikes() {
        let interner = StringInterner::new();
        let wrong_form = mtr06_physical_exam_quest(&interner, 0x00D784, "MTR06_PhysicalExam");
        let wrong_editor_id =
            mtr06_physical_exam_quest(&interner, 0x00D783, "MTR06_PhysicalExam_UnsafeCopy");

        for quest in [&wrong_form, &wrong_editor_id] {
            assert!(qust_has_untranslatable_event_alias(quest));
            assert_eq!(classify_quest(quest, &interner), None);
        }
    }

    #[test]
    fn exact_companion_runtime_adapters_restore_radiant_and_visitor_quests() {
        let interner = StringInterner::new();
        for (local, editor_id) in [
            (0x54F1A4, "COMP_RQ_Fetch"),
            (0x56FB76, "COMP_RQ_Kill"),
            (0x5727AD, "COMP_RQ_Rescue"),
            (0x55FD53, "COMP_Visitor"),
            (
                0x58215B,
                "COMP_RQ_Fetch_SpecificAliases_Beckett_000_SadDiary",
            ),
            (
                0x582164,
                "COMP_RQ_Rescue_SpecificAliases_Beckett_001_CultistSage",
            ),
            (0x582163, "COMP_RQ_Fetch_SpecificAliases_Beckett_002_Key"),
            (0x582160, "COMP_RQ_Kill_SpecificAliases_Beckett_003_Bronx"),
            (0x582167, "COMP_RQ_Fetch_SpecificAliases_Beckett_004_Cave"),
            (0x582165, "COMP_RQ_Kill_SpecificAliases_Beckett_005_Blood"),
            (0x58215A, "COMP_RQ_Rescue_SpecificAliases_Beckett_006_Pet"),
            (0x58215E, "COMP_RQ_Kill_SpecificAliases_Beckett_007_DJ"),
            (
                0x58216A,
                "COMP_RQ_Rescue_SpecificAliases_Beckett_008_MissNanny",
            ),
            (
                0x582168,
                "COMP_RQ_Fetch_SpecificAliases_Beckett_009_Holotapes",
            ),
            (
                0x58215F,
                "COMP_RQ_Fetch_SpecificAliases_Beckett_010_PoisonedFood",
            ),
            (0x58215D, "COMP_RQ_Kill_SpecificAliases_Beckett_011_Eye"),
            (
                0x5A272F,
                "COMP_RQ_Kill_SpecificAliases_Beckett_BloodEagleRandomLoc",
            ),
            (
                0x5A2730,
                "COMP_RQ_Kill_SpecificAliases_Beckett_BloodEagleDungeon",
            ),
            (0x5A272D, "COMP_RQ_Fetch_SpecificAliases_LegendaryArmor"),
            (0x5A272E, "COMP_RQ_Fetch_SpecificAliases_LegendaryWeapon"),
        ] {
            let mut quest = record(&interner, "QUST", local, Some(editor_id));
            append_event_alias(&mut quest, "ALST", 0, 0, 0x3352);
            assert!(qust_has_untranslatable_event_alias(&quest));
            assert_eq!(
                classify_quest(&quest, &interner),
                Some(StoryQuestKind::Generic),
                "{editor_id}"
            );

            let mut wrong_form = record(&interner, "QUST", local + 1, Some(editor_id));
            append_event_alias(&mut wrong_form, "ALST", 0, 0, 0x3352);
            assert_eq!(classify_quest(&wrong_form, &interner), None);

            let mut wrong_editor_id = record(
                &interner,
                "QUST",
                local,
                Some(&format!("{editor_id}_UnsafeCopy")),
            );
            append_event_alias(&mut wrong_editor_id, "ALST", 0, 0, 0x3352);
            assert_eq!(classify_quest(&wrong_editor_id, &interner), None);
        }

        let mut foreign = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey {
                local: 0x54F1A4,
                plugin: interner.intern("Foreign.esm"),
            },
        );
        foreign.eid = Some(interner.intern("COMP_RQ_Fetch"));
        append_event_alias(&mut foreign, "ALST", 0, 0, 0x3352);
        assert_eq!(classify_quest(&foreign, &interner), None);
    }

    #[test]
    fn beckett_specific_alias_adapters_preserve_the_exact_source_story_route() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let companion_branch_fk = fk(&interner, 0x003F_53A3);
        let specific_branch_fk = fk(&interner, 0x0055_934B);
        let node_fk = fk(&interner, 0x0058_2E3F);
        let start_keyword_fk = fk(&interner, 0x0055_934C);

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut companion_branch = record(
            &interner,
            "SMBN",
            companion_branch_fk.local,
            Some("COMP_Quests_Branch"),
        );
        companion_branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut specific_branch = record(
            &interner,
            "SMBN",
            specific_branch_fk.local,
            Some("COMP_RQ_SpecificAliases_Branch"),
        );
        specific_branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(companion_branch_fk)));
        specific_branch
            .fields
            .push(field("CITC", bytes(&1_u32.to_le_bytes())));
        specific_branch.fields.push(field(
            "CTDA",
            raw_ctda("000B00000000803F4002944300004B314C9355000000000000000000FFFFFFFF"),
        ));

        let mut node = record(
            &interner,
            "SMQN",
            node_fk.local,
            Some("COMP_RQ_SpecificAliases_Node_Beckett"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(specific_branch_fk)));
        node.fields.push(field(
            "SNAM",
            FieldValue::FormKey(fk(&interner, 0x0057_25C9)),
        ));
        node.fields
            .push(field("DNAM", bytes(&0x0002_0000_u32.to_le_bytes())));
        node.fields.push(field("XNAM", bytes(&0_u32.to_le_bytes())));
        node.fields.push(field(
            "QNAM",
            bytes(&(BECKETT_SPECIFIC_ALIAS_QUESTS.len() as u32).to_le_bytes()),
        ));
        node.fields.push(field("CITC", bytes(&2_u32.to_le_bytes())));
        for ctda in [
            "000B00000000803F4A00944399645A00000000000000000000000000FFFFFFFF",
            "000B00000000803F48009443BE9C560000000000070000000000000052310000",
        ] {
            node.fields.push(field("CTDA", raw_ctda(ctda)));
        }

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, companion_branch, specific_branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        let start_keyword = keyword(
            &interner,
            start_keyword_fk.local,
            "COMP_RQ_SpecificAliases_Start",
        );
        graph.keywords.insert(start_keyword.form_key, start_keyword);

        let mut quest_fks = Vec::with_capacity(BECKETT_SPECIFIC_ALIAS_QUESTS.len());
        for (quest_local, quest_editor_id) in BECKETT_SPECIFIC_ALIAS_QUESTS {
            let quest_fk = fk(&interner, quest_local);
            let mut quest = record(&interner, "QUST", quest_fk.local, Some(quest_editor_id));
            quest
                .fields
                .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
            append_event_alias(&mut quest, "ALST", 0, 0, 13_138);
            append_event_alias(&mut quest, "ALST", 1, 0, 12_626);
            graph.quests.insert(quest_fk, quest);
            graph
                .nodes
                .get_mut(&node_fk)
                .unwrap()
                .fields
                .push(field("NNAM", FieldValue::FormKey(quest_fk)));
            quest_fks.push(quest_fk);
        }

        let condition_diagnostics =
            normalize_story_manager_quest_start_conditions(&mut graph, &interner);
        assert!(
            condition_diagnostics.is_empty(),
            "{condition_diagnostics:?}"
        );
        let translated = quest_fks.iter().copied().collect::<FxHashSet<_>>();
        let selection = classify_story_manager_records(&graph, &translated, &interner);
        for selected in [root_fk, companion_branch_fk, specific_branch_fk, node_fk] {
            assert!(selection.selected_nodes.contains(&selected));
        }
        assert_eq!(
            selection.selected_quests_by_node.get(&node_fk),
            Some(&translated)
        );
        for quest_fk in &quest_fks {
            assert_eq!(
                selection.selected_event_roots_by_quest.get(quest_fk),
                Some(&FxHashSet::from_iter([root_fk]))
            );
        }

        let event_plan = plan_story_manager_quest_events(
            &selection,
            &graph,
            &FxHashMap::default(),
            &selection.selected_nodes,
            &interner,
        );
        assert!(event_plan.rewrites.is_empty());
        assert!(event_plan.unresolved.is_empty());

        let source_to_target = graph
            .nodes
            .keys()
            .chain(graph.quests.keys())
            .map(|form_key| (*form_key, *form_key))
            .collect::<FxHashMap<_, _>>();
        let target_records = graph
            .nodes
            .values()
            .chain(graph.quests.values())
            .cloned()
            .map(|record| (record.form_key, record))
            .collect::<FxHashMap<_, _>>();
        let report = build_story_manager_route_seed_for_pair(
            Game::Fo76,
            Game::Fo4,
            &graph,
            &selection,
            &source_to_target,
            &target_records,
            &selection.selected_nodes,
            &FxHashSet::default(),
            interner.intern("SeventySix.esm"),
            &interner,
        );
        let routes = report
            .routes
            .iter()
            .filter(|route| {
                route.source_node.as_deref() == Some(node_fk.format(&interner).as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(routes.len(), BECKETT_SPECIFIC_ALIAS_QUESTS.len());
        for route in routes {
            assert_eq!(route.emission_status, "emitted");
            assert_eq!(route.target_event.as_deref(), Some("SCPT"));
            assert_eq!(
                route.target_selector_keywords,
                vec![start_keyword_fk.format(&interner)]
            );
            assert!(route.condition_valid, "{:?}", route.issues);
            assert!(route.issues.is_empty());
        }
    }

    #[test]
    fn exact_en07_alias_adapters_restore_the_source_story_manager_chain() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, 0x029152);
        let branch_fk = fk(&interner, 0x188B73);
        let flee_node_fk = fk(&interner, 0x2D1161);
        let code_node_fk = fk(&interner, 0x2D1194);
        let intro_node_fk = fk(&interner, 0x2D119C);
        let flee_quest_fk = fk(&interner, 0x2D0F69);
        let code_quest_fk = fk(&interner, 0x2D0F6A);
        let intro_quest_fk = fk(&interner, 0x2D0F6B);

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(&interner, "SMBN", branch_fk.local, Some("EnclaveBranch"));
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));

        let mut flee_node = record(
            &interner,
            "SMQN",
            flee_node_fk.local,
            Some("EN07FleeBlastQuestNode"),
        );
        flee_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        flee_node
            .fields
            .push(field("SNAM", FieldValue::FormKey(fk(&interner, 0x2D1151))));
        flee_node
            .fields
            .push(field("CITC", bytes(&1_u32.to_le_bytes())));
        flee_node.fields.push(field(
            "CTDA",
            raw_ctda("00FFFFFF0000803F4002000000004B315D112D000000000000000000FFFFFFFF"),
        ));
        flee_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(flee_quest_fk)));

        let mut code_node = record(
            &interner,
            "SMQN",
            code_node_fk.local,
            Some("EN07CodeHuntQuestNode"),
        );
        code_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        code_node
            .fields
            .push(field("SNAM", FieldValue::FormKey(flee_node_fk)));
        code_node
            .fields
            .push(field("CITC", bytes(&2_u32.to_le_bytes())));
        for ctda in [
            "00FFFFFF000000003002000089112D0000000000070000000000000052330000",
            "00FFFFFF0000803F4002000000004B3188112D000000000000000000FFFFFFFF",
        ] {
            code_node.fields.push(field("CTDA", raw_ctda(ctda)));
        }
        code_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(code_quest_fk)));

        let mut intro_node = record(
            &interner,
            "SMQN",
            intro_node_fk.local,
            Some("EN07IntroMiscQuestNode"),
        );
        intro_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        intro_node
            .fields
            .push(field("SNAM", FieldValue::FormKey(code_node_fk)));
        intro_node
            .fields
            .push(field("CITC", bytes(&3_u32.to_le_bytes())));
        for ctda in [
            "00000000000000000E0000009D112D0000000000070000000000000052330000",
            "0000000000000000300200009A112D0000000000070000000000000052330000",
            "000000000000803F4002000000004B319B112D000000000000000000FFFFFFFF",
        ] {
            intro_node.fields.push(field("CTDA", raw_ctda(ctda)));
        }
        intro_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(intro_quest_fk)));

        let mut flee_quest = record(
            &interner,
            "QUST",
            flee_quest_fk.local,
            Some("EN07_MQ_FleeBlast"),
        );
        flee_quest
            .fields
            .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        append_event_alias(&mut flee_quest, "ALST", 0, 128, 12_626);
        append_event_alias(&mut flee_quest, "ALST", 14, 5_018, 12_882);

        let mut code_quest = record(
            &interner,
            "QUST",
            code_quest_fk.local,
            Some("EN07_MQ_CodeHunt"),
        );
        code_quest
            .fields
            .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        code_quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(fk(&interner, 0x2D1189))));
        append_event_alias(&mut code_quest, "ALST", 0, 528, 12_626);
        append_event_alias(&mut code_quest, "ALST", 7, 658, 12_882);
        append_event_alias(&mut code_quest, "ALST", 1, 33_554_448, 13_138);
        append_event_alias(&mut code_quest, "ALLS", 2, 66_306, 12_620);

        let mut intro_quest = record(
            &interner,
            "QUST",
            intro_quest_fk.local,
            Some("EN07_MQ_Death"),
        );
        intro_quest
            .fields
            .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        intro_quest
            .fields
            .push(field("KWDA", FieldValue::FormKey(fk(&interner, 0x2D119A))));

        assert!(qust_has_untranslatable_event_alias(&flee_quest));
        assert!(qust_has_untranslatable_event_alias(&code_quest));
        assert_eq!(
            classify_quest(&flee_quest, &interner),
            Some(StoryQuestKind::Generic)
        );
        assert_eq!(
            classify_quest(&code_quest, &interner),
            Some(StoryQuestKind::Generic)
        );
        let mut lookalike = code_quest.clone();
        lookalike.form_key = fk(&interner, 0x2D0F6C);
        assert_eq!(classify_quest(&lookalike, &interner), None);

        let mut graph = StoryManagerSourceGraph::default();
        for node in [root, branch, flee_node, code_node, intro_node] {
            graph.nodes.insert(node.form_key, node);
        }
        for quest in [flee_quest, code_quest, intro_quest] {
            graph.quests.insert(quest.form_key, quest);
        }
        for keyword_record in [
            keyword(&interner, 0x2D115D, "EN07_FleeBlastQuestStartKeyword"),
            keyword(&interner, 0x2D1188, "EN07_CodeHuntQuestStartKeyword"),
            keyword(&interner, 0x2D1189, "EN07_CodeHuntQuestActiveKeyword"),
            keyword(&interner, 0x2D119A, "EN07_IntroMiscQuestActiveKeyword"),
            keyword(&interner, 0x2D119B, "EN07_IntroMiscQuestStartKeyword"),
        ] {
            graph
                .keywords
                .insert(keyword_record.form_key, keyword_record);
        }

        let condition_diagnostics =
            normalize_story_manager_quest_start_conditions(&mut graph, &interner);
        assert!(
            condition_diagnostics.is_empty(),
            "{condition_diagnostics:?}"
        );
        assert_eq!(
            condition_function_ids(&graph.nodes[&flee_node_fk]),
            vec![576]
        );
        assert_eq!(
            condition_function_ids(&graph.nodes[&code_node_fk]),
            vec![56, 576]
        );
        assert_eq!(
            condition_parameter_1(&graph.nodes[&code_node_fk], 0),
            0x2D0F6A
        );
        assert_eq!(
            condition_function_ids(&graph.nodes[&intro_node_fk]),
            vec![14, 56, 576]
        );
        assert_eq!(
            condition_parameter_1(&graph.nodes[&intro_node_fk], 1),
            0x2D0F6B
        );

        let translated = FxHashSet::from_iter([flee_quest_fk, code_quest_fk, intro_quest_fk]);
        let selection = classify_story_manager_records(&graph, &translated, &interner);
        for (node, quest) in [
            (flee_node_fk, flee_quest_fk),
            (code_node_fk, code_quest_fk),
            (intro_node_fk, intro_quest_fk),
        ] {
            assert!(selection.selected_nodes.contains(&node));
            assert_eq!(
                selection.selected_quests_by_node.get(&node),
                Some(&FxHashSet::from_iter([quest]))
            );
            assert_eq!(
                selection.selected_event_roots_by_quest.get(&quest),
                Some(&FxHashSet::from_iter([root_fk]))
            );
        }
        assert!(selection.selected_nodes.contains(&branch_fk));
        assert!(selection.selected_nodes.contains(&root_fk));
        assert!(selection.diagnostics.iter().all(|diagnostic| {
            diagnostic.kind == StoryManagerDiagnosticKind::Selected && diagnostic.reason.is_none()
        }));

        let mut code_output = graph.nodes[&code_node_fk].clone();
        let mut intro_output = graph.nodes[&intro_node_fk].clone();
        assert!(!sanitize_story_manager_previous_node(
            &mut code_output,
            &selection.selected_nodes,
            &interner
        ));
        assert!(!sanitize_story_manager_previous_node(
            &mut intro_output,
            &selection.selected_nodes,
            &interner
        ));
        assert_eq!(
            story_manager_fk(&code_output, snam_sig()),
            Some(flee_node_fk)
        );
        assert_eq!(
            story_manager_fk(&intro_output, snam_sig()),
            Some(code_node_fk)
        );
        assert_eq!(
            fo4_story_manager_event_root(&graph.nodes[&root_fk]),
            Some(FO4_SCRIPT_EVENT_ROOT_LOCAL)
        );
        let event_plan = plan_story_manager_quest_events(
            &selection,
            &graph,
            &FxHashMap::default(),
            &selection.selected_nodes,
            &interner,
        );
        assert!(event_plan.rewrites.is_empty());
        assert!(event_plan.unresolved.is_empty());
    }

    #[test]
    fn exact_tw002_event_trigger_alias_restores_its_script_event_node() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let branch_fk = fk(&interner, 0x003C_0083);
        let node_fk = fk(&interner, 0x003C_0084);
        let quest_fk = fk(&interner, 0x0010_E201);
        let selector_fk = fk(&interner, 0x0004_A600);

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(
            &interner,
            "SMBN",
            branch_fk.local,
            Some("RESpecialSceneBranch"),
        );
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = record(&interner, "SMQN", node_fk.local, Some("RESpecialSceneNode"));
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields.push(field("CITC", bytes(&1_u32.to_le_bytes())));
        node.fields.push(field(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_slice(&script_event_keyword_condition(
                selector_fk.local,
            ))),
        ));
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let mut quest = record(&interner, "QUST", quest_fk.local, Some("TW002"));
        quest
            .fields
            .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        append_event_alias(&mut quest, "ALST", 39, 0, 12_626);
        let alias_anchor = quest
            .fields
            .iter()
            .rposition(|entry| entry.sig.0 == *b"ALST")
            .expect("TW002 event alias anchor");
        quest
            .fields
            .insert(alias_anchor + 1, field("ALID", bytes(b"EventTrigger\0")));
        assert!(!qust_has_untranslatable_event_alias_for_source(
            &interner, &quest
        ));
        assert_eq!(
            classify_quest(&quest, &interner),
            Some(StoryQuestKind::Generic)
        );

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        graph.quests.insert(quest_fk, quest);
        graph.keywords.insert(
            selector_fk,
            keyword(&interner, selector_fk.local, "REEncounterTypeScene"),
        );

        let selection =
            classify_story_manager_records(&graph, &FxHashSet::from_iter([quest_fk]), &interner);
        for selected in [root_fk, branch_fk, node_fk] {
            assert!(selection.selected_nodes.contains(&selected));
        }
        assert_eq!(
            selection.selected_quests_by_node.get(&node_fk),
            Some(&FxHashSet::from_iter([quest_fk]))
        );
        assert_eq!(
            selection.selected_event_roots_by_quest.get(&quest_fk),
            Some(&FxHashSet::from_iter([root_fk]))
        );
    }

    #[test]
    fn msilo_personal_adapter_emits_the_exact_selector_route() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, 0x029152);
        let branch_fk = fk(&interner, 0x50FDF0);
        let manager_node_fk = fk(&interner, 0x50FDF1);
        let personal_node_fk = fk(&interner, 0x4FBE14);
        let manager_quest_fk = fk(&interner, 0x3D72E6);
        let personal_quest_fk = fk(&interner, 0x3E03AA);
        let manager_keyword_fk = fk(&interner, 0x50FDEF);
        let personal_keyword_fk = fk(&interner, 0x3E03AB);

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));

        let mut branch = record(&interner, "SMBN", branch_fk.local, Some("MSiloBranch"));
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));

        let mut manager_node = record(&interner, "SMQN", manager_node_fk.local, Some("MSiloNode"));
        manager_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        manager_node
            .fields
            .push(field("CITC", bytes(&1_u32.to_le_bytes())));
        manager_node.fields.push(field(
            "CTDA",
            raw_ctda("000A00000000803F4002284300004B31EFFD50000000000000000000FFFFFFFF"),
        ));
        manager_node
            .fields
            .push(field("DNAM", bytes(&1_u32.to_le_bytes())));
        manager_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(manager_quest_fk)));

        let mut personal_node = record(
            &interner,
            "SMQN",
            personal_node_fk.local,
            Some("MSiloPersonalQuestNode"),
        );
        personal_node
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        personal_node
            .fields
            .push(field("SNAM", FieldValue::FormKey(manager_node_fk)));
        personal_node
            .fields
            .push(field("CITC", bytes(&1_u32.to_le_bytes())));
        personal_node.fields.push(field(
            "CTDA",
            raw_ctda("000A00000000803F40025F4300004B31AB033E000000000000000000FFFFFFFF"),
        ));
        personal_node
            .fields
            .push(field("DNAM", bytes(&1_u32.to_le_bytes())));
        personal_node
            .fields
            .push(field("NNAM", FieldValue::FormKey(personal_quest_fk)));

        let manager_quest = record(&interner, "QUST", manager_quest_fk.local, Some("MSilo"));
        let mut personal_quest = record(
            &interner,
            "QUST",
            personal_quest_fk.local,
            Some("MSiloPersonal"),
        );
        append_event_alias(&mut personal_quest, "ALST", 0, 0x1800_0210, 13_138);
        assert!(qust_has_untranslatable_event_alias(&personal_quest));
        assert_eq!(
            classify_quest(&personal_quest, &interner),
            Some(StoryQuestKind::Generic)
        );

        let mut graph = StoryManagerSourceGraph::default();
        for node in [root, branch, manager_node, personal_node] {
            graph.nodes.insert(node.form_key, node);
        }
        for quest in [manager_quest, personal_quest] {
            graph.quests.insert(quest.form_key, quest);
        }
        for keyword_record in [
            keyword(&interner, manager_keyword_fk.local, "MSiloQuestKeyword"),
            keyword(
                &interner,
                personal_keyword_fk.local,
                "MSiloPersonalQuestKeyword",
            ),
        ] {
            graph
                .keywords
                .insert(keyword_record.form_key, keyword_record);
        }

        let condition_diagnostics =
            normalize_story_manager_quest_start_conditions(&mut graph, &interner);
        assert!(
            condition_diagnostics.is_empty(),
            "{condition_diagnostics:?}"
        );
        assert!(graph.nodes[&personal_node_fk].fields.iter().any(|entry| {
            Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
                .is_some_and(|raw| raw & 0x00FF_FFFF == personal_keyword_fk.local)
        }));

        let translated = FxHashSet::from_iter([manager_quest_fk, personal_quest_fk]);
        let selection = classify_story_manager_records(&graph, &translated, &interner);
        for node in [root_fk, branch_fk, manager_node_fk, personal_node_fk] {
            assert!(selection.selected_nodes.contains(&node));
        }
        assert_eq!(
            selection.selected_quests_by_node.get(&personal_node_fk),
            Some(&FxHashSet::from_iter([personal_quest_fk]))
        );
        assert!(selection.diagnostics.iter().all(|diagnostic| {
            diagnostic.kind == StoryManagerDiagnosticKind::Selected && diagnostic.reason.is_none()
        }));

        let mut emitted_personal = graph.nodes[&personal_node_fk].clone();
        retain_story_manager_quests(
            &mut emitted_personal,
            &selection.selected_quests_by_node[&personal_node_fk],
        );
        assert!(!sanitize_story_manager_previous_node(
            &mut emitted_personal,
            &selection.selected_nodes,
            &interner
        ));
        assert_eq!(
            story_manager_fk(&emitted_personal, pnam_sig()),
            Some(branch_fk)
        );
        assert_eq!(
            story_manager_fk(&emitted_personal, snam_sig()),
            Some(manager_node_fk)
        );
        assert_eq!(
            story_manager_quests(&emitted_personal),
            vec![personal_quest_fk]
        );
        assert!(emitted_personal.fields.iter().any(|entry| {
            Fo76Fo4Hook::story_manager_start_keyword(&entry.value)
                .is_some_and(|raw| raw & 0x00FF_FFFF == personal_keyword_fk.local)
        }));
        assert!(
            emitted_personal
                .fields
                .iter()
                .any(|entry| entry.sig.0 == *b"DNAM")
        );

        let source_to_target = graph
            .nodes
            .keys()
            .chain(graph.quests.keys())
            .map(|form_key| (*form_key, *form_key))
            .collect::<FxHashMap<_, _>>();
        let target_records = graph
            .nodes
            .values()
            .chain(graph.quests.values())
            .cloned()
            .map(|record| (record.form_key, record))
            .collect::<FxHashMap<_, _>>();
        let report = build_story_manager_route_seed_for_pair(
            Game::Fo76,
            Game::Fo4,
            &graph,
            &selection,
            &source_to_target,
            &target_records,
            &selection.selected_nodes,
            &FxHashSet::default(),
            interner.intern("SeventySix.esm"),
            &interner,
        );
        let route = report
            .routes
            .iter()
            .find(|route| {
                route.source_node.as_deref() == Some(personal_node_fk.format(&interner).as_str())
            })
            .expect("MSiloPersonal route");
        assert_eq!(route.emission_status, "emitted");
        assert_eq!(route.skip_reason, None);
        assert_eq!(route.structural_status, "eligible");
        assert!(route.condition_valid);
        assert!(route.issues.is_empty());
        assert_eq!(route.target_event.as_deref(), Some("SCPT"));
        assert_eq!(
            route.source_selector_keywords,
            vec![personal_keyword_fk.format(&interner)]
        );
        assert_eq!(
            route.target_selector_keywords,
            vec![personal_keyword_fk.format(&interner)]
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
    fn classify_quest_skips_cut_w05_settler_dailies() {
        let interner = StringInterner::new();
        for (local, editor_id) in [
            (0x3F2DC7, "zzz_W05_SettlersDaily_Clinic"),
            (0x403436, "zzz_W05_SettlersDaily_Fieldhand"),
            (0x41B725, "zzz_W05_SettlersDaily_Restock"),
            (0x3F2DC9, "zzz_W05_SettlersDaily_Stew"),
        ] {
            let quest = record(&interner, "QUST", local, Some(editor_id));
            assert_eq!(classify_quest(&quest, &interner), None, "{editor_id}");
        }
    }

    #[test]
    fn workshop_clear_node_keeps_its_exact_story_manager_chain() {
        let interner = StringInterner::new();
        let root_fk = fk(&interner, 0x029152);
        let branch_fk = fk(&interner, 0x04FE7C);
        let previous_fk = fk(&interner, 0x3AEDF5);
        let node_fk = fk(&interner, 0x1789FA);
        let previous_quest_fk = fk(&interner, 0x011CCC);
        let quest_fk = fk(&interner, 0x1789F9);
        let selector_keyword_fk = fk(&interner, 0x1789FE);

        let mut root = record(&interner, "SMEN", root_fk.local, Some("ScriptEvent"));
        root.fields.push(field("ENAM", bytes(b"SCPT")));
        let mut branch = record(&interner, "SMBN", branch_fk.local, Some("GQ_Branch"));
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut previous = record(
            &interner,
            "SMQN",
            previous_fk.local,
            Some("GQ_WorkshopAttackNode"),
        );
        previous
            .fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        previous
            .fields
            .push(field("NNAM", FieldValue::FormKey(previous_quest_fk)));
        let mut node = record(
            &interner,
            "SMQN",
            node_fk.local,
            Some("GQ_WorkshopClearNode"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields
            .push(field("SNAM", FieldValue::FormKey(previous_fk)));
        node.fields.push(field("CITC", bytes(&1_u32.to_le_bytes())));
        node.fields.push(field(
            "CTDA",
            raw_ctda("000000000000803F4002000000004B31FE8917000000000000000000FFFFFFFF"),
        ));
        node.fields
            .push(field("NNAM", FieldValue::FormKey(quest_fk)));

        let previous_quest = record(
            &interner,
            "QUST",
            previous_quest_fk.local,
            Some("GQ_WorkshopAttack"),
        );
        let mut quest = record(&interner, "QUST", quest_fk.local, Some("GQ_WorkshopClear"));
        append_event_alias(&mut quest, "ALST", 0, 0x4000_0100, 12_626);
        quest.fields.insert(
            quest
                .fields
                .iter()
                .position(|entry| entry.sig.0 == *b"FNAM")
                .expect("Workshop alias flags"),
            field("ALID", bytes(b"Workshop\0")),
        );

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, previous, node] {
            graph.nodes.insert(record.form_key, record);
        }
        for record in [previous_quest, quest] {
            graph.quests.insert(record.form_key, record);
        }
        let selector_keyword = keyword(
            &interner,
            selector_keyword_fk.local,
            "GQ_WorkshopClearKeyword",
        );
        graph
            .keywords
            .insert(selector_keyword.form_key, selector_keyword);
        let translated = FxHashSet::from_iter([previous_quest_fk, quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        for form_key in [root_fk, branch_fk, previous_fk, node_fk] {
            assert!(selection.selected_nodes.contains(&form_key));
        }
        assert_eq!(
            selection.selected_quests_by_node.get(&node_fk),
            Some(&FxHashSet::from_iter([quest_fk]))
        );
        let mut emitted = graph.nodes[&node_fk].clone();
        assert!(!sanitize_story_manager_previous_node(
            &mut emitted,
            &selection.selected_nodes,
            &interner,
        ));
        assert_eq!(story_manager_fk(&emitted, snam_sig()), Some(previous_fk));

        let source_to_target = graph
            .nodes
            .keys()
            .chain(graph.quests.keys())
            .chain(graph.keywords.keys())
            .map(|form_key| (*form_key, *form_key))
            .collect::<FxHashMap<_, _>>();
        let target_records = graph
            .nodes
            .values()
            .chain(graph.quests.values())
            .chain(graph.keywords.values())
            .cloned()
            .map(|record| (record.form_key, record))
            .collect::<FxHashMap<_, _>>();
        let report = build_story_manager_route_seed_for_pair(
            Game::Fo76,
            Game::Fo4,
            &graph,
            &selection,
            &source_to_target,
            &target_records,
            &selection.selected_nodes,
            &FxHashSet::default(),
            interner.intern("SeventySix.esm"),
            &interner,
        );
        let route = report
            .routes
            .iter()
            .find(|route| route.source_node.as_deref() == Some(node_fk.format(&interner).as_str()))
            .expect("GQ_WorkshopClear route");
        assert_eq!(route.emission_status, "emitted");
        assert_eq!(route.target_quest, Some(quest_fk.format(&interner)));
        assert_eq!(route.target_event.as_deref(), Some("SCPT"));
        assert!(route.issues.is_empty(), "{:?}", route.issues);
    }

    #[test]
    fn classify_quest_skips_breadcrumb_player_connect_bridge() {
        let interner = StringInterner::new();
        let breadcrumb_connect = record(
            &interner,
            "QUST",
            0x5EAD3B,
            Some("BS01_MQ00_Breadcrumb_OnConnect"),
        );
        assert_eq!(classify_quest(&breadcrumb_connect, &interner), None);
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
    fn story_manager_keeps_quest_with_script_event_location1_alias() {
        let interner = StringInterner::new();
        let (mut graph, smqn_fk, quest_fk) = graph_with_radio(&interner);
        let quest = graph.quests.get_mut(&quest_fk).unwrap();
        quest
            .fields
            .push(field("ALLS", bytes(&1_u32.to_le_bytes())));
        quest
            .fields
            .push(field("FNAM", bytes(&0_u32.to_le_bytes())));
        quest
            .fields
            .push(field("ALFE", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        quest
            .fields
            .push(field("ALFD", bytes(&12_620_u32.to_le_bytes())));
        let translated = FxHashSet::from_iter([quest_fk]);

        let selection = classify_story_manager_records(&graph, &translated, &interner);

        assert!(selection.selected_nodes.contains(&smqn_fk));
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

    fn lite_ally_intro_graph(
        interner: &StringInterner,
    ) -> (StoryManagerSourceGraph, FormKey, Vec<FormKey>) {
        let root_fk = fk(interner, FO4_SCRIPT_EVENT_ROOT_LOCAL);
        let branch_fk = fk(interner, LITE_ALLY_INTRO_BRANCH_LOCAL);
        let node_fk = fk(interner, LITE_ALLY_INTRO_NODE_LOCAL);
        let previous_fk = fk(interner, LITE_ALLY_INTRO_PREVIOUS_LOCAL);

        let mut root = record(interner, "SMEN", root_fk.local, None);
        root.fields
            .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
        let mut branch = record(
            interner,
            "SMBN",
            branch_fk.local,
            Some("COMP_Quests_Branch"),
        );
        branch
            .fields
            .push(field("PNAM", FieldValue::FormKey(root_fk)));
        let mut node = record(
            interner,
            "SMQN",
            node_fk.local,
            Some("COMP_Lite_Intro_QuestNodes"),
        );
        node.fields
            .push(field("PNAM", FieldValue::FormKey(branch_fk)));
        node.fields
            .push(field("SNAM", FieldValue::FormKey(previous_fk)));
        node.fields.push(field(
            "DNAM",
            bytes(&LITE_ALLY_INTRO_SHARED_EVENT_FLAGS.to_le_bytes()),
        ));
        node.fields.push(field("XNAM", bytes(&0_u32.to_le_bytes())));
        node.fields.push(field(
            "QNAM",
            bytes(&(LITE_ALLY_INTRO_ROUTES.len() as u32).to_le_bytes()),
        ));
        node.fields.push(field(
            "CITC",
            bytes(&(LITE_ALLY_INTRO_ROUTES.len() as u32).to_le_bytes()),
        ));
        for ctda in [
            "010B00000000803F4002944300004B31CC5658000000000000000000FFFFFFFF",
            "010000000000803F4002944300004B31CA5C58000000000000000000FFFFFFFF",
            "010000000000803F4002944300004B31275D58000000000000000000FFFFFFFF",
            "010000000000803F4002944300004B31D85C58000000000000000000FFFFFFFF",
            "010000000000803F4002944300004B31BF5C58000000000000000000FFFFFFFF",
        ] {
            node.fields.push(field("CTDA", raw_ctda(ctda)));
        }

        let mut graph = StoryManagerSourceGraph::default();
        for record in [root, branch, node] {
            graph.nodes.insert(record.form_key, record);
        }
        let mut quest_fks = Vec::new();
        for (quest_local, quest_editor_id, keyword_local, keyword_editor_id) in
            LITE_ALLY_INTRO_ROUTES
        {
            let quest_fk = fk(interner, quest_local);
            let mut quest = record(interner, "QUST", quest_fk.local, Some(quest_editor_id));
            quest
                .fields
                .push(field("ENAM", bytes(&SCPT_EVENT_TYPE.to_le_bytes())));
            append_event_alias(&mut quest, "ALST", 3, 0, 0x3152);
            graph.quests.insert(quest_fk, quest);
            let start_keyword = keyword(interner, keyword_local, keyword_editor_id);
            graph.keywords.insert(start_keyword.form_key, start_keyword);
            graph
                .nodes
                .get_mut(&node_fk)
                .unwrap()
                .fields
                .push(field("NNAM", FieldValue::FormKey(quest_fk)));
            quest_fks.push(quest_fk);
        }

        (graph, node_fk, quest_fks)
    }

    #[test]
    fn exact_lite_ally_shared_node_preserves_all_routes_and_scpt_quest_events() {
        let interner = StringInterner::new();
        let (mut graph, node_fk, quest_fks) = lite_ally_intro_graph(&interner);
        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(condition_count(&graph.nodes[&node_fk]), 5);
        assert!(exact_lite_ally_intro_node(&graph, node_fk, &interner));

        let translated = quest_fks.iter().copied().collect::<FxHashSet<_>>();
        let selection = classify_story_manager_records(&graph, &translated, &interner);
        assert!(selection.selected_nodes.contains(&node_fk));
        assert_eq!(
            selection.selected_quests_by_node.get(&node_fk),
            Some(&translated)
        );

        let event_plan = plan_story_manager_quest_events(
            &selection,
            &graph,
            &FxHashMap::default(),
            &selection.selected_nodes,
            &interner,
        );
        assert!(event_plan.unresolved.is_empty());
        assert_eq!(event_plan.rewrites.len(), LITE_ALLY_INTRO_ROUTES.len());
        for quest_fk in &quest_fks {
            assert_eq!(event_plan.rewrites.get(quest_fk), Some(&SCPT_EVENT_TYPE));
            let mut target = record(&interner, "QUST", quest_fk.local, Some("Converted"));
            assert!(set_qust_event_type(&mut target, SCPT_EVENT_TYPE));
            assert_eq!(story_manager_event_type(&target), Some(SCPT_EVENT_TYPE));
        }

        let source_to_target = graph
            .nodes
            .keys()
            .chain(graph.quests.keys())
            .map(|form_key| (*form_key, *form_key))
            .collect::<FxHashMap<_, _>>();
        let target_records = graph
            .nodes
            .values()
            .chain(graph.quests.values())
            .cloned()
            .map(|record| (record.form_key, record))
            .collect::<FxHashMap<_, _>>();
        let report = build_story_manager_route_seed_for_pair(
            Game::Fo76,
            Game::Fo4,
            &graph,
            &selection,
            &source_to_target,
            &target_records,
            &selection.selected_nodes,
            &FxHashSet::default(),
            interner.intern("SeventySix.esm"),
            &interner,
        );
        let routes = report
            .routes
            .iter()
            .filter(|route| {
                route.source_node.as_deref() == Some(node_fk.format(&interner).as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(routes.len(), LITE_ALLY_INTRO_ROUTES.len());
        for route in routes {
            assert!(route.condition_valid, "{:?}", route.issues);
            assert_eq!(
                route.target_selector_keywords.len(),
                LITE_ALLY_INTRO_ROUTES.len()
            );
            assert!(
                !route
                    .issues
                    .iter()
                    .any(|issue| issue == "conflicting_mandatory_k1_selectors")
            );
        }
    }

    #[test]
    fn lite_ally_shared_node_rejects_keyword_and_partial_route_near_misses() {
        let interner = StringInterner::new();
        let (mut graph, node_fk, quest_fks) = lite_ally_intro_graph(&interner);
        let wrong_keyword_fk = fk(&interner, LITE_ALLY_INTRO_ROUTES[2].2);
        graph.keywords.get_mut(&wrong_keyword_fk).unwrap().eid =
            Some(interner.intern("COMP_Keyword_QuestStart_RaiderPunk_Copy"));

        let diagnostics = normalize_story_manager_quest_start_conditions(&mut graph, &interner);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .starts_with("quest_start_unqualified:")
        );
        graph.condition_diagnostics = diagnostics;
        let translated = quest_fks.iter().copied().collect::<FxHashSet<_>>();
        let selection = classify_story_manager_records(&graph, &translated, &interner);
        assert!(!selection.selected_nodes.contains(&node_fk));
        assert_eq!(
            selection.diagnostics[0].reason,
            Some(StoryManagerSkipReason::UnsupportedQuest)
        );

        let (graph, node_fk, quest_fks) = lite_ally_intro_graph(&interner);
        let incomplete = quest_fks
            .iter()
            .copied()
            .take(quest_fks.len() - 1)
            .collect::<FxHashSet<_>>();
        let selection = classify_story_manager_records(&graph, &incomplete, &interner);
        assert!(!selection.selected_nodes.contains(&node_fk));
        assert_eq!(
            selection.diagnostics[0].reason,
            Some(StoryManagerSkipReason::QuestNotTranslated)
        );
    }

    #[test]
    fn lite_ally_shared_node_rejects_structural_near_misses() {
        let interner = StringInterner::new();
        for mutation in [
            "node_editor_id",
            "previous",
            "flags",
            "branch_signature",
            "branch_condition",
            "root_signature",
        ] {
            let (mut graph, node_fk, _) = lite_ally_intro_graph(&interner);
            match mutation {
                "node_editor_id" => {
                    graph.nodes.get_mut(&node_fk).unwrap().eid =
                        Some(interner.intern("COMP_Lite_Intro_QuestNodes_Copy"));
                }
                "previous" => {
                    let node = graph.nodes.get_mut(&node_fk).unwrap();
                    node.fields
                        .iter_mut()
                        .find(|entry| entry.sig.0 == *b"SNAM")
                        .unwrap()
                        .value =
                        FieldValue::FormKey(fk(&interner, LITE_ALLY_INTRO_PREVIOUS_LOCAL + 1));
                }
                "flags" => {
                    let node = graph.nodes.get_mut(&node_fk).unwrap();
                    node.fields
                        .iter_mut()
                        .find(|entry| entry.sig.0 == *b"DNAM")
                        .unwrap()
                        .value = bytes(&0_u32.to_le_bytes());
                }
                "branch_signature" => {
                    graph
                        .nodes
                        .get_mut(&fk(&interner, LITE_ALLY_INTRO_BRANCH_LOCAL))
                        .unwrap()
                        .sig = smqn_sig();
                }
                "branch_condition" => {
                    graph
                        .nodes
                        .get_mut(&fk(&interner, LITE_ALLY_INTRO_BRANCH_LOCAL))
                        .unwrap()
                        .fields
                        .push(field(
                            "CTDA",
                            raw_ctda(
                                "000000000000803F4002944300004B31CC5658000000000000000000FFFFFFFF",
                            ),
                        ));
                }
                "root_signature" => {
                    graph
                        .nodes
                        .get_mut(&fk(&interner, FO4_SCRIPT_EVENT_ROOT_LOCAL))
                        .unwrap()
                        .sig = smbn_sig();
                }
                _ => unreachable!(),
            }

            assert!(
                !exact_lite_ally_intro_node(&graph, node_fk, &interner),
                "{mutation} must fail closed"
            );
            let translated = LITE_ALLY_INTRO_ROUTES
                .iter()
                .map(|(quest, _, _, _)| fk(&interner, *quest))
                .collect::<FxHashSet<_>>();
            let selection = classify_story_manager_records(&graph, &translated, &interner);
            assert!(
                !selection.selected_nodes.contains(&node_fk),
                "{mutation} must not fall through to generic selection"
            );
        }
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
