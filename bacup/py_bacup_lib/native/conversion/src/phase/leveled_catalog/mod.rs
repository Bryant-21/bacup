//! `emit_leveled_catalog`: FO76 leveled-list semantics for the Tales runtime roll.
//! Design: mods/B21_TalesFromAppalachia/docs/2026-10-04-fo76-leveled-lists-runtime-design.md

pub(crate) mod decode;
pub(crate) mod ess;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::Path;

use esp_authoring_core::plugin_runtime::effective_subrecords_for_record;
use serde_json::{Map, Value, json};

use crate::fixups::curve_table::curve_points;
use crate::ids::{FormKey, SigCode};
use crate::phase::equipment_condition::curve_path;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::session::{HandleRawScan, PluginSession, open_session};
use crate::translator::Game;
use decode::{Diagnostics, Keys, ListIdentity, RawList, RawRegion};

pub struct LeveledCatalogPhase;

const SCHEMA_VERSION: u32 = 1;
const DELETED: u32 = 0x20;

struct SourceList {
    raw: u32,
    signature: &'static str,
    editor_id: String,
    list: RawList,
}

#[derive(Default)]
struct SourceData {
    lists: Vec<SourceList>,
    undecodable: Vec<String>,
    lvln: HashSet<u32>,
    factions: HashSet<u32>,
    condition_forms: HashMap<u32, Vec<[u8; 32]>>,
    regions: HashMap<u32, (String, RawRegion)>,
    curves: HashMap<u32, String>,
    npc_templates: Vec<(u32, u32)>,
    ess: ess::SourceEss,
}

pub(crate) struct Resolved {
    pub(crate) masters: Vec<String>,
    pub(crate) plugin: String,
    pub(crate) outputs: HashMap<u32, String>,
    pub(crate) factions: HashSet<u32>,
}

impl Resolved {
    pub(crate) fn owner(&self, raw: u32) -> Option<&str> {
        let index = (raw >> 24) as usize;
        if index == self.masters.len() || index == 0xFF {
            Some(&self.plugin)
        } else {
            self.masters.get(index).map(String::as_str)
        }
    }
}

impl Keys for Resolved {
    fn source(&self, raw: u32) -> Option<String> {
        self.owner(raw).map(|owner| format!("{owner}:{:06X}", raw & 0xFF_FFFF))
    }
    fn output(&self, raw: u32) -> Option<String> {
        self.outputs.get(&raw).cloned()
    }
    fn is_faction(&self, raw: u32) -> bool {
        self.factions.contains(&raw)
    }
}

fn internal(error: impl std::fmt::Display) -> PhaseError {
    PhaseError::Internal(error.to_string())
}

fn sig(signature: &str) -> SigCode {
    SigCode::from_str(signature).expect("four-character signature")
}

fn live<R>(scan: &HandleRawScan<'_>, raw: u32, read: impl FnOnce(&[esp_authoring_core::plugin_runtime::ParsedSubrecord]) -> R) -> Option<R> {
    scan.with_record(raw, |record| {
        (record.flags & DELETED == 0).then(|| read(&effective_subrecords_for_record(record)))
    })
    .flatten()
}

