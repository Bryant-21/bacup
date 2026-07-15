// Phase: convert_havok
//
// Params shape (JSON):
// {
//   "source_game":        "fo76" | "fo4" | "fnv" | "fo3" | ...,
//   "target_game":        "fo4" | ...,
//   "target_version_id":  "hk_2014.1.0-r1",          // null = copy as-is
//   "hkx_assets": [
//     {
//       "source_path":    "Meshes/Actors/Foo/Behavior.hkx",   // relative
//       "resolved_path":  "/abs/path/Behavior.hkx",           // absolute
//       "asset_type":     "behavior" | "animation"
//     },
//     ...
//   ],
//   "nif_assets":         [{ "source_path": "Meshes/Actors/Foo/Foo.nif", "resolved_path": "/abs/Foo.nif" }],
//   "target_behaviors":   ["actors/foo/behaviors/foo.hkx", ...],  // set of paths present in target game
//   "additional_source_asset_roots": ["/extracted/fo3", ...], // searched after primary
//   "asset_prefix":       "fnv",   // accepted for compatibility; output is unprefixed
//   "overwrite_existing": true
// }
//
// Phase output: writes converted .hkx files to mod_path/data/...
// PhaseReport:
//   assets_written    = successfully converted (behavior + animation)
//   warnings          = failed conversions + base-game-skipped (informational)
//   records_dropped   = base-game-skipped count

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

pub struct ConvertHavokPhase;

impl Phase for ConvertHavokPhase {
    fn name(&self) -> &'static str {
        "convert_havok"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        let target_version_id: Option<String> = p
            .get("target_version_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let _asset_prefix = p
            .get("asset_prefix")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let overwrite_existing = p
            .get("overwrite_existing")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Build the set of target-game behavior paths (lower-case, forward-slash)
        // so we can skip assets that already exist in the target game.
        let target_behaviors: HashSet<String> = p
            .get("target_behaviors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.replace('\\', "/").to_lowercase())
                    .collect()
            })
            .unwrap_or_default();

