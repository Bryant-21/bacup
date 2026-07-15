//! Process resident-set size for run_state.json (working set on Windows).

/// Best-effort: 0 when the platform query fails (never an error path).
pub fn current_rss_bytes() -> u64 {
    memory_stats::memory_stats().map_or(0, |s| s.physical_mem as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_nonzero_rss_for_this_process() {
        assert!(
            current_rss_bytes() > 1024 * 1024,
            "test process is surely >1 MiB resident"
        );
    }
}
