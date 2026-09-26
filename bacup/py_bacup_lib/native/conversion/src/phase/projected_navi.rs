//! Phase: rebuild_projected_navi

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

pub struct RebuildProjectedNaviPhase;

impl Phase for RebuildProjectedNaviPhase {
    fn name(&self) -> &'static str {
        "rebuild_projected_navi"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let stats = ctx
            .run
            .rebuild_projected_navi()
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
        plugin_handle_new_native,
    };
    use std::sync::atomic::AtomicBool;

    fn hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push_str(&format!("{byte:02X}"));
        }
        out
    }

    fn navmesh_geometry(parent_world: u32, cell: (i16, i16)) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes());
        data.extend_from_slice(&0xAABB_CCDD_u32.to_le_bytes());
        data.extend_from_slice(&parent_world.to_le_bytes());
        data.extend_from_slice(&cell.1.to_le_bytes());
        data.extend_from_slice(&cell.0.to_le_bytes());
        data.extend_from_slice(&3_u32.to_le_bytes());
        for vertex in [
            (8.0_f32, 18.0_f32, 28.0_f32),
            (10.0, 20.0, 30.0),
            (12.0, 22.0, 32.0),
        ] {
            data.extend_from_slice(&vertex.0.to_le_bytes());
            data.extend_from_slice(&vertex.1.to_le_bytes());
            data.extend_from_slice(&vertex.2.to_le_bytes());
        }
        data.extend_from_slice(&1_u32.to_le_bytes());
        for vertex_index in [0_u16, 1, 2] {
            data.extend_from_slice(&vertex_index.to_le_bytes());
        }
        for _ in 0..3 {
            data.extend_from_slice(&(-1_i16).to_le_bytes());
        }
        data.extend_from_slice(&0.0_f32.to_le_bytes());
        data.push(0);
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes()); // edge links
        for _ in 0..5 {
            data.extend_from_slice(&0_u32.to_le_bytes());
        }
        data
    }

    fn first_top_level_record<'a>(
        items: &'a [ParsedItem],
        signature: &str,
    ) -> Option<&'a esp_authoring_core::plugin_runtime::ParsedRecord> {
        let sig_bytes: [u8; 4] = signature.as_bytes().try_into().ok()?;
        items.iter().find_map(|item| match item {
            ParsedItem::Group(group) if group.group_type == 0 && group.label == sig_bytes => {
                group.children.iter().find_map(|child| match child {
                    ParsedItem::Record(record) if record.signature.as_str() == signature => {
                        Some(record)
                    }
                    _ => None,
                })
            }
            _ => None,
        })
    }

    #[test]
    fn phase_uses_fo4_canonical_navi_form_id() {
        let source_handle =
            plugin_handle_new_native("Source.esm", Some("fo76")).expect("source plugin handle");
        let target_handle =
            plugin_handle_new_native("Output.esm", Some("fo4")).expect("target plugin handle");
        insert_authoring_record_value(
            source_handle,
            &serde_json::json!({
                "signature": "NAVI",
                "form_id": "014B92:Source.esm",
                "subrecords": [
                    { "signature": "NVER", "data_hex": "0F000000" }
                ]
            }),
        )
        .expect("source NAVI");
        insert_authoring_record_value(
            target_handle,
            &serde_json::json!({
                "signature": "NAVM",
                "form_id": "000900:Output.esm",
                "subrecords": [
                    {
                        "signature": "NVNM",
                        "data_hex": hex(&navmesh_geometry(0x000800, (3, -2)))
                    }
                ]
            }),
        )
        .expect("target NAVM");

        let run_id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                preserve_source_ids: true,
                target_record_preflight: vec![TargetRecordPreflightRow {
                    editor_id: "TestWorld".into(),
                    signature: "WRLD".into(),
                    form_key: "000800:Output.esm".into(),
                }],
                ..RunConfig::default()
            },
        })
        .expect("conversion run");

        let report = with_run(run_id, |run| -> Result<PhaseReport, RunError> {
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
            RebuildProjectedNaviPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .expect("phase run");
        drop_run(run_id).expect("drop run");

        assert_eq!(report.records_added, 1);
        let (target_plugin, _) =
            clone_plugin_handle_state_no_py(target_handle).expect("target plugin snapshot");
        let navi = first_top_level_record(&target_plugin.root_items, "NAVI").expect("target NAVI");
        assert_eq!(navi.form_id, 0x000FF1);
    }
}
