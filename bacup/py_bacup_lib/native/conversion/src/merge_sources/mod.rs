use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use esp_authoring_core::plugin_runtime::{
    LocalizedStringsState, ParsedItem, ParsedPlugin, compiled_schema_for_game_str,
    ensure_core_section, plugin_handle_close_native, plugin_handle_load_no_py,
    plugin_handle_new_native, plugin_handle_save_no_py, plugin_handle_store_ref,
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use thiserror::Error;

use crate::legacy_pack_preflight::{LegacyPackExpectedCounts, LegacyPackOriginRow};

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
    #[serde(default)]
    pub source_strings_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct SigCounts {
    pub deduped: u64,
    pub copied: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct LegacyPackMergeAccounting {
    pub raw_source: LegacyPackExpectedCounts,
    pub override_winners: LegacyPackExpectedCounts,
    pub override_losers: LegacyPackExpectedCounts,
    pub editor_id_deduped: LegacyPackExpectedCounts,
    pub sanitization_drops: LegacyPackExpectedCounts,
    pub final_survivors: LegacyPackExpectedCounts,
    pub form_key_remaps: LegacyPackExpectedCounts,
}

impl LegacyPackMergeAccounting {
    fn is_conserved(&self) -> bool {
        self.raw_source.fnv
            == self.override_losers.fnv
                + self.editor_id_deduped.fnv
                + self.sanitization_drops.fnv
                + self.final_survivors.fnv
            && self.raw_source.fo3
                == self.override_losers.fo3
                    + self.editor_id_deduped.fo3
                    + self.sanitization_drops.fo3
                    + self.final_survivors.fo3
    }
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
    pub pack_origins: Vec<LegacyPackOriginRow>,
    pub pack_accounting: LegacyPackMergeAccounting,
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
    #[error("PACK provenance mismatch: expected {expected}, got {actual}")]
    PackProvenanceMismatch { expected: usize, actual: usize },
    #[error("PACK accounting does not conserve raw source records")]
    PackAccountingMismatch,
}

pub(crate) fn load_no_py(path: &str, game: Option<&str>) -> Result<u64, MergeError> {
    load_no_py_with_strings(path, game, None)
}

fn load_no_py_with_strings(
    path: &str,
    game: Option<&str>,
    strings_dir: Option<&std::path::Path>,
) -> Result<u64, MergeError> {
    plugin_handle_load_no_py(
        path,
        game,
        strings_dir.map(|path| path.to_string_lossy()).as_deref(),
        None,
        true,
    )
    .map_err(MergeError::Load)
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

fn add_pack_count(counts: &mut LegacyPackExpectedCounts, source_game: &str, amount: usize) {
    match source_game.trim().to_ascii_lowercase().as_str() {
        "fnv" => counts.fnv += amount,
        "fo3" => counts.fo3 += amount,
        _ => {}
    }
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
            let handle = load_no_py_with_strings(
                &path.to_string_lossy(),
                Some(&opts.game),
                opts.source_strings_dir.as_deref(),
            )?;
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
            let handle = load_no_py_with_strings(
                &path.to_string_lossy(),
                Some(grafted_game),
                opts.source_strings_dir.as_deref(),
            )?;
            handles.push(handle);
            grafted.push(handle);
        }
        let primary_schema = compiled_schema_for_game_str(&opts.game).map_err(MergeError::Load)?;
        let grafted_schema =
            compiled_schema_for_game_str(grafted_game).map_err(MergeError::Load)?;
        validate_master_chain(&primary)?;
        validate_master_chain(&grafted)?;
        let mut primary = flatten::flatten_lineage(&primary)?;
        let primary_records = count_records(&primary.tree);
        let primary_ids = primary.used_ids.clone();
        let mut pack_accounting = LegacyPackMergeAccounting::default();
        add_pack_count(
            &mut pack_accounting.raw_source,
            &opts.game,
            primary.raw_pack_records as usize,
        );
        add_pack_count(
            &mut pack_accounting.override_winners,
            &opts.game,
            primary.pack_origins.len(),
        );
        add_pack_count(
            &mut pack_accounting.override_losers,
            &opts.game,
            primary.raw_pack_records as usize - primary.pack_origins.len(),
        );

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
            add_pack_count(
                &mut pack_accounting.raw_source,
                grafted_game,
                grafted.raw_pack_records as usize,
            );
            add_pack_count(
                &mut pack_accounting.override_winners,
                grafted_game,
                grafted.pack_origins.len(),
            );
            add_pack_count(
                &mut pack_accounting.override_losers,
                grafted_game,
                grafted.raw_pack_records as usize - grafted.pack_origins.len(),
            );
            let classification = classify::classify_grafted(
                &grafted.tree,
                &primary.eid_index,
                &mut primary.used_ids,
            );
            let grafted_pack_origins = grafted.pack_origins;
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
            for (grafted_form_id, mut origin) in grafted_pack_origins {
                if classification.dropped.contains(&grafted_form_id) {
                    add_pack_count(
                        &mut pack_accounting.editor_id_deduped,
                        &origin.source_game,
                        1,
                    );
                    continue;
                }
                if let Some(&merged_form_id) = classification.remap.get(&grafted_form_id) {
                    origin.form_id_remapped |= merged_form_id != grafted_form_id;
                    primary.pack_origins.insert(merged_form_id, origin);
                }
            }
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
        let output_name = opts
            .output_path
            .file_name()
            .ok_or_else(|| MergeError::Io("output path has no file name".to_string()))?
            .to_string_lossy()
            .into_owned();
        let final_pack_ids = collect_record_ids_of_sig(&primary.tree, "PACK");
        for (&form_id, origin) in &primary.pack_origins {
            if !final_pack_ids.contains(&form_id) {
                add_pack_count(
                    &mut pack_accounting.sanitization_drops,
                    &origin.source_game,
                    1,
                );
            }
        }
        primary
            .pack_origins
            .retain(|form_id, _| final_pack_ids.contains(form_id));
        if primary.pack_origins.len() != final_pack_ids.len() {
            return Err(MergeError::PackProvenanceMismatch {
                expected: final_pack_ids.len(),
                actual: primary.pack_origins.len(),
            });
        }
        for origin in primary.pack_origins.values() {
            add_pack_count(&mut pack_accounting.final_survivors, &origin.source_game, 1);
            if origin.form_id_remapped {
                add_pack_count(&mut pack_accounting.form_key_remaps, &origin.source_game, 1);
            }
        }
        if !pack_accounting.is_conserved() {
            return Err(MergeError::PackAccountingMismatch);
        }
        let mut pack_origins = primary
            .pack_origins
            .iter()
            .map(|(&merged_form_id, origin)| LegacyPackOriginRow {
                merged_form_key: format!("{merged_form_id:06X}@{output_name}"),
                source_game: origin.source_game.clone(),
                source_plugin: origin.source_plugin.clone(),
                source_form_key: format!("{:08X}@{}", origin.source_form_id, origin.source_plugin),
            })
            .collect::<Vec<_>>();
        pack_origins.sort_by(|left, right| left.merged_form_key.cmp(&right.merged_form_key));
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
            pack_origins,
            pack_accounting,
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
        install_output_state(handle, primary.template, primary.strings)?;
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

fn collect_record_ids_of_sig(items: &[ParsedItem], signature: &str) -> HashSet<u32> {
    let mut ids = HashSet::new();
    collect_record_ids_of_sig_into(items, signature, &mut ids);
    ids
}

fn collect_record_ids_of_sig_into(items: &[ParsedItem], signature: &str, ids: &mut HashSet<u32>) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == signature => {
                ids.insert(record.form_id);
            }
            ParsedItem::Group(group) => {
                collect_record_ids_of_sig_into(&group.children, signature, ids)
            }
            ParsedItem::Record(_) => {}
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
            source_strings_dir: None,
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
    fn merge_report_preserves_pack_override_winners_and_grafted_origins() {
        let tmp = tempfile::tempdir().unwrap();
        let base = write_test_plugin(
            tmp.path(),
            "FalloutNV.esm",
            "fnv",
            vec![rec("PACK", 0x001200, "BasePackage")],
        );
        let override_record = rec("PACK", 0x001200, "WinningPackageOverride");
        let dlc = write_test_plugin_with_masters(
            tmp.path(),
            "DeadMoney.esm",
            "fnv",
            vec!["FalloutNV.esm".to_string()],
            vec![override_record],
        );
        let grafted = write_test_plugin(
            tmp.path(),
            "Fallout3.esm",
            "fo3",
            vec![rec("PACK", 0x001300, "WinningPackageOverride")],
        );
        let output = tmp.path().join("FNV_FO3_Merged.esm");
        let report_path = tmp.path().join("merge_report.json");

        let report = run(&MergeOptions {
            primary_paths: vec![base, dlc],
            grafted_paths: vec![grafted],
            output_path: output,
            report_path: Some(report_path.clone()),
            game: "fnv".to_string(),
            source_strings_dir: None,
        })
        .unwrap();

        assert_eq!(
            report.pack_origins,
            vec![
                LegacyPackOriginRow {
                    merged_form_key: "001200@FNV_FO3_Merged.esm".to_string(),
                    source_game: "fnv".to_string(),
                    source_plugin: "DeadMoney.esm".to_string(),
                    source_form_key: "00001200@DeadMoney.esm".to_string(),
                },
                LegacyPackOriginRow {
                    merged_form_key: "001300@FNV_FO3_Merged.esm".to_string(),
                    source_game: "fo3".to_string(),
                    source_plugin: "Fallout3.esm".to_string(),
                    source_form_key: "00001300@Fallout3.esm".to_string(),
                },
            ]
        );
        let serialized: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_path).unwrap()).unwrap();
        let roundtrip: Vec<LegacyPackOriginRow> =
            serde_json::from_value(serialized["pack_origins"].clone()).unwrap();
        assert_eq!(roundtrip, report.pack_origins);
        assert_eq!(
            report.by_signature["PACK"],
            SigCounts {
                deduped: 0,
                copied: 1
            }
        );
        assert_eq!(
            report.pack_accounting,
            LegacyPackMergeAccounting {
                raw_source: LegacyPackExpectedCounts { fnv: 2, fo3: 1 },
                override_winners: LegacyPackExpectedCounts { fnv: 1, fo3: 1 },
                override_losers: LegacyPackExpectedCounts { fnv: 1, fo3: 0 },
                editor_id_deduped: LegacyPackExpectedCounts::default(),
                sanitization_drops: LegacyPackExpectedCounts::default(),
                final_survivors: LegacyPackExpectedCounts { fnv: 1, fo3: 1 },
                form_key_remaps: LegacyPackExpectedCounts::default(),
            }
        );
        assert_eq!(serialized["pack_accounting"]["raw_source"]["fnv"], 2);
        assert_eq!(serialized["pack_accounting"]["final_survivors"]["fo3"], 1);
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
            source_strings_dir: None,
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
            source_strings_dir: None,
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
            source_strings_dir: None,
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
            source_strings_dir: None,
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
    fn wilderness_cell_collision_preserves_fo3_coordinate_and_children() {
        fn exterior_cell(
            form_id: u32,
            coordinates: (i32, i32),
            land_form_id: Option<u32>,
        ) -> Vec<ParsedItem> {
            let mut cell = rec("CELL", form_id, "Wilderness");
            let mut xclc = Vec::new();
            xclc.extend_from_slice(&coordinates.0.to_le_bytes());
            xclc.extend_from_slice(&coordinates.1.to_le_bytes());
            cell.subrecords.push(ParsedSubrecord {
                signature: "XCLC".into(),
                data: Bytes::from(xclc),
                semantic_type: None,
            });
            let children = land_form_id
                .map(|land| {
                    vec![ParsedItem::Group(ParsedGroup {
                        label: form_id.to_le_bytes(),
                        group_type: 9,
                        tail: Bytes::new(),
                        children: vec![ParsedItem::Record(rec("LAND", land, ""))],
                    })]
                })
                .unwrap_or_default();
            vec![
                ParsedItem::Record(cell),
                ParsedItem::Group(ParsedGroup {
                    label: form_id.to_le_bytes(),
                    group_type: 6,
                    tail: Bytes::new(),
                    children,
                }),
            ]
        }

        fn world(form_id: u32, editor_id: &str, cell_items: Vec<ParsedItem>) -> Vec<ParsedItem> {
            vec![
                ParsedItem::Record(rec("WRLD", form_id, editor_id)),
                ParsedItem::Group(ParsedGroup {
                    label: form_id.to_le_bytes(),
                    group_type: 1,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Group(ParsedGroup {
                        label: 0u32.to_le_bytes(),
                        group_type: 4,
                        tail: Bytes::new(),
                        children: vec![ParsedItem::Group(ParsedGroup {
                            label: 0u32.to_le_bytes(),
                            group_type: 5,
                            tail: Bytes::new(),
                            children: cell_items,
                        })],
                    })],
                }),
            ]
        }

        fn coordinates(record: &ParsedRecord) -> Option<(i32, i32)> {
            let bytes = &record
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "XCLC")?
                .data;
            Some((
                i32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?),
                i32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?),
            ))
        }

        fn find_cell_group<'a>(
            items: &'a [ParsedItem],
            world_editor_id: &str,
            wanted_coordinates: (i32, i32),
        ) -> Option<(&'a ParsedRecord, &'a ParsedGroup)> {
            let top = items.iter().find_map(|item| match item {
                ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"WRLD" => {
                    Some(group)
                }
                _ => None,
            })?;
            let mut world_children = None;
            for (index, item) in top.children.iter().enumerate() {
                let ParsedItem::Record(record) = item else {
                    continue;
                };
                if record.signature.as_str() != "WRLD"
                    || editor_id_from_effective_subrecords(&record.subrecords) != world_editor_id
                {
                    continue;
                }
                world_children =
                    top.children[index + 1..]
                        .iter()
                        .find_map(|candidate| match candidate {
                            ParsedItem::Group(group)
                                if group.group_type == 1
                                    && u32::from_le_bytes(group.label) == record.form_id =>
                            {
                                Some(group)
                            }
                            _ => None,
                        });
                break;
            }
            fn find<'a>(
                items: &'a [ParsedItem],
                wanted_coordinates: (i32, i32),
            ) -> Option<(&'a ParsedRecord, &'a ParsedGroup)> {
                for (index, item) in items.iter().enumerate() {
                    match item {
                        ParsedItem::Record(record)
                            if record.signature.as_str() == "CELL"
                                && coordinates(record) == Some(wanted_coordinates) =>
                        {
                            let child_group =
                                items[index + 1..].iter().find_map(
                                    |candidate| match candidate {
                                        ParsedItem::Group(group)
                                            if group.group_type == 6
                                                && u32::from_le_bytes(group.label)
                                                    == record.form_id =>
                                        {
                                            Some(group)
                                        }
                                        _ => None,
                                    },
                                )?;
                            return Some((record, child_group));
                        }
                        ParsedItem::Group(group) => {
                            if let Some(found) = find(&group.children, wanted_coordinates) {
                                return Some(found);
                            }
                        }
                        _ => {}
                    }
                }
                None
            }
            find(&world_children?.children, wanted_coordinates)
        }

        let tmp = tempfile::tempdir().unwrap();
        let mut primary_worlds = world(
            0x1000,
            "NVDLC03BigMT",
            exterior_cell(0x162A, (-8, 16), None),
        );
        primary_worlds.extend(world(
            0xDA726,
            "WastelandNV",
            exterior_cell(0xDDCAB, (16, 22), Some(0xDE20C)),
        ));
        let primary = write_test_plugin_items(
            tmp.path(),
            "FalloutNV.esm",
            "fnv",
            Vec::new(),
            vec![ParsedItem::Group(ParsedGroup {
                label: *b"WRLD",
                group_type: 0,
                tail: Bytes::new(),
                children: primary_worlds,
            })],
        );
        let grafted = write_test_plugin_items(
            tmp.path(),
            "Fallout3.esm",
            "fo3",
            Vec::new(),
            vec![ParsedItem::Group(ParsedGroup {
                label: *b"WRLD",
                group_type: 0,
                tail: Bytes::new(),
                children: world(
                    0x003C,
                    "Wasteland",
                    exterior_cell(0x162A, (18, 18), Some(0x1FD7)),
                ),
            })],
        );
        let output = tmp.path().join("FNV_FO3_Merged.esm");

        run(&MergeOptions {
            primary_paths: vec![primary],
            grafted_paths: vec![grafted],
            output_path: output.clone(),
            report_path: None,
            game: "fnv".to_string(),
            source_strings_dir: None,
        })
        .unwrap();

        let handle = load_no_py(output.to_str().unwrap(), Some("fnv")).unwrap();
        let (plugin, _) = clone_plugin_handle_state_no_py(handle).unwrap();
        let (fo3_cell, child_group) =
            find_cell_group(&plugin.root_items, "Wasteland", (18, 18)).unwrap();
        assert_ne!(fo3_cell.form_id, 0x162A);
        assert!(child_group.children.iter().any(|item| matches!(
            item,
            ParsedItem::Group(group)
                if group.group_type == 9
                    && u32::from_le_bytes(group.label) == fo3_cell.form_id
                    && group.children.iter().any(|child| matches!(
                        child,
                        ParsedItem::Record(record)
                            if record.signature.as_str() == "LAND" && record.form_id == 0x1FD7
                    ))
        )));
        let (fnv_cell, _) = find_cell_group(&plugin.root_items, "WastelandNV", (16, 22)).unwrap();
        assert_eq!(fnv_cell.form_id, 0xDDCAB);
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
            source_strings_dir: None,
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
            source_strings_dir: None,
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
            source_strings_dir: None,
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
