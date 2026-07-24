// Phase: convert_nifs
//
// Params shape (JSON):
// {
//   "source_game":    "fnv" | "fo3" | "fo76" | "fo4" | ...,
//   "target_game":    "fo4" | ...,
//   "asset_prefix":   "fnv"  // accepted for compatibility; output is unprefixed
//   "nif_paths": [
//     {
//       "source_path":   "Meshes/Weapons/Gun.nif",   // relative game path
//       "resolved_path": "/abs/path/to/Gun.nif",      // absolute disk path
//       "weapon_role":   "gun" | "melee" | null       // optional
//     },
//     ...
//   ],
//   "addon_index_map": { "20000": 20001 },
//   "bgsm_output_dir": "/abs/mod/data/Materials",
//   // Skin-conversion options (all optional):
//   "translation_maps_dir":       "/abs/path",
//   "auto_skin_reference_body":   "/abs/path/malebody.nif",
//   "emit_first_person":          false,
//   "first_person_reference":     "/abs/path/1stpersonmalebody.nif",
//   "morph_weight_cap":           0.5
// }
//
// Phase output: writes NIF files under mod_path/data/Meshes/...
// PhaseReport.assets_written = count of successfully converted NIFs.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use std::sync::{Arc, Mutex};
use std::time::Instant;

use rayon::prelude::*;
use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use nif_core_native::convert_file::{ConvertFileOptions, ConvertFileReport, convert_nif_file};
use nif_core_native::model::{NifFile, ReferencedAssetPaths};
use nif_core_native::skeleton_repose::{
    BindMatrix, collect_bind_matrices_by_name, repose_skeleton_to_inverse_bind,
};

pub struct ConvertNifsV2Phase;

impl Phase for ConvertNifsV2Phase {
    fn name(&self) -> &'static str {
        "convert_nifs_v2"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        run_convert_nifs(ctx, "convert_nifs_v2")
    }
}

fn run_convert_nifs(
    ctx: &mut PhaseCtx<'_>,
    phase_name: &'static str,
) -> Result<PhaseReport, PhaseError> {
    let p = ctx.params;

    let source_game = p["source_game"]
        .as_str()
        .ok_or_else(|| PhaseError::BadParams("missing source_game".into()))?
        .to_string();
    let target_game = p["target_game"]
        .as_str()
        .ok_or_else(|| PhaseError::BadParams("missing target_game".into()))?
        .to_string();
    let _asset_prefix = p
        .get("asset_prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let conversion_workers = parse_conversion_workers(p, ctx.run.config.conversion_workers);
    let worker_label = conversion_workers
        .map(|workers| workers.to_string())
        .unwrap_or_else(|| "rayon-default".to_string());

    let mut nif_entries: Vec<NifEntry> = parse_nif_entries(p)?;
    let relocation_namespace = crate::run::base_asset_namespace_for_run(ctx.run);
    // Relocation members are FO76-side paths collected from the source extracted
    // dir, so resolve them against that dir — NOT the phase's `source_extracted_dir`
    // param, which the Python NIF driver passes as the FO4 *target* dir. Resolving
    // against the target relocated the FO4 base mesh into the FO76 namespace instead
    // of the converted FO76 mesh (the "purple tree" bug). The materials/textures
    // phases already resolve from the FO76 source; this aligns the NIF phase.
    let relocation_source_dir: &Path = ctx
        .run
        .config
        .source_extracted_dir
        .as_deref()
        .unwrap_or(ctx.source_extracted_dir);
    let relocation_enabled =
        !ctx.run.relocation_members.is_empty() && !relocation_namespace.trim().is_empty();
    let material_source_overrides = crate::material_source_overrides::material_source_overrides();
    let relocation_start_len = nif_entries.len();
    let relocation_started = Instant::now();
    if relocation_enabled {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: format!(
                "{phase_name}: relocation setup started entries={} members={} workers={worker_label}",
                nif_entries.len(),
                ctx.run.relocation_members.len()
            ),
        });
    }
    apply_nif_relocation(
        &mut nif_entries,
        &ctx.run.relocation_members,
        &relocation_namespace,
        relocation_source_dir,
    );
    if relocation_enabled {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: format!(
                "{phase_name}: relocation setup completed entries={} appended={} elapsed_ms={}",
                nif_entries.len(),
                nif_entries.len().saturating_sub(relocation_start_len),
                relocation_started.elapsed().as_millis()
            ),
        });
    }
    // FO76 source data root for material-aware shader normalization (Glow_Map
    // flag derivation). Same source dir the relocation resolves against.
    let nif_source_material_dir = relocation_source_dir.to_path_buf();
    let addon_index_map: HashMap<i64, i64> = parse_addon_index_map(p)?;
    let bgsm_output_dir: Option<PathBuf> = p
        .get("bgsm_output_dir")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    // Skin options (all optional)
    let translation_maps_dir: Option<PathBuf> = p
        .get("translation_maps_dir")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let auto_skin_reference_body: Option<PathBuf> = p
        .get("auto_skin_reference_body")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let emit_first_person: bool = p
        .get("emit_first_person")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let first_person_reference: Option<PathBuf> = p
        .get("first_person_reference")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let morph_weight_cap: f32 = p
        .get("morph_weight_cap")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(0.5);
    let skip_existing: bool = p
        .get("skip_existing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mod_path = ctx.mod_path;
    // When a sink is attached (unified driver runs only — legacy runs
    // never attach one), register the loose artifact with the BA2 spill
    // after each successful convert / skip-existing reuse.
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
    let total = nif_entries.len() as u32;
    let (reserved_entries, destination_ownership, destination_collisions) =
        reserve_nif_entry_destinations(
            mod_path,
            nif_entries,
            &source_game,
            &target_game,
            emit_first_person,
            bgsm_output_dir.as_deref(),
        );
    if destination_collisions > 0 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Warn,
            message: format!(
                "{phase_name}: omitted {destination_collisions} NIF entry/entries with case-insensitive artifact destination collisions"
            ),
        });
    }
    let mut skipped_existing: u32 = 0;
    let mut sink_failures: u32 = 0;
    let mut foreign_existing_artifacts: u32 = 0;
    let mut unowned_stale_artifacts: u32 = 0;
    let mut work_entries = Vec::with_capacity(reserved_entries.len());
    for reserved in reserved_entries {
        let dst = nif_output_path(mod_path, &reserved.entry);
        if skip_existing && dst.is_file() {
            skipped_existing += 1;
            match existing_nif_artifacts(
                &dst,
                reserved.artifacts.cleanup_first_person,
                reserved
                    .artifacts
                    .material_relative
                    .as_deref()
                    .zip(bgsm_output_dir.as_deref()),
            ) {
                Ok(artifacts) => {
                    if sink.is_some() {
                        let registration = register_owned_existing_artifacts(
                            artifacts,
                            &reserved.artifacts,
                            &destination_ownership,
                            &register_with_sink,
                        );
                        sink_failures += registration.sink_failures;
                        foreign_existing_artifacts += registration.foreign_skipped;
                        unowned_stale_artifacts += registration.unowned_stale_skipped;
                    }
                }
                Err(message) => {
                    sink_failures += 1;
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: phase_name,
                        level: LogLevel::Warn,
                        message: format!(
                            "{phase_name}: skip-existing sidecar discovery: {message}"
                        ),
                    });
                }
            }
        } else {
            work_entries.push(reserved);
        }
    }
    let work_count = work_entries.len();
    if skipped_existing > 0 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: format!("{phase_name}: skipped {skipped_existing} existing NIF(s)"),
        });
    }
    if foreign_existing_artifacts > 0 || unowned_stale_artifacts > 0 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: format!(
                "{phase_name}: skip-existing omitted {foreign_existing_artifacts} foreign-owned and {unowned_stale_artifacts} unowned stale sidecar artifact(s) from the output sink"
            ),
        });
    }
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: format!(
                "{phase_name}: {work_count} to convert, {skipped_existing} existing-output skipped, workers={worker_label}"
            ),
        });

    // Build per-entry results in parallel via rayon.
    let reporter = Arc::new(ProgressReporter::new(
        phase_name,
        work_count as u32,
        ctx.run.event_tx.clone(),
    ));
    let cancel = ctx.cancel;
    let publication_lock = Arc::new(Mutex::new(()));
    let destination_ownership = Arc::new(destination_ownership);
    // Fold directly into NifAgg to avoid retaining all NifResult objects.
    let convert_work = || {
        work_entries
            .into_par_iter()
            .fold(NifAgg::default, |mut agg, reserved| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    agg.cancelled = true;
                    return agg;
                }
                let entry = &reserved.entry;
                let source_path = entry.source_path.clone();
                reporter.set_item(source_path.clone());
                let options = ConvertFileOptions {
                    asset_prefix: entry.asset_namespace.clone(),
                    material_namespace: entry.material_namespace.clone(),
                    asset_namespace_paths: entry.asset_namespace_paths.iter().cloned().collect(),
                    material_namespace_paths: entry
                        .material_namespace_paths
                        .iter()
                        .cloned()
                        .collect(),
                    addon_index_map: addon_index_map.clone(),
                    translation_maps_dir: translation_maps_dir.clone(),
                    auto_skin_reference_body: auto_skin_reference_body.clone(),
                    emit_first_person,
                    first_person_reference: first_person_reference.clone(),
                    morph_weight_cap,
                    weapon_role: entry.weapon_role.clone(),
                    source_material_dir: Some(nif_source_material_dir.clone()),
                    material_source_overrides: material_source_overrides.clone(),
                };

                let dst = nif_output_path(mod_path, &entry);
                let result = match staged_destination_or_cleanup(
                    &dst,
                    bgsm_output_dir.as_deref(),
                    &reserved.artifacts,
                    &destination_ownership,
                    &publication_lock,
                    StagedNifDestination::new,
                ) {
                    Ok(mut staged) => {
                        let stage_skyrim_materials =
                            source_game == "skyrimse" && bgsm_output_dir.is_some();
                        let conversion_bgsm_output_dir = if stage_skyrim_materials {
                            Some(staged.materials_root.as_path())
                        } else {
                            bgsm_output_dir.as_deref()
                        };
                        let raw_result = match catch_unwind(AssertUnwindSafe(
                            || -> Result<ConvertFileReport, String> {
                                let src = Path::new(&entry.resolved_path);
                                if !src.exists() {
                                    return Err(format!("NIF not found: {}", entry.resolved_path));
                                }
                                convert_nif_file(
                                    src,
                                    &staged.path,
                                    &source_game,
                                    &target_game,
                                    conversion_bgsm_output_dir,
                                    &options,
                                )
                                .map_err(|e| format!("{source_path}: {e}"))
                            },
                        )) {
                            Ok(result) => result,
                            Err(payload) => Err(format!(
                                "native NIF converter panicked: {}",
                                panic_payload_to_string(&*payload)
                            )),
                        };
                        publish_staged_result(
                            classify_convert_result(raw_result, staged.path.is_file()),
                            &mut staged,
                            &dst,
                            if stage_skyrim_materials {
                                bgsm_output_dir.as_deref()
                            } else {
                                None
                            },
                            &reserved.artifacts,
                            &destination_ownership,
                            &publication_lock,
                        )
                    }
                    Err(message) => NifConversionResult::Failed {
                        message,
                        report: None,
                    },
                };

                if let NifConversionResult::Written(report) = &result {
                    if !register_with_sink(&dst) {
                        agg.sink_failures += 1;
                    }
                    for emitted_material in &report.emitted_bgsms {
                        if !register_with_sink(Path::new(emitted_material)) {
                            agg.sink_failures += 1;
                        }
                    }
                    if let Some(emitted_first_person) = &report.emitted_first_person
                        && !register_with_sink(Path::new(emitted_first_person))
                    {
                        agg.sink_failures += 1;
                    }
                }
                reporter.inc(1);
                agg.add(source_path, result);
                agg
            })
            .reduce(NifAgg::default, NifAgg::merge)
    };
    let agg: NifAgg = if let Some(workers) = conversion_workers {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|err| PhaseError::Internal(format!("rayon pool error: {err}")))?;
        pool.install(convert_work)
    } else {
        convert_work()
    };
    reporter.finish();

    // Repose creature skeleton.nif files into the FO4 inverse-bind rest pose.
    // FO76 skeletons keep raw FO76 bone locals; for an undeformed skin each
    // skinned bone must satisfy `world @ bind == I`. Gated + name-matched, so
    // already-correct skeletons and non-skinned helper nodes are untouched.
    let repose = repose_creature_skeletons(&data_root);
    if repose.files_reposed > 0 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: format!(
                "{phase_name}: reposed {}/{} creature skeleton(s) ({} bones) to FO4 inverse-bind rest pose",
                repose.files_reposed, repose.files_scanned, repose.bones_reposed
            ),
        });
    }

    let ssf = mirror_all_source_ssf_files(relocation_source_dir, &data_root, |path| {
        register_with_sink(path)
    });
    if ssf.files_found > 0 || ssf.failures > 0 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: format!(
                "{phase_name}: SSF sidecars found={} copied={} reused={} failed={} sink_failed={}",
                ssf.files_found,
                ssf.files_copied,
                ssf.files_reused,
                ssf.failures,
                ssf.sink_failures,
            ),
        });
    }
    for message in &ssf.warning_messages {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Warn,
            message: message.clone(),
        });
    }

    let assets_written: u32 = skipped_existing + agg.assets_written + ssf.files_copied;
    let warnings: u32 = agg.warnings + ssf.failures + ssf.sink_failures;
    for msg in agg.error_messages {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Error,
            message: msg,
        });
    }
    if agg.report_warnings > 0 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Warn,
            message: format!(
                "{phase_name}: emitted {} NIF report warning(s)",
                agg.report_warnings
            ),
        });
    }
    // Summary telemetry is emitted BEFORE the
    // per-file warning flood: the event channel is bounded and try_send
    // drops on overflow, so a multi-thousand-warning burst at phase end
    // must cost warning detail, never the phase summary lines.
    for message in agg.timing_summary.log_messages() {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message,
        });
    }
    for msg in agg.warning_messages {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Warn,
            message: msg,
        });
    }
    if agg.cancelled {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Warn,
            message: format!("{phase_name}: cancelled"),
        });
    }

    let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
        phase: phase_name,
        current: total,
        total,
        item: None,
    });

    Ok(PhaseReport {
        assets_written,
        warnings,
        items_failed: agg.warnings
            + agg.sink_failures
            + sink_failures
            + ssf.failures
            + ssf.sink_failures,
        ..Default::default()
    })
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

