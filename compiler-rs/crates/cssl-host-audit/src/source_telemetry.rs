//! § source_telemetry — parser for `loa_telemetry.jsonl`.
//! ════════════════════════════════════════════════════════════════════
//!
//! Each line is a JSON object emitted by `loa_host::telemetry` containing
//! a frame-time snapshot or counter increment. Canonical shape :
//!
//! ```json
//! {"ts":"2026-04-30T00:00:00Z","kind":"frame","fps":60.0,"p95_ms":2.1,
//!  "draw_calls":124,"vertices":8400}
//! ```
//!
//! The `kind` field discriminates between frame snapshots / counter
//! events / mode-change events. We map every one of them to a single
//! [`AuditRow`] with `source = Telemetry` and `level = Info`. A
//! `kind = "render_mode_change"` row is flagged `sovereign_cap_used`
//! because render-mode is a substrate-affecting operation under the
//! PRIME-DIRECTIVE.

use crate::row::{AuditLevel, AuditRow, AuditSource};
use crate::source_runtime::parse_iso_to_micros;
use serde_json::Value;

/// Parse a single line from `loa_telemetry.jsonl` into an [`AuditRow`].
/// Returns `None` for blank lines or lines that do not parse as JSON.
#[must_use]
pub fn parse_telemetry_line(line: &str) -> Option<AuditRow> {
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
    let kind = obj
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("frame")
        .to_string();

    let level = match kind.as_str() {
        "warn" | "warning" => AuditLevel::Warn,
        "error" => AuditLevel::Error,
        _ => AuditLevel::Info,
    };

    let sovereign_cap_used = matches!(
        kind.as_str(),
        "render_mode_change" | "mueller_apply" | "screenshot_capture" | "video_record_start"
    );

    let message = obj
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Stash everything except canonical-mapped fields into kv.
    let mut kv = Vec::new();
    for (k, val) in obj {
        if matches!(k.as_str(), "ts" | "kind" | "message") {
            continue;
        }
        kv.push((k.clone(), value_to_string(val)));
    }

    Some(AuditRow {
        ts_iso,
        ts_micros,
        source: AuditSource::Telemetry,
        level,
        kind,
        message,
        sovereign_cap_used,
        kv,
    })
}

/// Render any JSON value to a flat string for kv storage. Strings are
/// passed through unquoted ; numbers / bools / null get their JSON
/// representation ; objects + arrays are serialized via `to_string`.
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
    fn parses_frame_snapshot() {
        let row = parse_telemetry_line(
            r#"{"ts":"2026-04-30T00:00:00Z","kind":"frame","fps":60.0,"p95_ms":2.1,"draw_calls":124}"#,
        )
        .expect("parse ok");
        assert_eq!(row.source, AuditSource::Telemetry);
        assert_eq!(row.kind, "frame");
        assert_eq!(row.level, AuditLevel::Info);
        assert_eq!(row.kv_get("fps"), Some("60.0"));
        assert_eq!(row.kv_get("draw_calls"), Some("124"));
        assert!(!row.sovereign_cap_used);
    }

    #[test]
    fn render_mode_change_flags_cap() {
        let row = parse_telemetry_line(
            r#"{"ts":"2026-04-30T00:01:00Z","kind":"render_mode_change","from":"mueller","to":"spectral"}"#,
        )
        .expect("parse ok");
        assert!(row.sovereign_cap_used);
        // Source-classified as Telemetry but cap-flagged → security-relevant.
        assert!(row.is_security_relevant());
        assert_eq!(row.kv_get("from"), Some("mueller"));
    }

    #[test]
    fn skips_blank_and_malformed() {
        assert!(parse_telemetry_line("").is_none());
        assert!(parse_telemetry_line("not-json").is_none());
        // Valid JSON but not an object.
        assert!(parse_telemetry_line("[1,2,3]").is_none());
    }
}
