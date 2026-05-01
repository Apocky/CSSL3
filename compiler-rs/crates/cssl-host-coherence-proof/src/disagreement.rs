// § disagreement.rs · DisagreementFlag emitted when consensus diverges
// ══════════════════════════════════════════════════════════════════════════════
// § I> Flag carries event-id at which validators diverged · expected vs actual
//   merkle-root · flagger pubkey for accountability · reason-enum
// § I> MUST emit-via AuditEmitter (¬ silent) — enforced at consensus call-site
// ══════════════════════════════════════════════════════════════════════════════
use serde::{Deserialize, Serialize};

use crate::event::{EventId, PubKey};
use crate::merkle::MerkleRoot;

/// Why the disagreement happened.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DisagreementReason {
    /// Validators agree on event-set but disagree on merkle-root.
    MerkleMismatch,
    /// Validators disagree on event-count at the same point.
    EventCountMismatch,
    /// Differing parent-pointers — chain-fork suspected.
    LineageFork,
}

/// A flagged disagreement between two consensus-validators.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisagreementFlag {
    /// The event-id at which divergence was first observed.
    pub event_id: EventId,
    /// Root claimed by the LOSING side (or one side, when symmetric).
    pub expected_root: MerkleRoot,
    /// Root claimed by the OTHER side.
    pub actual_root: MerkleRoot,
    /// Pubkey of the validator emitting this flag.
    pub flagger_pubkey: PubKey,
    /// Categorical reason.
    pub reason: DisagreementReason,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_serde_round_trip() {
        let f = DisagreementFlag {
            event_id: [11u8; 32],
            expected_root: [22u8; 32],
            actual_root: [33u8; 32],
            flagger_pubkey: [44u8; 32],
            reason: DisagreementReason::MerkleMismatch,
        };
        let json = serde_json::to_string(&f).unwrap();
        let de: DisagreementFlag = serde_json::from_str(&json).unwrap();
        assert_eq!(f, de);
    }

    #[test]
    fn reason_enum_distinct_serialization() {
        let m = serde_json::to_string(&DisagreementReason::MerkleMismatch).unwrap();
        let c = serde_json::to_string(&DisagreementReason::EventCountMismatch).unwrap();
        let f = serde_json::to_string(&DisagreementReason::LineageFork).unwrap();
        assert_ne!(m, c);
        assert_ne!(c, f);
        assert_ne!(m, f);
    }
}
