use std::fs::OpenOptions;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rustc_hash::FxHashMap;
use serde::Serialize;

use crate::error::{PrevisError, Result};
use crate::generation::write_json;
use crate::memory::{MemoryMonitor, MemoryPeaks};
use crate::metadata::{CellMetadata, Xcri, stamped_cells, write_patched_plugin};
use crate::plugin::Plugin;
use crate::precombine_stage::{self, IndexReport, PrecombineReport, PrecombineRequest};

#[derive(Default, Serialize)]
pub struct Summary {
    pub cells: usize,
    pub groups: usize,
    pub excluded_references: usize,
    pub patched_cells: usize,
    pub cleared_cells: usize,
    pub skipped_unsupported: usize,
    pub skipped_invalid: Vec<String>,
    pub report_path: PathBuf,
    pub timing_path: PathBuf,
    pub index: Option<IndexReport>,
}

#[derive(Default, Serialize)]
struct Timing {
    generate_seconds: f64,
    report_seconds: f64,
    collect_metadata_seconds: f64,
    stamp_seconds: f64,
    index_seconds: f64,
    total_seconds: f64,
    memory: Option<MemoryPeaks>,
}

pub struct PreparedPrecombines {
    plugin: PathBuf,
    data_dir: PathBuf,
    interior_cdx_only: bool,
    cells: FxHashMap<u32, CellMetadata>,
    summary: Summary,
    timing: Timing,
    started: Instant,
    memory: MemoryMonitor,
}

