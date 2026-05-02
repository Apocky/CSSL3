//! § RoyaltyShareGift — GIFT-ONLY · ¬ enforced · sovereign-revocable always-true.
//!
//! § PRIME-DIRECTIVE invariant : a creator-of-remix MAY pledge a percentage
//! of any tips received-on-the-remix to flow back to the original creator.
//! This is a GIFT-PLEDGE — the platform NEVER enforces extraction. The
//! creator can cancel the pledge at any time and the running cumulative-
//! gifted total is informational-only.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// § Sovereignty axiom embedded in code for transparency-audit.
pub const ROYALTY_SHARE_AXIOM: &str =
    "GIFT-ONLY · ¬ enforced-extraction · sovereign-revocable always-true · ¬ pay-for-power";

/// Pledged tip-share from remix-creator to original-creator. PURELY GIFT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoyaltyShareGift {
    /// 0..=100 · creator-set · ¬ binding. 0 = no pledge.
    pub pledged_pct: u8,
    /// Running-total of gifts-given via Stripe-tip-channel (lamports-style
    /// integer · 1 unit = 1/100 of smallest currency-cent · informational).
    pub cumulative_gifted_lamports: u64,
    /// ALWAYS true · creator-of-remix can-stop-pledge any-time. Public-API
    /// constructors enforce this — there is no path to set it false.
    pub sovereign_revocable: bool,
}

#[derive(Debug, Error)]
pub enum RoyaltyShareGiftError {
    #[error("pledged_pct {got} exceeds 100 (must be 0..=100)")]
    PctOutOfRange { got: u8 },
    #[error("sovereign_revocable cannot be set false (PRIME-DIRECTIVE invariant)")]
    NotRevocable,
}

impl RoyaltyShareGift {
    /// No-pledge default. Cumulative gifts = 0. Always sovereign-revocable.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            pledged_pct: 0,
            cumulative_gifted_lamports: 0,
            sovereign_revocable: true,
        }
    }

    /// Pledge a tip-share. `pct` must be 0..=100. Always sovereign-revocable.
    pub fn pledged(pct: u8) -> Result<Self, RoyaltyShareGiftError> {
        if pct > 100 {
            return Err(RoyaltyShareGiftError::PctOutOfRange { got: pct });
        }
        Ok(Self {
            pledged_pct: pct,
            cumulative_gifted_lamports: 0,
            sovereign_revocable: true,
        })
    }

    /// Increase the running cumulative-gifted total. Returns the new total.
    /// Caller passes the per-tip lamport amount that was actually-shared.
    /// Saturating-add — never panics on overflow.
    pub fn record_gift(&mut self, gifted_lamports: u64) -> u64 {
        self.cumulative_gifted_lamports = self
            .cumulative_gifted_lamports
            .saturating_add(gifted_lamports);
        self.cumulative_gifted_lamports
    }

    /// Cancel the pledge (sovereign-revoke). Pct → 0. Cumulative-total
    /// preserved (history is not erased). Cannot fail under any condition.
    pub fn revoke(&mut self) {
        self.pledged_pct = 0;
        // sovereign_revocable stays true · cumulative_gifted_lamports preserved.
    }

    /// Validate invariants pre-persist. Rejects sovereign_revocable=false
    /// (which can only happen via direct-field-mutation attempting to
    /// circumvent the API).
    pub fn validate(&self) -> Result<(), RoyaltyShareGiftError> {
        if self.pledged_pct > 100 {
            return Err(RoyaltyShareGiftError::PctOutOfRange {
                got: self.pledged_pct,
            });
        }
        if !self.sovereign_revocable {
            return Err(RoyaltyShareGiftError::NotRevocable);
        }
        Ok(())
    }

    /// Compute the gift-amount (lamports) for a given tip-amount based
    /// on `pledged_pct`. Returns 0 when pct=0. Integer-rounded-down.
    #[must_use]
    pub fn gift_share_of(&self, tip_amount_lamports: u64) -> u64 {
        if self.pledged_pct == 0 {
            return 0;
        }
        // pct 0..=100 fits in u128 multiply path.
        let n = (tip_amount_lamports as u128) * (self.pledged_pct as u128) / 100u128;
        u64::try_from(n).unwrap_or(u64::MAX)
    }
}

impl Default for RoyaltyShareGift {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_starts_zero_and_revocable() {
        let g = RoyaltyShareGift::none();
        assert_eq!(g.pledged_pct, 0);
        assert_eq!(g.cumulative_gifted_lamports, 0);
        assert!(g.sovereign_revocable);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn pledge_25pct_share_arithmetic() {
        let g = RoyaltyShareGift::pledged(25).unwrap();
        assert_eq!(g.gift_share_of(1000), 250);
        assert_eq!(g.gift_share_of(0), 0);
    }

    #[test]
    fn pledge_over_100_rejected() {
        matches!(
            RoyaltyShareGift::pledged(101).unwrap_err(),
            RoyaltyShareGiftError::PctOutOfRange { .. }
        );
    }

    #[test]
    fn revoke_zeroes_pct_preserves_history() {
        let mut g = RoyaltyShareGift::pledged(40).unwrap();
        g.record_gift(500);
        g.record_gift(700);
        assert_eq!(g.cumulative_gifted_lamports, 1200);
        g.revoke();
        assert_eq!(g.pledged_pct, 0);
        assert_eq!(g.cumulative_gifted_lamports, 1200, "history preserved");
        assert!(g.sovereign_revocable);
    }

    #[test]
    fn validate_rejects_tampered_revocable_false() {
        let mut g = RoyaltyShareGift::none();
        g.sovereign_revocable = false;
        matches!(g.validate().unwrap_err(), RoyaltyShareGiftError::NotRevocable);
    }

    #[test]
    fn record_gift_saturates_no_panic() {
        let mut g = RoyaltyShareGift::pledged(10).unwrap();
        g.cumulative_gifted_lamports = u64::MAX - 5;
        let total = g.record_gift(1000);
        assert_eq!(total, u64::MAX);
    }
}
