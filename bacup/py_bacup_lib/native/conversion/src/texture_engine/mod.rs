//! texture_engine.
//!
//! Triage-driven FO76→FO4 texture conversion: PassThrough byte-copies,
//! PerTexel mip-preserving u8 transforms, BundleRecompile via the legacy
//! shader math, BC7 GPU encodes through one submission thread (GpuService).
//! Standalone library consumed by the canonical `convert_textures_v2` phase.

pub mod gpu_service;
pub mod materials;
pub mod triage;

#[cfg(test)]
mod corpus_tests;
mod cubemaps;
mod executors;

pub use gpu_service::GpuService;
pub use triage::{TextureTask, triage_request, triage_terrain_request};

/// Stable interface — do not rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriageClass {
    PassThrough,
    PerTexel,
    BundleRecompile,
}

/// One group's classified work. The driver produces one `TextureJob` per
/// legacy work item; each task inside carries its own `TriageClass`.
#[derive(Debug)]
pub struct TextureJob {
    pub output_dir: std::path::PathBuf,
    pub tasks: Vec<TextureTask>,
}

/// Zero the blue channel in place — u8 equivalent of
/// `fo76_normalized_normal_to_fo4_buffer` (the clamp ops are no-ops on
/// u8-sourced data).
pub fn kernel_normal_zero_b(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        px[2] = 0;
    }
}

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use materials_native::texture_convert::{
    TextureConversionParamsPayload, TexturePathInput, TexturePathOutput, TextureSetPathRequest,
};
use rayon::prelude::*;

use crate::phase::textures::{
    TextureEntry, TextureWorkItem, bucket_textures_by_output_subdir, build_request,
    build_texture_work_items, enumerate_source_textures, game_texture_suffixes,
    group_is_base_owned, output_exists_in_target_game,
};
use crate::terrain_textures::manifest::TerrainTextureJob;
use executors::TextureOutputSink;

#[derive(Debug, Clone)]
pub struct TextureEntryIn {
    pub source_path: String,
    pub output_subpath: Option<String>,
}

pub struct TextureEngineParams {
    pub source_extracted: PathBuf,
    /// `<mod>/data` — outputs land under `data_root/Textures/...`.
    pub data_root: PathBuf,
    pub source_game: String,
    pub target_game: String,
    /// Explicit input list (ignored when `convert_all`, mirroring the legacy
    /// phase).
    pub textures: Vec<TextureEntryIn>,
    pub terrain_jobs: Vec<TerrainTextureJob>,
    pub convert_all: bool,
    pub pbr_carry: bool,
    pub landscape_mip_flooding: bool,
    /// Run-init product — consumed, never built here.
    pub relocation_members: HashSet<String>,
    pub namespace: String,
    /// Target-game dirs for the diffuse-keyed base-owned skip.
    pub target_dirs: Vec<PathBuf>,
    pub target_assets: Option<std::sync::Arc<crate::target_assets::TargetAssetStore>>,
    pub skip_existing: bool,
    pub use_gpu: bool,
    pub gpu_min_pixels: u32,
    pub gpu_queue_cap: usize,
    pub workers: Option<usize>,
    pub conv_params: TextureConversionParamsPayload,
    pub format_overrides: HashMap<String, String>,
    /// Output sink — successful outputs are streamed to the sink when an
    /// executor has the bytes in memory (None = legacy behavior).
    pub sink: Option<std::sync::Arc<crate::sinks::SinkSet>>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ClassCounters {
    pub pass_through: u64,
    pub per_texel: u64,
    /// Subset of `per_texel` whose source mips were not box-consistent with
    /// mip0 and therefore executed on the legacy residue path
    /// (executors::source_mips_box_consistent).
    pub per_texel_demoted: u64,
    pub bundle: u64,
    pub legacy_residue: u64,
    /// Cumulative worker-time per class in ms (sums across workers, so it can
    /// exceed wall clock; divide by worker count for a wall-clock feel).
    pub pass_through_ms: u64,
    pub per_texel_ms: u64,
    pub bundle_ms: u64,
    pub legacy_residue_ms: u64,
}

#[derive(Debug, Default)]
pub struct TextureEngineReport {
    pub groups: u64,
    pub outputs_written: u64,
    pub failed: u64,
    pub errors: Vec<String>,
    pub skipped_existing: u64,
    pub skipped_base_owned_groups: u64,
    pub skipped_base_owned_outputs: u64,
    /// Retained for report compatibility; terrain jobs do not suppress regular groups.
    pub skipped_terrain_groups: u64,
    pub no_request_groups: u64,
    pub mip_flooded_outputs: u64,
    pub class: ClassCounters,
    pub gpu: gpu_service::GpuServiceStats,
    pub elapsed_ms: u64,
}

#[derive(Default)]
struct GroupOutcome {
    outputs_written: u64,
    failed: u64,
    skipped_existing: u64,
    skipped_base_owned_groups: u64,
    skipped_base_owned_outputs: u64,
    no_request_groups: u64,
    mip_flooded_outputs: u64,
    error_messages: Vec<String>,
    pass_through: u64,
    per_texel: u64,
    per_texel_demoted: u64,
    bundle: u64,
    legacy_residue: u64,
    pass_through_ms: u64,
    per_texel_ms: u64,
    bundle_ms: u64,
    legacy_residue_ms: u64,
}

fn panic_payload_to_string(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<unknown payload>".to_string()
    }
}

