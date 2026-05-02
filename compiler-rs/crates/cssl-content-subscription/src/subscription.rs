//! § subscription — Subscription record + registry + target-kinds.
//!
//! § DATA MODEL
//!   `Subscription { id, subscriber_pubkey, target_kind, target_id,
//!                   sigma_mask, frequency, created_at_ns, revoked_at_ns NULLABLE }`
//!
//! § TARGET KINDS (3)
//!   - `Creator(pubkey)`     : follow-author across all their content
//!   - `Tag(tag-string)`     : follow a tag (e.g. "horror" / "puzzle")
//!   - `ContentChain(id)`    : follow a remix-chain (root content_id)
//!
//! § FREQUENCY (3)
//!   - `Realtime` : push notif on every event (rate-limited 1/min default)
//!   - `Daily`    : roll-up to one digest notif per 24 h
//!   - `Manual`   : never auto-push ; player pulls feed on demand
//!
//! § INVARIANTS
//!   - `revoked_at_ns` IS-SET ⇒ `is_active() = false` ; future notifications skipped.
//!   - Aggregate counts (k-anon ≥ 10) gate `subscription_count_for_target`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// § Hard k-anon floor for any aggregate count exposed to clients.
/// Below this, `subscription_count_for_target` returns `None`.
pub const K_ANON_MIN_AGGREGATE: u64 = 10;

/// § Stable subscription identifier : 32-byte BLAKE3 of (pubkey · target · created-at).
pub type SubscriptionId = [u8; 32];

/// § Target identifier : structurally bounded to the kind it pairs with.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TargetId {
    /// 32-byte Ed25519 author pubkey (used with `TargetKind::Creator`).
    Creator([u8; 32]),
    /// UTF-8 tag string (used with `TargetKind::Tag`). 1..=64 chars.
    Tag(String),
    /// 32-byte content-id (root of a remix-chain — used with `TargetKind::ContentChain`).
    ContentChain([u8; 32]),
}

impl TargetId {
    #[must_use]
    pub fn kind(&self) -> TargetKind {
        match self {
            Self::Creator(_) => TargetKind::Creator,
            Self::Tag(_) => TargetKind::Tag,
            Self::ContentChain(_) => TargetKind::ContentChain,
        }
    }
}

/// § Target kind enum (3 variants · stable repr for serde).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum TargetKind {
    /// (1) Follow-an-author across all their published content.
    Creator = 1,
    /// (2) Follow a tag (e.g. "horror" / "puzzle"). 1..=64 char UTF-8.
    Tag = 2,
    /// (3) Follow a remix-chain (root content_id).
    ContentChain = 3,
}

impl TargetKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Creator => "creator",
            Self::Tag => "tag",
            Self::ContentChain => "content-chain",
        }
    }
}

/// § Frequency of pushed notifications (3 variants).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Frequency {
    /// Push on every matching event (rate-limited 1/min default).
    Realtime = 1,
    /// Roll up to one digest per 24 h.
    Daily = 2,
    /// Never push ; player pulls feed on demand.
    Manual = 3,
}

impl Frequency {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Realtime => "realtime",
            Self::Daily => "daily",
            Self::Manual => "manual",
        }
    }
}

/// § Subscription record (in-memory + serializable for SQL row 1:1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub subscriber_pubkey: [u8; 32],
    pub target: TargetId,
    pub sigma_mask: [u8; 16],
    pub frequency: Frequency,
    pub created_at_ns: u64,
    pub revoked_at_ns: Option<u64>,
}

impl Subscription {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.revoked_at_ns.is_none()
    }
}

/// § Errors the registry can raise.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubscriptionError {
    #[error("tag must be 1..=64 chars (was {0})")]
    TagLength(usize),
    #[error("subscription not found")]
    NotFound,
    #[error("subscription already revoked")]
    AlreadyRevoked,
}

/// § In-memory registry · authoritative for unit tests + sync-store cache.
/// SQL row is the source of truth in production ; this is the runtime mirror.
#[derive(Debug, Default, Clone)]
pub struct SubscriptionRegistry {
    subs: BTreeMap<SubscriptionId, Subscription>,
}

