// § sovereign_bypass.rs : recorder + flag for sovereign-bypass send-paths
//
// PRIME-DIRECTIVE § 3 (Substrate-Sovereignty) carves out a "sovereign-cap
// bypass" hatch where trusted host code can short-circuit the cap-grant
// ceremony for first-run / boot-strap paths (mirrors `cssl-rt::http`'s
// `SOVEREIGN_CAP` magic). When the bypass is active for a transport,
// every `send` call must emit `mp.sovereign.bypass` BEFORE the actual
// network write so the bypass is loud + auditable.
//
// `SovereignBypassRecorder` is the toggle + recorder. `enable()` /
// `disable()` flip a `Mutex<bool>` ; `is_active()` is the read-side. The
// transport calls `record_if_active(...)` from the send hot-path to
// (a) emit the audit event and (b) increment a counter for assertion in
// tests.
//
// The recorder holds a reference to a sink (via `&dyn AuditSink`) ; we
// don't take ownership so a single sink can fan out to multiple
// transports.

use crate::config::{AuditEvent, AuditSink};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Sovereign-bypass recorder. Default-disabled ; flip via `enable()`.
///
/// `Send + Sync` so it can sit inside a `RealSupabaseTransport` shared
/// across threads.
#[derive(Debug)]
pub struct SovereignBypassRecorder {
    /// Whether bypass is currently active. Mutex (not AtomicBool) because
    /// callers may want compare-and-set semantics in future ; keeps the
    /// API pluggable.
    active: Mutex<bool>,
    /// Total bypass-records emitted since construction. Atomic so the
    /// transport's cold counter-read is lock-free.
    records: AtomicU64,
}

impl Default for SovereignBypassRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl SovereignBypassRecorder {
    /// Construct a recorder ; default disabled.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: Mutex::new(false),
            records: AtomicU64::new(0),
        }
    }

    /// Enable bypass. Subsequent sends will emit `mp.sovereign.bypass`
    /// audit events before they hit the wire.
    pub fn enable(&self) {
        if let Ok(mut g) = self.active.lock() {
            *g = true;
        }
    }

    /// Disable bypass. Returns the recorder to default state.
    pub fn disable(&self) {
        if let Ok(mut g) = self.active.lock() {
            *g = false;
        }
    }

    /// Probe : is bypass currently active?
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.lock().map(|g| *g).unwrap_or(false)
    }

    /// Number of bypass-records emitted since construction. Atomic ;
    /// cheap in the test hot-path.
    #[must_use]
    pub fn records(&self) -> u64 {
        self.records.load(Ordering::Relaxed)
    }

    /// If bypass is active, emit a `mp.sovereign.bypass` event via `sink`
    /// with the given `msg_kind` + monotonic `ts_micros` field. Audit
    /// failures are silently swallowed — bypass-recording is best-effort
    /// observability ; a panic here would silently weaponize the
    /// transport against its own host.
    ///
    /// Returns `true` iff a record was emitted (i.e. bypass was active).
    pub fn record_if_active(&self, sink: &dyn AuditSink, msg_kind: &str, ts_micros: u64) -> bool {
        if !self.is_active() {
            return false;
        }
        let event = AuditEvent::new("mp.sovereign.bypass")
            .with("msg_kind", msg_kind)
            .with("ts_micros", ts_micros);
        // Audit-failures are non-fatal ; intentionally drop the result.
        let _ = sink.emit(event);
        self.records.fetch_add(1, Ordering::Relaxed);
        true
    }
}

// ─── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{NoopAuditSink, RecordingAuditSink};

    #[test]
    fn default_disabled() {
        let r = SovereignBypassRecorder::new();
        assert!(!r.is_active());
        assert_eq!(r.records(), 0);
    }

    #[test]
    fn enable_disable_flips_flag() {
        let r = SovereignBypassRecorder::new();
        r.enable();
        assert!(r.is_active());
        r.disable();
        assert!(!r.is_active());
        // re-enable is idempotent
        r.enable();
        r.enable();
        assert!(r.is_active());
    }

    #[test]
    fn record_if_active_no_op_when_disabled() {
        let r = SovereignBypassRecorder::new();
        let sink = RecordingAuditSink::new();
        let emitted = r.record_if_active(&sink, "Hello", 1234);
        assert!(!emitted);
        assert_eq!(sink.events().len(), 0);
        assert_eq!(r.records(), 0);
    }

    #[test]
    fn record_if_active_emits_when_enabled() {
        let r = SovereignBypassRecorder::new();
        r.enable();
        let sink = RecordingAuditSink::new();
        let emitted = r.record_if_active(&sink, "Offer", 9_999_999);
        assert!(emitted);
        let evs = sink.events();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, "mp.sovereign.bypass");
        assert_eq!(evs[0].fields.get("msg_kind").map(String::as_str), Some("Offer"));
        assert_eq!(
            evs[0].fields.get("ts_micros").map(String::as_str),
            Some("9999999")
        );
        assert_eq!(r.records(), 1);
    }

    #[test]
    fn audit_failure_is_swallowed() {
        // FailingSink always Errs ; record_if_active must not panic.
        #[derive(Debug)]
        struct FailingSink;
        impl AuditSink for FailingSink {
            fn emit(&self, _: AuditEvent) -> Result<(), crate::config::AuditErr> {
                Err(crate::config::AuditErr::Closed)
            }
        }
        let r = SovereignBypassRecorder::new();
        r.enable();
        let sink = FailingSink;
        let _ = r.record_if_active(&sink, "Bye", 0);
        // Counter still increments (we tried).
        assert_eq!(r.records(), 1);
    }

    #[test]
    fn noop_sink_path_compiles() {
        // Smoke-test : the `&dyn AuditSink` parameter accepts a NoopAuditSink ref.
        let r = SovereignBypassRecorder::new();
        r.enable();
        let _ = r.record_if_active(&NoopAuditSink, "Ping", 0);
    }
}