fn read_source(ctx: &PhaseCtx<'_>, scan: &HandleRawScan<'_>) -> Result<SourceData, PhaseError> {
    let mut data = SourceData::default();
    for signature in ["LVLI", "LVLN"] {
        for raw in scan.raw_form_ids_of_sig(sig(signature)) {
            ctx.check_cancel()?;
            if signature == "LVLN" {
                data.lvln.insert(raw);
            }
            match live(scan, raw, |fields| (decode::editor_id(fields), decode::decode_list(fields))) {
                Some((editor_id, Ok(list))) => data.lists.push(SourceList { raw, signature, editor_id, list }),
                Some((editor_id, Err(error))) => data.undecodable.push(format!("{raw:08X} {editor_id}: {error}")),
                None => {}
            }
        }
    }
    data.factions = scan.raw_form_ids_of_sig(sig("FACT")).into_iter().collect();
    for raw in scan.raw_form_ids_of_sig(sig("CNDF")) {
        if let Some(Ok(rows)) = live(scan, raw, decode::decode_conditions) {
            data.condition_forms.insert(raw, rows);
        }
    }
    for raw in scan.raw_form_ids_of_sig(sig("REGN")) {
        if let Some(region) = live(scan, raw, |fields| (decode::editor_id(fields), decode::decode_region(fields))) {
            data.regions.insert(raw, region);
        }
    }
    for raw in scan.raw_form_ids_of_sig(sig("CURV")) {
        if let Some(Some(path)) = live(scan, raw, decode::curve_json_path) {
            data.curves.insert(raw, path);
        }
    }
    for raw in scan.raw_form_ids_of_sig(sig("NPC_")) {
        if let Some(Some(template)) = live(scan, raw, decode::whole_template) {
            data.npc_templates.push((raw, template));
        }
    }
    data.ess = ess::read(scan);
    Ok(data)
}

/// Placed refs are copied by FormID, not through the mapper (`projected_placed.rs`), so an
/// unmapped REFR/ACHR keeps its FormID in the converted plugin.
fn placed_ref_key(source_signature: &str, output_plugin: &str, raw: u32) -> Option<String> {
    matches!(source_signature, "REFR" | "ACHR").then(|| format!("{output_plugin}:{:06X}", raw & 0xFF_FFFF))
}

/// Source FormID -> output FormKey, for forms whose mapped target exists in the
/// converted plugin or one of its masters.
pub(crate) fn resolve_outputs(
    ctx: &PhaseCtx<'_>,
    session: &mut PluginSession<'_>,
    keys: &Resolved,
    wanted: &BTreeSet<u32>,
) -> Result<HashMap<u32, String>, PhaseError> {
    let Some(state) = ctx.run.mapper_state.as_ref() else {
        return Ok(HashMap::new());
    };
    let mut pending = BTreeMap::new();
    let mut placed = Vec::new();
    for &raw in wanted {
        let Some(owner) = keys.owner(raw) else { continue };
        let source = FormKey { local: raw & 0xFF_FFFF, plugin: ctx.run.interner.intern(owner) };
        if let Some(target) = state.source_to_target.get(&source) {
            let owner = ctx.run.interner.resolve(target.plugin).unwrap_or("");
            pending.insert(raw, format!("{owner}:{:06X}", target.local & 0xFF_FFFF));
        } else if owner == keys.plugin {
            placed.push(raw);
        }
    }
    let mut placed_keys = Vec::new();
    {
        let scan = session.handle_raw_scan(ctx.run.source_handle_id).map_err(internal)?;
        for raw in placed {
            let Some((_, signature, _)) = keys.source(raw).and_then(|key| scan.record_metadata_by_form_key(&key)) else {
                continue;
            };
            if let Some(key) = placed_ref_key(&signature, &ctx.run.config.output_plugin_name, raw) {
                placed_keys.push((raw, key, signature));
            }
        }
    }
    let mut outputs = HashMap::new();
    {
        let scan = session.handle_raw_scan(ctx.run.target_handle_id).map_err(internal)?;
        for (raw, key, signature) in placed_keys {
            if scan.record_metadata_by_form_key(&key).is_some_and(|(_, found, _)| found == signature) {
                outputs.insert(raw, key);
            }
        }
    }
    for handle in std::iter::once(ctx.run.target_handle_id).chain(ctx.run.master_handle_ids.iter().copied()) {
        let scan = session.handle_raw_scan(handle).map_err(internal)?;
        pending.retain(|raw, key| {
            if scan.record_metadata_by_form_key(key).is_none() {
                return true;
            }
            outputs.insert(*raw, key.clone());
            false
        });
    }
    Ok(outputs)
}