fn texture_group_label(item: &TextureWorkItem) -> String {
    item.group
        .files
        .first()
        .map(|(path, role)| format!("{} ({role})", path.display()))
        .unwrap_or_else(|| item.output_dir.display().to_string())
}

fn texture_task_label(task: &TextureTask) -> String {
    match task {
        TextureTask::Single { input, output, .. }
        | TextureTask::SingleResidue { input, output } => {
            format!("{} -> {}", input.path.display(), output.path.display())
        }
        TextureTask::Bundle {
            diffuse,
            reflectivity,
            lighting,
            ..
        } => format!(
            "bundle {}, {}, {}",
            diffuse.path.display(),
            reflectivity.path.display(),
            lighting.path.display()
        ),
        TextureTask::SpecGloss {
            reflectivity,
            lighting,
            out_specular,
        } => format!(
            "specgloss {}, {} -> {}",
            reflectivity.path.display(),
            lighting.path.display(),
            out_specular.path.display()
        ),
        TextureTask::LegacySpecGloss {
            normal,
            envmask,
            out_specular,
            ..
        } => format!(
            "legacy specgloss {}{} -> {}",
            normal.path.display(),
            envmask
                .as_ref()
                .map(|mask| format!(", {}", mask.path.display()))
                .unwrap_or_default(),
            out_specular.path.display()
        ),
        TextureTask::CubemapNormalize { input, output } => format!(
            "cubemap {} -> {}",
            input.path.display(),
            output.path.display()
        ),
    }
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn terrain_texture_request(
    job: &TerrainTextureJob,
    params: &TextureEngineParams,
) -> Result<TextureSetPathRequest, String> {
    let prefix = job.output_prefix.trim().replace('\\', "/");
    let relative = Path::new(&prefix);
    if prefix.is_empty()
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
            "invalid terrain texture output prefix: {}",
            job.output_prefix
        ));
    }
    for (role, value) in [
        ("diffuse", &job.diffuse_path),
        ("normal", &job.normal_path),
        ("reflectivity", &job.reflectivity_path),
        ("lighting", &job.lighting_path),
    ] {
        if value.trim().is_empty() {
            return Err(format!(
                "terrain texture job {} has no {role} input",
                job.output_prefix
            ));
        }
    }

    let output_base = params.data_root.join(relative);
    Ok(TextureSetPathRequest {
        source_game: params.source_game.clone(),
        target_game: params.target_game.clone(),
        inputs: vec![
            TexturePathInput {
                role: "diffuse".to_owned(),
                path: PathBuf::from(&job.diffuse_path),
            },
            TexturePathInput {
                role: "normal".to_owned(),
                path: PathBuf::from(&job.normal_path),
            },
            TexturePathInput {
                role: "reflectivity".to_owned(),
                path: PathBuf::from(&job.reflectivity_path),
            },
            TexturePathInput {
                role: "lighting".to_owned(),
                path: PathBuf::from(&job.lighting_path),
            },
        ],
        outputs: vec![
            TexturePathOutput {
                role: "diffuse".to_owned(),
                path: path_with_suffix(&output_base, "_d.dds"),
                format: "BC7_UNORM".to_owned(),
            },
            TexturePathOutput {
                role: "normal".to_owned(),
                path: path_with_suffix(&output_base, "_n.dds"),
                format: "BC5_UNORM".to_owned(),
            },
            TexturePathOutput {
                role: "specular".to_owned(),
                path: path_with_suffix(&output_base, "_s.dds"),
                format: "BC5_UNORM".to_owned(),
            },
            TexturePathOutput {
                role: "glow".to_owned(),
                path: path_with_suffix(&output_base, "_g.dds"),
                format: "BC7_UNORM".to_owned(),
            },
        ],
        params: params.conv_params,
        use_gpu: params.use_gpu,
        gpu_min_pixels: params.gpu_min_pixels,
        parallel_compression: false,
    })
}

