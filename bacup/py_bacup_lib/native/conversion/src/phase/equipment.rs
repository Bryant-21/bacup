// Phase: convert_equipment  +  extract_atx
//
// Both equipment-related phases live in this file.
//
// ──────────────────────────────────────────────────────────────────────────────
// Phase `convert_equipment`
// ──────────────────────────────────────────────────────────────────────────────
//
// Params shape (JSON):
// {
//   "weapon_form_keys":  ["FalloutNV.esm:00F4F4"],   // WEAP records to process
//   "addon_index_start": 20000,                        // first AddonNode index to reserve
//   "mod_prefix":        "B21"                         // mod EditorID prefix
// }
//
// Reads each WEAP record from the run's source plugin handle, checks for
// populated ModelMod1/2/3 fields, and reports how many attachment-slot
// synthetic record groups would be generated. Synthetic record insertion is a
// native follow-up; this phase must not delegate record mutation to Python.
//
// PhaseReport:
//   records_added  = count of synthetic records that would be created
//                    (5 per slot × slots + 1 association keyword)
//   warnings       = WEAP records that could not be read
//
// ──────────────────────────────────────────────────────────────────────────────
// Phase `extract_atx`
// ──────────────────────────────────────────────────────────────────────────────
//
// Params shape (JSON):
// {
//   "atx_slugs":         ["gausspistol", "gaussrifle"],
// // source_extracted defaults to ctx.source_extracted_dir when absent/empty
//   "source_extracted":  "/abs/path/to/extracted/fo76",
//   "mod_name":          "B21_GaussPistol"
// }
//
// Walks `<source_extracted>/materials/atx/weapons/<slug>/` for ATX skin BGSMs
// and counts them. Full MaterialSwap synthesis and texture walking is handled
// by the Python workflow.
//
// PhaseReport:
//   assets_written = number of .bgsm files found across all slugs
//   warnings       = slugs whose directory could not be read

use std::path::Path;

use serde_json::Value as JsonValue;

use crate::ids::SigCode;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::source_read::{form_key_to_read_str, iter_form_keys_of_sig, read_record};

// ══════════════════════════════════════════════════════════════════════════════
// convert_equipment
// ══════════════════════════════════════════════════════════════════════════════

pub struct ConvertEquipmentPhase;

impl Phase for ConvertEquipmentPhase {
    fn name(&self) -> &'static str {
        "convert_equipment"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        let mut weapon_form_keys: Vec<String> = p
            .get("weapon_form_keys")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if weapon_form_keys.is_empty() {
            let sig = SigCode::from_str("WEAP")
                .map_err(|e| PhaseError::Internal(format!("WEAP signature: {e}")))?;
            weapon_form_keys =
                iter_form_keys_of_sig(ctx.run.source_handle_id, sig, &ctx.run.interner)
                    .map_err(|e| PhaseError::Internal(format!("{e}")))?
                    .iter()
                    .map(|fk| form_key_to_read_str(fk, &ctx.run.interner))
                    .filter(|fk| !fk.is_empty())
                    .collect();
        }

        let _addon_index_start: u32 = p
            .get("addon_index_start")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or(20000);

        let mod_prefix = p
            .get("mod_prefix")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if weapon_form_keys.is_empty() {
            return Ok(PhaseReport::default());
        }
        if mod_prefix.is_empty() {
            return Err(PhaseError::BadParams("mod_prefix is required".into()));
        }

        let source_handle_id = ctx.run.source_handle_id;
        let schema = ctx.run.schema_source.clone();

        let mut records_added: u32 = 0;
        let mut warnings: u32 = 0;

        for form_key_str in &weapon_form_keys {
            ctx.check_cancel()?;

            let record =
                match read_record(source_handle_id, form_key_str, &schema, &ctx.run.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[convert_equipment] WARN: cannot read {form_key_str}: {e}");
                        warnings += 1;
                        continue;
                    }
                };

            // EID is needed for EditorID-based record naming.
            let weap_eid = match record.eid.as_ref() {
                Some(eid) => match ctx.run.interner.resolve(*eid) {
                    Some(s) => s.to_string(),
                    None => {
                        warnings += 1;
                        continue;
                    }
                },
                None => {
                    warnings += 1;
                    continue;
                }
            };

