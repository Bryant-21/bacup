//! Phase: `generate_precombines` — source-free CK-free precombine generation
//! (v0 spike). Runs `ck_native::precombine::{plan, bake, stamp}` against the
//! open target handle only; no source plugin is read.
//!
//! Plan: `docs/superpowers/plans/2026-07-12-precombine-generation-v0.md` Task 5.
//!
//! ## Params (JSON)
//! ```text
//! {
//!   "include_cells": ["0062781C"], // exactly one 8-hex cell form id
//!   "min_eligible_refs": 1,        // optional, default 1
//!   "no_previs": true,             // optional, default true
//!   "mesh_extract_roots": ["..."], // optional, default []. Extra loose
//!                                  // search roots (e.g. a pre-extracted
//!                                  // vanilla/DLC asset dir mirroring
//!                                  // Data\ layout), tried in order after
//!                                  // data_root and before mesh_archives.
//!   "mesh_archives": ["..."]       // optional, default []. Fallback BA2
//!                                  // archives for a source MODL that
//!                                  // isn't a loose file, tried in order
//!                                  // after all loose roots.
//! }
//! ```
//! `pcmb_date` is the grounded internal constant `0x1F24`; it is never a JSON
//! key. Stale keys `output_handle_id`, `own_index`, and `vc_stamp` are
//! rejected outright.
//!
//! Phase contract: NO Python / GIL. `ctx.run.target_handle_id` is passed to
//! `ck_native` internally — both crates are compiled into this same BACUP
//! native extension and therefore share one `esp_authoring_core` handle
//! registry (see `py_creation_lib/native/ck/src/precombine/plan.rs` module
//! doc and root CLAUDE.md's handle-isolation rule).

use std::path::{Path, PathBuf};

use ck_native::precombine::{bake, plan, stamp};

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

/// v0 grounded internal constant (see plan "Current-state grounding" — the
/// live FO76 CELL's PCMB bytes decode to this date). Never a JSON key.
const PCMB_DATE: u16 = 0x1F24;

const STALE_PARAM_KEYS: [&str; 3] = ["output_handle_id", "own_index", "vc_stamp"];

pub struct GeneratePrecombinesPhase;

impl Phase for GeneratePrecombinesPhase {
    fn name(&self) -> &'static str {
        "generate_precombines"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;
        for stale_key in STALE_PARAM_KEYS {
            if p.get(stale_key).is_some() {
                return Err(PhaseError::BadParams(format!(
                    "generate_precombines: legacy parameter is not supported: {stale_key}"
                )));
            }
        }
        let include_cells = parse_include_cells(p)?;
        let min_eligible_refs = p
            .get("min_eligible_refs")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;
        let no_previs = p.get("no_previs").and_then(|v| v.as_bool()).unwrap_or(true);
        let mesh_extract_roots = parse_mesh_extract_roots(p)?;
        let mesh_archives = parse_mesh_archives(p)?;

        let data_root = ctx
            .target_data_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| ctx.mod_path.join("data"));

        let plan_params = plan::Params {
            target_handle_id: ctx.run.target_handle_id,
            plugin_name: ctx.run.config.output_plugin_name.clone(),
            data_root,
            include_cells,
            min_eligible_refs,
            pcmb_date: PCMB_DATE,
            no_previs,
            mesh_extract_roots,
            mesh_archives,
        };

        ctx.check_cancel()?;
        let built_plan = plan::build_plan(&plan_params)
            .map_err(|error| PhaseError::BadParams(format!("generate_precombines: {error}")))?;

        let mut assets_written = 0u32;
        let mut records_changed = 0u32;
        let mut warning_count = 0u32;
        let mut items_failed = 0u32;

        for cell_plan in &built_plan.cells {
            ctx.check_cancel()?;
            let bake_report = bake::bake_cell(cell_plan, &plan_params);
            for warning in &bake_report.warnings {
                warning_count += 1;
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "generate_precombines",
                    level: LogLevel::Warn,
                    message: warning.clone(),
                });
            }
            // Never stamp a zero-mesh cell.
            let Some(baked) = bake_report.baked else {
                items_failed += 1;
                continue;
            };

            ctx.check_cancel()?;
            if let Some(missing) = first_missing_file(&plan_params.data_root, &baked) {
                items_failed += 1;
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "generate_precombines",
                    level: LogLevel::Warn,
                    message: format!(
                        "cell {:08X} skipped: baked file missing on disk: {}",
                        baked.cell_form_id,
                        missing.display()
                    ),
                });
                continue;
            }

            assets_written += baked.meshes.len() as u32;
            let stats = stamp::stamp_cell(ctx.run.target_handle_id, &baked, PCMB_DATE, no_previs)
                .map_err(|error| {
                PhaseError::Internal(format!("generate_precombines: stamp_cell: {error}"))
            })?;
            records_changed += 1 + stats.refs_stamped;
        }

        Ok(PhaseReport {
            records_changed,
            assets_written,
            warnings: warning_count,
            items_failed,
            ..Default::default()
        })
    }
}

