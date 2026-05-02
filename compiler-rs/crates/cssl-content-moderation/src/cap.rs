//! § cap — Σ-mask cap-bit policy for moderation actions.
//! ════════════════════════════════════════════════════════════════════════
//!
//! Cap-bit layout (byte) :
//!   bit 0 : MOD_CAP_FLAG_SUBMIT       (community-default-ON · revocable)
//!   bit 1 : MOD_CAP_APPEAL            (author-cap · always-ON for content-author)
//!   bit 2 : MOD_CAP_CURATE_A          (community-elected · time-limited)
//!   bit 3 : MOD_CAP_CURATE_B          (substrate-team-appointed · public)
//!   bit 4 : MOD_CAP_CHAIN_ANCHOR      (curator-decision Σ-Chain-write)
//!   bit 5 : MOD_CAP_AGGREGATE_READ    (author transparency-read)
//!   bits 6..7 : reserved (must be 0)

use serde::{Deserialize, Serialize};

pub const MOD_CAP_FLAG_SUBMIT: u8 = 0x01;
pub const MOD_CAP_APPEAL: u8 = 0x02;
pub const MOD_CAP_CURATE_A: u8 = 0x04;
pub const MOD_CAP_CURATE_B: u8 = 0x08;
pub const MOD_CAP_CHAIN_ANCHOR: u8 = 0x10;
pub const MOD_CAP_AGGREGATE_READ: u8 = 0x20;

pub const MOD_CAP_RESERVED_MASK: u8 = 0xC0;

/// Curator cap classes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CapClass {
    /// Community-elected · time-limited · revocable-by-creator-community.
    CommunityElected,
    /// Substrate-team-appointed · rare cases · fully-public-actions.
    SubstrateAppointed,
}

impl CapClass {
    pub fn cap_bit(self) -> u8 {
        match self {
            CapClass::CommunityElected => MOD_CAP_CURATE_A,
            CapClass::SubstrateAppointed => MOD_CAP_CURATE_B,
        }
    }
}

/// Per-actor cap-policy. Holds the bitmask of granted caps + an optional
/// expiry epoch-second (0 = never expires).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapPolicy {
    pub bits: u8,
    pub expires_at: u32,
}

impl CapPolicy {
    pub fn new(bits: u8, expires_at: u32) -> Self {
        Self { bits, expires_at }
    }
    pub fn allows(&self, required: u8, now: u32) -> bool {
        if self.expires_at != 0 && now >= self.expires_at {
            return false;
        }
        (self.bits & required) == required && (self.bits & MOD_CAP_RESERVED_MASK) == 0
    }
    pub fn revoke(&mut self) {
        self.bits = 0;
    }
    pub fn is_revoked(&self) -> bool {
        self.bits == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_policy_allows_when_bit_set() {
        let p = CapPolicy::new(MOD_CAP_FLAG_SUBMIT, 0);
        assert!(p.allows(MOD_CAP_FLAG_SUBMIT, 1_000));
    }

    #[test]
    fn cap_policy_denies_missing_bit() {
        let p = CapPolicy::new(MOD_CAP_FLAG_SUBMIT, 0);
        assert!(!p.allows(MOD_CAP_CURATE_A, 1_000));
    }

    #[test]
    fn cap_policy_denies_expired() {
        let p = CapPolicy::new(MOD_CAP_FLAG_SUBMIT, 100);
        assert!(p.allows(MOD_CAP_FLAG_SUBMIT, 50));
        assert!(!p.allows(MOD_CAP_FLAG_SUBMIT, 200));
    }

    #[test]
    fn cap_policy_revoke_zeroes() {
        let mut p = CapPolicy::new(MOD_CAP_CURATE_A | MOD_CAP_CHAIN_ANCHOR, 0);
        p.revoke();
        assert!(p.is_revoked());
        assert!(!p.allows(MOD_CAP_CURATE_A, 1_000));
    }

    #[test]
    fn cap_policy_reserved_bits_deny() {
        // Reserved bits set ⟶ malformed-tampered ⟶ deny-all.
        let p = CapPolicy::new(MOD_CAP_FLAG_SUBMIT | 0x80, 0);
        assert!(!p.allows(MOD_CAP_FLAG_SUBMIT, 1_000));
    }
}
