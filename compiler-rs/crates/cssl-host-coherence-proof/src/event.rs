// § event.rs · SigmaEventLike trait + mock-impl for tests
// ══════════════════════════════════════════════════════════════════════════════
// § I> trait abstracts cssl-host-sigma-chain (W8-C1 sibling) ; mock-impl for tests
//   ⊑ id() : 32-byte canonical content-hash (BLAKE3 over event-payload)
//   ⊑ payload_blake3() : 32-byte hash of payload-only (excluding sig)
//   ⊑ ts() : event-timestamp (server-assigned via ServerTick)
//   ⊑ emitter_pubkey() : Ed25519 32-byte verifying-key bytes
//   ⊑ sig() : Ed25519 64-byte detached-signature over (id || ts || parent_id?)
//   ⊑ parent_id() : Some(prev) for chained ; None for genesis
// § I> trait-shape MUST match sibling-crate · audit at integration-time
// ══════════════════════════════════════════════════════════════════════════════
use serde::{Deserialize, Serialize};

/// 32-byte BLAKE3 event-id (canonical content-hash).
pub type EventId = [u8; 32];

/// 32-byte Ed25519 verifying-key bytes.
pub type PubKey = [u8; 32];

/// 64-byte Ed25519 detached-signature.
pub type SigBytes = [u8; 64];

/// Trait abstracting the shape of a Σ-Chain event for consensus-validation.
///
/// The trait is intentionally minimal — concrete events live in
/// `cssl-host-sigma-chain` (W8-C1, sibling). This crate must NOT depend on
/// the sibling at compile-time. Integration-tests verify trait-shape match.
pub trait SigmaEventLike {
    /// Canonical 32-byte event-id (BLAKE3 over event-payload).
    fn id(&self) -> EventId;
    /// 32-byte BLAKE3 hash of payload (excluding signature).
    fn payload_blake3(&self) -> [u8; 32];
    /// Server-assigned timestamp (must come from a `ServerTick`).
    fn ts(&self) -> u64;
    /// Ed25519 emitter verifying-key (32 bytes).
    fn emitter_pubkey(&self) -> PubKey;
    /// Ed25519 detached-signature over canonical-bytes (64 bytes).
    fn sig(&self) -> SigBytes;
    /// Parent-event-id for chained-events ; `None` for genesis.
    fn parent_id(&self) -> Option<EventId>;
}

/// Mock-impl used by tests + downstream-fixtures.
///
/// Wire-format is stable for serde round-trip tests. The 64-byte `sig` field
/// uses the crate-local `sig_serde` module since serde does not auto-impl
/// `Deserialize` for `[u8; 64]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MockSigmaEvent {
    pub id: EventId,
    pub payload_blake3: [u8; 32],
    pub ts: u64,
    pub emitter_pubkey: PubKey,
    #[serde(with = "crate::sig_serde")]
    pub sig: SigBytes,
    pub parent_id: Option<EventId>,
}

impl MockSigmaEvent {
    /// Construct a new mock with explicit fields.
    pub fn new(
        id: EventId,
        payload_blake3: [u8; 32],
        ts: u64,
        emitter_pubkey: PubKey,
        sig: SigBytes,
        parent_id: Option<EventId>,
    ) -> Self {
        Self {
            id,
            payload_blake3,
            ts,
            emitter_pubkey,
            sig,
            parent_id,
        }
    }

    /// Construct a deterministically-seeded mock (test-fixture helper).
    /// Derives id/payload from seed-byte ; sig + pubkey are zero-bytes.
    pub fn seeded(seed: u8, ts: u64, parent_id: Option<EventId>) -> Self {
        let mut id = [0u8; 32];
        let mut payload = [0u8; 32];
        for i in 0..32 {
            id[i] = seed.wrapping_add(i as u8);
            payload[i] = seed.wrapping_mul(3).wrapping_add(i as u8);
        }
        Self {
            id,
            payload_blake3: payload,
            ts,
            emitter_pubkey: [0u8; 32],
            sig: [0u8; 64],
            parent_id,
        }
    }
}

impl SigmaEventLike for MockSigmaEvent {
    fn id(&self) -> EventId {
        self.id
    }
    fn payload_blake3(&self) -> [u8; 32] {
        self.payload_blake3
    }
    fn ts(&self) -> u64 {
        self.ts
    }
    fn emitter_pubkey(&self) -> PubKey {
        self.emitter_pubkey
    }
    fn sig(&self) -> SigBytes {
        self.sig
    }
    fn parent_id(&self) -> Option<EventId> {
        self.parent_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_event_construct_explicit() {
        let id = [1u8; 32];
        let payload = [2u8; 32];
        let pk = [3u8; 32];
        let sig = [4u8; 64];
        let ev = MockSigmaEvent::new(id, payload, 100, pk, sig, None);
        assert_eq!(ev.id(), id);
        assert_eq!(ev.payload_blake3(), payload);
        assert_eq!(ev.ts(), 100);
        assert_eq!(ev.emitter_pubkey(), pk);
        assert_eq!(ev.sig(), sig);
        assert_eq!(ev.parent_id(), None);
    }

    #[test]
    fn mock_event_seeded_distinct_ids() {
        let a = MockSigmaEvent::seeded(0x10, 1, None);
        let b = MockSigmaEvent::seeded(0x20, 2, Some(a.id));
        assert_ne!(a.id(), b.id());
        assert_eq!(b.parent_id(), Some(a.id()));
    }
}
