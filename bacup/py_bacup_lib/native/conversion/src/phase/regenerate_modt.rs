//! Phase: `regenerate_modt` — post-asset MODT (re)population (Plan B).
//!
//! Runs after the asset waves and before `build_esp`, against the open target
//! handle. For every model-bearing record it resolves the record's `MODT` by
//! this precedence and inserts the bytes after the model-path subrecord:
//!
//! 1. **Compute** (Plan B) — the record's mesh is in the fresh manifest AND the
//!    record is NOT material-swapped: compute `MODT` from the resolved graph and
//!    replace any source-carried value.
//! 2. **Already harvested** (Plan A) — otherwise, a structurally valid FO4
//!    hash subrecord belongs to a vanilla/reused mesh; leave it untouched.
//! 3. **Reuse from the deployed ESM** (upgrade only) — the mesh is NOT in the
//!    manifest (its asset family was reused this upgrade): re-inject the `MODT`
//!    bytes harvested from the live deployed ESM for that model path.
//! 4. **Drop** — nothing available; remove an empty hash subrecord and leave the
//!    slot without `MODT`.
//!
//! v1 restrictions: material-swapped records (`MODS`/`MSWP`) are NOT computed —
//! they fall through to reuse (upgrade) / drop. The deployed-ESM reuse reads an
//! eagerly-loaded handle (lazy/index-only handles have an empty tree).
//!
//! ## Params (JSON)
//! ```text
//! {
//!   "manifest_path":           "<path>", // OR "manifest": { <inline MeshModtManifest> }
//!   "is_upgrade":              <bool>,   // default false
//!   "deployed_esm_path":       "<path>" // deployed ESM for step-3 reuse (upgrade)
//! }
//! ```
//!
//! Phase-contract: NO Python / GIL. Records are walked and mutated directly on
//! the plugin-handle store (`bytes::Bytes` subrecords), like `build_esp`.

use bytes::Bytes;
use rustc_hash::FxHashMap;
use serde_json::Value as JsonValue;
use smol_str::SmolStr;
use std::path::Path;

use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_store_ref,
};

use crate::fixups::harvest_modt::{decode_debr_model_path, fo4_model_slots, normalize_model_path};
use crate::modt_compute::{compute_modt, decode_modt};
use crate::modt_manifest::MeshModtManifest;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::run::OwnedPluginHandle;

fn model_path_string(path_sig: &str, data: &Bytes) -> Option<String> {
    if path_sig == "DATA" {
        return decode_debr_model_path(data);
    }
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    (end > 0).then(|| String::from_utf8_lossy(&data[..end]).into_owned())
}

fn valid_fo4_modt(data: &[u8]) -> bool {
    decode_modt(data).is_some()
}

fn material_swap_sig(path_sig: &str) -> Option<&'static str> {
    match path_sig {
        "MODL" => Some("MODS"),
        "MOD2" => Some("MO2S"),
        "MOD3" => Some("MO3S"),
        "MOD4" => Some("MO4S"),
        "MOD5" => Some("MO5S"),
        _ => None,
    }
}

/// Walk every record under `items` (recursing through groups), immutably.
fn walk_records<'a>(items: &'a [ParsedItem], f: &mut impl FnMut(&'a ParsedRecord)) {
    for item in items {
        match item {
            ParsedItem::Record(r) => f(r),
            ParsedItem::Group(g) => walk_records(&g.children, f),
        }
    }
}

/// Walk every record under `items` (recursing through groups), mutably.
fn walk_records_mut(items: &mut [ParsedItem], f: &mut impl FnMut(&mut ParsedRecord)) {
    for item in items {
        match item {
            ParsedItem::Record(r) => f(r),
            ParsedItem::Group(g) => walk_records_mut(&mut g.children, f),
        }
    }
}

