//! § source_runtime — parser for `loa_runtime.log`.
//! ════════════════════════════════════════════════════════════════════
//!
//! cssl-rt's `log_event` writes lines in the canonical format :
//!
//! ```text
//! [<ISO-TS>] [<LEVEL>] [<SOURCE>] <message>
//! ```
//!
//! Example :
//!
//! ```text
//! [2026-04-30T00:00:00Z] [INFO] [loa_startup] § LoA-v13 starting · pure-CSSL native
//! [2026-04-30T00:00:01Z] [FATAL] [loa_startup/panic_hook] panic at src/foo.rs:42 — assertion failed
//! ```
//!
//! Lines that do not match this shape are SKIPPED (return `None`) so a
//! malformed entry never aborts an ingest run.

use crate::row::{AuditLevel, AuditRow, AuditSource};

/// Parse a single line from `loa_runtime.log` into an [`AuditRow`].
/// Returns `None` for blank lines or lines that do not match the
/// canonical `[ts] [LEVEL] [SOURCE] msg` shape.
#[must_use]
pub fn parse_runtime_line(line: &str) -> Option<AuditRow> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() {
        return None;
    }
    // Walk three `[...]` brackets at the front.
    let (ts, rest) = take_bracketed(line)?;
    let (level_str, rest) = take_bracketed(rest.trim_start())?;
    let (src_str, rest) = take_bracketed(rest.trim_start())?;
    let message = rest.trim_start().to_string();

    let level = AuditLevel::parse_lossy(level_str);
    let kind = src_str.to_string();
    let sovereign_cap_used = kind.contains("panic_hook")
        || kind.contains("/cap")
        || level >= AuditLevel::Critical;

    Some(AuditRow {
        ts_iso: ts.to_string(),
        ts_micros: parse_iso_to_micros(ts),
        source: AuditSource::Runtime,
        level,
        kind,
        message,
        sovereign_cap_used,
        kv: Vec::new(),
    })
}

/// Extract the contents of a leading `[...]` bracket. Returns the
/// inner text + the rest of the line. Returns `None` if the line does
/// not start with `[` or has no closing `]`.
fn take_bracketed(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('[')?;
    let end = s.find(']')?;
    Some((&s[..end], &s[end + 1..]))
}

/// Parse an ISO-8601-ish UTC timestamp (`YYYY-MM-DDTHH:MM:SS[.fff]Z`)
/// into microseconds-since-epoch. Returns 0 on parse failure ; the
/// caller treats `0` as the "no-timestamp" sentinel.
///
/// This is a deliberately-tiny zero-dependency parser ; full RFC-3339
/// support is the caller's problem (use `chrono` if you need it).
#[must_use]
pub fn parse_iso_to_micros(s: &str) -> u64 {
    // Tolerate a trailing 'Z' or '+00:00'.
    let core = s
        .strip_suffix('Z')
        .or_else(|| s.split_once('+').map(|(a, _)| a))
        .unwrap_or(s);
    let Some((date, time)) = core.split_once('T') else {
        return 0;
    };
    let mut date_parts = date.split('-');
    let (Some(y), Some(m), Some(d)) = (date_parts.next(), date_parts.next(), date_parts.next())
    else {
        return 0;
    };
    let (time_main, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut t_parts = time_main.split(':');
    let (Some(hh), Some(mm), Some(ss)) = (t_parts.next(), t_parts.next(), t_parts.next()) else {
        return 0;
    };
    let (Ok(y), Ok(mo), Ok(d), Ok(hh), Ok(mm), Ok(ss)) = (
        y.parse::<i64>(),
        m.parse::<i64>(),
        d.parse::<i64>(),
        hh.parse::<i64>(),
        mm.parse::<i64>(),
        ss.parse::<i64>(),
    ) else {
        return 0;
    };
    // Pad/truncate frac to 6 digits (microseconds).
    let mut frac_buf = String::with_capacity(6);
    for c in frac.chars().take(6) {
        frac_buf.push(c);
    }
    while frac_buf.len() < 6 {
        frac_buf.push('0');
    }
    let micros_frac: i64 = frac_buf.parse().unwrap_or(0);
    let days = days_from_civil(y, mo, d);
    let secs = days * 86400 + hh * 3600 + mm * 60 + ss;
    if secs < 0 {
        return 0;
    }
    secs as u64 * 1_000_000 + micros_frac as u64
}

/// Howard Hinnant's days_from_civil algorithm — y/m/d → days-since-epoch.
/// Public-domain ; used here because we have no chrono dependency.
#[allow(clippy::similar_names)] // Hinnant's algorithm names yoe/doy/doe — preserve
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let day_of_year = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let day_of_era = yoe * 365 + yoe / 4 - yoe / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_startup_line() {
        let row = parse_runtime_line(
            "[2026-04-30T00:00:00Z] [INFO] [loa_startup] § LoA-v13 starting · pure-CSSL native",
        )
        .expect("parse ok");
        assert_eq!(row.source, AuditSource::Runtime);
        assert_eq!(row.level, AuditLevel::Info);
        assert_eq!(row.kind, "loa_startup");
        assert!(row.message.contains("LoA-v13 starting"));
        assert_eq!(row.ts_iso, "2026-04-30T00:00:00Z");
        assert!(!row.sovereign_cap_used);
    }

    #[test]
    fn parses_panic_line_marks_cap_used() {
        let row = parse_runtime_line(
            "[2026-04-30T00:00:01Z] [FATAL] [loa_startup/panic_hook] assertion failed",
        )
        .expect("parse ok");
        assert_eq!(row.level, AuditLevel::Critical);
        assert!(row.sovereign_cap_used, "panic_hook → cap-used");
        assert!(row.is_error());
    }

    #[test]
    fn skips_blank_lines() {
        assert!(parse_runtime_line("").is_none());
        assert!(parse_runtime_line("\n").is_none());
        assert!(parse_runtime_line("   \r\n").is_none());
    }

    #[test]
    fn skips_malformed_lines() {
        // Missing brackets entirely.
        assert!(parse_runtime_line("not a log line").is_none());
        // Only one bracket.
        assert!(parse_runtime_line("[ts] no level no source").is_none());
        // Two brackets but missing source bracket.
        assert!(parse_runtime_line("[ts] [INFO] no-source-bracket").is_none());
    }

    #[test]
    fn iso_to_micros_smoke() {
        // 1970-01-01T00:00:00Z = 0.
        assert_eq!(parse_iso_to_micros("1970-01-01T00:00:00Z"), 0);
        // 1970-01-01T00:00:01Z = 1_000_000 micros.
        assert_eq!(parse_iso_to_micros("1970-01-01T00:00:01Z"), 1_000_000);
        // Malformed → 0.
        assert_eq!(parse_iso_to_micros("not-a-ts"), 0);
        // Fractional seconds (500ms).
        assert_eq!(parse_iso_to_micros("1970-01-01T00:00:00.500Z"), 500_000);
    }
}