pub fn run_texture_engine(
    params: &TextureEngineParams,
    cancel: &AtomicBool,
    progress: Option<&(dyn Fn(u64, u64) + Sync)>,
) -> Result<TextureEngineReport, String> {
    let source_game = params.source_game.to_ascii_lowercase();
    let is_gamebryo = matches!(source_game.as_str(), "fnv" | "fo3" | "skyrim" | "skyrimse");
    if (params.source_game != "fo76" && !is_gamebryo) || params.target_game != "fo4" {
        return Err(format!(
            "texture_engine supports fo76/fnv/fo3/skyrimse -> fo4 only (got {}->{})",
            params.source_game, params.target_game
        ));
    }
    let started = Instant::now();
    let source_suffixes = game_texture_suffixes(&source_game);
    let target_suffixes = game_texture_suffixes("fo4");

    // 1. Input set — mirror of the legacy phase.
    let mut entries: Vec<TextureEntry> = if params.convert_all {
        enumerate_source_textures(&params.source_extracted)
            .into_iter()
            .map(|source_path| TextureEntry {
                source_path,
                output_subpath: None,
            })
            .collect()
    } else {
        params
            .textures
            .iter()
            .map(|e| TextureEntry {
                source_path: e.source_path.clone(),
                output_subpath: e.output_subpath.clone(),
            })
            .collect()
    };

    // 2. Relocation union + forced FO76 output subpaths — verbatim port of
    //    the legacy phase.
    if !params.namespace.is_empty() && !params.relocation_members.is_empty() {
        for entry in entries.iter_mut() {
            let key = crate::relocation::member_key_for_source_path(
                &entry.source_path,
                &params.source_extracted,
            );
            if params.relocation_members.contains(&key) {
                entry.output_subpath = Some(crate::relocation::insert_namespace_after_root(
                    &key,
                    &params.namespace,
                ));
            }
        }
        let mut existing: HashSet<String> = entries
            .iter()
            .map(|e| {
                crate::relocation::member_key_for_source_path(
                    &e.source_path,
                    &params.source_extracted,
                )
            })
            .collect();
        for member in params.relocation_members.iter() {
            if !member.starts_with("textures/") || existing.contains(member) {
                continue;
            }
            let abs = params
                .source_extracted
                .join(member.replace('/', std::path::MAIN_SEPARATOR_STR));
            if !abs.is_file() {
                continue;
            }
            entries.push(TextureEntry {
                source_path: abs.to_string_lossy().to_string(),
                output_subpath: Some(crate::relocation::insert_namespace_after_root(
                    member,
                    &params.namespace,
                )),
            });
            existing.insert(member.clone());
        }
    }

    let mut seen_terrain_prefixes = HashSet::new();
    let terrain_jobs: Vec<TerrainTextureJob> = params
        .terrain_jobs
        .iter()
        .filter(|job| seen_terrain_prefixes.insert(job.output_prefix.to_ascii_lowercase()))
        .cloned()
        .collect();

    if entries.is_empty() && terrain_jobs.is_empty() {
        return Ok(TextureEngineReport::default());
    }

    // 3. Buckets + work items — same plumbing as the legacy phase.
    let output_dir = params.data_root.join("Textures");
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("create {}: {e}", output_dir.display()))?;
    let buckets = bucket_textures_by_output_subdir(&entries, &params.source_extracted);
    let all_items = build_texture_work_items(
        &buckets,
        &params.source_extracted,
        source_suffixes,
        &params.source_game,
        &output_dir,
    );

    let work_items = all_items;

    let total = (work_items.len() + terrain_jobs.len()) as u64;
    let gpu = if params.use_gpu {
        GpuService::start(params.gpu_queue_cap.max(1))
    } else {
        GpuService::start_cpu_only()
    };
    let done = AtomicU64::new(0);

    let texture_output_sink = params.sink.as_ref().map(|sink| TextureOutputSink {
        data_root: &params.data_root,
        sink: sink.as_ref(),
    });

    let register_paths_with_sink =
        |paths: Vec<&std::path::Path>, outcome: &mut GroupOutcome, context: &str| {
            let Some(sink) = &params.sink else {
                return;
            };
            for path in paths {
                if !path.is_file() {
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&params.data_root) else {
                    continue;
                };
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                if let Err(err) = sink.add_existing_file(&rel_str, path) {
                    outcome.failed += 1;
                    outcome.error_messages.push(format!(
                        "texture sink registration failed {context}: {rel_str}: {err}"
                    ));
                }
            }
        };

    let process_request = |request: &TextureSetPathRequest,
                           group_label: &str,
                           pbr_carry: bool,
                           terrain: bool| {
        let mut outcome = GroupOutcome::default();
        if cancel.load(Ordering::Relaxed) {
            return outcome;
        }
        let mut filtered_request = None;
        if group_is_base_owned(
            &request.outputs,
            &params.data_root,
            &params.target_dirs,
            params.target_assets.as_deref(),
        ) {
            let mut missing_identity_outputs = request.outputs.clone();
            missing_identity_outputs.retain(|output| {
                !output_exists_in_target_game(
                    &output.path,
                    &params.data_root,
                    &params.target_dirs,
                    params.target_assets.as_deref(),
                ) && request.inputs.iter().any(|input| {
                    input.role == output.role
                        && input
                            .path
                            .file_name()
                            .zip(output.path.file_name())
                            .is_some_and(|(input_name, output_name)| {
                                input_name
                                    .to_string_lossy()
                                    .eq_ignore_ascii_case(&output_name.to_string_lossy())
                            })
                })
            });
            outcome.skipped_base_owned_groups += 1;
            outcome.skipped_base_owned_outputs += request
                .outputs
                .len()
                .saturating_sub(missing_identity_outputs.len())
                as u64;
            if missing_identity_outputs.is_empty() {
                return outcome;
            }
            let mut request_with_missing_dependencies = request.clone();
            request_with_missing_dependencies.outputs = missing_identity_outputs;
            filtered_request = Some(request_with_missing_dependencies);
        }
        let request = filtered_request.as_ref().unwrap_or(request);
        let tasks = if terrain {
            triage_terrain_request(request, &params.format_overrides)
        } else {
            triage_request(request, &params.format_overrides, pbr_carry)
        };
        let expected_output_paths: Vec<&Path> = if pbr_carry {
            tasks.iter().flat_map(TextureTask::output_paths).collect()
        } else {
            request
                .outputs
                .iter()
                .map(|output| output.path.as_path())
                .collect()
        };

        if params.skip_existing && expected_output_paths.iter().all(|path| path.is_file()) {
            let n = expected_output_paths.len() as u64;
            outcome.outputs_written += n;
            outcome.skipped_existing += n;
            register_paths_with_sink(expected_output_paths, &mut outcome, group_label);
            return outcome;
        }

        for task in tasks {
            let is_residue = matches!(task, TextureTask::SingleResidue { .. });
            match task.class() {
                TriageClass::PassThrough => outcome.pass_through += 1,
                TriageClass::PerTexel => outcome.per_texel += 1,
                TriageClass::BundleRecompile => {
                    if is_residue {
                        outcome.legacy_residue += 1;
                    } else {
                        outcome.bundle += 1;
                    }
                }
            }
            let task_label = texture_task_label(&task);
            let needs_existing_register = matches!(task, TextureTask::SingleResidue { .. });
            let task_started = std::time::Instant::now();
            let task_result = catch_unwind(AssertUnwindSafe(|| {
                executors::execute_task_with_landscape_mip_flooding(
                    &task,
                    params.conv_params,
                    &gpu,
                    params.use_gpu,
                    params.gpu_min_pixels,
                    params.landscape_mip_flooding,
                    texture_output_sink.as_ref(),
                )
            }));
            let task_ms = u64::try_from(task_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            match task.class() {
                TriageClass::PassThrough => outcome.pass_through_ms += task_ms,
                TriageClass::PerTexel => outcome.per_texel_ms += task_ms,
                TriageClass::BundleRecompile => {
                    if is_residue {
                        outcome.legacy_residue_ms += task_ms;
                    } else {
                        outcome.bundle_ms += task_ms;
                    }
                }
            }
            match task_result {
                Ok(Ok((written, skipped, demoted, mip_flooded))) => {
                    outcome.outputs_written += u64::from(written);
                    outcome.mip_flooded_outputs += u64::from(mip_flooded);
                    outcome.failed += u64::from(skipped);
                    if skipped > 0 {
                        outcome.error_messages.push(format!(
                            "texture task skipped {skipped} output(s): {task_label}"
                        ));
                    }
                    if demoted {
                        outcome.per_texel_demoted += 1;
                    }
                    if needs_existing_register || demoted {
                        register_paths_with_sink(task.output_paths(), &mut outcome, &task_label);
                    }
                }
                Ok(Err(error)) => {
                    outcome.failed += 1;
                    outcome
                        .error_messages
                        .push(format!("texture task failed {task_label}: {error}"));
                }
                Err(payload) => {
                    outcome.failed += 1;
                    outcome.error_messages.push(format!(
                        "texture task panicked {task_label}: {}",
                        panic_payload_to_string(&*payload)
                    ));
                }
            }
        }
        outcome
    };

    let process = |item: &TextureWorkItem| -> GroupOutcome {
        let mut outcome = GroupOutcome::default();
        let group_label = texture_group_label(item);
        if cancel.load(Ordering::Relaxed) {
            return outcome;
        }
        if let Err(err) = std::fs::create_dir_all(&item.output_dir) {
            outcome.failed += item.group.files.len() as u64;
            outcome.error_messages.push(format!(
                "texture group failed {group_label}: create {}: {err}",
                item.output_dir.display()
            ));
            return outcome;
        }
        let Some(request) = build_request(
            &item.group,
            &item.output_dir,
            &params.source_game,
            "fo4",
            source_suffixes,
            target_suffixes,
            &params.format_overrides,
            params.conv_params,
            params.use_gpu,
            params.gpu_min_pixels,
        ) else {
            outcome.no_request_groups += 1;
            return outcome;
        };
        process_request(&request, &group_label, params.pbr_carry, false)
    };

    let process_terrain = |job: &TerrainTextureJob| -> GroupOutcome {
        let label = format!("terrain bundle {}", job.output_prefix);
        match terrain_texture_request(job, params) {
            Ok(request) => process_request(&request, &label, false, true),
            Err(error) => GroupOutcome {
                failed: 1,
                error_messages: vec![format!("texture job failed {label}: {error}")],
                ..Default::default()
            },
        }
    };

    let run_all = || -> Vec<GroupOutcome> {
        let regular = work_items.par_iter().map(|item| {
            let outcome = match catch_unwind(AssertUnwindSafe(|| process(item))) {
                Ok(outcome) => outcome,
                Err(payload) => {
                    let mut outcome = GroupOutcome::default();
                    outcome.failed = item.group.files.len().max(1) as u64;
                    outcome.error_messages.push(format!(
                        "texture group panicked {}: {}",
                        texture_group_label(item),
                        panic_payload_to_string(&*payload)
                    ));
                    outcome
                }
            };
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(cb) = progress {
                cb(n, total);
            }
            outcome
        });
        let terrain = terrain_jobs.par_iter().map(|job| {
            let outcome = match catch_unwind(AssertUnwindSafe(|| process_terrain(job))) {
                Ok(outcome) => outcome,
                Err(payload) => GroupOutcome {
                    failed: 1,
                    error_messages: vec![format!(
                        "terrain texture job panicked {}: {}",
                        job.output_prefix,
                        panic_payload_to_string(&*payload)
                    )],
                    ..Default::default()
                },
            };
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(cb) = progress {
                cb(n, total);
            }
            outcome
        });
        regular.chain(terrain).collect()
    };
    let outcomes: Vec<GroupOutcome> = if let Some(workers) = params.workers.filter(|w| *w > 0) {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|e| format!("texture engine pool: {e}"))?
            .install(run_all)
    } else {
        run_all()
    };

    if cancel.load(Ordering::Relaxed) {
        return Err("texture engine cancelled".to_string());
    }

    let mut report = TextureEngineReport {
        groups: total,
        elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        gpu: gpu.stats(),
        ..Default::default()
    };
    for o in outcomes {
        report.outputs_written += o.outputs_written;
        report.failed += o.failed;
        report.skipped_existing += o.skipped_existing;
        report.skipped_base_owned_groups += o.skipped_base_owned_groups;
        report.skipped_base_owned_outputs += o.skipped_base_owned_outputs;
        report.no_request_groups += o.no_request_groups;
        report.mip_flooded_outputs += o.mip_flooded_outputs;
        report.errors.extend(o.error_messages);
        report.class.pass_through += o.pass_through;
        report.class.per_texel += o.per_texel;
        report.class.per_texel_demoted += o.per_texel_demoted;
        report.class.bundle += o.bundle;
        report.class.legacy_residue += o.legacy_residue;
        report.class.pass_through_ms += o.pass_through_ms;
        report.class.per_texel_ms += o.per_texel_ms;
        report.class.bundle_ms += o.bundle_ms;
        report.class.legacy_residue_ms += o.legacy_residue_ms;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    fn write_tex(dir: &Path, rel: &str, w: u32, h: u32, format: &str, mips: bool) {
        let p = dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let rgba: Vec<u8> = (0..(w as usize) * (h as usize) * 4)
            .map(|i| (i % 256) as u8)
            .collect();
        directxtex_native::write_dds_rgba_image(&p, w, h, &rgba, format, mips).unwrap();
    }

    fn write_uniform_tex(dir: &Path, rel: &str, rgba: [u8; 4]) {
        let path = dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pixels = rgba.repeat(16 * 16);
        directxtex_native::write_dds_rgba_image(&path, 16, 16, &pixels, "R8G8B8A8_UNORM", true)
            .unwrap();
    }

    fn base_params(source: &Path, data_root: &Path) -> TextureEngineParams {
        TextureEngineParams {
            source_extracted: source.to_path_buf(),
            data_root: data_root.to_path_buf(),
            source_game: "fo76".to_string(),
            target_game: "fo4".to_string(),
            textures: Vec::new(),
            terrain_jobs: Vec::new(),
            convert_all: false,
            pbr_carry: false,
            landscape_mip_flooding: false,
            relocation_members: HashSet::new(),
            namespace: String::new(),
            target_dirs: Vec::new(),
            target_assets: None,
            skip_existing: false,
            use_gpu: false,
            gpu_min_pixels: 512 * 512,
            gpu_queue_cap: 8,
            workers: Some(2),
            conv_params: materials_native::texture_convert::TextureConversionParamsPayload::default(
            ),
            format_overrides: HashMap::new(),
            sink: None,
        }
    }

    fn run(params: &TextureEngineParams) -> TextureEngineReport {
        let cancel = AtomicBool::new(false);
        run_texture_engine(params, &cancel, None).unwrap()
    }

    #[test]
    fn convert_all_enumerates_and_counts_classes() {
        let tmp = std::env::temp_dir().join("engine_convert_all");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        write_tex(&source, "Textures/A/rock_d.dds", 16, 16, "BC7_UNORM", true); // PassThrough
        write_tex(&source, "Textures/A/rock_n.dds", 16, 16, "BC7_UNORM", true); // PerTexel
        write_tex(&source, "Textures/B/kit_d.dds", 16, 16, "BC7_UNORM", true); // Bundle (d+r+l)
        write_tex(&source, "Textures/B/kit_r.dds", 16, 16, "BC7_UNORM", true);
        write_tex(&source, "Textures/B/kit_l.dds", 16, 16, "BC7_UNORM", true);
        write_tex(&source, "Textures/C/loose.dds", 16, 16, "BC7_UNORM", true); // bare FO76 diffuse

        let mut params = base_params(&source, &tmp.join("mod").join("data"));
        params.convert_all = true;
        let report = run(&params);

        assert_eq!(report.class.pass_through, 2);
        assert_eq!(report.class.per_texel, 1);
        assert_eq!(report.class.bundle, 1);
        assert_eq!(report.no_request_groups, 0);
        assert_eq!(report.failed, 0);
        // PassThrough(_d) + PerTexel(_n) + bundle d/s/g... glow only when _l has
        // an alpha-emitting output — count written files on disk instead:
        assert!(
            tmp.join("mod/data/Textures/A/rock_d.dds".replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file()
        );
        assert!(
            tmp.join("mod/data/Textures/A/rock_n.dds".replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file()
        );
        assert!(
            tmp.join("mod/data/Textures/B/kit_d.dds".replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file()
        );
        assert!(
            tmp.join("mod/data/Textures/B/kit_s.dds".replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file()
        );
        assert!(
            tmp.join("mod/data/Textures/C/loose.dds".replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn engine_preserves_named_glow_color_from_lighting_rgb() {
        let tmp = std::env::temp_dir().join("engine_named_glow_rgb");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        write_uniform_tex(
            &source,
            "Textures/Actors/Wendigo/wendigo_glow_d.dds",
            [128, 128, 128, 255],
        );
        write_uniform_tex(
            &source,
            "Textures/Actors/Wendigo/wendigo_glow_r.dds",
            [0, 0, 0, 255],
        );
        write_uniform_tex(
            &source,
            "Textures/Actors/Wendigo/wendigo_glow_l.dds",
            [64, 255, 0, 128],
        );

        let mut params = base_params(&source, &tmp.join("mod").join("data"));
        params.convert_all = true;
        let report = run(&params);

        assert_eq!(report.failed, 0);
        let output = tmp.join(
            "mod/data/Textures/Actors/Wendigo/wendigo_glow_g.dds"
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let image = directxtex_native::read_dds_float_rgba_image(&output).unwrap();
        let expected = [(64.0 / 255.0) * (128.0 / 255.0), 128.0 / 255.0, 0.0];
        for (actual, expected) in image.rgba[..3].iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.01,
                "expected {expected}, got {actual}"
            );
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn terrain_jobs_preserve_relocated_generic_outputs() {
        let tmp = std::env::temp_dir().join("engine_terrain_jobs");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        for suffix in ["d", "n", "r", "l"] {
            write_tex(
                &source,
                &format!("Textures/Land/soil_{suffix}.dds"),
                16,
                16,
                "R8G8B8A8_UNORM",
                true,
            );
        }
        let input = |suffix: &str| {
            source
                .join("Textures")
                .join("Land")
                .join(format!("soil_{suffix}.dds"))
                .to_string_lossy()
                .into_owned()
        };
        let data_root = tmp.join("mod").join("data");
        let mut params = base_params(&source, &data_root);
        params.convert_all = true;
        params.namespace = "FO76".to_owned();
        for suffix in ["d", "n", "r", "l"] {
            params
                .relocation_members
                .insert(format!("textures/land/soil_{suffix}.dds"));
        }
        params.terrain_jobs = vec![TerrainTextureJob {
            diffuse_path: input("d"),
            normal_path: input("n"),
            reflectivity_path: input("r"),
            lighting_path: input("l"),
            output_prefix: "textures/terrain/appalachia/Soil".to_owned(),
        }];

        let report = run(&params);

        assert_eq!(report.groups, 2);
        assert_eq!(report.skipped_terrain_groups, 0);
        assert_eq!(report.class.bundle, 2);
        assert_eq!(report.failed, 0, "{:?}", report.errors);
        assert_eq!(report.outputs_written, 8);
        for suffix in ["d", "n", "s", "g"] {
            assert!(
                data_root
                    .join("Textures")
                    .join("FO76")
                    .join("Land")
                    .join(format!("soil_{suffix}.dds"))
                    .is_file()
            );
            assert!(
                data_root
                    .join("textures")
                    .join("terrain")
                    .join("appalachia")
                    .join(format!("Soil_{suffix}.dds"))
                    .is_file()
            );
        }
        let diffuse = directxtex_native::read_dds_rgba_image(
            &data_root
                .join("textures")
                .join("terrain")
                .join("appalachia")
                .join("Soil_d.dds"),
        )
        .unwrap();
        assert!(diffuse.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn landscape_mip_flooding_only_rewrites_landscape_diffuse() {
        let tmp = std::env::temp_dir().join("engine_landscape_mip_flooding");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        let mut rgba = vec![0u8; 4 * 4 * 4];
        let seed = (2 * 4 + 2) * 4;
        rgba[seed..seed + 4].copy_from_slice(&[220, 40, 10, 128]);
        for rel in [
            "Textures/Landscape/Grass/grass_d.dds",
            "Textures/Effects/grass_d.dds",
        ] {
            let path = source.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            directxtex_native::write_dds_rgba_image(&path, 4, 4, &rgba, "R8G8B8A8_UNORM", true)
                .unwrap();
        }

        let data_root = tmp.join("mod").join("data");
        let mut params = base_params(&source, &data_root);
        params.convert_all = true;
        params.landscape_mip_flooding = true;
        let report = run(&params);

        assert_eq!(report.failed, 0);
        assert_eq!(report.mip_flooded_outputs, 1);
        let landscape = directxtex_native::read_dds_mips_rgba8(
            &data_root.join("Textures/Landscape/Grass/grass_d.dds"),
        )
        .unwrap();
        assert!(
            landscape.mips[0]
                .2
                .chunks_exact(4)
                .all(|pixel| pixel[..3] == [220, 40, 10])
        );
        assert_eq!(landscape.mips[0].2[seed + 3], 128);
        assert_eq!(landscape.mips[0].2[3], 0);
        assert!(
            landscape.mips[1]
                .2
                .chunks_exact(4)
                .any(|pixel| pixel[..3] == [220, 40, 10])
        );

        let effects =
            directxtex_native::read_dds_mips_rgba8(&data_root.join("Textures/Effects/grass_d.dds"))
                .unwrap();
        assert_eq!(effects.mips[0].2, rgba);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn landscape_mip_flooding_promotes_bc1_cutout_to_bc3() {
        let tmp = std::env::temp_dir().join("engine_landscape_mip_flooding_bc1");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        let path = source.join(
            "Textures/Landscape/Grass/grass_d.dds".replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut rgba = vec![0u8; 8 * 8 * 4];
        rgba[..4].copy_from_slice(&[40, 180, 70, 255]);
        directxtex_native::write_dds_rgba_image(&path, 8, 8, &rgba, "BC1_UNORM_SRGB", true)
            .unwrap();

        let data_root = tmp.join("mod").join("data");
        let mut params = base_params(&source, &data_root);
        params.convert_all = true;
        params.landscape_mip_flooding = true;
        let report = run(&params);

        assert_eq!(report.failed, 0);
        assert_eq!(report.mip_flooded_outputs, 1);
        let output = directxtex_native::read_dds_mips_rgba8(
            &data_root.join("Textures/Landscape/Grass/grass_d.dds"),
        )
        .unwrap();
        assert_eq!(output.dxgi_format, 78);
        assert!(
            output.mips[0]
                .2
                .chunks_exact(4)
                .any(|pixel| { pixel[3] == 0 && pixel[..3].iter().any(|channel| *channel != 0) })
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn relocation_member_absent_from_list_converts_into_namespace() {
        // Port of texture_phase_converts_relocation_member_absent_from_params
        // at engine level.
        let tmp = std::env::temp_dir().join("engine_relocation");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        write_tex(
            &source,
            "Textures/Landscape/Rocks/rock_d.dds",
            8,
            8,
            "R8G8B8A8_UNORM",
            false,
        );

        let mut params = base_params(&source, &tmp.join("mod").join("data"));
        // textures intentionally EMPTY — the member must still convert.
        params.namespace = "FO76".to_string();
        params
            .relocation_members
            .insert("textures/landscape/rocks/rock_d.dds".to_string());
        let report = run(&params);

        assert!(report.outputs_written >= 1);
        fn any_dds_under(dir: &Path) -> bool {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return false;
            };
            rd.flatten().any(|e| {
                let p = e.path();
                if p.is_dir() {
                    any_dds_under(&p)
                } else {
                    p.extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("dds"))
                        .unwrap_or(false)
                }
            })
        }
        let fo76_dir = tmp.join("mod").join("data").join("Textures").join("FO76");
        assert!(
            any_dds_under(&fo76_dir),
            "expected a relocated output under {}",
            fo76_dir.display()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn base_owned_skip_is_diffuse_keyed() {
        // Production fix carry: diffuse exists in target -> whole group skipped,
        // even though the synthesized _g/_s outputs do NOT exist in target.
        let tmp = std::env::temp_dir().join("engine_base_owned");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        for n in ["kit_d.dds", "kit_r.dds", "kit_l.dds"] {
            write_tex(
                &source,
                &format!("Textures/Kit/{n}"),
                16,
                16,
                "BC7_UNORM",
                true,
            );
        }
        let target = tmp.join("fo4");
        write_tex(&target, "Textures/Kit/kit_d.dds", 4, 4, "BC1_UNORM", true);

        let mut params = base_params(&source, &tmp.join("mod").join("data"));
        params.convert_all = true;
        params.target_dirs = vec![target.clone()];
        let report = run(&params);

        assert_eq!(report.skipped_base_owned_groups, 1);
        assert_eq!(report.outputs_written, 0);
        assert!(
            !tmp.join(
                "mod/data/Textures/Kit/kit_s.dds".replace('/', std::path::MAIN_SEPARATOR_STR)
            )
            .exists(),
            "no synthesized output may clobber-adjacent a base-owned diffuse"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn base_owned_group_recovers_missing_identity_normal() {
        let tmp = std::env::temp_dir().join("engine_base_owned_missing_normal");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        for name in ["leaves_d.dds", "leaves_n.dds"] {
            write_tex(
                &source,
                &format!("Textures/Effects/{name}"),
                16,
                16,
                "BC7_UNORM",
                true,
            );
        }
        let target = tmp.join("fo4");
        write_tex(
            &target,
            "Textures/Effects/leaves_d.dds",
            4,
            4,
            "BC1_UNORM",
            true,
        );

        let data_root = tmp.join("mod").join("data");
        let mut params = base_params(&source, &data_root);
        params.convert_all = true;
        params.target_dirs = vec![target];
        let report = run(&params);

        assert_eq!(report.skipped_base_owned_groups, 1);
        assert_eq!(report.skipped_base_owned_outputs, 1);
        assert_eq!(report.outputs_written, 1);
        assert!(!data_root.join("Textures/Effects/leaves_d.dds").exists());
        assert!(data_root.join("Textures/Effects/leaves_n.dds").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn convert_all_separates_bare_diffuse_from_suffixed_bundle() {
        let tmp = std::env::temp_dir().join("engine_bare_and_suffixed_diffuse");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        for name in ["mist.dds", "mist_d.dds", "mist_n.dds"] {
            write_tex(
                &source,
                &format!("Textures/Effects/{name}"),
                16,
                16,
                "BC7_UNORM",
                true,
            );
        }
        let target = tmp.join("fo4");
        write_tex(
            &target,
            "Textures/Effects/mist.dds",
            4,
            4,
            "BC1_UNORM",
            true,
        );

        let data_root = tmp.join("mod").join("data");
        let mut params = base_params(&source, &data_root);
        params.convert_all = true;
        params.target_dirs = vec![target];
        let report = run(&params);

        assert_eq!(report.skipped_base_owned_groups, 1);
        assert_eq!(report.skipped_base_owned_outputs, 1);
        assert_eq!(report.outputs_written, 2);
        assert!(!data_root.join("Textures/Effects/mist.dds").exists());
        assert!(data_root.join("Textures/Effects/mist_d.dds").is_file());
        assert!(data_root.join("Textures/Effects/mist_n.dds").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pbr_carry_preserves_diffuse_keyed_base_owned_skip() {
        let tmp = std::env::temp_dir().join("engine_pbr_base_owned");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        for name in ["kit_d.dds", "kit_r.dds", "kit_l.dds"] {
            write_tex(
                &source,
                &format!("Textures/Kit/{name}"),
                16,
                16,
                "BC7_UNORM",
                true,
            );
        }
        let target = tmp.join("fo4");
        write_tex(&target, "Textures/Kit/kit_d.dds", 4, 4, "BC1_UNORM", true);

        let data_root = tmp.join("mod").join("data");
        let mut params = base_params(&source, &data_root);
        params.convert_all = true;
        params.pbr_carry = true;
        params.target_dirs = vec![target];
        let report = run(&params);

        assert_eq!(report.skipped_base_owned_groups, 1);
        assert_eq!(report.skipped_base_owned_outputs, 3);
        assert_eq!(report.outputs_written, 0);
        for suffix in ["d", "s", "g", "r", "l"] {
            assert!(
                !data_root
                    .join("Textures")
                    .join("Kit")
                    .join(format!("kit_{suffix}.dds"))
                    .exists(),
                "base-owned diffuse must suppress every adjacent output"
            );
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn skip_existing_counts_outputs_as_written() {
        let tmp = std::env::temp_dir().join("engine_skip_existing");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        write_tex(
            &source,
            "Textures/A/rock_d.dds",
            8,
            8,
            "R8G8B8A8_UNORM",
            false,
        );
        let data_root = tmp.join("mod").join("data");
        write_tex(
            &data_root,
            "Textures/A/rock_d.dds",
            4,
            4,
            "R8G8B8A8_UNORM",
            false,
        ); // pre-existing

        let mut params = base_params(&source, &data_root);
        params.convert_all = true;
        params.skip_existing = true;
        let report = run(&params);

        // Legacy semantics: existing outputs count as written AND as
        // skipped_existing.
        assert_eq!(report.skipped_existing, 1);
        assert_eq!(report.outputs_written, 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pbr_carry_skip_existing_requires_and_counts_sidecars() {
        let tmp = std::env::temp_dir().join("engine_pbr_skip_existing");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        for name in ["kit_d.dds", "kit_r.dds", "kit_l.dds"] {
            write_tex(
                &source,
                &format!("Textures/Kit/{name}"),
                16,
                16,
                "BC7_UNORM",
                true,
            );
        }
        let data_root = tmp.join("mod").join("data");

        let mut legacy_params = base_params(&source, &data_root);
        legacy_params.convert_all = true;
        let legacy_report = run(&legacy_params);
        assert_eq!(legacy_report.outputs_written, 3);

        let mut pbr_params = base_params(&source, &data_root);
        pbr_params.convert_all = true;
        pbr_params.pbr_carry = true;
        pbr_params.skip_existing = true;
        let carry_report = run(&pbr_params);
        assert_eq!(carry_report.skipped_existing, 0);
        assert_eq!(carry_report.outputs_written, 5);

        let reused_report = run(&pbr_params);
        assert_eq!(reused_report.skipped_existing, 5);
        assert_eq!(reused_report.outputs_written, 5);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn invalid_texture_records_error_message() {
        let tmp = std::env::temp_dir().join("engine_invalid_texture_error");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        let bad = source.join("Textures").join("Bad").join("bad_d.dds");
        std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
        std::fs::write(&bad, b"not a dds").unwrap();

        let mut params = base_params(&source, &tmp.join("mod").join("data"));
        params.convert_all = true;
        let report = run(&params);

        assert_eq!(report.outputs_written, 0);
        assert_eq!(report.failed, 1);
        assert!(
            report.errors.iter().any(|message| {
                message.contains("texture task failed") && message.contains("bad_d.dds")
            }),
            "expected bad texture path in errors: {:?}",
            report.errors
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn panic_payload_to_string_extracts_message() {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| panic!("bad texture")));
        let payload = result.unwrap_err();
        assert_eq!(panic_payload_to_string(&*payload), "bad texture");
    }

    #[test]
    fn cancel_aborts_with_error() {
        let tmp = std::env::temp_dir().join("engine_cancel");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        write_tex(
            &source,
            "Textures/A/rock_d.dds",
            8,
            8,
            "R8G8B8A8_UNORM",
            false,
        );
        let mut params = base_params(&source, &tmp.join("mod").join("data"));
        params.convert_all = true;
        let cancel = AtomicBool::new(true);
        let err = run_texture_engine(&params, &cancel, None).unwrap_err();
        assert!(err.contains("cancel"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
