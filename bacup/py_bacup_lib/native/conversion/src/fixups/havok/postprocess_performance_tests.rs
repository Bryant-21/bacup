use std::path::Path;
use std::time::{Duration, Instant};

use havok_native::hkx::descriptors::DescriptorRegistry;
use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxFile, HkxMember, HkxObject, write_hkx};

use crate::fixups::FixupReport;
use crate::full_plugin::{FixupScope, FixupStatus};
use crate::sym::Sym;

use super::inject_attack_stop_events::inject_attack_stop_events_in_mod_path;
use super::inject_hitframe_events::inject_hitframe_events_in_mod_path;
use super::retime_reload_complete_events::retime_reload_complete_events_in_mod_path;
use super::strip_source_game_events::{
    strip_source_game_events_in_mod_path, strip_source_game_events_without_prefilter_in_mod_path,
};

#[derive(Debug, PartialEq, Eq)]
struct PassCounts {
    stripped: u32,
    hitframes: u32,
    attack_stops: u32,
    reloads: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct ReportSnapshot {
    records_changed: u32,
    records_dropped: u32,
    records_added: u32,
    warnings: Vec<Sym>,
    diagnostics: Vec<Sym>,
    elapsed_ms: u64,
    iteration: u32,
    status: FixupStatus,
    scope: FixupScope,
    message: Option<Sym>,
    addon_index_remap: Vec<(i64, i64)>,
}

impl From<&FixupReport> for ReportSnapshot {
    fn from(report: &FixupReport) -> Self {
        Self {
            records_changed: report.records_changed,
            records_dropped: report.records_dropped,
            records_added: report.records_added,
            warnings: report.warnings.clone(),
            diagnostics: report.diagnostics.clone(),
            elapsed_ms: report.elapsed_ms,
            iteration: report.iteration,
            status: report.status,
            scope: report.scope,
            message: report.message,
            addon_index_remap: report.addon_index_remap.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PassReports {
    strip: ReportSnapshot,
    hitframe: ReportSnapshot,
    attack_stop: ReportSnapshot,
    reload: ReportSnapshot,
}

impl PassReports {
    fn counts(&self) -> PassCounts {
        PassCounts {
            stripped: self.strip.records_changed,
            hitframes: self.hitframe.records_changed,
            attack_stops: self.attack_stop.records_changed,
            reloads: self.reload.records_changed,
        }
    }
}

#[derive(Debug)]
struct PassTimings {
    strip: Duration,
    hitframe: Duration,
    attack_stop: Duration,
    reload: Duration,
}

fn run_passes(corpus: &Path, baseline: bool) -> (PassReports, PassTimings) {
    let strip_started = Instant::now();
    let stripped = if baseline {
        strip_source_game_events_without_prefilter_in_mod_path(corpus)
    } else {
        strip_source_game_events_in_mod_path(corpus)
    }
    .unwrap();
    let strip = strip_started.elapsed();

    let hitframe_started = Instant::now();
    let hitframes = inject_hitframe_events_in_mod_path(corpus).unwrap();
    let hitframe = hitframe_started.elapsed();

    let attack_stop_started = Instant::now();
    let attack_stops = inject_attack_stop_events_in_mod_path(corpus).unwrap();
    let attack_stop = attack_stop_started.elapsed();

    let reload_started = Instant::now();
    let reloads = retime_reload_complete_events_in_mod_path(corpus).unwrap();
    let reload = reload_started.elapsed();

    (
        PassReports {
            strip: ReportSnapshot::from(&stripped),
            hitframe: ReportSnapshot::from(&hitframes),
            attack_stop: ReportSnapshot::from(&attack_stops),
            reload: ReportSnapshot::from(&reloads),
        },
        PassTimings {
            strip,
            hitframe,
            attack_stop,
            reload,
        },
    )
}

fn annotation(time: f32, text: &str) -> HkxValue {
    HkxValue::Object(vec![
        HkxMember {
            name: "time".to_string(),
            value: HkxValue::F32(time),
        },
        HkxMember {
            name: "text".to_string(),
            value: HkxValue::String {
                value: text.to_string(),
                is_null: false,
            },
        },
    ])
}

fn write_clip(path: &Path, duration: f32, annotations: Vec<HkxValue>) {
    let hkx = HkxFile::from_tagxml(
        11,
        "hk_2014.1.0-r1",
        vec![HkxObject {
            name: Some("#0001".to_string()),
            offset: 0,
            signature: 0,
            class_name: "hkaSplineCompressedAnimation".to_string(),
            members: vec![
                HkxMember {
                    name: "duration".to_string(),
                    value: HkxValue::F32(duration),
                },
                HkxMember {
                    name: "annotationTracks".to_string(),
                    value: HkxValue::Array(vec![HkxValue::Object(vec![HkxMember {
                        name: "annotations".to_string(),
                        value: HkxValue::Array(annotations),
                    }])]),
                },
            ],
        }],
    );
    let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, write_hkx(&hkx, &mut registry)).unwrap();
}

fn write_positive_corpus(root: &Path) {
    write_clip(
        &root.join("data/Meshes/Actors/Test/Animations/attackforward.hkx"),
        1.7,
        vec![
            annotation(0.2, "FO76_WeaponAttackStart"),
            annotation(0.7, "weaponSwing"),
            annotation(0.8, "preHitFrame"),
        ],
    );
    write_clip(
        &root.join("data/Meshes/Actors/Test/Animations/WPNReload.hkx"),
        1.3,
        vec![
            annotation(0.1, "FO76_SprintStop"),
            annotation(1.2, "reloadComplete"),
        ],
    );
}

fn collect_hkx_paths(root: &Path, current: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(current).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_hkx_paths(root, &path, paths);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("hkx"))
        {
            paths.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

fn assert_corpora_equal(baseline: &Path, optimized: &Path) {
    let mut baseline_paths = Vec::new();
    let mut optimized_paths = Vec::new();
    collect_hkx_paths(baseline, baseline, &mut baseline_paths);
    collect_hkx_paths(optimized, optimized, &mut optimized_paths);
    baseline_paths.sort();
    optimized_paths.sort();
    assert_eq!(baseline_paths, optimized_paths, "HKX path sets differ");
    for relative in baseline_paths {
        assert_eq!(
            std::fs::read(baseline.join(&relative)).unwrap(),
            std::fs::read(optimized.join(&relative)).unwrap(),
            "output differs for {}",
            relative.display()
        );
    }
}

#[test]
fn prefilters_preserve_positive_fixture_reports_and_bytes() {
    let baseline = tempfile::tempdir().unwrap();
    let optimized = tempfile::tempdir().unwrap();
    write_positive_corpus(baseline.path());
    write_positive_corpus(optimized.path());

    let (baseline_reports, _) = run_passes(baseline.path(), true);
    let (optimized_reports, _) = run_passes(optimized.path(), false);
    assert_eq!(baseline_reports, optimized_reports);
    let baseline_counts = baseline_reports.counts();
    assert_eq!(
        baseline_counts,
        PassCounts {
            stripped: 2,
            hitframes: 1,
            attack_stops: 1,
            reloads: 1,
        }
    );

    assert_corpora_equal(baseline.path(), optimized.path());
}

#[test]
fn prefilter_preserves_public_report_for_malformed_irrelevant_file() {
    let baseline = tempfile::tempdir().unwrap();
    let optimized = tempfile::tempdir().unwrap();
    let relative = "data/Meshes/Actors/Test/malformed.hkx";
    for root in [baseline.path(), optimized.path()] {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"malformed packfile without an event token").unwrap();
    }

    let baseline_report =
        strip_source_game_events_without_prefilter_in_mod_path(baseline.path()).unwrap();
    let optimized_report = strip_source_game_events_in_mod_path(optimized.path()).unwrap();
    assert_eq!(
        ReportSnapshot::from(&baseline_report),
        ReportSnapshot::from(&optimized_report)
    );
    assert_eq!(
        std::fs::read(baseline.path().join(relative)).unwrap(),
        std::fs::read(optimized.path().join(relative)).unwrap()
    );
}

#[test]
#[ignore = "requires isolated baseline and optimized postprocessed mod trees"]
fn benchmark_annotation_postprocess_passes() {
    let baseline = std::env::var("BACUP_HAVOK_BENCH_BASELINE_CORPUS")
        .expect("set BACUP_HAVOK_BENCH_BASELINE_CORPUS");
    let optimized = std::env::var("BACUP_HAVOK_BENCH_OPTIMIZED_CORPUS")
        .expect("set BACUP_HAVOK_BENCH_OPTIMIZED_CORPUS");
    let baseline = Path::new(&baseline);
    let optimized = Path::new(&optimized);

    let (baseline_reports, baseline_timings) = run_passes(baseline, true);
    let (optimized_reports, optimized_timings) = run_passes(optimized, false);
    assert_eq!(baseline_reports, optimized_reports);
    assert_corpora_equal(baseline, optimized);
    let baseline_counts = baseline_reports.counts();

    eprintln!(
        "havok_annotation_benchmark baseline={baseline_timings:?} optimized={optimized_timings:?} counts={baseline_counts:?}"
    );
}
