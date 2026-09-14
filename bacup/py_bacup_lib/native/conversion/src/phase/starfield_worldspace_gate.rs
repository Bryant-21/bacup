//! Phase: `starfield_worldspace_gate` — drop Starfield worldspaces (and every
//! CELL/REFR/etc. record nested under them) that have no baked terrain. A WRLD
//! converts only if `<terrain_dir>/<edid.lower()>.btd`
//! exists; WRLDs without one (procedural planet worldspaces, unbaked
//! overlay/template tiles) are pruned from the source plugin before cell
//! slicing so their subtrees never reach the translator.
//!
//! Params: `{"terrain_dir": "<abs path>"}`. Phase-contract: NO Python / GIL.

use std::collections::HashSet;
use std::path::Path;

use esp_authoring_core::plugin_runtime::{
    ParsedGroup, ParsedItem, ParsedRecord, plugin_handle_remove_records_native,
    plugin_handle_store_ref,
};

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

const TOP_GROUP_TYPE: i32 = 0;
const WORLD_CHILDREN_GROUP_TYPE: i32 = 1;

pub struct StarfieldWorldspaceGatePhase;

impl Phase for StarfieldWorldspaceGatePhase {
    fn name(&self) -> &'static str {
        "starfield_worldspace_gate"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let terrain_dir = ctx
            .params
            .get("terrain_dir")
            .and_then(|value| value.as_str())
            .ok_or_else(|| PhaseError::BadParams("missing terrain_dir".into()))?;
        let terrain_dir = Path::new(terrain_dir);

        let source_handle_id = ctx.run.source_handle().ok_or_else(|| {
            PhaseError::Internal("starfield_worldspace_gate requires a source plugin".into())
        })?;

        let (drop_form_ids, dropped_editor_ids) = {
            let store = plugin_handle_store_ref()
                .lock()
                .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
            let slot = store.get(&source_handle_id).ok_or_else(|| {
                PhaseError::Internal(format!("unknown source plugin handle: {source_handle_id}"))
            })?;
            ungated_worldspaces(&slot.parsed.root_items, terrain_dir)
        };

        for editor_id in &dropped_editor_ids {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Info,
                message: format!(
                    "starfield_worldspace_gate: dropped worldspace {editor_id} \
                     (no terrain/{}.btd found)",
                    editor_id.to_ascii_lowercase()
                ),
            });
        }

        if drop_form_ids.is_empty() {
            return Ok(PhaseReport::default());
        }

        let removed = plugin_handle_remove_records_native(
            source_handle_id,
            drop_form_ids.into_iter().collect(),
        )
        .map_err(|error| PhaseError::Internal(error.to_string()))?;

        Ok(PhaseReport {
            records_dropped: removed as u32,
            ..PhaseReport::default()
        })
    }
}

fn editor_id(record: &ParsedRecord) -> Option<String> {
    let subrecord = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "EDID")?;
    let bytes = subrecord.data.as_ref();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let editor_id = std::str::from_utf8(&bytes[..end]).ok()?;
    (!editor_id.trim().is_empty()).then(|| editor_id.to_owned())
}

fn world_children_group(wrld_group: &ParsedGroup, world_form_id: u32) -> Option<&ParsedGroup> {
    wrld_group.children.iter().find_map(|item| match item {
        ParsedItem::Group(group)
            if group.group_type == WORLD_CHILDREN_GROUP_TYPE
                && u32::from_le_bytes(group.label) == world_form_id =>
        {
            Some(group)
        }
        _ => None,
    })
}

fn collect_all_form_ids(items: &[ParsedItem], out: &mut HashSet<u32>) {
    for item in items {
        match item {
            ParsedItem::Group(group) => collect_all_form_ids(&group.children, out),
            ParsedItem::Record(record) => {
                out.insert(record.form_id);
            }
        }
    }
}

