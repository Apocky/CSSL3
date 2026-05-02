//! § refund — 14-day pro-rated refund window for Premium purchase.
//!
//! Sovereign-rule per `Labyrinth of Apocalypse/systems/battle_pass.csl
//! § sovereign-rules` :
//!
//! ```text
//!   - 14-day window starting at purchase-time.
//!   - Pro-rated : refund-amount = price × (1 - days_used / 14).
//!   - After-window : refund = 0 cents (¬ refundable).
//!   - Boundary day-14 : zero-cents refund (window closes inclusive).
//! ```
//!
//! Pro-ration is integer-division based on full-days-elapsed to keep the
//! math deterministic and bit-stable across hosts.

use serde::{Deserialize, Serialize};

use crate::rules::SeasonRules;
use crate::season::SeasonId;

const SEC_PER_DAY: i64 = 86_400;

/// A purchase-receipt for a Premium-track unlock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseReceipt {
    pub player_id_hash: u64,
    pub season_id: SeasonId,
    pub price_cents: u64,
    pub purchased_at_unix_s: i64,
    pub stripe_session_id: String,
}

impl PurchaseReceipt {
    /// `true` iff `now` is within the refund window (inclusive of day-0,
    /// exclusive of day-`REFUND_WINDOW_DAYS`).
    pub fn is_within_refund_window(&self, now_unix_s: i64) -> bool {
        let elapsed = now_unix_s.saturating_sub(self.purchased_at_unix_s);
        if elapsed < 0 {
            return false;
        }
        let days = elapsed / SEC_PER_DAY;
        (days as u32) < SeasonRules::REFUND_WINDOW_DAYS
    }
}

/// Refund decision returned by [`pro_rate_refund_cents`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundDecision {
    pub refund_cents: u64,
    pub days_elapsed: u32,
    pub days_remaining: u32,
    pub eligible: bool,
}

/// Compute the pro-rated refund. `now < purchase` returns the full price
/// (clock-skew kindness) ; `now >= purchase + 14d` returns 0-cents.
///
/// Formula : `refund = price * days_remaining / 14` (integer math · floor).
/// Day-0 → full-price. Day-7 → ~50%. Day-13 → ~7%. Day-14+ → 0.
pub fn pro_rate_refund_cents(receipt: &PurchaseReceipt, now_unix_s: i64) -> RefundDecision {
    let elapsed_secs = now_unix_s.saturating_sub(receipt.purchased_at_unix_s);
    if elapsed_secs < 0 {
        // Clock-skew kindness — treat as day-0.
        return RefundDecision {
            refund_cents: receipt.price_cents,
            days_elapsed: 0,
            days_remaining: SeasonRules::REFUND_WINDOW_DAYS,
            eligible: true,
        };
    }
    let days_elapsed = (elapsed_secs / SEC_PER_DAY) as u32;
    if days_elapsed >= SeasonRules::REFUND_WINDOW_DAYS {
        return RefundDecision {
            refund_cents: 0,
            days_elapsed,
            days_remaining: 0,
            eligible: false,
        };
    }
    let days_remaining = SeasonRules::REFUND_WINDOW_DAYS - days_elapsed;
    let refund = receipt
        .price_cents
        .saturating_mul(u64::from(days_remaining))
        / u64::from(SeasonRules::REFUND_WINDOW_DAYS);
    RefundDecision {
        refund_cents: refund,
        days_elapsed,
        days_remaining,
        eligible: refund > 0,
    }
}

/// Refund-flow errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefundErr {
    /// Receipt is outside the 14-day window.
    OutsideWindow { days_elapsed: u32 },
    /// Receipt was already fully refunded.
    AlreadyRefunded,
}

impl core::fmt::Display for RefundErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutsideWindow { days_elapsed } => write!(
                f,
                "refund window expired ({days_elapsed} days elapsed > {} day limit)",
                SeasonRules::REFUND_WINDOW_DAYS
            ),
            Self::AlreadyRefunded => write!(f, "receipt already refunded"),
        }
    }
}
impl std::error::Error for RefundErr {}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(price_cents: u64, purchased_at: i64) -> PurchaseReceipt {
        PurchaseReceipt {
            player_id_hash: 0xCAFE,
            season_id: SeasonId(1),
            price_cents,
            purchased_at_unix_s: purchased_at,
            stripe_session_id: "cs_test_xyz".to_string(),
        }
    }

    #[test]
    fn refund_full_at_day_0() {
        let r = rec(999, 1_700_000_000);
        let d = pro_rate_refund_cents(&r, 1_700_000_000);
        assert_eq!(d.refund_cents, 999);
        assert_eq!(d.days_elapsed, 0);
        assert_eq!(d.days_remaining, 14);
        assert!(d.eligible);
    }

    #[test]
    fn refund_zero_at_day_14() {
        let r = rec(999, 1_700_000_000);
        let day_14 = 1_700_000_000 + 14 * SEC_PER_DAY;
        let d = pro_rate_refund_cents(&r, day_14);
        assert_eq!(d.refund_cents, 0);
        assert_eq!(d.days_elapsed, 14);
        assert!(!d.eligible);
    }

    #[test]
    fn refund_zero_after_day_14() {
        let r = rec(999, 1_700_000_000);
        let day_30 = 1_700_000_000 + 30 * SEC_PER_DAY;
        let d = pro_rate_refund_cents(&r, day_30);
        assert_eq!(d.refund_cents, 0);
        assert!(!d.eligible);
    }

    #[test]
    fn refund_pro_rated_at_day_7() {
        let r = rec(1400, 1_700_000_000);
        let day_7 = 1_700_000_000 + 7 * SEC_PER_DAY;
        let d = pro_rate_refund_cents(&r, day_7);
        // 1400 * 7 / 14 = 700.
        assert_eq!(d.refund_cents, 700);
        assert_eq!(d.days_elapsed, 7);
        assert_eq!(d.days_remaining, 7);
    }

    #[test]
    fn refund_pro_rated_at_day_13() {
        let r = rec(1400, 1_700_000_000);
        let day_13 = 1_700_000_000 + 13 * SEC_PER_DAY;
        let d = pro_rate_refund_cents(&r, day_13);
        // 1400 * 1 / 14 = 100.
        assert_eq!(d.refund_cents, 100);
        assert_eq!(d.days_remaining, 1);
    }

    #[test]
    fn is_within_window_boundary() {
        let r = rec(999, 1_700_000_000);
        let day_13 = 1_700_000_000 + 13 * SEC_PER_DAY;
        let day_14 = 1_700_000_000 + 14 * SEC_PER_DAY;
        assert!(r.is_within_refund_window(day_13));
        assert!(!r.is_within_refund_window(day_14));
    }

    #[test]
    fn clock_skew_treats_negative_elapsed_as_full_refund() {
        let r = rec(999, 1_700_000_000);
        let d = pro_rate_refund_cents(&r, 1_699_000_000);
        assert_eq!(d.refund_cents, 999);
        assert!(d.eligible);
    }
}
