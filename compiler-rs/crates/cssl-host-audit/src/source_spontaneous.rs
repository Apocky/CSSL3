//! § source_spontaneous — parser for `spontaneous.jsonl`.
//! ════════════════════════════════════════════════════════════════════
//!
//! Spontaneous-manifestation events record the path from a substrate
//! seed to a fully-realized in-world manifestation. Canonical shape :
//!
//! ```json
//! {"ts":"2026-04-30T00:00:00Z","kind":"manifest","seed":"creature/raven",
//!  "scene":"test_room","entity_id":42}
//! ```
//!
//! Sub-kinds : `seed_emit` (substrate → seed) / `manifest`
//! (seed → in-world entity) / `manifest_failed` (cap or budget block).
//! Failures are mapped to [`AuditLevel::Warn`].

use crate::row::{AuditLevel, AuditRow, AuditSource};
use crate::source_runtime::parse_iso_to_micros;
use serde_json::Value;

/// Parse a single line from `spontaneous.jsonl` into an [`AuditRow`].
/// Returns `None` for blank or malformed lines.
#[must_use]
pub fn parse_spontaneous_line(line: &str) -> Option<AuditRow> {
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
        .unwrap_or("manifest")
        .to_string();

    let level = match kind.as_str() {
        "manifest_failed" => AuditLevel::Warn,
        "manifest_error" => AuditLevel::Error,
        _ => AuditLevel::Info,
    };

    let message = obj
        .get("seed")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut kv = Vec::new();
    for (k, val) in obj {
        if matches!(k.as_str(), "ts" | "seed" | "kind") {
            continue;
        }
        kv.push((k.clone(), value_to_string(val)));
    }

    Some(AuditRow {
        ts_iso,
        ts_micros,
        source: AuditSource::Spontaneous,
        level,
        kind,
        message,
        sovereign_cap_used: false,
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
    fn parses_manifest() {
        let row = parse_spontaneous_line(
            r#"{"ts":"2026-04-30T00:00:00Z","kind":"manifest","seed":"creature/raven","scene":"test_room","entity_id":42}"#,
        )
        .expect("parse ok");
        assert_eq!(row.source, AuditSource::Spontaneous);
        assert_eq!(row.kind, "manifest");
        assert_eq!(row.level, AuditLevel::Info);
        assert_eq!(row.message, "creature/raven");
        assert_eq!(row.kv_get("scene"), Some("test_room"));
        assert_eq!(row.kv_get("entity_id"), Some("42"));
    }

    #[test]
    fn manifest_failed_is_warn() {
        let row = parse_spontaneous_line(
            r#"{"ts":"2026-04-30T00:00:00Z","kind":"manifest_failed","seed":"x","reason":"cap-budget"}"#,
        )
        .expect("parse ok");
        assert_eq!(row.level, AuditLevel::Warn);
        assert_eq!(row.kv_get("reason"), Some("cap-budget"));
    }
}
