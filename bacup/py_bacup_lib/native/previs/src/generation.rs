use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rustc_hash::FxHashMap;
use serde::Serialize;

use crate::error::{PrevisError, Result};
use crate::memory::{MemoryMonitor, MemoryPeaks};
use crate::metadata::{CellMetadata, Xcri, write_patched_plugin};
use crate::plugin::Plugin;
use crate::precombine_stage::{self, IndexReport};
use crate::stage::{CellEntry, PlanReport, PlanRequest, PlanTiming};

#[derive(Debug, Default, Serialize)]
pub struct GenerationSummary {
    pub generated: usize,
    pub stamped: usize,
    pub failed: Vec<(String, String)>,
    pub skipped_unsupported: usize,
    pub skipped_invalid: usize,
    pub patched_cells: usize,
    pub report_path: PathBuf,
    pub timing_path: PathBuf,
    pub warnings: Vec<String>,
    pub index: Option<IndexReport>,
}

#[derive(Default, Serialize)]
pub struct GenerationTiming {
    pub plan: PlanTiming,
    pub report_seconds: f64,
    pub collect_metadata_seconds: f64,
    pub stamp_seconds: f64,
    pub index_seconds: f64,
    pub total_seconds: f64,
    pub memory: Option<MemoryPeaks>,
}

pub struct PreparedGeneration {
    plugin: PathBuf,
    data_dir: PathBuf,
    interior_cdx_only: bool,
    cells: FxHashMap<u32, CellMetadata>,
    summary: GenerationSummary,
    timing: GenerationTiming,
    started: Instant,
    memory: MemoryMonitor,
}

pub fn collect_metadata(entries: impl IntoIterator<Item = CellEntry>) -> Result<FxHashMap<u32, CellMetadata>> {
    let mut cells = FxHashMap::default();
    for entry in entries {
        let metadata = CellMetadata {
            previs: entry.previs,
            precombine: entry.precombine,
            root_visibility_cell: entry.root_visibility_cell,
            previs_reference_ids: entry.previs_reference_ids,
            xcri: entry.xcri.map(|x| Xcri { mesh_ids: x.mesh_ids, reference_mesh_pairs: x.reference_mesh_pairs }),
        };
        if cells.insert(entry.cell_form_id, metadata).is_some() {
            return Err(PrevisError::invalid(format!("CELL {:08X} is listed twice", entry.cell_form_id)));
        }
    }
    Ok(cells)
}

pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let io = |e| PrevisError::io(path.display().to_string(), e);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let mut writer = BufWriter::with_capacity(1 << 20, File::create(path).map_err(io)?);
    serde_json::to_writer_pretty(&mut writer, value).map_err(|e| io(std::io::Error::other(e)))?;
    writer.flush().map_err(io)
}