        // Whole-plugin runs set `convert_all`: enumerate every source `.hkx`
        // under `<source_extracted_dir>/Meshes` and dedup against the target
        // base set, instead of taking the dependency-graph asset list. The
        // graph only reaches actor behaviors (via NPC/RACE refs), so it
        // silently drops UniqueBehaviors/GenericBehaviors/weapon behaviors;
        // the graph path is for bounded / cell-slice runs only.
        let convert_all = p
            .get("convert_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut assets = if convert_all {
            let source_roots = convert_all_source_roots(ctx.source_extracted_dir, p);
            let found = enumerate_source_hkx_assets_from_roots(&source_roots);
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "convert_havok",
                level: crate::phase::LogLevel::Info,
                message: format!(
                    "HKX convert-all: enumerated {} source .hkx from {} ordered root(s), primary={}",
                    found.len(),
                    source_roots.len(),
                    ctx.source_extracted_dir.display()
                ),
            });
            found
        } else {
            parse_hkx_assets(p)?
        };
        if is_fo76_to_fo4(p) {
            append_fo76_to_fo4_creature_animation_aliases(&mut assets);
        }
        let nif_assets = parse_nif_assets(p)?;
        let nif_lookup = nif_asset_lookup(&nif_assets);
        let total = assets.len() as u32;
        let mod_path = ctx.mod_path.to_path_buf();
        let classify_setup_only_cloth = is_fo4_havok_target(target_version_id.as_deref());
        let sink = ctx.run.output_sink.clone();
        let data_root = mod_path.join("data");
        let register_with_sink = |dst: &Path| -> bool {
            let Some(s) = &sink else { return true };
            let Ok(rel) = dst.strip_prefix(&data_root) else {
                return true;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            s.add_existing_file(&rel_str, dst).is_ok()
        };

        // Partition: remapped (exists in target game) vs needs conversion
        let mut remapped = 0u32;
        let mut to_convert: Vec<HkxAsset> = Vec::new();

        for asset in assets {
            let norm = asset.source_path.replace('\\', "/").to_lowercase();
            // Strip leading "meshes/" for DB matching
            let db_key = norm.strip_prefix("meshes/").unwrap_or(&norm).to_string();
            if target_behaviors.contains(&db_key) || target_behaviors.contains(&norm) {
                remapped += 1;
            } else {
                to_convert.push(asset);
            }
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_havok",
            level: crate::phase::LogLevel::Info,
            message: format!(
                "HKX phase: {} to convert, {} base-game-remapped",
                to_convert.len(),
                remapped
            ),
        });

        if to_convert.is_empty() {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                phase: "convert_havok",
                current: total,
                total,
                item: None,
            });
            return Ok(PhaseReport {
                assets_written: 0,
                warnings: 0,
                records_dropped: remapped,
                ..Default::default()
            });
        }

        // Build (asset, output_path) tasks, skipping missing sources.
        struct Task {
            asset: HkxAsset,
            output_path: PathBuf,
        }

        let mut tasks: Vec<Task> = Vec::with_capacity(to_convert.len());
        let mut early_warnings: u32 = 0;
        let mut early_item_failures: u32 = 0;
        let mut sink_failures: u32 = 0;

        for asset in to_convert {
            let resolved = Path::new(&asset.resolved_path);
            if !resolved.exists() {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "convert_havok",
                    level: crate::phase::LogLevel::Error,
                    message: format!("HKX not found: {}", asset.source_path),
                });
                early_warnings += 1;
                early_item_failures += 1;
                continue;
            }
            if classify_setup_only_cloth {
                let decision = match catch_unwind(AssertUnwindSafe(|| {
                    setup_only_cloth_decision(&asset, resolved, &nif_lookup)
                })) {
                    Ok(decision) => decision,
                    Err(payload) => Some(SetupOnlyClothDecision::Warn {
                        message: format!(
                            "HKX setup-only cloth warning: {}; companion inspection panicked ({}); converting anyway",
                            asset.source_path,
                            panic_payload_to_string(&*payload)
                        ),
                    }),
                };
                match decision {
                    Some(SetupOnlyClothDecision::Skip { message }) => {
                        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                            phase: "convert_havok",
                            level: crate::phase::LogLevel::Warn,
                            message,
                        });
                        early_warnings += 1;
                        continue;
                    }
                    Some(SetupOnlyClothDecision::Warn { message }) => {
                        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                            phase: "convert_havok",
                            level: crate::phase::LogLevel::Warn,
                            message,
                        });
                        early_warnings += 1;
                    }
                    None => {}
                }
            }
            let out = hkx_output_path(&mod_path, &asset.source_path);
            if !overwrite_existing && out.exists() {
                if !register_with_sink(&out) {
                    sink_failures += 1;
                }
                continue;
            }
            tasks.push(Task {
                asset,
                output_path: out,
            });
        }

        if tasks.is_empty() {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                phase: "convert_havok",
                current: total,
                total,
                item: None,
            });
            return Ok(PhaseReport {
                assets_written: 0,
                warnings: early_warnings,
                records_dropped: remapped,
                items_failed: early_item_failures + sink_failures,
                ..Default::default()
            });
        }

        // Convert in parallel via rayon.
        struct ConvertResult {
            source_path: String,
            success: bool,
            error: Option<String>,
            warnings: Vec<String>,
            sink_failed: bool,
        }

        let tv = target_version_id.clone();
        let reporter = Arc::new(ProgressReporter::new(
            "convert_havok",
            tasks.len() as u32,
            ctx.run.event_tx.clone(),
        ));
        let results: Vec<ConvertResult> = tasks
            .into_par_iter()
            .map(|task| {
                let source_path = task.asset.source_path.clone();
                reporter.set_item(source_path.clone());
                let src = Path::new(&task.asset.resolved_path);
                let outcome =
                    match catch_unwind(AssertUnwindSafe(|| -> Result<Vec<String>, String> {
                        if let Some(parent) = task.output_path.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
                        }
                        if let Some(ref version) = tv {
                            havok_native::api::havok_convert_file_report(
                                src,
                                &task.output_path,
                                version,
                            )
                            .map_err(|e| e.to_string())
                        } else {
                            std::fs::copy(src, &task.output_path)
                                .map(|_| Vec::new())
                                .map_err(|e| format!("copy: {e}"))
                        }
                    })) {
                        Ok(result) => result,
                        Err(payload) => Err(format!(
                            "native HKX converter panicked: {}",
                            panic_payload_to_string(&*payload)
                        )),
                    };
                let (success, error, conversion_warnings) = match outcome {
                    Ok(warnings) => (true, None, warnings),
                    Err(error) => (false, Some(error), Vec::new()),
                };
                let sink_failed = success && !register_with_sink(&task.output_path);

                reporter.inc(1);
                ConvertResult {
                    source_path,
                    success,
                    error,
                    warnings: conversion_warnings,
                    sink_failed,
                }
            })
            .collect();
        reporter.finish();

        let mut assets_written: u32 = 0;
        let mut warnings: u32 = early_warnings;
        let mut items_failed: u32 = early_item_failures + sink_failures;
        for r in &results {
            if r.success {
                assets_written += 1;
                for warning in &r.warnings {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "convert_havok",
                        level: crate::phase::LogLevel::Warn,
                        message: format!("HKX warning: {}: {}", r.source_path, warning),
                    });
                    warnings += 1;
                }
                if r.sink_failed {
                    items_failed += 1;
                }
            } else {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "convert_havok",
                    level: crate::phase::LogLevel::Error,
                    message: format!(
                        "HKX failed: {}: {}",
                        r.source_path,
                        r.error.as_deref().unwrap_or("unknown error")
                    ),
                });
                warnings += 1;
                items_failed += 1;
            }
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
            phase: "convert_havok",
            current: total,
            total,
            item: None,
        });

        Ok(PhaseReport {
            assets_written,
            warnings,
            records_dropped: remapped,
            items_failed,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn panic_payload_to_string(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<unknown payload>".to_string()
    }
}

struct HkxAsset {
    source_path: String,
    resolved_path: String,
    #[allow(dead_code)]
    asset_type: String,
}

struct NifAsset {
    source_path: String,
    resolved_path: String,
}

const FO76_TO_FO4_CREATURE_ANIMATION_ALIASES: &[(&str, &str)] = &[
    (
        "Meshes/Actors/Sheepsquatch/Animations/Ambush.hkx",
        "Meshes/Actors/Sheepsquatch/Animations/Ambush/Ambush_Burrow/Ambush.hkx",
    ),
    (
        "Meshes/Actors/UltraciteAbomination/Animations/Ambush.hkx",
        "Meshes/Actors/UltraciteAbomination/Animations/Ambush/Ambush.hkx",
    ),
];

fn is_fo76_to_fo4(params: &JsonValue) -> bool {
    params
        .get("source_game")
        .and_then(JsonValue::as_str)
        .map(|game| game.eq_ignore_ascii_case("fo76"))
        .unwrap_or(false)
        && params
            .get("target_game")
            .and_then(JsonValue::as_str)
            .map(|game| game.eq_ignore_ascii_case("fo4"))
            .unwrap_or(false)
}

fn append_fo76_to_fo4_creature_animation_aliases(assets: &mut Vec<HkxAsset>) {
    let by_path: HashMap<String, String> = assets
        .iter()
        .map(|asset| {
            (
                asset.source_path.replace('\\', "/").to_ascii_lowercase(),
                asset.resolved_path.clone(),
            )
        })
        .collect();

    for (expected_path, donor_path) in FO76_TO_FO4_CREATURE_ANIMATION_ALIASES {
        let expected_key = expected_path.to_ascii_lowercase();
        if by_path.contains_key(&expected_key) {
            continue;
        }
        let Some(resolved_path) = by_path.get(&donor_path.to_ascii_lowercase()) else {
            continue;
        };
        assets.push(HkxAsset {
            source_path: (*expected_path).to_string(),
            resolved_path: resolved_path.clone(),
            asset_type: "animation".to_string(),
        });
    }
}

enum SetupOnlyClothDecision {
    Skip { message: String },
    Warn { message: String },
}

fn parse_hkx_assets(p: &JsonValue) -> Result<Vec<HkxAsset>, PhaseError> {
    let arr = p
        .get("hkx_assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PhaseError::BadParams("missing hkx_assets array".into()))?;

    arr.iter()
        .enumerate()
        .map(|(i, entry)| {
            let source_path = entry["source_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "hkx_assets[{i}].source_path missing or not a string"
                    ))
                })?
                .to_string();
            let resolved_path = entry["resolved_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "hkx_assets[{i}].resolved_path missing or not a string"
                    ))
                })?
                .to_string();
            let asset_type = entry
                .get("asset_type")
                .and_then(|v| v.as_str())
                .unwrap_or("behavior")
                .to_string();
            Ok(HkxAsset {
                source_path,
                resolved_path,
                asset_type,
            })
        })
        .collect()
}