struct NifEntry {
    source_path: String,
    resolved_path: String,
    output_subpath: Option<String>,
    asset_namespace: Option<String>,
    material_namespace: Option<String>,
    asset_namespace_paths: Vec<String>,
    material_namespace_paths: Vec<String>,
    weapon_role: Option<String>,
}

struct ReservedNifEntry {
    entry: NifEntry,
    artifacts: EntryArtifactContext,
}

struct EntryArtifactContext {
    owner: usize,
    material_relative: Option<PathBuf>,
    cleanup_first_person: bool,
}

#[derive(Default)]
struct DestinationOwnership {
    owners: HashMap<String, usize>,
}

impl DestinationOwnership {
    fn owner(&self, path: &Path) -> Option<usize> {
        self.owners.get(&windows_destination_key(path)).copied()
    }

    fn is_owned_by(&self, owner: usize, path: &Path) -> bool {
        self.owner(path) == Some(owner)
    }

    fn can_remove(&self, owner: usize, path: &Path) -> bool {
        self.owner(path)
            .is_none_or(|reserved_owner| reserved_owner == owner)
    }
}

fn reserve_nif_entry_destinations(
    mod_path: &Path,
    entries: Vec<NifEntry>,
    source_game: &str,
    target_game: &str,
    emit_first_person: bool,
    final_material_root: Option<&Path>,
) -> (Vec<ReservedNifEntry>, DestinationOwnership, usize) {
    let cleanup_first_person = emit_first_person
        && target_game.eq_ignore_ascii_case("fo4")
        && !source_game.eq_ignore_ascii_case("skyrimse");
    let reserve_skyrim_materials =
        source_game.eq_ignore_ascii_case("skyrimse") && final_material_root.is_some();
    let mut ownership = DestinationOwnership::default();
    let mut kept = Vec::with_capacity(entries.len());
    let mut collisions = 0usize;
    for (owner, entry) in entries.into_iter().enumerate() {
        let destination = nif_output_path(mod_path, &entry);
        let material_relative = reserve_skyrim_materials
            .then(|| skyrim_material_relative_path(Path::new(&entry.resolved_path)));
        let mut destinations = vec![destination.clone()];
        if cleanup_first_person {
            destinations.push(first_person_destination_path(&destination));
        }
        if let (Some(relative), Some(final_root)) =
            (material_relative.as_deref(), final_material_root)
        {
            destinations.extend(possible_skyrim_material_destinations(
                Path::new(&entry.resolved_path),
                relative,
                final_root,
            ));
        }
        let mut entry_keys = HashSet::new();
        destinations.retain(|path| entry_keys.insert(windows_destination_key(path)));
        if destinations
            .iter()
            .any(|path| ownership.owner(path).is_some())
        {
            collisions += 1;
            continue;
        }
        for path in destinations {
            ownership
                .owners
                .insert(windows_destination_key(&path), owner);
        }
        kept.push(ReservedNifEntry {
            entry,
            artifacts: EntryArtifactContext {
                owner,
                material_relative,
                cleanup_first_person,
            },
        });
    }
    (kept, ownership, collisions)
}

fn windows_destination_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn first_person_destination_path(destination: &Path) -> PathBuf {
    let stem = destination
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("converted");
    destination.with_file_name(format!("{stem}_1stPerson.nif"))
}

fn skyrim_material_relative_path(source_path: &Path) -> PathBuf {
    let components = source_path.components().collect::<Vec<_>>();
    let meshes_index = components.iter().rposition(|component| match component {
        std::path::Component::Normal(value) => {
            value.to_string_lossy().eq_ignore_ascii_case("meshes")
        }
        _ => false,
    });
    meshes_index
        .map(|index| components[index + 1..].iter().collect::<PathBuf>())
        .unwrap_or_else(|| {
            source_path
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("skyrim_static.nif"))
        })
}

fn possible_skyrim_material_destinations(
    resolved_source: &Path,
    material_relative: &Path,
    final_material_root: &Path,
) -> Vec<PathBuf> {
    let Ok(nif) = NifFile::load(resolved_source) else {
        return Vec::new();
    };
    let parent = material_relative.parent().unwrap_or_else(|| Path::new(""));
    let stem = material_relative
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("skyrim_static");
    nif.blocks
        .iter()
        .filter_map(|block| {
            let extension = match block.type_name.as_str() {
                "BSLightingShaderProperty" => "bgsm",
                "BSEffectShaderProperty" => "bgem",
                _ => return None,
            };
            Some(
                final_material_root
                    .join(parent)
                    .join(format!("{stem}_{}.{extension}", block.block_id)),
            )
        })
        .collect()
}

fn existing_nif_artifacts(
    destination: &Path,
    include_first_person: bool,
    material_scope: Option<(&Path, &Path)>,
) -> Result<Vec<PathBuf>, String> {
    let mut artifacts = vec![destination.to_path_buf()];
    if include_first_person {
        let first_person = first_person_destination_path(destination);
        if first_person.is_file() {
            artifacts.push(first_person);
        }
    }
    if let Some((material_relative, final_root)) = material_scope {
        artifacts.extend(discover_material_sidecars(material_relative, final_root)?);
    }
    Ok(artifacts)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ExistingArtifactRegistration {
    sink_failures: u32,
    foreign_skipped: u32,
    unowned_stale_skipped: u32,
}

fn register_owned_existing_artifacts(
    artifacts: impl IntoIterator<Item = PathBuf>,
    context: &EntryArtifactContext,
    ownership: &DestinationOwnership,
    register: impl Fn(&Path) -> bool,
) -> ExistingArtifactRegistration {
    let mut outcome = ExistingArtifactRegistration::default();
    for artifact in artifacts {
        match ownership.owner(&artifact) {
            Some(owner) if owner == context.owner => {
                if !register(&artifact) {
                    outcome.sink_failures += 1;
                }
            }
            Some(_) => outcome.foreign_skipped += 1,
            // Numeric sidecars absent from preflight reservations are stale and
            // must not become first-wins BA2 entries.
            None => outcome.unowned_stale_skipped += 1,
        }
    }
    outcome
}

fn discover_material_sidecars(
    material_relative: &Path,
    final_material_root: &Path,
) -> Result<Vec<PathBuf>, String> {
    let Some(stem) = material_relative
        .file_stem()
        .and_then(|value| value.to_str())
    else {
        return Ok(Vec::new());
    };
    let material_parent =
        final_material_root.join(material_relative.parent().unwrap_or_else(|| Path::new("")));
    if !material_parent.exists() {
        return Ok(Vec::new());
    }
    let prefix = format!("{}_", stem.to_lowercase());
    let entries = std::fs::read_dir(&material_parent).map_err(|error| {
        format!(
            "read material sidecars for {}: {error}",
            material_relative.display()
        )
    })?;
    let mut sidecars = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "read material sidecar entry for {}: {error}",
                material_relative.display()
            )
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !extension.eq_ignore_ascii_case("bgsm") && !extension.eq_ignore_ascii_case("bgem") {
            continue;
        }
        let file_stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_lowercase();
        let Some(shader_id) = file_stem.strip_prefix(&prefix) else {
            continue;
        };
        if !shader_id.is_empty() && shader_id.bytes().all(|byte| byte.is_ascii_digit()) {
            sidecars.push(path);
        }
    }
    sidecars.sort_by_key(|path| windows_destination_key(path));
    Ok(sidecars)
}

