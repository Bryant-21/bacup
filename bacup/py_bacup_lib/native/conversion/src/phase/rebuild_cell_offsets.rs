//! Phase: `rebuild_cell_offsets` — regenerate WRLD OFST/CLSZ cell seek tables.
//!
//! The translator strips the FO76 source tables (their values are file offsets
//! into the FO76 layout); this rebuilds FO4-native ones against the open
//! target handle via `esp_authoring_core::rebuild_worldspace_cell_offsets`.
//! The tables encode the serialized byte layout of each worldspace group, so
//! this phase must be the LAST record mutation before the plugin is saved —
//! any later change to byte lengths inside a WRLD top-group goes stale.
//!
//! No params. Phase-contract: NO Python / GIL.

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use esp_authoring_core::plugin_runtime::{
    plugin_handle_store_ref, rebuild_worldspace_cell_offsets,
};

pub struct RebuildCellOffsetsPhase;

impl Phase for RebuildCellOffsetsPhase {
    fn name(&self) -> &'static str {
        "rebuild_cell_offsets"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let payload = {
            let mut store = plugin_handle_store_ref()
                .lock()
                .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
            let slot = store.get_mut(&ctx.run.target_handle_id).ok_or_else(|| {
                PhaseError::BadParams(format!(
                    "unknown target handle: {}",
                    ctx.run.target_handle_id
                ))
            })?;
            let payload = rebuild_worldspace_cell_offsets(&mut slot.parsed)
                .map_err(|err| PhaseError::Internal(format!("rebuild cell offsets: {err}")))?;
            if payload.worldspaces_rebuilt > 0 {
                slot.clear_record_count_cache();
                slot.invalidate_sections();
            }
            payload
        };

        for warning in &payload.warnings {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Warn,
                message: warning.clone(),
            });
        }
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "rebuilt OFST/CLSZ for {} worldspace(s): {} cells indexed, {} out of rect, {} missing XCLC, {} duplicates",
                payload.worldspaces_rebuilt,
                payload.cells_indexed,
                payload.cells_out_of_rect,
                payload.cells_missing_xclc,
                payload.duplicate_grid_cells,
            ),
        });

        Ok(PhaseReport {
            records_changed: payload.worldspaces_rebuilt,
            warnings: payload.warnings.len() as u32,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        COMPRESSED_RECORD_FLAG, ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord,
        compress_subrecords_payload, plugin_handle_close_native, plugin_handle_new_native,
        plugin_handle_save_no_py,
    };
    use serde_json::Value as JsonValue;
    use smol_str::SmolStr;

    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    const WRLD_ID: u32 = 0x0000_0F99;
    const CELL_ID: u32 = 0x0000_0200;

    fn sr(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::from(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn record(sig: &str, form_id: u32, flags: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::from(sig),
            form_id,
            flags,
            version_control: 0,
            form_version: Some(131),
            version2: Some(1),
            subrecords,
            raw_payload: None,
            parse_error: None,
        })
    }

    fn group(group_type: i32, label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label,
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    fn worldspace_items() -> Vec<ParsedItem> {
        let nam = |v: f32| v.to_le_bytes().to_vec();
        let wrld = record(
            "WRLD",
            WRLD_ID,
            0,
            vec![
                sr("EDID", b"TESTWORLD\0".to_vec()),
                sr("NAM0", [nam(-8192.0), nam(-8192.0)].concat()),
                sr("NAM9", [nam(8192.0), nam(8192.0)].concat()),
            ],
        );
        let cell_subs = vec![
            sr("DATA", vec![2, 0]),
            sr("XCLC", [0i32.to_le_bytes(), 0i32.to_le_bytes()].concat()),
        ];
        let payload = compress_subrecords_payload(&cell_subs).expect("compress cell");
        let cell = ParsedItem::Record(ParsedRecord {
            signature: SmolStr::from("CELL"),
            form_id: CELL_ID,
            flags: COMPRESSED_RECORD_FLAG,
            version_control: 0,
            form_version: Some(131),
            version2: Some(1),
            subrecords: Vec::new(),
            raw_payload: Some(Bytes::from(payload)),
            parse_error: None,
        });
        let children = group(
            1,
            WRLD_ID.to_le_bytes(),
            vec![group(
                4,
                [0, 0, 0, 0],
                vec![group(5, [0, 0, 0, 0], vec![cell])],
            )],
        );
        vec![group(0, *b"WRLD", vec![wrld, children])]
    }

    fn make_run(target_handle_id: u64) -> u64 {
        create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esp".into(),
                ..Default::default()
            },
        })
        .unwrap()
    }

    fn run_phase(target_handle: u64) -> PhaseReport {
        let run_id = make_run(target_handle);
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let params = JsonValue::Null;
        let report = with_run(run_id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let source_dir = mod_path.clone();
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            RebuildCellOffsetsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        drop_run(run_id).unwrap();
        report
    }

    #[test]
    fn rebuilds_tables_and_saved_offset_lands_on_cell_header() {
        let handle = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.root_items = worldspace_items();
        }

        let report = run_phase(handle);
        assert_eq!(report.records_changed, 1);
        assert_eq!(report.warnings, 0);

        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("Out.esp");
        plugin_handle_save_no_py(handle, out.to_str().unwrap()).unwrap();
        let bytes = std::fs::read(&out).unwrap();

        // Saved layout is TES4 record, then the WRLD top group whose first
        // child is the WRLD record. Verify the (0,0) entry points at the CELL
        // record header.
        let tes4_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let wrld_pos = 24 + tes4_size + 24;
        assert_eq!(&bytes[wrld_pos..wrld_pos + 4], b"WRLD");

        let dsize = u32::from_le_bytes(bytes[wrld_pos + 4..wrld_pos + 8].try_into().unwrap());
        let payload = &bytes[wrld_pos + 24..wrld_pos + 24 + dsize as usize];
        let ofst_at = payload
            .windows(4)
            .position(|w| w == b"OFST")
            .expect("OFST subrecord");
        let entry_base = ofst_at + 6;
        // Grid -2..2 (5x5), (0,0) => idx 12.
        let idx = 12usize;
        let rel = u32::from_le_bytes(
            payload[entry_base + idx * 4..entry_base + idx * 4 + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        assert!(rel > 0, "OFST(0,0) must be populated");
        assert_eq!(&bytes[wrld_pos + rel..wrld_pos + rel + 4], b"CELL");
        let cell_form_id = u32::from_le_bytes(
            bytes[wrld_pos + rel + 12..wrld_pos + rel + 16]
                .try_into()
                .unwrap(),
        );
        assert_eq!(cell_form_id, CELL_ID);

        plugin_handle_close_native(handle);
    }

    #[test]
    fn missing_handle_is_bad_params() {
        let run_id = make_run(0xDEAD_BEEF);
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let params = JsonValue::Null;
        let err = with_run(run_id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let source_dir = mod_path.clone();
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            Ok(match RebuildCellOffsetsPhase.run(&mut ctx) {
                Err(PhaseError::BadParams(_)) => PhaseReport::default(),
                other => panic!("expected BadParams, got {other:?}"),
            })
        })
        .unwrap();
        drop_run(run_id).unwrap();
        let _ = err;
    }
}