/// Enumerate every `.hkx` under `<source_extracted_dir>/Meshes` for whole-plugin
/// convert-all runs. The caller dedupes the result against the target-game
/// behavior set, so this must NOT pre-filter by role/category — returning every
/// behavior/animation/skeleton/character is the whole point (the dependency
/// graph only reaches actor behaviors, which is the bug this path avoids).
fn enumerate_source_hkx_assets(source_extracted_dir: &Path) -> Vec<HkxAsset> {
    let Some(meshes_dir) = resolve_meshes_dir(source_extracted_dir) else {
        return Vec::new();
    };
    let mut assets = Vec::new();
    collect_hkx_recursive(&meshes_dir, &meshes_dir, &mut assets);
    assets.sort_by(|left, right| {
        left.source_path
            .to_ascii_lowercase()
            .cmp(&right.source_path.to_ascii_lowercase())
    });
    assets
}

fn convert_all_source_roots(primary: &Path, params: &JsonValue) -> Vec<PathBuf> {
    let candidates = std::iter::once(primary.to_path_buf()).chain(
        params
            .get("additional_source_asset_roots")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
            .filter_map(JsonValue::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from),
    );
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for root in candidates {
        let key = root
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase();
        if seen.insert(key) {
            roots.push(root);
        }
    }
    roots
}

fn enumerate_source_hkx_assets_from_roots(source_roots: &[PathBuf]) -> Vec<HkxAsset> {
    let mut assets = Vec::new();
    let mut seen = HashSet::new();
    for root in source_roots {
        for asset in enumerate_source_hkx_assets(root) {
            let key = asset.source_path.replace('\\', "/").to_ascii_lowercase();
            if seen.insert(key) {
                assets.push(asset);
            }
        }
    }
    assets
}

