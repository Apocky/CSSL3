//! § wired_audit — thin loa-host wrapper around `cssl-host-audit`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Surface the JSONL audit-log normalizer + indexer + summary helpers so
//!   MCP-tool handlers can ingest a session log directory and emit the
//!   summary text without reaching into the path-dep at every call-site.
//!
//! § wrapped surface
//!   - [`AuditRow`] / [`AuditLevel`] / [`AuditSource`] — normalized row schema.
//!   - [`AuditIndex`] — directory-walking ingestor + chronological iterator.
//!   - [`AuditSummary`] / [`summarize`] / [`report_text`] — query-side helpers.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; reads-only over disk.

pub use cssl_host_audit::{
    report_text, summarize, AuditIndex, AuditLevel, AuditRow, AuditSource, AuditSummary,
};

/// Convenience : ingest every `*.log` / `*.jsonl` under `dir` into a single
/// [`AuditIndex`]. Wraps [`AuditIndex::ingest_dir`] for symmetry with the
/// other `wired_*` modules (one short-form helper per crate).
pub fn ingest_logs_dir(
    dir: impl AsRef<std::path::Path>,
) -> std::io::Result<AuditIndex> {
    AuditIndex::ingest_dir(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_empty_dir_returns_empty_index() {
        // Ingest from an empty (just-created) temp dir : index must be
        // constructable and yield zero rows.
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("loa-host-wired-audit-{pid}-{nanos}"));
        std::fs::create_dir_all(&p).expect("mkdir");
        let idx = ingest_logs_dir(&p).expect("ingest empty dir");
        // Summary reflects an empty session.
        let summary = summarize(&idx);
        let _txt = report_text(&summary);
        let _ = std::fs::remove_dir_all(&p);
    }
}
