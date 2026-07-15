//! `walk` phase — native dependency-graph walker.
//!
//! # Params shape (JSON)
//! ```json
//! {
//!   "root_form_keys": ["MyPlugin.esp:000800", ...],
//!   "strict_unresolved_masters": false
//! }
//! ```
//! `root_form_keys` and `strict_unresolved_masters` are optional. When
//! `root_form_keys` is absent or empty the phase performs a full-index walk
//! over every record in the source plugin. Handles are always owned by the run.
//!
//! # Phase output
//! The `WalkOutput` is stored in `ctx.run.dependency_graph` for downstream
//! phases.  `PhaseReport` carries:
//! - `records_changed` = number of reached records
//! - `assets_written`  = number of asset references found
//! - `warnings`        = unresolved form-keys + walk error count

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value as JsonValue;

use esp_authoring_core::plugin_runtime::{WalkAsset, WalkOutput};

use crate::phase::lod_assets::{
    LOD_BASE_SIGS, LodAsset, append_lod_closures_for_modl_for_game, dedup_key,
};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::translator::Game;

// Bundled default policy — parsed at runtime, not compile time (serde-saphyr
// deserialises YAML→JSON; no serde_yaml dep needed).
const DEFAULT_POLICY_YAML: &str = include_str!("walk/policy.yaml");

/// Parse the bundled `policy.yaml` into the JSON string expected by
/// `esp_authoring_core::plugin_runtime::plugin_handle_walk_native_rs`.
fn policy_json_from_yaml(yaml_str: &str) -> Result<String, String> {
    // serde_saphyr knows how to deserialise YAML directly into serde_json::Value.
    let doc: JsonValue =
        serde_saphyr::from_str(yaml_str).map_err(|e| format!("policy.yaml parse error: {e}"))?;

    let follow_signatures = doc
        .get("follow_signatures")
        .cloned()
        .unwrap_or(JsonValue::Null);
    let asset_kinds = doc.get("asset_kinds").cloned().unwrap_or(JsonValue::Null);
    let reverse_passes = doc
        .get("reverse_passes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            JsonValue::Array(
                arr.iter()
                    .filter_map(|e| e.as_str().map(|s| JsonValue::String(s.to_string())))
                    .collect(),
            )
        })
        .unwrap_or_else(|| JsonValue::Array(vec![]));
    let behavior_bundle = doc
        .get("behavior_bundle")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let character_assets = doc
        .get("character_assets")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let animation_lookup = doc
        .get("animation_lookup")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_depth = doc.get("max_depth").cloned().unwrap_or(JsonValue::Null);

    let payload = serde_json::json!({
        "follow_signatures": follow_signatures,
        "asset_kinds": asset_kinds,
        "reverse_passes": reverse_passes,
        "behavior_bundle": behavior_bundle,
        "character_assets": character_assets,
        "animation_lookup": animation_lookup,
        "max_depth": max_depth,
        "terminal_signatures": null,
    });

    serde_json::to_string(&payload).map_err(|e| format!("policy JSON serialise: {e}"))
}

pub struct WalkPhase;

impl Phase for WalkPhase {
    fn name(&self) -> &'static str {
        "walk"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let params = ctx.params;
        for legacy_key in ["source_handle", "master_handles"] {
            if params.get(legacy_key).is_some() {
                return Err(PhaseError::BadParams(format!(
                    "walk: legacy parameter is not supported: {legacy_key}"
                )));
            }
        }

