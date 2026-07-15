//! Plan-json → PipelineSpec bridge: each stage dispatches a registered Phase
//! against its own run (per-run locks). No Python contact.
//!
//! Plan shape (built by the Python unified driver):
//! ```json
//! {
//!   "events_run_id": 7,
//!   "run_state_path": "mods/X/run_state.assets.json",   // optional
//!   "heartbeat_seconds": 30,                            // optional
//!   "max_asset_failures": 50,                           // optional
//!   "stages": [
//!     {"phase": "copy_sounds", "run_id": 8, "mod_path": "...",
//!      "source_extracted_dir": "...", "params": {...}, "after": []},
//!     ...
//!   ]
//! }
//! ```
//! `after` edges may only reference earlier stages (declaration order is the
//! topological order, matching the executor's forward-only hazard edges).
//! The pipeline cancel token is the events-run's cancel flag, so
//! `conversion_run_cancel(events_run_id)` cancels the plan AND every
//! dispatched phase (PhaseCtx.cancel is the pipeline token).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Deserialize;

use crate::phase::{self, PhaseCtx, PhaseError};
use crate::pipeline::{
    PipelineOptions, PipelineReport, PipelineSpec, ResourceId, Stage, StageCtx, StageError,
    StageReport, run_pipeline,
};

#[derive(Deserialize)]
struct PlanStage {
    phase: String,
    run_id: u64,
    mod_path: String,
    source_extracted_dir: String,
    #[serde(default)]
    target_extracted_dir: Option<String>,
    #[serde(default)]
    target_data_dir: Option<String>,
    #[serde(default)]
    params: serde_json::Value,
    #[serde(default)]
    after: Vec<String>,
}

#[derive(Deserialize)]
struct Plan {
    events_run_id: u64,
    #[serde(default)]
    run_state_path: Option<String>,
    #[serde(default)]
    heartbeat_seconds: Option<u64>,
    #[serde(default)]
    max_asset_failures: Option<u64>,
    stages: Vec<PlanStage>,
}

pub struct PlanData {
    stages: Vec<PlanStage>,
    /// Cumulative per-item failures across stages (failure-cap threshold).
    failures: AtomicU64,
    max_asset_failures: Option<u64>,
}

fn stage_body(ctx: &mut StageCtx<'_, PlanData>) -> Result<StageReport, StageError> {
    let ps = ctx
        .data
        .stages
        .iter()
        .find(|s| s.phase == ctx.stage_name)
        .expect("stage name registered from this plan");
    let slot = crate::run::run_slot(ps.run_id)
        .map_err(|e| StageError::Internal(format!("run lookup: {e}")))?;
    let phase = phase::registry()
        .get(&ps.phase)
        .ok_or_else(|| StageError::Internal(format!("unknown phase: {}", ps.phase)))?;

    // Per-stage rayon pool, mirroring run_phase_py (python_api.rs).
    let workers = {
        let run = slot
            .run
            .lock()
            .map_err(|_| StageError::Internal("run lock poisoned".into()))?;
        run.config.conversion_workers.filter(|w| *w > 0)
    };
    let dispatch = || -> Result<phase::PhaseReport, PhaseError> {
        let mut run = slot
            .run
            .lock()
            .map_err(|_| PhaseError::Internal("run lock poisoned".into()))?;
        let mut pctx = PhaseCtx {
            run: &mut run,
            mod_path: std::path::Path::new(&ps.mod_path),
            source_extracted_dir: std::path::Path::new(&ps.source_extracted_dir),
            target_extracted_dir: ps.target_extracted_dir.as_deref().map(std::path::Path::new),
            target_data_dir: ps.target_data_dir.as_deref().map(std::path::Path::new),
            params: &ps.params,
            cancel: ctx.cancel, // the PIPELINE cancel token — one flag per plan
        };
        phase.run(&mut pctx)
    };
    let result = match workers {
        Some(w) => rayon::ThreadPoolBuilder::new()
            .num_threads(w)
            .build()
            .map_err(|e| StageError::Internal(format!("rayon pool: {e}")))?
            .install(dispatch),
        None => dispatch(),
    };
    let report = match result {
        Ok(r) => r,
        Err(PhaseError::Cancelled) => return Err(StageError::Cancelled),
        Err(e) => return Err(StageError::Internal(e.to_string())),
    };
    // Forward the phase Completed event so existing Python event consumers
    // see the same telemetry the run_phase path produces.
    let _ = slot.event_tx.try_send(phase::PhaseEvent::Completed {
        phase: phase.name(),
        report: report.clone(),
    });
    ctx.counters
        .inc("assets_written", u64::from(report.assets_written));
    let failed = u64::from(report.items_failed);
    let total = ctx.data.failures.fetch_add(failed, Ordering::Relaxed) + failed;
    if let Some(cap) = ctx.data.max_asset_failures {
        if total > cap {
            return Err(StageError::Internal(format!(
                "max-asset-failures exceeded: {total} > {cap}"
            )));
        }
    }
    Ok(StageReport {
        items_done: u64::from(report.assets_written)
            + u64::from(report.records_changed)
            + u64::from(report.records_added),
        items_failed: failed,
        warnings: report.warnings,
        elapsed_ms: 0, // executor fills it
    })
}

