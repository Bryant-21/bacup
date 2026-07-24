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

    fn legacy_nvmi(navmesh: u32, parent_world: u32, cell: (i16, i16)) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&navmesh.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        for value in [10.0_f32, 20.0, 30.0, 0.0] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.extend_from_slice(&0_u32.to_le_bytes()); // edge links
        data.extend_from_slice(&0_u32.to_le_bytes()); // preferred edge links
        data.extend_from_slice(&0_u32.to_le_bytes()); // door links
        data.push(0); // no island payload
        data.extend_from_slice(&0xDEAD_BEEF_u32.to_le_bytes());
        data.extend_from_slice(&parent_world.to_le_bytes());
        data.extend_from_slice(&cell.1.to_le_bytes());
        data.extend_from_slice(&cell.0.to_le_bytes());
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

    fn assert_legacy_fallout_phase_rebuilds_fo4_navi(source: Game, game_name: &str) {
        let source_handle =
            plugin_handle_new_native("Source.esm", Some(game_name)).expect("source plugin handle");
        let target_handle =
            plugin_handle_new_native("Output.esm", Some("fo4")).expect("target plugin handle");
        let retained_source_nvmi = legacy_nvmi(0x000900, 0x000800, (3, -2));
        let stale_source_nvmi = legacy_nvmi(0x000901, 0x000800, (4, -2));
        insert_authoring_record_value(
            source_handle,
            &serde_json::json!({
                "signature": "CELL",
                "form_id": "000801:Source.esm",
                "subrecords": [
                    { "signature": "DATA", "data_hex": "01" }
                ]
            }),
        )
        .expect("legacy source interior CELL");
        let mut legacy_data = Vec::new();
        for value in [0x000801_u32, 3, 1, 0, 0, 0] {
            legacy_data.extend_from_slice(&value.to_le_bytes());
        }
        let mut legacy_vertices = Vec::new();
        for vertex in [
            (0.0_f32, 0.0_f32, 0.0_f32),
            (100.0, 0.0, 0.0),
            (0.0, 100.0, 0.0),
        ] {
            legacy_vertices.extend_from_slice(&vertex.0.to_le_bytes());
            legacy_vertices.extend_from_slice(&vertex.1.to_le_bytes());
            legacy_vertices.extend_from_slice(&vertex.2.to_le_bytes());
        }
        let mut legacy_triangle = Vec::new();
        for vertex in [0_u16, 1, 2] {
            legacy_triangle.extend_from_slice(&vertex.to_le_bytes());
        }
        for edge in [-1_i16, -1, -1] {
            legacy_triangle.extend_from_slice(&edge.to_le_bytes());
        }
        legacy_triangle.extend_from_slice(&0_u16.to_le_bytes());
        legacy_triangle.extend_from_slice(&0_u16.to_le_bytes());
        insert_authoring_record_value(
            source_handle,
            &serde_json::json!({
                "signature": "NAVM",
                "form_id": "000900:Source.esm",
                "subrecords": [
                    { "signature": "NVER", "data_hex": "0B000000" },
                    { "signature": "DATA", "data_hex": hex(&legacy_data) },
                    { "signature": "NVVX", "data_hex": hex(&legacy_vertices) },
                    { "signature": "NVTR", "data_hex": hex(&legacy_triangle) },
                    { "signature": "NVGD", "data_hex": hex(&[0_u8; 36]) }
                ]
            }),
        )
        .expect("legacy split source NAVM");
        insert_authoring_record_value(
            source_handle,
            &serde_json::json!({
                "signature": "NAVI",
                "form_id": "000FF1:Source.esm",
                "subrecords": [
                    { "signature": "NVER", "data_hex": "0B000000" },
                    { "signature": "NVMI", "data_hex": hex(&retained_source_nvmi) },
                    { "signature": "NVMI", "data_hex": hex(&stale_source_nvmi) }
                ]
            }),
        )
        .expect("legacy source NAVI");

        let run_id = create_run(RunParams {
            source,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                preserve_source_ids: true,
                is_whole_plugin: true,
                ..RunConfig::default()
            },
        })
        .expect("conversion run");

        let (report, warnings) = with_run(run_id, |run| -> Result<_, RunError> {
            let report = run.translate_all()?;
            let warnings = run
                .warnings
                .iter()
                .filter_map(|symbol| run.interner.resolve(*symbol).map(str::to_string))
                .collect::<Vec<_>>();
            Ok((report, warnings))
        })
        .expect("full translation tail");
        drop_run(run_id).expect("drop run");

        assert!(report.records_translated > 0);
        let (target_plugin, _) =
            clone_plugin_handle_state_no_py(target_handle).expect("target plugin snapshot");
        fn collect_records<'a>(
            items: &'a [ParsedItem],
            signature: &str,
            out: &mut Vec<&'a esp_authoring_core::plugin_runtime::ParsedRecord>,
        ) {
            for item in items {
                match item {
                    ParsedItem::Record(record) if record.signature.as_str() == signature => {
                        out.push(record)
                    }
                    ParsedItem::Group(group) => collect_records(&group.children, signature, out),
                    _ => {}
                }
            }
        }
        let mut navms = Vec::new();
        collect_records(&target_plugin.root_items, "NAVM", &mut navms);
        let mut cells = Vec::new();
        collect_records(&target_plugin.root_items, "CELL", &mut cells);
        assert_eq!(
            navms.len(),
            1,
            "structured tail must emit one target NAVM; cells={:?}; navms={:?}; warnings={warnings:?}",
            cells
                .iter()
                .map(|record| record.form_id)
                .collect::<Vec<_>>(),
            navms
                .iter()
                .map(|record| (
                    record.form_id,
                    record
                        .subrecords
                        .iter()
                        .map(|subrecord| subrecord.signature.to_string())
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>()
        );
        let nvnm = navms[0]
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "NVNM")
            .expect("target packed NVNM");
        assert_eq!(&nvnm.data[..4], &15_u32.to_le_bytes());
        assert!(
            navms[0].subrecords.iter().all(|subrecord| !matches!(
                subrecord.signature.as_str(),
                "NVER" | "DATA" | "NVVX" | "NVTR" | "NVCA" | "NVDP" | "NVGD" | "NVEX"
            )),
            "legacy split NAVM fields must not pass through"
        );
        let navis: Vec<_> = target_plugin
            .root_items
            .iter()
            .filter_map(|item| match item {
                ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"NAVI" => {
                    Some(
                        group
                            .children
                            .iter()
                            .filter_map(|child| match child {
                                ParsedItem::Record(record)
                                    if record.signature.as_str() == "NAVI" =>
                                {
                                    Some(record)
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>(),
                    )
                }
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(navis.len(), 1, "FO4 output must contain exactly one NAVI");
        let navi = navis[0];
        let nver = navi
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "NVER")
            .expect("rebuilt NVER");
        assert_eq!(nver.data.as_ref(), 15_u32.to_le_bytes());
        let nvmis: Vec<_> = navi
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "NVMI")
            .collect();
        assert_eq!(nvmis.len(), 1, "stale legacy NVMI row must be dropped");
        assert_eq!(&nvmis[0].data[..4], &0x000900_u32.to_le_bytes());
        assert_ne!(
            nvmis[0].data.as_ref(),
            retained_source_nvmi.as_slice(),
            "the legacy NVMI bytes must be rebuilt, not carried verbatim"
        );
    }

    #[test]
    fn fnv_phase_rebuilds_one_fo4_navi_without_legacy_nvmi_rows() {
        assert_legacy_fallout_phase_rebuilds_fo4_navi(Game::Fnv, "fnv");
    }

    #[test]
    fn fo3_phase_rebuilds_one_fo4_navi_without_legacy_nvmi_rows() {
        assert_legacy_fallout_phase_rebuilds_fo4_navi(Game::Fo3, "fo3");
    }
}