/// Apply collision-relocation to the NIF work-list. Member entries are forced to
/// output under `<namespace>/` and get `asset_namespace`/`material_namespace` set
/// so `nif_core` rewrites their internal texture/material slots; mesh members the
/// dependency graph omitted are appended. Only paths in `members` are touched.
fn apply_nif_relocation(
    entries: &mut Vec<NifEntry>,
    members: &std::collections::HashSet<String>,
    namespace: &str,
    source_dir: &Path,
) {
    if members.is_empty() || namespace.trim().is_empty() {
        return;
    }
    for entry in entries.iter_mut() {
        let key = crate::relocation::member_key_for_source_path(&entry.source_path, source_dir);
        if members.contains(&key) {
            entry.asset_namespace = Some(namespace.to_string());
            entry.material_namespace = Some(namespace.to_string());
            entry.output_subpath = Some(crate::relocation::insert_namespace_after_root(
                &key, namespace,
            ));
        }
    }
    entries.par_iter_mut().for_each(|entry| {
        if entry.material_namespace.is_some() && entry.asset_namespace.is_some() {
            return;
        }
        let Some(refs) = referenced_paths_for_entry(entry, source_dir) else {
            return;
        };
        let relocated_materials: Vec<String> = refs
            .materials
            .iter()
            .map(|path| normalize_material_member(path))
            .filter(|path| members.contains(path))
            .collect();
        let relocated_textures: Vec<String> = refs
            .textures
            .iter()
            .map(|path| normalize_texture_member(path))
            .filter(|path| members.contains(path))
            .collect();
        if !relocated_materials.is_empty() {
            entry
                .material_namespace
                .get_or_insert_with(|| namespace.to_string());
            entry.material_namespace_paths = relocated_materials;
        }
        if !relocated_textures.is_empty() {
            entry
                .asset_namespace
                .get_or_insert_with(|| namespace.to_string());
            entry.asset_namespace_paths = relocated_textures;
        }
    });
    let mut existing: std::collections::HashSet<String> = entries
        .iter()
        .map(|e| crate::relocation::member_key_for_source_path(&e.source_path, source_dir))
        .collect();
    let mut sorted_members = members.iter().collect::<Vec<_>>();
    sorted_members.sort_unstable();
    for member in sorted_members {
        if !member.starts_with("meshes/") || existing.contains(member) {
            continue;
        }
        let abs = source_dir.join(member.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !abs.is_file() {
            continue;
        }
        entries.push(NifEntry {
            source_path: member.clone(),
            resolved_path: abs.to_string_lossy().to_string(),
            output_subpath: Some(crate::relocation::insert_namespace_after_root(
                member, namespace,
            )),
            asset_namespace: Some(namespace.to_string()),
            material_namespace: Some(namespace.to_string()),
            asset_namespace_paths: Vec::new(),
            material_namespace_paths: Vec::new(),
            weapon_role: None,
        });
        existing.insert(member.clone());
    }
}

fn referenced_paths_for_entry(entry: &NifEntry, source_dir: &Path) -> Option<ReferencedAssetPaths> {
    let resolved = Path::new(&entry.resolved_path);
    let path = if resolved.is_file() {
        resolved.to_path_buf()
    } else {
        let key = crate::relocation::member_key_for_source_path(&entry.source_path, source_dir);
        source_dir.join(key.replace('/', std::path::MAIN_SEPARATOR_STR))
    };
    NifFile::load(&path)
        .ok()
        .map(|nif| nif.referenced_asset_paths())
}

fn normalize_material_member(path: &str) -> String {
    let rel = crate::relocation::normalize_rel(path);
    if rel.is_empty() || rel.starts_with("materials/") {
        rel
    } else {
        format!("materials/{rel}")
    }
}

fn normalize_texture_member(path: &str) -> String {
    let rel = crate::relocation::normalize_rel(path);
    if rel.is_empty() || rel.starts_with("textures/") {
        rel
    } else {
        format!("textures/{rel}")
    }
}

/// Fold accumulator for the parallel NIF conversion pass.
/// Replaces Vec<NifResult> so results are not retained after aggregation.
#[derive(Default)]
struct NifAgg {
    assets_written: u32,
    warnings: u32,
    report_warnings: u32,
    /// Queued error-level log messages (one per failed NIF).
    error_messages: Vec<String>,
    /// Queued warn-level log messages (NIF report warnings).
    warning_messages: Vec<String>,
    timing_summary: TimingSummary,
    /// Set when a cooperative cancel was observed mid-fold.
    cancelled: bool,
    /// Successful converts whose BA2 sink registration failed.
    sink_failures: u32,
}

enum NifConversionResult {
    Written(ConvertFileReport),
    Failed {
        message: String,
        report: Option<ConvertFileReport>,
    },
}

struct StagedNifDestination {
    directory: tempfile::TempDir,
    path: PathBuf,
    materials_root: PathBuf,
}

impl StagedNifDestination {
    fn new(destination: &Path) -> Result<Self, String> {
        let parent = destination
            .parent()
            .ok_or_else(|| format!("destination has no parent: {}", destination.display()))?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create NIF output directory: {error}"))?;
        let directory = tempfile::Builder::new()
            .prefix(".modkit-nif-")
            .tempdir_in(parent)
            .map_err(|error| format!("create staged NIF directory: {error}"))?;
        let file_name = destination
            .file_name()
            .ok_or_else(|| format!("destination has no file name: {}", destination.display()))?;
        let path = directory.path().join(file_name);
        let materials_root = directory.path().join("materials");
        Ok(Self {
            directory,
            path,
            materials_root,
        })
    }
}

#[derive(Debug)]
struct ArtifactPublication {
    staged: PathBuf,
    destination: PathBuf,
}

struct ArtifactPlan {
    publications: Vec<ArtifactPublication>,
    stale_removals: Vec<PathBuf>,
    material_destinations: Option<Vec<String>>,
    first_person_destination: Option<String>,
}

struct PublishedArtifact {
    destination: PathBuf,
    backup: Option<PathBuf>,
}

struct ArtifactTransactionError {
    message: String,
    preserve_staging: bool,
}

fn classify_convert_result(
    result: Result<ConvertFileReport, String>,
    destination_written: bool,
) -> NifConversionResult {
    let report = match result {
        Ok(report) => report,
        Err(message) => {
            return NifConversionResult::Failed {
                message,
                report: None,
            };
        }
    };
    if !report.errors.is_empty() {
        return NifConversionResult::Failed {
            message: report.errors.join("; "),
            report: Some(report),
        };
    }
    if !report.supported {
        return NifConversionResult::Failed {
            message: "converter reported unsupported input".to_string(),
            report: Some(report),
        };
    }
    if !destination_written {
        return NifConversionResult::Failed {
            message: "converter reported success but wrote no destination NIF".to_string(),
            report: Some(report),
        };
    }
    NifConversionResult::Written(report)
}

fn publish_staged_result(
    result: NifConversionResult,
    staged: &mut StagedNifDestination,
    destination: &Path,
    final_material_root: Option<&Path>,
    artifacts: &EntryArtifactContext,
    ownership: &DestinationOwnership,
    publication_lock: &Mutex<()>,
) -> NifConversionResult {
    let _publication_guard = match publication_lock.lock() {
        Ok(guard) => guard,
        Err(_) => {
            cleanup_staged_artifacts(staged);
            return NifConversionResult::Failed {
                message: "NIF artifact publication lock is poisoned".to_string(),
                report: None,
            };
        }
    };
    let mut report = match result {
        NifConversionResult::Written(report) => report,
        NifConversionResult::Failed {
            mut message,
            report,
        } => {
            if let Err(cleanup_error) = remove_failed_outputs(
                destination,
                Some(staged),
                final_material_root,
                artifacts,
                ownership,
            ) {
                message.push_str("; stale output cleanup failed: ");
                message.push_str(&cleanup_error);
            }
            cleanup_staged_artifacts(staged);
            return NifConversionResult::Failed { message, report };
        }
    };

    let plan = match build_artifact_plan(
        &report,
        staged,
        destination,
        final_material_root,
        artifacts,
        ownership,
    ) {
        Ok(plan) => plan,
        Err(mut message) => {
            if let Err(cleanup_error) = remove_failed_outputs(
                destination,
                Some(staged),
                final_material_root,
                artifacts,
                ownership,
            ) {
                message.push_str("; stale output cleanup failed: ");
                message.push_str(&cleanup_error);
            }
            cleanup_staged_artifacts(staged);
            return NifConversionResult::Failed {
                message,
                report: Some(report),
            };
        }
    };
    if let Err(error) = publish_artifact_transaction(
        &plan.publications,
        &plan.stale_removals,
        &staged.directory.path().join("backups"),
    ) {
        let message = if error.preserve_staging {
            let recovery = preserve_recovery_directory(staged);
            format!(
                "{}; recovery artifacts preserved at {}",
                error.message,
                recovery.display()
            )
        } else {
            cleanup_staged_artifacts(staged);
            error.message
        };
        return NifConversionResult::Failed {
            message,
            report: Some(report),
        };
    }
    if let Some(material_destinations) = plan.material_destinations {
        report.emitted_bgsms = material_destinations;
    }
    report.emitted_first_person = plan.first_person_destination;
    NifConversionResult::Written(report)
}

fn build_artifact_plan(
    report: &ConvertFileReport,
    staged: &StagedNifDestination,
    destination: &Path,
    final_material_root: Option<&Path>,
    artifacts: &EntryArtifactContext,
    ownership: &DestinationOwnership,
) -> Result<ArtifactPlan, String> {
    let (mut publications, material_destinations) = match final_material_root {
        Some(final_root) => {
            let (publications, destinations) =
                validate_staged_materials(report, &staged.materials_root, final_root)?;
            (publications, Some(destinations))
        }
        None => (Vec::new(), None),
    };

    let first_person_destination = plan_first_person_publication(
        report,
        staged.directory.path(),
        destination,
        &mut publications,
    )?;
    for publication in &publications {
        if !ownership.is_owned_by(artifacts.owner, &publication.destination) {
            return Err(format!(
                "converter produced an unreserved or foreign-owned artifact destination: {}",
                publication.destination.display()
            ));
        }
    }
    let kept_destinations: HashSet<String> = publications
        .iter()
        .map(|publication| windows_destination_key(&publication.destination))
        .collect();
    let mut stale_removals = Vec::new();
    if let (Some(relative), Some(final_root)) =
        (artifacts.material_relative.as_deref(), final_material_root)
    {
        for existing in discover_material_sidecars(relative, final_root)? {
            if !kept_destinations.contains(&windows_destination_key(&existing)) {
                if ownership.can_remove(artifacts.owner, &existing) {
                    stale_removals.push(existing);
                }
            }
        }
    }
    if artifacts.cleanup_first_person {
        let first_person = first_person_destination_path(destination);
        if first_person.is_file()
            && !kept_destinations.contains(&windows_destination_key(&first_person))
            && ownership.can_remove(artifacts.owner, &first_person)
        {
            stale_removals.push(first_person);
        }
    }
    if !ownership.is_owned_by(artifacts.owner, destination) {
        return Err(format!(
            "primary NIF destination is not owned by this entry: {}",
            destination.display()
        ));
    }
    publications.push(ArtifactPublication {
        staged: staged.path.clone(),
        destination: destination.to_path_buf(),
    });
    Ok(ArtifactPlan {
        publications,
        stale_removals,
        material_destinations,
        first_person_destination,
    })
}

fn validate_staged_materials(
    report: &ConvertFileReport,
    staged_root: &Path,
    final_root: &Path,
) -> Result<(Vec<ArtifactPublication>, Vec<String>), String> {
    let actual_files = collect_staged_files(staged_root)?;
    let actual_set: HashSet<PathBuf> = actual_files.into_iter().collect();
    let mut reported_set = HashSet::new();
    let mut publications = Vec::with_capacity(report.emitted_bgsms.len());
    let mut destinations = Vec::with_capacity(report.emitted_bgsms.len());

    for emitted in &report.emitted_bgsms {
        let staged_path = PathBuf::from(emitted);
        let relative = staged_path.strip_prefix(staged_root).map_err(|_| {
            format!(
                "converter reported material outside staged root: {}",
                staged_path.display()
            )
        })?;
        if relative.as_os_str().is_empty()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "converter reported invalid staged material path: {}",
                staged_path.display()
            ));
        }
        let extension = staged_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !extension.eq_ignore_ascii_case("bgsm") && !extension.eq_ignore_ascii_case("bgem") {
            return Err(format!(
                "converter reported unsupported staged material artifact: {}",
                staged_path.display()
            ));
        }
        if !staged_path.is_file() {
            return Err(format!(
                "converter reported missing staged material: {}",
                staged_path.display()
            ));
        }
        if !reported_set.insert(staged_path.clone()) {
            return Err(format!(
                "converter reported duplicate staged material: {}",
                staged_path.display()
            ));
        }
        let final_path = final_root.join(relative);
        publications.push(ArtifactPublication {
            staged: staged_path,
            destination: final_path.clone(),
        });
        destinations.push(final_path.to_string_lossy().into_owned());
    }

    if actual_set != reported_set {
        let unreported = actual_set
            .difference(&reported_set)
            .next()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unknown material artifact".to_string());
        return Err(format!(
            "staged material artifact was not reported by converter: {unreported}"
        ));
    }
    Ok((publications, destinations))
}

fn collect_staged_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            format!(
                "read staged material directory {}: {error}",
                directory.display()
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "read staged material entry in {}: {error}",
                    directory.display()
                )
            })?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("inspect staged material {}: {error}", path.display()))?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                files.push(path);
            } else {
                return Err(format!(
                    "unsupported staged material artifact: {}",
                    path.display()
                ));
            }
        }
    }
    Ok(files)
}

fn plan_first_person_publication(
    report: &ConvertFileReport,
    staged_root: &Path,
    destination: &Path,
    publications: &mut Vec<ArtifactPublication>,
) -> Result<Option<String>, String> {
    let Some(staged_value) = report.emitted_first_person.clone() else {
        return Ok(None);
    };
    let staged_path = PathBuf::from(&staged_value);
    if !staged_path.starts_with(staged_root) || !staged_path.is_file() {
        return Err(format!(
            "first-person NIF output is missing or outside staged root: {}",
            staged_path.display()
        ));
    }
    let Some(file_name) = staged_path.file_name() else {
        return Err("first-person NIF output has no file name".to_string());
    };
    let Some(parent) = destination.parent() else {
        return Err("first-person NIF destination has no parent".to_string());
    };
    let published = parent.join(file_name);
    publications.push(ArtifactPublication {
        staged: staged_path,
        destination: published.clone(),
    });
    Ok(Some(published.to_string_lossy().into_owned()))
}

fn publish_artifact_transaction(
    publications: &[ArtifactPublication],
    stale_removals: &[PathBuf],
    backup_root: &Path,
) -> Result<(), ArtifactTransactionError> {
    for publication in publications {
        if !publication.staged.is_file() {
            return Err(ArtifactTransactionError {
                message: format!(
                    "staged artifact is missing: {}",
                    publication.staged.display()
                ),
                preserve_staging: false,
            });
        }
        let parent = publication
            .destination
            .parent()
            .ok_or_else(|| ArtifactTransactionError {
                message: format!(
                    "artifact destination has no parent: {}",
                    publication.destination.display()
                ),
                preserve_staging: false,
            })?;
        std::fs::create_dir_all(parent).map_err(|error| ArtifactTransactionError {
            message: format!(
                "create artifact output directory {}: {error}",
                parent.display()
            ),
            preserve_staging: false,
        })?;
    }

    std::fs::create_dir_all(backup_root).map_err(|error| ArtifactTransactionError {
        message: format!("create artifact backup directory: {error}"),
        preserve_staging: false,
    })?;
    let mut published = Vec::with_capacity(publications.len() + stale_removals.len());
    let mut backup_index = 0usize;
    for destination in stale_removals {
        if !destination.exists() {
            continue;
        }
        if !destination.is_file() {
            let rollback = rollback_artifacts(&mut published);
            return Err(transaction_error(
                format!("stale artifact is not a file: {}", destination.display()),
                None,
                rollback,
            ));
        }
        let backup = backup_root.join(format!("{backup_index}.backup"));
        backup_index += 1;
        if let Err(error) = std::fs::rename(destination, &backup) {
            let rollback = rollback_artifacts(&mut published);
            return Err(transaction_error(
                format!("stage stale artifact {}: {error}", destination.display()),
                None,
                rollback,
            ));
        }
        published.push(PublishedArtifact {
            destination: destination.clone(),
            backup: Some(backup),
        });
    }
    for publication in publications {
        let backup = if publication.destination.is_file() {
            let backup = backup_root.join(format!("{backup_index}.backup"));
            backup_index += 1;
            if let Err(error) = std::fs::rename(&publication.destination, &backup) {
                let rollback = rollback_artifacts(&mut published);
                return Err(transaction_error(
                    format!(
                        "stage previous artifact {}: {error}",
                        publication.destination.display()
                    ),
                    None,
                    rollback,
                ));
            }
            Some(backup)
        } else {
            None
        };

        if let Err(error) = std::fs::rename(&publication.staged, &publication.destination) {
            let restore = backup
                .as_ref()
                .map(|backup| restore_backup(backup, &publication.destination));
            let rollback = rollback_artifacts(&mut published);
            return Err(transaction_error(
                format!(
                    "publish artifact {}: {error}",
                    publication.destination.display()
                ),
                restore,
                rollback,
            ));
        }
        published.push(PublishedArtifact {
            destination: publication.destination.clone(),
            backup,
        });
    }
    Ok(())
}

fn restore_backup(backup: &Path, destination: &Path) -> Result<(), String> {
    std::fs::rename(backup, destination).map_err(|error| {
        format!(
            "restore current artifact {} from {}: {error}",
            destination.display(),
            backup.display()
        )
    })
}

