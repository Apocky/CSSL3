//! § report — `PlayTestReport` + publish-verdict + author-actionable
//! Suggestion strings (CSLv3-glyph formatted).
//!
//! § ROLE
//!   The report is the only value (alongside the Σ-anchor) that escapes
//!   the sandbox. It carries the four scores + the threshold-decision +
//!   the cosmetic-axiom attestation + a short list of CSLv3-glyph-styled
//!   suggestions the author can act on.
//!
//! § THRESHOLD-DECISION
//!   `is_publishable = (total ≥ thresholds.min_total) ∧ (safety ≥ min_safety)`.
//!   Both bars MUST hold ; safety acts as a no-tolerance veto.

use serde::{Deserialize, Serialize};

use crate::scoring::{
    weighted_total, BalanceScore, FunScore, PolishScore, SafetyScore, Thresholds,
};
use crate::PROTOCOL_VERSION;

/// § A single author-actionable suggestion in CSLv3-glyph notation.
///
/// § Examples
///   - `"§ FUN ◐ unique-intents=2/15 → add-NPC-dialog-paths"`
///   - `"§ SAFETY ✗ sovereign-violation @turn=12 rule=surveillance"`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Free-form CSLv3-glyph-formatted text (UTF-8).
    pub text: String,
}

impl Suggestion {
    /// § Convenience constructor.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// § The publish-verdict — is this content cleared to be Published?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportPublishVerdict {
    /// Both bars met (`total ≥ min_total` ∧ `safety ≥ min_safety`).
    Publishable,
    /// Aggregate met but Safety < `min_safety`. Hard-rejection.
    SafetyRejected,
    /// Safety OK but aggregate < `min_total`. Author can revise.
    BelowAggregateBar,
    /// Both bars failed.
    BothBarsFailed,
}

/// § Final report emitted at session-end. `serde`-stable so the host can
/// persist + diff across re-tests after author-revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayTestReport {
    /// Echoed from session config.
    pub content_id: u32,
    /// Echoed from session config — needed to verify deterministic re-test.
    pub agent_persona_seed: u64,
    /// Wire-format protocol-version (matches [`PROTOCOL_VERSION`]).
    pub protocol_version: u32,

    // -- raw counters -----------------------------------------------------
    /// Total crashes recorded in trace.
    pub crashes: u32,
    /// Total soft-locks detected in trace.
    pub softlocks: u32,
    /// Determinism check passed (replay-equal trace).
    pub determinism: bool,
    /// True iff zero pay-for-power paths were reachable (cosmetic-axiom).
    pub cosmetic_attest: bool,

    // -- scores -----------------------------------------------------------
    /// Fun score (`0..=100`).
    pub fun: FunScore,
    /// Balance score (`0..=100`).
    pub balance: BalanceScore,
    /// Safety score (`0..=100`).
    pub safety: SafetyScore,
    /// Polish score (`0..=100`).
    pub polish: PolishScore,
    /// Weighted-aggregate (`0..=100`).
    pub total: u8,

    // -- decision ---------------------------------------------------------
    /// Threshold-bars used.
    pub thresholds: Thresholds,
    /// Final publish-verdict.
    pub verdict: ReportPublishVerdict,

    // -- author-feedback --------------------------------------------------
    /// Author-actionable suggestions in CSLv3-glyph format.
    pub suggestions: Vec<Suggestion>,
}

impl PlayTestReport {
    /// § Helper — true iff the verdict is `Publishable`.
    #[must_use]
    pub const fn is_publishable(&self) -> bool {
        matches!(self.verdict, ReportPublishVerdict::Publishable)
    }

    /// § Helper — true iff Safety was the blocking-axis. Used by the
    /// host's sovereignty-dashboard to highlight no-tolerance failures.
    #[must_use]
    pub const fn safety_blocked(&self) -> bool {
        matches!(
            self.verdict,
            ReportPublishVerdict::SafetyRejected | ReportPublishVerdict::BothBarsFailed
        )
    }

    /// § Compute the verdict from the four scores + thresholds. The
    /// driver calls this when assembling the report ; callers can also
    /// re-derive it from raw scores in tests.
    #[must_use]
    pub fn verdict_from(
        fun: FunScore,
        balance: BalanceScore,
        safety: SafetyScore,
        polish: PolishScore,
        thresholds: Thresholds,
    ) -> ReportPublishVerdict {
        let total = weighted_total(fun, balance, safety, polish);
        let safety_ok = safety.0 >= thresholds.min_safety;
        let total_ok = total >= thresholds.min_total;
        match (total_ok, safety_ok) {
            (true, true) => ReportPublishVerdict::Publishable,
            (true, false) => ReportPublishVerdict::SafetyRejected,
            (false, true) => ReportPublishVerdict::BelowAggregateBar,
            (false, false) => ReportPublishVerdict::BothBarsFailed,
        }
    }

    /// § Construct from inputs — the canonical assembly path used by
    /// [`crate::driver::drive_session`]. Suggestions are caller-supplied
    /// so they can be derived from the trace's content-specific signals.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        content_id: u32,
        agent_persona_seed: u64,
        crashes: u32,
        softlocks: u32,
        determinism: bool,
        cosmetic_attest: bool,
        fun: FunScore,
        balance: BalanceScore,
        safety: SafetyScore,
        polish: PolishScore,
        thresholds: Thresholds,
        suggestions: Vec<Suggestion>,
    ) -> Self {
        let total = weighted_total(fun, balance, safety, polish);
        let verdict = Self::verdict_from(fun, balance, safety, polish, thresholds);
        Self {
            content_id,
            agent_persona_seed,
            protocol_version: PROTOCOL_VERSION,
            crashes,
            softlocks,
            determinism,
            cosmetic_attest,
            fun,
            balance,
            safety,
            polish,
            total,
            thresholds,
            verdict,
            suggestions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::Score;

    #[test]
    fn verdict_publishable_when_both_bars_met() {
        let v = PlayTestReport::verdict_from(
            Score(90),
            Score(80),
            Score(100),
            Score(80),
            Thresholds::default(),
        );
        assert_eq!(v, ReportPublishVerdict::Publishable);
    }

    #[test]
    fn verdict_safety_rejected_below_95() {
        // Aggregate ≥ 60 but safety = 50 → SafetyRejected
        // 90*0.4 + 90*0.3 + 50*0.2 + 90*0.1 = 36 + 27 + 10 + 9 = 82
        let v = PlayTestReport::verdict_from(
            Score(90),
            Score(90),
            Score(50),
            Score(90),
            Thresholds::default(),
        );
        assert_eq!(v, ReportPublishVerdict::SafetyRejected);
    }

    #[test]
    fn verdict_below_aggregate() {
        // Total ≈ 30 ; safety high → BelowAggregateBar
        let v = PlayTestReport::verdict_from(
            Score(20),
            Score(30),
            Score(100),
            Score(40),
            Thresholds::default(),
        );
        assert_eq!(v, ReportPublishVerdict::BelowAggregateBar);
    }

    #[test]
    fn assemble_round_trips_through_serde() {
        let r = PlayTestReport::assemble(
            42,
            0xDEAD_BEEF,
            0,
            0,
            true,
            true,
            Score(80),
            Score(70),
            Score(100),
            Score(60),
            Thresholds::default(),
            vec![Suggestion::new("§ all-clear")],
        );
        let json = serde_json::to_string(&r).unwrap();
        let r2: PlayTestReport = serde_json::from_str(&json).unwrap();
        assert_eq!(r, r2);
        assert!(r.is_publishable());
    }
}
