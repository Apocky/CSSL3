// § trait_def.rs : `MpTransport` trait + capability bits + result/error types.
//
// The trait is intentionally narrow : two operations (send + poll) plus a
// capability-bit accessor. This keeps adapter authors honest — anything beyond
// these primitives belongs in the consumer (the loa-host orchestrator) not in
// the transport layer. Trait-object-safe by design : `Send + Sync` + no
// generic-method parameters + no `Self`-by-value methods.

use cssl_host_multiplayer_signaling::SignalingMessage;
use serde::{Deserialize, Serialize};

// ─── capability bits ───────────────────────────────────────────────────────
//
// Capability-flags follow the §§ 11_IFC pattern : a transport advertises which
// directions it permits (send / recv) and the trait impls cap-deny operations
// the caller is not authorized to perform. The bit-pattern is `u32` so future
// orthogonal capabilities (e.g. broadcast-only / reliable-only) can be added
// without an enum-rewrite.

/// Capability bit : transport may originate outbound messages via `send`.
pub const TRANSPORT_CAP_SEND: u32 = 1;
/// Capability bit : transport may receive inbound messages via `poll`.
pub const TRANSPORT_CAP_RECV: u32 = 2;
/// Convenience : both directions.
pub const TRANSPORT_CAP_BOTH: u32 = TRANSPORT_CAP_SEND | TRANSPORT_CAP_RECV;

// ─── result / error ─────────────────────────────────────────────────────────

/// Success-shape returned by `MpTransport::send`. Carries the server-assigned
/// monotonic id (when the backend supports it ; loopback uses a local counter)
/// plus a coarse round-trip latency estimate so the caller can drive backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportResult {
    /// Server-side id of the persisted message (monotonic per-room).
    pub sent_id: u64,
    /// Coarse round-trip latency in milliseconds.
    pub latency_ms: u32,
}

/// Failure-shape for transport operations. Maps to retry/backoff policy in
/// `TransportRouter` : `Backoff` / `Timeout` / `ServerErr` are router-fallback
/// triggers ; `CapDenied` / `MalformedMessage` / `NotConnected` are not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportErr {
    /// Operation requires a capability the transport does not hold.
    /// Inner `u32` echoes the requested cap-bit for diagnostics.
    CapDenied(u32),
    /// Transport is not connected (e.g. WebSocket not yet handshaken).
    NotConnected,
    /// Server-side rate-limit ; inner `u32` is the recommended retry-delay
    /// in milliseconds.
    Backoff(u32),
    /// Message failed local validation before being put on the wire.
    MalformedMessage,
    /// Server returned an error ; inner string is the server-side detail.
    ServerErr(String),
    /// Operation exceeded the transport's deadline.
    Timeout,
}

impl core::fmt::Display for TransportErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CapDenied(bit) => write!(f, "capability denied (bit 0x{bit:x})"),
            Self::NotConnected => f.write_str("transport not connected"),
            Self::Backoff(ms) => write!(f, "server backoff ({ms} ms)"),
            Self::MalformedMessage => f.write_str("message failed local validation"),
            Self::ServerErr(s) => write!(f, "server error: {s}"),
            Self::Timeout => f.write_str("transport operation timed out"),
        }
    }
}

impl std::error::Error for TransportErr {}

// ─── trait ────────────────────────────────────────────────────────────────

/// Transport-adapter contract. Pairs the W4 signaling envelope with arbitrary
/// backends (loopback / Supabase REST / WebSocket / …).
///
/// Trait-object-safe : `Send + Sync` + no generic-method parameters +
/// no `Self`-by-value methods. Construct as `Box<dyn MpTransport>` for the
/// router.
pub trait MpTransport: Send + Sync + core::fmt::Debug {
    /// Short, human-readable name (e.g. "loopback" / "supabase-rest").
    /// Used in diagnostic logs ; must not allocate per-call.
    fn name(&self) -> &str;

