//! Throttled progress reporter for long-running phases.
//!
//! Emits `PhaseEvent::Progress` at most once per `MIN_INTERVAL` OR once per
//! `PERCENT_STEP` of completion, whichever comes first — never per item.
//! Cheap and lock-free: workers call `inc()` from rayon threads; only the
//! worker that wins the throttle CAS emits.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use crossbeam_channel::Sender;

use crate::phase::PhaseEvent;

const PERCENT_STEP: u32 = 2; // emit every 2% of completion
const MIN_INTERVAL_MS: u64 = 1000; // ...or every 1s, whichever first

pub struct ProgressReporter {
    phase: &'static str,
    total: u32,
    done: AtomicU32,
    last_pct: AtomicU32,
    last_emit_ms: AtomicU64,
    start: Instant,
    last_item: Mutex<Option<String>>,
    tx: Sender<PhaseEvent>,
}

impl ProgressReporter {
    pub fn new(phase: &'static str, total: u32, tx: Sender<PhaseEvent>) -> Self {
        Self {
            phase,
            total,
            done: AtomicU32::new(0),
            last_pct: AtomicU32::new(0),
            last_emit_ms: AtomicU64::new(0),
            start: Instant::now(),
            last_item: Mutex::new(None),
            tx,
        }
    }

    /// Optional: record the most recent item label for the next emit.
    pub fn set_item(&self, label: impl Into<String>) {
        if let Ok(mut guard) = self.last_item.lock() {
            *guard = Some(label.into());
        }
    }

    pub fn inc(&self, n: u32) {
        if self.total == 0 {
            return;
        }
        let current = self.done.fetch_add(n, Ordering::Relaxed) + n;
        let pct = (u64::from(current) * 100 / u64::from(self.total)) as u32;
        let elapsed_ms = self.start.elapsed().as_millis() as u64;

        let last_pct = self.last_pct.load(Ordering::Relaxed);
        let last_ms = self.last_emit_ms.load(Ordering::Relaxed);
        let crossed_pct = pct >= last_pct + PERCENT_STEP;
        let crossed_time = elapsed_ms >= last_ms + MIN_INTERVAL_MS;
        if !crossed_pct && !crossed_time {
            return;
        }
        // Win the right to emit: CAS the percent (monotonic) or the time slot.
        if crossed_pct
            && self
                .last_pct
                .compare_exchange(last_pct, pct, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
        {
            return;
        }
        self.last_emit_ms.store(elapsed_ms, Ordering::Relaxed);
        self.emit(current.min(self.total));
    }

    pub fn finish(&self) {
        if self.total == 0 {
            return;
        }
        self.emit(self.total);
    }

    fn emit(&self, current: u32) {
        let item = self.last_item.lock().ok().and_then(|g| g.clone());
        let _ = self.tx.try_send(PhaseEvent::Progress {
            phase: self.phase,
            current,
            total: self.total,
            item,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::PhaseEvent;
    use crossbeam_channel::unbounded;

    fn count_progress(rx: &crossbeam_channel::Receiver<PhaseEvent>) -> (usize, Option<(u32, u32)>) {
        let mut n = 0;
        let mut last = None;
        while let Ok(ev) = rx.try_recv() {
            if let PhaseEvent::Progress { current, total, .. } = ev {
                n += 1;
                last = Some((current, total));
            }
        }
        (n, last)
    }

    #[test]
    fn emits_on_percent_boundary_not_every_item() {
        let (tx, rx) = unbounded();
        // total=100, 2% step => emit at 2,4,6,...; first inc to 1 should NOT emit.
        let reporter = ProgressReporter::new("convert_textures_v2", 100, tx);
        reporter.inc(1);
        let (n_after_1, _) = count_progress(&rx);
        assert_eq!(n_after_1, 0, "1/100 (1%) must not cross the 2% boundary");
        reporter.inc(1); // now 2/100 = 2%
        let (n_after_2, last) = count_progress(&rx);
        assert_eq!(n_after_2, 1, "2/100 must emit exactly once");
        assert_eq!(last, Some((2, 100)));
    }

    #[test]
    fn finish_emits_final_hundred_percent() {
        let (tx, rx) = unbounded();
        let reporter = ProgressReporter::new("convert_textures_v2", 3, tx);
        reporter.inc(3);
        reporter.finish();
        // drain; the last Progress must be 3/3.
        let mut last = None;
        while let Ok(ev) = rx.try_recv() {
            if let PhaseEvent::Progress { current, total, .. } = ev {
                last = Some((current, total));
            }
        }
        assert_eq!(last, Some((3, 3)));
    }

    #[test]
    fn zero_total_never_panics_and_never_emits() {
        let (tx, rx) = unbounded();
        let reporter = ProgressReporter::new("convert_textures_v2", 0, tx);
        reporter.inc(1);
        reporter.finish();
        let (n, _) = count_progress(&rx);
        assert_eq!(n, 0);
    }
}
