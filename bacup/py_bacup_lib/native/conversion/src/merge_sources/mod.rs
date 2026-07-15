use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use esp_authoring_core::plugin_runtime::{
    LocalizedStringsState, ParsedItem, ParsedPlugin, clone_plugin_handle_state_no_py,
    compiled_schema_for_game_str, ensure_core_section, plugin_handle_close_native,
    plugin_handle_load_no_py, plugin_handle_new_native, plugin_handle_save_no_py,
    plugin_handle_store_ref,
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use thiserror::Error;

mod classify;
mod flatten;
mod graft;
mod repoint;
mod sanitize;

#[derive(Debug, Clone, Deserialize)]
pub struct MergeOptions {
    pub primary_paths: Vec<PathBuf>,
    pub grafted_paths: Vec<PathBuf>,
    pub output_path: PathBuf,
    pub report_path: Option<PathBuf>,
    pub game: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct SigCounts {
    pub deduped: u64,
    pub copied: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MergeReport {
    pub primary_records: u64,
    pub grafted_records: u64,
    pub deduped: u64,
    pub copied: u64,
    pub remapped_refs: u64,
    pub sanitized_occurrences: u64,
    pub sanitized_by_context: BTreeMap<String, u64>,
    pub sanitizer_actions: BTreeMap<String, u64>,
    pub dropped_owners: BTreeMap<String, u64>,
    pub dangling: Vec<String>,
    pub by_signature: BTreeMap<String, SigCounts>,
}

#[derive(Debug, Error)]
pub enum MergeError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("plugin load error: {0}")]
    Load(String),
    #[error("unknown or out-of-order master: {0}")]
    UnknownMaster(String),
    #[error("{0} dangling references")]
    Dangling(usize),
    #[error("record count mismatch: expected {expected}, got {actual}")]
    CountMismatch { expected: u64, actual: u64 },
}

pub(crate) fn load_no_py(path: &str, game: Option<&str>) -> Result<u64, MergeError> {
    plugin_handle_load_no_py(path, game, None, None, true).map_err(MergeError::Load)
}

pub(crate) fn build_primary_eid_index(
    handle_id: u64,
) -> Result<HashMap<(String, SmolStr), u32>, MergeError> {
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|e| MergeError::Load(e.to_string()))?;
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| MergeError::Load(format!("unknown plugin handle: {handle_id}")))?;
    let core = ensure_core_section(slot);
    Ok(core
        .by_form_key
        .values()
        .filter(|entry| !entry.eid.is_empty())
        .map(|entry| {
            (
                (entry.eid.to_lowercase(), entry.signature.clone()),
                entry.raw_form_id,
            )
        })
        .collect())
}

pub(crate) fn collect_used_ids(handle_id: u64) -> Result<HashSet<u32>, MergeError> {
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|e| MergeError::Load(e.to_string()))?;
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| MergeError::Load(format!("unknown plugin handle: {handle_id}")))?;
    let core = ensure_core_section(slot);
    Ok(core
        .by_form_key
        .values()
        .map(|entry| entry.raw_form_id)
        .collect())
}

fn validate_master_chain(handles: &[u64]) -> Result<(), MergeError> {
    let store = plugin_handle_store_ref()
        .lock()
        .map_err(|e| MergeError::Load(e.to_string()))?;
    let mut earlier = HashSet::new();
    for handle in handles {
        let slot = store
            .get(handle)
            .ok_or_else(|| MergeError::Load(format!("unknown plugin handle: {handle}")))?;
        for master in &slot.parsed.header.masters {
            if !earlier.contains(&master.to_lowercase()) {
                return Err(MergeError::UnknownMaster(format!(
                    "{} requires {master}",
                    slot.parsed.plugin_name
                )));
            }
        }
        earlier.insert(slot.parsed.plugin_name.to_lowercase());
    }
    Ok(())
}

