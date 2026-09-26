use std::path::PathBuf;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::error::PrevisError;
use crate::generation::{self, PreparedGeneration, collect_metadata};
use crate::metadata::{pack_generation_date, stamped_cells, write_patched_plugin};
use crate::precombine_stage::{self, PrecombineRequest};
use crate::plugin::Plugin;
use crate::stage::{CellEntry, PlanRequest, plan};
use crate::tome;

fn to_py(error: PrevisError) -> PyErr {
    match error {
        PrevisError::Unsupported(_) | PrevisError::Invalid(_) => PyValueError::new_err(error.to_string()),
        other => PyRuntimeError::new_err(other.to_string()),
    }
}

/// Plans every previs job for a plugin; returns the JSON report.
#[pyfunction]
#[pyo3(signature = (request_json, workers = None))]
fn plan_previs(py: Python<'_>, request_json: &str, workers: Option<usize>) -> PyResult<String> {
    let request: PlanRequest =
        serde_json::from_str(request_json).map_err(|e| PyValueError::new_err(format!("invalid request: {e}")))?;
    let report = py.detach(|| crate::worker_pool(workers).install(|| plan(&request))).map_err(to_py)?;
    serde_json::to_string(&report).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pyclass]
struct PreparedPrevis {
    generation: Option<PreparedGeneration>,
}