        // -- Extract params --------------------------------------------------
        let root_form_keys: Vec<String> = match params.get("root_form_keys") {
            Some(JsonValue::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => vec![],
        };

        let source_handle_id = ctx
            .run
            .require_source_handle()
            .map_err(|error| PhaseError::BadParams(format!("walk: {error}")))?;
        let master_handle_ids = ctx.run.master_handle_ids.clone();

        let strict: bool = params
            .get("strict_unresolved_masters")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // -- Build policy JSON from bundled policy.yaml ----------------------
        let policy_json = policy_json_from_yaml(DEFAULT_POLICY_YAML)
            .map_err(|e| PhaseError::Internal(format!("walk policy: {e}")))?;

        ctx.check_cancel()?;

        // -- Run the walk (no GIL) -------------------------------------------
        let mut output = esp_authoring_core::plugin_runtime::plugin_handle_walk_native_rs(
            vec![source_handle_id],
            master_handle_ids,
            root_form_keys,
            &policy_json,
            strict,
        )
        .map_err(|e| PhaseError::Internal(format!("walk_dependencies: {e}")))?;

        // Part A — LOD-by-convention discovery. FO76 ships `_lod.nif` meshes that
        // NO record field references, so the walker never reaches them. For each
        // reached LOD-capable base, derive its `_lod[_N].nif` via the shared rule
        // (`lod_paths`) and append every existing hit plus its material/texture
        // closure, so the asset phases convert+ship them (else the synthesized-MNAM
        // object LOD renders pink). Existence-gated + dedup'd against the walk.
        append_lod_convention_assets(&mut output, ctx.source_extracted_dir, ctx.run.source);

        let n_records = output.reached_records.len() as u32;
        let n_assets = output.assets.len() as u32;
        let n_warnings = (output.errors.len() + output.unresolved_form_keys.len()) as u32;

        // Store the graph for downstream phases.
        ctx.run.dependency_graph = Some(output);

        Ok(PhaseReport {
            records_changed: n_records,
            assets_written: n_assets,
            warnings: n_warnings,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Part A — LOD-by-convention asset discovery (graph/bounded flows)
// ---------------------------------------------------------------------------
//
// Delegates the path rule + closure expansion to `lod_assets` (the SAME single
// source used by the regen whole-plugin path via `conversion_run_collect_lod_closures`
// and by `synthesize_object_lod`), so the three stay byte-consistent.

/// Append LOD-convention assets (`_lod.nif` + material/texture closure) to
/// `output.assets`. Returns the number of assets added.
fn append_lod_convention_assets(
    output: &mut WalkOutput,
    source_dir: &Path,
    source_game: Game,
) -> usize {
    let mut seen: HashSet<String> = output
        .assets
        .iter()
        .map(|a| dedup_key(&a.asset_kind, &a.source_path))
        .collect();

    // Snapshot LOD-capable base model assets to avoid borrowing `output` while
    // mutating it. The base model `source_path` IS the MODL the rule consumes.
    let bases: Vec<(String, String, String, u32)> = output
        .assets
        .iter()
        .filter(|a| {
            a.asset_kind == "nif" && LOD_BASE_SIGS.contains(&a.source_record_signature.as_str())
        })
        .map(|a| {
            (
                a.source_path.clone(),
                a.source_record_signature.clone(),
                a.source_form_key.clone(),
                a.walk_depth,
            )
        })
        .collect();

    let mut added = 0usize;
    for (modl, sig, fk, depth) in bases {
        let mut closures: Vec<LodAsset> = Vec::new();
        append_lod_closures_for_modl_for_game(
            &modl,
            source_dir,
            source_game,
            &mut seen,
            &mut closures,
        );
        for a in closures {
            output.assets.push(WalkAsset {
                asset_kind: a.kind,
                source_path: a.source_path,
                source_form_key: fk.clone(),
                source_record_signature: sig.clone(),
                source_subrecord_sig: "MNAM".to_string(),
                walk_depth: depth,
                walker_pass: "lod_convention".to_string(),
            });
            added += 1;
        }
    }
    added
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_yaml_is_valid() {
        let json =
            policy_json_from_yaml(DEFAULT_POLICY_YAML).expect("policy.yaml should parse cleanly");
        let parsed: JsonValue =
            serde_json::from_str(&json).expect("serialised policy must be valid JSON");
        assert!(parsed.get("reverse_passes").is_some());
        assert!(parsed.get("behavior_bundle").is_some());
    }

    #[test]
    fn walk_phase_name() {
        let phase = WalkPhase;
        assert_eq!(phase.name(), "walk");
    }

    /// A reached LOD-capable base whose `_lod.nif` exists on disk gets the LOD
    /// mesh appended to the asset list (Part A).
    #[test]
    fn lod_convention_appends_existing_lod_nif() {
        let dir = std::env::temp_dir().join("walk_lod_convention_present");
        let _ = std::fs::remove_dir_all(&dir);
        let lod = dir
            .join("meshes")
            .join("lod")
            .join("architecture")
            .join("foo");
        std::fs::create_dir_all(&lod).unwrap();
        std::fs::write(lod.join("bar01_lod.nif"), b"nif").unwrap();

        let mut output = WalkOutput::default();
        output.assets.push(WalkAsset {
            asset_kind: "nif".into(),
            source_path: "Meshes\\Architecture\\Foo\\Bar01.nif".into(),
            source_form_key: "001234:Test.esm".into(),
            source_record_signature: "STAT".into(),
            source_subrecord_sig: "MODL".into(),
            walk_depth: 1,
            walker_pass: "main".into(),
        });

        let added = append_lod_convention_assets(&mut output, &dir, Game::Fo76);
        assert!(added >= 1, "expected the LOD nif to be appended");
        assert!(
            output.assets.iter().any(|a| a.asset_kind == "nif"
                && a.walker_pass == "lod_convention"
                && a.source_path
                    .to_ascii_lowercase()
                    .contains("lod\\architecture\\foo\\bar01_lod.nif")),
            "appended LOD nif missing; assets={:?}",
            output
                .assets
                .iter()
                .map(|a| &a.source_path)
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A base whose `_lod.nif` does NOT exist appends nothing.
    #[test]
    fn lod_convention_skips_absent_lod() {
        let dir = std::env::temp_dir().join("walk_lod_convention_absent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut output = WalkOutput::default();
        output.assets.push(WalkAsset {
            asset_kind: "nif".into(),
            source_path: "Meshes\\Architecture\\Foo\\NoLod01.nif".into(),
            source_form_key: "001234:Test.esm".into(),
            source_record_signature: "STAT".into(),
            source_subrecord_sig: "MODL".into(),
            walk_depth: 1,
            walker_pass: "main".into(),
        });

        let added = append_lod_convention_assets(&mut output, &dir, Game::Fo76);
        assert_eq!(added, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// WalkPhase with a sentinel (missing) handle returns PhaseError::Internal,
    /// not a panic.
    #[test]
    fn walk_phase_returns_error_on_missing_handle() {
        use crate::run::{RunConfig, RunParams, create_run, drop_run, run_slot};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let run_id = create_run(RunParams {
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
        .expect("create_run");

        let cancel = std::sync::Arc::new(AtomicBool::new(false));

        let result = {
            let slot = run_slot(run_id).expect("slot");
            let mut run = slot.run.lock().expect("run lock");
            let params = serde_json::json!({
                "root_form_keys": ["Output.esp:000800"],
                "strict_unresolved_masters": false
            });
            let mut ctx = PhaseCtx {
                run: &mut run,
                mod_path: std::path::Path::new("/fake/mod"),
                source_extracted_dir: std::path::Path::new("/fake/extracted"),
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            WalkPhase.run(&mut ctx)
        };

        drop_run(run_id).ok();

        assert!(
            result.is_err(),
            "expected PhaseError for unknown handle but got Ok"
        );
    }
}
