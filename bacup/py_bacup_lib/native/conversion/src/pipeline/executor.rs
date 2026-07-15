//! Topological hazard-edge scheduler. Stages with disjoint resources run
//! concurrently on scoped threads; rayon parallelism lives INSIDE stages.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::phase::PhaseEvent;

use super::counters::Counters;
use super::run_state::RunStateWriter;
use super::{
    PipelineError, PipelineReport, PipelineSpec, Stage, StageCtx, StageError, StageReport,
};

pub struct PipelineOptions {
    /// None = no run_state.json (unit tests that don't care).
    pub run_state_path: Option<std::path::PathBuf>,
    pub heartbeat: Duration,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            run_state_path: None,
            heartbeat: Duration::from_secs(30),
        }
    }
}

/// Derive hazard edges from declared resource sets. For each pair i < j
/// (declaration order): RAW (i writes what j reads), WAW (both write),
/// WAR (i reads what j writes). Edges always point forward -> acyclic.
pub(super) fn build_edges<P>(spec: &PipelineSpec<P>) -> (Vec<usize>, Vec<Vec<usize>>) {
    let n = spec.stages.len();
    let mut indeg = vec![0usize; n];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
    for j in 0..n {
        for i in 0..j {
            let a = &spec.stages[i];
            let b = &spec.stages[j];
            let raw = a.writes.iter().any(|r| b.reads.contains(r));
            let waw = a.writes.iter().any(|r| b.writes.contains(r));
            let war = a.reads.iter().any(|r| b.writes.contains(r));
            if raw || waw || war {
                dependents[i].push(j);
                indeg[j] += 1;
            }
        }
    }
    (indeg, dependents)
}

/// Sends a synthetic failure if the stage thread unwinds before reporting,
/// so the scheduler never deadlocks waiting on a panicked stage.
struct CompletionGuard {
    idx: usize,
    tx: Sender<(usize, Result<StageReport, StageError>)>,
    sent: bool,
}

impl CompletionGuard {
    fn complete(mut self, result: Result<StageReport, StageError>) {
        self.sent = true;
        let _ = self.tx.send((self.idx, result));
    }
}

impl Drop for CompletionGuard {
    fn drop(&mut self) {
        if !self.sent {
            let _ = self.tx.send((
                self.idx,
                Err(StageError::Internal(
                    "stage panicked before reporting".into(),
                )),
            ));
        }
    }
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

fn start_stage<'scope, 'env, P: Sync>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    stage: &'env Stage<P>,
    idx: usize,
    data: &'env P,
    events: &'env Sender<PhaseEvent>,
    cancel: &'env AtomicBool,
    counters: &'env Counters,
    run_state: Option<&RunStateWriter>,
    done_tx: Sender<(usize, Result<StageReport, StageError>)>,
) {
    let _ = events.try_send(PhaseEvent::StageStarted { stage: stage.name });
    if let Some(w) = run_state {
        w.stage_started(stage.name);
    }
    scope.spawn(move || {
        let guard = CompletionGuard {
            idx,
            tx: done_tx,
            sent: false,
        };
        let started = Instant::now();
        let mut ctx = StageCtx {
            data,
            events,
            cancel,
            counters,
            stage_name: stage.name,
        };
        let result = match catch_unwind(AssertUnwindSafe(|| {
            (stage.run)(&mut ctx).map(|mut r| {
                r.elapsed_ms = started.elapsed().as_millis() as u64;
                r
            })
        })) {
            Ok(result) => result,
            Err(payload) => Err(StageError::Internal(format!(
                "stage panicked: {}",
                panic_payload_to_string(&*payload)
            ))),
        };
        guard.complete(result);
    });
}

