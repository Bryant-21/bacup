//! Env-gated real-corpus gates. `--ignored` only.
//! Corpus root: MODBOX_FO76_EXTRACTED else <repo>/extracted/fo76. Outputs only
//! under MODBOX_PLAN5_OUT else <repo>/tmp/plan5gate. Never writes to a game
//! Data dir.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use materials_native::texture_convert::{
    TextureConversionParamsPayload, convert_texture_set_paths,
};

use crate::phase::textures::{
    TextureEntry, bucket_textures_by_output_subdir, build_request, build_texture_work_items,
    enumerate_source_textures, game_texture_suffixes,
};

use super::executors::execute_task;
use super::gpu_service::GpuService;
use super::triage::TextureTask;
use super::{TextureEngineParams, TriageClass, run_texture_engine, triage_request};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("conversion crate lives at repo/bacup/py_bacup_lib/native/conversion")
        .to_path_buf()
}

fn fo76_dir() -> PathBuf {
    std::env::var("MODBOX_FO76_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("extracted").join("fo76"))
}

fn fo4_dir() -> PathBuf {
    std::env::var("MODBOX_FO4_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("extracted").join("fo4"))
}

fn out_root() -> PathBuf {
    std::env::var("MODBOX_PLAN5_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("tmp").join("plan5gate"))
}

fn corpus_or_skip() -> Option<PathBuf> {
    let src = fo76_dir();
    if src.join("Textures").is_dir() {
        Some(src)
    } else {
        eprintln!("skip: FO76 extracted dir absent at {}", src.display());
        None
    }
}

fn rmse(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "buffer size mismatch");
    let sum: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = f64::from(*x) - f64::from(*y);
            d * d
        })
        .sum();
    (sum / a.len() as f64).sqrt()
}

fn task_quota_key(task: &TextureTask) -> &'static str {
    match task {
        TextureTask::Single {
            class: TriageClass::PassThrough,
            ..
        } => "pass_through",
        TextureTask::Single { .. } => "per_texel",
        TextureTask::Bundle { .. } => "bundle",
        TextureTask::SpecGloss { .. } => "specgloss",
        TextureTask::SingleResidue { .. } => "residue",
    }
}

