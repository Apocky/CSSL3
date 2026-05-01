// § config.rs : SupabaseConfig + AuditSink trait + Noop/Recording impls
//
// `SupabaseConfig` carries every backend-tunable for `RealSupabaseTransport` :
// the project URL, the anon key (always required), the per-user JWT (optional
// — anon-key-only is valid for service-role bypass paths), per-call timeouts,
// and the default backoff used when a 429 lacks a Retry-After header.
//
// `AuditSink` is the observability hook — the transport calls it once per
// send / poll / sovereign-bypass with a typed `AuditEvent`. The trait returns
// `Result<(), AuditErr>` ; the transport IGNORES the result so audit failures
// can never crash the transport. Audit is best-effort.
//
// Two impls ship here :
//   • `NoopAuditSink` — discards every event ; the default for prod when the
//     caller hasn't wired in attestation yet.
//   • `RecordingAuditSink` — buffers every event in a `Mutex<Vec>` for test
//     introspection. Used by every test in the crate.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Configuration for `RealSupabaseTransport`.
///
/// All fields are owned ; the transport clones nothing per-call. The struct
/// is `Clone` so callers can share a single config across multiple
/// transports (e.g. one for the host one for the peer).
#[derive(Debug, Clone)]
pub struct SupabaseConfig {
    /// Project URL — `https://{project-ref}.supabase.co` typically. Stored
    /// without trailing slash ; the transport composes endpoints by
    /// appending `/rest/v1/...`.
    pub base_url: String,
    /// Public anon key — sent in the `apikey` header on every call.
    pub anon_key: String,
    /// Optional per-user JWT — when `Some`, sent in the
    /// `Authorization: Bearer …` header. When `None`, anon-key-only auth
    /// applies (subject to RLS).
    pub jwt: Option<String>,
    /// Per-call send timeout in milliseconds. ureq distinguishes
    /// connect / read / write timeouts ; the transport sets all three to
    /// this value.
    pub send_timeout_ms: u32,
    /// Per-call poll timeout in milliseconds. Same semantics as
    /// `send_timeout_ms`.
    pub poll_timeout_ms: u32,
    /// Backoff applied when a 429 response carries no parseable
    /// `Retry-After` header. Conservative default = 2000 ms.
    pub default_backoff_ms: u32,
}

impl SupabaseConfig {
    /// Construct a config with the default timeouts. `base_url` should NOT
    /// carry a trailing slash ; it's stripped if present.
    #[must_use]
    pub fn new(base_url: impl Into<String>, anon_key: impl Into<String>) -> Self {
        let mut url = base_url.into();
        while url.ends_with('/') {
            url.pop();
        }
        Self {
            base_url: url,
            anon_key: anon_key.into(),
            jwt: None,
            send_timeout_ms: 5_000,
            poll_timeout_ms: 5_000,
            default_backoff_ms: 2_000,
        }
    }

    /// Builder : attach a per-user JWT for RLS-authenticated calls.
    #[must_use]
    pub fn with_jwt(mut self, jwt: impl Into<String>) -> Self {
        self.jwt = Some(jwt.into());
        self
    }

    /// Builder : override the send timeout.
    #[must_use]
    pub fn with_send_timeout_ms(mut self, ms: u32) -> Self {
        self.send_timeout_ms = ms;
        self
    }

    /// Builder : override the poll timeout.
    #[must_use]
    pub fn with_poll_timeout_ms(mut self, ms: u32) -> Self {
        self.poll_timeout_ms = ms;
        self
    }

    /// Builder : override the default backoff applied when a 429 has no
    /// parseable `Retry-After` header.
    #[must_use]
    pub fn with_default_backoff_ms(mut self, ms: u32) -> Self {
        self.default_backoff_ms = ms;
        self
    }
}

// ─── audit ─────────────────────────────────────────────────────────────────

/// Typed audit event emitted by `RealSupabaseTransport`. The `kind` field is
/// a stable string identifier matching the §§ 22 TELEMETRY event-name
/// convention (`mp.send.ok` / `mp.poll.ok` / `mp.sovereign.bypass` / …).
///
/// `fields` is a `BTreeMap<String, String>` so serialization order is
/// deterministic — important for golden-file tests in downstream consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Stable event identifier. Examples : `"mp.send.ok"` ·
    /// `"mp.send.backoff"` · `"mp.poll.ok"` · `"mp.sovereign.bypass"`.
    pub kind: String,
    /// Structured fields. Keys match the per-event schema documented in
    /// the transport module ; values are stringified for sink-portability.
    pub fields: BTreeMap<String, String>,
}

impl AuditEvent {
    /// Construct a fresh event with the given kind. Add fields via
    /// `with` chained calls.
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            fields: BTreeMap::new(),
        }
    }

    /// Builder : attach a key/value field. Both are stringified.
    #[must_use]
    pub fn with(mut self, k: impl Into<String>, v: impl ToString) -> Self {
        self.fields.insert(k.into(), v.to_string());
        self
    }
}

