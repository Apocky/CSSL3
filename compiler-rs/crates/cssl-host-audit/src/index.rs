//! § index — multi-source audit-log ingest + query surface.
//! ════════════════════════════════════════════════════════════════════
//!
//! [`AuditIndex`] owns a flat `Vec<AuditRow>` sorted by `ts_micros`.
//! Construction via [`AuditIndex::ingest_dir`] walks a directory and
//! dispatches each `*.log` / `*.jsonl` file to the parser whose
//! filename matches.
//!
//! Filename → parser mapping :
//!
//!   - `loa_runtime.log` (or `loa_runtime_*.log`) → `source_runtime`
//!   - `loa_telemetry.jsonl` (or `loa_events.jsonl`) → `source_telemetry`
//!   - `asset_fetch.jsonl` / `http_*.jsonl` → `source_network`
//!   - `intent.jsonl` → `source_intent`
//!   - `spontaneous.jsonl` → `source_spontaneous`
//!   - anything else → SKIPPED (counted in `malformed_count`)
//!
//! Query iterators all return `&AuditRow` and are zero-allocation.

use crate::row::{AuditLevel, AuditRow, AuditSource};
use crate::source_intent::parse_intent_line;
use crate::source_network::parse_network_line;
use crate::source_runtime::parse_runtime_line;
use crate::source_spontaneous::parse_spontaneous_line;
use crate::source_telemetry::parse_telemetry_line;
use std::fs;
use std::io;
use std::path::Path;

/// Sorted-by-time index of audit rows from one or more JSONL/log streams.
#[derive(Debug, Default, Clone)]
pub struct AuditIndex {
    /// All rows, sorted by `ts_micros` ascending after ingest.
    pub rows: Vec<AuditRow>,
    /// Number of input lines that could not be parsed (across all
    /// sources). Useful for the summarizer to flag corrupt streams.
    pub malformed_count: u64,
}

impl AuditIndex {
    /// Build an empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Walk `dir` (non-recursive) and ingest every recognized
    /// `*.log` / `*.jsonl` file. Returns the assembled index, sorted
    /// by `ts_micros` ascending. I/O errors on individual files are
    /// silently skipped (counted in `malformed_count`) so a single
    /// unreadable file cannot abort the whole ingest.
    pub fn ingest_dir<P: AsRef<Path>>(dir: P) -> io::Result<Self> {
        let mut idx = Self::new();
        let entries = fs::read_dir(dir.as_ref())?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let parser = pick_parser(name);
            let Some(parse_fn) = parser else {
                continue;
            };
            let Ok(content) = fs::read_to_string(&path) else {
                idx.malformed_count = idx.malformed_count.saturating_add(1);
                continue;
            };
            for line in content.lines() {
                match parse_fn(line) {
                    Some(row) => idx.rows.push(row),
                    None => {
                        // Don't count blank lines as malformed.
                        if !line.trim().is_empty() {
                            idx.malformed_count = idx.malformed_count.saturating_add(1);
                        }
                    }
                }
            }
        }
        idx.rows.sort_by_key(|r| r.ts_micros);
        Ok(idx)
    }

    /// Iterate all rows in chronological order.
    pub fn iter(&self) -> impl Iterator<Item = &AuditRow> {
        self.rows.iter()
    }

    /// Iterate rows whose source equals `src`.
    pub fn filter_source<'a>(
        &'a self,
        src: AuditSource,
    ) -> impl Iterator<Item = &'a AuditRow> + 'a {
        self.rows.iter().filter(move |r| r.source == src)
    }

    /// Iterate rows with `level >= lvl`.
    pub fn filter_min_level<'a>(
        &'a self,
        lvl: AuditLevel,
    ) -> impl Iterator<Item = &'a AuditRow> + 'a {
        self.rows.iter().filter(move |r| r.level >= lvl)
    }

    /// Iterate rows whose `ts_micros` falls in `[t0, t1]` (inclusive).
    pub fn between<'a>(&'a self, t0: u64, t1: u64) -> impl Iterator<Item = &'a AuditRow> + 'a {
        self.rows
            .iter()
            .filter(move |r| r.ts_micros >= t0 && r.ts_micros <= t1)
    }

    /// Number of rows in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True if the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Type of a single-line parser : take a `&str`, return an optional row.
type ParseFn = fn(&str) -> Option<AuditRow>;

