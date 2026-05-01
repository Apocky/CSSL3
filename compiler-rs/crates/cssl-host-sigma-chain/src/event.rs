// § event.rs — SigmaEvent core types + canonical-encoding stub
// §§ EventKind narrow-list per spec/14 § ARCHITECTURE TIER-2 shareable-events

use serde::{Deserialize, Serialize};

use crate::privacy::PrivacyTier;
use crate::sign::{PUBKEY_LEN, SIG_LEN};

/// 256-bit deterministic event identifier (BLAKE3 of canonical-bytes prefix).
pub type EventId = [u8; 32];

/// Canonical event categories per spec/14 § TIER-2 shareable-events list.
/// Narrow-list intentional : adding a kind requires spec-update + recompile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Loot drop emission (Bazaar-listing precursor).
    LootDrop,
    /// Combat outcome (win/loss/flee).
    CombatOutcome,
    /// Crafting success record.
    CraftSuccess,
    /// NPC death (with epitaph data).
    NpcDeath,
    /// Multiversal-Nemesis defeat (cross-shard rumor).
    NemesisDefeat,
    /// Asset/gear transfer between players.
    GearTransfer,
    /// Achievement unlock notification.
    AchievementUnlock,
    /// KAN canary observation (transparency channel per spec/14 § /transparency).
    KanCanary,
}

impl EventKind {
    /// Stable tag-string used inside canonical-bytes (must NEVER change for an existing kind).
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            EventKind::LootDrop => "loot_drop",
            EventKind::CombatOutcome => "combat_outcome",
            EventKind::CraftSuccess => "craft_success",
            EventKind::NpcDeath => "npc_death",
            EventKind::NemesisDefeat => "nemesis_defeat",
            EventKind::GearTransfer => "gear_transfer",
            EventKind::AchievementUnlock => "achievement_unlock",
            EventKind::KanCanary => "kan_canary",
        }
    }
}

/// Application-level event payload, kept opaque-ish so canonical-bytes are stable.
///
/// Field order is canonical : BTreeMap-only · Vec<u8> raw-bytes · NO HashMap/Random-iteration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SigmaPayload {
    /// Per-event domain bytes — already canonical-serialized by the emitter.
    pub bytes: Vec<u8>,
}

impl SigmaPayload {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

/// A single Σ-Chain event after sign-pipeline completion.
///
/// Construction goes through [`crate::sign::sign_event`] · NEVER hand-build with arbitrary `sig`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigmaEvent {
    /// Deterministic id : BLAKE3(canonical_bytes_without_sig).
    pub id: EventId,
    /// Event category (canonical narrow-list).
    pub kind: EventKind,
    /// Server-tick or monotonic-counter timestamp (NOT trusted clock — per spec/14).
    pub ts: u64,
    /// Emitter Ed25519 public-key.
    pub emitter_pubkey: [u8; PUBKEY_LEN],
    /// Optional parent-event for lineage chains (root = None).
    pub parent_event_id: Option<EventId>,
    /// BLAKE3 of canonical payload-bytes (32B).
    pub payload_blake3: [u8; 32],
    /// Privacy-tier @ emit-time. LocalOnly events MUST never egress (structural-guard).
    pub privacy_tier: PrivacyTier,
    /// Ed25519 signature over canonical-bytes (64B).
    pub ed25519_sig: [u8; SIG_LEN],
}

impl SigmaEvent {
    /// Returns true iff this event may legally egress the local TIER-1 boundary.
    /// LocalOnly tier ALWAYS returns false ; checked by [`crate::privacy::egress_check`].
    #[must_use]
    pub fn may_egress(&self) -> bool {
        !matches!(self.privacy_tier, PrivacyTier::LocalOnly)
    }
}