fn rollback_artifacts(published: &mut Vec<PublishedArtifact>) -> Result<(), String> {
    let mut errors = Vec::new();
    while let Some(artifact) = published.pop() {
        if let Err(error) = std::fs::remove_file(&artifact.destination) {
            if error.kind() != std::io::ErrorKind::NotFound {
                errors.push(format!(
                    "remove {}: {error}",
                    artifact.destination.display()
                ));
            }
        }
        if let Some(backup) = artifact.backup
            && let Err(error) = std::fs::rename(&backup, &artifact.destination)
        {
            errors.push(format!(
                "restore {}: {error}",
                artifact.destination.display()
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn transaction_error(
    mut message: String,
    current_restore: Option<Result<(), String>>,
    rollback: Result<(), String>,
) -> ArtifactTransactionError {
    let mut preserve_staging = false;
    if let Some(Err(error)) = current_restore {
        preserve_staging = true;
        message.push_str("; ");
        message.push_str(&error);
    }
    if let Err(error) = rollback {
        preserve_staging = true;
        message.push_str("; rollback failed: ");
        message.push_str(&error);
    }
    ArtifactTransactionError {
        message,
        preserve_staging,
    }
}

fn cleanup_staged_artifacts(staged: &StagedNifDestination) {
    let _ = std::fs::remove_dir_all(staged.directory.path());
}

fn preserve_recovery_directory(staged: &mut StagedNifDestination) -> PathBuf {
    staged.directory.disable_cleanup(true);
    staged.directory.path().to_path_buf()
}

fn staged_destination_or_cleanup<F>(
    destination: &Path,
    final_material_root: Option<&Path>,
    artifacts: &EntryArtifactContext,
    ownership: &DestinationOwnership,
    publication_lock: &Mutex<()>,
    create: F,
) -> Result<StagedNifDestination, String>
where
    F: FnOnce(&Path) -> Result<StagedNifDestination, String>,
{
    match create(destination) {
        Ok(staged) => Ok(staged),
        Err(mut message) => {
            if let Err(cleanup_error) = cleanup_after_staging_init_failure(
                destination,
                final_material_root,
                artifacts,
                ownership,
                publication_lock,
            ) {
                message.push_str("; stale output cleanup failed: ");
                message.push_str(&cleanup_error);
            }
            Err(message)
        }
    }
}

fn cleanup_after_staging_init_failure(
    destination: &Path,
    final_material_root: Option<&Path>,
    artifacts: &EntryArtifactContext,
    ownership: &DestinationOwnership,
    publication_lock: &Mutex<()>,
) -> Result<(), String> {
    let _publication_guard = publication_lock
        .lock()
        .map_err(|_| "NIF artifact publication lock is poisoned".to_string())?;
    remove_failed_outputs(destination, None, final_material_root, artifacts, ownership)
}

fn remove_failed_outputs(
    destination: &Path,
    staged: Option<&StagedNifDestination>,
    final_material_root: Option<&Path>,
    artifacts: &EntryArtifactContext,
    ownership: &DestinationOwnership,
) -> Result<(), String> {
    let mut outputs = vec![destination.to_path_buf()];
    if artifacts.cleanup_first_person {
        outputs.push(first_person_destination_path(destination));
    }
    if let (Some(relative), Some(final_root)) =
        (artifacts.material_relative.as_deref(), final_material_root)
    {
        outputs.extend(discover_material_sidecars(relative, final_root)?);
        let staged_material_root = staged.map(|staged| staged.materials_root.as_path());
        for staged_file in staged_material_root
            .map(collect_staged_files)
            .transpose()?
            .unwrap_or_default()
        {
            let extension = staged_file
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !extension.eq_ignore_ascii_case("bgsm") && !extension.eq_ignore_ascii_case("bgem") {
                continue;
            }
            if let Some(staged_root) = staged_material_root
                && let Ok(relative) = staged_file.strip_prefix(staged_root)
            {
                outputs.push(final_root.join(relative));
            }
        }
    }
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for output in outputs {
        if !seen.insert(windows_destination_key(&output)) {
            continue;
        }
        if !ownership.can_remove(artifacts.owner, &output) {
            continue;
        }
        if let Err(error) = std::fs::remove_file(&output)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            errors.push(format!("remove {}: {error}", output.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

impl NifAgg {
    fn add(&mut self, source_path: String, result: NifConversionResult) {
        match result {
            NifConversionResult::Written(report) => {
                self.assets_written += 1;
                self.timing_summary.add(&source_path, &report);
                self.add_report_warnings(&source_path, &report);
            }
            NifConversionResult::Failed { message, report } => {
                self.error_messages
                    .push(format!("convert_nifs failed {source_path}: {message}"));
                self.warnings += 1;
                if let Some(report) = report {
                    self.timing_summary.add(&source_path, &report);
                    self.add_report_warnings(&source_path, &report);
                }
            }
        }
    }

    fn add_report_warnings(&mut self, source_path: &str, report: &ConvertFileReport) {
        for warning in &report.warnings {
            self.warning_messages
                .push(nif_report_warning_message(source_path, warning));
            self.report_warnings += 1;
        }
    }

    fn merge(mut self, other: NifAgg) -> NifAgg {
        self.assets_written += other.assets_written;
        self.warnings += other.warnings;
        self.report_warnings += other.report_warnings;
        self.error_messages.extend(other.error_messages);
        self.warning_messages.extend(other.warning_messages);
        self.timing_summary = self.timing_summary.merge(other.timing_summary);
        self.cancelled |= other.cancelled;
        self.sink_failures += other.sink_failures;
        self
    }
}

fn nif_report_warning_message(source_path: &str, warning: &str) -> String {
    if is_havok_or_collision_warning(warning) {
        format!("NIF Havok/collision warning {source_path}: {warning}")
    } else {
        format!("NIF warning {source_path}: {warning}")
    }
}

fn is_havok_or_collision_warning(warning: &str) -> bool {
    let lower = warning.to_ascii_lowercase();
    lower.contains("havok")
        || lower.contains("bhk")
        || lower.contains("hknp")
        || lower.contains("collision")
}

#[derive(Default)]
struct TimingSummary {
    file_count: u32,
    total_file_ms: u64,
    by_step_ms: HashMap<String, u64>,
    slowest: Vec<SlowNifTiming>,
}

struct SlowNifTiming {
    elapsed_ms: u64,
    source_path: String,
}

impl TimingSummary {
    fn merge(mut self, other: TimingSummary) -> TimingSummary {
        self.file_count += other.file_count;
        self.total_file_ms += other.total_file_ms;
        for (step, elapsed_ms) in other.by_step_ms {
            *self.by_step_ms.entry(step).or_insert(0) += elapsed_ms;
        }
        self.slowest.extend(other.slowest);
        self.slowest
            .sort_by(|left, right| right.elapsed_ms.cmp(&left.elapsed_ms));
        self.slowest.truncate(5);
        self
    }

    fn add(&mut self, source_path: &str, report: &ConvertFileReport) {
        if report.timings_ms.is_empty() {
            return;
        }

        self.file_count += 1;
        let mut file_total_ms = 0;
        for (step, elapsed_ms) in &report.timings_ms {
            if step == "total" {
                file_total_ms = *elapsed_ms;
            } else {
                *self.by_step_ms.entry(step.clone()).or_insert(0) += *elapsed_ms;
            }
        }
        if file_total_ms == 0 {
            file_total_ms = report
                .timings_ms
                .iter()
                .filter(|(step, _)| step != "total")
                .map(|(_, elapsed_ms)| *elapsed_ms)
                .sum();
        }
        self.total_file_ms += file_total_ms;
        self.slowest.push(SlowNifTiming {
            elapsed_ms: file_total_ms,
            source_path: source_path.to_string(),
        });
        self.slowest
            .sort_by(|left, right| right.elapsed_ms.cmp(&left.elapsed_ms));
        self.slowest.truncate(5);
    }

    fn log_messages(&self) -> Vec<String> {
        if self.file_count == 0 {
            return Vec::new();
        }

        let avg_ms = self.total_file_ms as f64 / f64::from(self.file_count);
        let mut step_totals: Vec<_> = self.by_step_ms.iter().collect();
        step_totals.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
        let step_text = step_totals
            .into_iter()
            .take(12)
            .map(|(step, elapsed_ms)| format!("{step}={elapsed_ms}ms"))
            .collect::<Vec<_>>()
            .join(", ");

        let mut messages = vec![format!(
            "convert_nifs timings: files={}, total_file_ms={}, avg_file_ms={avg_ms:.1}, steps=[{step_text}]",
            self.file_count, self.total_file_ms
        )];
        let slowest_text = self
            .slowest
            .iter()
            .map(|timing| format!("{}ms {}", timing.elapsed_ms, timing.source_path))
            .collect::<Vec<_>>()
            .join("; ");
        if !slowest_text.is_empty() {
            messages.push(format!("convert_nifs slowest: {slowest_text}"));
        }
        messages
    }
}

fn parse_nif_entries(p: &JsonValue) -> Result<Vec<NifEntry>, PhaseError> {
    let arr = p
        .get("nif_paths")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PhaseError::BadParams("missing nif_paths array".into()))?;

    arr.iter()
        .enumerate()
        .map(|(i, entry)| {
            let source_path = entry["source_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "nif_paths[{i}].source_path missing or not a string"
                    ))
                })?
                .to_string();
            let resolved_path = entry["resolved_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "nif_paths[{i}].resolved_path missing or not a string"
                    ))
                })?
                .to_string();
            let weapon_role = entry
                .get("weapon_role")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let output_subpath = entry
                .get("output_subpath")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let asset_namespace = entry
                .get("asset_namespace")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let material_namespace = entry
                .get("material_namespace")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let asset_namespace_paths = parse_string_list(
                entry.get("asset_namespace_paths"),
                &format!("nif_paths[{i}].asset_namespace_paths"),
            )?;
            let material_namespace_paths = parse_string_list(
                entry.get("material_namespace_paths"),
                &format!("nif_paths[{i}].material_namespace_paths"),
            )?;
            Ok(NifEntry {
                source_path,
                resolved_path,
                output_subpath,
                asset_namespace,
                material_namespace,
                asset_namespace_paths,
                material_namespace_paths,
                weapon_role,
            })
        })
        .collect()
}

fn parse_string_list(value: Option<&JsonValue>, field: &str) -> Result<Vec<String>, PhaseError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(PhaseError::BadParams(format!("{field} must be an array")));
    };
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(text) = item.as_str() else {
            return Err(PhaseError::BadParams(format!(
                "{field}[{index}] missing or not a string"
            )));
        };
        if !text.is_empty() {
            out.push(text.to_string());
        }
    }
    Ok(out)
}

fn parse_addon_index_map(p: &JsonValue) -> Result<HashMap<i64, i64>, PhaseError> {
    let Some(obj) = p.get("addon_index_map").and_then(|v| v.as_object()) else {
        return Ok(HashMap::new());
    };
    obj.iter()
        .map(|(k, v)| {
            let key: i64 = k.parse().map_err(|_| {
                PhaseError::BadParams(format!("addon_index_map key not integer: {k}"))
            })?;
            let val: i64 = v.as_i64().ok_or_else(|| {
                PhaseError::BadParams(format!("addon_index_map[{k}] not integer"))
            })?;
            Ok((key, val))
        })
        .collect()
}

fn parse_conversion_workers(p: &JsonValue, fallback: Option<usize>) -> Option<usize> {
    p.get("conversion_workers")
        .and_then(|v| v.as_u64())
        .and_then(|v| usize::try_from(v).ok())
        .filter(|workers| *workers > 0)
        .or_else(|| fallback.filter(|workers| *workers > 0))
}

/// Compute the output path for a NIF inside the mod directory.
///
/// Layout:  `mod_path/data/Meshes/<relative>`
///
/// The `source_path` is a Data-relative game path, e.g. `Meshes/Weapons/Gun.nif`.
/// Paths already containing a leading `Data/` or `Meshes/` segment are normalized
/// so the final tree is `mod_path/data/Meshes/<prefix>/...`.
fn nif_output_path(mod_path: &Path, entry: &NifEntry) -> PathBuf {
    let mut out = mod_path.to_path_buf();
    out.push("data");
    if let Some(output_subpath) = entry.output_subpath.as_deref() {
        push_rel_components(&mut out, output_subpath);
        return out;
    }

    out.push("Meshes");
    let rel = mesh_relative_nif_path(&entry.source_path);
    for component in rel.split('/') {
        if !component.is_empty() {
            out.push(component);
        }
    }
    out
}

fn push_rel_components(out: &mut PathBuf, rel: &str) {
    for component in rel.replace('\\', "/").split('/') {
        let component = component.trim();
        if component.is_empty() || component == "." || component == ".." {
            continue;
        }
        out.push(component);
    }
}

fn mesh_relative_nif_path(source_path: &str) -> String {
    let mut rel = source_path.replace('\\', "/");
    rel = rel.trim_start_matches('/').to_string();
    if rel.len() >= 5 && rel[..5].eq_ignore_ascii_case("data/") {
        rel = rel[5..].to_string();
    }
    if rel.len() >= 7 && rel[..7].eq_ignore_ascii_case("meshes/") {
        rel = rel[7..].to_string();
    }
    rel = strip_known_asset_prefix(&rel).to_string();
    rel
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

#[derive(Default)]
struct SsfMirrorSummary {
    files_found: u32,
    files_copied: u32,
    files_reused: u32,
    failures: u32,
    sink_failures: u32,
    warning_messages: Vec<String>,
}

fn mirror_all_source_ssf_files(
    source_extracted_dir: &Path,
    data_root: &Path,
    mut register: impl FnMut(&Path) -> bool,
) -> SsfMirrorSummary {
    let mut summary = SsfMirrorSummary::default();
    let Some(source_meshes) = find_child_ci(source_extracted_dir, "meshes") else {
        return summary;
    };
    let output_meshes =
        find_child_ci(data_root, "meshes").unwrap_or_else(|| data_root.join("Meshes"));
    let mut pending = vec![source_meshes.clone()];

    while let Some(directory) = pending.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                summary.failures += 1;
                summary.warning_messages.push(format!(
                    "read SSF source directory {}: {error}",
                    directory.display()
                ));
                continue;
            }
        };
        let mut sorted_entries = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => sorted_entries.push(entry),
                Err(error) => {
                    summary.failures += 1;
                    summary.warning_messages.push(format!(
                        "read SSF source entry in {}: {error}",
                        directory.display()
                    ));
                }
            }
        }
        let mut entries = sorted_entries;
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());

        for entry in entries {
            let source = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    summary.failures += 1;
                    summary
                        .warning_messages
                        .push(format!("inspect SSF source {}: {error}", source.display()));
                    continue;
                }
            };
            if file_type.is_dir() {
                pending.push(source);
                continue;
            }
            if !file_type.is_file()
                || !source
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ssf"))
            {
                continue;
            }

            summary.files_found += 1;
            let relative = source
                .strip_prefix(&source_meshes)
                .expect("walked SSF must remain under source Meshes");
            let destination = join_relative_ci(&output_meshes, relative);
            if destination.is_file() {
                summary.files_reused += 1;
            } else {
                let copy_result = destination
                    .parent()
                    .ok_or_else(|| "SSF destination has no parent".to_string())
                    .and_then(|parent| {
                        std::fs::create_dir_all(parent)
                            .map_err(|error| format!("create SSF output directory: {error}"))
                    })
                    .and_then(|_| {
                        std::fs::copy(&source, &destination)
                            .map(|_| ())
                            .map_err(|error| format!("copy SSF: {error}"))
                    });
                if let Err(error) = copy_result {
                    summary.failures += 1;
                    summary.warning_messages.push(format!(
                        "{} -> {}: {error}",
                        source.display(),
                        destination.display()
                    ));
                    continue;
                }
                summary.files_copied += 1;
            }

            if !register(&destination) {
                summary.sink_failures += 1;
                summary.warning_messages.push(format!(
                    "register SSF output with sink: {}",
                    destination.display()
                ));
            }
        }
    }

    summary
}

fn join_relative_ci(root: &Path, relative: &Path) -> PathBuf {
    let mut path = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            continue;
        };
        let name = name.to_string_lossy();
        path = find_child_ci(&path, &name).unwrap_or_else(|| path.join(name.as_ref()));
    }
    path
}

// ---------------------------------------------------------------------------
// Creature skeleton.nif repose (FO4 inverse-bind rest pose)
// ---------------------------------------------------------------------------

