//! § decline — sovereign-revoke registry.
//!
//! § ROLE
//!   A creator may decline auto-playtest for any of their content. The
//!   registry is queried by the driver before a session starts ; if the
//!   creator declined, the driver fails fast with [`PlayTestError::Declined`].
//!
//! § COST
//!   Per spec : declined-content is HELD but EXCLUDED from trending until
//!   a fresh playtest is consented-to. The cost is enforced by the
//!   downstream discover/trending crate (W12-6) — this crate exposes the
//!   decline-record + provides the read-side query.
//!
//! § ANCHORING
//!   Each `DeclineRecord` carries a timestamp + a creator-pubkey-hash so
//!   the host cannot retroactively claim consent. The Σ-Chain anchor
//!   path lives in [`crate::anchor::anchor_decline`].

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::session::PlayTestError;

/// § One decline-record. Persisted by the host's keystore ; replays of
/// the registry MUST yield equal `DeclineRecord` rows (deterministic
/// ordering via `BTreeMap` storage in [`SovereignDecline`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclineRecord {
    /// Content the creator declined for.
    pub content_id: u32,
    /// First-8-bytes of BLAKE3(creator-pubkey) — privacy-preserving link.
    pub creator_pubkey_hash: [u8; 8],
    /// Minutes-since-epoch the decline was logged.
    pub ts_minutes: u32,
    /// Optional human-readable reason ; capped at 256 bytes.
    pub reason: String,
}

impl DeclineRecord {
    /// § Maximum reason length in bytes (UTF-8). Anything longer is
    /// rejected at construction.
    pub const REASON_MAX_BYTES: usize = 256;

    /// § Construct + validate. Returns `Err` if `reason` exceeds the cap.
    pub fn new(
        content_id: u32,
        creator_pubkey_hash: [u8; 8],
        ts_minutes: u32,
        reason: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let r = reason.into();
        if r.len() > Self::REASON_MAX_BYTES {
            return Err("reason exceeds REASON_MAX_BYTES");
        }
        Ok(Self {
            content_id,
            creator_pubkey_hash,
            ts_minutes,
            reason: r,
        })
    }
}

/// § In-memory decline-registry. Stage-0 self-sufficient ; the host can
/// persist it to disk via the included serde impls.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SovereignDecline {
    /// `content_id` → record. `BTreeMap` for stable iteration order.
    pub records: BTreeMap<u32, DeclineRecord>,
}

impl SovereignDecline {
    /// § Empty registry ; equivalent to `Default::default()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// § Set (or replace) a decline-record. The most-recent record for a
    /// given `content_id` wins.
    pub fn set(&mut self, rec: DeclineRecord) {
        self.records.insert(rec.content_id, rec);
    }

    /// § Remove the decline-record for `content_id` ; returns `true` if a
    /// record was present.
    pub fn revoke_decline(&mut self, content_id: u32) -> bool {
        self.records.remove(&content_id).is_some()
    }

    /// § Is the given content currently declined ?
    #[must_use]
    pub fn is_declined(&self, content_id: u32) -> bool {
        self.records.contains_key(&content_id)
    }

    /// § Pre-flight check used by the driver. Returns `Err(Declined)` if
    /// the creator declined ; `Ok` otherwise.
    pub fn check(&self, content_id: u32) -> Result<(), PlayTestError> {
        if self.is_declined(content_id) {
            Err(PlayTestError::Declined(content_id))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decline_blocks_check() {
        let mut reg = SovereignDecline::new();
        let rec = DeclineRecord::new(7, [0; 8], 100, "I want to revise first").unwrap();
        reg.set(rec);
        assert!(reg.is_declined(7));
        assert_eq!(reg.check(7), Err(PlayTestError::Declined(7)));
    }

    #[test]
    fn revoke_decline_clears_registry() {
        let mut reg = SovereignDecline::new();
        reg.set(DeclineRecord::new(7, [1; 8], 100, "").unwrap());
        assert!(reg.revoke_decline(7));
        assert!(!reg.is_declined(7));
        assert_eq!(reg.check(7), Ok(()));
    }

    #[test]
    fn reason_too_long_rejected() {
        let big = "x".repeat(DeclineRecord::REASON_MAX_BYTES + 1);
        assert!(DeclineRecord::new(1, [0; 8], 0, big).is_err());
    }
}
