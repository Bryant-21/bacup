//! Phase: emit_projected_navmeshes

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

pub struct EmitProjectedNavmeshesPhase;

impl Phase for EmitProjectedNavmeshesPhase {
    fn name(&self) -> &'static str {
        "emit_projected_navmeshes"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let stats = ctx
            .run
            .emit_projected_navmeshes()
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        Ok(PhaseReport {
            records_added: stats.records_translated,
            records_dropped: stats.records_dropped,
            warnings: stats.records_failed,
            ..PhaseReport::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{
        RunConfig, RunError, RunParams, TargetRecordPreflightRow, create_run, drop_run, with_run,
    };
    use crate::translator::Game;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, clone_plugin_handle_state_no_py, insert_authoring_record_value,
        plugin_handle_new_native, plugin_handle_replace_authoring_record_value,
        plugin_handle_replace_projected_cell_authoring_record_value,
    };
    use std::sync::atomic::AtomicBool;

    fn hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push_str(&format!("{byte:02X}"));
        }
        out
    }

    fn empty_navmesh_geometry(parent_world: u32, cell: (i16, i16)) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&parent_world.to_le_bytes());
        data.extend_from_slice(&cell.1.to_le_bytes());
        data.extend_from_slice(&cell.0.to_le_bytes());
        for _ in 0..7 {
            data.extend_from_slice(&0_u32.to_le_bytes());
        }
        data
    }

    /// One real triangle plus a door ref to `door_raw_formid` (a REFR raw
    /// form id in source space). REFRs are skip_records on the emit path, so
    /// the door-ref rewrite exercises the mapper's per-record fallback
    /// allocation — the mutation from clone-and-discard must NOT persist
    /// into the run's mapper state.
    fn door_ref_navmesh_geometry(
        parent_world: u32,
        cell: (i16, i16),
        door_raw_formid: u32,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&parent_world.to_le_bytes());
        data.extend_from_slice(&cell.1.to_le_bytes());
        data.extend_from_slice(&cell.0.to_le_bytes());
        data.extend_from_slice(&3_u32.to_le_bytes());
        for (x, y) in [(0.0_f32, 0.0_f32), (100.0, 0.0), (0.0, 100.0)] {
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
            data.extend_from_slice(&0.0_f32.to_le_bytes());
        }
        data.extend_from_slice(&1_u32.to_le_bytes());
        for v in [0_u16, 1, 2] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for l in [-1_i16, -1, -1] {
            data.extend_from_slice(&l.to_le_bytes());
        }
        data.extend_from_slice(&[0u8; 9]);
        // 0 edge links
        data.extend_from_slice(&0_u32.to_le_bytes());
        // 1 door ref: triangle 0, 4 padding bytes, REFR raw form id
        data.extend_from_slice(&1_u32.to_le_bytes());
        data.extend_from_slice(&0_i16.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
        data.extend_from_slice(&door_raw_formid.to_le_bytes());
        // cover array, cover triangle mappings, waypoints all empty
        for _ in 0..3 {
            data.extend_from_slice(&0_u32.to_le_bytes());
        }
        data
    }

    fn collect_navmesh_parent_group_types(
        items: &[ParsedItem],
        parent_group_type: Option<i32>,
        out: &mut Vec<(Option<i32>, u32)>,
    ) {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.signature.as_str() == "NAVM" => {
                    out.push((parent_group_type, record.form_id));
                }
                ParsedItem::Group(group) => {
                    collect_navmesh_parent_group_types(
                        &group.children,
                        Some(group.group_type),
                        out,
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn phase_places_navmeshes_in_projected_cell_temporary_group_with_worker_pool() {
        let source_handle =
            plugin_handle_new_native("Source.esm", Some("fo76")).expect("source plugin handle");
        let target_handle =
            plugin_handle_new_native("Output.esm", Some("fo4")).expect("target plugin handle");
        let world_payload = serde_json::json!({
            "signature": "WRLD",
            "form_id": "000800:Source.esm",
            "eid": "TestWorld",
            "subrecords": [
                { "signature": "EDID", "data_hex": "54657374576F726C6400" }
            ]
        });
        insert_authoring_record_value(source_handle, &world_payload).expect("source WRLD");
        for local in [0x000900_u32, 0x000901] {
            let navmesh_payload = serde_json::json!({
                "signature": "NAVM",
                "form_id": format!("{local:06X}:Source.esm"),
                "subrecords": [
                    {
                        "signature": "NVNM",
                        "data_hex": hex(&empty_navmesh_geometry(0x000800, (3, -2)))
                    }
                ]
            });
            insert_authoring_record_value(source_handle, &navmesh_payload).expect("source NAVM");
        }

        let target_world_payload = serde_json::json!({
            "signature": "WRLD",
            "form_id": "000800:Output.esm",
            "eid": "TestWorld",
            "subrecords": [
                { "signature": "EDID", "data_hex": "54657374576F726C6400" }
            ]
        });
        plugin_handle_replace_authoring_record_value(target_handle, &target_world_payload)
            .expect("target WRLD");
        let cell_payload = serde_json::json!({
            "signature": "CELL",
            "form_id": "000801:Output.esm",
            "eid": "TestCell",
            "subrecords": [
                { "signature": "EDID", "data_hex": "5465737443656C6C00" },
                { "signature": "XCLC", "data_hex": "03000000FEFFFFFF00000000" }
            ],
            "Landscape": {
                "form_id": "000803:Output.esm",
                "subrecords": []
            }
        });
        plugin_handle_replace_projected_cell_authoring_record_value(
            target_handle,
            &cell_payload,
            "records/WRLD/TestWorld - 000800_Output.esm/0,0/0,0/3,-2/RecordData.yaml",
        )
        .expect("projected target CELL");

        let run_id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                conversion_workers: Some(2),
                target_record_preflight: vec![TargetRecordPreflightRow {
                    editor_id: "TestWorld".into(),
                    signature: "WRLD".into(),
                    form_key: "000800:Output.esm".into(),
                }],
                ..RunConfig::default()
            },
        })
        .expect("conversion run");

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("rayon pool");
        let report = pool
            .install(|| {
                with_run(run_id, |run| -> Result<PhaseReport, RunError> {
                    let cancel = std::sync::Arc::new(AtomicBool::new(false));
                    let params = serde_json::json!({});
                    let mod_dir = std::path::PathBuf::from("/nonexistent");
                    let src_dir = std::path::PathBuf::from("/nonexistent");
                    let mut ctx = PhaseCtx {
                        run,
                        mod_path: &mod_dir,
                        source_extracted_dir: &src_dir,
                        target_extracted_dir: None,
                        target_data_dir: None,
                        params: &params,
                        cancel: &cancel,
                    };
                    EmitProjectedNavmeshesPhase
                        .run(&mut ctx)
                        .map_err(|e| RunError::InvalidConfig(e.to_string()))
                })
            })
            .expect("phase run");
        drop_run(run_id).expect("drop run");

        assert_eq!(report.records_added, 2);
        assert_eq!(report.records_dropped, 0);
        assert_eq!(report.warnings, 0);

        let (target_plugin, _) =
            clone_plugin_handle_state_no_py(target_handle).expect("target plugin snapshot");
        let mut navmesh_locations = Vec::new();
        collect_navmesh_parent_group_types(&target_plugin.root_items, None, &mut navmesh_locations);
        assert_eq!(
            navmesh_locations,
            vec![(Some(9), 0x000900), (Some(9), 0x000901)]
        );
        assert!(!target_plugin.root_items.iter().any(|item| {
            matches!(
                item,
                ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"NAVM"
            )
        }));
    }

    /// Pins the emit phase's per-record clone-and-discard mapper semantics:
    /// a door-ref REFR allocation made while preparing one navmesh must NOT
    /// persist into the run's long-lived mapper state.
    #[test]
    fn emit_discards_per_record_mapper_mutations() {
        let source_handle =
            plugin_handle_new_native("Source.esm", Some("fo76")).expect("source plugin handle");
        let target_handle =
            plugin_handle_new_native("Output.esm", Some("fo4")).expect("target plugin handle");
        insert_authoring_record_value(
            source_handle,
            &serde_json::json!({
                "signature": "WRLD",
                "form_id": "000800:Source.esm",
                "eid": "TestWorld",
                "subrecords": [
                    { "signature": "EDID", "data_hex": "54657374576F726C6400" }
                ]
            }),
        )
        .expect("source WRLD");
        insert_authoring_record_value(
            source_handle,
            &serde_json::json!({
                "signature": "NAVM",
                "form_id": "000902:Source.esm",
                "subrecords": [
                    {
                        "signature": "NVNM",
                        "data_hex": hex(&door_ref_navmesh_geometry(0x000800, (3, -2), 0x0000_0950))
                    }
                ]
            }),
        )
        .expect("source NAVM");

        plugin_handle_replace_authoring_record_value(
            target_handle,
            &serde_json::json!({
                "signature": "WRLD",
                "form_id": "000800:Output.esm",
                "eid": "TestWorld",
                "subrecords": [
                    { "signature": "EDID", "data_hex": "54657374576F726C6400" }
                ]
            }),
        )
        .expect("target WRLD");
        plugin_handle_replace_projected_cell_authoring_record_value(
            target_handle,
            &serde_json::json!({
                "signature": "CELL",
                "form_id": "000801:Output.esm",
                "eid": "TestCell",
                "subrecords": [
                    { "signature": "EDID", "data_hex": "5465737443656C6C00" },
                    { "signature": "XCLC", "data_hex": "03000000FEFFFFFF00000000" }
                ],
                "Landscape": {
                    "form_id": "000803:Output.esm",
                    "subrecords": []
                }
            }),
            "records/WRLD/TestWorld - 000800_Output.esm/0,0/0,0/3,-2/RecordData.yaml",
        )
        .expect("projected target CELL");

        let run_id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                conversion_workers: Some(2),
                target_record_preflight: vec![TargetRecordPreflightRow {
                    editor_id: "TestWorld".into(),
                    signature: "WRLD".into(),
                    form_key: "000800:Output.esm".into(),
                }],
                ..RunConfig::default()
            },
        })
        .expect("conversion run");

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("rayon pool");
        let report = pool
            .install(|| {
                with_run(run_id, |run| -> Result<PhaseReport, RunError> {
                    let cancel = std::sync::Arc::new(AtomicBool::new(false));
                    let params = serde_json::json!({});
                    let mod_dir = std::path::PathBuf::from("/nonexistent");
                    let src_dir = std::path::PathBuf::from("/nonexistent");
                    let mut ctx = PhaseCtx {
                        run,
                        mod_path: &mod_dir,
                        source_extracted_dir: &src_dir,
                        target_extracted_dir: None,
                        target_data_dir: None,
                        params: &params,
                        cancel: &cancel,
                    };
                    EmitProjectedNavmeshesPhase
                        .run(&mut ctx)
                        .map_err(|e| RunError::InvalidConfig(e.to_string()))
                })
            })
            .expect("phase run");

        assert_eq!(report.records_added, 1);
        assert_eq!(report.records_dropped, 0);

        with_run(run_id, |run| -> Result<(), RunError> {
            let src_plugin = run.interner.intern("Source.esm");
            let navm_fk = crate::ids::FormKey {
                local: 0x000902,
                plugin: src_plugin,
            };
            let refr_fk = crate::ids::FormKey {
                local: 0x000950,
                plugin: src_plugin,
            };
            let state = run.mapper_state.as_ref().expect("mapper state");
            assert!(
                state.source_to_target.contains_key(&navm_fk),
                "pre-pass NAVM allocation must persist in the run mapper state"
            );
            assert!(
                !state.source_to_target.contains_key(&refr_fk),
                "per-record door-ref REFR allocation must be discarded"
            );
            // Exactly the preflight WRLD eid-seed (000800) and the pre-pass
            // NAVM allocation (000902) may persist — nothing from the
            // per-record prepare.
            let mut keys: Vec<u32> = state.source_to_target.keys().map(|k| k.local).collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                vec![0x000800, 0x000902],
                "only the preflight WRLD seed + pre-pass NAVM mapping may persist"
            );
            Ok(())
        })
        .expect("inspect run");
        drop_run(run_id).expect("drop run");
    }
}
