//! Time helpers. Every `*_at` column in §33 is stored as an ISO-8601 UTC
//! string; this module is the single place that formats/parses them so no
//! crate invents its own timestamp convention.

use std::time::{SystemTime, UNIX_EPOCH};

/// The current time as a Unix timestamp (seconds), used as the basis for
/// `*_at` columns (§33) before formatting.
pub fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Format a Unix timestamp (seconds) as an ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`), without pulling in a date/time crate
/// dependency (§28.5: no new dependency without justification -- this is a
/// small, dependency-free implementation of a well-defined calendar
/// calculation).
pub fn format_unix_secs(unix_secs: i64) -> String {
    let days_since_epoch = unix_secs.div_euclid(86_400);
    let secs_of_day = unix_secs.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days_since_epoch);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// The current time, formatted per [`format_unix_secs`].
pub fn now_iso8601() -> String {
    format_unix_secs(now_unix_secs())
}

/// Convert a day count since the Unix epoch to a (year, month, day) civil
/// date, using Howard Hinnant's well-known `civil_from_days` algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_epoch_zero_is_1970_01_01() {
        assert_eq!(format_unix_secs(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_known_timestamp() {
        // 2024-01-01T00:00:00Z
        assert_eq!(format_unix_secs(1_704_067_200), "2024-01-01T00:00:00Z");
    }

    #[test]
    fn format_preserves_time_of_day() {
        // 1970-01-01T01:02:03Z
        assert_eq!(format_unix_secs(3723), "1970-01-01T01:02:03Z");
    }

    #[test]
    fn now_iso8601_is_well_formed() {
        let ts = now_iso8601();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
    }
}
