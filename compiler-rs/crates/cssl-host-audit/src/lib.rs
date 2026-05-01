//! § cssl-host-audit — JSONL audit-log normalizer + indexer.
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The LoA-v13 host emits multiple structured audit streams under the
//!   per-process log directory (default `logs/`) :
//!
//! ```text
//!     • loa_runtime.log     — cssl-rt panic-hook + startup events
//!                             (text-format `[ts] [LEVEL] [SOURCE] msg`)
//!     • loa_telemetry.jsonl — frame-time + counter snapshots
//!                             (one JSON object per line)
//!     • asset_fetch.jsonl   — network-audit (HTTP request/response,
//!                             cap-gating decisions, cache hit/miss)
//!     • intent.jsonl        — text-intent classify + dispatch events
//!     • spontaneous.jsonl   — seed → manifestation events
//! ```
//!
//!   This crate ingests all of them, normalizes to a single canonical
//!   row schema ([`row::AuditRow`]), indexes by time, and exposes
//!   query helpers ([`index::AuditIndex`]) + summary statistics
//!   ([`summary::AuditSummary`]).
//!
//! § DESIGN
//!   - Each per-source parser is isolated in its own module
//!     (`source_runtime` / `source_telemetry` / `source_network` /
//!     `source_intent` / `source_spontaneous`) so a malformed schema in
//!     one stream cannot corrupt the others.
//!   - Parsers are infallible : malformed lines are SKIPPED (counted)
//!     never panic. The library has zero panic-points.
//!   - `AuditIndex::ingest_dir` walks a directory and dispatches each
//!     `*.log` / `*.jsonl` file to the matching parser by filename.
//!   - Rows are sorted by `ts_micros` after ingest so iterators yield
//!     in chronological order regardless of source file ordering.
//!
//! § PRIME-DIRECTIVE binding
//!   - `AuditRow::is_security_relevant` flags rows where a sovereign
//!     capability was used (network egress / render-mode change /
//!     cap-deny event) so a human auditor can grep the full audit-trail
//!     for substrate-affecting operations without trawling raw JSON.
//!   - The crate NEVER writes outbound — it is a pure ingest+query
//!     surface. No new audit streams originate here.
//!
//! § DEPENDENCIES
//!   Only `serde` + `serde_json`. No tokio / no async / no FS-watch ;
//!   the library is sync-only and reads files via `std::fs`.

#![forbid(unsafe_code)]

pub mod index;
pub mod row;
pub mod source_intent;
pub mod source_network;
pub mod source_runtime;
pub mod source_spontaneous;
pub mod source_telemetry;
pub mod summary;

pub use index::AuditIndex;
pub use row::{AuditLevel, AuditRow, AuditSource};
pub use summary::{report_text, summarize, AuditSummary};
