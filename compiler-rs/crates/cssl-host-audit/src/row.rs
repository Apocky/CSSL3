//! § row — canonical AuditRow schema.
//! ════════════════════════════════════════════════════════════════════
//!
//! All five JSONL/text streams normalize to [`AuditRow`]. Ordering is
//! by `ts_micros` (microseconds since UNIX epoch) ; `ts_iso` is kept
//! alongside as a human-readable label and is NOT used for sorting.

use serde::{Deserialize, Serialize};

/// Canonical normalized audit-row.
///
/// Each parser produces these from its native line format (text or JSON).
/// The `kv` field is an ordered key-value list of additional fields that
/// were present in the source line but did not map to a canonical column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRow {
    /// ISO-8601-ish UTC timestamp string, exactly as emitted by the source.
    /// Empty when a stream omitted a timestamp ; treat `ts_micros = 0` as
    /// the canonical "no-timestamp" sentinel for sorting.
    pub ts_iso: String,
    /// Microseconds since UNIX epoch. Used for ordering. Zero on parse
    /// failure ; rows with `ts_micros == 0` sort to the front of the index.
    pub ts_micros: u64,
    /// Stream-of-origin classifier.
    pub source: AuditSource,
    /// Severity level of the row.
    pub level: AuditLevel,
    /// Short event-kind tag (e.g. `"startup"` / `"http_request"` / `"intent_dispatch"`).
    pub kind: String,
    /// Free-form human-readable message body. May be empty.
    pub message: String,
    /// True if the row records use of a sovereign capability (network
    /// egress / render-mode change / cap-deny event / panic). Used by
    /// the summarizer to count `security_events`.
    pub sovereign_cap_used: bool,
    /// Ordered key/value pairs preserving non-canonical fields from the
    /// source line. Use [`AuditRow::kv_get`] for lookup.
    pub kv: Vec<(String, String)>,
}

/// Stream-of-origin classifier.
///
/// `Custom` carries an arbitrary tag and is used when the parser cannot
/// identify the source from a known set of canonical streams.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditSource {
    /// `loa_runtime.log` — cssl-rt panic-hook + startup events.
    Runtime,
    /// `loa_telemetry.jsonl` — frame-time + counter snapshots.
    Telemetry,
    /// `asset_fetch.jsonl` / `http_*.jsonl` — network audit.
    AssetFetch,
    /// `intent.jsonl` — text-intent classify + dispatch.
    Intent,
    /// `spontaneous.jsonl` — seed → manifestation events.
    Spontaneous,
    /// Sense-organ events (gaze / pose / haptic).
    Sense,
    /// Generic network events outside asset-fetch.
    Network,
    /// Render-mode changes (mueller / spectral / mode-switch).
    Render,
    /// Out-of-band custom tag.
    Custom(String),
}

/// Severity level. `Ord` follows numeric ordering (Trace < ... < Critical)
/// so `level >= AuditLevel::Warn` is the canonical "show me warnings+" filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AuditLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

impl AuditLevel {
    /// Parse a level string in the canonical formats accepted by the
    /// LoA logging surface : `TRACE` / `DEBUG` / `INFO` / `WARN` /
    /// `ERROR` / `CRITICAL` / `FATAL`. Case-insensitive. Unknown levels
    /// fall back to `Info` so a malformed level never silently drops a row.
    #[must_use]
    pub fn parse_lossy(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "TRACE" => Self::Trace,
            "DEBUG" => Self::Debug,
            "WARN" | "WARNING" => Self::Warn,
            "ERROR" | "ERR" => Self::Error,
            "CRITICAL" | "FATAL" => Self::Critical,
            // INFO + anything-else
            _ => Self::Info,
        }
    }
}