fn parse_include_cells(p: &serde_json::Value) -> Result<Vec<u32>, PhaseError> {
    let raw = p
        .get("include_cells")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            PhaseError::BadParams("generate_precombines: missing include_cells".to_string())
        })?;
    raw.iter()
        .map(|v| {
            let s = v.as_str().ok_or_else(|| {
                PhaseError::BadParams(
                    "generate_precombines: include_cells entries must be strings".to_string(),
                )
            })?;
            u32::from_str_radix(s.trim(), 16).map_err(|error| {
                PhaseError::BadParams(format!(
                    "generate_precombines: invalid include_cells entry {s:?}: {error}"
                ))
            })
        })
        .collect()
}

/// `mesh_extract_roots` / `mesh_archives` are both optional (default `[]`,
/// back-compat with params JSON that predates this key) ordered lists of
/// path strings; shares its array/string-entry validation between both keys.
fn parse_mesh_extract_roots(p: &serde_json::Value) -> Result<Vec<PathBuf>, PhaseError> {
    parse_optional_path_list(p, "mesh_extract_roots")
}

fn parse_mesh_archives(p: &serde_json::Value) -> Result<Vec<PathBuf>, PhaseError> {
    parse_optional_path_list(p, "mesh_archives")
}

fn parse_optional_path_list(p: &serde_json::Value, key: &str) -> Result<Vec<PathBuf>, PhaseError> {
    let Some(value) = p.get(key) else {
        return Ok(Vec::new());
    };
    let raw = value.as_array().ok_or_else(|| {
        PhaseError::BadParams(format!("generate_precombines: {key} must be an array"))
    })?;
    raw.iter()
        .map(|v| {
            v.as_str().map(PathBuf::from).ok_or_else(|| {
                PhaseError::BadParams(format!(
                    "generate_precombines: {key} entries must be strings"
                ))
            })
        })
        .collect()
}

/// Every `BakedMesh` is already load-verified by `bake_cell` itself, but the
/// plan calls for an explicit existence gate as its own pipeline stage
/// between bake and stamp — defense against the file having been removed (or
/// never actually flushed) between the two steps.
fn first_missing_file(data_root: &Path, baked: &bake::BakedCell) -> Option<PathBuf> {
    baked
        .meshes
        .iter()
        .map(|mesh| resolve_rel_path(data_root, &mesh.rel_path))
        .find(|path| !path.is_file())
}

