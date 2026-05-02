// § sigma.rs : Σ-mask gating for cross-player replication
//
// Apocky PRIME-DIRECTIVE consent-OS : every replicated cell carries a
// `SigmaMask` that says WHO may observe it. Default-deny ; the producer must
// explicitly opt cells into observation by other peers.
//
// `SigmaMask` here is a per-cell bitmap-by-recipient ; we model peers via
// short `PeerSlot` indices (0..MAX_PEERS) so masks are 64-bit even with 64
// players in a session. The structurally-banned categories (biometric / gaze
// / face / body) are NOT modeled here as data-types — instead, this crate
// REFUSES to publish any cell whose `category` is `Sensitive`. The check
// happens at `gate_for_send` ; tests pin the refusal.

use serde::{Deserialize, Serialize};

/// Maximum peers per netcode session ; matches typical FPS lobby cap.
/// Cell-level masks are 64-bit so this tops out at 64.
pub const MAX_PEERS: usize = 64;

/// Short peer-index ; 0..MAX_PEERS . Persistent identity is the player-pubkey
/// hash kept upstream in `cssl-host-multiplayer-signaling::Peer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct PeerSlot(pub u8);

impl PeerSlot {
    /// As a bit position in a `SigmaMask` (None if out-of-range).
    #[must_use]
    pub fn bit(self) -> Option<u64> {
        if (self.0 as usize) < MAX_PEERS {
            Some(1u64 << self.0)
        } else {
            None
        }
    }
}

/// Replication-category for a cell. `Public` = visible-to-all (default-aware
/// world state) ; `Scoped` = bitmask gates which peers may observe (PvP
/// equipment / private inventory) ; `Local` = client-only, NEVER replicated ;
/// `Sensitive` = STRUCTURALLY BANNED from replication (biometric / gaze /
/// face / body — refusal pinned by tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SigmaCategory {
    /// World-public ; mask ignored.
    Public,
    /// Per-recipient gated by mask bit.
    Scoped,
    /// Client-only ; refused for send.
    Local,
    /// PRIME-DIRECTIVE banned ; refused for send (loud-fail + record).
    Sensitive,
}

/// Per-cell consent mask. `bits` selects recipient peer-slots when category
/// = `Scoped`. Default = all-zeros = default-deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SigmaMask {
    pub category: SigmaCategoryRepr,
    pub bits: u64,
}

/// Wire-friendly category enum (Default impl required for serde-default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SigmaCategoryRepr {
    #[default]
    Local,
    Public,
    Scoped,
    Sensitive,
}

impl From<SigmaCategory> for SigmaCategoryRepr {
    fn from(c: SigmaCategory) -> Self {
        match c {
            SigmaCategory::Public => Self::Public,
            SigmaCategory::Scoped => Self::Scoped,
            SigmaCategory::Local => Self::Local,
            SigmaCategory::Sensitive => Self::Sensitive,
        }
    }
}

impl From<SigmaCategoryRepr> for SigmaCategory {
    fn from(c: SigmaCategoryRepr) -> Self {
        match c {
            SigmaCategoryRepr::Public => Self::Public,
            SigmaCategoryRepr::Scoped => Self::Scoped,
            SigmaCategoryRepr::Local => Self::Local,
            SigmaCategoryRepr::Sensitive => Self::Sensitive,
        }
    }
}

