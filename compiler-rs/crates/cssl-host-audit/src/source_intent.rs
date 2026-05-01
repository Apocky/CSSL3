//! § source_intent — parser for `intent.jsonl`.
//! ════════════════════════════════════════════════════════════════════
//!
//! Text-input → intent classifier emits these rows when a user query
//! is dispatched. Canonical shape :
//!
//! ```json
//! {"ts":"2026-04-30T00:00:00Z","kind":"intent_classify","query":"summon a creature",
//!  "intent":"summon","confidence":0.92}
//! ```
//!
//! `kind = "intent_dispatch"` rows record what subsystem the dispatch
//! was routed to (DM / GM / Collaborator / Coder).

use crate::row::{AuditLevel, AuditRow, AuditSource};
use crate::source_runtime::parse_iso_to_micros;
use serde_json::Value;

/// Parse a single line from `intent.jsonl` into an [`AuditRow`].
/// Returns `None` for blank or malformed lines.
#[must_use]
pub fn parse_intent_line(line: &str) -> Option<AuditRow> {
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
        .unwrap_or("intent")
        .to_string();
    let message = obj
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut kv = Vec::new();
    for (k, val) in obj {
        if matches!(k.as_str(), "ts" | "query" | "kind") {
            continue;
        }
        kv.push((k.clone(), value_to_string(val)));
    }

    Some(AuditRow {
        ts_iso,
        ts_micros,
        source: AuditSource::Intent,
        level: AuditLevel::Info,
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
    fn parses_classify() {
        let row = parse_intent_line(
            r#"{"ts":"2026-04-30T00:00:00Z","kind":"intent_classify","query":"summon a creature","intent":"summon","confidence":0.92}"#,
        )
        .expect("parse ok");
        assert_eq!(row.source, AuditSource::Intent);
        assert_eq!(row.kind, "intent_classify");
        assert_eq!(row.message, "summon a creature");
        assert_eq!(row.kv_get("intent"), Some("summon"));
        assert_eq!(row.kv_get("confidence"), Some("0.92"));
    }

    #[test]
    fn skips_blank_and_malformed() {
        assert!(parse_intent_line("").is_none());
        assert!(parse_intent_line("not json").is_none());
        assert!(parse_intent_line("123").is_none());
    }
}