/// Resolve the `Meshes` directory case-insensitively (NTFS is case-insensitive,
/// but be explicit so a lower-cased extracted tree still walks).
fn resolve_meshes_dir(source_extracted_dir: &Path) -> Option<PathBuf> {
    for name in ["Meshes", "meshes"] {
        let candidate = source_extracted_dir.join(name);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn collect_hkx_recursive(meshes_root: &Path, current: &Path, out: &mut Vec<HkxAsset>) {
    let Ok(read_dir) = std::fs::read_dir(current) else {
        return;
    };
    for item in read_dir.flatten() {
        let path = item.path();
        if path.is_dir() {
            collect_hkx_recursive(meshes_root, &path, out);
            continue;
        }
        let is_hkx = path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("hkx"))
            .unwrap_or(false);
        if !is_hkx {
            continue;
        }
        let Ok(rel) = path.strip_prefix(meshes_root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        out.push(HkxAsset {
            source_path: format!("Meshes/{rel_str}"),
            resolved_path: path.to_string_lossy().into_owned(),
            asset_type: "behavior".to_string(),
        });
    }
}

fn parse_nif_assets(p: &JsonValue) -> Result<Vec<NifAsset>, PhaseError> {
    let Some(value) = p.get("nif_assets") else {
        return Ok(Vec::new());
    };
    let arr = value
        .as_array()
        .ok_or_else(|| PhaseError::BadParams("nif_assets must be an array".into()))?;

    arr.iter()
        .enumerate()
        .map(|(i, entry)| {
            let source_path = entry["source_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "nif_assets[{i}].source_path missing or not a string"
                    ))
                })?
                .to_string();
            let resolved_path = entry["resolved_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "nif_assets[{i}].resolved_path missing or not a string"
                    ))
                })?
                .to_string();
            Ok(NifAsset {
                source_path,
                resolved_path,
            })
        })
        .collect()
}

fn nif_asset_lookup(nif_assets: &[NifAsset]) -> HashMap<String, String> {
    nif_assets
        .iter()
        .map(|asset| {
            (
                mesh_relative_asset_path(&asset.source_path).to_lowercase(),
                asset.resolved_path.clone(),
            )
        })
        .collect()
}

fn setup_only_cloth_decision(
    asset: &HkxAsset,
    resolved_hkx: &Path,
    nif_lookup: &HashMap<String, String>,
) -> Option<SetupOnlyClothDecision> {
    let data = std::fs::read(resolved_hkx).ok()?;
    let summary = havok_native::api::hkx_class_summary(&data).ok()?;
    if !summary.is_setup_only_cloth {
        return None;
    }

    let companion_key = companion_nif_key_for_hkx(&asset.source_path);
    let Some(companion_nif) = nif_lookup.get(&companion_key) else {
        return Some(SetupOnlyClothDecision::Warn {
            message: format!(
                "HKX setup-only cloth warning: {}; no runtime companion NIF in conversion asset graph; converting anyway",
                asset.source_path
            ),
        });
    };

    match nif_has_runtime_cloth(Path::new(companion_nif)) {
        Ok(true) => Some(SetupOnlyClothDecision::Skip {
            message: format!(
                "HKX skipped setup-only cloth: {}; companion NIF contains runtime BSClothExtraData",
                asset.source_path
            ),
        }),
        Ok(false) => Some(SetupOnlyClothDecision::Warn {
            message: format!(
                "HKX setup-only cloth warning: {}; companion NIF has no runtime hclClothData; converting anyway",
                asset.source_path
            ),
        }),
        Err(error) => Some(SetupOnlyClothDecision::Warn {
            message: format!(
                "HKX setup-only cloth warning: {}; could not inspect companion NIF ({error}); converting anyway",
                asset.source_path
            ),
        }),
    }
}

fn nif_has_runtime_cloth(path: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let blobs =
        nif_core_native::cloth::extract_cloth_blobs(&bytes).map_err(|error| error.to_string())?;
    Ok(blobs.iter().any(|blob| {
        havok_native::api::hkx_class_summary(blob)
            .map(|summary| summary.has_cloth_data)
            .unwrap_or(false)
    }))
}

fn is_fo4_havok_target(target_version_id: Option<&str>) -> bool {
    target_version_id.is_some_and(|version| {
        version.eq_ignore_ascii_case("fo4")
            || version == "53"
            || version.eq_ignore_ascii_case("hk_2014.1.0-r1")
    })
}

