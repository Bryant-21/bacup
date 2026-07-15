//! Phase: convert_interior_cells
//!
//! Gated FO76→FO4 interior-cell conversion. Translates each interior CELL
//! (DATA `IsInteriorCell` set), emits it into the FO4 Interior-Block/Sub-Block
//! topology, copies + normalizes its Persistent/Temporary placed children, and
//! emits interior NAVM with an `Interior` NVNM parent. Inert unless source=FO76
//! and target=FO4. Reads `ctx.params["carry_previs"]` (bool, default false).

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

pub struct ConvertInteriorCellsPhase;

impl Phase for ConvertInteriorCellsPhase {
    fn name(&self) -> &'static str {
        "convert_interior_cells"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let carry_previs = ctx
            .params
            .get("carry_previs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let stats = ctx
            .run
            .emit_interior_cells(carry_previs)
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
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;
    use bytes::Bytes;
    use esp_authoring_core::nvnm::{NvnmParent, NvnmPayload, write_nvnm};
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord, clone_plugin_handle_state_no_py,
        ensure_interior_cell_and_child_group, insert_placed_child_into_cell_group,
        plugin_handle_new_native, plugin_handle_replace_authoring_record_value,
    };
    use smol_str::SmolStr;
    use std::sync::atomic::AtomicBool;

    const PERSISTENT_GROUP: i32 = 8;
    const TEMPORARY_GROUP: i32 = 9;
    const CELL_INTERIOR_FLAG: u8 = 0x01;

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
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

    fn edid(name: &str) -> ParsedSubrecord {
        let mut bytes = name.as_bytes().to_vec();
        bytes.push(0);
        sub("EDID", bytes)
    }

    /// An interior CELL ParsedRecord with EDID + DATA(interior) + XCLL + XCLW +
    /// PCMB + VISI. XCLL/XCLW/previs bodies are opaque placeholder bytes —
    /// the FO76 schema decodes them via `parsed_with_raw_fallback`, so they
    /// round-trip as raw fields.
    fn interior_cell_record(form_id: u32, eid_name: &str) -> ParsedRecord {
        parsed_record(
            "CELL",
            form_id,
            vec![
                edid(eid_name),
                sub("DATA", vec![CELL_INTERIOR_FLAG, 0x00]),
                sub("XCLL", vec![0u8; 92]),
                sub("XCLW", 3.0_f32.to_le_bytes().to_vec()),
                sub("PCMB", vec![0xEF, 0xBE, 0xAD, 0xDE]),
                sub("VISI", vec![0u8; 8]),
            ],
        )
    }

    fn refr_record(form_id: u32, base_raw: u32) -> ParsedRecord {
        parsed_record(
            "REFR",
            form_id,
            vec![sub("NAME", base_raw.to_le_bytes().to_vec())],
        )
    }

    /// A REFR carrying an XEZN pointing at `xezn_raw` (a FO76 LCTN). The placed
    /// normalizer strips XEZN that resolves to a non-ECZN type.
    fn refr_with_xezn(form_id: u32, base_raw: u32, xezn_raw: u32) -> ParsedRecord {
        parsed_record(
            "REFR",
            form_id,
            vec![
                sub("NAME", base_raw.to_le_bytes().to_vec()),
                sub("XEZN", xezn_raw.to_le_bytes().to_vec()),
            ],
        )
    }

    fn interior_navm_record(form_id: u32, cell_local: u32) -> ParsedRecord {
        let payload = NvnmPayload {
            version: 15,
            flags: 0,
            parent: NvnmParent::Interior { cell: cell_local },
            vertices: vec![],
            triangles: vec![],
            edge_links: vec![],
            door_refs: vec![],
            cover_array: vec![],
            cover_triangle_mappings: vec![],
            waypoints: vec![],
            grid: Default::default(),
        };
        parsed_record("NAVM", form_id, vec![sub("NVNM", write_nvnm(&payload))])
    }

    fn new_fo76_source() -> u64 {
        plugin_handle_new_native("SeventySix.esm", Some("fo76")).expect("source handle")
    }

    fn new_fo4_target() -> u64 {
        plugin_handle_new_native("SeventySix.esm", Some("fo4")).expect("target handle")
    }