#[test]
#[ignore]
fn sample_equivalence_vs_legacy() {
    let Some(src) = corpus_or_skip() else { return };
    let out = out_root().join("sample");
    let _ = std::fs::remove_dir_all(&out);
    let legacy_base = out.join("legacy").join("Textures");
    let new_base = out.join("new").join("Textures");

    let entries: Vec<TextureEntry> = enumerate_source_textures(&src)
        .into_iter()
        .map(|source_path| TextureEntry {
            source_path,
            output_subpath: None,
        })
        .collect();
    let buckets = bucket_textures_by_output_subdir(&entries, &src);
    let items = build_texture_work_items(
        &buckets,
        &src,
        game_texture_suffixes("fo76"),
        "fo76",
        &new_base,
    );
    eprintln!("corpus: {} entries, {} groups", entries.len(), items.len());

    let mut quota: HashMap<&'static str, usize> = HashMap::from([
        ("pass_through", 100),
        ("per_texel", 100),
        ("bundle", 60),
        ("specgloss", 25),
        ("residue", 25),
    ]);
    let overrides: HashMap<String, String> = HashMap::new();
    let svc = GpuService::start_cpu_only();
    let started = Instant::now();
    let mut sampled_groups = 0usize;
    let mut checks = 0usize;

    for item in &items {
        if quota.values().all(|v| *v == 0) {
            break;
        }
        let Some(new_req) = build_request(
            &item.group,
            &item.output_dir,
            "fo76",
            "fo4",
            game_texture_suffixes("fo76"),
            game_texture_suffixes("fo4"),
            &overrides,
            TextureConversionParamsPayload::default(),
            false,
            0,
        ) else {
            continue;
        };
        let tasks = triage_request(&new_req, &overrides, false);
        let wanted: Vec<&TextureTask> = tasks
            .iter()
            .filter(|t| quota.get(task_quota_key(t)).copied().unwrap_or(0) > 0)
            .collect();
        if wanted.is_empty() {
            continue;
        }
        // Legacy twin only needed for non-PassThrough checks (PassThrough is
        // compared against the SOURCE).
        let needs_legacy = wanted.iter().any(|t| {
            !matches!(
                t,
                TextureTask::Single {
                    class: TriageClass::PassThrough,
                    ..
                }
            )
        });
        let rel = item
            .output_dir
            .strip_prefix(&new_base)
            .expect("under new_base");
        let legacy_dir = legacy_base.join(rel);
        if needs_legacy {
            let legacy_req = build_request(
                &item.group,
                &legacy_dir,
                "fo76",
                "fo4",
                game_texture_suffixes("fo76"),
                game_texture_suffixes("fo4"),
                &overrides,
                TextureConversionParamsPayload::default(),
                false,
                0,
            )
            .expect("legacy twin request");
            std::fs::create_dir_all(&legacy_dir).unwrap();
            if convert_texture_set_paths(legacy_req).is_err() {
                continue; // legacy can't convert it either — not an equivalence sample
            }
        }
        std::fs::create_dir_all(&item.output_dir).unwrap();

        for task in wanted {
            let key = task_quota_key(task);
            let q = quota.get_mut(key).expect("known key");
            if *q == 0 {
                continue;
            }
            execute_task(
                task,
                TextureConversionParamsPayload::default(),
                &svc,
                false,
                0,
                None,
            )
            .unwrap_or_else(|e| panic!("engine failed on {key}: {e}"));
            match task {
                TextureTask::Single {
                    input,
                    output,
                    class: TriageClass::PassThrough,
                    ..
                } => {
                    assert_eq!(
                        std::fs::read(&output.path).unwrap(),
                        std::fs::read(&input.path).unwrap(),
                        "PassThrough must equal source: {}",
                        input.path.display()
                    );
                }
                TextureTask::Single { input, output, .. } => {
                    let legacy_path = legacy_dir.join(output.path.file_name().unwrap());
                    let l = directxtex_native::read_dds_mips_rgba8(&legacy_path).unwrap();
                    let n = directxtex_native::read_dds_mips_rgba8(&output.path).unwrap();
                    assert_eq!(n.mips.len(), l.mips.len(), "{}", input.path.display());
                    assert_eq!(
                        n.mips[0].2,
                        l.mips[0].2,
                        "mip0 must match legacy exactly: {}",
                        input.path.display()
                    );
                    for k in 1..n.mips.len() {
                        let e = rmse(&n.mips[k].2, &l.mips[k].2);
                        assert!(
                            e <= 24.0,
                            "mip {k} RMSE {e:.2} > 24 for {}",
                            input.path.display()
                        );
                    }
                }
                TextureTask::Bundle { .. }
                | TextureTask::SpecGloss { .. }
                | TextureTask::SingleResidue { .. } => {
                    for out_path in task.output_paths() {
                        let legacy_path = legacy_dir.join(out_path.file_name().unwrap());
                        assert_eq!(
                            std::fs::read(out_path).unwrap(),
                            std::fs::read(&legacy_path).unwrap(),
                            "recompiled output must byte-match legacy: {}",
                            out_path.display()
                        );
                    }
                }
            }
            *q -= 1;
            checks += 1;
        }
        sampled_groups += 1;
        if sampled_groups % 25 == 0 {
            eprintln!(
                "[{}s] sampled {sampled_groups} groups, {checks} checks, quotas {quota:?}",
                started.elapsed().as_secs()
            );
        }
    }
    svc.shutdown();
    eprintln!(
        "DONE: {sampled_groups} groups, {checks} checks in {}s; residual quotas {quota:?}",
        started.elapsed().as_secs()
    );
    assert!(
        quota["pass_through"] < 100 && quota["per_texel"] < 100 && quota["bundle"] < 60,
        "corpus must exercise the three main classes — triage may be misrouting"
    );
}