impl AuditRow {
    /// True if `level >= Error`.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.level >= AuditLevel::Error
    }

    /// True if the row records a sovereign-cap event OR originates from
    /// a security-sensitive source (Network / AssetFetch / Render).
    #[must_use]
    pub fn is_security_relevant(&self) -> bool {
        if self.sovereign_cap_used {
            return true;
        }
        matches!(
            self.source,
            AuditSource::AssetFetch | AuditSource::Network | AuditSource::Render
        )
    }

    /// Look up a key in [`AuditRow::kv`]. Returns the FIRST match (the
    /// list preserves source-line ordering ; duplicate keys are kept
    /// for fidelity but only the first is surfaced here).
    #[must_use]
    pub fn kv_get(&self, key: &str) -> Option<&str> {
        self.kv
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Set a `key=value` pair, replacing any existing entry under the
    /// same key. Inserts a new pair if none exists.
    pub fn kv_set(&mut self, key: String, value: String) {
        if let Some(slot) = self.kv.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.kv.push((key, value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> AuditRow {
        AuditRow {
            ts_iso: "2026-04-30T00:00:00Z".to_string(),
            ts_micros: 1_777_000_000_000_000,
            source: AuditSource::Runtime,
            level: AuditLevel::Info,
            kind: "startup".to_string(),
            message: "§ LoA-v13 starting".to_string(),
            sovereign_cap_used: false,
            kv: vec![("pid".to_string(), "12345".to_string())],
        }
    }

    #[test]
    fn round_trip_serialize() {
        let r = sample_row();
        let s = serde_json::to_string(&r).expect("serialize");
        let back: AuditRow = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(r, back);
    }

    #[test]
    fn level_ordering_total() {
        assert!(AuditLevel::Trace < AuditLevel::Debug);
        assert!(AuditLevel::Debug < AuditLevel::Info);
        assert!(AuditLevel::Info < AuditLevel::Warn);
        assert!(AuditLevel::Warn < AuditLevel::Error);
        assert!(AuditLevel::Error < AuditLevel::Critical);
        assert_eq!(AuditLevel::parse_lossy("warn"), AuditLevel::Warn);
        assert_eq!(AuditLevel::parse_lossy("FATAL"), AuditLevel::Critical);
        assert_eq!(AuditLevel::parse_lossy("nonsense"), AuditLevel::Info);
    }

    #[test]
    fn is_error_on_error_and_critical_only() {
        let mut r = sample_row();
        r.level = AuditLevel::Info;
        assert!(!r.is_error());
        r.level = AuditLevel::Warn;
        assert!(!r.is_error());
        r.level = AuditLevel::Error;
        assert!(r.is_error());
        r.level = AuditLevel::Critical;
        assert!(r.is_error());
    }

    #[test]
    fn is_security_relevant_cap_or_source() {
        let mut r = sample_row();
        // Runtime + no-cap → not security-relevant.
        assert!(!r.is_security_relevant());
        // Cap-flag flips it.
        r.sovereign_cap_used = true;
        assert!(r.is_security_relevant());
        // Or sensitive source.
        r.sovereign_cap_used = false;
        r.source = AuditSource::AssetFetch;
        assert!(r.is_security_relevant());
        r.source = AuditSource::Network;
        assert!(r.is_security_relevant());
        r.source = AuditSource::Render;
        assert!(r.is_security_relevant());
        // Telemetry alone is NOT security-relevant.
        r.source = AuditSource::Telemetry;
        assert!(!r.is_security_relevant());
    }

    #[test]
    fn kv_get_and_set_overwrite() {
        let mut r = sample_row();
        assert_eq!(r.kv_get("pid"), Some("12345"));
        assert_eq!(r.kv_get("missing"), None);
        // Overwrite existing.
        r.kv_set("pid".to_string(), "67890".to_string());
        assert_eq!(r.kv_get("pid"), Some("67890"));
        assert_eq!(r.kv.len(), 1);
        // Insert new.
        r.kv_set("user".to_string(), "apocky".to_string());
        assert_eq!(r.kv_get("user"), Some("apocky"));
        assert_eq!(r.kv.len(), 2);
    }
}
