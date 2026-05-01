// § ledger.rs — append-only BTreeMap-backed ledger + snapshot + CoherenceProof
// §§ BTreeMap iteration is sorted-by-key ASC ⇒ deterministic merkle-root across-machines

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::event::{EventId, SigmaEvent};
use crate::merkle::{merkle_path_of, merkle_root_of, Digest, MerkleStep};
use crate::sign::recompute_event_id;

/// Append-only Σ-Chain local-ledger. Wraps a sorted BTreeMap so iteration order
/// is canonical & deterministic across machines (no HashMap allowed per spec/14).
#[derive(Debug, Clone, Default)]
pub struct SigmaLedger {
    events: BTreeMap<EventId, SigmaEvent>,
}

impl SigmaLedger {
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: BTreeMap::new(),
        }
    }

    /// Insert an event. Validates that `event.id` matches recomputed-id (tamper-guard).
    ///
    /// # Errors
    /// - [`LedgerInsertError::IdMismatch`] when `event.id` ≠ recompute(canonical-bytes).
    /// - [`LedgerInsertError::DuplicateId`] when this id is already present.
    pub fn insert(&mut self, event: SigmaEvent) -> Result<(), LedgerInsertError> {
        let recomputed = recompute_event_id(&event);
        if recomputed != event.id {
            return Err(LedgerInsertError::IdMismatch);
        }
        if self.events.contains_key(&event.id) {
            return Err(LedgerInsertError::DuplicateId);
        }
        self.events.insert(event.id, event);
        Ok(())
    }

    /// Number of events in the ledger.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Borrow an event by id.
    #[must_use]
    pub fn get(&self, id: &EventId) -> Option<&SigmaEvent> {
        self.events.get(id)
    }

    /// Iterate events in canonical id-order (BTreeMap sort).
    pub fn iter_events(&self) -> impl Iterator<Item = (&EventId, &SigmaEvent)> {
        self.events.iter()
    }

    /// Sorted vector of event-ids (canonical merkle-leaf-order).
    #[must_use]
    pub fn sorted_event_ids(&self) -> Vec<EventId> {
        self.events.keys().copied().collect()
    }

    /// Compute the merkle-root over all currently-held events.
    #[must_use]
    pub fn merkle_root(&self) -> Digest {
        merkle_root_of(&self.sorted_event_ids())
    }

    /// Build a snapshot (serializable cross-process / cross-machine for opt-in egress).
    ///
    /// Events are emitted as a Vec sorted-by-id ASC (deterministic) — JSON-friendly
    /// since BTreeMap with byte-array keys would require string-keyed encoding.
    #[must_use]
    pub fn snapshot(&self) -> LedgerSnapshot {
        let events: Vec<SigmaEvent> = self.events.values().cloned().collect();
        LedgerSnapshot {
            events,
            merkle_root: self.merkle_root(),
        }
    }

    /// Build a Coherence-Proof for a single event (signature + merkle-path + lineage).
    #[must_use]
    pub fn coherence_proof_for(&self, event_id: &EventId) -> Option<CoherenceProof> {
        let event = self.events.get(event_id)?.clone();
        let ids = self.sorted_event_ids();
        let path = merkle_path_of(&ids, event_id)?;
        let root = merkle_root_of(&ids);
        Some(CoherenceProof {
            event,
            merkle_path: path,
            merkle_root: root,
        })
    }

    /// Reconstruct a deterministic merkle-root from `(seed_ids + lineage_events)` —
    /// basis of Coherence-Proof per spec/14 § COHERENCE-PROOF step 4.
    ///
    /// `seed_ids` are pre-existing baseline ids ; `lineage_events` are appended-then-sorted.
    /// Duplicates within lineage are deduplicated (idempotent).
    #[must_use]
    pub fn deterministic_recompute_root(
        seed_ids: &[EventId],
        lineage_events: &[SigmaEvent],
    ) -> Digest {
        let mut all: BTreeMap<EventId, ()> = BTreeMap::new();
        for s in seed_ids {
            all.insert(*s, ());
        }
        for e in lineage_events {
            all.insert(e.id, ());
        }
        let sorted: Vec<EventId> = all.keys().copied().collect();
        merkle_root_of(&sorted)
    }
}

/// Serializable snapshot of the ledger for opt-in egress / golden-file regression tests.
///
/// `events` is sorted-by-id ASC — caller can rebuild a `BTreeMap` if needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerSnapshot {
    /// All events sorted-by-id ASC (canonical merkle-leaf-order).
    pub events: Vec<SigmaEvent>,
    /// Merkle-root @ snapshot-time.
    pub merkle_root: Digest,
}

/// Coherence-Proof bundle : event + merkle-path + claimed-root.
///
/// Verifiers receive this and run [`crate::verify::verify_event`] (signature) +
/// [`crate::merkle::merkle_path_verify`] (inclusion). If both pass and a deterministic
/// recompute on `lineage` reproduces `merkle_root`, the proof is COHERENT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceProof {
    /// The event being proven.
    pub event: SigmaEvent,
    /// Merkle inclusion path for `event.id`.
    pub merkle_path: Vec<MerkleStep>,
    /// Claimed merkle-root (must match deterministic-recompute on receiver).
    pub merkle_root: Digest,
}

// MerkleStep needs Serialize/Deserialize for inclusion in CoherenceProof.
// Implemented manually below (merkle.rs keeps the type Plain-Old-Data).
impl Serialize for MerkleStep {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = ser.serialize_struct("MerkleStep", 2)?;
        s.serialize_field("sibling", &self.sibling)?;
        s.serialize_field("sibling_is_left", &self.sibling_is_left)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for MerkleStep {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            sibling: [u8; 32],
            sibling_is_left: bool,
        }
        let r = Raw::deserialize(de)?;
        Ok(MerkleStep {
            sibling: r.sibling,
            sibling_is_left: r.sibling_is_left,
        })
    }
}

/// Errors from [`SigmaLedger::insert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerInsertError {
    /// `event.id` does not equal recompute(canonical-bytes) — tampered.
    IdMismatch,
    /// An event with this id is already in the ledger.
    DuplicateId,
}

impl core::fmt::Display for LedgerInsertError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LedgerInsertError::IdMismatch => f.write_str("event id mismatch (tamper detected)"),
            LedgerInsertError::DuplicateId => f.write_str("event id already in ledger"),
        }
    }
}

impl std::error::Error for LedgerInsertError {}
