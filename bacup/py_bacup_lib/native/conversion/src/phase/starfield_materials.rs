//! starfield_materials: BGSM -> Starfield `.mat` phase.
//!
//! Params shape (JSON): a pre-resolved entry list, like `starfield_textures`.
//! The driver's NIF/record walk builds it:
//! {
//!   "materials": [
//!     { "bgsm_rel": "Materials/Weapons/Foo.BGSM", "resolved_path": "Materials/Weapons/Foo.bgsm" },
//!     ...
//!   ],
//!   "source_extracted": "/abs/path/to/extracted"   // resolves relative resolved_path entries
//! }
//!
//! Output: one loose `.mat` per BGSM at `<mod>/data/<mat_out_rel(bgsm_rel)>`
//! (never packed, R5-mat-format.md section 1.4).
//!
//! CRC casing contract (R5-mat-format.md sections 2.3 and 4): the NIF's
//! MaterialID extra-data CRC and this phase's `.mat` object ids are both
//! `bethesda_crc32(mat_rel.lower())`, where `mat_rel` is the **backslash**
//! form written into the NIF's BSLightingShaderProperty.Name (e.g.
//! `materials\foo\bar.mat`). `mat_out_rel` forces backslashes so the CRC input
//! doesn't depend on the caller's `bgsm_rel` spelling; the on-disk path
//! converts back to `/` at `data_root.join`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use materials_native::starfield_mat::{map_bgsm_to_mat_inputs, write_starfield_mat};

/// lowercase, `.bgsm` -> `.mat`, BACKSLASH separators always. Duplicated
/// verbatim in `phase/starfield_meshes.rs`; the two must stay byte-identical.
pub fn mat_out_rel(bgsm_rel: &str) -> String {
    let lower = bgsm_rel.to_ascii_lowercase().replace('/', "\\");
    match lower.strip_suffix(".bgsm") {
        Some(stem) => format!("{stem}.mat"),
        None => lower,
    }
}

fn resolve_path(source_extracted: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        source_extracted.join(p)
    }
}

/// Converts one already-read BGSM to a Starfield `.mat` and writes it under
/// `data_root` at `mat_out_rel(bgsm_rel)`. `seen_crcs` guards CRC32
/// collisions across the whole phase run (R5 section 2.3) — callers thread
/// the same set across every call in a run; a collision is a hard error,
/// never a silent overwrite.
pub fn convert_one_bgsm(
    bgsm_bytes: &[u8],
    bgsm_rel: &str,
    data_root: &Path,
    seen_crcs: &mut HashSet<u32>,
) -> Result<PathBuf, String> {
    let (tex, settings) = map_bgsm_to_mat_inputs(bgsm_bytes)?;
    let rel = mat_out_rel(bgsm_rel);
    let json_text = write_starfield_mat(&rel, &tex, &settings, seen_crcs)?;

    let dest = data_root.join(rel.replace('\\', "/"));
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&dest, json_text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(dest)
}

struct MaterialEntry {
    bgsm_rel: String,
    resolved_path: PathBuf,
}

fn parse_materials(
    params: &JsonValue,
    source_extracted: &Path,
) -> Result<Vec<MaterialEntry>, PhaseError> {
    let Some(arr) = params.get("materials").and_then(JsonValue::as_array) else {
        return Ok(Vec::new());
    };
    arr.iter()
        .enumerate()
        .map(|(index, entry)| {
            let bgsm_rel = entry
                .get("bgsm_rel")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    PhaseError::BadParams(format!("materials[{index}].bgsm_rel missing"))
                })?
                .to_string();
            let resolved_raw = entry
                .get("resolved_path")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    PhaseError::BadParams(format!("materials[{index}].resolved_path missing"))
                })?;
            Ok(MaterialEntry {
                resolved_path: resolve_path(source_extracted, resolved_raw),
                bgsm_rel,
            })
        })
        .collect()
}

pub struct StarfieldMaterialsPhase;

impl Phase for StarfieldMaterialsPhase {
    fn name(&self) -> &'static str {
        "starfield_materials"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let entries = parse_materials(ctx.params, ctx.source_extracted_dir)?;
        let data_root = ctx.mod_path.join("data");

        let mut seen_crcs: HashSet<u32> = HashSet::new();
        let mut mats_written = 0u32;
        let mut bgsm_failed = 0u32;

        let reporter = ProgressReporter::new(
            "starfield_materials",
            entries.len() as u32,
            ctx.run.event_tx.clone(),
        );