/// Gate tolerance: a skinned bone is "already correct" when `world @ bind` is
/// within this of the identity (rotation dimensionless, translation game
/// units). FO76's broken locals miss by ~10²; correct ones cancel to ~1e-9.
const REPOSE_TOL: f64 = 1e-3;

#[derive(Default)]
struct ReposeSummary {
    files_scanned: usize,
    files_reposed: usize,
    bones_reposed: usize,
}

/// Walk `data/Meshes/Actors/<creature>/CharacterAssets/` for `skeleton.nif`,
/// gather bind matrices from the sibling body mesh(es), and repose the
/// skeleton into the FO4 inverse-bind rest pose. Only writes when a bone was
/// actually reposed (already-correct skeletons stay byte-identical).
fn repose_creature_skeletons(data_root: &Path) -> ReposeSummary {
    let mut summary = ReposeSummary::default();
    let Some(meshes) = find_child_ci(data_root, "meshes") else {
        return summary;
    };
    let Some(actors) = find_child_ci(&meshes, "actors") else {
        return summary;
    };
    let Ok(entries) = std::fs::read_dir(&actors) else {
        return summary;
    };
    for entry in entries.flatten() {
        let creature_dir = entry.path();
        if !creature_dir.is_dir() {
            continue;
        }
        let Some(ca_dir) = find_child_ci(&creature_dir, "characterassets") else {
            continue;
        };
        let Some(skel_path) = find_child_ci(&ca_dir, "skeleton.nif") else {
            continue;
        };
        summary.files_scanned += 1;

        let bind = gather_bind_matrices(&ca_dir, &skel_path);
        if bind.is_empty() {
            continue;
        }
        let Ok(mut skel) = NifFile::load(&skel_path) else {
            continue;
        };
        let report = repose_skeleton_to_inverse_bind(&mut skel, &bind, REPOSE_TOL);
        if report.reposed > 0 && skel.save(Some(skel_path.clone())).is_ok() {
            summary.files_reposed += 1;
            summary.bones_reposed += report.reposed;
        }
    }
    summary
}

/// Merge bind matrices from every skinned sibling NIF in `ca_dir` (excluding
/// the skeleton itself), largest file first so the primary body mesh wins on
/// name collisions.
fn gather_bind_matrices(ca_dir: &Path, skel_path: &Path) -> HashMap<String, BindMatrix> {
    let skel_name = skel_path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase());
    let mut siblings: Vec<(u64, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(ca_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !path
                .extension()
                .map(|ext| ext.eq_ignore_ascii_case("nif"))
                .unwrap_or(false)
            {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase());
            if name == skel_name {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            siblings.push((size, path));
        }
    }
    siblings.sort_by(|a, b| b.0.cmp(&a.0));

    let mut binds: HashMap<String, BindMatrix> = HashMap::new();
    for (_, path) in siblings {
        let Ok(nif) = NifFile::load(&path) else {
            continue;
        };
        for (name, matrix) in collect_bind_matrices_by_name(&nif) {
            binds.entry(name).or_insert(matrix);
        }
    }
    binds
}