impl SubscriptionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) a subscription. Returns the (potentially newly minted) id.
    /// Validates target-shape : tags must be 1..=64 chars.
    pub fn subscribe(
        &mut self,
        subscriber_pubkey: [u8; 32],
        target: TargetId,
        sigma_mask: [u8; 16],
        frequency: Frequency,
        now_ns: u64,
    ) -> Result<SubscriptionId, SubscriptionError> {
        if let TargetId::Tag(t) = &target {
            let n = t.chars().count();
            if !(1..=64).contains(&n) {
                return Err(SubscriptionError::TagLength(n));
            }
        }
        let id = derive_subscription_id(&subscriber_pubkey, &target, now_ns);
        let s = Subscription {
            id,
            subscriber_pubkey,
            target,
            sigma_mask,
            frequency,
            created_at_ns: now_ns,
            revoked_at_ns: None,
        };
        self.subs.insert(id, s);
        Ok(id)
    }

    /// Sovereign-revoke. ALWAYS available · sets `revoked_at_ns` if currently active.
    pub fn revoke(
        &mut self,
        id: SubscriptionId,
        now_ns: u64,
    ) -> Result<(), SubscriptionError> {
        let s = self
            .subs
            .get_mut(&id)
            .ok_or(SubscriptionError::NotFound)?;
        if s.revoked_at_ns.is_some() {
            return Err(SubscriptionError::AlreadyRevoked);
        }
        s.revoked_at_ns = Some(now_ns);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &SubscriptionId) -> Option<&Subscription> {
        self.subs.get(id)
    }

    /// All ACTIVE subscriptions matching `target` (creator / tag / chain).
    /// Used by cascade hooks to find subscribers to notify on a publish event.
    #[must_use]
    pub fn matching_active(&self, target: &TargetId) -> Vec<&Subscription> {
        self.subs
            .values()
            .filter(|s| s.is_active() && &s.target == target)
            .collect()
    }

    /// Aggregate count for a target (k-anon ≥ 10 ; returns None if below).
    /// Used by sibling W12-6 discover-pages to surface "trending" creators
    /// without leaking individual subscriber identities.
    #[must_use]
    pub fn subscription_count_for_target(&self, target: &TargetId) -> Option<u64> {
        let n = self
            .subs
            .values()
            .filter(|s| s.is_active() && &s.target == target)
            .count() as u64;
        if n < K_ANON_MIN_AGGREGATE {
            None
        } else {
            Some(n)
        }
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &Subscription> {
        self.subs.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.subs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.subs.is_empty()
    }
}

/// Stable subscription-id : BLAKE3(pubkey · target-bytes · ts-le8).
fn derive_subscription_id(
    pubkey: &[u8; 32],
    target: &TargetId,
    now_ns: u64,
) -> SubscriptionId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(pubkey);
    match target {
        TargetId::Creator(k) => {
            hasher.update(b"creator");
            hasher.update(k);
        }
        TargetId::Tag(t) => {
            hasher.update(b"tag");
            hasher.update(t.as_bytes());
        }
        TargetId::ContentChain(c) => {
            hasher.update(b"chain");
            hasher.update(c);
        }
    }
    hasher.update(&now_ns.to_le_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(seed: u8) -> [u8; 32] {
        [seed; 32]
    }
    fn mask(seed: u8) -> [u8; 16] {
        [seed; 16]
    }

    #[test]
    fn subscribe_to_creator_then_revoke() {
        let mut reg = SubscriptionRegistry::new();
        let id = reg
            .subscribe(
                pk(1),
                TargetId::Creator(pk(2)),
                mask(3),
                Frequency::Realtime,
                1_000,
            )
            .unwrap();
        assert!(reg.get(&id).unwrap().is_active());
        reg.revoke(id, 2_000).unwrap();
        assert!(!reg.get(&id).unwrap().is_active());
        // Double-revoke errors.
        assert_eq!(
            reg.revoke(id, 3_000).unwrap_err(),
            SubscriptionError::AlreadyRevoked
        );
    }

    #[test]
    fn subscribe_tag_length_validation() {
        let mut reg = SubscriptionRegistry::new();
        assert!(matches!(
            reg.subscribe(
                pk(1),
                TargetId::Tag(String::new()),
                mask(0),
                Frequency::Manual,
                1
            )
            .unwrap_err(),
            SubscriptionError::TagLength(0)
        ));
        let too_long = "x".repeat(65);
        assert!(matches!(
            reg.subscribe(
                pk(1),
                TargetId::Tag(too_long),
                mask(0),
                Frequency::Manual,
                1
            )
            .unwrap_err(),
            SubscriptionError::TagLength(65)
        ));
    }

    #[test]
    fn matching_active_excludes_revoked() {
        let mut reg = SubscriptionRegistry::new();
        let creator = TargetId::Creator(pk(9));
        let id1 = reg
            .subscribe(pk(1), creator.clone(), mask(0), Frequency::Realtime, 1)
            .unwrap();
        let _id2 = reg
            .subscribe(pk(2), creator.clone(), mask(0), Frequency::Daily, 2)
            .unwrap();
        reg.revoke(id1, 3).unwrap();
        let v = reg.matching_active(&creator);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].subscriber_pubkey, pk(2));
    }

    #[test]
    fn aggregate_count_below_k_anon_returns_none() {
        let mut reg = SubscriptionRegistry::new();
        let creator = TargetId::Creator(pk(9));
        for i in 0..9u8 {
            reg.subscribe(pk(i + 1), creator.clone(), mask(0), Frequency::Realtime, i as u64)
                .unwrap();
        }
        assert_eq!(reg.subscription_count_for_target(&creator), None);
        // Crossing k-anon = 10 unlocks aggregate.
        reg.subscribe(pk(99), creator.clone(), mask(0), Frequency::Realtime, 10)
            .unwrap();
        assert_eq!(reg.subscription_count_for_target(&creator), Some(10));
    }

    #[test]
    fn frequency_names_stable() {
        assert_eq!(Frequency::Realtime.name(), "realtime");
        assert_eq!(Frequency::Daily.name(), "daily");
        assert_eq!(Frequency::Manual.name(), "manual");
    }

    #[test]
    fn target_kind_inferred_from_target_id() {
        assert_eq!(TargetId::Creator(pk(0)).kind(), TargetKind::Creator);
        assert_eq!(
            TargetId::Tag("horror".into()).kind(),
            TargetKind::Tag
        );
        assert_eq!(TargetId::ContentChain(pk(0)).kind(), TargetKind::ContentChain);
    }
}