/// Build a `normalized-model-path -> MODT bytes` index from a (deployed) plugin
/// handle's records — the reuse source for upgrade step 3. Mirrors
/// `harvest_modt::harvest_modt_index` but reads raw `ParsedSubrecord`s (no schema
/// decode needed: model paths are zstrings, hashes are opaque bytes).
fn harvest_deployed_index(handle_id: u64) -> Result<FxHashMap<String, Vec<u8>>, PhaseError> {
    let store = plugin_handle_store_ref()
        .lock()
        .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
    let slot = store.get(&handle_id).ok_or_else(|| {
        PhaseError::BadParams(format!("unknown deployed plugin handle: {handle_id}"))
    })?;

    let mut index: FxHashMap<String, Vec<u8>> = FxHashMap::default();
    walk_records(&slot.parsed.root_items, &mut |rec| {
        let Some(slots) = fo4_model_slots().get(rec.signature.as_str()) else {
            return;
        };
        for (position, path_subrecord) in rec.subrecords.iter().enumerate() {
            let Some(model_slot) = slots
                .iter()
                .find(|slot| path_subrecord.signature.as_str() == slot.path_sig)
            else {
                continue;
            };
            let Some(hash_subrecord) = rec.subrecords.get(position + 1) else {
                continue;
            };
            if hash_subrecord.signature.as_str() != model_slot.hash_sig
                || !valid_fo4_modt(&hash_subrecord.data)
            {
                continue;
            }
            let Some(path) = model_path_string(&model_slot.path_sig, &path_subrecord.data) else {
                continue;
            };
            let key = normalize_model_path(&path);
            if key.is_empty() {
                continue;
            }
            index
                .entry(key)
                .or_insert_with(|| hash_subrecord.data.to_vec());
        }
    });
    Ok(index)
}

/// Apply the MODT precedence to a single record. Returns the number of slots
/// whose `MODT` was inserted.
fn apply_record(
    rec: &mut ParsedRecord,
    manifest: &MeshModtManifest,
    deployed_index: &FxHashMap<String, Vec<u8>>,
    is_upgrade: bool,
) -> u32 {
    let Some(slots) = fo4_model_slots().get(rec.signature.as_str()) else {
        return 0;
    };
    let material_swaps = slots
        .iter()
        .filter_map(|slot| {
            material_swap_sig(&slot.path_sig).map(|swap| (slot.path_sig.clone(), swap))
        })
        .map(|(path, swap)| {
            let present = rec
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature.as_str() == swap);
            (path, present)
        })
        .collect::<FxHashMap<_, _>>();

    let mut source = std::mem::take(&mut rec.subrecords).into_iter().peekable();
    let mut rebuilt = Vec::new();
    let mut changed = 0u32;
    while let Some(path_subrecord) = source.next() {
        let Some(model_slot) = slots
            .iter()
            .find(|slot| path_subrecord.signature.as_str() == slot.path_sig)
        else {
            if slots
                .iter()
                .any(|slot| path_subrecord.signature.as_str() == slot.hash_sig)
            {
                changed += 1;
            } else {
                rebuilt.push(path_subrecord);
            }
            continue;
        };

        let key = model_path_string(&model_slot.path_sig, &path_subrecord.data)
            .map(|path| normalize_model_path(&path))
            .filter(|path| !path.is_empty());
        rebuilt.push(path_subrecord);

        let existing = source
            .peek()
            .is_some_and(|next| next.signature.as_str() == model_slot.hash_sig)
            .then(|| source.next().expect("peeked model-info subrecord"));
        let existing_bytes = existing.as_ref().map(|model_info| model_info.data.to_vec());
        let has_swap = material_swaps
            .get(&model_slot.path_sig)
            .copied()
            .unwrap_or(false);
        let replacement = key
            .as_deref()
            .and_then(|path| manifest.get(path))
            .and_then(|entry| compute_modt(entry, has_swap))
            .or_else(|| {
                existing_bytes
                    .as_ref()
                    .filter(|bytes| valid_fo4_modt(bytes))
                    .cloned()
            })
            .or_else(|| {
                is_upgrade
                    .then(|| key.as_deref().and_then(|path| deployed_index.get(path)))
                    .flatten()
                    .cloned()
            });

        if replacement != existing_bytes {
            changed += 1;
        }
        if let Some(bytes) = replacement {
            rebuilt.push(ParsedSubrecord {
                signature: SmolStr::from(&model_slot.hash_sig),
                data: Bytes::from(bytes),
                semantic_type: None,
            });
        }
    }
    rec.subrecords = rebuilt;
    changed
}

fn apply_to_handle(
    handle_id: u64,
    manifest: &MeshModtManifest,
    deployed_index: &FxHashMap<String, Vec<u8>>,
    is_upgrade: bool,
) -> Result<u32, PhaseError> {
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| PhaseError::BadParams(format!("unknown output_handle_id: {handle_id}")))?;

    let mut changed = 0u32;
    walk_records_mut(&mut slot.parsed.root_items, &mut |rec| {
        changed += apply_record(rec, manifest, deployed_index, is_upgrade);
    });
    if changed > 0 {
        slot.invalidate_sections();
    }
    Ok(changed)
}