pub(crate) fn write_catalog(
    mod_path: &Path,
    folder: &str,
    output_plugin: &str,
    catalog: &Value,
) -> Result<(), PhaseError> {
    let filename = Path::new(output_plugin)
        .file_name()
        .ok_or_else(|| PhaseError::BadParams("missing output plugin name".into()))?;
    let destination = mod_path.join(crate::plugin_catalog::catalog_path(folder, &filename.to_string_lossy()));
    let directory = destination.parent().unwrap().to_path_buf();
    let data = serde_json::to_vec(catalog).map_err(internal)?;
    std::fs::create_dir_all(&directory).map_err(internal)?;
    let mut pending = tempfile::NamedTempFile::new_in(&directory).map_err(internal)?;
    pending.write_all(&data).map_err(internal)?;
    pending.persist(destination).map_err(internal)?;
    Ok(())
}

impl Phase for LeveledCatalogPhase {
    fn name(&self) -> &'static str {
        "emit_leveled_catalog"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        if ctx.mod_path.as_os_str().is_empty() {
            return Err(PhaseError::BadParams("leveled catalog requires mod_path".into()));
        }
        let output_plugin = ctx.run.config.output_plugin_name.clone();
        let mut session = open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id)).map_err(internal)?;
        let (masters, plugin) = session
            .handle_load_order(ctx.run.source_handle_id)
            .map(|(masters, plugin)| (masters.to_vec(), plugin.to_string()))
            .map_err(internal)?;
        let source = {
            let scan = session.handle_raw_scan(ctx.run.source_handle_id).map_err(internal)?;
            read_source(ctx, &scan)?
        };

        let mut wanted = BTreeSet::new();
        for item in &source.lists {
            wanted.insert(item.raw);
            decode::list_references(&item.list, &mut wanted);
        }
        for rows in source.condition_forms.values() {
            decode::condition_references(rows, &mut wanted);
        }
        for (npc, template) in &source.npc_templates {
            if source.lvln.contains(template) {
                wanted.insert(*npc);
                wanted.insert(*template);
            }
        }
        for (_, region) in source.regions.values() {
            wanted.extend(region.world);
        }
        let own_index = masters.len() as u32;
        ess::references(&source.ess, &ctx.run.ess_placed_locations, own_index, &mut wanted);
        let mut keys = Resolved { masters, plugin, outputs: HashMap::new(), factions: source.factions.clone() };
        keys.outputs = resolve_outputs(ctx, &mut session, &keys, &wanted)?;
        drop(session);

        let mut diagnostics = Diagnostics::default();
        let mut lists = Vec::new();
        let mut entry_count = 0usize;
        let mut curve_roots = BTreeSet::new();
        let mut condition_form_roots = BTreeSet::new();
        let mut region_roots = BTreeSet::new();
        let own_prefix = format!("{output_plugin}:");
        for item in &source.lists {
            let Some(target) = keys.output(item.raw) else { continue };
            // FO76 lists redirected onto an FO4 record by EditorID (design 2.3).
            let redirected = !target.starts_with(&own_prefix);
            if !redirected && !decode::needs_runtime(&item.list) {
                continue;
            }
            let source_key = keys.source(item.raw).unwrap_or_default();
            let identity = ListIdentity {
                source: &source_key,
                target: &target,
                editor_id: &item.editor_id,
                signature: item.signature,
                scope: if redirected { "seventysix" } else { "always" },
            };
            decode::curve_references(&item.list, &mut curve_roots);
            let rows = decode::conditions_of(&item.list);
            decode::parameter_1_of(&rows, 875, &mut condition_form_roots);
            decode::parameter_1_of(&rows, 849, &mut region_roots);
            decode::parameter_2_of(&rows, 149, &mut region_roots);
            let row = decode::list_json(&identity, &item.list, &keys, &mut diagnostics);
            entry_count += row["entries"].as_array().map_or(0, Vec::len);
            lists.push(row);
        }

        let mut condition_forms = Map::new();
        let mut pending: Vec<u32> = condition_form_roots.into_iter().collect();
        while let Some(raw) = pending.pop() {
            let Some(key) = keys.source(raw) else { continue };
            if condition_forms.contains_key(&key) {
                continue;
            }
            let Some(rows) = source.condition_forms.get(&raw) else { continue };
            let mut nested = BTreeSet::new();
            decode::parameter_1_of(rows, 875, &mut nested);
            decode::parameter_1_of(rows, 849, &mut region_roots);
            decode::parameter_2_of(rows, 149, &mut region_roots);
            pending.extend(nested);
            condition_forms.insert(key, Value::Array(decode::conditions_json(rows, &keys, &mut diagnostics)));
        }

        let mut regions = Map::new();
        for raw in region_roots {
            let (Some(key), Some((editor_id, region))) = (keys.source(raw), source.regions.get(&raw)) else {
                continue;
            };
            regions.insert(
                key,
                json!({
                    "world": region.world.and_then(|world| keys.output(world)),
                    // NukeRegionA/B/C are FO76's dynamic nuke-zone regions (design 4.2.4).
                    "nuke": editor_id.starts_with("NukeRegion"),
                    "polygons": region.polygons,
                }),
            );
        }

        let source_root = ctx
            .run
            .config
            .source_extracted_dir
            .as_deref()
            .unwrap_or(ctx.source_extracted_dir);
        let mut curves = Map::new();
        let mut missing_curves = Vec::new();
        for raw in curve_roots {
            let Some(key) = keys.source(raw) else { continue };
            let points = source
                .curves
                .get(&raw)
                .ok_or_else(|| "no CRVE/JASF".to_string())
                .and_then(|relative| curve_path(source_root, relative).map_err(|error| error.to_string()))
                .and_then(|path| std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display())))
                .and_then(|text| curve_points(&text));
            match points {
                Ok(points) => {
                    curves.insert(key, json!({ "points": points }));
                }
                Err(error) => missing_curves.push(format!("{key}: {error}")),
            }
        }

        let mut delegates = BTreeMap::new();
        for (npc, template) in &source.npc_templates {
            if !source.lvln.contains(template) {
                continue;
            }
            if let (Some(npc), Some(list)) = (keys.output(*npc), keys.output(*template)) {
                delegates.insert(npc, json!(list));
            }
        }
        let delegates: Map<String, Value> = delegates.into_iter().collect();

        // serde_json's preserve_order keeps insertion order; sort so the file is stable.
        lists.sort_by(|left, right| left["source"].as_str().cmp(&right["source"].as_str()));
        let list_count = lists.len();
        let catalog = json!({
            "version": SCHEMA_VERSION,
            "settings": {"source_plugin": keys.plugin},
            "lists": lists,
            "condition_forms": condition_forms,
            "curves": curves,
            "regions": regions,
            "delegates": delegates,
            "ess": ess::catalog_json(&source.ess, &keys, &ctx.run.ess_placed_locations, &output_plugin, own_index),
            "diagnostics": {
                "lists": list_count,
                "entries": entry_count,
                "unmapped_entries": diagnostics.unmapped_entries,
                "unmapped_forms": diagnostics.unmapped_forms,
                "unresolved_conditions": diagnostics.unresolved_conditions,
                "unresolved_values": diagnostics.unresolved_values,
                "undecodable_lists": source.undecodable,
                "missing_curves": missing_curves,
            },
        });
        write_catalog(ctx.mod_path, "LeveledLists", &output_plugin, &catalog)?;
        Ok(PhaseReport {
            assets_written: 1,
            warnings: u32::try_from(diagnostics.unmapped_forms.len()).unwrap_or(u32::MAX),
            ..PhaseReport::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::placed_ref_key;

    #[test]
    fn an_unmapped_placed_ref_keeps_its_formid_in_the_output_plugin() {
        // EN07_NukeCenterMarkerAREF (2D1173), read by SootFlower's GetDistance.
        assert_eq!(placed_ref_key("REFR", "SeventySix.esm", 0x002D_1173).as_deref(), Some("SeventySix.esm:2D1173"));
        assert_eq!(placed_ref_key("ACHR", "SeventySix.esm", 0x0012_3456).as_deref(), Some("SeventySix.esm:123456"));
        assert_eq!(placed_ref_key("WEAP", "SeventySix.esm", 0x0012_3456), None);
    }
}
