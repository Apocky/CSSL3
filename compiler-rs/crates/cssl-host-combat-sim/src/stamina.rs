// § stamina.rs — saturating-arithmetic stamina-economy (per GDD § STAMINA-ECONOMY)
// ════════════════════════════════════════════════════════════════════
// § I> capacity 0..200 (base 100 + 5/Endurance) ; clamp 0
// § I> drain : per-action enum lookup ; saturating-sub
// § I> regen : Idle 25/sec · Acting 0/sec · Blocking-Held 5/sec · Walking 18 · Running 0
// § I> starvation : stam<cost ⇒ try_consume returns false ; stam=0 ⇒ forced-Idle
// § I> NaN-safe : f32 clamp ∈ [0, capacity] ; underflow-clamp-to-zero (audit-emit)
// § I> ¬ panic ; ¬ unwrap ; saturating everywhere
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Action that drains stamina ; cost-table baked-in from GDD § DRAIN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StaminaAction {
    LightAttack,
    HeavyAttack,
    DodgeRoll,
    ParryAttempt,
    BlockHitLight,
    BlockHitHeavy,
    Sprint,
    SkillCastTier1,
    SkillCastTier2,
    SkillCastTier3,
}

impl StaminaAction {
    /// Stamina cost from GDD § DRAIN. Sprint cost is per-second ; caller
    /// scales by `dt` before invoking `try_consume`.
    #[must_use]
    pub const fn cost(self) -> f32 {
        match self {
            Self::LightAttack => 18.0,
            Self::HeavyAttack => 35.0,
            Self::DodgeRoll => 25.0,
            Self::ParryAttempt => 12.0,
            Self::BlockHitLight => 15.0,
            Self::BlockHitHeavy => 35.0,
            Self::Sprint => 8.0,
            Self::SkillCastTier1 => 25.0,
            Self::SkillCastTier2 => 50.0,
            Self::SkillCastTier3 => 80.0,
        }
    }
}

/// Regen-mode ; selects per-state regen rate per GDD § REGEN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum RegenMode {
    #[default]
    Idle,
    Acting,
    BlockingHeld,
    Walking,
    Running,
}

impl RegenMode {
    /// Regen-rate in stamina-units / sec from GDD § REGEN.
    #[must_use]
    pub const fn rate(self) -> f32 {
        match self {
            Self::Idle => 25.0,
            Self::Acting | Self::Running => 0.0,
            Self::BlockingHeld => 5.0,
            Self::Walking => 18.0,
        }
    }
}

/// Saturating stamina-pool ; values clamped ∈ [0, capacity].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StaminaPool {
    /// Maximum stamina (base 100 ; cap 200 per GDD).
    pub capacity: f32,
    /// Current stamina ∈ [0, capacity].
    pub current: f32,
    /// Active regen mode ; caller toggles.
    pub regen_mode: RegenMode,
    /// Post-action delay (350ms per GDD) — regen suppressed until 0.
    pub post_action_delay_secs: f32,
}

impl StaminaPool {
    /// Construct a new pool with `capacity = current` ; both clamped ≥ 0.
    #[must_use]
    pub fn new(capacity: f32) -> Self {
        let cap = capacity.max(0.0).min(200.0);
        Self {
            capacity: cap,
            current: cap,
            regen_mode: RegenMode::Idle,
            post_action_delay_secs: 0.0,
        }
    }

    /// Tick the pool forward by `dt` seconds. Saturating ; NaN-clamped.
    pub fn tick(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        // Decay post-action-delay first
        self.post_action_delay_secs = (self.post_action_delay_secs - dt).max(0.0);
        // Regen suppressed during delay window
        if self.post_action_delay_secs <= 0.0 {
            let gained = self.regen_mode.rate() * dt;
            self.current = (self.current + gained).min(self.capacity).max(0.0);
        }
    }

    /// Try to consume an action's cost. Returns true iff successful.
    /// On success : current saturates ≥ 0 ; post-action-delay set to 350ms.
    pub fn try_consume(&mut self, action: StaminaAction) -> bool {
        let cost = action.cost();
        if self.current < cost {
            return false;
        }
        self.current = (self.current - cost).max(0.0);
        self.post_action_delay_secs = 0.350;
        true
    }

    /// Drain a raw amount (for Sprint dt-scaled drain or block-hit drain).
    /// Saturating ; never panics ; underflow clamped to 0 with audit-flag.
    /// Returns true iff the full amount was consumed without underflow.
    pub fn drain_raw(&mut self, amount: f32) -> bool {
        let amt = if amount.is_finite() { amount.max(0.0) } else { 0.0 };
        if self.current < amt {
            self.current = 0.0;
            // caller can audit STAM_UNDERFLOW per GDD § FAILURE-MODES
            return false;
        }
        self.current = (self.current - amt).max(0.0);
        true
    }

    /// Returns true iff the pool is fully starved (forced-Idle per GDD).
    #[must_use]
    pub fn is_starved(&self) -> bool {
        self.current <= 0.0
    }

    /// Set regen mode (caller toggles based on FSM state).
    pub fn set_regen_mode(&mut self, mode: RegenMode) {
        self.regen_mode = mode;
    }
}

impl Default for StaminaPool {
    fn default() -> Self {
        Self::new(100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_consume_fails_when_below_cost() {
        let mut p = StaminaPool::new(10.0);
        assert!(!p.try_consume(StaminaAction::LightAttack)); // cost 18 > 10
        assert_eq!(p.current, 10.0);
    }

    #[test]
    fn tick_regen_clamps_to_capacity() {
        let mut p = StaminaPool::new(100.0);
        p.current = 90.0;
        p.set_regen_mode(RegenMode::Idle);
        p.tick(10.0); // 25/s × 10 = 250 ; clamps to 100
        assert!((p.current - 100.0).abs() < 1e-3);
    }

    #[test]
    fn drain_raw_underflow_clamps_zero() {
        let mut p = StaminaPool::new(50.0);
        let ok = p.drain_raw(100.0);
        assert!(!ok);
        assert_eq!(p.current, 0.0);
        assert!(p.is_starved());
    }
}