/// Case-insensitive single-level child lookup (handles `Meshes` vs `meshes`).
fn find_child_ci(parent: &Path, name: &str) -> Option<PathBuf> {
    let target = name.to_ascii_lowercase();
    std::fs::read_dir(parent)
        .ok()?
        .flatten()
        .find(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase() == target)
        .map(|entry| entry.path())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact_context(
        owner: usize,
        material_relative: Option<&str>,
        cleanup_first_person: bool,
    ) -> EntryArtifactContext {
        EntryArtifactContext {
            owner,
            material_relative: material_relative.map(PathBuf::from),
            cleanup_first_person,
        }
    }

    fn destination_ownership(owner: usize, paths: &[&Path]) -> DestinationOwnership {
        DestinationOwnership {
            owners: paths
                .iter()
                .map(|path| (windows_destination_key(path), owner))
                .collect(),
        }
    }

    #[test]
    fn mirrors_all_ssf_files_across_the_source_mesh_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let data_root = tmp.path().join("output/data");
        let clothing = source.join("Meshes/Clothes/Raiders/Outfit.ssf");
        let creature = source.join("Meshes/Actors/Mothman/CharacterAssets/Mothman.SSF");
        let ignored = source.join("Meshes/Clothes/Raiders/Outfit.txt");
        for (path, bytes) in [
            (&clothing, b"clothing".as_slice()),
            (&creature, b"creature".as_slice()),
            (&ignored, b"ignored".as_slice()),
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let mut registered = Vec::new();

        let summary = mirror_all_source_ssf_files(&source, &data_root, |path| {
            registered.push(path.to_path_buf());
            true
        });

        assert_eq!(summary.files_found, 2);
        assert_eq!(summary.files_copied, 2);
        assert_eq!(summary.files_reused, 0);
        assert_eq!(summary.failures, 0);
        assert_eq!(summary.sink_failures, 0);
        assert_eq!(registered.len(), 2);
        assert_eq!(
            std::fs::read(data_root.join("Meshes/Clothes/Raiders/Outfit.ssf")).unwrap(),
            b"clothing"
        );
        assert_eq!(
            std::fs::read(data_root.join("Meshes/Actors/Mothman/CharacterAssets/Mothman.SSF"))
                .unwrap(),
            b"creature"
        );
        assert!(!data_root.join("Meshes/Clothes/Raiders/Outfit.txt").exists());
    }

    #[test]
    fn preserves_and_registers_an_existing_ssf_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let data_root = tmp.path().join("output/data");
        let source_ssf = source.join("Meshes/Clothes/Raiders/Outfit.ssf");
        let existing_ssf = data_root.join("meshes/clothes/raiders/OUTFIT.SSF");
        std::fs::create_dir_all(source_ssf.parent().unwrap()).unwrap();
        std::fs::write(&source_ssf, b"source").unwrap();
        std::fs::create_dir_all(existing_ssf.parent().unwrap()).unwrap();
        std::fs::write(&existing_ssf, b"existing").unwrap();
        let mut registered = Vec::new();

        let summary = mirror_all_source_ssf_files(&source, &data_root, |path| {
            registered.push(path.to_path_buf());
            true
        });

        assert_eq!(summary.files_found, 1);
        assert_eq!(summary.files_copied, 0);
        assert_eq!(summary.files_reused, 1);
        assert_eq!(summary.failures, 0);
        assert_eq!(std::fs::read(&existing_ssf).unwrap(), b"existing");
        assert_eq!(registered, vec![existing_ssf]);
    }

    #[test]
    fn missing_source_meshes_is_an_ssf_no_op() {
        let tmp = tempfile::tempdir().unwrap();
        let mut registered = Vec::new();

        let summary = mirror_all_source_ssf_files(
            &tmp.path().join("source"),
            &tmp.path().join("output/data"),
            |path| {
                registered.push(path.to_path_buf());
                true
            },
        );

        assert_eq!(summary.files_found, 0);
        assert_eq!(summary.files_copied, 0);
        assert_eq!(summary.failures, 0);
        assert!(registered.is_empty());
    }

    #[test]
    fn report_errors_fail_without_counting_an_asset_and_preserve_warnings() {
        let report = ConvertFileReport {
            supported: true,
            errors: vec!["animated Skyrim NIF excluded".to_string()],
            warnings: vec!["collision stripped".to_string()],
            ..Default::default()
        };
        let mut agg = NifAgg::default();
        agg.add(
            "Meshes/Architecture/Animated.nif".to_string(),
            classify_convert_result(Ok(report), true),
        );

        assert_eq!(agg.assets_written, 0);
        assert_eq!(agg.warnings, 1);
        assert_eq!(agg.report_warnings, 1);
        assert!(agg.error_messages[0].contains("animated Skyrim NIF excluded"));
        assert!(agg.warning_messages[0].contains("collision stripped"));
    }

    #[test]
    fn unsupported_or_missing_destination_reports_are_failures() {
        let unsupported = classify_convert_result(Ok(ConvertFileReport::default()), false);
        assert!(matches!(
            unsupported,
            NifConversionResult::Failed { message, .. }
                if message == "converter reported unsupported input"
        ));

        let missing_output = classify_convert_result(
            Ok(ConvertFileReport {
                supported: true,
                ..Default::default()
            }),
            false,
        );
        assert!(matches!(
            missing_output,
            NifConversionResult::Failed { message, .. }
                if message == "converter reported success but wrote no destination NIF"
        ));
    }

    #[test]
    fn supported_written_report_counts_one_asset() {
        let mut agg = NifAgg::default();
        agg.add(
            "Meshes/Architecture/Wall.nif".to_string(),
            classify_convert_result(
                Ok(ConvertFileReport {
                    supported: true,
                    ..Default::default()
                }),
                true,
            ),
        );

        assert_eq!(agg.assets_written, 1);
        assert_eq!(agg.warnings, 0);
        assert!(agg.error_messages.is_empty());
    }

    #[test]
    fn failed_staged_conversion_removes_stale_destination() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("Meshes/Architecture/Wall.nif");
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"stale").unwrap();
        let mut staged = StagedNifDestination::new(&destination).unwrap();
        let publication_lock = Mutex::new(());
        let artifacts = artifact_context(0, None, false);
        let ownership = destination_ownership(0, &[&destination]);

        let result = publish_staged_result(
            NifConversionResult::Failed {
                message: "conversion rejected input".to_string(),
                report: None,
            },
            &mut staged,
            &destination,
            None,
            &artifacts,
            &ownership,
            &publication_lock,
        );

        assert!(matches!(result, NifConversionResult::Failed { .. }));
        assert!(!destination.exists());
    }

    #[test]
    fn validated_staged_conversion_replaces_stale_destination() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("Meshes/Architecture/Wall.nif");
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"stale").unwrap();
        let mut staged = StagedNifDestination::new(&destination).unwrap();
        let publication_lock = Mutex::new(());
        std::fs::write(&staged.path, b"converted").unwrap();
        let artifacts = artifact_context(0, None, false);
        let ownership = destination_ownership(0, &[&destination]);

        let result = publish_staged_result(
            classify_convert_result(
                Ok(ConvertFileReport {
                    supported: true,
                    ..Default::default()
                }),
                true,
            ),
            &mut staged,
            &destination,
            None,
            &artifacts,
            &ownership,
            &publication_lock,
        );

        assert!(matches!(result, NifConversionResult::Written(_)));
        assert_eq!(std::fs::read(destination).unwrap(), b"converted");
    }

    #[test]
    fn material_written_before_later_nif_failure_never_reaches_final_output() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("mod/data/Meshes/Architecture/Wall.nif");
        let final_material_root = temp.path().join("mod/data/Materials");
        let final_material = final_material_root.join("Architecture/Wall_1.bgsm");
        std::fs::create_dir_all(final_material.parent().unwrap()).unwrap();
        std::fs::write(&final_material, b"stale-material").unwrap();
        let mut staged = StagedNifDestination::new(&destination).unwrap();
        let publication_lock = Mutex::new(());
        let staged_material = staged.materials_root.join("Architecture/Wall_1.bgsm");
        std::fs::create_dir_all(staged_material.parent().unwrap()).unwrap();
        std::fs::write(&staged_material, b"material").unwrap();
        let artifacts = artifact_context(0, Some("Architecture/Wall.nif"), false);
        let ownership = destination_ownership(0, &[&destination, &final_material]);

        let result = publish_staged_result(
            classify_convert_result(
                Ok(ConvertFileReport {
                    supported: true,
                    errors: vec!["NIF encoding failed after material synthesis".to_string()],
                    emitted_bgsms: vec![staged_material.to_string_lossy().into_owned()],
                    ..Default::default()
                }),
                false,
            ),
            &mut staged,
            &destination,
            Some(&final_material_root),
            &artifacts,
            &ownership,
            &publication_lock,
        );

        assert!(matches!(result, NifConversionResult::Failed { .. }));
        assert!(!destination.exists());
        assert!(!final_material.exists());
        assert!(!staged.directory.path().exists());
    }

    #[test]
    fn multi_artifact_publish_failure_restores_previous_material() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("mod/data/Meshes/Architecture/Wall.nif");
        let final_material_root = temp.path().join("mod/data/Materials");
        let final_material = final_material_root.join("Architecture/Wall_1.bgsm");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::create_dir_all(final_material.parent().unwrap()).unwrap();
        std::fs::write(&final_material, b"previous-material").unwrap();

        let mut staged = StagedNifDestination::new(&destination).unwrap();
        let publication_lock = Mutex::new(());
        std::fs::write(&staged.path, b"converted-nif").unwrap();
        let staged_material = staged.materials_root.join("Architecture/Wall_1.bgsm");
        std::fs::create_dir_all(staged_material.parent().unwrap()).unwrap();
        std::fs::write(&staged_material, b"converted-material").unwrap();
        let artifacts = artifact_context(0, Some("Architecture/Wall.nif"), false);
        let ownership = destination_ownership(0, &[&destination, &final_material]);

        let result = publish_staged_result(
            classify_convert_result(
                Ok(ConvertFileReport {
                    supported: true,
                    emitted_bgsms: vec![staged_material.to_string_lossy().into_owned()],
                    ..Default::default()
                }),
                true,
            ),
            &mut staged,
            &destination,
            Some(&final_material_root),
            &artifacts,
            &ownership,
            &publication_lock,
        );

        assert!(matches!(result, NifConversionResult::Failed { .. }));
        assert_eq!(
            std::fs::read(&final_material).unwrap(),
            b"previous-material"
        );
        assert!(destination.is_dir());
        assert!(!staged.directory.path().exists());
    }

    #[test]
    fn case_variant_primary_destinations_are_reserved_before_parallel_work() {
        let mod_path = Path::new("C:/mods/Skyrim");
        let entries = vec![
            nif_entry("Meshes/Architecture/Whiterun/Wall.nif"),
            nif_entry("meshes/architecture/whiterun/WALL.NIF"),
        ];

        let (kept, ownership, collisions) =
            reserve_nif_entry_destinations(mod_path, entries, "fo76", "fo4", false, None);

        assert_eq!(kept.len(), 1);
        assert_eq!(collisions, 1);
        assert_eq!(ownership.owners.len(), 1);
    }

    #[test]
    fn primary_and_generated_first_person_collision_drops_later_owner() {
        let mod_path = Path::new("C:/mods/FNV");
        let entries = vec![
            nif_entry("Meshes/Armor/Cuirass.nif"),
            nif_entry("Meshes/Armor/Cuirass_1stPerson.nif"),
        ];

        let (kept, ownership, collisions) =
            reserve_nif_entry_destinations(mod_path, entries, "fnv", "fo4", true, None);

        let first_person =
            first_person_destination_path(&nif_output_path(mod_path, &kept[0].entry));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].entry.source_path, "Meshes/Armor/Cuirass.nif");
        assert_eq!(collisions, 1);
        assert_eq!(ownership.owner(&first_person), Some(0));

        let entries = vec![
            nif_entry("Meshes/Armor/Cuirass.nif"),
            nif_entry("Meshes/Armor/Cuirass_1stPerson.nif"),
        ];
        let (kept, _, collisions) =
            reserve_nif_entry_destinations(mod_path, entries, "fo76", "fo4", false, None);
        assert_eq!(kept.len(), 2);
        assert_eq!(collisions, 0);
        assert!(
            kept.iter()
                .all(|entry| !entry.artifacts.cleanup_first_person)
        );
    }

    #[test]
    fn skyrim_material_case_variant_collision_drops_later_primary_owner() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let final_material_root = mod_path.join("data/Materials");
        let source = temp
            .path()
            .join("source/Meshes/Skyrim/Architecture/Wall.nif");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        let mut nif = NifFile::new("skyrimse");
        let shader_id = nif.add_block("BSLightingShaderProperty", None);
        nif.save(Some(source.clone())).unwrap();

        let mut material_owner = nif_entry("Meshes/Skyrim/Architecture/Wall.nif");
        material_owner.resolved_path = source.to_string_lossy().into_owned();
        material_owner.output_subpath = Some("Meshes/Relocated/Wall.nif".to_string());
        let mut colliding_primary = nif_entry("Meshes/Collision.nif");
        colliding_primary.output_subpath = Some(format!(
            "Materials/sKyRiM/aRcHiTeCtUrE/wAlL_{shader_id}.BGSM"
        ));

        let (kept, ownership, collisions) = reserve_nif_entry_destinations(
            &mod_path,
            vec![material_owner, colliding_primary],
            "skyrimse",
            "fo4",
            false,
            Some(&final_material_root),
        );

        let material =
            final_material_root.join(format!("Skyrim/Architecture/Wall_{shader_id}.bgsm"));
        assert_eq!(kept.len(), 1);
        assert_eq!(collisions, 1);
        assert_eq!(ownership.owner(&material), Some(0));
    }

    #[test]
    fn skyrim_material_discovery_uses_resolved_source_not_output_subpath() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp
            .path()
            .join("mod/data/Meshes/Relocated/Architecture/Wall.nif");
        let final_material_root = temp.path().join("mod/data/Materials");
        let resolved_source =
            Path::new("X:/Extracted/SkyrimSE/Meshes/Skyrim/Architecture/Whiterun/Wall.nif");
        let relative = skyrim_material_relative_path(resolved_source);
        let expected = final_material_root.join("Skyrim/Architecture/Whiterun/Wall_7.bgsm");
        let wrong = final_material_root.join("Relocated/Architecture/Wall_7.bgsm");
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::create_dir_all(expected.parent().unwrap()).unwrap();
        std::fs::create_dir_all(wrong.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"nif").unwrap();
        std::fs::write(&expected, b"material").unwrap();
        std::fs::write(&wrong, b"wrong-material").unwrap();

        let artifacts =
            existing_nif_artifacts(&destination, false, Some((&relative, &final_material_root)))
                .unwrap();
        let keys = artifacts
            .iter()
            .map(|path| windows_destination_key(path))
            .collect::<HashSet<_>>();

        assert_eq!(relative, Path::new("Skyrim/Architecture/Whiterun/Wall.nif"));
        assert!(keys.contains(&windows_destination_key(&expected)));
        assert!(!keys.contains(&windows_destination_key(&wrong)));
    }

    #[test]
    fn staging_init_failure_cleanup_is_locked_and_ownership_aware() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("mod/data/Meshes/Architecture/Wall.nif");
        let first_person = first_person_destination_path(&destination);
        let final_material_root = temp.path().join("mod/data/Materials");
        let material = final_material_root.join("Architecture/Wall_0.bgsm");
        let foreign_material = final_material_root.join("Architecture/Wall_1.bgsm");
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::create_dir_all(material.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"stale-primary").unwrap();
        std::fs::write(&first_person, b"other-owner-primary").unwrap();
        std::fs::write(&material, b"stale-material").unwrap();
        std::fs::write(&foreign_material, b"other-owner-sidecar").unwrap();

        let artifacts = artifact_context(0, Some("Architecture/Wall.nif"), true);
        let mut ownership = destination_ownership(0, &[&destination, &material]);
        ownership
            .owners
            .insert(windows_destination_key(&first_person), 1);
        ownership
            .owners
            .insert(windows_destination_key(&foreign_material), 1);
        let publication_lock = Mutex::new(());

        let error = match staged_destination_or_cleanup(
            &destination,
            Some(&final_material_root),
            &artifacts,
            &ownership,
            &publication_lock,
            |_| Err("forced staging initialization failure".to_string()),
        ) {
            Ok(_) => panic!("forced staging initialization unexpectedly succeeded"),
            Err(error) => error,
        };

        assert_eq!(error, "forced staging initialization failure");
        assert!(!destination.exists());
        assert!(!material.exists());
        assert_eq!(std::fs::read(first_person).unwrap(), b"other-owner-primary");
        assert_eq!(
            std::fs::read(foreign_material).unwrap(),
            b"other-owner-sidecar"
        );
    }

    #[test]
    fn rollback_restore_failure_surfaces_and_preserves_backup() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("mod/data/Meshes/Architecture/Wall.nif");
        std::fs::create_dir_all(&destination).unwrap();
        let mut staged = StagedNifDestination::new(&destination).unwrap();
        let backup = staged.directory.path().join("backups/0.backup");
        std::fs::create_dir_all(backup.parent().unwrap()).unwrap();
        std::fs::write(&backup, b"previous-output").unwrap();
        let mut published = vec![PublishedArtifact {
            destination,
            backup: Some(backup.clone()),
        }];

        let rollback = rollback_artifacts(&mut published);
        assert!(
            rollback
                .as_ref()
                .is_err_and(|message| message.contains("restore"))
        );
        let error = transaction_error("publish failed".to_string(), None, rollback);
        assert!(error.preserve_staging);
        assert!(error.message.contains("rollback failed"));

        let recovery = preserve_recovery_directory(&mut staged);
        drop(staged);
        assert_eq!(std::fs::read(&backup).unwrap(), b"previous-output");
        std::fs::remove_dir_all(recovery).unwrap();
    }

    #[test]
    fn current_backup_restore_failure_surfaces_and_preserves_backup() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("mod/data/Meshes/Architecture/Wall.nif");
        std::fs::create_dir_all(&destination).unwrap();
        let mut staged = StagedNifDestination::new(&destination).unwrap();
        let backup = staged.directory.path().join("backups/current.backup");
        std::fs::create_dir_all(backup.parent().unwrap()).unwrap();
        std::fs::write(&backup, b"previous-output").unwrap();

        let restore = restore_backup(&backup, &destination);
        assert!(restore.is_err());
        let error = transaction_error("publish failed".to_string(), Some(restore), Ok(()));
        assert!(error.preserve_staging);
        assert!(error.message.contains("restore current artifact"));

        let recovery = preserve_recovery_directory(&mut staged);
        drop(staged);
        assert_eq!(std::fs::read(&backup).unwrap(), b"previous-output");
        std::fs::remove_dir_all(recovery).unwrap();
    }

    #[test]
    fn successful_publish_removes_stale_material_and_first_person_sidecars() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("mod/data/Meshes/Architecture/Wall.nif");
        let final_material_root = temp.path().join("mod/data/Materials");
        let material_1 = final_material_root.join("Architecture/Wall_1.bgsm");
        let material_2 = final_material_root.join("Architecture/Wall_2.bgem");
        let first_person = first_person_destination_path(&destination);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::create_dir_all(material_1.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"old-nif").unwrap();
        std::fs::write(&material_1, b"old-material-1").unwrap();
        std::fs::write(&material_2, b"old-material-2").unwrap();
        std::fs::write(&first_person, b"old-first-person").unwrap();

        let mut staged = StagedNifDestination::new(&destination).unwrap();
        std::fs::write(&staged.path, b"new-nif").unwrap();
        let staged_material = staged.materials_root.join("Architecture/Wall_1.bgsm");
        std::fs::create_dir_all(staged_material.parent().unwrap()).unwrap();
        std::fs::write(&staged_material, b"new-material-1").unwrap();
        let publication_lock = Mutex::new(());
        let artifacts = artifact_context(0, Some("Architecture/Wall.nif"), true);
        let ownership = destination_ownership(0, &[&destination, &material_1, &first_person]);

        let result = publish_staged_result(
            NifConversionResult::Written(ConvertFileReport {
                supported: true,
                emitted_bgsms: vec![staged_material.to_string_lossy().into_owned()],
                ..Default::default()
            }),
            &mut staged,
            &destination,
            Some(&final_material_root),
            &artifacts,
            &ownership,
            &publication_lock,
        );

        assert!(matches!(result, NifConversionResult::Written(_)));
        assert_eq!(std::fs::read(&destination).unwrap(), b"new-nif");
        assert_eq!(std::fs::read(&material_1).unwrap(), b"new-material-1");
        assert!(!material_2.exists());
        assert!(!first_person.exists());
    }

    #[test]
    fn skip_existing_discovers_primary_material_and_first_person_sidecars() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("mod/data/Meshes/Architecture/Wall.nif");
        let final_material_root = temp.path().join("mod/data/Materials");
        let material_1 = final_material_root.join("Architecture/Wall_1.bgsm");
        let material_2 = final_material_root.join("Architecture/Wall_2.bgem");
        let unrelated = final_material_root.join("Architecture/Wall_extra.bgsm");
        let first_person = first_person_destination_path(&destination);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::create_dir_all(material_1.parent().unwrap()).unwrap();
        for path in [
            &destination,
            &material_1,
            &material_2,
            &unrelated,
            &first_person,
        ] {
            std::fs::write(path, b"artifact").unwrap();
        }

        let artifacts = existing_nif_artifacts(
            &destination,
            true,
            Some((Path::new("Architecture/Wall.nif"), &final_material_root)),
        )
        .unwrap();
        let keys = artifacts
            .iter()
            .map(|path| windows_destination_key(path))
            .collect::<HashSet<_>>();

        assert_eq!(keys.len(), 4);
        for expected in [&destination, &material_1, &material_2, &first_person] {
            assert!(keys.contains(&windows_destination_key(expected)));
        }
        assert!(!keys.contains(&windows_destination_key(&unrelated)));
    }

    #[test]
    fn skip_existing_sink_omits_foreign_and_unowned_stale_sidecars() {
        use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};

        let temp = tempfile::tempdir().unwrap();
        let mod_root = temp.path().join("mod");
        let data_root = mod_root.join("data");
        let destination = data_root.join("Meshes/Architecture/Wall.nif");
        let final_material_root = data_root.join("Materials");
        let owned_material = final_material_root.join("Architecture/Wall_0.bgsm");
        let foreign_material = final_material_root.join("Architecture/Wall_1.bgsm");
        let unowned_stale = final_material_root.join("Architecture/Wall_2.bgsm");
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::create_dir_all(owned_material.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"existing-nif").unwrap();
        std::fs::write(&owned_material, b"owned-material").unwrap();
        std::fs::write(&foreign_material, b"stale-foreign-material").unwrap();
        std::fs::write(&unowned_stale, b"stale-unowned-material").unwrap();

        let artifacts = existing_nif_artifacts(
            &destination,
            false,
            Some((Path::new("Architecture/Wall.nif"), &final_material_root)),
        )
        .unwrap();
        let context = artifact_context(0, Some("Architecture/Wall.nif"), false);
        let mut ownership = destination_ownership(0, &[&destination, &owned_material]);
        ownership
            .owners
            .insert(windows_destination_key(&foreign_material), 1);
        let sink = SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: mod_root.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        };
        let register = |path: &Path| {
            let relative = path.strip_prefix(&data_root).unwrap();
            sink.add_existing_file(&relative.to_string_lossy().replace('\\', "/"), path)
                .is_ok()
        };

        let outcome = register_owned_existing_artifacts(artifacts, &context, &ownership, register);

        assert_eq!(
            outcome,
            ExistingArtifactRegistration {
                sink_failures: 0,
                foreign_skipped: 1,
                unowned_stale_skipped: 1,
            }
        );
        assert_eq!(
            sink.ba2.as_ref().unwrap().streamed_rel_paths(),
            vec![
                "materials/architecture/wall_0.bgsm".to_string(),
                "meshes/architecture/wall.nif".to_string(),
            ]
        );

        std::fs::write(&foreign_material, b"live-foreign-material").unwrap();
        std::fs::write(&unowned_stale, b"new-live-material").unwrap();
        assert!(
            sink.add_existing_file("Materials/Architecture/Wall_1.bgsm", &foreign_material)
                .unwrap()
        );
        assert!(
            sink.add_existing_file("Materials/Architecture/Wall_2.bgsm", &unowned_stale)
                .unwrap()
        );
    }

    #[test]
    fn declared_missing_first_person_output_fails_transaction() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("mod/data/Meshes/Armor/Cuirass.nif");
        let missing_first_person = temp.path().join("outside/Cuirass_1stPerson.nif");
        let mut staged = StagedNifDestination::new(&destination).unwrap();
        std::fs::write(&staged.path, b"converted").unwrap();
        let publication_lock = Mutex::new(());
        let expected_first_person = first_person_destination_path(&destination);
        let artifacts = artifact_context(0, None, true);
        let ownership = destination_ownership(0, &[&destination, &expected_first_person]);

        let result = publish_staged_result(
            NifConversionResult::Written(ConvertFileReport {
                supported: true,
                emitted_first_person: Some(missing_first_person.to_string_lossy().into_owned()),
                ..Default::default()
            }),
            &mut staged,
            &destination,
            None,
            &artifacts,
            &ownership,
            &publication_lock,
        );

        assert!(matches!(
            result,
            NifConversionResult::Failed { message, .. }
                if message.contains("first-person NIF output is missing or outside staged root")
        ));
        assert!(!destination.exists());
    }

    #[test]
    fn nif_output_path_ignores_prefix_component() {
        let base = Path::new("/mod");
        let result = nif_output_path(base, &nif_entry("Meshes/fnv/Weapons/Gun.nif"));
        assert_eq!(result, Path::new("/mod/data/Meshes/Weapons/Gun.nif"));
    }

    #[test]
    fn nif_output_path_unprefixed() {
        let base = Path::new("/mod");
        let result = nif_output_path(base, &nif_entry("Meshes/Weapons/Gun.nif"));
        assert_eq!(result, Path::new("/mod/data/Meshes/Weapons/Gun.nif"));
    }

    #[test]
    fn nif_output_path_adds_meshes_root_for_mesh_relative_path() {
        let base = Path::new("/mod");
        let result = nif_output_path(base, &nif_entry("Landscape/Trees/Tree.nif"));
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Landscape/Trees/Tree.nif")
        );
    }

    #[test]
    fn nif_output_path_uses_explicit_output_subpath() {
        let base = Path::new("/mod");
        let mut entry = nif_entry("Meshes/Landscape/Trees/Tree.nif");
        entry.output_subpath = Some("Meshes/FO76/Landscape/Trees/Tree.nif".into());
        let result = nif_output_path(base, &entry);
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/FO76/Landscape/Trees/Tree.nif")
        );
    }

    fn nif_entry(source_path: &str) -> NifEntry {
        NifEntry {
            source_path: source_path.into(),
            resolved_path: String::new(),
            output_subpath: None,
            asset_namespace: None,
            material_namespace: None,
            asset_namespace_paths: Vec::new(),
            material_namespace_paths: Vec::new(),
            weapon_role: None,
        }
    }

    /// Empty nif_paths => zero assets written, no error.
    #[test]
    fn convert_nifs_empty_list() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{
            ConversionRun, RunConfig, RunError, RunParams, create_run, drop_run, with_run,
        };
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
                "source_game": "fo4",
                "target_game": "fo4",
                "asset_prefix": "",
                "nif_paths": []
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
            ConvertNifsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 0);
        drop_run(id).unwrap();
    }

    /// Missing resolved_path returns warning, not panic.
    #[test]
    fn convert_nifs_missing_file_counts_as_warning() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{
            ConversionRun, RunConfig, RunError, RunParams, create_run, drop_run, with_run,
        };
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
                "source_game": "fo4",
                "target_game": "fo4",
                "asset_prefix": "",
                "nif_paths": [
                    {
                        "source_path": "Meshes/Test.nif",
                        "resolved_path": "/nonexistent/Test.nif"
                    }
                ]
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
            ConvertNifsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        // Missing file = warning, not a phase error.
        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);
        drop_run(id).unwrap();
    }

    #[test]
    fn convert_nifs_skip_existing_counts_existing_output() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{
            ConversionRun, RunConfig, RunError, RunParams, create_run, drop_run, with_run,
        };
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = std::env::temp_dir().join("convert_nifs_skip_existing_counts_existing_output");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("data").join("Meshes")).unwrap();
        std::fs::write(
            tmp.join("data").join("Meshes").join("Test.nif"),
            b"existing",
        )
        .unwrap();

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
                "source_game": "fo4",
                "target_game": "fo4",
                "asset_prefix": "",
                "skip_existing": true,
                "nif_paths": [
                    {
                        "source_path": "Meshes/Test.nif",
                        "resolved_path": "/nonexistent/Test.nif"
                    }
                ]
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &tmp,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertNifsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 1);
        assert_eq!(report.warnings, 0);
        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn convert_nifs_logs_havok_report_warnings_from_successful_nif() {
        use crate::phase::{PhaseCtx, PhaseEvent, PhaseReport};
        use crate::run::{
            ConversionRun, RunConfig, RunError, RunParams, create_run, drop_run, with_run,
        };
        use crate::translator::Game;
        use indexmap::IndexMap;
        use nif_core_native::model::{NifFile, NifValue};
        use std::sync::atomic::AtomicBool;

        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("bad_havok.nif");
        let mod_dir = temp.path().join("mod");

        let mut nif = NifFile::new("fo76");
        let mut binary_data = IndexMap::new();
        binary_data.insert("Data Size".to_string(), NifValue::UInt(4));
        binary_data.insert("Data".to_string(), NifValue::Bytes(vec![1, 2, 3, 4]));
        let mut fields = IndexMap::new();
        fields.insert("Binary Data".to_string(), NifValue::Struct(binary_data));
        nif.add_block("bhkRagdollSystem", Some(fields));
        nif.save(Some(src.clone())).unwrap();

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

        let (report, events) = with_run(
            id,
            |run| -> Result<(PhaseReport, Vec<PhaseEvent>), RunError> {
                let cancel = std::sync::Arc::new(AtomicBool::new(false));
                let params = serde_json::json!({
                    "source_game": "fo76",
                    "target_game": "fo4",
                    "asset_prefix": "",
                    "nif_paths": [
                        {
                            "source_path": "Meshes/BadHavok.nif",
                            "resolved_path": src.to_string_lossy()
                        }
                    ]
                });
                let source_dir = temp.path().join("source");
                let mut ctx = PhaseCtx {
                    run,
                    mod_path: &mod_dir,
                    source_extracted_dir: &source_dir,
                    target_extracted_dir: None,
                    target_data_dir: None,
                    params: &params,
                    cancel: &cancel,
                };
                let report = ConvertNifsV2Phase
                    .run(&mut ctx)
                    .map_err(|e| RunError::InvalidConfig(e.to_string()))?;
                let events = run.event_rx.try_iter().collect();
                Ok((report, events))
            },
        )
        .unwrap();

        assert_eq!(report.assets_written, 1);
        assert_eq!(report.warnings, 0);
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log {
                phase: "convert_nifs_v2",
                level: LogLevel::Warn,
                message,
            } if message.contains("NIF Havok/collision warning Meshes/BadHavok.nif")
                && message.contains("Havok blobs: failed to convert bhkRagdollSystem block")
        )));

        drop_run(id).unwrap();
    }

    #[test]
    fn parse_addon_index_map_parses_string_keys() {
        let p = serde_json::json!({ "addon_index_map": { "20000": 20001 } });
        let map = parse_addon_index_map(&p).unwrap();
        assert_eq!(map.get(&20000), Some(&20001i64));
    }

    #[test]
    fn parse_conversion_workers_prefers_phase_param() {
        let p = serde_json::json!({ "conversion_workers": 3 });
        assert_eq!(parse_conversion_workers(&p, Some(7)), Some(3));
    }

    #[test]
    fn parse_conversion_workers_uses_run_config_fallback() {
        let p = serde_json::json!({});
        assert_eq!(parse_conversion_workers(&p, Some(7)), Some(7));
    }

    #[test]
    fn parse_conversion_workers_ignores_zero_values() {
        let p = serde_json::json!({ "conversion_workers": 0 });
        assert_eq!(parse_conversion_workers(&p, Some(0)), None);
    }

    #[test]
    fn panic_payload_to_string_extracts_message() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| panic!("bad nif")));
        let payload = result.unwrap_err();
        assert_eq!(panic_payload_to_string(&*payload), "bad nif");
    }

    #[test]
    fn timing_summary_reports_step_totals_and_slowest_files() {
        let mut summary = TimingSummary::default();
        let first = ConvertFileReport {
            timings_ms: vec![
                ("load".to_string(), 3),
                ("fo76_havok_blobs".to_string(), 20),
                ("save".to_string(), 7),
                ("total".to_string(), 35),
            ],
            ..Default::default()
        };
        let second = ConvertFileReport {
            timings_ms: vec![
                ("load".to_string(), 5),
                ("fo76_havok_blobs".to_string(), 1),
                ("save".to_string(), 2),
                ("total".to_string(), 10),
            ],
            ..Default::default()
        };

        summary.add("Meshes/Slow.nif", &first);
        summary.add("Meshes/Fast.nif", &second);

        let messages = summary.log_messages();
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("files=2"));
        assert!(messages[0].contains("total_file_ms=45"));
        assert!(messages[0].contains("fo76_havok_blobs=21ms"));
        assert!(messages[0].contains("load=8ms"));
        assert!(messages[1].contains("35ms Meshes/Slow.nif"));
        assert!(messages[1].contains("10ms Meshes/Fast.nif"));
    }

    #[test]
    fn parse_nif_entries_missing_field_returns_error() {
        let p = serde_json::json!({
            "nif_paths": [{ "source_path": "Meshes/Foo.nif" }]
        });
        assert!(parse_nif_entries(&p).is_err());
    }

    #[test]
    fn convert_nifs_emits_progress_events() {
        use crate::phase::{PhaseCtx, PhaseEvent, PhaseReport};
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

        let _report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "source_game": "fo4",
                "target_game": "fo4",
                "asset_prefix": "",
                "nif_paths": [
                    {
                        "source_path": "Meshes/Test.nif",
                        "resolved_path": "/nonexistent/Test.nif"
                    }
                ]
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
            ConvertNifsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let progress_seen = with_run(id, |run| -> Result<bool, RunError> {
            let mut seen = false;
            while let Ok(ev) = run.event_rx.try_recv() {
                if matches!(
                    ev,
                    PhaseEvent::Progress {
                        phase: "convert_nifs_v2",
                        ..
                    }
                ) {
                    seen = true;
                }
            }
            Ok(seen)
        })
        .unwrap();
        assert!(
            progress_seen,
            "nif phase must emit at least one Progress event"
        );

        drop_run(id).unwrap();
    }

    /// Two missing NIFs => 0 assets_written, 2 warnings.
    #[test]
    fn nif_phase_does_not_retain_all_results() {
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
                "source_game": "fo4",
                "target_game": "fo4",
                "asset_prefix": "",
                "nif_paths": [
                    { "source_path": "Meshes/A.nif", "resolved_path": "/missing/A.nif" },
                    { "source_path": "Meshes/B.nif", "resolved_path": "/missing/B.nif" }
                ]
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
            ConvertNifsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        // Both NIFs are missing — fold must accumulate 0 written + 2 warnings.
        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 2);
        drop_run(id).unwrap();
    }

    #[test]
    fn apply_nif_relocation_namespaces_members_and_appends_absent() {
        let tmp = std::env::temp_dir().join("apply_nif_relocation_namespaces_members");
        let landscape = tmp.join("meshes").join("landscape");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&landscape).unwrap();
        std::fs::write(landscape.join("present.nif"), b"nif").unwrap();
        std::fs::write(landscape.join("rock01.nif"), b"nif").unwrap();

        let members: std::collections::HashSet<String> = [
            "meshes/landscape/present.nif".to_string(),
            "meshes/landscape/rock01.nif".to_string(),
        ]
        .into_iter()
        .collect();

        let mut entries = vec![
            nif_entry("Meshes/Landscape/Present.nif"),
            nif_entry("Meshes/Clutter/Unique.nif"),
        ];

        apply_nif_relocation(&mut entries, &members, "FO76", &tmp);

        // Present member: output namespaced + asset/material namespace set so
        // nif_core rewrites its internal texture/material slots.
        let present = entries
            .iter()
            .find(|e| e.source_path == "Meshes/Landscape/Present.nif")
            .unwrap();
        assert_eq!(present.asset_namespace.as_deref(), Some("FO76"));
        assert_eq!(present.material_namespace.as_deref(), Some("FO76"));
        assert_eq!(
            present.output_subpath.as_deref(),
            Some("meshes/FO76/landscape/present.nif")
        );

        // Non-member: left untouched.
        let unique = entries
            .iter()
            .find(|e| e.source_path == "Meshes/Clutter/Unique.nif")
            .unwrap();
        assert!(unique.asset_namespace.is_none());
        assert!(unique.material_namespace.is_none());
        assert!(unique.output_subpath.is_none());

        // Absent member present on disk: appended with namespaced output.
        let rock = entries
            .iter()
            .find(|e| e.source_path == "meshes/landscape/rock01.nif")
            .unwrap();
        assert_eq!(rock.asset_namespace.as_deref(), Some("FO76"));
        assert_eq!(
            rock.output_subpath.as_deref(),
            Some("meshes/FO76/landscape/rock01.nif")
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn apply_nif_relocation_selectively_namespaces_nonmember_relocated_material() {
        use indexmap::IndexMap;
        use nif_core_native::model::{NifFile, NifValue};

        let tmp = std::env::temp_dir()
            .join("apply_nif_relocation_namespaces_nonmember_referencing_relocated_material");
        let nif_path = tmp
            .join("meshes")
            .join("landscape")
            .join("trees")
            .join("treeforest03.nif");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(nif_path.parent().unwrap()).unwrap();

        let mut nif = NifFile::new("fo76");
        let mut fields = IndexMap::new();
        fields.insert(
            "Name".to_string(),
            NifValue::String("Materials\\Landscape\\Trees\\TreeForestBare.BGSM".to_string()),
        );
        nif.add_block("BSLightingShaderProperty", Some(fields));
        nif.save(Some(nif_path.clone())).unwrap();

        let members: std::collections::HashSet<String> =
            ["materials/landscape/trees/treeforestbare.bgsm".to_string()]
                .into_iter()
                .collect();
        let mut entries = vec![nif_entry("Meshes/Landscape/Trees/TreeForest03.nif")];

        apply_nif_relocation(&mut entries, &members, "FO76", &tmp);

        let entry = &entries[0];
        assert_eq!(entry.material_namespace.as_deref(), Some("FO76"));
        assert!(entry.asset_namespace.is_none());
        assert!(entry.output_subpath.is_none());
        assert_eq!(
            entry.material_namespace_paths,
            vec!["materials/landscape/trees/treeforestbare.bgsm"]
        );
        assert!(entry.asset_namespace_paths.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn apply_nif_relocation_namespaces_absolute_fo76_material_without_data_segment() {
        use indexmap::IndexMap;
        use nif_core_native::model::{NifFile, NifValue};

        let tmp = std::env::temp_dir()
            .join("apply_nif_relocation_namespaces_absolute_fo76_material_without_data_segment");
        let nif_path = tmp
            .join("meshes")
            .join("landscape")
            .join("dirtcliffs")
            .join("ecliffgrassstr01.nif");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(nif_path.parent().unwrap()).unwrap();

        let mut nif = NifFile::new("fo76");
        let mut fields = IndexMap::new();
        fields.insert(
            "Name".to_string(),
            NifValue::String(
                r"C:\Projects\76\Build\PC\Materials\Landscape\Rocks\MtnTopCliff_Tiled01.BGSM"
                    .to_string(),
            ),
        );
        nif.add_block("BSLightingShaderProperty", Some(fields));
        nif.save(Some(nif_path.clone())).unwrap();

        let members: std::collections::HashSet<String> =
            ["materials/landscape/rocks/mtntopcliff_tiled01.bgsm".to_string()]
                .into_iter()
                .collect();
        let mut entries = vec![nif_entry(
            "Meshes/Landscape/DirtCliffs/ECliffGrassStr01.nif",
        )];

        apply_nif_relocation(&mut entries, &members, "FO76", &tmp);

        let entry = &entries[0];
        assert_eq!(entry.material_namespace.as_deref(), Some("FO76"));
        assert_eq!(
            entry.material_namespace_paths,
            vec!["materials/landscape/rocks/mtntopcliff_tiled01.bgsm"]
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn convert_nifs_v2_is_registered_and_runs_empty_input() {
        use crate::phase::{PhaseCtx, PhaseEvent, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        assert!(
            crate::phase::build_registry()
                .get("convert_nifs_v2")
                .is_some()
        );

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

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "source_game": "fo76",
                "target_game": "fo4",
                "asset_prefix": "",
                "nif_paths": []
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
            let report = ConvertNifsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))?;
            Ok(report)
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        drop_run(id).unwrap();
    }

    /// A colliding landscape mesh (present in BOTH FO76 and FO4) must be relocated
    /// from the FO76 *source* content, even when the phase's `source_extracted_dir`
    /// param points at the FO4 (target) dir — as the Python NIF driver currently
    /// passes it. Regression guard for the "purple tree" bug where the FO4 base NIF
    /// was relocated into the FO76 namespace instead of the converted FO76 mesh.
    #[test]
    fn convert_nifs_relocation_member_resolves_from_config_source_not_phase_param() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use indexmap::IndexMap;
        use nif_core_native::model::{NifFile, NifValue};
        use std::sync::atomic::AtomicBool;

        fn write_material_nif(path: &Path, material: &str) {
            let mut nif = NifFile::new("fo76");
            let mut fields = IndexMap::new();
            fields.insert("Name".to_string(), NifValue::String(material.to_string()));
            nif.add_block("BSLightingShaderProperty", Some(fields));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            nif.save(Some(path.to_path_buf())).unwrap();
        }

        let tmp =
            std::env::temp_dir().join("convert_nifs_relocation_member_resolves_from_config_source");
        let _ = std::fs::remove_dir_all(&tmp);
        let fo76 = tmp.join("fo76");
        let fo4 = tmp.join("fo4");
        let mod_dir = tmp.join("mod");
        let member = "meshes/landscape/x.nif";
        let member_native = member.replace('/', std::path::MAIN_SEPARATOR_STR);
        write_material_nif(
            &fo76.join(&member_native),
            "Materials\\Landscape\\Fo76Mat.bgsm",
        );
        write_material_nif(
            &fo4.join(&member_native),
            "Materials\\Landscape\\Fo4Mat.bgsm",
        );

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                source_extracted_dir: Some(fo76.clone()),
                target_extracted_dir: Some(fo4.clone()),
                base_asset_relocation_mesh_roots: vec!["meshes/landscape".to_string()],
                base_asset_namespace: "FO76".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            assert!(
                run.relocation_members.contains(member),
                "colliding mesh should be a relocation member"
            );
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "source_game": "fo76",
                "target_game": "fo4",
                "asset_prefix": "",
                "nif_paths": []
            });
            // Phase param deliberately points at FO4 (target) — the relocation must
            // still resolve the member from the run's FO76 source dir.
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &fo4,
                target_extracted_dir: Some(fo4.as_path()),
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertNifsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        assert_eq!(report.assets_written, 1);

        let out = mod_dir.join("data/meshes/FO76/landscape/x.nif");
        let refs = NifFile::load(&out).unwrap().referenced_asset_paths();
        let joined = refs.materials.join(",");
        assert!(
            joined.contains("fo76mat"),
            "relocated NIF must carry the FO76 source material, got {joined:?}"
        );
        assert!(
            !joined.contains("fo4mat"),
            "relocated NIF must NOT carry the FO4 base material, got {joined:?}"
        );

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // -----------------------------------------------------------------------
    // Creature skeleton repose (on-disk orchestration)
    // -----------------------------------------------------------------------

    use indexmap::IndexMap;
    use nif_core_native::model::NifValue;
    use nif_core_native::skeleton_repose::skeleton_bind_consistency;

    fn rz(deg: f64) -> [[f64; 3]; 3] {
        let r = deg.to_radians();
        let (s, c) = r.sin_cos();
        [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
    }

    fn transpose3(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
        [
            [m[0][0], m[1][0], m[2][0]],
            [m[0][1], m[1][1], m[2][1]],
            [m[0][2], m[1][2], m[2][2]],
        ]
    }

    fn apply3(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    }

    fn rot_struct(m: [[f64; 3]; 3]) -> NifValue {
        let mut f = IndexMap::new();
        for (k, v) in [
            ("m11", m[0][0]),
            ("m21", m[0][1]),
            ("m31", m[0][2]),
            ("m12", m[1][0]),
            ("m22", m[1][1]),
            ("m32", m[1][2]),
            ("m13", m[2][0]),
            ("m23", m[2][1]),
            ("m33", m[2][2]),
        ] {
            f.insert(k.to_string(), NifValue::Float(v));
        }
        NifValue::Struct(f)
    }

    fn vec_struct(t: [f64; 3]) -> NifValue {
        let mut f = IndexMap::new();
        f.insert("x".to_string(), NifValue::Float(t[0]));
        f.insert("y".to_string(), NifValue::Float(t[1]));
        f.insert("z".to_string(), NifValue::Float(t[2]));
        NifValue::Struct(f)
    }

    fn add_skel_bone(
        nif: &mut NifFile,
        name: &str,
        rot: [[f64; 3]; 3],
        trans: [f64; 3],
        children: &[usize],
    ) -> usize {
        let mut fields = IndexMap::new();
        fields.insert("Name".to_string(), NifValue::String(name.to_string()));
        fields.insert("Translation".to_string(), vec_struct(trans));
        fields.insert("Rotation".to_string(), rot_struct(rot));
        fields.insert(
            "Num Children".to_string(),
            NifValue::UInt(children.len() as u64),
        );
        fields.insert(
            "Children".to_string(),
            NifValue::Array(children.iter().map(|c| NifValue::Ref(*c as i32)).collect()),
        );
        nif.add_block("NiNode", Some(fields))
    }

    /// Body mesh with a BSSkin chain binding BoneA (world Rz(30)+t) and
    /// BoneB (world Rz(75)+t). Bind = inverse(world) = (Rᵀ, -Rᵀt).
    fn write_body_mesh(path: &Path) {
        let bind_entry = |rot: [[f64; 3]; 3], trans: [f64; 3]| {
            let br = transpose3(rot);
            let bt = apply3(br, trans).map(|x| -x);
            let mut sphere = IndexMap::new();
            sphere.insert("Center".to_string(), NifValue::Vec3([0.0, 0.0, 0.0]));
            sphere.insert("Radius".to_string(), NifValue::Float(0.0));
            let mut e = IndexMap::new();
            e.insert("Bounding Sphere".to_string(), NifValue::Struct(sphere));
            e.insert("Rotation".to_string(), rot_struct(br));
            e.insert("Translation".to_string(), vec_struct(bt));
            e.insert("Scale".to_string(), NifValue::Float(1.0));
            NifValue::Struct(e)
        };

        let mut nif = NifFile::new("fo4");
        let a = add_skel_bone(&mut nif, "BoneA", rz(30.0), [10.0, 0.0, 0.0], &[]);
        let b = add_skel_bone(&mut nif, "BoneB", rz(75.0), [10.0, 20.0, 0.0], &[]);

        let mut data_fields = IndexMap::new();
        data_fields.insert("Num Bones".to_string(), NifValue::UInt(2));
        data_fields.insert(
            "Bone List".to_string(),
            NifValue::Array(vec![
                bind_entry(rz(30.0), [10.0, 0.0, 0.0]),
                bind_entry(rz(75.0), [10.0, 20.0, 0.0]),
            ]),
        );
        let data = nif.add_block("BSSkin::BoneData", Some(data_fields));

        let mut inst = IndexMap::new();
        inst.insert("Data".to_string(), NifValue::Ref(data as i32));
        inst.insert("Skin Partition".to_string(), NifValue::Ref(-1));
        inst.insert("Skeleton Root".to_string(), NifValue::Ref(0));
        inst.insert("Num Bones".to_string(), NifValue::UInt(2));
        inst.insert(
            "Bones".to_string(),
            NifValue::Array(vec![NifValue::Ref(a as i32), NifValue::Ref(b as i32)]),
        );
        nif.add_block("BSSkin::Instance", Some(inst));

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        nif.save(Some(path.to_path_buf())).unwrap();
    }

    fn body_bind_map(body_path: &Path) -> HashMap<String, BindMatrix> {
        collect_bind_matrices_by_name(&NifFile::load(body_path).unwrap())
    }

    #[test]
    fn repose_creature_skeletons_fixes_broken_skeleton_on_disk() {
        let tmp = std::env::temp_dir().join("repose_creature_fixes_broken");
        let _ = std::fs::remove_dir_all(&tmp);
        let ca = tmp.join("data/Meshes/Actors/TestCreature/CharacterAssets");
        write_body_mesh(&ca.join("TestCreature.nif"));

        // Broken skeleton: both bones at identity local.
        let skel_path = ca.join("skeleton.nif");
        let mut skel = NifFile::new("fo4");
        let b = add_skel_bone(&mut skel, "BoneB", rz(0.0), [0.0; 3], &[]);
        add_skel_bone(&mut skel, "BoneA", rz(0.0), [0.0; 3], &[b]);
        skel.save(Some(skel_path.clone())).unwrap();

        let bind = body_bind_map(&ca.join("TestCreature.nif"));
        let (before, total) =
            skeleton_bind_consistency(&NifFile::load(&skel_path).unwrap(), &bind, REPOSE_TOL);
        assert_eq!((before, total), (0, 2), "raw skeleton is inconsistent");

        let summary = repose_creature_skeletons(&tmp.join("data"));
        assert_eq!(summary.files_scanned, 1);
        assert_eq!(summary.files_reposed, 1);
        assert_eq!(summary.bones_reposed, 2);

        let (after, total) =
            skeleton_bind_consistency(&NifFile::load(&skel_path).unwrap(), &bind, REPOSE_TOL);
        assert_eq!(after, total, "every skinned bone consistent after repose");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn repose_creature_skeletons_noop_on_consistent_skeleton() {
        let tmp = std::env::temp_dir().join("repose_creature_noop_consistent");
        let _ = std::fs::remove_dir_all(&tmp);
        let ca = tmp.join("data/Meshes/Actors/TestCreature/CharacterAssets");
        write_body_mesh(&ca.join("TestCreature.nif"));

        // Consistent skeleton: localA = W_A, localB = inv(W_A) @ W_B.
        // inv(W_A) = (R_Aᵀ, -R_Aᵀ t_A); compose to get B's local.
        let ra = rz(30.0);
        let ta = [10.0, 0.0, 0.0];
        let rb = rz(75.0);
        let tb = [10.0, 20.0, 0.0];
        let rat = transpose3(ra);
        // local_b rotation = R_Aᵀ @ R_B ; translation = R_Aᵀ @ (t_B - t_A)
        let mut lb_rot = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                lb_rot[i][j] = rat[i][0] * rb[0][j] + rat[i][1] * rb[1][j] + rat[i][2] * rb[2][j];
            }
        }
        let lb_trans = apply3(rat, [tb[0] - ta[0], tb[1] - ta[1], tb[2] - ta[2]]);

        let skel_path = ca.join("skeleton.nif");
        let mut skel = NifFile::new("fo4");
        let b = add_skel_bone(&mut skel, "BoneB", lb_rot, lb_trans, &[]);
        add_skel_bone(&mut skel, "BoneA", ra, ta, &[b]);
        skel.save(Some(skel_path.clone())).unwrap();

        let bind = body_bind_map(&ca.join("TestCreature.nif"));
        let (before, total) =
            skeleton_bind_consistency(&NifFile::load(&skel_path).unwrap(), &bind, REPOSE_TOL);
        assert_eq!((before, total), (2, 2), "skeleton already consistent");

        let bytes_before = std::fs::read(&skel_path).unwrap();
        let summary = repose_creature_skeletons(&tmp.join("data"));
        assert_eq!(summary.files_scanned, 1);
        assert_eq!(
            summary.files_reposed, 0,
            "no-op gate must not rewrite skeleton"
        );
        let bytes_after = std::fs::read(&skel_path).unwrap();
        assert_eq!(
            bytes_before, bytes_after,
            "skeleton.nif must be byte-identical"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
