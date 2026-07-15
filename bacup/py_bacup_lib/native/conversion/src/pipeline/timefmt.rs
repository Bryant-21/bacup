//! UTC ISO-8601 formatting for run_state.json without a calendar dep.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Millisecond-precision UTC ISO-8601, e.g. `2026-06-09T18:04:11.503Z`.
/// Times before the epoch clamp to the epoch (never panics).
pub fn iso8601_utc(t: SystemTime) -> String {
    let d: Duration = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = d.as_secs();
    let millis = d.subsec_millis();
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (y, m, dd) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{dd:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 → (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(epoch_secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(epoch_secs)
    }

    #[test]
    fn formats_epoch() {
        assert_eq!(iso8601_utc(at(0)), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn formats_end_of_day_and_rollover() {
        assert_eq!(iso8601_utc(at(86_399)), "1970-01-01T23:59:59.000Z");
        assert_eq!(iso8601_utc(at(86_400)), "1970-01-02T00:00:00.000Z");
    }

    #[test]
    fn handles_leap_days() {
        assert_eq!(iso8601_utc(at(951_782_400)), "2000-02-29T00:00:00.000Z");
        assert_eq!(iso8601_utc(at(1_709_164_800)), "2024-02-29T00:00:00.000Z");
    }

    #[test]
    fn formats_recent_date_and_millis() {
        assert_eq!(iso8601_utc(at(1_735_689_600)), "2025-01-01T00:00:00.000Z");
        let t = UNIX_EPOCH + Duration::from_millis(1_500);
        assert_eq!(iso8601_utc(t), "1970-01-01T00:00:01.500Z");
    }

    #[test]
    fn pre_epoch_clamps_instead_of_panicking() {
        let t = UNIX_EPOCH - Duration::from_secs(5);
        assert_eq!(iso8601_utc(t), "1970-01-01T00:00:00.000Z");
    }
}
