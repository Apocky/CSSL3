//! § rollback — Applied → Reverted ; restores prior snapshot.
//!
//! Two trigger paths :
//!   - Manual : the player invokes the Mycelial-Terminal "rollback"
//!     button in their Home pocket-dimension.
//!   - Automatic : the 30-second balance-tier revert window expired
//!     without user-confirmation. (See `stream.rs` :: `tick_revert_window`.)

use crate::class::{HotfixId, HotfixState};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollbackOutcome {
    /// Successfully Applied → Reverted.
    Reverted,
    /// Already-Reverted (idempotent).
    AlreadyReverted,
}

#[derive(Debug, Error)]
pub enum RollbackError {
    #[error("hotfix `{0}` not in staging area")]
    NotFound(String),
    #[error(
        "hotfix `{id}` is in state `{state:?}` ; can only roll back from Applied"
    )]
    NotApplied {
        id: String,
        state: HotfixState,
    },
}

/// Helper exposed for `stream.rs` ; pure state-transition logic
/// validating an attempted rollback.
pub fn validate_rollback(id: &HotfixId, state: HotfixState) -> Result<RollbackOutcome, RollbackError> {
    match state {
        HotfixState::Applied => Ok(RollbackOutcome::Reverted),
        HotfixState::Reverted => Ok(RollbackOutcome::AlreadyReverted),
        other => Err(RollbackError::NotApplied {
            id: id.0.clone(),
            state: other,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::HotfixId;

    #[test]
    fn applied_state_rolls_back() {
        assert_eq!(
            validate_rollback(&HotfixId::new("a"), HotfixState::Applied).unwrap(),
            RollbackOutcome::Reverted
        );
    }

    #[test]
    fn reverted_state_is_idempotent() {
        assert_eq!(
            validate_rollback(&HotfixId::new("a"), HotfixState::Reverted).unwrap(),
            RollbackOutcome::AlreadyReverted
        );
    }

    #[test]
    fn rollback_from_pending_rejected() {
        for state in [
            HotfixState::Pending,
            HotfixState::Verified,
            HotfixState::Staged,
            HotfixState::Rejected,
        ] {
            let err = validate_rollback(&HotfixId::new("a"), state).unwrap_err();
            assert!(matches!(err, RollbackError::NotApplied { .. }));
        }
    }
}