pub fn run_plan(plan_json: &str) -> Result<PipelineReport, String> {
    let plan: Plan = serde_json::from_str(plan_json).map_err(|e| format!("bad plan: {e}"))?;
    if plan.stages.len() > 256 {
        return Err(format!(
            "plan has {} stages; max 256 (Synthetic(u8))",
            plan.stages.len()
        ));
    }
    // Names must be 'static for Stage; phase names are already &'static in the
    // registry — resolve through it (also validates names up front).
    let mut name_index: HashMap<&'static str, usize> = HashMap::new();
    let mut stages: Vec<Stage<PlanData>> = Vec::with_capacity(plan.stages.len());
    for (i, ps) in plan.stages.iter().enumerate() {
        let static_name = phase::registry()
            .get(&ps.phase)
            .ok_or_else(|| format!("unknown phase: {}", ps.phase))?
            .name();
        if name_index.insert(static_name, i).is_some() {
            return Err(format!("duplicate stage: {static_name}"));
        }
        // Each stage writes Synthetic(i); `after` names become reads.
        // Box::leak is bounded: ≤256 tiny slices per plan, a handful of
        // plans per process.
        let writes: &'static [ResourceId] =
            Box::leak(vec![ResourceId::Synthetic(i as u8)].into_boxed_slice());
        let reads: Vec<ResourceId> = ps
            .after
            .iter()
            .map(|a| {
                name_index
                    .get(a.as_str())
                    .map(|j| ResourceId::Synthetic(*j as u8))
                    .ok_or_else(|| format!("stage {static_name}: unknown 'after': {a}"))
            })
            .collect::<Result<_, _>>()?;
        stages.push(Stage {
            name: static_name,
            reads: Box::leak(reads.into_boxed_slice()),
            writes,
            run: stage_body,
        });
    }
    let slot = crate::run::run_slot(plan.events_run_id).map_err(|e| format!("events run: {e}"))?;
    let opts = PipelineOptions {
        run_state_path: plan.run_state_path.as_ref().map(PathBuf::from),
        heartbeat: Duration::from_secs(plan.heartbeat_seconds.unwrap_or(30)),
    };
    let data = PlanData {
        stages: plan.stages,
        failures: AtomicU64::new(0),
        max_asset_failures: plan.max_asset_failures,
    };
    let spec = PipelineSpec {
        stages,
        initial: vec![],
    };
    run_pipeline(&spec, &data, &slot.event_tx, slot.cancel.as_ref(), &opts)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::dispatcher_tests::test_run;
    use crate::run::drop_run;

    #[test]
    fn plan_runs_phases_as_stages_with_after_edges() {
        let id1 = test_run();
        let id2 = test_run();
        let plan = serde_json::json!({
            "events_run_id": id1,
            "stages": [
                {"phase": "test_handshake_left",  "run_id": id1, "mod_path": "", "source_extracted_dir": "", "params": {}, "after": []},
                {"phase": "test_noop",            "run_id": id1, "mod_path": "", "source_extracted_dir": "", "params": {}, "after": ["test_handshake_left"]},
                {"phase": "test_handshake_right", "run_id": id2, "mod_path": "", "source_extracted_dir": "", "params": {}, "after": []},
            ]
        });
        let report = run_plan(&plan.to_string()).unwrap();
        assert_eq!(report.stages.len(), 3);
        // Completion order respects the after-edge: noop only starts after
        // left completes.
        let pos = |n: &str| {
            report
                .stages
                .iter()
                .position(|(name, _)| *name == n)
                .unwrap_or_else(|| panic!("stage {n} missing from report"))
        };
        assert!(pos("test_handshake_left") < pos("test_noop"));
        drop_run(id1).unwrap();
        drop_run(id2).unwrap();
    }

    #[test]
    fn plan_with_unknown_phase_is_rejected() {
        let id = test_run();
        let plan = serde_json::json!({
            "events_run_id": id,
            "stages": [
                {"phase": "no_such_phase", "run_id": id, "mod_path": "", "source_extracted_dir": "", "params": {}, "after": []},
            ]
        });
        let err = run_plan(&plan.to_string()).unwrap_err();
        assert!(err.contains("unknown phase"), "got: {err}");
        drop_run(id).unwrap();
    }

    #[test]
    fn plan_with_unknown_after_edge_is_rejected() {
        let id = test_run();
        let plan = serde_json::json!({
            "events_run_id": id,
            "stages": [
                {"phase": "test_noop", "run_id": id, "mod_path": "", "source_extracted_dir": "", "params": {}, "after": ["not_a_stage"]},
            ]
        });
        let err = run_plan(&plan.to_string()).unwrap_err();
        assert!(err.contains("unknown 'after'"), "got: {err}");
        drop_run(id).unwrap();
    }
}