#[test]
#[ignore]
fn full_corpus_timed() {
    let Some(src) = corpus_or_skip() else { return };
    let fo4 = fo4_dir();
    let out = std::env::var("MODBOX_PLAN5_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| out_root().join("corpus_out"));
    assert!(
        !out.join("data").exists(),
        "clean {} before a timed run (stale outputs distort timing and trip skip_existing-free overwrites)",
        out.display()
    );

    // Real relocation member set — the same collision compare run-init uses.
    let roots: Vec<String> = crate::relocation::FO76_FO4_DEFAULT_RELOCATION_MESH_ROOTS
        .iter()
        .map(|s| s.to_string())
        .collect();
    let reloc = crate::relocation::build_relocation_member_set(&roots, &src, &fo4);
    for w in &reloc.warnings {
        eprintln!("reloc warning: {w}");
    }
    let members = reloc.members;
    eprintln!("relocation members: {}", members.len());

    let use_gpu = std::env::var("MODBOX_PLAN5_CPU").ok().as_deref() != Some("1");
    let params = TextureEngineParams {
        source_extracted: src.clone(),
        data_root: out.join("data"),
        source_game: "fo76".to_string(),
        target_game: "fo4".to_string(),
        textures: Vec::new(),
        terrain_jobs: Vec::new(),
        convert_all: true,
        pbr_carry: false,
        landscape_mip_flooding: false,
        relocation_members: members.clone(),
        namespace: "FO76".to_string(),
        target_dirs: if fo4.is_dir() {
            vec![fo4.clone()]
        } else {
            Vec::new()
        },
        target_assets: None,
        skip_existing: false,
        use_gpu,
        gpu_min_pixels: 512 * 512,
        gpu_queue_cap: 8,
        workers: std::env::var("MODBOX_PLAN5_WORKERS")
            .ok()
            .and_then(|v| v.parse().ok()),
        conv_params: TextureConversionParamsPayload::default(),
        format_overrides: HashMap::new(),
        sink: None,
    };

    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let progress = |done: u64, total: u64| {
        if done % 5000 == 0 || done == total {
            eprintln!("[{}s] {done}/{total} groups", started.elapsed().as_secs());
        }
    };
    let report = run_texture_engine(&params, &cancel, Some(&progress)).expect("engine run");
    let textures_secs = started.elapsed().as_secs();

    // Materials fold-in, timed informationally (small corpus).
    let m_started = Instant::now();
    let m_report =
        super::materials::run_materials_engine(super::materials::MaterialsEngineParams {
            mod_path: out.clone(),
            source_extracted: src.clone(),
            target_extracted: if fo4.is_dir() {
                Some(fo4.clone())
            } else {
                None
            },
            target_data_dir: None,
            source_game: materials_native::convert::Game::Fo76,
            target_game: materials_native::convert::Game::Fo4,
            materials: Vec::new(),
            convert_all: true,
            pbr_carry: false,
            relocation_members: members,
            namespace: "FO76".to_string(),
            source_materialsdb: None,
            overwrite_existing: true,
            target_asset_paths: HashSet::new(),
        });
    let materials_secs = m_started.elapsed().as_secs();

    let summary = format!(
        "# Plan 5 full-corpus timed run\n\n\
         - gpu: {use_gpu}, workers: {:?}\n\
         - textures: groups={} written={} failed={} skipped_existing={} base_owned_groups={} no_request={} \n\
         - classes: pass_through={} per_texel={} per_texel_demoted={} bundle={} residue={}\n\
         - gpu stats: submissions={} dispatches={} cpu_encodes={} overflow={} failures={}\n\
         - texture wall: {}s (budget 900s) — {}\n\
         - materials: written={} warnings={} wall {}s (informational)\n",
        params.workers,
        report.groups,
        report.outputs_written,
        report.failed,
        report.skipped_existing,
        report.skipped_base_owned_groups,
        report.no_request_groups,
        report.class.pass_through,
        report.class.per_texel,
        report.class.per_texel_demoted,
        report.class.bundle,
        report.class.legacy_residue,
        report.gpu.gpu_submissions,
        report.gpu.gpu_dispatch_batches,
        report.gpu.cpu_encodes,
        report.gpu.overflow_to_cpu,
        report.gpu.gpu_failures,
        textures_secs,
        if textures_secs <= 900 { "PASS" } else { "FAIL" },
        m_report.assets_written,
        m_report.warnings,
        materials_secs,
    );
    let _ = std::fs::create_dir_all(out_root());
    let _ = std::fs::write(out_root().join("REPORT.md"), &summary);
    eprintln!("{summary}");

    assert!(
        (report.failed as f64) <= (report.groups.max(1) as f64) * 0.01,
        "failure rate above 1% ({} of {}) — investigate before gating",
        report.failed,
        report.groups
    );
    assert!(
        textures_secs <= 900,
        "texture corpus took {textures_secs}s > 900s budget"
    );
}
