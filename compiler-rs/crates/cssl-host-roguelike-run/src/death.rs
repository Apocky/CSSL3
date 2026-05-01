// § death-penalty ← GDDs/ROGUELIKE_LOOP.csl §DEATH-PENALTY
// ════════════════════════════════════════════════════════════════════
// § I> soft-perma DEFAULT · keep 50%-Echoes-scaled-by-floor + class-XP-capped
// § I> hard-perma OPT-IN · seasonal · ConsentToken<"hard-perma"> required
// § I> Sanctum-of-Returns 1×/day mercy-grant ; ¬ paywalled ; ¬ jump-scare
// ════════════════════════════════════════════════════════════════════

use crate::run_state::{RunPhase, RunState};
use serde::{Deserialize, Serialize};

/// Cap on Echoes carried over per soft-perma death. GDD-spec intentionally
/// permissive (¬ punishing) ; cap exists to defuse pathological grind-spirals.
const ECHOES_CAP: u64 = 100_000;

/// § Carryover bundle — what survives soft-perma death.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoftPermaCarryover {
    /// Echoes-banked-back to MetaProgress (50% × floor-multiplier of in-run).
    pub echoes_carried: u64,
    /// Per-class XP carryover entries (capped at 1_000_000 per class).
    pub class_xp_carried: Vec<(u32, u64)>,
    /// Did this death consume the Sanctum-of-Returns daily-mercy ?
    pub mercy_used: bool,
}

/// § Outcome of applying death-penalty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathOutcome {
    /// New RunState with phase = Death.
    pub state: RunState,
    /// Carryover bundle (empty under hard-perma except for cosmetic-only).
    pub carryover: SoftPermaCarryover,
    /// Was this a hard-perma death ? (locks-out resume).
    pub hard_perma: bool,
}

/// § Apply death-penalty to the run-state.
///
/// Soft-mode (default) : keep 50%-Echoes scaled by floor-reached + class-XP capped.
/// Hard-mode : lose all run-progress ; cosmetic-only-reward retained at caller.
///
/// Mercy-grant : if `mercy_available` is true and mode is soft, the caller
/// MAY revive at the start of the current floor — this function only
/// records that the mercy was offered ; revival is a separate state-transition
/// owned by the run-driver.
pub fn apply_death_penalty(
    state: &RunState,
    hard_mode: bool,
    mercy_available: bool,
) -> DeathOutcome {
    if hard_mode {
        // Hard-perma : zero carryover ; new phase = Death.
        let mut next = state.clone();
        next.phase = RunPhase::Death;
        next.echoes_in_run = 0;
        return DeathOutcome {
            state: next,
            carryover: SoftPermaCarryover {
                echoes_carried: 0,
                class_xp_carried: Vec::new(),
                mercy_used: false,
            },
            hard_perma: true,
        };
    }

    // Soft-perma : 50% × floor-reached / target-floor-count multiplier of in-run-Echoes.
    // Floor-reached pulled from `state.depth` ; cap at 1.0 multiplier when at-floor-count.
    let floor_reached = u64::from(state.depth);
    let target = u64::from(state.floor_count.max(1));
    let half = state.echoes_in_run / 2;
    // Scale-by-floor : `floor_reached / target`, in 1/256 fixed-point to avoid f64.
    let scale_num = (floor_reached.min(target) * 256) / target;
    let echoes_carried = ((half.saturating_mul(scale_num)) / 256).min(ECHOES_CAP);

    let mut next = state.clone();
    next.phase = RunPhase::Death;
    let prev_in_run = next.echoes_in_run;
    next.echoes_pre = next.echoes_pre.saturating_add(echoes_carried);
    next.echoes_in_run = 0;
    let _ = prev_in_run; // bookkeeping-anchor for future audit-emit

    DeathOutcome {
        state: next,
        carryover: SoftPermaCarryover {
            echoes_carried,
            class_xp_carried: Vec::new(), // populated by caller from MetaProgress
            mercy_used: mercy_available,
        },
        hard_perma: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_state::RunState;

    #[test]
    fn soft_perma_keeps_half_echoes_at_full_depth() {
        let mut s = RunState::genesis(0xABCD, 0xCAFE_F00D);
        s.echoes_in_run = 1000;
        s.floor_count = 5;
        s.depth = 5;
        let out = apply_death_penalty(&s, false, false);
        // 1000 / 2 = 500 ; full-depth multiplier = 1.0 → 500 carried.
        assert_eq!(out.carryover.echoes_carried, 500);
        assert!(!out.hard_perma);
    }

    #[test]
    fn hard_perma_zeros_carryover() {
        let mut s = RunState::genesis(0x1234, 0x5678);
        s.echoes_in_run = 9999;
        let out = apply_death_penalty(&s, true, false);
        assert_eq!(out.carryover.echoes_carried, 0);
        assert!(out.hard_perma);
    }
}