        for entry in &entries {
            ctx.check_cancel()?;
            reporter.set_item(entry.bgsm_rel.clone());

            let result = std::fs::read(&entry.resolved_path)
                .map_err(|e| format!("{}: {e}", entry.resolved_path.display()))
                .and_then(|bytes| {
                    convert_one_bgsm(&bytes, &entry.bgsm_rel, &data_root, &mut seen_crcs)
                });

            match result {
                Ok(_) => mats_written += 1,
                Err(error) => {
                    bgsm_failed += 1;
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "starfield_materials",
                        level: LogLevel::Error,
                        message: format!("starfield_materials: {}: {error}", entry.bgsm_rel),
                    });
                }
            }
            reporter.inc(1);
        }
        reporter.finish();

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "starfield_materials",
            level: LogLevel::Info,
            message: format!(
                "starfield_materials: mats_written={mats_written} bgsm_failed={bgsm_failed}"
            ),
        });

        Ok(PhaseReport {
            assets_written: mats_written,
            warnings: bgsm_failed,
            items_failed: bgsm_failed,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use materials_native::bgsm;

    fn write_fixture_bgsm(path: &Path, diffuse: &str, normal: &str) {
        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 20;
        data.DiffuseTexture = diffuse.to_string();
        data.NormalTexture = normal.to_string();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bgsm::write(&data)).unwrap();
    }

    #[test]
    fn mat_out_rel_lowercases_swaps_extension_and_forces_backslash() {
        // Must stay byte-identical to `phase::starfield_meshes::mat_out_rel`.
        assert_eq!(
            mat_out_rel("Materials/Foo/Bar.BGSM"),
            "materials\\foo\\bar.mat"
        );
        assert_eq!(
            mat_out_rel("Materials\\Foo\\Bar.BGSM"),
            "materials\\foo\\bar.mat"
        );
        // Non-.bgsm inputs pass through lowercased/backslashed, untouched otherwise.
        assert_eq!(
            mat_out_rel("Materials/Foo/Bar.other"),
            "materials\\foo\\bar.other"
        );
        assert_eq!(
            mat_out_rel("Materials/Foo/Bar.BGSM"),
            crate::phase::starfield_meshes::mat_out_rel("Materials/Foo/Bar.BGSM")
        );
    }

    #[test]
    fn convert_one_bgsm_writes_mat_with_color_path_at_mapped_location() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("Materials/Weapons/Foo.bgsm");
        write_fixture_bgsm(
            &src,
            "Textures\\Weapons\\Foo_d.dds",
            "Textures\\Weapons\\Foo_n.dds",
        );
        let bytes = std::fs::read(&src).unwrap();

        let data_root = tmp.path().join("data");
        let mut seen = HashSet::new();
        let dest =
            convert_one_bgsm(&bytes, "Materials/Weapons/Foo.BGSM", &data_root, &mut seen).unwrap();

        assert_eq!(dest, data_root.join("materials/weapons/foo.mat"));
        assert!(dest.is_file());

        let text = std::fs::read_to_string(&dest).unwrap();
        let doc: JsonValue = serde_json::from_str(&text).expect("output .mat must parse as JSON");
        let has_color_path = doc["Objects"]
            .as_array()
            .expect("Objects array")
            .iter()
            .flat_map(|obj| obj["Components"].as_array().cloned().unwrap_or_default())
            .any(|component| {
                component["Data"]
                    .get("FileName")
                    .and_then(JsonValue::as_str)
                    .map(|f| f.to_ascii_lowercase().contains("_color"))
                    .unwrap_or(false)
            });
        assert!(has_color_path, "expected a _color texture path in {text}");
    }

    #[test]
    fn collision_guard_trips_on_two_bgsms_mapping_to_the_same_crc() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("Materials/Weapons/Foo.bgsm");
        write_fixture_bgsm(&src, "Textures\\Weapons\\Foo_d.dds", "");
        let bytes = std::fs::read(&src).unwrap();

        let data_root = tmp.path().join("data");
        let mut seen = HashSet::new();

        convert_one_bgsm(&bytes, "Materials/Weapons/Foo.BGSM", &data_root, &mut seen)
            .expect("first write succeeds");
        let error = convert_one_bgsm(&bytes, "Materials/Weapons/Foo.BGSM", &data_root, &mut seen)
            .expect_err("same output path must trip the CRC collision guard");
        assert!(
            error.to_ascii_lowercase().contains("collision"),
            "expected a CRC collision error, got: {error}"
        );
    }

    #[test]
    fn phase_run_converts_one_material_and_reports_missing_file_by_path() {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let mod_path = tmp.path().join("mod");
        write_fixture_bgsm(
            &source.join("Materials/Weapons/Foo.bgsm"),
            "Textures\\Weapons\\Foo_d.dds",
            "",
        );

        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Starfield,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Fallout4_SF.esm".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let params = serde_json::json!({
            "materials": [
                { "bgsm_rel": "Materials/Weapons/Foo.BGSM", "resolved_path": "Materials/Weapons/Foo.bgsm" },
                { "bgsm_rel": "Materials/Missing/Bar.BGSM", "resolved_path": "Materials/Missing/Bar.bgsm" }
            ],
            "source_extracted": source.to_string_lossy(),
        });

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            StarfieldMaterialsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 1, "mats_written");
        assert_eq!(report.warnings, 1, "bgsm_failed");
        assert!(mod_path.join("data/materials/weapons/foo.mat").is_file());

        let events = with_run(id, |run| -> Result<Vec<PhaseEvent>, RunError> {
            Ok(run.event_rx.try_iter().collect())
        })
        .unwrap();
        assert!(
            events.iter().any(|event| matches!(
                event,
                PhaseEvent::Log { level: LogLevel::Error, message, .. }
                    if message.contains("Materials/Missing/Bar.BGSM")
            )),
            "missing BGSM must be reported by path in a Log event, not silently dropped"
        );

        drop_run(id).unwrap();
    }
}
