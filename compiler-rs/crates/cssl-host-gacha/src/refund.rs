// § refund.rs — Sovereign 7-day-window full-refund · automated-via-API
// ════════════════════════════════════════════════════════════════════
// § PRIME-DIRECTIVE : sovereign-revocable. The player can revoke any pull
//   within 7 days · NO QUESTIONS ASKED · automated · the cosmetic is
//   removed from inventory · the pull is marked `refunded_at` · pity-counter
//   rolls back by the number of refunded-pulls · a Σ-Chain refund-event is
//   anchored for-attribution-immutable-history.
//
// § AUTHORITATIVE-WINDOW : `pull.ts` (epoch-secs) + REFUND_WINDOW_SECS ≤ now
//   ⇒ refundable. After the window, automated-API rejects ; player can still
//   contact support for case-by-case (out-of-scope for this crate).
//
// § COSMETIC-REMOVAL : refunding a pull cancels the cosmetic-handle. The
//   inventory crate (cssl-host-loot or sibling) consumes RefundOutcome.removed
//   to apply the reversal.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::banner::Rarity;
use crate::pity::PityCounter;

/// § REFUND_WINDOW_SECS — 7 days in seconds. Public constant.
pub const REFUND_WINDOW_SECS: u64 = 7 * 24 * 60 * 60; // 604_800

/// § RefundWindow — predicate over (pull-time, now). Returns true if
/// the pull is within the 7-day refund window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundWindow {
    pub pull_ts_epoch_secs: u64,
    pub now_epoch_secs: u64,
}

impl RefundWindow {
    /// Predicate : is the pull within the refund window?
    #[must_use]
    pub fn is_refundable(&self) -> bool {
        // Reject if now < pull_ts (clock-skew) — caller must supply
        // monotonically-non-decreasing now_epoch_secs.
        if self.now_epoch_secs < self.pull_ts_epoch_secs {
            return false;
        }
        let elapsed = self.now_epoch_secs - self.pull_ts_epoch_secs;
        elapsed <= REFUND_WINDOW_SECS
    }

    /// Seconds remaining in the refund window (saturating to 0 when expired).
    #[must_use]
    pub fn seconds_remaining(&self) -> u64 {
        if self.now_epoch_secs < self.pull_ts_epoch_secs {
            // Future-dated pull (clock-skew) — return the full window.
            return REFUND_WINDOW_SECS;
        }
        let elapsed = self.now_epoch_secs - self.pull_ts_epoch_secs;
        REFUND_WINDOW_SECS.saturating_sub(elapsed)
    }
}

/// § RefundRequest — caller-input · pubkey-tied · pull-id-scoped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundRequest {
    pub player_pubkey: Vec<u8>,
    pub pull_id: String,
    pub banner_id: String,
    /// Original cosmetic-handle to revoke from inventory.
    pub cosmetic_handle: String,
    pub rarity_at_pull: Rarity,
    pub pull_ts_epoch_secs: u64,
    pub now_epoch_secs: u64,
    /// Pity counter as it stood AFTER the pull was applied. The refund
    /// rolls it BACK by 1 (this refunds 1 pull). For multi-pull bundle
    /// refunds the API caller can issue N RefundRequests.
    pub pity_after_pull: PityCounter,
}

/// § RefundOutcome — full state-transition. Used by the SQL-side caller
/// to (1) update gacha_pulls.refunded_at (2) insert into gacha_refunds
/// (3) revoke cosmetic from inventory (4) anchor a Σ-Chain refund-event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundOutcome {
    pub pull_id: String,
    pub banner_id: String,
    pub refund_ts_epoch_secs: u64,
    pub refunded: bool,
    /// Updated pity counter (rolled back by 1). When the original pull
    /// produced a Mythic, the rollback restores the pity-counter to its
    /// pre-mythic value (approximation : we cannot reconstruct the exact
    /// pre-mythic value from a single refund, so we tick-up by 1 instead
    /// of full-reset · documented in the spec).
    pub pity_after_refund: PityCounter,
    /// Cosmetic-handle to remove from inventory. The inventory crate
    /// consumes this and emits its own audit-event.
    pub removed_cosmetic_handle: String,
    /// Was the original-pull a Mythic? Affects pity-rollback semantics.
    pub original_was_mythic: bool,
}

/// § RefundErr — public error-enum.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RefundErr {
    #[error("refund window expired (>7 days since pull)")]
    WindowExpired,
    #[error("empty pubkey")]
    EmptyPubkey,
    #[error("empty pull_id")]
    EmptyPullId,
}