            let populated_slots = weapon_populated_slots_from_record(&record, &ctx.run.interner);
            if populated_slots.is_empty() {
                continue;
            }

            // One association keyword per weapon.
            let _assoc_eid = format!("{mod_prefix}_KW_{weap_eid}Association");
            records_added += 1; // KYWD association

            // Per-slot: AttachPoint KYWD + OMOD + MISC + COBJ = 4 records
            records_added += 4 * populated_slots.len() as u32;
        }

        Ok(PhaseReport {
            records_added,
            warnings,
            ..Default::default()
        })
    }
}

/// Return the slot numbers (1, 2, 3) that have a ModelMod field populated.
///
/// ModelMod1–3 are stored as MOD2/MOD3/MOD4 subrecords in the WEAP binary.
/// After schema decoding they appear with their authoring field names.
fn weapon_populated_slots_from_record(
    record: &crate::record::Record,
    interner: &crate::sym::StringInterner,
) -> Vec<u8> {
    let mut slots = Vec::new();
    // ModelMod1 = MOD2, ModelMod2 = MOD3, ModelMod3 = MOD4 in FNV/FO4 WEAP.
    // Populated = a non-empty MOD2/MOD3/MOD4 string value, or a struct field
    // whose key contains "filename".
    for field in &record.fields {
        let sig_str = field.sig.as_str();
        // FNV WEAP model-mod subrecords:  MOD2 MOD3 MOD4 → slots 1 2 3
        let slot = match sig_str {
            "MOD2" => 1u8,
            "MOD3" => 2u8,
            "MOD4" => 3u8,
            _ => continue,
        };
        let non_empty = match &field.value {
            crate::record::FieldValue::String(sym) => interner
                .resolve(*sym)
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            crate::record::FieldValue::Struct(fields) => fields.iter().any(|(k, v)| {
                let key = interner.resolve(*k).unwrap_or("");
                if key.to_ascii_lowercase().contains("filename") || key == "Filename" {
                    if let crate::record::FieldValue::String(s) = v {
                        return interner
                            .resolve(*s)
                            .map(|sv| !sv.trim().is_empty())
                            .unwrap_or(false);
                    }
                }
                false
            }),
            _ => false,
        };
        if non_empty && !slots.contains(&slot) {
            slots.push(slot);
        }
    }
    slots.sort_unstable();
    slots
}

// ══════════════════════════════════════════════════════════════════════════════
// extract_atx
// ══════════════════════════════════════════════════════════════════════════════

pub struct ExtractAtxPhase;

impl Phase for ExtractAtxPhase {
    fn name(&self) -> &'static str {
        "extract_atx"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        let atx_slugs: Vec<String> = p
            .get("atx_slugs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let source_extracted: String = p
            .get("source_extracted")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| ctx.source_extracted_dir.to_string_lossy().into_owned());

        let _mod_name = p
            .get("mod_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if atx_slugs.is_empty() || source_extracted.is_empty() {
            return Ok(PhaseReport::default());
        }

        let extracted_root = Path::new(&source_extracted);
        if !extracted_root.is_dir() {
            return Ok(PhaseReport::default());
        }

        let mut assets_written: u32 = 0;
        let mut warnings: u32 = 0;