/// Audit-sink failure. `RealSupabaseTransport` callers will observe this only
/// if they probe their own sink ; the transport itself swallows audit-errors
/// so a misbehaving sink can't crash the wire-protocol path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditErr {
    /// Sink-internal error — unspecified ; the inner string carries the
    /// sink's own diagnostic.
    SinkInternal(String),
    /// Sink is closed / shutdown ; further calls will keep failing.
    Closed,
}

impl core::fmt::Display for AuditErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SinkInternal(s) => write!(f, "audit sink internal error: {s}"),
            Self::Closed => f.write_str("audit sink is closed"),
        }
    }
}

impl std::error::Error for AuditErr {}

/// Sink for audit events. Every method takes `&self` ; impls must be
/// `Send + Sync` so the transport can hold them in `Box<dyn AuditSink>`.
pub trait AuditSink: Send + Sync + core::fmt::Debug {
    /// Publish a single event. Returning `Err` is non-fatal — the
    /// transport ignores the result. Best-effort observability.
    fn emit(&self, event: AuditEvent) -> Result<(), AuditErr>;
}

/// No-op audit sink — discards every event without allocation. Default for
/// production wiring before the attestation aggregator is online.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAuditSink;

impl AuditSink for NoopAuditSink {
    fn emit(&self, _event: AuditEvent) -> Result<(), AuditErr> {
        Ok(())
    }
}

/// Recording audit sink — buffers events in a `Mutex<Vec>`. Used by every
/// test in the crate to assert that the transport emits the expected event
/// stream.
#[derive(Debug, Default)]
pub struct RecordingAuditSink {
    inner: Mutex<Vec<AuditEvent>>,
}

impl RecordingAuditSink {
    /// Construct an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded events in insertion order.
    pub fn events(&self) -> Vec<AuditEvent> {
        self.inner
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Convenience : count events whose `kind` matches `kind`.
    pub fn count_kind(&self, kind: &str) -> usize {
        self.inner
            .lock()
            .map(|g| g.iter().filter(|e| e.kind == kind).count())
            .unwrap_or(0)
    }
}

impl AuditSink for RecordingAuditSink {
    fn emit(&self, event: AuditEvent) -> Result<(), AuditErr> {
        // § scoped lock — tighten Mutex hold-time so concurrent emitters
        // don't pile up behind a long-lived guard. Mirrors the loopback
        // transport's pattern.
        {
            let mut g = self
                .inner
                .lock()
                .map_err(|_| AuditErr::SinkInternal("mutex poisoned".into()))?;
            g.push(event);
        }
        Ok(())
    }
}

// ─── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_strips_trailing_slash() {
        let c = SupabaseConfig::new("https://x.supabase.co/", "anon");
        assert_eq!(c.base_url, "https://x.supabase.co");
        let c2 = SupabaseConfig::new("https://y.supabase.co////", "anon");
        assert_eq!(c2.base_url, "https://y.supabase.co");
    }

    #[test]
    fn config_builders_chain() {
        let c = SupabaseConfig::new("https://z.supabase.co", "anon")
            .with_jwt("jwt-token")
            .with_send_timeout_ms(7_500)
            .with_poll_timeout_ms(3_000)
            .with_default_backoff_ms(500);
        assert_eq!(c.jwt.as_deref(), Some("jwt-token"));
        assert_eq!(c.send_timeout_ms, 7_500);
        assert_eq!(c.poll_timeout_ms, 3_000);
        assert_eq!(c.default_backoff_ms, 500);
    }

    #[test]
    fn audit_event_builder() {
        let e = AuditEvent::new("mp.send.ok")
            .with("msg_id", 42u64)
            .with("room_id", "r-x");
        assert_eq!(e.kind, "mp.send.ok");
        assert_eq!(e.fields.get("msg_id"), Some(&"42".to_string()));
        assert_eq!(e.fields.get("room_id"), Some(&"r-x".to_string()));
        // BTreeMap → deterministic order
        let keys: Vec<_> = e.fields.keys().collect();
        assert_eq!(keys, vec!["msg_id", "room_id"]);
    }

    #[test]
    fn noop_sink_swallows_everything() {
        let s = NoopAuditSink;
        assert!(s.emit(AuditEvent::new("anything")).is_ok());
    }

    #[test]
    fn recording_sink_captures_in_order() {
        let s = RecordingAuditSink::new();
        s.emit(AuditEvent::new("a")).expect("ok");
        s.emit(AuditEvent::new("b")).expect("ok");
        s.emit(AuditEvent::new("a")).expect("ok");
        let evs = s.events();
        assert_eq!(evs.len(), 3);
        assert_eq!(evs[0].kind, "a");
        assert_eq!(evs[1].kind, "b");
        assert_eq!(evs[2].kind, "a");
        assert_eq!(s.count_kind("a"), 2);
        assert_eq!(s.count_kind("b"), 1);
        assert_eq!(s.count_kind("missing"), 0);
    }
}