fn load_manifest(p: &JsonValue) -> Result<MeshModtManifest, PhaseError> {
    if let Some(path) = p.get("manifest_path").and_then(|v| v.as_str()) {
        let text = std::fs::read_to_string(path)
            .map_err(|e| PhaseError::Internal(format!("read manifest '{path}': {e}")))?;
        serde_json::from_str(&text)
            .map_err(|e| PhaseError::BadParams(format!("parse manifest '{path}': {e}")))
    } else if let Some(inline) = p.get("manifest") {
        serde_json::from_value(inline.clone())
            .map_err(|e| PhaseError::BadParams(format!("parse inline manifest: {e}")))
    } else {
        Ok(MeshModtManifest::default())
    }
}

pub struct RegenerateModtPhase;

impl Phase for RegenerateModtPhase {
    fn name(&self) -> &'static str {
        "regenerate_modt"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;
        let is_upgrade = p
            .get("is_upgrade")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        for legacy_key in ["output_handle_id", "deployed_esm_handle_id"] {
            if p.get(legacy_key).is_some() {
                return Err(PhaseError::BadParams(format!(
                    "regenerate_modt: legacy parameter is not supported: {legacy_key}"
                )));
            }
        }
        let deployed_path = p
            .get("deployed_esm_path")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty());

        let manifest = load_manifest(p)?;
        ctx.check_cancel()?;

        let deployed = match (is_upgrade, deployed_path) {
            (true, Some(path)) => Some(
                OwnedPluginHandle::load(Path::new(path), ctx.run.target.as_str(), None)
                    .map_err(|error| PhaseError::BadParams(format!("regenerate_modt: {error}")))?,
            ),
            _ => None,
        };
        let deployed_index = deployed
            .as_ref()
            .map(|handle| harvest_deployed_index(handle.id()))
            .transpose()?
            .unwrap_or_default();
        ctx.check_cancel()?;

        let changed = apply_to_handle(
            ctx.run.target_handle_id,
            &manifest,
            &deployed_index,
            is_upgrade,
        )?;
        Ok(PhaseReport {
            records_changed: changed,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use esp_authoring_core::plugin_runtime::{
        insert_parsed_record_in_slot, plugin_handle_close_native, plugin_handle_new_native,
        plugin_handle_save_no_py,
    };

    use crate::modt_compute::encode_modt;
    use crate::modt_manifest::{ManifestTexture, MeshModtEntry, MeshModtManifest};
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    fn sr(sig: &str, data: &[u8]) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::from(sig),
            data: Bytes::copy_from_slice(data),
            semantic_type: None,
        }
    }

    fn stat(form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        record("STAT", form_id, subrecords)
    }

    fn record(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::from(sig),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn debr_data(percentage: u8, path: &str, has_collision: u8) -> ParsedSubrecord {
        let mut data = vec![percentage];
        data.extend_from_slice(path.as_bytes());
        data.push(0);
        data.push(has_collision);
        sr("DATA", &data)
    }

    fn modt_of(rec: &ParsedRecord) -> Option<Vec<u8>> {
        rec.subrecords
            .iter()
            .find(|s| s.signature.as_str() == "MODT")
            .map(|s| s.data.to_vec())
    }

    /// Read a record back from a handle by form_id.
    fn read_record(handle_id: u64, form_id: u32) -> ParsedRecord {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle_id).unwrap();
        let mut found = None;
        walk_records(&slot.parsed.root_items, &mut |r| {
            if r.form_id == form_id {
                found = Some(r.clone());
            }
        });
        found.unwrap_or_else(|| panic!("record {form_id:06X} not found"))
    }

    fn diffuse(path: &str) -> ManifestTexture {
        ManifestTexture {
            path: path.to_string(),
            role: "diffuse".to_string(),
        }
    }

    fn sample_entry() -> MeshModtEntry {
        MeshModtEntry {
            materials: vec!["materials\\novel\\mesh.bgsm".to_string()],
            textures: vec![
                diffuse("textures\\novel\\mesh_d.dds"),
                ManifestTexture {
                    path: "textures\\novel\\mesh_n.dds".to_string(),
                    role: "normal".to_string(),
                },
            ],
            addon_nodes: vec![],
        }
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

    fn run_phase(output_handle: u64, params: JsonValue) -> PhaseReport {
        let run_id = make_run(output_handle);
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
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
            RegenerateModtPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        drop_run(run_id).unwrap();
        report
    }

    #[test]
    fn full_build_computes_novel_leaves_swap_and_harvested() {
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();

        // 1. Novel non-swapped record → gets computed MODT.
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            stat(0x0100_0001, vec![sr("MODL", b"novel\\mesh.nif\0")]),
        );
        // 2. Swapped record (MODS present), mesh in manifest → dropped (no compute).
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            stat(
                0x0100_0002,
                vec![sr("MODL", b"swap\\mesh.nif\0"), sr("MODS", &[0u8; 4])],
            ),
        );
        // 3. Structurally valid harvested record → untouched.
        let valid_harvested = encode_modt(&sample_entry());
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            stat(
                0x0100_0003,
                vec![
                    sr("MODL", b"vanilla\\mesh.nif\0"),
                    sr("MODT", &valid_harvested),
                ],
            ),
        );

        let mut manifest = MeshModtManifest::default();
        manifest
            .meshes
            .insert("novel/mesh.nif".to_string(), sample_entry());
        // Swapped record's mesh is ALSO in the manifest — but must not compute.
        manifest
            .meshes
            .insert("swap/mesh.nif".to_string(), sample_entry());

        let params = serde_json::json!({
            "manifest": serde_json::to_value(&manifest).unwrap(),
            "is_upgrade": false,
        });
        let report = run_phase(out, params);
        assert_eq!(report.records_changed, 1, "only the novel record computed");

        // 1. Novel → computed MODT == encode_modt(entry).
        let novel = read_record(out, 0x0100_0001);
        assert_eq!(
            modt_of(&novel),
            Some(encode_modt(&sample_entry())),
            "novel record gets the computed MODT"
        );
        // 2. Swapped → no MODT (dropped).
        let swapped = read_record(out, 0x0100_0002);
        assert_eq!(modt_of(&swapped), None, "swapped record left without MODT");
        // 3. Harvested → untouched.
        let harvested = read_record(out, 0x0100_0003);
        assert_eq!(
            modt_of(&harvested),
            Some(valid_harvested),
            "harvested record untouched"
        );

        plugin_handle_close_native(out);
    }

    #[test]
    fn fresh_manifest_replaces_source_carried_modt() {
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            stat(
                0x0100_0001,
                vec![sr("MODL", b"novel\\mesh.nif\0"), sr("MODT", b"SOURCE")],
            ),
        );

        let mut manifest = MeshModtManifest::default();
        manifest
            .meshes
            .insert("novel/mesh.nif".to_string(), sample_entry());
        let report = run_phase(
            out,
            serde_json::json!({
                "manifest": serde_json::to_value(&manifest).unwrap(),
                "is_upgrade": false,
            }),
        );

        assert_eq!(report.records_changed, 1);
        assert_eq!(
            modt_of(&read_record(out, 0x0100_0001)),
            Some(encode_modt(&sample_entry()))
        );
        plugin_handle_close_native(out);
    }

    #[test]
    fn empty_modt_is_removed_when_no_replacement_exists() {
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            stat(
                0x0100_0007,
                vec![sr("MODL", b"absent\\mesh.nif\0"), sr("MODT", b"")],
            ),
        );

        let report = run_phase(
            out,
            serde_json::json!({
                "manifest": {},
                "is_upgrade": false,
            }),
        );

        assert_eq!(report.records_changed, 1);
        assert_eq!(modt_of(&read_record(out, 0x0100_0007)), None);
        plugin_handle_close_native(out);
    }

    #[test]
    fn upgrade_reuses_deployed_modt_for_reused_mesh() {
        // Deployed ESM handle: carries the prior-gen MODT for a novel mesh whose
        // assets were reused this upgrade (so it's NOT in the fresh manifest).
        let deployed = plugin_handle_new_native("Deployed.esm", Some("fo4")).unwrap();
        let valid_deployed = encode_modt(&sample_entry());
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&deployed)
                .unwrap(),
            stat(
                0x0100_0009,
                vec![
                    sr("MODL", b"reused\\mesh.nif\0"),
                    sr("MODT", &valid_deployed),
                ],
            ),
        );
        let temp = tempfile::tempdir().unwrap();
        let deployed_path = temp.path().join("Deployed.esm");
        plugin_handle_save_no_py(deployed, deployed_path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(deployed);

        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            stat(0x0100_0009, vec![sr("MODL", b"reused\\mesh.nif\0")]),
        );

        // Empty manifest → mesh not computed → upgrade reuse from deployed.
        let params = serde_json::json!({
            "manifest": {},
            "is_upgrade": true,
            "deployed_esm_path": deployed_path,
        });
        let report = run_phase(out, params);
        assert_eq!(report.records_changed, 1);
        assert!(
            plugin_handle_store_ref()
                .lock()
                .unwrap()
                .values()
                .all(|slot| slot.parsed.file_path != deployed_path.to_string_lossy())
        );

        let rec = read_record(out, 0x0100_0009);
        assert_eq!(
            modt_of(&rec),
            Some(valid_deployed),
            "reused-mesh record gets the deployed-ESM MODT"
        );

        plugin_handle_close_native(out);
    }

    #[test]
    fn upgrade_rejects_malformed_deployed_modt() {
        let deployed = plugin_handle_new_native("Deployed.esm", Some("fo4")).unwrap();
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&deployed)
                .unwrap(),
            stat(
                0x0100_000a,
                vec![
                    sr("MODL", b"reused\\bad.nif\0"),
                    sr("MODT", b"SOURCE-GAME-BYTES"),
                ],
            ),
        );
        let temp = tempfile::tempdir().unwrap();
        let deployed_path = temp.path().join("Deployed.esm");
        plugin_handle_save_no_py(deployed, deployed_path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(deployed);

        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            stat(0x0100_000a, vec![sr("MODL", b"reused\\bad.nif\0")]),
        );

        let report = run_phase(
            out,
            serde_json::json!({
                "manifest": {},
                "is_upgrade": true,
                "deployed_esm_path": deployed_path,
            }),
        );
        assert_eq!(report.records_changed, 0);
        assert_eq!(modt_of(&read_record(out, 0x0100_000a)), None);
        assert!(
            plugin_handle_store_ref()
                .lock()
                .unwrap()
                .values()
                .all(|slot| slot.parsed.file_path != deployed_path.to_string_lossy())
        );

        plugin_handle_close_native(out);
    }

    #[test]
    fn full_build_drops_novel_mesh_not_in_manifest() {
        // Not upgrade, not in manifest, no deployed → drop (no MODT).
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            stat(0x0100_0007, vec![sr("MODL", b"absent\\mesh.nif\0")]),
        );

        let params = serde_json::json!({
            "manifest": {},
            "is_upgrade": false,
        });
        let report = run_phase(out, params);
        assert_eq!(report.records_changed, 0);
        assert_eq!(modt_of(&read_record(out, 0x0100_0007)), None);

        plugin_handle_close_native(out);
    }

    #[test]
    fn debr_rebuilds_every_repeated_data_row_from_manifest() {
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        let mut skyrim_legacy = vec![0u8; 72];
        skyrim_legacy[..4].copy_from_slice(&0x85f3_0f60_u32.to_le_bytes());
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            record(
                "DEBR",
                0x0100_0010,
                vec![
                    sr("EDID", b"TestDebris\0"),
                    debr_data(50, "Effects\\IceA.nif", 1),
                    sr("MODT", &skyrim_legacy),
                    debr_data(50, "Effects\\IceB.nif", 0),
                    sr("MODT", b"FNV-LEGACY"),
                ],
            ),
        );

        let mut manifest = MeshModtManifest::default();
        manifest
            .meshes
            .insert("effects/icea.nif".to_string(), sample_entry());
        manifest
            .meshes
            .insert("effects/iceb.nif".to_string(), sample_entry());
        let report = run_phase(
            out,
            serde_json::json!({
                "manifest": serde_json::to_value(&manifest).unwrap(),
                "is_upgrade": false,
            }),
        );

        assert_eq!(report.records_changed, 2);
        let converted = read_record(out, 0x0100_0010);
        let sigs = converted
            .subrecords
            .iter()
            .map(|subrecord| subrecord.signature.as_str())
            .collect::<Vec<_>>();
        assert_eq!(sigs, vec!["EDID", "DATA", "MODT", "DATA", "MODT"]);
        let modts = converted
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "MODT")
            .collect::<Vec<_>>();
        assert_eq!(modts.len(), 2);
        for modt in modts {
            assert_eq!(&modt.data[..4], &4u32.to_le_bytes());
            assert!(decode_modt(&modt.data).is_some());
        }
        plugin_handle_close_native(out);
    }

    #[test]
    fn debr_drops_legacy_modt_when_manifest_has_no_mesh() {
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        let mut skyrim_legacy = vec![0u8; 72];
        skyrim_legacy[..4].copy_from_slice(&0x85f3_0f60_u32.to_le_bytes());
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            record(
                "DEBR",
                0x0100_0011,
                vec![
                    debr_data(100, "Effects\\MissingA.nif", 1),
                    sr("MODT", &skyrim_legacy),
                    debr_data(100, "Effects\\MissingB.nif", 1),
                    sr("MODT", &[0x60, 0x0f, 0xf3, 0x85]),
                ],
            ),
        );

        let report = run_phase(
            out,
            serde_json::json!({
                "manifest": {},
                "is_upgrade": false,
            }),
        );

        assert_eq!(report.records_changed, 2);
        let converted = read_record(out, 0x0100_0011);
        assert_eq!(
            converted
                .subrecords
                .iter()
                .map(|subrecord| subrecord.signature.as_str())
                .collect::<Vec<_>>(),
            vec!["DATA", "DATA"]
        );
        plugin_handle_close_native(out);
    }

    #[test]
    fn schema_derived_slots_cover_omitted_signature_mod5_and_anam() {
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        for (sig, form_id, path_sig, hash_sig, path) in [
            ("IPCT", 0x0100_0020, "MODL", "MODT", "Effects\\Impact.nif"),
            ("ARMA", 0x0100_0021, "MOD5", "MO5T", "Armor\\Female1st.nif"),
            ("MATT", 0x0100_0022, "ANAM", "MODT", "Materials\\Layer.nif"),
        ] {
            insert_parsed_record_in_slot(
                &mut *plugin_handle_store_ref()
                    .lock()
                    .unwrap()
                    .get_mut(&out)
                    .unwrap(),
                record(
                    sig,
                    form_id,
                    vec![
                        sr(path_sig, format!("{path}\0").as_bytes()),
                        sr(hash_sig, b"LEGACY"),
                    ],
                ),
            );
        }

        let mut manifest = MeshModtManifest::default();
        for path in [
            "effects/impact.nif",
            "armor/female1st.nif",
            "materials/layer.nif",
        ] {
            manifest.meshes.insert(path.to_string(), sample_entry());
        }
        let report = run_phase(
            out,
            serde_json::json!({
                "manifest": serde_json::to_value(&manifest).unwrap(),
                "is_upgrade": false,
            }),
        );

        assert_eq!(report.records_changed, 3);
        for (form_id, path_sig, hash_sig) in [
            (0x0100_0020, "MODL", "MODT"),
            (0x0100_0021, "MOD5", "MO5T"),
            (0x0100_0022, "ANAM", "MODT"),
        ] {
            let converted = read_record(out, form_id);
            assert_eq!(converted.subrecords[0].signature.as_str(), path_sig);
            assert_eq!(converted.subrecords[1].signature.as_str(), hash_sig);
            assert!(decode_modt(&converted.subrecords[1].data).is_some());
        }
        plugin_handle_close_native(out);
    }

    #[test]
    fn repeated_race_rows_are_processed_independently() {
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            record(
                "RACE",
                0x0100_0023,
                vec![
                    sr("ANAM", b"Actors\\RaceA.nif\0"),
                    sr("MODT", b"LEGACY-A"),
                    sr("MODL", b"Actors\\RaceBody.nif\0"),
                    sr("MODT", b"LEGACY-B"),
                    sr("MODL", b"Actors\\MissingBody.nif\0"),
                    sr("MODT", b"LEGACY-C"),
                ],
            ),
        );

        let mut manifest = MeshModtManifest::default();
        manifest
            .meshes
            .insert("actors/racea.nif".to_string(), sample_entry());
        manifest
            .meshes
            .insert("actors/racebody.nif".to_string(), sample_entry());
        let report = run_phase(
            out,
            serde_json::json!({
                "manifest": serde_json::to_value(&manifest).unwrap(),
                "is_upgrade": false,
            }),
        );

        assert_eq!(report.records_changed, 3);
        let converted = read_record(out, 0x0100_0023);
        assert_eq!(
            converted
                .subrecords
                .iter()
                .map(|subrecord| subrecord.signature.as_str())
                .collect::<Vec<_>>(),
            vec!["ANAM", "MODT", "MODL", "MODT", "MODL"]
        );
        for modt in converted
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "MODT")
        {
            assert!(decode_modt(&modt.data).is_some());
        }
        plugin_handle_close_native(out);
    }
}
