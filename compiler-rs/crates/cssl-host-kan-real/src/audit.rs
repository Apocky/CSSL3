//! § Audit-emit hook (I-3 invariant).
//! ════════════════════════════════════════════════════════════════════
//!
//! Every classify-call emits an [`AuditEvent`] via [`audit_log`]. The host
//! wires a real sink (e.g. `cssl-host-attestation`) at registry-construction
//! time via [`set_audit_sink`] ; the default sink is a no-op so unit tests
//! and stage-0-only paths run without side-effects.
//!
//! § DESIGN NOTE
//!   We intentionally avoid a hard dep on `cssl-host-attestation` at the
//!   call-site (avoids a tight coupling that would force every
//!   classify-call to take an `AttestationStore` parameter). Instead the
//!   sink is a process-global `parking_lot`-free `OnceLock<Box<dyn
//!   AuditSink>>` style cell with `RwLock`-light semantics. We use a
//!   `std::sync::RwLock` rather than a `OnceLock` because the host may
//!   re-install the sink across registry-rebuilds (canary toggle).
//!
//! § DETERMINISM
//!   The audit hook does NOT participate in the deterministic-output
//!   contract — it is a side-effect channel only. classify() returns the
//!   same output regardless of whether the sink is installed.

use std::sync::RwLock;

/// § One audit event emitted per classify-call.
///
/// Carries the swap-point id (`sp-id` per spec § INVARIANTS I-3), an
/// input-hash, an output-hash, and the impl-id ("real-kan" / "stage-0-
/// fallback" / "stub"). Hashes are 64-bit FNV-1a for cheap-and-stable
/// comparison ; the host can normalize to blake3 in its own pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// § Stable swap-point id matching `specs/grand-vision/11_KAN_RIDE.csl`
    ///   `§ SWAP-POINTS` ("intent_router" / "cocreative" / "spontaneous").
    pub sp_id: &'static str,
    /// § Stable impl-id ("real-kan" / "stage-0-fallback" / "stub").
    pub impl_id: &'static str,
    /// § FNV-1a-64 of the canonical input bytes.
    pub in_hash: u64,
    /// § FNV-1a-64 of the canonical output bytes.
    pub out_hash: u64,
}

/// § Trait implemented by audit-sinks. The host wires a sink at
///   registry-construction time ; the default is a no-op.
pub trait AuditSink: Send + Sync {
    /// § Receive one audit event. MUST NOT panic.
    fn emit(&self, event: &AuditEvent);
}

/// § No-op sink ; the default if no real sink is installed.
pub struct NoopSink;

impl AuditSink for NoopSink {
    fn emit(&self, _event: &AuditEvent) {
        // intentional no-op
    }
}

// § Process-global sink. We use std::sync::RwLock to keep the dep-tree
//   lean — `parking_lot` is workspace-available but unnecessary here :
//   contention on this lock is per-classify-call and effectively zero in
//   the typical single-classifier-per-host topology.
static SINK: RwLock<Option<Box<dyn AuditSink>>> = RwLock::new(None);

/// § Install a global audit sink. Replaces the previous sink. Calling
///   with `None` would not be useful here ; if you want to disable
///   auditing, install a [`NoopSink`].
pub fn set_audit_sink(sink: Box<dyn AuditSink>) {
    if let Ok(mut guard) = SINK.write() {
        *guard = Some(sink);
    }
}

/// § Drop the currently-installed sink (revert to no-op). Useful in tests.
pub fn clear_audit_sink() {
    if let Ok(mut guard) = SINK.write() {
        *guard = None;
    }
}

/// § Emit one audit event. The host wires this through `cssl-host-
///   attestation` at registry-construction time ; the default sink is a
///   no-op so this is callable from anywhere without ceremony.
pub fn audit_log(event: AuditEvent) {
    if let Ok(guard) = SINK.read() {
        if let Some(sink) = guard.as_ref() {
            sink.emit(&event);
        }
    }
}

/// § FNV-1a-64 hash of a byte slice. Used to populate
///   [`AuditEvent::in_hash`] / [`AuditEvent::out_hash`] without pulling
///   in a heavier crypto-grade hash on the hot path.
#[must_use]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// § Counter sink — captures emit-count for test assertions.
    struct CounterSink {
        count: Arc<AtomicUsize>,
    }
    impl AuditSink for CounterSink {
        fn emit(&self, _event: &AuditEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn fnv_deterministic() {
        assert_eq!(fnv1a_64(b"hello"), fnv1a_64(b"hello"));
        assert_ne!(fnv1a_64(b"hello"), fnv1a_64(b"world"));
        // Empty input has the canonical FNV-1a-64 offset basis.
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn noop_sink_does_not_panic() {
        let sink = NoopSink;
        sink.emit(&AuditEvent {
            sp_id: "intent_router",
            impl_id: "stub",
            in_hash: 0,
            out_hash: 0,
        });
    }

    #[test]
    fn audit_log_no_sink_is_silent() {
        // Default state : no sink installed ; should not panic.
        clear_audit_sink();
        audit_log(AuditEvent {
            sp_id: "test",
            impl_id: "test",
            in_hash: 1,
            out_hash: 2,
        });
    }
}
