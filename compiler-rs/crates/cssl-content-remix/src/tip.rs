//! § tip — gift-channel via Stripe-integration. 100% to-tipped-creator
//! (minus Stripe-fee). Platform never appears in money-path.
//!
//! § INVARIANTS (PRIME-DIRECTIVE)
//!   ─ ¬ platform-tax (no cut taken on top)
//!   ─ ¬ pay-for-power (cosmetic-only-axiom · gift-pure)
//!   ─ sovereign-revocable (sender can refund per Stripe ToS · receiver-
//!     creator can revoke royalty-pledge any-time)
//!   ─ records cumulative_gifted_lamports on the recipient's RoyaltyShareGift

use crate::link::ContentId;
use crate::royalty::RoyaltyShareGift;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// § Sovereignty axiom embedded in code for transparency-audit.
pub const STRIPE_TIP_AXIOM: &str =
    "100%-to-tipped-creator · ¬ platform-tax · ¬ pay-for-power · sovereign-revocable-pledge · gift-channel-only";

/// Receipt returned after a successful tip. Mirrors the cssl-edge endpoint
/// response shape for 1:1 server-side parity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TipReceipt {
    /// Stripe checkout-session id (cs_xxxx) — opaque to creators.
    pub stripe_session_id: String,
    /// Receiver creator pubkey (32-byte hex).
    pub to_creator_pubkey: String,
    /// Content-id being tipped.
    pub content_id: ContentId,
    /// Amount in lamports (smallest-unit) BEFORE Stripe-fee.
    pub gross_lamports: u64,
    /// Estimated stripe-fee deducted (informational · actual in webhook).
    pub stripe_fee_estimate_lamports: u64,
    /// Net lamports the creator should receive (gross - estimate).
    pub net_lamports_to_creator: u64,
    /// Optional gift-share lamports flowed to ORIGINAL creator (when the
    /// tipped content is itself a remix with `royalty_share_gift.pledged_pct`
    /// > 0). Always 0 when tipping a genesis content. ¬ enforced ; only
    /// flows when `royalty_share_gift.sovereign_revocable && pledged_pct > 0`.
    pub onward_gift_share_lamports: u64,
    /// Stable Unix-seconds timestamp of receipt creation.
    pub created_at: u64,
}

#[derive(Debug, Error)]
pub enum TipFlowError {
    #[error("amount {got} below minimum 50 lamports (Stripe minimum)")]
    AmountTooSmall { got: u64 },
    #[error("amount {got} exceeds platform sanity-cap 100_000_000 lamports")]
    AmountTooLarge { got: u64 },
    #[error("creator pubkey hex must be 64 lower-hex chars (got {got})")]
    BadCreatorPubkey { got: usize },
    #[error("content_id length {got} not in 1..=64")]
    BadContentId { got: usize },
    #[error("royalty pledge marked non-revocable (PRIME-DIRECTIVE invariant)")]
    NonRevocablePledge,
}

/// Stripe-fee estimate (2.9% + 30 lamports flat per Stripe's standard rate
/// at time-of-writing). Pure-function so test-suites are deterministic.
#[must_use]
pub fn stripe_fee_estimate(gross_lamports: u64) -> u64 {
    // 2.9% rounded up + 30 flat. Use u128 to avoid mid-multiplication wrap.
    let pct = ((gross_lamports as u128) * 29 + 999) / 1000; // 2.9% with ceiling
    u64::try_from(pct + 30).unwrap_or(u64::MAX)
}

/// Tip-flow orchestrator. Pure-state machine — does NOT actually call Stripe.
/// The cssl-edge `/api/content/tip.ts` endpoint adapts this output into a
/// Stripe checkout-session create-call.
pub struct TipFlow;

