//! # HistogramRegistry : map of name → Histogram
//!
//! Per-process (or per-worker) registry that owns a set of named histograms.
//! `record(name, value)` does get-or-create, so callers don't have to
//! pre-declare streams ; spontaneous metric names are first-class.
//!
//! Concurrency : the registry is `&mut` for record. Callers serialize access
//! (one registry per worker thread, then merge ; or wrap in a `Mutex` /
//! `parking_lot::Mutex` at the call site). This keeps the hot path lock-free
//! when the worker model is one-registry-per-thread.
//!
//! ## Reports
//!
//! - [`HistogramRegistry::report_text`] : human-readable table with
//!   name / count / mean / p50 / p95 / p99 columns. Suitable for in-game
//!   debug overlays + CLI dumps.
//! - [`HistogramRegistry::report_jsonl`] : one JSON object per line,
//!   suitable for replay-bundle inclusion + downstream log-aggregator
//!   pipelines.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::histogram::Histogram;

/// Registry of named histograms ; one entry per stream-name.
#[derive(Debug, Clone, Default)]
pub struct HistogramRegistry {
    hists: HashMap<String, Histogram>,
}

impl HistogramRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a sample under `name`, creating the histogram on first sight.
    ///
    /// O(1) amortized : one HashMap probe + one `Histogram::record`. Allocates
    /// only on first-record-for-this-name (the histogram + name string).
    pub fn record(&mut self, name: &str, value_us: u64) {
        let entry = self
            .hists
            .entry(name.to_string())
            .or_insert_with(|| Histogram::new(name));
        entry.record(value_us);
    }

    /// Get a reference to the histogram for `name`, if present.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Histogram> {
        self.hists.get(name)
    }

    /// Snapshot of all histograms, sorted by name.
    ///
    /// Returns references — no allocation for the histogram bodies. Use
    /// `iter().cloned().collect()` if owned snapshots are needed.
    #[must_use]
    pub fn snapshot(&self) -> Vec<&Histogram> {
        let mut hs: Vec<&Histogram> = self.hists.values().collect();
        hs.sort_by(|a, b| a.name.cmp(&b.name));
        hs
    }

    /// Plain-text tabular report : one row per histogram, sorted by name.
    ///
    /// Columns : `name | count | mean(us) | p50 | p95 | p99`. Headers + a
    /// trailing summary row are NOT included to keep the output greppable.
    /// Each row is `\n`-terminated.
    #[must_use]
    pub fn report_text(&self) -> String {
        // Pre-size to roughly avoid reallocation for typical telemetry
        // workloads of < 64 streams.
        let mut out = String::with_capacity(self.hists.len() * 96);
        // Header
        out.push_str(
            "name                                              count       mean(us)         p50         p95         p99\n",
        );
        for h in self.snapshot() {
            // 50-char name column ; truncate-with-ellipsis to keep table shape.
            let name_col = if h.name.len() > 50 {
                format!("{}…", &h.name[..49])
            } else {
                h.name.clone()
            };
            // Reasonable formatting for telemetry-grain values ; mean uses
            // 1-decimal precision.
            // `write!` on a `String` is infallible (the only error variant from
            // the `Write` impl on `String` is `Infallible`), so panics in
            // library code via `expect` are still avoided.
            // We use a raw `format_args!`-equivalent through `write!` with a
            // direct unwrap on infallible.
            let _ = writeln!(
                out,
                "{name_col:<50}{count:>10} {mean:>14.1} {p50:>11} {p95:>11} {p99:>11}",
                count = h.count,
                mean = h.mean_us(),
                p50 = h.p50(),
                p95 = h.p95(),
                p99 = h.p99(),
            );
        }
        out
    }

    /// JSON-Lines report : one JSON object per line, sorted by name.
    ///
    /// Each line is the full `Histogram` (via serde) — round-trippable through
    /// [`serde_json::from_str::<Histogram>`].
    #[must_use]
    pub fn report_jsonl(&self) -> String {
        let mut out = String::new();
        for h in self.snapshot() {
            // serde_json::to_string is fallible only on serializer-rejected
            // types — all our fields are plain integers / strings / arrays
            // and round-trip cleanly. On the (impossible) error path we
            // emit an empty line to keep line-count stable rather than
            // panicking in library code.
            match serde_json::to_string(h) {
                Ok(line) => {
                    out.push_str(&line);
                    out.push('\n');
                }
                Err(_) => out.push('\n'),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § empty registry : snapshot empty, get returns None, reports include
    /// only the header (text) or are empty (jsonl).
    #[test]
    fn empty_registry() {
        let reg = HistogramRegistry::new();
        assert!(reg.snapshot().is_empty());
        assert!(reg.get("anything").is_none());
        // text report has the header only
        let txt = reg.report_text();
        assert!(txt.starts_with("name"), "text report missing header");
        // jsonl report empty for empty registry
        let jsonl = reg.report_jsonl();
        assert!(jsonl.is_empty(), "expected empty jsonl, got {jsonl:?}");
    }

    /// § record(name, v) creates the histogram on first sight.
    #[test]
    fn record_creates_histogram() {
        let mut reg = HistogramRegistry::new();
        reg.record("frame.total", 16_667);
        assert!(reg.get("frame.total").is_some());
        let h = reg.get("frame.total").expect("present");
        assert_eq!(h.count(), 1);
        // Recording again into the same name uses the existing histogram
        reg.record("frame.total", 16_500);
        let h2 = reg.get("frame.total").expect("present");
        assert_eq!(h2.count(), 2);
    }

    /// § different names are isolated streams.
    #[test]
    fn multi_name_isolation() {
        let mut reg = HistogramRegistry::new();
        reg.record("a", 100);
        reg.record("a", 200);
        reg.record("b", 1000);
        assert_eq!(reg.get("a").map(Histogram::count), Some(2));
        assert_eq!(reg.get("b").map(Histogram::count), Some(1));
        // a's stats should NOT include b's sample
        let a_max = reg.get("a").unwrap().max_us;
        assert_eq!(a_max, 200);
    }

    /// § snapshot is sorted by name (lexicographic).
    #[test]
    fn snapshot_sorted() {
        let mut reg = HistogramRegistry::new();
        reg.record("z.last", 1);
        reg.record("a.first", 1);
        reg.record("m.middle", 1);
        let snap = reg.snapshot();
        let names: Vec<&str> = snap.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["a.first", "m.middle", "z.last"]);
    }

    /// § text report : one row per histogram (plus header), newline-terminated,
    /// includes count + mean + p50 + p95 + p99 columns.
    #[test]
    fn text_report_format() {
        let mut reg = HistogramRegistry::new();
        for v in [10_000u64, 12_000, 14_000, 16_000, 18_000] {
            reg.record("frame", v);
        }
        let txt = reg.report_text();
        // header row + 1 data row = 2 newlines minimum
        let line_count = txt.matches('\n').count();
        assert!(line_count >= 2, "expected ≥2 lines, got {line_count}: {txt}");
        // contains the stream name
        assert!(txt.contains("frame"), "missing 'frame' name: {txt}");
        // contains count of 5
        assert!(txt.contains('5'), "missing count value 5");
    }

    /// § JSONL report round-trips through `serde_json::from_str::<Histogram>`.
    #[test]
    fn jsonl_roundtrip() {
        let mut reg = HistogramRegistry::new();
        reg.record("alpha", 100);
        reg.record("alpha", 200);
        reg.record("beta", 1000);
        let jsonl = reg.report_jsonl();
        let lines: Vec<&str> = jsonl.lines().collect();
        assert_eq!(lines.len(), 2);
        // Each line parses as a Histogram
        for line in &lines {
            let parsed: Histogram = serde_json::from_str(line).expect("valid json line");
            assert!(parsed.count > 0);
        }
        // First line should be 'alpha' (sorted)
        let first: Histogram = serde_json::from_str(lines[0]).expect("parse");
        assert_eq!(first.name, "alpha");
        assert_eq!(first.count, 2);
    }
}
