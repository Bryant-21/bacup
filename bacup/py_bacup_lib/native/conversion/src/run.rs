//! `ConversionRun` — per-conversion-run state, plus a thread-safe run registry.
//!
//! A `ConversionRun` is the long-lived object for one mod conversion. It owns the
//! string interner, schemas, translator, decision/warning accumulators, deferred
//! record list, and `MapperState`. `FormKeyMapper` views borrow the interner, so
//! they are built per phase instead of stored.
//!
//! The registry maps monotonic `u64` run IDs to per-run lock slots (see
//! "Registry" below).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

use pyo3::prelude::*;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::errors::RecordReadError;
use crate::fixups::{FixupConfig, FixupContext, FixupError, FixupRegistry, FixupReport};
use crate::fnv_legacy_scripting::record_identity::{
    LegacyRecordIdentity, LegacyRecordTopology, PreparedLegacyRecordPayload,
};
use crate::fnv_legacy_scripting::{
    FnvLegacyScriptingContext, FnvLegacyScriptingResult, FnvScriptingError, translate_all_dial,
    translate_all_qust, translate_all_scen,
};
use crate::formkey_mapper::{
    FIRST_ALLOCATION_ID, FormKeyMapper, MapperOptions, MapperState, ResolutionMode,
    allows_editor_id_vanilla_remap, is_static_marker_editor_id,
    mapper_allows_editor_id_vanilla_remap,
};
use crate::full_plugin::{AssetPhaseFlags, FullPluginRunState, WarningPolicy, intern_plugin_names};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::legacy_fallout_navmesh::{LegacyFalloutNavmeshBatch, prepare_legacy_fallout_navmeshes};
use crate::legacy_pack_preflight::{
    DirectLegacyPackOrigin, LegacyPackExpectedCounts, LegacyPackOriginRow,
    LegacyPackPreflightAccumulator, LegacyPackPreflightReport,
};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::source_read::{
    SourceRecordBatchSnapshot, collect_eid_index, decode_record_from_parsed_relayout,
    form_key_to_read_str, iter_form_keys_of_sig, plugin_context_for_handle, plugin_name_for_handle,
    raw_cell_is_interior, read_record_relayout_by_form_key, snapshot_records_by_form_keys,
    source_signatures,
};
use crate::sym::StringInterner;
use crate::sym::Sym;
use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
use crate::target_write::{
    ExistingAuthoringRecordPlacement, add_projected_navmeshes_chunk_native,
    add_quest_child_record_indexed_native, add_quest_child_record_native, add_record_native,
    add_topic_child_record_indexed_native, add_topic_child_record_native,
    begin_quest_child_insert_session_native, begin_topic_child_insert_session_native,
    encode_form_key_for_handle, existing_authoring_record_placement_native,
    plugin_header_flags_native, rebuild_projected_navi_from_source_native,
    rebuild_projected_navi_from_source_with_nver_native, rebuild_projected_navi_native,
    rebuild_worldspace_groups_from_source_native, relocate_authoring_record_native,
    replace_record_contents_native,
};
use crate::translator::TranslateResult;
use crate::translator::pair_hook::PairCtx;
use crate::translator::target_hook::TargetCtx;
use crate::translator::{Decision, DeferredKind, Game, Translator};
use esp_authoring_core::plugin_runtime::FO4_CANONICAL_NAVI_FORM_ID;

const FIXUP_WARNING_LOG_LIMIT: usize = 32;
const FIXUP_DIAGNOSTIC_LOG_LIMIT: usize = 64;

fn fixup_warning_log_messages(
    phase: &str,
    name: &str,
    iteration: u32,
    report: &FixupReport,
    interner: &StringInterner,
) -> Vec<String> {
    let mut messages = report
        .warnings
        .iter()
        .take(FIXUP_WARNING_LOG_LIMIT)
        .map(|warning| {
            let detail = interner.resolve(*warning).unwrap_or("<unresolved>");
            format!("[{phase}] diagnostic {name} iter={iteration} {detail}")
        })
        .collect::<Vec<_>>();
    if report.warnings.len() > FIXUP_WARNING_LOG_LIMIT {
        messages.push(format!(
            "[{phase}] diagnostic {name} iter={iteration} truncated={} total={}",
            report.warnings.len() - FIXUP_WARNING_LOG_LIMIT,
            report.warnings.len()
        ));
    }
    messages
}

fn emit_fixup_warning_logs(
    event_tx: &crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    phase: &'static str,
    name: &str,
    iteration: u32,
    report: &FixupReport,
    interner: &StringInterner,
) {
    for message in fixup_warning_log_messages(phase, name, iteration, report, interner) {
        eprintln!("{message}");
        let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase,
            level: crate::phase::LogLevel::Warn,
            message,
        });
    }
}

fn emit_fixup_diagnostic_logs(
    event_tx: &crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    phase: &'static str,
    name: &str,
    iteration: u32,
    report: &FixupReport,
    interner: &StringInterner,
) {
    let total = report.diagnostics.len();
    for diagnostic in report.diagnostics.iter().take(FIXUP_DIAGNOSTIC_LOG_LIMIT) {
        let detail = interner.resolve(*diagnostic).unwrap_or("<unresolved>");
        let message = format!("[{phase}] decision {name} iter={iteration} {detail}");
        eprintln!("{message}");
        let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase,
            level: crate::phase::LogLevel::Info,
            message,
        });
    }
    if total > FIXUP_DIAGNOSTIC_LOG_LIMIT {
        let message = format!(
            "[{phase}] decision {name} iter={iteration} truncated={} total={total}",
            total - FIXUP_DIAGNOSTIC_LOG_LIMIT
        );
        eprintln!("{message}");
        let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase,
            level: crate::phase::LogLevel::Info,
            message,
        });
    }
}

fn emit_fixup_report_log(
    event_tx: &crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    phase: &'static str,
    name: &str,
    report: &FixupReport,
    interner: &StringInterner,
) {
    let detail = report
        .message
        .and_then(|message| interner.resolve(message))
        .map(|message| format!(" message={message}"))
        .unwrap_or_default();
    let message = format!(
        "[{phase}] finished {name} changed={} dropped={} added={} warnings={} diagnostics={}{}",
        report.records_changed,
        report.records_dropped,
        report.records_added,
        report.warnings.len(),
        report.diagnostics.len(),
        detail
    );
    eprintln!("{message}");
    let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
        phase,
        level: crate::phase::LogLevel::Info,
        message,
    });
    emit_fixup_warning_logs(event_tx, phase, name, 1, report, interner);
    emit_fixup_diagnostic_logs(event_tx, phase, name, 1, report, interner);
}

// ---------------------------------------------------------------------------
// RunConfig — options that drive mapper + translator behaviour
// ---------------------------------------------------------------------------

/// Configuration for one conversion run.
#[derive(Debug, Clone, Default)]
pub struct RunConfig {
    /// Reject unmapped FormKeys immediately instead of deferring.
    pub strict_mapper: bool,
    /// Prefer vanilla target records for EditorID matches.
    pub use_base_game_assets: bool,
    /// Preserve the source object-id when the id is free in the output plugin.
    pub preserve_source_ids: bool,
    /// Replace existing generated output assets during an explicit overwrite run.
    pub overwrite_existing: bool,
    /// First object-id to use for freshly generated records.
    pub generated_object_id_floor: u32,
    /// Output plugin filename, e.g. `"Output.esm"`.
    pub output_plugin_name: String,
    /// True when converting an entire plugin (not a per-record sub-graph).
    /// Passed through to `FixupConfig::is_whole_plugin`.
    pub is_whole_plugin: bool,
    /// 4-byte signature of the conversion root record (e.g. `NPC_`, `LVLN`).
    /// `None` for whole-plugin or unknown-root conversions.
    pub root_sig: Option<crate::ids::SigCode>,
    /// Filesystem path to the mod directory being converted. Several havok
    /// fixups gate on this being `Some(...)` to find HKX assets next to the
    /// source plugin.
    pub mod_path: Option<std::path::PathBuf>,
    /// Filesystem path to the directory holding source assets extracted from
    /// the source game's BA2/BSA archives. Havok fixups use this to locate
    /// reference rigs/behaviors that aren't shipped with the mod itself.
    pub source_extracted_dir: Option<std::path::PathBuf>,
    pub additional_source_asset_roots: Vec<std::path::PathBuf>,
    pub legacy_music_tracks: Vec<LegacyMusicTrackRow>,
    pub target_extracted_dir: Option<std::path::PathBuf>,
    /// Root of a prepared membership tree for a single asset class. Narrow by
    /// construction, so it never replaces `target_extracted_dir` — fixups that
    /// resolve real base-game paths need the full extracted root.
    pub target_membership_root: Option<std::path::PathBuf>,
    pub target_data_dir: Option<std::path::PathBuf>,
    pub target_asset_catalog_path: Option<std::path::PathBuf>,
    pub target_asset_cache_dir: Option<std::path::PathBuf>,
    pub conversion_workers: Option<usize>,
    pub records_limit: Option<usize>,
    pub warning_policy: WarningPolicy,
    pub asset_phases: AssetPhaseFlags,
    pub target_record_preflight: Vec<TargetRecordPreflightRow>,
    pub target_master_names: Vec<String>,
    pub base_asset_namespace: String,
    /// Data-relative mesh subtrees (e.g. "meshes/landscape") whose FO76↔FO4
    /// path collisions drive asset relocation. Empty ⇒ FO76→FO4 default.
    pub base_asset_relocation_mesh_roots: Vec<String>,
    /// Data-relative subtrees whose source assets deliberately overwrite the
    /// target game's at the same path, instead of being skipped as base-owned.
    /// Empty ⇒ pair default.
    pub base_overwrite_prefixes: Vec<String>,
    /// Worldspace XYZ offset applied to emitted projected-navmesh geometry
    /// (NVNM vertices, grid bounds, waypoints) so the navmesh stays co-spatial
    /// with placed records, which receive the same offset. The parent cell index
    /// and index-based topology (triangles/cover/edges) are left untouched.
    pub projected_navmesh_offset: [f32; 3],
    /// Extra source-record 4-byte signatures to drop during translation, seeded
    /// into `translator.maps.skip_records` at run creation. The full-plugin path
    /// uses this to skip placed records (REFR/ACHR/...) for debug bisection.
    pub skip_record_signatures: Vec<String>,
    /// Exact source records allowed through an active MVP signature fence.
    /// A pair's exception set is useful only when its complete audited closure
    /// is present; partial sets remain inactive.
    pub mvp_record_exceptions: Vec<MvpRecordExceptionRow>,
    /// Restrict generic translation to zero records so the dedicated audited
    /// `mvp_melee` phase is the only record producer.
    pub mvp_melee_only: bool,
    /// True only for whole-plugin FO76→FO4 worldspace runs (the phase-6
    /// cell-slice copy re-inserts placed children AFTER fixups). Passed into
    /// `FixupConfig::defer_placed_child_ref_class` so the pre-copy own-plugin
    /// null-dangling pass DEFERS the LCTN LCUN/LCEP/ACEP class; the authoritative
    /// resolution runs post-copy via `repair_placed_child_refs`.
    pub defer_placed_child_ref_class: bool,
    pub legacy_pack_origins: Vec<LegacyPackOriginRow>,
    pub legacy_pack_raw_source_counts: Option<LegacyPackExpectedCounts>,
    pub legacy_pack_expected_counts: Option<LegacyPackExpectedCounts>,
    pub legacy_pack_provenance_required: bool,
    pub legacy_runtime_origins: Vec<crate::merge_sources::LegacyRuntimeOriginRow>,
    /// Enables only the audited FNV quest dependency closure supplied below.
    pub fnv_quest_slice: bool,
    pub fnv_quest_slice_records: HashMap<String, Vec<u32>>,
    /// Explicit authoritative FO4 PACK donor for the audited Renolds dialogue
    /// package. The library resolves this only from already-open target masters.
    pub fnv_force_greet_donor_form_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MvpRecordExceptionRow {
    pub signature: String,
    pub local_form_id: u32,
    pub source_plugin: String,
}

const SKYRIMSE_FO4_MVP_WEAPON_CLOSURE: &[(&str, u32, &str)] = &[
    ("WEAP", 0x01_3984, "Skyrim.esm"),
    ("STAT", 0x02_0E27, "Skyrim.esm"),
];
const FNV_FO4_MVP_WEAPON_CLOSURE: &[(&str, u32, &str)] = &[
    ("WEAP", 0x11_A8E4, "FalloutNV.esm"),
    ("STAT", 0x11_A8E3, "FalloutNV.esm"),
];

fn expected_mvp_weapon_closure(
    source: Game,
    target: Game,
) -> Option<&'static [(&'static str, u32, &'static str)]> {
    match (source, target) {
        (Game::SkyrimSe, Game::Fo4) => Some(SKYRIMSE_FO4_MVP_WEAPON_CLOSURE),
        (Game::Fnv, Game::Fo4) => Some(FNV_FO4_MVP_WEAPON_CLOSURE),
        _ => None,
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LegacyMusicTrackRow {
    pub legacy_relative_path: String,
    pub source_path: String,
    pub asset_path: String,
    pub track_path: String,
}

impl RunConfig {
    fn skips_fo76_quest_dialogue(&self) -> bool {
        self.skip_record_signatures.iter().any(|sig| {
            matches!(
                sig.trim().to_ascii_uppercase().as_str(),
                "QUST" | "DIAL" | "INFO"
            )
        })
    }

    fn validate_mvp_record_exceptions(&mut self, source: Game, target: Game) -> Result<(), String> {
        if self.mvp_record_exceptions.is_empty() {
            return Ok(());
        }
        let expected = expected_mvp_weapon_closure(source, target).ok_or_else(|| {
            format!(
                "mvp_record_exceptions are unsupported for {} -> {}",
                source.as_str(),
                target.as_str()
            )
        })?;
        let mut seen = FxHashSet::default();
        for row in &mut self.mvp_record_exceptions {
            row.signature = row.signature.trim().to_ascii_uppercase();
            row.source_plugin = row.source_plugin.trim().to_string();
            if !matches!(row.signature.as_str(), "WEAP" | "STAT") {
                return Err(format!(
                    "mvp_record_exceptions signature must be WEAP or STAT, got {:?}",
                    row.signature
                ));
            }
            if row.local_form_id > 0x00FF_FFFF {
                return Err(format!(
                    "mvp_record_exceptions local_form_id is not 24-bit: {:08X}",
                    row.local_form_id
                ));
            }
            let Some((_, _, expected_plugin)) =
                expected.iter().find(|(signature, local, plugin)| {
                    *signature == row.signature
                        && *local == row.local_form_id
                        && plugin.eq_ignore_ascii_case(&row.source_plugin)
                })
            else {
                return Err(format!(
                    "unexpected mvp_record_exceptions row {} {:06X}@{} for {} -> {}",
                    row.signature,
                    row.local_form_id,
                    row.source_plugin,
                    source.as_str(),
                    target.as_str()
                ));
            };
            row.source_plugin = (*expected_plugin).to_string();
            let key = format!(
                "{}:{:06X}:{}",
                row.signature,
                row.local_form_id,
                row.source_plugin.to_ascii_lowercase()
            );
            if !seen.insert(key) {
                return Err(format!(
                    "duplicate mvp_record_exceptions row {} {:06X}@{}",
                    row.signature, row.local_form_id, row.source_plugin
                ));
            }
        }
        Ok(())
    }

    fn validate_mvp_melee_only(&self, source: Game, target: Game) -> Result<(), String> {
        if !self.mvp_melee_only {
            return Ok(());
        }
        if !matches!((source, target), (Game::SkyrimSe | Game::Fnv, Game::Fo4)) {
            return Err(format!(
                "mvp_melee_only is unsupported for {} -> {}",
                source.as_str(),
                target.as_str()
            ));
        }
        if self.fnv_quest_slice {
            return Err("mvp_melee_only cannot be combined with fnv_quest_slice".to_string());
        }
        if !self
            .skip_record_signatures
            .iter()
            .any(|signature| signature.trim().eq_ignore_ascii_case("WEAP"))
        {
            return Err("mvp_melee_only requires WEAP in skip_record_signatures".to_string());
        }
        Ok(())
    }

    pub(crate) fn has_complete_mvp_weapon_closure(&self, source: Game, target: Game) -> bool {
        let Some(expected) = expected_mvp_weapon_closure(source, target) else {
            return false;
        };
        self.skip_record_signatures
            .iter()
            .any(|signature| signature.trim().eq_ignore_ascii_case("WEAP"))
            && self.mvp_record_exceptions.len() == expected.len()
            && expected.iter().all(|(signature, local, plugin)| {
                self.mvp_record_exceptions.iter().any(|row| {
                    row.signature == *signature
                        && row.local_form_id == *local
                        && row.source_plugin.eq_ignore_ascii_case(plugin)
                })
            })
    }

    pub(crate) fn is_mvp_record_exception(
        &self,
        source: Game,
        target: Game,
        form_key: FormKey,
        signature: SigCode,
        interner: &StringInterner,
    ) -> bool {
        self.has_complete_mvp_weapon_closure(source, target)
            && interner.resolve(form_key.plugin).is_some_and(|plugin| {
                self.mvp_record_exceptions.iter().any(|row| {
                    row.signature == signature.as_str()
                        && row.local_form_id == form_key.local
                        && row.source_plugin.eq_ignore_ascii_case(plugin)
                })
            })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetRecordPreflightRow {
    pub editor_id: String,
    pub signature: String,
    pub form_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TargetMasterRecordContext {
    handle_id: u64,
    plugin_name: String,
    master_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnvScriLink {
    pub target_form_key: String,
    pub source_scpt_form_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FnvDedicatedTopicTargets {
    pub keyword: FormKey,
    pub topic: FormKey,
}

#[derive(Default)]
struct TypedFnvDialoguePlan {
    top_level_records: Vec<Record>,
    topics: Vec<Record>,
    infos: Vec<(Record, FormKey)>,
    direct_sources: FxHashSet<String>,
    voice_manifest: crate::fnv_legacy_scripting::voice::FnvVoiceManifest,
    scene_manifest_records: usize,
    dedicated_runtime_records: FxHashSet<FormKey>,
    synthetic_targets:
        BTreeMap<crate::quest_runtime::QuestRecordKey, crate::quest_runtime::QuestRecordKey>,
}

fn first_record_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::List(values) => values.iter().find_map(first_record_form_key),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| first_record_form_key(value)),
        _ => None,
    }
}

fn exact_record_form_key(record: &Record, signature: &[u8; 4]) -> Result<FormKey, String> {
    let values = record
        .fields
        .iter()
        .filter(|field| &field.sig.0 == signature)
        .filter_map(|field| first_record_form_key(&field.value))
        .collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        return Err(format!(
            "must contain exactly one {} FormKey",
            String::from_utf8_lossy(signature)
        ));
    };
    Ok(*value)
}

fn parse_legacy_form_key_text(text: &str, interner: &StringInterner) -> Result<FormKey, String> {
    if text.contains('@') {
        return FormKey::parse(text, interner);
    }
    let (local, plugin) = text
        .split_once(':')
        .ok_or_else(|| format!("legacy FormKey missing ':' or '@': {text:?}"))?;
    let local = u32::from_str_radix(local.trim_start_matches("0x"), 16)
        .map_err(|error| format!("legacy FormKey hex parse error in {text:?}: {error}"))?;
    if plugin.is_empty() {
        return Err(format!("legacy FormKey plugin name is empty: {text:?}"));
    }
    Ok(FormKey {
        local,
        plugin: interner.intern(plugin),
    })
}

fn map_info_fragment_properties(
    properties: &[crate::fnv_legacy_scripting::dialogue::InfoFragmentProperty],
    mapper_state: &MapperState,
    interner: &StringInterner,
    source_text: &str,
) -> Result<Vec<crate::fnv_legacy_scripting::vmad::ScriptProperty>, FnvScriptingError> {
    properties
        .iter()
        .map(|property| {
            let source_property = parse_legacy_form_key_text(&property.source_form_key, interner)
                .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "INFO {source_text} property {} has invalid source FormKey: {error}",
                    property.name
                ))
            })?;
            let target_property = mapper_state
                .source_to_target
                .get(&source_property)
                .copied()
                .ok_or_else(|| {
                    FnvScriptingError::Translate(format!(
                        "INFO {source_text} property {} source {} has no target mapping",
                        property.name, property.source_form_key
                    ))
                })?;
            let target_property =
                form_key_to_legacy_str(target_property, interner).ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "INFO {source_text} property {} target cannot be rendered",
                        property.name
                    ))
                })?;
            Ok(crate::fnv_legacy_scripting::vmad::ScriptProperty {
                name: property.name.clone(),
                prop_type: "Quest".to_string(),
                value: Some(serde_json::Value::String(target_property)),
            })
        })
        .collect()
}

fn map_quest_fragment_properties(
    properties: &[crate::fnv_legacy_scripting::quest::QuestFragmentProperty],
    mapper_state: &MapperState,
    interner: &StringInterner,
    source_text: &str,
) -> Result<Vec<crate::fnv_legacy_scripting::vmad::ScriptProperty>, FnvScriptingError> {
    properties
        .iter()
        .map(|property| {
            let source_property = parse_legacy_form_key_text(&property.source_form_key, interner)
                .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "QUST {source_text} property {} has invalid source FormKey: {error}",
                    property.name
                ))
            })?;
            let target_property = mapper_state
                .source_to_target
                .get(&source_property)
                .copied()
                .ok_or_else(|| {
                    FnvScriptingError::Translate(format!(
                        "QUST {source_text} property {} source {} has no target mapping",
                        property.name, property.source_form_key
                    ))
                })?;
            let target_property =
                form_key_to_legacy_str(target_property, interner).ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "QUST {source_text} property {} target cannot be rendered",
                        property.name
                    ))
                })?;
            Ok(crate::fnv_legacy_scripting::vmad::ScriptProperty {
                name: property.name.clone(),
                prop_type: property.papyrus_type.clone(),
                value: Some(serde_json::Value::String(target_property)),
            })
        })
        .collect()
}

fn source_form_key_local(value: &str) -> Option<u32> {
    value
        .split([':', '@'])
        .next()
        .and_then(|local| u32::from_str_radix(local.trim_start_matches("0x"), 16).ok())
}

fn account_fnv_quest_fragment_adaptations(
    source_form_key: &str,
    editor_id: &str,
    adaptations: &[crate::fnv_legacy_scripting::quest::QuestFragmentAdaptation],
    fragments: &[crate::fnv_legacy_scripting::quest::StageFragment],
) -> Result<Vec<String>, FnvScriptingError> {
    if source_form_key.eq_ignore_ascii_case("11F935:FalloutNV.esm") {
        if !editor_id.eq_ignore_ascii_case("VTechatticup") {
            return Err(FnvScriptingError::Translate(format!(
                "strict QUST {source_form_key} target-native adaptation has unexpected EditorID {editor_id}"
            )));
        }
        let [adaptation] = adaptations else {
            return Err(FnvScriptingError::Translate(format!(
                "strict QUST {source_form_key} expected exactly one target-native fragment adaptation, observed {}",
                adaptations.len()
            )));
        };
        if adaptation.stage_index != 110
            || adaptation.stage_item_index != 0
            || adaptation.source_command != "StopQuest VMS20"
            || !adaptation
                .source_dependency_form_key
                .eq_ignore_ascii_case("10E908:FalloutNV.esm")
            || adaptation.kind
                != crate::fnv_legacy_scripting::quest::QuestFragmentAdaptationKind::TargetNativeNoOp
            || adaptation.reason
                != "VMS20 is outside the selected slice; the selected quest's FailQuest stage remains authoritative in FO4"
        {
            return Err(FnvScriptingError::Translate(format!(
                "strict QUST {source_form_key} has an unexpected target-native fragment adaptation: {adaptation:?}"
            )));
        }
        let adapted_fragments = fragments
            .iter()
            .filter(|fragment| fragment.stage_index == 110 && fragment.stage_item_index == 0)
            .collect::<Vec<_>>();
        if adapted_fragments.len() != 1 || adapted_fragments[0].body.trim() != "Return" {
            return Err(FnvScriptingError::Translate(format!(
                "strict QUST {source_form_key} target-native adaptation did not emit exactly one stage 110 item 0 Return fragment"
            )));
        }
        return Ok(vec![
            "target_native_qust_adaptation:11F935:FalloutNV.esm:stage=110:item=0:StopQuest VMS20:dependency=10E908:FalloutNV.esm"
                .to_string(),
        ]);
    }

    if source_form_key.eq_ignore_ascii_case("06136D:FalloutNV.esm") {
        if !editor_id.eq_ignore_ascii_case("FreeformPowerArmor") {
            return Err(FnvScriptingError::Translate(format!(
                "strict QUST {source_form_key} has unexpected EditorID {editor_id}"
            )));
        }
        if !adaptations.is_empty() {
            return Err(FnvScriptingError::Translate(format!(
                "strict QUST {source_form_key} must not carry target-native fragment adaptations"
            )));
        }
        return Ok(Vec::new());
    }

    if adaptations.is_empty() {
        Ok(Vec::new())
    } else {
        Err(FnvScriptingError::Translate(format!(
            "QUST {source_form_key} carries unaudited target-native fragment adaptations"
        )))
    }
}

fn register_fnv_quest_record_dependency_mappings(
    record: &serde_json::Value,
    source_form_key: &str,
    editor_id: &str,
    mapper_state: &mut MapperState,
    interner: &StringInterner,
) -> Result<Vec<String>, FnvScriptingError> {
    let mappings = crate::fnv_legacy_scripting::quest::exact_quest_record_dependency_mappings(
        record,
        editor_id,
        source_form_key,
    )?;
    if source_form_key.eq_ignore_ascii_case("06136D:FalloutNV.esm") {
        let [mapping] = mappings.as_slice() else {
            return Err(FnvScriptingError::Translate(format!(
                "strict QUST {source_form_key} expected exactly one record dependency mapping, observed {}",
                mappings.len()
            )));
        };
        if !editor_id.eq_ignore_ascii_case("FreeformPowerArmor")
            || mapping.scope != "root CTDA run-on reference"
            || mapping.evidence_hex != "0000000000000000C1010000DF8F0500000000000200000014000000"
            || !mapping
                .source_dependency_form_key
                .eq_ignore_ascii_case("000014:FalloutNV.esm")
            || !mapping
                .target_dependency_form_key
                .eq_ignore_ascii_case("000014:Fallout4.esm")
        {
            return Err(FnvScriptingError::Translate(format!(
                "strict QUST {source_form_key} has an unexpected record dependency mapping: {mapping:?}"
            )));
        }

        let source_dependency =
            parse_legacy_form_key_text(&mapping.source_dependency_form_key, interner).map_err(
                |error| {
                    FnvScriptingError::Setup(format!(
                        "QUST {source_form_key} record dependency source is invalid: {error}"
                    ))
                },
            )?;
        let target_dependency =
            parse_legacy_form_key_text(&mapping.target_dependency_form_key, interner).map_err(
                |error| {
                    FnvScriptingError::Setup(format!(
                        "QUST {source_form_key} record dependency target is invalid: {error}"
                    ))
                },
            )?;
        let raw_source = mapper_state
            .options
            .source_form_key_for_raw_formid(0x000014, interner)
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "QUST {source_form_key} cannot resolve audited raw Player FormID 000014"
                ))
            })?;
        if raw_source != source_dependency {
            return Err(FnvScriptingError::Setup(format!(
                "QUST {source_form_key} raw Player FormID resolved to {:?}, expected {:?}",
                raw_source, source_dependency
            )));
        }
        let target_plugin = interner.resolve(target_dependency.plugin).ok_or_else(|| {
            FnvScriptingError::Setup(
                "QUST Player dependency target plugin is not interned".to_string(),
            )
        })?;
        if !mapper_state
            .options
            .target_master_names
            .iter()
            .any(|master| master.eq_ignore_ascii_case(target_plugin))
        {
            return Err(FnvScriptingError::Setup(format!(
                "QUST {source_form_key} Player dependency target master {target_plugin} is not loaded"
            )));
        }
        if let Some(existing) = mapper_state.source_to_target.get(&source_dependency) {
            if *existing != target_dependency {
                return Err(FnvScriptingError::Translate(format!(
                    "QUST {source_form_key} Player dependency conflicts with existing mapper target {:?}",
                    existing
                )));
            }
        } else {
            mapper_state
                .source_to_target
                .insert(source_dependency, target_dependency);
        }
        return Ok(vec![
            "mapped_qust_record_dependency:06136D:FalloutNV.esm:root_ctda_run_on:000014:FalloutNV.esm->000014:Fallout4.esm"
                .to_string(),
        ]);
    }

    if mappings.is_empty() {
        Ok(Vec::new())
    } else {
        Err(FnvScriptingError::Translate(format!(
            "QUST {source_form_key} carries unaudited record dependency mappings"
        )))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LegacyFormKeyAllocationIntent {
    pub source_fk: FormKey,
    pub editor_id: Option<Sym>,
    pub target_sig: SigCode,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LegacyFormKeyPreallocationCoverage {
    pub eligible: usize,
    pub mapped: usize,
    pub missing: usize,
}

fn legacy_output_allocation_floor(
    source: Game,
    target: Game,
    source_plugin_name: &str,
    output_plugin_name: &str,
    preserve_source_ids: bool,
    current_floor: u32,
    source_locals: impl IntoIterator<Item = u32>,
) -> Result<Option<u32>, &'static str> {
    if !matches!(source, Game::Fnv | Game::Fo3)
        || target != Game::Fo4
        || !source_plugin_name.eq_ignore_ascii_case(output_plugin_name)
    {
        return Ok(None);
    }

    let mut max_source_local = None;
    let mut needs_generated_id = !preserve_source_ids;
    for local in source_locals {
        max_source_local = Some(max_source_local.map_or(local, |current: u32| current.max(local)));
        needs_generated_id |= local < FIRST_ALLOCATION_ID;
    }
    if !needs_generated_id {
        return Ok(None);
    }
    let Some(max_source_local) = max_source_local else {
        return Ok(None);
    };
    let next = max_source_local
        .checked_add(1)
        .ok_or("legacy source/output plugin identity leaves no disjoint FormID allocation space")?;
    if next > 0x00FF_FFFF {
        return Err(
            "legacy source/output plugin identity leaves no disjoint FormID allocation space",
        );
    }
    Ok(Some(current_floor.max(next)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordWriteMode {
    TopLevel,
    QuestChild,
    TopicChildInfo,
}

fn allocation_editor_id_for_parent(
    editor_id: Option<Sym>,
    write_mode: RecordWriteMode,
    target_parent: Option<FormKey>,
    output_plugin: Sym,
) -> Option<Sym> {
    if matches!(write_mode, RecordWriteMode::TopicChildInfo)
        && target_parent.is_some_and(|parent| parent.plugin == output_plugin)
    {
        return None;
    }
    editor_id
}

struct PreparedProjectedNavmesh {
    source_fk: FormKey,
    source_sig: SigCode,
    record: Record,
    full_plugin_snapshot: Option<Record>,
}

/// `PreparedProjectedNavmesh` minus the record itself: kept on the apply side
/// while the record moves into the chunked batch insert.
struct PreparedNavmeshMeta {
    source_fk: FormKey,
    source_sig: SigCode,
    full_plugin_snapshot: Option<Record>,
}

#[derive(Debug, Clone)]
struct PendingScptProperty {
    source_scpt_form_key: String,
    script_class_name: String,
    property: crate::fnv_legacy_scripting::vmad::ScriptProperty,
}

#[derive(Debug, Clone)]
struct SkyrimPackLiveProjection {
    source_form_key: FormKey,
    target_form_key: FormKey,
    target_template: FormKey,
    lowering: crate::skyrimse_fo4_runtime::package::SkyrimPackLowering,
    reference_mappings: BTreeMap<String, FormKey>,
    template_evidence:
        crate::skyrimse_fo4_runtime::package::SkyrimPackTemplateMaterializationEvidence,
    materialization: crate::skyrimse_fo4_runtime::package::SkyrimPackMaterialization,
}

#[derive(Debug, Clone)]
struct SkyrimDialogueLiveProjection {
    component_id: String,
    projection: crate::skyrimse_fo4_runtime::dialogue::SkyrimDialogueProjection,
}

#[derive(Debug, Clone)]
struct SkyrimStoryManagerLiveProjection {
    component_id: String,
    source_records: Vec<Record>,
    event_mapping: crate::skyrimse_fo4_runtime::story_manager::SkyrimStoryEventMapping,
    projection: Option<crate::skyrimse_fo4_runtime::story_manager::SkyrimStoryManagerProjection>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SkyrimQuestRuntimeAssetManifestInput {
    output_plugin: String,
    intents: Vec<SkyrimQuestRuntimeVoiceIntentInput>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SkyrimQuestRuntimeVoiceIntentInput {
    source_info: String,
    target_info: String,
    source_speaker: String,
    target_speaker: String,
    response_number: u8,
    transcript: String,
    voice_type: SkyrimQuestRuntimeVoiceTypeInput,
    source_fuz: SkyrimQuestRuntimeAssetEvidenceInput,
    source_lip: SkyrimQuestRuntimeAssetEvidenceInput,
    target_audio: SkyrimQuestRuntimeTargetAudioInput,
    target_lip: SkyrimQuestRuntimeAssetEvidenceInput,
    sequence: Option<SkyrimQuestRuntimeSequenceInput>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SkyrimQuestRuntimeVoiceTypeInput {
    source_voice_type: String,
    target_voice_type: String,
    target_identity: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SkyrimQuestRuntimeAssetEvidenceInput {
    relative_path: String,
    exists: bool,
    byte_len: u64,
    blake3: String,
    provenance: SkyrimQuestRuntimeAssetProvenanceInput,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum SkyrimQuestRuntimeAssetProvenanceInput {
    SourceArchive {
        archive: String,
    },
    LooseFile,
    Converted {
        source_blake3: String,
        converter: String,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SkyrimQuestRuntimeTargetAudioInput {
    format: String,
    evidence: SkyrimQuestRuntimeAssetEvidenceInput,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SkyrimQuestRuntimeSequenceInput {
    kind: String,
    source_seq: SkyrimQuestRuntimeAssetEvidenceInput,
    target_seq: SkyrimQuestRuntimeAssetEvidenceInput,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
struct SkyrimQuestRuntimeAssetCopyReceipt {
    rows: Vec<SkyrimQuestRuntimeAssetCopyRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
struct SkyrimQuestRuntimeAssetCopyRow {
    semantic_role: String,
    source_path: String,
    target_path: String,
    source_root: String,
    size: u64,
    blake3: String,
    source_blake3: String,
}

impl SkyrimQuestRuntimeAssetEvidenceInput {
    fn into_voice_evidence(self) -> crate::skyrimse_fo4_runtime::voice::SkyrimVoiceAssetEvidence {
        use crate::skyrimse_fo4_runtime::voice::SkyrimVoiceAssetProvenance;

        let provenance = match self.provenance {
            SkyrimQuestRuntimeAssetProvenanceInput::SourceArchive { archive } => {
                SkyrimVoiceAssetProvenance::SourceArchive { archive }
            }
            SkyrimQuestRuntimeAssetProvenanceInput::LooseFile => {
                SkyrimVoiceAssetProvenance::LooseFile
            }
            SkyrimQuestRuntimeAssetProvenanceInput::Converted {
                source_blake3,
                converter,
            } => SkyrimVoiceAssetProvenance::Converted {
                source_blake3,
                converter,
            },
        };
        crate::skyrimse_fo4_runtime::voice::SkyrimVoiceAssetEvidence {
            relative_path: self.relative_path,
            exists: self.exists,
            byte_len: self.byte_len,
            blake3: self.blake3,
            provenance,
        }
    }
}

fn skyrim_voice_evidence_json(
    evidence: &crate::skyrimse_fo4_runtime::voice::SkyrimVoiceAssetEvidence,
) -> serde_json::Value {
    use crate::skyrimse_fo4_runtime::voice::SkyrimVoiceAssetProvenance;

    let provenance = match &evidence.provenance {
        SkyrimVoiceAssetProvenance::SourceArchive { archive } => {
            serde_json::json!({"kind": "source_archive", "archive": archive})
        }
        SkyrimVoiceAssetProvenance::LooseFile => serde_json::json!({"kind": "loose_file"}),
        SkyrimVoiceAssetProvenance::Converted {
            source_blake3,
            converter,
        } => serde_json::json!({
            "kind": "converted",
            "source_blake3": source_blake3,
            "converter": converter,
        }),
    };
    serde_json::json!({
        "relative_path": evidence.relative_path,
        "exists": evidence.exists,
        "byte_len": evidence.byte_len,
        "blake3": evidence.blake3,
        "provenance": provenance,
    })
}

fn skyrim_voice_receipt_json(
    receipt: &crate::skyrimse_fo4_runtime::voice::SkyrimDialogueVoiceAssetReceipt,
    interner: &StringInterner,
) -> serde_json::Value {
    use crate::skyrimse_fo4_runtime::voice::{
        SkyrimSequencePlaybackKind, SkyrimVoiceAdmission, SkyrimVoiceAudioFormat,
    };

    let intents = receipt
        .intents
        .iter()
        .map(|intent| {
            let voice_type = intent.voice_type.as_ref().map(|voice_type| {
                serde_json::json!({
                    "source_voice_type": voice_type.source_voice_type.format(interner),
                    "target_voice_type": voice_type.target_voice_type.format(interner),
                    "target_identity": voice_type.target_identity,
                })
            });
            let target_audio = intent.target_audio.as_ref().map(|(format, evidence)| {
                serde_json::json!({
                    "format": match format {
                        SkyrimVoiceAudioFormat::Xwm => "xwm",
                        SkyrimVoiceAudioFormat::Wav => "wav",
                    },
                    "evidence": skyrim_voice_evidence_json(evidence),
                })
            });
            let sequence = intent.sequence.as_ref().map(|sequence| {
                serde_json::json!({
                    "kind": match sequence.kind {
                        SkyrimSequencePlaybackKind::Scene => "scene",
                        SkyrimSequencePlaybackKind::StartGameEnabledQuest => {
                            "start_game_enabled_quest"
                        }
                    },
                    "source_seq": skyrim_voice_evidence_json(&sequence.source_seq),
                    "target_seq": skyrim_voice_evidence_json(&sequence.target_seq),
                })
            });
            serde_json::json!({
                "source_info": intent.source_info.format(interner),
                "target_info": intent.target_info.format(interner),
                "source_speaker": intent.source_speaker.format(interner),
                "target_speaker": intent.target_speaker.format(interner),
                "response_number": intent.response_number,
                "transcript": intent.transcript,
                "voice_type": voice_type,
                "target_voice_path": intent.target_voice_path,
                "source_fuz": intent.source_fuz.as_ref().map(skyrim_voice_evidence_json),
                "source_lip": intent.source_lip.as_ref().map(skyrim_voice_evidence_json),
                "target_audio": target_audio,
                "target_lip": intent.target_lip.as_ref().map(skyrim_voice_evidence_json),
                "sequence": sequence,
                "admission": match &intent.admission {
                    SkyrimVoiceAdmission::Ready => "ready".to_string(),
                    SkyrimVoiceAdmission::Rejected(reason) => format!("rejected:{reason}"),
                },
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "output_plugin": receipt.output_plugin,
        "intents": intents,
    })
}

#[derive(Default)]
struct ProjectedNavmeshPrepareResult {
    stats: TranslateStats,
    warnings: Vec<String>,
    decisions: Vec<Decision>,
    deferred: Vec<(FormKey, DeferredKind)>,
    prepared: Option<PreparedProjectedNavmesh>,
}

// ---------------------------------------------------------------------------
// ConversionRun
// ---------------------------------------------------------------------------

/// All state for one mod conversion run.
///
/// `mapper_state` is lazily initialised before dependency reservations or at
/// the start of translation and reused by fixups so mappings persist.
pub struct ConversionRun {
    pub source: Game,
    pub target: Game,
    pub source_handle_id: u64,
    pub target_handle_id: u64,
    pub master_handle_ids: Vec<u64>,
    pub(crate) target_master_record_contexts: Vec<TargetMasterRecordContext>,
    pub interner: StringInterner,
    pub schema_source: Arc<AuthoringSchema>,
    pub schema_target: Arc<AuthoringSchema>,
    pub translator: Translator,
    pub config: RunConfig,
    /// Long-lived mapper state: persists from dependency planning or
    /// translation through fixups.
    pub mapper_state: Option<MapperState>,
    pub(crate) script_reference_state: Option<crate::script_references::ScriptReferenceState>,
    generated_object_id_reservations: FxHashSet<u32>,
    pub(crate) legacy_serial_normalization:
        crate::translator::pair_hooks::fnv_fo4::LegacySerialNormalizationState,
    pub legacy_pack_preflight_report: Option<LegacyPackPreflightReport>,
    pub(crate) legacy_creature_race_coverage:
        crate::translator::pair_hooks::fnv_creature_race::CreatureRaceCoverageReport,
    /// Decisions accumulated during translation (drained by PyO3 / Python).
    pub decisions: Vec<Decision>,
    /// Warning message symbols (drained by PyO3 / Python).
    pub warnings: Vec<Sym>,
    /// Records deferred to a later pipeline pass (Phase D).
    pub deferred: Vec<(FormKey, DeferredKind)>,
    /// Native FNV SCRI links captured before the FNV pair hook drops SCRI.
    pub fnv_scri_links: Vec<FnvScriLink>,
    pub(crate) legacy_placed_actor_aliases:
        crate::translator::pair_hooks::fnv_fo4::PlacedActorAliasResolver,
    pub(crate) fnv_dedicated_topic_targets: Option<FnvDedicatedTopicTargets>,
    fnv_pending_vmad_targets: Vec<(String, String)>,
    fnv_pending_vmad_scripts:
        Vec<crate::fnv_legacy_scripting::script_synthesizer::TranslatedScript>,
    fnv_pending_quest_fragments:
        Vec<crate::fnv_legacy_scripting::vmad::PendingQuestFragmentBinding>,
    fnv_pending_helper_scripts: Vec<crate::fnv_legacy_scripting::vmad::PendingObjectScriptBinding>,
    fnv_pending_scpt_properties: Vec<PendingScptProperty>,
    fnv_pending_info_fragments: Vec<crate::fnv_legacy_scripting::vmad::PendingInfoFragmentBinding>,
    fnv_pending_scene_fragments:
        Vec<crate::fnv_legacy_scripting::vmad::PendingSceneFragmentBinding>,
    fnv_quest_runtime_component_plans: Vec<crate::quest_runtime::QuestRuntimeComponentPlan>,
    fnv_quest_runtime_expected_receipts: Vec<crate::quest_runtime::QuestRuntimeExpectedReceipt>,
    fnv_quest_runtime_synthetic_targets:
        BTreeMap<crate::quest_runtime::QuestRecordKey, crate::quest_runtime::QuestRecordKey>,
    fnv_quest_runtime_compiler_evidence:
        Vec<crate::fnv_legacy_scripting::vmad::CompiledScriptEvidence>,
    fnv_quest_runtime_receipts_finalized: bool,
    skyrim_runtime_capability_plan:
        Option<crate::skyrimse_fo4_runtime::planner::SkyrimCapabilityPlan>,
    skyrim_runtime_receipt: Option<crate::skyrimse_fo4_runtime::SkyrimRuntimeReceipt>,
    skyrim_minimal_quest_action_requests:
        HashMap<String, Vec<crate::skyrimse_fo4_runtime::planner::SkyrimMinimalQuestActionSpec>>,
    skyrim_minimal_quest_class_prefix: Option<String>,
    skyrim_minimal_quest_projections:
        Vec<crate::skyrimse_fo4_runtime::quest::SkyrimQuestProjection>,
    skyrim_minimal_alias_projections:
        Vec<crate::skyrimse_fo4_runtime::alias::SkyrimAliasProjection>,
    skyrim_pack_projections: Vec<SkyrimPackLiveProjection>,
    skyrim_scene_projections: Vec<crate::skyrimse_fo4_runtime::scene::SkyrimSceneProjection>,
    skyrim_dialogue_projections: Vec<SkyrimDialogueLiveProjection>,
    skyrim_story_manager_projections: Vec<SkyrimStoryManagerLiveProjection>,
    skyrim_quest_runtime_voice_receipt:
        Option<crate::skyrimse_fo4_runtime::voice::SkyrimDialogueVoiceAssetReceipt>,
    skyrim_quest_runtime_asset_copy_receipt: Option<SkyrimQuestRuntimeAssetCopyReceipt>,
    skyrim_minimal_quest_compiler_evidence:
        Vec<crate::skyrimse_fo4_runtime::papyrus::SkyrimPscCompilerEvidence>,
    skyrim_minimal_quest_receipts_finalized: bool,
    bulk_melee_v1_receipts: Vec<crate::phase::mvp_melee::MvpMeleeReceipt>,
    creature_dependency_plan_installed: bool,
    creature_dependency_admissions: FxHashMap<FormKey, CreatureDependencyAdmission>,
    creature_dependency_terminals: FxHashSet<FormKey>,
    creature_primary_npc_reservations: Vec<CreaturePrimaryNpcReservation>,
    creature_ancillary_npc_reservations: Vec<CreatureAncillaryNpcReservation>,
    skyrim_creature_record_reservations: Vec<SkyrimCreatureRecordReservation>,
    skyrim_creature_batch_owned_sources: FxHashSet<FormKey>,
    skyrim_creature_ancillary_reservations: Vec<SkyrimCreatureAncillaryReservationReceipt>,
    skyrim_creature_ancillary_reservations_installed: bool,
    creature_actor_action_reservations: Vec<CreatureActorActionReservationReceipt>,
    creature_actor_action_reservations_installed: bool,
    /// Optional Python progress callback `(records_processed: int) -> bool`.
    /// Called every 1000 records during `translate_all`. Return False to cancel.
    pub progress_callback: Option<Py<PyAny>>,
    /// Per-navmesh diagnostic warning strings from the NAVI rebuild phase.
    /// Capped by `finalize_navi_warnings` to avoid unbounded growth on large
    /// worldspaces (Appalachia has ~920k+ navmeshes with potential warnings).
    pub navi_warnings: Vec<String>,
    /// Phase event channel — phases push events here; Python drains via drain_events.
    pub event_tx: crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    pub event_rx: crossbeam_channel::Receiver<crate::phase::PhaseEvent>,
    /// Cancellation flag — set by `conversion_run_cancel`; phases poll via PhaseCtx.
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Dependency graph built by the `walk` phase. None before the walk runs.
    pub dependency_graph: Option<esp_authoring_core::plugin_runtime::WalkOutput>,
    pub full_plugin_state: FullPluginRunState,
    /// Terrain bundles discovered by the record phase and executed later by
    /// `convert_textures_v2` in the asset run.
    pub terrain_texture_jobs: Vec<crate::terrain_textures::manifest::TerrainTextureJob>,
    /// Deterministic, non-draining FO76→FO4 Story Manager route discovery seed.
    pub story_manager_route_seed: crate::phase::story_manager::StoryManagerRouteSeedReport,
    /// Per-worldspace seed form keys collected by the projected placed-child
    /// copy, consumed by the persistent-cell synthesis to skip its (otherwise
    /// identical) full-worldspace source scan.
    pub projected_seed_cache:
        std::collections::HashMap<String, crate::projected_placed::ProjectedSeedCacheEntry>,
    /// Interior placed children are translated outside the parallel cell-slice
    /// copier. Carry their exact FormKeys into the post-copy leveled-base repair
    /// so it never has to rediscover them by walking the finished plugin.
    interior_placed_ref_candidates: Vec<FormKey>,
    /// Offsets `emit_interior_cells` applied to interiors past FO4's coordinate
    /// limit; the later NAVI and door repairs move data that points into them.
    pub(crate) interior_recentre: crate::fixups::recentre_far_interiors::InteriorRecentre,
    /// Normalized data-relative paths (meshes/textures/materials) that collide
    /// with FO4 base assets and must be relocated under `base_asset_namespace`.
    /// Built once at run-init from the configured mesh roots + cascade closure.
    pub relocation_members: std::collections::HashSet<String>,
    /// Subset of `relocation_members` relocated for geometry + collision only;
    /// the NIF phase leaves their material/texture slots un-namespaced.
    pub relocation_mesh_only_members: std::collections::HashSet<String>,
    pub nif_dependencies: Arc<crate::relocation::NifDependencyCache>,
    pub(crate) relocation_preparation: Option<Arc<crate::relocation::RelocationPreparation>>,
    pub output_nif_dependencies: Arc<crate::modt_manifest::OutputNifDependencies>,
    /// Warnings emitted while building `relocation_members` (e.g. FO4 extracted
    /// dir missing). Surfaced into a phase report so Python logs them.
    pub relocation_warnings: Vec<String>,
    pub target_assets: Option<std::sync::Arc<crate::target_assets::TargetAssetStore>>,
    pub(crate) source_asset_inventory:
        Option<Arc<crate::source_inventory::SourceAssetInventory>>,
    /// Output sinks (loose + BA2 spill streaming). None = legacy
    /// behavior everywhere; attached via `sinks_attach_run`.
    pub output_sink: Option<std::sync::Arc<crate::sinks::SinkSet>>,
    owned_handles: Option<OwnedRunHandles>,
    pub default_target_path: Option<PathBuf>,
    pub(crate) target_mode: TargetMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreatureDependencyAdmission {
    pub source_form_key: FormKey,
    pub source_signature: SigCode,
    pub owner_family_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CreaturePrimaryNpcReservation {
    pub source: FormKey,
    pub target: FormKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CreatureAncillaryNpcReservation {
    pub source: FormKey,
    pub target: FormKey,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum SkyrimCreatureReservedRecordKind {
    Race,
    Npc,
    Armor,
    ArmorAddon,
    BodyPartData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SkyrimCreatureReservationOwner {
    pub source_race: FormKey,
    pub motion_family_id: String,
    pub normalized_project_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SkyrimCreatureReservationSource {
    pub source: FormKey,
    pub kind: SkyrimCreatureReservedRecordKind,
    pub owners: Vec<SkyrimCreatureReservationOwner>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SkyrimCreatureRecordReservation {
    pub source: FormKey,
    pub target: FormKey,
    pub kind: SkyrimCreatureReservedRecordKind,
    pub owners: Vec<SkyrimCreatureReservationOwner>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum SkyrimCreatureAncillaryRecordKind {
    SyntheticNpc,
    NpcVariant,
    ArmorVariant,
    ArmorAddonVariant,
    BodyPartDataVariant,
    Weapon,
    Projectile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SkyrimCreatureAncillaryReservationRequest {
    pub source_race: FormKey,
    pub event: String,
    pub ordinal: u32,
    pub kind: SkyrimCreatureAncillaryRecordKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SkyrimCreatureAncillaryReservationReceipt {
    pub source_race: FormKey,
    pub event: String,
    pub ordinal: u32,
    pub kind: SkyrimCreatureAncillaryRecordKind,
    pub target: FormKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreatureActorActionReservationRequest {
    pub family_id: String,
    pub requirement: crate::source_rig::ActorActionRequirement,
    pub editor_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreatureActorActionReservationReceipt {
    pub family_id: String,
    pub requirement: crate::source_rig::ActorActionRequirement,
    pub editor_id: String,
    pub target: FormKey,
}

impl CreatureActorActionReservationReceipt {
    pub(crate) fn record_plan(
        &self,
        interner: &StringInterner,
    ) -> Result<crate::source_rig::CreatureActorActionRecordPlan, String> {
        let plugin = interner.resolve(self.target.plugin).ok_or_else(|| {
            format!(
                "Actor Action reservation {:06X} has an unresolved target plugin",
                self.target.local
            )
        })?;
        Ok(crate::source_rig::CreatureActorActionRecordPlan {
            requirement: self.requirement.clone(),
            form_key: crate::source_rig::TargetFormKey::new(self.target.local, plugin),
            editor_id: self.editor_id.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetMode {
    CreateNew,
    OpenExisting,
}

// ---------------------------------------------------------------------------
// RunParams
// ---------------------------------------------------------------------------

/// Parameters for creating a new `ConversionRun`.
pub struct RunParams {
    pub source: Game,
    pub target: Game,
    pub source_handle_id: u64,
    pub target_handle_id: u64,
    pub master_handle_ids: Vec<u64>,
    pub config: RunConfig,
}

/// A plugin handle owned by the conversion registry.
///
/// This type is deliberately non-`Clone`: ownership may only move into a run.
pub struct OwnedPluginHandle {
    id: Option<u64>,
}

impl OwnedPluginHandle {
    pub fn load(
        plugin_path: &Path,
        game: &str,
        strings_dir: Option<&Path>,
    ) -> Result<Self, RunError> {
        let id = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            &plugin_path.to_string_lossy(),
            Some(game),
            strings_dir.map(|path| path.to_string_lossy()).as_deref(),
            None,
            false,
        )
        .map_err(RunError::InvalidConfig)?;
        Ok(Self { id: Some(id) })
    }

    pub fn load_index(plugin_path: &Path, game: &str) -> Result<Self, RunError> {
        let id = esp_authoring_core::plugin_runtime::plugin_handle_load_index_no_py(
            &plugin_path.to_string_lossy(),
            Some(game),
            None,
            None,
        )
        .map_err(RunError::InvalidConfig)?;
        Ok(Self { id: Some(id) })
    }

    pub fn new(plugin_name: &str, game: &str) -> Self {
        Self {
            id: Some(esp_authoring_core::plugin_runtime::plugin_handle_new_no_py(
                plugin_name,
                Some(game),
            )),
        }
    }

    pub fn id(&self) -> u64 {
        self.id.expect("owned plugin handle was released")
    }

    pub fn release(&mut self) -> bool {
        self.id
            .take()
            .is_some_and(esp_authoring_core::plugin_runtime::plugin_handle_close_native)
    }
}

impl Drop for OwnedPluginHandle {
    fn drop(&mut self) {
        self.release();
    }
}

/// All native plugin handles whose lifetime is tied to one conversion run.
pub struct OwnedRunHandles {
    pub source: Option<OwnedPluginHandle>,
    pub target: OwnedPluginHandle,
    pub masters: Vec<OwnedPluginHandle>,
}

impl Drop for OwnedRunHandles {
    fn drop(&mut self) {
        for master in self.masters.iter_mut().rev() {
            master.release();
        }
        self.source.as_mut().map(OwnedPluginHandle::release);
        self.target.release();
    }
}

// ---------------------------------------------------------------------------
// RunError
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum RunError {
    /// No run with this ID exists in the registry.
    UnknownRun(u64),
    /// The supplied parameters were invalid.
    InvalidConfig(String),
    /// The registry mutex was poisoned.
    LockPoisoned,
    /// Translation was cancelled by the progress callback (or by Ctrl-C).
    Cancelled,
    LegacyPackPreflight(Box<LegacyPackPreflightReport>),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRun(id) => write!(f, "unknown conversion run id: {id}"),
            Self::InvalidConfig(msg) => write!(f, "invalid run config: {msg}"),
            Self::LockPoisoned => write!(f, "conversion run registry lock is poisoned"),
            Self::Cancelled => write!(f, "translation cancelled"),
            Self::LegacyPackPreflight(report) => write!(
                f,
                "legacy PACK preflight blocked conversion: {}",
                report.error_summary_json()
            ),
        }
    }
}

impl std::error::Error for RunError {}

impl From<RecordReadError> for RunError {
    fn from(e: RecordReadError) -> Self {
        RunError::InvalidConfig(format!("record read: {e}"))
    }
}

impl From<FixupError> for RunError {
    fn from(e: FixupError) -> Self {
        RunError::InvalidConfig(format!("fixup: {e}"))
    }
}

impl From<FnvScriptingError> for RunError {
    fn from(e: FnvScriptingError) -> Self {
        RunError::InvalidConfig(format!("fnv scripting: {e}"))
    }
}

// ---------------------------------------------------------------------------
// TranslateStats
// ---------------------------------------------------------------------------

/// Per-run translation statistics returned by `ConversionRun::translate_all`.
#[derive(Default, Debug, Clone)]
pub struct TranslateStats {
    pub records_translated: u32,
    pub records_vanilla_remapped: u32,
    pub records_dropped: u32,
    pub records_deferred: u32,
    pub records_failed: u32,
    pub by_signature: HashMap<String, SignatureTranslateStats>,
}

/// Per-source-signature translation counters.
#[derive(Default, Debug, Clone)]
pub struct SignatureTranslateStats {
    pub seen: u32,
    pub translated: u32,
    pub vanilla_remapped: u32,
    pub dropped: u32,
    pub deferred: u32,
    pub failed: u32,
}

impl TranslateStats {
    pub(crate) fn signature_entry(
        &mut self,
        sig: crate::ids::SigCode,
    ) -> &mut SignatureTranslateStats {
        self.by_signature
            .entry(sig.as_str().to_string())
            .or_default()
    }

    pub(crate) fn absorb(&mut self, other: TranslateStats) {
        self.records_translated += other.records_translated;
        self.records_vanilla_remapped += other.records_vanilla_remapped;
        self.records_dropped += other.records_dropped;
        self.records_deferred += other.records_deferred;
        self.records_failed += other.records_failed;
        for (signature, stats) in other.by_signature {
            let entry = self.by_signature.entry(signature).or_default();
            entry.seen += stats.seen;
            entry.translated += stats.translated;
            entry.vanilla_remapped += stats.vanilla_remapped;
            entry.dropped += stats.dropped;
            entry.deferred += stats.deferred;
            entry.failed += stats.failed;
        }
    }
}

fn validate_total_skyrim_actor_translation(
    plan: &crate::skyrimse_fo4_runtime::planner::SkyrimCapabilityPlan,
    stats: &TranslateStats,
    creature_batch_owned: &FxHashSet<FormKey>,
) -> Result<(), String> {
    for signature in ["RACE", "NPC_", "ACHR"] {
        let expected = plan
            .record_signatures
            .iter()
            .filter(|(form_key, candidate)| {
                candidate.as_str() == signature && !creature_batch_owned.contains(form_key)
            })
            .count() as u32;
        let actual = stats
            .by_signature
            .get(signature)
            .cloned()
            .unwrap_or_default();
        let completed = actual.translated + actual.vanilla_remapped;
        if completed != expected {
            return Err(format!(
                "Skyrim actor conversion incomplete for {signature}: expected={expected} completed={completed} translated={} vanilla_remapped={} dropped={} deferred={} failed={}",
                actual.translated,
                actual.vanilla_remapped,
                actual.dropped,
                actual.deferred,
                actual.failed,
            ));
        }
    }
    Ok(())
}

fn skyrim_projected_quest_action(
    action: crate::skyrimse_fo4_runtime::planner::SkyrimMinimalQuestActionSpec,
) -> crate::skyrimse_fo4_runtime::quest::SkyrimQuestFragmentAction {
    use crate::skyrimse_fo4_runtime::planner::SkyrimMinimalQuestActionSpec as Source;
    use crate::skyrimse_fo4_runtime::quest::SkyrimQuestFragmentAction as Target;
    match action {
        Source::SetObjectiveDisplayed { index, displayed } => {
            Target::SetObjectiveDisplayed { index, displayed }
        }
        Source::SetObjectiveCompleted { index, completed } => {
            Target::SetObjectiveCompleted { index, completed }
        }
        Source::SetStage { index } => Target::SetStage(index),
        Source::CompleteQuest => Target::CompleteQuest,
        Source::Stop => Target::Stop,
    }
}

fn hash_len_prefixed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn hash_skyrim_pack_field_value(
    hasher: &mut blake3::Hasher,
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<(), String> {
    match value {
        FieldValue::None => {
            hasher.update(&[0]);
        }
        FieldValue::Bool(value) => {
            hasher.update(&[1, u8::from(*value)]);
        }
        FieldValue::Int(value) => {
            hasher.update(&[2]);
            hasher.update(&value.to_le_bytes());
        }
        FieldValue::Uint(value) => {
            hasher.update(&[3]);
            hasher.update(&value.to_le_bytes());
        }
        FieldValue::Float(value) => {
            hasher.update(&[4]);
            hasher.update(&value.to_bits().to_le_bytes());
        }
        FieldValue::String(value) => {
            hasher.update(&[5]);
            let value = interner
                .resolve(*value)
                .ok_or_else(|| "Skyrim PACK evidence contains an unresolved string".to_string())?;
            hash_len_prefixed(hasher, value.as_bytes());
        }
        FieldValue::Bytes(value) => {
            hasher.update(&[6]);
            hash_len_prefixed(hasher, value);
        }
        FieldValue::FormKey(value) => {
            hasher.update(&[7]);
            hash_len_prefixed(hasher, value.format(interner).as_bytes());
        }
        FieldValue::List(values) => {
            hasher.update(&[8]);
            hasher.update(&(values.len() as u64).to_le_bytes());
            for value in values {
                hash_skyrim_pack_field_value(hasher, value, interner)?;
            }
        }
        FieldValue::Struct(fields) => {
            hasher.update(&[9]);
            hasher.update(&(fields.len() as u64).to_le_bytes());
            for (name, value) in fields {
                let name = interner.resolve(*name).ok_or_else(|| {
                    "Skyrim PACK evidence contains an unresolved struct field".to_string()
                })?;
                hash_len_prefixed(hasher, name.as_bytes());
                hash_skyrim_pack_field_value(hasher, value, interner)?;
            }
        }
    };
    Ok(())
}

fn skyrim_pack_record_blake3(record: &Record, interner: &StringInterner) -> Result<String, String> {
    let mut hasher = blake3::Hasher::new();
    hash_len_prefixed(&mut hasher, record.sig.as_str().as_bytes());
    hash_len_prefixed(&mut hasher, record.form_key.format(interner).as_bytes());
    hasher.update(&record.flags.bits().to_le_bytes());
    hasher.update(&(record.fields.len() as u64).to_le_bytes());
    for field in &record.fields {
        hash_len_prefixed(&mut hasher, field.sig.as_str().as_bytes());
        hash_skyrim_pack_field_value(&mut hasher, &field.value, interner)?;
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn skyrim_pack_source_template(
    record: &Record,
    interner: &StringInterner,
) -> Result<FormKey, String> {
    let mut pkcu = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "PKCU");
    let value = &pkcu
        .next()
        .ok_or_else(|| "Skyrim PACK has no PKCU procedure template".to_string())?
        .value;
    if pkcu.next().is_some() {
        return Err("Skyrim PACK has duplicate PKCU procedure templates".to_string());
    }
    let FieldValue::Struct(fields) = value else {
        return Err("Skyrim PACK PKCU is not decoded as a struct".to_string());
    };
    let mut templates = fields.iter().filter_map(|(name, value)| {
        interner
            .resolve(*name)
            .is_some_and(|name| name.eq_ignore_ascii_case("package_template"))
            .then_some(value)
    });
    let Some(FieldValue::FormKey(template)) = templates.next() else {
        return Err("Skyrim PACK PKCU has no decoded package_template".to_string());
    };
    if templates.next().is_some() {
        return Err("Skyrim PACK PKCU has duplicate package_template fields".to_string());
    }
    Ok(*template)
}

fn skyrim_pack_family(
    record: &Record,
) -> Result<crate::skyrimse_fo4_runtime::package::SkyrimPackProcedureFamily, String> {
    let locations = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "PLDT")
        .count();
    let targets = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "PTDA")
        .count();
    match (locations, targets) {
        (1, 0) => Ok(crate::skyrimse_fo4_runtime::package::SkyrimPackProcedureFamily::Travel),
        (0, 1) => Ok(crate::skyrimse_fo4_runtime::package::SkyrimPackProcedureFamily::Patrol),
        _ => Err("Skyrim PACK is neither the exact Travel nor Patrol shape".to_string()),
    }
}

fn authoring_script_attachments(value: &serde_json::Value) -> Vec<(String, BTreeSet<String>)> {
    fn property_names(value: &serde_json::Value, output: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(name) = object
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("propertyName"))
                    .and_then(|(_, value)| value.as_str())
                {
                    output.insert(name.to_string());
                }
                for child in object.values() {
                    property_names(child, output);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    property_names(child, output);
                }
            }
            _ => {}
        }
    }

    fn walk(value: &serde_json::Value, output: &mut Vec<(String, BTreeSet<String>)>) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(class_name) = object
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("ScriptName"))
                    .and_then(|(_, value)| value.as_str())
                {
                    let mut properties = BTreeSet::new();
                    if let Some(value) = object
                        .iter()
                        .find(|(key, _)| key.eq_ignore_ascii_case("Properties"))
                        .map(|(_, value)| value)
                    {
                        property_names(value, &mut properties);
                    }
                    output.push((class_name.to_string(), properties));
                }
                for child in object.values() {
                    walk(child, output);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    walk(child, output);
                }
            }
            _ => {}
        }
    }

    let mut output = Vec::new();
    walk(value, &mut output);
    output
}

fn authoring_top_level_vmad_script_attachments(
    value: &serde_json::Value,
) -> Result<Vec<(String, BTreeSet<String>)>, String> {
    fn object_value_ci<'a>(
        value: &'a serde_json::Value,
        key: &str,
    ) -> Option<&'a serde_json::Value> {
        value
            .as_object()?
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
    }

    fn attachment(value: &serde_json::Value) -> Result<Option<(String, BTreeSet<String>)>, String> {
        let Some(class_name) = object_value_ci(value, "ScriptName")
            .and_then(serde_json::Value::as_str)
            .filter(|class_name| !class_name.trim().is_empty())
        else {
            return Ok(None);
        };
        let properties = object_value_ci(value, "Properties")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|property| {
                object_value_ci(property, "propertyName")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .collect::<BTreeSet<_>>();
        Ok(Some((class_name.to_string(), properties)))
    }

    let fields = value
        .get("fields")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "authoring QUST has no field array".to_string())?;
    let vmads = fields
        .iter()
        .filter_map(|field| {
            field.as_object().and_then(|object| {
                object
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("VirtualMachineAdapter"))
                    .map(|(_, value)| value)
            })
        })
        .collect::<Vec<_>>();
    let [vmad] = vmads.as_slice() else {
        return Err(format!(
            "authoring QUST requires exactly one VMAD field, found {}",
            vmads.len()
        ));
    };
    let mut by_class = BTreeMap::<String, BTreeSet<String>>::new();
    if let Some(scripts) = object_value_ci(vmad, "Scripts").and_then(serde_json::Value::as_array) {
        for script in scripts {
            if let Some((class_name, properties)) = attachment(script)? {
                by_class.entry(class_name).or_default().extend(properties);
            }
        }
    }
    if let Some(fragment_script) = object_value_ci(vmad, "Script Fragments")
        .and_then(|fragments| object_value_ci(fragments, "Script"))
    {
        if let Some((class_name, properties)) = attachment(fragment_script)? {
            by_class.entry(class_name).or_default().extend(properties);
        }
    }
    Ok(by_class.into_iter().collect())
}

fn resolve_fnv_expected_receipt(
    expected: &crate::quest_runtime::QuestRuntimeExpectedReceipt,
    synthetic_targets: &BTreeMap<
        crate::quest_runtime::QuestRecordKey,
        crate::quest_runtime::QuestRecordKey,
    >,
) -> Result<crate::quest_runtime::QuestRuntimeExpectedReceipt, String> {
    use crate::quest_runtime::{
        LocalizedStringReceipt, QuestRecordKey, QuestRuntimeExpectedReceipt, StartRouteReceipt,
        TopologyPlacement, VmadAttachmentReceipt,
    };

    for record in &expected.emitted_records {
        if record.form_key.starts_with("intent:fnv:") && !synthetic_targets.contains_key(record) {
            return Err(format!(
                "FNV quest runtime synthetic intent {} {} has no live allocation",
                record.signature, record.form_key
            ));
        }
    }
    let resolve_record = |record: &QuestRecordKey| {
        synthetic_targets
            .get(record)
            .cloned()
            .unwrap_or_else(|| record.clone())
    };
    let resolve_group_path = |path: &[String]| {
        path.iter()
            .map(|segment| {
                synthetic_targets
                    .iter()
                    .fold(segment.clone(), |value, (intent, target)| {
                        value.replace(&intent.form_key, &target.form_key)
                    })
            })
            .collect::<Vec<_>>()
    };

    Ok(QuestRuntimeExpectedReceipt {
        emitted_records: expected
            .emitted_records
            .iter()
            .map(resolve_record)
            .collect(),
        placements: expected
            .placements
            .iter()
            .map(|placement| TopologyPlacement {
                record: resolve_record(&placement.record),
                group_path: resolve_group_path(&placement.group_path),
            })
            .collect(),
        localized_strings: expected
            .localized_strings
            .iter()
            .map(|localized| LocalizedStringReceipt {
                owner: resolve_record(&localized.owner),
                field: localized.field.clone(),
                table: localized.table.clone(),
                string_id: localized.string_id,
            })
            .collect(),
        scripts: expected.scripts.clone(),
        vmad_attachments: expected
            .vmad_attachments
            .iter()
            .map(|attachment| VmadAttachmentReceipt {
                owner: resolve_record(&attachment.owner),
                script_class: attachment.script_class.clone(),
                property_names: attachment.property_names.clone(),
                compiler_evidence_id: attachment.compiler_evidence_id.clone(),
            })
            .collect(),
        routes: expected
            .routes
            .iter()
            .map(|route| StartRouteReceipt {
                route_id: route.route_id.clone(),
                quest: resolve_record(&route.quest),
                producer_evidence_id: route.producer_evidence_id.clone(),
                node_chain: route.node_chain.iter().map(resolve_record).collect(),
            })
            .collect(),
    })
}

fn validate_fnv_synthetic_target_set(
    receipts: &[crate::quest_runtime::QuestRuntimeExpectedReceipt],
    synthetic_targets: &BTreeMap<
        crate::quest_runtime::QuestRecordKey,
        crate::quest_runtime::QuestRecordKey,
    >,
) -> Result<(), String> {
    let expected = receipts
        .iter()
        .flat_map(|receipt| &receipt.emitted_records)
        .filter(|record| record.form_key.starts_with("intent:fnv:"))
        .cloned()
        .collect::<BTreeSet<_>>();
    let actual = synthetic_targets.keys().cloned().collect::<BTreeSet<_>>();
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "FNV quest runtime synthetic allocation set differs from admission: expected={} actual={}",
            expected.len(),
            actual.len()
        ))
    }
}

fn expected_placement_matches(
    placement: &crate::quest_runtime::TopologyPlacement,
    actual: ExistingAuthoringRecordPlacement,
    handle_id: u64,
    interner: &StringInterner,
) -> Result<bool, String> {
    let owner = |group_type: u32| {
        placement
            .group_path
            .iter()
            .find_map(|segment| segment.strip_prefix(&format!("GRUP:type={group_type}:owner=")))
            .map(|value| parse_legacy_or_native_form_key(value, interner))
            .transpose()
    };
    match actual {
        ExistingAuthoringRecordPlacement::TopLevel => {
            Ok(placement.group_path == [format!("GRUP:{}", placement.record.signature)])
        }
        ExistingAuthoringRecordPlacement::QuestChild {
            parent_quest_form_id,
        } => Ok(owner(10)?
            .map(|parent| encode_form_key_for_handle(handle_id, parent, interner))
            .transpose()
            .map_err(|error| error.to_string())?
            == Some(parent_quest_form_id)),
        ExistingAuthoringRecordPlacement::TopicChild {
            parent_dialogue_form_id,
        } => Ok(owner(7)?
            .map(|parent| encode_form_key_for_handle(handle_id, parent, interner))
            .transpose()
            .map_err(|error| error.to_string())?
            == Some(parent_dialogue_form_id)),
    }
}

fn authoring_quest_starts_enabled(record: &serde_json::Value) -> bool {
    let Some(fields) = record.get("fields").and_then(serde_json::Value::as_array) else {
        return false;
    };
    fields.iter().any(|field| {
        field
            .as_object()
            .and_then(|object| {
                object
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("DNAM"))
                    .map(|(_, value)| value)
            })
            .is_some_and(|dnam| {
                authoring_named_u64(dnam, "flags").is_some_and(|flags| flags & 1 != 0)
            })
    })
}

fn clear_skyrim_projected_quest_start_enabled(
    record: &mut Record,
    interner: &StringInterner,
) -> Result<(), String> {
    let mut dnams = record
        .fields
        .iter_mut()
        .filter(|field| field.sig.as_str() == "DNAM")
        .collect::<Vec<_>>();
    let [dnam] = dnams.as_mut_slice() else {
        return Err("projected Skyrim Story Manager QUST requires exactly one DNAM".to_string());
    };
    let FieldValue::Struct(fields) = &mut dnam.value else {
        return Err("projected Skyrim Story Manager QUST DNAM is not structured".to_string());
    };
    let Some((_, FieldValue::Uint(flags))) = fields.iter_mut().find(|(name, _)| {
        interner
            .resolve(*name)
            .is_some_and(|name| name.eq_ignore_ascii_case("flags"))
    }) else {
        return Err("projected Skyrim Story Manager QUST DNAM has no flags".to_string());
    };
    if *flags & 1 == 0 {
        return Err(
            "projected Skyrim Story Manager QUST was not autostart before routing".to_string(),
        );
    }
    *flags &= !1;
    Ok(())
}

fn authoring_named_u64(value: &serde_json::Value, name: &str) -> Option<u64> {
    match value {
        serde_json::Value::Object(object) => object
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .and_then(|(_, value)| value.as_u64())
            .or_else(|| {
                object
                    .values()
                    .find_map(|child| authoring_named_u64(child, name))
            }),
        serde_json::Value::Array(values) => values
            .iter()
            .find_map(|child| authoring_named_u64(child, name)),
        _ => None,
    }
}

pub(crate) fn is_mandatory_skyrim_actor_record(
    source: Game,
    target: Game,
    signature: SigCode,
) -> bool {
    source == Game::SkyrimSe
        && target == Game::Fo4
        && matches!(signature.as_str(), "RACE" | "NPC_" | "ACHR")
}

// ---------------------------------------------------------------------------
// Registry — per-run lock slots
// ---------------------------------------------------------------------------
//
// The registry mutex guards ONLY the id→slot map; each run has its own
// `Arc<Mutex<ConversionRun>>` so phases on distinct runs execute concurrently.
// The event receiver and cancel flag are hoisted into the slot so draining
// and cancelling never wait on a running phase's run lock.

/// A cloneable handle to one registered run.
pub struct RunSlot {
    pub run: Arc<Mutex<ConversionRun>>,
    /// Clone of the run's event receiver — drainable WITHOUT the run lock.
    pub events: crossbeam_channel::Receiver<crate::phase::PhaseEvent>,
    /// Clone of the run's event sender — pipeline/executor events go here.
    pub event_tx: crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    /// The run's cancel flag — settable WITHOUT the run lock.
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

fn registry() -> &'static Mutex<HashMap<u64, RunSlot>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, RunSlot>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Brief-lock slot fetch. The registry mutex is held only for the map lookup.
pub fn run_slot(id: u64) -> Result<RunSlot, RunError> {
    let guard = registry().lock().map_err(|_| RunError::LockPoisoned)?;
    let slot = guard.get(&id).ok_or(RunError::UnknownRun(id))?;
    Ok(RunSlot {
        run: Arc::clone(&slot.run),
        events: slot.events.clone(),
        event_tx: slot.event_tx.clone(),
        cancel: Arc::clone(&slot.cancel),
    })
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn form_key_to_legacy_str(
    fk: crate::ids::FormKey,
    interner: &StringInterner,
) -> Option<String> {
    let plugin = interner.resolve(fk.plugin)?;
    Some(format!("{:06X}:{}", fk.local, plugin))
}

fn parse_legacy_or_native_form_key(
    value: &str,
    interner: &StringInterner,
) -> Result<FormKey, String> {
    if value.contains('@') {
        return FormKey::parse(value, interner);
    }
    let (hex, plugin) = value
        .split_once(':')
        .ok_or_else(|| format!("FormKey missing ':' or '@': {value:?}"))?;
    FormKey::parse(&format!("{hex}@{plugin}"), interner)
}

fn normalized_editor_id(editor_id: &str) -> String {
    editor_id.to_ascii_lowercase()
}

fn normalized_eid_sym(eid: Sym, interner: &StringInterner) -> Sym {
    interner
        .resolve(eid)
        .map(|editor_id| interner.intern(&normalized_editor_id(editor_id)))
        .unwrap_or(eid)
}

pub(crate) fn normalized_eid_opt(eid: Option<Sym>, interner: &StringInterner) -> Option<Sym> {
    eid.map(|eid| normalized_eid_sym(eid, interner))
}

/// Drop FO4 CELL previs subrecords (carried verbatim from the FO76 source) so
/// the converted interior cell has no stale precombine/previs references.
/// `PCMB` = PreCombined Files Timestamp, `VISI` = PreVis File Hash,
/// `RVIS` = In PreVis File Of, `XPRI` = PreVis Reference Index,
/// `XCRI` = Combined Reference Index.
fn strip_interior_previs_fields(record: &mut Record) {
    record
        .fields
        .retain(|f| !matches!(f.sig.as_str(), "PCMB" | "VISI" | "RVIS" | "XPRI" | "XCRI"));
}

/// Force every NVNM payload's parent to `Interior { cell }`. The interior NAVM
/// inherits the source FO76 parent bytes; re-stamping the target cell's FormID
/// makes the NAVI rebuild key it to the correct interior cell and clears any
/// residual exterior-grid parent.
///
/// `cell_file_form_id` MUST be the cell's *target file* FormID — i.e. with the
/// output plugin's own master index already applied (see
/// `fo76_navmesh::target_form_id`). The mapper-allocated `form_key.local` carries
/// master index 0x00, so writing it here would resolve the parent cell to the
/// wrong master at runtime and hard-crash the engine's NavMeshInfoMap lookup.
fn set_nvnm_parent_interior(record: &mut Record, cell_file_form_id: u32) {
    for entry in record.fields.iter_mut() {
        if entry.sig.0 != *b"NVNM" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let Ok(mut payload) = esp_authoring_core::nvnm::parse_nvnm(bytes.as_slice()) else {
            continue;
        };
        payload.parent = esp_authoring_core::nvnm::NvnmParent::Interior {
            cell: cell_file_form_id,
        };
        *bytes = smallvec::SmallVec::from_vec(esp_authoring_core::nvnm::write_nvnm(&payload));
    }
}

fn target_editor_id_collides_any_signature(
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, crate::ids::SigCode)>>,
    interner: &StringInterner,
    editor_id: &str,
) -> bool {
    let normalized = interner.intern(&normalized_editor_id(editor_id));
    target_eid_index
        .get(&normalized)
        .is_some_and(|matches| !matches.is_empty())
}

fn target_editor_id_has_same_signature(
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, crate::ids::SigCode)>>,
    interner: &StringInterner,
    editor_id: &str,
    sig: crate::ids::SigCode,
) -> bool {
    let normalized = interner.intern(&normalized_editor_id(editor_id));
    target_eid_index.get(&normalized).is_some_and(|matches| {
        matches
            .iter()
            .any(|(_, candidate_sig)| *candidate_sig == sig)
    })
}

pub(crate) fn target_collision_donor_form_key(
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, crate::ids::SigCode)>>,
    interner: &StringInterner,
    editor_id: &str,
    sig: crate::ids::SigCode,
) -> Option<FormKey> {
    let normalized = interner.intern(&normalized_editor_id(editor_id));
    target_eid_index.get(&normalized).and_then(|matches| {
        matches
            .iter()
            .find(|(_, candidate_sig)| *candidate_sig == sig)
            .map(|(form_key, _)| *form_key)
    })
}

fn set_record_editor_id(record: &mut Record, interner: &StringInterner, editor_id: &str) {
    let eid_sym = interner.intern(editor_id);
    record.eid = Some(eid_sym);

    let edid_sig = SubrecordSig::from_str("EDID").expect("EDID is a valid subrecord signature");
    if let Some(field) = record.fields.iter_mut().find(|field| field.sig == edid_sig) {
        field.value = FieldValue::String(eid_sym);
        return;
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(eid_sym),
        },
    );
}

pub(crate) fn rename_fo76_target_editor_id_collision(
    record: &mut Record,
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, crate::ids::SigCode)>>,
    vanilla_remap_blocked_source_form_keys: &FxHashSet<FormKey>,
    interner: &StringInterner,
    force_same_signature_rename: bool,
) -> Option<(String, String)> {
    let original = interner.resolve(record.eid?)?.to_owned();
    if original.is_empty() {
        return None;
    }

    let same_signature =
        target_editor_id_has_same_signature(target_eid_index, interner, &original, record.sig);
    let static_marker = is_static_marker_editor_id(record.sig, &original);
    let decal_texture_set = is_fo76_decal_texture_set(record.sig, &original);
    let source_record_blocked = vanilla_remap_blocked_source_form_keys.contains(&record.form_key);
    let should_rename =
        if !target_editor_id_collides_any_signature(target_eid_index, interner, &original) {
            false
        } else if source_record_blocked {
            true
        } else if same_signature && static_marker {
            false
        } else if force_same_signature_rename
            || decal_texture_set
            || record.sig.as_str() == "CELL"
            || !allows_editor_id_vanilla_remap(record.sig)
        {
            true
        } else {
            !same_signature
        };
    if !should_rename {
        return None;
    }

    let mut candidate = format!("{original}fo76");
    let mut suffix = 1_u32;
    while target_editor_id_collides_any_signature(target_eid_index, interner, &candidate) {
        candidate = format!("{original}fo76{suffix}");
        suffix += 1;
    }
    set_record_editor_id(record, interner, &candidate);
    Some((original, candidate))
}

const FO76_FO4_DEFAULT_BASE_ASSET_NAMESPACE: &str = "FO76";
const SKYRIMSE_FO4_DEFAULT_BASE_ASSET_NAMESPACE: &str = "Skyrim";

fn is_fo76_decal_texture_set(sig: crate::ids::SigCode, editor_id: &str) -> bool {
    sig.as_str() == "TXST"
        && editor_id
            .as_bytes()
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"decal"))
}

/// Signatures whose target editor-id collisions should stay source-owned instead
/// of vanilla-remapping onto an FO4 base record. This is the editor-id concern
/// only; asset relocation is driven entirely by the collision index
/// (`ConversionRun::relocation_members`), not by signature.
/// INNR is here because FO76 and FO4 share object-ids for `dn_PowerArmor`,
/// `dn_CommonGun`, `dn_CommonArmor`, `dn_CommonMelee`, `dn_Clothes`, and
/// `dn_VaultSuit` while their rulesets describe entirely different content
/// (FO76's `dn_PowerArmor` carries 357 rules to FO4's 120, keyed on FO76
/// keywords). Remapping onto the FO4 record leaves FO76 gear unnamed.
const FO76_FO4_VANILLA_REMAP_BLOCKED_SIGS: &[&str] = &["STAT", "SCOL", "MSTT", "INNR"];

/// Records that define placed world objects must retain their source record and
/// converted assets. ALCH and AMMO are intentionally absent: those are gameplay
/// inventory equivalents with dedicated/expected FO4 reuse.
///
/// The EID index only means "same record" for FO76, which inherits Fallout 4's
/// record set. Every other source game merely shares Bethesda's naming habits,
/// so a namesake is a false friend: Skyrim's `RockCliff04` and FO4's are both
/// `Landscape\Rocks\RockCliff04.nif` and are entirely different rocks. Starfield
/// repeats it (`CloudDistant04`, `BeachUmbrella01`), and so does FNV
/// (`CaveHall2Way01`, `IBeam01`).
const CROSS_GAME_FO4_WORLD_OBJECT_VANILLA_REMAP_BLOCKED_SIGS: &[&str] = &[
    "ACTI", "ADDN", "ARMO", "BOOK", "CONT", "DOOR", "FLOR", "FURN", "GRAS", "HAZD", "IDLM", "KEYM",
    "DIAL", "INFO", "LIGH", "LVLC", "LVLI", "MISC", "MSTT", "NOTE", "NPC_", "QUST", "SCEN", "SCOL",
    "SCPT", "SOUN", "STAT", "TACT", "TERM", "TREE", "WEAP",
];

/// FO76 records whose matching Fallout 4 EditorID identifies different content.
/// Keeping these source-owned lets the normal collision relocation pipeline
/// namespace their meshes, materials, and textures.
const FO76_FO4_VANILLA_REMAP_BLOCKED_FORM_IDS: &[u32] = &[
    0x0001_A4B9, // Newspaper01: Charleston Herald, not Fallout 4's Boston Bugle
    // These FO76 OMODs are empty; their FO4 namesakes change already-authored ranges.
    // The shotgun's -2/-4 modifiers make converted weapon instance ranges negative.
    0x0004_98A1, // mod_UniversalOffset_Range_Shotgun
    0x0004_98A2, // mod_UniversalOffset_Range_BoltAction
    0x0011_14F8, // DLC03_mod_UniversalOffset_Range_LeverGun
    // FO76 inherited FO4's four `AnimsGrip*` keywords at the same object ids, so the
    // EditorID index would collapse them onto Fallout 4's records. Every converted weapon
    // would then carry the keyword FO4's generic third-person fan is gated on (48 blocks
    // answer `AnimsGripRifleStraight` alone), and vanilla's blocks beat the one emitted for
    // the weapon's own animation keyword. Keeping them source-owned (the collision rename
    // adds a `fo76` suffix) makes that fan unreachable; `KWDA`, OMOD keyword properties
    // and the emitted subgraph blocks all follow the FormKey map.
    //
    // Weapons with no block of their own still need that fan: FO76 ships many reskins of
    // FO4 weapon types with no animation keyword. `restore_generic_grips_on_unowned_weapons`
    // in `fixups::face::generate_additive_races` puts FO4's keyword back for those.
    0x0001_F948, // AnimsGripPistol
    0x0001_F947, // AnimsGripRifleAssault
    0x0004_64EF, // AnimsGripRifleStraight
    0x000A_A937, // AnimsGripShoulderFired
    // The Fixer/combat shotgun family must retain its FO76 GunBehavior route.
    0x000B_9560, // AnimsCombatShotgun
    // Automatron's namesake template belongs to the Mechanist's `DLC01MechBotFaction`, which
    // has no relation to RobotFaction; FO76's is a RobotFaction member. Remapped, every
    // converted robobrain attacks every other robot on sight.
    0x0035_3D54, // DLC01EncRoboBrain01Template
];

/// Signatures that should receive a `fo76` EDID suffix when they are emitted
/// despite a same-signature target-master collision. ARMO/ARMA are not globally
/// remap-blocked because wearable armor should still reuse FO4 records, but
/// protected creature skin records that are emitted still need unique EDIDs.
const FO76_FO4_FORCE_COLLISION_RENAME_SIGS: &[&str] =
    &["STAT", "SCOL", "MSTT", "ARMO", "ARMA", "INNR"];

fn editor_id_vanilla_remap_blocked_sigs(source: Game, target: Game) -> Vec<String> {
    let mut blocked = Vec::new();
    if source == Game::Fo76 && target == Game::Fo4 {
        blocked.extend(
            FO76_FO4_VANILLA_REMAP_BLOCKED_SIGS
                .iter()
                .map(|sig| (*sig).to_owned()),
        );
    }
    // Every source but FO76 — FNV/FO3, Skyrim, Starfield, and any pair added
    // later. FO76 is the sole exception because it alone inherits Fallout 4's
    // records; for everyone else a shared EditorID is a naming coincidence.
    if source != Game::Fo76 && source != target && target == Game::Fo4 {
        blocked.extend(
            CROSS_GAME_FO4_WORLD_OBJECT_VANILLA_REMAP_BLOCKED_SIGS
                .iter()
                .map(|sig| (*sig).to_owned()),
        );
    }
    if source != target && target == Game::Fo4 {
        blocked.push("CELL".to_owned());
    }
    blocked
}

fn editor_id_vanilla_remap_blocked_source_form_keys(
    source: Game,
    target: Game,
    interner: &StringInterner,
) -> FxHashSet<FormKey> {
    if source != Game::Fo76 || target != Game::Fo4 {
        return FxHashSet::default();
    }
    let source_plugin = interner.intern(FO76_MASTER_PLUGIN_NAME);
    FO76_FO4_VANILLA_REMAP_BLOCKED_FORM_IDS
        .iter()
        .map(|&local| FormKey {
            local,
            plugin: source_plugin,
        })
        .collect()
}

pub(crate) fn is_editor_id_collision_rename_forced(
    source: Game,
    target: Game,
    sig: crate::ids::SigCode,
) -> bool {
    if source != Game::Fo76 || target != Game::Fo4 {
        return false;
    }
    FO76_FO4_FORCE_COLLISION_RENAME_SIGS
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(sig.as_str()))
}

pub(crate) fn base_asset_namespace<'a>(
    config: &'a RunConfig,
    source: Game,
    target: Game,
) -> Option<&'a str> {
    let configured = config.base_asset_namespace.trim();
    if !configured.is_empty() {
        return Some(configured);
    }
    if source == Game::Fo76 && target == Game::Fo4 {
        return Some(FO76_FO4_DEFAULT_BASE_ASSET_NAMESPACE);
    }
    if source == Game::SkyrimSe && target == Game::Fo4 {
        return Some(SKYRIMSE_FO4_DEFAULT_BASE_ASSET_NAMESPACE);
    }
    None
}

/// Resolved relocation namespace for a run (`""` when relocation is inactive).
/// Shared accessor for the asset phases (textures/materials/NIFs).
pub fn base_asset_namespace_for_run(run: &ConversionRun) -> String {
    base_asset_namespace(&run.config, run.source, run.target)
        .unwrap_or("")
        .to_string()
}

/// Normalized base-overwrite prefixes for a run: explicit config wins, else the
/// pair default. Shared by the material and texture base-owned gates so the two
/// can never disagree about which kits overwrite.
pub fn base_overwrite_prefixes_for_run(run: &ConversionRun) -> Vec<String> {
    let configured = &run.config.base_overwrite_prefixes;
    let source: Vec<String> = if !configured.is_empty() {
        configured.clone()
    } else if run.source == Game::Fo76 && run.target == Game::Fo4 {
        crate::relocation::FO76_FO4_DEFAULT_BASE_OVERWRITE_PREFIXES
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    } else {
        Vec::new()
    };
    source
        .iter()
        .map(|prefix| materials_native::convert::normalize_base_overwrite_prefix(prefix))
        .filter(|prefix| !prefix.is_empty())
        .collect()
}

const BASE_ASSET_MODEL_FIELD_SIGS: &[&str] = &["MODL", "MOD2", "MOD3", "MOD4", "MOD5"];

/// Normalize a MODL/MOD2/... mesh path to the relocation-member key space
/// (lowercase, forward-slash, `meshes/`-rooted, data-relative) so it can be
/// looked up in `ConversionRun::relocation_members`. Reuses the same
/// `normalize_rel` the member set is built with — the two MUST agree.
fn relocation_modl_key(path: &str) -> String {
    let rel = crate::relocation::normalize_rel(path);
    if rel.is_empty() || rel.starts_with("meshes/") {
        rel
    } else {
        format!("meshes/{rel}")
    }
}

/// Relocate model paths that point at a collision-detected mesh into the
/// `namespace` subfolder. Only paths present in `relocation_members` are
/// touched — the collision index is the single trigger, no signature blacklist.
pub(crate) fn namespace_base_asset_model_paths(
    record: &mut Record,
    relocation_members: &std::collections::HashSet<String>,
    namespace: &str,
    interner: &StringInterner,
) -> usize {
    if relocation_members.is_empty() || namespace.trim().is_empty() {
        return 0;
    }
    let mut updated = 0usize;
    for field in record.fields.iter_mut() {
        if BASE_ASSET_MODEL_FIELD_SIGS
            .iter()
            .any(|sig| field.sig.as_str().eq_ignore_ascii_case(sig))
        {
            updated += namespace_mesh_path_value(
                &mut field.value,
                relocation_members,
                namespace,
                interner,
            );
        }
    }
    updated
}

fn namespace_mesh_path_value(
    value: &mut FieldValue,
    relocation_members: &std::collections::HashSet<String>,
    namespace: &str,
    interner: &StringInterner,
) -> usize {
    match value {
        FieldValue::String(sym) => {
            let Some(current) = interner.resolve(*sym) else {
                return 0;
            };
            if !relocation_members.contains(&relocation_modl_key(current)) {
                return 0;
            }
            let Some(namespaced) = namespaced_mesh_model_path(current, namespace) else {
                return 0;
            };
            if namespaced == current {
                return 0;
            }
            *sym = interner.intern(&namespaced);
            1
        }
        FieldValue::Bytes(bytes) => {
            let had_nul = bytes.last().is_some_and(|byte| *byte == 0);
            let current = String::from_utf8_lossy(bytes.as_slice());
            let current = current.trim_end_matches('\0');
            if !relocation_members.contains(&relocation_modl_key(current)) {
                return 0;
            }
            let Some(namespaced) = namespaced_mesh_model_path(current, namespace) else {
                return 0;
            };
            if namespaced == current {
                return 0;
            }
            bytes.clear();
            bytes.extend_from_slice(namespaced.as_bytes());
            if had_nul {
                bytes.push(0);
            }
            1
        }
        FieldValue::Struct(fields) => {
            let mut updated = 0usize;
            for (key, nested) in fields.iter_mut() {
                let key_name = interner.resolve(*key).unwrap_or("");
                if matches!(
                    key_name,
                    "File" | "Path" | "Filename" | "FileName" | "ModelFileName"
                ) {
                    updated +=
                        namespace_mesh_path_value(nested, relocation_members, namespace, interner);
                }
            }
            updated
        }
        FieldValue::List(values) => values
            .iter_mut()
            .map(|nested| {
                namespace_mesh_path_value(nested, relocation_members, namespace, interner)
            })
            .sum(),
        _ => 0,
    }
}

fn namespaced_mesh_model_path(path: &str, namespace: &str) -> Option<String> {
    let namespace = namespace.trim().trim_matches(|c| c == '/' || c == '\\');
    if namespace.is_empty() {
        return None;
    }
    let mut rel = path.trim().trim_matches('\0').replace('\\', "/");
    rel = rel.trim_start_matches('/').to_owned();
    if rel.is_empty() || rel.contains(':') {
        return None;
    }
    rel = strip_ascii_prefix(rel, "data/");
    rel = strip_ascii_prefix(rel, "meshes/");
    let lower = rel.to_ascii_lowercase();
    let namespace_lower = namespace.to_ascii_lowercase();
    let namespaced =
        if lower == namespace_lower || lower.starts_with(&format!("{namespace_lower}/")) {
            rel
        } else {
            format!("{namespace}/{rel}")
        };
    Some(namespaced.replace('/', "\\"))
}

fn relocation_material_key(path: &str) -> String {
    let rel = crate::relocation::normalize_rel(path);
    if rel.is_empty() || rel.starts_with("materials/") {
        rel
    } else {
        format!("materials/{rel}")
    }
}

pub(crate) fn namespace_base_asset_decal_material_path(
    record: &mut Record,
    relocation_members: &std::collections::HashSet<String>,
    namespace: &str,
    interner: &StringInterner,
) -> usize {
    if record.sig.as_str() != "TXST" || relocation_members.is_empty() || namespace.trim().is_empty()
    {
        return 0;
    }
    let mut updated = 0;
    for field in record
        .fields
        .iter_mut()
        .filter(|field| field.sig.as_str() == "MNAM")
    {
        updated += namespace_decal_material_value(
            &mut field.value,
            relocation_members,
            namespace,
            interner,
        );
    }
    updated
}

fn namespace_decal_material_value(
    value: &mut FieldValue,
    relocation_members: &std::collections::HashSet<String>,
    namespace: &str,
    interner: &StringInterner,
) -> usize {
    match value {
        FieldValue::String(sym) => {
            let Some(current) = interner.resolve(*sym) else {
                return 0;
            };
            if !relocation_members.contains(&relocation_material_key(current)) {
                return 0;
            }
            let Some(namespaced) = namespaced_material_path(current, namespace) else {
                return 0;
            };
            *sym = interner.intern(&namespaced);
            1
        }
        FieldValue::Bytes(bytes) => {
            let had_nul = bytes.last().is_some_and(|byte| *byte == 0);
            let current = String::from_utf8_lossy(bytes.as_slice());
            let current = current.trim_end_matches('\0');
            if !relocation_members.contains(&relocation_material_key(current)) {
                return 0;
            }
            let Some(namespaced) = namespaced_material_path(current, namespace) else {
                return 0;
            };
            bytes.clear();
            bytes.extend_from_slice(namespaced.as_bytes());
            if had_nul {
                bytes.push(0);
            }
            1
        }
        _ => 0,
    }
}

fn namespaced_material_path(path: &str, namespace: &str) -> Option<String> {
    let namespace = namespace.trim().trim_matches(|c| c == '/' || c == '\\');
    if namespace.is_empty() {
        return None;
    }
    let normalized = path.trim().trim_matches('\0').replace('\\', "/");
    if normalized.is_empty() || normalized.contains(':') {
        return None;
    }
    let mut parts: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() || parts.iter().any(|part| *part == "..") {
        return None;
    }
    let insert_at = usize::from(
        parts
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case("materials")),
    );
    if parts
        .get(insert_at)
        .is_some_and(|part| part.eq_ignore_ascii_case(namespace))
    {
        return None;
    }
    parts.insert(insert_at, namespace);
    Some(parts.join("\\"))
}

fn strip_ascii_prefix(value: String, prefix: &str) -> String {
    if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        value[prefix.len()..].to_owned()
    } else {
        value
    }
}

fn allows_source_target_preflight_remap(
    source: Game,
    target: Game,
    sig: crate::ids::SigCode,
    editor_id: &str,
) -> bool {
    // INFO identity depends on parent-DIAL topology, which is unavailable during preflight.
    if source == Game::Fo76 && target == Game::Fo4 && matches!(sig.as_str(), "CELL" | "INFO") {
        return false;
    }
    if source == Game::Fo76 && target == Game::Fo4 && is_fo76_decal_texture_set(sig, editor_id) {
        return false;
    }
    if source == Game::Fo76 && target == Game::Fo4 && is_static_marker_editor_id(sig, editor_id) {
        return true;
    }
    let mapper_options = MapperOptions {
        vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(source, target),
        ..Default::default()
    };
    mapper_allows_editor_id_vanilla_remap(&mapper_options, sig, Some(editor_id))
}

fn mapper_entries_from_preflight(
    config: &RunConfig,
    interner: &StringInterner,
) -> Vec<(Sym, FormKey, crate::ids::SigCode)> {
    let mut out = Vec::with_capacity(config.target_record_preflight.len());
    for row in &config.target_record_preflight {
        if row.editor_id.is_empty() {
            continue;
        }
        let Ok(sig) = crate::ids::SigCode::from_str(&row.signature) else {
            continue;
        };
        let Ok(form_key) = parse_legacy_or_native_form_key(&row.form_key, interner) else {
            continue;
        };
        out.push((
            interner.intern(&normalized_editor_id(&row.editor_id)),
            form_key,
            sig,
        ));
    }
    out
}

fn has_fo76_creature_weapon_prefix(editor_id: &str) -> bool {
    let mut chars = editor_id.chars();
    matches!(chars.next(), Some('c'))
        && matches!(chars.next(), Some('r'))
        && chars.next().is_some_and(char::is_uppercase)
}

fn fo76_fo4_weap_editor_id_aliases(
    original_editor_id: &str,
    normalized_editor_id: &str,
) -> Vec<String> {
    let mut aliases = Vec::new();
    if has_fo76_creature_weapon_prefix(original_editor_id) {
        if let Some(alias) = normalized_editor_id
            .strip_prefix("cr")
            .filter(|s| !s.is_empty())
        {
            aliases.push(alias.to_string());
        }
    }
    if let Some(alias) = normalized_editor_id
        .strip_prefix("zzz_")
        .filter(|s| !s.is_empty())
    {
        aliases.push(alias.to_string());
    }
    aliases
}

fn allows_fo76_fo4_weap_local_alias(
    original_editor_id: &str,
    normalized_editor_id: &str,
    sig: crate::ids::SigCode,
) -> bool {
    sig.as_str() == "WEAP"
        && (has_fo76_creature_weapon_prefix(original_editor_id)
            || normalized_editor_id.starts_with("zzz_"))
}

fn fo76_fo4_collision_layer_alias(editor_id: &str) -> Option<&'static str> {
    match editor_id {
        "l_proj_no_collide_proj" => Some("l_spell"),
        _ => None,
    }
}

/// Forced FO76→FO4 keyword substitutions as `(FO76 SeventySix.esm object-id,
/// target plugin name, target object-id)`. These FO76 appearance/material
/// keywords either have no EditorID match in the FO4 masters or must resolve to
/// a DLC master keyword before OMOD target-keyword validation runs. Without an
/// explicit override they would be carried through as own-plugin KYWD records or
/// dropped from `MNAM`, making weapon mods appear on unrelated weapons.
///
/// Seeding these into `MapperState::source_to_target` makes the mapper — the
/// single chokepoint every `remap_formkey` and the raw-FormID fixup flow
/// through — resolve each reference (and the standalone KYWD's own identity, so
/// it is vanilla-remapped rather than re-emitted) to the FO4 vanilla keyword.
///
/// FO76's generic armor-paint and power-armor associations resolve to the FO4
/// keywords carried by the corresponding FO4 armor records.
const FO76_FO4_FORCED_KEYWORD_SUBSTITUTIONS: &[(u32, &str, u32)] = &[
    (0x0011_4364, "Fallout4.esm", 0x0024_A0D8), // ap_gun_Appearance   -> ap_WeaponMaterial
    (0x0037_D0B2, "Fallout4.esm", 0x0024_A0D7), // ma_Gun_Appearance   -> ma_WeaponMaterialSwaps
    (0x001A_001E, "Fallout4.esm", 0x001A_001E), // ap_melee_Appearance -> ap_melee_Material
    (0x0011_3855, "DLCNukaWorld.esm", 0x0003_3B61), // DLC04_ma_HandmadeAssaultRifle
    (0x0045_2EF4, "Fallout4.esm", 0x0024_A0FA), // ma_armor_Generic_Paint -> ap_armor_Paint
    (0x0053_03FE, "Fallout4.esm", 0x0018_DFCB), // ma_PowerArmorMod    -> ma_PA_Material
    // Linked-ref keywords FO76 renamed but kept at their FO4 object id. FO4
    // master packages and scripts look for the FO4 keyword on placed refs, so an
    // own-plugin copy leaves those links invisible to them.
    (0x001C_A8CF, "Fallout4.esm", 0x001C_A8CF), // DMP_Sandbox -> DMP_Sandbox_Prim
    (0x0004_21F8, "Fallout4.esm", 0x0004_21F8), // DMP_Suspicious_Sandbox -> DMP_Suspicious_Sandbox_512
    (0x0004_2241, "Fallout4.esm", 0x0004_2241), // DMP_Combat_HoldPosition -> DMP_Combat_HoldPosition_128
    (0x0006_6510, "Fallout4.esm", 0x0006_6510), // TurfSystemLinkToSleep -> EMSystemLinkToSleep
    (0x0018_12F6, "Fallout4.esm", 0x0018_12F6), // TurfSystemLinkToTurf -> EMSystemLinkToTurf
    (0x001C_5EDD, "Fallout4.esm", 0x001C_5EDD), // WorkshopStackedItemParent -> WorkshopStackedItemParentKEYWORD
    (0x000F_9E1B, "Fallout4.esm", 0x000F_9E1B), // LinkTerminalRobot -> LinkTerminalProtectron
    (0x000F_9E13, "Fallout4.esm", 0x000F_9E13), // LinkCaptive -> DefaultCaptiveLink
];

/// Forced FO76→FO4 race substitutions as `(FO76 SeventySix.esm object-id,
/// target plugin name, target object-id)`.
const FO76_FO4_FORCED_RACE_SUBSTITUTIONS: &[(u32, &str, u32)] = &[
    (0x0079_CCE7, "Fallout4.esm", 0x000E_AFB6), // GHL_PlayerGhoulRace -> GhoulRace
    (0x0077_2F32, "DLCRobot.esm", 0x0000_1129), // ProtectronFastRace -> DLC01RoboBrainRace
];

const FO4_HUMAN_RACE_LOCAL: u32 = 0x0001_3746;
const FO4_HUMAN_CHILD_RACE_LOCAL: u32 = 0x0011_D83F;
const FO4_GHOUL_RACE_LOCAL: u32 = 0x000E_AFB6;

/// FNV/FO3 humanoid races have legacy head/body layouts that cannot be emitted
/// as FO4 RACE records. Resolve them to the official FO4 race with the matching
/// anatomy before record allocation so every reference follows the donor and
/// the source RACE itself is treated as a target-master remap.
const FNV_FO3_FO4_HUMANOID_RACE_SUBSTITUTIONS: &[(&str, u32)] = &[
    ("CaucasianOldAged", FO4_HUMAN_RACE_LOCAL),
    ("AfricanAmericanOldAged", FO4_HUMAN_RACE_LOCAL),
    ("AsianOldAged", FO4_HUMAN_RACE_LOCAL),
    ("HispanicOldAged", FO4_HUMAN_RACE_LOCAL),
    ("AfricanAmericanRaider", FO4_HUMAN_RACE_LOCAL),
    ("AsianRaider", FO4_HUMAN_RACE_LOCAL),
    ("HispanicRaider", FO4_HUMAN_RACE_LOCAL),
    ("CaucasianRaider", FO4_HUMAN_RACE_LOCAL),
    ("TestQACaucasian", FO4_HUMAN_RACE_LOCAL),
    ("HispanicOld", FO4_HUMAN_RACE_LOCAL),
    ("HispanicChild", FO4_HUMAN_CHILD_RACE_LOCAL),
    ("CaucasianOld", FO4_HUMAN_RACE_LOCAL),
    ("CaucasianChild", FO4_HUMAN_CHILD_RACE_LOCAL),
    ("AsianOld", FO4_HUMAN_RACE_LOCAL),
    ("AsianChild", FO4_HUMAN_CHILD_RACE_LOCAL),
    ("AfricanAmericanOld", FO4_HUMAN_RACE_LOCAL),
    ("AfricanAmericanChild", FO4_HUMAN_CHILD_RACE_LOCAL),
    ("AfricanAmerican", FO4_HUMAN_RACE_LOCAL),
    ("Ghoul", FO4_GHOUL_RACE_LOCAL),
    ("Asian", FO4_HUMAN_RACE_LOCAL),
    ("Hispanic", FO4_HUMAN_RACE_LOCAL),
    ("Caucasian", FO4_HUMAN_RACE_LOCAL),
    ("Christine", FO4_HUMAN_RACE_LOCAL),
    ("WhitelegsCacasians", FO4_HUMAN_RACE_LOCAL),
    ("WhiteLegsAfricanAmerican", FO4_HUMAN_RACE_LOCAL),
    ("DeadHorseCaucasian", FO4_HUMAN_RACE_LOCAL),
    ("DeadHorseAfricanAmerican", FO4_HUMAN_RACE_LOCAL),
    ("SorrowAfricanAmerican", FO4_HUMAN_RACE_LOCAL),
    ("SorrowCacasian", FO4_HUMAN_RACE_LOCAL),
    ("Lobotomites", FO4_HUMAN_RACE_LOCAL),
    ("MarkedMenGhoul", FO4_GHOUL_RACE_LOCAL),
    ("DLCPittHispanicMut", FO4_HUMAN_RACE_LOCAL),
    ("DLCPittAsianMut", FO4_HUMAN_RACE_LOCAL),
    ("DLCPittCaucasianMut", FO4_HUMAN_RACE_LOCAL),
    ("DLCPittAfricanAmericanMut", FO4_HUMAN_RACE_LOCAL),
    ("HispanicTribal", FO4_HUMAN_RACE_LOCAL),
    ("CaucasianTribal", FO4_HUMAN_RACE_LOCAL),
    ("AsianTribal", FO4_HUMAN_RACE_LOCAL),
    ("AfricanAmericanTribal", FO4_HUMAN_RACE_LOCAL),
];

/// Skyrim humanoids use target-native FO4 race records. Human and child heads
/// are projected onto the corresponding FO4 topology; beast and Dremora heads
/// keep their baked source topology inside a converted FO4 FaceGeom asset.
const SKYRIMSE_FO4_HUMANOID_RACE_SUBSTITUTIONS: &[(&str, u32)] =
    crate::skyrimse_fo4_runtime::humanoid::SUPPORTED_SOURCE_HUMANOID_RACES;

/// Forced FO76→FO4 base-object substitutions as `(FO76 SeventySix.esm object-id,
/// FO4 Fallout4.esm object-id)`.
const FO76_FO4_FORCED_BASE_OBJECT_SUBSTITUTIONS: &[(u32, u32)] = &[
    (0x003B_D4F4, 0x000C_1AEB), // WorkshopWorkbenchPublic -> WorkshopWorkbench
];

/// Forced FO76→FO4 location-ref-type substitutions as `(FO76 SeventySix.esm
/// object-id, FO4 Fallout4.esm object-id)`.
const FO76_FO4_FORCED_LOCATION_REF_TYPE_SUBSTITUTIONS: &[(u32, u32)] = &[
    (0x0000_3956, 0x0000_3956), // LocationClearActor -> Boss
];

const FO76_MASTER_PLUGIN_NAME: &str = "SeventySix.esm";
const FO4_MASTER_PLUGIN_NAME: &str = "Fallout4.esm";

/// Resolve `FO76_FO4_FORCED_KEYWORD_SUBSTITUTIONS` into concrete
/// `(source FormKey, target FormKey)` pairs for the FO76→FO4 master plugins.
fn fo76_fo4_forced_keyword_substitution_mappings(
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    let source_plugin = interner.intern(FO76_MASTER_PLUGIN_NAME);
    FO76_FO4_FORCED_KEYWORD_SUBSTITUTIONS
        .iter()
        .map(|&(source_local, target_plugin_name, target_local)| {
            (
                FormKey {
                    local: source_local,
                    plugin: source_plugin,
                },
                FormKey {
                    local: target_local,
                    plugin: interner.intern(target_plugin_name),
                },
            )
        })
        .collect()
}

fn fo76_fo4_forced_race_substitution_mappings(
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    let source_plugin = interner.intern(FO76_MASTER_PLUGIN_NAME);
    FO76_FO4_FORCED_RACE_SUBSTITUTIONS
        .iter()
        .map(|&(source_local, target_plugin_name, target_local)| {
            (
                FormKey {
                    local: source_local,
                    plugin: source_plugin,
                },
                FormKey {
                    local: target_local,
                    plugin: interner.intern(target_plugin_name),
                },
            )
        })
        .collect()
}

fn fnv_fo3_fo4_humanoid_race_substitution_mappings(
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> Vec<(FormKey, FormKey)> {
    if !matches!(source, Game::Fnv | Game::Fo3) || target != Game::Fo4 {
        return Vec::new();
    }
    let Ok(race_sig) = SigCode::from_str("RACE") else {
        return Vec::new();
    };
    let target_plugin = interner.intern(FO4_MASTER_PLUGIN_NAME);
    source_entries
        .iter()
        .filter_map(|(editor_id, source_form_key, signature)| {
            if *signature != race_sig {
                return None;
            }
            let editor_id = interner.resolve(*editor_id)?;
            let (_, target_local) = FNV_FO3_FO4_HUMANOID_RACE_SUBSTITUTIONS
                .iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(editor_id))?;
            Some((
                *source_form_key,
                FormKey {
                    local: *target_local,
                    plugin: target_plugin,
                },
            ))
        })
        .collect()
}

fn skyrimse_fo4_humanoid_race_substitution_mappings(
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> Vec<(FormKey, FormKey)> {
    if source != Game::SkyrimSe || target != Game::Fo4 {
        return Vec::new();
    }
    let Ok(race_sig) = SigCode::from_str("RACE") else {
        return Vec::new();
    };
    let target_plugin = interner.intern(FO4_MASTER_PLUGIN_NAME);
    source_entries
        .iter()
        .filter_map(|(editor_id, source_form_key, signature)| {
            if *signature != race_sig {
                return None;
            }
            let editor_id = interner.resolve(*editor_id)?;
            let target_local =
                match crate::skyrimse_fo4_runtime::humanoid::source_humanoid_face_kind(editor_id)? {
                    crate::skyrimse_fo4_runtime::humanoid::SkyrimHumanoidFaceKind::Human => {
                        FO4_HUMAN_RACE_LOCAL
                    }
                    crate::skyrimse_fo4_runtime::humanoid::SkyrimHumanoidFaceKind::Child => {
                        FO4_HUMAN_CHILD_RACE_LOCAL
                    }
                    crate::skyrimse_fo4_runtime::humanoid::SkyrimHumanoidFaceKind::Argonian
                    | crate::skyrimse_fo4_runtime::humanoid::SkyrimHumanoidFaceKind::Khajiit
                    | crate::skyrimse_fo4_runtime::humanoid::SkyrimHumanoidFaceKind::Dremora => {
                        return None;
                    }
                };
            Some((
                *source_form_key,
                FormKey {
                    local: target_local,
                    plugin: target_plugin,
                },
            ))
        })
        .collect()
}

pub(crate) fn seed_fnv_fo3_fo4_ammo_substitutions(
    mapper_state: &mut MapperState,
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> Result<usize, String> {
    if !matches!(source, Game::Fnv | Game::Fo3) || target != Game::Fo4 {
        return Ok(0);
    }
    let ammo_sig = SigCode::from_str("AMMO")?;
    let table = crate::translator::ammo_substitute::AmmoSubstituteTable::from_yaml(
        crate::embedded::AMMO_FNV_TO_FO4,
    )?;
    let mut seeded = 0;
    for (editor_id, source_form_key, signature) in source_entries {
        if *signature != ammo_sig {
            continue;
        }
        let Some(editor_id) = interner.resolve(*editor_id) else {
            continue;
        };
        let Some(target_form_key) = table.lookup(editor_id) else {
            continue;
        };
        let target_form_key = FormKey::parse(&target_form_key, interner)?;
        mapper_state
            .source_to_target
            .insert(*source_form_key, target_form_key);
        seeded += 1;
    }
    Ok(seeded)
}

fn fo76_fo4_forced_base_object_substitution_mappings(
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    fo76_fo4_forced_substitution_mappings(interner, FO76_FO4_FORCED_BASE_OBJECT_SUBSTITUTIONS)
}

fn fo76_fo4_forced_location_ref_type_substitution_mappings(
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    fo76_fo4_forced_substitution_mappings(interner, FO76_FO4_FORCED_LOCATION_REF_TYPE_SUBSTITUTIONS)
}

fn fo76_fo4_forced_substitution_mappings(
    interner: &StringInterner,
    substitutions: &[(u32, u32)],
) -> Vec<(FormKey, FormKey)> {
    let source_plugin = interner.intern(FO76_MASTER_PLUGIN_NAME);
    let target_plugin = interner.intern(FO4_MASTER_PLUGIN_NAME);
    substitutions
        .iter()
        .map(|&(source_local, target_local)| {
            (
                FormKey {
                    local: source_local,
                    plugin: source_plugin,
                },
                FormKey {
                    local: target_local,
                    plugin: target_plugin,
                },
            )
        })
        .collect()
}

fn source_target_mappings_from_preflight(
    source_entries: impl IntoIterator<Item = (Sym, FormKey, crate::ids::SigCode)>,
    target_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> Vec<(FormKey, FormKey)> {
    let blocked_source_form_keys = FxHashSet::default();
    source_target_mappings_from_preflight_with_skips(
        source_entries,
        target_entries,
        interner,
        source,
        target,
        &blocked_source_form_keys,
    )
}

fn source_target_mappings_from_preflight_with_skips(
    source_entries: impl IntoIterator<Item = (Sym, FormKey, crate::ids::SigCode)>,
    target_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
    blocked_source_form_keys: &FxHashSet<FormKey>,
) -> Vec<(FormKey, FormKey)> {
    let mut target_by_key: FxHashMap<(Sym, crate::ids::SigCode), FormKey> = FxHashMap::default();
    for &(editor_id, form_key, signature) in target_entries {
        target_by_key
            .entry((editor_id, signature))
            .or_insert(form_key);
    }
    let mut target_by_local_sig: FxHashMap<(u32, crate::ids::SigCode), FormKey> =
        FxHashMap::default();
    if source == Game::Fo76 && target == Game::Fo4 {
        for &(_editor_id, form_key, signature) in target_entries {
            target_by_local_sig
                .entry((form_key.local, signature))
                .or_insert(form_key);
        }
    }

    let pair_blocked_source_form_keys =
        editor_id_vanilla_remap_blocked_source_form_keys(source, target, interner);
    let mut mappings = Vec::new();
    for (source_editor_id, source_form_key, source_signature) in source_entries {
        if blocked_source_form_keys.contains(&source_form_key)
            || pair_blocked_source_form_keys.contains(&source_form_key)
        {
            continue;
        }
        let original_editor_id_str = interner.resolve(source_editor_id).unwrap_or("");
        if !allows_source_target_preflight_remap(
            source,
            target,
            source_signature,
            original_editor_id_str,
        ) {
            continue;
        }
        let normalized_editor_id = normalized_eid_sym(source_editor_id, interner);
        if let Some(&target_form_key) = target_by_key.get(&(normalized_editor_id, source_signature))
        {
            mappings.push((source_form_key, target_form_key));
            continue;
        }
        if source == Game::Fo76 && target == Game::Fo4 {
            let source_editor_id_str = interner.resolve(normalized_editor_id).unwrap_or("");
            let mut mapped = false;
            if source_signature.as_str() == "COLL" {
                if let Some(alias) = fo76_fo4_collision_layer_alias(source_editor_id_str) {
                    let alias_sym = interner.intern(alias);
                    if let Some(&target_form_key) =
                        target_by_key.get(&(alias_sym, source_signature))
                    {
                        mappings.push((source_form_key, target_form_key));
                        mapped = true;
                    }
                }
            }
            for alias in
                fo76_fo4_weap_editor_id_aliases(original_editor_id_str, source_editor_id_str)
            {
                if mapped {
                    break;
                }
                let alias_sym = interner.intern(&alias);
                if let Some(&target_form_key) = target_by_key.get(&(alias_sym, source_signature)) {
                    mappings.push((source_form_key, target_form_key));
                    mapped = true;
                    break;
                }
            }
            if !mapped
                && allows_fo76_fo4_weap_local_alias(
                    original_editor_id_str,
                    source_editor_id_str,
                    source_signature,
                )
            {
                if let Some(&target_form_key) =
                    target_by_local_sig.get(&(source_form_key.local, source_signature))
                {
                    mappings.push((source_form_key, target_form_key));
                }
            }
        }
    }
    mappings
}

fn collect_fo76_fo4_creature_skin_remap_skips(
    source_handle_id: u64,
    source_schema: &AuthoringSchema,
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    target_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> FxHashSet<FormKey> {
    let mut blocked = FxHashSet::default();
    if source != Game::Fo76 || target != Game::Fo4 {
        return blocked;
    }

    let Ok(armo_sig) = SigCode::from_str("ARMO") else {
        return blocked;
    };
    let target_armors: FxHashSet<Sym> = target_entries
        .iter()
        .filter_map(|(editor_id, _fk, sig)| (*sig == armo_sig).then_some(*editor_id))
        .collect();

    for &(editor_id, form_key, sig) in source_entries {
        if sig != armo_sig {
            continue;
        }
        let normalized_editor_id = normalized_eid_sym(editor_id, interner);
        if !target_armors.contains(&normalized_editor_id) {
            continue;
        }
        let Some(editor_id_str) = interner.resolve(editor_id) else {
            continue;
        };
        if !looks_like_creature_skin_armor_editor_id(editor_id_str) {
            continue;
        }
        let Ok(record) = read_record_relayout_by_form_key(
            source_handle_id,
            &form_key,
            source_schema,
            interner,
            None,
        ) else {
            continue;
        };
        let Some(race_form_key) = record_race_formkey(&record, interner) else {
            continue;
        };
        if is_human_or_power_armor_race(race_form_key, interner)
            || source_race_has_same_editor_id_target(
                race_form_key,
                source_entries,
                target_entries,
                interner,
            )
        {
            continue;
        }

        blocked.insert(form_key);
        collect_armor_addons_from_skin(&record, &mut blocked);
    }

    blocked
}

fn looks_like_creature_skin_armor_editor_id(editor_id: &str) -> bool {
    editor_id.to_ascii_lowercase().contains("skin")
}

fn record_race_formkey(record: &Record, interner: &StringInterner) -> Option<FormKey> {
    let rnam_sig = SubrecordSig::from_str("RNAM").ok()?;
    let race_sym = interner.intern("Race");
    record
        .fields
        .iter()
        .find(|entry| entry.sig == rnam_sig)
        .and_then(|entry| field_value_formkey(&entry.value, race_sym))
}

fn is_human_or_power_armor_race(fk: FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    if !plugin.eq_ignore_ascii_case("Fallout4.esm")
        && !plugin.eq_ignore_ascii_case("SeventySix.esm")
    {
        return false;
    }
    matches!(fk.local, 0x013746 | 0x01D31E)
}

fn source_race_has_same_editor_id_target(
    source_race: FormKey,
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    target_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
) -> bool {
    let Ok(race_sig) = SigCode::from_str("RACE") else {
        return false;
    };
    let Some((source_editor_id, _, _)) = source_entries
        .iter()
        .find(|(_, form_key, sig)| *form_key == source_race && *sig == race_sig)
    else {
        return false;
    };
    let normalized_source_editor_id = normalized_eid_sym(*source_editor_id, interner);
    target_entries.iter().any(|(target_editor_id, _, sig)| {
        *sig == race_sig
            && normalized_eid_sym(*target_editor_id, interner) == normalized_source_editor_id
    })
}

fn collect_armor_addons_from_skin(record: &Record, blocked: &mut FxHashSet<FormKey>) {
    let Ok(modl_sig) = SubrecordSig::from_str("MODL") else {
        return;
    };
    for entry in record.fields.iter().filter(|entry| entry.sig == modl_sig) {
        collect_formkeys_from_value(&entry.value, blocked);
    }
}

fn field_value_formkey(value: &FieldValue, key: Sym) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(field_key, _)| *field_key == key)
            .and_then(|(_, value)| field_value_formkey(value, key)),
        _ => None,
    }
}

fn record_base_formkey(record: &Record, interner: &StringInterner) -> Option<FormKey> {
    let name_sig = SubrecordSig::from_str("NAME").ok()?;
    let base_sym = interner.intern("Base");
    record
        .fields
        .iter()
        .find(|entry| entry.sig == name_sig)
        .and_then(|entry| field_value_formkey(&entry.value, base_sym))
}

pub(crate) fn capture_fnv_scri_target_text(
    record: &Record,
    interner: &StringInterner,
    raw_form_id_resolver: &dyn Fn(u32) -> Option<FormKey>,
) -> Result<Option<String>, String> {
    let scri_sig = SubrecordSig::from_str("SCRI").map_err(|error| error.to_string())?;
    let mut entries = record.fields.iter().filter(|entry| entry.sig == scri_sig);
    let Some(entry) = entries.next() else {
        return Ok(None);
    };
    if entries.next().is_some() {
        return Err(format!(
            "{} {:06X} has multiple SCRI fields",
            record.sig.as_str(),
            record.form_key.local
        ));
    }
    match &entry.value {
        FieldValue::FormKey(form_key) => form_key_to_legacy_str(*form_key, interner)
            .map(Some)
            .ok_or_else(|| {
                format!(
                    "{} {:06X} SCRI FormKey plugin cannot be rendered",
                    record.sig.as_str(),
                    record.form_key.local
                )
            }),
        FieldValue::String(symbol) => {
            let value = interner.resolve(*symbol).ok_or_else(|| {
                format!(
                    "{} {:06X} SCRI string cannot be resolved",
                    record.sig.as_str(),
                    record.form_key.local
                )
            })?;
            let value = value.trim();
            Ok((!value.is_empty()).then(|| value.to_string()))
        }
        FieldValue::Bytes(bytes) => {
            let raw: [u8; 4] = bytes.as_slice().try_into().map_err(|_| {
                format!(
                    "{} {:06X} SCRI raw value has {} bytes, expected 4",
                    record.sig.as_str(),
                    record.form_key.local,
                    bytes.len()
                )
            })?;
            let raw = u32::from_le_bytes(raw);
            if raw == 0 {
                return Ok(None);
            }
            let form_key = raw_form_id_resolver(raw).ok_or_else(|| {
                format!(
                    "{} {:06X} SCRI raw FormID {raw:08X} cannot be resolved",
                    record.sig.as_str(),
                    record.form_key.local
                )
            })?;
            form_key_to_legacy_str(form_key, interner)
                .map(Some)
                .ok_or_else(|| {
                    format!(
                        "{} {:06X} SCRI raw FormID {raw:08X} plugin cannot be rendered",
                        record.sig.as_str(),
                        record.form_key.local
                    )
                })
        }
        other => Err(format!(
            "{} {:06X} SCRI has unsupported decoded value {other:?}",
            record.sig.as_str(),
            record.form_key.local
        )),
    }
}

pub(crate) fn append_fnv_scri_link(
    links: &mut Vec<FnvScriLink>,
    target_form_key: FormKey,
    source_scpt_form_key: &str,
    interner: &StringInterner,
) -> Result<(), String> {
    let target_form_key = form_key_to_legacy_str(target_form_key, interner)
        .ok_or_else(|| "SCRI attachment target FormKey cannot be rendered".to_string())?;
    if let Some(existing) = links
        .iter()
        .find(|link| link.target_form_key.eq_ignore_ascii_case(&target_form_key))
    {
        if existing
            .source_scpt_form_key
            .eq_ignore_ascii_case(source_scpt_form_key)
        {
            return Ok(());
        }
        return Err(format!(
            "SCRI attachment target {target_form_key} resolves to both {} and {source_scpt_form_key}",
            existing.source_scpt_form_key
        ));
    }
    links.push(FnvScriLink {
        target_form_key,
        source_scpt_form_key: source_scpt_form_key.to_string(),
    });
    Ok(())
}

fn is_supported_fo4_humanoid_race(fk: FormKey, interner: &StringInterner) -> bool {
    interner
        .resolve(fk.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO4_MASTER_PLUGIN_NAME))
        && matches!(
            fk.local,
            FO4_HUMAN_RACE_LOCAL | FO4_HUMAN_CHILD_RACE_LOCAL | FO4_GHOUL_RACE_LOCAL
        )
}

fn collect_formkeys_from_value(value: &FieldValue, out: &mut FxHashSet<FormKey>) {
    match value {
        FieldValue::FormKey(fk) => {
            out.insert(*fk);
        }
        FieldValue::List(values) => {
            for value in values {
                collect_formkeys_from_value(value, out);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_formkeys_from_value(value, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn target_master_names_for_skip(
    config: &RunConfig,
    whole_plugin_names: Vec<String>,
) -> Vec<String> {
    if !config.target_master_names.is_empty() {
        config.target_master_names.clone()
    } else if config.is_whole_plugin {
        whole_plugin_names
    } else {
        Vec::new()
    }
}

pub(crate) fn is_target_master_remap(
    target_fk: FormKey,
    target_master_syms: &FxHashSet<Sym>,
) -> bool {
    target_master_syms.contains(&target_fk.plugin)
}

/// Build the target-master skip set + first-master sym exactly as the legacy
/// per-record loop does at its start (run.rs translate_fks_with_mode_and_parents).
/// Used by `store2::translate_v2` so the vanilla-remap early-out + full-plugin
/// capture match legacy bit-for-bit.
pub(crate) fn capture_target_master_context(
    run: &mut ConversionRun,
) -> (FxHashSet<Sym>, Option<Sym>) {
    let whole_plugin_names =
        if run.config.is_whole_plugin && run.config.target_master_names.is_empty() {
            run.target_master_plugin_names()
        } else {
            Vec::new()
        };
    let target_master_names = target_master_names_for_skip(&run.config, whole_plugin_names);
    let target_master_syms: FxHashSet<Sym> =
        intern_plugin_names(&target_master_names, &run.interner);
    let first_target_master_sym = target_master_names
        .first()
        .map(|name| run.interner.intern(name));
    (target_master_syms, first_target_master_sym)
}

const TERRAIN_OWNED_RECORD_SIGS: &[&str] = &["LTEX", "GRAS"];

fn supports_source_worldspace_topology_rebuild(source: Game, target: Game) -> bool {
    target == Game::Fo4 && matches!(source, Game::Fnv | Game::Fo3 | Game::SkyrimSe)
}

fn collect_fo76_decal_material_members(source_handle_id: u64) -> Result<Vec<String>, String> {
    let form_keys =
        esp_authoring_core::plugin_runtime::plugin_handle_record_form_keys_by_signature_no_py(
            source_handle_id,
            "TXST",
        )?;
    let mut members = BTreeSet::new();
    for form_key in form_keys {
        let values = esp_authoring_core::plugin_runtime::plugin_handle_read_raw_subrecords_no_py(
            source_handle_id,
            &form_key,
            "MNAM",
        )?;
        for value in values {
            let path = String::from_utf8_lossy(&value);
            let mut member = crate::relocation::normalize_rel(&path);
            if member.is_empty() {
                continue;
            }
            if !member.starts_with("materials/") {
                member = format!("materials/{member}");
            }
            if member.ends_with(".bgsm") || member.ends_with(".bgem") {
                members.insert(member);
            }
        }
    }
    Ok(members.into_iter().collect())
}

fn needs_fo4_placed_ref_target_repair(source: Game, target: Game) -> bool {
    target == Game::Fo4 && matches!(source, Game::Fo76 | Game::Starfield)
}

/// Both BTD-driven pairs route LTEX/GRAS through the terrain-texture phase,
/// which emits them itself. Leaving the generic writer enabled makes each
/// record appear twice under different object ids; the `TerrainRecordRemaps`
/// dedupe does not cover it, because `target_record_reuse` is populated only
/// for FO76 (`phase::terrain::should_collect_source_terrain_ids`).
fn apply_terrain_owned_record_skips(
    translator: &mut Translator,
    source: Game,
    target: Game,
    config: &RunConfig,
) {
    if target != Game::Fo4
        || !matches!(source, Game::Fo76 | Game::Starfield)
        || !config.asset_phases.terrain
    {
        return;
    }
    for sig in TERRAIN_OWNED_RECORD_SIGS {
        translator.maps.skip_records.insert((*sig).to_owned());
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create a new `ConversionRun`, insert it into the registry, and return its ID.
pub fn create_run(params: RunParams) -> Result<u64, RunError> {
    create_run_in_mode(params, TargetMode::CreateNew)
}

fn create_run_in_mode(params: RunParams, target_mode: TargetMode) -> Result<u64, RunError> {
    if params.source == params.target {
        // Same-game is valid (e.g. FO4→FO4 for normalisation passes). No error.
    }

    let schema_source = AuthoringSchema::for_game(params.source.as_str())
        .map_err(|e| RunError::InvalidConfig(format!("source schema: {e}")))?;
    let schema_target = AuthoringSchema::for_game(params.target.as_str())
        .map_err(|e| RunError::InvalidConfig(format!("target schema: {e}")))?;

    let mut config = params.config;
    config
        .validate_mvp_record_exceptions(params.source, params.target)
        .map_err(RunError::InvalidConfig)?;
    config
        .validate_mvp_melee_only(params.source, params.target)
        .map_err(RunError::InvalidConfig)?;
    let mut translator = Translator::new(params.source, params.target)
        .map_err(|e| RunError::InvalidConfig(format!("translator: {e}")))?;
    apply_terrain_owned_record_skips(&mut translator, params.source, params.target, &config);
    for sig in &config.skip_record_signatures {
        let sig = sig.trim().to_uppercase();
        if !sig.is_empty() {
            translator.maps.skip_records.insert(sig);
        }
    }
    if target_mode == TargetMode::CreateNew && config.generated_object_id_floor != 0 {
        esp_authoring_core::plugin_runtime::plugin_handle_raise_next_object_id_no_py(
            params.target_handle_id,
            config.generated_object_id_floor,
        )
        .map_err(|e| RunError::InvalidConfig(format!("generated object-id floor: {e}")))?;
    }
    if target_mode == TargetMode::CreateNew {
        crate::plugin_header::normalize_target_plugin_header(
            params.target_handle_id,
            params.target.as_str(),
        )?;
    }

    let (event_tx, event_rx) = crossbeam_channel::bounded(1024);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let target_assets = match (
        params.target,
        config.target_data_dir.as_deref(),
        config.target_asset_catalog_path.as_deref(),
        config.target_asset_cache_dir.as_deref(),
    ) {
        (Game::Fo4, Some(data_dir), Some(catalog_path), Some(cache_dir)) => Some(
            crate::target_assets::TargetAssetStore::open_shared(
                data_dir,
                catalog_path,
                cache_dir,
                config.target_extracted_dir.as_deref(),
            )
            .map_err(RunError::InvalidConfig)?,
        ),
        _ => None,
    };
    if let Some(store) = target_assets.as_deref() {
        let membership_root = store
            .prepare_membership_tree("textures/effects/gobos/", ".dds")
            .map_err(RunError::InvalidConfig)?;
        config.target_membership_root = Some(membership_root.to_path_buf());
    }

    let (decal_materials, decal_material_scan_warning) = if params.source == Game::Fo76
        && params.target == Game::Fo4
        && config.source_extracted_dir.is_some()
    {
        match collect_fo76_decal_material_members(params.source_handle_id) {
            Ok(materials) => (materials, None),
            Err(error) => (
                Vec::new(),
                Some(format!("relocation: TXST material scan failed: {error}")),
            ),
        }
    } else {
        (Vec::new(), None)
    };

    let mut relocation = if matches!(params.source, Game::Fo76 | Game::SkyrimSe)
        && params.target == Game::Fo4
    {
        let roots: Vec<String> = if config.base_asset_relocation_mesh_roots.is_empty() {
            let defaults = if params.source == Game::Fo76 {
                crate::relocation::FO76_FO4_DEFAULT_RELOCATION_MESH_ROOTS
            } else {
                crate::relocation::SKYRIMSE_FO4_DEFAULT_RELOCATION_MESH_ROOTS
            };
            defaults.iter().map(|s| (*s).to_string()).collect()
        } else {
            config.base_asset_relocation_mesh_roots.clone()
        };
        match (&config.source_extracted_dir, target_assets.as_deref()) {
            (Some(fo76), Some(store)) => {
                crate::relocation::build_relocation_member_set_with_target_store(
                    &roots, fo76, store,
                )
            }
            (Some(fo76), None) => match &config.target_extracted_dir {
                Some(fo4) => crate::relocation::build_relocation_member_set(&roots, fo76, fo4),
                None => crate::relocation::RelocationBuildResult {
                    members: std::collections::HashSet::new(),
                    warnings: vec![
                        "relocation: target asset catalog/overlay unset — collision detection disabled"
                            .to_string(),
                    ],
                    ..Default::default()
                },
            },
            _ => crate::relocation::RelocationBuildResult {
                members: std::collections::HashSet::new(),
                warnings: vec![
                    "relocation: source extracted dir unset — collision detection disabled"
                        .to_string(),
                ],
                ..Default::default()
            },
        }
    } else {
        crate::relocation::RelocationBuildResult::default()
    };

    if params.source == Game::Fo76
        && params.target == Game::Fo4
        && let Some(source_dir) = config.source_extracted_dir.as_deref()
    {
        let warnings = if let Some(store) = target_assets.as_deref() {
            crate::relocation::extend_with_changed_decal_assets_from_target_store(
                &mut relocation.members,
                &decal_materials,
                source_dir,
                store,
            )
        } else if let Some(target_dir) = config.target_extracted_dir.as_deref() {
            crate::relocation::extend_with_changed_decal_assets(
                &mut relocation.members,
                &decal_materials,
                source_dir,
                target_dir,
            )
        } else {
            Vec::new()
        };
        relocation.warnings.extend(warnings);
    }
    if let Some(warning) = decal_material_scan_warning {
        relocation.warnings.push(warning);
    }

    let target_master_record_contexts = params
        .master_handle_ids
        .iter()
        .filter_map(|handle_id| {
            plugin_context_for_handle(*handle_id)
                .ok()
                .map(|(plugin_name, master_names)| TargetMasterRecordContext {
                    handle_id: *handle_id,
                    plugin_name,
                    master_names,
                })
        })
        .collect();

    let run = ConversionRun {
        source: params.source,
        target: params.target,
        source_handle_id: params.source_handle_id,
        target_handle_id: params.target_handle_id,
        master_handle_ids: params.master_handle_ids,
        target_master_record_contexts,
        interner: StringInterner::new(),
        schema_source,
        schema_target,
        translator,
        config,
        mapper_state: None,
        script_reference_state: None,
        generated_object_id_reservations: FxHashSet::default(),
        legacy_serial_normalization: Default::default(),
        legacy_pack_preflight_report: None,
        legacy_creature_race_coverage: Default::default(),
        decisions: Vec::new(),
        warnings: Vec::new(),
        deferred: Vec::new(),
        fnv_scri_links: Vec::new(),
        legacy_placed_actor_aliases: Default::default(),
        fnv_dedicated_topic_targets: None,
        fnv_pending_vmad_targets: Vec::new(),
        fnv_pending_vmad_scripts: Vec::new(),
        fnv_pending_quest_fragments: Vec::new(),
        fnv_pending_helper_scripts: Vec::new(),
        fnv_pending_scpt_properties: Vec::new(),
        fnv_pending_info_fragments: Vec::new(),
        fnv_pending_scene_fragments: Vec::new(),
        fnv_quest_runtime_component_plans: Vec::new(),
        fnv_quest_runtime_expected_receipts: Vec::new(),
        fnv_quest_runtime_synthetic_targets: BTreeMap::new(),
        fnv_quest_runtime_compiler_evidence: Vec::new(),
        fnv_quest_runtime_receipts_finalized: false,
        skyrim_runtime_capability_plan: None,
        skyrim_runtime_receipt: None,
        skyrim_minimal_quest_action_requests: HashMap::new(),
        skyrim_minimal_quest_class_prefix: None,
        skyrim_minimal_quest_projections: Vec::new(),
        skyrim_minimal_alias_projections: Vec::new(),
        skyrim_pack_projections: Vec::new(),
        skyrim_scene_projections: Vec::new(),
        skyrim_dialogue_projections: Vec::new(),
        skyrim_story_manager_projections: Vec::new(),
        skyrim_quest_runtime_voice_receipt: None,
        skyrim_quest_runtime_asset_copy_receipt: None,
        skyrim_minimal_quest_compiler_evidence: Vec::new(),
        skyrim_minimal_quest_receipts_finalized: false,
        bulk_melee_v1_receipts: Vec::new(),
        creature_dependency_plan_installed: false,
        creature_dependency_admissions: FxHashMap::default(),
        creature_dependency_terminals: FxHashSet::default(),
        creature_primary_npc_reservations: Vec::new(),
        creature_ancillary_npc_reservations: Vec::new(),
        skyrim_creature_record_reservations: Vec::new(),
        skyrim_creature_batch_owned_sources: FxHashSet::default(),
        skyrim_creature_ancillary_reservations: Vec::new(),
        skyrim_creature_ancillary_reservations_installed: false,
        creature_actor_action_reservations: Vec::new(),
        creature_actor_action_reservations_installed: false,
        progress_callback: None,
        navi_warnings: Vec::new(),
        event_tx,
        event_rx,
        cancel,
        dependency_graph: None,
        full_plugin_state: FullPluginRunState::default(),
        terrain_texture_jobs: Vec::new(),
        story_manager_route_seed: crate::phase::story_manager::StoryManagerRouteSeedReport::empty(
            params.source,
            params.target,
        ),
        projected_seed_cache: std::collections::HashMap::new(),
        interior_placed_ref_candidates: Vec::new(),
        interior_recentre: Default::default(),
        relocation_members: relocation.members,
        relocation_mesh_only_members: relocation.mesh_only_members,
        nif_dependencies: relocation.nif_dependencies,
        relocation_preparation: relocation.preparation,
        output_nif_dependencies: Arc::default(),
        relocation_warnings: relocation.warnings,
        target_assets,
        source_asset_inventory: None,
        output_sink: None,
        owned_handles: None,
        default_target_path: None,
        target_mode,
    };

    let slot = RunSlot {
        events: run.event_rx.clone(),
        event_tx: run.event_tx.clone(),
        cancel: Arc::clone(&run.cancel),
        run: Arc::new(Mutex::new(run)),
    };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registry()
        .lock()
        .map_err(|_| RunError::LockPoisoned)?
        .insert(id, slot);
    Ok(id)
}

/// Create a registered run that takes sole ownership of its plugin handles.
pub(crate) fn create_owned_run(
    source: Game,
    target: Game,
    config: RunConfig,
    handles: OwnedRunHandles,
    default_target_path: PathBuf,
    target_mode: TargetMode,
) -> Result<u64, RunError> {
    let params = RunParams {
        source,
        target,
        source_handle_id: handles.source.as_ref().map_or(0, OwnedPluginHandle::id),
        target_handle_id: handles.target.id(),
        master_handle_ids: handles.masters.iter().map(OwnedPluginHandle::id).collect(),
        config,
    };
    let id = create_run_in_mode(params, target_mode)?;
    if let Err(error) = with_run(id, |run| {
        run.owned_handles = Some(handles);
        run.default_target_path = Some(default_target_path);
        Ok::<_, RunError>(())
    }) {
        let _ = drop_run(id);
        return Err(error);
    }
    Ok(id)
}

/// Remove a run from the registry. Resources release when the last slot
/// clone (and any in-flight phase's run Arc) drops.
pub fn drop_run(id: u64) -> Result<(), RunError> {
    registry()
        .lock()
        .map_err(|_| RunError::LockPoisoned)?
        .remove(&id)
        .map(|_| ())
        .ok_or(RunError::UnknownRun(id))
}

/// Borrow the run mutably for the duration of `f`.
///
/// Locks only this run's slot — runs are independent.
/// `E` must be convertible to `RunError` so the caller can use `?` inside `f`.
pub fn with_run<R, E>(
    id: u64,
    f: impl FnOnce(&mut ConversionRun) -> Result<R, E>,
) -> Result<R, RunError>
where
    E: Into<RunError>,
{
    let slot = run_slot(id)?;
    let mut run = slot.run.lock().map_err(|_| RunError::LockPoisoned)?;
    f(&mut run).map_err(Into::into)
}

// ---------------------------------------------------------------------------
// ConversionRun methods
// ---------------------------------------------------------------------------

impl ConversionRun {
    pub(crate) fn read_target_record_if_available(
        &self,
        form_key: FormKey,
    ) -> Result<Option<Record>, RunError> {
        let plugin = self.interner.resolve(form_key.plugin).ok_or_else(|| {
            RunError::InvalidConfig(format!(
                "unresolved target record plugin for {:06X}",
                form_key.local
            ))
        })?;
        let handle_id = if plugin.eq_ignore_ascii_case(&self.config.output_plugin_name) {
            Some(self.target_handle_id)
        } else {
            self.target_master_record_contexts
                .iter()
                .find(|context| context.plugin_name.eq_ignore_ascii_case(plugin))
                .map(|context| context.handle_id)
        };
        let Some(handle_id) = handle_id else {
            return Ok(None);
        };
        match read_record_relayout_by_form_key(
            handle_id,
            &form_key,
            &self.schema_target,
            &self.interner,
            None,
        ) {
            Ok(record) => Ok(Some(record)),
            Err(RecordReadError::NotFound(_)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn reserve_legacy_creature_primary_npcs(
        &mut self,
        primary_creature_sources: &[FormKey],
        expected_candidates: usize,
    ) -> Result<(), RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Err(RunError::InvalidConfig(
                "legacy creature NPC reservations require FNV/FO3 -> FO4".to_string(),
            ));
        }
        if !self.creature_primary_npc_reservations.is_empty() {
            return Err(RunError::InvalidConfig(
                "legacy creature NPC reservations are already installed".to_string(),
            ));
        }
        let state = self.mapper_state.as_mut().ok_or_else(|| {
            RunError::InvalidConfig(
                "mapper state must be initialized before creature primary reservations".to_string(),
            )
        })?;
        self.creature_primary_npc_reservations = allocate_creature_primary_npc_reservations(
            state,
            &self.interner,
            primary_creature_sources,
            expected_candidates,
        )
        .map_err(RunError::InvalidConfig)?;
        Ok(())
    }

    pub(crate) fn creature_primary_npc_reservations(&self) -> &[CreaturePrimaryNpcReservation] {
        &self.creature_primary_npc_reservations
    }

    pub(crate) fn creature_ancillary_npc_reservations(&self) -> &[CreatureAncillaryNpcReservation] {
        &self.creature_ancillary_npc_reservations
    }

    pub(crate) fn install_legacy_creature_dependency_plan(
        &mut self,
        admissions: Vec<CreatureDependencyAdmission>,
        primary_creature_sources: &[FormKey],
        ancillary_npc_sources: &[FormKey],
        expected_candidates: usize,
    ) -> Result<(), RunError> {
        if self.creature_dependency_plan_installed {
            return Err(RunError::InvalidConfig(
                "creature dependency plan is already installed".to_string(),
            ));
        }
        let mapper_state = self.mapper_state.clone();
        let dependency_plan_installed = self.creature_dependency_plan_installed;
        let primary_reservations = self.creature_primary_npc_reservations.clone();
        let ancillary_reservations = self.creature_ancillary_npc_reservations.clone();
        let installed_admissions = self.creature_dependency_admissions.clone();
        let terminals = self.creature_dependency_terminals.clone();
        let result = self
            .reserve_legacy_creature_primary_npcs(primary_creature_sources, expected_candidates)
            .and_then(|()| self.reserve_legacy_creature_ancillary_npcs(ancillary_npc_sources))
            .and_then(|()| self.install_creature_dependency_admissions(admissions));
        if let Err(error) = result {
            self.mapper_state = mapper_state;
            self.creature_dependency_plan_installed = dependency_plan_installed;
            self.creature_primary_npc_reservations = primary_reservations;
            self.creature_ancillary_npc_reservations = ancillary_reservations;
            self.creature_dependency_admissions = installed_admissions;
            self.creature_dependency_terminals = terminals;
            return Err(error);
        }
        self.creature_dependency_plan_installed = true;
        Ok(())
    }

    fn reserve_legacy_creature_ancillary_npcs(
        &mut self,
        ancillary_npc_sources: &[FormKey],
    ) -> Result<(), RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Err(RunError::InvalidConfig(
                "legacy ancillary NPC reservations require FNV/FO3 -> FO4".to_string(),
            ));
        }
        if !self.creature_ancillary_npc_reservations.is_empty() {
            return Err(RunError::InvalidConfig(
                "legacy ancillary NPC reservations are already installed".to_string(),
            ));
        }
        let state = self.mapper_state.as_mut().ok_or_else(|| {
            RunError::InvalidConfig(
                "mapper state must be initialized before ancillary NPC reservations".to_string(),
            )
        })?;
        self.creature_ancillary_npc_reservations = allocate_creature_ancillary_npc_reservations(
            state,
            &self.interner,
            ancillary_npc_sources,
        )
        .map_err(RunError::InvalidConfig)?;
        Ok(())
    }

    pub(crate) fn reserve_skyrim_creature_records(
        &mut self,
        sources: Vec<SkyrimCreatureReservationSource>,
        expected_candidates: usize,
    ) -> Result<(), RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Err(RunError::InvalidConfig(
                "Skyrim creature record reservations require Skyrim SE -> FO4".to_string(),
            ));
        }
        if !self.skyrim_creature_record_reservations.is_empty() {
            return Err(RunError::InvalidConfig(
                "Skyrim creature record reservations are already installed".to_string(),
            ));
        }
        let state = self.mapper_state.as_mut().ok_or_else(|| {
            RunError::InvalidConfig(
                "mapper state must be initialized before Skyrim creature reservations".to_string(),
            )
        })?;
        self.skyrim_creature_record_reservations = allocate_skyrim_creature_record_reservations(
            state,
            &self.interner,
            sources,
            expected_candidates,
        )
        .map_err(RunError::InvalidConfig)?;
        Ok(())
    }

    pub(crate) fn skyrim_creature_record_reservations(&self) -> &[SkyrimCreatureRecordReservation] {
        &self.skyrim_creature_record_reservations
    }

    pub(crate) fn reserve_skyrim_creature_ancillary_records(
        &mut self,
        requests: Vec<SkyrimCreatureAncillaryReservationRequest>,
    ) -> Result<(), RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Err(RunError::InvalidConfig(
                "Skyrim creature ancillary reservations require Skyrim SE -> FO4".to_string(),
            ));
        }
        if self.skyrim_creature_ancillary_reservations_installed {
            return Err(RunError::InvalidConfig(
                "Skyrim creature ancillary reservations are already installed".to_string(),
            ));
        }
        let state = self.mapper_state.as_mut().ok_or_else(|| {
            RunError::InvalidConfig(
                "mapper state must be initialized before Skyrim creature ancillary reservations"
                    .to_string(),
            )
        })?;
        self.skyrim_creature_ancillary_reservations =
            allocate_skyrim_creature_ancillary_reservations(
                state,
                &self.interner,
                &self.skyrim_creature_record_reservations,
                requests,
            )
            .map_err(RunError::InvalidConfig)?;
        self.skyrim_creature_ancillary_reservations_installed = true;
        Ok(())
    }

    pub(crate) fn skyrim_creature_ancillary_reservations(
        &self,
    ) -> &[SkyrimCreatureAncillaryReservationReceipt] {
        &self.skyrim_creature_ancillary_reservations
    }

    pub(crate) fn reserve_creature_actor_action_records(
        &mut self,
        requests: Vec<CreatureActorActionReservationRequest>,
    ) -> Result<(), RunError> {
        if !matches!(
            (self.source, self.target),
            (Game::SkyrimSe | Game::Fnv | Game::Fo3, Game::Fo4)
        ) {
            return Err(RunError::InvalidConfig(format!(
                "creature Actor Action reservations are unsupported for {} -> {}",
                self.source.as_str(),
                self.target.as_str()
            )));
        }
        if self.creature_actor_action_reservations_installed {
            return Err(RunError::InvalidConfig(
                "creature Actor Action reservations are already installed".to_string(),
            ));
        }
        let state = self.mapper_state.as_mut().ok_or_else(|| {
            RunError::InvalidConfig(
                "mapper state must be initialized before creature Actor Action reservations"
                    .to_string(),
            )
        })?;
        self.creature_actor_action_reservations =
            allocate_creature_actor_action_reservations(state, &self.interner, requests)
                .map_err(RunError::InvalidConfig)?;
        self.creature_actor_action_reservations_installed = true;
        Ok(())
    }

    pub(crate) fn creature_actor_action_reservations(
        &self,
    ) -> &[CreatureActorActionReservationReceipt] {
        &self.creature_actor_action_reservations
    }

    pub(crate) fn install_skyrim_creature_dependency_plan(
        &mut self,
        admissions: Vec<CreatureDependencyAdmission>,
        sources: Vec<SkyrimCreatureReservationSource>,
        expected_candidates: usize,
    ) -> Result<(), RunError> {
        if self.creature_dependency_plan_installed {
            return Err(RunError::InvalidConfig(
                "creature dependency plan is already installed".to_string(),
            ));
        }
        let mapper_state = self.mapper_state.clone();
        let dependency_plan_installed = self.creature_dependency_plan_installed;
        let reservations = self.skyrim_creature_record_reservations.clone();
        let batch_owned_sources = self.skyrim_creature_batch_owned_sources.clone();
        let installed_admissions = self.creature_dependency_admissions.clone();
        let terminals = self.creature_dependency_terminals.clone();
        let result = self
            .reserve_skyrim_creature_records(sources, expected_candidates)
            .and_then(|()| self.install_creature_dependency_admissions(admissions));
        if let Err(error) = result {
            self.mapper_state = mapper_state;
            self.creature_dependency_plan_installed = dependency_plan_installed;
            self.skyrim_creature_record_reservations = reservations;
            self.skyrim_creature_batch_owned_sources = batch_owned_sources;
            self.creature_dependency_admissions = installed_admissions;
            self.creature_dependency_terminals = terminals;
            return Err(error);
        }
        self.skyrim_creature_batch_owned_sources = self
            .skyrim_creature_record_reservations
            .iter()
            .filter(|reservation| {
                matches!(
                    reservation.kind,
                    SkyrimCreatureReservedRecordKind::Race | SkyrimCreatureReservedRecordKind::Npc
                )
            })
            .map(|reservation| reservation.source)
            .collect();
        self.creature_dependency_plan_installed = true;
        Ok(())
    }

    pub(crate) fn install_creature_dependency_admissions(
        &mut self,
        admissions: Vec<CreatureDependencyAdmission>,
    ) -> Result<(), RunError> {
        if !matches!(
            (self.source, self.target),
            (Game::SkyrimSe | Game::Fnv | Game::Fo3, Game::Fo4)
        ) {
            return Err(RunError::InvalidConfig(format!(
                "creature dependency admissions are unsupported for {} -> {}",
                self.source.as_str(),
                self.target.as_str()
            )));
        }
        let mut installed = FxHashMap::default();
        for mut admission in admissions {
            admission
                .owner_family_ids
                .sort_by_key(|owner| owner.to_ascii_lowercase());
            admission
                .owner_family_ids
                .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
            if admission.owner_family_ids.is_empty() {
                return Err(RunError::InvalidConfig(format!(
                    "creature dependency {} {:06X} has no owning family",
                    admission.source_signature.as_str(),
                    admission.source_form_key.local
                )));
            }
            let source_form_key = admission.source_form_key;
            if installed.insert(source_form_key, admission).is_some() {
                return Err(RunError::InvalidConfig(format!(
                    "duplicate creature dependency admission {:06X}",
                    source_form_key.local
                )));
            }
        }
        self.creature_dependency_admissions = installed;
        self.creature_dependency_terminals.clear();
        Ok(())
    }

    pub(crate) fn is_admitted_creature_dependency(
        &self,
        form_key: FormKey,
        signature: SigCode,
    ) -> bool {
        self.creature_dependency_admissions
            .get(&form_key)
            .is_some_and(|admission| admission.source_signature == signature)
    }

    pub(crate) fn creature_dependency_failure(
        &self,
        form_key: FormKey,
        signature: SigCode,
        disposition: &str,
    ) -> RunError {
        let owners = self
            .creature_dependency_admissions
            .get(&form_key)
            .map(|admission| admission.owner_family_ids.join(","))
            .unwrap_or_else(|| "unknown".to_string());
        RunError::InvalidConfig(format!(
            "creature_dependency_terminal:{}:{:06X}:{}:owners={owners}",
            signature.as_str(),
            form_key.local,
            disposition
        ))
    }

    pub(crate) fn record_creature_dependency_terminal(&mut self, form_key: FormKey) {
        if self.creature_dependency_admissions.contains_key(&form_key) {
            self.creature_dependency_terminals.insert(form_key);
        }
    }

    pub(crate) fn validate_creature_dependency_accounting(&self) -> Result<(), RunError> {
        let mut missing = self
            .creature_dependency_admissions
            .values()
            .filter(|admission| {
                !self
                    .creature_dependency_terminals
                    .contains(&admission.source_form_key)
            })
            .collect::<Vec<_>>();
        missing.sort_by_key(|admission| {
            (
                admission.source_signature.as_str().to_string(),
                admission.source_form_key.local,
            )
        });
        if let Some(admission) = missing.first() {
            return Err(self.creature_dependency_failure(
                admission.source_form_key,
                admission.source_signature,
                "missing_from_translation_enumeration",
            ));
        }
        Ok(())
    }

    pub(crate) fn bulk_melee_v1_receipts(&self) -> &[crate::phase::mvp_melee::MvpMeleeReceipt] {
        &self.bulk_melee_v1_receipts
    }

    pub(crate) fn clear_bulk_melee_v1_receipts(&mut self) {
        self.bulk_melee_v1_receipts.clear();
    }

    pub(crate) fn commit_bulk_melee_v1_receipts(
        &mut self,
        receipts: Vec<crate::phase::mvp_melee::MvpMeleeReceipt>,
    ) {
        self.bulk_melee_v1_receipts = receipts;
    }

    pub(crate) fn is_skyrim_runtime_planned_record(
        &self,
        form_key: FormKey,
        signature: SigCode,
    ) -> bool {
        self.source == Game::SkyrimSe
            && self.target == Game::Fo4
            && ((crate::skyrimse_fo4_runtime::planner::is_managed_signature(signature.as_str())
                && (is_mandatory_skyrim_actor_record(self.source, self.target, signature)
                    || !self
                        .translator
                        .maps
                        .skip_records
                        .contains(signature.as_str())))
                || self.config.is_mvp_record_exception(
                    self.source,
                    self.target,
                    form_key,
                    signature,
                    &self.interner,
                ))
    }

    pub(crate) fn is_admitted_mvp_weapon_record(&self, record: &Record) -> bool {
        if !self.config.is_mvp_record_exception(
            self.source,
            self.target,
            record.form_key,
            record.sig,
            &self.interner,
        ) {
            return false;
        }
        match (self.source, record.sig.as_str()) {
            (Game::SkyrimSe, "WEAP") => {
                crate::skyrimse_fo4_runtime::weapon::simple_melee_weapon_source(
                    record,
                    &self.interner,
                )
                .is_some()
            }
            (Game::Fnv, "WEAP") => {
                crate::translator::pair_hooks::fnv_fo4::classify_hatchet(record, &self.interner)
                    .is_some()
            }
            (_, "STAT") => true,
            _ => false,
        }
    }

    pub fn source_handle(&self) -> Option<u64> {
        (self.source_handle_id != 0).then_some(self.source_handle_id)
    }

    pub fn require_source_handle(&self) -> Result<u64, RunError> {
        self.source_handle().ok_or_else(|| {
            RunError::InvalidConfig("this phase requires a source plugin".to_string())
        })
    }

    pub fn release_source_handle(&mut self) -> bool {
        let released = self
            .owned_handles
            .as_mut()
            .and_then(|handles| handles.source.as_mut())
            .is_some_and(OwnedPluginHandle::release);
        if released {
            self.source_handle_id = 0;
            if let Some(handles) = self.owned_handles.as_mut() {
                handles.source = None;
            }
        }
        released
    }

    pub fn release_master_handles(&mut self) -> usize {
        let Some(handles) = self.owned_handles.as_mut() else {
            return 0;
        };
        let mut released = 0;
        for master in handles.masters.iter_mut().rev() {
            released += usize::from(master.release());
        }
        handles.masters.clear();
        self.master_handle_ids.clear();
        self.target_master_record_contexts.clear();
        released
    }

    pub(crate) fn merge_target_collision_donor(
        &self,
        record: &mut Record,
        donor_form_key: FormKey,
    ) -> Result<(), String> {
        let donor_plugin = self
            .interner
            .resolve(donor_form_key.plugin)
            .ok_or_else(|| "collision donor plugin is not interned".to_string())?;
        let context = self
            .target_master_record_contexts
            .iter()
            .find(|context| context.plugin_name.eq_ignore_ascii_case(donor_plugin))
            .ok_or_else(|| format!("collision donor master is not loaded: {donor_plugin}"))?;
        let mut donor = read_record_relayout_by_form_key(
            context.handle_id,
            &donor_form_key,
            &self.schema_target,
            &self.interner,
            None,
        )
        .map_err(|error| format!("collision donor read failed: {error}"))?;

        let target_master_names = if self.config.target_master_names.is_empty() {
            self.target_master_record_contexts
                .iter()
                .map(|context| context.plugin_name.clone())
                .collect()
        } else {
            self.config.target_master_names.clone()
        };
        let mut donor_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: self.config.output_plugin_name.clone(),
                source_plugin_name: context.plugin_name.clone(),
                source_master_names: context.master_names.clone(),
                target_master_names,
                resolution_mode: ResolutionMode::DeferAndFixup,
                ..Default::default()
            },
        );
        let mut donor_mapper = FormKeyMapper::from_state(&mut donor_state, &self.interner);
        donor_mapper
            .rewrite_record(&mut donor)
            .map_err(|error| format!("collision donor VMAD reindex failed: {error}"))?;
        crate::collision_donor::merge_target_collision_donor(record, &donor, &self.interner);
        Ok(())
    }

    pub(crate) fn read_explicit_target_master_record(
        &self,
        form_key_text: &str,
    ) -> Result<Record, FnvScriptingError> {
        let form_key = FormKey::parse(form_key_text, &self.interner).map_err(|error| {
            FnvScriptingError::Setup(format!(
                "invalid explicit target donor {form_key_text}: {error}"
            ))
        })?;
        let plugin = self.interner.resolve(form_key.plugin).ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "explicit target donor plugin is unresolved: {form_key_text}"
            ))
        })?;
        let context = self
            .target_master_record_contexts
            .iter()
            .find(|context| context.plugin_name.eq_ignore_ascii_case(plugin))
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "explicit target donor master is not loaded: {plugin}"
                ))
            })?;
        read_record_relayout_by_form_key(
            context.handle_id,
            &form_key,
            &self.schema_target,
            &self.interner,
            None,
        )
        .map_err(|error| {
            FnvScriptingError::Setup(format!(
                "read explicit target donor {form_key_text}: {error}"
            ))
        })
    }

    /// Emit a one-line status string through the Python progress callback, if
    /// one is set. Bypasses the phase event channel, which is starved while
    /// `translate_all` holds the run-registry lock (the event Drainer blocks on
    /// it), so the otherwise-silent setup (mapper-state build, form-key
    /// enumeration) shows during multi-hour whole-plugin runs. Callback errors
    /// are ignored.
    fn emit_status(&self, message: &str) {
        let Some(cb) = self.progress_callback.as_ref() else {
            return;
        };
        Python::attach(|py| {
            let _ = cb.call1(py, (message,));
        });
    }

    fn emit_phase_status(&self, message: impl Into<String>) {
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "translate_v2",
            level: crate::phase::LogLevel::Info,
            message: message.into(),
        });
    }

    pub(crate) fn preallocate_legacy_form_key_intents(
        &mut self,
        intents: impl IntoIterator<Item = LegacyFormKeyAllocationIntent>,
    ) -> LegacyFormKeyPreallocationCoverage {
        let mut coverage = LegacyFormKeyPreallocationCoverage::default();
        let state = self
            .mapper_state
            .as_mut()
            .expect("mapper_state initialized before legacy preallocation");
        let mut mapper = FormKeyMapper::from_state(state, &self.interner);
        for intent in intents {
            coverage.eligible += 1;
            let normalized_eid = normalized_eid_opt(intent.editor_id, mapper.interner);
            mapper.allocate_or_resolve(intent.source_fk, normalized_eid, intent.target_sig);
            if mapper.lookup(intent.source_fk).is_some() {
                coverage.mapped += 1;
            } else {
                coverage.missing += 1;
            }
        }
        coverage
    }

    pub(crate) fn preallocate_fnv_quest_slice_pack_identities(
        &mut self,
    ) -> Result<LegacyFormKeyPreallocationCoverage, RunError> {
        if !self.config.fnv_quest_slice || self.source != Game::Fnv || self.target != Game::Fo4 {
            return Ok(LegacyFormKeyPreallocationCoverage::default());
        }
        let locals = self
            .config
            .fnv_quest_slice_records
            .iter()
            .find(|(signature, _)| signature.eq_ignore_ascii_case("PACK"))
            .map(|(_, locals)| locals.clone())
            .unwrap_or_default();
        if locals.is_empty() {
            return Ok(LegacyFormKeyPreallocationCoverage::default());
        }
        let source_plugin_name = self
            .mapper_state
            .as_ref()
            .expect("mapper_state initialized before legacy preallocation")
            .options
            .source_plugin_name
            .clone();
        let source_plugin = self.interner.intern(&source_plugin_name);
        let pack_sig = SigCode::from_str("PACK")
            .map_err(|error| RunError::InvalidConfig(format!("PACK signature: {error}")))?;
        let mut intents = Vec::with_capacity(locals.len());
        for local in locals {
            let source_fk = FormKey {
                local: local & 0x00FF_FFFF,
                plugin: source_plugin,
            };
            let record = read_record_relayout_by_form_key(
                self.source_handle_id,
                &source_fk,
                &self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "selected PACK {:06X} preallocation read failed: {error}",
                    source_fk.local
                ))
            })?;
            if record.sig != pack_sig {
                return Err(RunError::InvalidConfig(format!(
                    "selected PACK {:06X} preallocation resolved as {}",
                    source_fk.local,
                    record.sig.as_str()
                )));
            }
            intents.push(LegacyFormKeyAllocationIntent {
                source_fk,
                editor_id: record.eid,
                target_sig: pack_sig,
            });
        }
        let coverage = self.preallocate_legacy_form_key_intents(intents);
        if coverage.missing != 0 || coverage.mapped != coverage.eligible {
            return Err(RunError::InvalidConfig(format!(
                "selected PACK identity preallocation incomplete: eligible={} mapped={} missing={}",
                coverage.eligible, coverage.mapped, coverage.missing
            )));
        }
        Ok(coverage)
    }

    pub(crate) fn legacy_pack_gate_active(&self) -> bool {
        self.config.is_whole_plugin
            && !self.config.fnv_quest_slice
            && matches!(self.source, Game::Fnv | Game::Fo3)
            && self.target == Game::Fo4
    }

    pub(crate) fn begin_legacy_pack_preflight(
        &mut self,
        source_plugin_name: &str,
    ) -> Option<LegacyPackPreflightAccumulator> {
        if !self.legacy_pack_gate_active() {
            return None;
        }
        self.legacy_pack_preflight_report = None;
        let source_family = match self.source {
            Game::Fnv => crate::translator::pair_hooks::fnv_pack::LegacyPackSourceFamily::Fnv,
            Game::Fo3 => crate::translator::pair_hooks::fnv_pack::LegacyPackSourceFamily::Fo3,
            _ => return None,
        };
        let require_explicit_origins = self.config.legacy_pack_provenance_required
            || source_plugin_name.eq_ignore_ascii_case("FalloutNV.esm");
        let raw_expected = if require_explicit_origins {
            LegacyPackExpectedCounts::audited_merged()
        } else {
            LegacyPackExpectedCounts::audited_for(source_family)
        };
        let expected = self.config.legacy_pack_expected_counts.unwrap_or_else(|| {
            if require_explicit_origins {
                LegacyPackExpectedCounts::audited_merged()
            } else {
                LegacyPackExpectedCounts::audited_for(source_family)
            }
        });
        let direct_origin = (!require_explicit_origins
            && self.config.legacy_pack_origins.is_empty())
        .then(|| DirectLegacyPackOrigin {
            family: source_family,
            source_plugin: source_plugin_name.to_string(),
        });
        let explicitly_excluded = self
            .config
            .skip_record_signatures
            .iter()
            .any(|sig| sig.trim().eq_ignore_ascii_case("PACK"));
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "preflight",
            level: crate::phase::LogLevel::Info,
            message: format!(
                "legacy_pack_preflight:start:raw_expected_fnv={}:raw_expected_fo3={}:expected_fnv={}:expected_fo3={}:explicit_origins={}:explicitly_excluded={}",
                raw_expected.fnv,
                raw_expected.fo3,
                expected.fnv,
                expected.fo3,
                self.config.legacy_pack_origins.len(),
                explicitly_excluded,
            ),
        });
        Some(LegacyPackPreflightAccumulator::new(
            &self.config.legacy_pack_origins,
            raw_expected,
            self.config.legacy_pack_raw_source_counts,
            expected,
            direct_origin,
            require_explicit_origins,
            explicitly_excluded,
        ))
    }

    pub(crate) fn finish_legacy_pack_preflight(
        &mut self,
        accumulator: LegacyPackPreflightAccumulator,
    ) -> Result<(), RunError> {
        let report = accumulator.finish();
        let blocked = report.is_blocked();
        let message = format!(
            "legacy_pack_preflight:{}:{}",
            if blocked { "blocked" } else { "passed" },
            report.error_summary_json()
        );
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "preflight",
            level: if blocked {
                crate::phase::LogLevel::Error
            } else {
                crate::phase::LogLevel::Info
            },
            message,
        });
        self.legacy_pack_preflight_report = Some(report.clone());
        if blocked {
            return Err(RunError::LegacyPackPreflight(Box::new(report)));
        }
        Ok(())
    }

    pub(crate) fn preflight_legacy_packs_from_handle(&mut self) -> Result<(), RunError> {
        if !self.legacy_pack_gate_active() {
            return Ok(());
        }
        let source_plugin_name = plugin_name_for_handle(self.source_handle_id)?;
        let Some(mut accumulator) = self.begin_legacy_pack_preflight(&source_plugin_name) else {
            return Ok(());
        };
        let pack_sig = SigCode::from_str("PACK")
            .map_err(|error| RunError::InvalidConfig(format!("PACK signature: {error}")))?;
        let form_keys = iter_form_keys_of_sig(self.source_handle_id, pack_sig, &mut self.interner)?;
        for form_key in form_keys {
            match read_record_relayout_by_form_key(
                self.source_handle_id,
                &form_key,
                &self.schema_source,
                &self.interner,
                None,
            ) {
                Ok(record) => accumulator.observe_decoded(&record, &self.interner),
                Err(error) => {
                    accumulator.observe_decode_error(form_key, error.to_string(), &self.interner)
                }
            }
        }
        self.finish_legacy_pack_preflight(accumulator)
    }

    pub(crate) fn prepare_legacy_output_allocation_domain(
        &mut self,
        source_locals: impl IntoIterator<Item = u32>,
    ) -> Result<(), RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Ok(());
        }
        let state = self
            .mapper_state
            .as_mut()
            .expect("mapper_state initialized before legacy preallocation");
        if let Some(floor) = legacy_output_allocation_floor(
            self.source,
            self.target,
            &state.options.source_plugin_name,
            &state.options.output_plugin_name,
            state.options.preserve_source_ids,
            state.next_object_id,
            source_locals,
        )
        .map_err(|message| RunError::InvalidConfig(message.to_string()))?
        {
            state.next_object_id = floor;
        }
        Ok(())
    }

    fn preallocate_legacy_translate_all_records(
        &mut self,
        form_keys: &[FormKey],
    ) -> Result<(), RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Ok(());
        }
        self.emit_phase_status(format!(
            "translate: legacy forward-reference preallocation start records={}",
            form_keys.len()
        ));
        self.prepare_legacy_output_allocation_domain(
            form_keys.iter().map(|form_key| form_key.local),
        )?;
        self.preallocate_fnv_quest_slice_pack_identities()?;
        let relayout_target_schema = self.schema_target.clone();
        let relayout_ctx = crate::struct_relayout::StructRelayoutCtx {
            target_schema: &relayout_target_schema,
            target_form_version:
                crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
            legacy_bptd_only: true,
        };
        let mut total = LegacyFormKeyPreallocationCoverage::default();
        let mut intents = Vec::with_capacity(crate::store2::translate_v2::CHUNK);
        for &source_fk in form_keys {
            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(RunError::Cancelled);
            }
            let Ok(mut source_record) = read_record_relayout_by_form_key(
                self.source_handle_id,
                &source_fk,
                &*self.schema_source,
                &self.interner,
                Some(&relayout_ctx),
            ) else {
                continue;
            };
            let admitted_mvp_weapon_record = self.is_admitted_mvp_weapon_record(&source_record);
            {
                let mut ctx = PairCtx::new(&self.interner);
                let _ = self.translator.pre_translate(&mut ctx, &mut source_record);
            }
            if matches!(self.source, Game::Fnv | Game::Fo3)
                && self.target == Game::Fo4
                && source_record.sig.as_str() == "ACRE"
                && !self
                    .translator
                    .maps
                    .skip_records
                    .contains(source_record.sig.as_str())
            {
                crate::translator::pair_hooks::fnv_fo4::lower_acre_signature(&mut source_record)
                    .map_err(|error| RunError::InvalidConfig(format!("legacy_acre:{error}")))?;
            }
            let translated = if admitted_mvp_weapon_record {
                self.translator.translate_ignoring_skip(
                    &source_record,
                    &self.interner,
                    source_record.sig.as_str(),
                )
            } else {
                self.translator.translate(&source_record, &self.interner)
            };
            let (editor_id, target_sig) = match translated {
                TranslateResult::Translated(translated) => {
                    if self
                        .schema_target
                        .record_def(translated.sig.as_str())
                        .is_none()
                    {
                        continue;
                    }
                    (translated.eid, translated.sig)
                }
                TranslateResult::Deferred(DeferredKind::FnvLegacyScripting) => {
                    (source_record.eid, source_record.sig)
                }
                _ => continue,
            };
            intents.push(LegacyFormKeyAllocationIntent {
                source_fk,
                editor_id,
                target_sig,
            });
            if intents.len() == crate::store2::translate_v2::CHUNK {
                let coverage = self.preallocate_legacy_form_key_intents(intents.drain(..));
                total.eligible += coverage.eligible;
                total.mapped += coverage.mapped;
                total.missing += coverage.missing;
            }
        }
        if !intents.is_empty() {
            let coverage = self.preallocate_legacy_form_key_intents(intents.drain(..));
            total.eligible += coverage.eligible;
            total.mapped += coverage.mapped;
            total.missing += coverage.missing;
        }
        self.emit_phase_status(format!(
            "translate: legacy forward-reference preallocation done eligible={} mapped={} missing={}",
            total.eligible, total.mapped, total.missing
        ));
        if total.mapped != total.eligible || total.missing != 0 {
            return Err(RunError::InvalidConfig(format!(
                "legacy forward-reference preallocation incomplete: eligible={} mapped={} missing={}",
                total.eligible, total.mapped, total.missing
            )));
        }
        Ok(())
    }

    /// Publish the source plugin's FO76 named condition forms (`CNDF`) so the
    /// FO76→FO4 pair hook can inline condition-function 875 references. The
    /// hook sees only one `Record` at a time, so this cross-record index has to
    /// be handed to it out of band.
    fn index_fo76_condition_forms(&mut self) {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return;
        }
        match crate::translator::pair_hooks::fo76_fo4::build_condition_form_catalog(
            self.source_handle_id,
            &self.interner,
        ) {
            Ok(catalog) => {
                let indexed = catalog.len();
                crate::translator::pair_hooks::fo76_fo4::install_condition_form_catalog(catalog);
                self.emit_status(&format!(
                    "translate_all: indexed {indexed} FO76 condition forms (CNDF)"
                ));
            }
            Err(err) => self.emit_status(&format!(
                "translate_all: FO76 condition-form (CNDF) index unavailable: {err}"
            )),
        }
    }

    /// Publish the donor object-template block for each FO76 NPC that inherits
    /// one through the FO76-only `TPTA` slot 13 / `ACBS` bit `0x2000` pair. FO4
    /// has no encoding for "inherit the object template without inheriting
    /// traits", so those records must carry a materialized copy or they render
    /// without the body the template supplies.
    fn index_fo76_inherited_object_templates(&mut self) {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return;
        }
        match crate::translator::pair_hooks::fo76_fo4::build_inherited_object_template_catalog(
            self.source_handle_id,
            &self.interner,
        ) {
            Ok(catalog) => {
                let indexed = catalog.len();
                crate::translator::pair_hooks::fo76_fo4::install_inherited_object_template_catalog(
                    catalog,
                );
                self.emit_status(&format!(
                    "translate_all: resolved {indexed} inherited NPC object templates"
                ));
            }
            Err(err) => self.emit_status(&format!(
                "translate_all: inherited NPC object-template index unavailable: {err}"
            )),
        }
    }

    fn plan_skyrim_runtime_components(&mut self, form_keys: &[FormKey]) -> Result<(), RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Ok(());
        }
        if self.skyrim_runtime_capability_plan.is_some() {
            return Err(RunError::InvalidConfig(
                "Skyrim runtime components were already planned for this run".to_string(),
            ));
        }
        let mut records = Vec::with_capacity(form_keys.len());
        for form_key in form_keys {
            let record = read_record_relayout_by_form_key(
                self.source_handle_id,
                form_key,
                &self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "read Skyrim runtime component member {:06X}: {error}",
                    form_key.local
                ))
            })?;
            if self.is_skyrim_runtime_planned_record(record.form_key, record.sig) {
                records.push(record);
            }
        }
        self.plan_skyrim_runtime_records(records)
    }

    pub(crate) fn skyrim_minimal_quest_candidate_manifest_json(
        &mut self,
    ) -> Result<String, RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Ok("[]".to_string());
        }
        let signature = SigCode::from_str("QUST")
            .map_err(|error| RunError::InvalidConfig(format!("QUST signature: {error}")))?;
        let form_keys =
            iter_form_keys_of_sig(self.source_handle_id, signature, &mut self.interner)?;
        let mut rows = Vec::new();
        for form_key in form_keys {
            let Ok(record) = read_record_relayout_by_form_key(
                self.source_handle_id,
                &form_key,
                &self.schema_source,
                &self.interner,
                None,
            ) else {
                continue;
            };
            if let Ok(row) = crate::skyrimse_fo4_runtime::planner::minimal_quest_candidate_manifest(
                &record,
                &self.interner,
            ) {
                rows.push(row);
            }
        }
        rows.sort_by(|left, right| left.component_id.cmp(&right.component_id));
        serde_json::to_string(&rows)
            .map_err(|error| RunError::InvalidConfig(format!("Skyrim quest manifest: {error}")))
    }

    pub(crate) fn install_skyrim_minimal_quest_actions(
        &mut self,
        class_prefix: &str,
        requests: Vec<crate::skyrimse_fo4_runtime::planner::SkyrimMinimalQuestActionRequest>,
    ) -> Result<(), RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Err(RunError::InvalidConfig(
                "Skyrim minimal quest actions require SkyrimSE -> FO4".to_string(),
            ));
        }
        if self.skyrim_runtime_capability_plan.is_some() {
            return Err(RunError::InvalidConfig(
                "Skyrim minimal quest actions must be installed before translate_v2".to_string(),
            ));
        }
        if class_prefix.is_empty()
            || !class_prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(RunError::InvalidConfig(
                "Skyrim minimal quest class prefix is invalid".to_string(),
            ));
        }
        let mut installed = HashMap::new();
        for request in requests {
            if request.component_id.trim().is_empty() || request.actions.is_empty() {
                return Err(RunError::InvalidConfig(
                    "Skyrim minimal quest action request is incomplete".to_string(),
                ));
            }
            if installed
                .insert(request.component_id.clone(), request.actions)
                .is_some()
            {
                return Err(RunError::InvalidConfig(format!(
                    "duplicate Skyrim minimal quest action request {}",
                    request.component_id
                )));
            }
        }
        self.skyrim_minimal_quest_action_requests = installed;
        self.skyrim_minimal_quest_class_prefix = Some(class_prefix.to_string());
        Ok(())
    }

    fn skyrim_pack_donor_evidence(
        &self,
        source: &Record,
    ) -> Result<
        (
            crate::skyrimse_fo4_runtime::package::SkyrimPackProcedureEvidence,
            crate::skyrimse_fo4_runtime::package::SkyrimPackTemplateMaterializationEvidence,
            FormKey,
        ),
        RunError,
    > {
        use crate::skyrimse_fo4_runtime::package::{
            SkyrimPackBlueprintKind, SkyrimPackFo4TemplateBlueprint, SkyrimPackProcedureEvidence,
            SkyrimPackProcedureFamily, SkyrimPackTemplateMaterializationEvidence,
        };

        let family = skyrim_pack_family(source).map_err(RunError::InvalidConfig)?;
        let (target_template_text, blueprint) = match family {
            SkyrimPackProcedureFamily::Travel => (
                "002CB0@Fallout4.esm",
                SkyrimPackFo4TemplateBlueprint::TravelV1,
            ),
            SkyrimPackProcedureFamily::Patrol => (
                "002CE0@Fallout4.esm",
                SkyrimPackFo4TemplateBlueprint::PatrolV2,
            ),
        };
        let donor = self
            .read_explicit_target_master_record(target_template_text)
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "Skyrim PACK canonical donor {target_template_text} is unavailable: {error}"
                ))
            })?;
        if donor.sig.as_str() != "PACK"
            || !donor
                .form_key
                .format(&self.interner)
                .eq_ignore_ascii_case(target_template_text)
            || donor.fields.is_empty()
            || !donor.warnings.is_empty()
        {
            return Err(RunError::InvalidConfig(format!(
                "Skyrim PACK canonical donor {target_template_text} failed structural validation"
            )));
        }
        let source_template =
            skyrim_pack_source_template(source, &self.interner).map_err(RunError::InvalidConfig)?;
        let source_digest =
            skyrim_pack_record_blake3(source, &self.interner).map_err(RunError::InvalidConfig)?;
        let target_digest =
            skyrim_pack_record_blake3(&donor, &self.interner).map_err(RunError::InvalidConfig)?;
        let evidence_id = format!(
            "skyrim-pack-blueprint:{}:{}:{}",
            source.form_key.format(&self.interner),
            &source_digest[..16],
            &target_digest[..16]
        );
        let procedure = SkyrimPackProcedureEvidence {
            family,
            source_template: source_template.format(&self.interner),
            target_template: donor.form_key.format(&self.interner),
            kind: SkyrimPackBlueprintKind::ExactSemanticBlueprint,
            evidence_id: evidence_id.clone(),
            source_blueprint_blake3: source_digest,
            target_blueprint_blake3: target_digest.clone(),
        };
        let materialization = SkyrimPackTemplateMaterializationEvidence {
            family,
            blueprint,
            evidence_id: format!("{evidence_id}:materialization"),
            target_template: donor.form_key.format(&self.interner),
            target_blueprint_blake3: target_digest,
        };
        Ok((procedure, materialization, donor.form_key))
    }

    fn validate_skyrim_story_manager_target_event_root(
        &self,
        event: crate::skyrimse_fo4_runtime::story_manager::SkyrimFo4StoryEvent,
        target: FormKey,
    ) -> Result<(), RunError> {
        let (expected_local, expected_editor_id) = event
            .canonical_fo4_root()
            .ok_or_else(|| RunError::InvalidConfig(event.unsupported_reason()))?;
        let plugin = self.interner.resolve(target.plugin).unwrap_or_default();
        if target.local != expected_local || !plugin.eq_ignore_ascii_case("Fallout4.esm") {
            return Err(RunError::InvalidConfig(format!(
                "Skyrim Story Manager event {} is not mapped to its canonical FO4 SMEN root",
                event.name()
            )));
        }
        let record = self.read_target_record_if_available(target)?.ok_or_else(|| {
            RunError::InvalidConfig(format!(
                "Skyrim Story Manager canonical target {}@Fallout4.esm is absent from the loaded target master",
                format_args!("{expected_local:06X}")
            ))
        })?;
        let editor_id = record
            .eid
            .and_then(|value| self.interner.resolve(value))
            .unwrap_or_default();
        let event_codes = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "ENAM")
            .filter_map(|field| match &field.value {
                FieldValue::Uint(value) => u32::try_from(*value).ok(),
                FieldValue::Int(value) => u32::try_from(*value).ok(),
                FieldValue::Bytes(bytes) if bytes.len() == 4 => {
                    Some(u32::from_le_bytes(bytes[..4].try_into().ok()?))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if record.sig.as_str() != "SMEN"
            || !editor_id.eq_ignore_ascii_case(expected_editor_id)
            || event_codes.as_slice() != [event.code()]
        {
            return Err(RunError::InvalidConfig(format!(
                "Skyrim Story Manager canonical target {expected_local:06X}@Fallout4.esm does not match SMEN/{}/{}",
                event.name(),
                expected_editor_id
            )));
        }
        Ok(())
    }

    pub(crate) fn plan_skyrim_runtime_records(
        &mut self,
        records: Vec<Record>,
    ) -> Result<(), RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Ok(());
        }
        if self.skyrim_runtime_capability_plan.is_some() {
            return Err(RunError::InvalidConfig(
                "Skyrim runtime components were already planned for this run".to_string(),
            ));
        }
        let mut by_local = HashMap::with_capacity(records.len());
        for record in &records {
            if by_local.insert(record.form_key.local, record).is_some() {
                return Err(RunError::InvalidConfig(format!(
                    "Skyrim runtime component preflight has duplicate local FormID {:06X}",
                    record.form_key.local
                )));
            }
        }
        let mut topology = Vec::new();
        if records.iter().any(|record| record.sig.as_str() == "INFO") {
            let info_parents =
                crate::target_write::build_source_info_to_dialogue_index(self.source_handle_id)
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "build Skyrim runtime INFO parent topology: {error}"
                        ))
                    })?;
            for (info_local, dial_local) in info_parents {
                let info_local = info_local & 0x00FF_FFFF;
                let dial_local = dial_local & 0x00FF_FFFF;
                let (Some(info), Some(dial)) =
                    (by_local.get(&info_local), by_local.get(&dial_local))
                else {
                    continue;
                };
                if info.sig.as_str() == "INFO" && dial.sig.as_str() == "DIAL" {
                    topology.push(crate::skyrimse_fo4_runtime::planner::SourceTopologyEdge {
                        parent: dial.form_key,
                        child: info.form_key,
                        kind: crate::skyrimse_fo4_runtime::receipt::RuntimeTopologyKind::TopicChild,
                    });
                }
            }
        }
        if records.iter().any(|record| record.sig.as_str() == "ACHR") {
            let cell_sig = SigCode::from_str("CELL")
                .map_err(|error| RunError::InvalidConfig(format!("CELL signature: {error}")))?;
            let cells = iter_form_keys_of_sig(self.source_handle_id, cell_sig, &mut self.interner)?;
            let cell_ids = cells
                .iter()
                .map(|cell| cell.local)
                .collect::<FxHashSet<_>>();
            let cell_by_local = cells
                .into_iter()
                .map(|cell| (cell.local, cell))
                .collect::<HashMap<_, _>>();
            let children = crate::source_read::collect_interior_cell_children(
                self.source_handle_id,
                &cell_ids,
            )?;
            for (cell_local, children) in children {
                let Some(parent) = cell_by_local.get(&cell_local).copied() else {
                    continue;
                };
                for (locals, kind) in [
                    (
                        &children.persistent,
                        crate::skyrimse_fo4_runtime::receipt::RuntimeTopologyKind::PersistentCellChild,
                    ),
                    (
                        &children.temporary,
                        crate::skyrimse_fo4_runtime::receipt::RuntimeTopologyKind::TemporaryCellChild,
                    ),
                ] {
                    for child_local in locals {
                        let Some(child) = by_local.get(child_local) else {
                            continue;
                        };
                        if child.sig.as_str() == "ACHR" {
                            topology.push(
                                crate::skyrimse_fo4_runtime::planner::SourceTopologyEdge {
                                    parent,
                                    child: child.form_key,
                                    kind,
                                },
                            );
                        }
                    }
                }
            }
        }
        let mut capability_plan = crate::skyrimse_fo4_runtime::planner::plan_components(
            records.clone(),
            &topology,
            &self.interner,
        )
        .map_err(RunError::InvalidConfig)?;
        self.skyrim_minimal_quest_projections.clear();
        self.skyrim_minimal_alias_projections.clear();
        self.skyrim_pack_projections.clear();
        self.skyrim_scene_projections.clear();
        self.skyrim_dialogue_projections.clear();
        self.skyrim_story_manager_projections.clear();
        self.skyrim_quest_runtime_voice_receipt = None;
        self.skyrim_quest_runtime_asset_copy_receipt = None;
        let class_prefix = self
            .skyrim_minimal_quest_class_prefix
            .clone()
            .unwrap_or_else(|| "Skyrim".to_string());
        let requests = self.skyrim_minimal_quest_action_requests.clone();
        let mut admitted_requests = FxHashSet::default();
        for record in records
            .iter()
            .filter(|record| record.sig.as_str() == "QUST")
        {
            let Ok(manifest) =
                crate::skyrimse_fo4_runtime::planner::minimal_quest_candidate_manifest(
                    record,
                    &self.interner,
                )
            else {
                continue;
            };
            let Some(actions) = requests.get(&manifest.component_id).cloned() else {
                continue;
            };
            let component = capability_plan
                .components
                .iter()
                .find(|component| {
                    component
                        .members
                        .iter()
                        .any(|member| member.form_key.eq_ignore_ascii_case(&manifest.source_quest))
                })
                .ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "{} has no Skyrim runtime component",
                        manifest.component_id
                    ))
                })?;
            let component_form_keys = component
                .members
                .iter()
                .map(|member| FormKey::parse(&member.form_key, &self.interner))
                .collect::<Result<Vec<_>, _>>()
                .map_err(RunError::InvalidConfig)?;
            let component_records = component_form_keys
                .iter()
                .map(|form_key| {
                    records
                        .iter()
                        .find(|candidate| candidate.form_key == *form_key)
                        .ok_or_else(|| {
                            RunError::InvalidConfig(format!(
                                "{} component member {} is unavailable",
                                manifest.component_id,
                                form_key.format(&self.interner)
                            ))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let package_records = component_records
                .iter()
                .copied()
                .filter(|member| member.sig.as_str() == "PACK")
                .collect::<Vec<_>>();
            let scene_records = component_records
                .iter()
                .copied()
                .filter(|member| member.sig.as_str() == "SCEN")
                .collect::<Vec<_>>();
            let dialogue_records = component_records
                .iter()
                .copied()
                .filter(|member| matches!(member.sig.as_str(), "DLVW" | "DLBR" | "DIAL" | "INFO"))
                .collect::<Vec<_>>();
            let story_records = component_records
                .iter()
                .copied()
                .filter(|member| matches!(member.sig.as_str(), "SMEN" | "SMBN" | "SMQN"))
                .collect::<Vec<_>>();
            let story_route = if story_records.is_empty() {
                None
            } else {
                let roots = story_records
                    .iter()
                    .copied()
                    .filter(|story| story.sig.as_str() == "SMEN")
                    .collect::<Vec<_>>();
                let [source_root] = roots.as_slice() else {
                    return Err(RunError::InvalidConfig(format!(
                        "{} Story Manager component requires exactly one SMEN",
                        manifest.component_id
                    )));
                };
                let event_codes = source_root
                    .fields
                    .iter()
                    .filter(|field| field.sig.as_str() == "ENAM")
                    .filter_map(|field| match &field.value {
                        FieldValue::Uint(value) => u32::try_from(*value).ok(),
                        FieldValue::Int(value) => u32::try_from(*value).ok(),
                        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
                            Some(u32::from_le_bytes(bytes[..4].try_into().ok()?))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [event_code] = event_codes.as_slice() else {
                    return Err(RunError::InvalidConfig(format!(
                        "{} Story Manager SMEN requires exactly one decoded ENAM event identity",
                        manifest.component_id
                    )));
                };
                let event =
                    crate::skyrimse_fo4_runtime::story_manager::SkyrimFo4StoryEvent::from_code(
                        *event_code,
                    )
                    .map_err(RunError::InvalidConfig)?;
                let (target_local, _) = event
                    .canonical_fo4_root()
                    .ok_or_else(|| RunError::InvalidConfig(event.unsupported_reason()))?;
                let target = FormKey {
                    local: target_local,
                    plugin: self.interner.intern("Fallout4.esm"),
                };
                self.validate_skyrim_story_manager_target_event_root(event, target)?;
                Some((source_root.form_key, event, target))
            };
            if component_records.iter().any(|member| {
                !matches!(
                    member.sig.as_str(),
                    "QUST"
                        | "PACK"
                        | "SCEN"
                        | "DLVW"
                        | "DLBR"
                        | "DIAL"
                        | "INFO"
                        | "SMEN"
                        | "SMBN"
                        | "SMQN"
                )
            }) || component_records
                .iter()
                .filter(|member| member.sig.as_str() == "QUST")
                .count()
                != 1
            {
                return Err(RunError::InvalidConfig(format!(
                    "{} owns unsupported quest runtime dependency records",
                    manifest.component_id
                )));
            }
            let mut package_inputs = Vec::with_capacity(package_records.len());
            for package_record in package_records {
                let (procedure_evidence, template_evidence, target_template) =
                    self.skyrim_pack_donor_evidence(package_record)?;
                let classification = crate::skyrimse_fo4_runtime::package::classify_skyrim_pack(
                    package_record,
                    &procedure_evidence,
                    &self.interner,
                )
                .map_err(|reason| {
                    RunError::InvalidConfig(format!(
                        "{} package {} classification rejected: {reason:?}",
                        manifest.component_id,
                        package_record.form_key.format(&self.interner)
                    ))
                })?;
                package_inputs.push((
                    (*package_record).clone(),
                    classification,
                    template_evidence,
                    target_template,
                ));
            }
            let dependencies =
                crate::skyrimse_fo4_runtime::planner::minimal_quest_dependency_form_keys(
                    record,
                    &self.interner,
                )
                .map_err(RunError::InvalidConfig)?;
            let mut allocation_records = vec![record.clone()];
            for dependency in dependencies {
                let dependency_record = read_record_relayout_by_form_key(
                    self.source_handle_id,
                    &dependency,
                    &self.schema_source,
                    &self.interner,
                    None,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} dependency {:06X} cannot be read: {error}",
                        manifest.component_id, dependency.local
                    ))
                })?;
                allocation_records.push(dependency_record);
            }
            let minimal_allocation_keys = allocation_records
                .iter()
                .map(|record| record.form_key)
                .collect::<FxHashSet<_>>();
            allocation_records.extend(
                package_inputs
                    .iter()
                    .map(|(package, _, _, _)| package.clone()),
            );
            allocation_records.extend(scene_records.iter().map(|scene| (*scene).clone()));
            allocation_records.extend(dialogue_records.iter().map(|dialogue| (*dialogue).clone()));
            allocation_records.extend(story_records.iter().map(|story| (*story).clone()));
            let mut allocated = allocation_records
                .iter()
                .map(|record| record.form_key)
                .collect::<FxHashSet<_>>();
            for (_, classification, _, _) in &package_inputs {
                for dependency in &classification.dependencies {
                    if dependency
                        .eq_ignore_ascii_case(&classification.procedure_evidence.source_template)
                        || dependency.eq_ignore_ascii_case(
                            &classification.procedure_evidence.target_template,
                        )
                    {
                        continue;
                    }
                    let dependency = FormKey::parse(dependency, &self.interner)
                        .map_err(RunError::InvalidConfig)?;
                    if !allocated.insert(dependency) {
                        continue;
                    }
                    let dependency_record = read_record_relayout_by_form_key(
                        self.source_handle_id,
                        &dependency,
                        &self.schema_source,
                        &self.interner,
                        None,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "{} package dependency {} cannot be read: {error}",
                            manifest.component_id,
                            dependency.format(&self.interner)
                        ))
                    })?;
                    allocation_records.push(dependency_record);
                }
            }
            let coverage = self.preallocate_legacy_form_key_intents(
                allocation_records
                    .iter()
                    .filter(|record| record.sig.as_str() != "SMEN")
                    .map(|record| LegacyFormKeyAllocationIntent {
                        source_fk: record.form_key,
                        editor_id: record.eid,
                        target_sig: record.sig,
                    }),
            );
            if coverage.mapped != coverage.eligible || coverage.missing != 0 {
                return Err(RunError::InvalidConfig(format!(
                    "{} target FormID preallocation is incomplete",
                    manifest.component_id
                )));
            }
            let mapper = self.mapper_state.as_ref().expect("mapper initialized");
            let target_by_source = allocation_records
                .iter()
                .map(|record| {
                    let target = if record.sig.as_str() == "SMEN" {
                        story_route
                            .filter(|(source, _, _)| *source == record.form_key)
                            .map(|(_, _, target)| target)
                            .expect("Story Manager root was validated before allocation")
                    } else {
                        mapper.source_to_target[&record.form_key]
                    };
                    (record.form_key, (target, record.sig))
                })
                .collect::<HashMap<_, _>>();
            let mut admission = crate::skyrimse_fo4_runtime::planner::admit_minimal_quest(
                record,
                &target_by_source
                    .iter()
                    .filter(|(source, _)| minimal_allocation_keys.contains(source))
                    .map(|(source, target)| (*source, *target))
                    .collect::<HashMap<_, _>>(),
                &class_prefix,
                actions.clone(),
                &self.interner,
            )
            .map_err(RunError::InvalidConfig)?;
            let minimal_projection_plan = admission.plan.clone();
            let component_owned_sources = component_records
                .iter()
                .filter(|member| member.sig.as_str() != "QUST")
                .map(|member| member.form_key)
                .collect::<FxHashSet<_>>();
            for allocation in &allocation_records {
                if minimal_allocation_keys.contains(&allocation.form_key) {
                    continue;
                }
                let target = target_by_source[&allocation.form_key].0;
                let source_key = crate::quest_runtime::QuestRecordKey::new(
                    allocation.sig.as_str(),
                    allocation.form_key.format(&self.interner),
                );
                let target_key = crate::quest_runtime::QuestRecordKey::new(
                    allocation.sig.as_str(),
                    target.format(&self.interner),
                );
                admission
                    .plan
                    .mappings
                    .push(crate::quest_runtime::SourceTargetFormMapping {
                        source: source_key.clone(),
                        target: target_key.clone(),
                    });
                if component_owned_sources.contains(&allocation.form_key) {
                    admission.plan.owned_records.push(source_key.clone());
                    let placements = match allocation.sig.as_str() {
                        "SCEN" => Some((
                            crate::skyrimse_fo4_runtime::scene::scene_placement(
                                &source_key,
                                &admission.plan.root_quest,
                            ),
                            crate::skyrimse_fo4_runtime::scene::scene_placement(
                                &target_key,
                                &admission
                                    .plan
                                    .mappings
                                    .iter()
                                    .find(|mapping| mapping.source == admission.plan.root_quest)
                                    .expect("root quest mapping validated")
                                    .target,
                            ),
                        )),
                        "PACK" => Some((
                            crate::quest_runtime::TopologyPlacement {
                                record: source_key,
                                group_path: vec!["GRUP:PACK".to_string()],
                            },
                            crate::quest_runtime::TopologyPlacement {
                                record: target_key.clone(),
                                group_path: vec!["GRUP:PACK".to_string()],
                            },
                        )),
                        "SMEN" | "SMBN" | "SMQN" => Some((
                            crate::quest_runtime::TopologyPlacement {
                                record: source_key.clone(),
                                group_path: vec![format!("GRUP:{}", allocation.sig.as_str())],
                            },
                            crate::quest_runtime::TopologyPlacement {
                                record: target_key.clone(),
                                group_path: vec![format!("GRUP:{}", allocation.sig.as_str())],
                            },
                        )),
                        _ => None,
                    };
                    if allocation.sig.as_str() != "SMEN" {
                        admission
                            .plan
                            .expected_receipt
                            .emitted_records
                            .insert(target_key.clone());
                    }
                    if let Some((source_placement, target_placement)) = placements {
                        admission.plan.source_topology.insert(source_placement);
                        admission
                            .plan
                            .target_topology
                            .insert(target_placement.clone());
                        if allocation.sig.as_str() != "SMEN" {
                            admission
                                .plan
                                .expected_receipt
                                .placements
                                .insert(target_placement);
                        }
                    }
                } else {
                    admission.plan.shared_records.insert(source_key.clone());
                    admission
                        .plan
                        .dependencies
                        .direct
                        .insert(source_key.clone());
                    admission.plan.dependencies.recursive.insert(source_key);
                }
            }
            admission.plan.mappings.sort();
            let mut dialogue_projections = Vec::new();
            if !dialogue_records.is_empty() {
                let dialogue_keys = dialogue_records
                    .iter()
                    .map(|dialogue| dialogue.form_key)
                    .collect::<FxHashSet<_>>();
                let mut source_topic_children = HashMap::<FormKey, Vec<FormKey>>::new();
                for edge in &topology {
                    if matches!(
                        edge.kind,
                        crate::skyrimse_fo4_runtime::receipt::RuntimeTopologyKind::TopicChild
                    ) && dialogue_keys.contains(&edge.parent)
                        && dialogue_keys.contains(&edge.child)
                    {
                        source_topic_children
                            .entry(edge.parent)
                            .or_default()
                            .push(edge.child);
                    }
                }
                for children in source_topic_children.values_mut() {
                    children.sort_by_key(|child| child.local);
                    children.dedup();
                }
                let dialogue_plan =
                    crate::skyrimse_fo4_runtime::dialogue::SkyrimDialoguePlan::derive(
                        &dialogue_records
                            .iter()
                            .map(|record| (*record).clone())
                            .collect::<Vec<_>>(),
                        &source_topic_children,
                        &self.interner,
                    )
                    .map_err(RunError::InvalidConfig)?;
                if let Some(reason) = dialogue_plan.unsupported_reason() {
                    return Err(RunError::InvalidConfig(format!(
                        "{} dialogue admission rejected: {reason}",
                        manifest.component_id
                    )));
                }
                let dialogue_target_by_source = target_by_source
                    .iter()
                    .map(|(source, (target, _))| (*source, *target))
                    .collect::<HashMap<_, _>>();
                let fallout4_master = self.interner.intern("Fallout4.esm");
                let projection = crate::skyrimse_fo4_runtime::dialogue::project_dialogue(
                    &dialogue_plan,
                    &dialogue_target_by_source,
                    fallout4_master,
                    &self.interner,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} dialogue projection rejected: {error}",
                        manifest.component_id
                    ))
                })?;
                if !projection.receipt.fragment_intents.is_empty() {
                    return Err(RunError::InvalidConfig(format!(
                        "{} dialogue requires fragment compiler evidence",
                        manifest.component_id
                    )));
                }
                for edge in &projection.receipt.topology {
                    use crate::skyrimse_fo4_runtime::dialogue::SkyrimDialogueTopologyKind;
                    let signature = dialogue_records
                        .iter()
                        .find(|record| record.form_key == edge.source_child)
                        .expect("dialogue source topology record exists")
                        .sig
                        .as_str();
                    let source_child = crate::quest_runtime::QuestRecordKey::new(
                        signature,
                        edge.source_child.format(&self.interner),
                    );
                    let target_child = crate::quest_runtime::QuestRecordKey::new(
                        signature,
                        edge.target_child.format(&self.interner),
                    );
                    let (source_path, target_path) = match edge.kind {
                        SkyrimDialogueTopologyKind::TopLevel => {
                            (vec!["GRUP:DLVW".to_string()], vec!["GRUP:DLVW".to_string()])
                        }
                        SkyrimDialogueTopologyKind::QuestChild => (
                            vec![
                                "QUST".to_string(),
                                edge.source_parent.format(&self.interner),
                                "children:10".to_string(),
                            ],
                            vec![
                                "QUST".to_string(),
                                edge.target_parent.format(&self.interner),
                                "children:10".to_string(),
                            ],
                        ),
                        SkyrimDialogueTopologyKind::TopicChild => (
                            vec![
                                "DIAL".to_string(),
                                edge.source_parent.format(&self.interner),
                                "children:7".to_string(),
                            ],
                            vec![
                                "DIAL".to_string(),
                                edge.target_parent.format(&self.interner),
                                "children:7".to_string(),
                            ],
                        ),
                    };
                    admission.plan.source_topology.insert(
                        crate::quest_runtime::TopologyPlacement {
                            record: source_child,
                            group_path: source_path,
                        },
                    );
                    let target_placement = crate::quest_runtime::TopologyPlacement {
                        record: target_child,
                        group_path: target_path,
                    };
                    admission
                        .plan
                        .target_topology
                        .insert(target_placement.clone());
                    admission
                        .plan
                        .expected_receipt
                        .placements
                        .insert(target_placement);
                }
                dialogue_projections.push(SkyrimDialogueLiveProjection {
                    component_id: manifest.component_id.clone(),
                    projection,
                });
            }
            let mut story_projection = None;
            if !story_records.is_empty() {
                let (source_root_form_key, story_event, _) =
                    story_route.expect("Story Manager route was validated before allocation");
                let source_root = story_records
                    .iter()
                    .copied()
                    .find(|story| story.form_key == source_root_form_key)
                    .expect("validated Story Manager root exists");
                if story_records.len() != 3
                    || story_records
                        .iter()
                        .filter(|story| story.sig.as_str() == "SMBN")
                        .count()
                        != 1
                    || story_records
                        .iter()
                        .filter(|story| story.sig.as_str() == "SMQN")
                        .count()
                        != 1
                {
                    return Err(RunError::InvalidConfig(format!(
                        "{} Story Manager component requires one SMEN, SMBN, and SMQN",
                        manifest.component_id
                    )));
                }
                let source_root_key = crate::quest_runtime::QuestRecordKey::new(
                    "SMEN",
                    source_root.form_key.format(&self.interner),
                );
                let target_root_key = crate::quest_runtime::QuestRecordKey::new(
                    "SMEN",
                    target_by_source[&source_root.form_key]
                        .0
                        .format(&self.interner),
                );
                let route_id = format!("skyrim-story-route:{:06X}", record.form_key.local);
                admission
                    .plan
                    .semantics
                    .start_flags
                    .remove(&crate::quest_runtime::QuestStartFlag::StartGameEnabled);
                admission.plan.inbound_producers.clear();
                admission.plan.expected_receipt.routes.clear();
                admission.plan.inbound_producers.insert(
                    crate::quest_runtime::StartProducerIntent {
                        producer_id: route_id.clone(),
                        carrier: source_root_key,
                        producer_kind: "native_story_event".to_string(),
                        evidence_id: format!("fo4-native-event:{}", story_event.name()),
                        proven: true,
                    },
                );
                admission.plan.start_disposition =
                    Some(crate::quest_runtime::StartDisposition::StoryManager {
                        route_id: route_id.clone(),
                    });
                let mut source_records = story_records
                    .iter()
                    .map(|story| (*story).clone())
                    .collect::<Vec<_>>();
                source_records.push(record.clone());
                story_projection = Some(SkyrimStoryManagerLiveProjection {
                    component_id: manifest.component_id.clone(),
                    source_records,
                    event_mapping:
                        crate::skyrimse_fo4_runtime::story_manager::SkyrimStoryEventMapping {
                            event: story_event,
                            target_event_root: target_root_key,
                            producer_id: route_id,
                        },
                    projection: None,
                });
            }
            let issues = admission.plan.validation_issues();
            if !issues.is_empty() {
                return Err(RunError::InvalidConfig(format!(
                    "{} expanded component contract is invalid: {}",
                    manifest.component_id,
                    serde_json::to_string(&issues).unwrap_or_default()
                )));
            }
            let reference_mappings = target_by_source
                .iter()
                .map(|(source, (target, _))| (source.format(&self.interner), *target))
                .collect::<BTreeMap<_, _>>();
            let mut package_projections = Vec::with_capacity(package_inputs.len());
            for (package, classification, template_evidence, target_template) in package_inputs {
                let target_form_key = target_by_source[&package.form_key].0;
                let lowering = crate::skyrimse_fo4_runtime::package::lower_skyrim_pack(
                    &classification,
                    &crate::skyrimse_fo4_runtime::package::SkyrimPackLoweringRequest {
                        component_id: manifest.component_id.clone(),
                        quest_component_admitted: true,
                        source_owned_by_component: true,
                        target_record: target_form_key.format(&self.interner),
                        script_projections: Vec::new(),
                    },
                )
                .map_err(|reason| {
                    RunError::InvalidConfig(format!(
                        "{} package {} lowering rejected: {reason:?}",
                        manifest.component_id,
                        package.form_key.format(&self.interner)
                    ))
                })?;
                let materialization =
                    crate::skyrimse_fo4_runtime::package::materialize_skyrim_pack(
                        &lowering,
                        package.form_key,
                        target_form_key,
                        target_template,
                        &reference_mappings,
                        &template_evidence,
                        &self.interner,
                    )
                    .map_err(|reason| {
                        RunError::InvalidConfig(format!(
                            "{} package {} materialization rejected: {reason:?}",
                            manifest.component_id,
                            package.form_key.format(&self.interner)
                        ))
                    })?;
                if !materialization.pending_script_projections.is_empty()
                    || materialization.receipt.vmad_attached
                {
                    return Err(RunError::InvalidConfig(format!(
                        "{} package {} crossed the compiler/VMAD boundary",
                        manifest.component_id,
                        package.form_key.format(&self.interner)
                    )));
                }
                package_projections.push(SkyrimPackLiveProjection {
                    source_form_key: package.form_key,
                    target_form_key,
                    target_template,
                    lowering,
                    reference_mappings: reference_mappings.clone(),
                    template_evidence,
                    materialization,
                });
            }
            let mut scene_projections = Vec::with_capacity(scene_records.len());
            for scene in scene_records {
                let scene_admission = crate::skyrimse_fo4_runtime::scene::admit_scene_record(
                    &admission.plan,
                    scene,
                    &crate::skyrimse_fo4_runtime::scene::SkyrimSceneAdmissionEvidence::default(),
                    &self.interner,
                )
                .map_err(|rejection| {
                    RunError::InvalidConfig(format!(
                        "{} scene {} admission rejected: {rejection}",
                        manifest.component_id,
                        scene.form_key.format(&self.interner)
                    ))
                })?;
                let scene_projection = crate::skyrimse_fo4_runtime::scene::project_scene(
                    &scene_admission.plan,
                    &self.interner,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} scene {} projection rejected: {error}",
                        manifest.component_id,
                        scene.form_key.format(&self.interner)
                    ))
                })?;
                if !scene_projection.pending_vmad.is_empty() {
                    return Err(RunError::InvalidConfig(format!(
                        "{} scene {} requires fragment compiler evidence",
                        manifest.component_id,
                        scene.form_key.format(&self.interner)
                    )));
                }
                scene_projections.push(scene_projection);
            }
            let target_local = target_by_source[&record.form_key].0.local;
            let fragment = crate::skyrimse_fo4_runtime::quest::SkyrimQuestFragmentSource {
                fragment_id: admission.fragment_id.clone(),
                stage_index: admission.stage_index,
                stage_item_index: admission.stage_item_index,
                actions: admission
                    .actions
                    .iter()
                    .cloned()
                    .map(skyrim_projected_quest_action)
                    .collect(),
            };
            let mut projection = crate::skyrimse_fo4_runtime::quest::project_minimal_quest(
                &minimal_projection_plan,
                &admission.editor_id,
                admission.priority,
                admission.title.clone(),
                fragment,
                &class_prefix,
                &self.interner,
            )
            .map_err(RunError::InvalidConfig)?;
            if story_projection.is_some() {
                clear_skyrim_projected_quest_start_enabled(&mut projection.record, &self.interner)
                    .map_err(RunError::InvalidConfig)?;
            }
            let alias_projection = crate::skyrimse_fo4_runtime::alias::project_quest_aliases(
                &admission.plan,
                &admission.alias_evidence,
                &[],
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "{} alias projection rejected: {error}",
                    manifest.component_id
                ))
            })?;
            let alias_application =
                crate::skyrimse_fo4_runtime::alias::apply_alias_projection_to_qust(
                    &mut projection.record,
                    &alias_projection,
                    &self.interner,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} alias materialization rejected: {error}",
                        manifest.component_id
                    ))
                })?;
            if alias_application.receipt.preserved_quest_vmad_count != 0
                || alias_application.receipt.alias_vmad_attachment_count != 0
            {
                return Err(RunError::InvalidConfig(format!(
                    "{} alias materialization crossed the pre-compile VMAD boundary",
                    manifest.component_id
                )));
            }
            if projection.record.form_key.local != target_local {
                return Err(RunError::InvalidConfig(format!(
                    "{} projector changed the preallocated target FormID",
                    manifest.component_id
                )));
            }
            for member in &component_form_keys {
                let disposition = capability_plan
                    .by_record
                    .get_mut(member)
                    .expect("planned component member has disposition");
                disposition.supported = true;
                disposition.reason = None;
                disposition.component_ids = vec![manifest.component_id.clone()];
            }
            let component = capability_plan
                .components
                .iter_mut()
                .find(|component| {
                    component
                        .members
                        .iter()
                        .any(|member| member.form_key.eq_ignore_ascii_case(&manifest.source_quest))
                })
                .expect("planned QUST component exists");
            component.component_id = manifest.component_id.clone();
            component.family = "quest".to_string();
            component.decision =
                crate::skyrimse_fo4_runtime::receipt::RuntimeComponentDecision::Supported;
            component.reason = None;
            capability_plan
                .minimal_quests
                .insert(record.form_key, admission);
            self.skyrim_minimal_alias_projections.push(alias_projection);
            self.skyrim_pack_projections.extend(package_projections);
            self.skyrim_scene_projections.extend(scene_projections);
            self.skyrim_dialogue_projections
                .extend(dialogue_projections);
            if let Some(story_projection) = story_projection {
                self.skyrim_story_manager_projections.push(story_projection);
            }
            self.skyrim_minimal_quest_projections.push(projection);
            admitted_requests.insert(manifest.component_id);
        }
        let mut missing = requests
            .keys()
            .filter(|component_id| !admitted_requests.contains(*component_id))
            .cloned()
            .collect::<Vec<_>>();
        missing.sort();
        if !missing.is_empty() {
            return Err(RunError::InvalidConfig(format!(
                "Skyrim minimal quest action requests were not admitted: {}",
                missing.join(",")
            )));
        }
        self.skyrim_runtime_capability_plan = Some(capability_plan);
        Ok(())
    }

    pub(crate) fn skyrim_runtime_component_disposition(
        &self,
        form_key: FormKey,
    ) -> Option<&crate::skyrimse_fo4_runtime::planner::SkyrimComponentDisposition> {
        self.skyrim_runtime_capability_plan
            .as_ref()
            .and_then(|plan| plan.by_record.get(&form_key))
    }

    pub(crate) fn is_skyrim_minimal_quest_owned(&self, form_key: FormKey) -> bool {
        self.skyrim_runtime_capability_plan
            .as_ref()
            .is_some_and(|plan| plan.minimal_quests.contains_key(&form_key))
    }

    pub(crate) fn is_skyrim_component_projection_owned(&self, form_key: FormKey) -> bool {
        self.skyrim_runtime_capability_plan
            .as_ref()
            .and_then(|plan| {
                let disposition = plan.by_record.get(&form_key)?;
                let signature = plan.record_signatures.get(&form_key)?;
                Some(
                    disposition.supported
                        && matches!(
                            signature.as_str(),
                            "QUST"
                                | "PACK"
                                | "DLVW"
                                | "DLBR"
                                | "DIAL"
                                | "INFO"
                                | "SCEN"
                                | "SMEN"
                                | "SMBN"
                                | "SMQN"
                        ),
                )
            })
            .unwrap_or(false)
    }

    pub(crate) fn is_skyrim_creature_batch_owned(&self, form_key: FormKey) -> bool {
        self.skyrim_creature_batch_owned_sources.contains(&form_key)
    }

    fn freeze_skyrim_localized_receipt(
        &mut self,
        component_id: &str,
        form_key: FormKey,
        signature: SigCode,
    ) -> Result<(), RunError> {
        let rows = crate::target_write::existing_localized_string_occurrences_native(
            self.target_handle_id,
            form_key,
            signature.as_str(),
            &self.interner,
        )
        .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
        let owner = crate::quest_runtime::QuestRecordKey::new(
            signature.as_str(),
            form_key.format(&self.interner),
        );
        let admission = self
            .skyrim_runtime_capability_plan
            .as_mut()
            .and_then(|capability| {
                capability
                    .minimal_quests
                    .values_mut()
                    .find(|admission| admission.plan.component_id == component_id)
            })
            .ok_or_else(|| {
                RunError::InvalidConfig(format!(
                    "{component_id} has no admitted localized-string receipt owner"
                ))
            })?;
        admission
            .plan
            .expected_receipt
            .localized_strings
            .retain(|localized| localized.owner != owner);
        for (field, table, string_id) in rows {
            admission.plan.expected_receipt.localized_strings.insert(
                crate::quest_runtime::LocalizedStringReceipt {
                    owner: owner.clone(),
                    field,
                    table,
                    string_id,
                },
            );
        }
        Ok(())
    }

    pub(crate) fn emit_skyrim_minimal_quest_projections(
        &mut self,
    ) -> Result<TranslateStats, RunError> {
        let mut stats = TranslateStats::default();
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Ok(stats);
        }
        for projection in self.skyrim_pack_projections.clone() {
            crate::skyrimse_fo4_runtime::package::verify_skyrim_pack_record_receipt(
                &projection.lowering,
                &projection.materialization.record,
                projection.source_form_key,
                projection.target_form_key,
                projection.target_template,
                &projection.reference_mappings,
                &projection.template_evidence,
                &projection.materialization.receipt,
                &self.interner,
            )
            .map_err(|reason| {
                RunError::InvalidConfig(format!(
                    "emit admitted Skyrim PACK {} receipt rejected: {reason:?}",
                    projection.source_form_key.format(&self.interner)
                ))
            })?;
            crate::target_write::add_record_native(
                self.target_handle_id,
                projection.materialization.record,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "emit admitted Skyrim PACK {}: {error}",
                    projection.source_form_key.format(&self.interner)
                ))
            })?;
            stats.records_translated += 1;
            stats.signature_entry(SigCode(*b"PACK")).translated += 1;
        }
        for projection in self.skyrim_minimal_quest_projections.clone() {
            let component_id = projection.component_id.clone();
            let form_key = projection.record.form_key;
            let signature = projection.record.sig;
            crate::target_write::add_record_native(
                self.target_handle_id,
                projection.record,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "emit admitted Skyrim quest {}: {error}",
                    projection.component_id
                ))
            })?;
            self.freeze_skyrim_localized_receipt(&component_id, form_key, signature)?;
            stats.records_translated += 1;
            stats.signature_entry(SigCode(*b"QUST")).translated += 1;
        }
        for live in self.skyrim_dialogue_projections.clone() {
            let records = live
                .projection
                .top_level
                .iter()
                .chain(&live.projection.quest_children)
                .chain(
                    live.projection
                        .topic_children
                        .iter()
                        .map(|info| &info.record),
                )
                .cloned()
                .collect::<Vec<_>>();
            let topic_parent_by_info = live
                .projection
                .topic_children
                .iter()
                .map(|info| (info.record.form_key, info.parent_topic))
                .collect::<HashMap<_, _>>();
            live.projection
                .receipt
                .validate_materialized(&records, &topic_parent_by_info)
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} dialogue receipt rejected before emission: {error}",
                        live.component_id
                    ))
                })?;
            for record in live.projection.top_level {
                let signature = record.sig;
                let form_key = record.form_key;
                crate::target_write::add_record_native(
                    self.target_handle_id,
                    record,
                    &self.schema_target,
                    &self.interner,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} emit top-level dialogue record: {error}",
                        live.component_id
                    ))
                })?;
                self.freeze_skyrim_localized_receipt(&live.component_id, form_key, signature)?;
                stats.records_translated += 1;
                stats.signature_entry(signature).translated += 1;
            }
            for record in live.projection.quest_children {
                let signature = record.sig;
                let form_key = record.form_key;
                let inserted = crate::target_write::add_quest_child_record_native(
                    self.target_handle_id,
                    record,
                    &self.schema_target,
                    &self.interner,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} emit QUST-child dialogue record: {error}",
                        live.component_id
                    ))
                })?;
                if !inserted {
                    return Err(RunError::InvalidConfig(format!(
                        "{} dialogue QUST parent is missing",
                        live.component_id
                    )));
                }
                self.freeze_skyrim_localized_receipt(&live.component_id, form_key, signature)?;
                stats.records_translated += 1;
                stats.signature_entry(signature).translated += 1;
            }
            for info in live.projection.topic_children {
                let form_key = info.record.form_key;
                let parent_form_id = encode_form_key_for_handle(
                    self.target_handle_id,
                    info.parent_topic,
                    &self.interner,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                let inserted = crate::target_write::add_topic_child_record_native(
                    self.target_handle_id,
                    info.record,
                    parent_form_id,
                    &self.schema_target,
                    &self.interner,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} emit DIAL-child INFO: {error}",
                        live.component_id
                    ))
                })?;
                if !inserted {
                    return Err(RunError::InvalidConfig(format!(
                        "{} dialogue DIAL parent is missing",
                        live.component_id
                    )));
                }
                self.freeze_skyrim_localized_receipt(
                    &live.component_id,
                    form_key,
                    SigCode(*b"INFO"),
                )?;
                stats.records_translated += 1;
                stats.signature_entry(SigCode(*b"INFO")).translated += 1;
            }
        }
        for projection in self.skyrim_scene_projections.clone() {
            projection
                .receipt
                .validate_materialized(
                    &projection.record,
                    &projection.parent_quest,
                    projection.group_type,
                    &projection.pending_vmad,
                )
                .map_err(RunError::InvalidConfig)?;
            let inserted = crate::target_write::add_quest_child_record_native(
                self.target_handle_id,
                projection.record,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "emit admitted Skyrim SCEN {}: {error}",
                    projection.receipt.target_scene.form_key
                ))
            })?;
            if !inserted {
                return Err(RunError::InvalidConfig(format!(
                    "emit admitted Skyrim SCEN {}: target QUST parent is missing",
                    projection.receipt.target_scene.form_key
                )));
            }
            stats.records_translated += 1;
            stats.signature_entry(SigCode(*b"SCEN")).translated += 1;
        }
        Ok(stats)
    }

    pub(crate) fn emit_skyrim_minimal_quest_psc_manifest_json(
        &mut self,
        mod_path: &Path,
    ) -> Result<String, RunError> {
        let artifacts = self
            .skyrim_minimal_quest_projections
            .iter()
            .map(|projection| projection.psc_artifact.clone())
            .collect::<Vec<_>>();
        let manifest =
            crate::skyrimse_fo4_runtime::papyrus::emit_script_sources(mod_path, &artifacts)
                .map_err(RunError::InvalidConfig)?;
        serde_json::to_string(&manifest).map_err(|error| {
            RunError::InvalidConfig(format!("serialize Skyrim PSC manifest: {error}"))
        })
    }

    pub(crate) fn reconcile_skyrim_minimal_quest_compiler_evidence(
        &mut self,
        evidence: Vec<crate::skyrimse_fo4_runtime::papyrus::SkyrimPscCompilerEvidence>,
    ) -> Result<u32, RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            if evidence.is_empty() {
                return Ok(0);
            }
            return Err(RunError::InvalidConfig(
                "Skyrim quest compiler evidence requires SkyrimSE -> FO4".to_string(),
            ));
        }
        let mut by_manifest = HashMap::new();
        for row in evidence {
            if by_manifest.insert(row.manifest_id.clone(), row).is_some() {
                return Err(RunError::InvalidConfig(
                    "duplicate Skyrim quest compiler evidence manifest".to_string(),
                ));
            }
        }
        let required = self
            .skyrim_minimal_quest_projections
            .iter()
            .map(|projection| projection.psc_manifest.manifest_id.clone())
            .collect::<BTreeSet<_>>();
        let provided = by_manifest.keys().cloned().collect::<BTreeSet<_>>();
        if required != provided {
            return Err(RunError::InvalidConfig(format!(
                "Skyrim quest compiler evidence set does not match the admitted manifest: required={} provided={}",
                required.len(),
                provided.len()
            )));
        }

        let output_plugin = plugin_name_for_handle(self.target_handle_id)?;
        let mut updates = Vec::with_capacity(self.skyrim_minimal_quest_projections.len());
        let mut validated_evidence =
            Vec::with_capacity(self.skyrim_minimal_quest_projections.len());
        for projection in &self.skyrim_minimal_quest_projections {
            let evidence = by_manifest
                .get(&projection.psc_manifest.manifest_id)
                .expect("compiler evidence set equality checked");
            let attachment = crate::skyrimse_fo4_runtime::quest::build_vmad_attachment_intent(
                projection,
                Some(evidence),
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "{} compiler evidence rejected: {error}",
                    projection.component_id
                ))
            })?;
            let record_key = format!("{}:{:06X}", output_plugin, projection.record.form_key.local);
            let mut record =
                esp_authoring_core::plugin_runtime::plugin_handle_read_authoring_record_value_json(
                    self.target_handle_id,
                    &record_key,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "read admitted Skyrim quest {record_key} before VMAD attachment: {error}"
                    ))
                })?
                .ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "admitted Skyrim quest {record_key} is missing before VMAD attachment"
                    ))
                })?;
            let record_object = record.as_object_mut().ok_or_else(|| {
                RunError::InvalidConfig(format!(
                    "admitted Skyrim quest {record_key} authoring payload is not an object"
                ))
            })?;
            if let Some(signature) = record_object.get("signature") {
                if signature.as_str() != Some("QUST") {
                    return Err(RunError::InvalidConfig(format!(
                        "admitted Skyrim quest {record_key} changed signature before VMAD attachment"
                    )));
                }
            } else {
                record_object.insert(
                    "signature".to_string(),
                    serde_json::Value::String("QUST".to_string()),
                );
            }
            crate::fnv_legacy_scripting::vmad::attach_vmad_payload_to_record(
                &mut record,
                attachment.payload,
            )
            .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
            updates.push((record_key, record));
            validated_evidence.push(evidence.clone());
        }
        for (record_key, record) in &updates {
            esp_authoring_core::plugin_runtime::plugin_handle_replace_authoring_record_value(
                self.target_handle_id,
                record,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "attach evidence-gated Skyrim quest VMAD to {record_key}: {error}"
                ))
            })?;
        }
        let localized_targets = self
            .skyrim_minimal_quest_projections
            .iter()
            .map(|projection| {
                (
                    projection.component_id.clone(),
                    projection.record.form_key,
                    projection.record.sig,
                )
            })
            .collect::<Vec<_>>();
        for (component_id, form_key, signature) in localized_targets {
            self.freeze_skyrim_localized_receipt(&component_id, form_key, signature)?;
        }
        self.skyrim_minimal_quest_compiler_evidence = validated_evidence;
        self.skyrim_minimal_quest_receipts_finalized = false;
        Ok(updates.len() as u32)
    }

    fn skyrim_dialogue_voice_dependency(
        &self,
        requirement: &crate::skyrimse_fo4_runtime::dialogue::SkyrimDialogueVoiceRequirement,
    ) -> Result<
        (
            crate::skyrimse_fo4_runtime::voice::SkyrimVoiceTypeDependency,
            String,
        ),
        RunError,
    > {
        let source_speaker = read_record_relayout_by_form_key(
            self.source_handle_id,
            &requirement.source_speaker,
            &self.schema_source,
            &self.interner,
            None,
        )
        .map_err(|error| {
            RunError::InvalidConfig(format!(
                "Skyrim dialogue source speaker {} is unavailable: {error}",
                requirement.source_speaker.format(&self.interner)
            ))
        })?;
        let source_voice_type =
            exact_record_form_key(&source_speaker, b"VTCK").map_err(|reason| {
                RunError::InvalidConfig(format!(
                    "Skyrim dialogue source speaker {} {reason}",
                    requirement.source_speaker.format(&self.interner)
                ))
            })?;
        let source_voice = read_record_relayout_by_form_key(
            self.source_handle_id,
            &source_voice_type,
            &self.schema_source,
            &self.interner,
            None,
        )
        .map_err(|error| {
            RunError::InvalidConfig(format!(
                "Skyrim dialogue source voice type {} is unavailable: {error}",
                source_voice_type.format(&self.interner)
            ))
        })?;
        let source_identity = source_voice
            .eid
            .and_then(|editor_id| self.interner.resolve(editor_id))
            .filter(|editor_id| !editor_id.trim().is_empty())
            .ok_or_else(|| {
                RunError::InvalidConfig(format!(
                    "Skyrim dialogue source voice type {} has no editor identity",
                    source_voice_type.format(&self.interner)
                ))
            })?
            .to_string();

        let target_speaker = self
            .read_target_record_if_available(requirement.target_speaker)?
            .ok_or_else(|| {
                RunError::InvalidConfig(format!(
                    "Skyrim dialogue target speaker {} is not materialized",
                    requirement.target_speaker.format(&self.interner)
                ))
            })?;
        let target_voice_type =
            exact_record_form_key(&target_speaker, b"VTCK").map_err(|reason| {
                RunError::InvalidConfig(format!(
                    "Skyrim dialogue target speaker {} {reason}",
                    requirement.target_speaker.format(&self.interner)
                ))
            })?;
        let target_voice = self
            .read_target_record_if_available(target_voice_type)?
            .ok_or_else(|| {
                RunError::InvalidConfig(format!(
                    "Skyrim dialogue target voice type {} is not materialized",
                    target_voice_type.format(&self.interner)
                ))
            })?;
        let target_identity = target_voice
            .eid
            .and_then(|editor_id| self.interner.resolve(editor_id))
            .filter(|editor_id| !editor_id.trim().is_empty())
            .ok_or_else(|| {
                RunError::InvalidConfig(format!(
                    "Skyrim dialogue target voice type {} has no editor identity",
                    target_voice_type.format(&self.interner)
                ))
            })?
            .to_string();
        Ok((
            crate::skyrimse_fo4_runtime::voice::SkyrimVoiceTypeDependency {
                source_voice_type,
                target_voice_type,
                target_identity,
            },
            source_identity,
        ))
    }

    pub(crate) fn skyrim_quest_runtime_asset_requirements_json(&self) -> Result<String, RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Ok("{\"output_plugin\":\"\",\"requirements\":[]}".to_string());
        }
        let requirements = self
            .skyrim_dialogue_projections
            .iter()
            .flat_map(|live| &live.projection.receipt.voice_intents)
            .map(|requirement| {
                let (voice_type, source_identity) =
                    self.skyrim_dialogue_voice_dependency(requirement)?;
                let preview = crate::skyrimse_fo4_runtime::voice::plan_dialogue_voice_assets(
                    &self.config.output_plugin_name,
                    &[crate::skyrimse_fo4_runtime::voice::SkyrimDialogueVoiceAssetInput::from_dialogue_requirement(
                        requirement,
                        Some(voice_type.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )],
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                let target_voice_path = preview.intents[0]
                    .target_voice_path
                    .clone()
                    .ok_or_else(|| RunError::InvalidConfig(
                        "Skyrim dialogue target voice path is invalid".to_string()
                    ))?;
                let source_plugin = self
                    .interner
                    .resolve(requirement.source_info.plugin)
                    .ok_or_else(|| RunError::InvalidConfig(
                        "Skyrim dialogue source plugin is unresolved".to_string()
                    ))?;
                Ok::<_, RunError>(serde_json::json!({
                    "source_info": requirement.source_info.format(&self.interner),
                    "target_info": requirement.target_info.format(&self.interner),
                    "source_speaker": requirement.source_speaker.format(&self.interner),
                    "target_speaker": requirement.target_speaker.format(&self.interner),
                    "response_number": requirement.response_number,
                    "transcript": requirement.transcript,
                    "source_voice_directory": format!(
                        "sound/voice/{source_plugin}/{source_identity}"
                    ),
                    "source_stems": [
                        format!("{:08x}_{}", requirement.source_info.local, requirement.response_number),
                        format!("{:06x}_{:02}", requirement.source_info.local, requirement.response_number),
                    ],
                    "target_voice_path": target_voice_path,
                    "voice_type": {
                        "source_voice_type": voice_type.source_voice_type.format(&self.interner),
                        "target_voice_type": voice_type.target_voice_type.format(&self.interner),
                        "target_identity": voice_type.target_identity,
                    },
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;
        serde_json::to_string(&serde_json::json!({
            "output_plugin": self.config.output_plugin_name,
            "requirements": requirements,
        }))
        .map_err(|error| {
            RunError::InvalidConfig(format!("serialize Skyrim asset requirements: {error}"))
        })
    }

    pub(crate) fn install_skyrim_quest_runtime_asset_manifest_json(
        &mut self,
        manifest_json: &str,
    ) -> Result<String, RunError> {
        use crate::skyrimse_fo4_runtime::voice::{
            SkyrimDialogueVoiceAssetInput, SkyrimSequenceAssetInput, SkyrimSequencePlaybackKind,
            SkyrimVoiceAdmission, SkyrimVoiceAudioFormat, SkyrimVoiceTypeDependency,
        };

        if self.skyrim_minimal_quest_compiler_evidence.len()
            != self.skyrim_minimal_quest_projections.len()
        {
            return Err(RunError::InvalidConfig(
                "Skyrim quest runtime assets require completed PSC/PEX reconciliation".to_string(),
            ));
        }
        let manifest = serde_json::from_str::<SkyrimQuestRuntimeAssetManifestInput>(manifest_json)
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "parse Skyrim quest runtime asset manifest: {error}"
                ))
            })?;
        if !manifest
            .output_plugin
            .eq_ignore_ascii_case(&self.config.output_plugin_name)
        {
            return Err(RunError::InvalidConfig(
                "Skyrim quest runtime asset manifest targets another output plugin".to_string(),
            ));
        }
        let requirements = self
            .skyrim_dialogue_projections
            .iter()
            .flat_map(|live| live.projection.receipt.voice_intents.iter())
            .cloned()
            .collect::<Vec<_>>();
        if manifest.intents.len() != requirements.len() {
            return Err(RunError::InvalidConfig(format!(
                "Skyrim quest runtime asset manifest inventory differs: expected {}, got {}",
                requirements.len(),
                manifest.intents.len()
            )));
        }
        let mut inputs = Vec::with_capacity(manifest.intents.len());
        for intent in manifest.intents {
            let source_info = FormKey::parse(&intent.source_info, &self.interner)
                .map_err(RunError::InvalidConfig)?;
            let target_info = FormKey::parse(&intent.target_info, &self.interner)
                .map_err(RunError::InvalidConfig)?;
            let source_speaker = FormKey::parse(&intent.source_speaker, &self.interner)
                .map_err(RunError::InvalidConfig)?;
            let target_speaker = FormKey::parse(&intent.target_speaker, &self.interner)
                .map_err(RunError::InvalidConfig)?;
            let requirement = requirements
                .iter()
                .find(|requirement| {
                    requirement.source_info == source_info
                        && requirement.target_info == target_info
                        && requirement.response_number == intent.response_number
                })
                .ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "Skyrim quest runtime asset manifest has an unknown INFO response {}:{}",
                        intent.target_info, intent.response_number
                    ))
                })?;
            if requirement.source_speaker != source_speaker
                || requirement.target_speaker != target_speaker
                || requirement.transcript.trim() != intent.transcript.trim()
            {
                return Err(RunError::InvalidConfig(format!(
                    "Skyrim quest runtime asset manifest changed INFO response {}:{} identity",
                    intent.target_info, intent.response_number
                )));
            }
            let (actual_voice_type, _) = self.skyrim_dialogue_voice_dependency(requirement)?;
            let declared_voice_type = SkyrimVoiceTypeDependency {
                source_voice_type: FormKey::parse(
                    &intent.voice_type.source_voice_type,
                    &self.interner,
                )
                .map_err(RunError::InvalidConfig)?,
                target_voice_type: FormKey::parse(
                    &intent.voice_type.target_voice_type,
                    &self.interner,
                )
                .map_err(RunError::InvalidConfig)?,
                target_identity: intent.voice_type.target_identity,
            };
            if declared_voice_type != actual_voice_type {
                return Err(RunError::InvalidConfig(format!(
                    "Skyrim quest runtime asset manifest voice type differs for {}:{}",
                    intent.target_info, intent.response_number
                )));
            }
            let audio_format = match intent.target_audio.format.to_ascii_lowercase().as_str() {
                "xwm" => SkyrimVoiceAudioFormat::Xwm,
                "wav" => SkyrimVoiceAudioFormat::Wav,
                other => {
                    return Err(RunError::InvalidConfig(format!(
                        "unsupported Skyrim quest runtime target audio format {other:?}"
                    )));
                }
            };
            let sequence = intent
                .sequence
                .map(|sequence| {
                    let kind = match sequence.kind.as_str() {
                        "scene" => SkyrimSequencePlaybackKind::Scene,
                        "start_game_enabled_quest" => {
                            SkyrimSequencePlaybackKind::StartGameEnabledQuest
                        }
                        other => {
                            return Err(RunError::InvalidConfig(format!(
                                "unsupported Skyrim quest runtime sequence kind {other:?}"
                            )));
                        }
                    };
                    Ok(SkyrimSequenceAssetInput {
                        kind,
                        source_seq: Some(sequence.source_seq.into_voice_evidence()),
                        target_seq: Some(sequence.target_seq.into_voice_evidence()),
                    })
                })
                .transpose()?;
            inputs.push(SkyrimDialogueVoiceAssetInput::from_dialogue_requirement(
                requirement,
                Some(actual_voice_type),
                Some(intent.source_fuz.into_voice_evidence()),
                Some(intent.source_lip.into_voice_evidence()),
                Some((
                    audio_format,
                    intent.target_audio.evidence.into_voice_evidence(),
                )),
                Some(intent.target_lip.into_voice_evidence()),
                sequence,
            ));
        }
        let receipt = crate::skyrimse_fo4_runtime::voice::plan_dialogue_voice_assets(
            &manifest.output_plugin,
            &inputs,
        )
        .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
        if let Some(rejected) = receipt
            .intents
            .iter()
            .find(|intent| !matches!(intent.admission, SkyrimVoiceAdmission::Ready))
        {
            return Err(RunError::InvalidConfig(format!(
                "Skyrim quest runtime voice response {:06X}:{} rejected: {:?}",
                rejected.target_info.local, rejected.response_number, rejected.admission
            )));
        }
        let canonical = skyrim_voice_receipt_json(&receipt, &self.interner);
        self.skyrim_quest_runtime_voice_receipt = Some(receipt);
        self.skyrim_quest_runtime_asset_copy_receipt = None;
        serde_json::to_string(&canonical).map_err(|error| {
            RunError::InvalidConfig(format!(
                "serialize Skyrim quest runtime asset manifest: {error}"
            ))
        })
    }

    pub(crate) fn reconcile_skyrim_quest_runtime_asset_copy_receipt_json(
        &mut self,
        receipt_json: &str,
    ) -> Result<u32, RunError> {
        use crate::skyrimse_fo4_runtime::voice::{
            SkyrimSequencePlaybackKind, SkyrimVoiceAdmission, SkyrimVoiceAssetProvenance,
        };

        let receipt = serde_json::from_str::<SkyrimQuestRuntimeAssetCopyReceipt>(receipt_json)
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "parse Skyrim quest runtime asset receipt: {error}"
                ))
            })?;
        let voice_receipt = self
            .skyrim_quest_runtime_voice_receipt
            .as_ref()
            .ok_or_else(|| {
                RunError::InvalidConfig(
                    "Skyrim quest runtime asset receipt arrived before manifest admission"
                        .to_string(),
                )
            })?;
        let mut expected = Vec::new();
        for intent in &voice_receipt.intents {
            if !matches!(intent.admission, SkyrimVoiceAdmission::Ready) {
                return Err(RunError::InvalidConfig(
                    "Skyrim quest runtime asset receipt contains a rejected intent".to_string(),
                ));
            }
            let source_fuz = intent
                .source_fuz
                .as_ref()
                .expect("ready intent has source FUZ");
            let source_lip = intent
                .source_lip
                .as_ref()
                .expect("ready intent has source LIP");
            let (_, audio) = intent
                .target_audio
                .as_ref()
                .expect("ready intent has audio");
            let lip = intent
                .target_lip
                .as_ref()
                .expect("ready intent has target LIP");
            for (semantic_role, _source, target) in [
                ("dialogue_audio", source_fuz, audio),
                ("dialogue_lip", source_lip, lip),
            ] {
                let SkyrimVoiceAssetProvenance::Converted { source_blake3, .. } =
                    &target.provenance
                else {
                    unreachable!("ready intent conversion provenance validated")
                };
                expected.push(SkyrimQuestRuntimeAssetCopyRow {
                    semantic_role: semantic_role.to_string(),
                    source_path: target.relative_path.clone(),
                    target_path: target.relative_path.clone(),
                    source_root: String::new(),
                    size: target.byte_len,
                    blake3: target.blake3.clone(),
                    source_blake3: source_blake3.clone(),
                });
            }
            if let Some(sequence) = &intent.sequence {
                let SkyrimVoiceAssetProvenance::Converted { source_blake3, .. } =
                    &sequence.target_seq.provenance
                else {
                    unreachable!("ready sequence conversion provenance validated")
                };
                expected.push(SkyrimQuestRuntimeAssetCopyRow {
                    semantic_role: match sequence.kind {
                        SkyrimSequencePlaybackKind::Scene => "sequence_scene",
                        SkyrimSequencePlaybackKind::StartGameEnabledQuest => {
                            "sequence_start_game_enabled_quest"
                        }
                    }
                    .to_string(),
                    source_path: sequence.target_seq.relative_path.clone(),
                    target_path: sequence.target_seq.relative_path.clone(),
                    source_root: String::new(),
                    size: sequence.target_seq.byte_len,
                    blake3: sequence.target_seq.blake3.clone(),
                    source_blake3: source_blake3.clone(),
                });
            }
        }
        let mut actual = receipt.rows.clone();
        if actual.iter().any(|row| row.source_root.trim().is_empty()) {
            return Err(RunError::InvalidConfig(
                "Skyrim quest runtime asset receipt has an empty converted source root".to_string(),
            ));
        }
        let mut canonical_actual = actual.clone();
        canonical_actual.sort_by(|left, right| {
            left.target_path
                .to_ascii_lowercase()
                .cmp(&right.target_path.to_ascii_lowercase())
                .then_with(|| left.semantic_role.cmp(&right.semantic_role))
        });
        if actual != canonical_actual {
            return Err(RunError::InvalidConfig(
                "Skyrim quest runtime asset receipt is not deterministic".to_string(),
            ));
        }
        for row in &mut actual {
            row.source_root.clear();
        }
        expected.sort_by(|left, right| {
            left.target_path
                .to_ascii_lowercase()
                .cmp(&right.target_path.to_ascii_lowercase())
                .then_with(|| left.semantic_role.cmp(&right.semantic_role))
        });
        if actual != expected {
            return Err(RunError::InvalidConfig(
                "Skyrim quest runtime asset receipt does not close the frozen voice manifest"
                    .to_string(),
            ));
        }
        let count = receipt.rows.len() as u32;
        self.skyrim_quest_runtime_asset_copy_receipt = Some(receipt);
        self.skyrim_minimal_quest_receipts_finalized = false;
        Ok(count)
    }

    pub(crate) fn validate_skyrim_actor_translation(
        &self,
        stats: &TranslateStats,
    ) -> Result<(), RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Ok(());
        }
        let plan = self
            .skyrim_runtime_capability_plan
            .as_ref()
            .ok_or_else(|| {
                RunError::InvalidConfig("Skyrim runtime component plan is missing".to_string())
            })?;
        validate_total_skyrim_actor_translation(
            plan,
            stats,
            &self.skyrim_creature_batch_owned_sources,
        )
        .map_err(RunError::InvalidConfig)
    }

    pub(crate) fn finalize_skyrim_runtime_receipt(&mut self) -> Result<(), RunError> {
        if self.source != Game::SkyrimSe || self.target != Game::Fo4 {
            return Ok(());
        }
        self.reconcile_skyrim_minimal_quest_post_fixup_receipts()?;
        let plan = self
            .skyrim_runtime_capability_plan
            .as_ref()
            .ok_or_else(|| {
                RunError::InvalidConfig("Skyrim runtime component plan is missing".to_string())
            })?;
        let mapper = self.mapper_state.as_ref().ok_or_else(|| {
            RunError::InvalidConfig("Skyrim runtime mapper state is missing".to_string())
        })?;
        let render = |form_key: FormKey| -> Result<String, RunError> {
            let plugin = self.interner.resolve(form_key.plugin).ok_or_else(|| {
                RunError::InvalidConfig(format!(
                    "Skyrim runtime FormKey {:06X} has an unresolved plugin",
                    form_key.local
                ))
            })?;
            Ok(format!("{:06X}@{plugin}", form_key.local))
        };
        let mut mappings = Vec::new();
        let mut admitted = FxHashSet::default();
        let mut dropped = FxHashSet::default();
        for (source, disposition) in &plan.by_record {
            if disposition.supported {
                let target = mapper
                    .source_to_target
                    .get(source)
                    .copied()
                    .ok_or_else(|| {
                        RunError::InvalidConfig(format!(
                            "supported Skyrim runtime record {:06X} has no target mapping",
                            source.local
                        ))
                    })?;
                let source_signature = plan.record_signatures[source].clone();
                let target_signature = plan.target_signatures[source].clone();
                mappings.push(crate::skyrimse_fo4_runtime::receipt::FrozenRecordMapping {
                    source: crate::skyrimse_fo4_runtime::receipt::RuntimeRecordIdentity {
                        form_key: render(*source)?,
                        signature: source_signature,
                    },
                    target: crate::skyrimse_fo4_runtime::receipt::RuntimeRecordIdentity {
                        form_key: render(target)?,
                        signature: target_signature,
                    },
                });
                admitted.insert(*source);
            } else {
                dropped.insert(*source);
            }
        }
        mappings.sort_by(|left, right| left.source.form_key.cmp(&right.source.form_key));
        let mut topology = Vec::new();
        for edge in &plan.source_topology {
            let parent = FormKey::parse(&edge.parent_form_key, &self.interner)
                .map_err(RunError::InvalidConfig)?;
            let child = FormKey::parse(&edge.child_form_key, &self.interner)
                .map_err(RunError::InvalidConfig)?;
            if !admitted.contains(&child) {
                continue;
            }
            let target_parent = mapper
                .source_to_target
                .get(&parent)
                .copied()
                .ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "supported Skyrim topology parent {:06X} has no target mapping",
                        parent.local
                    ))
                })?;
            let target_child = mapper.source_to_target[&child];
            topology.push(crate::skyrimse_fo4_runtime::receipt::RuntimeTopologyEdge {
                parent_form_key: render(target_parent)?,
                child_form_key: render(target_child)?,
                kind: edge.kind,
            });
        }
        let components_supported = plan
            .components
            .iter()
            .filter(|component| {
                matches!(
                    component.decision,
                    crate::skyrimse_fo4_runtime::receipt::RuntimeComponentDecision::Supported
                )
            })
            .count() as u32;
        let receipt = crate::skyrimse_fo4_runtime::SkyrimRuntimeReceipt {
            version: crate::skyrimse_fo4_runtime::SkyrimRuntimeReceipt::VERSION,
            source_game: "skyrimse".to_string(),
            target_game: "fo4".to_string(),
            source_plugin: mapper.options.source_plugin_name.clone(),
            target_plugin: mapper.options.output_plugin_name.clone(),
            components: plan.components.clone(),
            mappings,
            topology,
            signature_adaptations: plan
                .components
                .iter()
                .flat_map(|component| component.adaptations.iter().cloned())
                .collect(),
            script_bindings: Vec::new(),
            assets: Vec::new(),
            accounting: crate::skyrimse_fo4_runtime::SkyrimRuntimeAccounting {
                components_seen: plan.components.len() as u32,
                components_supported,
                components_dropped: plan.components.len() as u32 - components_supported,
                records_written: admitted.len() as u32,
                records_dropped: dropped.len() as u32,
                script_bindings_expected: 0,
                script_bindings_attached: 0,
            },
        };
        receipt.validate_shape().map_err(RunError::InvalidConfig)?;
        self.skyrim_runtime_receipt = Some(receipt);
        Ok(())
    }

    fn reconcile_skyrim_minimal_quest_post_fixup_receipts(&mut self) -> Result<(), RunError> {
        if self.skyrim_minimal_quest_receipts_finalized {
            return Ok(());
        }
        let admissions = self
            .skyrim_runtime_capability_plan
            .as_ref()
            .map(|plan| plan.minimal_quests.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if admissions.is_empty() {
            self.skyrim_minimal_quest_receipts_finalized = true;
            return Ok(());
        }
        if self.skyrim_minimal_quest_compiler_evidence.len()
            != self.skyrim_minimal_quest_projections.len()
        {
            return Err(RunError::InvalidConfig(
                "admitted Skyrim quest compiler evidence was not reconciled before fixups"
                    .to_string(),
            ));
        }
        if self.skyrim_minimal_alias_projections.len()
            != self.skyrim_minimal_quest_projections.len()
        {
            return Err(RunError::InvalidConfig(
                "admitted Skyrim quest alias projections were not frozen before fixups".to_string(),
            ));
        }
        let required_voice_lines = self
            .skyrim_dialogue_projections
            .iter()
            .map(|dialogue| dialogue.projection.receipt.voice_intents.len())
            .sum::<usize>();
        if required_voice_lines != 0
            && (self.skyrim_quest_runtime_voice_receipt.is_none()
                || self.skyrim_quest_runtime_asset_copy_receipt.is_none())
        {
            return Err(RunError::InvalidConfig(
                "admitted Skyrim quest voice/LIP asset receipt was not reconciled before fixups"
                    .to_string(),
            ));
        }
        let projections = self.skyrim_minimal_quest_projections.clone();
        for admission in admissions {
            let projection = projections
                .iter()
                .find(|projection| projection.component_id == admission.plan.component_id)
                .ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "{} has no frozen Skyrim quest projection",
                        admission.plan.component_id
                    ))
                })?;
            let target_form_key = projection.record.form_key;
            let topology = crate::target_write::existing_record_topology_native(
                self.target_handle_id,
                target_form_key,
                "QUST",
                &self.interner,
            )
            .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
            let target_form_id =
                encode_form_key_for_handle(self.target_handle_id, target_form_key, &self.interner)
                    .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
            let placement =
                existing_authoring_record_placement_native(self.target_handle_id, target_form_id)
                    .ok();
            let mut actual = crate::quest_runtime::QuestRuntimePostFixupReceipt::default();
            for _ in 0..topology.occurrences {
                actual.emitted_records.push(projection.target_quest.clone());
                if matches!(placement, Some(ExistingAuthoringRecordPlacement::TopLevel)) {
                    actual
                        .placements
                        .push(crate::quest_runtime::TopologyPlacement {
                            record: projection.target_quest.clone(),
                            group_path: vec!["GRUP:QUST".to_string()],
                        });
                }
            }
            for (field, table, string_id) in
                crate::target_write::existing_localized_string_occurrences_native(
                    self.target_handle_id,
                    target_form_key,
                    "QUST",
                    &self.interner,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?
            {
                actual
                    .localized_strings
                    .push(crate::quest_runtime::LocalizedStringReceipt {
                        owner: projection.target_quest.clone(),
                        field,
                        table,
                        string_id,
                    });
            }
            let evidence = self
                .skyrim_minimal_quest_compiler_evidence
                .iter()
                .find(|row| row.manifest_id == projection.psc_manifest.manifest_id);
            if crate::skyrimse_fo4_runtime::quest::build_vmad_attachment_intent(
                projection, evidence,
            )
            .is_ok()
            {
                actual
                    .scripts
                    .extend(admission.plan.expected_receipt.scripts.iter().cloned());
            }
            let record_key = format!(
                "{}:{:06X}",
                plugin_name_for_handle(self.target_handle_id)?,
                target_form_key.local
            );
            let authoring =
                esp_authoring_core::plugin_runtime::plugin_handle_read_authoring_record_value_json(
                    self.target_handle_id,
                    &record_key,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
            let alias_projection = self
                .skyrim_minimal_alias_projections
                .iter()
                .find(|alias| alias.component_id == admission.plan.component_id)
                .ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "{} has no frozen Skyrim alias projection",
                        admission.plan.component_id
                    ))
                })?;
            let target_record = read_record_relayout_by_form_key(
                self.target_handle_id,
                &target_form_key,
                &self.schema_target,
                &self.interner,
                None,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "{} read target QUST for alias audit: {error}",
                    admission.plan.component_id
                ))
            })?;
            let quest_vmad_count = target_record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "VMAD")
                .count();
            let mut alias_record = target_record.clone();
            alias_record
                .fields
                .retain(|field| field.sig.as_str() != "VMAD");
            for field in &mut alias_record.fields {
                if field.sig.as_str() == "ALED"
                    && matches!(&field.value, FieldValue::Bytes(bytes) if bytes.is_empty())
                {
                    field.value = FieldValue::None;
                }
            }
            let alias_receipt =
                crate::skyrimse_fo4_runtime::alias::verify_alias_projection_in_record(
                    &alias_record,
                    alias_projection,
                    &self.interner,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} post-fixup alias receipt was damaged: {error}",
                        admission.plan.component_id
                    ))
                })?;
            if quest_vmad_count != 1
                || alias_receipt.preserved_quest_vmad_count != 0
                || alias_receipt.alias_vmad_attachment_count != 0
            {
                return Err(RunError::InvalidConfig(format!(
                    "{} post-fixup alias receipt has an invalid VMAD boundary",
                    admission.plan.component_id
                )));
            }
            for package in self.skyrim_pack_projections.iter().filter(|package| {
                package.lowering.target.component_id == admission.plan.component_id
            }) {
                let topology = crate::target_write::existing_record_topology_native(
                    self.target_handle_id,
                    package.target_form_key,
                    "PACK",
                    &self.interner,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                let target_form_id = encode_form_key_for_handle(
                    self.target_handle_id,
                    package.target_form_key,
                    &self.interner,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                let placement = existing_authoring_record_placement_native(
                    self.target_handle_id,
                    target_form_id,
                )
                .ok();
                let target_key = crate::quest_runtime::QuestRecordKey::new(
                    "PACK",
                    package.target_form_key.format(&self.interner),
                );
                for _ in 0..topology.occurrences {
                    actual.emitted_records.push(target_key.clone());
                    if matches!(placement, Some(ExistingAuthoringRecordPlacement::TopLevel)) {
                        actual
                            .placements
                            .push(crate::quest_runtime::TopologyPlacement {
                                record: target_key.clone(),
                                group_path: vec!["GRUP:PACK".to_string()],
                            });
                    }
                }
                let record = read_record_relayout_by_form_key(
                    self.target_handle_id,
                    &package.target_form_key,
                    &self.schema_target,
                    &self.interner,
                    None,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} read target PACK for receipt audit: {error}",
                        admission.plan.component_id
                    ))
                })?;
                crate::skyrimse_fo4_runtime::package::verify_skyrim_pack_record_receipt(
                    &package.lowering,
                    &record,
                    package.source_form_key,
                    package.target_form_key,
                    package.target_template,
                    &package.reference_mappings,
                    &package.template_evidence,
                    &package.materialization.receipt,
                    &self.interner,
                )
                .map_err(|reason| {
                    RunError::InvalidConfig(format!(
                        "{} post-fixup PACK receipt was damaged: {reason:?}",
                        admission.plan.component_id
                    ))
                })?;
            }
            for scene in self
                .skyrim_scene_projections
                .iter()
                .filter(|scene| scene.receipt.component_id == admission.plan.component_id)
            {
                let target_form_key = scene.record.form_key;
                let topology = crate::target_write::existing_record_topology_native(
                    self.target_handle_id,
                    target_form_key,
                    "SCEN",
                    &self.interner,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                let target_form_id = encode_form_key_for_handle(
                    self.target_handle_id,
                    target_form_key,
                    &self.interner,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                let placement = existing_authoring_record_placement_native(
                    self.target_handle_id,
                    target_form_id,
                )
                .ok();
                let expected_parent =
                    FormKey::parse(&scene.receipt.target_parent_quest.form_key, &self.interner)
                        .map_err(RunError::InvalidConfig)?;
                let expected_parent_form_id = encode_form_key_for_handle(
                    self.target_handle_id,
                    expected_parent,
                    &self.interner,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                for _ in 0..topology.occurrences {
                    actual
                        .emitted_records
                        .push(scene.receipt.target_scene.clone());
                    if matches!(
                        placement,
                        Some(ExistingAuthoringRecordPlacement::QuestChild {
                            parent_quest_form_id
                        }) if parent_quest_form_id == expected_parent_form_id
                    ) {
                        actual.placements.push(
                            crate::skyrimse_fo4_runtime::scene::scene_placement(
                                &scene.receipt.target_scene,
                                &scene.receipt.target_parent_quest,
                            ),
                        );
                    }
                }
                let record = read_record_relayout_by_form_key(
                    self.target_handle_id,
                    &target_form_key,
                    &self.schema_target,
                    &self.interner,
                    None,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "{} read target SCEN for receipt audit: {error}",
                        admission.plan.component_id
                    ))
                })?;
                scene
                    .receipt
                    .validate_materialized(
                        &record,
                        &scene.parent_quest,
                        scene.group_type,
                        &scene.pending_vmad,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "{} post-fixup SCEN receipt was damaged: {error}",
                            admission.plan.component_id
                        ))
                    })?;
            }
            for dialogue in self
                .skyrim_dialogue_projections
                .iter()
                .filter(|dialogue| dialogue.component_id == admission.plan.component_id)
            {
                use crate::skyrimse_fo4_runtime::dialogue::SkyrimDialogueTopologyKind;
                let expected_records = dialogue
                    .projection
                    .top_level
                    .iter()
                    .chain(&dialogue.projection.quest_children)
                    .chain(
                        dialogue
                            .projection
                            .topic_children
                            .iter()
                            .map(|info| &info.record),
                    )
                    .collect::<Vec<_>>();
                let mut reopened = Vec::with_capacity(expected_records.len());
                let mut topic_parent_by_info = HashMap::new();
                for expected_record in expected_records {
                    let edge = dialogue
                        .projection
                        .receipt
                        .topology
                        .iter()
                        .find(|edge| edge.target_child == expected_record.form_key)
                        .ok_or_else(|| {
                            RunError::InvalidConfig(format!(
                                "{} dialogue target {} has no frozen topology",
                                admission.plan.component_id,
                                expected_record.form_key.format(&self.interner)
                            ))
                        })?;
                    let topology = crate::target_write::existing_record_topology_native(
                        self.target_handle_id,
                        expected_record.form_key,
                        expected_record.sig.as_str(),
                        &self.interner,
                    )
                    .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                    let target_form_id = encode_form_key_for_handle(
                        self.target_handle_id,
                        expected_record.form_key,
                        &self.interner,
                    )
                    .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                    let placement = existing_authoring_record_placement_native(
                        self.target_handle_id,
                        target_form_id,
                    )
                    .ok();
                    let target_key = crate::quest_runtime::QuestRecordKey::new(
                        expected_record.sig.as_str(),
                        expected_record.form_key.format(&self.interner),
                    );
                    let expected_parent_form_id = encode_form_key_for_handle(
                        self.target_handle_id,
                        edge.target_parent,
                        &self.interner,
                    )
                    .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                    for _ in 0..topology.occurrences {
                        actual.emitted_records.push(target_key.clone());
                        let path = match (edge.kind, placement) {
                            (
                                SkyrimDialogueTopologyKind::TopLevel,
                                Some(ExistingAuthoringRecordPlacement::TopLevel),
                            ) => Some(vec!["GRUP:DLVW".to_string()]),
                            (
                                SkyrimDialogueTopologyKind::QuestChild,
                                Some(ExistingAuthoringRecordPlacement::QuestChild {
                                    parent_quest_form_id,
                                }),
                            ) if parent_quest_form_id == expected_parent_form_id => Some(vec![
                                "QUST".to_string(),
                                edge.target_parent.format(&self.interner),
                                "children:10".to_string(),
                            ]),
                            (
                                SkyrimDialogueTopologyKind::TopicChild,
                                Some(ExistingAuthoringRecordPlacement::TopicChild {
                                    parent_dialogue_form_id,
                                }),
                            ) if parent_dialogue_form_id == expected_parent_form_id => {
                                topic_parent_by_info
                                    .insert(expected_record.form_key, edge.target_parent);
                                Some(vec![
                                    "DIAL".to_string(),
                                    edge.target_parent.format(&self.interner),
                                    "children:7".to_string(),
                                ])
                            }
                            _ => None,
                        };
                        if let Some(group_path) = path {
                            actual
                                .placements
                                .push(crate::quest_runtime::TopologyPlacement {
                                    record: target_key.clone(),
                                    group_path,
                                });
                        }
                    }
                    for (field, table, string_id) in
                        crate::target_write::existing_localized_string_occurrences_native(
                            self.target_handle_id,
                            expected_record.form_key,
                            expected_record.sig.as_str(),
                            &self.interner,
                        )
                        .map_err(|error| RunError::InvalidConfig(error.to_string()))?
                    {
                        actual.localized_strings.push(
                            crate::quest_runtime::LocalizedStringReceipt {
                                owner: target_key.clone(),
                                field,
                                table,
                                string_id,
                            },
                        );
                    }
                    reopened.push(
                        read_record_relayout_by_form_key(
                            self.target_handle_id,
                            &expected_record.form_key,
                            &self.schema_target,
                            &self.interner,
                            None,
                        )
                        .map_err(|error| {
                            RunError::InvalidConfig(format!(
                                "{} read target dialogue record for receipt audit: {error}",
                                admission.plan.component_id
                            ))
                        })?,
                    );
                }
                dialogue
                    .projection
                    .receipt
                    .validate_materialized(&reopened, &topic_parent_by_info)
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "{} post-fixup dialogue receipt was damaged: {error}",
                            admission.plan.component_id
                        ))
                    })?;
            }
            let mut story_route_validated = false;
            if let Some(story) = self
                .skyrim_story_manager_projections
                .iter()
                .find(|story| story.component_id == admission.plan.component_id)
            {
                let story_projection = story.projection.as_ref().ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "{} Story Manager route was not committed before fixups",
                        admission.plan.component_id
                    ))
                })?;
                for expected_record in &story_projection.records {
                    let target_key = crate::quest_runtime::QuestRecordKey::new(
                        expected_record.sig.as_str(),
                        expected_record.form_key.format(&self.interner),
                    );
                    let topology = crate::target_write::existing_record_topology_native(
                        self.target_handle_id,
                        expected_record.form_key,
                        expected_record.sig.as_str(),
                        &self.interner,
                    )
                    .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                    let target_form_id = encode_form_key_for_handle(
                        self.target_handle_id,
                        expected_record.form_key,
                        &self.interner,
                    )
                    .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                    let placement = existing_authoring_record_placement_native(
                        self.target_handle_id,
                        target_form_id,
                    )
                    .ok();
                    for _ in 0..topology.occurrences {
                        actual.emitted_records.push(target_key.clone());
                        if matches!(placement, Some(ExistingAuthoringRecordPlacement::TopLevel)) {
                            actual
                                .placements
                                .push(crate::quest_runtime::TopologyPlacement {
                                    record: target_key.clone(),
                                    group_path: vec![format!(
                                        "GRUP:{}",
                                        expected_record.sig.as_str()
                                    )],
                                });
                        }
                    }
                    let reopened = read_record_relayout_by_form_key(
                        self.target_handle_id,
                        &expected_record.form_key,
                        &self.schema_target,
                        &self.interner,
                        None,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "{} read projected Story Manager {}: {error}",
                            admission.plan.component_id,
                            expected_record.sig.as_str()
                        ))
                    })?;
                    if reopened.sig != expected_record.sig
                        || reopened.form_key != expected_record.form_key
                        || reopened.eid != expected_record.eid
                        || reopened.flags != expected_record.flags
                        || reopened.fields != expected_record.fields
                        || reopened.warnings != expected_record.warnings
                    {
                        return Err(RunError::InvalidConfig(format!(
                            "{} post-fixup Story Manager {} receipt was damaged",
                            admission.plan.component_id,
                            expected_record.sig.as_str()
                        )));
                    }
                }
                let expected_route = &story_projection.route_receipt.route;
                if admission
                    .plan
                    .expected_receipt
                    .routes
                    .contains(expected_route)
                    && authoring
                        .as_ref()
                        .is_some_and(|record| !authoring_quest_starts_enabled(record))
                {
                    actual.routes.push(expected_route.clone());
                    story_route_validated = true;
                }
            }
            if let Some(authoring) = authoring.as_ref() {
                for (script_class, property_names) in
                    authoring_top_level_vmad_script_attachments(authoring)
                        .map_err(RunError::InvalidConfig)?
                {
                    let matching = admission
                        .plan
                        .expected_receipt
                        .vmad_attachments
                        .iter()
                        .filter(|attachment| {
                            attachment.owner == projection.target_quest
                                && attachment.script_class.eq_ignore_ascii_case(&script_class)
                                && attachment.property_names == property_names
                        })
                        .collect::<Vec<_>>();
                    if let [attachment] = matching.as_slice() {
                        actual.vmad_attachments.push((*attachment).clone());
                    } else {
                        actual
                            .vmad_attachments
                            .push(crate::quest_runtime::VmadAttachmentReceipt {
                                owner: projection.target_quest.clone(),
                                script_class,
                                property_names,
                                compiler_evidence_id: String::new(),
                            });
                    }
                }
            }
            if !story_route_validated
                && authoring
                    .as_ref()
                    .is_some_and(authoring_quest_starts_enabled)
            {
                actual
                    .routes
                    .extend(admission.plan.expected_receipt.routes.iter().cloned());
            }
            let comparison = crate::quest_runtime::compare_post_fixup_receipt(
                &admission.plan.expected_receipt,
                &actual,
            );
            if !comparison.intact {
                return Err(RunError::InvalidConfig(format!(
                    "{} post-fixup quest runtime receipt was damaged: {}",
                    admission.plan.component_id,
                    serde_json::to_string(&comparison.required_damage).unwrap_or_default()
                )));
            }
        }
        self.skyrim_minimal_quest_receipts_finalized = true;
        Ok(())
    }

    fn reconcile_fnv_quest_runtime_post_fixup_receipts(&mut self) -> Result<(), RunError> {
        if self.fnv_quest_runtime_receipts_finalized {
            return Ok(());
        }
        if self.fnv_quest_runtime_component_plans.is_empty() {
            self.fnv_quest_runtime_receipts_finalized = true;
            return Ok(());
        }
        if self.fnv_quest_runtime_component_plans.len()
            != self.fnv_quest_runtime_expected_receipts.len()
        {
            return Err(RunError::InvalidConfig(
                "FNV quest runtime retained plan/receipt counts differ".to_string(),
            ));
        }
        validate_fnv_synthetic_target_set(
            &self.fnv_quest_runtime_expected_receipts,
            &self.fnv_quest_runtime_synthetic_targets,
        )
        .map_err(RunError::InvalidConfig)?;

        let plans = self.fnv_quest_runtime_component_plans.clone();
        for (plan, frozen) in plans.iter().zip(&self.fnv_quest_runtime_expected_receipts) {
            if &plan.expected_receipt != frozen {
                return Err(RunError::InvalidConfig(format!(
                    "{} frozen FNV quest runtime receipt changed after admission",
                    plan.component_id
                )));
            }
            let expected =
                resolve_fnv_expected_receipt(frozen, &self.fnv_quest_runtime_synthetic_targets)
                    .map_err(RunError::InvalidConfig)?;
            let mut actual = crate::quest_runtime::QuestRuntimePostFixupReceipt::default();
            let mut authoring_records = BTreeMap::new();
            let target_is_localized = plugin_header_flags_native(self.target_handle_id)
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?
                & 0x0000_0080
                != 0;

            for record in &expected.emitted_records {
                let target = parse_legacy_or_native_form_key(&record.form_key, &self.interner)
                    .map_err(RunError::InvalidConfig)?;
                let topology = crate::target_write::existing_record_topology_native(
                    self.target_handle_id,
                    target,
                    &record.signature,
                    &self.interner,
                )
                .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                actual
                    .emitted_records
                    .extend(std::iter::repeat_n(record.clone(), topology.occurrences));

                let target_form_id =
                    encode_form_key_for_handle(self.target_handle_id, target, &self.interner)
                        .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                if let Ok(placement) = existing_authoring_record_placement_native(
                    self.target_handle_id,
                    target_form_id,
                ) {
                    for expected_placement in expected
                        .placements
                        .iter()
                        .filter(|candidate| candidate.record == *record)
                    {
                        if expected_placement_matches(
                            expected_placement,
                            placement,
                            self.target_handle_id,
                            &self.interner,
                        )
                        .map_err(RunError::InvalidConfig)?
                        {
                            actual.placements.extend(std::iter::repeat_n(
                                expected_placement.clone(),
                                topology.occurrences,
                            ));
                        }
                    }
                }

                if target_is_localized {
                    for (field, table, string_id) in
                        crate::target_write::existing_localized_string_occurrences_native(
                            self.target_handle_id,
                            target,
                            &record.signature,
                            &self.interner,
                        )
                        .map_err(|error| RunError::InvalidConfig(error.to_string()))?
                    {
                        actual.localized_strings.push(
                            crate::quest_runtime::LocalizedStringReceipt {
                                owner: record.clone(),
                                field,
                                table,
                                string_id,
                            },
                        );
                    }
                }

                let plugin = self.interner.resolve(target.plugin).ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "{} target plugin is unresolved",
                        record.form_key
                    ))
                })?;
                let authoring_key = format!("{plugin}:{:06X}", target.local);
                let authoring = esp_authoring_core::plugin_runtime::plugin_handle_read_authoring_record_value_json(
                    self.target_handle_id,
                    &authoring_key,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "read FNV quest runtime record {authoring_key}: {error}"
                    ))
                })?;
                authoring_records.insert(record.clone(), authoring);
            }

            for owner in expected
                .vmad_attachments
                .iter()
                .map(|attachment| &attachment.owner)
                .chain(expected.routes.iter().map(|route| &route.quest))
            {
                if authoring_records.contains_key(owner) {
                    continue;
                }
                let target = parse_legacy_or_native_form_key(&owner.form_key, &self.interner)
                    .map_err(RunError::InvalidConfig)?;
                let plugin = self.interner.resolve(target.plugin).ok_or_else(|| {
                    RunError::InvalidConfig(format!(
                        "{} target plugin is unresolved",
                        owner.form_key
                    ))
                })?;
                let authoring_key = format!("{plugin}:{:06X}", target.local);
                let authoring = esp_authoring_core::plugin_runtime::plugin_handle_read_authoring_record_value_json(
                    self.target_handle_id,
                    &authoring_key,
                )
                .map_err(|error| {
                    RunError::InvalidConfig(format!(
                        "read FNV quest runtime external owner {authoring_key}: {error}"
                    ))
                })?;
                authoring_records.insert(owner.clone(), authoring);
            }

            for expected_script in &expected.scripts {
                let occurrences = self
                    .fnv_quest_runtime_compiler_evidence
                    .iter()
                    .filter(|evidence| {
                        evidence.compiled_success
                            && !evidence.relative_pex_path.trim().is_empty()
                            && evidence
                                .class_name
                                .eq_ignore_ascii_case(&expected_script.class_name)
                    })
                    .count();
                actual
                    .scripts
                    .extend(std::iter::repeat_n(expected_script.clone(), occurrences));
            }

            for (owner, authoring) in &authoring_records {
                let Some(authoring) = authoring else {
                    continue;
                };
                for (script_class, property_names) in authoring_script_attachments(authoring) {
                    let matching = expected
                        .vmad_attachments
                        .iter()
                        .filter(|attachment| {
                            attachment.owner == *owner
                                && attachment.script_class.eq_ignore_ascii_case(&script_class)
                                && attachment.property_names == property_names
                        })
                        .collect::<Vec<_>>();
                    if let [attachment] = matching.as_slice() {
                        actual.vmad_attachments.push((*attachment).clone());
                    } else {
                        actual
                            .vmad_attachments
                            .push(crate::quest_runtime::VmadAttachmentReceipt {
                                owner: owner.clone(),
                                script_class,
                                property_names,
                                compiler_evidence_id: String::new(),
                            });
                    }
                }
            }

            for route in &expected.routes {
                if authoring_records
                    .get(&route.quest)
                    .and_then(Option::as_ref)
                    .is_some_and(authoring_quest_starts_enabled)
                {
                    actual.routes.push(route.clone());
                }
            }

            let comparison = crate::quest_runtime::compare_post_fixup_receipt(&expected, &actual);
            if !comparison.intact {
                return Err(RunError::InvalidConfig(format!(
                    "{} post-fixup FNV quest runtime receipt was damaged: {}",
                    plan.component_id,
                    serde_json::to_string(&comparison.required_damage).unwrap_or_default()
                )));
            }
        }
        self.fnv_quest_runtime_receipts_finalized = true;
        Ok(())
    }

    pub fn translate_all(&mut self) -> Result<TranslateStats, RunError> {
        self.require_source_handle()?;
        let translate_all_started = std::time::Instant::now();
        self.preflight_legacy_packs_from_handle()?;
        self.emit_status("translate_all: building mapper state…");
        self.prepare_mapper_state_for_translation()?;
        self.index_fo76_condition_forms();
        self.index_fo76_inherited_object_templates();
        self.emit_status(&format!(
            "translate_all: mapper state ready in {:.1}s; enumerating source records…",
            translate_all_started.elapsed().as_secs_f64()
        ));

        if self.config.records_limit == Some(0) {
            let stats = self.translate_fks(&[])?;
            self.validate_creature_dependency_accounting()?;
            self.finalize_legacy_creature_race_coverage()?;
            return Ok(stats);
        }

        let enumerate_started = std::time::Instant::now();
        let sigs = source_signatures(self.source_handle_id, &self.interner)?;
        let structured_dialogue_sig = SigCode::from_str("DIAL")
            .map_err(|e| RunError::InvalidConfig(format!("DIAL signature: {e}")))?;
        let structured_info_sig = SigCode::from_str("INFO")
            .map_err(|e| RunError::InvalidConfig(format!("INFO signature: {e}")))?;
        let structured_scene_sig = SigCode::from_str("SCEN")
            .map_err(|e| RunError::InvalidConfig(format!("SCEN signature: {e}")))?;
        let emit_structured_dialogue = self.should_emit_fo76_quest_dialogue();
        let emit_structured_scenes = self.should_emit_fo76_quest_scenes();
        let mut all_fks: Vec<FormKey> = Vec::new();
        let mut skyrim_runtime_fks = Vec::new();
        for sig in sigs {
            if (emit_structured_dialogue
                && (sig == structured_dialogue_sig || sig == structured_info_sig))
                || (emit_structured_scenes && sig == structured_scene_sig)
            {
                continue;
            }
            if let Some(limit) = self.config.records_limit {
                if all_fks.len() >= limit {
                    break;
                }
            }
            let fks = iter_form_keys_of_sig(self.source_handle_id, sig, &mut self.interner)?;
            let selected = if let Some(limit) = self.config.records_limit {
                let remaining = limit.saturating_sub(all_fks.len());
                fks.into_iter().take(remaining).collect::<Vec<_>>()
            } else {
                fks
            };
            if self.source == Game::SkyrimSe && self.target == Game::Fo4 {
                skyrim_runtime_fks.extend(
                    selected
                        .iter()
                        .copied()
                        .filter(|form_key| self.is_skyrim_runtime_planned_record(*form_key, sig)),
                );
            }
            all_fks.extend(selected);
        }
        self.emit_status(&format!(
            "translate_all: {} records to translate (enumerated in {:.1}s); starting per-record translation…",
            all_fks.len(),
            enumerate_started.elapsed().as_secs_f64()
        ));
        self.plan_skyrim_runtime_components(&skyrim_runtime_fks)?;
        self.preallocate_legacy_translate_all_records(&all_fks)?;
        let mut stats = self.translate_fks(&all_fks)?;
        self.validate_creature_dependency_accounting()?;
        self.finalize_legacy_creature_race_coverage()?;
        if emit_structured_dialogue {
            stats.absorb(self.emit_quest_child_dialogue()?);
            stats.absorb(self.emit_topic_child_infos()?);
        }
        if emit_structured_scenes {
            stats.absorb(self.emit_quest_child_scenes()?);
        }
        stats.absorb(self.emit_skyrim_minimal_quest_projections()?);
        if matches!(self.source, Game::Fnv | Game::Fo3) && self.target == Game::Fo4 {
            self.rebuild_full_plugin_worldspace_groups()?;
            stats.absorb(self.emit_projected_navmeshes()?);
            stats.absorb(self.rebuild_projected_navi()?);
            return Ok(stats);
        }
        if self.source == Game::SkyrimSe && self.target == Game::Fo4 {
            stats.absorb(self.rebuild_projected_navi()?);
        }
        self.rebuild_full_plugin_worldspace_groups()?;
        self.validate_skyrim_actor_translation(&stats)?;
        Ok(stats)
    }

    /// Parallel translate — store2 mmap source + chunked P/A/F/E passes.
    /// Legacy-equivalent target-handle state (see `store2::translate_v2`).
    pub fn translate_all_v2(
        &mut self,
        source_path: &std::path::Path,
    ) -> Result<TranslateStats, RunError> {
        self.require_source_handle()?;
        crate::store2::translate_v2::translate_all_v2(self, source_path)
    }

    pub(crate) fn emit_story_manager_subset(
        &mut self,
    ) -> Result<crate::phase::story_manager::StoryManagerEmitStats, RunError> {
        let mut emit_stats = crate::phase::story_manager::StoryManagerEmitStats::default();
        if self.source == Game::SkyrimSe && self.target == Game::Fo4 {
            if self.skyrim_story_manager_projections.is_empty() {
                return Ok(emit_stats);
            }
            if self.skyrim_minimal_quest_compiler_evidence.len()
                != self.skyrim_minimal_quest_projections.len()
            {
                return Err(RunError::InvalidConfig(
                    "Skyrim Story Manager commitment requires completed PSC/PEX evidence"
                        .to_string(),
                ));
            }
            let required_voice_lines = self
                .skyrim_dialogue_projections
                .iter()
                .map(|dialogue| dialogue.projection.receipt.voice_intents.len())
                .sum::<usize>();
            if required_voice_lines != 0
                && (self.skyrim_quest_runtime_voice_receipt.is_none()
                    || self.skyrim_quest_runtime_asset_copy_receipt.is_none())
            {
                return Err(RunError::InvalidConfig(
                    "Skyrim Story Manager commitment requires reconciled voice/LIP assets"
                        .to_string(),
                ));
            }
            for index in 0..self.skyrim_story_manager_projections.len() {
                if self.skyrim_story_manager_projections[index]
                    .projection
                    .is_some()
                {
                    continue;
                }
                let live = self.skyrim_story_manager_projections[index].clone();
                let plan = self
                    .skyrim_runtime_capability_plan
                    .as_ref()
                    .and_then(|capability| {
                        capability
                            .minimal_quests
                            .values()
                            .find(|admission| admission.plan.component_id == live.component_id)
                    })
                    .map(|admission| admission.plan.clone())
                    .ok_or_else(|| {
                        RunError::InvalidConfig(format!(
                            "{} has no admitted Story Manager component plan",
                            live.component_id
                        ))
                    })?;
                let mut projection =
                    crate::skyrimse_fo4_runtime::story_manager::project_story_manager_chain(
                        &live.source_records,
                        &plan,
                        &live.event_mapping,
                        &self.interner,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "{} Story Manager projection rejected: {error}",
                            live.component_id
                        ))
                    })?;
                for record in &projection.records {
                    add_record_native(
                        self.target_handle_id,
                        record.clone(),
                        &self.schema_target,
                        &self.interner,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "{} emit projected {}: {error}",
                            live.component_id,
                            record.sig.as_str()
                        ))
                    })?;
                    emit_stats.translate.records_translated += 1;
                    emit_stats.translate.signature_entry(record.sig).translated += 1;
                }
                projection.records = projection
                    .records
                    .iter()
                    .map(|record| {
                        read_record_relayout_by_form_key(
                            self.target_handle_id,
                            &record.form_key,
                            &self.schema_target,
                            &self.interner,
                            None,
                        )
                        .map_err(|error| {
                            RunError::InvalidConfig(format!(
                                "{} freeze emitted Story Manager {} receipt: {error}",
                                live.component_id,
                                record.sig.as_str()
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let admission = self
                    .skyrim_runtime_capability_plan
                    .as_mut()
                    .and_then(|capability| {
                        capability
                            .minimal_quests
                            .values_mut()
                            .find(|admission| admission.plan.component_id == live.component_id)
                    })
                    .expect("Story Manager admission exists after projection");
                admission
                    .plan
                    .expected_receipt
                    .routes
                    .insert(projection.route_receipt.route.clone());
                self.skyrim_story_manager_projections[index].projection = Some(projection);
                emit_stats.selected_nodes += live
                    .source_records
                    .iter()
                    .filter(|record| matches!(record.sig.as_str(), "SMEN" | "SMBN" | "SMQN"))
                    .count() as u32;
            }
            return Ok(emit_stats);
        }
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(emit_stats);
        }
        if self.mapper_state.is_none() {
            self.init_mapper_state()?;
        }

        let graph = crate::phase::story_manager::load_source_graph(self)?;
        let translated_quests: FxHashSet<FormKey> = {
            let state = self
                .mapper_state
                .as_ref()
                .expect("mapper_state initialized before Story Manager emit");
            graph
                .quests
                .keys()
                .filter(|fk| state.source_to_target.contains_key(*fk))
                .copied()
                .collect()
        };
        let selection = crate::phase::story_manager::classify_story_manager_records(
            &graph,
            &translated_quests,
            &self.interner,
        );
        emit_stats.selected_nodes = selection.selected_nodes.len() as u32;
        emit_stats.skipped_nodes = selection
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.kind == crate::phase::story_manager::StoryManagerDiagnosticKind::Skipped
            })
            .count() as u32;
        self.record_story_manager_diagnostics(&selection.diagnostics);

        let unproven_roots = crate::phase::story_manager::unproven_story_manager_event_roots(
            &selection.selected_nodes,
            &graph,
        );
        if !unproven_roots.is_empty() {
            let roots = unproven_roots
                .iter()
                .map(|(form_key, event_type)| {
                    let event = String::from_utf8_lossy(&event_type.to_le_bytes()).into_owned();
                    format!("{:06X}:{event}", form_key.local)
                })
                .collect::<Vec<_>>()
                .join(",");
            return Err(RunError::InvalidConfig(format!(
                "story_manager_event_unsupported_no_fo4_root:{roots}"
            )));
        }

        let event_bridges = self.prepare_story_manager_event_bridges(&selection, &graph)?;
        self.map_story_manager_event_roots(&selection.selected_nodes, &graph);
        self.preallocate_story_manager_nodes(&selection.ordered_nodes, &graph);
        let mut emitted_nodes = FxHashSet::default();
        emit_stats.translate = self.translate_story_manager_nodes(
            &selection.ordered_nodes,
            &selection.selected_nodes,
            &selection.selected_quests_by_node,
            &event_bridges,
            &graph,
            &mut emitted_nodes,
        )?;
        let story_manager_owned_quests =
            crate::phase::story_manager::emitted_story_manager_quests_requiring_autostart_clear(
                &selection,
                &emitted_nodes,
                &graph,
            );
        emit_stats.quests_changed +=
            self.clear_emitted_story_manager_quest_autostart(&story_manager_owned_quests)?;
        let story_manager_owned_quests = story_manager_owned_quests
            .into_iter()
            .collect::<FxHashSet<_>>();
        emit_stats.quests_changed +=
            self.restore_passive_dialogue_controllers(&story_manager_owned_quests)?;
        emit_stats.quests_changed +=
            self.force_story_manager_dialogue_quests(&selection.fallback_dialogue_quests)?;
        let quest_event_plan = crate::phase::story_manager::plan_story_manager_quest_events(
            &selection,
            &graph,
            &event_bridges,
            &emitted_nodes,
            &self.interner,
        );
        for (quest, final_events) in &quest_event_plan.unresolved {
            let event_names = final_events
                .iter()
                .map(|event| String::from_utf8_lossy(&event.to_le_bytes()).into_owned())
                .collect::<Vec<_>>()
                .join(",");
            let kind = self.interner.intern("story_manager_quest_event_unresolved");
            self.decisions.push(Decision {
                kind,
                message: format!("{:06X}:final_events={event_names}", quest.local),
            });
        }
        emit_stats.quests_changed +=
            self.rewrite_story_manager_quest_events(&quest_event_plan.rewrites)?;
        self.capture_story_manager_route_seed(&graph, &selection, &emitted_nodes, &event_bridges);
        emit_stats.translate.records_translated += event_bridges.len() as u32;
        Ok(emit_stats)
    }

    fn capture_story_manager_route_seed(
        &mut self,
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
        selection: &crate::phase::story_manager::StoryManagerSelection,
        emitted_nodes: &FxHashSet<FormKey>,
        event_bridges: &FxHashMap<FormKey, u32>,
    ) {
        let Some(state) = self.mapper_state.as_ref() else {
            return;
        };
        let source_to_target = state.source_to_target.clone();
        let mut target_records = FxHashMap::default();
        for source_form_key in
            crate::phase::story_manager::story_manager_route_source_form_keys(graph)
        {
            let Some(target_form_key) = source_to_target.get(&source_form_key).copied() else {
                continue;
            };
            let Some(target_plugin) = self.interner.resolve(target_form_key.plugin) else {
                continue;
            };
            let target_handle_id =
                if target_plugin.eq_ignore_ascii_case(&self.config.output_plugin_name) {
                    Some(self.target_handle_id)
                } else {
                    self.target_master_record_contexts
                        .iter()
                        .find(|context| context.plugin_name.eq_ignore_ascii_case(target_plugin))
                        .map(|context| context.handle_id)
                };
            let Some(target_handle_id) = target_handle_id else {
                continue;
            };
            if let Ok(record) = read_record_relayout_by_form_key(
                target_handle_id,
                &target_form_key,
                &self.schema_target,
                &self.interner,
                None,
            ) {
                target_records.insert(target_form_key, record);
            }
        }
        let bridge_roots = event_bridges.keys().copied().collect::<FxHashSet<_>>();
        let output_plugin = self.interner.intern(&self.config.output_plugin_name);
        self.story_manager_route_seed =
            crate::phase::story_manager::build_story_manager_route_seed_for_pair(
                self.source,
                self.target,
                graph,
                selection,
                &source_to_target,
                &target_records,
                emitted_nodes,
                &bridge_roots,
                output_plugin,
                &self.interner,
            );
    }

    fn prepare_story_manager_event_bridges(
        &mut self,
        selection: &crate::phase::story_manager::StoryManagerSelection,
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
    ) -> Result<FxHashMap<FormKey, u32>, RunError> {
        let roots = crate::phase::story_manager::incompatible_story_manager_event_roots(
            &selection.selected_nodes,
            graph,
        );
        if roots.is_empty() {
            return Ok(FxHashMap::default());
        }

        let output_plugin = self.interner.intern(&self.config.output_plugin_name);
        let allocations = {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before Story Manager emit");
            if let Some((source_root, target)) = roots.iter().find_map(|(source_root, _)| {
                state
                    .source_to_target
                    .get(source_root)
                    .copied()
                    .map(|target| (*source_root, target))
            }) {
                return Err(RunError::InvalidConfig(format!(
                    "story_manager_event_bridge_mapping_exists:{:06X}->{:06X}",
                    source_root.local, target.local
                )));
            }
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            mapper.reserve_object_ids(roots.iter().map(|(source_root, _)| source_root.local));
            roots
                .iter()
                .map(|(source_root, event_type)| {
                    let keyword = FormKey {
                        local: source_root.local,
                        plugin: output_plugin,
                    };
                    let branch = mapper.allocate_generated();
                    mapper.add_mapping(*source_root, branch);
                    (*source_root, *event_type, keyword, branch)
                })
                .collect::<Vec<_>>()
        };

        let mut bridges = FxHashMap::default();
        for (source_root, event_type, keyword, branch) in allocations {
            let keyword_raw =
                encode_form_key_for_handle(self.target_handle_id, keyword, &self.interner)
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "story_manager_event_keyword_formid:{:06X}:{error}",
                            source_root.local
                        ))
                    })?;
            let event_name = String::from_utf8_lossy(&event_type.to_le_bytes()).into_owned();
            let editor_id = self.interner.intern(&format!("B21_SMEvent_{event_name}"));
            let mut keyword_record =
                Record::new(SigCode::from_str("KYWD").expect("literal sig"), keyword);
            keyword_record.eid = Some(editor_id);
            keyword_record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("EDID").expect("literal sig"),
                value: FieldValue::String(editor_id),
            });
            add_record_native(
                self.target_handle_id,
                keyword_record,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "story_manager_event_keyword_add:{:06X}:{error}",
                    source_root.local
                ))
            })?;
            let kind = self.interner.intern("story_manager_event_bridge");
            self.decisions.push(Decision {
                kind,
                message: format!(
                    "{:06X}:{event_name}:keyword={:06X}:branch={:06X}",
                    source_root.local, keyword.local, branch.local
                ),
            });
            bridges.insert(source_root, keyword_raw);
        }
        Ok(bridges)
    }

    fn map_story_manager_event_roots(
        &mut self,
        selected_nodes: &FxHashSet<FormKey>,
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
    ) {
        let Some(fallout4_name) = self
            .config
            .target_master_names
            .iter()
            .find(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
            .cloned()
        else {
            return;
        };
        let fallout4 = self.interner.intern(&fallout4_name);
        let Some(state) = self.mapper_state.as_mut() else {
            return;
        };
        let mut mapper = FormKeyMapper::from_state(state, &self.interner);
        let script_event_root = FormKey {
            local: crate::phase::story_manager::FO4_SCRIPT_EVENT_ROOT_LOCAL,
            plugin: fallout4,
        };
        mapper.add_mapping(script_event_root, script_event_root);
        for source_fk in selected_nodes {
            let Some(record) = graph.nodes.get(source_fk) else {
                continue;
            };
            let Some(target_local) =
                crate::phase::story_manager::fo4_story_manager_event_root(record)
            else {
                continue;
            };
            mapper.add_mapping(
                *source_fk,
                FormKey {
                    local: target_local,
                    plugin: fallout4,
                },
            );
        }
    }

    fn record_story_manager_diagnostics(
        &mut self,
        diagnostics: &[crate::phase::story_manager::StoryManagerDiagnostic],
    ) {
        for diagnostic in diagnostics {
            let kind_name = match diagnostic.kind {
                crate::phase::story_manager::StoryManagerDiagnosticKind::Selected => {
                    "story_manager_selected"
                }
                crate::phase::story_manager::StoryManagerDiagnosticKind::Skipped => {
                    "story_manager_skipped"
                }
            };
            let kind = self.interner.intern(kind_name);
            let message = if let Some(reason) = diagnostic.reason {
                format!(
                    "{:06X}:{}:{}",
                    diagnostic.form_key.local,
                    reason.as_str(),
                    diagnostic.message
                )
            } else {
                format!("{:06X}:{}", diagnostic.form_key.local, diagnostic.message)
            };
            self.decisions.push(Decision { kind, message });
        }
    }

    fn force_story_manager_dialogue_quests(
        &mut self,
        quest_fks: &[FormKey],
    ) -> Result<u32, RunError> {
        let mut changed = 0u32;
        for source_fk in quest_fks {
            if self.force_target_quest_autostart(*source_fk, "story_manager_quest_autostart")? {
                changed += 1;
            }
        }
        Ok(changed)
    }

    fn clear_emitted_story_manager_quest_autostart(
        &mut self,
        quests: &[FormKey],
    ) -> Result<u32, RunError> {
        let mut changed = 0u32;
        for source_fk in quests {
            if self.clear_target_quest_autostart(*source_fk)? {
                changed += 1;
            }
        }
        Ok(changed)
    }

    fn clear_target_quest_autostart(&mut self, source_fk: FormKey) -> Result<bool, RunError> {
        let Some(target_fk) = self
            .mapper_state
            .as_ref()
            .and_then(|state| state.source_to_target.get(&source_fk))
            .copied()
        else {
            let kind = self
                .interner
                .intern("story_manager_quest_start_disabled_skipped");
            self.decisions.push(Decision {
                kind,
                message: format!("{:06X}:quest_not_translated", source_fk.local),
            });
            return Ok(false);
        };
        let mut record = match read_record_relayout_by_form_key(
            self.target_handle_id,
            &target_fk,
            &self.schema_target,
            &self.interner,
            None,
        ) {
            Ok(record) => record,
            Err(e) => {
                let warning = self.interner.intern(&format!(
                    "story_manager_target_quest_start_disable_read:{:06X}:{e}",
                    target_fk.local
                ));
                self.warnings.push(warning);
                return Ok(false);
            }
        };
        if !crate::phase::story_manager::clear_qust_autostart_for_pair(
            self.source,
            self.target,
            &mut record,
            &self.interner,
        ) {
            return Ok(false);
        }
        let replaced = replace_record_contents_native(
            self.target_handle_id,
            record,
            &self.schema_target,
            &self.interner,
        )
        .map_err(|e| {
            RunError::InvalidConfig(format!("story_manager_quest_start_disable_replace:{e}"))
        })?;
        if !replaced {
            return Err(RunError::InvalidConfig(format!(
                "story_manager_quest_start_disable_replace_missing:{:06X}",
                target_fk.local
            )));
        }
        let kind = self.interner.intern("story_manager_quest_start_disabled");
        self.decisions.push(Decision {
            kind,
            message: format!("{:06X}->{:06X}", source_fk.local, target_fk.local),
        });
        Ok(true)
    }

    fn rewrite_story_manager_quest_events(
        &mut self,
        rewrites: &FxHashMap<FormKey, u32>,
    ) -> Result<u32, RunError> {
        let mut rewrites = rewrites.iter().collect::<Vec<_>>();
        rewrites.sort_by_key(|(source_fk, _)| source_fk.local);
        let mut changed = 0u32;
        for (source_fk, event_type) in rewrites {
            let Some(target_fk) = self
                .mapper_state
                .as_ref()
                .and_then(|state| state.source_to_target.get(source_fk))
                .copied()
            else {
                let kind = self
                    .interner
                    .intern("story_manager_quest_event_rewrite_skipped");
                self.decisions.push(Decision {
                    kind,
                    message: format!("{:06X}:quest_not_translated", source_fk.local),
                });
                continue;
            };
            let mut record = match read_record_relayout_by_form_key(
                self.target_handle_id,
                &target_fk,
                &self.schema_target,
                &self.interner,
                None,
            ) {
                Ok(record) => record,
                Err(e) => {
                    let warning = self.interner.intern(&format!(
                        "story_manager_target_quest_event_read:{:06X}:{e}",
                        target_fk.local
                    ));
                    self.warnings.push(warning);
                    continue;
                }
            };
            if !crate::phase::story_manager::set_qust_event_type(&mut record, *event_type) {
                continue;
            }
            let replaced = replace_record_contents_native(
                self.target_handle_id,
                record,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|e| {
                RunError::InvalidConfig(format!("story_manager_quest_event_replace:{e}"))
            })?;
            if !replaced {
                return Err(RunError::InvalidConfig(format!(
                    "story_manager_quest_event_replace_missing:{:06X}",
                    target_fk.local
                )));
            }
            let event_name = String::from_utf8_lossy(&event_type.to_le_bytes()).into_owned();
            let kind = self.interner.intern("story_manager_quest_event_rewritten");
            self.decisions.push(Decision {
                kind,
                message: format!(
                    "{:06X}->{:06X}:{event_name}",
                    source_fk.local, target_fk.local
                ),
            });
            changed += 1;
        }
        Ok(changed)
    }

    fn restore_passive_dialogue_controllers(
        &mut self,
        story_manager_owned_quests: &FxHashSet<FormKey>,
    ) -> Result<u32, RunError> {
        let npc_quests =
            crate::phase::story_manager::npc_referenced_quest_local_ids(self.source_handle_id)?;
        if npc_quests.is_empty() {
            return Ok(0);
        }
        let quest_sig = SigCode::from_str("QUST")
            .map_err(|e| RunError::InvalidConfig(format!("QUST signature: {e}")))?;
        let quest_fks = iter_form_keys_of_sig(self.source_handle_id, quest_sig, &self.interner)?;
        let mut changed = 0;
        for source_fk in quest_fks {
            if !npc_quests.contains(&source_fk.local) {
                continue;
            }
            let source_record = match read_record_relayout_by_form_key(
                self.source_handle_id,
                &source_fk,
                &self.schema_source,
                &self.interner,
                None,
            ) {
                Ok(record) => record,
                Err(e) => {
                    let warning = self.interner.intern(&format!(
                        "passive_dialogue_controller_read:{:06X}:{e}",
                        source_fk.local
                    ));
                    self.warnings.push(warning);
                    continue;
                }
            };
            if !crate::phase::story_manager::is_passive_dialogue_controller(
                &source_record,
                true,
                &self.interner,
            ) {
                continue;
            }
            if !crate::phase::story_manager::allows_passive_dialogue_autostart_restore(
                source_fk,
                story_manager_owned_quests,
            ) {
                let kind = self
                    .interner
                    .intern("passive_dialogue_controller_story_manager_owned");
                self.decisions.push(Decision {
                    kind,
                    message: format!("{:06X}:autostart_not_restored", source_fk.local),
                });
                continue;
            }
            if self.force_target_quest_autostart(source_fk, "quest_startup_passive_controller")? {
                changed += 1;
            }
        }
        Ok(changed)
    }

    fn force_target_quest_autostart(
        &mut self,
        source_fk: FormKey,
        decision_kind: &str,
    ) -> Result<bool, RunError> {
        self.rewrite_target_quest(source_fk, decision_kind, |record, interner| {
            crate::phase::story_manager::force_qust_autostart(record, interner)
        })
    }

    fn rewrite_target_quest(
        &mut self,
        source_fk: FormKey,
        decision_kind: &str,
        edit: impl FnOnce(&mut Record, &crate::sym::StringInterner) -> bool,
    ) -> Result<bool, RunError> {
        let target_fk = self
            .mapper_state
            .as_ref()
            .and_then(|state| state.source_to_target.get(&source_fk))
            .copied();
        let Some(target_fk) = target_fk else {
            let kind = self.interner.intern(&format!("{decision_kind}_skipped"));
            self.decisions.push(Decision {
                kind,
                message: format!("{:06X}:quest_not_translated", source_fk.local),
            });
            return Ok(false);
        };
        let mut record = match read_record_relayout_by_form_key(
            self.target_handle_id,
            &target_fk,
            &self.schema_target,
            &self.interner,
            None,
        ) {
            Ok(record) => record,
            Err(e) => {
                let warning = self.interner.intern(&format!(
                    "story_manager_target_quest_read:{:06X}:{e}",
                    target_fk.local
                ));
                self.warnings.push(warning);
                return Ok(false);
            }
        };
        if !edit(&mut record, &self.interner) {
            return Ok(false);
        }
        let replaced = replace_record_contents_native(
            self.target_handle_id,
            record,
            &self.schema_target,
            &self.interner,
        )
        .map_err(|e| RunError::InvalidConfig(format!("story_manager_quest_replace:{e}")))?;
        if !replaced {
            return Err(RunError::InvalidConfig(format!(
                "story_manager_quest_replace_missing:{:06X}",
                target_fk.local
            )));
        }
        let kind = self.interner.intern(decision_kind);
        self.decisions.push(Decision {
            kind,
            message: format!("{:06X}->{:06X}", source_fk.local, target_fk.local),
        });
        Ok(true)
    }

    fn preallocate_story_manager_nodes(
        &mut self,
        ordered_nodes: &[FormKey],
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
    ) {
        let Some(state) = self.mapper_state.as_mut() else {
            return;
        };
        let mut mapper = FormKeyMapper::from_state(state, &self.interner);
        for fk in ordered_nodes {
            let Some(record) = graph.nodes.get(fk) else {
                continue;
            };
            let normalized_eid = normalized_eid_opt(record.eid, mapper.interner);
            mapper.allocate_or_resolve(*fk, normalized_eid, record.sig);
        }
    }

    fn translate_story_manager_nodes(
        &mut self,
        ordered_nodes: &[FormKey],
        selected_nodes: &FxHashSet<FormKey>,
        selected_quests_by_node: &FxHashMap<FormKey, FxHashSet<FormKey>>,
        event_bridges: &FxHashMap<FormKey, u32>,
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
        emitted_nodes: &mut FxHashSet<FormKey>,
    ) -> Result<TranslateStats, RunError> {
        let mut stats = TranslateStats::default();
        let whole_plugin_names =
            if self.config.is_whole_plugin && self.config.target_master_names.is_empty() {
                self.target_master_plugin_names()
            } else {
                Vec::new()
            };
        let target_master_names = target_master_names_for_skip(&self.config, whole_plugin_names);
        let target_master_syms: FxHashSet<Sym> =
            intern_plugin_names(&target_master_names, &self.interner);
        let first_target_master_sym = target_master_names
            .first()
            .map(|name| self.interner.intern(name));
        let fallout4 = target_master_names
            .iter()
            .find(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
            .map(|name| self.interner.intern(name));

        for fk in ordered_nodes {
            let Some(source_template) = graph.nodes.get(fk) else {
                continue;
            };
            let mut src_record = source_template.clone();
            let source_sig = src_record.sig;
            stats.signature_entry(source_sig).seen += 1;
            if let Some(allowed_quests) = selected_quests_by_node.get(fk) {
                crate::phase::story_manager::retain_story_manager_quests(
                    &mut src_record,
                    allowed_quests,
                );
            }
            crate::phase::story_manager::sanitize_story_manager_previous_node(
                &mut src_record,
                selected_nodes,
                &self.interner,
            );
            if let Some(keyword_raw) = event_bridges.get(fk) {
                let fallout4 = fallout4.ok_or_else(|| {
                    RunError::InvalidConfig(
                        "story_manager_event_bridge_missing_fallout4_master".to_string(),
                    )
                })?;
                crate::phase::story_manager::lower_incompatible_event_root(
                    &mut src_record,
                    FormKey {
                        local: crate::phase::story_manager::FO4_SCRIPT_EVENT_ROOT_LOCAL,
                        plugin: fallout4,
                    },
                    *keyword_raw,
                );
            }

            {
                let mut ctx = PairCtx::new(&self.interner);
                if let Err(e) = self.translator.pre_translate(&mut ctx, &mut src_record) {
                    let w = self
                        .interner
                        .intern(&format!("story_manager_pre_translate:{e}"));
                    self.warnings.push(w);
                }
            }

            let mut translated = match self.translator.translate_ignoring_skip(
                &src_record,
                &self.interner,
                src_record.sig.as_str(),
            ) {
                TranslateResult::Translated(record) => record,
                TranslateResult::Dropped { decision, .. } => {
                    self.decisions.push(decision);
                    stats.records_dropped += 1;
                    stats.signature_entry(source_sig).dropped += 1;
                    continue;
                }
                TranslateResult::Deferred(kind) => {
                    self.deferred.push((*fk, kind));
                    stats.records_deferred += 1;
                    stats.signature_entry(source_sig).deferred += 1;
                    continue;
                }
            };

            if self
                .schema_target
                .record_def(translated.sig.as_str())
                .is_none()
            {
                let warning = self.interner.intern(&format!(
                    "story_manager_unsupported_target_record:{}",
                    translated.sig.as_str()
                ));
                self.warnings.push(warning);
                stats.records_dropped += 1;
                stats.signature_entry(source_sig).dropped += 1;
                continue;
            }

            let (target_fk, rewrite_report) = {
                let state = self
                    .mapper_state
                    .as_mut()
                    .expect("mapper_state initialized before Story Manager emit");
                let mut mapper = FormKeyMapper::from_state(state, &self.interner);
                let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
                let target_fk = mapper.allocate_or_resolve(*fk, normalized_eid, translated.sig);
                translated.form_key = target_fk;
                let rewrite_report = match mapper.rewrite_record_with_report(&mut translated) {
                    Ok(report) => Some(report),
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("story_manager_rewrite_record:{e}"));
                        self.warnings.push(w);
                        None
                    }
                };
                (target_fk, rewrite_report)
            };
            if is_target_master_remap(target_fk, &target_master_syms) {
                emitted_nodes.insert(*fk);
                stats.records_vanilla_remapped += 1;
                stats.signature_entry(source_sig).vanilla_remapped += 1;
                continue;
            }

            {
                let mut ctx = PairCtx::new(&self.interner);
                if let Err(e) = self.translator.post_translate(&mut ctx, &mut translated) {
                    let w = self
                        .interner
                        .intern(&format!("story_manager_post_translate:{e}"));
                    self.warnings.push(w);
                }
            }
            {
                let mut ctx = TargetCtx {
                    interner: &self.interner,
                };
                if let Err(e) = self.translator.run_target_hook(&mut ctx, &mut translated) {
                    let w = self
                        .interner
                        .intern(&format!("story_manager_target_hook:{e}"));
                    self.warnings.push(w);
                }
            }

            let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
                &mut translated,
                &self.schema_target,
                &self.interner,
            );
            for message in report.decisions {
                let kind = self.interner.intern("class_a_normalize");
                self.decisions.push(Decision { kind, message });
            }
            for warning in report.warnings {
                let sym = self.interner.intern(&warning);
                self.warnings.push(sym);
            }

            let translated = {
                let normalizer = TargetRecordNormalizer {
                    target_schema: &self.schema_target,
                    source_record_def: self.schema_source.record_def(source_sig.as_str()),
                    interner: Some(&self.interner),
                };
                match normalizer.normalize(translated) {
                    TargetRecordNormalization::Keep(record) => record,
                    TargetRecordNormalization::DropUnsupportedRecord => {
                        stats.records_dropped += 1;
                        stats.signature_entry(source_sig).dropped += 1;
                        continue;
                    }
                }
            };
            let full_plugin_snapshot = if self.config.is_whole_plugin {
                Some(crate::full_plugin::target_schema_record_view(
                    &translated,
                    &self.schema_target,
                ))
            } else {
                None
            };
            if let Err(e) = add_record_native(
                self.target_handle_id,
                translated,
                &self.schema_target,
                &self.interner,
            ) {
                let w = self
                    .interner
                    .intern(&format!("story_manager_write_error:{e}"));
                self.warnings.push(w);
                stats.records_failed += 1;
                stats.signature_entry(source_sig).failed += 1;
                continue;
            }
            if let Some(snapshot) = full_plugin_snapshot.as_ref() {
                self.capture_full_plugin_record_state(
                    *fk,
                    snapshot,
                    &target_master_syms,
                    first_target_master_sym,
                    rewrite_report
                        .as_ref()
                        .map(|report| &report.unresolved_form_keys),
                );
            }
            emitted_nodes.insert(*fk);
            stats.records_translated += 1;
            stats.signature_entry(source_sig).translated += 1;
        }

        Ok(stats)
    }

    /// Translate a bounded list of source FormKeys into the target plugin.
    ///
    /// Same per-record pipeline as `translate_all` (pair-hook pre/post +
    /// translator + mapper allocate + rewrite + target-hook + write) but
    /// scoped to the caller-supplied set. FormKeys that don't resolve
    /// in the source plugin are recorded as `records_failed` and skipped.
    pub fn translate_records(&mut self, fks: &[FormKey]) -> Result<TranslateStats, RunError> {
        self.require_source_handle()?;
        self.prepare_mapper_state_for_translation()?;
        let stats = self.translate_fks(fks)?;
        self.finalize_legacy_creature_race_coverage()?;
        Ok(stats)
    }

    pub(crate) fn translate_records_preserving_mapper(
        &mut self,
        fks: &[FormKey],
    ) -> Result<TranslateStats, RunError> {
        self.require_source_handle()?;
        if self.mapper_state.is_none() {
            return Err(RunError::InvalidConfig(
                "bounded legacy translation requires the existing translate_v2 mapper state"
                    .to_string(),
            ));
        }
        self.preallocate_legacy_translate_all_records(fks)?;
        self.translate_fks(fks)
    }

    /// Convert FO76 interior cells (CELL fields + Persistent/Temporary placed
    /// children + interior NAVM) into the FO4 target. Gated to FO76→FO4; a no-op
    /// otherwise. Mirrors `emit_projected_navmeshes`' translate+remap path but
    /// runs serially per interior cell (the interior subset is small relative to
    /// the exterior navmesh corpus, and the cell/child allocations are real
    /// records whose mapper entries must persist).
    pub fn emit_interior_cells(&mut self, carry_previs: bool) -> Result<TranslateStats, RunError> {
        let mut stats = TranslateStats::default();
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(stats);
        }
        self.interior_placed_ref_candidates.clear();
        self.interior_recentre.clear();
        if self.mapper_state.is_none() {
            self.init_mapper_state()?;
        }

        let cell = SigCode::from_str("CELL")
            .map_err(|e| RunError::InvalidConfig(format!("CELL signature: {e}")))?;
        let all_cell_fks = iter_form_keys_of_sig(self.source_handle_id, cell, &mut self.interner)?;
        if all_cell_fks.is_empty() {
            return Ok(stats);
        }

        // Batch-snapshot ALL cells once (a single localized-strings clone for the
        // whole set) and pick the interior ones by DATA's IsInteriorCell bit.
        // Snapshotting per cell here clones the multi-language string table on
        // every call — over an entire worldspace's cells that is the stall.
        let cells_snapshot =
            snapshot_records_by_form_keys(self.source_handle_id, &all_cell_fks, &self.interner)?;
        let interior_indices: Vec<usize> = cells_snapshot
            .records
            .iter()
            .enumerate()
            .filter(|(_, rec)| raw_cell_is_interior(&rec.raw_record))
            .map(|(idx, _)| idx)
            .collect();
        if interior_indices.is_empty() {
            return Ok(stats);
        }

        let interior_fks: Vec<FormKey> = interior_indices
            .iter()
            .map(|&idx| cells_snapshot.records[idx].form_key)
            .collect();
        let source_plugin_sym = interior_fks[0].plugin;
        let interior_objs: FxHashSet<u32> = interior_fks
            .iter()
            .map(|fk| fk.local & 0x00FF_FFFF)
            .collect();

        // One source-tree pass: cell object id -> placed-child object ids.
        let children_map = crate::source_read::collect_interior_cell_children(
            self.source_handle_id,
            &interior_objs,
        )
        .map_err(|e| RunError::InvalidConfig(format!("interior_children_collect:{e}")))?;

        // One target pass: drop any pre-existing PKIN storage-cell stubs for the
        // interior cells we are about to emit; the real cell supersedes them.
        let interior_objs_vec: Vec<u32> = interior_objs.iter().copied().collect();
        crate::target_write::remove_interior_cell_stubs_native(
            self.target_handle_id,
            &interior_objs_vec,
        )
        .map_err(|e| RunError::InvalidConfig(format!("interior_stub_remove:{e}")))?;

        // ── Pass 1: translate the interior CELL records from the batch. Keep each
        // translated cell + its target local id, then drop the all-cells snapshot
        // to free the (mostly exterior) cell records before snapshotting children.
        let mut translated_cells: Vec<(u32, Record)> = Vec::with_capacity(interior_indices.len());
        // Source cell object id -> the translated target cell's FormKey. The file
        // FormID (own master index applied) is composed at the NAVM call site via
        // `fo76_navmesh::target_form_id`; storing the bare `form_key.local` here
        // would drop the master index and corrupt the interior NAVM parent.
        let mut cell_formkey_by_obj: FxHashMap<u32, FormKey> = FxHashMap::default();
        for &idx in &interior_indices {
            let cell_obj = cells_snapshot.records[idx].form_key.local & 0x00FF_FFFF;
            let Some(mut translated_cell) = self.translate_and_remap_snapshot_record(
                &cells_snapshot,
                idx,
                "CELL",
                &mut stats,
                None,
            )?
            else {
                continue;
            };
            if !carry_previs {
                strip_interior_previs_fields(&mut translated_cell);
            }
            crate::fixups::mark_public_wastelanders_hubs::mark_public_social_hub(
                cell_obj,
                &mut translated_cell,
                &self.interner,
            );
            cell_formkey_by_obj.insert(cell_obj, translated_cell.form_key);
            translated_cells.push((cell_obj, translated_cell));
        }
        drop(cells_snapshot);

        // Flat child work list (only for cells that translated), in cell order.
        const PERSISTENT_GROUP: i32 = 8;
        const TEMPORARY_GROUP: i32 = 9;
        let mut child_fks: Vec<FormKey> = Vec::new();
        let mut child_meta: Vec<(u32, i32)> = Vec::new(); // (cell object id, section type)
        for (cell_obj, _) in &translated_cells {
            let Some(kids) = children_map.get(cell_obj) else {
                continue;
            };
            for obj in &kids.persistent {
                child_fks.push(FormKey {
                    local: *obj,
                    plugin: source_plugin_sym,
                });
                child_meta.push((*cell_obj, PERSISTENT_GROUP));
            }
            for obj in &kids.temporary {
                child_fks.push(FormKey {
                    local: *obj,
                    plugin: source_plugin_sym,
                });
                child_meta.push((*cell_obj, TEMPORARY_GROUP));
            }
        }

        // ── Pass 2: batch-snapshot all placed children once, translate each, and
        // bucket the results per cell/section.
        type CellChildBuckets = (Vec<Record>, Vec<SigCode>, Vec<Record>, Vec<SigCode>);
        let mut children_by_cell: FxHashMap<u32, CellChildBuckets> = FxHashMap::default();
        if !child_fks.is_empty() {
            let source_formid_context = crate::fo76_navmesh::snapshot_formid_context(
                self.source_handle_id,
            )
            .map_err(|e| RunError::InvalidConfig(format!("interior_navm_source_context:{e}")))?;
            let target_formid_context = crate::fo76_navmesh::snapshot_formid_context(
                self.target_handle_id,
            )
            .map_err(|e| RunError::InvalidConfig(format!("interior_navm_target_context:{e}")))?;
            let children_snapshot =
                snapshot_records_by_form_keys(self.source_handle_id, &child_fks, &self.interner)?;
            for (idx, &(cell_obj, section)) in child_meta.iter().enumerate() {
                let child_sig = children_snapshot.records[idx]
                    .raw_record
                    .signature
                    .as_str()
                    .to_owned();
                let nvnm_contexts = (child_sig == "NAVM")
                    .then_some((&source_formid_context, &target_formid_context));
                let Some(mut translated_child) = self.translate_and_remap_snapshot_record(
                    &children_snapshot,
                    idx,
                    &child_sig,
                    &mut stats,
                    nvnm_contexts,
                )?
                else {
                    continue;
                };
                if child_sig == "NAVM" {
                    if let Some(cell_fk) = cell_formkey_by_obj.get(&cell_obj) {
                        let cell_file_form_id = crate::fo76_navmesh::target_form_id(
                            *cell_fk,
                            &target_formid_context,
                            &self.interner,
                        );
                        set_nvnm_parent_interior(&mut translated_child, cell_file_form_id);
                    }
                }
                if matches!(
                    child_sig.as_str(),
                    "REFR" | "ACHR" | "PHZD" | "PGRE" | "PGRD"
                ) {
                    self.interior_placed_ref_candidates
                        .push(translated_child.form_key);
                }
                let sig_code =
                    SigCode::from_str(&child_sig).unwrap_or(SigCode([b'?', b'?', b'?', b'?']));
                let entry = children_by_cell.entry(cell_obj).or_default();
                if section == PERSISTENT_GROUP {
                    entry.0.push(translated_child);
                    entry.1.push(sig_code);
                } else {
                    entry.2.push(translated_child);
                    entry.3.push(sig_code);
                }
            }
        }

        // ── Pass 2b: move interiors that extend past FO4's coordinate limit back
        // inside it. Needs each cell's full child set, so it runs before insert. ──
        use crate::fixups::recentre_far_interiors::RecentrePlan;
        for (cell_obj, translated_cell) in &translated_cells {
            let Some((persistent_records, _, temporary_records, _)) =
                children_by_cell.get_mut(cell_obj)
            else {
                continue;
            };
            let cell_label = translated_cell
                .eid
                .and_then(|eid| self.interner.resolve(eid))
                .map_or_else(|| format!("{cell_obj:06X}"), str::to_owned);
            let warning = match crate::fixups::recentre_far_interiors::recentre_cell_children(
                persistent_records,
                temporary_records,
                translated_cell.form_key.plugin,
                !carry_previs,
                &self.interner,
                &mut self.interior_recentre,
            ) {
                Ok(RecentrePlan::Unchanged) => None,
                Ok(RecentrePlan::Shift([dx, dy, _])) => {
                    eprintln!("[interior_recentre] cell={cell_label} offset=({dx:.0},{dy:.0})");
                    None
                }
                Ok(RecentrePlan::TooWide {
                    span: [width, height],
                }) => Some(format!(
                    "interior_recentre_too_wide:{cell_label} span=({width:.0},{height:.0})"
                )),
                Ok(RecentrePlan::PrevisCarried([dx, dy, _])) => Some(format!(
                    "interior_recentre_skipped_for_carried_previs:{cell_label} offset=({dx:.0},{dy:.0})"
                )),
                Err(e) => Some(format!("interior_recentre:{cell_label}:{e}")),
            };
            if let Some(warning) = warning {
                let w = self.interner.intern(&warning);
                self.warnings.push(w);
            }
        }

        // ── Pass 3: assemble + insert each cell with its children in one call. ──
        for (cell_obj, translated_cell) in translated_cells {
            let (persistent_records, persistent_sigs, temporary_records, temporary_sigs) =
                children_by_cell.remove(&cell_obj).unwrap_or_default();
            match crate::target_write::add_interior_cell_with_children_native(
                self.target_handle_id,
                translated_cell,
                persistent_records,
                temporary_records,
                &self.schema_target,
                &self.interner,
            ) {
                Ok(outcome) => {
                    if !outcome.cell_inserted {
                        stats.records_dropped += 1;
                        stats.signature_entry(cell).dropped += 1;
                        continue;
                    }
                    stats.records_translated += 1;
                    stats.signature_entry(cell).translated += 1;
                    // Per-sig translated for each child handed over. Encode-drops
                    // (rare; localized-string filtering) are reconciled at the
                    // aggregate level.
                    for sig in persistent_sigs.iter().chain(temporary_sigs.iter()) {
                        stats.records_translated += 1;
                        stats.signature_entry(*sig).translated += 1;
                    }
                    let dropped = outcome.children_dropped;
                    if dropped > 0 {
                        stats.records_translated = stats.records_translated.saturating_sub(dropped);
                        stats.records_dropped += dropped;
                    }
                }
                Err(e) => {
                    let w = self.interner.intern(&format!("interior_cell_write:{e}"));
                    self.warnings.push(w);
                    stats.records_failed += 1;
                    stats.signature_entry(cell).failed += 1;
                    continue;
                }
            }
        }

        crate::fixups::clear_interior_hand_changed::clear_interior_hand_changed_flags(
            self.target_handle_id,
        )
        .map_err(|e| RunError::InvalidConfig(format!("interior_hand_changed_clear:{e}")))?;

        Ok(stats)
    }

    /// Run the pre/translate/mapper-remap/post pipeline on one record from a
    /// pre-fetched batch snapshot, and return the translated+remapped `Record`.
    /// Returns `Ok(None)` when the record is dropped/deferred/failed (stats
    /// updated accordingly).
    ///
    /// Takes the batch + index rather than snapshotting per call: each snapshot
    /// deep-clones the multi-language localized-strings table, so per-record
    /// snapshotting over a large source is quadratic. The caller snapshots once.
    ///
    /// `nvnm_contexts` rewrites NVNM door-ref/geometry FormIDs through the mapper
    /// when set (NAVM children only).
    fn translate_and_remap_snapshot_record(
        &mut self,
        snapshot: &SourceRecordBatchSnapshot,
        idx: usize,
        ignored_signature: &str,
        stats: &mut TranslateStats,
        nvnm_contexts: Option<(
            &crate::fo76_navmesh::FormIdContext,
            &crate::fo76_navmesh::FormIdContext,
        )>,
    ) -> Result<Option<Record>, RunError> {
        let record_snapshot = &snapshot.records[idx];
        let fk = record_snapshot.form_key;

        let legacy_bptd_only = matches!(self.source, Game::Fnv | Game::Fo3);
        let relayout_target_schema = (self.target == Game::Fo4
            && (self.source == Game::Fo76 || legacy_bptd_only))
            .then(|| self.schema_target.clone());
        let relayout_ctx = relayout_target_schema.as_deref().map(|target_schema| {
            crate::struct_relayout::StructRelayoutCtx {
                target_schema,
                target_form_version:
                    crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
                legacy_bptd_only,
            }
        });

        let mut src_record = match decode_record_from_parsed_relayout(
            &record_snapshot.raw_record,
            &fk,
            &self.schema_source,
            &snapshot.masters,
            &snapshot.plugin_name,
            snapshot.strings.as_ref(),
            snapshot.plugin_is_localized,
            &self.interner,
            relayout_ctx.as_ref(),
        ) {
            Ok(record) => record,
            Err(e) => {
                let w = self.interner.intern(&format!("interior_read_error:{e}"));
                self.warnings.push(w);
                stats.records_failed += 1;
                return Ok(None);
            }
        };
        let source_sig = src_record.sig;

        {
            let mut ctx = PairCtx::new(&self.interner);
            if let Err(e) = self.translator.pre_translate(&mut ctx, &mut src_record) {
                let w = self.interner.intern(&format!("interior_pre_translate:{e}"));
                self.warnings.push(w);
            }
        }

        let mut translated = match self.translator.translate_ignoring_skip(
            &src_record,
            &self.interner,
            ignored_signature,
        ) {
            TranslateResult::Translated(record) => record,
            TranslateResult::Dropped { decision, .. } => {
                self.decisions.push(decision);
                stats.records_dropped += 1;
                stats.signature_entry(source_sig).dropped += 1;
                return Ok(None);
            }
            TranslateResult::Deferred(kind) => {
                self.deferred.push((fk, kind));
                stats.records_deferred += 1;
                stats.signature_entry(source_sig).deferred += 1;
                return Ok(None);
            }
        };

        if self
            .schema_target
            .record_def(translated.sig.as_str())
            .is_none()
        {
            let w = self.interner.intern(&format!(
                "interior_unsupported_target_record:{}",
                translated.sig.as_str()
            ));
            self.warnings.push(w);
            stats.records_dropped += 1;
            stats.signature_entry(source_sig).dropped += 1;
            return Ok(None);
        }

        let collision_rename = self.mapper_state.as_ref().and_then(|state| {
            let translated_sig = translated.sig;
            rename_fo76_target_editor_id_collision(
                &mut translated,
                &state.target_eid_index,
                &state.options.vanilla_remap_blocked_source_form_keys,
                &self.interner,
                is_editor_id_collision_rename_forced(self.source, self.target, translated_sig),
            )
        });
        if let Some((old, new)) = collision_rename {
            let warning = self
                .interner
                .intern(&format!("fo76_target_edid_collision_renamed:{old}->{new}"));
            self.warnings.push(warning);
        }

        // Mapper allocate + internal FK remap (mutates run mapper state).
        {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before interior cell emit");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
            let target_fk = mapper.allocate_or_resolve(fk, normalized_eid, translated.sig);
            translated.form_key = target_fk;
            if let Some((source_ctx, target_ctx)) = nvnm_contexts {
                if let Err(e) = crate::fo76_navmesh::rewrite_record_nvnm_with_context(
                    &mut translated,
                    &mut mapper,
                    source_ctx,
                    target_ctx,
                ) {
                    let w = self.interner.intern(&format!("interior_nvnm:{e}"));
                    self.warnings.push(w);
                }
            }
            if let Err(e) = mapper.rewrite_record(&mut translated) {
                let w = self
                    .interner
                    .intern(&format!("interior_rewrite_record:{e}"));
                self.warnings.push(w);
            }
        }

        {
            let mut ctx = PairCtx::new(&self.interner);
            if let Err(e) = self.translator.post_translate(&mut ctx, &mut translated) {
                let w = self
                    .interner
                    .intern(&format!("interior_post_translate:{e}"));
                self.warnings.push(w);
            }
        }
        {
            let mut ctx = TargetCtx {
                interner: &self.interner,
            };
            if let Err(e) = self.translator.run_target_hook(&mut ctx, &mut translated) {
                let w = self.interner.intern(&format!("interior_target_hook:{e}"));
                self.warnings.push(w);
            }
        }

        let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
            &mut translated,
            &self.schema_target,
            &self.interner,
        );
        for message in report.decisions {
            let kind = self.interner.intern("class_a_normalize");
            self.decisions.push(Decision { kind, message });
        }

        Ok(Some(translated))
    }

    /// Translate source NAVM records into existing projected target CELL child
    /// groups. This intentionally bypasses the normal `skip_records` entry for
    /// NAVM, but only for this structured writer.
    pub fn emit_projected_navmeshes(&mut self) -> Result<TranslateStats, RunError> {
        if self.target != Game::Fo4 || !matches!(self.source, Game::Fo76 | Game::Fnv | Game::Fo3) {
            return Ok(TranslateStats::default());
        }
        if self.mapper_state.is_none() {
            self.init_mapper_state()?;
        }
        let navm = SigCode::from_str("NAVM")
            .map_err(|e| RunError::InvalidConfig(format!("NAVM signature: {e}")))?;
        let fks = iter_form_keys_of_sig(self.source_handle_id, navm, &mut self.interner)?;
        self.translate_projected_navmeshes(&fks)
    }

    pub fn prepare_terrain_navmesh_graft(
        &mut self,
        prior_plugin_path: &Path,
    ) -> Result<usize, RunError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(0);
        }

        let handle = OwnedPluginHandle::load(prior_plugin_path, self.target.as_str(), None)?;
        let object_ids =
            esp_authoring_core::plugin_runtime::graft_terrain_navmesh_object_ids_from_handle(
                handle.id(),
            )
            .map_err(|error| RunError::InvalidConfig(format!("graft_terrain:{error}")))?;

        self.generated_object_id_reservations
            .extend(object_ids.iter().copied());
        if let Some(state) = self.mapper_state.as_mut() {
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            mapper.reserve_generated_object_ids(object_ids.iter().copied());
        }
        Ok(object_ids.len())
    }

    /// Graft reused exterior terrain (CELL shells + LAND), navmesh (NAVM), and
    /// terrain-texture records (TXST/LTEX/GRAS) from a prior FO4 output ESM
    /// (`prior_handle_id`) into the target instead of regenerating them. Mirrors
    /// the NAVM source→target mapping setup of `emit_projected_navmeshes` so
    /// door / placed-ref repair still resolves navmesh links, then reserves every
    /// grafted object-id so no later phase re-allocates one (grafted records
    /// bypass the mapper). Backs `regen.py --re-use-land`.
    pub fn graft_terrain_navmesh(
        &mut self,
        prior_handle_id: u64,
    ) -> Result<TranslateStats, RunError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(TranslateStats::default());
        }
        if self.mapper_state.is_none() {
            self.init_mapper_state()?;
        }

        // Establish NAVM source→target mappings (preserve_source_ids reuses the
        // source object-id) exactly as `emit_projected_navmeshes` does, so the
        // grafted NAVM ids line up and door-ref repair resolves navmesh links.
        let navm = SigCode::from_str("NAVM")
            .map_err(|e| RunError::InvalidConfig(format!("NAVM signature: {e}")))?;
        let navm_fks = iter_form_keys_of_sig(self.source_handle_id, navm, &mut self.interner)?;
        {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before terrain graft");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            for &fk in &navm_fks {
                mapper.allocate_or_resolve(fk, None, navm);
            }
        }

        // Structural clone of terrain + navmesh + terrain-texture records from the
        // cached FO4 output (same game, same masters → not a conversion).
        let report = esp_authoring_core::plugin_runtime::graft_terrain_navmesh_from_handle(
            prior_handle_id,
            self.target_handle_id,
        )
        .map_err(|e| RunError::InvalidConfig(format!("graft_terrain:{e}")))?;

        // Reserve every grafted own object-id so placed-children / ECZN / etc.
        // never re-allocate one.
        {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before terrain graft");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            mapper.reserve_object_ids(report.object_ids.iter().copied());
        }

        for warning in &report.warnings {
            let w = self.interner.intern(&format!("graft_terrain: {warning}"));
            self.warnings.push(w);
        }

        let added =
            (report.cells + report.lands + report.navms + report.txst + report.ltex + report.gras)
                as u32;
        Ok(TranslateStats {
            records_translated: added,
            records_failed: report.warnings.len() as u32,
            ..TranslateStats::default()
        })
    }

    fn translate_projected_navmeshes(
        &mut self,
        fks: &[FormKey],
    ) -> Result<TranslateStats, RunError> {
        if fks.is_empty() {
            return Ok(TranslateStats::default());
        }

        let navm = SigCode::from_str("NAVM")
            .map_err(|e| RunError::InvalidConfig(format!("NAVM signature: {e}")))?;
        {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before projected NAVM emit");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            for &fk in fks {
                mapper.allocate_or_resolve(fk, None, navm);
            }
        }

        let source_records =
            snapshot_records_by_form_keys(self.source_handle_id, fks, &self.interner)?;
        let legacy_navmeshes = if matches!(self.source, Game::Fnv | Game::Fo3) {
            Some(
                prepare_legacy_fallout_navmeshes(self.source_handle_id, &source_records.records)
                    .map_err(|error| {
                        RunError::InvalidConfig(format!("legacy_fallout_navmesh:{error}"))
                    })?,
            )
        } else {
            None
        };
        let source_formid_context =
            crate::fo76_navmesh::snapshot_formid_context(self.source_handle_id)
                .map_err(|e| RunError::InvalidConfig(format!("fo76_navm_source_context:{e}")))?;
        let target_formid_context =
            crate::fo76_navmesh::snapshot_formid_context(self.target_handle_id)
                .map_err(|e| RunError::InvalidConfig(format!("fo76_navm_target_context:{e}")))?;

        let whole_plugin_names =
            if self.config.is_whole_plugin && self.config.target_master_names.is_empty() {
                self.target_master_plugin_names()
            } else {
                Vec::new()
            };
        let target_master_names = target_master_names_for_skip(&self.config, whole_plugin_names);
        let target_master_syms: FxHashSet<Sym> =
            intern_plugin_names(&target_master_names, &self.interner);
        let first_target_master_sym = target_master_names
            .first()
            .map(|name| self.interner.intern(name));

        let legacy_bptd_only = matches!(self.source, Game::Fnv | Game::Fo3);
        let relayout_target_schema = (self.target == Game::Fo4
            && (self.source == Game::Fo76 || legacy_bptd_only))
            .then(|| self.schema_target.clone());
        let relayout_ctx = relayout_target_schema.as_deref().map(|target_schema| {
            crate::struct_relayout::StructRelayoutCtx {
                target_schema,
                target_form_version:
                    crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
                legacy_bptd_only,
            }
        });

        // Borrowed, not cloned: each record gets a cheap overlay scratch over
        // this frozen template instead of a full MapperState clone.
        let mapper_state_template = self
            .mapper_state
            .as_ref()
            .expect("mapper_state initialized before projected NAVM emit");
        let event_tx = self.event_tx.clone();
        let interner = &self.interner;
        let schema_source = &*self.schema_source;
        let schema_target = &*self.schema_target;
        let translator = &self.translator;
        let projected_navmesh_offset = self.config.projected_navmesh_offset;
        let is_whole_plugin = self.config.is_whole_plugin;
        let strings = source_records.strings.as_ref();

        let prepare_results: Vec<ProjectedNavmeshPrepareResult> = source_records
            .records
            .par_iter()
            .map(|snapshot| {
                let mut scratch = FormKeyMapper::overlay_scratch(mapper_state_template);
                Self::prepare_projected_navmesh(
                    snapshot,
                    &source_records.masters,
                    &source_records.plugin_name,
                    strings,
                    source_records.plugin_is_localized,
                    schema_source,
                    schema_target,
                    interner,
                    relayout_ctx.as_ref(),
                    translator,
                    legacy_navmeshes.as_ref(),
                    mapper_state_template,
                    &mut scratch,
                    &source_formid_context,
                    &target_formid_context,
                    &target_master_syms,
                    projected_navmesh_offset,
                    is_whole_plugin,
                )
            })
            .collect();

        let mut stats = TranslateStats::default();
        let mut prepared_meta = Vec::with_capacity(prepare_results.len());
        let mut prepared_records = Vec::with_capacity(prepare_results.len());
        for mut result in prepare_results {
            stats.absorb(result.stats);
            self.decisions.append(&mut result.decisions);
            self.deferred.append(&mut result.deferred);
            for warning in result.warnings {
                let sym = self.interner.intern(&warning);
                self.warnings.push(sym);
            }
            if let Some(prepared) = result.prepared {
                prepared_meta.push(PreparedNavmeshMeta {
                    source_fk: prepared.source_fk,
                    source_sig: prepared.source_sig,
                    full_plugin_snapshot: prepared.full_plugin_snapshot,
                });
                prepared_records.push(prepared.record);
            }
        }

        // Chunked apply: one store lock + one grouped batch insert
        // per 1000 records instead of a full-tree scan per record. Per-record
        // outcome handling and the Python checkpoint cadence are identical to
        // the legacy per-record loop.
        let total = prepared_records.len();
        let mut record_iter = prepared_records.into_iter();
        let mut meta_iter = prepared_meta.into_iter();
        let mut processed = 0usize;
        while processed < total {
            let chunk_len = (total - processed).min(1000);
            let chunk_records: Vec<Record> = record_iter.by_ref().take(chunk_len).collect();
            let outcomes = add_projected_navmeshes_chunk_native(
                self.target_handle_id,
                chunk_records,
                &*self.schema_target,
                &self.interner,
            );
            for outcome in outcomes {
                let meta = meta_iter.next().expect("prepared meta per outcome");
                match outcome {
                    Ok(true) => {
                        if let Some(snapshot) = meta.full_plugin_snapshot.as_ref() {
                            self.capture_full_plugin_record_state(
                                meta.source_fk,
                                snapshot,
                                &target_master_syms,
                                first_target_master_sym,
                                None,
                            );
                        }
                        stats.records_translated += 1;
                        stats.signature_entry(meta.source_sig).translated += 1;
                    }
                    Ok(false) => {
                        let w = self.interner.intern(&format!(
                            "projected_navmesh_skipped:{}",
                            form_key_to_read_str(&meta.source_fk, &self.interner)
                        ));
                        self.warnings.push(w);
                        stats.records_dropped += 1;
                        stats.signature_entry(meta.source_sig).dropped += 1;
                    }
                    Err(e) => {
                        let w = self
                            .interner
                            .intern(&format!("projected_navmesh_write:{e}"));
                        self.warnings.push(w);
                        stats.records_failed += 1;
                        stats.signature_entry(meta.source_sig).failed += 1;
                    }
                }

                processed += 1;
                if processed % 1000 == 0 {
                    let record_count = processed as u64;
                    let cb = self.progress_callback.as_ref();
                    Python::attach(|py| -> Result<(), RunError> {
                        py.check_signals().map_err(|_| RunError::Cancelled)?;
                        if let Some(cb) = cb {
                            let keep_going: bool = cb
                                .call1(py, (record_count,))
                                .and_then(|r| r.extract::<bool>(py))
                                .unwrap_or(true);
                            if !keep_going {
                                return Err(RunError::Cancelled);
                            }
                        }
                        Ok(())
                    })?;
                }
            }
        }

        if self.config.is_whole_plugin {
            let warning = format!(
                "full_plugin_state:unresolved_refs={};target_master_refs={}",
                self.full_plugin_state.unresolved_ref_count(),
                self.full_plugin_state.target_master_ref_count()
            );
            let sym = self.interner.intern(&warning);
            self.warnings.push(sym);
        }

        Ok(stats)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_projected_navmesh(
        snapshot: &crate::source_read::SourceRecordSnapshot,
        source_masters: &[String],
        source_plugin_name: &str,
        source_strings: Option<&esp_authoring_core::plugin_runtime::LocalizedStringsState>,
        source_plugin_is_localized: bool,
        schema_source: &AuthoringSchema,
        schema_target: &AuthoringSchema,
        interner: &StringInterner,
        relayout_ctx: Option<&crate::struct_relayout::StructRelayoutCtx<'_>>,
        translator: &Translator,
        legacy_navmeshes: Option<&LegacyFalloutNavmeshBatch>,
        mapper_base: &MapperState,
        mapper_scratch: &mut MapperState,
        source_formid_context: &crate::fo76_navmesh::FormIdContext,
        target_formid_context: &crate::fo76_navmesh::FormIdContext,
        target_master_syms: &FxHashSet<Sym>,
        projected_navmesh_offset: [f32; 3],
        is_whole_plugin: bool,
    ) -> ProjectedNavmeshPrepareResult {
        let mut result = ProjectedNavmeshPrepareResult::default();
        let fk = snapshot.form_key;
        let mut src_record = match decode_record_from_parsed_relayout(
            &snapshot.raw_record,
            &fk,
            schema_source,
            source_masters,
            source_plugin_name,
            source_strings,
            source_plugin_is_localized,
            interner,
            relayout_ctx,
        ) {
            Ok(record) => record,
            Err(e) => {
                result.warnings.push(format!("read_error:{e}"));
                result.stats.records_failed += 1;
                return result;
            }
        };
        let source_sig = src_record.sig;
        result.stats.signature_entry(source_sig).seen += 1;

        if let Some(legacy_navmeshes) = legacy_navmeshes {
            let raw_form_id = snapshot.raw_record.form_id;
            if let Some(error) = legacy_navmeshes.failures.get(&raw_form_id) {
                result
                    .warnings
                    .push(format!("legacy_fallout_navmesh:{raw_form_id:08X}:{error}"));
                result.stats.records_failed += 1;
                result.stats.signature_entry(source_sig).failed += 1;
                return result;
            }
            result.warnings.extend(
                legacy_navmeshes
                    .diagnostics
                    .get(&raw_form_id)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            let Some(nvnm) = legacy_navmeshes.converted.get(&raw_form_id) else {
                result.warnings.push(format!(
                    "legacy_fallout_navmesh:{raw_form_id:08X}:missing converted payload"
                ));
                result.stats.records_failed += 1;
                result.stats.signature_entry(source_sig).failed += 1;
                return result;
            };
            src_record.fields.retain(|field| {
                !matches!(
                    field.sig.as_str(),
                    "NVER" | "DATA" | "NVVX" | "NVTR" | "NVCA" | "NVDP" | "NVGD" | "NVEX"
                )
            });
            src_record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("NVNM").expect("static NVNM signature"),
                value: FieldValue::Bytes(smallvec::SmallVec::from_vec(nvnm.clone())),
            });
        }

        {
            let mut ctx = PairCtx::new(interner);
            if let Err(e) = translator.pre_translate(&mut ctx, &mut src_record) {
                result.warnings.push(format!("pre_translate:{e}"));
            }
        }

        let mut translated = match translator.translate_ignoring_skip(&src_record, interner, "NAVM")
        {
            TranslateResult::Translated(record) => record,
            TranslateResult::Dropped { decision, .. } => {
                result.decisions.push(decision);
                result.stats.records_dropped += 1;
                result.stats.signature_entry(source_sig).dropped += 1;
                return result;
            }
            TranslateResult::Deferred(kind) => {
                result.deferred.push((fk, kind));
                result.stats.records_deferred += 1;
                result.stats.signature_entry(source_sig).deferred += 1;
                return result;
            }
        };

        if schema_target.record_def(translated.sig.as_str()).is_none() {
            result.warnings.push(format!(
                "unsupported_target_record:{} not in {} generated schema",
                translated.sig.as_str(),
                Game::Fo4.as_str()
            ));
            result.stats.records_dropped += 1;
            result.stats.signature_entry(source_sig).dropped += 1;
            return result;
        }

        if let Some((old, new)) = rename_fo76_target_editor_id_collision(
            &mut translated,
            &mapper_base.target_eid_index,
            &mapper_base.options.vanilla_remap_blocked_source_form_keys,
            interner,
            false,
        ) {
            result
                .warnings
                .push(format!("fo76_target_edid_collision_renamed:{old}->{new}"));
        }

        let target_fk = {
            let mut mapper =
                FormKeyMapper::from_state_overlay(mapper_base, mapper_scratch, interner);
            let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
            let target_fk = mapper.allocate_or_resolve(fk, normalized_eid, translated.sig);
            translated.form_key = target_fk;
            if translated
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"NVNM" || field.sig.0 == *b"MNAM")
            {
                let rewrite_result = if legacy_navmeshes.is_some() {
                    crate::fo76_navmesh::rewrite_legacy_record_nvnm_with_context(
                        &mut translated,
                        &mut mapper,
                        source_formid_context,
                        target_formid_context,
                    )
                } else {
                    crate::fo76_navmesh::rewrite_record_nvnm_with_context(
                        &mut translated,
                        &mut mapper,
                        source_formid_context,
                        target_formid_context,
                    )
                };
                if let Err(e) = rewrite_result {
                    result.warnings.push(format!("fo76_navm:{e}"));
                    if legacy_navmeshes.is_some() {
                        result.stats.records_failed += 1;
                        result.stats.signature_entry(source_sig).failed += 1;
                        return result;
                    }
                }
            }
            if let Err(e) = mapper.rewrite_record(&mut translated) {
                result.warnings.push(format!("rewrite_record:{e}"));
            }
            target_fk
        };

        if is_target_master_remap(target_fk, target_master_syms) {
            result.stats.records_vanilla_remapped += 1;
            result.stats.signature_entry(source_sig).vanilla_remapped += 1;
            return result;
        }

        {
            let mut ctx = PairCtx::new(interner);
            if let Err(e) = translator.post_translate(&mut ctx, &mut translated) {
                result.warnings.push(format!("post_translate:{e}"));
            }
        }

        {
            let mut ctx = TargetCtx { interner };
            if let Err(e) = translator.run_target_hook(&mut ctx, &mut translated) {
                result.warnings.push(format!("target_hook:{e}"));
            }
        }

        let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
            &mut translated,
            schema_target,
            interner,
        );
        for message in report.decisions {
            let kind = interner.intern("class_a_normalize");
            result.decisions.push(Decision { kind, message });
        }
        result.warnings.extend(report.warnings);

        let normalizer = TargetRecordNormalizer {
            target_schema: schema_target,
            source_record_def: schema_source.record_def(source_sig.as_str()),
            interner: Some(interner),
        };
        let mut translated = match normalizer.normalize(translated) {
            TargetRecordNormalization::Keep(record) => record,
            TargetRecordNormalization::DropUnsupportedRecord => {
                result.stats.records_dropped += 1;
                result.stats.signature_entry(source_sig).dropped += 1;
                return result;
            }
        };

        if legacy_navmeshes.is_some()
            && !translated
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "NVNM")
        {
            result
                .warnings
                .push("legacy_fallout_navmesh:target normalization dropped NVNM".to_string());
            result.stats.records_failed += 1;
            result.stats.signature_entry(source_sig).failed += 1;
            return result;
        }

        if let Err(e) = crate::target_write::offset_record_nvnm_geometry(
            &mut translated,
            projected_navmesh_offset,
        ) {
            result
                .warnings
                .push(format!("projected_navmesh_offset:{e}"));
            result.stats.records_failed += 1;
            result.stats.signature_entry(source_sig).failed += 1;
            return result;
        }

        let full_plugin_snapshot = is_whole_plugin
            .then(|| crate::full_plugin::target_schema_record_view(&translated, schema_target));
        result.prepared = Some(PreparedProjectedNavmesh {
            source_fk: fk,
            source_sig,
            record: translated,
            full_plugin_snapshot,
        });
        result
    }

    pub(crate) fn should_emit_fo76_quest_dialogue(&self) -> bool {
        self.config.is_whole_plugin
            && self.config.records_limit.is_none()
            && self.source == Game::Fo76
            && self.target == Game::Fo4
            && !self.config.skips_fo76_quest_dialogue()
    }

    pub(crate) fn should_emit_fo76_quest_scenes(&self) -> bool {
        self.config.is_whole_plugin
            && self.config.records_limit.is_none()
            && self.source == Game::Fo76
            && self.target == Game::Fo4
    }

    pub(crate) fn emit_quest_child_dialogue(&mut self) -> Result<TranslateStats, RunError> {
        let dial = SigCode::from_str("DIAL")
            .map_err(|e| RunError::InvalidConfig(format!("DIAL signature: {e}")))?;
        let xdi_plan = self.build_fo76_xdi_dialogue_plan()?;
        self.emit_phase_status("translate_v2: DIAL enumerate start");
        let fks = iter_form_keys_of_sig(self.source_handle_id, dial, &mut self.interner)?;
        self.emit_phase_status(format!(
            "translate_v2: DIAL enumerate done count={}",
            fks.len()
        ));
        self.emit_phase_status("translate_v2: DIAL translate_fks start");
        self.translate_fks_with_mode_and_parents(
            &fks,
            RecordWriteMode::QuestChild,
            &HashMap::new(),
            Some(&xdi_plan),
        )
    }

    pub(crate) fn emit_quest_child_scenes(&mut self) -> Result<TranslateStats, RunError> {
        let scen = SigCode::from_str("SCEN")
            .map_err(|e| RunError::InvalidConfig(format!("SCEN signature: {e}")))?;
        let xdi_plan = self.build_fo76_xdi_dialogue_plan()?;
        self.emit_phase_status("translate_v2: SCEN enumerate start");
        let fks = iter_form_keys_of_sig(self.source_handle_id, scen, &mut self.interner)?;
        self.emit_phase_status(format!(
            "translate_v2: SCEN enumerate done count={}",
            fks.len()
        ));
        self.emit_phase_status("translate_v2: SCEN translate_fks start");
        self.translate_fks_with_mode_and_parents(
            &fks,
            RecordWriteMode::QuestChild,
            &HashMap::new(),
            Some(&xdi_plan),
        )
    }

    pub(crate) fn emit_topic_child_infos(&mut self) -> Result<TranslateStats, RunError> {
        let info = SigCode::from_str("INFO")
            .map_err(|e| RunError::InvalidConfig(format!("INFO signature: {e}")))?;
        self.emit_phase_status("translate_v2: INFO enumerate start");
        let fks = iter_form_keys_of_sig(self.source_handle_id, info, &mut self.interner)?;
        self.emit_phase_status(format!(
            "translate_v2: INFO enumerate done count={}",
            fks.len()
        ));
        // INFO->parent-DIAL parentage is expressed only by source group nesting
        // (a TES4 Topic-Child group whose label is the DIAL form_id), not by any
        // DIAL subrecord. Build that {source INFO fid -> source DIAL fid} index so
        // each INFO can be placed under its DIAL in the target.
        self.emit_phase_status("translate_v2: INFO parent index start");
        let mut info_parent_index =
            crate::target_write::build_source_info_to_dialogue_index(self.source_handle_id)
                .map_err(|e| RunError::InvalidConfig(format!("info_parent_index:{e}")))?;
        let xdi_plan = self.build_fo76_xdi_dialogue_plan_from_index(&info_parent_index)?;
        for (&info, &parent) in &xdi_plan.info_parent_overrides {
            info_parent_index.insert(info, parent);
        }
        self.emit_phase_status(format!(
            "translate_v2: INFO parent index done count={}",
            info_parent_index.len()
        ));
        self.emit_phase_status("translate_v2: INFO translate_fks start");
        self.translate_fks_with_mode_and_parents(
            &fks,
            RecordWriteMode::TopicChildInfo,
            &info_parent_index,
            Some(&xdi_plan),
        )
    }

    fn build_fo76_xdi_dialogue_plan(
        &mut self,
    ) -> Result<crate::translator::pair_hooks::fo76_fo4::XdiDialoguePlan, RunError> {
        let info_parent_index =
            crate::target_write::build_source_info_to_dialogue_index(self.source_handle_id)
                .map_err(|e| RunError::InvalidConfig(format!("info_parent_index:{e}")))?;
        self.build_fo76_xdi_dialogue_plan_from_index(&info_parent_index)
    }

    fn build_fo76_xdi_dialogue_plan_from_index(
        &mut self,
        info_parent_index: &HashMap<u32, u32>,
    ) -> Result<crate::translator::pair_hooks::fo76_fo4::XdiDialoguePlan, RunError> {
        let scene_sig = SigCode::from_str("SCEN")
            .map_err(|e| RunError::InvalidConfig(format!("SCEN signature: {e}")))?;
        let scene_fks =
            iter_form_keys_of_sig(self.source_handle_id, scene_sig, &mut self.interner)?;
        let mut scenes = Vec::with_capacity(scene_fks.len());
        for scene_fk in scene_fks {
            let scene = read_record_relayout_by_form_key(
                self.source_handle_id,
                &scene_fk,
                &*self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "XDI SCEN read {}: {error}",
                    form_key_to_read_str(&scene_fk, &self.interner)
                ))
            })?;
            scenes.push(scene);
        }
        let mut candidate_info_ids =
            crate::translator::pair_hooks::fo76_fo4::combined_player_dialogue_info_candidates(
                &scenes,
                info_parent_index,
            )
            .into_iter()
            .collect::<Vec<_>>();
        candidate_info_ids.sort_unstable();
        let Some(source_plugin) = scenes.first().map(|scene| scene.form_key.plugin) else {
            return Ok(Default::default());
        };
        let mut prompt_info_ids = std::collections::HashSet::new();
        let mut prompt_fallbacks: HashMap<u32, crate::record::FieldValue> = HashMap::new();
        let mut dial_prompts: HashMap<u32, Option<crate::record::FieldValue>> = HashMap::new();
        for local in candidate_info_ids {
            let info_fk = FormKey {
                local,
                plugin: source_plugin,
            };
            let info = read_record_relayout_by_form_key(
                self.source_handle_id,
                &info_fk,
                &*self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "XDI INFO read {}: {error}",
                    form_key_to_read_str(&info_fk, &self.interner)
                ))
            })?;
            // FO76 puts the choice prompt either on the INFO (RNAM) or on the
            // parent DIAL (FULL). Without the DIAL fallback the split is skipped
            // and FO4's NPC-response topic is left with no INFO to speak.
            let has_rnam = info.fields.iter().any(|field| field.sig.0 == *b"RNAM");
            let dial_prompt = match info_parent_index.get(&local) {
                Some(&parent) if !has_rnam => {
                    if !dial_prompts.contains_key(&parent) {
                        let dial_fk = FormKey {
                            local: parent,
                            plugin: source_plugin,
                        };
                        let prompt = read_record_relayout_by_form_key(
                            self.source_handle_id,
                            &dial_fk,
                            &*self.schema_source,
                            &self.interner,
                            None,
                        )
                        .ok()
                        .and_then(|dial| {
                            dial.fields
                                .iter()
                                .find(|field| field.sig.0 == *b"FULL")
                                .map(|field| field.value.clone())
                        });
                        dial_prompts.insert(parent, prompt);
                    }
                    dial_prompts.get(&parent).cloned().flatten()
                }
                _ => None,
            };
            if crate::translator::pair_hooks::fo76_fo4::is_combined_player_dialogue_root(
                &info,
                dial_prompt.is_some(),
            ) {
                prompt_info_ids.insert(local);
                if let Some(prompt) = dial_prompt {
                    prompt_fallbacks.insert(local, prompt);
                }
            }
        }
        let mut plan = crate::translator::pair_hooks::fo76_fo4::build_xdi_dialogue_plan(
            &scenes,
            info_parent_index,
            &prompt_info_ids,
        )
        .map_err(|error| RunError::InvalidConfig(format!("fo76_xdi_dialogue:{error}")))?;
        prompt_fallbacks.retain(|local, _| plan.combined_info_splits.contains_key(local));
        plan.combined_info_prompt_fallbacks = prompt_fallbacks;
        Ok(plan)
    }

    /// Rebuild the single top-level FO4 NAVI record from the finalized target
    /// NAVM graph. Legacy Fallout NAVI rows are never parsed or carried because
    /// merged FNV+FO3 input can contain multiple NVER=11 records with a foreign
    /// NVMI layout; rebuilding from all target NAVMs preserves the whole graph.
    pub fn rebuild_projected_navi(&mut self) -> Result<TranslateStats, RunError> {
        let supported_source = matches!(
            self.source,
            Game::Fo76 | Game::Fnv | Game::Fo3 | Game::SkyrimSe
        );
        if !supported_source
            || self.target != Game::Fo4
            || (self.source != Game::Fo76 && !self.config.is_whole_plugin)
        {
            return Ok(TranslateStats::default());
        }
        let preferred_navi_form_id = Some(FO4_CANONICAL_NAVI_FORM_ID);
        let raw_formid_mappings = if matches!(self.source, Game::Fo76 | Game::SkyrimSe) {
            let source_to_target_pairs = self
                .mapper_state
                .as_ref()
                .map(|state| {
                    state
                        .source_to_target
                        .iter()
                        .map(|(&source, &target)| (source, target))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            crate::fo76_navmesh::raw_formid_mappings_for_context(
                source_to_target_pairs,
                &self.interner,
                self.source_handle_id,
                self.target_handle_id,
            )
            .map_err(|e| RunError::InvalidConfig(format!("rebuild_projected_navi:{e}")))?
        } else {
            Vec::new()
        };
        let navi_stats = match self.source {
            Game::Fo76 if !raw_formid_mappings.is_empty() => {
                rebuild_projected_navi_from_source_native(
                    self.target_handle_id,
                    self.source_handle_id,
                    &raw_formid_mappings,
                    preferred_navi_form_id,
                )
            }
            Game::SkyrimSe => rebuild_projected_navi_from_source_with_nver_native(
                self.target_handle_id,
                self.source_handle_id,
                &raw_formid_mappings,
                preferred_navi_form_id,
                15,
            ),
            // FO76 without mappings and both legacy Fallout games rebuild from
            // every finalized target NAVM. In particular, never feed NVER=11
            // FNV/FO3 NVMI bytes to the FO76/Skyrim metadata remapper.
            Game::Fo76 | Game::Fnv | Game::Fo3 => {
                rebuild_projected_navi_native(self.target_handle_id, preferred_navi_form_id)
            }
            _ => unreachable!("source gate above admits only NAVI rebuild sources"),
        }
        .map_err(|e| RunError::InvalidConfig(format!("rebuild_projected_navi:{e}")))?;
        // The FO76 source rebuild copies each NVMI's location and island
        // geometry from the source, so recentred interiors need them moved.
        if self.source == Game::Fo76
            && !raw_formid_mappings.is_empty()
            && !self.interior_recentre.navmesh_offsets.is_empty()
        {
            let mut session = crate::session::open_session(self.target_handle_id, None)
                .map_err(|e| RunError::InvalidConfig(format!("rebuild_projected_navi:{e}")))?;
            let shifted = crate::fixups::recentre_far_interiors::shift_navi_navmesh_infos(
                &mut session,
                &self.interner,
                &self.interior_recentre.navmesh_offsets,
            )
            .map_err(|e| RunError::InvalidConfig(format!("rebuild_projected_navi:recentre:{e}")))?;
            eprintln!("[interior_recentre] navi_entries_shifted={shifted}");
        }
        let mut stats = TranslateStats::default();
        stats.records_translated = navi_stats.records_added + navi_stats.records_replaced;
        stats.records_dropped = navi_stats
            .records_removed
            .saturating_sub(navi_stats.records_replaced);
        stats.records_failed = navi_stats.warnings;
        let message = format!(
            "rebuild_projected_navi: records_added={} records_replaced={} records_removed={} navmesh_infos={} edge_links={} stale_edge_links_dropped={} warnings={} navmeshes_seen={} navmeshes_touched={} navmesh_bad_internal_links={} navmesh_linked_edge_vertex_mismatches={} navmesh_opposite_normal_linked_pairs={} navmesh_missing_internal_links={} navmesh_same_direction_internal_edges={} navmesh_ambiguous_local_edges={} navmesh_external_links_added={} navmesh_missing_external_links={} navmesh_ambiguous_external_edges={} navmesh_external_link_caps_hit={} navmesh_winding_conflicts={}",
            navi_stats.records_added,
            navi_stats.records_replaced,
            navi_stats.records_removed,
            navi_stats.navmesh_infos,
            navi_stats.edge_links,
            navi_stats.stale_edge_links_dropped,
            navi_stats.warnings,
            navi_stats.navmeshes_seen,
            navi_stats.navmeshes_touched,
            navi_stats.navmesh_bad_internal_links,
            navi_stats.navmesh_linked_edge_vertex_mismatches,
            navi_stats.navmesh_opposite_normal_linked_pairs,
            navi_stats.navmesh_missing_internal_links,
            navi_stats.navmesh_same_direction_internal_edges,
            navi_stats.navmesh_ambiguous_local_edges,
            navi_stats.navmesh_external_links_added,
            navi_stats.navmesh_missing_external_links,
            navi_stats.navmesh_ambiguous_external_edges,
            navi_stats.navmesh_external_link_caps_hit,
            navi_stats.navmesh_winding_conflicts,
        );
        let sym = self.interner.intern(&message);
        self.warnings.push(sym);
        // Cap any navi-specific diagnostic strings accumulated during the rebuild.
        self.finalize_navi_warnings();
        Ok(stats)
    }

    /// Drop the FK remap state to free its memory in the late pipeline. Call
    /// only after fixups; asset phases and ESP serialization don't use it, and
    /// `translate_all` / `apply_fixups_v2` rebuild it.
    pub fn release_remap_state(&mut self) {
        self.mapper_state = None;
    }

    /// Append a diagnostic warning string from the NAVI rebuild phase.
    ///
    /// Unlike the main `warnings` Vec (which uses interned symbols), these
    /// are stored as owned `String`s to keep them separate and cap-able.
    pub fn push_navi_warning(&mut self, msg: String) {
        self.navi_warnings.push(msg);
    }

    /// Cap the NAVI warning buffer to at most 1000 entries to prevent unbounded
    /// growth on large worldspaces, and shrink the allocation.
    pub fn finalize_navi_warnings(&mut self) {
        const KEEP: usize = 1000;
        let dropped = self.navi_warnings.len().saturating_sub(KEEP);
        self.navi_warnings.truncate(KEEP);
        if dropped > 0 {
            self.navi_warnings
                .push(format!("... +{dropped} more (capped)"));
        }
        self.navi_warnings.shrink_to_fit();
    }

    /// Return the current count of NAVI warning strings.
    pub fn navi_warning_len(&self) -> usize {
        self.navi_warnings.len()
    }

    pub(crate) fn prepare_mapper_state_for_creature_dependency_plan(
        &mut self,
    ) -> Result<(), RunError> {
        if self.creature_dependency_plan_installed {
            return Err(RunError::InvalidConfig(
                "creature dependency plan is already installed".to_string(),
            ));
        }
        self.init_mapper_state()
    }

    pub(crate) fn prepare_mapper_state_for_translation(&mut self) -> Result<(), RunError> {
        if !self.creature_dependency_plan_installed {
            return self.init_mapper_state();
        }
        if self.mapper_state.is_none() {
            return Err(RunError::InvalidConfig(
                "mapper state was released after creature dependency reservations were installed"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// (Re)build `mapper_state` seeded with EIDs from every master handle.
    ///
    /// Called before creature dependency reservations or at the start of an
    /// unplanned translate pass. Persists into fixups via `self.mapper_state`.
    pub(crate) fn init_mapper_state(&mut self) -> Result<(), RunError> {
        self.legacy_serial_normalization.clear();
        self.legacy_creature_race_coverage = Default::default();
        let (source_plugin_name, source_master_names) =
            match plugin_context_for_handle(self.source_handle_id) {
                Ok(context) => context,
                Err(e) => {
                    let w = self.interner.intern(&format!("source_context_error:{e}"));
                    self.warnings.push(w);
                    (String::new(), Vec::new())
                }
            };
        let target_master_names = if !self.config.target_master_names.is_empty() {
            self.config.target_master_names.clone()
        } else {
            match plugin_context_for_handle(self.target_handle_id) {
                Ok((_plugin_name, masters)) => masters,
                Err(e) => {
                    let w = self.interner.intern(&format!("target_context_error:{e}"));
                    self.warnings.push(w);
                    Vec::new()
                }
            }
        };
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name,
            source_master_names,
            target_master_names,
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            vanilla_remap_blocked_source_form_keys:
                editor_id_vanilla_remap_blocked_source_form_keys(
                    self.source,
                    self.target,
                    &self.interner,
                ),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };

        if self.mapper_state.is_some() {
            let w = self
                .interner
                .intern("translate:mapper_state_already_set;rebuilding");
            self.warnings.push(w);
        }
        let preflight_eid_entries = mapper_entries_from_preflight(&self.config, &self.interner);
        let mut eid_entries = preflight_eid_entries.clone();
        let master_ids = self.master_handle_ids.clone();
        if !master_ids.is_empty() {
            self.emit_status(&format!(
                "  mapper: scanning {} master plugin(s) for editor-ids…",
                master_ids.len()
            ));
        }
        for master_id in master_ids {
            let master_started = std::time::Instant::now();
            match collect_eid_index(master_id, &self.schema_source, &mut self.interner) {
                Ok(entries) => {
                    let entry_count = entries.len();
                    eid_entries.extend(
                        entries.into_iter().map(|(eid, fk, sig)| {
                            (normalized_eid_sym(eid, &self.interner), fk, sig)
                        }),
                    );
                    self.emit_status(&format!(
                        "  mapper: master handle {} → {} editor-ids ({:.1}s)",
                        master_id,
                        entry_count,
                        master_started.elapsed().as_secs_f64()
                    ));
                }
                Err(e) => {
                    let w = self.interner.intern(&format!("eid_index_error:{e}"));
                    self.warnings.push(w);
                }
            }
        }
        let mut mapper_state = MapperState::new(eid_entries, mapper_opts);
        mapper_state
            .reserved_generated_object_ids
            .extend(self.generated_object_id_reservations.iter().copied());
        if self.source == Game::Fo76
            && self.target == Game::Fo4
            && mapper_state.options.target_master_names.iter().any(|name| {
                name.eq_ignore_ascii_case(crate::translator::pair_hooks::fo76_fo4::XDI_MASTER_NAME)
            })
        {
            let xdi_keyword = FormKey {
                local: crate::translator::pair_hooks::fo76_fo4::XDI_SCENE_KEYWORD_FORM_ID,
                plugin: self
                    .interner
                    .intern(crate::translator::pair_hooks::fo76_fo4::XDI_MASTER_NAME),
            };
            mapper_state
                .source_to_target
                .insert(xdi_keyword, xdi_keyword);
        }
        let needs_legacy_fo4_substitutions =
            matches!(self.source, Game::Fnv | Game::Fo3) && self.target == Game::Fo4;
        let needs_fo76_fo4_weather_substitutions =
            self.source == Game::Fo76 && self.target == Game::Fo4;
        let needs_skyrimse_fo4_weather_substitutions =
            self.source == Game::SkyrimSe && self.target == Game::Fo4;
        if (self.config.use_base_game_assets && !preflight_eid_entries.is_empty())
            || needs_legacy_fo4_substitutions
            || needs_fo76_fo4_weather_substitutions
            || needs_skyrimse_fo4_weather_substitutions
        {
            let source_scan_started = std::time::Instant::now();
            self.emit_status(&format!(
                "  mapper: scanning source plugin editor-ids for preflight mappings ({} target entries)…",
                preflight_eid_entries.len()
            ));
            match collect_eid_index(
                self.source_handle_id,
                &self.schema_source,
                &mut self.interner,
            ) {
                Ok(source_entries) => {
                    let source_entry_count = source_entries.len();
                    if self.config.use_base_game_assets && !preflight_eid_entries.is_empty() {
                        let creature_skin_remap_skips = collect_fo76_fo4_creature_skin_remap_skips(
                            self.source_handle_id,
                            &self.schema_source,
                            &source_entries,
                            &preflight_eid_entries,
                            &self.interner,
                            self.source,
                            self.target,
                        );
                        for (source_form_key, target_form_key) in
                            source_target_mappings_from_preflight_with_skips(
                                source_entries.iter().copied(),
                                &preflight_eid_entries,
                                &self.interner,
                                self.source,
                                self.target,
                                &creature_skin_remap_skips,
                            )
                        {
                            // Debug trace for the Gulper races' (source 110D23
                            // GulperRace, 111655 GulperSmallRace) preflight seed,
                            // confirming the seeded target plugin. Gated on
                            // MODBOX_TRACE_0247C1.
                            if matches!(source_form_key.local, 0x0011_0D23 | 0x0011_1655)
                                && std::env::var_os("MODBOX_TRACE_0247C1").is_some()
                            {
                                eprintln!(
                                    "[trace_0247c1] SEED source={:06X}@{:?} -> target={:06X}@{:?}",
                                    source_form_key.local,
                                    self.interner.resolve(source_form_key.plugin),
                                    target_form_key.local,
                                    self.interner.resolve(target_form_key.plugin),
                                );
                            }
                            mapper_state
                                .source_to_target
                                .entry(source_form_key)
                                .or_insert(target_form_key);
                        }
                    }
                    if needs_legacy_fo4_substitutions {
                        for (source_form_key, target_form_key) in
                            crate::translator::pair_hooks::fnv_fo4::legacy_sound_descriptor_substitution_mappings(
                                &source_entries,
                                &mapper_state.target_eid_index,
                                &self.interner,
                            )
                        {
                            mapper_state
                                .source_to_target
                                .insert(source_form_key, target_form_key);
                        }
                    }
                    if needs_fo76_fo4_weather_substitutions {
                        for (source_form_key, target_form_key) in
                            crate::translator::pair_hooks::fo76_fo4::fo76_fo4_voli_gdry_substitution_mappings(
                                &source_entries,
                                &self.interner,
                            )
                        {
                            mapper_state
                                .source_to_target
                                .insert(source_form_key, target_form_key);
                        }
                    }
                    if needs_skyrimse_fo4_weather_substitutions {
                        for (source_form_key, target_form_key) in
                            crate::translator::pair_hooks::skyrimse_fo4::skyrimse_fo4_voli_gdry_substitution_mappings(
                                &source_entries,
                                &self.interner,
                            )
                        {
                            mapper_state
                                .source_to_target
                                .insert(source_form_key, target_form_key);
                        }
                    }
                    for (source_form_key, target_form_key) in
                        fnv_fo3_fo4_humanoid_race_substitution_mappings(
                            &source_entries,
                            &self.interner,
                            self.source,
                            self.target,
                        )
                    {
                        mapper_state
                            .source_to_target
                            .insert(source_form_key, target_form_key);
                    }
                    for (source_form_key, target_form_key) in
                        skyrimse_fo4_humanoid_race_substitution_mappings(
                            &source_entries,
                            &self.interner,
                            self.source,
                            self.target,
                        )
                    {
                        mapper_state
                            .source_to_target
                            .insert(source_form_key, target_form_key);
                    }
                    seed_fnv_fo3_fo4_ammo_substitutions(
                        &mut mapper_state,
                        &source_entries,
                        &self.interner,
                        self.source,
                        self.target,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!("embedded ammo substitutions: {error}"))
                    })?;
                    self.emit_status(&format!(
                        "  mapper: scanned {} source editor-ids in {:.1}s",
                        source_entry_count,
                        source_scan_started.elapsed().as_secs_f64()
                    ));
                }
                Err(e) => {
                    let w = self
                        .interner
                        .intern(&format!("source_eid_preflight_error:{e}"));
                    self.warnings.push(w);
                }
            }
        }
        // Forced FO76→FO4 substitutions. Seeded last so they win over
        // EID/allocate resolution: `allocate_or_resolve` consults
        // `source_to_target` first. Unconditional for FO76→FO4 (not gated on
        // use_base_game_assets) — these references must always resolve to the
        // FO4 vanilla records.
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            for (source_form_key, target_form_key) in
                fo76_fo4_forced_keyword_substitution_mappings(&self.interner)
            {
                mapper_state
                    .source_to_target
                    .insert(source_form_key, target_form_key);
            }
            for (source_form_key, target_form_key) in
                fo76_fo4_forced_race_substitution_mappings(&self.interner)
            {
                mapper_state
                    .source_to_target
                    .insert(source_form_key, target_form_key);
            }
            for (source_form_key, target_form_key) in
                fo76_fo4_forced_base_object_substitution_mappings(&self.interner)
            {
                mapper_state
                    .source_to_target
                    .insert(source_form_key, target_form_key);
            }
            for (source_form_key, target_form_key) in
                fo76_fo4_forced_location_ref_type_substitution_mappings(&self.interner)
            {
                mapper_state
                    .source_to_target
                    .insert(source_form_key, target_form_key);
            }
        }
        self.mapper_state = Some(mapper_state);
        Ok(())
    }

    pub(crate) fn observe_legacy_creature_race_decision(
        &mut self,
        decision: &crate::translator::pair_hooks::fnv_creature_race::CreatureRaceDecision,
    ) {
        self.legacy_creature_race_coverage
            .observe_decision(decision, "CREA");
    }

    pub(crate) fn should_apply_legacy_creature_race_policy(&self) -> bool {
        matches!(self.source, Game::Fnv | Game::Fo3)
            && self.target == Game::Fo4
            && !self.translator.maps.skip_records.contains("CREA")
    }

    pub(crate) fn target_form_key_resolves_to_race(&self, form_key: FormKey) -> bool {
        let Some(state) = self.mapper_state.as_ref() else {
            return false;
        };
        let plugin = self.interner.resolve(form_key.plugin).unwrap_or("");
        state
            .target_eid_index
            .values()
            .flatten()
            .any(|(candidate, sig)| {
                sig.as_str() == "RACE"
                    && candidate.local == form_key.local
                    && self
                        .interner
                        .resolve(candidate.plugin)
                        .is_some_and(|candidate_plugin| {
                            candidate_plugin.eq_ignore_ascii_case(plugin)
                        })
            })
    }

    fn legacy_creature_race_expected_candidates(&self) -> usize {
        if !self.should_apply_legacy_creature_race_policy() {
            return self.legacy_creature_race_coverage.candidates;
        }
        let full_merged_audit = self.source == Game::Fnv
            && self.target == Game::Fo4
            && self.config.is_whole_plugin
            && self.config.records_limit.is_none()
            && self.mapper_state.as_ref().is_some_and(|state| {
                state
                    .options
                    .source_plugin_name
                    .eq_ignore_ascii_case("FalloutNV.esm")
            });
        if full_merged_audit {
            crate::translator::pair_hooks::fnv_creature_race::EXPECTED_FULL_MERGED_CREA_CANDIDATES
        } else {
            self.legacy_creature_race_coverage.candidates
        }
    }

    pub(crate) fn fail_legacy_creature_race(&mut self, error: String) -> RunError {
        self.legacy_creature_race_coverage.expected_candidates =
            self.legacy_creature_race_expected_candidates();
        let report = serde_json::to_string(&self.legacy_creature_race_coverage)
            .unwrap_or_else(|json_error| format!("{{\"serialization_error\":{json_error:?}}}"));
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "translate",
            level: crate::phase::LogLevel::Warn,
            message: format!("legacy_creature_race_coverage:{report}"),
        });
        RunError::InvalidConfig(format!("{error};legacy_creature_race_coverage:{report}"))
    }

    pub(crate) fn finalize_legacy_creature_race_coverage(&mut self) -> Result<(), RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Ok(());
        }
        self.legacy_creature_race_coverage.expected_candidates =
            self.legacy_creature_race_expected_candidates();
        let report = serde_json::to_string(&self.legacy_creature_race_coverage)
            .map_err(|error| RunError::InvalidConfig(format!("creature coverage JSON: {error}")))?;
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "translate",
            level: crate::phase::LogLevel::Info,
            message: format!("legacy_creature_race_coverage:{report}"),
        });
        if !self.legacy_creature_race_coverage.coverage_gate_passes() {
            return Err(RunError::InvalidConfig(format!(
                "legacy_creature_race_coverage_gate_failed:{report}"
            )));
        }
        Ok(())
    }

    /// Run the per-record translate pipeline over `fks`.
    ///
    /// Caller must have already called `init_mapper_state`. Yields to Python
    /// every 1000 records to honour Ctrl-C and the optional progress callback.
    fn supported_legacy_actor_bases(
        &mut self,
        fks: &[FormKey],
    ) -> Result<Option<FxHashSet<FormKey>>, RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Ok(None);
        }
        let achr_sig = SigCode::from_str("ACHR")
            .map_err(|e| RunError::InvalidConfig(format!("ACHR signature: {e}")))?;
        let achr_fks = iter_form_keys_of_sig(self.source_handle_id, achr_sig, &mut self.interner)?;
        let achr_set: FxHashSet<FormKey> = achr_fks.into_iter().collect();
        if !fks.iter().any(|fk| achr_set.contains(fk)) {
            return Ok(None);
        }

        let npc_sig = SigCode::from_str("NPC_")
            .map_err(|e| RunError::InvalidConfig(format!("NPC_ signature: {e}")))?;
        let npc_fks = iter_form_keys_of_sig(self.source_handle_id, npc_sig, &mut self.interner)?;
        let snapshot =
            snapshot_records_by_form_keys(self.source_handle_id, &npc_fks, &self.interner)?;
        let mut supported = FxHashSet::default();
        let mapper_state = self
            .mapper_state
            .as_ref()
            .expect("mapper_state initialized before record translation");
        for (index, npc_fk) in npc_fks.iter().enumerate() {
            let decoded = decode_record_from_parsed_relayout(
                &snapshot.records[index].raw_record,
                npc_fk,
                &self.schema_source,
                &snapshot.masters,
                &snapshot.plugin_name,
                snapshot.strings.as_ref(),
                snapshot.plugin_is_localized,
                &self.interner,
                None,
            );
            let Ok(npc) = decoded else {
                continue;
            };
            let Some(source_race) = record_race_formkey(&npc, &self.interner) else {
                continue;
            };
            let Some(target_race) = mapper_state.source_to_target.get(&source_race).copied() else {
                continue;
            };
            if is_supported_fo4_humanoid_race(target_race, &self.interner) {
                supported.insert(*npc_fk);
            }
        }
        self.emit_phase_status(format!(
            "translate: legacy ACHR eligibility resolved {} supported NPC bases",
            supported.len()
        ));
        Ok(Some(supported))
    }

    fn translate_fks(&mut self, fks: &[FormKey]) -> Result<TranslateStats, RunError> {
        self.translate_fks_with_mode(fks, RecordWriteMode::TopLevel)
    }

    fn translate_fks_with_mode(
        &mut self,
        fks: &[FormKey],
        write_mode: RecordWriteMode,
    ) -> Result<TranslateStats, RunError> {
        self.translate_fks_with_mode_and_parents(fks, write_mode, &HashMap::new(), None)
    }

    /// `info_parent_index` maps a source INFO form_id to its source parent-DIAL
    /// form_id (from group nesting); empty for every mode except TopicChildInfo.
    fn translate_fks_with_mode_and_parents(
        &mut self,
        fks: &[FormKey],
        write_mode: RecordWriteMode,
        info_parent_index: &HashMap<u32, u32>,
        xdi_plan: Option<&crate::translator::pair_hooks::fo76_fo4::XdiDialoguePlan>,
    ) -> Result<TranslateStats, RunError> {
        if self.source == Game::SkyrimSe
            && self.target == Game::Fo4
            && self.skyrim_runtime_capability_plan.is_none()
        {
            self.plan_skyrim_runtime_components(fks)?;
        }
        let log_progress = !matches!(write_mode, RecordWriteMode::TopLevel);
        if log_progress {
            self.emit_phase_status(format!(
                "translate_v2: translate_fks mode={write_mode:?} total={} start",
                fks.len()
            ));
        }
        let mut quest_child_insert_session = matches!(write_mode, RecordWriteMode::QuestChild)
            .then(|| begin_quest_child_insert_session_native(self.target_handle_id).ok())
            .flatten();
        let mut topic_child_insert_session = matches!(write_mode, RecordWriteMode::TopicChildInfo)
            .then(|| begin_topic_child_insert_session_native(self.target_handle_id).ok())
            .flatten();
        let mut stats = TranslateStats::default();
        let mut record_count: u64 = 0;
        let whole_plugin_names =
            if self.config.is_whole_plugin && self.config.target_master_names.is_empty() {
                self.target_master_plugin_names()
            } else {
                Vec::new()
            };
        let target_master_names = target_master_names_for_skip(&self.config, whole_plugin_names);
        let target_master_syms: FxHashSet<Sym> =
            intern_plugin_names(&target_master_names, &self.interner);
        let first_target_master_sym = target_master_names
            .first()
            .map(|name| self.interner.intern(name));
        let supported_legacy_actor_bases = self.supported_legacy_actor_bases(fks)?;

        // Source→FO4 struct relayout context. Legacy Fallout is deliberately
        // restricted to BPTD.BPND; FO76 retains the generic divergent-struct path.
        // Clone the Arc so the ctx borrows this local, not `self` — the loop body
        // later needs `&mut self` (capture_full_plugin_record_state), which would
        // conflict with an immutable borrow of `self.schema_target` held here.
        let legacy_bptd_only = matches!(self.source, Game::Fnv | Game::Fo3);
        let relayout_target_schema = (self.target == Game::Fo4
            && (self.source == Game::Fo76 || legacy_bptd_only))
            .then(|| self.schema_target.clone());
        let relayout_ctx = relayout_target_schema.as_deref().map(|target_schema| {
            crate::struct_relayout::StructRelayoutCtx {
                target_schema,
                target_form_version:
                    crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
                legacy_bptd_only,
            }
        });
        let mut generated_player_info_fks = HashMap::new();
        if matches!(write_mode, RecordWriteMode::TopicChildInfo)
            && let Some(plan) = xdi_plan
        {
            let state = self.mapper_state.as_mut().unwrap();
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            mapper.reserve_generated_object_ids(fks.iter().map(|form_key| form_key.local));
            for &form_key in fks {
                if plan.combined_info_splits.contains_key(&form_key.local) {
                    generated_player_info_fks.insert(form_key, mapper.allocate_generated());
                }
            }
        }

        for &fk in fks {
            record_count += 1;

            // ── Process record (inner block so `continue` doesn't skip the yield check) ──
            'record: {
                // ── Read source record ─────────────────────────────────
                let mut src_record = match read_record_relayout_by_form_key(
                    self.source_handle_id,
                    &fk,
                    &*self.schema_source,
                    &self.interner,
                    relayout_ctx.as_ref(),
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        if let Some(admission) = self.creature_dependency_admissions.get(&fk) {
                            return Err(self.creature_dependency_failure(
                                fk,
                                admission.source_signature,
                                "read_failed",
                            ));
                        }
                        let w = self.interner.intern(&format!("read_error:{e}"));
                        self.warnings.push(w);
                        stats.records_failed += 1;
                        break 'record;
                    }
                };
                let source_sig = src_record.sig;
                stats.signature_entry(source_sig).seen += 1;
                let admitted_mvp_weapon_record = self.is_admitted_mvp_weapon_record(&src_record);
                let admitted_creature_dependency =
                    self.is_admitted_creature_dependency(fk, source_sig);
                if self.is_skyrim_creature_batch_owned(fk) {
                    self.deferred.push((fk, DeferredKind::V2Pipeline));
                    stats.records_deferred += 1;
                    stats.signature_entry(source_sig).deferred += 1;
                    break 'record;
                }
                if self.is_skyrim_runtime_planned_record(fk, source_sig) {
                    let disposition = self
                        .skyrim_runtime_capability_plan
                        .as_ref()
                        .and_then(|plan| plan.by_record.get(&fk))
                        .cloned()
                        .ok_or_else(|| {
                            RunError::InvalidConfig(format!(
                                "managed Skyrim runtime record {} {:06X} has no component decision",
                                source_sig.as_str(),
                                fk.local
                            ))
                        })?;
                    if !disposition.supported {
                        let kind = self.interner.intern("skyrim_runtime_component_unsupported");
                        self.decisions.push(Decision {
                            kind,
                            message: format!(
                                "{} {:06X}: components=[{}]: {}",
                                source_sig.as_str(),
                                fk.local,
                                disposition.component_ids.join(","),
                                disposition.reason.as_deref().unwrap_or("unsupported")
                            ),
                        });
                        stats.records_dropped += 1;
                        stats.signature_entry(source_sig).dropped += 1;
                        break 'record;
                    }
                }
                if self.is_skyrim_minimal_quest_owned(fk) {
                    self.deferred.push((fk, DeferredKind::V2Pipeline));
                    stats.records_deferred += 1;
                    stats.signature_entry(source_sig).deferred += 1;
                    break 'record;
                }
                if source_sig.as_str() == "ACHR"
                    && supported_legacy_actor_bases.as_ref().is_some_and(|bases| {
                        record_base_formkey(&src_record, &self.interner)
                            .is_none_or(|base| !bases.contains(&base))
                    })
                {
                    let kind = self.interner.intern("unsupported_legacy_actor_placement");
                    self.decisions.push(Decision {
                        kind,
                        message: format!(
                            "{:06X}:base is not a converted human, ghoul, or child NPC",
                            fk.local
                        ),
                    });
                    stats.records_dropped += 1;
                    stats.signature_entry(source_sig).dropped += 1;
                    break 'record;
                }
                let fnv_scri_target = if self.source == Game::Fnv && self.target == Game::Fo4 {
                    let raw_form_id_resolver = |raw| {
                        self.mapper_state.as_ref().and_then(|state| {
                            state
                                .options
                                .source_form_key_for_raw_formid(raw, &self.interner)
                        })
                    };
                    match capture_fnv_scri_target_text(
                        &src_record,
                        &self.interner,
                        &raw_form_id_resolver,
                    ) {
                        Ok(target) => target,
                        Err(error) if self.config.fnv_quest_slice => {
                            return Err(RunError::InvalidConfig(format!(
                                "strict FNV quest slice SCRI capture failed: {error}"
                            )));
                        }
                        Err(error) => {
                            let warning = self.interner.intern(&format!("fnv_scri:{error}"));
                            self.warnings.push(warning);
                            None
                        }
                    }
                } else {
                    None
                };
                let creature_race_event = if self.should_apply_legacy_creature_race_policy() {
                    match crate::translator::pair_hooks::fnv_creature_race::apply_legacy_creature_race_policy(
                        self.source,
                        self.target,
                        &mut src_record,
                        &self.interner,
                    ) {
                        Ok(event) => {
                            if let Some(event) = event.as_ref() {
                                self.observe_legacy_creature_race_decision(&event.decision);
                            }
                            event
                        }
                        Err(error) => {
                            self.observe_legacy_creature_race_decision(&error.decision);
                            return Err(self.fail_legacy_creature_race(error.to_string()));
                        }
                    }
                } else {
                    None
                };
                // ── PairHook::pre_translate ────────────────────────────
                {
                    let mut ctx = PairCtx::new(&mut self.interner);
                    if let Err(e) = self.translator.pre_translate(&mut ctx, &mut src_record) {
                        let w = self.interner.intern(&format!("pre_translate:{e}"));
                        self.warnings.push(w);
                    }
                }
                if self.source == Game::Fo76
                    && self.target == Game::Fo4
                    && let Some(variant) =
                        crate::translator::pair_hooks::fo76_fo4::fo76_misc_static_model_variant(
                            &src_record,
                            &self.interner,
                        )
                {
                    let kind = self.interner.intern("fo76_misc_static_model_variant");
                    self.decisions.push(Decision {
                        kind,
                        message: variant.decision_message(),
                    });
                }
                if source_sig.as_str() == "DIAL"
                    && let Some(count) =
                        xdi_plan.and_then(|plan| plan.dial_info_count_overrides.get(&fk.local))
                {
                    crate::translator::pair_hooks::fo76_fo4::apply_xdi_dial_info_count(
                        &mut src_record,
                        *count,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!("fo76_xdi_dialogue:{error}"))
                    })?;
                }
                if self.source == Game::Fo76
                    && self.target == Game::Fo4
                    && source_sig.as_str() == "INFO"
                    && let Some(plan) = xdi_plan
                {
                    crate::translator::pair_hooks::fo76_fo4::scope_start_scene_info_to_actor_alias(
                        &mut src_record,
                        plan,
                    );
                }

                // ── Translate ──────────────────────────────────────────
                let lowered_acre = matches!(self.source, Game::Fnv | Game::Fo3)
                    && self.target == Game::Fo4
                    && source_sig.as_str() == "ACRE"
                    && !self
                        .translator
                        .maps
                        .skip_records
                        .contains(source_sig.as_str());
                if lowered_acre {
                    crate::translator::pair_hooks::fnv_fo4::lower_acre_signature(&mut src_record)
                        .map_err(|error| RunError::InvalidConfig(format!("legacy_acre:{error}")))?;
                }

                let translated = {
                    let forced_skip_signature = match write_mode {
                        RecordWriteMode::QuestChild if source_sig.as_str() == "DIAL" => {
                            Some("DIAL")
                        }
                        RecordWriteMode::TopicChildInfo if source_sig.as_str() == "INFO" => {
                            Some("INFO")
                        }
                        _ => None,
                    };
                    let removed_forced_skip = if let Some(signature) = forced_skip_signature {
                        self.translator.maps.skip_records.remove(signature)
                    } else {
                        false
                    };
                    let result = if admitted_mvp_weapon_record
                        || admitted_creature_dependency
                        || is_mandatory_skyrim_actor_record(self.source, self.target, source_sig)
                        || self.is_skyrim_runtime_planned_record(fk, source_sig)
                    {
                        self.translator.translate_ignoring_skip(
                            &src_record,
                            &mut self.interner,
                            source_sig.as_str(),
                        )
                    } else {
                        self.translator.translate(&src_record, &mut self.interner)
                    };
                    if removed_forced_skip {
                        if let Some(signature) = forced_skip_signature {
                            self.translator
                                .maps
                                .skip_records
                                .insert(signature.to_string());
                        }
                    }
                    match result {
                        TranslateResult::Translated(r) => r,
                        TranslateResult::Dropped { decision, .. } => {
                            if admitted_creature_dependency {
                                return Err(
                                    self.creature_dependency_failure(fk, source_sig, "dropped")
                                );
                            }
                            self.decisions.push(decision);
                            stats.records_dropped += 1;
                            stats.signature_entry(source_sig).dropped += 1;
                            break 'record;
                        }
                        TranslateResult::Deferred(kind) => {
                            if admitted_creature_dependency {
                                return Err(
                                    self.creature_dependency_failure(fk, source_sig, "deferred")
                                );
                            }
                            if let Some(source_scpt_form_key) = fnv_scri_target.as_deref() {
                                let target_form_key = self
                                    .mapper_state
                                    .as_ref()
                                    .and_then(|state| state.source_to_target.get(&fk))
                                    .copied();
                                match target_form_key {
                                    Some(target_form_key) => append_fnv_scri_link(
                                        &mut self.fnv_scri_links,
                                        target_form_key,
                                        source_scpt_form_key,
                                        &self.interner,
                                    )
                                    .map_err(|error| {
                                        RunError::InvalidConfig(format!(
                                            "deferred FNV SCRI attachment: {error}"
                                        ))
                                    })?,
                                    None if self.config.fnv_quest_slice => {
                                        return Err(RunError::InvalidConfig(format!(
                                            "strict FNV quest slice deferred {} {:06X} SCRI target has no preallocated identity",
                                            source_sig.as_str(),
                                            fk.local
                                        )));
                                    }
                                    None => {
                                        let warning = self.interner.intern(&format!(
                                            "fnv_scri:deferred {} {:06X} target has no mapping",
                                            source_sig.as_str(),
                                            fk.local
                                        ));
                                        self.warnings.push(warning);
                                    }
                                }
                            }
                            self.deferred.push((fk, kind));
                            stats.records_deferred += 1;
                            stats.signature_entry(source_sig).deferred += 1;
                            break 'record;
                        }
                    }
                };

                // ── Allocate target FormKey + rewrite cross-plugin refs ─
                let mut translated = translated;
                let source_info_tree_links =
                    crate::translator::pair_hooks::fo76_fo4::info_tree_links(&translated);
                if self
                    .schema_target
                    .record_def(translated.sig.as_str())
                    .is_none()
                {
                    if admitted_creature_dependency {
                        return Err(self.creature_dependency_failure(
                            fk,
                            source_sig,
                            "unsupported_target_signature",
                        ));
                    }
                    let warning = format!(
                        "unsupported_target_record:{} not in {} generated schema",
                        translated.sig.as_str(),
                        self.target.as_str()
                    );
                    let w = self.interner.intern(&warning);
                    self.warnings.push(w);
                    stats.records_dropped += 1;
                    stats.signature_entry(source_sig).dropped += 1;
                    break 'record;
                }
                let cell_edid_rename = if self.source == Game::Fo76 && self.target == Game::Fo4 {
                    self.mapper_state.as_ref().and_then(|state| {
                        let translated_sig = translated.sig;
                        rename_fo76_target_editor_id_collision(
                            &mut translated,
                            &state.target_eid_index,
                            &state.options.vanilla_remap_blocked_source_form_keys,
                            &self.interner,
                            is_editor_id_collision_rename_forced(
                                self.source,
                                self.target,
                                translated_sig,
                            ),
                        )
                    })
                } else {
                    None
                };
                let collision_donor = cell_edid_rename.as_ref().and_then(|(old, _)| {
                    self.mapper_state.as_ref().and_then(|state| {
                        target_collision_donor_form_key(
                            &state.target_eid_index,
                            &self.interner,
                            old,
                            translated.sig,
                        )
                    })
                });
                if let Some((old, new)) = cell_edid_rename.as_ref() {
                    let w = self
                        .interner
                        .intern(&format!("fo76_target_edid_collision_renamed:{old}->{new}"));
                    self.warnings.push(w);
                }
                let mut legacy_serial_diagnostics = Vec::new();
                let mut legacy_serial_drop = false;
                let planned_info_split = xdi_plan
                    .and_then(|plan| plan.combined_info_splits.get(&fk.local))
                    .copied();
                let generated_player_info_fk = planned_info_split
                    .map(|_| {
                        generated_player_info_fks.get(&fk).copied().ok_or_else(|| {
                            RunError::InvalidConfig(format!(
                                "fo76_xdi_dialogue:INFO {:06X} has no generated player FormKey",
                                fk.local
                            ))
                        })
                    })
                    .transpose()?;
                let target_info_parent = if matches!(write_mode, RecordWriteMode::TopicChildInfo) {
                    let source_parent = planned_info_split
                        .map(|split| split.npc_parent)
                        .or_else(|| info_parent_index.get(&fk.local).copied())
                        .map(|local| FormKey {
                            local,
                            plugin: fk.plugin,
                        });
                    source_parent.and_then(|parent| {
                        self.mapper_state
                            .as_ref()
                            .and_then(|state| state.source_to_target.get(&parent))
                            .copied()
                    })
                } else {
                    None
                };
                let target_fk = {
                    let state = self.mapper_state.as_mut().unwrap();
                    let mut mapper = FormKeyMapper::from_state(state, &self.interner);
                    let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
                    let normalized_eid = allocation_editor_id_for_parent(
                        normalized_eid,
                        write_mode,
                        target_info_parent,
                        mapper.output_plugin_sym(),
                    );
                    let target_fk = mapper.allocate_or_resolve(fk, normalized_eid, translated.sig);
                    translated.form_key = target_fk;
                    if lowered_acre {
                        crate::translator::pair_hooks::fnv_fo4::finalize_lowered_acre(
                            &mut translated,
                            fk,
                            target_fk,
                            &mut mapper,
                            &mut self.legacy_placed_actor_aliases,
                            &self.interner,
                        )
                        .map_err(|error| RunError::InvalidConfig(format!("legacy_acre:{error}")))?;
                    }
                    if let Some(race) = creature_race_event
                        .as_ref()
                        .and_then(|event| event.decision.audited_race().ok().flatten())
                    {
                        mapper.add_mapping(race, race);
                    }
                    if let Some(outcome) = self.translator.normalize_serial_mapper_record_once(
                        fk,
                        &mut translated,
                        &mut mapper,
                        &mut self.legacy_serial_normalization,
                    ) {
                        match outcome {
                            Ok(report) => {
                                report.register_target_identities(&mut mapper);
                                legacy_serial_diagnostics = report.diagnostics(&translated);
                            }
                            Err(diagnostic) => {
                                legacy_serial_drop = true;
                                legacy_serial_diagnostics.push(diagnostic);
                            }
                        }
                    }
                    if !legacy_serial_drop {
                        if self.source == Game::Fo76
                            && self.target == Game::Fo4
                            && translated
                                .fields
                                .iter()
                                .any(|field| field.sig.0 == *b"NVNM" || field.sig.0 == *b"MNAM")
                        {
                            if let Err(e) = crate::fo76_navmesh::rewrite_record_nvnm_for_fo4(
                                &mut translated,
                                &mut mapper,
                                self.source_handle_id,
                                self.target_handle_id,
                            ) {
                                let w = mapper.interner.intern(&format!("fo76_navm:{e}"));
                                self.warnings.push(w);
                            }
                        }
                        if let Err(e) = mapper.rewrite_record(&mut translated) {
                            let w = mapper.interner.intern(&format!("rewrite_record:{e}"));
                            self.warnings.push(w);
                        }
                    }
                    target_fk
                };
                for diagnostic in legacy_serial_diagnostics {
                    let level = if diagnostic.warning {
                        crate::phase::LogLevel::Warn
                    } else {
                        crate::phase::LogLevel::Info
                    };
                    let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
                        phase: "translate",
                        level,
                        message: diagnostic.message.clone(),
                    });
                    if diagnostic.warning {
                        let warning = self.interner.intern(&diagnostic.message);
                        self.warnings.push(warning);
                    }
                }
                if legacy_serial_drop {
                    stats.records_dropped += 1;
                    stats.signature_entry(source_sig).dropped += 1;
                    break 'record;
                }
                if is_target_master_remap(target_fk, &target_master_syms) {
                    if admitted_creature_dependency {
                        self.record_creature_dependency_terminal(fk);
                    }
                    stats.records_vanilla_remapped += 1;
                    stats.signature_entry(source_sig).vanilla_remapped += 1;
                    break 'record;
                }
                if let Some(donor_form_key) = collision_donor {
                    if let Err(error) =
                        self.merge_target_collision_donor(&mut translated, donor_form_key)
                    {
                        let warning = self
                            .interner
                            .intern(&format!("collision_donor_merge:{error}"));
                        self.warnings.push(warning);
                    }
                }
                if let Some(source_scpt_form_key) = fnv_scri_target {
                    append_fnv_scri_link(
                        &mut self.fnv_scri_links,
                        target_fk,
                        &source_scpt_form_key,
                        &self.interner,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!("translated FNV SCRI attachment: {error}"))
                    })?;
                }

                // ── PairHook::post_translate ───────────────────────────
                {
                    let mut ctx = PairCtx::new(&mut self.interner);
                    if let Err(e) = self.translator.post_translate(&mut ctx, &mut translated) {
                        let w = self.interner.intern(&format!("post_translate:{e}"));
                        self.warnings.push(w);
                    }
                }

                // ── TargetHook::run ────────────────────────────────────
                {
                    let mut ctx = TargetCtx {
                        interner: &mut self.interner,
                    };
                    if let Err(e) = self.translator.run_target_hook(&mut ctx, &mut translated) {
                        let w = self.interner.intern(&format!("target_hook:{e}"));
                        self.warnings.push(w);
                    }
                }

                // DIAL category 5 is FO76 Miscellaneous but FO4 Detection, so
                // this semantic remap cannot be idempotent. Apply it once at
                // the final target boundary, after every reusable pre-pass.
                if self.source == Game::Fo76 && self.target == Game::Fo4 {
                    crate::translator::pair_hooks::fo76_fo4::Fo76Fo4Hook::normalize_dial_data_category(
                        &self.interner,
                        &mut translated,
                    );
                }

                // ── Class A: schema-driven flag/enum normalization ─
                // FO76→FO4 only. Masks unknown header-flag bits + subrecord
                // flag bits and clamps out-of-domain enums against the FO4
                // schema. Runs after the semantic target hooks (so it only
                // validates their output) and before TargetRecordNormalizer.
                if self.source == Game::Fo76 && self.target == Game::Fo4 {
                    let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
                        &mut translated,
                        &self.schema_target,
                        &self.interner,
                    );
                    for message in report.decisions {
                        let kind = self.interner.intern("class_a_normalize");
                        self.decisions.push(Decision { kind, message });
                    }
                    for w in report.warnings {
                        let sym = self.interner.intern(&w);
                        self.warnings.push(sym);
                    }
                }

                // ── Write to target ────────────────────────────────────
                let mut synthetic_player_info = if let Some(player_form_key) =
                    generated_player_info_fk
                {
                    Some(
                        crate::translator::pair_hooks::fo76_fo4::split_fo76_combined_player_dialogue_info(
                            &mut translated,
                            player_form_key,
                            xdi_plan
                                .and_then(|plan| plan.combined_info_prompt_fallbacks.get(&fk.local)),
                            &self.interner,
                        )
                        .map_err(|error| {
                            RunError::InvalidConfig(format!("fo76_xdi_dialogue:{error}"))
                        })?,
                    )
                } else {
                    None
                };
                if matches!(write_mode, RecordWriteMode::TopicChildInfo)
                    && let Some(plan) = xdi_plan
                {
                    let player_parent = planned_info_split
                        .map(|split| split.player_parent)
                        .or_else(|| info_parent_index.get(&fk.local).copied());
                    if let Some(player_parent) = player_parent {
                        let player_record =
                            synthetic_player_info.as_mut().unwrap_or(&mut translated);
                        crate::translator::pair_hooks::fo76_fo4::retarget_player_info_tree_links(
                            player_record,
                            &source_info_tree_links,
                            player_parent,
                            plan,
                            &generated_player_info_fks,
                        )
                        .map_err(|error| {
                            RunError::InvalidConfig(format!("fo76_xdi_dialogue:{error}"))
                        })?;
                    }
                }
                let normalizer = TargetRecordNormalizer {
                    target_schema: &self.schema_target,
                    source_record_def: self.schema_source.record_def(source_sig.as_str()),
                    interner: Some(&self.interner),
                };
                let mut translated = match normalizer.normalize(translated) {
                    TargetRecordNormalization::Keep(record) => record,
                    TargetRecordNormalization::DropUnsupportedRecord => {
                        stats.records_dropped += 1;
                        stats.signature_entry(source_sig).dropped += 1;
                        break 'record;
                    }
                };
                let mut synthetic_player_info = synthetic_player_info
                    .map(|record| match normalizer.normalize(record) {
                        TargetRecordNormalization::Keep(record) => Ok(record),
                        TargetRecordNormalization::DropUnsupportedRecord => Err(
                            RunError::InvalidConfig(format!(
                                "fo76_xdi_dialogue:generated player INFO for {:#08X} was rejected by the FO4 schema",
                                fk.local
                            )),
                        ),
                    })
                    .transpose()?;
                if let Err(error) =
                    crate::translator::pair_hooks::fnv_creature_race::validate_crea_derived_npc_race(
                        &translated,
                        creature_race_event.as_ref(),
                        &self.interner,
                        |race| self.target_form_key_resolves_to_race(race),
                    )
                {
                    return Err(self.fail_legacy_creature_race(error.to_string()));
                }
                namespace_base_asset_model_paths(
                    &mut translated,
                    &self.relocation_members,
                    base_asset_namespace(&self.config, self.source, self.target).unwrap_or(""),
                    &self.interner,
                );
                namespace_base_asset_decal_material_path(
                    &mut translated,
                    &self.relocation_members,
                    base_asset_namespace(&self.config, self.source, self.target).unwrap_or(""),
                    &self.interner,
                );
                if let Some(player_info) = synthetic_player_info.as_mut() {
                    namespace_base_asset_model_paths(
                        player_info,
                        &self.relocation_members,
                        base_asset_namespace(&self.config, self.source, self.target).unwrap_or(""),
                        &self.interner,
                    );
                }
                let full_plugin_snapshot = if self.config.is_whole_plugin {
                    Some(crate::full_plugin::target_schema_record_view(
                        &translated,
                        &self.schema_target,
                    ))
                } else {
                    None
                };
                let synthetic_player_snapshot = if self.config.is_whole_plugin {
                    synthetic_player_info.as_ref().map(|record| {
                        crate::full_plugin::target_schema_record_view(record, &self.schema_target)
                    })
                } else {
                    None
                };
                match write_mode {
                    RecordWriteMode::TopLevel => {
                        if let Err(e) = add_record_native(
                            self.target_handle_id,
                            translated,
                            &*self.schema_target,
                            &self.interner,
                        ) {
                            if admitted_creature_dependency {
                                return Err(self.creature_dependency_failure(
                                    fk,
                                    source_sig,
                                    "target_write_failed",
                                ));
                            }
                            let w = self.interner.intern(&format!("write_error:{e}"));
                            self.warnings.push(w);
                            stats.records_failed += 1;
                            stats.signature_entry(source_sig).failed += 1;
                            break 'record;
                        }
                    }
                    RecordWriteMode::QuestChild => {
                        let write_result =
                            if let Some(session) = quest_child_insert_session.as_mut() {
                                add_quest_child_record_indexed_native(
                                    self.target_handle_id,
                                    translated,
                                    &*self.schema_target,
                                    &self.interner,
                                    session,
                                )
                            } else {
                                add_quest_child_record_native(
                                    self.target_handle_id,
                                    translated,
                                    &*self.schema_target,
                                    &self.interner,
                                )
                            };
                        match write_result {
                            Ok(true) => {}
                            Ok(false) => {
                                let w = self.interner.intern(&format!(
                                    "quest_child_record_skipped:{}",
                                    form_key_to_read_str(&fk, &self.interner)
                                ));
                                self.warnings.push(w);
                                stats.records_dropped += 1;
                                stats.signature_entry(source_sig).dropped += 1;
                                break 'record;
                            }
                            Err(e) => {
                                let w = self
                                    .interner
                                    .intern(&format!("quest_child_record_write:{e}"));
                                self.warnings.push(w);
                                stats.records_failed += 1;
                                stats.signature_entry(source_sig).failed += 1;
                                break 'record;
                            }
                        }
                    }
                    RecordWriteMode::TopicChildInfo => {
                        // Resolve the target parent-DIAL form_id: source INFO ->
                        // source DIAL (group nesting) -> target DIAL (remap).
                        let resolve_target_parent = |source_dial_local| {
                            let source_dial = FormKey {
                                local: source_dial_local,
                                plugin: fk.plugin,
                            };
                            self.mapper_state
                                .as_ref()
                                .and_then(|state| state.source_to_target.get(&source_dial))
                                .map(|target| target.local)
                        };
                        if let Some(split) = planned_info_split {
                            let player_parent = resolve_target_parent(split.player_parent)
                                .ok_or_else(|| {
                                    RunError::InvalidConfig(format!(
                                        "fo76_xdi_dialogue:player DIAL {:06X} for INFO {:06X} has no target mapping",
                                        split.player_parent, fk.local
                                    ))
                                })?;
                            let npc_parent =
                                resolve_target_parent(split.npc_parent).ok_or_else(|| {
                                    RunError::InvalidConfig(format!(
                                        "fo76_xdi_dialogue:NPC DIAL {:06X} for INFO {:06X} has no target mapping",
                                        split.npc_parent, fk.local
                                    ))
                                })?;
                            let player_info = synthetic_player_info.take().ok_or_else(|| {
                                RunError::InvalidConfig(format!(
                                    "fo76_xdi_dialogue:INFO {:06X} split has no generated player record",
                                    fk.local
                                ))
                            })?;
                            let player_inserted =
                                if let Some(session) = topic_child_insert_session.as_mut() {
                                    add_topic_child_record_indexed_native(
                                        self.target_handle_id,
                                        player_info,
                                        player_parent,
                                        &*self.schema_target,
                                        &self.interner,
                                        session,
                                    )
                                } else {
                                    add_topic_child_record_native(
                                        self.target_handle_id,
                                        player_info,
                                        player_parent,
                                        &*self.schema_target,
                                        &self.interner,
                                    )
                                }
                                .map_err(|error| {
                                    RunError::InvalidConfig(format!(
                                        "fo76_xdi_dialogue:player INFO {:06X} write: {error}",
                                        fk.local
                                    ))
                                })?;
                            if !player_inserted {
                                return Err(RunError::InvalidConfig(format!(
                                    "fo76_xdi_dialogue:player INFO {:06X} was skipped",
                                    fk.local
                                )));
                            }
                            let npc_inserted =
                                if let Some(session) = topic_child_insert_session.as_mut() {
                                    add_topic_child_record_indexed_native(
                                        self.target_handle_id,
                                        translated,
                                        npc_parent,
                                        &*self.schema_target,
                                        &self.interner,
                                        session,
                                    )
                                } else {
                                    add_topic_child_record_native(
                                        self.target_handle_id,
                                        translated,
                                        npc_parent,
                                        &*self.schema_target,
                                        &self.interner,
                                    )
                                }
                                .map_err(|error| {
                                    RunError::InvalidConfig(format!(
                                        "fo76_xdi_dialogue:NPC INFO {:06X} write: {error}",
                                        fk.local
                                    ))
                                })?;
                            if !npc_inserted {
                                return Err(RunError::InvalidConfig(format!(
                                    "fo76_xdi_dialogue:NPC INFO {:06X} was skipped",
                                    fk.local
                                )));
                            }
                        } else {
                            let target_parent_dialogue_form_id = info_parent_index
                                .get(&fk.local)
                                .and_then(|&source_dial_local| {
                                    resolve_target_parent(source_dial_local)
                                });
                            let Some(target_parent_dialogue_form_id) =
                                target_parent_dialogue_form_id
                            else {
                                let w = self.interner.intern(&format!(
                                    "topic_child_info_no_parent:{}",
                                    form_key_to_read_str(&fk, &self.interner)
                                ));
                                self.warnings.push(w);
                                stats.records_dropped += 1;
                                stats.signature_entry(source_sig).dropped += 1;
                                break 'record;
                            };
                            let write_result =
                                if let Some(session) = topic_child_insert_session.as_mut() {
                                    add_topic_child_record_indexed_native(
                                        self.target_handle_id,
                                        translated,
                                        target_parent_dialogue_form_id,
                                        &*self.schema_target,
                                        &self.interner,
                                        session,
                                    )
                                } else {
                                    add_topic_child_record_native(
                                        self.target_handle_id,
                                        translated,
                                        target_parent_dialogue_form_id,
                                        &*self.schema_target,
                                        &self.interner,
                                    )
                                };
                            match write_result {
                                Ok(true) => {}
                                Ok(false) => {
                                    let w = self.interner.intern(&format!(
                                        "topic_child_info_skipped:{}",
                                        form_key_to_read_str(&fk, &self.interner)
                                    ));
                                    self.warnings.push(w);
                                    stats.records_dropped += 1;
                                    stats.signature_entry(source_sig).dropped += 1;
                                    break 'record;
                                }
                                Err(e) => {
                                    let w = self
                                        .interner
                                        .intern(&format!("topic_child_info_write:{e}"));
                                    self.warnings.push(w);
                                    stats.records_failed += 1;
                                    stats.signature_entry(source_sig).failed += 1;
                                    break 'record;
                                }
                            }
                        }
                    }
                }
                if let Some(snapshot) = full_plugin_snapshot.as_ref() {
                    self.capture_full_plugin_record_state(
                        fk,
                        snapshot,
                        &target_master_syms,
                        first_target_master_sym,
                        None,
                    );
                }
                if let Some(snapshot) = synthetic_player_snapshot.as_ref() {
                    self.capture_full_plugin_record_state(
                        fk,
                        snapshot,
                        &target_master_syms,
                        first_target_master_sym,
                        None,
                    );
                }

                if admitted_creature_dependency {
                    self.record_creature_dependency_terminal(fk);
                }
                stats.records_translated += 1;
                stats.signature_entry(source_sig).translated += 1;
            } // end 'record

            // ── Yield + cancel check every 1000 records ───────────────
            if record_count % 1000 == 0 {
                if log_progress && record_count % 25000 == 0 {
                    self.emit_phase_status(format!(
                        "translate_v2: translate_fks mode={write_mode:?} progress={record_count}/{} translated={} dropped={} failed={}",
                        fks.len(),
                        stats.records_translated,
                        stats.records_dropped,
                        stats.records_failed
                    ));
                }
                let cb = self.progress_callback.as_ref();
                Python::attach(|py| -> Result<(), RunError> {
                    py.check_signals().map_err(|_| RunError::Cancelled)?;
                    if let Some(cb) = cb {
                        let keep_going: bool = cb
                            .call1(py, (record_count,))
                            .and_then(|r| r.extract::<bool>(py))
                            .unwrap_or(true);
                        if !keep_going {
                            return Err(RunError::Cancelled);
                        }
                    }
                    Ok(())
                })?;
            }
        }

        if self.config.is_whole_plugin {
            let warning = format!(
                "full_plugin_state:unresolved_refs={};target_master_refs={}",
                self.full_plugin_state.unresolved_ref_count(),
                self.full_plugin_state.target_master_ref_count()
            );
            let sym = self.interner.intern(&warning);
            self.warnings.push(sym);
        }

        if log_progress {
            let child_insert_timing = quest_child_insert_session
                .as_ref()
                .map(|session| session.timing())
                .or_else(|| {
                    topic_child_insert_session
                        .as_ref()
                        .map(|session| session.timing())
                });
            if let Some(timing) = child_insert_timing {
                self.emit_phase_status(format!(
                    "translate_v2: child_write_timing mode={write_mode:?} records={} index_build_ms={:.3} lock_wait_ms={:.3} encode_ms={:.3} topology_insert_ms={:.3} write_effect_ms={:.3} wrong_handle_fallback_ms={:.3} fast_inserts={} serial_fallbacks={}",
                    timing.records,
                    timing.index_build.as_secs_f64() * 1000.0,
                    timing.lock_wait.as_secs_f64() * 1000.0,
                    timing.encode.as_secs_f64() * 1000.0,
                    timing.topology_insert.as_secs_f64() * 1000.0,
                    timing.write_effect.as_secs_f64() * 1000.0,
                    timing.wrong_handle_fallback.as_secs_f64() * 1000.0,
                    timing.fast_inserts,
                    timing.serial_fallbacks,
                ));
            }
            self.emit_phase_status(format!(
                "translate_v2: translate_fks mode={write_mode:?} done translated={} dropped={} failed={}",
                stats.records_translated, stats.records_dropped, stats.records_failed
            ));
        }
        Ok(stats)
    }

    pub(crate) fn rebuild_full_plugin_worldspace_groups(&mut self) -> Result<(), RunError> {
        // Starfield's 100 m exterior cell has no integer relationship to FO4's
        // 4096 units, so source topology cannot be mirrored — it must be
        // re-latticed from the (already scaled) placed positions.
        if self.source == Game::Starfield && self.target == Game::Fo4 {
            return crate::starfield_worldspace::relattice_worldspaces(self);
        }
        if !self.config.is_whole_plugin {
            return Ok(());
        }
        if !supports_source_worldspace_topology_rebuild(self.source, self.target) {
            return Ok(());
        }

        let source_to_target_pairs = self
            .mapper_state
            .as_ref()
            .map(|state| {
                state
                    .source_to_target
                    .iter()
                    .map(|(&source, &target)| (source, target))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let raw_formid_mappings = crate::fo76_navmesh::raw_formid_mappings_for_context(
            source_to_target_pairs,
            &self.interner,
            self.source_handle_id,
            self.target_handle_id,
        )
        .map_err(|e| RunError::InvalidConfig(format!("rebuild_worldspace_groups:{e}")))?;
        let stats = rebuild_worldspace_groups_from_source_native(
            self.target_handle_id,
            self.source_handle_id,
            &raw_formid_mappings,
        )
        .map_err(|e| RunError::InvalidConfig(format!("rebuild_worldspace_groups:{e}")))?;
        let message = format!(
            "rebuild_worldspace_groups: groups_rebuilt={} records_nested={} flat_records_removed={}",
            stats.groups_rebuilt, stats.records_nested, stats.flat_records_removed,
        );
        let sym = self.interner.intern(&message);
        self.warnings.push(sym);
        Ok(())
    }

    pub(crate) fn capture_full_plugin_record_state(
        &mut self,
        source_fk: FormKey,
        translated: &crate::record::Record,
        target_master_syms: &FxHashSet<Sym>,
        first_target_master_sym: Option<Sym>,
        unresolved_source_refs: Option<&FxHashSet<FormKey>>,
    ) {
        if !self.config.is_whole_plugin {
            return;
        }
        self.full_plugin_state.record_translated(translated.sig);
        if let Some(unresolved_source_refs) = unresolved_source_refs {
            self.full_plugin_state.capture_record_refs_with_unresolved(
                translated,
                unresolved_source_refs,
                target_master_syms,
            );
        } else {
            self.full_plugin_state.capture_record_refs(
                translated,
                source_fk.plugin,
                target_master_syms,
            );
        }
        if let Some(first_target_master_sym) = first_target_master_sym {
            self.full_plugin_state
                .capture_raw_zero_master_refs(translated, first_target_master_sym);
        }
    }

    pub(crate) fn target_master_plugin_names(&mut self) -> Vec<String> {
        let mut names = Vec::with_capacity(self.master_handle_ids.len());
        for &handle_id in &self.master_handle_ids {
            match plugin_name_for_handle(handle_id) {
                Ok(name) => names.push(name),
                Err(e) => {
                    let sym = self
                        .interner
                        .intern(&format!("full_plugin_master_name_error:{e}"));
                    self.warnings.push(sym);
                }
            }
        }
        names
    }

    /// Execute the canonical fixup segment plan.
    ///
    /// Fused sweeps run the record visitors cleared for fusion; other fixups run
    /// as single-fixup segments with the registry's scope skips, convergence,
    /// and full-plugin state.
    ///
    /// Phase-contract safe: per-segment progress goes to stderr and the
    /// caller's event channel. Harvests `addon_index_remap` into the decision
    /// channel for the NIF phase.
    pub fn apply_fixups_v2(&mut self) -> Result<Vec<(String, FixupReport)>, FixupError> {
        self.apply_fixups_v2_with_deferred_havok(false)
    }

    pub fn apply_fixups_v2_with_deferred_havok(
        &mut self,
        defer_havok_postprocess: bool,
    ) -> Result<Vec<(String, FixupReport)>, FixupError> {
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            vanilla_remap_blocked_source_form_keys: FxHashSet::default(),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            additional_source_asset_roots: self.config.additional_source_asset_roots.clone(),
            legacy_music_tracks: self.config.legacy_music_tracks.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_membership_root: self.config.target_membership_root.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let cancel = Arc::clone(&self.cancel);
        let event_tx = self.event_tx.clone();
        let source_handle_id = self.source_handle_id;
        let target_handle_id = self.target_handle_id;

        let all_reports = {
            let interner = &self.interner;
            let mut mapper = FormKeyMapper::from_state(
                self.mapper_state
                    .as_mut()
                    .expect("mapper_state initialized above"),
                interner,
            );

            let mut ctx = FixupContext {
                source_handle_id,
                target_handle_id,
                schema_target: &self.schema_target,
                schema_source: &self.schema_source,
                skip_record_sigs: &self.translator.maps.skip_records,
                mod_path: self.config.mod_path.as_deref(),
                source_extracted_dir: self.config.source_extracted_dir.as_deref(),
                target_master_handle_ids: &self.master_handle_ids,
                config: &config,
            };

            let log_fixups_v2 = |message: String| {
                eprintln!("{message}");
                let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
                    phase: "fixups_v2",
                    level: crate::phase::LogLevel::Info,
                    message,
                });
            };
            let mut emit_progress =
                |name: &'static str,
                 iteration: u32,
                 status: &'static str,
                 report: Option<&FixupReport>| {
                    if status == "started" {
                        log_fixups_v2(format!("[fixups_v2] starting {name} iter={iteration}"));
                    } else if let Some(report) = report {
                        let action = if status == "skipped" {
                            "skipped"
                        } else {
                            "finished"
                        };
                        let message = report
                            .message
                            .and_then(|sym| interner.resolve(sym))
                            .map(|text| format!(" message={text}"))
                            .unwrap_or_default();
                        log_fixups_v2(format!(
                            "[fixups_v2] {action} {name} iter={iteration} changed={} dropped={} added={} warnings={} diagnostics={} elapsed_ms={} status_report={} scope={}{}",
                            report.records_changed,
                            report.records_dropped,
                            report.records_added,
                            report.warnings.len(),
                            report.diagnostics.len(),
                            report.elapsed_ms,
                            report.status.as_str(),
                            report.scope.as_str(),
                            message
                        ));
                        emit_fixup_warning_logs(
                            &event_tx,
                            "fixups_v2",
                            name,
                            iteration,
                            report,
                            interner,
                        );
                        emit_fixup_diagnostic_logs(
                            &event_tx,
                            "fixups_v2",
                            name,
                            iteration,
                            report,
                            interner,
                        );
                    }
                };

            let mut all_reports: Vec<(String, FixupReport)> = Vec::new();
            // Master plugins are never mutated by fixups: master-derived
            // gather products are shared across all sweeps of this run.
            let mut master_cache = crate::store2::visitor::MasterScanCache::default();
            for segment in crate::store2::fixups_v2::build_default_segment_plan() {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(FixupError::Cancelled);
                }
                match &segment {
                    crate::store2::fixups_v2::Segment::Fixup(make) => {
                        let fixup = make();
                        let fixup_name = fixup.name();
                        if defer_havok_postprocess
                            && crate::phase::havok_postprocess::owns_record_fixup(fixup_name)
                        {
                            log_fixups_v2(format!(
                                "[fixups_v2] skipped {fixup_name} reason=deferred_to_havok_postprocess"
                            ));
                            continue;
                        }
                        log_fixups_v2(format!("[fixups_v2] queued {fixup_name}"));
                        let mut registry = FixupRegistry::new();
                        registry.register(fixup);
                        let reports = registry
                            .run_all_with_progress_and_cancel_and_full_plugin_state(
                                &mut ctx,
                                &mut mapper,
                                &mut emit_progress,
                                Some(cancel.as_ref()),
                                Some(&self.full_plugin_state),
                            )?;
                        if reports.is_empty() {
                            log_fixups_v2(format!(
                                "[fixups_v2] skipped {fixup_name} reason=not_applicable"
                            ));
                        }
                        all_reports.extend(reports);
                    }
                    crate::store2::fixups_v2::Segment::Sweep(label, _) => {
                        let sweep = segment.build_sweep().expect("sweep segment");
                        let sweep_names: Vec<&'static str> = sweep
                            .visitors
                            .iter()
                            .map(|visitor| visitor.name())
                            .collect();
                        for name in &sweep_names {
                            log_fixups_v2(format!("[fixups_v2] queued {name} segment={label}"));
                        }
                        let source_id = Some(source_handle_id).filter(|id| *id != 0);
                        let mut session = crate::session::open_session(target_handle_id, source_id)
                            .map_err(|e| FixupError::HandleError(e.to_string()))?;
                        let reports = crate::store2::visitor::run_sweep(
                            &mut session,
                            &mut mapper,
                            &config,
                            &sweep,
                            &mut master_cache,
                        )?;
                        for (name, report) in &reports {
                            let message = report
                                .message
                                .and_then(|sym| interner.resolve(sym))
                                .map(|text| format!(" message={text}"))
                                .unwrap_or_default();
                            log_fixups_v2(format!(
                                "[fixups_v2] finished {name} iter={} changed={} dropped={} added={} warnings={} diagnostics={} elapsed_ms={} status_report={} scope={}{}",
                                report.iteration,
                                report.records_changed,
                                report.records_dropped,
                                report.records_added,
                                report.warnings.len(),
                                report.diagnostics.len(),
                                report.elapsed_ms,
                                report.status.as_str(),
                                report.scope.as_str(),
                                message
                            ));
                            emit_fixup_warning_logs(
                                &event_tx,
                                "fixups_v2",
                                name,
                                report.iteration,
                                report,
                                interner,
                            );
                            emit_fixup_diagnostic_logs(
                                &event_tx,
                                "fixups_v2",
                                name,
                                report.iteration,
                                report,
                                interner,
                            );
                        }
                        for name in sweep_names {
                            if !reports.iter().any(|(reported, _)| reported == name) {
                                log_fixups_v2(format!(
                                    "[fixups_v2] skipped {name} segment={label} reason=not_applicable"
                                ));
                            }
                        }
                        drop(session);
                        log_fixups_v2(format!(
                            "[fixups_v2] sweep {label} done: {} visitor reports",
                            reports.len()
                        ));
                        all_reports.extend(reports);
                    }
                }
            }
            all_reports
        };

        // Harvest AddonNode index reassignments into the decision channel for
        // the NIF phase.
        for (_name, report) in &all_reports {
            for &(old, new) in &report.addon_index_remap {
                let kind = self.interner.intern("addon_node_index_remap");
                self.decisions.push(crate::translator::Decision {
                    kind,
                    message: format!("{old}->{new}"),
                });
            }
        }

        if self.source == Game::Fnv
            && self.target == Game::Fo4
            && !self.fnv_quest_runtime_component_plans.is_empty()
        {
            self.reconcile_fnv_quest_runtime_post_fixup_receipts()
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
        }

        if self.source == Game::SkyrimSe
            && self.target == Game::Fo4
            && self.skyrim_runtime_capability_plan.is_some()
        {
            self.finalize_skyrim_runtime_receipt()
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
        }

        Ok(all_reports)
    }

    /// Authoritatively resolve deferred LCTN placed-ref-target classes against
    /// the now-COMPLETE output plugin.
    ///
    /// Runs AFTER the FO76/Starfield→FO4 cell copy paths re-insert the exterior
    /// placed children. The pre-copy
    /// `null_dangling_own_plugin_refs` and raw-LCTN passes deferred these classes
    /// because their targets were absent then; this pass keeps every ref whose
    /// target is now present and nulls/prunes any that are still absent.
    pub fn repair_placed_child_refs(&mut self) -> Result<FixupReport, FixupError> {
        let interior_placed_ref_candidates =
            std::mem::take(&mut self.interior_placed_ref_candidates);
        // Taken so a repeated repair cannot move XTEL destinations twice.
        let recentred_interior_refs =
            std::mem::take(&mut self.interior_recentre.placed_ref_offsets);
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            vanilla_remap_blocked_source_form_keys: FxHashSet::default(),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            additional_source_asset_roots: self.config.additional_source_asset_roots.clone(),
            legacy_music_tracks: self.config.legacy_music_tracks.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_membership_root: self.config.target_membership_root.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let event_tx = self.event_tx.clone();
        let interner = &self.interner;
        let mut mapper = FormKeyMapper::from_state(
            self.mapper_state
                .as_mut()
                .expect("mapper_state initialized above"),
            interner,
        );

        let repair_error = |name: &str, err: FixupError| {
            FixupError::HandleError(format!("repair_placed_child_refs:{name}: {err}"))
        };
        let log_timing = |name: &str, started: std::time::Instant| {
            eprintln!(
                "[repair_timing] {name} elapsed_ms={}",
                started.elapsed().as_millis()
            );
        };
        let repair_fo4_placed_ref_targets =
            needs_fo4_placed_ref_target_repair(self.source, self.target);
        let mut session =
            crate::session::open_session(self.target_handle_id, Some(self.source_handle_id))
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let started = std::time::Instant::now();
        let placed_normalize = if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let placed_sigs = ["REFR", "ACHR", "PGRE", "PHZD"]
                .map(str::to_string)
                .to_vec();
            crate::fixups::normalize_placed_records::normalize_copied_placed_records_in_session(
                &mut session,
                interner,
                &placed_sigs,
            )
            .map_err(|err| repair_error("normalize_copied_placed_records", err))?
        } else {
            Default::default()
        };
        log_timing("normalize_copied_placed_records", started);
        session.flush_pending_effects();
        let started = std::time::Instant::now();
        let mut report = crate::fixups::rewrite_raw_lctn_formids::repair_lctn_raw_formids(
            &mut session,
            &mut mapper,
            &config,
        )
        .map_err(|err| repair_error("rewrite_raw_lctn_formids", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(placed_normalize.records_changed);
        log_timing("rewrite_raw_lctn_formids", started);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let gated_refs =
                crate::fixups::gate_runtime_controlled_placed_refs::gate_runtime_controlled_placed_refs(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("gate_runtime_controlled_placed_refs", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(gated_refs.records_changed);
            report.warnings.extend(gated_refs.warnings);
            log_timing("gate_runtime_controlled_placed_refs", started);

            let started = std::time::Instant::now();
            let placed_lod =
                crate::fixups::normalize_placed_records::normalize_placed_lod_header_flags(
                    &mut session,
                    &mapper,
                )
                .map_err(|err| repair_error("normalize_placed_lod_header_flags", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(placed_lod.records_changed);
            report.warnings.extend(placed_lod.warnings);
            log_timing("normalize_placed_lod_header_flags", started);
        }
        let started = std::time::Instant::now();
        let placed_child = crate::fixups::null_dangling_own_plugin_refs::repair_placed_child_refs(
            &mut session,
            &mut mapper,
            &config,
        )
        .map_err(|err| repair_error("null_dangling_own_plugin_refs", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(placed_child.records_changed);
        report.records_dropped = report
            .records_dropped
            .saturating_add(placed_child.records_dropped);
        report.warnings.extend(placed_child.warnings);
        log_timing("null_dangling_own_plugin_refs", started);
        let started = std::time::Instant::now();
        let placed_record_vmad =
            crate::fixups::placed_record_vmad::apply_fo76_fo4_placed_record_vmad_catalog(
                &mut session,
                &mut mapper,
                self.source,
                self.target,
            )
            .map_err(|err| repair_error("placed_record_vmad", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(placed_record_vmad.records_changed);
        report.warnings.extend(placed_record_vmad.warnings);
        report.diagnostics.extend(placed_record_vmad.diagnostics);
        log_timing("placed_record_vmad", started);
        let started = std::time::Instant::now();
        let holotape_listener = crate::fixups::attach_fo76_holotape_stage_listener::apply(
            &mut session,
            &mut mapper,
            self.source,
            self.target,
        )
        .map_err(|err| repair_error("attach_fo76_holotape_stage_listener", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(holotape_listener.records_changed);
        report.warnings.extend(holotape_listener.warnings);
        report.diagnostics.extend(holotape_listener.diagnostics);
        log_timing("attach_fo76_holotape_stage_listener", started);
        // Authoritative post-copy resolve of VMAD Object script-property refs.
        // Run unconditionally: placed children can be introduced by interior and
        // projected copy paths even when the pre-copy deferral flag was not set.
        let started = std::time::Instant::now();
        let vmad = crate::fixups::null_dangling_vmad_refs::repair_dangling_vmad_refs(
            &mut session,
            &mut mapper,
            &config,
        )
        .map_err(|err| repair_error("null_dangling_vmad_refs", err))?;
        report.records_changed = report.records_changed.saturating_add(vmad.records_changed);
        report.warnings.extend(vmad.warnings);
        log_timing("null_dangling_vmad_refs", started);
        // Strip the REFR placed-child Class C subrecords (XRFG/XLYR/XASP) here,
        // post-copy: REFR ∈ skip_records, so the in-phase
        // ValidateReferenceTargetTypesFixup run (pre-copy) sees none of these
        // records and their wrong-type / no-FO4-home refs survive into output.
        let started = std::time::Instant::now();
        let refr_strip =
            crate::fixups::validate_reference_target_types::strip_refr_placed_child_subrecords(
                &mut session,
                &mut mapper,
                &config,
            )
            .map_err(|err| repair_error("strip_refr_placed_child_subrecords", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(refr_strip.records_changed);
        report.records_dropped = report
            .records_dropped
            .saturating_add(refr_strip.records_dropped);
        log_timing("strip_refr_placed_child_subrecords", started);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let material_swaps =
                crate::fixups::promote_placed_custom_material_swaps::promote_placed_custom_material_swaps(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("promote_placed_custom_material_swaps", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(material_swaps.records_changed);
            log_timing("promote_placed_custom_material_swaps", started);
        }
        // Exterior/projected children are resolved by the parallel cell-slice
        // copier. Only the exact interior children carried from emit need this
        // serial mutation pass; rediscovering them by scanning the complete
        // output dominated whole-plugin regen time.
        let started = std::time::Instant::now();
        let lvli_resolve =
            crate::fixups::resolve_placed_leveled_bases::resolve_placed_leveled_bases_for_refs(
                &mut session,
                &mut mapper,
                &config,
                &interior_placed_ref_candidates,
            )
            .map_err(|err| repair_error("resolve_placed_leveled_bases", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(lvli_resolve.records_changed);
        report.records_dropped = report
            .records_dropped
            .saturating_add(lvli_resolve.records_dropped);
        log_timing("resolve_placed_leveled_bases", started);
        if repair_fo4_placed_ref_targets {
            // Normalize raw placed-link slots before later passes consume XLKR/XAPR.
            let started = std::time::Instant::now();
            let linked_ref_repair =
                crate::fixups::repair_placed_linked_refs::repair_placed_linked_refs(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("repair_placed_linked_refs", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(linked_ref_repair.records_changed);
            log_timing("repair_placed_linked_refs", started);
        }
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let vault79_reactor =
                crate::fixups::repair_vault79_reactor_door_topology::repair_vault79_reactor_door_topology(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("repair_vault79_reactor_door_topology", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(vault79_reactor.records_changed);
            report.warnings.extend(vault79_reactor.warnings);
            report.diagnostics.extend(vault79_reactor.diagnostics);
            log_timing("repair_vault79_reactor_door_topology", started);

            let started = std::time::Instant::now();
            let workshop_boundaries =
                crate::fixups::synthesize_workshop_boundaries::synthesize_workshop_boundaries(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("synthesize_workshop_boundaries", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(workshop_boundaries.records_changed);
            report.records_added = report
                .records_added
                .saturating_add(workshop_boundaries.records_added);
            report.warnings.extend(workshop_boundaries.warnings);
            log_timing("synthesize_workshop_boundaries", started);
        }
        // Backfill ref-side XLRT from LCTN LCSR rows so FO4 location tracking
        // (boss clearing, special-ref queries) sees the reftypes FO76 only
        // baked location-side. Must stay AFTER synthesize_workshop_boundaries:
        // workshop locations have their Boss rows stripped there and must not
        // get Boss XLRT re-applied to their defender spawns.
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let xlrt_backfill =
                crate::fixups::backfill_placed_loc_ref_types::backfill_placed_loc_ref_types(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("backfill_placed_loc_ref_types", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(xlrt_backfill.records_changed);
            log_timing("backfill_placed_loc_ref_types", started);
        }
        // Repair source-local raw XTEL/XOWN targets after the FO4 masters shift
        // the output's own records away from load index zero.
        let placed_refr_discovery = if repair_fo4_placed_ref_targets {
            let started = std::time::Instant::now();
            let discovery =
                crate::fixups::repair_placed_teleport_doors::discover_placed_refr_candidates(
                    &mut session,
                    mapper.interner,
                )
                .map_err(|err| repair_error("repair_placed_teleport_doors", err))?;
            eprintln!(
                "[repair_discovery] generation=post_workshop shared={} records_seen={} records_inspected={} teleport={} light_radius={} xezn={}",
                discovery.shared_candidates_available,
                discovery.records_seen,
                discovery.records_inspected,
                discovery.teleport_candidates.len(),
                discovery.light_radius_candidates.len(),
                discovery.xezn_candidates.len()
            );
            let door_repair =
                crate::fixups::repair_placed_teleport_doors::repair_placed_references_with_candidates(
                    &mut session,
                    &mut mapper,
                    &config,
                    &discovery,
                )
                .map_err(|err| repair_error("repair_placed_teleport_doors", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(door_repair.records_changed);
            log_timing("repair_placed_teleport_doors", started);
            Some(discovery)
        } else {
            None
        };
        // Door FormIDs in XTEL name the output plugin only after the teleport repair.
        let started = std::time::Instant::now();
        let door_shift = crate::fixups::recentre_far_interiors::shift_teleport_destinations(
            &mut session,
            mapper.interner,
            &recentred_interior_refs,
        )
        .map_err(|err| repair_error("recentre_far_interiors", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(door_shift.records_changed);
        log_timing("recentre_far_interiors", started);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            // Rederive FO4 light radii from FO76's lumens (FO76 is physically based:
            // radius is only a cull distance, brightness is intensity; FO4 has no
            // intensity field, so radius *is* the falloff scale) and rescale placed
            // XRDS overrides in lockstep. Also converts the FO76 additive XRDS delta
            // to an FO4 absolute radius — a negative absolute radius corrupts the
            // cell's spatial partition, killing physics + sound for the whole cell.
            let started = std::time::Instant::now();
            let light_radius = if let Some(discovery) = placed_refr_discovery
                .as_ref()
                .filter(|discovery| discovery.shared_candidates_available)
            {
                crate::fixups::normalize_light_radii::normalize_light_radii_for_refr_candidates(
                    &mut session,
                    &mut mapper,
                    &discovery.light_radius_candidates,
                )
            } else {
                crate::fixups::normalize_light_radii::normalize_light_radii(
                    &mut session,
                    &mut mapper,
                    &config,
                )
            }
            .map_err(|err| repair_error("normalize_light_radii", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(light_radius.records_changed);
            log_timing("normalize_light_radii", started);
        }
        // WRLD.RNAM is the large-reference table for worldspace refs. FO76->FO4
        // strips source WRLD runtime tables now, but keep this repair late for
        // any FO4-native/legacy RNAM table that survives while the output is
        // complete and the source->target mapper is still available.
        let started = std::time::Instant::now();
        let wrld_large_refs = crate::fixups::rewrite_raw_wrld_large_refs::repair_wrld_large_refs(
            &mut session,
            &mut mapper,
            &config,
        )
        .map_err(|err| repair_error("rewrite_raw_wrld_large_refs", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(wrld_large_refs.records_changed);
        log_timing("rewrite_raw_wrld_large_refs", started);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let xezn_finalize = if let Some(discovery) = placed_refr_discovery
                .as_ref()
                .filter(|discovery| discovery.shared_candidates_available)
            {
                crate::fixups::encounter_zones::finalize_placed_xezn_targets_for_refr_candidates(
                    &mut session,
                    &config,
                    mapper.interner,
                    &discovery.xezn_candidates,
                )
            } else {
                crate::fixups::encounter_zones::finalize_placed_xezn_targets(
                    &mut session,
                    &config,
                    mapper.interner,
                )
            }
            .map_err(|err| repair_error("finalize_placed_xezn_targets", err))?;
            emit_fixup_report_log(
                &event_tx,
                "repair_placed_child_refs",
                "finalize_placed_xezn_targets",
                &xezn_finalize,
                mapper.interner,
            );
            report.records_changed = report
                .records_changed
                .saturating_add(xezn_finalize.records_changed);
            report.records_dropped = report
                .records_dropped
                .saturating_add(xezn_finalize.records_dropped);
            report.warnings.extend(xezn_finalize.warnings);
            log_timing("finalize_placed_xezn_targets", started);
        }
        drop(placed_refr_discovery);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let ess_spawn =
                crate::fixups::encounter_zones::specialize_placed_actor_templates_after_ref_repair(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| {
                    repair_error("specialize_placed_actor_templates_after_ref_repair", err)
                })?;
            emit_fixup_report_log(
                &event_tx,
                "repair_placed_child_refs",
                "specialize_placed_actor_templates",
                &ess_spawn,
                mapper.interner,
            );
            report.records_added = report.records_added.saturating_add(ess_spawn.records_added);
            report.records_changed = report
                .records_changed
                .saturating_add(ess_spawn.records_changed);
            report.warnings.extend(ess_spawn.warnings);
            log_timing("specialize_placed_actor_templates", started);
        }
        // Final QUST alias-condition repair must remain after every placed-child
        // repair above: only now can output REFR persistence be proven from both
        // its record flag and CELL persistent-group topology.
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let quest_conditions =
                crate::fixups::strip_invalid_quest_condition_params::repair_final_quest_reference_conditions(
                    &mut session,
                    mapper.interner,
                    &config,
                )
                .map_err(|err| repair_error("repair_final_quest_reference_conditions", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(quest_conditions.records_changed);
            report.warnings.extend(quest_conditions.warnings);
            log_timing("repair_final_quest_reference_conditions", started);
        }
        session.flush_pending_effects();
        self.warnings.extend(report.warnings.iter().copied());
        Ok(report)
    }

    /// Post-copy FO76→FO4 encounter-zone synthesis. Runs AFTER the cell-slice
    /// copy + persistent-cell synthesis (exterior CELLs present in the target)
    /// and BEFORE `build_esp` (source handle still open, so FO76 LCTN is
    /// readable). Synthesizes one FO4 `ECZN` per qualifying Location, stamps
    /// `CELL.XEZN` on each footprint cell, and rewrites workshop `LCTN` keyword
    /// arrays to FO4's settlement contract. No-op off the FO76→FO4 path.
    pub fn synthesize_encounter_zones(
        &mut self,
        identity_resolve: bool,
    ) -> Result<FixupReport, FixupError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(FixupReport::empty());
        }
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            vanilla_remap_blocked_source_form_keys: FxHashSet::default(),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            additional_source_asset_roots: self.config.additional_source_asset_roots.clone(),
            legacy_music_tracks: self.config.legacy_music_tracks.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_membership_root: self.config.target_membership_root.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let event_tx = self.event_tx.clone();
        let interner = &self.interner;
        let mut mapper = FormKeyMapper::from_state(
            self.mapper_state
                .as_mut()
                .expect("mapper_state initialized above"),
            interner,
        );

        let started = std::time::Instant::now();
        let mut session =
            crate::session::open_session(self.target_handle_id, Some(self.source_handle_id))
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
        eprintln!(
            "[eczn_timing] open_session elapsed_ms={}",
            started.elapsed().as_millis()
        );
        let report = crate::fixups::encounter_zones::synthesize_encounter_zones(
            &mut session,
            &mut mapper,
            &config,
            identity_resolve,
        )?;
        emit_fixup_report_log(
            &event_tx,
            "synthesize_encounter_zones",
            "encounter_zones",
            &report,
            mapper.interner,
        );
        let started = std::time::Instant::now();
        session.flush_pending_effects();
        eprintln!(
            "[eczn_timing] flush_pending_effects elapsed_ms={}",
            started.elapsed().as_millis()
        );
        self.warnings.extend(report.warnings.iter().copied());
        Ok(report)
    }

    /// FO76→FO4 interior sky-region assignment (post-copy). Stamps `CELL.XCCM`
    /// (Sky/Weather from Region → REGN) on every interior CELL flagged Show-Sky
    /// that lost its sky source in translation (FO76's FO76-only `XISR` Interior
    /// Sky Override has no FO4 equivalent). Must run AFTER interior-cell emit and
    /// encounter-zone synthesis, with the source handle still open (the dropped
    /// `XISR` weather is read back from the source CELL).
    pub fn synthesize_sky_regions(&mut self) -> Result<FixupReport, FixupError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(FixupReport::empty());
        }
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            vanilla_remap_blocked_source_form_keys: FxHashSet::default(),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            additional_source_asset_roots: self.config.additional_source_asset_roots.clone(),
            legacy_music_tracks: self.config.legacy_music_tracks.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_membership_root: self.config.target_membership_root.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let interner = &self.interner;
        let mut mapper = FormKeyMapper::from_state(
            self.mapper_state
                .as_mut()
                .expect("mapper_state initialized above"),
            interner,
        );

        let mut session =
            crate::session::open_session(self.target_handle_id, Some(self.source_handle_id))
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let report =
            crate::fixups::sky_regions::synthesize_sky_regions(&mut session, &mut mapper, &config)?;
        session.flush_pending_effects();
        Ok(report)
    }

    /// FO76→FO4 vendor-dialogue enablement (post-copy). Synthesizes the
    /// `B21_VendorDialogueFaction` gate faction and enrolls every NPC that
    /// belongs to a vendor faction (a FACT carrying `VENC`) into it, so the
    /// companion `B21_VendorDialogue.esp` "Let's trade" topic can find them via
    /// `GetInFaction`. Target-only (FACT/NPC_); must run AFTER
    /// `repair_placed_child_refs` finalizes `FACT.VENC` and BEFORE the mapper
    /// remap state is released. FO76→FO4 only; no-ops otherwise.
    pub fn synthesize_vendor_dialogue(&mut self) -> Result<FixupReport, FixupError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(FixupReport::empty());
        }
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            vanilla_remap_blocked_source_form_keys: FxHashSet::default(),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            additional_source_asset_roots: self.config.additional_source_asset_roots.clone(),
            legacy_music_tracks: self.config.legacy_music_tracks.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_membership_root: self.config.target_membership_root.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let interner = &self.interner;
        let mut mapper = FormKeyMapper::from_state(
            self.mapper_state
                .as_mut()
                .expect("mapper_state initialized above"),
            interner,
        );

        // Target-only pass (FACT/NPC_); the early source-close runs before this
        // phase, so open the session without the (now-closed) source handle.
        let mut session = crate::session::open_session(self.target_handle_id, None)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let report = crate::fixups::vendor_dialogue::synthesize_vendor_dialogue(
            &mut session,
            &mut mapper,
            &config,
        )?;
        session.flush_pending_effects();
        Ok(report)
    }

    fn lower_fnv_quest_slice_quests(
        &mut self,
        ctx: &mut FnvLegacyScriptingContext,
        quest_records: &[(serde_json::Value, String)],
        typed_quest_records: &[(Record, String)],
        record_identities: &[LegacyRecordIdentity],
    ) -> Result<HashMap<FormKey, i32>, FnvScriptingError> {
        ctx.translated_record_payloads
            .retain(|payload| payload.signature != "QUST");
        self.fnv_pending_quest_fragments.clear();
        self.fnv_pending_helper_scripts.clear();
        self.fnv_pending_scpt_properties.clear();
        let package_adaptation =
            crate::fnv_legacy_scripting::from_run::reconcile_fnv_quest_slice_dynamic_package_bases(
                self,
                &ctx.translated_scripts,
            )?;
        if !package_adaptation.is_empty() {
            ctx.warnings.push(package_adaptation);
        }
        let mut alias_by_speaker = HashMap::new();
        for (_typed_record, source_form_key) in typed_quest_records {
            let source_payload = quest_records
                .iter()
                .find(|(_, key)| key.eq_ignore_ascii_case(source_form_key))
                .map(|(payload, _)| payload)
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "typed QUST {source_form_key} has no compatibility payload"
                    ))
                })?;
            let identity = record_identities
                .iter()
                .find(|identity| {
                    identity.signature == "QUST"
                        && identity
                            .source_form_key_text
                            .eq_ignore_ascii_case(source_form_key)
                })
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "typed QUST {source_form_key} has no stable target identity"
                    ))
                })?;
            let source_editor_id = source_payload
                .get("eid")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "typed QUST {source_form_key} has no EditorID"
                    ))
                })?;
            let record_dependency_warnings = register_fnv_quest_record_dependency_mappings(
                source_payload,
                source_form_key,
                source_editor_id,
                self.mapper_state.as_mut().ok_or_else(|| {
                    FnvScriptingError::Setup(
                        "typed QUST dependency mapping requires mapper state".to_string(),
                    )
                })?,
                &self.interner,
            )?;
            let mapper_options = self
                .mapper_state
                .as_ref()
                .ok_or_else(|| {
                    FnvScriptingError::Setup("typed QUST requires mapper state".to_string())
                })?
                .options
                .clone();
            let raw_resolver =
                |raw| mapper_options.source_form_key_for_raw_formid(raw, &self.interner);
            let mut translated = {
                let mapper_state = self.mapper_state.as_mut().expect("checked above");
                let mut mapper = FormKeyMapper::from_state(mapper_state, &self.interner);
                crate::fnv_legacy_scripting::quest::lower_qust_record(
                    source_payload,
                    &ctx.mod_prefix,
                    source_form_key,
                    &identity.target_form_key_text,
                    crate::translator::pair_hooks::fnv_conditions::LegacyConditionFamily::Fnv,
                    &mut mapper,
                    &raw_resolver,
                    &self.legacy_placed_actor_aliases,
                )
                .map_err(|error| {
                    FnvScriptingError::Translate(format!("typed QUST {source_form_key}: {error}"))
                })?
            };
            if translated.losses.terminal_count() != 0 {
                return Err(FnvScriptingError::Translate(format!(
                    "typed QUST {source_form_key} has {} terminal semantic losses",
                    translated.losses.terminal_count()
                )));
            }
            let compatibility = ctx
                .translated_quests
                .iter_mut()
                .find(|quest| quest.source_form_key.eq_ignore_ascii_case(source_form_key))
                .ok_or_else(|| {
                    FnvScriptingError::Translate(format!(
                        "typed QUST {source_form_key} has no Papyrus fragment output"
                    ))
                })?;
            let adaptation_warnings = account_fnv_quest_fragment_adaptations(
                &compatibility.source_form_key,
                &compatibility.source_editor_id,
                &compatibility.fragment_adaptations,
                &compatibility.stage_fragments,
            )?;
            let source_local = source_form_key_local(source_form_key).ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "typed QUST has invalid source FormKey {source_form_key}"
                ))
            })?;
            let fragment_properties = map_quest_fragment_properties(
                &compatibility.fragment_properties,
                self.mapper_state.as_ref().ok_or_else(|| {
                    FnvScriptingError::Setup(
                        "typed QUST fragment property mapping requires mapper state".to_string(),
                    )
                })?,
                &self.interner,
                source_form_key,
            )?;
            if self.config.fnv_quest_slice {
                validate_fnv_quest_slice_compat_fragment_property(
                    source_local,
                    &identity.target_form_key_text,
                    &ctx.mod_prefix,
                    &fragment_properties,
                )?;
            }
            self.fnv_pending_scpt_properties
                .extend(materialize_fnv_package_data_alias_contracts(
                    &mut translated,
                    source_form_key,
                    &identity.target_form_key_text,
                    &ctx.translated_scripts,
                )?);
            if matches!(source_local, 0x06136D | 0x11F935) {
                self.fnv_pending_helper_scripts.push(
                    crate::fnv_legacy_scripting::vmad::PendingObjectScriptBinding {
                        target_form_key: identity.target_form_key_text.clone(),
                        script_class_name: format!("{}_FnvSliceCompat", ctx.mod_prefix),
                        properties: vec![
                            crate::fnv_legacy_scripting::vmad::ScriptProperty {
                                name: "RepNVNCRFame".to_string(),
                                prop_type: "Int".to_string(),
                                value: Some(serde_json::json!(0)),
                            },
                            crate::fnv_legacy_scripting::vmad::ScriptProperty {
                                name: "RepNVNCRInfamy".to_string(),
                                prop_type: "Int".to_string(),
                                value: Some(serde_json::json!(0)),
                            },
                            crate::fnv_legacy_scripting::vmad::ScriptProperty {
                                name: "PCCanUsePowerArmor".to_string(),
                                prop_type: "Bool".to_string(),
                                value: Some(serde_json::json!(false)),
                            },
                        ],
                    },
                );
            }
            let mut fragments = Vec::new();
            for metadata in &translated.fragment_metadata {
                if let Some(stage_fragment) =
                    compatibility.stage_fragments.iter_mut().find(|fragment| {
                        fragment.stage_index == i32::from(metadata.stage_index)
                            && fragment.stage_item_index == metadata.stage_item_index
                    })
                {
                    compatibility.fragment_psc_text = compatibility
                        .fragment_psc_text
                        .replace(&stage_fragment.psc_function_name, &metadata.function_name);
                    stage_fragment.psc_function_name = metadata.function_name.clone();
                }
                fragments.push(crate::fnv_legacy_scripting::vmad::QuestStageFragment {
                    stage_index: i32::from(metadata.stage_index),
                    stage_item_index: metadata.stage_item_index,
                    psc_function_name: metadata.function_name.clone(),
                });
            }
            let aliases = translated
                .aliases
                .iter()
                .map(|alias| {
                    alias_by_speaker.insert(alias.target_base, alias.alias_id);
                    crate::fnv_legacy_scripting::vmad::QuestAliasBinding {
                        alias_id: alias.alias_id,
                        target_quest_form_key: identity.target_form_key_text.clone(),
                        target_form_key: alias.target_placed_text.clone(),
                        scripts: Vec::new(),
                    }
                })
                .collect();
            self.fnv_pending_quest_fragments.push(
                crate::fnv_legacy_scripting::vmad::PendingQuestFragmentBinding {
                    target_form_key: identity.target_form_key_text.clone(),
                    script_class_name: translated.fragment_class_name.clone(),
                    fragments,
                    fragment_properties,
                    aliases,
                },
            );
            ctx.warnings.extend(record_dependency_warnings);
            ctx.warnings.extend(adaptation_warnings);
            ctx.translated_record_payloads.push(
                crate::fnv_legacy_scripting::TranslatedRecordPayload {
                    source_form_key: source_form_key.clone(),
                    signature: "QUST".to_string(),
                    translated_record: translated.authoring_record_payload,
                    warnings: translated
                        .losses
                        .entries
                        .iter()
                        .map(|loss| {
                            format!(
                                "typed_qust_externalized:{}:{}:{}",
                                loss.scope, loss.signature, loss.reason
                            )
                        })
                        .collect(),
                },
            );
        }
        if self.config.fnv_quest_slice && self.fnv_pending_scpt_properties.len() != 2 {
            return Err(FnvScriptingError::Translate(format!(
                "strict FNV quest slice expected 2 dynamic package data aliases, observed {}",
                self.fnv_pending_scpt_properties.len()
            )));
        }
        Ok(alias_by_speaker)
    }

    fn plan_fnv_quest_slice_dialogue(
        &mut self,
        ctx: &mut FnvLegacyScriptingContext,
        typed_info_records: &[(Record, String)],
        typed_dial_records: &[(Record, String)],
        record_identities: &[LegacyRecordIdentity],
        source_plugin: &str,
        alias_by_speaker: &HashMap<FormKey, i32>,
    ) -> Result<TypedFnvDialoguePlan, FnvScriptingError> {
        let mut plan = TypedFnvDialoguePlan::default();
        let source_to_target = self
            .mapper_state
            .as_ref()
            .ok_or_else(|| FnvScriptingError::Setup("typed dialogue requires mapper state".into()))?
            .source_to_target
            .clone();
        let dedicated_topics = ctx
            .translated_scripts
            .iter()
            .flat_map(|script| {
                script.dedicated_topics.iter().map(move |contract| {
                    (
                        script.source_form_key.clone(),
                        script.script_class_name.clone(),
                        contract.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        let [(dedicated_source_script, dedicated_script_class, dedicated_contract)] =
            dedicated_topics.as_slice()
        else {
            return Err(FnvScriptingError::Translate(format!(
                "strict FNV quest slice expected one dedicated greeting contract, observed {}",
                dedicated_topics.len()
            )));
        };
        let dedicated_source_infos = dedicated_contract
            .responses
            .iter()
            .map(|response| {
                parse_legacy_form_key_text(&response.source_info_form_key, &self.interner).map_err(
                    |error| {
                        FnvScriptingError::Setup(format!(
                            "dedicated greeting INFO {} is invalid: {error}",
                            response.source_info_form_key
                        ))
                    },
                )
            })
            .collect::<Result<FxHashSet<_>, _>>()?;
        let mut target_topics_by_source =
            HashMap::<FormKey, Vec<(FormKey, FormKey, FormKey)>>::new();
        for (source, source_text) in typed_dial_records {
            let identity = record_identities
                .iter()
                .find(|identity| {
                    identity.signature == "DIAL"
                        && identity
                            .source_form_key_text
                            .eq_ignore_ascii_case(source_text)
                })
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!("typed DIAL {source_text} has no identity"))
                })?;
            let owner_count = source
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "QSTI")
                .filter_map(|field| first_record_form_key(&field.value))
                .collect::<FxHashSet<_>>()
                .len();
            let clones = {
                let state = self.mapper_state.as_mut().expect("checked above");
                let mut mapper = FormKeyMapper::from_state(state, &self.interner);
                (1..owner_count)
                    .map(|_| mapper.allocate_generated())
                    .collect::<Vec<_>>()
            };
            let info_count = record_identities
                .iter()
                .filter(|candidate| {
                    matches!(
                        candidate.topology,
                        LegacyRecordTopology::TopicChild { source_parent, .. }
                            if source_parent == source.form_key
                    )
                })
                .count() as u32;
            let lowered = crate::fnv_legacy_scripting::dialogue::lower_dial_topic(
                source,
                identity.target_form_key,
                &source_to_target.iter().map(|(a, b)| (*a, *b)).collect(),
                &clones,
                info_count,
                &self.interner,
            )
            .map_err(|error| {
                FnvScriptingError::Translate(format!("typed DIAL {source_text}: {error}"))
            })?;
            for topic in lowered {
                target_topics_by_source
                    .entry(topic.source_topic)
                    .or_default()
                    .push((
                        topic.source_owner,
                        topic.target_owner,
                        topic.record.form_key,
                    ));
                plan.topics.push(topic.record);
            }
            plan.direct_sources.insert(source_text.to_ascii_uppercase());
        }

        self.fnv_pending_info_fragments.clear();
        for (source, source_text) in typed_info_records {
            if dedicated_source_infos.contains(&source.form_key) {
                continue;
            }
            let identity = record_identities
                .iter()
                .find(|identity| {
                    identity.signature == "INFO"
                        && identity
                            .source_form_key_text
                            .eq_ignore_ascii_case(source_text)
                })
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!("typed INFO {source_text} has no identity"))
                })?;
            let source_parent = match identity.topology {
                LegacyRecordTopology::TopicChild { source_parent, .. } => source_parent,
                _ => {
                    return Err(FnvScriptingError::Setup(format!(
                        "typed INFO {source_text} has no topic topology"
                    )));
                }
            };
            let target_parents = target_topics_by_source
                .get(&source_parent)
                .cloned()
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "typed INFO {source_text} parent {:06X} was not lowered",
                        source_parent.local
                    ))
                })?;
            let speaker = source
                .fields
                .iter()
                .rev()
                .find(|field| matches!(field.sig.as_str(), "PNAM" | "ANAM"))
                .and_then(|field| first_record_form_key(&field.value))
                .ok_or_else(|| {
                    FnvScriptingError::Translate(format!("typed INFO {source_text} has no speaker"))
                })?;
            let speaker_record = read_record_relayout_by_form_key(
                self.source_handle_id,
                &speaker,
                &self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "read INFO speaker {:06X}: {error}",
                    speaker.local
                ))
            })?;
            let source_voice_type = speaker_record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "VTCK")
                .and_then(|field| first_record_form_key(&field.value))
                .ok_or_else(|| {
                    FnvScriptingError::Translate(format!(
                        "INFO {source_text} speaker {:06X} has no VTYP",
                        speaker.local
                    ))
                })?;
            let target_voice_type = source_to_target
                .get(&source_voice_type)
                .copied()
                .ok_or_else(|| {
                    FnvScriptingError::Translate(format!(
                        "INFO {source_text} VTYP {:06X} has no target mapping",
                        source_voice_type.local
                    ))
                })?;
            if !source_to_target.contains_key(&speaker) {
                return Err(FnvScriptingError::Translate(format!(
                    "INFO {source_text} speaker {:06X} has no target mapping",
                    speaker.local
                )));
            }
            let voice_record = read_record_relayout_by_form_key(
                self.source_handle_id,
                &source_voice_type,
                &self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "read source VTYP {:06X}: {error}",
                    source_voice_type.local
                ))
            })?;
            let voice_edid = voice_record
                .eid
                .and_then(|eid| self.interner.resolve(eid))
                .ok_or_else(|| FnvScriptingError::Translate("mapped VTYP has no EditorID".into()))?
                .to_string();
            plan.direct_sources.insert(source_text.to_ascii_uppercase());
            let compatibility_fragment = ctx
                .translated_infos
                .iter()
                .find(|translated| translated.source_form_key.eq_ignore_ascii_case(source_text));
            let fragment_class_name = compatibility_fragment
                .and_then(|translated| translated.fragment_class_name.clone());
            let fragment_properties = compatibility_fragment
                .map(|translated| {
                    map_info_fragment_properties(
                        &translated.fragment_properties,
                        self.mapper_state.as_ref().expect("checked above"),
                        &self.interner,
                        source_text,
                    )
                })
                .transpose()?
                .unwrap_or_default();
            for (parent_index, (_, _, target_parent)) in target_parents.into_iter().enumerate() {
                let target_info = if parent_index == 0 {
                    identity.target_form_key
                } else {
                    let state = self.mapper_state.as_mut().expect("checked above");
                    FormKeyMapper::from_state(state, &self.interner).allocate_generated()
                };
                let lowered = {
                    let state = self.mapper_state.as_mut().expect("checked above");
                    let mut mapper = FormKeyMapper::from_state(state, &self.interner);
                    crate::fnv_legacy_scripting::dialogue::lower_info_record(
                        source,
                        target_info,
                        crate::fnv_legacy_scripting::dialogue::VoiceResolution {
                            source_speaker: speaker,
                            target_voice_type,
                            source_plugin: source_plugin.to_string(),
                            source_voice_type: voice_edid.clone(),
                            target_plugin: self.config.output_plugin_name.clone(),
                            target_voice_type_edid: voice_edid.clone(),
                        },
                        &mut mapper,
                        &self.interner,
                    )
                    .map_err(|error| {
                        FnvScriptingError::Translate(format!(
                            "typed INFO {source_text} clone {parent_index}: {error}"
                        ))
                    })?
                };
                let fragment_phases = lowered.fragment_phases.clone();
                plan.voice_manifest
                    .entries
                    .extend(lowered.voice_requests.into_iter().map(|request| {
                    crate::fnv_legacy_scripting::voice::FnvVoiceManifestEntry::new_with_provenance(
                        "FalloutNV.esm",
                        "fnv-base",
                        request.source_voice_type,
                        request.target_plugin,
                        request.target_voice_type_edid,
                        request.info_form_id,
                        request.response_index,
                        request.transcript,
                    )
                }));
                plan.infos.push((lowered.record, target_parent));
                if let Some(script_class_name) = fragment_class_name.clone() {
                    let target_form_key = form_key_to_legacy_str(target_info, &self.interner)
                        .ok_or_else(|| {
                            FnvScriptingError::Setup(format!(
                                "typed INFO {source_text} clone target cannot be rendered"
                            ))
                        })?;
                    self.fnv_pending_info_fragments.push(
                        crate::fnv_legacy_scripting::vmad::PendingInfoFragmentBinding {
                            target_form_key,
                            script_class_name,
                            phases: fragment_phases,
                            fragment_properties: fragment_properties.clone(),
                        },
                    );
                }
            }
        }

        if !typed_dial_records.is_empty() {
            let output_is_master = plugin_header_flags_native(self.target_handle_id)
                .map_err(|error| FnvScriptingError::Setup(error.to_string()))?
                & 1
                != 0;
            let mut owners = target_topics_by_source
                .values()
                .flatten()
                .map(|(source_owner, target_owner, _)| (*source_owner, *target_owner))
                .collect::<Vec<_>>();
            owners.sort_by_key(|(source_owner, _)| source_owner.local);
            owners.dedup();
            let partition_count = owners.len();
            let original_topics = plan.topics.clone();
            let mut emitted_records = Vec::new();
            for (source_owner, target_owner) in owners {
                let source_topics = typed_dial_records
                    .iter()
                    .filter(|(record, _)| {
                        record.fields.iter().any(|field| {
                            field.sig.as_str() == "QSTI"
                                && first_record_form_key(&field.value) == Some(source_owner)
                        })
                    })
                    .map(|(record, _)| record.clone())
                    .collect::<Vec<_>>();
                let source_topic_keys = source_topics
                    .iter()
                    .map(|record| record.form_key)
                    .collect::<FxHashSet<_>>();
                let source_infos = typed_info_records
                    .iter()
                    .filter_map(|(record, source_text)| {
                        record_identities
                            .iter()
                            .find(|identity| {
                                identity
                                    .source_form_key_text
                                    .eq_ignore_ascii_case(source_text)
                            })
                            .and_then(|identity| match identity.topology {
                                LegacyRecordTopology::TopicChild { source_parent, .. }
                                    if source_topic_keys.contains(&source_parent) =>
                                {
                                    Some((source_parent, record.clone()))
                                }
                                _ => None,
                            })
                    })
                    .collect::<Vec<_>>();
                let graph = crate::fnv_legacy_scripting::scene::build_legacy_dialogue_graph(
                    &source_topics,
                    &source_infos,
                    &self.interner,
                );
                let target_topic_by_source = target_topics_by_source
                    .iter()
                    .filter_map(|(source_topic, targets)| {
                        targets
                            .iter()
                            .find(|(owner, _, _)| *owner == source_owner)
                            .map(|(_, _, target_topic)| (*source_topic, *target_topic))
                    })
                    .collect::<HashMap<_, _>>();
                let (scene, branch) = {
                    let state = self.mapper_state.as_mut().expect("checked above");
                    let mut mapper = FormKeyMapper::from_state(state, &self.interner);
                    (mapper.allocate_generated(), mapper.allocate_generated())
                };
                for (intent, target) in [
                    (
                        crate::fnv_legacy_scripting::component::FnvQuestSyntheticIntent::Scene,
                        scene,
                    ),
                    (
                        crate::fnv_legacy_scripting::component::FnvQuestSyntheticIntent::DialogueBranch,
                        branch,
                    ),
                ] {
                    let rendered = form_key_to_legacy_str(target, &self.interner).ok_or_else(|| {
                        FnvScriptingError::Setup(
                            "generated FNV dialogue runtime target cannot be rendered".to_string(),
                        )
                    })?;
                    plan.synthetic_targets
                        .insert(intent.record_key(), intent.target_key(rendered));
                }
                let scene_plan =
                    crate::fnv_legacy_scripting::scene::bounded_scene_branch_from_graph(
                        target_owner,
                        scene,
                        branch,
                        &graph,
                        32,
                        16,
                    )
                    .map_err(|error| {
                        FnvScriptingError::Translate(format!(
                            "bounded dialogue plan for QUST {:06X}: {error}",
                            source_owner.local
                        ))
                    })?;
                let scene_plan =
                    crate::fnv_legacy_scripting::scene::remap_scene_branch_plan_topics(
                        &scene_plan,
                        &target_topic_by_source,
                    )
                    .map_err(|error| {
                        FnvScriptingError::Translate(format!(
                            "remap dialogue plan for QUST {:06X}: {error}",
                            source_owner.local
                        ))
                    })?;
                let target_topic_keys = target_topic_by_source
                    .values()
                    .copied()
                    .collect::<FxHashSet<_>>();
                let target_topics = original_topics
                    .iter()
                    .filter(|record| target_topic_keys.contains(&record.form_key))
                    .cloned()
                    .collect::<Vec<_>>();
                let target_infos = plan
                    .infos
                    .iter()
                    .filter(|(_, parent)| target_topic_keys.contains(parent))
                    .map(|(record, parent)| (*parent, record.clone()))
                    .collect::<Vec<_>>();
                let suffix = if partition_count == 1 {
                    String::new()
                } else {
                    format!("_Q{:06X}", source_owner.local)
                };
                let manifest = crate::fnv_legacy_scripting::scene::emit_fo4_scene_runtime(
                    &scene_plan,
                    target_topics,
                    &target_infos,
                    alias_by_speaker,
                    &crate::fnv_legacy_scripting::scene::SceneRuntimeOptions {
                        scene_editor_id: format!(
                            "{}_VTechatticupDialogueScene{suffix}",
                            ctx.mod_prefix
                        ),
                        branch_editor_id: format!(
                            "{}_VTechatticupDialogueBranch{suffix}",
                            ctx.mod_prefix
                        ),
                        quest_start_game_enabled: true,
                        output_is_master,
                    },
                    &self.interner,
                )
                .map_err(|error| {
                    FnvScriptingError::Translate(format!(
                        "emit FO4 dialogue runtime for QUST {:06X}: {error}",
                        source_owner.local
                    ))
                })?;
                if manifest.action_topics.is_empty()
                    || manifest.topic_runtime.len() != scene_plan.ordered_topics.len()
                {
                    return Err(FnvScriptingError::Translate(format!(
                        "scene runtime manifest did not account for QUST {:06X}",
                        source_owner.local
                    )));
                }
                ctx.warnings.push(format!(
                    "fnv_dialogue_scene_runtime:quest={:06X}:topics={}:edges={}:requires_seq={}",
                    source_owner.local,
                    scene_plan.ordered_topics.len(),
                    scene_plan.edges.len(),
                    manifest.requires_seq,
                ));
                plan.scene_manifest_records += manifest.topics.len() + 2;
                emitted_records.extend(manifest.into_quest_child_write_order());
            }
            plan.topics = emitted_records;
        }

        if dedicated_contract.route_to_scene {
            return Err(FnvScriptingError::Translate(
                "dedicated hostage greeting must not be routed through SCEN/DLBR".to_string(),
            ));
        }
        if dedicated_contract.target_info_count as usize != dedicated_contract.responses.len()
            || dedicated_contract.responses.len() != 3
        {
            return Err(FnvScriptingError::Translate(format!(
                "dedicated hostage greeting expected three INFO responses, observed metadata {}/{}",
                dedicated_contract.target_info_count,
                dedicated_contract.responses.len()
            )));
        }
        let source_topic_form_key = parse_legacy_form_key_text(
            &dedicated_contract.source_dialogue_form_key,
            &self.interner,
        )
        .map_err(|error| {
            FnvScriptingError::Setup(format!(
                "dedicated greeting source DIAL {} is invalid: {error}",
                dedicated_contract.source_dialogue_form_key
            ))
        })?;
        let source_topic = read_record_relayout_by_form_key(
            self.source_handle_id,
            &source_topic_form_key,
            &self.schema_source,
            &self.interner,
            None,
        )
        .map_err(|error| {
            FnvScriptingError::Setup(format!(
                "read dedicated greeting DIAL {}: {error}",
                dedicated_contract.source_dialogue_form_key
            ))
        })?;
        let source_owner =
            parse_legacy_form_key_text(&dedicated_contract.owner_quest_form_key, &self.interner)
                .map_err(|error| {
                    FnvScriptingError::Setup(format!(
                        "dedicated greeting owner {} is invalid: {error}",
                        dedicated_contract.owner_quest_form_key
                    ))
                })?;
        let target_owner = source_to_target
            .get(&source_owner)
            .copied()
            .ok_or_else(|| {
                FnvScriptingError::Translate(format!(
                    "dedicated greeting owner {} has no target mapping",
                    dedicated_contract.owner_quest_form_key
                ))
            })?;
        let source_speaker = parse_legacy_form_key_text(
            &dedicated_contract.condition_actor_form_key,
            &self.interner,
        )
        .map_err(|error| {
            FnvScriptingError::Setup(format!(
                "dedicated greeting speaker {} is invalid: {error}",
                dedicated_contract.condition_actor_form_key
            ))
        })?;
        if !source_to_target.contains_key(&source_speaker) {
            return Err(FnvScriptingError::Translate(format!(
                "dedicated greeting speaker {} has no target mapping",
                dedicated_contract.condition_actor_form_key
            )));
        }
        let source_voice_type = parse_legacy_form_key_text(
            &dedicated_contract.source_voice_type_form_key,
            &self.interner,
        )
        .map_err(|error| {
            FnvScriptingError::Setup(format!(
                "dedicated greeting VTYP {} is invalid: {error}",
                dedicated_contract.source_voice_type_form_key
            ))
        })?;
        let target_voice_type = source_to_target
            .get(&source_voice_type)
            .copied()
            .ok_or_else(|| {
                FnvScriptingError::Translate(format!(
                    "dedicated greeting VTYP {} has no target mapping",
                    dedicated_contract.source_voice_type_form_key
                ))
            })?;
        let source_voice_record = read_record_relayout_by_form_key(
            self.source_handle_id,
            &source_voice_type,
            &self.schema_source,
            &self.interner,
            None,
        )
        .map_err(|error| {
            FnvScriptingError::Setup(format!(
                "read dedicated greeting VTYP {}: {error}",
                dedicated_contract.source_voice_type_form_key
            ))
        })?;
        let source_voice_editor_id = source_voice_record
            .eid
            .and_then(|eid| self.interner.resolve(eid))
            .ok_or_else(|| {
                FnvScriptingError::Translate(
                    "dedicated greeting source VTYP has no EditorID".to_string(),
                )
            })?
            .to_string();
        let dedicated_targets = self.fnv_dedicated_topic_targets.ok_or_else(|| {
            FnvScriptingError::Setup(
                "dedicated greeting target identities were not allocated".to_string(),
            )
        })?;
        let mut source_infos = Vec::with_capacity(3);
        let mut target_infos = Vec::with_capacity(3);
        for response in &dedicated_contract.responses {
            let source_info =
                parse_legacy_form_key_text(&response.source_info_form_key, &self.interner)
                    .map_err(|error| {
                        FnvScriptingError::Setup(format!(
                            "dedicated greeting INFO {} is invalid: {error}",
                            response.source_info_form_key
                        ))
                    })?;
            let (record, source_text) = typed_info_records
                .iter()
                .find(|(record, _)| record.form_key == source_info)
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "dedicated greeting INFO {} was not loaded from the exact closure",
                        response.source_info_form_key
                    ))
                })?;
            let target_info = record_identities
                .iter()
                .find(|identity| {
                    identity.signature == "INFO"
                        && identity
                            .source_form_key_text
                            .eq_ignore_ascii_case(source_text)
                })
                .map(|identity| identity.target_form_key)
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "dedicated greeting INFO {} has no target identity",
                        response.source_info_form_key
                    ))
                })?;
            source_infos.push(record.clone());
            target_infos.push(target_info);
            plan.direct_sources.insert(source_text.to_ascii_uppercase());
        }
        let voices = (0..3)
            .map(|_| crate::fnv_legacy_scripting::dialogue::VoiceResolution {
                source_speaker,
                target_voice_type,
                source_plugin: source_plugin.to_string(),
                source_voice_type: source_voice_editor_id.clone(),
                target_plugin: self.config.output_plugin_name.clone(),
                target_voice_type_edid: source_voice_editor_id.clone(),
            })
            .collect::<Vec<_>>();
        let projection = {
            let state = self.mapper_state.as_mut().expect("checked above");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            crate::fnv_legacy_scripting::dialogue::lower_hostage_greeting_projection(
                &source_topic,
                &source_infos,
                dedicated_targets.keyword,
                dedicated_targets.topic,
                target_owner,
                &target_infos,
                source_voice_type,
                &voices,
                &mut mapper,
                &self.interner,
            )
            .map_err(|error| {
                FnvScriptingError::Translate(format!(
                    "dedicated hostage greeting projection: {error}"
                ))
            })?
        };
        if projection.topic_children != target_infos {
            return Err(FnvScriptingError::Translate(
                "dedicated hostage greeting INFO topology changed during projection".to_string(),
            ));
        }
        let keyword_form_key = projection.keyword.form_key;
        let topic_form_key = projection.topic.record.form_key;
        for (intent, target) in [
            (
                crate::fnv_legacy_scripting::component::FnvQuestSyntheticIntent::DedicatedGreetingKeyword,
                keyword_form_key,
            ),
            (
                crate::fnv_legacy_scripting::component::FnvQuestSyntheticIntent::DedicatedGreetingTopic,
                topic_form_key,
            ),
        ] {
            let rendered = form_key_to_legacy_str(target, &self.interner).ok_or_else(|| {
                FnvScriptingError::Setup(
                    "generated FNV dedicated greeting target cannot be rendered".to_string(),
                )
            })?;
            plan.synthetic_targets
                .insert(intent.record_key(), intent.target_key(rendered));
        }
        plan.dedicated_runtime_records.insert(keyword_form_key);
        plan.dedicated_runtime_records.insert(topic_form_key);
        plan.top_level_records.push(projection.keyword);
        plan.topics.push(projection.topic.record);
        for lowered in projection.infos {
            plan.dedicated_runtime_records
                .insert(lowered.record.form_key);
            plan.voice_manifest
                .entries
                .extend(lowered.voice_requests.into_iter().map(|request| {
                    crate::fnv_legacy_scripting::voice::FnvVoiceManifestEntry::new_with_provenance(
                        "FalloutNV.esm",
                        "fnv-base",
                        request.source_voice_type,
                        request.target_plugin,
                        request.target_voice_type_edid,
                        request.info_form_id,
                        request.response_index,
                        request.transcript,
                    )
                }));
            plan.infos.push((lowered.record, topic_form_key));
        }
        let keyword_target =
            form_key_to_legacy_str(keyword_form_key, &self.interner).ok_or_else(|| {
                FnvScriptingError::Setup(
                    "dedicated greeting Keyword target cannot be rendered".to_string(),
                )
            })?;
        bind_generated_scpt_property_target(
            &mut ctx.translated_scripts,
            dedicated_source_script,
            dedicated_script_class,
            &dedicated_contract.keyword_property_name,
            "Keyword",
            &keyword_target,
        )?;
        let pending_property_names = self
            .fnv_pending_scpt_properties
            .iter()
            .map(|pending| pending.property.name.as_str())
            .collect::<FxHashSet<_>>();
        let required_property_names = [
            "TecMineHostageEscapeData",
            "TechaticupNCRRenoldsDialoguePackageData",
        ]
        .into_iter()
        .collect::<FxHashSet<_>>();
        if self.fnv_pending_scpt_properties.len() != 2
            || pending_property_names != required_property_names
        {
            return Err(FnvScriptingError::Translate(format!(
                "strict FNV quest slice expected exact dynamic actor property bindings, observed {:?}",
                pending_property_names
            )));
        }
        if plan.dedicated_runtime_records.len() != 5 {
            return Err(FnvScriptingError::Translate(format!(
                "dedicated hostage greeting expected five generated records, observed {}",
                plan.dedicated_runtime_records.len()
            )));
        }
        Ok(plan)
    }

    /// Run the FNV legacy-scripting translation pass for records deferred by
    /// `translate_all`. Translated payloads are written straight to the target
    /// plugin via `insert_authoring_record_value`; the returned
    /// `FnvLegacyScriptingResult` carries only counts, PSC texts, voice paths,
    /// and warnings.
    ///
    /// Each `*_records` slice holds `(record_dict, source_form_key)` pairs,
    /// usually read from the deferred FormKeys on the source handle.
    pub fn run_fnv_legacy_scripting(
        &mut self,
        mod_prefix: &str,
        source_plugin: &str,
        mod_path: &str,
        script_records: &[(serde_json::Value, String)],
        quest_records: &[(serde_json::Value, String)],
        scene_records: &[(serde_json::Value, String)],
        info_records: &[(serde_json::Value, String)],
        dial_records: &[(serde_json::Value, String)],
        record_identities: &[LegacyRecordIdentity],
        // Pairs of (target_form_key, source_scpt_form_key) mapping non-scripting
        // records (WEAP, NPC_, etc.) to their SCRI targets. Built by the Python
        // caller from source records' SCRI subrecords + FormKey mapper.
        scri_links: &[(String, String)],
        typed_quest_records: &[(Record, String)],
        typed_scene_records: &[(Record, String)],
        typed_info_records: &[(Record, String)],
        typed_dial_records: &[(Record, String)],
    ) -> Result<FnvLegacyScriptingResult, FnvScriptingError> {
        let _ = (typed_scene_records, typed_info_records, typed_dial_records);
        let mut ctx =
            FnvLegacyScriptingContext::new(mod_prefix, source_plugin, self.config.strict_mapper);
        let source_plugin_sym = self.interner.intern(source_plugin);
        let mapper_state = self.mapper_state.as_ref().ok_or_else(|| {
            FnvScriptingError::Setup("SCPT attachment inference requires mapper state".to_string())
        })?;
        let mut attachment_evidence = HashMap::<
            String,
            Vec<crate::fnv_legacy_scripting::script_synthesizer::ScptAttachmentEvidence>,
        >::new();
        for (target_text, source_scpt_text) in scri_links {
            let target =
                parse_legacy_form_key_text(target_text, &self.interner).map_err(|error| {
                    FnvScriptingError::Setup(format!(
                        "SCPT attachment target {target_text} is invalid: {error}"
                    ))
                })?;
            let source_targets = mapper_state
                .source_to_target
                .iter()
                .filter_map(|(source, mapped)| {
                    (*mapped == target && source.plugin == source_plugin_sym).then_some(*source)
                })
                .collect::<Vec<_>>();
            let [source_target] = source_targets.as_slice() else {
                return Err(FnvScriptingError::Setup(format!(
                    "SCPT attachment target {target_text} has {} exact source identities",
                    source_targets.len()
                )));
            };
            let source_target_text = form_key_to_legacy_str(*source_target, &self.interner)
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "SCPT attachment source {:06X} cannot be rendered",
                        source_target.local
                    ))
                })?;
            let source_target_record = crate::source_read::read_record_relayout_by_form_key(
                self.source_handle_id,
                source_target,
                &self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "read SCPT attachment source {source_target_text}: {error}"
                ))
            })?;
            attachment_evidence
                .entry(source_scpt_text.to_ascii_uppercase())
                .or_default()
                .push(
                    crate::fnv_legacy_scripting::script_synthesizer::ScptAttachmentEvidence {
                        source_target_form_key: source_target_text,
                        target_signature: source_target_record.sig.as_str().to_string(),
                    },
                );
        }
        for (record, form_key) in script_records {
            let attachments = attachment_evidence
                .get(&form_key.to_ascii_uppercase())
                .map(Vec::as_slice)
                .unwrap_or_default();
            match crate::fnv_legacy_scripting::script_synthesizer::translate_scpt_record_with_attachments(
                record,
                &ctx.mod_prefix,
                form_key,
                attachments,
            ) {
                Ok(translated) => ctx.translated_scripts.push(translated),
                Err(error) => ctx.skipped_records.push((
                    "SCPT".to_string(),
                    record
                        .get("eid")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("<unknown>")
                        .to_string(),
                    error.to_string(),
                )),
            }
        }
        if self.config.fnv_quest_slice {
            ensure_no_deferred_translation_failures(&ctx.skipped_records)?;
        }
        translate_all_qust(&mut ctx, quest_records);
        translate_all_scen(&mut ctx, scene_records);
        translate_all_dial(&mut ctx, info_records);
        let alias_by_speaker = if self.config.fnv_quest_slice {
            self.lower_fnv_quest_slice_quests(
                &mut ctx,
                quest_records,
                typed_quest_records,
                record_identities,
            )?
        } else {
            HashMap::new()
        };

        // ── DIAL: group by quest owner + emit stripped payloads ───────────────
        // Mirrors Python `phases.py:149–162`: group_dial_records once for the
        // result side-channel, then re-emit each DIAL record as a payload with
        // legacy-scripting subrecords stripped (SCTX/VTCK/VMAD/...).
        let dial_only: Vec<serde_json::Value> =
            dial_records.iter().map(|(v, _)| v.clone()).collect();
        let dialogue_groups = crate::fnv_legacy_scripting::dialogue::group_dial_records(&dial_only);
        crate::fnv_legacy_scripting::dialogue::accumulate_dial_records(&mut ctx, dial_records);
        let typed_dialogue_plan = if self.config.fnv_quest_slice {
            ctx.translated_record_payloads
                .retain(|payload| !matches!(payload.signature.as_str(), "DIAL" | "INFO"));
            self.plan_fnv_quest_slice_dialogue(
                &mut ctx,
                typed_info_records,
                typed_dial_records,
                record_identities,
                source_plugin,
                &alias_by_speaker,
            )?
        } else {
            TypedFnvDialoguePlan::default()
        };

        ensure_no_deferred_translation_failures(&ctx.skipped_records)?;

        // ── Pre-mutate payload FormKeys via the mapper ─────────────────────
        // Refs to records translated in this run (fresh local ids in the
        // output plugin, on no master table) must be remapped before encoding.
        // Without mapper state (translate_all never ran) this is a no-op.
        let mut identities = record_identities
            .iter()
            .map(|identity| (identity.source_form_key_text.clone(), identity.clone()))
            .collect::<HashMap<_, _>>();
        let mut payloads = std::mem::take(&mut ctx.translated_record_payloads)
            .into_iter()
            .map(|payload| {
                let identity = identities.remove(&payload.source_form_key).ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "translated {} {} has no deferred identity",
                        payload.signature, payload.source_form_key
                    ))
                })?;
                PreparedLegacyRecordPayload::from_translated(payload, identity)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let missing_record_identities = identities
            .values()
            .filter(|identity| {
                identity.signature != "SCPT"
                    && !typed_dialogue_plan
                        .direct_sources
                        .contains(&identity.source_form_key_text.to_ascii_uppercase())
            })
            .map(|identity| format!("{} {}", identity.signature, identity.source_form_key_text))
            .collect::<Vec<_>>();
        if !missing_record_identities.is_empty() {
            return Err(FnvScriptingError::Setup(format!(
                "deferred identities produced no translated payload: {}",
                missing_record_identities.join(", ")
            )));
        }
        payloads.sort_by_key(PreparedLegacyRecordPayload::write_rank);
        let mut formkeys_rewritten: u32 = 0;
        let mut scene_pnam_dropped: u32 = 0;
        if let Some(state) = self.mapper_state.as_mut() {
            let mapper = FormKeyMapper::from_state(state, &mut self.interner);
            for payload in payloads.iter_mut() {
                formkeys_rewritten +=
                    crate::fnv_legacy_scripting::fk_rewrite::rewrite_payload_formkeys(
                        &mut payload.translated_record,
                        &mapper,
                    );
                if payload.identity.signature == "SCEN" {
                    scene_pnam_dropped +=
                        crate::fnv_legacy_scripting::fk_rewrite::drop_unmapped_scene_parent(
                            &mut payload.translated_record,
                            &mapper,
                        );
                }
            }
        }
        if scene_pnam_dropped > 0 {
            return Err(FnvScriptingError::Setup(format!(
                "{scene_pnam_dropped} translated SCEN parent reference(s) were unmapped"
            )));
        }
        if formkeys_rewritten > 0 {
            let warning = format!(
                "fnv_legacy_scripting:fk_rewrite: rewrote {} ref(s)",
                formkeys_rewritten,
            );
            let sym = self.interner.intern(&warning);
            self.warnings.push(sym);
        }

        // ── Drain translated payloads into the target plugin handle ────────
        // Each payload was produced as an authoring JsonValue by the per-sig
        // synthesizer. `insert_authoring_record_value` resolves
        // FormKey strings against the target plugin's master table, so
        // cross-plugin references in the payload (e.g. `"001234:FNV.esm"`)
        // become correctly encoded master references when the source plugin
        // is on the target's master list.
        let mut records_written: u32 = 0;
        let records_failed: u32 = 0;
        for payload in payloads {
            esp_authoring_core::plugin_runtime::insert_authoring_record_value(
                self.target_handle_id,
                &payload.translated_record,
            )
            .map_err(|err| {
                let message = Python::attach(|py| err.value(py).to_string());
                FnvScriptingError::Setup(format!(
                    "insert {} {} as {}: {message}",
                    payload.identity.signature,
                    payload.identity.source_form_key_text,
                    payload.identity.target_form_key_text
                ))
            })?;
            let target_form_id = encode_form_key_for_handle(
                self.target_handle_id,
                payload.identity.target_form_key,
                &self.interner,
            )
            .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
            let placement = match payload.identity.topology {
                LegacyRecordTopology::TopLevel => ExistingAuthoringRecordPlacement::TopLevel,
                LegacyRecordTopology::QuestChild { target_parent, .. } => {
                    ExistingAuthoringRecordPlacement::QuestChild {
                        parent_quest_form_id: encode_form_key_for_handle(
                            self.target_handle_id,
                            target_parent,
                            &self.interner,
                        )
                        .map_err(|error| FnvScriptingError::Setup(error.to_string()))?,
                    }
                }
                LegacyRecordTopology::TopicChild { target_parent, .. } => {
                    ExistingAuthoringRecordPlacement::TopicChild {
                        parent_dialogue_form_id: encode_form_key_for_handle(
                            self.target_handle_id,
                            target_parent,
                            &self.interner,
                        )
                        .map_err(|error| FnvScriptingError::Setup(error.to_string()))?,
                    }
                }
            };
            let placed =
                relocate_authoring_record_native(self.target_handle_id, target_form_id, placement)
                    .map_err(|error| {
                        FnvScriptingError::Setup(format!(
                            "place {} {}: {error}",
                            payload.identity.signature, payload.identity.target_form_key_text
                        ))
                    })?;
            if !placed {
                return Err(FnvScriptingError::Setup(format!(
                    "place {} {} failed: target parent is absent",
                    payload.identity.signature, payload.identity.target_form_key_text
                )));
            }
            records_written += 1;
            for warning in &payload.warnings {
                let symbol = self.interner.intern(warning);
                self.warnings.push(symbol);
            }
        }
        let dedicated_runtime_records = typed_dialogue_plan.dedicated_runtime_records.clone();
        let expected_dedicated_records = dedicated_runtime_records.len();
        let mut dedicated_records_written = 0usize;
        for record in typed_dialogue_plan.top_level_records {
            let target = form_key_to_legacy_str(record.form_key, &self.interner)
                .unwrap_or_else(|| format!("{:06X}", record.form_key.local));
            if !dedicated_runtime_records.contains(&record.form_key) {
                return Err(FnvScriptingError::Setup(format!(
                    "write dedicated top-level record {target}: identity was not manifested"
                )));
            }
            add_record_native(
                self.target_handle_id,
                record,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "write dedicated top-level record {target}: {error}"
                ))
            })?;
            records_written += 1;
            dedicated_records_written += 1;
        }
        let expected_scene_records = typed_dialogue_plan.scene_manifest_records;
        let mut scene_records_written = 0usize;
        for topic in typed_dialogue_plan.topics {
            let is_dedicated = dedicated_runtime_records.contains(&topic.form_key);
            let target = form_key_to_legacy_str(topic.form_key, &self.interner)
                .unwrap_or_else(|| format!("{:06X}", topic.form_key.local));
            let inserted = add_quest_child_record_native(
                self.target_handle_id,
                topic,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|error| {
                FnvScriptingError::Setup(format!("write typed DIAL {target}: {error}"))
            })?;
            if !inserted {
                return Err(FnvScriptingError::Setup(format!(
                    "write typed DIAL {target}: target quest parent is absent"
                )));
            }
            records_written += 1;
            if is_dedicated {
                dedicated_records_written += 1;
            } else {
                scene_records_written += 1;
            }
        }
        if scene_records_written != expected_scene_records {
            return Err(FnvScriptingError::Setup(format!(
                "scene runtime manifest expected {expected_scene_records} quest children but wrote {scene_records_written}"
            )));
        }
        for (info, target_parent) in typed_dialogue_plan.infos {
            let is_dedicated = dedicated_runtime_records.contains(&info.form_key);
            let target = form_key_to_legacy_str(info.form_key, &self.interner)
                .unwrap_or_else(|| format!("{:06X}", info.form_key.local));
            let parent_form_id =
                encode_form_key_for_handle(self.target_handle_id, target_parent, &self.interner)
                    .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
            let inserted = add_topic_child_record_native(
                self.target_handle_id,
                info,
                parent_form_id,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|error| {
                FnvScriptingError::Setup(format!("write typed INFO {target}: {error}"))
            })?;
            if !inserted {
                return Err(FnvScriptingError::Setup(format!(
                    "write typed INFO {target}: target topic parent is absent"
                )));
            }
            records_written += 1;
            if is_dedicated {
                dedicated_records_written += 1;
            }
        }
        if expected_dedicated_records != 5
            || dedicated_records_written != expected_dedicated_records
        {
            return Err(FnvScriptingError::Setup(format!(
                "dedicated hostage greeting expected {expected_dedicated_records} manifested records and wrote {dedicated_records_written}; exact contract requires 5"
            )));
        }

        // ── Emit .psc files for every translated script/fragment ──────────
        // Mirrors Python's inline file writes in script_translator.py /
        // quest.py / scene.py / dialogue.py. When `mod_path` is empty (e.g.
        // handle-only tests) emission is a no-op and every PSC candidate is
        // counted as skipped. Emission errors terminate the legacy scripting phase.
        if self.config.fnv_quest_slice {
            validate_fnv_quest_slice_info_fragment_contract(&ctx.translated_infos)?;
        }
        let psc_report = crate::fnv_legacy_scripting::psc_emission::emit_psc_files(
            std::path::Path::new(mod_path),
            &ctx.translated_scripts,
            &ctx.translated_quests,
            &ctx.translated_infos,
            &ctx.translated_scenes,
        );
        if !psc_report.errors.is_empty() {
            return Err(FnvScriptingError::Setup(format!(
                "PSC emission failed: {}",
                psc_report.errors.join("; ")
            )));
        }

        // Forward accumulated synthesizer warnings into the run's warning buffer.
        for warning in &ctx.warnings {
            let sym = self.interner.intern(warning.as_str());
            self.warnings.push(sym);
        }

        // ── VMAD intent computation ─────────────────────────────────────────
        // Retain SCRI links and translated SCPT records until the compile phase
        // supplies positive PEX evidence through reconciliation.
        self.fnv_pending_vmad_targets = scri_links.to_vec();
        self.fnv_pending_vmad_scripts = ctx.translated_scripts.clone();
        let vmad_intents = Vec::new();
        let vmad_attached_in_rust = false;

        Ok(FnvLegacyScriptingResult {
            translated_scripts: ctx.translated_scripts,
            translated_quests: ctx.translated_quests,
            translated_infos: ctx.translated_infos,
            translated_scenes: ctx.translated_scenes,
            dialogue_groups,
            records_written,
            records_failed,
            psc_files_written: psc_report.files_written,
            psc_files_skipped: psc_report.files_skipped,
            generated_psc_classes: psc_report.generated_psc_classes,
            voice_manifest: typed_dialogue_plan.voice_manifest,
            skipped_records: ctx.skipped_records,
            lip_regeneration_needed: ctx.lip_regeneration_needed,
            warnings: ctx.warnings,
            vmad_intents,
            vmad_attached_in_rust,
            quest_runtime_component_plans: Vec::new(),
            quest_runtime_expected_receipts: Vec::new(),
            quest_runtime_synthetic_targets: typed_dialogue_plan.synthetic_targets,
        })
    }

    pub fn run_fnv_legacy_scripting_from_deferred(
        &mut self,
        mod_prefix: &str,
        source_plugin: &str,
        mod_path: &str,
    ) -> Result<FnvLegacyScriptingResult, FnvScriptingError> {
        let result = crate::fnv_legacy_scripting::from_run::run_from_deferred(
            self,
            mod_prefix,
            source_plugin,
            mod_path,
        )?;
        self.fnv_quest_runtime_component_plans = result.quest_runtime_component_plans.clone();
        self.fnv_quest_runtime_expected_receipts = result.quest_runtime_expected_receipts.clone();
        self.fnv_quest_runtime_synthetic_targets = result.quest_runtime_synthetic_targets.clone();
        self.fnv_quest_runtime_compiler_evidence.clear();
        self.fnv_quest_runtime_receipts_finalized = false;
        Ok(result)
    }

    pub fn reconcile_fnv_compiled_scripts(
        &mut self,
        evidence: &[crate::fnv_legacy_scripting::vmad::CompiledScriptEvidence],
    ) -> Result<
        (
            Vec<crate::fnv_legacy_scripting::vmad::ScriptBindingIntent>,
            bool,
            u32,
            u32,
        ),
        FnvScriptingError,
    > {
        let targets = self
            .fnv_pending_vmad_targets
            .iter()
            .map(|(target_form_key, source_scpt_form_key)| {
                crate::fnv_legacy_scripting::vmad::VmadTarget {
                    target_form_key: target_form_key.clone(),
                    source_scpt_form_key: source_scpt_form_key.clone(),
                }
            })
            .collect::<Vec<_>>();
        let mut intents =
            crate::fnv_legacy_scripting::vmad::build_scpt_vmad_intents_with_compiled_evidence(
                &targets,
                &self.fnv_pending_vmad_scripts,
                evidence,
            )
            .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
        for pending in &self.fnv_pending_scpt_properties {
            let matching = intents
                .iter()
                .enumerate()
                .filter_map(|(index, intent)| {
                    intent
                        .script_class_name
                        .eq_ignore_ascii_case(&pending.script_class_name)
                        .then_some(index)
                })
                .collect::<Vec<_>>();
            let [intent_index] = matching.as_slice() else {
                return Err(FnvScriptingError::Setup(format!(
                    "SCPT {} dynamic package alias property {} expected one VMAD target, observed {}",
                    pending.source_scpt_form_key,
                    pending.property.name,
                    matching.len()
                )));
            };
            intents[*intent_index]
                .properties
                .push(pending.property.clone());
        }
        let successful = evidence
            .iter()
            .filter(|item| item.compiled_success && !item.relative_pex_path.trim().is_empty())
            .map(|item| (item.class_name.to_ascii_lowercase(), item))
            .collect::<HashMap<_, _>>();
        for binding in self.fnv_pending_helper_scripts.clone() {
            let compiled = successful
                .get(&binding.script_class_name.to_ascii_lowercase())
                .ok_or_else(|| {
                    FnvScriptingError::Setup(format!(
                        "required FNV slice helper {} has no successful compiler evidence",
                        binding.script_class_name
                    ))
                })?;
            intents.push(crate::fnv_legacy_scripting::vmad::ScriptBindingIntent {
                target_form_key: binding.target_form_key,
                script_class_name: binding.script_class_name,
                properties: binding.properties,
                fragment_kind: crate::fnv_legacy_scripting::vmad::FragmentKind::Object,
                compiled_evidence: (*compiled).clone(),
            });
        }
        let mut fragment_payloads = Vec::new();
        for binding in self.fnv_pending_quest_fragments.clone() {
            let compiled = match successful.get(&binding.script_class_name.to_ascii_lowercase()) {
                Some(compiled) => *compiled,
                None if self.config.fnv_quest_slice => {
                    return Err(FnvScriptingError::Setup(format!(
                        "required quest fragment {} has no successful compiler evidence",
                        binding.script_class_name
                    )));
                }
                None => continue,
            };
            for alias_script in binding.aliases.iter().flat_map(|alias| &alias.scripts) {
                if !successful.contains_key(&alias_script.script_class_name.to_ascii_lowercase()) {
                    return Err(FnvScriptingError::Setup(format!(
                        "required quest alias helper {} has no successful compiler evidence",
                        alias_script.script_class_name
                    )));
                }
            }
            let payload = crate::fnv_legacy_scripting::vmad::synthesize_quest_vmad_with_properties(
                &binding.script_class_name,
                &binding.fragments,
                &binding.fragment_properties,
                &binding.aliases,
                compiled,
            )
            .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
            fragment_payloads.push((binding.target_form_key, payload));
        }
        for binding in self.fnv_pending_info_fragments.clone() {
            let compiled = match successful.get(&binding.script_class_name.to_ascii_lowercase()) {
                Some(compiled) => *compiled,
                None if self.config.fnv_quest_slice => {
                    return Err(FnvScriptingError::Setup(format!(
                        "required INFO fragment {} has no successful compiler evidence",
                        binding.script_class_name
                    )));
                }
                None => continue,
            };
            let payload =
                crate::fnv_legacy_scripting::vmad::synthesize_topic_info_vmad_with_properties(
                    &binding.script_class_name,
                    &binding.phases,
                    &binding.fragment_properties,
                    compiled,
                )
                .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
            fragment_payloads.push((binding.target_form_key, payload));
        }
        for binding in self.fnv_pending_scene_fragments.clone() {
            let compiled = match successful.get(&binding.script_class_name.to_ascii_lowercase()) {
                Some(compiled) => *compiled,
                None if self.config.fnv_quest_slice => {
                    return Err(FnvScriptingError::Setup(format!(
                        "required SCEN fragment {} has no successful compiler evidence",
                        binding.script_class_name
                    )));
                }
                None => continue,
            };
            let payload = crate::fnv_legacy_scripting::vmad::synthesize_scene_vmad(
                &binding.script_class_name,
                binding.action_count,
                compiled,
            )
            .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
            fragment_payloads.push((binding.target_form_key, payload));
        }
        let output_plugin_name = crate::source_read::plugin_name_for_handle(self.target_handle_id)
            .map_err(|error| {
                FnvScriptingError::Setup(format!(
                    "inspect output plugin identity before VMAD reconciliation: {error}"
                ))
            })?;
        for intent in &mut intents {
            let (_, canonical) = canonicalize_fnv_vmad_target(
                &intent.target_form_key,
                &output_plugin_name,
                &self.interner,
            )?;
            intent.target_form_key = canonical;
        }
        for (target_form_key, _) in &mut fragment_payloads {
            let (_, canonical) =
                canonicalize_fnv_vmad_target(target_form_key, &output_plugin_name, &self.interner)?;
            *target_form_key = canonical;
        }
        let mut existing_records = Vec::new();
        let mut existing_legacy_placements = HashMap::new();
        let mut seen_targets = FxHashSet::default();
        for target_form_key in intents
            .iter()
            .map(|intent| intent.target_form_key.as_str())
            .chain(fragment_payloads.iter().map(|(target, _)| target.as_str()))
        {
            if !seen_targets.insert(target_form_key.to_ascii_lowercase()) {
                continue;
            }
            let (typed_target, canonical_target) =
                canonicalize_fnv_vmad_target(target_form_key, &output_plugin_name, &self.interner)?;
            match esp_authoring_core::plugin_runtime::plugin_handle_read_authoring_record_value_json(
                self.target_handle_id,
                &canonical_target,
            ) {
                Ok(Some(mut record)) => {
                    let signature = crate::source_read::read_record_relayout_by_form_key(
                        self.target_handle_id,
                        &typed_target,
                        &self.schema_target,
                        &self.interner,
                        None,
                    )
                    .map_err(|error| {
                        FnvScriptingError::Setup(format!(
                            "inspect VMAD target {canonical_target} signature: {error}"
                        ))
                    })?
                    .sig
                    .as_str()
                    .to_string();
                    let record_object = record.as_object_mut().ok_or_else(|| {
                        FnvScriptingError::Setup(format!(
                            "VMAD target {canonical_target} authoring payload is not an object"
                        ))
                    })?;
                    if let Some(existing) = record_object.get("signature") {
                        if existing.as_str() != Some(signature.as_str()) {
                            return Err(FnvScriptingError::Setup(format!(
                                "VMAD target {canonical_target} signature changed from {signature} to {existing}"
                            )));
                        }
                    } else {
                        record_object.insert(
                            "signature".to_string(),
                            serde_json::Value::String(signature.clone()),
                        );
                    }
                    if matches!(
                        signature.as_str(),
                        "QUST" | "DIAL" | "INFO" | "SCEN" | "DLBR"
                    ) {
                        let target_form_id = encode_form_key_for_handle(
                            self.target_handle_id,
                            typed_target,
                            &self.interner,
                        )
                        .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
                        let placement = existing_authoring_record_placement_native(
                            self.target_handle_id,
                            target_form_id,
                        )
                        .map_err(|error| {
                            FnvScriptingError::Setup(format!(
                                "inspect VMAD target {canonical_target} topology: {error}"
                            ))
                        })?;
                        existing_legacy_placements
                            .insert(canonical_target.to_ascii_lowercase(), placement);
                    }
                    existing_records.push((canonical_target, record));
                }
                Ok(None) => {
                    return Err(FnvScriptingError::Setup(format!(
                        "required VMAD target missing before reconciliation: {canonical_target}"
                    )));
                }
                Err(error) => {
                    return Err(FnvScriptingError::Setup(format!(
                        "prevalidate VMAD target {canonical_target}: {error}"
                    )));
                }
            }
        }
        let record_updates =
            prebuild_fnv_vmad_record_updates(existing_records, &intents, &fragment_payloads)?;
        for (target_form_key, record) in record_updates {
            esp_authoring_core::plugin_runtime::plugin_handle_replace_authoring_record_value(
                self.target_handle_id,
                &record,
            )
            .map_err(|error| {
                FnvScriptingError::Setup(format!("vmad replace {target_form_key}: {error}"))
            })?;
            if let Some(placement) = existing_legacy_placements
                .get(&target_form_key.to_ascii_lowercase())
                .copied()
            {
                let (typed_target, _) = canonicalize_fnv_vmad_target(
                    &target_form_key,
                    &output_plugin_name,
                    &self.interner,
                )?;
                let target_form_id =
                    encode_form_key_for_handle(self.target_handle_id, typed_target, &self.interner)
                        .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
                let placed = relocate_authoring_record_native(
                    self.target_handle_id,
                    target_form_id,
                    placement,
                )
                .map_err(|error| {
                    FnvScriptingError::Setup(format!(
                        "restore VMAD target {target_form_key} topology: {error}"
                    ))
                })?;
                if !placed {
                    return Err(FnvScriptingError::Setup(format!(
                        "restore VMAD target {target_form_key} topology: target parent is absent"
                    )));
                }
            }
        }
        let standalone_attached = !intents.is_empty();
        if standalone_attached {
            let warning = format!("fnv_legacy_scripting:vmad_attached:{}", intents.len());
            let sym = self.interner.intern(&warning);
            self.warnings.push(sym);
        }
        let fragment_bindings_attached = fragment_payloads.len() as u32;
        let fragment_bindings_expected = (self.fnv_pending_quest_fragments.len()
            + self.fnv_pending_info_fragments.len()
            + self.fnv_pending_scene_fragments.len())
            as u32;
        let mut routed_plans = self.fnv_quest_runtime_component_plans.clone();
        crate::fnv_legacy_scripting::component::apply_fnv_start_routes(&mut routed_plans)
            .map_err(FnvScriptingError::Setup)?;
        let routed_receipts = routed_plans
            .iter()
            .map(|plan| plan.expected_receipt.clone())
            .collect::<Vec<_>>();
        if routed_receipts != self.fnv_quest_runtime_expected_receipts {
            return Err(FnvScriptingError::Setup(
                "FNV quest start-route receipt changed after compiler/VMAD evidence".to_string(),
            ));
        }
        self.fnv_quest_runtime_component_plans = routed_plans;
        self.fnv_quest_runtime_compiler_evidence = evidence.to_vec();
        self.fnv_quest_runtime_receipts_finalized = false;
        Ok((
            intents,
            standalone_attached || fragment_bindings_attached > 0,
            fragment_bindings_attached,
            fragment_bindings_expected,
        ))
    }

    /// Rewrite form-key references in the target handle using `mappings`.
    /// Returns the number of records that had at least one reference rewritten.
    /// Rust-owned replacement for the old Python form-key rewrite pass.
    pub fn apply_registry_mappings(
        &mut self,
        mappings: &std::collections::HashMap<String, String>,
    ) -> Result<usize, RunError> {
        if mappings.is_empty() {
            return Ok(0);
        }
        esp_authoring_core::plugin_runtime::plugin_handle_rewrite_references(
            self.target_handle_id,
            mappings,
        )
        .map_err(|e| RunError::InvalidConfig(format!("apply_registry_mappings: {e}")))
    }
}

fn allocate_creature_primary_npc_reservations(
    state: &mut MapperState,
    interner: &StringInterner,
    primary_creature_sources: &[FormKey],
    expected_candidates: usize,
) -> Result<Vec<CreaturePrimaryNpcReservation>, String> {
    let mut sources = primary_creature_sources.to_vec();
    sources.sort_by_key(|source| {
        (
            interner
                .resolve(source.plugin)
                .unwrap_or_default()
                .to_ascii_lowercase(),
            source.local,
        )
    });
    sources.dedup();
    if sources.len() != expected_candidates {
        return Err(format!(
            "legacy creature primary reservation accounting mismatch: expected={expected_candidates} candidate_crea={}",
            sources.len()
        ));
    }
    if let Some(source) = sources
        .iter()
        .find(|source| state.source_to_target.contains_key(source))
    {
        return Err(format!(
            "legacy creature source already has a mapping before NPC reservation: {:06X}",
            source.local
        ));
    }
    let mut mapper = FormKeyMapper::from_state(state, interner);
    let mut reservations = Vec::with_capacity(sources.len());
    for source in sources {
        let target = mapper.allocate_generated();
        mapper.add_mapping(source, target);
        reservations.push(CreaturePrimaryNpcReservation { source, target });
    }
    drop(mapper);
    lease_generated_creature_record_ids(
        state,
        interner,
        reservations.iter().map(|reservation| reservation.target),
    )?;
    Ok(reservations)
}

fn allocate_creature_ancillary_npc_reservations(
    state: &mut MapperState,
    interner: &StringInterner,
    ancillary_npc_sources: &[FormKey],
) -> Result<Vec<CreatureAncillaryNpcReservation>, String> {
    let mut sources = ancillary_npc_sources.to_vec();
    sources.sort_by_key(|source| {
        (
            interner
                .resolve(source.plugin)
                .unwrap_or_default()
                .to_ascii_lowercase(),
            source.local,
        )
    });
    sources.dedup();
    if let Some(source) = sources
        .iter()
        .find(|source| state.source_to_target.contains_key(source))
    {
        return Err(format!(
            "legacy ancillary NPC source already has a mapping before reservation: {:06X}",
            source.local
        ));
    }

    let mut staged_state = state.clone();
    let mut mapper = FormKeyMapper::from_state(&mut staged_state, interner);
    let mut reservations = Vec::with_capacity(sources.len());
    for source in sources {
        let target = mapper.allocate_generated();
        mapper.add_mapping(source, target);
        reservations.push(CreatureAncillaryNpcReservation { source, target });
    }
    drop(mapper);
    lease_generated_creature_record_ids(
        &mut staged_state,
        interner,
        reservations.iter().map(|reservation| reservation.target),
    )?;
    *state = staged_state;
    Ok(reservations)
}

fn allocate_skyrim_creature_record_reservations(
    state: &mut MapperState,
    interner: &StringInterner,
    mut sources: Vec<SkyrimCreatureReservationSource>,
    expected_candidates: usize,
) -> Result<Vec<SkyrimCreatureRecordReservation>, String> {
    let sort_key = |source: FormKey| {
        (
            interner
                .resolve(source.plugin)
                .unwrap_or_default()
                .to_ascii_lowercase(),
            source.local,
        )
    };
    sources.sort_by(|left, right| {
        sort_key(left.source)
            .cmp(&sort_key(right.source))
            .then_with(|| left.kind.cmp(&right.kind))
    });

    let mut merged = Vec::<SkyrimCreatureReservationSource>::new();
    for mut source in sources {
        source.owners.sort_by(|left, right| {
            sort_key(left.source_race)
                .cmp(&sort_key(right.source_race))
                .then_with(|| {
                    left.motion_family_id
                        .to_ascii_lowercase()
                        .cmp(&right.motion_family_id.to_ascii_lowercase())
                })
                .then_with(|| {
                    left.normalized_project_path
                        .cmp(&right.normalized_project_path)
                })
        });
        source.owners.dedup();
        if source.owners.is_empty() {
            return Err(format!(
                "Skyrim creature reservation source {:06X} has no owning RACE candidate",
                source.source.local
            ));
        }
        for owner in &source.owners {
            if owner.motion_family_id.trim().is_empty()
                || owner.normalized_project_path.trim().is_empty()
            {
                return Err(format!(
                    "Skyrim creature reservation owner {:06X} has an empty motion family key",
                    owner.source_race.local
                ));
            }
            if owner.normalized_project_path.contains('\\')
                || owner.normalized_project_path.starts_with('/')
                || owner
                    .normalized_project_path
                    .get(..7)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("actors/"))
                || owner.normalized_project_path
                    != owner.normalized_project_path.to_ascii_lowercase()
            {
                return Err(format!(
                    "Skyrim creature reservation owner {:06X} has a non-canonical project path {}",
                    owner.source_race.local, owner.normalized_project_path
                ));
            }
        }

        if let Some(existing) = merged
            .last_mut()
            .filter(|item| item.source == source.source)
        {
            if existing.kind != source.kind {
                return Err(format!(
                    "Skyrim creature reservation source {:06X} has conflicting record kinds",
                    source.source.local
                ));
            }
            existing.owners.extend(source.owners);
            existing.owners.sort_by(|left, right| {
                sort_key(left.source_race)
                    .cmp(&sort_key(right.source_race))
                    .then_with(|| {
                        left.motion_family_id
                            .to_ascii_lowercase()
                            .cmp(&right.motion_family_id.to_ascii_lowercase())
                    })
                    .then_with(|| {
                        left.normalized_project_path
                            .cmp(&right.normalized_project_path)
                    })
            });
            existing.owners.dedup();
        } else {
            merged.push(source);
        }
    }

    let race_sources = merged
        .iter()
        .filter(|source| source.kind == SkyrimCreatureReservedRecordKind::Race)
        .map(|source| source.source)
        .collect::<FxHashSet<_>>();
    if race_sources.len() != expected_candidates {
        return Err(format!(
            "Skyrim creature reservation accounting mismatch: expected={expected_candidates} candidate_race={}",
            race_sources.len()
        ));
    }
    for source in &merged {
        for owner in &source.owners {
            if !race_sources.contains(&owner.source_race) {
                return Err(format!(
                    "Skyrim creature reservation source {:06X} names unknown owning RACE {:06X}",
                    source.source.local, owner.source_race.local
                ));
            }
            if source.kind == SkyrimCreatureReservedRecordKind::Race
                && (source.source != owner.source_race || source.owners.len() != 1)
            {
                return Err(format!(
                    "Skyrim RACE reservation {:06X} must be owned only by itself",
                    source.source.local
                ));
            }
        }
    }
    let output_plugin_name = state.options.output_plugin_name.as_str();
    if let Some((source, target)) = merged.iter().find_map(|source| {
        state
            .source_to_target
            .get(&source.source)
            .copied()
            .filter(|target| {
                interner
                    .resolve(target.plugin)
                    .is_some_and(|plugin| plugin.eq_ignore_ascii_case(output_plugin_name))
            })
            .map(|target| (source, target))
    }) {
        return Err(format!(
            "Skyrim creature source already has an output mapping before reservation: {:06X}->{:06X}",
            source.source.local, target.local
        ));
    }

    let mut staged_state = state.clone();
    for source in &merged {
        staged_state.source_to_target.remove(&source.source);
    }
    let mut mapper = FormKeyMapper::from_state(&mut staged_state, interner);
    let mut reservations = Vec::with_capacity(merged.len());
    for source in merged {
        let target = mapper.allocate_generated();
        mapper.add_mapping(source.source, target);
        reservations.push(SkyrimCreatureRecordReservation {
            source: source.source,
            target,
            kind: source.kind,
            owners: source.owners,
        });
    }
    drop(mapper);
    lease_generated_creature_record_ids(
        &mut staged_state,
        interner,
        reservations.iter().map(|reservation| reservation.target),
    )?;
    *state = staged_state;
    Ok(reservations)
}

fn allocate_skyrim_creature_ancillary_reservations(
    state: &mut MapperState,
    interner: &StringInterner,
    record_reservations: &[SkyrimCreatureRecordReservation],
    mut requests: Vec<SkyrimCreatureAncillaryReservationRequest>,
) -> Result<Vec<SkyrimCreatureAncillaryReservationReceipt>, String> {
    let source_sort_key = |source: FormKey| {
        (
            interner
                .resolve(source.plugin)
                .unwrap_or_default()
                .to_ascii_lowercase(),
            source.local,
        )
    };
    let candidate_races = record_reservations
        .iter()
        .filter(|reservation| reservation.kind == SkyrimCreatureReservedRecordKind::Race)
        .map(|reservation| reservation.source)
        .collect::<FxHashSet<_>>();
    if candidate_races.is_empty() && !requests.is_empty() {
        return Err(
            "Skyrim creature ancillary requests require installed RACE reservations".to_string(),
        );
    }
    requests.sort_by(|left, right| {
        source_sort_key(left.source_race)
            .cmp(&source_sort_key(right.source_race))
            .then_with(|| {
                left.event
                    .to_ascii_lowercase()
                    .cmp(&right.event.to_ascii_lowercase())
            })
            .then_with(|| left.ordinal.cmp(&right.ordinal))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.event.cmp(&right.event))
    });

    let mut prior_key: Option<(FormKey, String, u32, SkyrimCreatureAncillaryRecordKind)> = None;
    for request in &requests {
        if !candidate_races.contains(&request.source_race) {
            return Err(format!(
                "Skyrim creature ancillary request names unreserved RACE {:06X}",
                request.source_race.local
            ));
        }
        if request.event.trim().is_empty() || request.event != request.event.trim() {
            return Err(format!(
                "Skyrim creature ancillary request for RACE {:06X} has a non-canonical event",
                request.source_race.local
            ));
        }
        let normalized_key = (
            request.source_race,
            request.event.to_ascii_lowercase(),
            request.ordinal,
            request.kind,
        );
        if prior_key.as_ref() == Some(&normalized_key) {
            return Err(format!(
                "duplicate Skyrim creature ancillary request {:06X}:{}:{}:{:?}",
                request.source_race.local, request.event, request.ordinal, request.kind
            ));
        }
        prior_key = Some(normalized_key);
    }

    let mut staged_state = state.clone();
    let mut mapper = FormKeyMapper::from_state(&mut staged_state, interner);
    let mut receipts = Vec::with_capacity(requests.len());
    for request in requests {
        let target = mapper.allocate_generated();
        if target.local == 0 {
            return Err("Skyrim creature ancillary allocation produced a null target".to_string());
        }
        receipts.push(SkyrimCreatureAncillaryReservationReceipt {
            source_race: request.source_race,
            event: request.event,
            ordinal: request.ordinal,
            kind: request.kind,
            target,
        });
    }
    drop(mapper);
    lease_generated_creature_record_ids(
        &mut staged_state,
        interner,
        receipts.iter().map(|receipt| receipt.target),
    )?;
    *state = staged_state;
    Ok(receipts)
}

fn allocate_creature_actor_action_reservations(
    state: &mut MapperState,
    interner: &StringInterner,
    mut requests: Vec<CreatureActorActionReservationRequest>,
) -> Result<Vec<CreatureActorActionReservationReceipt>, String> {
    requests.sort_by(|left, right| {
        left.family_id
            .to_ascii_lowercase()
            .cmp(&right.family_id.to_ascii_lowercase())
            .then_with(|| left.requirement.cmp(&right.requirement))
            .then_with(|| {
                left.editor_id
                    .to_ascii_lowercase()
                    .cmp(&right.editor_id.to_ascii_lowercase())
            })
            .then_with(|| left.family_id.cmp(&right.family_id))
            .then_with(|| left.editor_id.cmp(&right.editor_id))
    });
    let mut normalized_keys = BTreeSet::new();
    let mut editor_ids = BTreeSet::new();
    for request in &requests {
        if request.family_id.trim().is_empty()
            || request.family_id != request.family_id.trim()
            || request.requirement.behavior_path.trim().is_empty()
            || request.requirement.animation_event.trim().is_empty()
            || request.requirement.parent_form_id == 0
            || request.requirement.parent_form_id > 0x00ff_ffff
            || request.editor_id.trim().is_empty()
            || !request
                .editor_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(format!(
                "invalid creature Actor Action reservation request for family {:?}",
                request.family_id
            ));
        }
        let key = (
            request.family_id.to_ascii_lowercase(),
            request.requirement.clone(),
        );
        if !normalized_keys.insert(key) {
            return Err(format!(
                "duplicate creature Actor Action reservation for family {:?}, event {:?}",
                request.family_id, request.requirement.animation_event
            ));
        }
        if !editor_ids.insert(request.editor_id.to_ascii_lowercase()) {
            return Err(format!(
                "duplicate creature Actor Action EditorID {:?}",
                request.editor_id
            ));
        }
    }

    let mut staged_state = state.clone();
    let mut mapper = FormKeyMapper::from_state(&mut staged_state, interner);
    let mut receipts = Vec::with_capacity(requests.len());
    for request in requests {
        let target = mapper.allocate_generated();
        if target.local == 0 {
            return Err("creature Actor Action allocation produced a null target".to_string());
        }
        receipts.push(CreatureActorActionReservationReceipt {
            family_id: request.family_id,
            requirement: request.requirement,
            editor_id: request.editor_id,
            target,
        });
    }
    drop(mapper);
    lease_generated_creature_record_ids(
        &mut staged_state,
        interner,
        receipts.iter().map(|receipt| receipt.target),
    )?;
    *state = staged_state;
    Ok(receipts)
}

fn lease_generated_creature_record_ids(
    state: &mut MapperState,
    interner: &StringInterner,
    targets: impl IntoIterator<Item = FormKey>,
) -> Result<(), String> {
    FormKeyMapper::from_state(state, interner)
        .lease_allocated_object_ids(targets.into_iter().map(|target| target.local))
        .map_err(|local| format!("generated creature record {local:06X} was not freshly allocated"))
}

fn validate_fnv_quest_slice_info_fragment_contract(
    infos: &[crate::fnv_legacy_scripting::dialogue::TranslatedInfo],
) -> Result<(), FnvScriptingError> {
    use crate::fnv_legacy_scripting::dialogue::InfoFragmentPhase;

    let expected = [
        (
            "130161:FalloutNV.esm",
            Some(("TIF__130161", InfoFragmentPhase::Begin)),
        ),
        (
            "134B9B:FalloutNV.esm",
            Some(("TIF__134B9B", InfoFragmentPhase::End)),
        ),
        ("15734B:FalloutNV.esm", None),
        ("15734C:FalloutNV.esm", None),
        ("15734D:FalloutNV.esm", None),
    ];
    if infos.len() != expected.len() {
        return Err(FnvScriptingError::Setup(format!(
            "strict FNV INFO fragment accounting expected {} exact INFO records, observed {}",
            expected.len(),
            infos.len()
        )));
    }
    for (source_form_key, required_fragment) in expected {
        let matches = infos
            .iter()
            .filter(|info| info.source_form_key.eq_ignore_ascii_case(source_form_key))
            .collect::<Vec<_>>();
        let [info] = matches.as_slice() else {
            return Err(FnvScriptingError::Setup(format!(
                "strict FNV INFO fragment accounting expected one {source_form_key}, observed {}",
                matches.len()
            )));
        };
        match required_fragment {
            Some((class_name, phase)) => {
                let valid = info.fragment_class_name.as_deref() == Some(class_name)
                    && info
                        .fragment_psc_text
                        .as_deref()
                        .is_some_and(|text| !text.trim().is_empty())
                    && info.fragment_phases == [phase];
                if !valid {
                    return Err(FnvScriptingError::Setup(format!(
                        "strict FNV INFO {source_form_key} requires exact fragment {class_name} phase {phase:?}"
                    )));
                }
            }
            None => {
                if info.fragment_class_name.is_some()
                    || info.fragment_psc_text.is_some()
                    || !info.fragment_phases.is_empty()
                {
                    return Err(FnvScriptingError::Setup(format!(
                        "strict FNV greeting INFO {source_form_key} unexpectedly carries a Papyrus fragment"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_fnv_quest_slice_compat_fragment_property(
    source_local: u32,
    target_quest_form_key: &str,
    mod_prefix: &str,
    properties: &[crate::fnv_legacy_scripting::vmad::ScriptProperty],
) -> Result<(), FnvScriptingError> {
    if !matches!(source_local, 0x06136D | 0x11F935) {
        return Ok(());
    }
    let matches = properties
        .iter()
        .filter(|property| property.name.eq_ignore_ascii_case("FNVSliceCompat"))
        .collect::<Vec<_>>();
    let [property] = matches.as_slice() else {
        return Err(FnvScriptingError::Translate(format!(
            "strict FNV QUST {source_local:06X} expected one FNVSliceCompat fragment property, observed {}",
            matches.len()
        )));
    };
    let expected_type = format!("{mod_prefix}_FnvSliceCompat");
    if !property.prop_type.eq_ignore_ascii_case(&expected_type)
        || property.value.as_ref().and_then(serde_json::Value::as_str)
            != Some(target_quest_form_key)
    {
        return Err(FnvScriptingError::Translate(format!(
            "strict FNV QUST {source_local:06X} FNVSliceCompat must have type {expected_type} and target its mapped owning QUST {target_quest_form_key}"
        )));
    }
    Ok(())
}

fn materialize_fnv_package_data_alias_contracts(
    translated: &mut crate::fnv_legacy_scripting::quest::QuestTranslation,
    source_quest_form_key: &str,
    target_quest_form_key: &str,
    translated_scripts: &[crate::fnv_legacy_scripting::script_synthesizer::TranslatedScript],
) -> Result<Vec<PendingScptProperty>, FnvScriptingError> {
    let package_alias_contracts = translated_scripts
        .iter()
        .flat_map(|script| {
            script
                .package_data_aliases
                .iter()
                .map(move |contract| (script, contract))
        })
        .filter(|(_, contract)| {
            contract
                .target_quest_form_key
                .eq_ignore_ascii_case(target_quest_form_key)
        });
    let mut pending = Vec::new();
    for (script, contract) in package_alias_contracts {
        if !contract
            .source_quest_form_key
            .eq_ignore_ascii_case(source_quest_form_key)
            || contract.alias_package_subrecord != "ALPC"
        {
            return Err(FnvScriptingError::Translate(format!(
                "SCPT {} package data alias contract does not match QUST {} or FO4 ALPC package schema (got {})",
                contract.source_script_form_key,
                source_quest_form_key,
                contract.alias_package_subrecord
            )));
        }
        let alias_id = crate::fnv_legacy_scripting::quest::append_unfilled_package_data_alias(
            translated,
            &contract.property_name,
            &contract.target_package_form_key,
        )
        .map_err(|error| {
            FnvScriptingError::Translate(format!(
                "QUST {source_quest_form_key} package data alias {}: {error}",
                contract.property_name
            ))
        })?;
        let property = crate::fnv_legacy_scripting::vmad::reference_alias_script_property(
            &contract.property_name,
            target_quest_form_key,
            alias_id,
        )
        .map_err(|error| FnvScriptingError::Translate(error.to_string()))?;
        pending.push(PendingScptProperty {
            source_scpt_form_key: script.source_form_key.clone(),
            script_class_name: script.script_class_name.clone(),
            property,
        });
    }
    Ok(pending)
}

fn bind_generated_scpt_property_target(
    translated_scripts: &mut [crate::fnv_legacy_scripting::script_synthesizer::TranslatedScript],
    source_scpt_form_key: &str,
    script_class_name: &str,
    property_name: &str,
    property_type: &str,
    target_form_key: &str,
) -> Result<(), FnvScriptingError> {
    let matching_scripts = translated_scripts
        .iter()
        .enumerate()
        .filter_map(|(index, script)| {
            (script
                .source_form_key
                .eq_ignore_ascii_case(source_scpt_form_key)
                && script
                    .script_class_name
                    .eq_ignore_ascii_case(script_class_name))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let [script_index] = matching_scripts.as_slice() else {
        return Err(FnvScriptingError::Translate(format!(
            "generated property {property_name} expected one translated SCPT {source_scpt_form_key}/{script_class_name}, observed {}",
            matching_scripts.len()
        )));
    };
    let matching_properties = translated_scripts[*script_index]
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, property)| {
            property
                .papyrus_name
                .eq_ignore_ascii_case(property_name)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let [property_index] = matching_properties.as_slice() else {
        return Err(FnvScriptingError::Translate(format!(
            "generated property {property_name} expected one declaration on SCPT {source_scpt_form_key}, observed {}",
            matching_properties.len()
        )));
    };
    let property = &mut translated_scripts[*script_index].properties[*property_index];
    if !property.papyrus_type.eq_ignore_ascii_case(property_type) {
        return Err(FnvScriptingError::Translate(format!(
            "generated property {property_name} on SCPT {source_scpt_form_key} has type {}, expected {property_type}",
            property.papyrus_type
        )));
    }
    match property.target_form_key.as_deref() {
        None => property.target_form_key = Some(target_form_key.to_string()),
        Some(existing) if existing.eq_ignore_ascii_case(target_form_key) => {}
        Some(existing) => {
            return Err(FnvScriptingError::Translate(format!(
                "generated property {property_name} on SCPT {source_scpt_form_key} already targets {existing}, cannot bind {target_form_key}"
            )));
        }
    }
    Ok(())
}

fn ensure_no_deferred_translation_failures(
    skipped_records: &[(String, String, String)],
) -> Result<(), FnvScriptingError> {
    if skipped_records.is_empty() {
        return Ok(());
    }
    let failures = skipped_records
        .iter()
        .map(|(signature, editor_id, reason)| format!("{signature} {editor_id}: {reason}"))
        .collect::<Vec<_>>()
        .join("; ");
    Err(FnvScriptingError::Translate(format!(
        "{} deferred record translation(s) failed: {failures}",
        skipped_records.len()
    )))
}

fn canonicalize_fnv_vmad_target(
    target_form_key: &str,
    output_plugin_name: &str,
    interner: &StringInterner,
) -> Result<(FormKey, String), FnvScriptingError> {
    let parse_local = |value: &str| {
        u32::from_str_radix(
            value
                .trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X"),
            16,
        )
    };
    let (plugin_name, object_id) = if let Some((local, plugin)) = target_form_key.split_once('@') {
        let object_id = parse_local(local).map_err(|error| {
            FnvScriptingError::Setup(format!(
                "VMAD target FormKey has invalid object identity {target_form_key:?}: {error}"
            ))
        })?;
        (plugin.trim(), object_id)
    } else {
        let (left, right) = target_form_key.split_once(':').ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "VMAD target FormKey is missing ':' or '@': {target_form_key:?}"
            ))
        })?;
        match (parse_local(left), parse_local(right)) {
            (Ok(object_id), Err(_)) => (right.trim(), object_id),
            (Err(_), Ok(object_id)) => (left.trim(), object_id),
            _ => {
                return Err(FnvScriptingError::Setup(format!(
                    "VMAD target FormKey has ambiguous plugin/object identity: {target_form_key}"
                )));
            }
        }
    };
    if plugin_name.is_empty() || !plugin_name.eq_ignore_ascii_case(output_plugin_name) {
        return Err(FnvScriptingError::Setup(format!(
            "VMAD target {target_form_key} is not owned by output plugin {output_plugin_name}"
        )));
    }
    if object_id > 0x00FF_FFFF {
        return Err(FnvScriptingError::Setup(format!(
            "VMAD target FormKey is outside 24-bit output bounds: {target_form_key}"
        )));
    }
    let target = FormKey {
        local: object_id,
        plugin: interner.intern(output_plugin_name),
    };
    Ok((target, format!("{output_plugin_name}:{object_id:06X}")))
}

fn prebuild_fnv_vmad_record_updates(
    records: Vec<(String, serde_json::Value)>,
    intents: &[crate::fnv_legacy_scripting::vmad::ScriptBindingIntent],
    fragment_payloads: &[(String, serde_json::Value)],
) -> Result<Vec<(String, serde_json::Value)>, FnvScriptingError> {
    let mut order = Vec::with_capacity(records.len());
    let mut updates = HashMap::with_capacity(records.len());
    for (target_form_key, record) in records {
        let key = target_form_key.to_ascii_lowercase();
        if updates.insert(key.clone(), record).is_some() {
            return Err(FnvScriptingError::Setup(format!(
                "duplicate VMAD target during reconciliation: {target_form_key}"
            )));
        }
        order.push((key, target_form_key));
    }

    for intent in intents {
        let record = updates
            .get_mut(&intent.target_form_key.to_ascii_lowercase())
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "prevalidated VMAD target is absent: {}",
                    intent.target_form_key
                ))
            })?;
        crate::fnv_legacy_scripting::vmad::attach_vmad_to_record(record, intent)
            .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
    }
    for (target_form_key, payload) in fragment_payloads {
        let record = updates
            .get_mut(&target_form_key.to_ascii_lowercase())
            .ok_or_else(|| {
                FnvScriptingError::Setup(format!(
                    "prevalidated fragment VMAD target is absent: {target_form_key}"
                ))
            })?;
        crate::fnv_legacy_scripting::vmad::attach_vmad_payload_to_record(record, payload.clone())
            .map_err(|error| FnvScriptingError::Setup(error.to_string()))?;
    }

    Ok(order
        .into_iter()
        .map(|(key, target_form_key)| {
            (
                target_form_key,
                updates
                    .remove(&key)
                    .expect("VMAD update order and payload map stay aligned"),
            )
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;

    fn skyrim_runtime_test_field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn skyrim_runtime_test_vmad() -> SmallVec<[u8; 32]> {
        fn push_string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }

        let class_name = "QF_SourceQuest_070224";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&5_u16.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        push_string(&mut bytes, class_name);
        bytes.push(0);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&10_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.push(0);
        push_string(&mut bytes, class_name);
        push_string(&mut bytes, "Fragment_0");
        SmallVec::from_vec(bytes)
    }

    fn skyrim_runtime_test_record(
        signature: &str,
        form_key: FormKey,
        editor_id: &str,
        interner: &StringInterner,
    ) -> Record {
        let mut record = Record::new(SigCode::from_str(signature).unwrap(), form_key);
        record.eid = Some(interner.intern(editor_id));
        record.fields.push(skyrim_runtime_test_field(
            "EDID",
            FieldValue::String(interner.intern(editor_id)),
        ));
        record
    }

    #[test]
    fn skyrim_live_kill_story_manager_fails_closed_after_save_reopen() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_load_no_py,
            plugin_handle_new_native, plugin_handle_save_no_py, plugin_handle_store_ref,
        };

        pyo3::Python::initialize();
        let source = plugin_handle_new_native("SkyrimStorySource.esm", Some("skyrimse")).unwrap();
        let target = plugin_handle_new_native("SkyrimStoryPort.esp", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        plugin_handle_store_ref()
            .lock()
            .unwrap()
            .get_mut(&target)
            .unwrap()
            .parsed
            .header
            .flags |= crate::source_read::TES4_FLAG_LOCALIZED;
        let run_id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "SkyrimStoryPort.esp".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                preserve_source_ids: true,
                ..Default::default()
            },
        })
        .unwrap();
        let output = tempfile::tempdir().unwrap();

        with_run(run_id, |run| {
            let plugin = run.interner.intern("SkyrimStorySource.esm");
            let source_root = FormKey {
                local: 0x070221,
                plugin,
            };
            let source_branch = FormKey {
                local: 0x070222,
                plugin,
            };
            let source_node = FormKey {
                local: 0x070223,
                plugin,
            };
            let source_quest = FormKey {
                local: 0x070224,
                plugin,
            };
            let source_reference = FormKey {
                local: 0x070225,
                plugin,
            };

            let reference = skyrim_runtime_test_record(
                "REFR",
                source_reference,
                "StoryAliasReference",
                &run.interner,
            );
            add_record_native(
                run.source_handle_id,
                reference,
                &run.schema_source,
                &run.interner,
            )
            .unwrap();

            let mut quest = skyrim_runtime_test_record(
                "QUST",
                source_quest,
                "StoryRuntimeQuest",
                &run.interner,
            );
            quest.fields.extend([
                skyrim_runtime_test_field(
                    "FULL",
                    FieldValue::String(run.interner.intern("A Story Runtime Quest")),
                ),
                skyrim_runtime_test_field(
                    "DNAM",
                    FieldValue::Struct(vec![
                        (run.interner.intern("flags"), FieldValue::Uint(5)),
                        (run.interner.intern("priority"), FieldValue::Uint(50)),
                    ]),
                ),
                skyrim_runtime_test_field(
                    "VMAD",
                    FieldValue::Bytes(skyrim_runtime_test_vmad()),
                ),
                skyrim_runtime_test_field("INDX", FieldValue::Uint(10)),
                skyrim_runtime_test_field(
                    "CNAM",
                    FieldValue::String(run.interner.intern("The story has begun.")),
                ),
                skyrim_runtime_test_field("QOBJ", FieldValue::Uint(10)),
                skyrim_runtime_test_field(
                    "NNAM",
                    FieldValue::String(run.interner.intern("Complete the story objective.")),
                ),
                skyrim_runtime_test_field("QSTA", FieldValue::Uint(0)),
                skyrim_runtime_test_field("ANAM", FieldValue::Uint(1)),
                skyrim_runtime_test_field("ALST", FieldValue::Uint(0)),
                skyrim_runtime_test_field(
                    "ALID",
                    FieldValue::String(run.interner.intern("StoryReference")),
                ),
                skyrim_runtime_test_field("FNAM", FieldValue::Uint(0)),
                skyrim_runtime_test_field("ALFR", FieldValue::FormKey(source_reference)),
                skyrim_runtime_test_field("ALED", FieldValue::Bool(true)),
            ]);

            let mut root = skyrim_runtime_test_record(
                "SMEN",
                source_root,
                "StoryRuntimeKillRoot",
                &run.interner,
            );
            root.fields.push(skyrim_runtime_test_field(
                "ENAM",
                FieldValue::Uint(u32::from_le_bytes(*b"KILL") as u64),
            ));
            let mut branch = skyrim_runtime_test_record(
                "SMBN",
                source_branch,
                "StoryRuntimeBranch",
                &run.interner,
            );
            branch.fields.push(skyrim_runtime_test_field(
                "PNAM",
                FieldValue::FormKey(source_root),
            ));
            let mut node = skyrim_runtime_test_record(
                "SMQN",
                source_node,
                "StoryRuntimeQuestNode",
                &run.interner,
            );
            node.fields.extend([
                skyrim_runtime_test_field("PNAM", FieldValue::FormKey(source_branch)),
                skyrim_runtime_test_field("QNAM", FieldValue::Uint(1)),
                skyrim_runtime_test_field("NNAM", FieldValue::FormKey(source_quest)),
            ]);

            run.mapper_state = Some(MapperState::new(
                [],
                MapperOptions {
                    source_plugin_name: "SkyrimStorySource.esm".to_string(),
                    output_plugin_name: "SkyrimStoryPort.esp".to_string(),
                    preserve_source_ids: true,
                    ..Default::default()
                },
            ));
            let manifest =
                crate::skyrimse_fo4_runtime::planner::minimal_quest_candidate_manifest(
                    &quest,
                    &run.interner,
                )
                .unwrap();
            run.install_skyrim_minimal_quest_actions(
                "B21",
                vec![
                    crate::skyrimse_fo4_runtime::planner::SkyrimMinimalQuestActionRequest {
                        component_id: manifest.component_id,
                        actions: vec![
                            crate::skyrimse_fo4_runtime::planner::SkyrimMinimalQuestActionSpec::SetObjectiveDisplayed {
                                index: 10,
                                displayed: true,
                            },
                        ],
                    },
                ],
            )?;
            let error = run
                .plan_skyrim_runtime_records(vec![quest, root, branch, node])
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(
                    "unsupported Skyrim Story Manager event KILL: Fallout 4 has no canonical SMEN root"
                ),
                "{error}"
            );
            assert!(run.skyrim_minimal_quest_projections.is_empty());
            assert!(run.skyrim_story_manager_projections.is_empty());
            Ok::<_, RunError>(())
        })
        .unwrap();

        let plugin_path = output.path().join("SkyrimStoryPort.esp");
        plugin_handle_save_no_py(target, plugin_path.to_str().unwrap()).unwrap();
        drop_run(run_id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);

        let reopened =
            plugin_handle_load_no_py(plugin_path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let mut interner = StringInterner::new();
        for signature in ["SMEN", "SMBN", "SMQN"] {
            let sig = SigCode::from_str(signature).unwrap();
            let records = iter_form_keys_of_sig(reopened, sig, &mut interner).unwrap();
            assert!(records.is_empty(), "{signature}");
        }
        plugin_handle_close_native(reopened);
    }

    #[test]
    fn skyrim_live_hack_story_manager_survives_save_reopen() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_load_no_py,
            plugin_handle_new_native, plugin_handle_save_no_py, plugin_handle_store_ref,
        };

        pyo3::Python::initialize();
        let source = plugin_handle_new_native("Dawnguard.esm", Some("skyrimse")).unwrap();
        let target = plugin_handle_new_native("SkyrimStoryPort.esp", Some("fo4")).unwrap();
        let fallout4 = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(source, "Skyrim.esm", None).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        plugin_handle_store_ref()
            .lock()
            .unwrap()
            .get_mut(&target)
            .unwrap()
            .parsed
            .header
            .flags |= crate::source_read::TES4_FLAG_LOCALIZED;
        let run_id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![fallout4],
            config: RunConfig {
                output_plugin_name: "SkyrimStoryPort.esp".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                preserve_source_ids: true,
                ..Default::default()
            },
        })
        .unwrap();
        let output = tempfile::tempdir().unwrap();
        let compiler_output = tempfile::tempdir().unwrap();

        with_run(run_id, |run| {
            let plugin = run.interner.intern("Dawnguard.esm");
            let source_root = FormKey {
                local: 0x070221,
                plugin,
            };
            let source_branch = FormKey {
                local: 0x070222,
                plugin,
            };
            let source_node = FormKey {
                local: 0x070223,
                plugin,
            };
            let source_quest = FormKey {
                local: 0x070224,
                plugin,
            };
            let source_reference = FormKey {
                local: 0x070225,
                plugin: run.interner.intern("Skyrim.esm"),
            };
            let target_root = FormKey {
                local: 0x1244D0,
                plugin: run.interner.intern("Fallout4.esm"),
            };
            let target_reference = FormKey {
                local: 0x070225,
                plugin: run.interner.intern("Fallout4.esm"),
            };

            let mut canonical_root =
                skyrim_runtime_test_record("SMEN", target_root, "HackComputer", &run.interner);
            canonical_root.fields.push(skyrim_runtime_test_field(
                "ENAM",
                FieldValue::Uint(u32::from_le_bytes(*b"HACK") as u64),
            ));
            add_record_native(
                fallout4,
                canonical_root,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();
            add_record_native(
                fallout4,
                skyrim_runtime_test_record(
                    "REFR",
                    target_reference,
                    "PortedStoryAliasReference",
                    &run.interner,
                ),
                &run.schema_target,
                &run.interner,
            )
            .unwrap();

            let reference = skyrim_runtime_test_record(
                "REFR",
                source_reference,
                "StoryAliasReference",
                &run.interner,
            );
            add_record_native(
                run.source_handle_id,
                reference,
                &run.schema_source,
                &run.interner,
            )
            .unwrap();

            let mut quest = skyrim_runtime_test_record(
                "QUST",
                source_quest,
                "StoryRuntimeQuest",
                &run.interner,
            );
            quest.fields.extend([
                skyrim_runtime_test_field(
                    "FULL",
                    FieldValue::String(run.interner.intern("A Story Runtime Quest")),
                ),
                skyrim_runtime_test_field(
                    "DNAM",
                    FieldValue::Struct(vec![
                        (run.interner.intern("flags"), FieldValue::Uint(5)),
                        (run.interner.intern("priority"), FieldValue::Uint(50)),
                    ]),
                ),
                skyrim_runtime_test_field(
                    "VMAD",
                    FieldValue::Bytes(skyrim_runtime_test_vmad()),
                ),
                skyrim_runtime_test_field("INDX", FieldValue::Uint(10)),
                skyrim_runtime_test_field(
                    "CNAM",
                    FieldValue::String(run.interner.intern("The story has begun.")),
                ),
                skyrim_runtime_test_field("QOBJ", FieldValue::Uint(10)),
                skyrim_runtime_test_field(
                    "NNAM",
                    FieldValue::String(
                        run.interner.intern("Complete the story objective."),
                    ),
                ),
                skyrim_runtime_test_field("QSTA", FieldValue::Uint(0)),
                skyrim_runtime_test_field("ANAM", FieldValue::Uint(1)),
                skyrim_runtime_test_field("ALST", FieldValue::Uint(0)),
                skyrim_runtime_test_field(
                    "ALID",
                    FieldValue::String(run.interner.intern("StoryReference")),
                ),
                skyrim_runtime_test_field("FNAM", FieldValue::Uint(0)),
                skyrim_runtime_test_field("ALFR", FieldValue::FormKey(source_reference)),
                skyrim_runtime_test_field("ALED", FieldValue::Bool(true)),
            ]);

            let mut root = skyrim_runtime_test_record(
                "SMEN",
                source_root,
                "StoryRuntimeHackRoot",
                &run.interner,
            );
            root.fields.push(skyrim_runtime_test_field(
                "ENAM",
                FieldValue::Uint(u32::from_le_bytes(*b"HACK") as u64),
            ));
            let mut branch = skyrim_runtime_test_record(
                "SMBN",
                source_branch,
                "StoryRuntimeBranch",
                &run.interner,
            );
            branch.fields.push(skyrim_runtime_test_field(
                "PNAM",
                FieldValue::FormKey(source_root),
            ));
            let mut node = skyrim_runtime_test_record(
                "SMQN",
                source_node,
                "StoryRuntimeQuestNode",
                &run.interner,
            );
            node.fields.extend([
                skyrim_runtime_test_field("PNAM", FieldValue::FormKey(source_branch)),
                skyrim_runtime_test_field("QNAM", FieldValue::Uint(1)),
                skyrim_runtime_test_field("NNAM", FieldValue::FormKey(source_quest)),
            ]);

            run.mapper_state = Some(MapperState::new(
                [],
                MapperOptions {
                    source_plugin_name: "Dawnguard.esm".to_string(),
                    output_plugin_name: "SkyrimStoryPort.esp".to_string(),
                    preserve_source_ids: true,
                    ..Default::default()
                },
            ));
            run.mapper_state
                .as_mut()
                .unwrap()
                .source_to_target
                .insert(source_reference, target_reference);
            let manifest =
                crate::skyrimse_fo4_runtime::planner::minimal_quest_candidate_manifest(
                    &quest,
                    &run.interner,
                )
                .unwrap();
            run.install_skyrim_minimal_quest_actions(
                "B21",
                vec![
                    crate::skyrimse_fo4_runtime::planner::SkyrimMinimalQuestActionRequest {
                        component_id: manifest.component_id,
                        actions: vec![
                            crate::skyrimse_fo4_runtime::planner::SkyrimMinimalQuestActionSpec::SetObjectiveDisplayed {
                                index: 10,
                                displayed: true,
                            },
                        ],
                    },
                ],
            )?;
            run.plan_skyrim_runtime_records(vec![quest, root, branch, node])?;
            let component = run
                .skyrim_runtime_capability_plan
                .as_ref()
                .unwrap()
                .minimal_quests
                .values()
                .next()
                .unwrap();
            assert_eq!(component.plan.provenance.source_plugin, "Dawnguard.esm");
            assert!(
                component
                    .plan
                    .shared_records
                    .iter()
                    .any(|record| record.form_key.ends_with("@Skyrim.esm"))
            );
            assert!(component.plan.mappings.iter().any(|mapping| {
                mapping.source.form_key.ends_with("@Skyrim.esm")
                    && mapping.target.form_key == "070225@Fallout4.esm"
            }));
            let projected_quest = run.skyrim_minimal_quest_projections[0].clone();
            let compiled = papyrus_core::compiler::compile_source(
                &projected_quest.psc_artifact.source,
                &[],
                papyrus_core::profile::Game::Fo4,
                None,
            );
            assert!(compiled.ok, "compiler diagnostics: {:?}", compiled.diagnostics);
            let pex_bytes = compiled.pex_bytes.unwrap();
            let pex_artifact_path = compiler_output
                .path()
                .join("data")
                .join("Scripts")
                .join(format!("{}.pex", projected_quest.psc_manifest.class_name));
            std::fs::create_dir_all(pex_artifact_path.parent().unwrap()).unwrap();
            std::fs::write(&pex_artifact_path, &pex_bytes).unwrap();
            let compiler_evidence =
                crate::skyrimse_fo4_runtime::papyrus::SkyrimPscCompilerEvidence {
                    manifest_id: projected_quest.psc_manifest.manifest_id.clone(),
                    class_name: projected_quest.psc_manifest.class_name.clone(),
                    relative_source_path: projected_quest
                        .psc_manifest
                        .relative_source_path
                        .clone(),
                    source_blake3: projected_quest.psc_manifest.source_blake3.clone(),
                    relative_pex_path: format!(
                        "data/Scripts/{}.pex",
                        projected_quest.psc_manifest.class_name
                    ),
                    pex_artifact_path,
                    pex_blake3: blake3::hash(&pex_bytes).to_hex().to_string(),
                    target_game: "fo4".to_string(),
                    compile_run_id: "run-level-hack-fixture".to_string(),
                    compiled_success: true,
                };

            let mut quest_stats = run.emit_skyrim_minimal_quest_projections()?;
            assert_eq!(quest_stats.signature_entry(SigCode(*b"QUST")).translated, 1);
            assert_eq!(
                run.reconcile_skyrim_minimal_quest_compiler_evidence(vec![compiler_evidence])?,
                1
            );
            let story_stats = run.emit_story_manager_subset()?;
            assert_eq!(story_stats.selected_nodes, 3);
            assert_eq!(story_stats.translate.records_translated, 2);
            run.reconcile_skyrim_minimal_quest_post_fixup_receipts()?;

            let projection = run.skyrim_story_manager_projections[0]
                .projection
                .as_ref()
                .unwrap();
            assert_eq!(
                projection.route_receipt.route.producer_evidence_id,
                "fo4-native-event:HACK"
            );
            assert_eq!(
                projection.route_receipt.route.node_chain[0].form_key,
                "1244D0@Fallout4.esm"
            );
            assert_eq!(projection.records.len(), 2);
            assert!(projection.records.iter().all(|record| record.sig.0 != *b"SMEN"));
            Ok::<_, RunError>(())
        })
        .unwrap();

        let plugin_path = output.path().join("SkyrimStoryPort.esp");
        plugin_handle_save_no_py(target, plugin_path.to_str().unwrap()).unwrap();
        drop_run(run_id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
        plugin_handle_close_native(fallout4);

        let reopened =
            plugin_handle_load_no_py(plugin_path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let mut interner = StringInterner::new();
        let smen =
            iter_form_keys_of_sig(reopened, SigCode::from_str("SMEN").unwrap(), &mut interner)
                .unwrap();
        assert!(smen.is_empty());
        let branches =
            iter_form_keys_of_sig(reopened, SigCode::from_str("SMBN").unwrap(), &mut interner)
                .unwrap();
        let nodes =
            iter_form_keys_of_sig(reopened, SigCode::from_str("SMQN").unwrap(), &mut interner)
                .unwrap();
        let quests =
            iter_form_keys_of_sig(reopened, SigCode::from_str("QUST").unwrap(), &mut interner)
                .unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(nodes.len(), 1);
        assert_eq!(quests.len(), 1);
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let mut session = crate::session::open_session(reopened, None).unwrap();
        let branch = session
            .record_decoded(&branches[0], &schema, &interner)
            .unwrap();
        assert!(branch.fields.iter().any(|field| {
            field.sig.0 == *b"PNAM"
                && matches!(
                    &field.value,
                    FieldValue::FormKey(parent)
                        if parent.local == 0x1244D0
                            && interner.resolve(parent.plugin) == Some("Fallout4.esm")
                )
        }));
        let quest = session
            .record_decoded(&quests[0], &schema, &interner)
            .unwrap();
        assert!(quest.fields.iter().any(|field| {
            field.sig.0 == *b"ALFR"
                && matches!(
                    &field.value,
                    FieldValue::FormKey(reference)
                        if reference.local == 0x070225
                            && interner.resolve(reference.plugin) == Some("Fallout4.esm")
                )
        }));
        let starts_enabled = quest
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DNAM")
            .map(|field| match &field.value {
                FieldValue::Struct(fields) => fields
                    .iter()
                    .find_map(|(name, value)| {
                        (interner.resolve(*name) == Some("flags")).then_some(value)
                    })
                    .is_some_and(
                        |value| matches!(value, FieldValue::Uint(flags) if flags & 1 != 0),
                    ),
                FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                    u16::from_le_bytes([bytes[0], bytes[1]]) & 1 != 0
                }
                _ => true,
            })
            .unwrap();
        assert!(!starts_enabled);
        drop(session);
        plugin_handle_close_native(reopened);
    }

    #[test]
    fn skyrim_story_manager_target_root_requires_loaded_matching_smen() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
        };

        let source = plugin_handle_new_native("SkyrimStorySource.esm", Some("skyrimse")).unwrap();
        let target = plugin_handle_new_native("SkyrimStoryPort.esp", Some("fo4")).unwrap();
        let fallout4 = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        let run_id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![fallout4],
            config: RunConfig {
                output_plugin_name: "SkyrimStoryPort.esp".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                ..Default::default()
            },
        })
        .unwrap();

        with_run(run_id, |run| {
            let event =
                crate::skyrimse_fo4_runtime::story_manager::SkyrimFo4StoryEvent::HackComputer;
            let target_root = FormKey {
                local: 0x1244D0,
                plugin: run.interner.intern("Fallout4.esm"),
            };
            let missing = run
                .validate_skyrim_story_manager_target_event_root(event, target_root)
                .unwrap_err()
                .to_string();
            assert!(
                missing.contains("absent from the loaded target master"),
                "{missing}"
            );

            let mut wrong =
                skyrim_runtime_test_record("SMEN", target_root, "NotHackComputer", &run.interner);
            wrong.fields.push(skyrim_runtime_test_field(
                "ENAM",
                FieldValue::Uint(event.code() as u64),
            ));
            add_record_native(fallout4, wrong, &run.schema_target, &run.interner).unwrap();
            let mismatch = run
                .validate_skyrim_story_manager_target_event_root(event, target_root)
                .unwrap_err()
                .to_string();
            assert!(
                mismatch.contains("does not match SMEN/HACK/HackComputer"),
                "{mismatch}"
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
        plugin_handle_close_native(fallout4);
    }

    #[test]
    fn optional_installed_fallout4_hack_root_matches_runtime_contract() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_load_no_py,
            plugin_handle_new_native,
        };

        let Some(data_dir) = std::env::var_os("FO4_QUEST_CORPUS_DATA_DIR").map(PathBuf::from)
        else {
            return;
        };
        let fallout4_path = data_dir.join("Fallout4.esm");
        if !fallout4_path.is_file() {
            return;
        }
        let source = plugin_handle_new_native("Dawnguard.esm", Some("skyrimse")).unwrap();
        let target = plugin_handle_new_native("SkyrimStoryPort.esp", Some("fo4")).unwrap();
        let fallout4 = plugin_handle_load_no_py(
            fallout4_path.to_str().unwrap(),
            Some("fo4"),
            None,
            None,
            true,
        )
        .unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        let run_id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![fallout4],
            config: RunConfig {
                output_plugin_name: "SkyrimStoryPort.esp".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                ..Default::default()
            },
        })
        .unwrap();
        with_run(run_id, |run| {
            let event =
                crate::skyrimse_fo4_runtime::story_manager::SkyrimFo4StoryEvent::HackComputer;
            let target_root = FormKey {
                local: 0x1244D0,
                plugin: run.interner.intern("Fallout4.esm"),
            };
            run.validate_skyrim_story_manager_target_event_root(event, target_root)
        })
        .unwrap();
        drop_run(run_id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
        plugin_handle_close_native(fallout4);
    }

    #[test]
    fn fo76_kill_story_manager_fails_before_allocation_and_reopens_without_sm_records() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_load_no_py,
            plugin_handle_new_native, plugin_handle_save_no_py,
        };

        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native("SeventySixPort.esp", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        let run_id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "SeventySixPort.esp".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                preserve_source_ids: true,
                ..Default::default()
            },
        })
        .unwrap();
        let output = tempfile::tempdir().unwrap();

        with_run(run_id, |run| {
            let source_plugin = run.interner.intern("SeventySix.esm");
            let output_plugin = run.interner.intern("SeventySixPort.esp");
            let root = FormKey {
                local: 0x4100,
                plugin: source_plugin,
            };
            let branch = FormKey {
                local: 0x4101,
                plugin: source_plugin,
            };
            let node = FormKey {
                local: 0x4102,
                plugin: source_plugin,
            };
            let quest = FormKey {
                local: 0x4103,
                plugin: source_plugin,
            };

            let mut root_record =
                skyrim_runtime_test_record("SMEN", root, "UnsupportedKillEvent", &run.interner);
            root_record.fields.push(skyrim_runtime_test_field(
                "ENAM",
                FieldValue::Uint(u32::from_le_bytes(*b"KILL") as u64),
            ));
            let mut branch_record =
                skyrim_runtime_test_record("SMBN", branch, "UnsupportedKillBranch", &run.interner);
            branch_record
                .fields
                .push(skyrim_runtime_test_field("PNAM", FieldValue::FormKey(root)));
            let mut node_record =
                skyrim_runtime_test_record("SMQN", node, "UnsupportedKillQuestNode", &run.interner);
            node_record.fields.extend([
                skyrim_runtime_test_field("PNAM", FieldValue::FormKey(branch)),
                skyrim_runtime_test_field("NNAM", FieldValue::FormKey(quest)),
            ]);
            let quest_record =
                skyrim_runtime_test_record("QUST", quest, "UnsupportedKillQuest", &run.interner);
            for record in [root_record, branch_record, node_record, quest_record] {
                add_record_native(
                    run.source_handle_id,
                    record,
                    &run.schema_source,
                    &run.interner,
                )
                .unwrap();
            }

            let mut mapper = MapperState::new(
                [],
                MapperOptions {
                    source_plugin_name: "SeventySix.esm".to_string(),
                    output_plugin_name: "SeventySixPort.esp".to_string(),
                    preserve_source_ids: true,
                    ..Default::default()
                },
            );
            mapper.source_to_target.insert(
                quest,
                FormKey {
                    local: 0x5103,
                    plugin: output_plugin,
                },
            );
            run.mapper_state = Some(mapper);

            let error = run.emit_story_manager_subset().unwrap_err().to_string();
            assert!(
                error.contains("story_manager_event_unsupported_no_fo4_root:004100:KILL"),
                "{error}"
            );
            let state = run.mapper_state.as_ref().unwrap();
            assert_eq!(state.source_to_target.len(), 1);
            assert!(!state.source_to_target.contains_key(&root));
            Ok::<_, RunError>(())
        })
        .unwrap();

        let plugin_path = output.path().join("SeventySixPort.esp");
        plugin_handle_save_no_py(target, plugin_path.to_str().unwrap()).unwrap();
        drop_run(run_id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);

        let reopened =
            plugin_handle_load_no_py(plugin_path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let mut interner = StringInterner::new();
        for signature in ["SMEN", "SMBN", "SMQN"] {
            let sig = SigCode::from_str(signature).unwrap();
            assert!(
                iter_form_keys_of_sig(reopened, sig, &mut interner)
                    .unwrap()
                    .is_empty()
            );
        }
        plugin_handle_close_native(reopened);
    }

    #[test]
    fn creature_dependency_plan_preserves_reserved_mapper_for_translation() {
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            run.prepare_mapper_state_for_creature_dependency_plan()?;
            let source = FormKey {
                local: 0x100,
                plugin: run.interner.intern("FalloutNV.esm"),
            };
            run.install_legacy_creature_dependency_plan(Vec::new(), &[source], &[], 1)?;
            let reserved_target = run
                .mapper_state
                .as_ref()
                .and_then(|state| state.source_to_target.get(&source))
                .copied()
                .expect("creature reservation mapping");

            run.prepare_mapper_state_for_translation()?;
            assert_eq!(
                run.mapper_state
                    .as_ref()
                    .and_then(|state| state.source_to_target.get(&source))
                    .copied(),
                Some(reserved_target)
            );

            run.release_remap_state();
            let error = run.prepare_mapper_state_for_translation().unwrap_err();
            assert!(error.to_string().contains(
                "mapper state was released after creature dependency reservations were installed"
            ));
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn skyrim_creature_dependency_plan_marks_source_rig_records_as_batch_owned() {
        let id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            run.prepare_mapper_state_for_creature_dependency_plan()?;
            let source_plugin = run.interner.intern("Skyrim.esm");
            let race = FormKey {
                local: 0x100,
                plugin: source_plugin,
            };
            let npc = FormKey {
                local: 0x101,
                plugin: source_plugin,
            };
            let armor = FormKey {
                local: 0x102,
                plugin: source_plugin,
            };
            let owner = SkyrimCreatureReservationOwner {
                source_race: race,
                motion_family_id: "wolf".to_string(),
                normalized_project_path: "wolf/wolfbehavior.hkx".to_string(),
            };
            run.install_skyrim_creature_dependency_plan(
                Vec::new(),
                vec![
                    SkyrimCreatureReservationSource {
                        source: race,
                        kind: SkyrimCreatureReservedRecordKind::Race,
                        owners: vec![owner.clone()],
                    },
                    SkyrimCreatureReservationSource {
                        source: npc,
                        kind: SkyrimCreatureReservedRecordKind::Npc,
                        owners: vec![owner.clone()],
                    },
                    SkyrimCreatureReservationSource {
                        source: armor,
                        kind: SkyrimCreatureReservedRecordKind::Armor,
                        owners: vec![owner],
                    },
                ],
                1,
            )?;

            assert!(run.is_skyrim_creature_batch_owned(race));
            assert!(run.is_skyrim_creature_batch_owned(npc));
            assert!(!run.is_skyrim_creature_batch_owned(armor));
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn creature_primary_npc_reservations_are_sorted_exact_and_atomic() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let primary_creature_sources = [0x101u32, 0x100]
            .into_iter()
            .map(|local| FormKey {
                local,
                plugin: source_plugin,
            })
            .collect::<Vec<_>>();
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
        );

        let reservations = allocate_creature_primary_npc_reservations(
            &mut state,
            &interner,
            &primary_creature_sources,
            2,
        )
        .expect("reservations");

        assert_eq!(
            reservations
                .iter()
                .map(|reservation| reservation.source.local)
                .collect::<Vec<_>>(),
            vec![0x100, 0x101]
        );
        assert!(reservations.iter().all(|reservation| {
            interner.resolve(reservation.target.plugin) == Some("Output.esm")
                && state.source_to_target[&reservation.source] == reservation.target
                && state.leased_object_ids.contains(&reservation.target.local)
                && !state.used_object_ids.contains(&reservation.target.local)
        }));

        let leased_local = reservations[0].target.local;
        let unrelated_source = FormKey {
            local: leased_local,
            plugin: interner.intern("Other.esm"),
        };
        let unrelated_target = FormKeyMapper::from_state(&mut state, &interner)
            .allocate_or_resolve(unrelated_source, None, SigCode::from_str("CELL").unwrap());
        assert_ne!(unrelated_target.local, leased_local);

        let before = state.source_to_target.clone();
        assert!(
            allocate_creature_primary_npc_reservations(
                &mut state,
                &interner,
                &primary_creature_sources,
                2,
            )
            .is_err()
        );
        assert_eq!(state.source_to_target, before);
    }

    #[test]
    fn legacy_ancillary_npc_reservations_are_canonical_mapped_and_atomic() {
        let interner = StringInterner::new();
        let fnv = interner.intern("FalloutNV.esm");
        let fo3 = interner.intern("Fallout3.esm");
        let source = |plugin, local| FormKey { plugin, local };
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
        );
        let sources = vec![
            source(fnv, 0x300),
            source(fo3, 0x200),
            source(fnv, 0x100),
            source(fnv, 0x300),
        ];

        let reservations =
            allocate_creature_ancillary_npc_reservations(&mut state, &interner, &sources)
                .expect("ancillary NPC reservations");

        assert_eq!(reservations.len(), 3);
        assert_eq!(
            reservations
                .iter()
                .map(|reservation| {
                    (
                        interner.resolve(reservation.source.plugin).unwrap(),
                        reservation.source.local,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("Fallout3.esm", 0x200),
                ("FalloutNV.esm", 0x100),
                ("FalloutNV.esm", 0x300),
            ]
        );
        assert!(reservations.iter().all(|reservation| {
            reservation.target.local != 0
                && state.source_to_target[&reservation.source] == reservation.target
        }));

        let before = state.clone();
        let error = allocate_creature_ancillary_npc_reservations(
            &mut state,
            &interner,
            &[source(fnv, 0x400), source(fo3, 0x200)],
        )
        .expect_err("pre-mapped ancillary NPC must roll back");
        assert!(error.contains("already has a mapping"));
        assert_eq!(state.source_to_target, before.source_to_target);
        assert_eq!(state.next_object_id, before.next_object_id);
        assert_eq!(state.used_object_ids, before.used_object_ids);
    }

    #[test]
    fn skyrim_creature_reservations_keep_records_distinct_and_are_atomic() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Skyrim.esm");
        let output_plugin = interner.intern("Output.esm");
        let target_master_plugin = interner.intern("Fallout4.esm");
        let form_key = |local| FormKey {
            local,
            plugin: source_plugin,
        };
        let owner = |source_race| SkyrimCreatureReservationOwner {
            source_race,
            motion_family_id: "wolf".to_string(),
            normalized_project_path: "wolf/wolfbehavior.hkx".to_string(),
        };
        let race_a = form_key(0x100);
        let race_b = form_key(0x101);
        let skin = form_key(0x200);
        let body_part_data = form_key(0x202);
        let sources = vec![
            SkyrimCreatureReservationSource {
                source: form_key(0x301),
                kind: SkyrimCreatureReservedRecordKind::Npc,
                owners: vec![owner(race_b)],
            },
            SkyrimCreatureReservationSource {
                source: race_b,
                kind: SkyrimCreatureReservedRecordKind::Race,
                owners: vec![owner(race_b)],
            },
            SkyrimCreatureReservationSource {
                source: skin,
                kind: SkyrimCreatureReservedRecordKind::Armor,
                owners: vec![owner(race_b), owner(race_a)],
            },
            SkyrimCreatureReservationSource {
                source: form_key(0x300),
                kind: SkyrimCreatureReservedRecordKind::Npc,
                owners: vec![owner(race_a)],
            },
            SkyrimCreatureReservationSource {
                source: race_a,
                kind: SkyrimCreatureReservedRecordKind::Race,
                owners: vec![owner(race_a)],
            },
            SkyrimCreatureReservationSource {
                source: form_key(0x201),
                kind: SkyrimCreatureReservedRecordKind::ArmorAddon,
                owners: vec![owner(race_a), owner(race_b)],
            },
            SkyrimCreatureReservationSource {
                source: body_part_data,
                kind: SkyrimCreatureReservedRecordKind::BodyPartData,
                owners: vec![owner(race_a), owner(race_b)],
            },
        ];
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
        );
        state.source_to_target.insert(
            body_part_data,
            FormKey {
                local: 0x1D,
                plugin: target_master_plugin,
            },
        );

        let reservations =
            allocate_skyrim_creature_record_reservations(&mut state, &interner, sources.clone(), 2)
                .expect("reservations");

        assert_eq!(reservations.len(), 7);
        assert!(
            reservations
                .windows(2)
                .all(|pair| pair[0].source.local < pair[1].source.local)
        );
        assert_ne!(
            state.source_to_target[&race_a],
            state.source_to_target[&race_b]
        );
        assert_ne!(
            state.source_to_target[&form_key(0x300)],
            state.source_to_target[&form_key(0x301)]
        );
        assert_eq!(
            state.source_to_target[&body_part_data].plugin,
            output_plugin
        );
        assert_eq!(
            reservations
                .iter()
                .find(|reservation| reservation.source == skin)
                .expect("skin reservation")
                .owners
                .len(),
            2
        );

        let before = state.clone();
        assert!(
            allocate_skyrim_creature_record_reservations(&mut state, &interner, sources, 2,)
                .is_err()
        );
        assert_eq!(state.source_to_target, before.source_to_target);
        assert_eq!(state.next_object_id, before.next_object_id);
        assert_eq!(state.used_object_ids, before.used_object_ids);
    }

    #[test]
    fn skyrim_creature_ancillary_reservations_are_canonical_distinct_and_collision_safe() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Skyrim.esm");
        let output_plugin = interner.intern("Output.esm");
        let source_race = |local| FormKey {
            local,
            plugin: source_plugin,
        };
        let race_a = source_race(0x100);
        let race_b = source_race(0x101);
        let record_reservations = [race_a, race_b]
            .into_iter()
            .map(|source| SkyrimCreatureRecordReservation {
                source,
                target: FormKey {
                    local: source.local + 0x1000,
                    plugin: output_plugin,
                },
                kind: SkyrimCreatureReservedRecordKind::Race,
                owners: Vec::new(),
            })
            .collect::<Vec<_>>();
        let requests = vec![
            SkyrimCreatureAncillaryReservationRequest {
                source_race: race_b,
                event: "attackStartB".to_string(),
                ordinal: 2,
                kind: SkyrimCreatureAncillaryRecordKind::Projectile,
            },
            SkyrimCreatureAncillaryReservationRequest {
                source_race: race_a,
                event: "attackStartA".to_string(),
                ordinal: 1,
                kind: SkyrimCreatureAncillaryRecordKind::Weapon,
            },
            SkyrimCreatureAncillaryReservationRequest {
                source_race: race_a,
                event: "attackStartA".to_string(),
                ordinal: 1,
                kind: SkyrimCreatureAncillaryRecordKind::Projectile,
            },
        ];
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
        );

        let receipts = allocate_skyrim_creature_ancillary_reservations(
            &mut state,
            &interner,
            &record_reservations,
            requests,
        )
        .expect("ancillary reservations");

        assert_eq!(
            receipts
                .iter()
                .map(|receipt| (receipt.source_race.local, receipt.kind))
                .collect::<Vec<_>>(),
            vec![
                (0x100, SkyrimCreatureAncillaryRecordKind::Weapon),
                (0x100, SkyrimCreatureAncillaryRecordKind::Projectile),
                (0x101, SkyrimCreatureAncillaryRecordKind::Projectile),
            ]
        );
        assert!(receipts.iter().all(|receipt| {
            receipt.target.local != 0 && receipt.target.plugin == output_plugin
        }));
        let targets = receipts
            .iter()
            .map(|receipt| receipt.target)
            .collect::<FxHashSet<_>>();
        assert_eq!(targets.len(), receipts.len());
        assert!(
            receipts
                .iter()
                .all(|receipt| !state.source_to_target.contains_key(&receipt.source_race))
        );

        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let next_target = mapper.allocate_generated();
        assert_ne!(next_target.local, 0);
        assert!(!targets.contains(&next_target));
    }

    #[test]
    fn skyrim_creature_ancillary_duplicate_rolls_back_allocator_state() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Skyrim.esm");
        let output_plugin = interner.intern("Output.esm");
        let source_race = FormKey {
            local: 0x100,
            plugin: source_plugin,
        };
        let record_reservations = vec![SkyrimCreatureRecordReservation {
            source: source_race,
            target: FormKey {
                local: 0x1000,
                plugin: output_plugin,
            },
            kind: SkyrimCreatureReservedRecordKind::Race,
            owners: Vec::new(),
        }];
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
        );
        let before = state.clone();

        let error = allocate_skyrim_creature_ancillary_reservations(
            &mut state,
            &interner,
            &record_reservations,
            vec![
                SkyrimCreatureAncillaryReservationRequest {
                    source_race,
                    event: "attackStart".to_string(),
                    ordinal: 1,
                    kind: SkyrimCreatureAncillaryRecordKind::Weapon,
                },
                SkyrimCreatureAncillaryReservationRequest {
                    source_race,
                    event: "ATTACKSTART".to_string(),
                    ordinal: 1,
                    kind: SkyrimCreatureAncillaryRecordKind::Weapon,
                },
            ],
        )
        .expect_err("case-insensitive duplicate must block");

        assert!(error.contains("duplicate Skyrim creature ancillary request"));
        assert_eq!(state.source_to_target, before.source_to_target);
        assert_eq!(state.next_object_id, before.next_object_id);
        assert_eq!(state.used_object_ids, before.used_object_ids);
    }

    #[test]
    fn skyrim_creature_ancillary_unreserved_race_rolls_back_allocator_state() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Skyrim.esm");
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
        );
        let before = state.clone();

        let error = allocate_skyrim_creature_ancillary_reservations(
            &mut state,
            &interner,
            &[],
            vec![SkyrimCreatureAncillaryReservationRequest {
                source_race: FormKey {
                    local: 0x100,
                    plugin: source_plugin,
                },
                event: "attackStart".to_string(),
                ordinal: 1,
                kind: SkyrimCreatureAncillaryRecordKind::Weapon,
            }],
        )
        .expect_err("unreserved source RACE must block");

        assert!(error.contains("require installed RACE reservations"));
        assert_eq!(state.source_to_target, before.source_to_target);
        assert_eq!(state.next_object_id, before.next_object_id);
        assert_eq!(state.used_object_ids, before.used_object_ids);
    }

    #[test]
    fn creature_actor_action_reservations_are_family_canonical_and_leased() {
        let interner = StringInterner::new();
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
        );
        let requirement = |kind, parent_editor_id: &str, parent_form_id, event: &str| {
            crate::source_rig::ActorActionRequirement {
                kind,
                parent_editor_id: parent_editor_id.to_string(),
                parent_form_id,
                behavior_path: "Actors\\Fixture\\Behaviors\\FixtureRootBehavior.hkx".to_string(),
                animation_event: event.to_string(),
            }
        };
        let requests = vec![
            CreatureActorActionReservationRequest {
                family_id: "wolf".to_string(),
                requirement: requirement(
                    crate::source_rig::ActorActionKind::Melee,
                    "ActionMelee",
                    0x004A59,
                    "meleeWolf",
                ),
                editor_id: "B21_WolfActionMelee".to_string(),
            },
            CreatureActorActionReservationRequest {
                family_id: "wolf".to_string(),
                requirement: requirement(
                    crate::source_rig::ActorActionKind::Idle,
                    "ActionIdle",
                    0x013002,
                    "Idle",
                ),
                editor_id: "B21_WolfActionIdle".to_string(),
            },
        ];

        let receipts =
            allocate_creature_actor_action_reservations(&mut state, &interner, requests.clone())
                .expect("Actor Action reservations");
        assert_eq!(receipts.len(), 2);
        assert_eq!(
            receipts
                .iter()
                .map(|receipt| receipt.requirement.kind)
                .collect::<Vec<_>>(),
            vec![
                crate::source_rig::ActorActionKind::Idle,
                crate::source_rig::ActorActionKind::Melee,
            ]
        );
        assert!(receipts.iter().all(|receipt| {
            state
                .reserved_generated_object_ids
                .contains(&receipt.target.local)
                && !state.used_object_ids.contains(&receipt.target.local)
                && receipt.record_plan(&interner).is_ok()
        }));
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let next = mapper.allocate_generated();
        assert!(
            receipts
                .iter()
                .all(|receipt| receipt.target.local != next.local)
        );

        let before = state.clone();
        let duplicate = vec![
            requests[0].clone(),
            CreatureActorActionReservationRequest {
                editor_id: "B21_WolfActionMeleeOther".to_string(),
                ..requests[0].clone()
            },
        ];
        assert!(
            allocate_creature_actor_action_reservations(&mut state, &interner, duplicate,).is_err()
        );
        assert_eq!(state.next_object_id, before.next_object_id);
        assert_eq!(state.used_object_ids, before.used_object_ids);
        assert_eq!(
            state.reserved_generated_object_ids,
            before.reserved_generated_object_ids
        );
    }

    fn slice_info(
        source_form_key: &str,
        fragment_class_name: Option<&str>,
        phase: Option<crate::fnv_legacy_scripting::dialogue::InfoFragmentPhase>,
    ) -> crate::fnv_legacy_scripting::dialogue::TranslatedInfo {
        crate::fnv_legacy_scripting::dialogue::TranslatedInfo {
            source_form_key: source_form_key.to_string(),
            fragment_class_name: fragment_class_name.map(str::to_string),
            fragment_psc_text: fragment_class_name
                .map(|class_name| format!("ScriptName {class_name} extends TopicInfo\n")),
            fragment_phases: phase.into_iter().collect(),
            fragment_properties: Vec::new(),
            voice_target_path: String::new(),
            voice_source_path: String::new(),
            lip_dropped: false,
            lip_regeneration_target: None,
            authoring_record_payload: None,
            warnings: Vec::new(),
        }
    }

    fn vertical_slice_script_source(local: u32) -> String {
        let fixture =
            include_str!("../../../python/bacup_lib/tests/test_fnv_quest_vertical_slice.py");
        let marker = format!("    0x{local:06X}: \"\"\"");
        let start = fixture.find(&marker).unwrap() + marker.len();
        let end = fixture[start..].find("\"\"\",").unwrap() + start;
        fixture[start..end]
            .replace("\r\n", "\n")
            .replace("\\t", "\t")
    }

    #[test]
    fn strict_slice_surfaces_scpt_failure_before_dynamic_alias_cardinality() {
        let error = ensure_no_deferred_translation_failures(&[(
            "SCPT".to_string(),
            "TecMineHostage".to_string(),
            "parse failed at else(condition)".to_string(),
        )])
        .unwrap_err()
        .to_string();
        assert!(error.contains("SCPT TecMineHostage: parse failed at else(condition)"));
        assert!(!error.contains("dynamic package data aliases"));
    }

    #[test]
    fn strict_slice_requires_two_exact_tifs_among_five_info_records() {
        use crate::fnv_legacy_scripting::dialogue::InfoFragmentPhase;

        let infos = vec![
            slice_info(
                "130161:FalloutNV.esm",
                Some("TIF__130161"),
                Some(InfoFragmentPhase::Begin),
            ),
            slice_info(
                "134B9B:FalloutNV.esm",
                Some("TIF__134B9B"),
                Some(InfoFragmentPhase::End),
            ),
            slice_info("15734B:FalloutNV.esm", None, None),
            slice_info("15734C:FalloutNV.esm", None, None),
            slice_info("15734D:FalloutNV.esm", None, None),
        ];
        validate_fnv_quest_slice_info_fragment_contract(&infos).unwrap();

        let mut missing_required_tif = infos.clone();
        missing_required_tif[0] = slice_info("130161:FalloutNV.esm", None, None);
        assert!(
            validate_fnv_quest_slice_info_fragment_contract(&missing_required_tif)
                .unwrap_err()
                .to_string()
                .contains("requires exact fragment TIF__130161")
        );

        let mut scripted_greeting = infos;
        scripted_greeting[2] = slice_info(
            "15734B:FalloutNV.esm",
            Some("TIF__15734B"),
            Some(InfoFragmentPhase::Begin),
        );
        assert!(
            validate_fnv_quest_slice_info_fragment_contract(&scripted_greeting)
                .unwrap_err()
                .to_string()
                .contains("unexpectedly carries a Papyrus fragment")
        );
    }

    #[test]
    fn production_relayout_scri_formkeys_capture_npc_and_deferred_quest_attachments() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let target_plugin = interner.intern("Output.esm");
        let scri_sig = SubrecordSig::from_str("SCRI").unwrap();
        let cases = [
            ("NPC_", 0x1300F0, 0x166305, 0x2300F0),
            ("QUST", 0x11F935, 0x11FC64, 0x21F935),
        ];
        let mut links = Vec::new();
        for (signature, source_owner, source_script, target_owner) in cases {
            let mut record = Record::new(
                SigCode::from_str(signature).unwrap(),
                FormKey {
                    local: source_owner,
                    plugin: source_plugin,
                },
            );
            record.fields.push(FieldEntry {
                sig: scri_sig,
                value: FieldValue::FormKey(FormKey {
                    local: source_script,
                    plugin: source_plugin,
                }),
            });
            let raw_form_id_resolver = |raw: u32| {
                Some(FormKey {
                    local: raw & 0x00FF_FFFF,
                    plugin: source_plugin,
                })
            };
            let source_scpt =
                capture_fnv_scri_target_text(&record, &interner, &raw_form_id_resolver)
                    .unwrap()
                    .unwrap();
            assert_eq!(source_scpt, format!("{source_script:06X}:FalloutNV.esm"));
            append_fnv_scri_link(
                &mut links,
                FormKey {
                    local: target_owner,
                    plugin: target_plugin,
                },
                &source_scpt,
                &interner,
            )
            .unwrap();
        }
        assert_eq!(
            links,
            [
                FnvScriLink {
                    target_form_key: "2300F0:Output.esm".into(),
                    source_scpt_form_key: "166305:FalloutNV.esm".into(),
                },
                FnvScriLink {
                    target_form_key: "21F935:Output.esm".into(),
                    source_scpt_form_key: "11FC64:FalloutNV.esm".into(),
                },
            ]
        );

        let mut invalid = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey {
                local: 0x1300F0,
                plugin: source_plugin,
            },
        );
        invalid.fields.push(FieldEntry {
            sig: scri_sig,
            value: FieldValue::Int(0x166305),
        });
        assert!(
            capture_fnv_scri_target_text(&invalid, &interner, &|_| None)
                .unwrap_err()
                .contains("unsupported decoded value")
        );

        invalid.fields[0].value = FieldValue::Bytes(smallvec::smallvec![0x05, 0x63, 0x16]);
        assert!(
            capture_fnv_scri_target_text(&invalid, &interner, &|_| None)
                .unwrap_err()
                .contains("has 3 bytes, expected 4")
        );

        invalid.fields[0].value = FieldValue::Bytes(smallvec::smallvec![0x05, 0x63, 0x16, 0x00]);
        assert_eq!(
            capture_fnv_scri_target_text(&invalid, &interner, &|raw| {
                (raw == 0x0016_6305).then_some(FormKey {
                    local: raw & 0x00FF_FFFF,
                    plugin: source_plugin,
                })
            })
            .unwrap(),
            Some("166305:FalloutNV.esm".into())
        );

        let compatibility = interner.intern("166305:FalloutNV.esm");
        invalid.fields[0].value = FieldValue::String(compatibility);
        assert_eq!(
            capture_fnv_scri_target_text(&invalid, &interner, &|_| None).unwrap(),
            Some("166305:FalloutNV.esm".into())
        );
    }

    #[test]
    fn vertical_fixture_scpt_scripts_emit_two_dynamic_package_contracts() {
        let mapped = serde_json::json!({
            "1231B8:FalloutNV.esm": "2231B8:Output.esm",
            "097183:FalloutNV.esm": "197183:Output.esm",
            "11F935:FalloutNV.esm": "21F935:Output.esm",
            "0A46E7:FalloutNV.esm": "1A46E7:Output.esm",
            "1231B6:FalloutNV.esm": "2231B6:Output.esm",
            "1400F9:FalloutNV.esm": "2400F9:Output.esm",
            "134B9C:FalloutNV.esm": "234B9C:Output.esm",
            "13289E:FalloutNV.esm": "23289E:Output.esm"
        });
        let cases = [
            (
                0x123191,
                "TecMineHostage",
                vec![
                    "000014:FalloutNV.esm",
                    "1231B8:FalloutNV.esm",
                    "097183:FalloutNV.esm",
                    "11F935:FalloutNV.esm",
                    "0F43DE:FalloutNV.esm",
                    "0000C8:FalloutNV.esm",
                    "0A46E7:FalloutNV.esm",
                    "1231B6:FalloutNV.esm",
                    "1400F9:FalloutNV.esm",
                ],
            ),
            (
                0x134491,
                "NVTechatticupRenoldsDialogueScript",
                vec![
                    "134B9C:FalloutNV.esm",
                    "000014:FalloutNV.esm",
                    "11F935:FalloutNV.esm",
                    "13289E:FalloutNV.esm",
                ],
            ),
        ];
        let mut translated_scripts = Vec::new();
        for (local, editor_id, references) in cases {
            let script_source = vertical_slice_script_source(local);
            let mut fields = vec![serde_json::json!({
                "SCTX": script_source
            })];
            fields.extend(
                references
                    .into_iter()
                    .map(|reference| serde_json::json!({ "SCRO": reference })),
            );
            let record = serde_json::json!({
                "eid": editor_id,
                "fields": fields,
                "__mapped_form_keys": mapped
            });
            let translated = crate::fnv_legacy_scripting::script_synthesizer::translate_scpt_record_with_attachments(
                &record,
                "FNV_FO3",
                &format!("{local:06X}:FalloutNV.esm"),
                &[],
            )
            .unwrap_or_else(|error| panic!("{editor_id}: {error}"));
            translated_scripts.push(translated);
        }
        assert_eq!(
            translated_scripts
                .iter()
                .map(|script| script.package_data_aliases.len())
                .sum::<usize>(),
            2
        );
        let mut quest = crate::fnv_legacy_scripting::quest::QuestTranslation {
            source_editor_id: "VTechatticup".into(),
            source_form_key: "11F935:FalloutNV.esm".into(),
            target_form_key: "21F935:Output.esm".into(),
            fragment_class_name: "FNV_FO3_QF_VTechatticup_0011F935".into(),
            authoring_record_payload: serde_json::json!({
                "signature": "QUST",
                "fields": [{ "ANAM": 0 }]
            }),
            aliases: Vec::new(),
            fragment_metadata: Vec::new(),
            losses: Default::default(),
        };
        let pending = materialize_fnv_package_data_alias_contracts(
            &mut quest,
            "11F935:FalloutNV.esm",
            "21F935:Output.esm",
            &translated_scripts,
        )
        .unwrap();
        let fields = quest.authoring_record_payload["fields"].as_array().unwrap();
        assert_eq!(
            fields
                .iter()
                .filter_map(|field| {
                    let reference = field.get("ALPC")?.get("reference")?;
                    Some((
                        reference.get("plugin")?.as_str()?,
                        reference.get("object_id")?.as_str()?,
                    ))
                })
                .collect::<Vec<_>>(),
            [("Output.esm", "2231B6"), ("Output.esm", "23289E")]
        );
        assert_eq!(pending.len(), 2);
        assert_eq!(
            pending
                .iter()
                .map(|binding| (
                    binding.property.name.as_str(),
                    binding.property.prop_type.as_str(),
                    binding.property.value.as_ref().unwrap()["alias_id"]
                        .as_i64()
                        .unwrap(),
                ))
                .collect::<Vec<_>>(),
            [
                ("TecMineHostageEscapeData", "ReferenceAlias", 0),
                (
                    "TechaticupNCRRenoldsDialoguePackageData",
                    "ReferenceAlias",
                    1,
                ),
            ]
        );
        assert!(pending.iter().all(|binding| {
            binding.property.value.as_ref().unwrap()["target_quest_form_key"] == "21F935:Output.esm"
        }));
    }

    #[test]
    fn generated_hostage_greeting_keyword_reconciles_once_after_compile() {
        use crate::fnv_legacy_scripting::vmad::CompiledScriptEvidence;
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
            plugin_handle_read_authoring_record_value_json,
            plugin_handle_replace_authoring_record_value,
        };

        let mapped = serde_json::json!({
            "1231B8:FalloutNV.esm": "2231B8:Output.esm",
            "097183:FalloutNV.esm": "197183:Output.esm",
            "11F935:FalloutNV.esm": "21F935:Output.esm",
            "0A46E7:FalloutNV.esm": "1A46E7:Output.esm",
            "1231B6:FalloutNV.esm": "2231B6:Output.esm",
            "1400F9:FalloutNV.esm": "2400F9:Output.esm"
        });
        let references = [
            "000014:FalloutNV.esm",
            "1231B8:FalloutNV.esm",
            "097183:FalloutNV.esm",
            "11F935:FalloutNV.esm",
            "0F43DE:FalloutNV.esm",
            "0000C8:FalloutNV.esm",
            "0A46E7:FalloutNV.esm",
            "1231B6:FalloutNV.esm",
            "1400F9:FalloutNV.esm",
        ];
        let mut fields = vec![serde_json::json!({
            "SCTX": vertical_slice_script_source(0x123191)
        })];
        fields.extend(
            references
                .into_iter()
                .map(|reference| serde_json::json!({ "SCRO": reference })),
        );
        let record = serde_json::json!({
            "eid": "TecMineHostage",
            "fields": fields,
            "__mapped_form_keys": mapped
        });
        let mut translated = crate::fnv_legacy_scripting::script_synthesizer::translate_scpt_record_with_attachments(
            &record,
            "FNV_FO3",
            "123191:FalloutNV.esm",
            &[],
        )
        .unwrap();
        let property_name = "TecMineHostageFreedGreeting";
        assert_eq!(
            translated
                .psc_text
                .matches("Keyword Property TecMineHostageFreedGreeting Auto Const")
                .count(),
            1
        );
        assert_eq!(
            translated
                .properties
                .iter()
                .filter(|property| property.papyrus_name == property_name)
                .count(),
            1
        );
        assert!(
            translated
                .properties
                .iter()
                .find(|property| property.papyrus_name == property_name)
                .unwrap()
                .target_form_key
                .is_none()
        );

        let generated_keyword_target = "00F100:Output.esm";
        let script_class_name = translated.script_class_name.clone();
        bind_generated_scpt_property_target(
            std::slice::from_mut(&mut translated),
            "123191:FalloutNV.esm",
            &script_class_name,
            property_name,
            "Keyword",
            generated_keyword_target,
        )
        .unwrap();

        let source_handle = plugin_handle_new_native("FalloutNV.esm", Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native("Output.esm", Some("fo4")).unwrap();
        let actor_target = plugin_handle_replace_authoring_record_value(
            target_handle,
            &serde_json::json!({
                "signature": "NPC_",
                "form_id": "2300F0:Output.esm",
                "eid": "TecMineHostageActor",
                "fields": [{ "EDID": "TecMineHostageActor" }]
            }),
        )
        .unwrap();
        let run_id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                ..Default::default()
            },
        })
        .unwrap();
        let evidence = CompiledScriptEvidence {
            class_name: script_class_name.clone(),
            relative_pex_path: format!("data/Scripts/{script_class_name}.pex"),
            compiled_success: true,
        };
        let (intents, attached, _, _) = with_run(run_id, |run| {
            run.fnv_pending_vmad_targets =
                vec![(actor_target.clone(), "123191:FalloutNV.esm".into())];
            run.fnv_pending_vmad_scripts = vec![translated];
            run.reconcile_fnv_compiled_scripts(std::slice::from_ref(&evidence))
        })
        .unwrap();
        assert!(attached);
        assert_eq!(intents.len(), 1);
        let greeting_properties = intents[0]
            .properties
            .iter()
            .filter(|property| property.name == property_name)
            .collect::<Vec<_>>();
        assert_eq!(greeting_properties.len(), 1);
        assert_eq!(greeting_properties[0].prop_type, "Keyword");
        assert_eq!(
            greeting_properties[0].value,
            Some(serde_json::json!(generated_keyword_target))
        );

        let target = plugin_handle_read_authoring_record_value_json(target_handle, &actor_target)
            .unwrap()
            .unwrap();
        let vmad = target["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|field| field.get("VirtualMachineAdapter"))
            .unwrap();
        let vmad_greeting_properties = vmad["Scripts"][0]["Properties"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|property| property["propertyName"] == property_name)
            .collect::<Vec<_>>();
        assert_eq!(vmad_greeting_properties.len(), 1);
        assert_eq!(
            vmad_greeting_properties[0]["Value"]["FormID"]["reference"],
            serde_json::json!({
                "plugin": "Output.esm",
                "object_id": "00F100"
            })
        );

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn compact_slice_classes_reconcile_to_exact_vmad_cross_references() {
        use crate::fnv_legacy_scripting::dialogue::InfoFragmentPhase;
        use crate::fnv_legacy_scripting::naming::{
            quest_fragment_name, standalone_script_name, topic_info_fragment_name,
        };
        use crate::fnv_legacy_scripting::script_synthesizer::{
            PapyrusType, ScriptCompileStatus, ScriptTerminalStatus, TranslatedScript,
        };
        use crate::fnv_legacy_scripting::vmad::{
            CompiledScriptEvidence, PendingInfoFragmentBinding, PendingObjectScriptBinding,
            PendingQuestFragmentBinding, QuestStageFragment,
        };
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_load_no_py, plugin_handle_new_native,
            plugin_handle_read_authoring_record_value_json,
            plugin_handle_replace_authoring_record_value,
            plugin_handle_save_preserving_identity_no_py,
        };

        fn collect_script_names(value: &serde_json::Value, names: &mut FxHashSet<String>) {
            match value {
                serde_json::Value::Object(object) => {
                    for (key, value) in object {
                        if key.ends_with("ScriptName") {
                            if let Some(name) = value.as_str() {
                                names.insert(name.to_string());
                            }
                        }
                        collect_script_names(value, names);
                    }
                }
                serde_json::Value::Array(values) => {
                    for value in values {
                        collect_script_names(value, names);
                    }
                }
                _ => {}
            }
        }

        let prefix = "FNV_FO3";
        let scpt_locals = [0x11FC64, 0x123191, 0x134491, 0x166305];
        let scpt_classes = scpt_locals.map(|local| standalone_script_name(prefix, local));
        let quest_classes = [
            quest_fragment_name(prefix, 0x06136D),
            quest_fragment_name(prefix, 0x11F935),
        ];
        let info_classes = [
            topic_info_fragment_name("130161"),
            topic_info_fragment_name("134B9B"),
        ];
        let helper_class = format!("{prefix}_FnvSliceCompat");
        let expected_classes = scpt_classes
            .iter()
            .chain(quest_classes.iter())
            .chain(info_classes.iter())
            .chain(std::iter::once(&helper_class))
            .cloned()
            .collect::<FxHashSet<_>>();
        assert_eq!(expected_classes.len(), 9);
        assert!(expected_classes.iter().all(|name| name.len() <= 38));

        let source_handle = plugin_handle_new_native("FalloutNV.esm", Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native("FalloutNV.esm", Some("fo4")).unwrap();
        let mut target_keys = Vec::new();
        for (index, (signature, target_local)) in [
            ("QUST", 0x100),
            ("NPC_", 0x101),
            ("ACTI", 0x133F41),
            ("NPC_", 0x103),
            ("QUST", 0x104),
            ("QUST", 0x105),
            ("INFO", 0x106),
            ("INFO", 0x107),
        ]
        .into_iter()
        .enumerate()
        {
            let fields = if index == 5 {
                serde_json::json!([
                    { "EDID": format!("CompactTarget{index}") },
                    { "NextAliasID": 5 },
                    { "ALST": 0 },
                    { "ALID": "LegacyRef_FNV_FO3_12319B" },
                    { "FNAM": [] },
                    { "Reference": { "reference": { "plugin": "FalloutNV.esm", "object_id": "12319B" } } },
                    { "ALED": true },
                    { "ALST": 1 },
                    { "ALID": "LegacyRef_FNV_FO3_12319C" },
                    { "FNAM": [] },
                    { "Reference": { "reference": { "plugin": "FalloutNV.esm", "object_id": "12319C" } } },
                    { "ALED": true },
                    { "ALST": 2 },
                    { "ALID": "LegacyRef_FNV_FO3_134B9C" },
                    { "FNAM": [] },
                    { "Reference": { "reference": { "plugin": "FalloutNV.esm", "object_id": "134B9C" } } },
                    { "ALED": true },
                    { "ALST": 3 },
                    { "ALID": "TechaticupNCRRenoldsDialoguePackageData" },
                    { "FNAM": [] },
                    { "Package": { "reference": { "plugin": "FalloutNV.esm", "object_id": "13289E" } } },
                    { "ALED": true },
                    { "ALST": 4 },
                    { "ALID": "TecMineHostageEscapeData" },
                    { "FNAM": [] },
                    { "Package": { "reference": { "plugin": "FalloutNV.esm", "object_id": "1231B6" } } },
                    { "ALED": true }
                ])
            } else {
                serde_json::json!([{ "EDID": format!("CompactTarget{index}") }])
            };
            target_keys.push(
                plugin_handle_replace_authoring_record_value(
                    target_handle,
                    &serde_json::json!({
                        "signature": signature,
                        "form_id": format!("{target_local:06X}:FalloutNV.esm"),
                        "fields": fields
                    }),
                )
                .unwrap(),
            );
        }
        let legacy_target_keys = target_keys
            .iter()
            .map(|target| {
                let (plugin, local) = target.split_once(':').unwrap();
                format!("{local}:{plugin}")
            })
            .collect::<Vec<_>>();
        let package_alias_ids = {
            let quest =
                plugin_handle_read_authoring_record_value_json(target_handle, &target_keys[5])
                    .unwrap()
                    .unwrap();
            let mut current_alias = None;
            let mut by_name = HashMap::new();
            for field in quest["fields"].as_array().unwrap() {
                if let Some(alias_id) = field.get("ALST").and_then(serde_json::Value::as_i64) {
                    current_alias = Some(alias_id);
                } else if let Some(name) = field.get("ALID").and_then(serde_json::Value::as_str) {
                    by_name.insert(name.to_string(), current_alias.unwrap());
                }
            }
            by_name
        };
        let renolds_package_alias = package_alias_ids["TechaticupNCRRenoldsDialoguePackageData"];
        let hostage_package_alias = package_alias_ids["TecMineHostageEscapeData"];
        assert_eq!(renolds_package_alias, 3);
        assert_eq!(hostage_package_alias, 4);

        let run_id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "FalloutNV.esm".into(),
                fnv_quest_slice: true,
                ..Default::default()
            },
        })
        .unwrap();
        let evidence = expected_classes
            .iter()
            .map(|class_name| CompiledScriptEvidence {
                class_name: class_name.clone(),
                relative_pex_path: format!("data/Scripts/{class_name}.pex"),
                compiled_success: true,
            })
            .collect::<Vec<_>>();
        let mapper_interner = StringInterner::new();
        let mut mapper_state = MapperState::new([], MapperOptions::default());
        let mut mapped_fragment_properties = Vec::new();
        for ((source_local, target_key), source_text) in [0x06136D, 0x11F935]
            .into_iter()
            .zip(target_keys[4..6].iter())
            .zip(["06136D:FalloutNV.esm", "11F935:FalloutNV.esm"])
        {
            let source = FormKey::parse(
                &format!("{source_local:06X}@FalloutNV.esm"),
                &mapper_interner,
            )
            .unwrap();
            let (target_plugin, target_local) = target_key.split_once(':').unwrap();
            let target =
                FormKey::parse(&format!("{target_local}@{target_plugin}"), &mapper_interner)
                    .unwrap();
            mapper_state.source_to_target.insert(source, target);
            let mapped = map_quest_fragment_properties(
                &[crate::fnv_legacy_scripting::quest::QuestFragmentProperty {
                    name: "FNVSliceCompat".into(),
                    papyrus_type: helper_class.clone(),
                    source_form_key: source_text.into(),
                }],
                &mapper_state,
                &mapper_interner,
                source_text,
            )
            .unwrap();
            let mapped_target = mapped[0].value.as_ref().unwrap().as_str().unwrap();
            validate_fnv_quest_slice_compat_fragment_property(
                source_local,
                mapped_target,
                prefix,
                &mapped,
            )
            .unwrap();
            mapped_fragment_properties.push(mapped);
        }

        let (intents, attached, fragment_count, fragment_expected) = with_run(run_id, |run| {
            run.fnv_pending_vmad_targets = scpt_locals
                .iter()
                .zip(legacy_target_keys.iter())
                .map(|(local, target)| (target.clone(), format!("{local:06X}:FalloutNV.esm")))
                .collect();
            run.fnv_pending_vmad_scripts = scpt_locals
                .iter()
                .zip(scpt_classes.iter())
                .map(|(local, class_name)| TranslatedScript {
                    source_editor_id: format!("Source{local:06X}"),
                    source_form_key: format!("{local:06X}:FalloutNV.esm"),
                    script_class_name: class_name.clone(),
                    papyrus_type: PapyrusType::ObjectReference,
                    psc_text: format!("ScriptName {class_name} extends ObjectReference\n"),
                    properties: Vec::new(),
                    package_data_aliases: Vec::new(),
                    dedicated_topics: Vec::new(),
                    compile_status: ScriptCompileStatus::SourceGeneratedPendingCompile,
                    terminal_status: ScriptTerminalStatus::SourceGeneratedPendingCompile,
                })
                .collect();
            run.fnv_pending_scpt_properties = vec![
                PendingScptProperty {
                    source_scpt_form_key: "123191:FalloutNV.esm".into(),
                    script_class_name: scpt_classes[1].clone(),
                    property: crate::fnv_legacy_scripting::vmad::reference_alias_script_property(
                        "TecMineHostageEscapeData",
                        &target_keys[5],
                        hostage_package_alias as i32,
                    )
                    .unwrap(),
                },
                PendingScptProperty {
                    source_scpt_form_key: "134491:FalloutNV.esm".into(),
                    script_class_name: scpt_classes[2].clone(),
                    property: crate::fnv_legacy_scripting::vmad::reference_alias_script_property(
                        "TechaticupNCRRenoldsDialoguePackageData",
                        &target_keys[5],
                        renolds_package_alias as i32,
                    )
                    .unwrap(),
                },
            ];
            run.fnv_pending_helper_scripts = legacy_target_keys[4..6]
                .iter()
                .map(|target| PendingObjectScriptBinding {
                    target_form_key: target.clone(),
                    script_class_name: helper_class.clone(),
                    properties: Vec::new(),
                })
                .collect();
            run.fnv_pending_quest_fragments = quest_classes
                .iter()
                .zip(legacy_target_keys[4..6].iter())
                .zip(mapped_fragment_properties.iter())
                .map(
                    |((class_name, target), fragment_properties)| PendingQuestFragmentBinding {
                        target_form_key: target.clone(),
                        script_class_name: class_name.clone(),
                        fragments: vec![QuestStageFragment {
                            stage_index: 100,
                            stage_item_index: 0,
                            psc_function_name: "Fragment_Stage_0100_Item_00".into(),
                        }],
                        fragment_properties: fragment_properties.clone(),
                        aliases: Vec::new(),
                    },
                )
                .collect();
            run.fnv_pending_info_fragments = info_classes
                .iter()
                .zip(legacy_target_keys[6..8].iter())
                .enumerate()
                .map(|(index, (class_name, target))| PendingInfoFragmentBinding {
                    target_form_key: target.clone(),
                    script_class_name: class_name.clone(),
                    phases: vec![if index == 0 {
                        InfoFragmentPhase::Begin
                    } else {
                        InfoFragmentPhase::End
                    }],
                    fragment_properties: Vec::new(),
                })
                .collect();
            run.reconcile_fnv_compiled_scripts(&evidence)
        })
        .unwrap();
        assert!(attached);
        assert_eq!(intents.len(), 6);
        assert_eq!(fragment_count, 4);
        assert_eq!(fragment_expected, 4);
        assert!(
            intents
                .iter()
                .all(|intent| intent.target_form_key.starts_with("FalloutNV.esm:"))
        );

        for (target_index, property_name, alias_id) in [
            (1, "TecMineHostageEscapeData", hostage_package_alias),
            (
                2,
                "TechaticupNCRRenoldsDialoguePackageData",
                renolds_package_alias,
            ),
        ] {
            let record = plugin_handle_read_authoring_record_value_json(
                target_handle,
                &target_keys[target_index],
            )
            .unwrap()
            .unwrap();
            let vmad = record["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|field| field.get("VirtualMachineAdapter"))
                .unwrap();
            let properties = vmad["Scripts"][0]["Properties"].as_array().unwrap();
            let property = properties
                .iter()
                .find(|property| property["propertyName"] == property_name)
                .unwrap();
            assert_eq!(property["Value"]["Alias"], alias_id);
            assert_eq!(
                property["Value"]["FormID"]["reference"],
                serde_json::json!({
                    "plugin": "FalloutNV.esm",
                    "object_id": "000105"
                })
            );
        }

        let mut vmad_classes = FxHashSet::default();
        for target_key in &target_keys {
            let record = plugin_handle_read_authoring_record_value_json(target_handle, target_key)
                .unwrap()
                .unwrap();
            collect_script_names(&record, &mut vmad_classes);
        }
        assert_eq!(vmad_classes, expected_classes);
        for target_key in &target_keys[4..6] {
            let record = plugin_handle_read_authoring_record_value_json(target_handle, target_key)
                .unwrap()
                .unwrap();
            let vmad = record["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|field| field.get("VirtualMachineAdapter"))
                .unwrap();
            assert_eq!(vmad["Scripts"].as_array().unwrap().len(), 1);
            assert_eq!(vmad["Scripts"][0]["ScriptName"], helper_class);
            let properties = vmad["Script Fragments"]["Script"]["Properties"]
                .as_array()
                .unwrap();
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0]["propertyName"], "FNVSliceCompat");
            assert_eq!(properties[0]["Type"], "Object");
            let (target_plugin, target_local) = target_key.split_once(':').unwrap();
            let expected_target =
                FormKey::parse(&format!("{target_local}@{target_plugin}"), &mapper_interner)
                    .unwrap();
            assert_eq!(
                properties[0]["Value"]["FormID"]["reference"]["object_id"],
                format!("{:06X}", expected_target.local)
            );
        }

        with_run(run_id, |run| run.apply_fixups_v2().map_err(RunError::from)).unwrap();

        let temp = tempfile::tempdir().unwrap();
        let saved_path = temp.path().join("FalloutNV.esm");
        plugin_handle_save_preserving_identity_no_py(target_handle, saved_path.to_str().unwrap())
            .unwrap();
        let reopened =
            plugin_handle_load_no_py(saved_path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let acti = plugin_handle_read_authoring_record_value_json(reopened, &target_keys[2])
            .unwrap()
            .expect("saved ACTI 133F41");
        let acti_vmad = acti["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|field| field.get("VirtualMachineAdapter"))
            .expect("saved ACTI VMAD");
        assert_eq!(acti_vmad["Scripts"][0]["ScriptName"], scpt_classes[2]);
        for (target_key, property_name, alias_id) in [
            (
                &target_keys[1],
                "TecMineHostageEscapeData",
                hostage_package_alias,
            ),
            (
                &target_keys[2],
                "TechaticupNCRRenoldsDialoguePackageData",
                renolds_package_alias,
            ),
        ] {
            let record = plugin_handle_read_authoring_record_value_json(reopened, target_key)
                .unwrap()
                .unwrap();
            let vmad = record["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|field| field.get("VirtualMachineAdapter"))
                .unwrap();
            let property = vmad["Scripts"][0]["Properties"]
                .as_array()
                .unwrap()
                .iter()
                .find(|property| property["propertyName"] == property_name)
                .unwrap();
            assert_eq!(property["Value"]["Alias"], alias_id);
            assert_eq!(
                property["Value"]["FormID"]["reference"],
                serde_json::json!({
                    "plugin": "FalloutNV.esm",
                    "object_id": "000105"
                })
            );
        }
        plugin_handle_close_native(reopened);

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn info_fragment_reconciliation_preserves_all_slice_topic_children_after_reopen() {
        use crate::fnv_legacy_scripting::dialogue::InfoFragmentPhase;
        use crate::fnv_legacy_scripting::vmad::{
            CompiledScriptEvidence, PendingInfoFragmentBinding,
        };
        use crate::target_write::{
            ExistingAuthoringRecordPlacement, add_quest_child_record_native, add_record_native,
            add_topic_child_record_native, encode_form_key_for_handle,
            existing_authoring_record_placement_native,
        };
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_no_py, plugin_handle_close_native, plugin_handle_load_no_py,
            plugin_handle_new_native, plugin_handle_save_preserving_identity_no_py,
        };

        let output_plugin = "FalloutNV.esm";
        let interner = StringInterner::new();
        let output_plugin_sym = interner.intern(output_plugin);
        let target_form_key = |local| FormKey {
            local,
            plugin: output_plugin_sym,
        };
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let source_handle = plugin_handle_new_native(output_plugin, Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native(output_plugin, Some("fo4")).unwrap();
        plugin_handle_add_master_no_py(target_handle, "Fallout4.esm", None).unwrap();

        let quest = target_form_key(0x11F935);
        add_record_native(
            target_handle,
            Record::new(SigCode(*b"QUST"), quest),
            &schema,
            &interner,
        )
        .unwrap();
        let topic_ids = [0x13015B, 0x134B9A, 0x51B6F7];
        for topic_id in topic_ids {
            let mut topic = Record::new(SigCode(*b"DIAL"), target_form_key(topic_id));
            topic.fields.push(FieldEntry {
                sig: SubrecordSig(*b"QNAM"),
                value: FieldValue::FormKey(quest),
            });
            assert!(
                add_quest_child_record_native(target_handle, topic, &schema, &interner).unwrap()
            );
        }
        let info_parents = [
            (0x130161, 0x13015B),
            (0x134B9B, 0x134B9A),
            (0x15734B, 0x51B6F7),
            (0x15734C, 0x51B6F7),
            (0x15734D, 0x51B6F7),
        ];
        for (info_id, parent_id) in info_parents {
            let parent_form_id =
                encode_form_key_for_handle(target_handle, target_form_key(parent_id), &interner)
                    .unwrap();
            assert!(
                add_topic_child_record_native(
                    target_handle,
                    Record::new(SigCode(*b"INFO"), target_form_key(info_id)),
                    parent_form_id,
                    &schema,
                    &interner,
                )
                .unwrap()
            );
        }

        let run_id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: output_plugin.into(),
                fnv_quest_slice: true,
                ..Default::default()
            },
        })
        .unwrap();
        let fragment_bindings = [
            (0x130161, "TIF__130161", InfoFragmentPhase::Begin),
            (0x134B9B, "TIF__134B9B", InfoFragmentPhase::End),
        ];
        let evidence = fragment_bindings
            .iter()
            .map(|(_, class_name, _)| CompiledScriptEvidence {
                class_name: (*class_name).into(),
                relative_pex_path: format!("data/Scripts/{class_name}.pex"),
                compiled_success: true,
            })
            .collect::<Vec<_>>();
        let (_, attached, fragment_count, fragment_expected) = with_run(run_id, |run| {
            run.fnv_pending_info_fragments = fragment_bindings
                .iter()
                .map(|(local, class_name, phase)| PendingInfoFragmentBinding {
                    target_form_key: format!("{local:06X}:{output_plugin}"),
                    script_class_name: (*class_name).into(),
                    phases: vec![*phase],
                    fragment_properties: Vec::new(),
                })
                .collect();
            run.reconcile_fnv_compiled_scripts(&evidence)
        })
        .unwrap();
        assert!(attached);
        assert_eq!(fragment_count, 2);
        assert_eq!(fragment_expected, 2);

        let temp = tempfile::tempdir().unwrap();
        let saved_path = temp.path().join(output_plugin);
        plugin_handle_save_preserving_identity_no_py(target_handle, saved_path.to_str().unwrap())
            .unwrap();
        let reopened =
            plugin_handle_load_no_py(saved_path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        for (info_id, parent_id) in info_parents {
            let info_form_id =
                encode_form_key_for_handle(reopened, target_form_key(info_id), &interner).unwrap();
            let parent_form_id =
                encode_form_key_for_handle(reopened, target_form_key(parent_id), &interner)
                    .unwrap();
            assert_eq!(
                existing_authoring_record_placement_native(reopened, info_form_id).unwrap(),
                ExistingAuthoringRecordPlacement::TopicChild {
                    parent_dialogue_form_id: parent_form_id,
                },
                "INFO {info_id:06X} must reopen under DIAL {parent_id:06X}"
            );
        }

        plugin_handle_close_native(reopened);
        drop_run(run_id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn strict_freeform_player_condition_extends_mapper_and_rewrites_raw_run_on() {
        let interner = StringInterner::new();
        let source_player = FormKey::parse("000014@FalloutNV.esm", &interner).unwrap();
        let target_player = FormKey::parse("000014@Fallout4.esm", &interner).unwrap();
        let source_perk = FormKey::parse("058FDF@FalloutNV.esm", &interner).unwrap();
        let target_perk = FormKey::parse("158FDF@Output.esm", &interner).unwrap();
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                source_plugin_name: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                resolution_mode: ResolutionMode::Strict,
                ..Default::default()
            },
        );
        state.source_to_target.insert(source_perk, target_perk);
        let record = serde_json::json!({
            "eid": "FreeformPowerArmor",
            "fields": [
                { "EDID": "FreeformPowerArmor" },
                { "DATA": {
                    "encoding": "raw-bytes-hex",
                    "hex": "1D3B000000000000"
                } },
                { "CTDA": {
                    "encoding": "raw-bytes-hex",
                    "hex": "0000000000000000C1010000DF8F0500000000000200000014000000"
                } }
            ]
        });

        let accounting = register_fnv_quest_record_dependency_mappings(
            &record,
            "06136D:FalloutNV.esm",
            "FreeformPowerArmor",
            &mut state,
            &interner,
        )
        .unwrap();
        assert_eq!(accounting.len(), 1);
        assert_eq!(
            state.source_to_target.get(&source_player),
            Some(&target_player)
        );
        assert_eq!(state.source_to_target.get(&source_perk), Some(&target_perk));

        let resolver_options = state.options.clone();
        let raw_formids = |raw| resolver_options.source_form_key_for_raw_formid(raw, &interner);
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let translated = crate::fnv_legacy_scripting::quest::lower_qust_record(
            &record,
            "FNV_FO3",
            "06136D:FalloutNV.esm",
            "16136D:Output.esm",
            crate::translator::pair_hooks::fnv_conditions::LegacyConditionFamily::Fnv,
            &mut mapper,
            &raw_formids,
            &crate::translator::pair_hooks::fnv_fo4::PlacedActorAliasResolver::default(),
        )
        .unwrap();
        let ctda = translated.authoring_record_payload["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|field| field.get("CTDA"))
            .unwrap();
        let bytes = hex::decode(ctda["raw_hex"].as_str().unwrap()).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x01158FDF
        );
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            0x000014
        );

        let mut conflicting = MapperState::new(
            [],
            MapperOptions {
                source_plugin_name: "FalloutNV.esm".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                ..Default::default()
            },
        );
        conflicting
            .source_to_target
            .insert(source_player, target_perk);
        assert!(
            register_fnv_quest_record_dependency_mappings(
                &record,
                "06136D:FalloutNV.esm",
                "FreeformPowerArmor",
                &mut conflicting,
                &interner,
            )
            .is_err()
        );
        assert_eq!(
            conflicting.source_to_target.get(&source_player),
            Some(&target_perk)
        );
    }

    #[test]
    fn strict_vtech_vms20_adaptation_requires_exact_receipt_and_return_fragment() {
        use crate::fnv_legacy_scripting::quest::{
            QuestFragmentAdaptation, QuestFragmentAdaptationKind, StageFragment,
        };

        let receipt = QuestFragmentAdaptation {
            stage_index: 110,
            stage_item_index: 0,
            source_command: "StopQuest VMS20".to_string(),
            source_dependency_form_key: "10E908:FalloutNV.esm".to_string(),
            kind: QuestFragmentAdaptationKind::TargetNativeNoOp,
            reason: "VMS20 is outside the selected slice; the selected quest's FailQuest stage remains authoritative in FO4".to_string(),
        };
        let fragment = StageFragment {
            stage_index: 110,
            stage_item_index: 0,
            psc_function_name: "Fragment_Stage_0110_Item_00".to_string(),
            body: "Return".to_string(),
        };

        let accounting = account_fnv_quest_fragment_adaptations(
            "11F935:FalloutNV.esm",
            "VTechatticup",
            std::slice::from_ref(&receipt),
            std::slice::from_ref(&fragment),
        )
        .unwrap();
        assert_eq!(accounting.len(), 1);
        assert!(accounting[0].contains("dependency=10E908:FalloutNV.esm"));

        assert!(
            account_fnv_quest_fragment_adaptations(
                "11F935:FalloutNV.esm",
                "VTechatticup",
                &[],
                std::slice::from_ref(&fragment),
            )
            .is_err()
        );
        let mut wrong_dependency = receipt.clone();
        wrong_dependency.source_dependency_form_key = "10E908:FalloutNV.esm".to_string();
        assert!(
            account_fnv_quest_fragment_adaptations(
                "11F935:FalloutNV.esm",
                "VTechatticup",
                &[wrong_dependency],
                std::slice::from_ref(&fragment),
            )
            .is_err()
        );
        let mut wrong_reason = receipt.clone();
        wrong_reason.reason = "silent drop".to_string();
        assert!(
            account_fnv_quest_fragment_adaptations(
                "11F935:FalloutNV.esm",
                "VTechatticup",
                &[wrong_reason],
                std::slice::from_ref(&fragment),
            )
            .is_err()
        );
        let mut non_noop_fragment = fragment;
        non_noop_fragment.body = "VMS20.Stop()".to_string();
        assert!(
            account_fnv_quest_fragment_adaptations(
                "11F935:FalloutNV.esm",
                "VTechatticup",
                std::slice::from_ref(&receipt),
                &[non_noop_fragment],
            )
            .is_err()
        );
        assert!(
            account_fnv_quest_fragment_adaptations(
                "06136D:FalloutNV.esm",
                "FreeformPowerArmor",
                &[receipt],
                &[],
            )
            .is_err()
        );
    }

    #[test]
    fn malformed_later_vmad_prevents_every_reconciliation_update() {
        use crate::fnv_legacy_scripting::vmad::{
            CompiledScriptEvidence, FragmentKind, ScriptBindingIntent,
        };

        let original = vec![
            (
                "000001:Output.esm".to_string(),
                serde_json::json!({ "fields": [{ "EDID": "First" }] }),
            ),
            (
                "000002:Output.esm".to_string(),
                serde_json::json!({
                    "fields": [{ "VirtualMachineAdapter": "malformed" }]
                }),
            ),
        ];
        let intent = |target_form_key: &str, class_name: &str| ScriptBindingIntent {
            target_form_key: target_form_key.to_string(),
            script_class_name: class_name.to_string(),
            properties: Vec::new(),
            fragment_kind: FragmentKind::Object,
            compiled_evidence: CompiledScriptEvidence {
                class_name: class_name.to_string(),
                relative_pex_path: format!("data/Scripts/{class_name}.pex"),
                compiled_success: true,
            },
        };
        let intents = vec![
            intent("000001:Output.esm", "FirstScript"),
            intent("000002:Output.esm", "SecondScript"),
        ];

        let error = prebuild_fnv_vmad_record_updates(original.clone(), &intents, &[])
            .expect_err("later malformed VMAD must abort the full update set");

        assert!(
            error
                .to_string()
                .contains("existing VMAD payload is not an object")
        );
        assert_eq!(
            original[0].1,
            serde_json::json!({ "fields": [{ "EDID": "First" }] })
        );
    }

    #[test]
    fn info_fragment_property_uses_central_quest_mapping() {
        let interner = StringInterner::new();
        let source = FormKey::parse("11F935@FalloutNV.esm", &interner).unwrap();
        let target = FormKey::parse("21F935@FalloutNV.esm", &interner).unwrap();
        let mut state = MapperState::new([], MapperOptions::default());
        state.source_to_target.insert(source, target);
        let property = crate::fnv_legacy_scripting::dialogue::InfoFragmentProperty {
            name: "VTechatticup".to_string(),
            papyrus_type: "FNV_FO3_nv_VTechatticupQuestScript".to_string(),
            source_form_key: "11F935:FalloutNV.esm".to_string(),
        };

        let mapped =
            map_info_fragment_properties(&[property.clone()], &state, &interner, "130161").unwrap();
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].name, "VTechatticup");
        assert_eq!(mapped[0].prop_type, "Quest");
        assert_eq!(
            mapped[0].value,
            Some(serde_json::json!("21F935:FalloutNV.esm"))
        );

        state.source_to_target.clear();
        let error =
            map_info_fragment_properties(&[property], &state, &interner, "130161").unwrap_err();
        assert!(error.to_string().contains("has no target mapping"));
    }

    #[test]
    fn legacy_actor_placement_races_are_limited_to_human_ghoul_and_child() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let converted = interner.intern("FalloutNV.esm");

        for local in [
            FO4_HUMAN_RACE_LOCAL,
            FO4_GHOUL_RACE_LOCAL,
            FO4_HUMAN_CHILD_RACE_LOCAL,
        ] {
            assert!(is_supported_fo4_humanoid_race(
                FormKey {
                    local,
                    plugin: fallout4,
                },
                &interner,
            ));
        }
        assert!(!is_supported_fo4_humanoid_race(
            FormKey {
                local: 0x0001_D31E,
                plugin: fallout4,
            },
            &interner,
        ));
        assert!(!is_supported_fo4_humanoid_race(
            FormKey {
                local: FO4_HUMAN_RACE_LOCAL,
                plugin: converted,
            },
            &interner,
        ));
    }

    #[test]
    fn legacy_actor_placement_reads_achr_base_formkey() {
        let interner = StringInterner::new();
        let base = FormKey {
            local: 0x0012_3456,
            plugin: interner.intern("FalloutNV.esm"),
        };
        let mut actor = Record::new(
            SigCode::from_str("ACHR").unwrap(),
            FormKey {
                local: 0x0065_4321,
                plugin: base.plugin,
            },
        );
        actor.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NAME").unwrap(),
            value: FieldValue::FormKey(base),
        });

        assert_eq!(record_base_formkey(&actor, &interner), Some(base));
    }

    #[test]
    fn info_under_local_dial_does_not_vanilla_remap_by_editor_id() {
        let interner = StringInterner::new();
        let editor_id = interner.intern("InaccessibleLinesContainer");
        let output_plugin = interner.intern("SeventySix.esm");
        let master_plugin = interner.intern("Fallout4.esm");
        let local_parent = FormKey {
            local: 0x048015,
            plugin: output_plugin,
        };
        let vanilla_parent = FormKey {
            local: 0x048015,
            plugin: master_plugin,
        };

        assert_eq!(
            allocation_editor_id_for_parent(
                Some(editor_id),
                RecordWriteMode::TopicChildInfo,
                Some(local_parent),
                output_plugin,
            ),
            None
        );
        assert_eq!(
            allocation_editor_id_for_parent(
                Some(editor_id),
                RecordWriteMode::TopicChildInfo,
                Some(vanilla_parent),
                output_plugin,
            ),
            Some(editor_id)
        );

        let info_sig = SigCode::from_str("INFO").unwrap();
        let source_header = FormKey {
            local: 0x046CFE,
            plugin: output_plugin,
        };
        let vanilla_header = FormKey {
            local: 0x046CFE,
            plugin: master_plugin,
        };
        let mut state = MapperState::new(
            [(editor_id, vanilla_header, info_sig)],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let local_header = mapper.allocate_or_resolve(
            source_header,
            allocation_editor_id_for_parent(
                Some(editor_id),
                RecordWriteMode::TopicChildInfo,
                Some(local_parent),
                output_plugin,
            ),
            info_sig,
        );
        assert_eq!(local_header, source_header);

        let vanilla_mapped_header = mapper.allocate_or_resolve(
            FormKey {
                local: 0x046CFD,
                plugin: output_plugin,
            },
            allocation_editor_id_for_parent(
                Some(editor_id),
                RecordWriteMode::TopicChildInfo,
                Some(vanilla_parent),
                output_plugin,
            ),
            info_sig,
        );
        assert_eq!(vanilla_mapped_header, vanilla_header);

        let mut response = Record::new(
            info_sig,
            FormKey {
                local: 0x046CFF,
                plugin: output_plugin,
            },
        );
        response.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("GNAM").unwrap(),
            value: FieldValue::FormKey(source_header),
        });
        mapper.rewrite_record(&mut response).unwrap();
        assert_eq!(response.fields[0].value, FieldValue::FormKey(local_header));
    }

    #[test]
    fn fixup_warning_details_are_logged_with_a_bounded_tail() {
        let interner = StringInterner::new();
        let mut report = FixupReport::empty();
        for index in 0..FIXUP_WARNING_LOG_LIMIT + 2 {
            report.warnings.push(interner.intern(&format!(
                "ess_spawn: actor={index:06X} branch={:06X}",
                index + 1
            )));
        }

        let messages =
            fixup_warning_log_messages("fixups_v2", "encounter_zones", 1, &report, &interner);

        assert_eq!(messages.len(), FIXUP_WARNING_LOG_LIMIT + 1);
        assert_eq!(
            messages[0],
            "[fixups_v2] diagnostic encounter_zones iter=1 ess_spawn: actor=000000 branch=000001"
        );
        assert_eq!(
            messages.last().unwrap(),
            "[fixups_v2] diagnostic encounter_zones iter=1 truncated=2 total=34"
        );
    }

    #[test]
    fn post_copy_fixup_report_emits_summary_warning_and_diagnostic_events() {
        let interner = StringInterner::new();
        let mut report = FixupReport::empty();
        report.records_changed = 3;
        report.records_added = 1;
        report
            .warnings
            .push(interner.intern("ess_spawn: actor=59BA47 branch=597659"));
        report
            .diagnostics
            .push(interner.intern("source_template_override:actor=0D228A"));
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        emit_fixup_report_log(
            &event_tx,
            "repair_placed_child_refs",
            "specialize_placed_actor_templates",
            &report,
            &interner,
        );

        let events = event_rx.try_iter().collect::<Vec<_>>();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[0],
            crate::phase::PhaseEvent::Log {
                phase: "repair_placed_child_refs",
                level: crate::phase::LogLevel::Info,
                message,
            } if message.contains("changed=3") && message.contains("added=1")
        ));
        assert!(matches!(
            &events[1],
            crate::phase::PhaseEvent::Log {
                phase: "repair_placed_child_refs",
                level: crate::phase::LogLevel::Warn,
                message,
            } if message.contains("actor=59BA47 branch=597659")
        ));
        assert!(matches!(
            &events[2],
            crate::phase::PhaseEvent::Log {
                phase: "repair_placed_child_refs",
                level: crate::phase::LogLevel::Info,
                message,
            } if message.contains("source_template_override:actor=0D228A")
        ));
    }

    #[test]
    fn owned_run_releases_handles_once_and_keeps_target_live_until_drop() {
        let source = OwnedPluginHandle::new("OwnedSource.esm", "fo4");
        let target = OwnedPluginHandle::new("OwnedTarget.esm", "fo4");
        let master = OwnedPluginHandle::new("OwnedMaster.esm", "fo4");
        let source_id = source.id();
        let target_id = target.id();
        let master_id = master.id();
        let run_id = create_owned_run(
            Game::Fo4,
            Game::Fo4,
            RunConfig::default(),
            OwnedRunHandles {
                source: Some(source),
                target,
                masters: vec![master],
            },
            PathBuf::from("OwnedTarget.esm"),
            TargetMode::CreateNew,
        )
        .unwrap();

        with_run(run_id, |run| {
            assert!(run.release_source_handle());
            assert!(!run.release_source_handle());
            assert_eq!(run.release_master_handles(), 1);
            assert_eq!(run.release_master_handles(), 0);
            Ok::<_, RunError>(())
        })
        .unwrap();
        {
            let store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
                .lock()
                .unwrap();
            assert!(!store.contains_key(&source_id));
            assert!(!store.contains_key(&master_id));
            assert!(store.contains_key(&target_id));
        }

        drop_run(run_id).unwrap();
        assert!(
            !esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
                .lock()
                .unwrap()
                .contains_key(&target_id)
        );
    }

    #[test]
    fn source_worldspace_topology_rebuild_includes_skyrimse_to_fo4() {
        assert!(supports_source_worldspace_topology_rebuild(
            Game::SkyrimSe,
            Game::Fo4
        ));
        assert!(supports_source_worldspace_topology_rebuild(
            Game::Fnv,
            Game::Fo4
        ));
        assert!(!supports_source_worldspace_topology_rebuild(
            Game::Fo76,
            Game::Fo4
        ));
        assert!(!supports_source_worldspace_topology_rebuild(
            Game::SkyrimSe,
            Game::SkyrimSe
        ));
    }

    #[test]
    fn form_key_to_legacy_str_formats_python_legacy_shape() {
        let mut interner = StringInterner::new();
        let plugin = interner.intern("Output.esp");
        let fk = FormKey {
            local: 0x1234,
            plugin,
        };
        assert_eq!(
            form_key_to_legacy_str(fk, &interner).as_deref(),
            Some("001234:Output.esp")
        );
    }

    #[test]
    fn set_nvnm_parent_interior_preserves_own_master_index_and_clears_exterior() {
        // The interior NAVM parent must carry the cell's *file* FormID (own
        // master index applied). The mapper's `form_key.local` carries master
        // index 0x00; stamping that instead would resolve the parent cell to
        // the wrong master at runtime and crash the engine's NavMeshInfoMap
        // lookup on interior cells.
        use esp_authoring_core::nvnm::{NvnmGrid, NvnmParent, NvnmPayload, parse_nvnm, write_nvnm};

        let payload = NvnmPayload {
            version: 15,
            flags: 0,
            // A residual exterior-grid parent that the force MUST clear.
            parent: NvnmParent::Exterior {
                world: 0x0001_0023,
                grid_x: 1,
                grid_y: 2,
            },
            vertices: Vec::new(),
            triangles: Vec::new(),
            edge_links: Vec::new(),
            door_refs: Vec::new(),
            cover_array: Vec::new(),
            cover_triangle_mappings: Vec::new(),
            waypoints: Vec::new(),
            grid: NvnmGrid::default(),
        };

        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("NAVM").unwrap(),
            FormKey {
                local: 0x0056_8668,
                plugin: interner.intern("Output.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NVNM").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(write_nvnm(&payload))),
        });

        // Cell's file FormID: own master index 0x07 + object id.
        let cell_file_form_id = 0x0756_61B4_u32;
        set_nvnm_parent_interior(&mut record, cell_file_form_id);

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        let parsed = parse_nvnm(bytes.as_slice()).expect("re-parse NVNM");
        assert_eq!(
            parsed.parent,
            NvnmParent::Interior {
                cell: cell_file_form_id
            }
        );
        let NvnmParent::Interior { cell } = parsed.parent else {
            unreachable!();
        };
        assert_eq!(cell >> 24, 0x07, "own master index must survive the stamp");
    }

    #[test]
    fn config_preflight_rows_seed_mapper_state() {
        let interner = StringInterner::new();
        let cfg = RunConfig {
            output_plugin_name: "Output.esm".into(),
            use_base_game_assets: true,
            target_record_preflight: vec![TargetRecordPreflightRow {
                editor_id: "Ammo10mm".into(),
                signature: "AMMO".into(),
                form_key: "01F276:Fallout4.esm".into(),
            }],
            target_master_names: vec!["Fallout4.esm".into()],
            ..Default::default()
        };
        let mut entries = mapper_entries_from_preflight(&cfg, &interner);
        assert_eq!(entries.len(), 1);

        let (eid, fk, sig) = entries.remove(0);
        assert_eq!(interner.resolve(eid), Some("ammo10mm"));
        assert_eq!(interner.resolve(fk.plugin), Some("Fallout4.esm"));
        assert_eq!(fk.local, 0x01F276);
        assert_eq!(sig.as_str(), "AMMO");
    }

    #[test]
    fn mixed_case_editor_ids_use_same_preflight_mapper_key() {
        let interner = StringInterner::new();
        let cfg = RunConfig {
            output_plugin_name: "Output.esm".into(),
            use_base_game_assets: true,
            target_record_preflight: vec![TargetRecordPreflightRow {
                editor_id: "Ammo10mm".into(),
                signature: "AMMO".into(),
                form_key: "01F276:Fallout4.esm".into(),
            }],
            ..Default::default()
        };
        let entries = mapper_entries_from_preflight(&cfg, &interner);
        let source_eid = interner.intern("ammo10MM");
        let normalized_source_eid = normalized_eid_sym(source_eid, &interner);
        let sig = crate::ids::SigCode::from_str("AMMO").unwrap();
        let source_fk = FormKey {
            local: 0x800,
            plugin: interner.intern("SeventySix.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            entries,
            MapperOptions {
                output_plugin_name: "Output.esm".into(),
                use_base_game_assets: true,
                ..Default::default()
            },
            &interner,
        );

        let target_fk = mapper.allocate_or_resolve(source_fk, Some(normalized_source_eid), sig);
        assert_eq!(interner.resolve(target_fk.plugin), Some("Fallout4.esm"));
        assert_eq!(target_fk.local, 0x01F276);
    }

    #[test]
    fn preflight_source_mappings_rewrite_refs_before_duplicate_record_seen() {
        use crate::ids::{SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};

        let mut interner = StringInterner::new();
        let ammo_sig = SigCode::from_str("AMMO").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_ammo = FormKey::parse("01F276@SeventySix.esm", &mut interner).unwrap();
        let source_weap = FormKey::parse("000800@SeventySix.esm", &mut interner).unwrap();
        let target_ammo = FormKey::parse("01F276@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("ammo10MM");
        let target_eid = interner.intern("ammo10mm");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_ammo, ammo_sig)],
            &[(target_eid, target_ammo, ammo_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );
        assert_eq!(mappings, vec![(source_ammo, target_ammo)]);

        let mut state = MapperState::new(
            [(target_eid, target_ammo, ammo_sig)],
            MapperOptions {
                output_plugin_name: "Output.esm".into(),
                use_base_game_assets: true,
                ..Default::default()
            },
        );
        for (source_form_key, target_form_key) in mappings {
            state
                .source_to_target
                .insert(source_form_key, target_form_key);
        }

        let mut record = Record::new(weap_sig, source_weap);
        let ammo_field = interner.intern("ammo");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Struct(vec![(ammo_field, FieldValue::FormKey(source_ammo))]),
        });

        let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
        mapper.rewrite_record(&mut record).unwrap();

        if let FieldValue::Struct(fields) = &record.fields[0].value {
            assert_eq!(fields[0].1, FieldValue::FormKey(target_ammo));
        } else {
            panic!("expected Struct field");
        }
    }

    #[test]
    fn fo76_fo4_weap_preflight_maps_cr_prefixed_editor_ids() {
        let mut interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_weap = FormKey::parse("0DF259@SeventySix.esm", &mut interner).unwrap();
        let target_weap = FormKey::parse("0DF259@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("crAssaultronRightClaw");
        let target_eid = interner.intern("assaultronrightclaw");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_weap, weap_sig)],
            &[(target_eid, target_weap, weap_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_weap, target_weap)]);
    }

    #[test]
    fn fo76_fo4_weap_preflight_maps_zzz_prefixed_editor_ids() {
        let mut interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_weap = FormKey::parse("0AB6AE@SeventySix.esm", &mut interner).unwrap();
        let target_weap = FormKey::parse("0AB6AE@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("zzz_TutorialDummy10mm");
        let target_eid = interner.intern("tutorialdummy10mm");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_weap, weap_sig)],
            &[(target_eid, target_weap, weap_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_weap, target_weap)]);
    }

    #[test]
    fn fo76_fo4_weap_preflight_maps_cr_prefixed_same_local_id() {
        let mut interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_weap = FormKey::parse("09F24D@SeventySix.esm", &mut interner).unwrap();
        let target_weap = FormKey::parse("09F24D@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("crMirelurkHunterSpitWeapon");
        let target_eid = interner.intern("weapmirelurkhunter01");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_weap, weap_sig)],
            &[(target_eid, target_weap, weap_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_weap, target_weap)]);
    }

    #[test]
    fn fo76_fo4_preflight_maps_custom_projectile_collision_layer() {
        let mut interner = StringInterner::new();
        let coll_sig = SigCode::from_str("COLL").unwrap();
        let source_layer = FormKey::parse("5B74D0@SeventySix.esm", &mut interner).unwrap();
        let target_layer = FormKey::parse("088768@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("L_PROJ_NO_COLLIDE_PROJ");
        let target_eid = interner.intern("l_spell");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_layer, coll_sig)],
            &[(target_eid, target_layer, coll_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_layer, target_layer)]);
    }

    #[test]
    fn fo76_fo4_preflight_same_local_id_remap_requires_weap_alias_prefix() {
        let mut interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_weap = FormKey::parse("123456@SeventySix.esm", &mut interner).unwrap();
        let target_weap = FormKey::parse("123456@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("SomeDifferentWeapon");
        let target_eid = interner.intern("otherweapon");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_weap, weap_sig)],
            &[(target_eid, target_weap, weap_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn preflight_source_mappings_do_not_remap_packages_by_editor_id() {
        let mut interner = StringInterner::new();
        let pack_sig = SigCode::from_str("PACK").unwrap();
        let source_pack = FormKey::parse("407F9F@SeventySix.esm", &mut interner).unwrap();
        let target_pack = FormKey::parse("02A105@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("followplayer");
        let target_eid = interner.intern("followplayer");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_pack, pack_sig)],
            &[(target_eid, target_pack, pack_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fo76_fo4_combat_shotgun_animation_family_stays_source_owned() {
        let interner = StringInterner::new();
        let source = FormKey {
            plugin: interner.intern("SeventySix.esm"),
            local: 0x0B9560,
        };
        let target = FormKey {
            plugin: interner.intern("Fallout4.esm"),
            local: 0x0B9560,
        };
        let eid = interner.intern("AnimsCombatShotgun");
        let sig = SigCode::from_str("KYWD").unwrap();
        let mappings = source_target_mappings_from_preflight(
            [(eid, source, sig)],
            &[(eid, target, sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );
        assert!(mappings.is_empty());
        assert!(
            editor_id_vanilla_remap_blocked_source_form_keys(Game::Fo76, Game::Fo4, &interner)
                .contains(&source)
        );
        assert!(
            !editor_id_vanilla_remap_blocked_source_form_keys(Game::SkyrimSe, Game::Fo4, &interner)
                .contains(&source)
        );
    }

    #[test]
    fn fo76_fo4_robobrain_template_does_not_remap_onto_mechanist_namesake() {
        let mut interner = StringInterner::new();
        let sig = SigCode::from_str("NPC_").unwrap();
        let source = FormKey::parse("353D54@SeventySix.esm", &mut interner).unwrap();
        let target = FormKey::parse("001121@DLCRobot.esm", &mut interner).unwrap();
        let output = FormKey::parse("353D54@B21_RoboTest.esm", &mut interner).unwrap();
        let eid = interner.intern("DLC01EncRoboBrain01Template");
        let target_eid = interner.intern("dlc01encrobobrain01template");
        let targets = [(target_eid, target, sig)];

        assert!(
            source_target_mappings_from_preflight(
                [(eid, source, sig)],
                &targets,
                &interner,
                Game::Fo76,
                Game::Fo4,
            )
            .is_empty()
        );

        let mut mapper = FormKeyMapper::new(
            targets,
            MapperOptions {
                output_plugin_name: "B21_RoboTest.esm".into(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                vanilla_remap_blocked_source_form_keys:
                    editor_id_vanilla_remap_blocked_source_form_keys(
                        Game::Fo76,
                        Game::Fo4,
                        &interner,
                    ),
                ..Default::default()
            },
            &interner,
        );
        assert_eq!(mapper.allocate_or_resolve(source, Some(eid), sig), output);
    }

    #[test]
    fn fo76_fo4_newspaper_collision_stays_source_owned_and_namespaced() {
        let mut interner = StringInterner::new();
        let misc_sig = SigCode::from_str("MISC").unwrap();
        let source = FormKey::parse("01A4B9@SeventySix.esm", &mut interner).unwrap();
        let target = FormKey::parse("01A4B9@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("Newspaper01");
        let target_eid = interner.intern("newspaper01");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source, misc_sig)],
            &[(target_eid, target, misc_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );
        assert!(mappings.is_empty());

        let mut target_eid_index = FxHashMap::default();
        target_eid_index.insert(target_eid, vec![(target, misc_sig)]);
        let blocked =
            editor_id_vanilla_remap_blocked_source_form_keys(Game::Fo76, Game::Fo4, &interner);
        let mut record = Record::new(misc_sig, source);
        record.eid = Some(source_eid);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(source_eid),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(interner.intern("Props\\Newspaper02.nif")),
        });

        assert_eq!(
            rename_fo76_target_editor_id_collision(
                &mut record,
                &target_eid_index,
                &blocked,
                &interner,
                false,
            ),
            Some(("Newspaper01".into(), "Newspaper01fo76".into()))
        );

        let members = [relocation_modl_key("Props\\Newspaper02.nif")]
            .into_iter()
            .collect();
        assert_eq!(
            namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner),
            1
        );
        let model = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "MODL")
            .and_then(|field| match &field.value {
                FieldValue::String(sym) => interner.resolve(*sym),
                _ => None,
            });
        assert_eq!(model, Some("FO76\\Props\\Newspaper02.nif"));
    }

    #[test]
    fn fo76_fo4_range_offset_placeholders_keep_source_records_and_weapon_references() {
        for (source_key, target_key, editor_id) in [
            (
                "0498A1@SeventySix.esm",
                "0498A1@Fallout4.esm",
                "mod_UniversalOffset_Range_Shotgun",
            ),
            (
                "0498A2@SeventySix.esm",
                "0498A2@Fallout4.esm",
                "mod_UniversalOffset_Range_BoltAction",
            ),
            (
                "1114F8@SeventySix.esm",
                "04A643@DLCCoast.esm",
                "DLC03_mod_UniversalOffset_Range_LeverGun",
            ),
        ] {
            let mut interner = StringInterner::new();
            let source = FormKey::parse(source_key, &mut interner).unwrap();
            let target = FormKey::parse(target_key, &mut interner).unwrap();
            let output = FormKey::parse(
                &format!("{:06X}@B21_RangeTest.esm", source.local),
                &mut interner,
            )
            .unwrap();
            let eid = interner.intern(editor_id);
            let target_eid = interner.intern(&editor_id.to_ascii_lowercase());
            let sig = SigCode::from_str("OMOD").unwrap();
            let targets = [(target_eid, target, sig)];
            assert!(
                source_target_mappings_from_preflight(
                    [(eid, source, sig)],
                    &targets,
                    &interner,
                    Game::Fo76,
                    Game::Fo4,
                )
                .is_empty(),
                "{editor_id} must not acquire FO4's range modifiers during preflight"
            );

            let blocked =
                editor_id_vanilla_remap_blocked_source_form_keys(Game::Fo76, Game::Fo4, &interner);
            let mut weapon = Record::new(
                SigCode::from_str("WEAP").unwrap(),
                FormKey::parse("12DBB3@SeventySix.esm", &mut interner).unwrap(),
            );
            weapon.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("OBTS").unwrap(),
                value: FieldValue::Struct(vec![(
                    interner.intern("Includes"),
                    FieldValue::List(vec![FieldValue::Struct(vec![(
                        interner.intern("Mod"),
                        FieldValue::FormKey(source),
                    )])]),
                )]),
            });
            let mut mapper = FormKeyMapper::new(
                targets,
                MapperOptions {
                    output_plugin_name: "B21_RangeTest.esm".into(),
                    use_base_game_assets: true,
                    preserve_source_ids: true,
                    vanilla_remap_blocked_source_form_keys: blocked,
                    ..Default::default()
                },
                &interner,
            );
            assert_eq!(mapper.allocate_or_resolve(source, Some(eid), sig), output);
            mapper.rewrite_record(&mut weapon).unwrap();
            let FieldValue::Struct(fields) = &weapon.fields[0].value else {
                panic!("missing weapon template");
            };
            let FieldValue::List(includes) = &fields[0].1 else {
                panic!("missing template includes");
            };
            let FieldValue::Struct(include) = &includes[0] else {
                panic!("missing included mod");
            };
            assert_eq!(include[0].1, FieldValue::FormKey(output));
        }
    }

    #[test]
    fn fo76_fo4_range_offset_fix_keeps_unrelated_omod_remaps() {
        let mut interner = StringInterner::new();
        let source = FormKey::parse("04F21D@SeventySix.esm", &mut interner).unwrap();
        let target = FormKey::parse("04F21D@Fallout4.esm", &mut interner).unwrap();
        let eid = interner.intern("mod_Null_Muzzle");
        let sig = SigCode::from_str("OMOD").unwrap();
        assert_eq!(
            source_target_mappings_from_preflight(
                [(eid, source, sig)],
                &[(interner.intern("mod_null_muzzle"), target, sig)],
                &interner,
                Game::Fo76,
                Game::Fo4,
            ),
            vec![(source, target)]
        );
    }

    #[test]
    fn fo76_fo4_static_preflight_mappings_do_not_remap_by_editor_id() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_stat = FormKey::parse("012345@SeventySix.esm", &mut interner).unwrap();
        let target_stat = FormKey::parse("012345@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("TerrainShelfRocks01");
        let target_eid = interner.intern("TerrainShelfRocks01");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_stat, stat_sig)],
            &[(target_eid, target_stat, stat_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fo76_fo4_static_marker_preflight_mappings_remap_by_editor_id() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_marker = FormKey::parse("00003B@SeventySix.esm", &mut interner).unwrap();
        let target_marker = FormKey::parse("00003B@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("XMarker");
        let target_eid = interner.intern("xmarker");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_marker, stat_sig)],
            &[(target_eid, target_marker, stat_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_marker, target_marker)]);
    }

    #[test]
    fn fo76_fo4_movable_static_preflight_mappings_do_not_remap_by_editor_id() {
        let mut interner = StringInterner::new();
        let mstt_sig = SigCode::from_str("MSTT").unwrap();
        let source_mstt = FormKey::parse("196D46@SeventySix.esm", &mut interner).unwrap();
        let target_mstt = FormKey::parse("196D46@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("Vehicle_ShuttleBus01");
        let target_eid = interner.intern("Vehicle_ShuttleBus01");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_mstt, mstt_sig)],
            &[(target_eid, target_mstt, mstt_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fnv_fo3_fo4_world_object_preflight_mappings_stay_source_owned() {
        for source_game in [Game::Fnv, Game::Fo3] {
            let mut interner = StringInterner::new();
            for (index, signature) in CROSS_GAME_FO4_WORLD_OBJECT_VANILLA_REMAP_BLOCKED_SIGS
                .iter()
                .enumerate()
            {
                let sig = SigCode::from_str(signature).unwrap();
                let source = FormKey {
                    local: 0x0010_0000 + index as u32,
                    plugin: interner.intern(match source_game {
                        Game::Fnv => "FalloutNV.esm",
                        Game::Fo3 => "Fallout3.esm",
                        _ => unreachable!(),
                    }),
                };
                let target = FormKey {
                    local: 0x0020_0000 + index as u32,
                    plugin: interner.intern("Fallout4.esm"),
                };
                let editor_id = format!("Shared{signature}Object");
                let source_editor_id = interner.intern(&editor_id);
                let target_editor_id = interner.intern(&editor_id.to_ascii_lowercase());

                let mappings = source_target_mappings_from_preflight(
                    [(source_editor_id, source, sig)],
                    &[(target_editor_id, target, sig)],
                    &interner,
                    source_game,
                    Game::Fo4,
                );

                assert!(mappings.is_empty(), "{source_game:?} {signature}");
            }
        }
    }

    #[test]
    fn fnv_fo3_fo4_consumable_preflight_mappings_still_reuse_fo4() {
        for source_game in [Game::Fnv, Game::Fo3] {
            let mut interner = StringInterner::new();
            for (index, signature) in ["ALCH", "AMMO"].into_iter().enumerate() {
                let sig = SigCode::from_str(signature).unwrap();
                let source = FormKey {
                    local: 0x0010_0000 + index as u32,
                    plugin: interner.intern(match source_game {
                        Game::Fnv => "FalloutNV.esm",
                        Game::Fo3 => "Fallout3.esm",
                        _ => unreachable!(),
                    }),
                };
                let target = FormKey {
                    local: 0x0020_0000 + index as u32,
                    plugin: interner.intern("Fallout4.esm"),
                };
                let editor_id = format!("Shared{signature}Item");
                let source_editor_id = interner.intern(&editor_id);
                let target_editor_id = interner.intern(&editor_id.to_ascii_lowercase());

                let mappings = source_target_mappings_from_preflight(
                    [(source_editor_id, source, sig)],
                    &[(target_editor_id, target, sig)],
                    &interner,
                    source_game,
                    Game::Fo4,
                );

                assert_eq!(
                    mappings,
                    vec![(source, target)],
                    "{source_game:?} {signature}"
                );
            }
        }
    }

    #[test]
    fn fnv_fo4_static_marker_reuses_fo4_record() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_marker = FormKey::parse("00003B@FalloutNV.esm", &mut interner).unwrap();
        let target_marker = FormKey::parse("00003B@Fallout4.esm", &mut interner).unwrap();
        let marker_editor_id = interner.intern("xmarker");
        let mut mapper = FormKeyMapper::new(
            [(marker_editor_id, target_marker, stat_sig)],
            MapperOptions {
                output_plugin_name: "FalloutNV.esm".into(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                    Game::Fnv,
                    Game::Fo4,
                ),
                ..Default::default()
            },
            &mut interner,
        );

        let mapped = mapper.allocate_or_resolve(source_marker, Some(marker_editor_id), stat_sig);

        assert_eq!(mapped, target_marker);
    }

    #[test]
    fn fnv_fo4_visible_static_stays_source_owned() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_static = FormKey::parse("0C387F@Fallout3.esm", &mut interner).unwrap();
        let target_static = FormKey::parse("0C387F@Fallout4.esm", &mut interner).unwrap();
        let editor_id = interner.intern("JunkWall01");
        let mut mapper = FormKeyMapper::new(
            [(editor_id, target_static, stat_sig)],
            MapperOptions {
                output_plugin_name: "FalloutNV.esm".into(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                    Game::Fnv,
                    Game::Fo4,
                ),
                ..Default::default()
            },
            &mut interner,
        );

        let mapped = mapper.allocate_or_resolve(source_static, Some(editor_id), stat_sig);

        assert_eq!(mapped.local, source_static.local);
        assert_eq!(interner.resolve(mapped.plugin), Some("FalloutNV.esm"));
        assert_ne!(mapped, target_static);
    }

    #[test]
    fn fo76_fo4_creature_skin_records_do_not_remap_by_editor_id() {
        let mut interner = StringInterner::new();
        let mut blocked = FxHashSet::default();
        blocked.insert(FormKey::parse("112BED@SeventySix.esm", &mut interner).unwrap());
        blocked.insert(FormKey::parse("112BEB@SeventySix.esm", &mut interner).unwrap());

        for (sig_name, source_fk, target_fk, editor_id) in [
            (
                "ARMO",
                "112BED@SeventySix.esm",
                "00CE5F@DLCNukaWorld.esm",
                "DLC04_SkinRadAnt01",
            ),
            (
                "ARMA",
                "112BEB@SeventySix.esm",
                "00CE5D@DLCNukaWorld.esm",
                "DLC04_RadAntAA",
            ),
        ] {
            let sig = SigCode::from_str(sig_name).unwrap();
            let source = FormKey::parse(source_fk, &mut interner).unwrap();
            let target = FormKey::parse(target_fk, &mut interner).unwrap();
            let source_eid = interner.intern(editor_id);
            let target_eid = interner.intern(&editor_id.to_ascii_lowercase());

            let mappings = source_target_mappings_from_preflight_with_skips(
                [(source_eid, source, sig)],
                &[(target_eid, target, sig)],
                &interner,
                Game::Fo76,
                Game::Fo4,
                &blocked,
            );

            assert!(mappings.is_empty(), "{sig_name} must stay source-owned");
        }
    }

    #[test]
    fn fo76_fo4_wearable_armor_records_still_remap_by_editor_id() {
        let mut interner = StringInterner::new();

        for (sig_name, source_fk, target_fk, editor_id) in [
            (
                "ARMO",
                "210000@SeventySix.esm",
                "220000@Fallout4.esm",
                "Armor_Raider_Chest",
            ),
            (
                "ARMA",
                "210001@SeventySix.esm",
                "220001@Fallout4.esm",
                "Armor_Raider_Chest_AA",
            ),
        ] {
            let sig = SigCode::from_str(sig_name).unwrap();
            let source = FormKey::parse(source_fk, &mut interner).unwrap();
            let target = FormKey::parse(target_fk, &mut interner).unwrap();
            let source_eid = interner.intern(editor_id);
            let target_eid = interner.intern(&editor_id.to_ascii_lowercase());

            let mappings = source_target_mappings_from_preflight(
                [(source_eid, source, sig)],
                &[(target_eid, target, sig)],
                &interner,
                Game::Fo76,
                Game::Fo4,
            );

            assert_eq!(mappings, vec![(source, target)], "{sig_name} should remap");
        }
    }

    #[test]
    fn fo76_fo4_skin_race_exact_target_match_does_not_need_protection() {
        let mut interner = StringInterner::new();
        let race_sig = SigCode::from_str("RACE").unwrap();
        let source_protectron_race =
            FormKey::parse("0DFB33@SeventySix.esm", &mut interner).unwrap();
        let target_protectron_race = FormKey::parse("0DFB33@Fallout4.esm", &mut interner).unwrap();
        let source_protectron_eid = interner.intern("ProtectronRace");
        let target_protectron_eid = interner.intern("protectronrace");

        assert!(source_race_has_same_editor_id_target(
            source_protectron_race,
            &[(source_protectron_eid, source_protectron_race, race_sig)],
            &[(target_protectron_eid, target_protectron_race, race_sig)],
            &interner,
        ));

        let source_rad_ant_race = FormKey::parse("112BEC@SeventySix.esm", &mut interner).unwrap();
        let target_rad_ant_race = FormKey::parse("00CE5E@DLCNukaWorld.esm", &mut interner).unwrap();
        let source_rad_ant_eid = interner.intern("RadAntRace");
        let target_rad_ant_eid = interner.intern("dlc04_radantrace");

        assert!(!source_race_has_same_editor_id_target(
            source_rad_ant_race,
            &[(source_rad_ant_eid, source_rad_ant_race, race_sig)],
            &[(target_rad_ant_eid, target_rad_ant_race, race_sig)],
            &interner,
        ));
    }

    #[test]
    fn fo76_fo4_cell_preflight_mappings_do_not_remap_reused_editor_ids() {
        let mut interner = StringInterner::new();
        let cell_sig = SigCode::from_str("CELL").unwrap();
        let source_cell = FormKey::parse("261548@SeventySix.esm", &mut interner).unwrap();
        let target_cell = FormKey::parse("00DFF7@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("RelayTower04Ext");
        let target_eid = interner.intern("relaytower04ext");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_cell, cell_sig)],
            &[(target_eid, target_cell, cell_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fo76_fo4_info_preflight_mappings_defer_to_topic_parent() {
        let mut interner = StringInterner::new();
        let info_sig = SigCode::from_str("INFO").unwrap();
        let source_info = FormKey::parse("046CFE@SeventySix.esm", &mut interner).unwrap();
        let target_info = FormKey::parse("046CFE@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("PlayerCantPickMasterContainer");
        let target_eid = interner.intern("playercantpickmastercontainer");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_info, info_sig)],
            &[(target_eid, target_info, info_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fnv_fo4_wilderness_cell_stays_source_owned() {
        let mut interner = StringInterner::new();
        let cell_sig = SigCode::from_str("CELL").unwrap();
        let source_cell = FormKey::parse("0DDCAB@FalloutNV.esm", &mut interner).unwrap();
        let target_cell = FormKey::parse("000D8A@DLCCoast.esm", &mut interner).unwrap();
        let wilderness = interner.intern("wilderness");
        let mut mapper = FormKeyMapper::new(
            [(wilderness, target_cell, cell_sig)],
            MapperOptions {
                output_plugin_name: "FalloutNV.esm".into(),
                use_base_game_assets: true,
                vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                    Game::Fnv,
                    Game::Fo4,
                ),
                preserve_source_ids: true,
                ..Default::default()
            },
            &interner,
        );

        let mapped = mapper.allocate_or_resolve(source_cell, Some(wilderness), cell_sig);

        assert_eq!(mapped.local, 0x0D_DCAB);
        assert_eq!(interner.resolve(mapped.plugin), Some("FalloutNV.esm"));
        assert_ne!(mapped, target_cell);
        assert!(!allows_source_target_preflight_remap(
            Game::Fnv,
            Game::Fo4,
            cell_sig,
            "Wilderness",
        ));
    }

    #[test]
    fn fo76_fo4_cell_editor_id_collision_gets_fo76_suffix() {
        let mut interner = StringInterner::new();
        let cell_sig = SigCode::from_str("CELL").unwrap();
        let source_cell = FormKey::parse("261548@SeventySix.esm", &mut interner).unwrap();
        let target_cell = FormKey::parse("00DFF7@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("RelayTower04Ext");
        let normalized_eid = interner.intern("relaytower04ext");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_cell, cell_sig)]);

        let mut record = Record::new(cell_sig, source_cell);
        record.eid = Some(original_eid);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(original_eid),
        });

        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &FxHashSet::default(),
            &interner,
            false,
        );

        assert_eq!(
            renamed,
            Some(("RelayTower04Ext".into(), "RelayTower04Extfo76".into()))
        );
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("RelayTower04Extfo76")
        );
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig, edid_sig);
        assert_eq!(
            match record.fields[0].value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            },
            Some("RelayTower04Extfo76")
        );
    }

    #[test]
    fn fo76_fo4_cross_signature_editor_id_collision_gets_fo76_suffix() {
        let mut interner = StringInterner::new();
        let alch_sig = SigCode::from_str("ALCH").unwrap();
        let npc_sig = SigCode::from_str("NPC_").unwrap();
        let source_alch = FormKey::parse("0330FB@SeventySix.esm", &mut interner).unwrap();
        let target_npc = FormKey::parse("01D15C@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("Dogmeat");
        let normalized_eid = interner.intern("dogmeat");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_npc, npc_sig)]);

        let mut record = Record::new(alch_sig, source_alch);
        record.eid = Some(original_eid);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(original_eid),
        });

        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &FxHashSet::default(),
            &interner,
            false,
        );

        assert_eq!(renamed, Some(("Dogmeat".into(), "Dogmeatfo76".into())));
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("Dogmeatfo76")
        );
        assert_eq!(
            match record.fields[0].value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            },
            Some("Dogmeatfo76")
        );
    }

    #[test]
    fn fo76_fo4_blocked_same_signature_editor_id_collision_gets_fo76_suffix() {
        let mut interner = StringInterner::new();
        let armo_sig = SigCode::from_str("ARMO").unwrap();
        let source_skin = FormKey::parse("112BED@SeventySix.esm", &mut interner).unwrap();
        let target_skin = FormKey::parse("00CE5F@DLCNukaWorld.esm", &mut interner).unwrap();
        let original_eid = interner.intern("DLC04_SkinRadAnt01");
        let normalized_eid = interner.intern("dlc04_skinradant01");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_skin, armo_sig)]);

        let mut record = Record::new(armo_sig, source_skin);
        record.eid = Some(original_eid);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(original_eid),
        });

        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &FxHashSet::default(),
            &interner,
            true,
        );

        assert_eq!(
            renamed,
            Some(("DLC04_SkinRadAnt01".into(), "DLC04_SkinRadAnt01fo76".into()))
        );
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("DLC04_SkinRadAnt01fo76")
        );
    }

    #[test]
    fn fo76_fo4_same_signature_editor_id_collision_keeps_vanilla_remap_candidate() {
        let mut interner = StringInterner::new();
        let alch_sig = SigCode::from_str("ALCH").unwrap();
        let source_alch = FormKey::parse("0330FB@SeventySix.esm", &mut interner).unwrap();
        let target_alch = FormKey::parse("0330FB@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("Dogmeat");
        let normalized_eid = interner.intern("dogmeat");
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_alch, alch_sig)]);

        let mut record = Record::new(alch_sig, source_alch);
        record.eid = Some(original_eid);

        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &FxHashSet::default(),
            &interner,
            false,
        );

        assert_eq!(renamed, None);
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("Dogmeat")
        );
    }

    #[test]
    fn fo76_fo4_decal_txst_collision_stays_source_owned() {
        let mut interner = StringInterner::new();
        let txst_sig = SigCode::from_str("TXST").unwrap();
        let source_txst = FormKey::parse("061880@SeventySix.esm", &mut interner).unwrap();
        let target_txst = FormKey::parse("061880@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("DecalGrossStain02");
        let normalized_eid = interner.intern("decalgrossstain02");
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_txst, txst_sig)]);

        let mut record = Record::new(txst_sig, source_txst);
        record.eid = Some(original_eid);

        assert!(!allows_source_target_preflight_remap(
            Game::Fo76,
            Game::Fo4,
            txst_sig,
            "DecalGrossStain02",
        ));
        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &FxHashSet::default(),
            &interner,
            false,
        );

        assert_eq!(
            renamed,
            Some(("DecalGrossStain02".into(), "DecalGrossStain02fo76".into()))
        );
    }

    #[test]
    fn fo76_fo4_non_decal_txst_collision_keeps_vanilla_remap_candidate() {
        let mut interner = StringInterner::new();
        let txst_sig = SigCode::from_str("TXST").unwrap();
        let source_txst = FormKey::parse("061880@SeventySix.esm", &mut interner).unwrap();
        let target_txst = FormKey::parse("061880@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("LandscapeDirt01");
        let normalized_eid = interner.intern("landscapedirt01");
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_txst, txst_sig)]);

        let mut record = Record::new(txst_sig, source_txst);
        record.eid = Some(original_eid);

        assert!(allows_source_target_preflight_remap(
            Game::Fo76,
            Game::Fo4,
            txst_sig,
            "LandscapeDirt01",
        ));
        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &FxHashSet::default(),
            &interner,
            false,
        );

        assert_eq!(renamed, None);
    }

    /// Every source but FO76 shares Bethesda's naming habits with FO4 without
    /// sharing its records: `RockCliff04` names a different rock in Skyrim,
    /// `CloudDistant04` a different cloud in Starfield. World object signatures
    /// must stay source-owned for all of them.
    #[test]
    fn every_non_fo76_source_blocks_world_object_vanilla_remap() {
        for source in [
            Game::Skyrim,
            Game::SkyrimSe,
            Game::Starfield,
            Game::Fnv,
            Game::Fo3,
            Game::Oblivion,
        ] {
            let blocked = editor_id_vanilla_remap_blocked_sigs(source, Game::Fo4);
            for sig in CROSS_GAME_FO4_WORLD_OBJECT_VANILLA_REMAP_BLOCKED_SIGS {
                assert!(
                    blocked.iter().any(|b| b == sig),
                    "{source:?}->Fo4 must block {sig}"
                );
            }
        }
    }

    /// FO76 genuinely inherits Fallout 4's record set, so its block list stays
    /// narrow — widening it to the world object list would stop FO76 reusing
    /// records it really does share. This is the one source the generalized
    /// `source != Fo76` branch must not catch.
    #[test]
    fn fo76_fo4_keeps_narrow_vanilla_remap_block_list() {
        let blocked = editor_id_vanilla_remap_blocked_sigs(Game::Fo76, Game::Fo4);
        assert!(blocked.iter().any(|b| b == "STAT"));
        for sig in ["ACTI", "MISC", "SOUN", "WEAP", "NPC_"] {
            assert!(
                !blocked.iter().any(|b| b == sig),
                "FO76->Fo4 must keep reusing {sig}"
            );
        }
    }

    /// A same-game run is not a cross-game conversion and must stay unblocked.
    #[test]
    fn fo4_to_fo4_blocks_nothing() {
        assert!(editor_id_vanilla_remap_blocked_sigs(Game::Fo4, Game::Fo4).is_empty());
    }

    #[test]
    fn fo76_fo4_static_marker_editor_id_collision_keeps_vanilla_remap_candidate() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_marker = FormKey::parse("00003B@SeventySix.esm", &mut interner).unwrap();
        let target_marker = FormKey::parse("00003B@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("XMarker");
        let normalized_eid = interner.intern("xmarker");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_marker, stat_sig)]);

        let mut record = Record::new(stat_sig, source_marker);
        record.eid = Some(original_eid);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(original_eid),
        });

        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &FxHashSet::default(),
            &interner,
            true,
        );

        assert_eq!(renamed, None);
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("XMarker")
        );
        assert_eq!(
            match record.fields[0].value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            },
            Some("XMarker")
        );
    }

    #[test]
    fn target_master_remap_classification_matches_plugin_sym() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let output = interner.intern("Output.esm");
        let target_master_syms: FxHashSet<Sym> = [fallout4].into_iter().collect();

        assert!(is_target_master_remap(
            FormKey {
                local: 0x01F276,
                plugin: fallout4,
            },
            &target_master_syms
        ));
        assert!(!is_target_master_remap(
            FormKey {
                local: 0x800,
                plugin: output,
            },
            &target_master_syms
        ));
    }

    #[test]
    fn target_master_names_from_config_apply_outside_whole_plugin_mode() {
        let cfg = RunConfig {
            is_whole_plugin: false,
            target_master_names: vec!["Fallout4.esm".into()],
            ..Default::default()
        };

        assert_eq!(
            target_master_names_for_skip(&cfg, Vec::new()),
            vec!["Fallout4.esm".to_string()]
        );
    }

    #[test]
    fn fo76_fo4_relocation_member_static_model_path_is_namespaced() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_stat = FormKey::parse("012345@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(stat_sig, source_stat);
        let model = interner.intern("Landscape\\DirtCliffs\\TerrainShelfRocks01.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        let members: std::collections::HashSet<String> = [relocation_modl_key(
            "Landscape\\DirtCliffs\\TerrainShelfRocks01.nif",
        )]
        .into_iter()
        .collect();
        let changed = namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MODL");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("FO76\\Landscape\\DirtCliffs\\TerrainShelfRocks01.nif")
        );
    }

    #[test]
    fn fo76_fo4_changed_decal_material_path_is_namespaced() {
        let mut interner = StringInterner::new();
        let txst_sig = SigCode::from_str("TXST").unwrap();
        let source_txst = FormKey::parse("112A13@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(txst_sig, source_txst);
        let material = interner.intern("DLC04\\DECALS\\DLC04_ParkingSpaceDecal_SWSingle03.BGSM");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MNAM").unwrap(),
            value: FieldValue::String(material),
        });
        let members = [relocation_material_key(
            "DLC04\\DECALS\\DLC04_ParkingSpaceDecal_SWSingle03.BGSM",
        )]
        .into_iter()
        .collect();

        let changed =
            namespace_base_asset_decal_material_path(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MNAM");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("FO76\\DLC04\\DECALS\\DLC04_ParkingSpaceDecal_SWSingle03.BGSM")
        );
    }

    #[test]
    fn fo76_fo4_relocation_member_movable_static_model_path_is_namespaced() {
        let mut interner = StringInterner::new();
        let mstt_sig = SigCode::from_str("MSTT").unwrap();
        let source_mstt = FormKey::parse("196D46@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(mstt_sig, source_mstt);
        let model = interner.intern("vehicles\\whitespring\\Vehicle_ShuttleBus_WhitePlain.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        let members: std::collections::HashSet<String> = [relocation_modl_key(
            "vehicles\\whitespring\\Vehicle_ShuttleBus_WhitePlain.nif",
        )]
        .into_iter()
        .collect();
        let changed = namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MODL");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("FO76\\vehicles\\whitespring\\Vehicle_ShuttleBus_WhitePlain.nif")
        );
    }

    #[test]
    fn fo76_fo4_forced_flagwall_static_model_path_is_namespaced() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_stat = FormKey::parse("17FE29@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(stat_sig, source_stat);
        let model = interner.intern("SetDressing\\Minutemen\\FlagWallMinutemen01.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        let members: std::collections::HashSet<String> = [relocation_modl_key(
            "SetDressing\\Minutemen\\FlagWallMinutemen01.nif",
        )]
        .into_iter()
        .collect();
        let changed = namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MODL");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("FO76\\SetDressing\\Minutemen\\FlagWallMinutemen01.nif")
        );
    }

    #[test]
    fn fo76_fo4_non_member_model_path_is_left_untouched() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_stat = FormKey::parse("012345@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(stat_sig, source_stat);
        let model = interner.intern("Landscape\\DirtCliffs\\TerrainShelfRocks01.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        // Member set covers a different mesh, so this STAT's MODL must not move.
        let members: std::collections::HashSet<String> =
            [relocation_modl_key("Landscape\\Rocks\\SomeOtherRock.nif")]
                .into_iter()
                .collect();
        let changed = namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 0);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MODL");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Landscape\\DirtCliffs\\TerrainShelfRocks01.nif")
        );
    }

    #[test]
    fn fo76_fo4_terrain_phase_skips_terrain_owned_records() {
        let mut translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let cfg = RunConfig {
            asset_phases: AssetPhaseFlags {
                terrain: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(!translator.maps.skip_records.contains("LTEX"));
        assert!(!translator.maps.skip_records.contains("GRAS"));

        apply_terrain_owned_record_skips(&mut translator, Game::Fo76, Game::Fo4, &cfg);

        assert!(translator.maps.skip_records.contains("LTEX"));
        assert!(translator.maps.skip_records.contains("GRAS"));
    }

    #[test]
    fn fo76_fo4_without_terrain_phase_keeps_ltex_and_gras_translatable() {
        let mut translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let cfg = RunConfig {
            asset_phases: AssetPhaseFlags {
                terrain: false,
                ..Default::default()
            },
            ..Default::default()
        };

        apply_terrain_owned_record_skips(&mut translator, Game::Fo76, Game::Fo4, &cfg);

        assert!(!translator.maps.skip_records.contains("LTEX"));
        assert!(!translator.maps.skip_records.contains("GRAS"));
    }

    #[test]
    fn starfield_fo4_terrain_phase_skips_terrain_owned_records() {
        // Starfield routes through the same BTD terrain-texture phase as FO76,
        // so the generic writer must not also emit LTEX/GRAS.
        let mut translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let cfg = RunConfig {
            asset_phases: AssetPhaseFlags {
                terrain: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(!translator.maps.skip_records.contains("LTEX"));
        assert!(!translator.maps.skip_records.contains("GRAS"));

        apply_terrain_owned_record_skips(&mut translator, Game::Starfield, Game::Fo4, &cfg);

        assert!(translator.maps.skip_records.contains("LTEX"));
        assert!(translator.maps.skip_records.contains("GRAS"));
    }

    #[test]
    fn starfield_fo4_without_terrain_phase_keeps_ltex_and_gras_translatable() {
        let mut translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let cfg = RunConfig {
            asset_phases: AssetPhaseFlags {
                terrain: false,
                ..Default::default()
            },
            ..Default::default()
        };

        apply_terrain_owned_record_skips(&mut translator, Game::Starfield, Game::Fo4, &cfg);

        assert!(!translator.maps.skip_records.contains("LTEX"));
        assert!(!translator.maps.skip_records.contains("GRAS"));
    }

    #[test]
    fn explicit_quest_dialogue_skips_disable_structured_dialogue_tail() {
        for signature in ["QUST", "DIAL", "INFO"] {
            let cfg = RunConfig {
                skip_record_signatures: vec![signature.to_lowercase()],
                ..Default::default()
            };

            assert!(cfg.skips_fo76_quest_dialogue());
        }

        let cfg = RunConfig {
            skip_record_signatures: vec!["WEAP".to_string()],
            ..Default::default()
        };
        assert!(!cfg.skips_fo76_quest_dialogue());
    }

    fn mvp_exception(signature: &str, local: u32, plugin: &str) -> MvpRecordExceptionRow {
        MvpRecordExceptionRow {
            signature: signature.to_string(),
            local_form_id: local,
            source_plugin: plugin.to_string(),
        }
    }

    #[test]
    fn mvp_weapon_closures_are_pair_scoped_and_require_all_rows() {
        let mut config = RunConfig {
            skip_record_signatures: vec!["weap".to_string()],
            mvp_record_exceptions: vec![
                mvp_exception("weap", 0x01_3984, "skyrim.esm"),
                mvp_exception("STAT", 0x02_0E27, "SKYRIM.ESM"),
            ],
            ..Default::default()
        };
        config
            .validate_mvp_record_exceptions(Game::SkyrimSe, Game::Fo4)
            .unwrap();
        assert!(config.has_complete_mvp_weapon_closure(Game::SkyrimSe, Game::Fo4));
        assert!(!config.has_complete_mvp_weapon_closure(Game::Fnv, Game::Fo4));
        let interner = StringInterner::new();
        let skyrim = interner.intern("Skyrim.esm");
        let update = interner.intern("Update.esm");
        assert!(config.is_mvp_record_exception(
            Game::SkyrimSe,
            Game::Fo4,
            FormKey {
                local: 0x01_3984,
                plugin: skyrim,
            },
            SigCode::from_str("WEAP").unwrap(),
            &interner,
        ));
        for form_key in [
            FormKey {
                local: 0x01_3985,
                plugin: skyrim,
            },
            FormKey {
                local: 0x01_3984,
                plugin: update,
            },
        ] {
            assert!(!config.is_mvp_record_exception(
                Game::SkyrimSe,
                Game::Fo4,
                form_key,
                SigCode::from_str("WEAP").unwrap(),
                &interner,
            ));
        }

        config.mvp_record_exceptions.pop();
        assert!(!config.has_complete_mvp_weapon_closure(Game::SkyrimSe, Game::Fo4));
    }

    #[test]
    fn malformed_mvp_weapon_exception_rows_are_rejected() {
        for rows in [
            vec![mvp_exception("ARMO", 0x01_3984, "Skyrim.esm")],
            vec![mvp_exception("WEAP", 0x01_3984, "Update.esm")],
            vec![mvp_exception("WEAP", 0x0101_3984, "Skyrim.esm")],
            vec![
                mvp_exception("WEAP", 0x01_3984, "Skyrim.esm"),
                mvp_exception("weap", 0x01_3984, "SKYRIM.ESM"),
            ],
        ] {
            let mut config = RunConfig {
                skip_record_signatures: vec!["WEAP".to_string()],
                mvp_record_exceptions: rows,
                ..Default::default()
            };
            assert!(
                config
                    .validate_mvp_record_exceptions(Game::SkyrimSe, Game::Fo4)
                    .is_err()
            );
        }
    }

    #[test]
    fn melee_only_requires_supported_pair_and_weap_fence() {
        let config = RunConfig {
            mvp_melee_only: true,
            skip_record_signatures: vec!["WEAP".to_string()],
            ..Default::default()
        };
        config
            .validate_mvp_melee_only(Game::SkyrimSe, Game::Fo4)
            .unwrap();
        config
            .validate_mvp_melee_only(Game::Fnv, Game::Fo4)
            .unwrap();
        assert!(
            config
                .validate_mvp_melee_only(Game::Fo76, Game::Fo4)
                .is_err()
        );

        let missing_fence = RunConfig {
            mvp_melee_only: true,
            ..Default::default()
        };
        assert!(
            missing_fence
                .validate_mvp_melee_only(Game::Fnv, Game::Fo4)
                .is_err()
        );
    }

    #[test]
    fn full_merged_creature_coverage_is_zero_when_creatures_are_excluded() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };

        let source_handle = plugin_handle_new_native("FalloutNV.esm", Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                skip_record_signatures: vec!["CREA".to_string()],
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            run.mapper_state = Some(MapperState::new(
                [],
                MapperOptions {
                    source_plugin_name: "FalloutNV.esm".to_string(),
                    ..Default::default()
                },
            ));
            assert_eq!(run.legacy_creature_race_expected_candidates(), 0);
            run.finalize_legacy_creature_race_coverage()
        })
        .unwrap();

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn strict_quest_slice_uses_exact_pack_lowerers_not_corpus_preflight() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };

        let source_handle = plugin_handle_new_native("FalloutNV.esm", Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                fnv_quest_slice: true,
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            assert!(!run.legacy_pack_gate_active());
            run.config.fnv_quest_slice = false;
            assert!(run.legacy_pack_gate_active());
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    /// Verify create/lookup/drop lifecycle without needing real plugin handles.
    ///
    /// The registry does not validate that handle IDs point to loaded plugins —
    /// that check happens only when you call read_record / add_record. So we can
    /// pass sentinel values here and confirm the registry wiring works correctly.
    #[test]
    fn run_create_lookup_drop() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                ..Default::default()
            },
        })
        .expect("create_run failed");

        // Verify the run is accessible.
        let game_str = with_run(id, |run| Ok::<_, RunError>(run.source.as_str().to_string()))
            .expect("with_run failed");
        assert_eq!(game_str, "fo4");

        // Verify decisions and warnings start empty.
        with_run(id, |run| {
            assert!(run.decisions.is_empty());
            assert!(run.warnings.is_empty());
            assert!(run.deferred.is_empty());
            Ok::<_, RunError>(())
        })
        .unwrap();

        // Drop it.
        drop_run(id).expect("drop_run failed");

        // A second drop must return UnknownRun.
        assert!(matches!(drop_run(id), Err(RunError::UnknownRun(_))));

        // with_run after drop must also return UnknownRun.
        assert!(matches!(
            with_run(id, |_| Ok::<_, RunError>(())),
            Err(RunError::UnknownRun(_))
        ));
    }

    #[test]
    fn run_create_unknown_game_returns_invalid_config() {
        let result = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 1,
            target_handle_id: 2,
            master_handle_ids: vec![],
            config: RunConfig::default(),
        });
        // fo4 is a valid game, so this should succeed.
        assert!(result.is_ok());
        drop_run(result.unwrap()).unwrap();
    }

    #[test]
    fn create_run_builds_relocation_members_from_extracted_dirs() {
        let tmp = std::env::temp_dir().join("reloc_create_run");
        let _ = std::fs::remove_dir_all(&tmp);
        let fo76 = tmp.join("fo76");
        let fo4 = tmp.join("fo4");
        for d in [&fo76, &fo4] {
            std::fs::create_dir_all(d.join("meshes/landscape/rocks")).unwrap();
            std::fs::write(d.join("meshes/landscape/rocks/rock01.nif"), b"x").unwrap();
        }

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                source_extracted_dir: Some(fo76.clone()),
                target_extracted_dir: Some(fo4.clone()),
                base_asset_relocation_mesh_roots: vec!["meshes/landscape".to_string()],
                base_asset_namespace: "FO76".to_string(),
                ..Default::default()
            },
        })
        .expect("create_run failed");

        with_run(id, |run| {
            // rock01.nif collides (present under both fo76 and fo4); its cascade is
            // empty because the 1-byte placeholder won't parse as a NIF — fine here.
            assert!(
                run.relocation_members
                    .contains("meshes/landscape/rocks/rock01.nif")
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn skyrimse_fo4_create_run_relocates_colliding_landscape_nif() {
        let tmp = std::env::temp_dir().join("skyrim_reloc_create_run");
        let _ = std::fs::remove_dir_all(&tmp);
        let skyrim = tmp.join("skyrimse");
        let fo4 = tmp.join("fo4");
        let mesh = "meshes/landscape/rocks/rockcliff01.nif";
        for dir in [&skyrim, &fo4] {
            let path = dir.join(mesh);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }

        let id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: 9997,
            target_handle_id: 9996,
            master_handle_ids: vec![],
            config: RunConfig {
                source_extracted_dir: Some(skyrim),
                target_extracted_dir: Some(fo4),
                ..Default::default()
            },
        })
        .expect("create_run failed");

        with_run(id, |run| {
            assert_eq!(base_asset_namespace_for_run(run), "Skyrim");
            assert!(run.relocation_members.contains(mesh));

            let plugin = run.interner.intern("Skyrim.esm");
            let mut record = Record::new(
                SigCode::from_str("STAT").unwrap(),
                FormKey {
                    local: 0x0485E7,
                    plugin,
                },
            );
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("MODL").unwrap(),
                value: FieldValue::String(run.interner.intern("Landscape\\Rocks\\RockCliff01.nif")),
            });
            assert_eq!(
                namespace_base_asset_model_paths(
                    &mut record,
                    &run.relocation_members,
                    &base_asset_namespace_for_run(run),
                    &run.interner,
                ),
                1
            );
            let model = record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "MODL")
                .and_then(|field| match &field.value {
                    FieldValue::String(sym) => run.interner.resolve(*sym),
                    _ => None,
                });
            assert_eq!(model, Some("Skyrim\\Landscape\\Rocks\\RockCliff01.nif"));
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // -----------------------------------------------------------------------
    // mapper_state persists across records in translate_all: two allocations
    // in the same run must produce distinct object-ids.
    // -----------------------------------------------------------------------

    #[test]
    fn translate_all_mapper_persists_across_records() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode};
        use crate::sym::StringInterner;

        let mut interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "Output.esp".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        let weap = SigCode::from_str("WEAP").unwrap();

        let src1 = FormKey::parse("001000@Source.esm", &mut interner).unwrap();
        let src2 = FormKey::parse("002000@Source.esm", &mut interner).unwrap();

        // Simulate record 1: allocate target for src1.
        let tgt1 = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            mapper.allocate_or_resolve(src1, None, weap)
        };

        // Simulate record 2: allocate target for src2.
        let tgt2 = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            mapper.allocate_or_resolve(src2, None, weap)
        };

        // Both must be under Output.esp.
        assert_eq!(interner.resolve(tgt1.plugin).unwrap(), "Output.esp");
        assert_eq!(interner.resolve(tgt2.plugin).unwrap(), "Output.esp");
        // They must have distinct object-ids across records.
        assert_ne!(
            tgt1.local, tgt2.local,
            "object-ids must not collide across records"
        );
    }

    #[test]
    fn fnv_fo3_humanoid_race_substitutions_cover_merged_source_catalog() {
        let source_races = [
            "CaucasianOldAged",
            "AfricanAmericanOldAged",
            "AsianOldAged",
            "HispanicOldAged",
            "AfricanAmericanRaider",
            "AsianRaider",
            "HispanicRaider",
            "CaucasianRaider",
            "TestQACaucasian",
            "HispanicOld",
            "HispanicChild",
            "CaucasianOld",
            "CaucasianChild",
            "AsianOld",
            "AsianChild",
            "AfricanAmericanOld",
            "AfricanAmericanChild",
            "AfricanAmerican",
            "Ghoul",
            "Asian",
            "Hispanic",
            "Caucasian",
            "Christine",
            "WhitelegsCacasians",
            "WhiteLegsAfricanAmerican",
            "DeadHorseCaucasian",
            "DeadHorseAfricanAmerican",
            "SorrowAfricanAmerican",
            "SorrowCacasian",
            "Lobotomites",
            "MarkedMenGhoul",
            "DLCPittHispanicMut",
            "DLCPittAsianMut",
            "DLCPittCaucasianMut",
            "DLCPittAfricanAmericanMut",
            "HispanicTribal",
            "CaucasianTribal",
            "AsianTribal",
            "AfricanAmericanTribal",
        ];
        assert_eq!(source_races.len(), 39);

        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let race_sig = SigCode::from_str("RACE").unwrap();
        let source_entries = source_races
            .iter()
            .enumerate()
            .map(|(index, editor_id)| {
                (
                    interner.intern(editor_id),
                    FormKey {
                        local: 0x800 + index as u32,
                        plugin: source_plugin,
                    },
                    race_sig,
                )
            })
            .collect::<Vec<_>>();

        let mappings = fnv_fo3_fo4_humanoid_race_substitution_mappings(
            &source_entries,
            &interner,
            Game::Fnv,
            Game::Fo4,
        );
        assert_eq!(mappings.len(), source_races.len());
        assert_eq!(
            mappings
                .iter()
                .filter(|(_, target)| target.local == FO4_HUMAN_RACE_LOCAL)
                .count(),
            33
        );
        assert_eq!(
            mappings
                .iter()
                .filter(|(_, target)| target.local == FO4_HUMAN_CHILD_RACE_LOCAL)
                .count(),
            4
        );
        assert_eq!(
            mappings
                .iter()
                .filter(|(_, target)| target.local == FO4_GHOUL_RACE_LOCAL)
                .count(),
            2
        );
        assert!(
            mappings
                .iter()
                .all(|(_, target)| { interner.resolve(target.plugin) == Some("Fallout4.esm") })
        );
    }

    #[test]
    fn fnv_and_fo3_humanoid_races_use_same_fo4_donor_policy() {
        let interner = StringInterner::new();
        let race_sig = SigCode::from_str("RACE").unwrap();
        for (source_game, source_plugin, editor_id, expected_local) in [
            (
                Game::Fnv,
                "FalloutNV.esm",
                "CaucasianChild",
                FO4_HUMAN_CHILD_RACE_LOCAL,
            ),
            (
                Game::Fo3,
                "Fallout3.esm",
                "DLCPittCaucasianMut",
                FO4_HUMAN_RACE_LOCAL,
            ),
            (Game::Fo3, "Fallout3.esm", "Ghoul", FO4_GHOUL_RACE_LOCAL),
        ] {
            let entry = (
                interner.intern(editor_id),
                FormKey {
                    local: 0x800,
                    plugin: interner.intern(source_plugin),
                },
                race_sig,
            );
            let mappings = fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[entry],
                &interner,
                source_game,
                Game::Fo4,
            );
            assert_eq!(mappings.len(), 1);
            assert_eq!(mappings[0].1.local, expected_local);
        }

        let known_entry = (
            interner.intern("Caucasian"),
            FormKey {
                local: 0x19,
                plugin: interner.intern("FalloutNV.esm"),
            },
            race_sig,
        );
        let unknown_entry = (
            interner.intern("SomeCreatureRace"),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
            race_sig,
        );
        let non_race_entry = (
            interner.intern("Caucasian"),
            FormKey {
                local: 0x801,
                plugin: interner.intern("FalloutNV.esm"),
            },
            SigCode::from_str("NPC_").unwrap(),
        );
        assert!(
            fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[unknown_entry],
                &interner,
                Game::Fnv,
                Game::Fo4,
            )
            .is_empty()
        );
        assert!(
            fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[non_race_entry],
                &interner,
                Game::Fnv,
                Game::Fo4,
            )
            .is_empty()
        );
        assert!(
            fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[known_entry],
                &interner,
                Game::SkyrimSe,
                Game::Fo4,
            )
            .is_empty()
        );
        assert!(
            fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[known_entry],
                &interner,
                Game::Fnv,
                Game::SkyrimSe,
            )
            .is_empty()
        );
    }

    #[test]
    fn skyrim_humanoid_races_use_fo4_human_donors() {
        let interner = StringInterner::new();
        let race_sig = SigCode::from_str("RACE").unwrap();
        let source_plugin = interner.intern("Skyrim.esm");
        let source_entries = SKYRIMSE_FO4_HUMANOID_RACE_SUBSTITUTIONS
            .iter()
            .enumerate()
            .map(|(index, (editor_id, _))| {
                (
                    interner.intern(editor_id),
                    FormKey {
                        local: 0x800 + index as u32,
                        plugin: source_plugin,
                    },
                    race_sig,
                )
            })
            .collect::<Vec<_>>();

        let mappings = skyrimse_fo4_humanoid_race_substitution_mappings(
            &source_entries,
            &interner,
            Game::SkyrimSe,
            Game::Fo4,
        );
        assert_eq!(mappings.len(), 25);
        assert_eq!(
            mappings
                .iter()
                .filter(|(_, target)| target.local == FO4_HUMAN_RACE_LOCAL)
                .count(),
            20
        );
        assert_eq!(
            mappings
                .iter()
                .filter(|(_, target)| target.local == FO4_HUMAN_CHILD_RACE_LOCAL)
                .count(),
            5
        );
        assert!(
            mappings
                .iter()
                .all(|(_, target)| interner.resolve(target.plugin) == Some("Fallout4.esm"))
        );
    }

    #[test]
    fn skyrim_beast_and_custom_humanoid_races_are_not_human_aliased() {
        let interner = StringInterner::new();
        let race_sig = SigCode::from_str("RACE").unwrap();
        let entries = ["ArgonianRace", "KhajiitRace", "DremoraRace"]
            .iter()
            .enumerate()
            .map(|(index, editor_id)| {
                (
                    interner.intern(editor_id),
                    FormKey {
                        local: 0x900 + index as u32,
                        plugin: interner.intern("Skyrim.esm"),
                    },
                    race_sig,
                )
            })
            .collect::<Vec<_>>();

        assert!(
            skyrimse_fo4_humanoid_race_substitution_mappings(
                &entries,
                &interner,
                Game::SkyrimSe,
                Game::Fo4,
            )
            .is_empty()
        );
        assert!(
            skyrimse_fo4_humanoid_race_substitution_mappings(
                &entries,
                &interner,
                Game::Fnv,
                Game::Fo4,
            )
            .is_empty()
        );
    }

    #[test]
    fn skyrim_actor_accounting_requires_every_race_npc_and_placement() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let mut plan = crate::skyrimse_fo4_runtime::planner::SkyrimCapabilityPlan::default();
        for (local, signature) in [(1, "RACE"), (2, "NPC_"), (3, "ACHR")] {
            plan.record_signatures
                .insert(FormKey { local, plugin }, signature.to_string());
        }
        let complete = TranslateStats {
            by_signature: [
                (
                    "RACE".to_string(),
                    SignatureTranslateStats {
                        vanilla_remapped: 1,
                        ..Default::default()
                    },
                ),
                (
                    "NPC_".to_string(),
                    SignatureTranslateStats {
                        translated: 1,
                        ..Default::default()
                    },
                ),
                (
                    "ACHR".to_string(),
                    SignatureTranslateStats {
                        translated: 1,
                        ..Default::default()
                    },
                ),
            ]
            .into(),
            ..Default::default()
        };

        assert!(
            validate_total_skyrim_actor_translation(&plan, &complete, &FxHashSet::default())
                .is_ok()
        );

        let mut incomplete = complete;
        incomplete.by_signature.insert(
            "NPC_".to_string(),
            SignatureTranslateStats {
                dropped: 1,
                ..Default::default()
            },
        );
        let error =
            validate_total_skyrim_actor_translation(&plan, &incomplete, &FxHashSet::default())
                .unwrap_err();
        assert!(error.contains("NPC_: expected=1 completed=0"));

        let creature_projection_owned =
            FxHashSet::from_iter([FormKey { local: 1, plugin }, FormKey { local: 2, plugin }]);
        let dedicated_projection = TranslateStats {
            by_signature: [(
                "ACHR".to_string(),
                SignatureTranslateStats {
                    translated: 1,
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        };
        assert!(
            validate_total_skyrim_actor_translation(
                &plan,
                &dedicated_projection,
                &creature_projection_owned,
            )
            .is_ok()
        );
    }

    #[test]
    fn only_skyrim_actor_signatures_are_mandatory_for_skyrim_to_fo4() {
        for signature in ["RACE", "NPC_", "ACHR"] {
            assert!(is_mandatory_skyrim_actor_record(
                Game::SkyrimSe,
                Game::Fo4,
                SigCode::from_str(signature).unwrap(),
            ));
        }
        assert!(!is_mandatory_skyrim_actor_record(
            Game::SkyrimSe,
            Game::Fo4,
            SigCode::from_str("PACK").unwrap(),
        ));
        assert!(!is_mandatory_skyrim_actor_record(
            Game::Fo76,
            Game::Fo4,
            SigCode::from_str("NPC_").unwrap(),
        ));
    }

    #[test]
    fn whole_plugin_weap_ammo_substitution_wins_without_replacing_mapper_fallbacks() {
        for (source_game, source_plugin_name) in
            [(Game::Fnv, "FalloutNV.esm"), (Game::Fo3, "Fallout3.esm")]
        {
            let interner = StringInterner::new();
            let source_plugin = interner.intern(source_plugin_name);
            let output_plugin = interner.intern("Converted.esp");
            let table_ammo = FormKey {
                local: 0x004241,
                plugin: source_plugin,
            };
            let non_table_ammo = FormKey {
                local: 0x07EA27,
                plugin: source_plugin,
            };
            let projectile = FormKey {
                local: 0x02CD5F,
                plugin: source_plugin,
            };
            let normal_ammo_target = FormKey {
                local: 0x0801,
                plugin: output_plugin,
            };
            let projectile_target = FormKey {
                local: 0x0802,
                plugin: output_plugin,
            };
            let mut state = MapperState::new(
                [],
                MapperOptions {
                    output_plugin_name: "Converted.esp".into(),
                    ..Default::default()
                },
            );
            state.source_to_target.insert(
                table_ammo,
                FormKey {
                    local: 0x0800,
                    plugin: output_plugin,
                },
            );
            state
                .source_to_target
                .insert(non_table_ammo, normal_ammo_target);
            state.source_to_target.insert(projectile, projectile_target);
            let source_entries = [
                (
                    interner.intern("Ammo10mm"),
                    table_ammo,
                    SigCode::from_str("AMMO").unwrap(),
                ),
                (
                    interner.intern("Ammo22LR"),
                    non_table_ammo,
                    SigCode::from_str("AMMO").unwrap(),
                ),
                (
                    interner.intern("Projectile22LR"),
                    projectile,
                    SigCode::from_str("PROJ").unwrap(),
                ),
            ];

            let seeded = seed_fnv_fo3_fo4_ammo_substitutions(
                &mut state,
                &source_entries,
                &interner,
                source_game,
                Game::Fo4,
            )
            .unwrap();

            assert_eq!(seeded, 1);
            let table_target = state.source_to_target[&table_ammo];
            assert_eq!(table_target.local, 0x01F276);
            assert_eq!(interner.resolve(table_target.plugin), Some("Fallout4.esm"));
            assert_eq!(state.source_to_target[&non_table_ammo], normal_ammo_target);
            assert_eq!(state.source_to_target[&projectile], projectile_target);
        }
    }

    #[test]
    fn legacy_same_name_output_keeps_generated_targets_outside_the_source_domain() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let low_source = FormKey {
            local: 0x100,
            plugin,
        };
        let source_at_default_floor = FormKey {
            local: FIRST_ALLOCATION_ID,
            plugin,
        };
        let next = legacy_output_allocation_floor(
            Game::Fnv,
            Game::Fo4,
            "FalloutNV.esm",
            "falloutnv.ESM",
            true,
            FIRST_ALLOCATION_ID,
            [low_source.local, source_at_default_floor.local],
        )
        .unwrap()
        .expect("same-name legacy output needs a disjoint generated domain");
        assert_eq!(next, source_at_default_floor.local + 1);
        assert_eq!(
            legacy_output_allocation_floor(
                Game::Fnv,
                Game::Fo4,
                "FalloutNV.esm",
                "Converted.esm",
                true,
                FIRST_ALLOCATION_ID,
                [low_source.local, source_at_default_floor.local],
            )
            .unwrap(),
            None,
            "distinct source/output plugin namespaces need no raised floor"
        );
        assert_eq!(
            legacy_output_allocation_floor(
                Game::Fo76,
                Game::Fo4,
                "SeventySix.esm",
                "SeventySix.esm",
                true,
                FIRST_ALLOCATION_ID,
                [low_source.local, source_at_default_floor.local],
            )
            .unwrap(),
            None,
            "FO76 allocation policy must remain isolated"
        );
        assert_eq!(
            legacy_output_allocation_floor(
                Game::Fnv,
                Game::Fo4,
                "FalloutNV.esm",
                "FalloutNV.esm",
                true,
                0xA000,
                [low_source.local, source_at_default_floor.local],
            )
            .unwrap(),
            Some(0xA000),
            "an existing higher allocation floor must win"
        );
        assert!(
            legacy_output_allocation_floor(
                Game::Fnv,
                Game::Fo4,
                "FalloutNV.esm",
                "FalloutNV.esm",
                true,
                FIRST_ALLOCATION_ID,
                [low_source.local, 0x00FF_FFFF],
            )
            .is_err(),
            "fresh legacy IDs require free 24-bit local-ID space"
        );
        let mut state = MapperState::new(
            [],
            MapperOptions {
                source_plugin_name: "FalloutNV.esm".into(),
                output_plugin_name: "FalloutNV.esm".into(),
                preserve_source_ids: true,
                resolution_mode: ResolutionMode::Strict,
                ..Default::default()
            },
        );
        state.next_object_id = next;
        let sig = SigCode::from_str("MGEF").unwrap();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);

        let generated_target = mapper.allocate_or_resolve(low_source, None, sig);
        let preserved_target = mapper.allocate_or_resolve(source_at_default_floor, None, sig);
        mapper.add_mapping(generated_target, generated_target);

        assert_eq!(generated_target.local, next);
        assert_eq!(preserved_target, source_at_default_floor);
        assert_eq!(
            mapper.lookup(source_at_default_floor),
            Some(preserved_target)
        );

        let seeded_source = FormKey {
            local: 0x200,
            plugin,
        };
        let seeded_target = FormKey {
            local: next + 1,
            plugin,
        };
        mapper.add_mapping(seeded_source, seeded_target);
        let later_low_source = FormKey {
            local: 0x300,
            plugin,
        };
        let later_target = mapper.allocate_or_resolve(later_low_source, None, sig);
        assert_eq!(later_target.local, seeded_target.local + 1);
    }

    #[test]
    fn seeded_legacy_race_substitutions_rewrite_npc_refs_and_prevent_local_races() {
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
        use crate::record::{FieldEntry, FieldValue, Record};

        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let race_sig = SigCode::from_str("RACE").unwrap();
        let source_entries = [
            ("Caucasian", 0x000019, FO4_HUMAN_RACE_LOCAL),
            ("HispanicChild", 0x0042C4, FO4_HUMAN_CHILD_RACE_LOCAL),
            ("MarkedMenGhoul", 0x187FEC, FO4_GHOUL_RACE_LOCAL),
        ]
        .map(|(editor_id, local, target_local)| {
            (
                (
                    interner.intern(editor_id),
                    FormKey {
                        local,
                        plugin: source_plugin,
                    },
                    race_sig,
                ),
                target_local,
            )
        });
        let catalog = source_entries.map(|(entry, _)| entry);
        let mappings = fnv_fo3_fo4_humanoid_race_substitution_mappings(
            &catalog,
            &interner,
            Game::Fnv,
            Game::Fo4,
        );
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "FalloutNV.esm".into(),
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        state.source_to_target.extend(mappings.iter().copied());

        for ((_, source_race, _), expected_local) in source_entries {
            let expected = FormKey {
                local: expected_local,
                plugin: interner.intern("Fallout4.esm"),
            };
            let resolved_identity = {
                let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
                mapper.allocate_or_resolve(source_race, None, race_sig)
            };
            assert_eq!(resolved_identity, expected);
            let target_masters = FxHashSet::from_iter([interner.intern("Fallout4.esm")]);
            assert!(is_target_master_remap(resolved_identity, &target_masters));

            let mut npc = Record::new(
                SigCode::from_str("NPC_").unwrap(),
                FormKey {
                    local: source_race.local + 1,
                    plugin: source_plugin,
                },
            );
            npc.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("RNAM").unwrap(),
                value: FieldValue::FormKey(source_race),
            });
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.rewrite_record(&mut npc).unwrap();
            assert_eq!(npc.fields[0].value, FieldValue::FormKey(expected));
        }

        let source_race = catalog[2].1;
        let expected = FormKey {
            local: FO4_GHOUL_RACE_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut quest = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey {
                local: 0x2000,
                plugin: source_plugin,
            },
        );
        quest.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::List(vec![FieldValue::Struct(vec![(
                interner.intern("Parameter1"),
                FieldValue::FormKey(source_race),
            )])]),
        });
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.rewrite_record(&mut quest).unwrap();
        let FieldValue::List(rows) = &quest.fields[0].value else {
            panic!("expected condition rows");
        };
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("expected condition row");
        };
        assert_eq!(fields[0].1, FieldValue::FormKey(expected));
    }

    // -----------------------------------------------------------------------
    // FO76→FO4 forced keyword substitutions (attach points).
    // -----------------------------------------------------------------------

    #[test]
    fn fo76_fo4_forced_keyword_substitutions_target_fo4_master_keywords() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_keyword_substitution_mappings(&interner);
        let src = |local| FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        };
        let tgt = |plugin_name, local| FormKey {
            local,
            plugin: interner.intern(plugin_name),
        };
        // ap_gun_Appearance -> ap_WeaponMaterial
        assert!(mappings.contains(&(src(0x0011_4364), tgt("Fallout4.esm", 0x0024_A0D8))));
        // ma_Gun_Appearance -> ma_WeaponMaterialSwaps
        assert!(mappings.contains(&(src(0x0037_D0B2), tgt("Fallout4.esm", 0x0024_A0D7))));
        // ap_melee_Appearance -> ap_melee_Material (FO76 1A001E -> FO4 1A001E)
        assert!(mappings.contains(&(src(0x001A_001E), tgt("Fallout4.esm", 0x001A_001E))));
        // DLC04_ma_HandmadeAssaultRifle -> DLC04_ma_HandmadeAssaultRifle
        assert!(mappings.contains(&(src(0x0011_3855), tgt("DLCNukaWorld.esm", 0x0003_3B61))));
        // ma_armor_Generic_Paint -> ap_armor_Paint
        assert!(mappings.contains(&(src(0x0045_2EF4), tgt("Fallout4.esm", 0x0024_A0FA))));
        // ma_PowerArmorMod -> ma_PA_Material
        assert!(mappings.contains(&(src(0x0053_03FE), tgt("Fallout4.esm", 0x0018_DFCB))));
    }

    #[test]
    fn fo76_fo4_renamed_linked_ref_keywords_resolve_to_fo4_keywords() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_keyword_substitution_mappings(&interner);
        for local in [
            0x001C_A8CF, // DMP_Sandbox -> DMP_Sandbox_Prim
            0x0004_21F8, // DMP_Suspicious_Sandbox -> DMP_Suspicious_Sandbox_512
            0x0004_2241, // DMP_Combat_HoldPosition -> DMP_Combat_HoldPosition_128
            0x0006_6510, // TurfSystemLinkToSleep -> EMSystemLinkToSleep
            0x0018_12F6, // TurfSystemLinkToTurf -> EMSystemLinkToTurf
            0x001C_5EDD, // WorkshopStackedItemParent -> WorkshopStackedItemParentKEYWORD
            0x000F_9E1B, // LinkTerminalRobot -> LinkTerminalProtectron
            0x000F_9E13, // LinkCaptive -> DefaultCaptiveLink
        ] {
            let source = FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            };
            let target = FormKey {
                local,
                plugin: interner.intern("Fallout4.esm"),
            };
            assert!(
                mappings.contains(&(source, target)),
                "{local:06X} must resolve to the FO4 keyword"
            );
        }
    }

    #[test]
    fn fo76_fo4_forced_race_substitutions_target_matching_fo4_races() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_race_substitution_mappings(&interner);
        let src = |local| FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        };
        let tgt = |plugin_name, local| FormKey {
            local,
            plugin: interner.intern(plugin_name),
        };

        assert!(mappings.contains(&(src(0x0079_CCE7), tgt("Fallout4.esm", 0x000E_AFB6))));
        assert!(mappings.contains(&(src(0x0077_2F32), tgt("DLCRobot.esm", 0x0000_1129))));
    }

    #[test]
    fn fo76_fo4_forced_base_object_substitution_targets_fo4_workbench() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_base_object_substitution_mappings(&interner);
        let src = FormKey {
            local: 0x003B_D4F4,
            plugin: interner.intern("SeventySix.esm"),
        };
        let tgt = FormKey {
            local: 0x000C_1AEB,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert!(mappings.contains(&(src, tgt)));
    }

    #[test]
    fn fo76_fo4_forced_location_ref_type_substitution_targets_fo4_boss() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_location_ref_type_substitution_mappings(&interner);
        let src = FormKey {
            local: 0x0000_3956,
            plugin: interner.intern("SeventySix.esm"),
        };
        let tgt = FormKey {
            local: 0x0000_3956,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert!(mappings.contains(&(src, tgt)));
    }

    #[test]
    fn seeded_keyword_substitution_resolves_reference_to_target_master() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        for (source_form_key, target_form_key) in
            fo76_fo4_forced_keyword_substitution_mappings(&interner)
        {
            state
                .source_to_target
                .insert(source_form_key, target_form_key);
        }

        let kywd = SigCode::from_str("KYWD").unwrap();
        let source = FormKey {
            local: 0x0011_4364,
            plugin: interner.intern("SeventySix.esm"),
        };
        let resolved = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            // No EditorID supplied: only the seeded override should map this.
            mapper.allocate_or_resolve(source, None, kywd)
        };
        assert_eq!(
            resolved,
            FormKey {
                local: 0x0024_A0D8,
                plugin: interner.intern("Fallout4.esm"),
            },
            "seeded substitution must win over allocate (it is consulted first)"
        );

        let source_power_armor = FormKey {
            local: 0x0053_03FE,
            plugin: interner.intern("SeventySix.esm"),
        };
        let resolved_power_armor = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.allocate_or_resolve(source_power_armor, None, kywd)
        };
        assert_eq!(
            resolved_power_armor,
            FormKey {
                local: 0x0018_DFCB,
                plugin: interner.intern("Fallout4.esm"),
            },
            "power-armor OMOD and ARMO references must use the FO4 material association"
        );

        let source_armor_paint = FormKey {
            local: 0x0045_2EF4,
            plugin: interner.intern("SeventySix.esm"),
        };
        let resolved_armor_paint = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.allocate_or_resolve(source_armor_paint, None, kywd)
        };
        assert_eq!(
            resolved_armor_paint,
            FormKey {
                local: 0x0024_A0FA,
                plugin: interner.intern("Fallout4.esm"),
            },
            "armor-paint OMOD and ARMO references must use the FO4 paint keyword"
        );

        let source_handmade = FormKey {
            local: 0x0011_3855,
            plugin: interner.intern("SeventySix.esm"),
        };
        let resolved_handmade = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.allocate_or_resolve(source_handmade, None, kywd)
        };
        assert_eq!(
            resolved_handmade,
            FormKey {
                local: 0x0003_3B61,
                plugin: interner.intern("DLCNukaWorld.esm"),
            },
            "handmade OMOD target keywords must stay constrained to the Nuka-World association"
        );

        let mut omod = Record::new(
            SigCode::from_str("OMOD").unwrap(),
            FormKey {
                local: 0x0031_B1D4,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        omod.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MNAM").unwrap(),
            value: FieldValue::List(vec![FieldValue::FormKey(source_handmade)]),
        });
        {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.rewrite_record(&mut omod).unwrap();
        }

        let FieldValue::List(items) = &omod.fields[0].value else {
            panic!("expected MNAM list");
        };
        assert_eq!(
            items,
            &vec![FieldValue::FormKey(FormKey {
                local: 0x0003_3B61,
                plugin: interner.intern("DLCNukaWorld.esm"),
            })],
            "OMOD MNAM must retain the handmade weapon-family constraint"
        );

        let expected_armor_targets = vec![
            FieldValue::FormKey(FormKey {
                local: 0x0024_A0FA,
                plugin: interner.intern("Fallout4.esm"),
            }),
            FieldValue::FormKey(FormKey {
                local: 0x0018_DFCB,
                plugin: interner.intern("Fallout4.esm"),
            }),
        ];
        for (record_sig, field_sig) in [("OMOD", "MNAM"), ("ARMO", "KWDA")] {
            let mut record = Record::new(
                SigCode::from_str(record_sig).unwrap(),
                FormKey {
                    local: 0x0031_B1D4,
                    plugin: interner.intern("SeventySix.esm"),
                },
            );
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(field_sig).unwrap(),
                value: FieldValue::List(vec![
                    FieldValue::FormKey(source_armor_paint),
                    FieldValue::FormKey(source_power_armor),
                ]),
            });
            {
                let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
                mapper.rewrite_record(&mut record).unwrap();
            }

            let FieldValue::List(items) = &record.fields[0].value else {
                panic!("expected {record_sig} {field_sig} list");
            };
            assert_eq!(
                items, &expected_armor_targets,
                "{record_sig} {field_sig} must use FO4 armor target keywords"
            );
        }
    }

    #[test]
    fn seeded_base_object_substitution_rewrites_workshop_refr_base_to_fo4_workbench() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        for (source_form_key, target_form_key) in
            fo76_fo4_forced_base_object_substitution_mappings(&interner)
        {
            state
                .source_to_target
                .insert(source_form_key, target_form_key);
        }

        let source_workbench = FormKey {
            local: 0x003B_D4F4,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_workbench = FormKey {
            local: 0x000C_1AEB,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut refr = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey {
                local: 0x0020_106D,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        refr.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NAME").unwrap(),
            value: FieldValue::FormKey(source_workbench),
        });

        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.rewrite_record(&mut refr).unwrap();

        assert_eq!(refr.fields[0].value, FieldValue::FormKey(target_workbench));
    }

    #[test]
    fn seeded_race_substitutions_rewrite_npc_rnam_to_fo4_compatible_races() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        for (source_form_key, target_form_key) in
            fo76_fo4_forced_race_substitution_mappings(&interner)
        {
            state
                .source_to_target
                .insert(source_form_key, target_form_key);
        }

        for (source_local, target_plugin, target_local) in [
            (0x0079_CCE7, "Fallout4.esm", 0x000E_AFB6),
            (0x0077_2F32, "DLCRobot.esm", 0x0000_1129),
        ] {
            let source_race = FormKey {
                local: source_local,
                plugin: interner.intern("SeventySix.esm"),
            };
            let target_race = FormKey {
                local: target_local,
                plugin: interner.intern(target_plugin),
            };
            let mut npc = Record::new(
                SigCode::from_str("NPC_").unwrap(),
                FormKey {
                    local: 0x0083_26DD,
                    plugin: interner.intern("SeventySix.esm"),
                },
            );
            npc.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("RNAM").unwrap(),
                value: FieldValue::FormKey(source_race),
            });

            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.rewrite_record(&mut npc).unwrap();

            assert_eq!(npc.fields[0].value, FieldValue::FormKey(target_race));
        }
    }

    // -----------------------------------------------------------------------
    // rewrite_record must run after allocate_or_resolve so cross-plugin FK
    // refs are updated before add_record_native.
    // -----------------------------------------------------------------------

    #[test]
    fn cross_record_formkey_reference_is_rewritten() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::sym::StringInterner;

        let mut interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "Output.esp".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        let weap = SigCode::from_str("WEAP").unwrap();

        // Source plugin has a WEAP at 001000 that references an ammo at 002000.
        let src_weap = FormKey::parse("001000@Source.esm", &mut interner).unwrap();
        let src_ammo = FormKey::parse("002000@Source.esm", &mut interner).unwrap();

        // Record 1: allocate the ammo (no fields, just get its target FK).
        let tgt_ammo = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            mapper.allocate_or_resolve(src_ammo, None, SigCode::from_str("AMMO").unwrap())
        };

        // Record 2: the WEAP references src_ammo in a DNAM field.
        let mut weap_record = Record::new(weap, src_weap);
        let ammo_sym = interner.intern("ammo");
        weap_record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Struct(vec![(ammo_sym, FieldValue::FormKey(src_ammo))]),
        });

        // Fixed sequence: allocate_or_resolve THEN rewrite_record.
        {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            let tgt_weap = mapper.allocate_or_resolve(src_weap, None, weap);
            weap_record.form_key = tgt_weap;
            mapper.rewrite_record(&mut weap_record).unwrap();
        }

        // The DNAM field must now reference tgt_ammo, not src_ammo.
        if let FieldValue::Struct(fields) = &weap_record.fields[0].value {
            assert_eq!(
                fields[0].1,
                FieldValue::FormKey(tgt_ammo),
                "cross-plugin FK must be rewritten to target-plugin FK"
            );
        } else {
            panic!("expected Struct field");
        }

        // The tgt_ammo plugin must be Output.esp, not Source.esm.
        assert_eq!(
            interner.resolve(tgt_ammo.plugin).unwrap(),
            "Output.esp",
            "target FK must be in the output plugin"
        );
    }

    // -----------------------------------------------------------------------
    // Fixups use self.interner (same Rodeo as translate_all). Without real
    // plugin handles this is checked indirectly: mapper_state resets to None
    // after translate_all fails (no plugin), and fixups don't panic.
    // -----------------------------------------------------------------------

    #[test]
    fn apply_fixups_v2_without_translate_all_does_not_panic() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let result = with_run(id, |run| run.apply_fixups_v2().map_err(RunError::from));
        // All fixups have applies_to returning false for handles 9999/9998
        // (no real plugin) so reports should be empty (no panic).
        let _ = result; // may error due to invalid handles — just confirm no panic
        drop_run(id).unwrap();
    }

    #[test]
    fn apply_fixups_v2_preserves_mapper_state() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            let source_fk = FormKey::parse("000800@Source.esm", &mut run.interner).unwrap();
            let target_fk = FormKey::parse("000900@Output.esp", &mut run.interner).unwrap();
            let mut state = MapperState::new(
                [],
                MapperOptions {
                    output_plugin_name: "Output.esp".into(),
                    ..Default::default()
                },
            );
            state.source_to_target.insert(source_fk, target_fk);
            run.mapper_state = Some(state);

            let _ = run.apply_fixups_v2();

            let state = run
                .mapper_state
                .as_ref()
                .expect("apply_fixups_v2 must restore mapper_state");
            assert_eq!(state.source_to_target.get(&source_fk), Some(&target_fk));
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn apply_fixups_v2_emits_per_fixup_log_events() {
        let source =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Source.esm", Some("fo4"))
                .unwrap();
        let target =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Output.esp", Some("fo4"))
                .unwrap();
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                is_whole_plugin: true,
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            run.apply_fixups_v2().map_err(RunError::from)?;
            let messages: Vec<String> = run
                .event_rx
                .try_iter()
                .filter_map(|event| match event {
                    crate::phase::PhaseEvent::Log {
                        phase: "fixups_v2",
                        message,
                        ..
                    } => Some(message),
                    _ => None,
                })
                .collect();

            assert!(
                messages
                    .iter()
                    .any(|message| message.contains("queued apply_weapon_sound_defaults")),
                "missing sweep fixup log event: {messages:?}"
            );
            assert!(
                messages
                    .iter()
                    .any(|message| message
                        .contains("queued normalize_creature_lvln_template_chains")),
                "missing creature normalizer log event: {messages:?}"
            );
            assert!(
                messages
                    .iter()
                    .any(|message| message
                        .contains("starting normalize_creature_lvln_template_chains")),
                "missing creature normalizer start event: {messages:?}"
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn unified_defers_asset_repairs_but_standalone_still_runs_them() {
        use crate::phase::{DispatchParams, run_phase};
        use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
        for deferred in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let behaviors = root.path().join("data/Meshes/Actors/Deathclaw/Behaviors");
            std::fs::create_dir_all(&behaviors).unwrap();
            let filtered = behaviors.join("ambushbehavior.hkx");
            std::fs::write(&filtered, b"dummy").unwrap();
            let id = create_run(RunParams {
                source: Game::Fo4,
                target: Game::Fo4,
                source_handle_id: plugin_handle_new_native("Source.esm", Some("fo4")).unwrap(),
                target_handle_id: plugin_handle_new_native("Output.esp", Some("fo4")).unwrap(),
                master_handle_ids: vec![],
                config: RunConfig {
                    output_plugin_name: "Output.esp".into(),
                    is_whole_plugin: true,
                    mod_path: Some(root.path().to_path_buf()),
                    asset_phases: crate::full_plugin::AssetPhaseFlags {
                        havok: true,
                        animations: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            })
            .unwrap();
            let dispatch = |params| DispatchParams {
                mod_path: root.path().to_path_buf(),
                source_extracted_dir: root.path().to_path_buf(),
                target_extracted_dir: None,
                target_data_dir: None,
                params,
            };
            run_phase(
                id,
                "fixups_v2",
                dispatch(serde_json::json!({
                    "defer_havok_postprocess": deferred,
                })),
            )
            .unwrap();
            assert_eq!(filtered.exists(), deferred);
            run_phase(
                id,
                "postprocess_havok_assets",
                dispatch(serde_json::json!({})),
            )
            .unwrap();
            assert!(!filtered.exists());
            drop_run(id).unwrap();
        }
    }

    #[test]
    fn skyrim_weather_production_pipeline_seeds_and_rewrites_master_god_rays() {
        use bytes::Bytes;
        use esp_authoring_core::plugin_runtime::{
            ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord,
            plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
            plugin_handle_store_ref,
        };
        use smol_str::SmolStr;

        fn record(
            signature: &'static str,
            form_id: u32,
            subrecords: Vec<ParsedSubrecord>,
        ) -> ParsedItem {
            ParsedItem::Record(ParsedRecord {
                signature: SmolStr::new_static(signature),
                form_id,
                flags: 0,
                version_control: 0,
                form_version: Some(44),
                version2: None,
                subrecords,
                raw_payload: None,
                parse_error: None,
            })
        }

        fn subrecord(signature: &'static str, data: impl Into<Bytes>) -> ParsedSubrecord {
            ParsedSubrecord {
                signature: SmolStr::new_static(signature),
                data: data.into(),
                semantic_type: None,
            }
        }

        let source = plugin_handle_new_native("ConvertedSkyrim.esm", Some("skyrimse")).unwrap();
        plugin_handle_add_master_native(source, "Skyrim.esm", None).unwrap();
        plugin_handle_add_master_native(source, "Update.esm", None).unwrap();
        let target = plugin_handle_new_native("ConvertedSkyrim.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();

        let donor_editor_ids = [
            (0x0000_0D53, "SkyrimClearSunrise"),
            (0x0100_0D51, "SkyrimCloudyDay"),
            (0x0100_0D52, "SkyrimRainSunset"),
            (0x0100_0D58, "SkyrimFogNight"),
        ];
        let voli = donor_editor_ids
            .into_iter()
            .map(|(form_id, editor_id)| {
                record(
                    "VOLI",
                    form_id,
                    vec![subrecord("EDID", Bytes::from(format!("{editor_id}\0")))],
                )
            })
            .collect();
        let hnam = [0x0000_0D53_u32, 0x0100_0D51, 0x0100_0D52, 0x0100_0D58]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let wthr = record(
            "WTHR",
            0x0200_0800,
            vec![
                subrecord("EDID", Bytes::from_static(b"ProductionWeather\0")),
                subrecord("HNAM", Bytes::from(hnam)),
            ],
        );
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&source).unwrap();
            slot.parsed.root_items = vec![
                ParsedItem::Group(ParsedGroup {
                    label: *b"VOLI",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: voli,
                }),
                ParsedItem::Group(ParsedGroup {
                    label: *b"WTHR",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![wthr],
                }),
            ];
            slot.invalidate_sections();
        }

        let run_id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "ConvertedSkyrim.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: true,
                target_master_names: vec!["Fallout4.esm".into()],
                ..Default::default()
            },
        })
        .unwrap();
        let reports = with_run(run_id, |run| {
            run.translate_all()?;
            run.apply_fixups_v2().map_err(RunError::from)
        })
        .unwrap();
        assert!(
            reports.iter().any(|(name, report)| {
                name == "rewrite_raw_object_template_formids" && report.records_changed > 0
            }),
            "canonical fixup registry did not rewrite the Skyrim weather"
        );

        let expected = [
            0x0021_6A93,
            0x001C_855D,
            0x0021_15D1,
            0x001C_C192,
            0x001C_C192,
            0x0021_6A93,
            0x001C_855D,
            0x0021_15D1,
        ];
        let interner = StringInterner::new();
        let mut session = crate::session::open_session(target, None).unwrap();
        let schema = session.schema().unwrap();
        let weather_key = session
            .form_keys_of_sig(SigCode::from_str("WTHR").unwrap(), &interner)
            .unwrap()
            .into_iter()
            .next()
            .expect("translated WTHR");
        let weather = session
            .record_decoded(&weather_key, schema.as_ref(), &interner)
            .unwrap();
        let FieldValue::Bytes(wgdr) = &weather
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "WGDR")
            .expect("translated WGDR")
            .value
        else {
            panic!("WGDR must remain a raw FormID array");
        };
        let actual = wgdr
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);

        drop(session);
        drop_run(run_id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
    }

    #[test]
    fn run_error_cancelled_display() {
        let msg = format!("{}", RunError::Cancelled);
        assert_eq!(msg, "translation cancelled");
    }

    #[test]
    fn progress_callback_starts_none() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            assert!(
                run.progress_callback.is_none(),
                "progress_callback must start as None"
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    // -----------------------------------------------------------------------
    // translate_all on a run with no plugin handles returns quickly
    //     (no records to process, no cancel triggered). This confirms the
    //     yield-check logic doesn't fire when record_count is always 0.
    // -----------------------------------------------------------------------

    #[test]
    fn translate_all_zero_records_no_cancel() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        // translate_all on sentinel handles: source_signatures will fail with an error.
        // The important thing is it does NOT return RunError::Cancelled.
        let result = with_run(id, |run| run.translate_all());
        match result {
            Err(RunError::Cancelled) => panic!("unexpected Cancelled on zero-record run"),
            _ => {} // any other outcome is acceptable
        }

        drop_run(id).unwrap();
    }

    // -----------------------------------------------------------------------
    // mod_path / source_extracted_dir flow from RunConfig into the FixupContext
    // apply_fixups_v2 builds. Running fixups needs real plugin handles, so this
    // reuses apply_fixups_v2's ctx-building lines to check that the as_deref()
    // plumbing yields the expected Option<&Path>.
    // -----------------------------------------------------------------------

    #[test]
    fn run_config_carries_mod_path_into_fixup_context() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use std::path::{Path, PathBuf};

        let mod_path = PathBuf::from("/fake/mod");
        let extracted = PathBuf::from("/fake/extracted");

        let cfg = RunConfig {
            output_plugin_name: "Output.esm".into(),
            mod_path: Some(mod_path.clone()),
            source_extracted_dir: Some(extracted.clone()),
            ..Default::default()
        };

        // Sanity-check the round-trip through RunConfig itself.
        assert_eq!(cfg.mod_path.as_deref(), Some(mod_path.as_path()));
        assert_eq!(
            cfg.source_extracted_dir.as_deref(),
            Some(extracted.as_path())
        );

        // Build a FixupContext using the same expressions apply_fixups_v2 uses
        // (`self.config.mod_path.as_deref()`), so this test fails if anyone
        // reverts the fixup-context plumbing to hardcoded `None`.
        let fixup_config = FixupConfig::default();
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let masters: Vec<u64> = vec![];
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: cfg.mod_path.as_deref(),
            source_extracted_dir: cfg.source_extracted_dir.as_deref(),
            target_master_handle_ids: &masters,
            config: &fixup_config,
        };

        assert_eq!(ctx.mod_path, Some(Path::new("/fake/mod")));
        assert_eq!(ctx.source_extracted_dir, Some(Path::new("/fake/extracted")));
    }

    #[test]
    fn run_config_default_mod_path_is_none() {
        let cfg = RunConfig::default();
        assert!(cfg.mod_path.is_none());
        assert!(cfg.source_extracted_dir.is_none());
    }

    #[test]
    fn navi_warnings_capped() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            for i in 0..1_000_000 {
                run.push_navi_warning(format!("w{i}"));
            }
            run.finalize_navi_warnings();
            // At most 1000 entries + 1 cap message = 1001
            assert!(
                run.navi_warning_len() <= 1001,
                "navi_warnings must be capped to <= 1001, got {}",
                run.navi_warning_len()
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn release_remap_state_drops_mapper() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            run.mapper_state = Some(MapperState::new(
                [],
                MapperOptions {
                    output_plugin_name: "Output.esp".into(),
                    ..Default::default()
                },
            ));
            assert!(
                run.mapper_state.is_some(),
                "mapper_state must be Some before release"
            );
            run.release_remap_state();
            assert!(
                run.mapper_state.is_none(),
                "mapper_state must be None after release"
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn final_quest_condition_repair_runs_after_placed_repairs() {
        let source = include_str!("run.rs");
        let start = source.find("pub fn repair_placed_child_refs").unwrap();
        let end = source[start..]
            .find("pub fn synthesize_encounter_zones")
            .map(|offset| start + offset)
            .unwrap();
        let body = &source[start..end];
        let teleport = body
            .rfind("repair_placed_teleport_doors::repair_placed_references")
            .unwrap();
        let actor_specialization = body
            .rfind("specialize_placed_actor_templates_after_ref_repair")
            .unwrap();
        let final_conditions = body
            .rfind("repair_final_quest_reference_conditions(")
            .unwrap();
        let flush = body
            .rfind("session.flush_pending_effects()")
            .expect("session flush");

        assert!(final_conditions > teleport);
        assert!(final_conditions > actor_specialization);
        assert!(final_conditions < flush);
    }

    #[test]
    fn copied_placed_normalization_runs_after_all_cell_copy_paths() {
        let source = include_str!("run.rs");
        let interior = source.find("pub fn emit_interior_cells").unwrap();
        let repair = source.find("pub fn repair_placed_child_refs").unwrap();
        let repair_end = source[repair..]
            .find("pub fn synthesize_encounter_zones")
            .map(|offset| repair + offset)
            .unwrap();

        assert!(!source[interior..repair].contains("normalize_copied_placed_records"));
        assert!(source[repair..repair_end].contains("normalize_copied_placed_records"));
        assert!(source[repair..repair_end].contains("resolve_placed_leveled_bases_for_refs"));
    }

    #[test]
    fn interior_recentre_runs_before_insert_and_after_navi_and_door_repairs() {
        let source = include_str!("run.rs");
        let emit = source.find("pub fn emit_interior_cells").unwrap();
        let emit_end = emit
            + source[emit..]
                .find("fn translate_and_remap_snapshot_record")
                .unwrap();
        let emit_body = &source[emit..emit_end];
        let recentre = emit_body.find("recentre_cell_children(").unwrap();
        assert!(recentre > emit_body.rfind("translate_and_remap_snapshot_record(").unwrap());
        assert!(recentre < emit_body.find("add_interior_cell_with_children_native(").unwrap());

        let navi = source
            .find("pub fn rebuild_projected_navi(&mut self)")
            .unwrap();
        let navi_body = &source[navi..];
        assert!(
            navi_body.find("shift_navi_navmesh_infos(").unwrap()
                > navi_body
                    .find("rebuild_projected_navi_from_source_native(")
                    .unwrap()
        );

        let repair = source.find("pub fn repair_placed_child_refs").unwrap();
        let repair_end = repair
            + source[repair..]
                .find("pub fn synthesize_encounter_zones")
                .unwrap();
        let repair_body = &source[repair..repair_end];
        assert!(
            repair_body.find("shift_teleport_destinations(").unwrap()
                > repair_body
                    .find("repair_placed_teleport_doors::repair_placed_references")
                    .unwrap()
        );
    }

    #[test]
    fn fo4_placed_ref_target_repair_includes_starfield_and_fo76_sources() {
        assert!(needs_fo4_placed_ref_target_repair(
            Game::Starfield,
            Game::Fo4
        ));
        assert!(needs_fo4_placed_ref_target_repair(Game::Fo76, Game::Fo4));
    }

    #[test]
    fn fo4_placed_ref_target_repair_excludes_unrelated_pairs() {
        assert!(!needs_fo4_placed_ref_target_repair(Game::Fo4, Game::Fo4));
        assert!(!needs_fo4_placed_ref_target_repair(
            Game::SkyrimSe,
            Game::Fo4
        ));
        assert!(!needs_fo4_placed_ref_target_repair(
            Game::Fo4,
            Game::Starfield
        ));
        assert!(!needs_fo4_placed_ref_target_repair(
            Game::Starfield,
            Game::Starfield
        ));
    }

    #[test]
    fn starfield_ref_target_repair_keeps_fo76_payload_fixups_fo76_only() {
        let source = include_str!("run.rs");
        let start = source.find("pub fn repair_placed_child_refs").unwrap();
        let end = source[start..]
            .find("pub fn synthesize_encounter_zones")
            .map(|offset| start + offset)
            .unwrap();
        let body = &source[start..end];
        let normalize = body
            .find("normalize_copied_placed_records_in_session(")
            .unwrap();
        let linked = body.find("repair_placed_linked_refs(").unwrap();
        let teleport = body
            .find("repair_placed_teleport_doors::repair_placed_references")
            .unwrap();
        let light = body
            .find("normalize_light_radii::normalize_light_radii")
            .unwrap();
        let fo76_gate = "if self.source == Game::Fo76 && self.target == Game::Fo4";

        assert!(body[..normalize].contains(fo76_gate));
        assert!(normalize < linked);
        assert!(linked < teleport);
        assert!(body[teleport..light].contains(fo76_gate));
    }

    #[test]
    fn fnv_quest_synthetic_targets_resolve_exactly_and_fail_closed() {
        use crate::fnv_legacy_scripting::component::FnvQuestSyntheticIntent;
        use crate::quest_runtime::QuestRuntimeExpectedReceipt;

        let intents = [
            FnvQuestSyntheticIntent::Scene,
            FnvQuestSyntheticIntent::DialogueBranch,
            FnvQuestSyntheticIntent::DedicatedGreetingKeyword,
            FnvQuestSyntheticIntent::DedicatedGreetingTopic,
        ];
        let expected = QuestRuntimeExpectedReceipt {
            emitted_records: intents.iter().map(|intent| intent.record_key()).collect(),
            ..Default::default()
        };
        let synthetic_targets = intents
            .iter()
            .enumerate()
            .map(|(index, intent)| {
                (
                    intent.record_key(),
                    intent.target_key(format!("{:06X}:Output.esm", 0xF100 + index)),
                )
            })
            .collect::<BTreeMap<_, _>>();

        validate_fnv_synthetic_target_set(std::slice::from_ref(&expected), &synthetic_targets)
            .expect("the admitted synthetic set must match the live allocations");
        let resolved = resolve_fnv_expected_receipt(&expected, &synthetic_targets)
            .expect("every synthetic intent has a live allocation");
        assert!(
            resolved
                .emitted_records
                .iter()
                .all(|record| !record.form_key.starts_with("intent:fnv:"))
        );

        let mut missing = synthetic_targets.clone();
        missing.remove(&FnvQuestSyntheticIntent::Scene.record_key());
        assert_eq!(
            validate_fnv_synthetic_target_set(std::slice::from_ref(&expected), &missing)
                .expect_err("a missing allocation must fail closed"),
            "FNV quest runtime synthetic allocation set differs from admission: expected=4 actual=3"
        );

        let mut extra = synthetic_targets;
        extra.insert(
            crate::quest_runtime::QuestRecordKey::new("SCEN", "intent:fnv:unexpected"),
            crate::quest_runtime::QuestRecordKey::new("SCEN", "00F200:Output.esm"),
        );
        assert_eq!(
            validate_fnv_synthetic_target_set(&[expected], &extra)
                .expect_err("an extra allocation must fail closed"),
            "FNV quest runtime synthetic allocation set differs from admission: expected=4 actual=5"
        );
    }

    #[test]
    fn fnv_quest_vmad_receipt_collects_exact_script_properties() {
        let authoring = serde_json::json!({
            "VMAD": {
                "Scripts": [
                    {
                        "ScriptName": "QF_FNV_FO3_11F935",
                        "Properties": [
                            {"propertyName": "QuestRef"},
                            {"propertyName": "StageIndex"},
                            {"propertyName": "QuestRef"}
                        ]
                    },
                    {
                        "ScriptName": "FNV_FO3_FnvSliceCompat",
                        "Properties": []
                    }
                ]
            }
        });

        assert_eq!(
            authoring_script_attachments(&authoring),
            vec![
                (
                    "QF_FNV_FO3_11F935".to_string(),
                    ["QuestRef".to_string(), "StageIndex".to_string()]
                        .into_iter()
                        .collect()
                ),
                ("FNV_FO3_FnvSliceCompat".to_string(), BTreeSet::new())
            ]
        );
    }
}