impl SigmaMask {
    /// Default-deny construction. `Local` category ; nothing can leave.
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            category: SigmaCategoryRepr::Local,
            bits: 0,
        }
    }

    /// Public (world-visible) cell. Mask ignored.
    #[must_use]
    pub fn public() -> Self {
        Self {
            category: SigmaCategoryRepr::Public,
            bits: u64::MAX, // semantic only ; Public ignores bits
        }
    }

    /// Scoped to specific peers. Mask bits = recipient-slots.
    #[must_use]
    pub fn scoped(bits: u64) -> Self {
        Self {
            category: SigmaCategoryRepr::Scoped,
            bits,
        }
    }

    /// Add a recipient (Scoped category only).
    pub fn allow(&mut self, peer: PeerSlot) {
        if let Some(b) = peer.bit() {
            self.bits |= b;
        }
    }

    /// Remove a recipient.
    pub fn revoke(&mut self, peer: PeerSlot) {
        if let Some(b) = peer.bit() {
            self.bits &= !b;
        }
    }

    /// May this cell be sent to `recipient` ? Sensitive / Local always-deny.
    #[must_use]
    pub fn allows(&self, recipient: PeerSlot) -> bool {
        match self.category {
            SigmaCategoryRepr::Public => true,
            SigmaCategoryRepr::Scoped => recipient
                .bit()
                .is_some_and(|b| (self.bits & b) != 0),
            SigmaCategoryRepr::Local | SigmaCategoryRepr::Sensitive => false,
        }
    }
}

/// Reasons a `gate_for_send` decision can refuse a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigmaRefusal {
    /// Sensitive category ; STRUCTURALLY BANNED. Triggers attestation-record.
    SensitiveBanned,
    /// Local category ; never crosses the wire.
    LocalOnly,
    /// Scoped category but recipient bit not set.
    ScopedNotInSet,
}

/// Sovereignty-respecting decision : MAY the cell ship to `recipient` ?
///
/// - `Sensitive` → `Err(SensitiveBanned)` ; caller MUST surface to attestation.
/// - `Local`     → `Err(LocalOnly)`.
/// - `Public`    → `Ok(())`.
/// - `Scoped`    → `Ok(())` iff recipient bit set, else `Err(ScopedNotInSet)`.
pub fn gate_for_send(mask: &SigmaMask, recipient: PeerSlot) -> Result<(), SigmaRefusal> {
    match mask.category {
        SigmaCategoryRepr::Sensitive => Err(SigmaRefusal::SensitiveBanned),
        SigmaCategoryRepr::Local => Err(SigmaRefusal::LocalOnly),
        SigmaCategoryRepr::Public => Ok(()),
        SigmaCategoryRepr::Scoped => {
            if mask.allows(recipient) {
                Ok(())
            } else {
                Err(SigmaRefusal::ScopedNotInSet)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_all_is_default_deny() {
        let m = SigmaMask::deny_all();
        for slot in 0u8..16 {
            assert!(!m.allows(PeerSlot(slot)));
        }
    }

    #[test]
    fn public_allows_everyone() {
        let m = SigmaMask::public();
        assert!(m.allows(PeerSlot(0)));
        assert!(m.allows(PeerSlot(63)));
    }

    #[test]
    fn scoped_only_allows_set_bits() {
        let mut m = SigmaMask::scoped(0);
        m.allow(PeerSlot(2));
        m.allow(PeerSlot(7));
        assert!(m.allows(PeerSlot(2)));
        assert!(m.allows(PeerSlot(7)));
        assert!(!m.allows(PeerSlot(3)));
    }

    #[test]
    fn revoke_drops_recipient() {
        let mut m = SigmaMask::scoped(0);
        m.allow(PeerSlot(5));
        assert!(m.allows(PeerSlot(5)));
        m.revoke(PeerSlot(5));
        assert!(!m.allows(PeerSlot(5)));
    }

    #[test]
    fn sensitive_is_structurally_banned() {
        let m = SigmaMask {
            category: SigmaCategoryRepr::Sensitive,
            bits: u64::MAX, // even with all bits set
        };
        assert_eq!(
            gate_for_send(&m, PeerSlot(0)),
            Err(SigmaRefusal::SensitiveBanned)
        );
    }

    #[test]
    fn local_never_egresses() {
        let m = SigmaMask::deny_all(); // Local default
        assert_eq!(gate_for_send(&m, PeerSlot(0)), Err(SigmaRefusal::LocalOnly));
    }

    #[test]
    fn out_of_range_peer_slot_denied() {
        let m = SigmaMask::scoped(u64::MAX);
        // PeerSlot beyond MAX_PEERS has no valid bit
        assert!(!m.allows(PeerSlot(64)));
        assert!(!m.allows(PeerSlot(255)));
    }
}