/// `progress(done, total)` counts finished clusters (see `stage::plan_with_progress`).
pub fn prepare(request: &PlanRequest, report_path: &Path, timing_path: &Path, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<PreparedGeneration> {
    let started = Instant::now();
    let memory = MemoryMonitor::start();
    let data_dir = request.solve_into.as_ref().ok_or_else(|| PrevisError::invalid("generation requires solve_into"))?;
    let report = crate::stage::plan_with_progress(request, progress)?;
    let mut generation = prepare_report(&request.plugin, data_dir, report, report_path, timing_path, started)?;
    generation.interior_cdx_only = request.interior_cdx_only;
    generation.memory = memory;
    Ok(generation)
}

fn prepare_report(plugin: &Path, data_dir: &Path, report: PlanReport, report_path: &Path, timing_path: &Path, started: Instant) -> Result<PreparedGeneration> {
    let report_started = Instant::now();
    write_json(report_path, &report)?;
    let mut timing = GenerationTiming { report_seconds: report_started.elapsed().as_secs_f64(), ..GenerationTiming::default() };
    let collect_started = Instant::now();
    let mut summary = GenerationSummary {
        generated: report.jobs.len(),
        stamped: report.stamps.len(),
        report_path: report_path.to_path_buf(),
        timing_path: timing_path.to_path_buf(),
        ..GenerationSummary::default()
    };
    for skip in report.skipped {
        if skip.reason.starts_with("solve failed") {
            let cell = skip.cell.ok_or_else(|| PrevisError::invalid("solve failure without CELL"))?;
            summary.warnings.push(format!("previs: {cell:08X} {}", skip.reason));
            summary.failed.push((format!("{cell:08X}"), skip.reason));
        } else if skip.unsupported {
            summary.skipped_unsupported += 1;
        } else {
            summary.skipped_invalid += 1;
            summary.warnings.push(format!("previs: invalid input skipped: {}", skip.reason));
        }
    }
    // Consuming each job releases its diagnostic references before plugin stamping.
    let entries = report.jobs.into_iter().flat_map(|job| job.cells).chain(report.stamps.into_iter().flat_map(|stamp| stamp.cells));
    let cells = collect_metadata(entries)?;
    timing.plan = report.timing;
    timing.collect_metadata_seconds = collect_started.elapsed().as_secs_f64();
    Ok(PreparedGeneration { plugin: plugin.to_path_buf(), data_dir: data_dir.to_path_buf(), interior_cdx_only: false, cells, summary, timing, started, memory: MemoryMonitor::default() })
}

impl PreparedGeneration {
    pub fn finalize(mut self, packed_date: u16) -> Result<GenerationSummary> {
        let started = Instant::now();
        if !self.cells.is_empty() {
            let mut filename = self.plugin.file_name().unwrap().to_os_string();
            filename.push(".previs.tmp");
            let output = self.plugin.with_file_name(filename);
            {
                let source = Plugin::open(&self.plugin)?;
                self.summary.patched_cells = write_patched_plugin(&source, &output, &self.cells, packed_date)?;
            }
            // Windows cannot replace the input while its source mapping is alive.
            std::fs::rename(&output, &self.plugin).map_err(|e| PrevisError::io(self.plugin.display().to_string(), e))?;
        }
        drop(self.cells);
        self.timing.stamp_seconds = started.elapsed().as_secs_f64();
        let started = Instant::now();
        self.summary.index = crate::worker_pool(None).install(|| precombine_stage::write_index(&self.plugin, &self.data_dir, self.interior_cdx_only))?;
        self.timing.index_seconds = started.elapsed().as_secs_f64();
        self.timing.total_seconds = self.started.elapsed().as_secs_f64();
        self.timing.memory = self.memory.finish();
        write_json(&self.summary.timing_path, &self.timing)?;
        Ok(self.summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::pack_generation_date;
    use crate::precombine_stage::XcriEntry;
    use crate::previs::PlannedReference;
    use crate::stage::{Job, Skip, Stamp};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("previs_generation_{name}_{}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn plugin(&self) -> PathBuf {
            let path = self.0.join("Test.esp");
            let record = |signature: &[u8; 4], id: u32, payload: &[u8]| {
                let mut bytes = signature.to_vec();
                bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&0u32.to_le_bytes());
                bytes.extend_from_slice(&id.to_le_bytes());
                bytes.extend_from_slice(&[0; 8]);
                bytes.extend_from_slice(payload);
                bytes
            };
            let mut bytes = record(b"TES4", 0, b"HEDR\x0c\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
            let cells: Vec<_> = (1..=3).flat_map(|id| record(b"CELL", id, b"DATA\x02\x00\x01\x00")).collect();
            bytes.extend_from_slice(b"GRUP");
            bytes.extend_from_slice(&((24 + cells.len()) as u32).to_le_bytes());
            bytes.extend_from_slice(b"CELL");
            bytes.extend_from_slice(&[0; 12]);
            bytes.extend_from_slice(&cells);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn entry(cell_form_id: u32) -> CellEntry {
        CellEntry { cell_form_id, previs: true, root_visibility_cell: Some(1), previs_reference_ids: vec![12, 11], precombine: false, xcri: None }
    }

    fn report() -> PlanReport {
        PlanReport {
            plugin_name: "Test.esp".to_string(),
            jobs: vec![Job {
                root_cell: 1,
                world: None,
                grid: None,
                scene_name: "Café".into(),
                scene_path: PathBuf::new(),
                scene_sha256: String::new(),
                uvd_relative: "Vis\\Test.esp\\00000001.uvd".into(),
                task_bounds: Some([0.0, 1.25, -2.5, 512.0, 1024.0, 32.0]),
                cells: vec![entry(1)],
                references: vec![PlannedReference { form_id: 12, base_form_key: "Test.esp:000003".into(), model: "Café.nif".into() }],
                non_parity: vec!["diagnostic retained".into()],
                solved: true,
            }],
            stamps: vec![Stamp { root_cell: 2, world: "World".into(), grid: (0, 0), cells: vec![entry(2)] }],
            skipped: vec![
                Skip { world: None, grid: None, cell: Some(3), unsupported: false, reason: "solve failed: bad geometry".into() },
                Skip { world: None, grid: None, cell: Some(4), unsupported: true, reason: "outside subset".into() },
                Skip { world: None, grid: None, cell: Some(5), unsupported: false, reason: "malformed input".into() },
            ],
            timing: PlanTiming::default(),
        }
    }

    #[test]
    fn native_report_preserves_diagnostics_and_stamps_only_successes() {
        let fixture = Fixture::new("roundtrip");
        let plugin = fixture.plugin();
        let date = pack_generation_date(2026, 9, 25).unwrap();
        let report = report();
        let expected_json = serde_json::to_value(&report).unwrap();
        let expected_metadata = CellMetadata { previs: true, root_visibility_cell: Some(1), previs_reference_ids: vec![12, 11], ..CellMetadata::default() };
        let expected = fixture.0.join("expected.esp");
        write_patched_plugin(&Plugin::open(&plugin).unwrap(), &expected, &FxHashMap::from_iter([(1, expected_metadata.clone()), (2, expected_metadata)]), date)
            .unwrap();
        let report_path = fixture.0.join("work/report.json");
        let timing_path = fixture.0.join("work/timing.json");
        let prepared = prepare_report(&plugin, &fixture.0, report, &report_path, &timing_path, Instant::now()).unwrap();
        let actual_json: serde_json::Value = serde_json::from_reader(File::open(&report_path).unwrap()).unwrap();
        assert_eq!(actual_json, expected_json);
        assert_eq!(prepared.cells.len(), 2);
        let summary = prepared.finalize(date).unwrap();
        assert_eq!((summary.generated, summary.stamped, summary.patched_cells), (1, 1, 2));
        assert_eq!((summary.skipped_unsupported, summary.skipped_invalid), (1, 1));
        assert_eq!(summary.failed, [("00000003".into(), "solve failed: bad geometry".into())]);
        assert_eq!(summary.warnings.len(), 2);
        assert!(summary.index.is_none());
        assert_eq!(std::fs::read(&plugin).unwrap(), std::fs::read(expected).unwrap());
        assert!(timing_path.is_file());
        assert!(!plugin.with_file_name("Test.esp.previs.tmp").exists());
    }

    #[test]
    fn duplicate_cells_and_write_errors_leave_the_plugin_unchanged() {
        let fixture = Fixture::new("errors");
        let plugin = fixture.plugin();
        let before = std::fs::read(&plugin).unwrap();
        let path = fixture.0.join("report.json");
        let timing = fixture.0.join("timing.json");
        let mut duplicate = report();
        duplicate.stamps[0].cells = vec![entry(1)];
        let error = prepare_report(&plugin, &fixture.0, duplicate, &path, &timing, Instant::now()).err().unwrap();
        assert!(error.to_string().contains("CELL 00000001 is listed twice"));
        assert!(path.is_file());
        assert!(prepare_report(&plugin, &fixture.0, report(), &fixture.0, &timing, Instant::now()).is_err());
        let mut missing = report();
        missing.jobs[0].cells = vec![entry(99)];
        let prepared = prepare_report(&plugin, &fixture.0, missing, &path, &timing, Instant::now()).unwrap();
        assert!(prepared.finalize(pack_generation_date(2026, 9, 25).unwrap()).is_err());
        assert_eq!(std::fs::read(&plugin).unwrap(), before);
    }

    #[test]
    fn empty_generation_preserves_plugin_and_cell_conversion_preserves_precombines() {
        let fixture = Fixture::new("empty");
        let plugin = fixture.plugin();
        let before = std::fs::read(&plugin).unwrap();
        let mut empty = report();
        empty.jobs.clear();
        empty.stamps.clear();
        let prepared = prepare_report(&plugin, &fixture.0, empty, &fixture.0.join("report.json"), &fixture.0.join("timing.json"), Instant::now()).unwrap();
        assert_eq!(prepared.finalize(pack_generation_date(2026, 9, 25).unwrap()).unwrap().patched_cells, 0);
        assert_eq!(std::fs::read(&plugin).unwrap(), before);
        let mut combined = entry(1);
        combined.precombine = true;
        combined.xcri = Some(XcriEntry { mesh_ids: vec![8, 7], reference_mesh_pairs: vec![(12, 8), (11, 7)] });
        let metadata = collect_metadata([combined]).unwrap();
        assert!(metadata[&1].precombine);
        assert_eq!(metadata[&1].xcri, Some(Xcri { mesh_ids: vec![8, 7], reference_mesh_pairs: vec![(12, 8), (11, 7)] }));
    }

    #[test]
    fn generation_measures_the_complete_native_stage() {
        let fixture = Fixture::new("stage");
        let plugin = fixture.plugin();
        let request = PlanRequest {
            plugin,
            master_dirs: Vec::new(),
            loose_roots: Vec::new(),
            archives: Vec::new(),
            scene_dir: fixture.0.join("scenes"),
            worlds: Vec::new(),
            interiors: true,
            accept_reference_layer: false,
            solve_into: Some(fixture.0.join("data")),
            keep_scenes: false,
            interior_cdx_only: false,
        };
        let timing_path = fixture.0.join("timing.json");
        let reported = std::sync::Mutex::new(Vec::new());
        let progress = |done, total| reported.lock().unwrap().push((done, total));
        let prepared = crate::worker_pool(Some(2)).install(|| prepare(&request, &fixture.0.join("report.json"), &timing_path, &progress)).unwrap();
        let mut reported = reported.into_inner().unwrap();
        reported.sort();
        assert_eq!(reported, vec![(1, 3), (2, 3), (3, 3)]);
        prepared.finalize(pack_generation_date(2026, 9, 25).unwrap()).unwrap();
        let timing: serde_json::Value = serde_json::from_reader(File::open(timing_path).unwrap()).unwrap();
        assert_eq!(timing["plan"]["workers"], 2);
        assert_eq!(timing["plan"]["clusters"].as_array().unwrap().len(), 3);
        assert_eq!(timing["plan"]["summaries"][0]["count"], 3);
        assert!(timing["report_seconds"].as_f64().unwrap() > 0.0);
        #[cfg(windows)]
        assert!(timing["memory"]["sampled_peak_private_bytes"].as_u64().unwrap() > 0);
    }
}