/// Pick the parser for a filename based on canonical naming conventions.
fn pick_parser(name: &str) -> Option<ParseFn> {
    let ext_is = |target: &str| {
        std::path::Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(target))
    };
    if name == "loa_runtime.log" || (name.starts_with("loa_runtime_") && ext_is("log")) {
        Some(parse_runtime_line)
    } else if name == "loa_telemetry.jsonl" || name == "loa_events.jsonl" {
        Some(parse_telemetry_line)
    } else if name == "asset_fetch.jsonl"
        || (name.starts_with("http_") && ext_is("jsonl"))
    {
        Some(parse_network_line)
    } else if name == "intent.jsonl" {
        Some(parse_intent_line)
    } else if name == "spontaneous.jsonl" {
        Some(parse_spontaneous_line)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fresh_dir(label: &str) -> PathBuf {
        let nano = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cssl-host-audit-{label}-{nano}"));
        fs::create_dir_all(&dir).expect("mk tmp dir");
        dir
    }

    #[test]
    fn empty_dir_yields_empty_index() {
        let dir = fresh_dir("empty");
        let idx = AuditIndex::ingest_dir(&dir).expect("ingest");
        assert!(idx.is_empty());
        assert_eq!(idx.malformed_count, 0);
    }

    #[test]
    fn ingests_multi_source_directory() {
        let dir = fresh_dir("multi");
        fs::write(
            dir.join("loa_runtime.log"),
            "[2026-04-30T00:00:00Z] [INFO] [startup] launched\n\
             [2026-04-30T00:00:01Z] [WARN] [foo] something\n",
        )
        .expect("write runtime");
        fs::write(
            dir.join("loa_telemetry.jsonl"),
            "{\"ts\":\"2026-04-30T00:00:02Z\",\"kind\":\"frame\",\"fps\":60.0}\n",
        )
        .expect("write telemetry");
        fs::write(
            dir.join("asset_fetch.jsonl"),
            "{\"ts\":\"2026-04-30T00:00:03Z\",\"method\":\"GET\",\"url\":\"https://x/y\",\"status\":200,\"cap\":\"granted\"}\n",
        )
        .expect("write asset");
        fs::write(
            dir.join("intent.jsonl"),
            "{\"ts\":\"2026-04-30T00:00:04Z\",\"kind\":\"intent_classify\",\"query\":\"hi\"}\n",
        )
        .expect("write intent");
        fs::write(
            dir.join("spontaneous.jsonl"),
            "{\"ts\":\"2026-04-30T00:00:05Z\",\"kind\":\"manifest\",\"seed\":\"x\"}\n",
        )
        .expect("write spont");

        let idx = AuditIndex::ingest_dir(&dir).expect("ingest");
        assert_eq!(idx.len(), 6);
        // First row is the earliest (runtime startup at 0:00:00).
        assert_eq!(idx.rows[0].source, AuditSource::Runtime);
        assert_eq!(idx.rows[0].kind, "startup");
    }

    #[test]
    fn rows_are_sorted_by_time() {
        let dir = fresh_dir("sorted");
        // Write rows out-of-order across two files.
        fs::write(
            dir.join("loa_runtime.log"),
            "[2026-04-30T00:00:05Z] [INFO] [a] late\n\
             [2026-04-30T00:00:01Z] [INFO] [b] early\n",
        )
        .expect("write");
        fs::write(
            dir.join("intent.jsonl"),
            "{\"ts\":\"2026-04-30T00:00:03Z\",\"kind\":\"x\",\"query\":\"mid\"}\n",
        )
        .expect("write");

        let idx = AuditIndex::ingest_dir(&dir).expect("ingest");
        let times: Vec<u64> = idx.iter().map(|r| r.ts_micros).collect();
        let mut sorted = times.clone();
        sorted.sort_unstable();
        assert_eq!(times, sorted, "must be ts-sorted");
    }

    #[test]
    fn filter_source_isolates_runtime() {
        let dir = fresh_dir("filter-src");
        fs::write(
            dir.join("loa_runtime.log"),
            "[2026-04-30T00:00:00Z] [INFO] [startup] s\n",
        )
        .expect("");
        fs::write(
            dir.join("loa_telemetry.jsonl"),
            "{\"ts\":\"2026-04-30T00:00:01Z\",\"kind\":\"frame\"}\n",
        )
        .expect("");
        let idx = AuditIndex::ingest_dir(&dir).expect("ingest");
        let count = idx.filter_source(AuditSource::Runtime).count();
        assert_eq!(count, 1);
        let count_t = idx.filter_source(AuditSource::Telemetry).count();
        assert_eq!(count_t, 1);
    }

    #[test]
    fn filter_min_level_drops_below() {
        let dir = fresh_dir("filter-lvl");
        fs::write(
            dir.join("loa_runtime.log"),
            "[2026-04-30T00:00:00Z] [INFO] [a] info-row\n\
             [2026-04-30T00:00:01Z] [WARN] [b] warn-row\n\
             [2026-04-30T00:00:02Z] [ERROR] [c] error-row\n\
             [2026-04-30T00:00:03Z] [FATAL] [d/panic_hook] crit-row\n",
        )
        .expect("");
        let idx = AuditIndex::ingest_dir(&dir).expect("ingest");
        let warn_or_higher = idx.filter_min_level(AuditLevel::Warn).count();
        assert_eq!(warn_or_higher, 3);
        let err_or_higher = idx.filter_min_level(AuditLevel::Error).count();
        assert_eq!(err_or_higher, 2);
        let crit_only = idx.filter_min_level(AuditLevel::Critical).count();
        assert_eq!(crit_only, 1);
    }

    #[test]
    fn between_inclusive_window() {
        let dir = fresh_dir("between");
        fs::write(
            dir.join("loa_runtime.log"),
            "[2026-04-30T00:00:00Z] [INFO] [a] zero\n\
             [2026-04-30T00:00:01Z] [INFO] [b] one\n\
             [2026-04-30T00:00:02Z] [INFO] [c] two\n",
        )
        .expect("");
        let idx = AuditIndex::ingest_dir(&dir).expect("ingest");
        let t0 = idx.rows[0].ts_micros;
        let t1 = idx.rows[1].ts_micros;
        let cnt = idx.between(t0, t1).count();
        assert_eq!(cnt, 2);
    }
}