        for slug in &atx_slugs {
            ctx.check_cancel()?;

            let atx_dir = extracted_root
                .join("materials")
                .join("atx")
                .join("weapons")
                .join(slug);

            if !atx_dir.is_dir() {
                continue;
            }

            let entries = match std::fs::read_dir(&atx_dir) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("[extract_atx] WARN: cannot read {}: {e}", atx_dir.display());
                    warnings += 1;
                    continue;
                }
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase());
                if ext.as_deref() == Some("bgsm") {
                    assets_written += 1;
                }
            }
        }

        Ok(PhaseReport {
            assets_written,
            warnings,
            ..Default::default()
        })
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn make_run() -> (u64, crate::run::RunConfig) {
        use crate::run::{RunConfig, RunParams, create_run};
        use crate::translator::Game;
        let config = RunConfig {
            output_plugin_name: "Output.esp".into(),
            ..Default::default()
        };
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: config.clone(),
        })
        .unwrap();
        (id, config)
    }

    fn drop_run_id(id: u64) {
        let _ = crate::run::drop_run(id);
    }

    // ── convert_equipment ────────────────────────────────────────────────────

    /// Empty weapon_form_keys → zero records, no error.
    #[test]
    fn convert_equipment_empty_keys() {
        let (id, _) = make_run();
        let report = crate::run::with_run(id, |run| {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "weapon_form_keys": [],
                "addon_index_start": 20000,
                "mod_prefix": "B21"
            });
            let src = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &src,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertEquipmentPhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        assert_eq!(report.records_added, 0);
        assert_eq!(report.warnings, 0);
        drop_run_id(id);
    }

    /// Missing mod_prefix → BadParams error.
    #[test]
    fn convert_equipment_missing_prefix_errors() {
        let (id, _) = make_run();
        let result = crate::run::with_run(id, |run| {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "weapon_form_keys": ["FalloutNV.esm:00F4F4"],
                "addon_index_start": 20000,
                "mod_prefix": ""
            });
            let src = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &src,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertEquipmentPhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        });
        assert!(result.is_err());
        drop_run_id(id);
    }

    /// Unknown form key → warning counted, no panic.
    #[test]
    fn convert_equipment_unknown_form_key_counts_warning() {
        let (id, _) = make_run();
        let report = crate::run::with_run(id, |run| {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "weapon_form_keys": ["Nonexistent.esm:FFFFFF"],
                "addon_index_start": 20000,
                "mod_prefix": "B21"
            });
            let src = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &src,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertEquipmentPhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        // No panic; one form_key was attempted but source handle 9999 is a sentinel.
        assert_eq!(report.records_added, 0);
        drop_run_id(id);
    }

    // ── extract_atx ──────────────────────────────────────────────────────────

    /// Empty slugs → zero assets, no error.
    #[test]
    fn extract_atx_empty_slugs() {
        let (id, _) = make_run();
        let report = crate::run::with_run(id, |run| {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "atx_slugs": [],
                "source_extracted": "/nonexistent",
                "mod_name": "Test"
            });
            let src = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &src,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ExtractAtxPhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 0);
        drop_run_id(id);
    }

    /// Nonexistent source_extracted → empty report, no error.
    #[test]
    fn extract_atx_missing_extracted_dir() {
        let (id, _) = make_run();
        let report = crate::run::with_run(id, |run| {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "atx_slugs": ["gausspistol"],
                "source_extracted": "/nonexistent/path/extracted",
                "mod_name": "B21_GaussPistol"
            });
            let src = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &src,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ExtractAtxPhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        assert_eq!(report.assets_written, 0);
        drop_run_id(id);
    }

    /// Filesystem walk finds BGSM files in an ATX slug directory.
    #[test]
    fn extract_atx_counts_bgsm_files() {
        let tmp = std::env::temp_dir().join("extract_atx_test_bgsms");
        let atx_dir = tmp
            .join("materials")
            .join("atx")
            .join("weapons")
            .join("gausspistol");
        std::fs::create_dir_all(&atx_dir).unwrap();
        std::fs::write(
            atx_dir.join("atx_gausspistol_body_matteblack.bgsm"),
            b"BGSM",
        )
        .unwrap();
        std::fs::write(
            atx_dir.join("atx_gausspistol_barrel_matteblack.bgsm"),
            b"BGSM",
        )
        .unwrap();
        std::fs::write(atx_dir.join("unrelated.dds"), b"DDS").unwrap();

        let (id, _) = make_run();
        let report = crate::run::with_run(id, |run| {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "atx_slugs": ["gausspistol"],
                "source_extracted": tmp.to_string_lossy(),
                "mod_name": "B21_GaussPistol"
            });
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &tmp,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ExtractAtxPhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        // 2 BGSM files; .dds should not be counted
        assert_eq!(report.assets_written, 2);
        assert_eq!(report.warnings, 0);
        drop_run_id(id);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
