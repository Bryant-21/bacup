use std::collections::{BTreeMap, VecDeque};
use std::sync::{Condvar, Mutex};

pub(super) fn budget() -> (Option<usize>, usize) {
    let available = available_memory();
    (available, budget_for_available(available))
}

fn budget_for_available(available: Option<usize>) -> usize {
    available.map_or(8 << 30, |bytes| bytes.saturating_sub(2 << 30).min(16 << 30))
}

pub(super) fn estimate(source_bytes: u64) -> usize {
    // The largest measured SCOL used 7.2 GiB from a 76 MiB file, before other jobs.
    source_bytes
        .saturating_mul(96)
        .saturating_add(16 << 20)
        .min(usize::MAX as u64) as usize
}

#[cfg(windows)]
fn available_memory() -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullAvailPhys as usize)
}

#[cfg(not(windows))]
fn available_memory() -> Option<usize> {
    None
}

pub(super) struct MemoryQueue<T> {
    state: Mutex<State<T>>,
    ready: Condvar,
    budget: usize,
}

struct State<T> {
    pending: BTreeMap<usize, VecDeque<T>>,
    used: usize,
}

pub(super) struct Permit<'a, T> {
    queue: &'a MemoryQueue<T>,
    cost: usize,
}

impl<T> MemoryQueue<T> {
    pub fn new(jobs: impl IntoIterator<Item = (usize, T)>, budget: usize) -> Self {
        let mut pending = BTreeMap::<usize, VecDeque<T>>::new();
        for (cost, job) in jobs {
            pending.entry(cost.max(1)).or_default().push_back(job);
        }
        Self {
            state: Mutex::new(State { pending, used: 0 }),
            ready: Condvar::new(),
            budget,
        }
    }

    pub fn pop(&self) -> Option<(T, Permit<'_, T>)> {
        let mut state = self.state.lock().unwrap();
        loop {
            if state.pending.is_empty() {
                return None;
            }
            let available = self.budget.saturating_sub(state.used);
            // One job larger than the allowance must still make progress, alone.
            let cost = if state.used == 0 {
                state.pending.last_key_value().map(|(&cost, _)| cost)
            } else {
                state
                    .pending
                    .range(..=available)
                    .next_back()
                    .map(|(&cost, _)| cost)
            };
            if let Some(cost) = cost {
                let jobs = state.pending.get_mut(&cost).unwrap();
                let job = jobs.pop_front().unwrap();
                if jobs.is_empty() {
                    state.pending.remove(&cost);
                }
                state.used += cost;
                return Some((job, Permit { queue: self, cost }));
            }
            state = self.ready.wait(state).unwrap();
        }
    }
}

impl<T> Drop for Permit<'_, T> {
    fn drop(&mut self) {
        self.queue.state.lock().unwrap().used -= self.cost;
        self.queue.ready.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_work_fills_space_while_larger_work_waits() {
        let queue = MemoryQueue::new([(80, "large"), (70, "waiting"), (20, "small")], 100);
        let (large, large_permit) = queue.pop().unwrap();
        let (small, small_permit) = queue.pop().unwrap();
        assert_eq!((large, small), ("large", "small"));
        assert_eq!(queue.state.lock().unwrap().used, 100);
        drop(large_permit);
        let (waiting, waiting_permit) = queue.pop().unwrap();
        assert_eq!(waiting, "waiting");
        assert_eq!(queue.state.lock().unwrap().used, 90);
        drop((small_permit, waiting_permit));
        assert!(queue.pop().is_none());
    }

    #[test]
    fn oversized_work_runs_alone_and_wakes_waiters() {
        let queue = MemoryQueue::new([(80, 1), (1, 2)], 40);
        let (large, large_permit) = queue.pop().unwrap();
        assert_eq!(large, 1);
        std::thread::scope(|scope| {
            let (tx, rx) = std::sync::mpsc::channel();
            let queue = &queue;
            scope.spawn(move || {
                let (small, permit) = queue.pop().unwrap();
                tx.send(small).unwrap();
                drop(permit);
            });
            assert!(rx.try_recv().is_err());
            drop(large_permit);
            assert_eq!(
                rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
                2
            );
        });
        assert_eq!(queue.state.lock().unwrap().used, 0);
    }

    #[test]
    fn unwind_releases_cost_and_equal_costs_preserve_order() {
        let queue = MemoryQueue::new([(80, 1), (80, 2)], 100);
        assert!(
            std::panic::catch_unwind(|| {
                let (job, _permit) = queue.pop().unwrap();
                assert_eq!(job, 1);
                panic!("conversion failed");
            })
            .is_err()
        );
        assert_eq!(queue.pop().unwrap().0, 2);
    }

    #[test]
    fn budget_leaves_headroom_and_costs_include_small_job_overhead() {
        assert_eq!(budget_for_available(Some(64 << 30)), 16 << 30);
        assert_eq!(budget_for_available(Some(12 << 30)), 10 << 30);
        assert_eq!(budget_for_available(Some(6 << 30)), 4 << 30);
        assert_eq!(budget_for_available(Some(1 << 30)), 0);
        assert_eq!(budget_for_available(None), 8 << 30);
        assert_eq!(estimate(0), 16 << 20);
        assert!(estimate(79_598_737) < 8 << 30);
        assert_eq!(estimate(u64::MAX), usize::MAX);
    }

    #[test]
    fn zero_budget_still_drains_all_jobs() {
        let queue = MemoryQueue::new([(0, 1), (4, 2), (2, 3)], 0);
        assert_eq!(queue.pop().unwrap().0, 2);
        assert_eq!(queue.pop().unwrap().0, 3);
        assert_eq!(queue.pop().unwrap().0, 1);
        assert!(queue.pop().is_none());
        assert_eq!(queue.state.lock().unwrap().used, 0);
    }
}
