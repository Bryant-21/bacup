//! Incremental run_state.json — the crash-diagnosability fix.
//!
//! Rewritten atomically at every stage boundary and from a heartbeat
//! thread, so a dead run always leaves behind its last known stage,
//! counters, and RSS. Best-effort: write failures count, never abort.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use crossbeam_channel::RecvTimeoutError;

use super::counters::Counters;
use super::rss::current_rss_bytes;
use super::timefmt::iso8601_utc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunStatus {
    Running,
    Failed,
    Done,
}

impl RunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Done => "done",
        }
    }
}

struct State {
    status: RunStatus,
    running_stages: BTreeSet<&'static str>,
    /// Set on failure so `stage` keeps naming the culprit while the
    /// remaining stages drain.
    failed_stage: Option<&'static str>,
}

pub struct RunStateWriter {
    path: PathBuf,
    counters: Arc<Counters>,
    started_at: String,
    state: Mutex<State>,
    pub write_errors: AtomicU64,
}

impl RunStateWriter {
    pub fn new(path: PathBuf, counters: Arc<Counters>) -> Arc<Self> {
        Arc::new(Self {
            path,
            counters,
            started_at: iso8601_utc(SystemTime::now()),
            state: Mutex::new(State {
                status: RunStatus::Running,
                running_stages: BTreeSet::new(),
                failed_stage: None,
            }),
            write_errors: AtomicU64::new(0),
        })
    }

    pub fn stage_started(&self, name: &'static str) {
        self.state
            .lock()
            .expect("run_state poisoned")
            .running_stages
            .insert(name);
        self.write_now();
    }

    pub fn stage_finished(&self, name: &'static str) {
        self.state
            .lock()
            .expect("run_state poisoned")
            .running_stages
            .remove(name);
        self.write_now();
    }

    pub fn set_failed(&self, stage: &'static str) {
        {
            let mut st = self.state.lock().expect("run_state poisoned");
            st.status = RunStatus::Failed;
            st.failed_stage = Some(stage);
        }
        self.write_now();
    }

    pub fn set_done(&self) {
        {
            let mut st = self.state.lock().expect("run_state poisoned");
            st.status = RunStatus::Done;
            st.running_stages.clear();
        }
        self.write_now();
    }

    pub fn write_now(&self) {
        let json = self.render();
        if self.try_write(&json).is_err() {
            self.write_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn render(&self) -> String {
        let st = self.state.lock().expect("run_state poisoned");
        let stage = match (st.status, st.failed_stage) {
            (RunStatus::Failed, Some(name)) => name.to_string(),
            _ => st
                .running_stages
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .join("+"),
        };
        serde_json::json!({
            "status": st.status.as_str(),
            "stage": stage,
            "started_at": self.started_at,
            "updated_at": iso8601_utc(SystemTime::now()),
            "counters": self.counters.snapshot(),
            "rss_bytes": current_rss_bytes(),
        })
        .to_string()
    }

    fn try_write(&self, json: &str) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        let renamed = std::fs::rename(&tmp, &self.path);
        if renamed.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        renamed
    }

    /// Heartbeat thread: rewrites run_state.json every `interval` until the
    /// returned handle is stopped/dropped. Associated fn (not a method)
    /// because `self: &Arc<Self>` is an unstable receiver type.
    pub fn spawn_heartbeat(writer: &Arc<Self>, interval: Duration) -> HeartbeatHandle {
        let writer = Arc::clone(writer);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);
        let join = std::thread::spawn(move || {
            loop {
                match stop_rx.recv_timeout(interval) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => writer.write_now(),
                }
            }
        });
        HeartbeatHandle {
            stop_tx,
            join: Some(join),
        }
    }
}

pub struct HeartbeatHandle {
    stop_tx: crossbeam_channel::Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl HeartbeatHandle {
    pub fn stop(self) {}

    fn shutdown(&mut self) {
        let _ = self.stop_tx.try_send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for HeartbeatHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn read_json(path: &std::path::Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(path).expect("run_state readable"))
            .expect("run_state is valid JSON")
    }

    #[test]
    fn write_now_emits_locked_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run_state.json");
        let counters = Arc::new(Counters::default());
        counters.inc("widgets", 3);
        let writer = RunStateWriter::new(path.clone(), counters);
        writer.stage_started("alpha");

        let v = read_json(&path);
        assert_eq!(v["status"], "running");
        assert_eq!(v["stage"], "alpha");
        assert_eq!(v["counters"]["widgets"], 3);
        assert!(v["rss_bytes"].as_u64().unwrap() > 0);
        for key in ["started_at", "updated_at"] {
            let s = v[key].as_str().unwrap();
            assert!(
                s.ends_with('Z') && s.contains('T'),
                "{key} not ISO-8601: {s}"
            );
        }
        assert!(
            !path.with_extension("json.tmp").exists(),
            "tmp file must not linger"
        );
        assert_eq!(writer.write_errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn concurrent_stages_join_names_and_terminal_states_render() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run_state.json");
        let writer = RunStateWriter::new(path.clone(), Arc::new(Counters::default()));
        writer.stage_started("translate");
        writer.stage_started("textures");
        assert_eq!(read_json(&path)["stage"], "textures+translate"); // BTreeSet order

        writer.set_failed("textures");
        let v = read_json(&path);
        assert_eq!(v["status"], "failed");
        assert_eq!(v["stage"], "textures");

        writer.set_done();
        let v = read_json(&path);
        assert_eq!(v["status"], "done");
        assert_eq!(v["stage"], "");
    }

    #[test]
    fn heartbeat_rewrites_updated_at() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run_state.json");
        let writer = RunStateWriter::new(path.clone(), Arc::new(Counters::default()));
        writer.write_now();
        let first = read_json(&path)["updated_at"].as_str().unwrap().to_string();

        let heartbeat = RunStateWriter::spawn_heartbeat(&writer, Duration::from_millis(25));
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut changed = false;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
            let v = read_json(&path); // every observation must parse cleanly
            if v["updated_at"].as_str().unwrap() != first {
                changed = true;
                break;
            }
        }
        heartbeat.stop();
        assert!(changed, "heartbeat never rewrote run_state.json within 2s");
    }

    #[test]
    fn write_failure_is_counted_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        // The destination IS a directory -> rename must fail.
        let writer = RunStateWriter::new(dir.path().to_path_buf(), Arc::new(Counters::default()));
        writer.write_now();
        assert!(writer.write_errors.load(Ordering::Relaxed) >= 1);
    }
}