/// Compute the output path for an HKX inside the mod directory.
///
/// Layout: `mod_path/data/Meshes/<relative_tail>`
///
/// `source_path` may be a Data-relative path (`Meshes/Actors/...`) or the
/// mesh-relative spelling used by behavior records (`Actors/...`).
fn hkx_output_path(mod_path: &Path, source_path: &str) -> PathBuf {
    let mut out = mod_path.to_path_buf();
    out.push("data");
    for component in hkx_data_relative_path(source_path).split('/') {
        if !component.is_empty() {
            out.push(component);
        }
    }
    out
}

fn hkx_data_relative_path(source_path: &str) -> String {
    format!("Meshes/{}", mesh_relative_hkx_path(source_path))
}

fn mesh_relative_hkx_path(source_path: &str) -> String {
    mesh_relative_asset_path(source_path)
}

fn mesh_relative_asset_path(source_path: &str) -> String {
    let mut rel = source_path.replace('\\', "/");
    rel = rel.trim_start_matches('/').to_string();
    if rel.len() >= 5 && rel[..5].eq_ignore_ascii_case("data/") {
        rel = rel[5..].to_string();
    }
    if rel.len() >= 7 && rel[..7].eq_ignore_ascii_case("meshes/") {
        rel = rel[7..].to_string();
    }
    strip_known_asset_prefix(&rel).to_string()
}

fn companion_nif_key_for_hkx(source_path: &str) -> String {
    let rel = mesh_relative_asset_path(source_path);
    if rel.len() >= 4 && rel[rel.len() - 4..].eq_ignore_ascii_case(".hkx") {
        format!("{}{}", &rel[..rel.len() - 4], ".nif").to_lowercase()
    } else {
        format!("{rel}.nif").to_lowercase()
    }
}

fn strip_known_asset_prefix(path: &str) -> &str {
    let Some((first, rest)) = path.split_once('/') else {
        return path;
    };
    if is_known_asset_prefix(first) {
        rest
    } else {
        path
    }
}

