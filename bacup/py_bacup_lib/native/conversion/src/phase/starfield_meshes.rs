//! starfield_meshes: FO4 NIF -> Starfield NIF + `.mesh` phase.
//!
//! Params shape (JSON): a pre-resolved entry list like `starfield_materials`,
//! `starfield_textures`, and `phase/nifs.rs`'s `nif_paths`. The driver's record MODL
//! walk and extracted-dir/BA2 resolution build it; this phase never reads the source
//! plugin handle:
//! {
//!   "nifs": [
//!     { "nif_rel": "Architecture/Wall01.nif", "resolved_path": "Meshes/Architecture/Wall01.nif" },
//!     ...
//!   ],
//!   "source_extracted": "/abs/path/to/extracted"   // resolves relative resolved_path entries
//! }
//!
//! Output: one converted NIF per entry at `<mod>/data/meshes/<nif_rel>`, plus every
//! `.mesh` file `convert_fo4_nif_to_starfield` returns, written at
//! `<mod>/data/<geometries/.../*.mesh>` (the returned path already includes the
//! `geometries/` prefix). Collision mode is always `FromFo4Prims`. A source NIF whose
//! Havok collision blob exists but doesn't decode counts toward `collision_fallbacks`:
//! `starfield_collision::extract_fo4_collision_prims` then returns only `Box` prims from
//! its AABB-over-render-mesh fallback, while a genuine decode always yields `ConvexHull`s.
//!
//! CRC casing contract (R5-mat-format.md): `mat_out_rel` normalizes to lowercase +
//! backslash separators, and the writer's `mat_path_for` callback normalizes again before
//! computing the MaterialID CRC. The double normalization is idempotent.

use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use nif_core_native::model::{NifBlock, NifFile, NifValue};
use nif_core_native::starfield_collision::{self, CollisionPrim};
use nif_core_native::starfield_write::{self, CollisionMode};

/// Emit a `Log` progress update every N converted entries, independent of the throttled
/// `ProgressReporter` (which emits `Progress` events, not `Log`).
const LOG_PROGRESS_STEP: usize = 100;

/// lowercase, `.bgsm` -> `.mat`, BACKSLASH separators always. Duplicated verbatim in
/// `phase/starfield_materials.rs`; the two must stay byte-identical.
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

/// Reads the FO4 shape's linked `BSLightingShaderProperty`/`BSEffectShaderProperty` block
/// (via the shape's `Shader Property` ref) and returns its `Name` field — the source `.bgsm`
/// path — if present and non-empty.
fn shader_material_rel(nif: &NifFile, shape: &NifBlock) -> Option<String> {
    let id = match shape.get_field("Shader Property") {
        Some(NifValue::Ref(id)) if *id >= 0 => *id as usize,
        _ => return None,
    };
    let shader = nif.blocks.get(id)?;
    match shader.get_field("Name") {
        Some(NifValue::String(name)) => {
            let trimmed = name.trim().trim_matches('\0');
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        _ => None,
    }
}

/// `true` when the source NIF has collision that exists but failed to decode, so
/// `extract_fo4_collision_prims` fell back to a render-geometry AABB box (the only path that
/// produces an all-`Box` result — a genuine decode always yields `ConvexHull`s, and "no
/// collision at all" yields an empty `Vec`, per R7 spec-delta 8).
fn collision_used_fallback(fo4_bytes: &[u8]) -> bool {
    match starfield_collision::extract_fo4_collision_prims(fo4_bytes) {
        Ok(prims) => {
            !prims.is_empty() && prims.iter().all(|p| matches!(p, CollisionPrim::Box { .. }))
        }
        Err(_) => false,
    }
}

#[derive(Debug)]
pub struct ConvertedNif {
    pub nif_bytes: Vec<u8>,
    /// `(relative path under Data, e.g. "geometries/<hash>/<hash>.mesh", bytes)`.
    pub mesh_files: Vec<(String, Vec<u8>)>,
    pub collision_fallback: bool,
}

/// Converts one FO4 NIF's bytes to a Starfield NIF + `.mesh` files. `rel_path` is used only
/// for error-message context (it is not written to disk here — the phase's caller decides the
/// output layout).
pub fn convert_one_nif(fo4_bytes: &[u8], rel_path: &str) -> Result<ConvertedNif, String> {
    let nif_in = NifFile::from_bytes(fo4_bytes, None)
        .map_err(|error| format!("{rel_path}: failed to parse FO4 NIF: {error}"))?;

    let mat_path_for = move |shape: &NifBlock| -> String {
        shader_material_rel(&nif_in, shape)
            .map(|bgsm_rel| mat_out_rel(&bgsm_rel))
            .unwrap_or_default()
    };

    let out = starfield_write::convert_fo4_nif_to_starfield(
        fo4_bytes,
        &mat_path_for,
        CollisionMode::FromFo4Prims,
    )
    .map_err(|error| format!("{rel_path}: {error}"))?;

    Ok(ConvertedNif {
        nif_bytes: out.nif_bytes,
        mesh_files: out.mesh_files,
        collision_fallback: collision_used_fallback(fo4_bytes),
    })
}

struct NifEntry {
    nif_rel: String,
    resolved_path: PathBuf,
}

fn parse_nif_entries(
    params: &JsonValue,
    source_extracted: &Path,
) -> Result<Vec<NifEntry>, PhaseError> {
    let Some(arr) = params.get("nifs").and_then(JsonValue::as_array) else {
        return Ok(Vec::new());
    };
    arr.iter()
        .enumerate()
        .map(|(index, entry)| {
            let nif_rel = entry
                .get("nif_rel")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| PhaseError::BadParams(format!("nifs[{index}].nif_rel missing")))?
                .to_string();
            let resolved_raw = entry
                .get("resolved_path")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    PhaseError::BadParams(format!("nifs[{index}].resolved_path missing"))
                })?;
            Ok(NifEntry {
                resolved_path: resolve_path(source_extracted, resolved_raw),
                nif_rel,
            })
        })
        .collect()
}