/// `progress(done, total)` counts target CELLs (see `precombine_stage::generate_with_progress`).
pub fn prepare(
    request: &PrecombineRequest,
    report_path: &Path,
    timing_path: &Path,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Result<PreparedPrecombines> {
    let started = Instant::now();
    let memory = MemoryMonitor::start();
    let report = precombine_stage::generate_with_progress(request, progress)?;
    let generate_seconds = started.elapsed().as_secs_f64();
    let mut prepared = prepare_report(
        &request.plugin,
        &request.output_dir,
        report,
        report_path,
        timing_path,
    )?;
    prepared.interior_cdx_only = request.interior_cdx_only;
    prepared.started = started;
    prepared.memory = memory;
    prepared.timing.generate_seconds = generate_seconds;
    Ok(prepared)
}

fn prepare_report(
    plugin: &Path,
    data_dir: &Path,
    report: PrecombineReport,
    report_path: &Path,
    timing_path: &Path,
) -> Result<PreparedPrecombines> {
    let started = Instant::now();
    write_json(report_path, &report)?;
    let mut timing = Timing {
        report_seconds: started.elapsed().as_secs_f64(),
        ..Timing::default()
    };
    let collect_started = Instant::now();
    let mut summary = Summary {
        cells: report.cells.len(),
        report_path: report_path.into(),
        timing_path: timing_path.into(),
        ..Summary::default()
    };
    for skip in report.skipped {
        if skip.unsupported {
            summary.skipped_unsupported += 1;
        } else {
            summary.skipped_invalid.push(skip.reason);
        }
    }
    let mut cells = FxHashMap::default();
    for cell in report.cells {
        summary.groups += cell.groups.len();
        summary.excluded_references += cell.excluded_references.len();
        let metadata = CellMetadata {
            precombine: true,
            xcri: Some(Xcri {
                mesh_ids: cell.xcri.mesh_ids,
                reference_mesh_pairs: cell.xcri.reference_mesh_pairs,
            }),
            ..CellMetadata::default()
        };
        if cells.insert(cell.cell_form_id, metadata).is_some() {
            return Err(PrevisError::invalid(format!(
                "CELL {:08X} is listed twice",
                cell.cell_form_id
            )));
        }
    }
    for id in stamped_cells(&Plugin::open(plugin)?)? {
        if let std::collections::hash_map::Entry::Vacant(entry) = cells.entry(id) {
            entry.insert(CellMetadata::default());
            summary.cleared_cells += 1;
        }
    }
    timing.collect_metadata_seconds = collect_started.elapsed().as_secs_f64();
    Ok(PreparedPrecombines {
        plugin: plugin.into(),
        data_dir: data_dir.into(),
        interior_cdx_only: false,
        cells,
        summary,
        timing,
        started,
        memory: MemoryMonitor::default(),
    })
}

fn append_index(path: &Path, index: &Option<IndexReport>) -> Result<()> {
    let io = |e| PrevisError::io(path.display().to_string(), e);
    let mut file = OpenOptions::new().write(true).open(path).map_err(io)?;
    // The diagnostic body is already on disk and may be hundreds of MB.
    // Replace only its final object delimiter instead of loading or rewriting it.
    file.seek(SeekFrom::End(-1)).map_err(io)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(b",\n  \"index\": ").map_err(io)?;
    serde_json::to_writer_pretty(&mut writer, index).map_err(|e| io(std::io::Error::other(e)))?;
    writer.write_all(b"\n}").map_err(io)?;
    writer.flush().map_err(io)
}

impl PreparedPrecombines {
    pub fn finalize(mut self, date: u16) -> Result<Summary> {
        if !self.cells.is_empty() {
            let started = Instant::now();
            let mut filename = self.plugin.file_name().unwrap().to_os_string();
            filename.push(".precombine.tmp");
            let output = self.plugin.with_file_name(filename);
            {
                let source = Plugin::open(&self.plugin)?;
                self.summary.patched_cells =
                    write_patched_plugin(&source, &output, &self.cells, date)?;
            }
            std::fs::rename(&output, &self.plugin)
                .map_err(|e| PrevisError::io(self.plugin.display().to_string(), e))?;
            self.timing.stamp_seconds = started.elapsed().as_secs_f64();
            self.cells.clear();
            self.cells.shrink_to_fit();
            let started = Instant::now();
            self.summary.index = crate::worker_pool(None)
                .install(|| precombine_stage::write_index(&self.plugin, &self.data_dir, self.interior_cdx_only))?;
            self.timing.index_seconds = started.elapsed().as_secs_f64();
            let started = Instant::now();
            append_index(&self.summary.report_path, &self.summary.index)?;
            self.timing.report_seconds += started.elapsed().as_secs_f64();
        }
        self.timing.total_seconds = self.started.elapsed().as_secs_f64();
        self.timing.memory = self.memory.finish();
        write_json(&self.summary.timing_path, &self.timing)?;
        Ok(self.summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::precombine_stage::{CellSkip, CombinedCell, XcriEntry};
    use std::fs::File;

    fn plugin(dir: &Path) -> PathBuf {
        let record = |signature: &[u8; 4], id: u32, payload: &[u8]| {
            let mut data = signature.to_vec();
            data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&id.to_le_bytes());
            data.extend_from_slice(&[0; 8]);
            data.extend_from_slice(payload);
            data
        };
        let mut data = record(
            b"TES4",
            0,
            b"HEDR\x0c\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
        );
        let cells: Vec<_> = (1..=3)
            .flat_map(|id| record(b"CELL", id, b"DATA\x02\x00\x01\x00"))
            .collect();
        data.extend_from_slice(b"GRUP");
        data.extend_from_slice(&((24 + cells.len()) as u32).to_le_bytes());
        data.extend_from_slice(b"CELL");
        data.extend_from_slice(&[0; 12]);
        data.extend(cells);
        let path = dir.join("Test.esp");
        std::fs::write(&path, data).unwrap();
        path
    }

    fn cell(id: u32) -> CombinedCell {
        CombinedCell {
            cell_form_id: id,
            xcri: XcriEntry {
                mesh_ids: vec![],
                reference_mesh_pairs: vec![],
            },
            groups: vec![],
            physics: None,
            excluded_references: vec![],
        }
    }

    fn report() -> PrecombineReport {
        PrecombineReport {
            plugin_name: "Test.esp".into(),
            csg_relative_path: None,
            csg_sha256: None,
            psg_sha256: None,
            cells: vec![],
            skipped: vec![CellSkip {
                cell_form_id: 3,
                unsupported: false,
                reason: "bad geometry".into(),
            }],
        }
    }

    #[test]
    fn appending_index_preserves_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        let report = report();
        let mut expected = serde_json::to_value(&report).unwrap();
        write_json(&path, &report).unwrap();
        let index = Some(IndexReport {
            cdx_relative_path: "Test.cdx".into(),
            cdx_sha256: "hash".into(),
            cells: 3,
            cells_with_visibility: 0,
            exterior_cdx_relative_path: None,
        });
        append_index(&path, &index).unwrap();
        expected["index"] = serde_json::to_value(index).unwrap();
        assert_eq!(
            serde_json::from_reader::<_, serde_json::Value>(File::open(path).unwrap()).unwrap(),
            expected
        );
    }

    #[test]
    fn duplicates_fail_before_opening_or_modifying_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let mut report = report();
        for _ in 0..2 {
            report.cells.push(CombinedCell {
                cell_form_id: 1,
                xcri: XcriEntry {
                    mesh_ids: vec![],
                    reference_mesh_pairs: vec![],
                },
                groups: vec![],
                physics: None,
                excluded_references: vec![],
            });
        }
        let error = prepare_report(
            &dir.path().join("missing.esp"),
            dir.path(),
            report,
            &dir.path().join("report.json"),
            &dir.path().join("timing.json"),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("CELL 00000001 is listed twice"));
    }

    #[test]
    fn stale_cells_clear_and_regenerated_metadata_matches_previous_path() {
        let dir = tempfile::tempdir().unwrap();
        let original = plugin(dir.path());
        let stamped = dir.path().join("Stamped.esp");
        let stale = CellMetadata {
            previs: true,
            precombine: true,
            root_visibility_cell: Some(1),
            previs_reference_ids: vec![4, 5],
            xcri: Some(Xcri {
                mesh_ids: vec![9],
                reference_mesh_pairs: vec![(5, 9)],
            }),
        };
        write_patched_plugin(
            &Plugin::open(&original).unwrap(),
            &stamped,
            &FxHashMap::from_iter([(1, stale.clone()), (2, stale)]),
            1,
        )
        .unwrap();
        let mut report = report();
        report.cells.push(cell(1));
        let prepared = prepare_report(
            &stamped,
            dir.path(),
            report,
            &dir.path().join("report.json"),
            &dir.path().join("timing.json"),
        )
        .unwrap();
        let expected = dir.path().join("Expected.esp");
        let metadata = CellMetadata {
            precombine: true,
            xcri: Some(Xcri {
                mesh_ids: vec![],
                reference_mesh_pairs: vec![],
            }),
            ..Default::default()
        };
        write_patched_plugin(
            &Plugin::open(&stamped).unwrap(),
            &expected,
            &FxHashMap::from_iter([(1, metadata), (2, CellMetadata::default())]),
            2,
        )
        .unwrap();
        let result = prepared.finalize(2).unwrap();
        assert_eq!(result.cleared_cells, 1);
        assert_eq!(result.patched_cells, 2);
        assert_eq!(result.skipped_invalid, vec!["bad geometry"]);
        assert_eq!(
            std::fs::read(stamped).unwrap(),
            std::fs::read(expected).unwrap()
        );
        let diagnostic: serde_json::Value =
            serde_json::from_reader(File::open(result.report_path).unwrap()).unwrap();
        assert!(diagnostic.get("index").unwrap().is_null());
    }

    #[test]
    fn empty_generation_and_failed_stamp_preserve_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = plugin(dir.path());
        let before = std::fs::read(&plugin).unwrap();
        let path = dir.path().join("report.json");
        let timing = dir.path().join("timing.json");
        let result = prepare_report(&plugin, dir.path(), report(), &path, &timing)
            .unwrap()
            .finalize(1)
            .unwrap();
        assert_eq!(result.patched_cells, 0);
        assert_eq!(std::fs::read(&plugin).unwrap(), before);
        let diagnostic: serde_json::Value =
            serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        assert!(diagnostic.get("index").is_none());
        let mut missing = report();
        missing.cells.push(cell(99));
        assert!(
            prepare_report(&plugin, dir.path(), missing, &path, &timing)
                .unwrap()
                .finalize(1)
                .is_err()
        );
        assert_eq!(std::fs::read(&plugin).unwrap(), before);
        assert!(prepare_report(&plugin, dir.path(), report(), dir.path(), &timing).is_err());
        assert_eq!(std::fs::read(&plugin).unwrap(), before);
    }
}
