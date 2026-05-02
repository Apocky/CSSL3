//! § suggestions — derive author-actionable CSLv3-glyph suggestions
//! from the trace + scores. Output is `Vec<Suggestion>` — one bullet per
//! finding, in CSLv3-glyph notation that the author's UI can render
//! directly without further translation.
//!
//! § FORMAT (CSLv3-native ; per Apocky global preferences)
//!   `§ <axis> <evidence-glyph> <metric=value> → <directive>`
//!
//!   - axis      : FUN · BALANCE · SAFETY · POLISH
//!   - evidence  : ✓ ◐ ○ ✗ ⊘
//!   - directive : imperative recommendation (CSLv3-shorthand OK)

use crate::report::Suggestion;
use crate::scoring::{
    BalanceScore, FunScore, PolishScore, SafetyScore, DEFAULT_MIN_SAFETY, DEFAULT_MIN_TOTAL,
};
use crate::session::Trace;

/// § Build the suggestion-list. The driver hands us the trace + the four
/// per-axis scores ; we walk each axis and emit one bullet when the
/// score is below a per-axis threshold.
#[must_use]
pub fn build_suggestions(
    trace: &Trace,
    fun: FunScore,
    balance: BalanceScore,
    safety: SafetyScore,
    polish: PolishScore,
) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();

    // -- Fun --------------------------------------------------------------
    if fun.0 < 70 {
        out.push(Suggestion::new(format!(
            "§ FUN ◐ unique={}/total={} → +intent-paths · +NPC-branches · vary recipe-pool",
            trace.unique_intents(),
            trace.total_intents()
        )));
    }

    // -- Balance ----------------------------------------------------------
    if balance.0 < 60 {
        out.push(Suggestion::new(format!(
            "§ BALANCE ○ progress={}/turns={} → +early-rewards · ease difficulty-curve",
            trace.total_progress(),
            trace.turns_elapsed()
        )));
    }

    // -- Safety -----------------------------------------------------------
    let sov = trace.sovereign_violation_count();
    let cos = trace.cosmetic_axiom_violation_count();
    if safety.0 < DEFAULT_MIN_SAFETY {
        out.push(Suggestion::new(format!(
            "§ SAFETY ✗ sovereign-violations={sov} · cosmetic-axiom-violations={cos} \
             → audit cap-grants ; remove pay-for-power paths ; ¬-tolerance-bar"
        )));
    } else if cos > 0 {
        // safety might still pass if a cosmetic flag is borderline ;
        // surface it anyway so the author can resolve.
        out.push(Suggestion::new(format!(
            "§ SAFETY ◐ cosmetic-axiom-violations={cos} → audit shop-paths"
        )));
    }

    // -- Polish -----------------------------------------------------------
    if polish.0 < 80 {
        let crashes = trace.crash_count();
        let softlocks = trace.softlock_count();
        out.push(Suggestion::new(format!(
            "§ POLISH ○ crashes={crashes} · softlocks={softlocks} → fix runtime-stability ; \
             ensure deterministic-seed-feed everywhere"
        )));
    }

    // -- Aggregate cheer if all-clear --------------------------------------
    if out.is_empty() {
        out.push(Suggestion::new(format!(
            "§ ALL ✓ all-axes-meet-bar (min_total={DEFAULT_MIN_TOTAL} · \
             min_safety={DEFAULT_MIN_SAFETY}) → publish-ready"
        )));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::Score;
    use crate::session::TraceEvent;

    #[test]
    fn all_clear_emits_publish_ready() {
        let t = Trace::new();
        let s = build_suggestions(&t, Score(100), Score(100), Score(100), Score(100));
        assert_eq!(s.len(), 1);
        assert!(s[0].text.contains("publish-ready"));
    }

    #[test]
    fn safety_violation_emits_safety_directive() {
        let mut t = Trace::new();
        t.push(TraceEvent::SovereignViolation {
            turn: 0,
            rule: "surveillance".into(),
        });
        let s = build_suggestions(&t, Score(100), Score(100), Score(50), Score(100));
        assert!(s.iter().any(|x| x.text.starts_with("§ SAFETY ✗")));
    }

    #[test]
    fn low_fun_emits_fun_directive() {
        let mut t = Trace::new();
        t.push(TraceEvent::IntentInvoked {
            turn: 0,
            intent: "talk".into(),
        });
        t.push(TraceEvent::IntentInvoked {
            turn: 1,
            intent: "talk".into(),
        });
        let s = build_suggestions(&t, Score(40), Score(100), Score(100), Score(100));
        assert!(s.iter().any(|x| x.text.starts_with("§ FUN ◐")));
    }
}