/// Writes the converted NIF at `data_root/meshes/<nif_rel>` and every returned `.mesh` file at
/// `data_root/<mesh_rel>`. Returns the number of `.mesh` files written (the NIF itself is not
/// counted).
fn write_converted_nif(
    data_root: &Path,
    nif_rel: &str,
    converted: &ConvertedNif,
) -> Result<u32, String> {
    let nif_dest = data_root
        .join("meshes")
        .join(nif_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if let Some(parent) = nif_dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&nif_dest, &converted.nif_bytes).map_err(|e| e.to_string())?;

    for (mesh_rel, bytes) in &converted.mesh_files {
        let dest = data_root.join(mesh_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;
    }
    Ok(converted.mesh_files.len() as u32)
}

pub struct StarfieldMeshesPhase;

impl Phase for StarfieldMeshesPhase {
    fn name(&self) -> &'static str {
        "starfield_meshes"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let entries = parse_nif_entries(ctx.params, ctx.source_extracted_dir)?;
        let data_root = ctx.mod_path.join("data");
        let total = entries.len();

        let mut nifs_converted = 0u32;
        let mut mesh_files_written = 0u32;
        let mut collision_fallbacks = 0u32;
        let mut nifs_failed = 0u32;

        let reporter =
            ProgressReporter::new("starfield_meshes", total as u32, ctx.run.event_tx.clone());

        for (index, entry) in entries.iter().enumerate() {
            ctx.check_cancel()?;
            reporter.set_item(entry.nif_rel.clone());

            let result = std::fs::read(&entry.resolved_path)
                .map_err(|e| format!("{}: {e}", entry.resolved_path.display()))
                .and_then(|bytes| convert_one_nif(&bytes, &entry.nif_rel))
                .and_then(|converted| {
                    let mesh_count = write_converted_nif(&data_root, &entry.nif_rel, &converted)?;
                    Ok((mesh_count, converted.collision_fallback))
                });

            match result {
                Ok((mesh_count, fallback)) => {
                    nifs_converted += 1;
                    mesh_files_written += mesh_count;
                    if fallback {
                        collision_fallbacks += 1;
                    }
                }
                Err(error) => {
                    nifs_failed += 1;
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "starfield_meshes",
                        level: LogLevel::Error,
                        message: format!("starfield_meshes: {}: {error}", entry.nif_rel),
                    });
                }
            }
            reporter.inc(1);

            let processed = index + 1;
            if processed % LOG_PROGRESS_STEP == 0 {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "starfield_meshes",
                    level: LogLevel::Info,
                    message: format!(
                        "starfield_meshes: progress {processed}/{total} nifs_converted={nifs_converted} nifs_failed={nifs_failed}"
                    ),
                });
            }
        }
        reporter.finish();

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "starfield_meshes",
            level: LogLevel::Info,
            message: format!(
                "starfield_meshes: nifs_converted={nifs_converted} mesh_files_written={mesh_files_written} collision_fallbacks={collision_fallbacks} nifs_failed={nifs_failed}"
            ),
        });

        Ok(PhaseReport {
            assets_written: nifs_converted + mesh_files_written,
            warnings: nifs_failed,
            items_failed: nifs_failed,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use regex::Regex;

    /// Builds a minimal FO4 NIF (`NifFile::new("fo4")` + one `BSTriShape` child wired to a
    /// `BSLightingShaderProperty` whose `Name` is `bgsm_path`) via the crate's own model API —
    /// mirrors nif_core's own `starfield_write` fixture builder, adapted to also exercise
    /// material-path resolution.
    fn build_fo4_fixture_with_material(bgsm_path: &str) -> Vec<u8> {
        let mut nif = NifFile::new("fo4");

        let vertex = |x: f32, y: f32| -> NifValue {
            let mut fields = IndexMap::new();
            fields.insert("Vertex".to_string(), NifValue::Vec3([x, y, 0.0]));
            fields.insert("Bitangent X".to_string(), NifValue::Float(0.0));
            fields.insert("UV".to_string(), {
                let mut uv = IndexMap::new();
                uv.insert("u".to_string(), NifValue::Float(0.0));
                uv.insert("v".to_string(), NifValue::Float(0.0));
                NifValue::Struct(uv)
            });
            fields.insert("Normal".to_string(), NifValue::Vec3([0.0, 0.0, 1.0]));
            fields.insert("Bitangent Y".to_string(), NifValue::Float(1.0));
            fields.insert("Tangent".to_string(), NifValue::Vec3([1.0, 0.0, 0.0]));
            fields.insert("Bitangent Z".to_string(), NifValue::Float(0.0));
            NifValue::Struct(fields)
        };

        let vertex_data =
            NifValue::Array(vec![vertex(0.0, 0.0), vertex(1.0, 0.0), vertex(0.0, 1.0)]);
        let triangle = |a: i64, b: i64, c: i64| -> NifValue {
            let mut t = IndexMap::new();
            t.insert("v1".to_string(), NifValue::Int(a));
            t.insert("v2".to_string(), NifValue::Int(b));
            t.insert("v3".to_string(), NifValue::Int(c));
            NifValue::Struct(t)
        };

        let mut shader_fields = IndexMap::new();
        shader_fields.insert("Name".to_string(), NifValue::String(bgsm_path.to_string()));
        let shader_id = nif.add_block("BSLightingShaderProperty", Some(shader_fields));

        let mut fields = IndexMap::new();
        fields.insert(
            "Name".to_string(),
            NifValue::String("TestShape".to_string()),
        );
        fields.insert("Flags".to_string(), NifValue::UInt(14));
        fields.insert("Translation".to_string(), NifValue::Vec3([0.0, 0.0, 0.0]));
        fields.insert(
            "Rotation".to_string(),
            NifValue::Matrix33([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        );
        fields.insert("Scale".to_string(), NifValue::Float(1.0));
        fields.insert("Collision Object".to_string(), NifValue::Ref(-1));
        fields.insert("Skin".to_string(), NifValue::Ref(-1));
        fields.insert(
            "Shader Property".to_string(),
            NifValue::Ref(shader_id as i32),
        );
        fields.insert("Alpha Property".to_string(), NifValue::Ref(-1));
        // BSVertexDesc: full-precision Vertex (bit 0x400) + VF_VERTEX|VF_UVS|VF_NORMALS|VF_TANGENTS,
        // matching starfield_write.rs's own fixture, so the vertex data round-trips exactly.
        const FULLPREC_VERTEX_DESC_FLAGS: i64 = 0x0001 | 0x0002 | 0x0008 | 0x0010 | 0x0400;
        fields.insert(
            "Vertex Desc".to_string(),
            NifValue::Int(FULLPREC_VERTEX_DESC_FLAGS << 44),
        );
        fields.insert("Num Triangles".to_string(), NifValue::UInt(1));
        fields.insert("Num Vertices".to_string(), NifValue::UInt(3));
        fields.insert("Data Size".to_string(), NifValue::UInt(1));
        fields.insert("Vertex Data".to_string(), vertex_data);
        fields.insert(
            "Triangles".to_string(),
            NifValue::Array(vec![triangle(0, 1, 2)]),
        );

        let shape_id = nif.add_block("BSTriShape", Some(fields));
        nif.blocks[0].set_field("Name", NifValue::String("TestRoot".to_string()));
        nif.blocks[0].set_field(
            "Children",
            NifValue::Array(vec![NifValue::Ref(shape_id as i32)]),
        );
        nif.blocks[0].set_field("Num Children", NifValue::UInt(1));

        nif.rebuild_header();
        nif.header.footer_roots = vec![0];
        nif.to_bytes().expect("fixture NIF serializes")
    }

    #[test]
    fn mat_out_rel_lowercases_swaps_extension_and_forces_backslash() {
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
    }

    #[test]
    fn convert_one_nif_produces_valid_output_and_correct_mesh_path_shape() {
        let bytes = build_fo4_fixture_with_material("Materials\\Test\\Testmat.bgsm");
        let converted = convert_one_nif(&bytes, "Test/TestShape.nif").expect("conversion succeeds");

        let nif_out = NifFile::from_bytes(&converted.nif_bytes, None).expect("output NIF parses");
        assert!(
            (170..=179).contains(&nif_out.header.bs_version),
            "bs_version {} out of range",
            nif_out.header.bs_version
        );

        let geometry_count = nif_out
            .blocks
            .iter()
            .filter(|b| b.type_name == "BSGeometry")
            .count();
        assert_eq!(
            geometry_count, 1,
            "exactly one BSGeometry for one input shape"
        );
        assert_eq!(
            converted.mesh_files.len(),
            1,
            "one .mesh file per unique geometry"
        );

        let re = Regex::new(r"^geometries/[0-9a-f]{20}/[0-9a-f]{20}\.mesh$").unwrap();
        let (mesh_rel, _bytes) = &converted.mesh_files[0];
        assert!(
            re.is_match(mesh_rel),
            "unexpected mesh file path: {mesh_rel}"
        );
    }

    #[test]
    fn convert_one_nif_on_garbage_bytes_fails_named_by_path() {
        let error =
            convert_one_nif(b"not a nif", "broken.nif").expect_err("garbage input must fail");
        assert!(
            error.contains("broken.nif"),
            "error must name the path: {error}"
        );
    }

    #[test]
    fn phase_run_converts_one_writes_outputs_and_counts_failures_by_path() {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let mod_path = tmp.path().join("mod");
        std::fs::create_dir_all(&source).unwrap();

        let good_bytes = build_fo4_fixture_with_material("Materials\\Test\\Testmat.bgsm");
        std::fs::write(source.join("good.nif"), &good_bytes).unwrap();
        std::fs::write(source.join("bad.nif"), b"not a nif").unwrap();

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
            "nifs": [
                { "nif_rel": "Good.nif", "resolved_path": "good.nif" },
                { "nif_rel": "Bad.nif", "resolved_path": "bad.nif" },
                { "nif_rel": "Missing.nif", "resolved_path": "missing.nif" }
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
            StarfieldMeshesPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.warnings, 2, "nifs_failed");
        assert_eq!(report.items_failed, 2, "nifs_failed");
        assert!(mod_path.join("data/meshes/Good.nif").is_file());

        let events = with_run(id, |run| -> Result<Vec<PhaseEvent>, RunError> {
            Ok(run.event_rx.try_iter().collect())
        })
        .unwrap();
        assert!(
            events.iter().any(|event| matches!(
                event,
                PhaseEvent::Log { level: LogLevel::Error, message, .. }
                    if message.contains("Bad.nif")
            )),
            "unparseable NIF must be reported by path in a Log event, not silently dropped"
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                PhaseEvent::Log { level: LogLevel::Error, message, .. }
                    if message.contains("Missing.nif")
            )),
            "missing NIF must be reported by path in a Log event, not silently dropped"
        );

        drop_run(id).unwrap();
    }

    #[test]
    fn phase_run_emits_a_log_progress_event_every_100_items() {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let mod_path = tmp.path().join("mod");
        std::fs::create_dir_all(&source).unwrap();

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

        // All 100 entries resolve to a missing file, so each fails fast — this test exercises
        // the per-100 Log cadence, not conversion itself.
        let nifs: Vec<JsonValue> = (0..100)
            .map(|i| {
                serde_json::json!({ "nif_rel": format!("Item{i}.nif"), "resolved_path": "missing.nif" })
            })
            .collect();
        let params = serde_json::json!({
            "nifs": nifs,
            "source_extracted": source.to_string_lossy(),
        });

        with_run(id, |run| -> Result<PhaseReport, RunError> {
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
            StarfieldMeshesPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let events = with_run(id, |run| -> Result<Vec<PhaseEvent>, RunError> {
            Ok(run.event_rx.try_iter().collect())
        })
        .unwrap();
        assert!(
            events.iter().any(|event| matches!(
                event,
                PhaseEvent::Log { level: LogLevel::Info, message, .. }
                    if message.contains("progress 100/100")
            )),
            "expected a per-100 progress Log event"
        );

        drop_run(id).unwrap();
    }
}
