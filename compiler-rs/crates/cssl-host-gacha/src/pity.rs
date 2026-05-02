// § pity.rs — Pity-system : guaranteed-Mythic within PITY_THRESHOLD pulls
// ════════════════════════════════════════════════════════════════════
// § PUBLIC-DISCLOSURE : PITY_THRESHOLD constant exposed in the lib root
//   AND in the `gacha_banners.pity_threshold` SQL column · transparency
//   demands the player can KNOW the threshold before opting-in.
// § ATTESTATION : the pity-counter resets on Mythic-roll · resets on
//   refund-revocation · NEVER silently-ratchets-up · NEVER hides state.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// § PITY_THRESHOLD — guaranteed-Mythic by this many pulls. Public-knowledge.
///
/// 90 pulls is the canonical reference. Changing this constant requires a
/// migration of all live banners (bump `gacha_banners.pity_threshold`).
pub const PITY_THRESHOLD: u32 = 90;

/// § PityCounter — per-player-per-banner pull-without-mythic counter.
///
/// On Mythic-drop the counter resets to 0. On 7d-refund the counter rolls
/// BACK by the number of refunded-pulls (sovereign-revocable semantics).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PityCounter {
    /// Pulls since last Mythic (or since banner-start). Range : 0 ..= PITY_THRESHOLD-1.
    pub pulls_since_mythic: u32,
    /// Hardcoded threshold copy — set when counter is created · attest-stable.
    pub threshold: u32,
}

impl PityCounter {
    /// Fresh counter at threshold = PITY_THRESHOLD.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pulls_since_mythic: 0,
            threshold: PITY_THRESHOLD,
        }
    }

    /// Construct with a non-default threshold (e.g. for replay-validation
    /// of an old banner that ran with a different threshold). The supplied
    /// threshold MUST match the banner's `pity_threshold` column.
    pub fn with_threshold(threshold: u32) -> Result<Self, PityErr> {
        if threshold == 0 {
            return Err(PityErr::ZeroThreshold);
        }
        Ok(Self {
            pulls_since_mythic: 0,
            threshold,
        })
    }

    /// Should the next pull be force-rolled-up to Mythic?
    /// Returns true when `pulls_since_mythic + 1 >= threshold`.
    #[must_use]
    pub fn should_force_mythic(&self) -> bool {
        self.pulls_since_mythic + 1 >= self.threshold
    }

    /// Tick the counter for one non-mythic pull.
    pub fn tick_non_mythic(&mut self) {
        // Saturating-add : if (somehow) we exceed threshold, clamp · the
        // next-pull-force-mythic predicate still fires.
        self.pulls_since_mythic = self.pulls_since_mythic.saturating_add(1);
    }

    /// Reset on Mythic-drop (whether organically-rolled or pity-forced).
    pub fn reset_on_mythic(&mut self) {
        self.pulls_since_mythic = 0;
    }

    /// Roll-back N pulls (sovereign-refund). Subtracts N · saturating-floor at 0.
    pub fn rollback_pulls(&mut self, n: u32) {
        self.pulls_since_mythic = self.pulls_since_mythic.saturating_sub(n);
    }
}

impl Default for PityCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// § PityErr — public error-enum.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PityErr {
    #[error("pity threshold must be > 0")]
    ZeroThreshold,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_counter_is_zero() {
        let c = PityCounter::new();
        assert_eq!(c.pulls_since_mythic, 0);
        assert_eq!(c.threshold, PITY_THRESHOLD);
        assert!(!c.should_force_mythic());
    }

    #[test]
    fn pity_threshold_is_publicly_90() {
        assert_eq!(PITY_THRESHOLD, 90);
    }

    #[test]
    fn force_mythic_triggers_on_threshold_pull() {
        let mut c = PityCounter::new();
        // Pull 1..=88 should NOT force.
        for _ in 0..89 {
            assert!(!c.should_force_mythic());
            c.tick_non_mythic();
        }
        // After 89 ticks the next-pull (the 90th) must force-mythic.
        assert!(c.should_force_mythic());
    }

    #[test]
    fn reset_on_mythic_clears() {
        let mut c = PityCounter::new();
        for _ in 0..50 {
            c.tick_non_mythic();
        }
        assert_eq!(c.pulls_since_mythic, 50);
        c.reset_on_mythic();
        assert_eq!(c.pulls_since_mythic, 0);
    }

    #[test]
    fn rollback_pulls_saturating_at_zero() {
        let mut c = PityCounter::new();
        for _ in 0..10 {
            c.tick_non_mythic();
        }
        c.rollback_pulls(15);
        assert_eq!(c.pulls_since_mythic, 0);
    }

    #[test]
    fn zero_threshold_rejected() {
        assert!(matches!(
            PityCounter::with_threshold(0),
            Err(PityErr::ZeroThreshold)
        ));
    }
}
