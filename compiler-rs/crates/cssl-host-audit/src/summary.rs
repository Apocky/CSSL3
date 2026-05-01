//! § summary — aggregate statistics over an [`AuditIndex`].
//! ════════════════════════════════════════════════════════════════════
//!
//! [`AuditSummary`] is a one-shot report computed from an index :
//! row counts grouped by source + level, error count, security-event
//! count, and the time-span (max - min `ts_micros`) covered by rows
//! that had non-zero timestamps.
//!
//! [`report_text`] formats the summary as a multi-line human-readable
//! string suitable for printing to a terminal or for inclusion in an
//! audit-trail PDF.

use crate::index::AuditIndex;
use crate::row::{AuditLevel, AuditSource};
use std::collections::HashMap;
use std::fmt::Write;

/// Aggregate statistics over an [`AuditIndex`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AuditSummary {
    /// Total number of rows in the source index.
    pub total_rows: u64,
    /// Row count grouped by source.
    pub by_source: HashMap<AuditSource, u64>,
    /// Row count grouped by level.
    pub by_level: HashMap<AuditLevel, u64>,
    /// Number of rows where `is_error()` is true (level >= Error).
    pub errors: u64,
    /// Number of rows where `is_security_relevant()` is true.
    pub security_events: u64,
    /// Time-span in microseconds from the earliest to the latest
    /// `ts_micros > 0`. Zero if the index has no timestamped rows.
    pub time_span_micros: u64,
    /// Number of malformed input lines counted during ingest (carried
    /// over from `AuditIndex::malformed_count` for one-stop reporting).
    pub malformed_count: u64,
}

/// Compute an [`AuditSummary`] over the given index.
#[must_use]
pub fn summarize(idx: &AuditIndex) -> AuditSummary {
    let mut s = AuditSummary {
        total_rows: idx.rows.len() as u64,
        malformed_count: idx.malformed_count,
        ..AuditSummary::default()
    };
    let mut min_ts: Option<u64> = None;
    let mut max_ts: Option<u64> = None;

    for r in &idx.rows {
        *s.by_source.entry(r.source.clone()).or_insert(0) += 1;
        *s.by_level.entry(r.level).or_insert(0) += 1;
        if r.is_error() {
            s.errors += 1;
        }
        if r.is_security_relevant() {
            s.security_events += 1;
        }
        if r.ts_micros > 0 {
            min_ts = Some(min_ts.map_or(r.ts_micros, |m| m.min(r.ts_micros)));
            max_ts = Some(max_ts.map_or(r.ts_micros, |m| m.max(r.ts_micros)));
        }
    }

    if let (Some(lo), Some(hi)) = (min_ts, max_ts) {
        s.time_span_micros = hi.saturating_sub(lo);
    }

    s
}

/// Format an [`AuditSummary`] as a multi-line human-readable string.
///
/// Layout :
///
/// ```text
/// § AUDIT SUMMARY · N rows · span Xs
///   by source : Runtime=A · Telemetry=B · ...
///   by level  : Info=A · Warn=B · ...
///   errors    : N
///   security  : N
///   malformed : N
/// ```
#[must_use]
pub fn report_text(s: &AuditSummary) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "§ AUDIT SUMMARY · {} rows · span {}.{}s",
        s.total_rows,
        s.time_span_micros / 1_000_000,
        s.time_span_micros % 1_000_000
    );
    out.push_str("  by source :");
    let mut sources: Vec<(&AuditSource, &u64)> = s.by_source.iter().collect();
    sources.sort_by_key(|(k, _)| format!("{k:?}"));
    for (k, v) in sources {
        let _ = write!(out, " {k:?}={v}");
    }
    out.push('\n');

    out.push_str("  by level  :");
    let mut levels: Vec<(&AuditLevel, &u64)> = s.by_level.iter().collect();
    levels.sort_by_key(|(k, _)| **k);
    for (k, v) in levels {
        let _ = write!(out, " {k:?}={v}");
    }
    out.push('\n');

    let _ = writeln!(out, "  errors    : {}", s.errors);
    let _ = writeln!(out, "  security  : {}", s.security_events);
    let _ = writeln!(out, "  malformed : {}", s.malformed_count);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::AuditRow;

    fn row(src: AuditSource, lvl: AuditLevel, ts: u64) -> AuditRow {
        AuditRow {
            ts_iso: String::new(),
            ts_micros: ts,
            source: src,
            level: lvl,
            kind: String::new(),
            message: String::new(),
            sovereign_cap_used: false,
            kv: Vec::new(),
        }
    }

    #[test]
    fn empty_index_yields_zero_summary() {
        let idx = AuditIndex::new();
        let s = summarize(&idx);
        assert_eq!(s.total_rows, 0);
        assert_eq!(s.errors, 0);
        assert_eq!(s.security_events, 0);
        assert_eq!(s.time_span_micros, 0);
        let r = report_text(&s);
        assert!(r.contains("§ AUDIT SUMMARY · 0 rows"));
    }

    #[test]
    fn single_row_summary() {
        let mut idx = AuditIndex::new();
        idx.rows.push(row(AuditSource::Runtime, AuditLevel::Info, 1_000_000));
        let s = summarize(&idx);
        assert_eq!(s.total_rows, 1);
        assert_eq!(*s.by_source.get(&AuditSource::Runtime).unwrap(), 1);
        assert_eq!(*s.by_level.get(&AuditLevel::Info).unwrap(), 1);
        assert_eq!(s.errors, 0);
        // Single row → max == min → span = 0.
        assert_eq!(s.time_span_micros, 0);
    }

    #[test]
    fn multi_source_counts_correct() {
        let mut idx = AuditIndex::new();
        idx.rows
            .push(row(AuditSource::Runtime, AuditLevel::Info, 1_000_000));
        idx.rows
            .push(row(AuditSource::Runtime, AuditLevel::Error, 2_000_000));
        idx.rows.push(row(
            AuditSource::AssetFetch,
            AuditLevel::Warn,
            3_000_000,
        ));
        idx.rows
            .push(row(AuditSource::Telemetry, AuditLevel::Info, 4_000_000));

        let s = summarize(&idx);
        assert_eq!(s.total_rows, 4);
        assert_eq!(*s.by_source.get(&AuditSource::Runtime).unwrap(), 2);
        assert_eq!(*s.by_source.get(&AuditSource::AssetFetch).unwrap(), 1);
        assert_eq!(*s.by_source.get(&AuditSource::Telemetry).unwrap(), 1);
        assert_eq!(s.errors, 1, "one Error level row");
        // AssetFetch row + the Error-level Runtime row?
        // Runtime/Error is NOT security-relevant unless cap-flagged.
        // AssetFetch row is. → 1 security event.
        assert_eq!(s.security_events, 1);
        // Span = 4_000_000 - 1_000_000 = 3_000_000.
        assert_eq!(s.time_span_micros, 3_000_000);

        let txt = report_text(&s);
        assert!(txt.contains("AssetFetch"));
        assert!(txt.contains("Runtime"));
        assert!(txt.contains("errors    : 1"));
    }
}
