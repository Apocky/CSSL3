//! § stage — in-memory staging-area for verified hotfixes.
//!
//! Staging is the holding-pen between Verified and Applied. Apply
//! handlers mutate runtime state ; the original payload + the
//! pre-apply snapshot live here so rollback is O(1).
//!
//! Backed exclusively by `BTreeMap` for deterministic iteration.

use crate::class::{Hotfix, HotfixId, HotfixState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// § A verified hotfix held in staging, plus optional
/// pre-apply-snapshot bytes captured at apply-time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedHotfix {
    pub hotfix: Hotfix,
    pub state: HotfixState,
    /// Bytes captured immediately before `apply` mutates runtime.
    /// `None` while in `Verified` or `Staged` state. `Some` once
    /// `Applied`. Used by rollback to restore.
    pub pre_apply_snapshot: Option<Vec<u8>>,
    /// `Instant`-since-apply as nanoseconds. Captured via injected
    /// clock so tests are deterministic. `None` until `Applied`.
    pub applied_at_nanos: Option<u128>,
}

/// § The staging area itself.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StagingArea {
    /// `BTreeMap` keyed by hotfix-id, deterministic iteration.
    pub entries: BTreeMap<HotfixId, StagedHotfix>,
}

#[derive(Debug, Error)]
pub enum StageError {
    #[error("hotfix `{0}` already staged")]
    AlreadyStaged(String),
    #[error("hotfix `{0}` not in staging area")]
    NotFound(String),
    #[error(
        "hotfix `{id}` is in state `{state:?}`, expected `{expected:?}`"
    )]
    WrongState {
        id: String,
        state: HotfixState,
        expected: HotfixState,
    },
}

impl StagingArea {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert a verified hotfix into staging. Refuses duplicate ids.
    pub fn insert_verified(&mut self, hotfix: Hotfix) -> Result<(), StageError> {
        let id = hotfix.id.clone();
        if self.entries.contains_key(&id) {
            return Err(StageError::AlreadyStaged(id.0));
        }
        self.entries.insert(
            id,
            StagedHotfix {
                hotfix,
                state: HotfixState::Verified,
                pre_apply_snapshot: None,
                applied_at_nanos: None,
            },
        );
        Ok(())
    }

    /// Promote `Verified` → `Staged`. Idempotent if already `Staged`.
    pub fn promote_to_staged(&mut self, id: &HotfixId) -> Result<(), StageError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| StageError::NotFound(id.0.clone()))?;
        match entry.state {
            HotfixState::Staged => Ok(()),
            HotfixState::Verified => {
                entry.state = HotfixState::Staged;
                Ok(())
            }
            other => Err(StageError::WrongState {
                id: id.0.clone(),
                state: other,
                expected: HotfixState::Verified,
            }),
        }
    }

    pub fn get(&self, id: &HotfixId) -> Option<&StagedHotfix> {
        self.entries.get(id)
    }

    pub fn get_mut(&mut self, id: &HotfixId) -> Option<&mut StagedHotfix> {
        self.entries.get_mut(id)
    }

    pub fn remove(&mut self, id: &HotfixId) -> Option<StagedHotfix> {
        self.entries.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::{HotfixClass, HotfixId};

    fn make(id: &str, class: HotfixClass) -> Hotfix {
        let payload = vec![1, 2, 3];
        Hotfix {
            id: HotfixId::new(id),
            class,
            payload_blake3: blake3::hash(&payload).to_hex().to_string(),
            payload,
            ed25519_sig: [0u8; 64],
            issuer_pubkey: [0u8; 32],
            ts: 1,
            class_tier: class.tier(),
        }
    }

    #[test]
    fn insert_and_get_round_trip() {
        let mut sa = StagingArea::new();
        sa.insert_verified(make("a", HotfixClass::KanWeightUpdate)).unwrap();
        assert_eq!(sa.len(), 1);
        let entry = sa.get(&HotfixId::new("a")).unwrap();
        assert_eq!(entry.state, HotfixState::Verified);
    }

    #[test]
    fn duplicate_id_rejected() {
        let mut sa = StagingArea::new();
        sa.insert_verified(make("a", HotfixClass::KanWeightUpdate)).unwrap();
        let err = sa
            .insert_verified(make("a", HotfixClass::KanWeightUpdate))
            .unwrap_err();
        assert!(matches!(err, StageError::AlreadyStaged(_)));
    }

    #[test]
    fn promote_verified_to_staged() {
        let mut sa = StagingArea::new();
        sa.insert_verified(make("a", HotfixClass::KanWeightUpdate)).unwrap();
        sa.promote_to_staged(&HotfixId::new("a")).unwrap();
        assert_eq!(sa.get(&HotfixId::new("a")).unwrap().state, HotfixState::Staged);
        // Idempotent.
        sa.promote_to_staged(&HotfixId::new("a")).unwrap();
    }
}
