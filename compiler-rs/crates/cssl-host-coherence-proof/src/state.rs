// § state.rs · StateSnapshot trait + concrete-impl
// ══════════════════════════════════════════════════════════════════════════════
// § I> A state-snapshot is the merkle-root over `event_count` events,
//   captured at a specific `ServerTick`.
// § I> recompute folds events into a NEW snapshot · returns new merkle-root
// § I> StateSnapshotLike trait → allow downstream override (e.g. richer state)
// ══════════════════════════════════════════════════════════════════════════════
use serde::{Deserialize, Serialize};

use crate::merkle::MerkleRoot;

/// Trait for snapshot-of-state representation.
///
/// Implementations expose a merkle-root over their canonical event-stream.
pub trait StateSnapshotLike {
    /// Current merkle-root.
    fn merkle_root(&self) -> MerkleRoot;
    /// Number of events folded into this snapshot.
    fn event_count(&self) -> u64;
}

/// Concrete state-snapshot suitable for the validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub merkle_root: MerkleRoot,
    pub event_count: u64,
}

impl StateSnapshot {
    /// Build the genesis (empty) snapshot.
    pub fn empty() -> Self {
        Self {
            merkle_root: [0u8; 32],
            event_count: 0,
        }
    }

    /// Build with explicit fields.
    pub fn new(merkle_root: MerkleRoot, event_count: u64) -> Self {
        Self {
            merkle_root,
            event_count,
        }
    }
}

impl StateSnapshotLike for StateSnapshot {
    fn merkle_root(&self) -> MerkleRoot {
        self.merkle_root
    }
    fn event_count(&self) -> u64 {
        self.event_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_zero_root_zero_count() {
        let s = StateSnapshot::empty();
        assert_eq!(s.merkle_root(), [0u8; 32]);
        assert_eq!(s.event_count(), 0);
    }

    #[test]
    fn snapshot_serde_round_trip() {
        let s = StateSnapshot::new([7u8; 32], 42);
        let json = serde_json::to_string(&s).unwrap();
        let s2: StateSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }
}
