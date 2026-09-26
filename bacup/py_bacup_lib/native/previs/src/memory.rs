use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;

#[derive(Default, Serialize)]
pub struct MemoryPeaks {
    pub sample_interval_ms: u64,
    pub samples: usize,
    pub sampled_peak_working_set_bytes: usize,
    pub sampled_peak_private_bytes: usize,
}

#[derive(Default)]
pub struct MemoryMonitor {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<MemoryPeaks>>,
}

impl MemoryMonitor {
    pub fn start() -> Self {
        if !cfg!(windows) {
            return Self::default();
        }
        let (stop, receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let mut peaks = MemoryPeaks { sample_interval_ms: 250, ..MemoryPeaks::default() };
            loop {
                if let Some((working, private)) = sample() {
                    peaks.samples += 1;
                    peaks.sampled_peak_working_set_bytes = peaks.sampled_peak_working_set_bytes.max(working);
                    peaks.sampled_peak_private_bytes = peaks.sampled_peak_private_bytes.max(private);
                }
                match receiver.recv_timeout(Duration::from_millis(peaks.sample_interval_ms)) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    _ => break,
                }
            }
            peaks
        });
        Self { stop: Some(stop), thread: Some(thread) }
    }

    pub fn finish(&mut self) -> Option<MemoryPeaks> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.thread.take().and_then(|thread| thread.join().ok()).filter(|peaks| peaks.samples > 0)
    }
}

impl Drop for MemoryMonitor {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(windows)]
fn sample() -> Option<(usize, usize)> {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let mut counters = PROCESS_MEMORY_COUNTERS_EX { cb: size, ..unsafe { std::mem::zeroed() } };
    // The EX structure extends the API's base layout; cb selects its full size.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(), size) };
    (ok != 0).then_some((counters.WorkingSetSize, counters.PrivateUsage))
}

#[cfg(not(windows))]
fn sample() -> Option<(usize, usize)> {
    None
}
