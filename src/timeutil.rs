//! No date/time crate in this project (see push.rs's `chrono_free_today`) —
//! this is the read side of the same trade-off: turning a transcript's ISO
//! 8601 timestamps ("2026-08-01T12:02:49.435Z") into comparable seconds, so
//! the optimization engine can measure real elapsed time (session duration
//! for burn rate, day-span for the CLAUDE.md monthly estimate) without
//! pulling in a whole date library for it.

/// Parses an ISO 8601 UTC timestamp into seconds since the Unix epoch.
/// Returns `None` for anything that doesn't look like
/// `YYYY-MM-DDTHH:MM:SS[.fff]Z` — callers treat a missing/malformed
/// timestamp as "no signal", not an error.
pub fn parse_epoch_seconds(ts: &str) -> Option<f64> {
    if ts.len() < 19 {
        return None;
    }
    let y: i64 = ts.get(0..4)?.parse().ok()?;
    let m: i64 = ts.get(5..7)?.parse().ok()?;
    let d: i64 = ts.get(8..10)?.parse().ok()?;
    let hh: i64 = ts.get(11..13)?.parse().ok()?;
    let mm: i64 = ts.get(14..16)?.parse().ok()?;
    let ss: f64 = ts.get(17..)?.trim_end_matches('Z').parse().ok()?;

    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }

    let days = days_from_civil(y, m, d);
    Some(days as f64 * 86_400.0 + hh as f64 * 3600.0 + mm as f64 * 60.0 + ss)
}

/// Days since the Unix epoch for a given civil (Gregorian) date. Howard
/// Hinnant's `days_from_civil` algorithm (public domain) — the exact
/// inverse of `civil_from_days` in push.rs, so the two round-trip.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]: Mar=0 .. Feb=11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_start_is_zero() {
        assert_eq!(parse_epoch_seconds("1970-01-01T00:00:00.000Z"), Some(0.0));
    }

    #[test]
    fn one_day_is_86400_seconds_later() {
        let a = parse_epoch_seconds("2026-08-01T00:00:00.000Z").unwrap();
        let b = parse_epoch_seconds("2026-08-02T00:00:00.000Z").unwrap();
        assert!((b - a - 86_400.0).abs() < 0.001);
    }

    #[test]
    fn handles_timestamps_without_fractional_seconds() {
        assert!(parse_epoch_seconds("2026-08-01T12:00:00Z").is_some());
    }

    #[test]
    fn one_hour_apart_differs_by_3600_seconds() {
        let a = parse_epoch_seconds("2026-08-01T10:00:00.000Z").unwrap();
        let b = parse_epoch_seconds("2026-08-01T11:00:00.000Z").unwrap();
        assert!((b - a - 3600.0).abs() < 0.001);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_epoch_seconds("not a timestamp"), None);
        assert_eq!(parse_epoch_seconds(""), None);
    }

    #[test]
    fn thirty_days_apart_is_2_592_000_seconds() {
        let a = parse_epoch_seconds("2026-07-02T00:00:00.000Z").unwrap();
        let b = parse_epoch_seconds("2026-08-01T00:00:00.000Z").unwrap();
        assert!((b - a - 30.0 * 86_400.0).abs() < 0.001);
    }
}