pub fn run(opts: &MergeOptions) -> Result<MergeReport, MergeError> {
    let mut handles = Vec::new();
    let mut output_handle = None;
    let result = (|| {
        let mut primary = Vec::new();
        for path in &opts.primary_paths {
            let handle = load_no_py(&path.to_string_lossy(), Some(&opts.game))?;
            handles.push(handle);
            primary.push(handle);
        }
        let mut grafted = Vec::new();
        let grafted_game = if opts.game.eq_ignore_ascii_case("fnv") {
            "fo3"
        } else {
            opts.game.as_str()
        };
        for path in &opts.grafted_paths {
            let handle = load_no_py(&path.to_string_lossy(), Some(grafted_game))?;
            handles.push(handle);
            grafted.push(handle);
        }
        let primary_schema = compiled_schema_for_game_str(&opts.game).map_err(MergeError::Load)?;
        let grafted_schema =
            compiled_schema_for_game_str(grafted_game).map_err(MergeError::Load)?;
        validate_master_chain(&primary)?;
        validate_master_chain(&grafted)?;
        let first_primary = primary
            .first()
            .copied()
            .ok_or_else(|| MergeError::Load("primary lineage has no plugins".to_string()))?;
        let primary_strings = clone_plugin_handle_state_no_py(first_primary)
            .map_err(MergeError::Load)?
            .1;
        let mut primary = flatten::flatten_lineage(&primary)?;
        let primary_records = count_records(&primary.tree);
        let primary_ids = primary.used_ids.clone();

        let (grafted_records, classification, mut stats, grafted_record_ids) = if grafted.is_empty()
        {
            (
                0,
                classify::Classification {
                    remap: Default::default(),
                    dropped: Default::default(),
                    by_signature: Default::default(),
                },
                repoint::RepointStats::default(),
                HashSet::new(),
            )
        } else {
            let grafted = flatten::flatten_lineage(&grafted)?;
            let grafted_records = count_records(&grafted.tree);
            let classification = classify::classify_grafted(
                &grafted.tree,
                &primary.eid_index,
                &mut primary.used_ids,
            );
            let mut stats = repoint::RepointStats::default();
            graft::graft_lineage(
                &mut primary.tree,
                grafted.tree,
                &classification,
                &primary_ids,
                &mut stats,
                primary.template.header_size,
                Some(grafted_schema.as_ref()),
            );
            let grafted_record_ids = classification
                .remap
                .iter()
                .filter_map(|(raw, output)| {
                    (!classification.dropped.contains(raw)).then_some(*output)
                })
                .collect();
            (grafted_records, classification, stats, grafted_record_ids)
        };

        let initial_dangling = std::mem::take(&mut stats.dangling);
        let sanitization = if initial_dangling.is_empty() {
            sanitize::SanitizationStats::default()
        } else {
            sanitize::sanitize_dangling_references(
                &mut primary.tree,
                initial_dangling,
                Some(primary_schema.as_ref()),
                Some(grafted_schema.as_ref()),
                &grafted_record_ids,
            )
        };
        let dropped_owner_records = sanitization.dropped_owner_count();
        let report = MergeReport {
            primary_records,
            grafted_records,
            deduped: classification
                .by_signature
                .values()
                .map(|counts| counts.deduped)
                .sum(),
            copied: classification
                .by_signature
                .values()
                .map(|counts| counts.copied)
                .sum(),
            remapped_refs: stats.remapped,
            sanitized_occurrences: sanitization.occurrences,
            sanitized_by_context: sanitization.by_context,
            sanitizer_actions: sanitization.by_action,
            dropped_owners: sanitization.dropped_owners,
            dangling: sanitization.dangling,
            by_signature: classification
                .by_signature
                .into_iter()
                .map(|(signature, counts)| (signature.to_string(), counts))
                .collect(),
        };

        if let Some(report_path) = &opts.report_path {
            write_report(report_path, &report)?;
        }
        if !report.dangling.is_empty() {
            return Err(MergeError::Dangling(report.dangling.len()));
        }
        let expected = report.primary_records + report.grafted_records
            - report.deduped
            - dropped_owner_records;
        let actual = count_records(&primary.tree);
        if expected != actual {
            return Err(MergeError::CountMismatch { expected, actual });
        }

        let output_name = opts
            .output_path
            .file_name()
            .ok_or_else(|| MergeError::Io("output path has no file name".to_string()))?
            .to_string_lossy()
            .into_owned();

        let final_used_ids = collect_record_ids(&primary.tree);
        prepare_output_plugin(
            &mut primary.template,
            primary.tree,
            &final_used_ids,
            &opts.game,
            &output_name,
        );
        let handle = plugin_handle_new_native(&output_name, Some(&opts.game))
            .map_err(|error| MergeError::Load(error.to_string()))?;
        output_handle = Some(handle);
        install_output_state(handle, primary.template, primary_strings)?;
        if let Some(parent) = opts.output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| MergeError::Io(error.to_string()))?;
        }
        plugin_handle_save_no_py(handle, &opts.output_path.to_string_lossy())
            .map_err(MergeError::Io)?;
        Ok(report)
    })();
    if let Some(handle) = output_handle {
        plugin_handle_close_native(handle);
    }
    for handle in handles {
        plugin_handle_close_native(handle);
    }
    result
}

