//! Named run-wide counters surfaced in run_state.json.
//!
//! Coarse-grained by design: stage code bumps in batches (per chunk, per
//! 1000 items), never per hot-loop item — per-item visibility is the
//! ProgressReporter's job.

use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct Counters {
    inner: Mutex<BTreeMap<String, u64>>,
}

impl Counters {
    pub fn inc(&self, name: &str, n: u64) {
        let mut guard = self.inner.lock().expect("counters mutex poisoned");
        *guard.entry(name.to_string()).or_insert(0) += n;
    }

    pub fn snapshot(&self) -> BTreeMap<String, u64> {
        self.inner.lock().expect("counters mutex poisoned").clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_across_threads() {
        let counters = Counters::default();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for _ in 0..1000 {
                        counters.inc("widgets", 1);
                    }
                });
            }
        });
        counters.inc("other", 7);
        let snap = counters.snapshot();
        assert_eq!(snap.get("widgets"), Some(&4000));
        assert_eq!(snap.get("other"), Some(&7));
    }
}