fn is_known_asset_prefix(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fo4" | "fo76" | "fnv" | "fo3" | "skyrim" | "skyrimse" | "starfield" | "oblivion"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_hkx(class_name: &str) -> Vec<u8> {
        use havok_native::hkx::descriptors::DescriptorRegistry;
        use havok_native::hkx::{HkxFile, HkxObject, write_hkx};

        let hkx = HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![HkxObject {
                name: Some("#0001".to_string()),
                offset: 0,
                signature: 0,
                class_name: class_name.to_string(),
                members: Vec::new(),
            }],
        );
        let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
        write_hkx(&hkx, &mut registry)
    }

    fn write_runtime_cloth_nif(path: &Path) {
        use nif_core_native::model::NifFile;

        let blob = synthetic_hkx("hclClothData");
        let mut nif = NifFile::new("fo76");
        let nif_bytes = nif.to_bytes().expect("blank NIF serializes");
        let packed = nif_core_native::cloth::pack_cloth_blob(&nif_bytes, &blob)
            .expect("pack runtime cloth blob");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create NIF parent");
        }
        std::fs::write(path, packed).expect("write NIF");
    }

    fn run_convert_havok(
        params: serde_json::Value,
        mod_dir: &Path,
    ) -> (crate::phase::PhaseReport, Vec<crate::phase::PhaseEvent>) {
        run_convert_havok_inner(params, mod_dir, None)
    }

    fn run_convert_havok_with_sink(
        params: serde_json::Value,
        mod_dir: &Path,
        sink: std::sync::Arc<crate::sinks::SinkSet>,
    ) -> (crate::phase::PhaseReport, Vec<crate::phase::PhaseEvent>) {
        run_convert_havok_inner(params, mod_dir, Some(sink))
    }

    fn run_convert_havok_inner(
        params: serde_json::Value,
        mod_dir: &Path,
        sink: Option<std::sync::Arc<crate::sinks::SinkSet>>,
    ) -> (crate::phase::PhaseReport, Vec<crate::phase::PhaseEvent>) {
        use crate::phase::{PhaseCtx, PhaseEvent, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let result = with_run(
            id,
            |run| -> Result<(PhaseReport, Vec<PhaseEvent>), RunError> {
                if let Some(sink) = sink.clone() {
                    run.output_sink = Some(sink);
                }
                let cancel = std::sync::Arc::new(AtomicBool::new(false));
                let source_dir = mod_dir.join("source");
                let mut ctx = PhaseCtx {
                    run,
                    mod_path: mod_dir,
                    source_extracted_dir: &source_dir,
                    target_extracted_dir: None,
                    target_data_dir: None,
                    params: &params,
                    cancel: &cancel,
                };
                let report = ConvertHavokPhase
                    .run(&mut ctx)
                    .map_err(|e| RunError::InvalidConfig(e.to_string()))?;
                let events = run.event_rx.try_iter().collect();
                Ok((report, events))
            },
        )
        .unwrap();
        drop_run(id).unwrap();
        result
    }

    #[test]
    fn convert_all_enumerates_and_converts_non_actor_behavior() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_dir = tmp.path();
        // Harness sets source_extracted_dir = mod_dir/source.
        let src = mod_dir.join("source/Meshes/UniqueBehaviors/BroZookaFX/brozookafx.hkx");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&src, synthetic_hkx("hkbBehaviorGraph")).unwrap();

        let params = serde_json::json!({
            "source_game": "fo76",
            "target_game": "fo4",
            "target_version_id": "hk_2014.1.0-r1",
            "convert_all": true,
            "target_behaviors": [],
        });
        let (report, _events) = run_convert_havok(params, mod_dir);

        assert_eq!(
            report.assets_written, 1,
            "convert_all must convert the enumerated weapon behavior"
        );
        let out = mod_dir.join("data/Meshes/UniqueBehaviors/BroZookaFX/brozookafx.hkx");
        assert!(out.exists(), "converted weapon behavior should be written");
    }

    #[test]
    fn convert_all_emits_same_race_ambush_aliases_for_fo4_character_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_dir = tmp.path();
        let donors = [
            (
                "Actors/Sheepsquatch/Animations/Ambush/Ambush_Burrow/Ambush.hkx",
                "Actors/Sheepsquatch/Animations/Ambush.hkx",
            ),
            (
                "Actors/UltraciteAbomination/Animations/Ambush/Ambush.hkx",
                "Actors/UltraciteAbomination/Animations/Ambush.hkx",
            ),
        ];
        for (donor, _) in donors {
            let source = mod_dir.join("source/Meshes").join(donor);
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::write(source, synthetic_hkx("hkaAnimationContainer")).unwrap();
        }

        let params = serde_json::json!({
            "source_game": "fo76",
            "target_game": "fo4",
            "target_version_id": "hk_2014.1.0-r1",
            "convert_all": true,
            "target_behaviors": [],
        });
        let (report, _events) = run_convert_havok(params, mod_dir);

        assert_eq!(report.assets_written, 4);
        for (_, expected) in donors {
            assert!(
                mod_dir.join("data/Meshes").join(expected).exists(),
                "missing converted alias {expected}"
            );
        }
    }

    #[test]
    fn creature_animation_aliases_require_exact_same_race_donor() {
        let mut assets = vec![HkxAsset {
            source_path: "Meshes/Actors/Other/Animations/Ambush.hkx".to_string(),
            resolved_path: "other.hkx".to_string(),
            asset_type: "animation".to_string(),
        }];

        append_fo76_to_fo4_creature_animation_aliases(&mut assets);

        assert_eq!(assets.len(), 1);
    }

    #[test]
    fn convert_all_dedups_against_target_base_set() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_dir = tmp.path();
        let src = mod_dir.join("source/Meshes/Actors/Foo/Behaviors/foo.hkx");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&src, synthetic_hkx("hkbBehaviorGraph")).unwrap();

        let params = serde_json::json!({
            "source_game": "fo76",
            "target_game": "fo4",
            "target_version_id": "hk_2014.1.0-r1",
            "convert_all": true,
            // present in the FO4 base game -> must be skipped, not converted
            "target_behaviors": ["actors/foo/behaviors/foo.hkx"],
        });
        let (report, _events) = run_convert_havok(params, mod_dir);

        assert_eq!(report.assets_written, 0, "base-game path must not convert");
        assert_eq!(report.records_dropped, 1, "base-game path must be remapped");
    }

    #[test]
    fn convert_all_includes_grafted_root_with_primary_collision_precedence() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_dir = tmp.path();
        let primary = mod_dir.join("source");
        let grafted = mod_dir.join("fo3");
        let collision_rel = "Meshes/Actors/Shared/collision.hkx";
        let grafted_only_rel = "Meshes/Actors/FO3/only.hkx";
        for (root, rel, contents) in [
            (&primary, collision_rel, b"fnv-primary".as_slice()),
            (&grafted, collision_rel, b"fo3-collision".as_slice()),
            (&grafted, grafted_only_rel, b"fo3-only".as_slice()),
        ] {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }

        let params = serde_json::json!({
            "source_game": "fnv",
            "target_game": "fo4",
            "convert_all": true,
            "target_behaviors": [],
            "additional_source_asset_roots": [grafted],
        });
        let (report, _events) = run_convert_havok(params, mod_dir);

        assert_eq!(report.assets_written, 2);
        assert_eq!(
            std::fs::read(mod_dir.join("data").join(collision_rel)).unwrap(),
            b"fnv-primary"
        );
        assert_eq!(
            std::fs::read(mod_dir.join("data").join(grafted_only_rel)).unwrap(),
            b"fo3-only"
        );
    }

    #[test]
    fn enumerate_source_hkx_walks_every_subtree_not_just_actors() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let meshes = root.join("Meshes");
        for rel in [
            "UniqueBehaviors/BroZookaFX/brozookafx.hkx",
            "Actors/Bar/Behaviors/bar.hkx",
            "GenericBehaviors/baz.hkx",
            "Weapons/Foo/Foo.nif", // non-hkx — excluded
            "readme.txt",          // non-hkx — excluded
        ] {
            let p = meshes.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }

        let mut paths: Vec<String> = enumerate_source_hkx_assets(root)
            .into_iter()
            .map(|a| a.source_path)
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            vec![
                "Meshes/Actors/Bar/Behaviors/bar.hkx".to_string(),
                "Meshes/GenericBehaviors/baz.hkx".to_string(),
                "Meshes/UniqueBehaviors/BroZookaFX/brozookafx.hkx".to_string(),
            ],
            "convert-all must enumerate UniqueBehaviors/GenericBehaviors too, not only Actors/"
        );
    }

    #[test]
    fn hkx_output_path_strips_source_prefix_component() {
        let base = Path::new("/mod");
        let result = hkx_output_path(base, "Meshes/fnv/Actors/Foo/Behavior.hkx");
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Actors/Foo/Behavior.hkx")
        );
    }

    #[test]
    fn hkx_output_path_unprefixed() {
        let base = Path::new("/mod");
        let result = hkx_output_path(base, "Meshes/Actors/Foo/Behavior.hkx");
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Actors/Foo/Behavior.hkx")
        );
    }

    #[test]
    fn hkx_output_path_adds_meshes_root_for_actor_relative_path() {
        let base = Path::new("/mod");
        let result = hkx_output_path(base, "Actors/GraftonMonster/CharacterAssets/skeleton.hkx");
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Actors/GraftonMonster/CharacterAssets/skeleton.hkx")
        );
    }

    #[test]
    fn attached_sink_registers_hkx_loose_output_when_disabled() {
        use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};

        let temp = tempfile::tempdir().unwrap();
        let src_hkx = temp
            .path()
            .join("source")
            .join("Meshes/actors/alien/animations/attack1.hkx");
        std::fs::create_dir_all(src_hkx.parent().unwrap()).unwrap();
        std::fs::write(&src_hkx, b"hkx passthrough bytes").unwrap();

        let mod_dir = temp.path().join("mod");
        let sink = std::sync::Arc::new(SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: mod_dir.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        });
        let params = serde_json::json!({
            "source_game": "fo76",
            "target_game": "fo4",
            "target_version_id": "",
            "hkx_assets": [{
                "source_path": "Meshes/actors/alien/animations/attack1.hkx",
                "resolved_path": src_hkx.to_string_lossy(),
                "asset_type": "animation"
            }],
            "nif_assets": [],
            "target_behaviors": [],
            "asset_prefix": ""
        });

        let (report, _events) = run_convert_havok_with_sink(params, &mod_dir, sink.clone());

        assert_eq!(report.assets_written, 1);
        assert_eq!(report.warnings, 0);
        assert_eq!(report.items_failed, 0);
        assert!(
            mod_dir
                .join("data/Meshes/actors/alien/animations/attack1.hkx")
                .exists()
        );
        assert_eq!(
            sink.ba2.as_ref().unwrap().streamed_rel_paths(),
            vec!["meshes/actors/alien/animations/attack1.hkx"]
        );
    }

    #[test]
    fn setup_only_cloth_hkx_with_runtime_companion_nif_is_skipped() {
        use crate::phase::{LogLevel, PhaseEvent};

        let temp = tempfile::tempdir().unwrap();
        let src_hkx = temp
            .path()
            .join("source")
            .join("Actors/Mirelurk/seaweed.hkx");
        let src_nif = temp
            .path()
            .join("source")
            .join("Actors/Mirelurk/seaweed.nif");
        std::fs::create_dir_all(src_hkx.parent().unwrap()).unwrap();
        std::fs::write(&src_hkx, synthetic_hkx("hclClothSetupContainer")).unwrap();
        write_runtime_cloth_nif(&src_nif);
        assert!(nif_has_runtime_cloth(&src_nif).expect("inspect runtime cloth NIF"));

        let mod_dir = temp.path().join("mod");
        let params = serde_json::json!({
            "source_game": "fo76",
            "target_game": "fo4",
            "target_version_id": "hk_2014.1.0-r1",
            "hkx_assets": [{
                "source_path": "Actors/Mirelurk/seaweed.hkx",
                "resolved_path": src_hkx.to_string_lossy(),
                "asset_type": "behavior"
            }],
            "nif_assets": [{
                "source_path": "Actors/Mirelurk/seaweed.nif",
                "resolved_path": src_nif.to_string_lossy()
            }],
            "target_behaviors": [],
            "asset_prefix": ""
        });

        let (report, events) = run_convert_havok(params, &mod_dir);

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);
        assert!(
            !mod_dir
                .join("data/Meshes/Actors/Mirelurk/seaweed.hkx")
                .exists()
        );
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log {
                phase: "convert_havok",
                level: LogLevel::Warn,
                message,
            } if message.contains("HKX skipped setup-only cloth: Actors/Mirelurk/seaweed.hkx")
                && message.contains("companion NIF contains runtime BSClothExtraData")
        )));
    }

    #[test]
    fn setup_only_cloth_hkx_without_runtime_companion_warns_and_converts() {
        use crate::phase::{LogLevel, PhaseEvent};

        let temp = tempfile::tempdir().unwrap();
        let src_hkx = temp
            .path()
            .join("source")
            .join("Actors/Mirelurk/seaweed.hkx");
        std::fs::create_dir_all(src_hkx.parent().unwrap()).unwrap();
        std::fs::write(&src_hkx, synthetic_hkx("hclClothSetupContainer")).unwrap();

        let mod_dir = temp.path().join("mod");
        let params = serde_json::json!({
            "source_game": "fo76",
            "target_game": "fo4",
            "target_version_id": "hk_2014.1.0-r1",
            "hkx_assets": [{
                "source_path": "Actors/Mirelurk/seaweed.hkx",
                "resolved_path": src_hkx.to_string_lossy(),
                "asset_type": "behavior"
            }],
            "nif_assets": [],
            "target_behaviors": [],
            "asset_prefix": ""
        });

        let (report, events) = run_convert_havok(params, &mod_dir);

        assert_eq!(report.assets_written, 1);
        assert_eq!(report.warnings, 1);
        assert!(
            mod_dir
                .join("data/Meshes/Actors/Mirelurk/seaweed.hkx")
                .exists()
        );
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log {
                phase: "convert_havok",
                level: LogLevel::Warn,
                message,
            } if message.contains("HKX setup-only cloth warning: Actors/Mirelurk/seaweed.hkx")
                && message.contains("no runtime companion NIF in conversion asset graph")
        )));
    }

    #[test]
    fn empty_hkx_assets_succeeds() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "source_game": "fo76",
                "target_game": "fo4",
                "target_version_id": "hk_2014.1.0-r1",
                "hkx_assets": [],
                "target_behaviors": [],
                "asset_prefix": ""
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertHavokPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 0);
        drop_run(id).unwrap();
    }

    #[test]
    fn missing_resolved_path_counts_as_warning() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "source_game": "fo76",
                "target_game": "fo4",
                "target_version_id": "hk_2014.1.0-r1",
                "hkx_assets": [
                    {
                        "source_path": "Meshes/Actors/Foo/Behavior.hkx",
                        "resolved_path": "/nonexistent/Behavior.hkx",
                        "asset_type": "behavior"
                    }
                ],
                "target_behaviors": [],
                "asset_prefix": ""
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertHavokPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);
        drop_run(id).unwrap();
    }

    #[test]
    fn invalid_hkx_counts_and_logs_item_failure() {
        use crate::phase::{LogLevel, PhaseEvent};

        let temp = tempfile::tempdir().unwrap();
        let src_hkx = temp.path().join("source").join("Meshes/Actors/Foo/bad.hkx");
        std::fs::create_dir_all(src_hkx.parent().unwrap()).unwrap();
        std::fs::write(&src_hkx, b"not hkx").unwrap();

        let mod_dir = temp.path().join("mod");
        let params = serde_json::json!({
            "source_game": "fo76",
            "target_game": "fo4",
            "target_version_id": "hk_2014.1.0-r1",
            "hkx_assets": [{
                "source_path": "Meshes/Actors/Foo/bad.hkx",
                "resolved_path": src_hkx.to_string_lossy(),
                "asset_type": "behavior"
            }],
            "nif_assets": [],
            "target_behaviors": [],
            "asset_prefix": ""
        });

        let (report, events) = run_convert_havok(params, &mod_dir);

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);
        assert_eq!(report.items_failed, 1);
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log {
                phase: "convert_havok",
                level: LogLevel::Error,
                message,
            } if message.contains("HKX failed: Meshes/Actors/Foo/bad.hkx")
        )));
    }

    #[test]
    fn panic_payload_to_string_extracts_message() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| panic!("bad hkx")));
        let payload = result.unwrap_err();
        assert_eq!(panic_payload_to_string(&*payload), "bad hkx");
    }

    #[test]
    fn base_game_assets_are_remapped_not_converted() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "source_game": "fo76",
                "target_game": "fo4",
                "target_version_id": "hk_2014.1.0-r1",
                "hkx_assets": [
                    {
                        "source_path": "Meshes/Actors/Character/Character.hkx",
                        "resolved_path": "/nonexistent/Character.hkx",
                        "asset_type": "behavior"
                    }
                ],
                // The asset's path (stripped of "meshes/") is in the target game
                "target_behaviors": ["actors/character/character.hkx"],
                "asset_prefix": ""
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertHavokPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        // The asset is in target behaviors → remapped, not converted, not warned.
        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 0);
        assert_eq!(report.records_dropped, 1); // remapped count
        drop_run(id).unwrap();
    }
}
