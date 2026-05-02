//! § scoring — the four-axis playtest score with weighted aggregate.
//!
//! § AXES (each in `[0, 100]`)
//!   - Fun     = `40%` (intent-diversity · novelty · pacing)
//!   - Balance = `30%` (encounter-difficulty curve · resource-availability)
//!   - Safety  = `20%` (sovereign-violations · PRIME-DIRECTIVE breaches)
//!   - Polish  = `10%` (no-crashes · no-softlocks · clean-determinism)
//!
//! § THRESHOLD-BARS
//!   `min_total = 60` — needed to be Published.
//!   `min_safety = 95` — REQUIRED ; no-tolerance for sovereignty-violations.
//!
//! § INPUTS
//!   The score-impls take a `&Trace` (counts derived inside) plus a few
//!   tunables (e.g. softlock-window N for Polish). Caller can swap in
//!   custom thresholds by constructing `Thresholds::custom`.

use serde::{Deserialize, Serialize};

use crate::session::Trace;

/// § Per-axis weight constants — sum to 100. Public so the host's own
/// dashboards can label rows consistently with this crate's emission.
pub const WEIGHT_FUN: u32 = 40;
/// § See [`WEIGHT_FUN`].
pub const WEIGHT_BALANCE: u32 = 30;
/// § See [`WEIGHT_FUN`].
pub const WEIGHT_SAFETY: u32 = 20;
/// § See [`WEIGHT_FUN`].
pub const WEIGHT_POLISH: u32 = 10;

const _: () = assert!(WEIGHT_FUN + WEIGHT_BALANCE + WEIGHT_SAFETY + WEIGHT_POLISH == 100);

/// § Default `min_total = 60` — content below this aggregate is held back
/// from publish until revised.
pub const DEFAULT_MIN_TOTAL: u8 = 60;
/// § Default `min_safety = 95` — REQUIRED ; lower means hard-rejection
/// regardless of how high the other axes scored. No-tolerance gate.
pub const DEFAULT_MIN_SAFETY: u8 = 95;

/// § A 0..=100 score on a single axis. Stored as `u8` so the report
/// remains compact + bit-pack-friendly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score(pub u8);

impl Score {
    /// § Construct + clamp to `[0, 100]`. Out-of-range values saturate.
    #[must_use]
    pub fn clamped(v: u32) -> Self {
        Self(v.min(100) as u8)
    }
}

/// § Marker-type alias for the Fun axis.
pub type FunScore = Score;
/// § Marker-type alias for the Balance axis.
pub type BalanceScore = Score;
/// § Marker-type alias for the Safety axis.
pub type SafetyScore = Score;
/// § Marker-type alias for the Polish axis.
pub type PolishScore = Score;

/// § Threshold-bars for a publish-decision. Defaults match the spec
/// (`min_total = 60` · `min_safety = 95`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Aggregate-floor for publish-eligibility.
    pub min_total: u8,
    /// Hard-floor on Safety ; below this is auto-rejected.
    pub min_safety: u8,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_total: DEFAULT_MIN_TOTAL,
            min_safety: DEFAULT_MIN_SAFETY,
        }
    }
}

impl Thresholds {
    /// § Construct custom thresholds (caller supplies their own bars).
    #[must_use]
    pub fn custom(min_total: u8, min_safety: u8) -> Self {
        Self { min_total, min_safety }
    }
}

/// § Compute the Fun-axis score from the trace.
///
/// § FORMULA
///   `fun = round( 100 * unique_intents / max(total_intents, 1) )`
///   capped at 100. A trace with high repetition (`unique << total`)
///   scores low ; a trace that exercises every intent at least once
///   scores 100.
#[must_use]
pub fn fun_score(t: &Trace) -> FunScore {
    let total = t.total_intents().max(1);
    let unique = t.unique_intents();
    Score::clamped((u64::from(unique) * 100 / u64::from(total)) as u32)
}

/// § Compute the Balance-axis score from the trace.
///
/// § FORMULA (proxy for ability-tier × encounter-difficulty curve)
///   `balance = round( 100 * progress / max(turns_elapsed, 1) )`
///   capped at 100. A trace that progresses every turn scores 100 ;
///   a trace that grinds without progress scores 0. Approximates
///   resource-availability — the more progress per turn, the more
///   forgiving the difficulty curve.
#[must_use]
pub fn balance_score(t: &Trace) -> BalanceScore {
    let turns = t.turns_elapsed().max(1);
    let progress = t.total_progress();
    Score::clamped((u64::from(progress) * 100 / u64::from(turns)) as u32)
}

