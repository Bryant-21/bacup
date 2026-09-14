//! Final record mutations that share one open target-plugin session.

use std::time::Instant;

use esp_authoring_core::plugin_runtime::plugin_handle_repair_term_marker_parameters_from_source_no_py;

use crate::phase::rebuild_cell_offsets::RebuildCellOffsetsPhase;
use crate::phase::regenerate_modt::RegenerateModtPhase;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::run::OwnedPluginHandle;
use crate::translator::Game;

#[derive(Clone, Copy)]
struct Selection {
    regenerate_modt: bool,
    repair_term_markers: bool,
    rebuild_cell_offsets: bool,
}

#[derive(Default)]
struct TermReport {
    modified: u32,
    removed: usize,
    inserted: usize,
    source_open_ms: u64,
    scan_repair_ms: u64,
}

struct FinalReports {
    modt: PhaseReport,
    term: TermReport,
    offsets: PhaseReport,
    modt_ms: u64,
    term_ms: u64,
    offsets_ms: u64,
}

fn selected(params: &serde_json::Value, name: &str, default: bool) -> bool {
    params
        .get(name)
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

fn manifest_entry_count(params: &serde_json::Value) -> Option<u64> {
    params
        .get("manifest_entries")
        .and_then(|value| value.as_u64())
        .or_else(|| {
            params
                .get("manifest")
                .and_then(|value| value.as_object())
                .map(|entries| entries.len() as u64)
        })
}

fn run_selected<M, T, O>(
    ctx: &mut PhaseCtx<'_>,
    selection: Selection,
    mut regenerate_modt: M,
    mut repair_term_markers: T,
    mut rebuild_cell_offsets: O,
) -> Result<FinalReports, PhaseError>
where
    M: FnMut(&mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError>,
    T: FnMut(&mut PhaseCtx<'_>) -> Result<TermReport, PhaseError>,
    O: FnMut(&mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError>,
{
    ctx.check_cancel()?;
    let started = Instant::now();
    let modt = if selection.regenerate_modt {
        regenerate_modt(ctx)?
    } else {
        PhaseReport::default()
    };
    let modt_ms = started.elapsed().as_millis() as u64;

    ctx.check_cancel()?;
    let started = Instant::now();
    let term = if selection.repair_term_markers {
        repair_term_markers(ctx)?
    } else {
        TermReport::default()
    };
    let term_ms = started.elapsed().as_millis() as u64;

    ctx.check_cancel()?;
    let started = Instant::now();
    let offsets = if selection.rebuild_cell_offsets {
        rebuild_cell_offsets(ctx)?
    } else {
        PhaseReport::default()
    };
    let offsets_ms = started.elapsed().as_millis() as u64;
    ctx.check_cancel()?;

    Ok(FinalReports {
        modt,
        term,
        offsets,
        modt_ms,
        term_ms,
        offsets_ms,
    })
}

fn repair_term_markers(ctx: &mut PhaseCtx<'_>) -> Result<TermReport, PhaseError> {
    let source_open_started = Instant::now();
    let opened_source;
    let source_handle = if let Some(source_handle) = ctx.run.source_handle() {
        source_handle
    } else {
        let source_path = ctx
            .params
            .get("source_plugin_path")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                PhaseError::BadParams(
                    "finalize_plugin_records: TERM repair requires source_plugin_path".into(),
                )
            })?;
        opened_source = OwnedPluginHandle::load(
            std::path::Path::new(source_path),
            ctx.run.source.as_str(),
            None,
        )
        .map_err(|error| PhaseError::BadParams(format!("TERM source plugin: {error}")))?;
        opened_source.id()
    };
    let source_open_ms = source_open_started.elapsed().as_millis() as u64;
    let scan_repair_started = Instant::now();
    let changes = plugin_handle_repair_term_marker_parameters_from_source_no_py(
        ctx.run.target_handle_id,
        source_handle,
        false,
    )
    .map_err(|error| PhaseError::Internal(format!("repair TERM marker parameters: {error}")))?;
    let remaining = plugin_handle_repair_term_marker_parameters_from_source_no_py(
        ctx.run.target_handle_id,
        source_handle,
        true,
    )
    .map_err(|error| PhaseError::Internal(format!("audit TERM marker parameters: {error}")))?;
    if !remaining.is_empty() {
        return Err(PhaseError::Internal(format!(
            "TERM marker post-build audit found {} unrepaired record(s)",
            remaining.len()
        )));
    }
    Ok(TermReport {
        modified: changes.len() as u32,
        removed: changes.iter().map(|change| change.2).sum(),
        inserted: changes.iter().map(|change| change.3).sum(),
        source_open_ms,
        scan_repair_ms: scan_repair_started.elapsed().as_millis() as u64,
    })
}

pub struct FinalizePluginRecordsPhase;

impl Phase for FinalizePluginRecordsPhase {
    fn name(&self) -> &'static str {
        "finalize_plugin_records"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let selection = Selection {
            regenerate_modt: selected(ctx.params, "regenerate_modt", true),
            repair_term_markers: selected(ctx.params, "repair_term_markers", false),
            rebuild_cell_offsets: selected(ctx.params, "rebuild_cell_offsets", true),
        };
        if !selection.regenerate_modt
            && !selection.repair_term_markers
            && !selection.rebuild_cell_offsets
        {
            return Err(PhaseError::BadParams(
                "finalize_plugin_records: no operations selected".into(),
            ));
        }
        if ctx.run.target != Game::Fo4 {
            return Err(PhaseError::BadParams(
                "finalize_plugin_records requires an FO4 target".into(),
            ));
        }
        if selection.repair_term_markers
            && (ctx.run.source != Game::Fo76
                || (ctx.run.source_handle().is_none()
                    && ctx
                        .params
                        .get("source_plugin_path")
                        .and_then(|value| value.as_str())
                        .map_or(true, |value| value.trim().is_empty())))
        {
            return Err(PhaseError::BadParams(
                "finalize_plugin_records TERM repair requires an FO76 source plugin".into(),
            ));
        }

        let reports = run_selected(
            ctx,
            selection,
            |ctx| RegenerateModtPhase.run(ctx),
            repair_term_markers,
            |ctx| RebuildCellOffsetsPhase.run(ctx),
        )?;
        let manifest_entries = manifest_entry_count(ctx.params)
            .map(|count| count.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "post-asset MODT regeneration: manifest_entries={manifest_entries} records_changed={}",
                reports.modt.records_changed
            ),
        });
        if selection.repair_term_markers {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Info,
                message: format!(
                    "TERM marker post-build repair: modified={} removed={} inserted={} audit_modified=0",
                    reports.term.modified, reports.term.removed, reports.term.inserted
                ),
            });
        }
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "final plugin consolidation timing: modt_apply_ms={} term_repair_ms={} cell_offsets_ms={}",
                reports.modt_ms, reports.term_ms, reports.offsets_ms
            ),
        });
        if selection.repair_term_markers {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Info,
                message: format!(
                    "TERM marker timing: source_open_ms={} scan_repair_audit_ms={}",
                    reports.term.source_open_ms, reports.term.scan_repair_ms
                ),
            });
        }

        Ok(PhaseReport {
            assets_written: reports.modt.assets_written,
            records_changed: reports
                .modt
                .records_changed
                .saturating_add(reports.term.modified)
                .saturating_add(reports.offsets.records_changed),
            warnings: reports
                .modt
                .warnings
                .saturating_add(reports.offsets.warnings),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        COMPRESSED_RECORD_FLAG, ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord,
        compress_subrecords_payload, plugin_handle_close_native, plugin_handle_load_no_py,
        plugin_handle_new_native, plugin_handle_save_no_py, plugin_handle_store_ref,
    };
    use serde_json::Value as JsonValue;
    use smol_str::SmolStr;

    use crate::modt_compute::encode_modt;
    use crate::modt_manifest::{ManifestTexture, MeshModtEntry};
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};

    const WRLD_ID: u32 = 0x0000_0F99;
    const CELL_ID: u32 = 0x0000_0200;

    fn subrecord(signature: &str, data: impl Into<Bytes>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::from(signature),
            data: data.into(),
            semantic_type: None,
        }
    }

    fn record(signature: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::from(signature),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: Some(1),
            subrecords,
            raw_payload: None,
            parse_error: None,
        })
    }

    fn compressed_record(
        signature: &str,
        form_id: u32,
        subrecords: Vec<ParsedSubrecord>,
    ) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::from(signature),
            form_id,
            flags: COMPRESSED_RECORD_FLAG,
            version_control: 0,
            form_version: Some(131),
            version2: Some(1),
            subrecords: Vec::new(),
            raw_payload: Some(Bytes::from(
                compress_subrecords_payload(&subrecords).expect("compress fixture record"),
            )),
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

    fn record_by_form_id(items: &[ParsedItem], form_id: u32) -> Option<&ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id == form_id => return Some(record),
                ParsedItem::Group(group) => {
                    if let Some(record) = record_by_form_id(&group.children, form_id) {
                        return Some(record);
                    }
                }
                ParsedItem::Record(_) => {}
            }
        }
        None
    }

    fn target_items() -> Vec<ParsedItem> {
        let nam = |value: f32| value.to_le_bytes();
        let stat = record(
            "STAT",
            0x0000_0800,
            vec![
                subrecord("EDID", Bytes::from_static(b"ParityStatic\0")),
                subrecord("MODL", Bytes::from_static(b"novel\\mesh.nif\0")),
                subrecord("MODT", Bytes::from_static(b"SOURCE-MODT")),
            ],
        );
        let terminal = compressed_record(
            "TERM",
            0x0007_6E6C,
            vec![
                subrecord(
                    "EDID",
                    Bytes::from_static(b"Storm_UpperAtrium_ClinicTerminal\0"),
                ),
                subrecord(
                    "SNAM",
                    Bytes::copy_from_slice(&0x0009_80FB_u32.to_le_bytes()),
                ),
                subrecord(
                    "XMRK",
                    Bytes::from_static(b"Markers\\MarkerDeskTerminal3rdP.nif\0"),
                ),
                subrecord(
                    "SNAM",
                    Bytes::copy_from_slice(&0x0008_0000_u32.to_le_bytes()),
                ),
            ],
        );
        let world = record(
            "WRLD",
            WRLD_ID,
            vec![
                subrecord("EDID", Bytes::from_static(b"TESTWORLD\0")),
                subrecord(
                    "NAM0",
                    Bytes::copy_from_slice(&[nam(-8192.0), nam(-8192.0)].concat()),
                ),
                subrecord(
                    "NAM9",
                    Bytes::copy_from_slice(&[nam(8192.0), nam(8192.0)].concat()),
                ),
            ],
        );
        let cell = record(
            "CELL",
            CELL_ID,
            vec![
                subrecord("DATA", Bytes::from_static(&[2, 0])),
                subrecord(
                    "XCLC",
                    Bytes::copy_from_slice(&[0_i32.to_le_bytes(), 0_i32.to_le_bytes()].concat()),
                ),
            ],
        );
        let world_children = group(
            1,
            WRLD_ID.to_le_bytes(),
            vec![group(
                4,
                [0, 0, 0, 0],
                vec![group(5, [0, 0, 0, 0], vec![cell])],
            )],
        );
        vec![
            group(0, *b"STAT", vec![stat]),
            group(0, *b"TERM", vec![terminal]),
            group(0, *b"WRLD", vec![world, world_children]),
        ]
    }

    fn source_items() -> Vec<ParsedItem> {
        let mut marker_parameters = Vec::new();
        marker_parameters.extend_from_slice(&1.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&(-59.0_f32).to_le_bytes());
        marker_parameters.extend_from_slice(&1.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&0.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&0_u32.to_le_bytes());
        marker_parameters.extend_from_slice(&[0xFF, 1, 0, 0]);
        vec![group(
            0,
            *b"TERM",
            vec![compressed_record(
                "TERM",
                0x0007_6E6C,
                vec![
                    subrecord(
                        "EDID",
                        Bytes::from_static(b"Storm_UpperAtrium_ClinicTerminal\0"),
                    ),
                    subrecord("ZNAM", Bytes::from(marker_parameters)),
                ],
            )],
        )]
    }

    fn write_fixture(path: &Path, game: &str, items: Vec<ParsedItem>) {
        let handle = plugin_handle_new_native(
            path.file_name().and_then(|name| name.to_str()).unwrap(),
            Some(game),
        )
        .unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.header.next_object_id = 0x0007_6E6D;
            slot.parsed.root_items = items;
        }
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(handle);
    }

    fn run_phase_with_events(
        phase: &dyn Phase,
        target_handle_id: u64,
        source_handle_id: u64,
        params: &JsonValue,
        mod_path: &Path,
    ) -> (PhaseReport, Vec<PhaseEvent>) {
        let run_id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id,
            target_handle_id,
            master_handle_ids: vec![],
            config: RunConfig::default(),
        })
        .unwrap();
        let result = with_run(
            run_id,
            |run| -> Result<(PhaseReport, Vec<PhaseEvent>), RunError> {
                let cancel = Arc::new(AtomicBool::new(false));
                let mut ctx = PhaseCtx {
                    run,
                    mod_path,
                    source_extracted_dir: mod_path,
                    target_extracted_dir: None,
                    target_data_dir: None,
                    params,
                    cancel: &cancel,
                };
                let report = phase
                    .run(&mut ctx)
                    .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                let events = ctx.run.event_rx.try_iter().collect();
                Ok((report, events))
            },
        )
        .unwrap();
        drop_run(run_id).unwrap();
        result
    }

    fn run_phase(
        phase: &dyn Phase,
        target_handle_id: u64,
        source_handle_id: u64,
        params: &JsonValue,
        mod_path: &Path,
    ) -> PhaseReport {
        run_phase_with_events(phase, target_handle_id, source_handle_id, params, mod_path).0
    }

    fn reload(path: &Path, game: &str) -> u64 {
        plugin_handle_load_no_py(path.to_str().unwrap(), Some(game), None, None, false).unwrap()
    }

    fn reload_eager(path: &Path, game: &str) -> u64 {
        plugin_handle_load_no_py(path.to_str().unwrap(), Some(game), None, None, true).unwrap()
    }

    fn assert_offset_lands_on_cell(bytes: &[u8]) {
        let tes4_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let wrld_top_group = bytes
            .windows(12)
            .enumerate()
            .skip(24 + tes4_size)
            .find(|(_, header)| &header[..4] == b"GRUP" && &header[8..12] == b"WRLD")
            .map(|(index, _)| index)
            .expect("WRLD top group");
        let wrld_group = wrld_top_group + 24;
        let payload_size =
            u32::from_le_bytes(bytes[wrld_group + 4..wrld_group + 8].try_into().unwrap()) as usize;
        let payload = &bytes[wrld_group + 24..wrld_group + 24 + payload_size];
        let ofst = payload
            .windows(4)
            .position(|signature| signature == b"OFST")
            .expect("OFST subrecord");
        let entry_base = ofst + 6;
        let relative = u32::from_le_bytes(
            payload[entry_base + 12 * 4..entry_base + 13 * 4]
                .try_into()
                .unwrap(),
        ) as usize;
        assert!(relative > 0);
        assert_eq!(
            &bytes[wrld_group + relative..wrld_group + relative + 4],
            b"CELL"
        );
        assert_eq!(
            u32::from_le_bytes(
                bytes[wrld_group + relative + 12..wrld_group + relative + 16]
                    .try_into()
                    .unwrap(),
            ),
            CELL_ID
        );
    }

    fn run_with_cancel_after(child: &str) -> Result<(bool, bool, bool), PhaseError> {
        let run_id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 1,
            target_handle_id: 2,
            master_handle_ids: vec![],
            config: RunConfig::default(),
        })
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let cancelled = with_run(run_id, |run| -> Result<(bool, bool, bool), RunError> {
            let mut term_ran = false;
            let mut offsets_ran = false;
            let cancel = Arc::new(AtomicBool::new(false));
            let params = JsonValue::Null;
            let path = std::path::Path::new(".");
            let mut ctx = PhaseCtx {
                run,
                mod_path: path,
                source_extracted_dir: path,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            let result = run_selected(
                &mut ctx,
                Selection {
                    regenerate_modt: true,
                    repair_term_markers: true,
                    rebuild_cell_offsets: true,
                },
                |ctx| {
                    if child == "modt" {
                        ctx.cancel.store(true, Ordering::Relaxed);
                    }
                    Ok(PhaseReport::default())
                },
                |ctx| {
                    term_ran = true;
                    if child == "term" {
                        ctx.cancel.store(true, Ordering::Relaxed);
                    }
                    Ok(TermReport::default())
                },
                |ctx| {
                    offsets_ran = true;
                    if child == "offsets" {
                        ctx.cancel.store(true, Ordering::Relaxed);
                    }
                    Ok(PhaseReport::default())
                },
            );
            Ok((
                matches!(result, Err(PhaseError::Cancelled)),
                term_ran,
                offsets_ran,
            ))
        })
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
        drop_run(run_id).map_err(|error| PhaseError::Internal(error.to_string()))?;
        Ok(cancelled)
    }

    #[test]
    fn cancellation_between_children_stops_remaining_mutations() {
        assert_eq!(run_with_cancel_after("modt").unwrap(), (true, false, false));
        assert_eq!(run_with_cancel_after("term").unwrap(), (true, true, false));
        assert_eq!(
            run_with_cancel_after("offsets").unwrap(),
            (true, true, true)
        );
    }

    #[test]
    fn consolidated_output_matches_checkpointed_phases_byte_for_byte() {
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("Source.esm");
        let initial_path = temp.path().join("Initial.esm");
        let baseline_modt_path = temp.path().join("BaselineModt.esm");
        let baseline_term_path = temp.path().join("BaselineTerm.esm");
        let baseline_path = temp.path().join("Baseline.esm");
        let candidate_path = temp.path().join("Candidate.esm");
        write_fixture(&source_path, "fo76", source_items());
        write_fixture(&initial_path, "fo4", target_items());

        let expected_modt_entry = MeshModtEntry {
            materials: vec!["materials\\novel\\mesh.bgsm".to_string()],
            textures: vec![
                ManifestTexture {
                    path: "textures\\novel\\mesh_d.dds".to_string(),
                    role: "diffuse".to_string(),
                },
                ManifestTexture {
                    path: "textures\\novel\\mesh_n.dds".to_string(),
                    role: "normal".to_string(),
                },
            ],
            addon_nodes: vec![],
        };
        let expected_modt = encode_modt(&expected_modt_entry);
        let params = serde_json::json!({
            "manifest": {
                "novel/mesh.nif": expected_modt_entry
            },
            "is_upgrade": false
        });

        let mut baseline_handle = reload(&initial_path, "fo4");
        run_phase(
            &RegenerateModtPhase,
            baseline_handle,
            0,
            &params,
            temp.path(),
        );
        plugin_handle_save_no_py(baseline_handle, baseline_modt_path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(baseline_handle);
        baseline_handle = reload_eager(&baseline_modt_path, "fo4");

        let source_handle = reload(&source_path, "fo76");
        let changes = plugin_handle_repair_term_marker_parameters_from_source_no_py(
            baseline_handle,
            source_handle,
            false,
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        plugin_handle_save_no_py(baseline_handle, baseline_term_path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(baseline_handle);
        plugin_handle_close_native(source_handle);
        baseline_handle = reload_eager(&baseline_term_path, "fo4");
        let source_handle = reload(&source_path, "fo76");
        assert!(
            plugin_handle_repair_term_marker_parameters_from_source_no_py(
                baseline_handle,
                source_handle,
                true,
            )
            .unwrap()
            .is_empty()
        );
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(baseline_handle);
        baseline_handle = reload(&baseline_term_path, "fo4");
        run_phase(
            &RebuildCellOffsetsPhase,
            baseline_handle,
            0,
            &JsonValue::Null,
            temp.path(),
        );
        plugin_handle_save_no_py(baseline_handle, baseline_path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(baseline_handle);

        let candidate_handle = reload(&initial_path, "fo4");
        let candidate_params = serde_json::json!({
            "manifest": params["manifest"].clone(),
            "is_upgrade": false,
            "regenerate_modt": true,
            "repair_term_markers": true,
            "rebuild_cell_offsets": true,
            "source_plugin_path": source_path,
        });
        let (report, events) = run_phase_with_events(
            &FinalizePluginRecordsPhase,
            candidate_handle,
            0,
            &candidate_params,
            temp.path(),
        );
        assert_eq!(report.records_changed, 3);
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log { message, .. }
                if message.contains("manifest_entries=1 records_changed=1")
        )));
        let actual_modt = {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&candidate_handle).unwrap();
            record_by_form_id(&slot.parsed.root_items, 0x0000_0800)
                .unwrap()
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "MODT")
                .unwrap()
                .data
                .to_vec()
        };
        assert_eq!(actual_modt, expected_modt);
        assert_eq!(
            actual_modt[..20]
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect::<Vec<_>>(),
            vec![4, 2, 0, 1, 1]
        );
        plugin_handle_save_no_py(candidate_handle, candidate_path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(candidate_handle);

        let baseline_bytes = std::fs::read(&baseline_path).unwrap();
        let candidate_bytes = std::fs::read(&candidate_path).unwrap();
        assert_eq!(candidate_bytes, baseline_bytes);
        assert_offset_lands_on_cell(&candidate_bytes);
    }
}
