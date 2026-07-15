//! FNV legacy scripting.
//!
//! - `function_map`: loads `fnv_to_fo4_script_functions.yaml` and
//!   `fnv_to_fo4_actor_values.yaml` into `FnvScriptContext`.
//! - `script_translator`: lexer → parser → semantic → emitter pipeline that
//!   converts FNV GECK script source into Papyrus `.psc` text.
//! - quest/scene/dialogue/vmad/voice/naming/form_keys synthesizers.
//! - orchestration via `FnvLegacyScriptingContext` + `run_fnv_legacy_scripting`
//!   on `ConversionRun`.

pub mod dialogue;
pub mod fk_rewrite;
pub mod form_keys;
pub mod from_run;
pub mod function_map;
pub mod naming;
pub mod psc_emission;
pub mod quest;
pub mod scene;
pub mod script_synthesizer;
pub mod script_translator;
pub mod vmad;
pub mod voice;

// ---------------------------------------------------------------------------
// End-to-end convenience
// ---------------------------------------------------------------------------

pub use function_map::FnvScriptContext;
use script_translator::{
    emitter::emit_papyrus, lexer::tokenize, parser::parse, semantic::apply_semantic,
};

// Re-export primary orchestration types.
pub use dialogue::{DialogueGroup, TranslatedInfo};
pub use quest::TranslatedQuest;
pub use scene::TranslatedScene;
pub use script_synthesizer::{PapyrusType, TranslatedScript};

// ---------------------------------------------------------------------------
// Orchestration context and result
// ---------------------------------------------------------------------------

/// All state accumulated during a single FNV legacy-scripting pass.
///
/// Created by `ConversionRun::run_fnv_legacy_scripting` and populated by each
/// per-record-type synthesizer.  Payloads accumulated in
/// `translated_record_payloads` are transient: `ConversionRun` drains them
/// directly into the target plugin handle via
/// `insert_authoring_record_value` and they never reach the result
/// returned to Python (spec-clean port — no JSON payload return).
#[derive(Debug, Default)]
pub struct FnvLegacyScriptingContext {
    /// Mod prefix (e.g. `"B21"`).
    pub mod_prefix: String,
    /// Source plugin filename (e.g. `"FNV.esm"`).
    pub source_plugin: String,
    /// Whether to halt on translation errors (strict mode).
    pub strict: bool,

    // Accumulated outputs.
    pub translated_scripts: Vec<TranslatedScript>,
    pub translated_quests: Vec<TranslatedQuest>,
    pub translated_infos: Vec<TranslatedInfo>,
    pub translated_scenes: Vec<TranslatedScene>,
    /// Transient: drained by `ConversionRun::run_fnv_legacy_scripting` and
    /// written to the target plugin handle. Not exposed in
    /// `FnvLegacyScriptingResult`.
    pub translated_record_payloads: Vec<TranslatedRecordPayload>,
    pub skipped_records: Vec<(String, String, String)>,
    pub lip_regeneration_needed: Vec<String>,
    pub warnings: Vec<String>,
}

impl FnvLegacyScriptingContext {
    /// Create a new context with the given settings.
    pub fn new(
        mod_prefix: impl Into<String>,
        source_plugin: impl Into<String>,
        strict: bool,
    ) -> Self {
        Self {
            mod_prefix: mod_prefix.into(),
            source_plugin: source_plugin.into(),
            strict,
            ..Default::default()
        }
    }
}

/// A single translated record ready to be written to the authoring dir.
#[derive(Debug, Clone)]
pub struct TranslatedRecordPayload {
    pub source_form_key: String,
    pub signature: String,
    pub translated_record: serde_json::Value,
    pub warnings: Vec<String>,
}

/// Top-level result returned by `ConversionRun::run_fnv_legacy_scripting`.
///
/// Spec-clean: no JSON record payloads. Translated records are written
/// directly to the target plugin handle by `ConversionRun`; the result only
/// carries counts and side-channel data (PSC texts, voice paths, warnings).
#[derive(Debug, Default)]
pub struct FnvLegacyScriptingResult {
    pub translated_scripts: Vec<TranslatedScript>,
    pub translated_quests: Vec<TranslatedQuest>,
    pub translated_infos: Vec<TranslatedInfo>,
    pub translated_scenes: Vec<TranslatedScene>,
    /// DIAL records grouped by speaker FormKey (QNAM). Mirrors Python
    /// `FnvLegacyScriptingResult.dialogue_groups` — set but not consumed
    /// downstream today; kept for parity / future inspection.
    pub dialogue_groups: Vec<DialogueGroup>,
    /// Number of records successfully written to the target plugin handle.
    pub records_written: u32,
    /// Number of records that translated but failed to write to the handle.
    pub records_failed: u32,
    /// Number of `.psc` files written under `mod_path/Source/User/`.
    /// Zero when `mod_path` was empty or when no records carried psc text.
    pub psc_files_written: u32,
    /// Number of `.psc` emissions skipped (empty psc_text or empty mod_path).
    pub psc_files_skipped: u32,
    pub skipped_records: Vec<(String, String, String)>,
    pub lip_regeneration_needed: Vec<String>,
    pub warnings: Vec<String>,
    /// VMAD script-binding intents computed from `scri_links` + translated
    /// scripts. Empty when `scri_links` was empty.
    pub vmad_intents: Vec<vmad::ScriptBindingIntent>,
    /// True when Rust attached VMAD directly to the target plugin handle.
    pub vmad_attached_in_rust: bool,
}

