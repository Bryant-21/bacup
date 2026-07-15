//! Stage-DAG executor for the unified regen pipeline.
//!
//! Provides the executor + run_state.json + heartbeat + cancel + stage events.
//! Stage code follows the phase contract: no Python contact; events via
//! crossbeam_channel; rayon parallelism lives INSIDE stages. Per-item progress
//! reuses `crate::phase::progress::ProgressReporter`
//! (`ProgressReporter::new(ctx.stage_name, total, ctx.events.clone())`).

pub mod counters;
pub mod executor;
pub mod plan;
pub mod rss;
pub mod run_state;
pub mod timefmt;

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::Sender;
use thiserror::Error;

use crate::phase::PhaseEvent;
use counters::Counters;

pub use executor::{PipelineOptions, run_pipeline};

/// Typed resources stages read/write. Hazard edges between stages are
/// derived from these sets (see `executor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceId {
    SourcePlugin,
    TargetStore,
    AssetList,
    RelocationMembers,
    Ba2Shards,
    TerrainSidecars,
    LooseTree,
    OutputEsm,
    StringsTables,
    /// Test-only synthetic resources.
    Synthetic(u8),
}

/// What a stage reports back. Per-item failures are isolated: a failed item
/// increments `items_failed` and the stage keeps going — `StageError` means
/// the whole stage aborted.
#[derive(Debug, Default, Clone)]
pub struct StageReport {
    pub items_done: u64,
    pub items_failed: u64,
    pub warnings: u32,
    /// Filled in by the executor.
    pub elapsed_ms: u64,
}

#[derive(Debug, Error)]
pub enum StageError {
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Internal(String),
}

/// Everything a stage body may touch. `data` is the run-wide shared state
/// (the run's resources; tests: synthetic structs).
pub struct StageCtx<'a, P> {
    pub data: &'a P,
    pub events: &'a Sender<PhaseEvent>,
    pub cancel: &'a AtomicBool,
    pub counters: &'a Counters,
    pub stage_name: &'static str,
}

impl<'a, P> StageCtx<'a, P> {
    pub fn check_cancel(&self) -> Result<(), StageError> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(StageError::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub struct Stage<P> {
    pub name: &'static str,
    pub reads: &'static [ResourceId],
    pub writes: &'static [ResourceId],
    pub run: fn(&mut StageCtx<'_, P>) -> Result<StageReport, StageError>,
}

pub struct PipelineSpec<P> {
    /// Declaration order is the deterministic tie-break for hazard edges.
    pub stages: Vec<Stage<P>>,
    /// Resources available before any stage runs (run-init products).
    pub initial: Vec<ResourceId>,
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("invalid pipeline spec: {0}")]
    InvalidSpec(String),
    #[error("stage {stage} failed: {message}")]
    StageFailed {
        stage: &'static str,
        message: String,
    },
    #[error("cancelled")]
    Cancelled,
}

#[derive(Debug, Default)]
pub struct PipelineReport {
    /// (name, report) in completion order.
    pub stages: Vec<(&'static str, StageReport)>,
    pub elapsed_ms: u64,
    pub counters: BTreeMap<String, u64>,
}

impl<P> PipelineSpec<P> {
    /// Rejects: empty/duplicate stage names; a stage reading a resource
    /// that is neither in `initial` nor written by an earlier stage
    /// (declaration order is execution-legal order).
    pub fn validate(&self) -> Result<(), PipelineError> {
        let mut seen: HashSet<&'static str> = HashSet::new();
        let mut written: HashSet<ResourceId> = self.initial.iter().copied().collect();
        for stage in &self.stages {
            if stage.name.is_empty() {
                return Err(PipelineError::InvalidSpec("stage with empty name".into()));
            }
            if !seen.insert(stage.name) {
                return Err(PipelineError::InvalidSpec(format!(
                    "duplicate stage name: {}",
                    stage.name
                )));
            }
            for r in stage.reads {
                if !written.contains(r) {
                    return Err(PipelineError::InvalidSpec(format!(
                        "stage {} reads {r:?} before any writer",
                        stage.name
                    )));
                }
            }
            written.extend(stage.writes.iter().copied());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop(_: &mut StageCtx<'_, ()>) -> Result<StageReport, StageError> {
        Ok(StageReport::default())
    }

    fn stage(
        name: &'static str,
        reads: &'static [ResourceId],
        writes: &'static [ResourceId],
    ) -> Stage<()> {
        Stage {
            name,
            reads,
            writes,
            run: noop,
        }
    }

    #[test]
    fn valid_spec_passes() {
        let spec = PipelineSpec {
            stages: vec![
                stage("a", &[], &[ResourceId::Synthetic(0)]),
                stage("b", &[ResourceId::Synthetic(0)], &[]),
                stage("c", &[ResourceId::Synthetic(9)], &[]),
            ],
            initial: vec![ResourceId::Synthetic(9)],
        };
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn duplicate_name_rejected() {
        let spec = PipelineSpec {
            stages: vec![stage("a", &[], &[]), stage("a", &[], &[])],
            initial: vec![],
        };
        let err = spec.validate().unwrap_err();
        assert!(matches!(err, PipelineError::InvalidSpec(ref m) if m.contains("duplicate")));
    }

    #[test]
    fn empty_name_rejected() {
        let spec = PipelineSpec {
            stages: vec![stage("", &[], &[])],
            initial: vec![],
        };
        assert!(matches!(
            spec.validate().unwrap_err(),
            PipelineError::InvalidSpec(_)
        ));
    }

    #[test]
    fn read_before_any_writer_rejected_even_if_written_later() {
        let spec = PipelineSpec {
            stages: vec![
                stage("x", &[ResourceId::Synthetic(3)], &[]),
                stage("y", &[], &[ResourceId::Synthetic(3)]),
            ],
            initial: vec![],
        };
        let err = spec.validate().unwrap_err();
        assert!(
            matches!(err, PipelineError::InvalidSpec(ref m) if m.contains("before any writer"))
        );
    }
}