fn resolve_rel_path(data_root: &Path, rel_path: &str) -> PathBuf {
    let mut out = data_root.to_path_buf();
    let normalized = rel_path.replace('\\', "/");
    for component in normalized.split('/') {
        if !component.is_empty() {
            out.push(component);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bytes::Bytes;
    use indexmap::IndexMap;
    use nif_core_native::model::{NifFile, NifValue};
    use smol_str::SmolStr;

    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, ParsedSubrecord, ensure_interior_cell_and_child_group,
        insert_parsed_record, insert_placed_child_into_cell_group, plugin_handle_new_native,
        plugin_handle_store_ref,
    };

    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    const TEMPORARY: i32 = 9;
    const CELL_INTERIOR_FLAG: u8 = 0x01;

    fn dispatch(
        target_handle_id: u64,
        target_data_dir: Option<PathBuf>,
        params: serde_json::Value,
    ) -> Result<PhaseReport, String> {
        let run_id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Test.esm".into(),
                ..Default::default()
            },
        })
        .expect("create_run");
        let mod_path = PathBuf::new();
        let source_dir = mod_path.clone();
        let result = with_run(run_id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: target_data_dir.as_deref(),
                params: &params,
                cancel: &cancel,
            };
            GeneratePrecombinesPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        });
        drop_run(run_id).expect("drop_run");
        result.map_err(|e| e.to_string())
    }

    #[test]
    fn registered_as_source_free_precombine_phase() {
        let names = crate::phase::registry().names();
        assert!(names.contains(&"generate_precombines"), "{names:?}");
        assert!(
            !crate::phase::registry()
                .get("generate_precombines")
                .expect("registered")
                .requires_source_plugin(),
            "generate_precombines must be source-free (beside the post-asset MODT phases)"
        );
    }

    #[test]
    fn rejects_legacy_output_handle_id_key() {
        let err = dispatch(
            999_999,
            None,
            serde_json::json!({ "output_handle_id": 5, "include_cells": ["00100000"] }),
        )
        .expect_err("must reject legacy key");
        assert!(err.contains("output_handle_id"), "{err}");
    }

    #[test]
    fn rejects_legacy_own_index_key() {
        let err = dispatch(
            999_999,
            None,
            serde_json::json!({ "own_index": 0, "include_cells": ["00100000"] }),
        )
        .expect_err("must reject legacy key");
        assert!(err.contains("own_index"), "{err}");
    }

    #[test]
    fn rejects_legacy_vc_stamp_key() {
        let err = dispatch(
            999_999,
            None,
            serde_json::json!({ "vc_stamp": "1F24", "include_cells": ["00100000"] }),
        )
        .expect_err("must reject legacy key");
        assert!(err.contains("vc_stamp"), "{err}");
    }

    #[test]
    fn rejects_missing_include_cells() {
        let err = dispatch(999_999, None, serde_json::json!({}))
            .expect_err("must reject missing include_cells");
        assert!(err.contains("include_cells"), "{err}");
    }

    #[test]
    fn rejects_non_hex_include_cells_entry() {
        let err = dispatch(
            999_999,
            None,
            serde_json::json!({ "include_cells": ["not-hex"] }),
        )
        .expect_err("must reject malformed include_cells entry");
        assert!(err.contains("not-hex"), "{err}");
    }

    #[test]
    fn parse_mesh_extract_roots_and_mesh_archives_default_to_empty_when_absent() {
        let p = serde_json::json!({ "include_cells": ["00100000"] });
        assert_eq!(
            parse_mesh_extract_roots(&p).expect("parse"),
            Vec::<PathBuf>::new()
        );
        assert_eq!(
            parse_mesh_archives(&p).expect("parse"),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn parse_mesh_extract_roots_and_mesh_archives_preserve_array_order() {
        let p = serde_json::json!({
            "include_cells": ["00100000"],
            "mesh_extract_roots": ["C:\\extract\\one", "C:\\extract\\two"],
            "mesh_archives": ["C:\\ba2\\a.ba2", "C:\\ba2\\b.ba2", "C:\\ba2\\c.ba2"],
        });
        assert_eq!(
            parse_mesh_extract_roots(&p).expect("parse"),
            vec![
                PathBuf::from("C:\\extract\\one"),
                PathBuf::from("C:\\extract\\two")
            ]
        );
        assert_eq!(
            parse_mesh_archives(&p).expect("parse"),
            vec![
                PathBuf::from("C:\\ba2\\a.ba2"),
                PathBuf::from("C:\\ba2\\b.ba2"),
                PathBuf::from("C:\\ba2\\c.ba2"),
            ]
        );
    }

    #[test]
    fn rejects_non_string_mesh_archives_entry() {
        let err = dispatch(
            999_999,
            None,
            serde_json::json!({ "include_cells": ["00100000"], "mesh_archives": [123] }),
        )
        .expect_err("must reject non-string mesh_archives entry");
        assert!(err.contains("mesh_archives"), "{err}");
    }

    // -----------------------------------------------------------------
    // Full synthetic phase test — the run/handle live entirely inside this
    // extension; nothing is imported from creation_lib._native.
    // -----------------------------------------------------------------

    fn temp_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "conversion_native_precombines_{name}_{}_{suffix}",
            std::process::id()
        ))
    }

    fn struct_fields<const N: usize>(entries: [(&str, NifValue); N]) -> IndexMap<String, NifValue> {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    fn basic_vertex(position: [f32; 3]) -> NifValue {
        NifValue::Struct(struct_fields([
            ("Vertex", NifValue::Vec3(position)),
            ("Bitangent X", NifValue::Float(0.0)),
            (
                "UV",
                NifValue::Struct(struct_fields([
                    ("u", NifValue::Float(0.0)),
                    ("v", NifValue::Float(0.0)),
                ])),
            ),
            ("Normal", NifValue::Vec3([0.0, 0.0, 1.0])),
            ("Bitangent Y", NifValue::Float(1.0)),
            ("Tangent", NifValue::Vec3([1.0, 0.0, 0.0])),
            ("Bitangent Z", NifValue::Float(0.0)),
        ]))
    }

    fn triangle(v1: u64, v2: u64, v3: u64) -> NifValue {
        NifValue::Struct(struct_fields([
            ("v1", NifValue::UInt(v1)),
            ("v2", NifValue::UInt(v2)),
            ("v3", NifValue::UInt(v3)),
        ]))
    }

    /// Proven-good stride/attribute bit pattern (float3 position, UV, normal,
    /// tangent/bitangent, no vertex colors); mirrors
    /// `ck/src/precombine/bake.rs::tests::basic_vertex_desc`.
    fn basic_vertex_desc() -> i64 {
        let stride = 5i64;
        let flags = 0x0001 | 0x0002 | 0x0008 | 0x0010;
        stride | (2 << 8) | (3 << 16) | (4 << 20) | (flags << 44)
    }

    /// Writes a single-shape, one-supported-lighting-shader FO4 NIF that
    /// `ck_native::precombine::bake::bake_cell` can bake successfully.
    fn write_bakeable_source_nif(path: &Path) {
        let mut nif = NifFile::new("fo4");
        let texset_id = nif.add_block(
            "BSShaderTextureSet",
            Some(struct_fields([
                ("Num Textures", NifValue::UInt(1)),
                (
                    "Textures",
                    NifValue::Array(vec![NifValue::String(r"textures\test\a.dds".to_string())]),
                ),
            ])),
        );
        let shader_id = nif.add_block(
            "BSLightingShaderProperty",
            Some(struct_fields([
                ("Name", NifValue::String(String::new())),
                ("Texture Set", NifValue::Ref(texset_id as i32)),
            ])),
        );
        let vertex_data = vec![
            basic_vertex([0.0, 0.0, 0.0]),
            basic_vertex([1.0, 0.0, 0.0]),
            basic_vertex([0.0, 1.0, 0.0]),
            basic_vertex([1.0, 1.0, 0.0]),
        ];
        let triangles = vec![triangle(0, 1, 2), triangle(1, 3, 2)];
        let shape_id = nif.add_block(
            "BSTriShape",
            Some(struct_fields([
                ("Name", NifValue::String("Shape:0".to_string())),
                (
                    "Bounding Sphere",
                    NifValue::Struct(struct_fields([
                        ("Center", NifValue::Vec3([0.0, 0.0, 0.0])),
                        ("Radius", NifValue::Float(1.0)),
                    ])),
                ),
                ("Skin", NifValue::Ref(-1)),
                ("Shader Property", NifValue::Ref(shader_id as i32)),
                ("Alpha Property", NifValue::Ref(-1)),
                ("Vertex Desc", NifValue::Int(basic_vertex_desc())),
                ("Num Triangles", NifValue::UInt(triangles.len() as u64)),
                ("Num Vertices", NifValue::UInt(vertex_data.len() as u64)),
                ("Vertex Data", NifValue::Array(vertex_data)),
                ("Triangles", NifValue::Array(triangles)),
            ])),
        );
        nif.blocks[0].set_field("Num Children", NifValue::UInt(1));
        nif.blocks[0].set_field(
            "Children",
            NifValue::Array(vec![NifValue::Ref(shape_id as i32)]),
        );
        std::fs::create_dir_all(path.parent().expect("fixture path has a parent"))
            .expect("create fixture dir");
        nif.save(Some(path.to_path_buf()))
            .expect("write source nif");
    }

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn edid(name: &str) -> ParsedSubrecord {
        let mut bytes = name.as_bytes().to_vec();
        bytes.push(0);
        sub("EDID", bytes)
    }

    fn modl(path: &str) -> ParsedSubrecord {
        let mut bytes = path.as_bytes().to_vec();
        bytes.push(0);
        sub("MODL", bytes)
    }

    fn parsed_record(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new(sig),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn interior_cell(form_id: u32, name: &str) -> ParsedRecord {
        parsed_record(
            "CELL",
            form_id,
            vec![edid(name), sub("DATA", vec![CELL_INTERIOR_FLAG, 0x00])],
        )
    }

    fn stat_base(form_id: u32, model_path: &str) -> ParsedRecord {
        parsed_record(
            "STAT",
            form_id,
            vec![edid(&format!("Stat{form_id:06X}")), modl(model_path)],
        )
    }

    fn refr_data(pos: [f32; 3], rot: [f32; 3]) -> ParsedSubrecord {
        let mut bytes = Vec::with_capacity(24);
        for value in pos.iter().chain(rot.iter()) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        sub("DATA", bytes)
    }

    fn plain_refr(form_id: u32, base_form_id: u32, pos: [f32; 3], rot: [f32; 3]) -> ParsedRecord {
        parsed_record(
            "REFR",
            form_id,
            vec![
                sub("NAME", base_form_id.to_le_bytes().to_vec()),
                refr_data(pos, rot),
            ],
        )
    }

    fn find_record<'a>(
        items: &'a [ParsedItem],
        signature: &str,
        form_id: u32,
    ) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record)
                    if record.signature.as_str() == signature && record.form_id == form_id =>
                {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(found) = find_record(&group.children, signature, form_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn end_to_end_bakes_and_stamps_a_cell() {
        let dir = temp_dir("e2e");
        let data_root = dir.join("data");
        write_bakeable_source_nif(&data_root.join("meshes").join("test").join("chair01.nif"));

        let target = plugin_handle_new_native("Test.esm", Some("fo4")).expect("target handle");
        ensure_interior_cell_and_child_group(target, interior_cell(0x0000_1000, "TestCell"))
            .expect("insert cell");
        insert_parsed_record(target, stat_base(0x0000_0500, "meshes\\test\\chair01.nif"))
            .expect("insert stat");
        insert_placed_child_into_cell_group(
            target,
            0x0000_1000,
            TEMPORARY,
            plain_refr(0x0000_0600, 0x0000_0500, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        )
        .expect("insert refr 1");
        insert_placed_child_into_cell_group(
            target,
            0x0000_1000,
            TEMPORARY,
            plain_refr(0x0000_0601, 0x0000_0500, [10.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        )
        .expect("insert refr 2");

        let params = serde_json::json!({
            "include_cells": ["00001000"],
            "min_eligible_refs": 1,
            "no_previs": true,
        });
        let report =
            dispatch(target, Some(data_root.clone()), params).expect("phase should succeed");

        assert_eq!(report.items_failed, 0, "report: {report:?}");
        assert_eq!(report.warnings, 0, "report: {report:?}");
        assert!(report.assets_written >= 1, "report: {report:?}");
        assert_eq!(
            report.records_changed, 3,
            "1 stamped CELL + 2 stamped REFRs: {report:?}"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).expect("target handle still present");
        let cell = find_record(&slot.parsed.root_items, "CELL", 0x0000_1000).expect("cell present");
        let sigs: Vec<&str> = cell
            .subrecords
            .iter()
            .map(|s| s.signature.as_str())
            .collect();
        assert!(sigs.contains(&"PCMB"), "{sigs:?}");
        assert!(sigs.contains(&"XCRI"), "{sigs:?}");
        drop(store);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Proves `mesh_extract_roots` reaches `plan::Params` end-to-end, not
    /// just `parse_mesh_extract_roots` in isolation: the mesh lives ONLY
    /// under `extract_root` (never under `data_root`), so the bake can only
    /// succeed if the JSON key was actually threaded into the `plan::Params`
    /// literal `bake_cell` runs against.
    #[test]
    fn mesh_extract_roots_param_reaches_plan_params_and_bakes_from_it() {
        let dir = temp_dir("extract_root_e2e");
        let data_root = dir.join("data"); // deliberately left without the mesh
        let extract_root = dir.join("extract");
        write_bakeable_source_nif(
            &extract_root
                .join("meshes")
                .join("test")
                .join("extracted01.nif"),
        );

        let target = plugin_handle_new_native("Test.esm", Some("fo4")).expect("target handle");
        ensure_interior_cell_and_child_group(target, interior_cell(0x0000_4000, "ExtractRootCell"))
            .expect("insert cell");
        insert_parsed_record(
            target,
            stat_base(0x0000_0500, "meshes\\test\\extracted01.nif"),
        )
        .expect("insert stat");
        insert_placed_child_into_cell_group(
            target,
            0x0000_4000,
            TEMPORARY,
            plain_refr(0x0000_0600, 0x0000_0500, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        )
        .expect("insert refr");

        let params = serde_json::json!({
            "include_cells": ["00004000"],
            "mesh_extract_roots": [extract_root.to_string_lossy()],
        });
        let report = dispatch(target, Some(data_root), params).expect("phase should succeed");

        assert_eq!(report.items_failed, 0, "report: {report:?}");
        assert_eq!(report.warnings, 0, "report: {report:?}");
        assert!(
            report.assets_written >= 1,
            "mesh_extract_roots must reach plan::Params and let the group bake: {report:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zero_mesh_cell_is_not_stamped() {
        let dir = temp_dir("zero_mesh");
        let data_root = dir.join("data");
        // No STAT, no REFR: the cell has nothing to bake.
        let target = plugin_handle_new_native("Test.esm", Some("fo4")).expect("target handle");
        ensure_interior_cell_and_child_group(target, interior_cell(0x0000_2000, "EmptyCell"))
            .expect("insert cell");

        let params = serde_json::json!({
            "include_cells": ["00002000"],
        });
        let report = dispatch(target, Some(data_root), params).expect("phase should succeed");

        assert_eq!(report.assets_written, 0, "report: {report:?}");
        assert_eq!(report.records_changed, 0, "report: {report:?}");
        assert_eq!(
            report.items_failed, 1,
            "the zero-mesh cell counts as failed: {report:?}"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).expect("target handle still present");
        let cell = find_record(&slot.parsed.root_items, "CELL", 0x0000_2000).expect("cell present");
        assert!(
            !cell
                .subrecords
                .iter()
                .any(|s| s.signature.as_str() == "XCRI"),
            "zero-mesh cell must not be stamped"
        );
        drop(store);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MINOR-2: a group whose source mesh is missing on disk (e.g. it only
    /// exists BA2-packed, which v0 doesn't read) must surface as a phase
    /// report warning, not a silent zero-asset no-op. Confirms `bake_cell`'s
    /// warnings propagate through `PhaseReport.warnings` end-to-end.
    #[test]
    fn missing_source_mesh_surfaces_as_a_phase_warning_not_a_silent_noop() {
        let dir = temp_dir("missing_mesh_phase");
        let data_root = dir.join("data");
        // Deliberately do not write anything under `data_root/meshes/...` —
        // the STAT's MODL points at a mesh that only "exists" as far as the
        // ESP is concerned.
        let target = plugin_handle_new_native("Test.esm", Some("fo4")).expect("target handle");
        ensure_interior_cell_and_child_group(target, interior_cell(0x0000_3000, "MissingMeshCell"))
            .expect("insert cell");
        insert_parsed_record(target, stat_base(0x0000_0500, "meshes\\test\\ba2_only.nif"))
            .expect("insert stat");
        insert_placed_child_into_cell_group(
            target,
            0x0000_3000,
            TEMPORARY,
            plain_refr(0x0000_0600, 0x0000_0500, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        )
        .expect("insert refr");

        let params = serde_json::json!({ "include_cells": ["00003000"] });
        let report = dispatch(target, Some(data_root), params).expect("phase should succeed");

        assert_eq!(report.assets_written, 0, "report: {report:?}");
        assert_eq!(report.records_changed, 0, "report: {report:?}");
        assert_eq!(report.items_failed, 1, "report: {report:?}");
        assert!(
            report.warnings >= 1,
            "bake_cell's warnings must propagate into the phase report: {report:?}"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).expect("target handle still present");
        let cell = find_record(&slot.parsed.root_items, "CELL", 0x0000_3000).expect("cell present");
        assert!(
            !cell
                .subrecords
                .iter()
                .any(|s| s.signature.as_str() == "XCRI"),
            "cell with an unresolvable mesh must not be stamped"
        );
        drop(store);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