fn prepare_output_plugin(
    plugin: &mut ParsedPlugin,
    tree: Vec<ParsedItem>,
    used_ids: &HashSet<u32>,
    game: &str,
    output_name: &str,
) {
    plugin.plugin_name = output_name.to_string();
    plugin.file_path.clear();
    plugin.game = Some(game.to_string());
    plugin.header.masters.clear();
    plugin.header.master_sizes.clear();
    plugin.header.overridden_forms.clear();
    plugin.root_items = tree;
    plugin.header.num_records = count_records(&plugin.root_items) as u32;
    plugin.header.next_object_id = used_ids
        .iter()
        .copied()
        .max()
        .unwrap_or(0x7ff)
        .saturating_add(1)
        .max(0x800);
}

fn install_output_state(
    handle: u64,
    plugin: ParsedPlugin,
    strings: LocalizedStringsState,
) -> Result<(), MergeError> {
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|error| MergeError::Load(error.to_string()))?;
    let slot = store
        .get_mut(&handle)
        .ok_or_else(|| MergeError::Load(format!("unknown plugin handle: {handle}")))?;
    slot.parsed = plugin;
    *slot.strings_mut() = strings;
    slot.clear_record_count_cache();
    slot.invalidate_sections();
    Ok(())
}

fn write_report(path: &std::path::Path, report: &MergeReport) -> Result<(), MergeError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| MergeError::Io(error.to_string()))?;
    }
    let file = std::fs::File::create(path).map_err(|error| MergeError::Io(error.to_string()))?;
    serde_json::to_writer_pretty(file, report).map_err(|error| MergeError::Io(error.to_string()))
}

pub(crate) fn count_records(items: &[ParsedItem]) -> u64 {
    items
        .iter()
        .map(|item| match item {
            ParsedItem::Record(_) => 1,
            ParsedItem::Group(group) => count_records(&group.children),
        })
        .sum()
}

fn collect_record_ids(items: &[ParsedItem]) -> HashSet<u32> {
    let mut ids = HashSet::new();
    collect_record_ids_into(items, &mut ids);
    ids
}

fn collect_record_ids_into(items: &[ParsedItem], ids: &mut HashSet<u32>) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                ids.insert(record.form_id);
            }
            ParsedItem::Group(group) => collect_record_ids_into(&group.children, ids),
        }
    }
}