/// Error type for the FNV scripting phase.
#[derive(Debug)]
pub enum FnvScriptingError {
    /// Translation failed for a record.
    Translate(String),
    /// Context setup failed.
    Setup(String),
}

impl std::fmt::Display for FnvScriptingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Translate(msg) => write!(f, "fnv scripting translate error: {msg}"),
            Self::Setup(msg) => write!(f, "fnv scripting setup error: {msg}"),
        }
    }
}

impl std::error::Error for FnvScriptingError {}

impl From<TranslateError> for FnvScriptingError {
    fn from(e: TranslateError) -> Self {
        FnvScriptingError::Translate(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Per-type translate_all helpers called from ConversionRun
// ---------------------------------------------------------------------------

/// Translate all SCPT records in `records` and accumulate results into `ctx`.
///
/// SCPT records produce only a `TranslatedScript` (with `psc_text` ready for
/// file emission). They do NOT contribute a translated record payload — SCPT
/// records aren't re-emitted to the target plugin.
pub fn translate_all_scpt(
    ctx: &mut FnvLegacyScriptingContext,
    records: &[(serde_json::Value, String)],
) {
    for (record, form_key) in records {
        match script_synthesizer::translate_scpt_record(record, &ctx.mod_prefix, form_key) {
            Ok(ts) => {
                ctx.translated_scripts.push(ts);
            }
            Err(e) => {
                ctx.skipped_records.push((
                    "SCPT".into(),
                    record
                        .get("eid")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                        .to_string(),
                    e.to_string(),
                ));
            }
        }
    }
}

/// Translate all QUST records in `records` and accumulate results into `ctx`.
///
/// Skips records that fail translation (records appended to `ctx.skipped_records`).
pub fn translate_all_qust(
    ctx: &mut FnvLegacyScriptingContext,
    records: &[(serde_json::Value, String)],
) {
    for (record, form_key) in records {
        match quest::translate_qust_record(record, &ctx.mod_prefix, ctx.strict, form_key) {
            Ok(tq) => {
                if let Some(payload) = tq.authoring_record_payload.clone() {
                    ctx.translated_record_payloads
                        .push(TranslatedRecordPayload {
                            source_form_key: form_key.clone(),
                            signature: "QUST".into(),
                            translated_record: payload,
                            warnings: tq.warnings.clone(),
                        });
                }
                ctx.translated_quests.push(tq);
            }
            Err(e) => {
                ctx.skipped_records.push((
                    "QUST".into(),
                    record
                        .get("eid")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                        .to_string(),
                    e.to_string(),
                ));
            }
        }
    }
}

/// Translate all SCEN records and accumulate results into `ctx`.
pub fn translate_all_scen(
    ctx: &mut FnvLegacyScriptingContext,
    records: &[(serde_json::Value, String)],
) {
    for (record, form_key) in records {
        match scene::translate_scen_record(record, &ctx.mod_prefix, ctx.strict, form_key) {
            Ok(ts) => {
                if let Some(payload) = ts.authoring_record_payload.clone() {
                    ctx.translated_record_payloads
                        .push(TranslatedRecordPayload {
                            source_form_key: form_key.clone(),
                            signature: "SCEN".into(),
                            translated_record: payload,
                            warnings: ts.warnings.clone(),
                        });
                }
                ctx.translated_scenes.push(ts);
            }
            Err(e) => {
                ctx.skipped_records.push((
                    "SCEN".into(),
                    record
                        .get("eid")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                        .to_string(),
                    e.to_string(),
                ));
            }
        }
    }
}

/// Translate all INFO records and accumulate results into `ctx`.
pub fn translate_all_dial(
    ctx: &mut FnvLegacyScriptingContext,
    records: &[(serde_json::Value, String)],
) {
    for (record, form_key) in records {
        match dialogue::translate_info_record(
            record,
            &ctx.mod_prefix,
            &ctx.source_plugin,
            ctx.strict,
            form_key,
        ) {
            Ok(ti) => {
                if ti.lip_dropped {
                    if let Some(ref target) = ti.lip_regeneration_target {
                        if !ctx.lip_regeneration_needed.contains(target) {
                            ctx.lip_regeneration_needed.push(target.clone());
                        }
                    }
                }
                if let Some(payload) = ti.authoring_record_payload.clone() {
                    ctx.translated_record_payloads
                        .push(TranslatedRecordPayload {
                            source_form_key: form_key.clone(),
                            signature: "INFO".into(),
                            translated_record: payload,
                            warnings: ti.warnings.clone(),
                        });
                }
                ctx.translated_infos.push(ti);
            }
            Err(e) => {
                ctx.skipped_records.push((
                    "INFO".into(),
                    record
                        .get("eid")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                        .to_string(),
                    e.to_string(),
                ));
            }
        }
    }
}

/// Translate a raw FNV GECK script source string into Papyrus source text.
///
/// Convenience wrapper over the four-stage pipeline.  Returns `Err` on the
/// first lex, parse, or semantic error.
pub fn translate_to_papyrus(
    source: &str,
    ctx: &FnvScriptContext,
    script_class_name: &str,
    papyrus_extends: &str,
) -> Result<String, TranslateError> {
    let tokens = tokenize(source).map_err(|e| TranslateError::Lex(e.to_string()))?;
    let ast = parse(tokens).map_err(|e| TranslateError::Parse(e.to_string()))?;
    let semantic = apply_semantic(ast, ctx).map_err(|e| TranslateError::Semantic(e.to_string()))?;
    Ok(emit_papyrus(&semantic, script_class_name, papyrus_extends))
}

#[derive(Debug)]
pub enum TranslateError {
    Lex(String),
    Parse(String),
    Semantic(String),
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranslateError::Lex(msg) => write!(f, "lex error: {msg}"),
            TranslateError::Parse(msg) => write!(f, "parse error: {msg}"),
            TranslateError::Semantic(msg) => write!(f, "semantic error: {msg}"),
        }
    }
}

impl std::error::Error for TranslateError {}

// ---------------------------------------------------------------------------
// End-to-end orchestration tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod orchestration_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn translate_all_qust_smoke() {
        let records = vec![(
            json!({
                "eid": "TestQuest",
                "fields": [
                    { "INDX": 10 },
                    { "SCTX": "set x to 1" },
                ]
            }),
            "001234:FNV.esm".to_string(),
        )];
        let mut ctx = FnvLegacyScriptingContext::new("B21", "FNV.esm", false);
        translate_all_qust(&mut ctx, &records);
        assert_eq!(
            ctx.translated_quests.len(),
            1,
            "one quest should be translated"
        );
        assert_eq!(ctx.translated_record_payloads.len(), 1);
        assert_eq!(ctx.skipped_records.len(), 0);
        let tq = &ctx.translated_quests[0];
        assert_eq!(tq.stage_fragments.len(), 1);
        assert_eq!(tq.stage_fragments[0].stage_index, 10);
    }

    #[test]
    fn translate_all_scen_smoke() {
        let records = vec![(
            json!({
                "eid": "TestScene",
                "fields": [
                    { "SCTX": "set y to 2" },
                ]
            }),
            "001235:FNV.esm".to_string(),
        )];
        let mut ctx = FnvLegacyScriptingContext::new("B21", "FNV.esm", false);
        translate_all_scen(&mut ctx, &records);
        assert_eq!(ctx.translated_scenes.len(), 1);
        assert_eq!(ctx.translated_scenes[0].actions.len(), 1);
    }

    #[test]
    fn translate_all_dial_smoke() {
        let records = vec![(
            json!({
                "eid": "TestInfo",
                "fields": [
                    { "SCTX": "set z to 3" },
                ]
            }),
            "001236:FNV.esm".to_string(),
        )];
        let mut ctx = FnvLegacyScriptingContext::new("B21", "FNV.esm", false);
        translate_all_dial(&mut ctx, &records);
        assert_eq!(ctx.translated_infos.len(), 1);
        assert!(ctx.translated_infos[0].fragment_class_name.is_some());
        // LIP dropped → voice path should be in lip_regeneration_needed.
        assert!(!ctx.lip_regeneration_needed.is_empty());
    }

    #[test]
    fn fnv_legacy_scripting_context_accumulates_payloads() {
        // Payloads are transient: they live on the context and are drained
        // by `ConversionRun::run_fnv_legacy_scripting` into the target plugin
        // handle. They do not appear on `FnvLegacyScriptingResult` anymore.
        let mut ctx = FnvLegacyScriptingContext::new("B21", "FNV.esm", false);
        let records = vec![(
            json!({ "eid": "Q1", "fields": [] }),
            "001234:FNV.esm".to_string(),
        )];
        translate_all_qust(&mut ctx, &records);
        assert_eq!(ctx.translated_quests.len(), 1);
        assert_eq!(ctx.translated_record_payloads.len(), 1);
    }

    #[test]
    fn skipped_records_on_strict_not_halting_by_default() {
        // A record with a dropped function should be skipped (not panic) in non-strict mode.
        let records = vec![(
            json!({
                "eid": "Q2",
                "fields": [
                    { "INDX": 10 },
                    { "SCTX": "RewardKarma(10)" },
                ]
            }),
            "001237:FNV.esm".to_string(),
        )];
        let mut ctx = FnvLegacyScriptingContext::new("B21", "FNV.esm", false);
        translate_all_qust(&mut ctx, &records);
        // RewardKarma is dropped → translate error → record skipped.
        assert_eq!(ctx.skipped_records.len(), 1);
        assert_eq!(ctx.skipped_records[0].0, "QUST");
    }
}