/// WRLD records under the top `WRLD` group whose lowercased EditorID has no
/// matching `<terrain_dir>/<editorid>.btd`. Returns the full drop set (each
/// ungated WRLD's own form-id plus every record nested under its
/// world-children group) and the ungated WRLDs' EditorIDs, in tree order.
fn ungated_worldspaces(
    root_items: &[ParsedItem],
    terrain_dir: &Path,
) -> (HashSet<u32>, Vec<String>) {
    let mut drop_form_ids = HashSet::new();
    let mut dropped_editor_ids = Vec::new();

    let Some(wrld_top) = root_items.iter().find_map(|item| match item {
        ParsedItem::Group(group)
            if group.group_type == TOP_GROUP_TYPE && group.label == *b"WRLD" =>
        {
            Some(group)
        }
        _ => None,
    }) else {
        return (drop_form_ids, dropped_editor_ids);
    };

    for item in &wrld_top.children {
        let ParsedItem::Record(record) = item else {
            continue;
        };
        if record.signature.as_str() != "WRLD" {
            continue;
        }
        let Some(eid) = editor_id(record) else {
            // A BTD is keyed by WRLD EditorID.  Without an EDID there is no
            // deterministic terrain asset this world can own, so admitting it
            // would bypass the terrain gate entirely.
            drop_form_ids.insert(record.form_id);
            if let Some(children) = world_children_group(wrld_top, record.form_id) {
                collect_all_form_ids(&children.children, &mut drop_form_ids);
            }
            dropped_editor_ids.push(format!("<missing EDID:{:06X}>", record.form_id));
            continue;
        };
        let btd_path = terrain_dir.join(format!("{}.btd", eid.to_ascii_lowercase()));
        if btd_path.is_file() {
            continue;
        }
        drop_form_ids.insert(record.form_id);
        if let Some(children) = world_children_group(wrld_top, record.form_id) {
            collect_all_form_ids(&children.children, &mut drop_form_ids);
        }
        dropped_editor_ids.push(eid);
    }

    (drop_form_ids, dropped_editor_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedSubrecord, plugin_handle_close_native, plugin_handle_new_native,
    };
    use serde_json::Value as JsonValue;
    use smol_str::SmolStr;

    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    const AKILA_WRLD_ID: u32 = 0x0000_0800;
    const AKILA_CELL_ID: u32 = 0x0000_0801;
    const PROC_WRLD_ID: u32 = 0x0000_0900;
    const PROC_CELL_ID: u32 = 0x0000_0901;

    fn sr(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::from(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn record(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::from(sig),
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

    fn group(group_type: i32, label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label,
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    fn worldspace(wrld_id: u32, cell_id: u32, world_editor_id: &str) -> Vec<ParsedItem> {
        let wrld = record(
            "WRLD",
            wrld_id,
            vec![sr("EDID", format!("{world_editor_id}\0").into_bytes())],
        );
        let cell = record(
            "CELL",
            cell_id,
            vec![sr(
                "XCLC",
                [0i32.to_le_bytes(), 0i32.to_le_bytes()].concat(),
            )],
        );
        let children = group(WORLD_CHILDREN_GROUP_TYPE, wrld_id.to_le_bytes(), vec![cell]);
        vec![wrld, children]
    }

    fn make_run(source_handle_id: u64) -> u64 {
        create_run(RunParams {
            source: Game::Starfield,
            target: Game::Fo4,
            source_handle_id,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Starfield_Ported.esm".into(),
                ..Default::default()
            },
        })
        .unwrap()
    }

    fn run_phase(source_handle: u64, terrain_dir: &Path) -> (PhaseReport, Vec<PhaseEvent>) {
        let run_id = make_run(source_handle);
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let params = serde_json::json!({ "terrain_dir": terrain_dir.to_string_lossy() });
        let result = with_run(
            run_id,
            |run| -> Result<(PhaseReport, Vec<PhaseEvent>), RunError> {
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
                let report = StarfieldWorldspaceGatePhase
                    .run(&mut ctx)
                    .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
                let mut events = Vec::new();
                while let Ok(event) = ctx.run.event_rx.try_recv() {
                    events.push(event);
                }
                Ok((report, events))
            },
        )
        .unwrap();
        drop_run(run_id).unwrap();
        result
    }

    #[test]
    fn drops_worldspace_without_matching_btd_and_its_cell() {
        let handle = plugin_handle_new_native("Starfield.esm", Some("starfield")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            let mut items = worldspace(AKILA_WRLD_ID, AKILA_CELL_ID, "AkilaCity");
            items.extend(worldspace(PROC_WRLD_ID, PROC_CELL_ID, "ProcPlanet01"));
            slot.parsed.root_items = vec![group(TOP_GROUP_TYPE, *b"WRLD", items)];
        }

        let terrain_dir = tempfile::tempdir().unwrap();
        std::fs::write(terrain_dir.path().join("akilacity.btd"), b"").unwrap();

        let (report, events) = run_phase(handle, terrain_dir.path());

        assert_eq!(report.records_dropped, 2);
        assert!(
            events.iter().any(|event| matches!(
                event,
                PhaseEvent::Log { level: LogLevel::Info, message, .. }
                    if message.contains("ProcPlanet01")
            )),
            "expected a Log event naming ProcPlanet01: {events:?}"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle).unwrap();
        let mut remaining = HashSet::new();
        collect_all_form_ids(&slot.parsed.root_items, &mut remaining);
        assert!(remaining.contains(&AKILA_WRLD_ID), "AkilaCity WRLD kept");
        assert!(remaining.contains(&AKILA_CELL_ID), "AkilaCity CELL kept");
        assert!(
            !remaining.contains(&PROC_WRLD_ID),
            "ProcPlanet01 WRLD dropped"
        );
        assert!(
            !remaining.contains(&PROC_CELL_ID),
            "ProcPlanet01 CELL dropped"
        );
        drop(store);

        plugin_handle_close_native(handle);
    }

    #[test]
    fn drops_worldspace_without_editor_id_and_its_subtree() {
        let nameless_wrld_id = 0x0000_0A00;
        let nameless_cell_id = 0x0000_0A01;
        let nameless_ref_id = 0x0000_0A02;
        let root = vec![group(
            TOP_GROUP_TYPE,
            *b"WRLD",
            vec![
                record("WRLD", nameless_wrld_id, vec![]),
                group(
                    WORLD_CHILDREN_GROUP_TYPE,
                    nameless_wrld_id.to_le_bytes(),
                    vec![
                        record("CELL", nameless_cell_id, vec![]),
                        group(
                            6,
                            nameless_cell_id.to_le_bytes(),
                            vec![record("REFR", nameless_ref_id, vec![])],
                        ),
                    ],
                ),
            ],
        )];
        let terrain_dir = tempfile::tempdir().unwrap();

        let (dropped, editor_ids) = ungated_worldspaces(&root, terrain_dir.path());

        assert_eq!(
            dropped,
            HashSet::from([nameless_wrld_id, nameless_cell_id, nameless_ref_id])
        );
        assert_eq!(editor_ids, ["<missing EDID:000A00>"]);
    }

    #[test]
    fn drops_worldspace_with_empty_editor_id() {
        let root = vec![group(
            TOP_GROUP_TYPE,
            *b"WRLD",
            vec![record("WRLD", PROC_WRLD_ID, vec![sr("EDID", vec![0])])],
        )];
        let terrain_dir = tempfile::tempdir().unwrap();

        let (dropped, editor_ids) = ungated_worldspaces(&root, terrain_dir.path());

        assert_eq!(dropped, HashSet::from([PROC_WRLD_ID]));
        assert_eq!(editor_ids, ["<missing EDID:000900>"]);
    }

    #[test]
    fn directory_named_like_btd_does_not_admit_worldspace() {
        let root = vec![group(
            TOP_GROUP_TYPE,
            *b"WRLD",
            worldspace(PROC_WRLD_ID, PROC_CELL_ID, "ProcPlanet01"),
        )];
        let terrain_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(terrain_dir.path().join("procplanet01.btd")).unwrap();

        let (dropped, editor_ids) = ungated_worldspaces(&root, terrain_dir.path());

        assert_eq!(dropped, HashSet::from([PROC_WRLD_ID, PROC_CELL_ID]));
        assert_eq!(editor_ids, ["ProcPlanet01"]);
    }

    #[test]
    fn missing_terrain_dir_param_is_bad_params() {
        let handle = plugin_handle_new_native("Starfield2.esm", Some("starfield")).unwrap();
        let run_id = make_run(handle);
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let params = JsonValue::Null;
        with_run(run_id, |run| -> Result<(), RunError> {
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
            match StarfieldWorldspaceGatePhase.run(&mut ctx) {
                Err(PhaseError::BadParams(_)) => Ok(()),
                other => panic!("expected BadParams, got {other:?}"),
            }
        })
        .unwrap();
        drop_run(run_id).unwrap();
        plugin_handle_close_native(handle);
    }
}