impl TipFlow {
    /// Plan a tip. Validates the amount + ids and computes the receipt
    /// without calling Stripe. Caller invokes Stripe with the resulting
    /// gross/net amounts.
    pub fn plan_tip(
        to_creator_pubkey: &str,
        content_id: &ContentId,
        gross_lamports: u64,
        recipient_royalty_pledge: Option<&RoyaltyShareGift>,
        created_at: u64,
        stripe_session_id: String,
    ) -> Result<TipReceipt, TipFlowError> {
        if gross_lamports < 50 {
            return Err(TipFlowError::AmountTooSmall {
                got: gross_lamports,
            });
        }
        if gross_lamports > 100_000_000 {
            return Err(TipFlowError::AmountTooLarge {
                got: gross_lamports,
            });
        }
        if to_creator_pubkey.len() != 64 {
            return Err(TipFlowError::BadCreatorPubkey {
                got: to_creator_pubkey.len(),
            });
        }
        if content_id.is_empty() || content_id.len() > 64 {
            return Err(TipFlowError::BadContentId {
                got: content_id.len(),
            });
        }
        let fee = stripe_fee_estimate(gross_lamports);
        let net = gross_lamports.saturating_sub(fee);

        // Onward-gift-share to original-creator — ONLY when the recipient's
        // own RoyaltyShareGift pledges a percentage. This is GIFT-ONLY :
        // we still send `net` to the recipient ; the recipient may then
        // gift `onward` to their own original-creator. Records here are
        // informational — the actual onward-gift flow is a SEPARATE
        // /api/content/tip call by the recipient's wallet.
        let onward = if let Some(pledge) = recipient_royalty_pledge {
            if !pledge.sovereign_revocable {
                return Err(TipFlowError::NonRevocablePledge);
            }
            pledge.gift_share_of(net)
        } else {
            0
        };

        Ok(TipReceipt {
            stripe_session_id,
            to_creator_pubkey: to_creator_pubkey.to_string(),
            content_id: content_id.clone(),
            gross_lamports,
            stripe_fee_estimate_lamports: fee,
            net_lamports_to_creator: net,
            onward_gift_share_lamports: onward,
            created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk_hex() -> String {
        "9".repeat(64)
    }

    #[test]
    fn plan_tip_small_genesis_creator() {
        let r = TipFlow::plan_tip(
            &pk_hex(),
            &"genesis-content".to_string(),
            500,
            None,
            1234,
            "cs_test_x".to_string(),
        )
        .unwrap();
        assert_eq!(r.gross_lamports, 500);
        assert_eq!(r.onward_gift_share_lamports, 0);
        assert!(r.net_lamports_to_creator < r.gross_lamports);
    }

    #[test]
    fn plan_tip_with_royalty_pledge_15pct() {
        let pledge = RoyaltyShareGift::pledged(15).unwrap();
        let r = TipFlow::plan_tip(
            &pk_hex(),
            &"remix-of-x".to_string(),
            10_000,
            Some(&pledge),
            5678,
            "cs_test_y".to_string(),
        )
        .unwrap();
        // net = 10000 - (290 + 30) = 9680 ; 15% of 9680 = 1452.
        assert!(r.onward_gift_share_lamports > 0);
        assert!(r.onward_gift_share_lamports < r.net_lamports_to_creator);
    }

    #[test]
    fn amount_below_min_rejected() {
        let r = TipFlow::plan_tip(
            &pk_hex(),
            &"c".to_string(),
            10,
            None,
            0,
            "cs_test_z".to_string(),
        );
        matches!(r.unwrap_err(), TipFlowError::AmountTooSmall { .. });
    }

    #[test]
    fn non_revocable_pledge_rejected() {
        let mut pledge = RoyaltyShareGift::pledged(20).unwrap();
        pledge.sovereign_revocable = false; // simulate tampering
        let r = TipFlow::plan_tip(
            &pk_hex(),
            &"c".to_string(),
            500,
            Some(&pledge),
            0,
            "cs_test_w".to_string(),
        );
        matches!(r.unwrap_err(), TipFlowError::NonRevocablePledge);
    }

    #[test]
    fn fee_estimate_29pct_plus_30() {
        // 1000 lamports → 29 + 30 = 59
        assert_eq!(stripe_fee_estimate(1000), 59);
        // 100_000 → 2900 + 30 = 2930
        assert_eq!(stripe_fee_estimate(100_000), 2930);
        // 50 minimum → 2 (ceiling of 1.45) + 30 = 32
        assert_eq!(stripe_fee_estimate(50), 32);
    }
}