/// Run the spec to completion. Independent stages overlap; one failure
/// sets `cancel`, running stages drain cooperatively, unstarted stages
/// never start. The first non-cancel failure is the returned error.
pub fn run_pipeline<P: Sync>(
    spec: &PipelineSpec<P>,
    data: &P,
    events: &Sender<PhaseEvent>,
    cancel: &AtomicBool,
    opts: &PipelineOptions,
) -> Result<PipelineReport, PipelineError> {
    spec.validate()?;
    let started = Instant::now();
    let counters = Arc::new(Counters::default());
    let run_state = opts
        .run_state_path
        .as_ref()
        .map(|p| RunStateWriter::new(p.clone(), Arc::clone(&counters)));
    let rs: Option<&RunStateWriter> = run_state.as_deref();
    if let Some(w) = rs {
        w.write_now();
    }
    let _heartbeat = run_state
        .as_ref()
        .map(|w| RunStateWriter::spawn_heartbeat(w, opts.heartbeat));

    let n = spec.stages.len();
    let (mut indeg, dependents) = build_edges(spec);
    let mut started_flags = vec![false; n];
    let mut completed: Vec<(&'static str, StageReport)> = Vec::new();
    let mut first_failure: Option<(usize, String)> = None;
    let counters_ref: &Counters = &counters;
    let (done_tx, done_rx) =
        crossbeam_channel::unbounded::<(usize, Result<StageReport, StageError>)>();

    std::thread::scope(|scope| {
        let mut running = 0usize;
        for i in 0..n {
            if indeg[i] == 0 && !cancel.load(Ordering::Relaxed) {
                start_stage(
                    scope,
                    &spec.stages[i],
                    i,
                    data,
                    events,
                    cancel,
                    counters_ref,
                    rs,
                    done_tx.clone(),
                );
                started_flags[i] = true;
                running += 1;
            }
        }
        while running > 0 {
            let (i, result) = done_rx.recv().expect("stage completion channel broken");
            running -= 1;
            let name = spec.stages[i].name;
            if let Some(w) = rs {
                w.stage_finished(name);
            }
            match result {
                Ok(report) => {
                    let _ = events.try_send(PhaseEvent::StageCompleted {
                        stage: name,
                        items_done: report.items_done,
                        items_failed: report.items_failed,
                        elapsed_ms: report.elapsed_ms,
                    });
                    completed.push((name, report));
                    for &d in &dependents[i] {
                        indeg[d] -= 1;
                        if indeg[d] == 0 && !started_flags[d] && !cancel.load(Ordering::Relaxed) {
                            start_stage(
                                scope,
                                &spec.stages[d],
                                d,
                                data,
                                events,
                                cancel,
                                counters_ref,
                                rs,
                                done_tx.clone(),
                            );
                            started_flags[d] = true;
                            running += 1;
                        }
                    }
                }
                Err(StageError::Cancelled) => {
                    // Cooperative drain of an already-cancelled run; the
                    // root cause (failure or external cancel) is recorded
                    // elsewhere. Dependents never become ready.
                }
                Err(StageError::Internal(message)) => {
                    let _ = events.try_send(PhaseEvent::StageFailed {
                        stage: name,
                        message: message.clone(),
                    });
                    cancel.store(true, Ordering::Relaxed);
                    if first_failure.is_none() {
                        if let Some(w) = rs {
                            w.set_failed(name);
                        }
                        first_failure = Some((i, message));
                    }
                }
            }
        }
    });

    if let Some((idx, message)) = first_failure {
        return Err(PipelineError::StageFailed {
            stage: spec.stages[idx].name,
            message,
        });
    }
    if cancel.load(Ordering::Relaxed) {
        if let Some(w) = rs {
            w.set_failed("(cancelled)");
        }
        return Err(PipelineError::Cancelled);
    }
    if let Some(w) = rs {
        w.set_done();
    }
    Ok(PipelineReport {
        stages: completed,
        elapsed_ms: started.elapsed().as_millis() as u64,
        counters: counters.snapshot(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Duration;

    use crossbeam_channel::{Receiver, unbounded};

    use crate::phase::PhaseEvent;
    use crate::pipeline::{
        PipelineError, PipelineOptions, PipelineSpec, ResourceId, Stage, StageCtx, StageError,
        StageReport, run_pipeline,
    };

    pub(super) struct TestData {
        pub log: Mutex<Vec<&'static str>>,
        pub value: AtomicU64,
    }

    impl Default for TestData {
        fn default() -> Self {
            Self {
                log: Mutex::new(Vec::new()),
                value: AtomicU64::new(1),
            }
        }
    }

    fn drain_tags(rx: &Receiver<PhaseEvent>) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            match ev {
                PhaseEvent::StageStarted { stage } => out.push(format!("start:{stage}")),
                PhaseEvent::StageCompleted { stage, .. } => out.push(format!("done:{stage}")),
                PhaseEvent::StageFailed { stage, .. } => out.push(format!("fail:{stage}")),
                _ => {}
            }
        }
        out
    }

    fn init_stage(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        ctx.data.log.lock().unwrap().push("init");
        ctx.data.value.store(42, Ordering::SeqCst);
        ctx.counters.inc("inited", 1);
        Ok(StageReport {
            items_done: 1,
            ..Default::default()
        })
    }

    fn consume_stage(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        let v = ctx.data.value.load(Ordering::SeqCst);
        if v != 42 {
            return Err(StageError::Internal(format!(
                "consume ran before init: value={v}"
            )));
        }
        ctx.data.log.lock().unwrap().push("consume");
        Ok(StageReport {
            items_done: 1,
            ..Default::default()
        })
    }

    #[test]
    fn raw_dependency_runs_in_order_with_events() {
        let data = TestData::default();
        let (tx, rx) = unbounded();
        let cancel = AtomicBool::new(false);
        let spec = PipelineSpec {
            stages: vec![
                Stage {
                    name: "init",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(0)],
                    run: init_stage,
                },
                Stage {
                    name: "consume",
                    reads: &[ResourceId::Synthetic(0)],
                    writes: &[],
                    run: consume_stage,
                },
            ],
            initial: vec![],
        };

        let report = run_pipeline(&spec, &data, &tx, &cancel, &PipelineOptions::default()).unwrap();

        assert_eq!(*data.log.lock().unwrap(), vec!["init", "consume"]);
        assert_eq!(
            drain_tags(&rx),
            vec!["start:init", "done:init", "start:consume", "done:consume"]
        );
        let names: Vec<_> = report.stages.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["init", "consume"]);
        assert_eq!(report.counters.get("inited"), Some(&1));
        assert!(report.stages.iter().all(|(_, r)| r.items_done == 1));
    }

    #[test]
    fn invalid_spec_is_rejected_before_running_anything() {
        let data = TestData::default();
        let (tx, _rx) = unbounded();
        let cancel = AtomicBool::new(false);
        let spec = PipelineSpec {
            stages: vec![
                Stage {
                    name: "dup",
                    reads: &[],
                    writes: &[],
                    run: init_stage,
                },
                Stage {
                    name: "dup",
                    reads: &[],
                    writes: &[],
                    run: init_stage,
                },
            ],
            initial: vec![],
        };
        let err =
            run_pipeline(&spec, &data, &tx, &cancel, &PipelineOptions::default()).unwrap_err();
        assert!(matches!(err, PipelineError::InvalidSpec(_)));
        assert!(data.log.lock().unwrap().is_empty(), "no stage may have run");
    }

    #[test]
    fn build_edges_shapes() {
        fn noop(_: &mut StageCtx<'_, ()>) -> Result<StageReport, StageError> {
            Ok(StageReport::default())
        }
        let spec: PipelineSpec<()> = PipelineSpec {
            stages: vec![
                Stage {
                    name: "a",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(0)],
                    run: noop,
                },
                Stage {
                    name: "b",
                    reads: &[ResourceId::Synthetic(0)],
                    writes: &[],
                    run: noop,
                },
                Stage {
                    name: "c",
                    reads: &[],
                    writes: &[],
                    run: noop,
                },
            ],
            initial: vec![],
        };
        let (indeg, dependents) = super::build_edges(&spec);
        assert_eq!(indeg, vec![0, 1, 0]);
        assert_eq!(dependents[0], vec![1]);
        assert!(dependents[1].is_empty() && dependents[2].is_empty());
    }

    struct HandshakeData {
        left_to_right: (
            crossbeam_channel::Sender<&'static str>,
            crossbeam_channel::Receiver<&'static str>,
        ),
        right_to_left: (
            crossbeam_channel::Sender<&'static str>,
            crossbeam_channel::Receiver<&'static str>,
        ),
    }

    fn left_stage(ctx: &mut StageCtx<'_, HandshakeData>) -> Result<StageReport, StageError> {
        ctx.data.left_to_right.0.send("left").unwrap();
        ctx.data
            .right_to_left
            .1
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| {
                StageError::Internal("right never arrived — stages did not overlap".into())
            })?;
        Ok(StageReport::default())
    }

    fn right_stage(ctx: &mut StageCtx<'_, HandshakeData>) -> Result<StageReport, StageError> {
        ctx.data.right_to_left.0.send("right").unwrap();
        ctx.data
            .left_to_right
            .1
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| {
                StageError::Internal("left never arrived — stages did not overlap".into())
            })?;
        Ok(StageReport::default())
    }

    #[test]
    fn independent_stages_run_concurrently() {
        // Each stage blocks until it hears from the other: a serial
        // executor times out (clear Internal error), a concurrent one
        // completes both. No timing flakiness, only generous timeouts.
        let data = HandshakeData {
            left_to_right: unbounded(),
            right_to_left: unbounded(),
        };
        let (tx, _rx) = unbounded();
        let cancel = AtomicBool::new(false);
        let spec = PipelineSpec {
            stages: vec![
                Stage {
                    name: "left",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(1)],
                    run: left_stage,
                },
                Stage {
                    name: "right",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(2)],
                    run: right_stage,
                },
            ],
            initial: vec![],
        };
        let result = run_pipeline(&spec, &data, &tx, &cancel, &PipelineOptions::default());
        assert!(result.is_ok(), "stages did not overlap: {result:?}");
    }

    fn slow_writer_one(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        std::thread::sleep(Duration::from_millis(50));
        ctx.data.log.lock().unwrap().push("w1");
        Ok(StageReport::default())
    }

    fn fast_writer_two(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        ctx.data.log.lock().unwrap().push("w2");
        Ok(StageReport::default())
    }

    #[test]
    fn waw_serializes_in_declaration_order() {
        // w2 is instant, w1 sleeps: without the WAW edge w2 would log
        // first. The edge forces w1 -> w2.
        let data = TestData::default();
        let (tx, _rx) = unbounded();
        let cancel = AtomicBool::new(false);
        let spec = PipelineSpec {
            stages: vec![
                Stage {
                    name: "w1",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(0)],
                    run: slow_writer_one,
                },
                Stage {
                    name: "w2",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(0)],
                    run: fast_writer_two,
                },
            ],
            initial: vec![],
        };
        run_pipeline(&spec, &data, &tx, &cancel, &PipelineOptions::default()).unwrap();
        assert_eq!(*data.log.lock().unwrap(), vec!["w1", "w2"]);
    }

    fn slow_reader(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        std::thread::sleep(Duration::from_millis(30));
        let v = ctx.data.value.load(Ordering::SeqCst);
        if v != 1 {
            return Err(StageError::Internal(format!(
                "writer mutated the resource while reader held it: {v}"
            )));
        }
        Ok(StageReport::default())
    }

    fn late_writer(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        ctx.data.value.store(2, Ordering::SeqCst);
        Ok(StageReport::default())
    }

    #[test]
    fn war_makes_writer_wait_for_reader() {
        let data = TestData::default(); // value starts at 1
        let (tx, _rx) = unbounded();
        let cancel = AtomicBool::new(false);
        let spec = PipelineSpec {
            stages: vec![
                Stage {
                    name: "reader",
                    reads: &[ResourceId::Synthetic(7)],
                    writes: &[],
                    run: slow_reader,
                },
                Stage {
                    name: "writer",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(7)],
                    run: late_writer,
                },
            ],
            initial: vec![ResourceId::Synthetic(7)],
        };
        run_pipeline(&spec, &data, &tx, &cancel, &PipelineOptions::default()).unwrap();
        assert_eq!(data.value.load(Ordering::SeqCst), 2);
    }

    fn rayon_sum_stage(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        use rayon::prelude::*;
        let sum: u64 = (0u64..10_000).into_par_iter().sum();
        ctx.data.value.store(sum, Ordering::SeqCst);
        Ok(StageReport {
            items_done: 10_000,
            ..Default::default()
        })
    }

    #[test]
    fn rayon_parallelism_inside_a_stage() {
        let data = TestData::default();
        let (tx, _rx) = unbounded();
        let cancel = AtomicBool::new(false);
        let spec = PipelineSpec {
            stages: vec![Stage {
                name: "sum",
                reads: &[],
                writes: &[],
                run: rayon_sum_stage,
            }],
            initial: vec![],
        };
        run_pipeline(&spec, &data, &tx, &cancel, &PipelineOptions::default()).unwrap();
        assert_eq!(data.value.load(Ordering::SeqCst), 49_995_000);
    }

    fn boom_stage(_ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        std::thread::sleep(Duration::from_millis(10));
        Err(StageError::Internal("kaboom".into()))
    }

    fn slowpoke_stage(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        for _ in 0..200 {
            ctx.check_cancel()?;
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(StageReport::default())
    }

    fn never_runs_stage(ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        ctx.data.log.lock().unwrap().push("never");
        Ok(StageReport::default())
    }

    #[test]
    fn failure_cancels_running_and_skips_pending() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("run_state.json");
        let data = TestData::default();
        let (tx, rx) = unbounded();
        let cancel = AtomicBool::new(false);
        let spec = PipelineSpec {
            stages: vec![
                Stage {
                    name: "boom",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(1)],
                    run: boom_stage,
                },
                Stage {
                    name: "slowpoke",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(2)],
                    run: slowpoke_stage,
                },
                Stage {
                    name: "after_boom",
                    reads: &[ResourceId::Synthetic(1)],
                    writes: &[],
                    run: never_runs_stage,
                },
            ],
            initial: vec![],
        };
        let opts = PipelineOptions {
            run_state_path: Some(state_path.clone()),
            ..Default::default()
        };

        let err = run_pipeline(&spec, &data, &tx, &cancel, &opts).unwrap_err();

        assert!(
            matches!(err, PipelineError::StageFailed { stage: "boom", ref message } if message == "kaboom")
        );
        assert!(
            cancel.load(Ordering::SeqCst),
            "failure must set the cancel token"
        );
        assert!(
            data.log.lock().unwrap().is_empty(),
            "after_boom must never start"
        );
        let tags = drain_tags(&rx);
        assert!(tags.contains(&"fail:boom".to_string()));
        assert!(tags.contains(&"start:slowpoke".to_string()));
        assert!(!tags.iter().any(|t| t == "start:after_boom"));

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
        assert_eq!(v["status"], "failed");
        assert_eq!(v["stage"], "boom");
    }

    #[test]
    fn external_cancel_before_start_runs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("run_state.json");
        let data = TestData::default();
        let (tx, rx) = unbounded();
        let cancel = AtomicBool::new(true); // pre-set, e.g. Ctrl-C between runs
        let spec = PipelineSpec {
            stages: vec![Stage {
                name: "init",
                reads: &[],
                writes: &[],
                run: init_stage,
            }],
            initial: vec![],
        };
        let opts = PipelineOptions {
            run_state_path: Some(state_path.clone()),
            ..Default::default()
        };

        let err = run_pipeline(&spec, &data, &tx, &cancel, &opts).unwrap_err();

        assert!(matches!(err, PipelineError::Cancelled));
        assert!(data.log.lock().unwrap().is_empty());
        assert!(
            drain_tags(&rx).is_empty(),
            "no stage events on pre-cancelled run"
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
        assert_eq!(v["status"], "failed");
        assert_eq!(v["stage"], "(cancelled)");
    }

    fn panicker_stage(_ctx: &mut StageCtx<'_, TestData>) -> Result<StageReport, StageError> {
        panic!("synthetic stage panic");
    }

    #[test]
    fn stage_panic_does_not_deadlock_the_scheduler() {
        // The stage thread catches the panic and returns it as a stage failure.
        // The regression here is a deadlock (recv waiting forever).
        let data = TestData::default();
        let (tx, rx) = unbounded();
        let cancel = AtomicBool::new(false);
        let spec = PipelineSpec {
            stages: vec![
                Stage {
                    name: "panicker",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(1)],
                    run: panicker_stage,
                },
                Stage {
                    name: "slowpoke",
                    reads: &[],
                    writes: &[ResourceId::Synthetic(2)],
                    run: slowpoke_stage,
                },
            ],
            initial: vec![],
        };

        let err =
            run_pipeline(&spec, &data, &tx, &cancel, &PipelineOptions::default()).unwrap_err();

        assert!(
            matches!(
                err,
                PipelineError::StageFailed {
                    stage: "panicker",
                    ref message
                } if message.contains("stage panicked")
                    && message.contains("synthetic stage panic")
            ),
            "panic payload must be returned as a stage error: {err:?}"
        );
        assert!(cancel.load(Ordering::SeqCst), "panic must cancel the run");
        let tags = drain_tags(&rx);
        assert!(tags.contains(&"fail:panicker".to_string()));
    }
}