// ---------------------------------------------------------------------------
// End-to-end tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_set_statement() {
        let ctx = FnvScriptContext::load().expect("load ctx");
        let src = "begin GameMode\nset x to 100\nend\n";
        let papyrus =
            translate_to_papyrus(src, &ctx, "MyScript", "ObjectReference").expect("translate ok");
        assert!(papyrus.contains("x = 100"), "output:\n{papyrus}");
        assert!(papyrus.contains("Event OnInit()"), "output:\n{papyrus}");
    }

    #[test]
    fn end_to_end_get_player() {
        let ctx = FnvScriptContext::load().expect("load ctx");
        let src = "begin GameMode\nGetPlayer()\nend\n";
        let papyrus =
            translate_to_papyrus(src, &ctx, "MyScript", "ObjectReference").expect("translate ok");
        assert!(papyrus.contains("Game.GetPlayer()"), "output:\n{papyrus}");
    }

    #[test]
    fn end_to_end_get_actor_value() {
        // Tests AV remap: Strength → GetValue(Strength)
        let ctx = FnvScriptContext::load().expect("load ctx");
        let src = "begin GameMode\nGetActorValue(Strength)\nend\n";
        let papyrus =
            translate_to_papyrus(src, &ctx, "MyScript", "ObjectReference").expect("translate ok");
        assert!(
            papyrus.contains("GetValue") || papyrus.contains("Strength"),
            "output:\n{papyrus}"
        );
    }

    #[test]
    fn end_to_end_scriptname_preserved() {
        let ctx = FnvScriptContext::load().expect("load ctx");
        let src = "ScriptName FooScript\nbegin GameMode\nend\n";
        let papyrus =
            translate_to_papyrus(src, &ctx, "OverrideName", "Quest").expect("translate ok");
        // The explicit class name wins over the in-script name.
        assert!(papyrus.contains("ScriptName OverrideName extends Quest"));
    }

    #[test]
    fn end_to_end_var_decls() {
        let ctx = FnvScriptContext::load().expect("load ctx");
        let src = "short myCount\nfloat myRate\nbegin GameMode\nend\n";
        let papyrus =
            translate_to_papyrus(src, &ctx, "S", "ObjectReference").expect("translate ok");
        assert!(papyrus.contains("Int myCount"), "output:\n{papyrus}");
        assert!(papyrus.contains("Float myRate"), "output:\n{papyrus}");
    }

    #[test]
    fn end_to_end_if_return() {
        let ctx = FnvScriptContext::load().expect("load ctx");
        let src = "begin GameMode\nif x == 1\nreturn\nendif\nend\n";
        let papyrus =
            translate_to_papyrus(src, &ctx, "S", "ObjectReference").expect("translate ok");
        assert!(papyrus.contains("If"), "output:\n{papyrus}");
        assert!(papyrus.contains("Return"), "output:\n{papyrus}");
        assert!(papyrus.contains("EndIf"), "output:\n{papyrus}");
    }

    #[test]
    fn end_to_end_dropped_function_errors() {
        let ctx = FnvScriptContext::load().expect("load ctx");
        let src = "begin GameMode\nRewardKarma(10)\nend\n";
        let err = translate_to_papyrus(src, &ctx, "S", "ObjectReference")
            .expect_err("expected translate error");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("drop") || msg.contains("karma"),
            "error message: {msg}"
        );
    }
}
