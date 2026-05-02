//! § event — `LootDropEvent` + Σ-Chain anchor
//!
//! Per W13-8 spec :
//!   "Σ-Chain-anchor drop-event (immutable-history · sovereign-revocable)"
//!
//! Anchor-cadence : **every drop is anchored**. A drop is a discrete event
//! (one combat-encounter end), so per-drop emission is the appropriate
//! granularity — no batching, no aggregation, no implicit lossiness.
//!
//! The drop is serialized to a [`SigmaEvent`] of [`EventKind::LootDrop`]
//! with the canonical-bytes derived from [`LootItem::canonical_bytes`].
//! The `parent_event_id` is filled with the prior-drop event-id so a
//! per-player drop-chain emerges naturally.

use cssl_host_sigma_chain::{
    sign_event, EventId, EventKind, PrivacyTier, SigmaEvent, SigmaPayload,
};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use crate::item::LootItem;

// ───────────────────────────────────────────────────────────────────────
// § LootDropEvent — payload schema
// ───────────────────────────────────────────────────────────────────────

/// A single loot-drop event ready for Σ-Chain anchoring.
///
/// Carries the item plus the wall-clock timestamp (game-tick or monotonic
/// counter — NOT trusted clock per `cssl_host_sigma_chain` convention).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootDropEvent {
    /// The dropped item.
    pub item: LootItem,
    /// Server-tick / monotonic-counter timestamp.
    pub ts: u64,
    /// Optional prior drop-event id for the per-player chain.
    pub prior_drop_id: Option<EventId>,
    /// Privacy-tier @ emit-time. Default `Pseudonymous` for Akashic-feed.
    pub privacy_tier: PrivacyTier,
}

impl LootDropEvent {
    /// Construct a new event with sensible defaults.
    #[must_use]
    pub fn new(item: LootItem, ts: u64) -> Self {
        Self {
            item,
            ts,
            prior_drop_id: None,
            privacy_tier: PrivacyTier::Pseudonymous,
        }
    }

    /// Builder-style chain-link.
    #[must_use]
    pub fn with_parent(mut self, parent: EventId) -> Self {
        self.prior_drop_id = Some(parent);
        self
    }

    /// Override the privacy-tier (e.g. `LocalOnly` for a private-mode session).
    #[must_use]
    pub fn with_privacy_tier(mut self, tier: PrivacyTier) -> Self {
        self.privacy_tier = tier;
        self
    }

    /// Build the [`SigmaPayload`] for this event.
    #[must_use]
    pub fn build_payload(&self) -> SigmaPayload {
        SigmaPayload::new(self.item.canonical_bytes())
    }
}

// ───────────────────────────────────────────────────────────────────────
// § anchor_drop_to_sigma_chain
// ───────────────────────────────────────────────────────────────────────

/// Anchor a drop-event to the Σ-Chain. Returns the signed `SigmaEvent`
/// ready for ledger-insert.
///
/// **Pipeline** :
///   1. Serialize `event.item` to canonical-bytes (item.canonical_bytes()).
///   2. Wrap in [`SigmaPayload`].
///   3. Call [`sign_event`] with kind=[`EventKind::LootDrop`].
///   4. Caller is responsible for `SigmaLedger::insert(event)`.
///
/// This keeps the anchor pure (no I/O ; no ledger mutation) so callers
/// can decide whether to insert immediately, batch, or defer to a
/// privileged thread.
#[must_use]
pub fn anchor_drop_to_sigma_chain(signer: &SigningKey, event: &LootDropEvent) -> SigmaEvent {
    let payload = event.build_payload();
    sign_event(
        signer,
        EventKind::LootDrop,
        event.ts,
        event.prior_drop_id,
        &payload,
        event.privacy_tier,
    )
}
