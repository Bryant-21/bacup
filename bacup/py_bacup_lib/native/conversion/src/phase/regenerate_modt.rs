//! Phase: `regenerate_modt`, post-asset MODT (re)population.
//!
//! Runs after the asset waves and before `build_esp`, against the open target
//! handle. For every model-bearing record it resolves the record's `MODT` by
//! this precedence and inserts the bytes after the model-path subrecord:
//!
//! 1. **Compute**: the record's mesh is in the fresh manifest: compute
//!    `MODT` from the resolved graph and replace any source-carried value.
//!    Material-swapped slots resolve their `MSWP` against the converted
//!    materials on disk before hashing.
//! 2. **Already harvested**: otherwise, a structurally valid FO4
//!    hash subrecord belongs to a vanilla/reused mesh; leave it untouched.
//! 3. **Reuse from the deployed ESM** (upgrade only) — the mesh is NOT in the
//!    manifest (its asset family was reused this upgrade): re-inject the `MODT`
//!    bytes harvested from the live deployed ESM for that model path.
//! 4. **Drop** — nothing available; remove an empty hash subrecord and leave the
//!    slot without `MODT`.
//!
//! The deployed-ESM reuse reads an eagerly-loaded handle (lazy/index-only
//! handles have an empty tree).
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
use std::path::{Path, PathBuf};

use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_store_ref,
};

use crate::fixups::harvest_modt::{decode_debr_model_path, fo4_model_slots, normalize_model_path};
use crate::modt_compute::{compute_modt, decode_modt, encode_modt};
use crate::modt_manifest::{MeshModtEntry, MeshModtManifest};
use crate::phase::emit_modt_manifest::MaterialTextureCache;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::run::OwnedPluginHandle;

mod shelter_scenery;

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

type MaterialSwap = FxHashMap<String, String>;

#[derive(Clone, Copy)]
enum MaterialSwapRef {
    Absent,
    Invalid,
    FormId(u32),
}

fn zstring(data: &[u8]) -> Option<String> {
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    (end > 0).then(|| String::from_utf8_lossy(&data[..end]).into_owned())
}

fn normalize_material_path(path: &str) -> String {
    let normalized = path
        .trim_matches('\0')
        .trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase();
    normalized
        .strip_prefix("materials/")
        .unwrap_or(&normalized)
        .to_string()
}

fn material_disk_path(data_dir: &Path, material_path: &str) -> PathBuf {
    let normalized = material_path.trim_matches('\0').trim().replace('\\', "/");
    let relative = normalized
        .get(..10)
        .filter(|prefix| prefix.eq_ignore_ascii_case("materials/"))
        .map(|_| &normalized[10..])
        .unwrap_or(&normalized);
    data_dir.join("Materials").join(relative)
}

fn material_swap_from_record(rec: &ParsedRecord) -> Option<MaterialSwap> {
    if rec.signature.as_str() != "MSWP" {
        return None;
    }
    let mut original = None;
    let mut replacements = MaterialSwap::default();
    for subrecord in &rec.subrecords {
        match subrecord.signature.as_str() {
            "BNAM" => original = zstring(&subrecord.data),
            "SNAM" => {
                if let (Some(original), Some(replacement)) =
                    (original.take(), zstring(&subrecord.data))
                {
                    replacements.insert(normalize_material_path(&original), replacement);
                }
            }
            _ => {}
        }
    }
    (!replacements.is_empty()).then_some(replacements)
}

fn material_swap_index(items: &[ParsedItem]) -> FxHashMap<u32, MaterialSwap> {
    let mut index = FxHashMap::default();
    walk_records(items, &mut |rec| {
        if let Some(material_swap) = material_swap_from_record(rec) {
            index.insert(rec.form_id, material_swap);
        }
    });
    index
}

fn material_swap_for_form_id<'a>(
    index: &'a FxHashMap<u32, MaterialSwap>,
    form_id: u32,
) -> Option<&'a MaterialSwap> {
    if let Some(material_swap) = index.get(&form_id) {
        return Some(material_swap);
    }
    let local_form_id = form_id & 0x00ff_ffff;
    let mut matches = index
        .iter()
        .filter(|(candidate, _)| (**candidate & 0x00ff_ffff) == local_form_id)
        .map(|(_, material_swap)| material_swap);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn form_id(data: &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = data.try_into().ok()?;
    let value = u32::from_le_bytes(bytes);
    (value != 0).then_some(value)
}