    /// Originate an outbound message. Validates the envelope locally
    /// (`SignalingMessage::validate`) before delegating to the backend.
    ///
    /// # Errors
    /// Returns `TransportErr::CapDenied(TRANSPORT_CAP_SEND)` if the transport
    /// lacks send-capability ; `TransportErr::MalformedMessage` if local
    /// validation fails ; backend-specific errors otherwise.
    fn send(&self, msg: &SignalingMessage) -> Result<TransportResult, TransportErr>;

    /// Drain inbound messages addressed to `peer_id` in `room_id` whose
    /// server-side id is greater than `since_id`.
    ///
    /// # Errors
    /// Returns `TransportErr::CapDenied(TRANSPORT_CAP_RECV)` if the transport
    /// lacks recv-capability ; backend-specific errors otherwise.
    fn poll(
        &self,
        room_id: &str,
        peer_id: &str,
        since_id: u64,
    ) -> Result<Vec<SignalingMessage>, TransportErr>;

    /// Capability bits this transport advertises. The caller (or the router)
    /// uses this to short-circuit cap-denied operations without a backend
    /// round-trip.
    fn caps(&self) -> u32;
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A no-op transport — exists solely to prove the trait is object-safe and
    /// can be stored in a `Box<dyn MpTransport>`.
    #[derive(Debug)]
    struct DummyT;
    impl MpTransport for DummyT {
        fn name(&self) -> &str {
            "dummy"
        }
        fn send(&self, _: &SignalingMessage) -> Result<TransportResult, TransportErr> {
            Ok(TransportResult {
                sent_id: 0,
                latency_ms: 0,
            })
        }
        fn poll(
            &self,
            _: &str,
            _: &str,
            _: u64,
        ) -> Result<Vec<SignalingMessage>, TransportErr> {
            Ok(vec![])
        }
        fn caps(&self) -> u32 {
            TRANSPORT_CAP_BOTH
        }
    }

    #[test]
    fn trait_is_object_safe() {
        let boxed: Box<dyn MpTransport> = Box::new(DummyT);
        assert_eq!(boxed.name(), "dummy");
        assert_eq!(boxed.caps(), TRANSPORT_CAP_BOTH);
    }

    #[test]
    fn cap_bits_named_correctly() {
        assert_eq!(TRANSPORT_CAP_SEND, 1);
        assert_eq!(TRANSPORT_CAP_RECV, 2);
        assert_eq!(TRANSPORT_CAP_BOTH, 3);
        // overlapping-bits invariant : SEND | RECV == BOTH
        assert_eq!(TRANSPORT_CAP_SEND | TRANSPORT_CAP_RECV, TRANSPORT_CAP_BOTH);
        // disjoint-bits invariant : SEND & RECV == 0
        assert_eq!(TRANSPORT_CAP_SEND & TRANSPORT_CAP_RECV, 0);
    }

    #[test]
    fn transport_result_carries_id_and_latency() {
        let r = TransportResult {
            sent_id: 42,
            latency_ms: 17,
        };
        assert_eq!(r.sent_id, 42);
        assert_eq!(r.latency_ms, 17);

        // round-trips through serde
        let j = serde_json::to_string(&r).expect("ser");
        let back: TransportResult = serde_json::from_str(&j).expect("de");
        assert_eq!(back, r);
    }

    #[test]
    fn err_display_covers_all_variants() {
        assert!(format!("{}", TransportErr::CapDenied(1)).contains("capability denied"));
        assert!(format!("{}", TransportErr::NotConnected).contains("not connected"));
        assert!(format!("{}", TransportErr::Backoff(250)).contains("250"));
        assert!(format!("{}", TransportErr::MalformedMessage).contains("validation"));
        assert!(
            format!("{}", TransportErr::ServerErr("boom".into())).contains("boom")
        );
        assert!(format!("{}", TransportErr::Timeout).contains("timed out"));
    }
}