#[cfg(test)]
mod test_util;

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, ParsedRecord, ParsedSubrecord, clone_plugin_handle_state_no_py,
        editor_id_from_effective_subrecords,
    };

    use super::*;
    use test_util::{
        formid_sub, rec, write_test_plugin, write_test_plugin_items, write_test_plugin_with_masters,
    };

    fn find_record<'a>(items: &'a [ParsedItem], editor_id: &str) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record)
                    if editor_id_from_effective_subrecords(&record.subrecords) == editor_id =>
                {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(record) = find_record(&group.children, editor_id) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn collect_signature<'a>(
        items: &'a [ParsedItem],
        signature: &str,
        records: &mut Vec<&'a ParsedRecord>,
    ) {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.signature.as_str() == signature => {
                    records.push(record);
                }
                ParsedItem::Group(group) => {
                    collect_signature(&group.children, signature, records);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn eid_index_and_used_ids_from_loaded_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_test_plugin(
            tmp.path(),
            "Primary.esm",
            "fnv",
            vec![
                rec("GLOB", 0x001200, "TimeScale"),
                rec("FACT", 0x001300, "RaiderFaction"),
            ],
        );
        let handle = load_no_py(path.to_str().unwrap(), Some("fnv")).unwrap();
        let index = build_primary_eid_index(handle).unwrap();
        assert_eq!(
            index.get(&("timescale".to_string(), "GLOB".into())),
            Some(&0x001200)
        );
        let used = collect_used_ids(handle).unwrap();
        assert!(used.contains(&0x001300));
        plugin_handle_close_native(handle);
    }

    #[test]
    fn merge_sources_grafts_deduplicates_repoints_and_saves() {
        let tmp = tempfile::tempdir().unwrap();
        let primary = write_test_plugin(
            tmp.path(),
            "Primary.esm",
            "fnv",
            vec![
                rec("GLOB", 0x1200, "TimeScale"),
                rec("WEAP", 0x1300, "NVPistol"),
            ],
        );
        let mut rifle = rec("ACTI", 0x9A00, "FO3Rifle");
        // ACTI.SCRI is an FNV schema-declared FormID field, exercising the
        // schema-aware raw walker used for disk-loaded plugin records.
        rifle.subrecords.push(formid_sub("SCRI", 0x9900));
        let grafted = write_test_plugin(
            tmp.path(),
            "Grafted.esm",
            "fo3",
            vec![
                rec("GLOB", 0x9900, "TimeScale"),
                rifle,
                rec("FACT", 0x1300, "FO3Fact"),
            ],
        );
        let output = tmp.path().join("FNV_FO3_Merged.esm");
        let report_path = tmp.path().join("merge_report.json");
        let report = run(&MergeOptions {
            primary_paths: vec![primary],
            grafted_paths: vec![grafted],
            output_path: output.clone(),
            report_path: Some(report_path.clone()),
            game: "fnv".to_string(),
        })
        .unwrap();
        assert_eq!(report.primary_records, 2);
        assert_eq!(report.grafted_records, 3);
        assert_eq!(report.deduped, 1);
        assert_eq!(report.copied, 2);
        assert!(output.exists());
        assert!(report_path.exists());

        let handle = load_no_py(output.to_str().unwrap(), Some("fnv")).unwrap();
        let (plugin, _) = clone_plugin_handle_state_no_py(handle).unwrap();
        assert_eq!(count_records(&plugin.root_items), 4);
        assert!(plugin.header.masters.is_empty());
        let rifle = find_record(&plugin.root_items, "FO3Rifle").unwrap();
        let script = rifle
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "SCRI")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(script.data[0..4].try_into().unwrap()),
            0x1200
        );
        let fact = find_record(&plugin.root_items, "FO3Fact").unwrap();
        assert_ne!(fact.form_id, 0x1300);
        plugin_handle_close_native(handle);
    }

    #[test]
    fn graft_collision_repoints_fo3_npc_voice_to_copied_vtyp() {
        let tmp = tempfile::tempdir().unwrap();
        let mut deleted_primary_voice = rec("VTYP", 0x029FB1, "DeletedPrimaryVoice");
        deleted_primary_voice.flags |= 0x20;
        let mut primary_npc = rec("NPC_", 0x029FB0, "PrimaryNpc");
        primary_npc.subrecords.push(ParsedSubrecord {
            signature: "VTCK".into(),
            data: Bytes::copy_from_slice(&0x029FB1_u32.to_le_bytes()),
            semantic_type: None,
        });
        let primary = write_test_plugin(
            tmp.path(),
            "Primary.esm",
            "fnv",
            vec![deleted_primary_voice, primary_npc],
        );

        let grafted_voice = rec("VTYP", 0x029FB1, "DistinctGraftedVoice");
        let mut grafted_npc = rec("NPC_", 0x029FB2, "GraftedNpc");
        grafted_npc.flags |= 0x0004_0000;
        grafted_npc.subrecords.push(ParsedSubrecord {
            signature: "VTCK".into(),
            data: Bytes::copy_from_slice(&0x029FB1_u32.to_le_bytes()),
            semantic_type: None,
        });
        let grafted = write_test_plugin(
            tmp.path(),
            "Grafted.esm",
            "fo3",
            vec![grafted_voice, grafted_npc],
        );
        let output = tmp.path().join("FNV_FO3_Merged.esm");

        run(&MergeOptions {
            primary_paths: vec![primary],
            grafted_paths: vec![grafted],
            output_path: output.clone(),
            report_path: None,
            game: "fnv".to_string(),
        })
        .unwrap();

        let handle = load_no_py(output.to_str().unwrap(), Some("fnv")).unwrap();
        let (plugin, _) = clone_plugin_handle_state_no_py(handle).unwrap();
        let copied_voice = find_record(&plugin.root_items, "DistinctGraftedVoice").unwrap();
        assert_ne!(copied_voice.form_id, 0x029FB1);
        let primary_npc = find_record(&plugin.root_items, "PrimaryNpc").unwrap();
        let primary_voice = primary_npc
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "VTCK")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(primary_voice.data[0..4].try_into().unwrap()),
            0x029FB1
        );
        let npc = find_record(&plugin.root_items, "GraftedNpc").unwrap();
        let voice = npc
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "VTCK")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(voice.data[0..4].try_into().unwrap()),
            copied_voice.form_id
        );
        plugin_handle_close_native(handle);
    }

    #[test]
    fn compressed_lineage_master_reference_persists_repoint_after_reload() {
        let tmp = tempfile::tempdir().unwrap();
        let base = write_test_plugin(
            tmp.path(),
            "Base.esm",
            "fnv",
            vec![rec("GLOB", 0x1200, "OccupiedObjectId")],
        );
        let dlc_voice = rec("VTYP", 0x0100_1200, "DlcVoice");
        let dlc_one = write_test_plugin_with_masters(
            tmp.path(),
            "DlcOne.esm",
            "fnv",
            vec!["Base.esm".to_string()],
            vec![dlc_voice],
        );
        let mut dlc_npc = rec("NPC_", 0x0200_1300, "DlcNpc");
        dlc_npc.flags |= 0x0004_0000;
        dlc_npc.subrecords.push(ParsedSubrecord {
            signature: "VTCK".into(),
            data: Bytes::copy_from_slice(&0x0100_1200_u32.to_le_bytes()),
            semantic_type: None,
        });
        let dlc_two = write_test_plugin_with_masters(
            tmp.path(),
            "DlcTwo.esm",
            "fnv",
            vec!["Base.esm".to_string(), "DlcOne.esm".to_string()],
            vec![dlc_npc],
        );
        let output = tmp.path().join("FNV_Lineage_Merged.esm");

        run(&MergeOptions {
            primary_paths: vec![base, dlc_one, dlc_two],
            grafted_paths: Vec::new(),
            output_path: output.clone(),
            report_path: None,
            game: "fnv".to_string(),
        })
        .unwrap();

        let handle = load_no_py(output.to_str().unwrap(), Some("fnv")).unwrap();
        let (plugin, _) = clone_plugin_handle_state_no_py(handle).unwrap();
        let voice = find_record(&plugin.root_items, "DlcVoice").unwrap();
        assert_ne!(voice.form_id, 0x1200);
        let npc = find_record(&plugin.root_items, "DlcNpc").unwrap();
        let voice_ref = npc
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "VTCK")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(voice_ref.data[0..4].try_into().unwrap()),
            voice.form_id
        );
        plugin_handle_close_native(handle);
    }

    #[test]
    fn compressed_sanitizer_edits_and_removals_persist_after_reload() {
        let tmp = tempfile::tempdir().unwrap();
        let primary = write_test_plugin(
            tmp.path(),
            "Primary.esm",
            "fnv",
            vec![rec("GLOB", 0x1200, "Primary")],
        );
        let mut navm = rec("NAVM", 0x9900, "CompressedNavmesh");
        navm.flags |= 0x0004_0000;
        let mut nvex = vec![1, 2, 3, 4];
        nvex.extend_from_slice(&0xBEEF_u32.to_le_bytes());
        nvex.extend_from_slice(&[5, 6]);
        navm.subrecords.push(ParsedSubrecord {
            signature: "NVEX".into(),
            data: Bytes::from(nvex),
            semantic_type: None,
        });
        let mut refr = rec("REFR", 0x9901, "CompressedReference");
        refr.flags |= 0x0004_0000;
        let mut xndp = 0xBEEF_u32.to_le_bytes().to_vec();
        xndp.extend_from_slice(&[7, 8, 9, 10]);
        refr.subrecords.push(ParsedSubrecord {
            signature: "XNDP".into(),
            data: Bytes::from(xndp),
            semantic_type: None,
        });
        let grafted = write_test_plugin(tmp.path(), "Grafted.esm", "fo3", vec![navm, refr]);
        let output = tmp.path().join("FNV_FO3_Merged.esm");

        let report = run(&MergeOptions {
            primary_paths: vec![primary],
            grafted_paths: vec![grafted],
            output_path: output.clone(),
            report_path: None,
            game: "fnv".to_string(),
        })
        .unwrap();
        assert_eq!(report.sanitized_occurrences, 2);

        let handle = load_no_py(output.to_str().unwrap(), Some("fnv")).unwrap();
        let (plugin, _) = clone_plugin_handle_state_no_py(handle).unwrap();
        let navm = find_record(&plugin.root_items, "CompressedNavmesh").unwrap();
        let nvex = navm
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "NVEX")
            .unwrap();
        assert_eq!(&nvex.data[0..4], &[1, 2, 3, 4]);
        assert_eq!(&nvex.data[4..8], &[0, 0, 0, 0]);
        assert_eq!(&nvex.data[8..10], &[5, 6]);
        let refr = find_record(&plugin.root_items, "CompressedReference").unwrap();
        assert!(
            refr.subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "XNDP")
        );
        plugin_handle_close_native(handle);
    }

    #[test]
    fn duplicate_dialogue_container_grafts_child_infos() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy_tail = Bytes::from(vec![0; 4]);
        let primary = write_test_plugin_items(
            tmp.path(),
            "Primary.esm",
            "fnv",
            Vec::new(),
            vec![ParsedItem::Group(ParsedGroup {
                label: *b"DIAL",
                group_type: 0,
                tail: legacy_tail.clone(),
                children: vec![
                    ParsedItem::Record(rec("DIAL", 0x2000, "GREETING")),
                    ParsedItem::Group(ParsedGroup {
                        label: 0x2000_u32.to_le_bytes(),
                        group_type: 7,
                        tail: legacy_tail.clone(),
                        children: vec![ParsedItem::Record(rec("INFO", 0x2001, ""))],
                    }),
                ],
            })],
        );
        let grafted = write_test_plugin_items(
            tmp.path(),
            "Grafted.esm",
            "fo3",
            Vec::new(),
            vec![ParsedItem::Group(ParsedGroup {
                label: *b"DIAL",
                group_type: 0,
                tail: legacy_tail.clone(),
                children: vec![
                    ParsedItem::Record(rec("DIAL", 0x8800, "GREETING")),
                    ParsedItem::Group(ParsedGroup {
                        label: 0x8800_u32.to_le_bytes(),
                        group_type: 7,
                        tail: legacy_tail,
                        children: vec![ParsedItem::Record(rec("INFO", 0x8801, ""))],
                    }),
                ],
            })],
        );
        let output = tmp.path().join("FNV_FO3_Merged.esm");
        let report = run(&MergeOptions {
            primary_paths: vec![primary],
            grafted_paths: vec![grafted],
            output_path: output.clone(),
            report_path: None,
            game: "fnv".to_string(),
        })
        .unwrap();
        assert_eq!(report.deduped, 1);

        let handle = load_no_py(output.to_str().unwrap(), Some("fnv")).unwrap();
        let (plugin, _) = clone_plugin_handle_state_no_py(handle).unwrap();
        let mut dialogues = Vec::new();
        let mut infos = Vec::new();
        collect_signature(&plugin.root_items, "DIAL", &mut dialogues);
        collect_signature(&plugin.root_items, "INFO", &mut infos);
        assert_eq!(dialogues.len(), 1);
        assert_eq!(infos.len(), 2);
        plugin_handle_close_native(handle);
    }

    #[test]
    fn dangling_reference_writes_report_before_hard_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let primary = write_test_plugin(
            tmp.path(),
            "Primary.esm",
            "fnv",
            vec![rec("GLOB", 0x1200, "Primary")],
        );
        let mut dangling = rec("ACTI", 0x9900, "DanglingRef");
        dangling.subrecords.push(formid_sub("SCRI", 0xBEEF));
        let grafted = write_test_plugin(tmp.path(), "Grafted.esm", "fo3", vec![dangling]);
        let report_path = tmp.path().join("merge_report.json");
        let error = run(&MergeOptions {
            primary_paths: vec![primary],
            grafted_paths: vec![grafted],
            output_path: tmp.path().join("FNV_FO3_Merged.esm"),
            report_path: Some(report_path.clone()),
            game: "fnv".to_string(),
        })
        .unwrap_err();
        assert!(matches!(error, MergeError::Dangling(1)));
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_path).unwrap()).unwrap();
        assert_eq!(report["dangling"][0], "ACTI:00009900:SCRI:0000BEEF");
    }

    #[test]
    fn grafted_invalid_high_byte_reference_is_still_dangling() {
        let tmp = tempfile::tempdir().unwrap();
        let primary = write_test_plugin(
            tmp.path(),
            "Primary.esm",
            "fnv",
            vec![rec("GLOB", 0x1200, "Primary")],
        );
        let mut region = rec("ACTI", 0x9900, "GraftedRegion");
        region.subrecords.push(formid_sub("SCRI", 0x0102_76B2));
        let grafted = write_test_plugin(tmp.path(), "Grafted.esm", "fo3", vec![region]);
        let report_path = tmp.path().join("merge_report.json");
        let error = run(&MergeOptions {
            primary_paths: vec![primary],
            grafted_paths: vec![grafted],
            output_path: tmp.path().join("FNV_FO3_Merged.esm"),
            report_path: Some(report_path.clone()),
            game: "fnv".to_string(),
        })
        .unwrap_err();
        assert!(matches!(error, MergeError::Dangling(1)));
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_path).unwrap()).unwrap();
        assert_eq!(report["dangling"][0], "ACTI:00009900:SCRI:010276B2");
    }

    #[test]
    fn supported_dangling_owner_is_sanitized_reported_and_reconciled() {
        let tmp = tempfile::tempdir().unwrap();
        let primary = write_test_plugin(
            tmp.path(),
            "Primary.esm",
            "fnv",
            vec![rec("GLOB", 0x1200, "Primary")],
        );
        let mut actor = rec("ACRE", 0x9900, "DanglingActor");
        let mut xesp = 0xBEEF_u32.to_le_bytes().to_vec();
        xesp.extend_from_slice(&[0, 0, 0, 0]);
        actor.subrecords.push(ParsedSubrecord {
            signature: "XESP".into(),
            data: Bytes::from(xesp),
            semantic_type: None,
        });
        let grafted = write_test_plugin(tmp.path(), "Grafted.esm", "fo3", vec![actor]);
        let output = tmp.path().join("FNV_FO3_Merged.esm");
        let report_path = tmp.path().join("merge_report.json");
        let report = run(&MergeOptions {
            primary_paths: vec![primary],
            grafted_paths: vec![grafted],
            output_path: output.clone(),
            report_path: Some(report_path.clone()),
            game: "fnv".to_string(),
        })
        .unwrap();
        assert_eq!(report.sanitized_occurrences, 1);
        assert_eq!(report.sanitized_by_context["ACRE.XESP"], 1);
        assert_eq!(report.sanitizer_actions["drop_owner"], 1);
        assert_eq!(report.dropped_owners["ACRE"], 1);
        assert!(report.dangling.is_empty());

        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_path).unwrap()).unwrap();
        assert_eq!(json["sanitized_occurrences"], 1);
        assert_eq!(json["sanitized_by_context"]["ACRE.XESP"], 1);
        assert_eq!(json["sanitizer_actions"]["drop_owner"], 1);
        assert_eq!(json["dropped_owners"]["ACRE"], 1);
        assert_eq!(json["dangling"], serde_json::json!([]));

        let handle = load_no_py(output.to_str().unwrap(), Some("fnv")).unwrap();
        let (plugin, _) = clone_plugin_handle_state_no_py(handle).unwrap();
        assert_eq!(count_records(&plugin.root_items), 1);
        assert_eq!(plugin.header.next_object_id, 0x1201);
        plugin_handle_close_native(handle);
    }

    #[test]
    fn recursive_record_counter_excludes_groups() {
        let tree = vec![ParsedItem::Group(ParsedGroup {
            label: *b"GLOB",
            group_type: 0,
            tail: Bytes::new(),
            children: vec![
                ParsedItem::Record(rec("GLOB", 0x1200, "One")),
                ParsedItem::Group(ParsedGroup {
                    label: 0x1200_u32.to_le_bytes(),
                    group_type: 1,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Record(rec("CELL", 0x1300, "Two"))],
                }),
            ],
        })];
        assert_eq!(count_records(&tree), 2);
    }

    #[test]
    fn output_identity_comes_from_requested_file_name() {
        let tmp = tempfile::tempdir().unwrap();
        let source = write_test_plugin(
            tmp.path(),
            "Skyrim.esm",
            "skyrimse",
            vec![rec("GLOB", 0x1200, "Primary")],
        );
        let handle = load_no_py(&source.to_string_lossy(), Some("skyrimse")).unwrap();
        let (mut plugin, _) = clone_plugin_handle_state_no_py(handle).unwrap();
        plugin_handle_close_native(handle);

        prepare_output_plugin(
            &mut plugin,
            Vec::new(),
            &HashSet::new(),
            "skyrimse",
            "Skyrim_Merged.esm",
        );

        assert_eq!(plugin.plugin_name, "Skyrim_Merged.esm");
    }
}