#[pymethods]
impl PreparedPrevis {
    fn finalize(&mut self, py: Python<'_>, year: i32, month: u32, day: u32) -> PyResult<String> {
        let date = pack_generation_date(year, month, day).map_err(to_py)?;
        let generation = self.generation.take().ok_or_else(|| PyValueError::new_err("previs already finalized"))?;
        let summary = py.detach(|| generation.finalize(date)).map_err(to_py)?;
        serde_json::to_string(&summary).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

/// Passes `(done, total)` to a Python `progress` callable at most once a second
/// and always for the last item; the callable's exceptions are ignored.
fn progress_reporter(progress: Option<Py<PyAny>>) -> impl Fn(usize, usize) + Sync {
    let last_report = std::sync::Mutex::new(None::<std::time::Instant>);
    move |done, total| {
        let Some(callback) = &progress else { return };
        let finished = done == total;
        // A worker that finds another one reporting skips its update; the last item waits.
        let guard = if finished { last_report.lock().ok() } else { last_report.try_lock().ok() };
        let Some(mut last) = guard else { return };
        if !finished && last.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(1)) {
            return;
        }
        *last = Some(std::time::Instant::now());
        Python::attach(|py| {
            let _ = callback.call1(py, (done, total));
        });
    }
}

/// `progress(done, total)` receives finished clusters (see `progress_reporter`).
#[pyfunction]
#[pyo3(signature = (request_json, report_path, timing_path, workers = None, progress = None))]
fn prepare_previs(
    py: Python<'_>,
    request_json: &str,
    report_path: PathBuf,
    timing_path: PathBuf,
    workers: Option<usize>,
    progress: Option<Py<PyAny>>,
) -> PyResult<PreparedPrevis> {
    let request: PlanRequest = serde_json::from_str(request_json)
        .map_err(|e| PyValueError::new_err(format!("invalid request: {e}")))?;
    let report = progress_reporter(progress);
    let generation = py
        .detach(|| crate::worker_pool(workers).install(|| generation::prepare(&request, &report_path, &timing_path, &report)))
        .map_err(to_py)?;
    Ok(PreparedPrevis { generation: Some(generation) })
}

#[pyclass]
struct PreparedPrecombine {
    generation: Option<crate::precombine_generation::PreparedPrecombines>,
}

#[pymethods]
impl PreparedPrecombine {
    fn finalize(&mut self, py: Python<'_>, year: i32, month: u32, day: u32) -> PyResult<String> {
        let date = pack_generation_date(year, month, day).map_err(to_py)?;
        let generation = self.generation.take().ok_or_else(|| PyValueError::new_err("precombines already finalized"))?;
        let summary = py.detach(|| generation.finalize(date)).map_err(to_py)?;
        serde_json::to_string(&summary).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

/// `progress(done, total)` receives built target CELLs (see `progress_reporter`).
#[pyfunction]
#[pyo3(signature = (request_json, report_path, timing_path, workers = None, progress = None))]
fn prepare_precombines(
    py: Python<'_>,
    request_json: &str,
    report_path: PathBuf,
    timing_path: PathBuf,
    workers: Option<usize>,
    progress: Option<Py<PyAny>>,
) -> PyResult<PreparedPrecombine> {
    let request: PrecombineRequest = serde_json::from_str(request_json)
        .map_err(|e| PyValueError::new_err(format!("invalid request: {e}")))?;
    let report = progress_reporter(progress);
    let generation = py
        .detach(|| crate::worker_pool(workers).install(|| crate::precombine_generation::prepare(&request, &report_path, &timing_path, &report)))
        .map_err(to_py)?;
    Ok(PreparedPrecombine { generation: Some(generation) })
}

/// Generates every verified interior precombine: writes the group NIFs and
/// the plugin CSG under `output_dir` and returns the JSON report (CELL XCRI
/// data to stamp and the skipped CELLs with reasons).
#[pyfunction]
#[pyo3(signature = (request_json, workers = None))]
fn generate_precombines(py: Python<'_>, request_json: &str, workers: Option<usize>) -> PyResult<String> {
    let request: PrecombineRequest =
        serde_json::from_str(request_json).map_err(|e| PyValueError::new_err(format!("invalid request: {e}")))?;
    let report = py
        .detach(|| crate::worker_pool(workers).install(|| precombine_stage::generate(&request)))
        .map_err(to_py)?;
    serde_json::to_string(&report).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// Rebuilds `<plugin>.cdx` in `data_dir` from the stamped plugin; returns the
/// JSON index report, or None when no precombined CELL is present.
#[pyfunction]
#[pyo3(signature = (plugin, data_dir, interior_only = false))]
fn write_precombine_index(py: Python<'_>, plugin: PathBuf, data_dir: PathBuf, interior_only: bool) -> PyResult<Option<String>> {
    let report = py
        .detach(|| crate::worker_pool(None).install(|| precombine_stage::write_index(&plugin, &data_dir, interior_only)))
        .map_err(to_py)?;
    report
        .map(|r| serde_json::to_string(&r).map_err(|e| PyRuntimeError::new_err(e.to_string())))
        .transpose()
}

/// The plugin's CELL records carrying precombine or previs stamps, which become
/// stale once the stage replaces the plugin's precombined geometry.
#[pyfunction]
fn stamped_cell_form_ids(py: Python<'_>, plugin: PathBuf) -> PyResult<Vec<u32>> {
    py.detach(|| stamped_cells(&Plugin::open(&plugin)?)).map_err(to_py)
}

/// Rewrites plugin CELL records with previs/precombine metadata; returns the patched count.
#[pyfunction]
fn write_cell_metadata(
    py: Python<'_>,
    plugin: PathBuf,
    output: PathBuf,
    cells_json: &str,
    year: i32,
    month: u32,
    day: u32,
) -> PyResult<usize> {
    let entries: Vec<CellEntry> =
        serde_json::from_str(cells_json).map_err(|e| PyValueError::new_err(format!("invalid cells: {e}")))?;
    let packed_date = pack_generation_date(year, month, day).map_err(to_py)?;
    py.detach(|| {
        let source = Plugin::open(&plugin)?;
        let cells = collect_metadata(entries)?;
        write_patched_plugin(&source, &output, &cells, packed_date)
    })
    .map_err(to_py)
}

/// Rewrites a tome's optimizer version (for example to shipped `3.3.17`).
#[pyfunction]
fn rewrite_tome_version<'py>(py: Python<'py>, tome: &[u8], version: &str) -> PyResult<Bound<'py, PyBytes>> {
    let rewritten = tome::rewrite_optimizer_version(tome, version).map_err(to_py)?;
    Ok(PyBytes::new(py, &rewritten))
}

pub fn register_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(plan_previs, m)?)?;
    m.add_function(wrap_pyfunction!(prepare_previs, m)?)?;
    m.add_function(wrap_pyfunction!(prepare_precombines, m)?)?;
    m.add_function(wrap_pyfunction!(generate_precombines, m)?)?;
    m.add_function(wrap_pyfunction!(write_precombine_index, m)?)?;
    m.add_function(wrap_pyfunction!(stamped_cell_form_ids, m)?)?;
    m.add_function(wrap_pyfunction!(write_cell_metadata, m)?)?;
    m.add_function(wrap_pyfunction!(rewrite_tome_version, m)?)?;
    Ok(())
}