fn resolve_material_swap_entry(
    entry: &MeshModtEntry,
    material_swap: &MaterialSwap,
    data_dir: &Path,
    cache: &MaterialTextureCache,
) -> Option<MeshModtEntry> {
    let mut materials = Vec::with_capacity(entry.materials.len() * 2);
    let mut textures = Vec::new();
    for original in &entry.materials {
        let effective = material_swap
            .get(&normalize_material_path(original))
            .map(String::as_str)
            .unwrap_or(original);
        let cached = cache.get(&material_disk_path(data_dir, effective), effective);
        let parsed = cached.get()?.as_ref()?;
        textures.extend(parsed.textures.iter().cloned());
        materials.push(effective.to_string());
        if let Some(root) = &parsed.root {
            materials.push(root.clone());
        }
    }
    Some(MeshModtEntry {
        materials,
        textures,
        addon_nodes: entry.addon_nodes.clone(),
    })
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
    material_swap_index: &FxHashMap<u32, MaterialSwap>,
    data_dir: &Path,
    deployed_index: &FxHashMap<String, Vec<u8>>,
    is_upgrade: bool,
    materials: &MaterialTextureCache,
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
            let material_swap = rec
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == swap)
                .map(|subrecord| {
                    form_id(&subrecord.data)
                        .map(MaterialSwapRef::FormId)
                        .unwrap_or(MaterialSwapRef::Invalid)
                })
                .unwrap_or(MaterialSwapRef::Absent);
            (path, material_swap)
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
        let material_swap = material_swaps
            .get(&model_slot.path_sig)
            .copied()
            .unwrap_or(MaterialSwapRef::Absent);
        let replacement = key
            .as_deref()
            .and_then(|path| manifest.get(path))
            .and_then(|entry| match material_swap {
                MaterialSwapRef::FormId(form_id) => {
                    material_swap_for_form_id(material_swap_index, form_id)
                        .and_then(|material_swap| {
                            resolve_material_swap_entry(entry, material_swap, data_dir, materials)
                        })
                        .map(|resolved| encode_modt(&resolved))
                }
                MaterialSwapRef::Absent => compute_modt(entry, false),
                MaterialSwapRef::Invalid => None,
            })
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
    data_dir: &Path,
    deployed_index: &FxHashMap<String, Vec<u8>>,
    is_upgrade: bool,
) -> Result<u32, PhaseError> {
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| PhaseError::BadParams(format!("unknown output_handle_id: {handle_id}")))?;

    let material_swap_index = material_swap_index(&slot.parsed.root_items);
    let mut changed = 0u32;
    let materials = MaterialTextureCache::default();
    walk_records_mut(&mut slot.parsed.root_items, &mut |rec| {
        changed += apply_record(
            rec,
            manifest,
            &material_swap_index,
            data_dir,
            deployed_index,
            is_upgrade,
            &materials,
        );
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
            &ctx.mod_path.join("data"),
            &deployed_index,
            is_upgrade,
        )?;
        let mut report = if ctx.run.source == crate::translator::Game::Fo76
            && ctx.run.target == crate::translator::Game::Fo4
        {
            shelter_scenery::repair(ctx)?
        } else {
            PhaseReport::default()
        };
        report.records_changed += changed;
        Ok(report)
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
    use materials_native::bgsm::{self, BgsmData};

    use crate::modt_manifest::{ManifestTexture, MeshModtEntry, MeshModtManifest};
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    #[test]
    #[ignore = "requires MODT_PLUGIN_CORPUS, MODT_CORPUS_MANIFEST and MODT_CORPUS_ROOT"]
    fn shared_materials_preserve_every_corpus_modt_record() {
        let plugin = PathBuf::from(std::env::var_os("MODT_PLUGIN_CORPUS").unwrap());
        let manifest_path = PathBuf::from(std::env::var_os("MODT_CORPUS_MANIFEST").unwrap());
        let data = PathBuf::from(std::env::var_os("MODT_CORPUS_ROOT").unwrap()).join("data");
        let manifest: MeshModtManifest =
            serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
        let handle = OwnedPluginHandle::load(&plugin, "fo4", None).unwrap();
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle.id()).unwrap();
        let swaps = material_swap_index(&slot.parsed.root_items);
        let shared = MaterialTextureCache::default();
        let deployed = FxHashMap::default();
        let mut records_seen = 0usize;
        let mut records_compared = 0usize;
        let mut changes = 0u64;
        let mut digest = blake3::Hasher::new();
        walk_records(&slot.parsed.root_items, &mut |record| {
            records_seen += 1;
            if !fo4_model_slots().contains_key(record.signature.as_str()) {
                return;
            }
            let mut expected = record.clone();
            let mut actual = record.clone();
            let expected_changes = apply_record(
                &mut expected,
                &manifest,
                &swaps,
                &data,
                &deployed,
                false,
                &MaterialTextureCache::default(),
            );
            let actual_changes = apply_record(
                &mut actual,
                &manifest,
                &swaps,
                &data,
                &deployed,
                false,
                &shared,
            );
            assert_eq!(actual_changes, expected_changes);
            assert_eq!(actual.subrecords.len(), expected.subrecords.len());
            digest.update(record.signature.as_bytes());
            digest.update(&record.form_id.to_le_bytes());
            for (actual, expected) in actual.subrecords.iter().zip(&expected.subrecords) {
                assert_eq!(actual.signature, expected.signature);
                assert_eq!(actual.semantic_type, expected.semantic_type);
                assert_eq!(actual.data, expected.data);
                digest.update(actual.signature.as_bytes());
                digest.update(&(actual.data.len() as u64).to_le_bytes());
                digest.update(&actual.data);
            }
            records_compared += 1;
            changes += actual_changes as u64;
        });
        let report = serde_json::json!({"records_seen": records_seen, "records_compared": records_compared,
            "changes": changes, "identical_subrecords": true, "blake3": digest.finalize().to_hex().to_string()});
        if let Some(path) = std::env::var_os("MODT_RECORD_CORPUS_REPORT") {
            std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        }
        eprintln!("{report}");
    }

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

    fn run_phase_at(output_handle: u64, params: JsonValue, mod_path: &Path) -> PhaseReport {
        let run_id = make_run(output_handle);
        let report = with_run(run_id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let source_dir = mod_path.to_path_buf();
            let mut ctx = PhaseCtx {
                run,
                mod_path,
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

    fn run_phase(output_handle: u64, params: JsonValue) -> PhaseReport {
        let tmp = tempfile::tempdir().unwrap();
        run_phase_at(output_handle, params, tmp.path())
    }

    #[test]
    fn full_build_computes_novel_leaves_unresolved_swap_and_harvested() {
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
        // 2. Swapped record with no resolvable MSWP → dropped (no compute).
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
        // Swapped record's mesh is also in the manifest, but its MSWP is absent.
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
    fn material_swapped_arma_rebuilds_mo2t_and_mo3t_from_effective_bgsm() {
        let out = plugin_handle_new_native("Out.esp", Some("fo4")).unwrap();
        let material_swap_record_id = 0x013e_56a6_u32;
        let material_swap_reference = 0x003e_56a6_u32;

        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            record(
                "MSWP",
                material_swap_record_id,
                vec![
                    sr("BNAM", b"actors\\Wendigo\\Wendigo.BGSM\0"),
                    sr("SNAM", b"actors\\wendigo\\WendigoGlow.bgsm\0"),
                ],
            ),
        );
        insert_parsed_record_in_slot(
            &mut *plugin_handle_store_ref()
                .lock()
                .unwrap()
                .get_mut(&out)
                .unwrap(),
            record(
                "ARMA",
                0x013e_56a7,
                vec![
                    sr("MOD2", b"Actors\\Wendigo\\CharacterAssets\\Wendigo.nif\0"),
                    sr("MO2S", &material_swap_reference.to_le_bytes()),
                    sr("MOD3", b"Actors\\Wendigo\\CharacterAssets\\Wendigo.nif\0"),
                    sr("MO3S", &material_swap_reference.to_le_bytes()),
                ],
            ),
        );

        let temp = tempfile::tempdir().unwrap();
        let material_dir = temp
            .path()
            .join("data")
            .join("Materials")
            .join("actors")
            .join("wendigo");
        std::fs::create_dir_all(&material_dir).unwrap();
        let mut material = BgsmData::default();
        material.header.signature = bgsm::BGSM_SIGNATURE;
        material.header.version = 2;
        material.DiffuseTexture = "Actors\\Wendigo\\wendigo_glow_d.dds".into();
        material.GlowTexture = Some("Actors\\Wendigo\\wendigo_glow_g.dds".into());
        material.RootMaterialPath = "Actors\\Shared\\CreatureTemplate_Wet.bgsm".into();
        std::fs::write(
            material_dir.join("WendigoGlow.bgsm"),
            bgsm::write(&material),
        )
        .unwrap();

        let mut manifest = MeshModtManifest::default();
        manifest.meshes.insert(
            "actors/wendigo/characterassets/wendigo.nif".to_string(),
            MeshModtEntry {
                materials: vec!["actors\\Wendigo\\Wendigo.BGSM".to_string()],
                textures: vec![diffuse("Actors\\Wendigo\\wendigo_d.dds")],
                addon_nodes: vec![],
            },
        );
        let report = run_phase_at(
            out,
            serde_json::json!({
                "manifest": serde_json::to_value(&manifest).unwrap(),
                "is_upgrade": false,
            }),
            temp.path(),
        );

        assert_eq!(report.records_changed, 2);
        let expected = encode_modt(&MeshModtEntry {
            materials: vec![
                "actors\\wendigo\\WendigoGlow.bgsm".to_string(),
                "Actors\\Shared\\CreatureTemplate_Wet.bgsm".to_string(),
            ],
            textures: vec![
                diffuse("Actors\\Wendigo\\wendigo_glow_d.dds"),
                ManifestTexture {
                    path: "Actors\\Wendigo\\wendigo_glow_g.dds".to_string(),
                    role: "glow".to_string(),
                },
            ],
            addon_nodes: vec![],
        });
        let arma = read_record(out, 0x013e_56a7);
        for signature in ["MO2T", "MO3T"] {
            assert_eq!(
                arma.subrecords
                    .iter()
                    .find(|subrecord| subrecord.signature.as_str() == signature)
                    .map(|subrecord| subrecord.data.to_vec()),
                Some(expected.clone()),
                "{signature} should preload the swapped diffuse and glow"
            );
        }

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