/// § run_refund — execute the 7-day-window check + state transition.
/// Returns RefundOutcome ; caller persists to SQL + emits Σ-Chain anchor.
pub fn run_refund(req: &RefundRequest) -> Result<RefundOutcome, RefundErr> {
    if req.player_pubkey.is_empty() {
        return Err(RefundErr::EmptyPubkey);
    }
    if req.pull_id.is_empty() {
        return Err(RefundErr::EmptyPullId);
    }

    let window = RefundWindow {
        pull_ts_epoch_secs: req.pull_ts_epoch_secs,
        now_epoch_secs: req.now_epoch_secs,
    };
    if !window.is_refundable() {
        return Err(RefundErr::WindowExpired);
    }

    let original_was_mythic = req.rarity_at_pull == Rarity::Mythic;

    // Pity rollback :
    //   non-Mythic pull → roll back 1 (pulls_since_mythic -= 1)
    //   Mythic pull     → tick UP 1 (we don't know the pre-mythic value · +1
    //                                approximation · documented in spec)
    let mut pity_after = req.pity_after_pull.clone();
    if original_was_mythic {
        pity_after.tick_non_mythic();
    } else {
        pity_after.rollback_pulls(1);
    }

    Ok(RefundOutcome {
        pull_id: req.pull_id.clone(),
        banner_id: req.banner_id.clone(),
        refund_ts_epoch_secs: req.now_epoch_secs,
        refunded: true,
        pity_after_refund: pity_after,
        removed_cosmetic_handle: req.cosmetic_handle.clone(),
        original_was_mythic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_refund_req(
        rarity: Rarity,
        pull_ts: u64,
        now: u64,
        pity_after_pull: PityCounter,
    ) -> RefundRequest {
        RefundRequest {
            player_pubkey: b"player-32-byte-pubkey-fixed-!!!!".to_vec(),
            pull_id: "pull-uuid-001".into(),
            banner_id: "banner-A".into(),
            cosmetic_handle: "cosmetic:rare:banner-A:042".into(),
            rarity_at_pull: rarity,
            pull_ts_epoch_secs: pull_ts,
            now_epoch_secs: now,
            pity_after_pull,
        }
    }

    #[test]
    fn refund_window_constant_is_7_days() {
        assert_eq!(REFUND_WINDOW_SECS, 604_800);
    }

    #[test]
    fn within_7_days_refundable() {
        let w = RefundWindow {
            pull_ts_epoch_secs: 1_700_000_000,
            now_epoch_secs: 1_700_000_000 + 6 * 24 * 60 * 60, // 6 days
        };
        assert!(w.is_refundable());
    }

    #[test]
    fn after_7_days_expired() {
        let w = RefundWindow {
            pull_ts_epoch_secs: 1_700_000_000,
            now_epoch_secs: 1_700_000_000 + 8 * 24 * 60 * 60, // 8 days
        };
        assert!(!w.is_refundable());
    }

    #[test]
    fn future_pull_ts_rejects() {
        let w = RefundWindow {
            pull_ts_epoch_secs: 1_700_000_000,
            now_epoch_secs: 1_699_999_999,
        };
        assert!(!w.is_refundable());
    }

    #[test]
    fn seconds_remaining_decreases_over_time() {
        let pull_ts = 1_700_000_000;
        let w_t0 = RefundWindow { pull_ts_epoch_secs: pull_ts, now_epoch_secs: pull_ts };
        let w_t1 = RefundWindow { pull_ts_epoch_secs: pull_ts, now_epoch_secs: pull_ts + 3600 };
        assert_eq!(w_t0.seconds_remaining(), REFUND_WINDOW_SECS);
        assert_eq!(w_t1.seconds_remaining(), REFUND_WINDOW_SECS - 3600);
    }

    #[test]
    fn refund_within_window_succeeds() {
        let mut pity = PityCounter::new();
        pity.pulls_since_mythic = 5;
        let req = fresh_refund_req(
            Rarity::Rare,
            1_700_000_000,
            1_700_000_000 + 24 * 60 * 60, // 1 day later
            pity,
        );
        let out = run_refund(&req).unwrap();
        assert!(out.refunded);
        assert_eq!(out.pity_after_refund.pulls_since_mythic, 4);
        assert!(!out.original_was_mythic);
        assert_eq!(out.removed_cosmetic_handle, "cosmetic:rare:banner-A:042");
    }

    #[test]
    fn refund_outside_window_rejects() {
        let req = fresh_refund_req(
            Rarity::Common,
            1_700_000_000,
            1_700_000_000 + 8 * 24 * 60 * 60, // 8 days
            PityCounter::new(),
        );
        let err = run_refund(&req).unwrap_err();
        assert!(matches!(err, RefundErr::WindowExpired));
    }

    #[test]
    fn refund_mythic_ticks_pity_up_by_1() {
        let mut pity = PityCounter::new();
        pity.pulls_since_mythic = 0; // freshly-reset by Mythic
        let req = fresh_refund_req(
            Rarity::Mythic,
            1_700_000_000,
            1_700_000_000 + 60,
            pity,
        );
        let out = run_refund(&req).unwrap();
        assert_eq!(out.pity_after_refund.pulls_since_mythic, 1);
        assert!(out.original_was_mythic);
    }

    #[test]
    fn refund_empty_pubkey_rejects() {
        let mut req = fresh_refund_req(
            Rarity::Common,
            1_700_000_000,
            1_700_000_000 + 60,
            PityCounter::new(),
        );
        req.player_pubkey.clear();
        assert!(matches!(run_refund(&req), Err(RefundErr::EmptyPubkey)));
    }

    #[test]
    fn refund_empty_pull_id_rejects() {
        let mut req = fresh_refund_req(
            Rarity::Common,
            1_700_000_000,
            1_700_000_000 + 60,
            PityCounter::new(),
        );
        req.pull_id.clear();
        assert!(matches!(run_refund(&req), Err(RefundErr::EmptyPullId)));
    }

    #[test]
    fn refund_at_exact_7_day_boundary_still_refundable() {
        let req = fresh_refund_req(
            Rarity::Common,
            1_700_000_000,
            1_700_000_000 + REFUND_WINDOW_SECS,
            PityCounter::new(),
        );
        assert!(run_refund(&req).is_ok());
    }
}
