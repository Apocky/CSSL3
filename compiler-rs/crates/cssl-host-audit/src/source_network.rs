//! § source_network — parser for `asset_fetch.jsonl` + `http_*.jsonl`.
//! ════════════════════════════════════════════════════════════════════
//!
//! Network-audit rows record the full HTTP-egress trail for the host :
//! request URL, method, status, bytes-fetched, cap-decision, cache
//! hit/miss. Canonical shape (cssl-asset-fetcher) :
//!
//! ```json
//! {"ts":"2026-04-30T00:00:00Z","method":"GET","url":"https://...",
//!  "status":200,"bytes":4096,"cap":"granted","cache":"miss"}
//! ```
//!
//! When a request was denied by the cap-system, the emitter writes
//! `"cap":"denied"` and an explanation in `"reason"`. We map this to
//! [`AuditLevel::Error`] and flag `sovereign_cap_used = true`.
//!
//! Lifecycle :
//!   - `cap = "granted"` + `status < 400`        → Info  · cap-flagged
//!   - `cap = "granted"` + `400 <= status < 500` → Warn  · cap-flagged
//!   - `cap = "granted"` + `status >= 500`       → Error · cap-flagged
//!   - `cap = "denied"`                          → Error · cap-flagged

use crate::row::{AuditLevel, AuditRow, AuditSource};
use crate::source_runtime::parse_iso_to_micros;
use serde_json::Value;

/// Parse a single line from `asset_fetch.jsonl` (or any `http_*.jsonl`)
/// into an [`AuditRow`]. Returns `None` for blank or malformed lines.
#[must_use]
pub fn parse_network_line(line: &str) -> Option<AuditRow> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let obj = v.as_object()?;

    let ts_iso = obj
        .get("ts")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let ts_micros = parse_iso_to_micros(&ts_iso);

    let cap = obj.get("cap").and_then(Value::as_str).unwrap_or("granted");
    let status = obj
        .get("status")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let level = if cap == "denied" || status >= 500 {
        AuditLevel::Error
    } else if status >= 400 {
        AuditLevel::Warn
    } else {
        AuditLevel::Info
    };

    // Network rows are always sovereign-cap-relevant : either egress
    // happened (granted) or was blocked (denied) — both are auditable.
    let sovereign_cap_used = true;

    let kind = if cap == "denied" {
        "http_denied".to_string()
    } else {
        format!(
            "http_{}",
            obj.get("method")
                .and_then(Value::as_str)
                .unwrap_or("request")
                .to_ascii_lowercase()
        )
    };

    let message = obj
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut kv = Vec::new();
    for (k, val) in obj {
        if matches!(k.as_str(), "ts" | "url") {
            continue;
        }
        kv.push((k.clone(), value_to_string(val)));
    }

    Some(AuditRow {
        ts_iso,
        ts_micros,
        source: AuditSource::AssetFetch,
        level,
        kind,
        message,
        sovereign_cap_used,
        kv,
    })
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_success_2xx() {
        let row = parse_network_line(
            r#"{"ts":"2026-04-30T00:00:00Z","method":"GET","url":"https://example.org/asset.glb","status":200,"bytes":4096,"cap":"granted"}"#,
        )
        .expect("parse ok");
        assert_eq!(row.source, AuditSource::AssetFetch);
        assert_eq!(row.level, AuditLevel::Info);
        assert_eq!(row.kind, "http_get");
        assert!(row.sovereign_cap_used);
        assert_eq!(row.kv_get("status"), Some("200"));
        assert_eq!(row.kv_get("bytes"), Some("4096"));
        assert!(row.message.contains("example.org"));
    }

    #[test]
    fn parses_denied_by_caps() {
        let row = parse_network_line(
            r#"{"ts":"2026-04-30T00:00:00Z","method":"GET","url":"https://blocked.org/x","cap":"denied","reason":"egress-not-allow-listed"}"#,
        )
        .expect("parse ok");
        assert_eq!(row.level, AuditLevel::Error);
        assert_eq!(row.kind, "http_denied");
        assert!(row.is_error());
        assert!(row.sovereign_cap_used);
        assert_eq!(row.kv_get("reason"), Some("egress-not-allow-listed"));
    }

    #[test]
    fn parses_4xx_as_warn() {
        let row = parse_network_line(
            r#"{"ts":"2026-04-30T00:00:00Z","method":"GET","url":"https://example.org/missing","status":404,"cap":"granted"}"#,
        )
        .expect("parse ok");
        assert_eq!(row.level, AuditLevel::Warn);
        assert!(!row.is_error());
        assert!(row.is_security_relevant());
    }

    #[test]
    fn parses_5xx_as_error() {
        let row = parse_network_line(
            r#"{"ts":"2026-04-30T00:00:00Z","method":"GET","url":"https://example.org/down","status":503,"cap":"granted"}"#,
        )
        .expect("parse ok");
        assert_eq!(row.level, AuditLevel::Error);
        assert!(row.is_error());
    }
}