/// § Compute the Safety-axis score from the trace.
///
/// § FORMULA
///   Start at 100. Subtract 50 per sovereign-violation. Subtract 25 per
///   cosmetic-axiom violation. Floor at 0. Result is the cap-respecting
///   score : a single sovereign-violation drops to 50 (well below
///   `min_safety = 95`) so the no-tolerance gate fires.
#[must_use]
pub fn safety_score(t: &Trace) -> SafetyScore {
    let sov = t.sovereign_violation_count();
    let cos = t.cosmetic_axiom_violation_count();
    let mut score: i64 = 100;
    score -= i64::from(sov) * 50;
    score -= i64::from(cos) * 25;
    Score::clamped(score.max(0) as u32)
}

/// § Compute the Polish-axis score from the trace + the determinism flag.
///
/// § FORMULA
///   Start at 100. Subtract 30 per crash. Subtract 20 per soft-lock.
///   Subtract 25 if `determinism_ok == false`. Floor at 0.
///
///   A clean run with no crashes / no soft-locks / clean determinism
///   scores 100.
#[must_use]
pub fn polish_score(t: &Trace, determinism_ok: bool) -> PolishScore {
    let crashes = t.crash_count();
    let softlocks = t.softlock_count();
    let mut score: i64 = 100;
    score -= i64::from(crashes) * 30;
    score -= i64::from(softlocks) * 20;
    if !determinism_ok {
        score -= 25;
    }
    Score::clamped(score.max(0) as u32)
}

/// § Compute the weighted-aggregate total in `[0, 100]`. Rounds half-up
/// implicitly via integer division ; the rounding bias is ≤ 0.5 points
/// which is negligible at the threshold-gate granularity.
#[must_use]
pub fn weighted_total(
    fun: FunScore,
    balance: BalanceScore,
    safety: SafetyScore,
    polish: PolishScore,
) -> u8 {
    let total = u32::from(fun.0) * WEIGHT_FUN
        + u32::from(balance.0) * WEIGHT_BALANCE
        + u32::from(safety.0) * WEIGHT_SAFETY
        + u32::from(polish.0) * WEIGHT_POLISH;
    (total / 100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::TraceEvent;

    fn intent(turn: u32, name: &str) -> TraceEvent {
        TraceEvent::IntentInvoked {
            turn,
            intent: name.into(),
        }
    }

    #[test]
    fn weights_sum_to_100() {
        assert_eq!(WEIGHT_FUN + WEIGHT_BALANCE + WEIGHT_SAFETY + WEIGHT_POLISH, 100);
    }

    #[test]
    fn fun_score_high_diversity_is_100() {
        let mut t = Trace::new();
        for i in 0..10 {
            t.push(intent(i, &format!("a{i}")));
        }
        assert_eq!(fun_score(&t).0, 100);
    }

    #[test]
    fn fun_score_repetition_is_low() {
        let mut t = Trace::new();
        for i in 0..10 {
            t.push(intent(i, "talk"));
        }
        assert_eq!(fun_score(&t).0, 10); // 1 unique / 10 total
    }

    #[test]
    fn safety_score_single_sovereign_violation_below_95() {
        let mut t = Trace::new();
        t.push(TraceEvent::SovereignViolation {
            turn: 0,
            rule: "surveillance".into(),
        });
        let s = safety_score(&t);
        assert_eq!(s.0, 50);
        assert!(s.0 < DEFAULT_MIN_SAFETY);
    }

    #[test]
    fn polish_score_clean_run_is_100() {
        let t = Trace::new();
        assert_eq!(polish_score(&t, true).0, 100);
    }

    #[test]
    fn polish_score_one_crash_drops_30() {
        let mut t = Trace::new();
        t.push(TraceEvent::CrashRecorded {
            turn: 0,
            kind: "panic".into(),
        });
        assert_eq!(polish_score(&t, true).0, 70);
    }

    #[test]
    fn polish_score_determinism_failure_drops_25() {
        let t = Trace::new();
        assert_eq!(polish_score(&t, false).0, 75);
    }

    #[test]
    fn weighted_total_arithmetic_known_vector() {
        let fun = Score(80);
        let balance = Score(70);
        let safety = Score(100);
        let polish = Score(60);
        // 80*40 + 70*30 + 100*20 + 60*10 = 3200 + 2100 + 2000 + 600 = 7900
        // 7900 / 100 = 79
        assert_eq!(weighted_total(fun, balance, safety, polish), 79);
    }

    #[test]
    fn thresholds_default_matches_spec() {
        let th = Thresholds::default();
        assert_eq!(th.min_total, 60);
        assert_eq!(th.min_safety, 95);
    }
}