    fn make_run(source: u64, target: u64) -> u64 {
        create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "SeventySix.esm".into(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                ..RunConfig::default()
            },
        })
        .expect("conversion run")
    }

    fn run_emit(run_id: u64, carry_previs: bool) {
        with_run(run_id, |run| -> Result<(), RunError> {
            run.emit_interior_cells(carry_previs)?;
            Ok(())
        })
        .expect("emit_interior_cells");
    }

    fn run_emit_via_phase(run_id: u64) -> PhaseReport {
        with_run(run_id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
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
            ConvertInteriorCellsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .expect("phase run")
    }

    // ── target-tree inspection helpers ───────────────────────────────────────

    fn find_record<'a>(
        items: &'a [ParsedItem],
        sig: &str,
        object_id: u32,
    ) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record)
                    if record.signature.as_str() == sig
                        && record.form_id & 0x00FF_FFFF == object_id =>
                {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(found) = find_record(&group.children, sig, object_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn record_has_subrecord(record: &ParsedRecord, sig: &str) -> bool {
        record
            .subrecords
            .iter()
            .any(|s| s.signature.as_str() == sig)
    }

    fn cell_data_flags(record: &ParsedRecord) -> u16 {
        let data = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "DATA")
            .expect("CELL.DATA");
        assert!(data.data.len() >= 2, "CELL.DATA flags must be a u16");
        u16::from_le_bytes([data.data[0], data.data[1]])
    }

    /// Walk to the CELL record sitting under interior Block(2)/Sub-Block(3).
    /// Recurses through any group whose direct children hold the block/sub-block
    /// (the top CELL group has group_type 0, label "CELL").
    fn interior_block_contains(
        items: &[ParsedItem],
        block: i32,
        subblock: i32,
        object_id: u32,
    ) -> bool {
        for item in items {
            let ParsedItem::Group(group) = item else {
                continue;
            };
            if group.group_type == 2 && i32::from_le_bytes(group.label) == block {
                for sub_item in &group.children {
                    let ParsedItem::Group(sb) = sub_item else {
                        continue;
                    };
                    if sb.group_type == 3
                        && i32::from_le_bytes(sb.label) == subblock
                        && find_record(&sb.children, "CELL", object_id).is_some()
                    {
                        return true;
                    }
                }
            }
            if interior_block_contains(&group.children, block, subblock, object_id) {
                return true;
            }
        }
        false
    }

    /// Find the placed-child record inside the cell's Cell-Children(6)→section
    /// group of the requested type.
    fn cell_section_record<'a>(
        items: &'a [ParsedItem],
        cell_object_id: u32,
        group_type: i32,
        child_object_id: u32,
    ) -> Option<&'a ParsedRecord> {
        fn find_cell_child_group<'a>(
            items: &'a [ParsedItem],
            cell_object_id: u32,
        ) -> Option<&'a ParsedGroup> {
            for item in items {
                let ParsedItem::Group(group) = item else {
                    continue;
                };
                if group.group_type == 6
                    && u32::from_le_bytes(group.label) & 0x00FF_FFFF == cell_object_id
                {
                    return Some(group);
                }
                if let Some(found) = find_cell_child_group(&group.children, cell_object_id) {
                    return Some(found);
                }
            }
            None
        }
        let cell_child = find_cell_child_group(items, cell_object_id)?;
        for item in &cell_child.children {
            let ParsedItem::Group(section) = item else {
                continue;
            };
            if section.group_type != group_type {
                continue;
            }
            for child in &section.children {
                if let ParsedItem::Record(record) = child {
                    if record.form_id & 0x00FF_FFFF == child_object_id {
                        return Some(record);
                    }
                }
            }
        }
        None
    }

    fn nvnm_parent_is_interior(record: &ParsedRecord, cell_local: u32) -> bool {
        let Some(nvnm) = record
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == "NVNM")
        else {
            return false;
        };
        let Ok(payload) = esp_authoring_core::nvnm::parse_nvnm(nvnm.data.as_ref()) else {
            return false;
        };
        payload.parent == NvnmParent::Interior { cell: cell_local }
    }

    #[test]
    fn interior_cell_carries_lighting_and_water_not_just_data() {
        let source = new_fo76_source();
        let target = new_fo4_target();
        ensure_interior_cell_and_child_group(source, interior_cell_record(0x00275EDE, "TestVault"))
            .expect("source interior cell");
        let run_id = make_run(source, target);
        run_emit(run_id, false);

        let (plugin, _) = clone_plugin_handle_state_no_py(target).expect("target snapshot");
        let cell =
            find_record(&plugin.root_items, "CELL", 0x275EDE).expect("interior CELL present");
        assert!(record_has_subrecord(cell, "XCLL"), "lighting carried");
        assert!(record_has_subrecord(cell, "XCLW"), "water carried");
        // 0x00275EDE = 2,580,190 -> block = n%10 = 0, subblock = (n/10)%10 = 9.
        assert!(
            interior_block_contains(&plugin.root_items, 0, 9, 0x275EDE),
            "cell lands in interior block 0 / subblock 9"
        );
        drop_run(run_id).unwrap();
    }

    #[test]
    fn wastelanders_public_hub_allowlist_sets_public_area_and_keeps_owner() {
        const PUBLIC_AREA: u16 = 0x0020;
        const OWNER: u32 = 0x0001_1000;
        let source = new_fo76_source();
        let target = new_fo4_target();
        let cell_ids = [0x0040_41F2, 0x0040_A2C1, 0x003F_880F, 0x0040_41F3];

        for (index, cell_id) in cell_ids.into_iter().enumerate() {
            let mut cell = interior_cell_record(cell_id, &format!("HubCell{index}"));
            cell.subrecords
                .push(sub("XOWN", OWNER.to_le_bytes().to_vec()));
            ensure_interior_cell_and_child_group(source, cell).expect("source interior cell");
        }

        let run_id = make_run(source, target);
        run_emit(run_id, false);
        let (plugin, _) = clone_plugin_handle_state_no_py(target).expect("target snapshot");

        for cell_id in &cell_ids[..3] {
            let cell = find_record(&plugin.root_items, "CELL", *cell_id).expect("public hub cell");
            assert_eq!(
                cell_data_flags(cell),
                CELL_INTERIOR_FLAG as u16 | PUBLIC_AREA
            );
            assert!(
                record_has_subrecord(cell, "XOWN"),
                "Public Area must preserve the hub's faction owner"
            );
        }
        let unrelated =
            find_record(&plugin.root_items, "CELL", cell_ids[3]).expect("unrelated interior cell");
        assert_eq!(cell_data_flags(unrelated), CELL_INTERIOR_FLAG as u16);
        assert!(record_has_subrecord(unrelated, "XOWN"));

        drop_run(run_id).unwrap();
    }

    #[test]
    fn previs_stripped_by_default_carried_with_flag() {
        let source = new_fo76_source();
        let target = new_fo4_target();
        ensure_interior_cell_and_child_group(source, interior_cell_record(0x00275EDE, "TestVault"))
            .expect("source interior cell");
        let run_id = make_run(source, target);
        run_emit(run_id, false);
        let (plugin, _) = clone_plugin_handle_state_no_py(target).expect("snapshot");
        let cell = find_record(&plugin.root_items, "CELL", 0x275EDE).expect("cell");
        assert!(
            !record_has_subrecord(cell, "PCMB"),
            "precombined timestamp stripped by default"
        );
        assert!(
            !record_has_subrecord(cell, "VISI"),
            "previs hash stripped by default"
        );
        assert!(!record_has_subrecord(cell, "XPRI"));
        assert!(!record_has_subrecord(cell, "XCRI"));
        drop_run(run_id).unwrap();

        let source2 = new_fo76_source();
        let target2 = new_fo4_target();
        ensure_interior_cell_and_child_group(
            source2,
            interior_cell_record(0x00275EDE, "TestVault"),
        )
        .expect("source interior cell");
        let run_id2 = make_run(source2, target2);
        run_emit(run_id2, true);
        let (plugin2, _) = clone_plugin_handle_state_no_py(target2).expect("snapshot");
        let cell2 = find_record(&plugin2.root_items, "CELL", 0x275EDE).expect("cell");
        assert!(
            record_has_subrecord(cell2, "PCMB"),
            "precombined timestamp carried with flag"
        );
        assert!(
            record_has_subrecord(cell2, "VISI"),
            "previs hash carried with flag"
        );
        drop_run(run_id2).unwrap();
    }

    #[test]
    fn exterior_cells_are_not_emitted() {
        let source = new_fo76_source();
        let target = new_fo4_target();
        // A CELL with DATA interior bit CLEAR is exterior — must be skipped.
        plugin_handle_replace_authoring_record_value(
            source,
            &serde_json::json!({
                "signature": "CELL",
                "form_id": "0030FE:SeventySix.esm",
                "eid": "ExtCell",
                "subrecords": [
                    { "signature": "EDID", "data_hex": "457874436C6C00" },
                    { "signature": "DATA", "data_hex": "0200" }
                ]
            }),
        )
        .expect("exterior CELL");
        let run_id = make_run(source, target);
        run_emit(run_id, false);
        let (plugin, _) = clone_plugin_handle_state_no_py(target).expect("snapshot");
        assert!(
            find_record(&plugin.root_items, "CELL", 0x30FE).is_none(),
            "exterior cell must not be emitted"
        );
        drop_run(run_id).unwrap();
    }

    #[test]
    fn interior_cell_placed_children_copied_to_correct_groups() {
        let source = new_fo76_source();
        let target = new_fo4_target();
        ensure_interior_cell_and_child_group(source, interior_cell_record(0x00275EDE, "TestVault"))
            .expect("source interior cell");
        insert_placed_child_into_cell_group(
            source,
            0x00275EDE,
            PERSISTENT_GROUP,
            refr_record(0x002F749F, 0x0001_0000),
        )
        .expect("source persistent child");
        insert_placed_child_into_cell_group(
            source,
            0x00275EDE,
            TEMPORARY_GROUP,
            refr_record(0x002F74A0, 0x0001_0000),
        )
        .expect("source temporary child");

        let run_id = make_run(source, target);
        run_emit(run_id, false);
        let (plugin, _) = clone_plugin_handle_state_no_py(target).expect("snapshot");
        assert!(
            cell_section_record(&plugin.root_items, 0x275EDE, PERSISTENT_GROUP, 0x2F749F).is_some(),
            "persistent REFR copied"
        );
        assert!(
            cell_section_record(&plugin.root_items, 0x275EDE, TEMPORARY_GROUP, 0x2F74A0).is_some(),
            "temporary REFR copied"
        );
        drop_run(run_id).unwrap();
    }

    #[test]
    fn copied_children_keep_xezn_for_later_eczn_repoint() {
        let source = new_fo76_source();
        let target = new_fo4_target();
        ensure_interior_cell_and_child_group(source, interior_cell_record(0x00275EDE, "TestVault"))
            .expect("source interior cell");
        // FO76 placed refs carry XEZN→LCTN. The encounter-zone synthesis pass
        // (runs after emit) repoints XEZN→LCTN to the LCTN's synthesized ECZN,
        // so XEZN must survive this phase intact.
        insert_placed_child_into_cell_group(
            source,
            0x00275EDE,
            TEMPORARY_GROUP,
            refr_with_xezn(0x002F74A0, 0x0001_0000, 0x0002_F800),
        )
        .expect("source xezn child");

        let run_id = make_run(source, target);
        run_emit(run_id, false);
        let (plugin, _) = clone_plugin_handle_state_no_py(target).expect("snapshot");
        let child = cell_section_record(&plugin.root_items, 0x275EDE, TEMPORARY_GROUP, 0x2F74A0)
            .expect("temporary REFR copied");
        assert!(
            record_has_subrecord(child, "XEZN"),
            "XEZN preserved by normalizer for the encounter-zone synthesis repoint"
        );
        drop_run(run_id).unwrap();
    }

    #[test]
    fn interior_navm_emitted_with_interior_parent_into_temporary_group() {
        let source = new_fo76_source();
        let target = new_fo4_target();
        ensure_interior_cell_and_child_group(source, interior_cell_record(0x00275EDE, "TestVault"))
            .expect("source interior cell");
        insert_placed_child_into_cell_group(
            source,
            0x00275EDE,
            TEMPORARY_GROUP,
            interior_navm_record(0x002F7500, 0x00275EDE),
        )
        .expect("source navm");

        let run_id = make_run(source, target);
        run_emit(run_id, false);
        let (plugin, _) = clone_plugin_handle_state_no_py(target).expect("snapshot");
        let navm = cell_section_record(&plugin.root_items, 0x275EDE, TEMPORARY_GROUP, 0x2F7500)
            .expect("interior NAVM in temporary group");
        assert_eq!(navm.signature.as_str(), "NAVM");
        assert!(
            nvnm_parent_is_interior(navm, 0x275EDE),
            "NVNM parent must be Interior {{ cell }}"
        );
        drop_run(run_id).unwrap();
    }

    #[test]
    fn navi_includes_interior_navmesh_entry() {
        let source = new_fo76_source();
        let target = new_fo4_target();
        ensure_interior_cell_and_child_group(source, interior_cell_record(0x00275EDE, "TestVault"))
            .expect("source interior cell");
        insert_placed_child_into_cell_group(
            source,
            0x00275EDE,
            TEMPORARY_GROUP,
            interior_navm_record(0x002F7500, 0x00275EDE),
        )
        .expect("source navm");

        let run_id = make_run(source, target);
        with_run(run_id, |run| -> Result<(), RunError> {
            run.emit_interior_cells(false)?;
            run.rebuild_projected_navi()?;
            Ok(())
        })
        .expect("emit + navi");

        let (plugin, _) = clone_plugin_handle_state_no_py(target).expect("snapshot");
        let navi = find_record_by_sig(&plugin.root_items, "NAVI").expect("NAVI present");
        // Each NVMI entry's first u32 is the NAVM form_id. Confirm the interior
        // NAVM (object id 0x2F7500) appears.
        let found = navi
            .subrecords
            .iter()
            .filter(|s| s.signature.as_str() == "NVMI")
            .any(|s| {
                s.data.len() >= 4
                    && u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]])
                        & 0x00FF_FFFF
                        == 0x2F7500
            });
        assert!(
            found,
            "NAVI must contain an NVMI entry for the interior NAVM"
        );
        drop_run(run_id).unwrap();
    }

    fn find_record_by_sig<'a>(items: &'a [ParsedItem], sig: &str) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.signature.as_str() == sig => {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(found) = find_record_by_sig(&group.children, sig) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    // ── Phase wrapper ────────────────────────────────────────────────────────

    #[test]
    fn phase_reports_added_cell_and_children() {
        let source = new_fo76_source();
        let target = new_fo4_target();
        ensure_interior_cell_and_child_group(source, interior_cell_record(0x00275EDE, "TestVault"))
            .expect("source interior cell");
        insert_placed_child_into_cell_group(
            source,
            0x00275EDE,
            TEMPORARY_GROUP,
            refr_record(0x002F74A0, 0x0001_0000),
        )
        .expect("source child");
        let run_id = make_run(source, target);
        let report = run_emit_via_phase(run_id);
        assert!(report.records_added >= 2, "cell + child added: {report:?}");
        drop_run(run_id).unwrap();
    }

    // ── Source-side one-pass child collection ────────────────────────────────

    #[test]
    fn collect_interior_cell_children_groups_persistent_and_temporary() {
        let source = new_fo76_source();
        ensure_interior_cell_and_child_group(source, interior_cell_record(0x00275EDE, "TestVault"))
            .expect("source interior cell");
        insert_placed_child_into_cell_group(
            source,
            0x00275EDE,
            PERSISTENT_GROUP,
            refr_record(0x002F749F, 0x0001_0000),
        )
        .expect("persistent child");
        insert_placed_child_into_cell_group(
            source,
            0x00275EDE,
            TEMPORARY_GROUP,
            refr_record(0x002F74A0, 0x0001_0000),
        )
        .expect("temporary child");

        let mut set = rustc_hash::FxHashSet::default();
        set.insert(0x275EDE_u32);
        let map =
            crate::source_read::collect_interior_cell_children(source, &set).expect("collect");
        let kids = map.get(&0x275EDE).expect("entry for cell");
        assert_eq!(kids.persistent, vec![0x2F749F]);
        assert_eq!(kids.temporary, vec![0x2F74A0]);

        // A cell whose object id is not in the set must not be collected.
        let mut other = rustc_hash::FxHashSet::default();
        other.insert(0x00DEAD00_u32 & 0x00FF_FFFF);
        let map2 =
            crate::source_read::collect_interior_cell_children(source, &other).expect("collect2");
        assert!(
            map2.get(&0x275EDE).is_none(),
            "cell outside the set must not be collected"
        );
    }

    #[test]
    fn phase_is_noop_for_non_fo76_source() {
        let source = plugin_handle_new_native("Source.esm", Some("fo4")).expect("source");
        let target = plugin_handle_new_native("Output.esm", Some("fo4")).expect("target");
        let run_id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                ..RunConfig::default()
            },
        })
        .expect("run");
        let report = run_emit_via_phase(run_id);
        assert_eq!(report.records_added, 0);
        drop_run(run_id).unwrap();
    }
}
