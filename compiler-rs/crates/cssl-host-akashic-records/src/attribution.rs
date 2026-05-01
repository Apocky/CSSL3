// § attribution.rs · author-pubkey + AttributionLedger
// § eternal-attribution one-time-per (author · scene-name) · NEVER-revoked

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// 32-byte Ed25519 public-key newtype (canonical fixed-width).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthorPubkey(pub [u8; 32]);

impl AuthorPubkey {
    /// Construct from raw 32 bytes.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Canonical bytes (used for BLAKE3-content-hash).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Tracks which (author, scene-name) pairs have already-claimed
/// `EternalAttribution` · enforces one-time-permanence (spec/18 line 62).
///
/// Eternal-attribution NEVER-revoked · NEVER-removed · t∞.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttributionLedger {
    /// (author, scene-name) → first imprint-id that claimed eternal.
    eternal_claims: BTreeMap<(AuthorPubkey, String), super::imprint::ImprintId>,
    /// Distinct (author, scene-name) pairs claimed eternal · for fast rejection
    /// without scanning the map.
    claimed_pairs: BTreeSet<(AuthorPubkey, String)>,
}

impl AttributionLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` iff this (author, scene-name) pair has already claimed eternal.
    #[must_use]
    pub fn has_claimed(&self, author: &AuthorPubkey, scene_name: &str) -> bool {
        self.claimed_pairs
            .contains(&(*author, scene_name.to_owned()))
    }

    /// Record a one-time eternal-claim · returns `false` if pair was already
    /// claimed (caller should then return `AlreadyOwnedEternal`).
    pub fn try_claim(
        &mut self,
        author: AuthorPubkey,
        scene_name: &str,
        imprint_id: super::imprint::ImprintId,
    ) -> bool {
        let key = (author, scene_name.to_owned());
        if self.claimed_pairs.contains(&key) {
            return false;
        }
        self.claimed_pairs.insert(key.clone());
        self.eternal_claims.insert(key, imprint_id);
        true
    }

    /// Return imprint-id of the original eternal-claim, if any.
    #[must_use]
    pub fn original_claim(
        &self,
        author: &AuthorPubkey,
        scene_name: &str,
    ) -> Option<super::imprint::ImprintId> {
        self.eternal_claims
            .get(&(*author, scene_name.to_owned()))
            .copied()
    }

    #[must_use]
    pub fn claim_count(&self) -> usize {
        self.claimed_pairs.len()
    }
}
