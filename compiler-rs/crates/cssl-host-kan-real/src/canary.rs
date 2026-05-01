//! § canary — 10% session-id-hash gate per spec § A/B-PROTOCOL.
//!
//! `specs/grand-vision/11_KAN_RIDE.csl § A/B-TESTING PROTOCOL § CANARY` :
//!
//!     canary-pct  : 10% of-sessions ; opt-in-flag-respected
//!     selection   : session-id-hash mod 10 == 0 ; deterministic per-session
//!     duration    : ≥ 1k classify-calls per-SP ; W! before-graduate
//!
//! § DISAGREEMENT KIND
//!   The rollback trigger T-2 ("output-divergence(stage-0, stage-1) > 5%
//!   over canary-window") needs a structured way to characterize a
//!   disagreement. We classify into [`DisagreementKind`] :
//!
//!     - `Agree`       : stage-0 + stage-1 produced equivalent outputs.
//!     - `IntentKind`  : the `IntentClass.kind` field differs.
//!     - `Confidence`  : same kind ; |conf-delta| > THRESH_CONF.
//!     - `Args`        : same kind + conf ; args-list differs.
//!     - `Cardinality` : seed-cell counts differ.
//!     - `Cells`       : same count ; per-cell content differs.
//!     - `Score`       : cocreative score-delta > THRESH_SCORE.
//!
//! § DETERMINISM
//!   `enrolled(session_id)` is purely a function of the input ; FNV-1a
//!   keeps the gate stable across hosts.

use crate::audit::fnv1a_64;
use cssl_host_kan_substrate_bridge::{IntentClass, SeedCell};

/// § Default canary percentage (per spec § CANARY).
pub const DEFAULT_CANARY_PCT: u8 = 10;

/// § Confidence-delta threshold above which two same-kind outputs
///   register as a `DisagreementKind::Confidence`. Spec calls for
///   `<2%` as the graduate-criterion ; we use 5% as the trigger.
pub const THRESH_CONF: f32 = 0.05;

/// § Cocreative-score-delta threshold for `DisagreementKind::Score`.
pub const THRESH_SCORE: f32 = 0.05;

/// § Categorized disagreement between stage-0 + stage-1 outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisagreementKind {
    /// § Outputs are equivalent.
    Agree,
    /// § Intent.kind differs.
    IntentKind {
        stage0_kind: String,
        stage1_kind: String,
    },
    /// § Same kind ; |conf-delta| above threshold.
    Confidence {
        delta_e2: u32, // delta * 10000 ; integer for Eq.
    },
    /// § Same kind + conf ; args differ.
    Args,
    /// § Seed-cell counts differ.
    Cardinality { stage0: usize, stage1: usize },
    /// § Same count ; per-cell content differs.
    Cells,
    /// § Cocreative score-delta above threshold.
    Score {
        delta_e4: u32, // delta * 10000.
    },
}

/// § Canary-enrollment gate.
#[derive(Debug, Clone, Copy)]
pub struct CanaryGate {
    /// § Enrollment percentage in `[0, 100]`. Sessions with
    ///   `hash(session_id) mod 100 < enroll_pct` are enrolled.
    pub enroll_pct: u8,
    /// § Override : if true, every session is enrolled (force-on).
    pub force_on: bool,
    /// § Override : if true, every session is NOT enrolled (force-off).
    ///   Wins over `force_on` when both are set (defensive).
    pub force_off: bool,
}

impl Default for CanaryGate {
    fn default() -> Self {
        Self {
            enroll_pct: DEFAULT_CANARY_PCT,
            force_on: false,
            force_off: false,
        }
    }
}

impl CanaryGate {
    /// § Construct with a custom enrollment-pct (clamped to 0..=100).
    #[must_use]
    pub fn with_pct(pct: u8) -> Self {
        Self {
            enroll_pct: pct.min(100),
            force_on: false,
            force_off: false,
        }
    }

    /// § Construct a force-on gate (all sessions enrolled).
    #[must_use]
    pub fn force_on() -> Self {
        Self {
            enroll_pct: 100,
            force_on: true,
            force_off: false,
        }
    }

    /// § Construct a force-off gate (no sessions enrolled).
    #[must_use]
    pub fn force_off() -> Self {
        Self {
            enroll_pct: 0,
            force_on: false,
            force_off: true,
        }
    }

    /// § True iff the given session is enrolled in the canary. Pure
    ///   function of the session-id ; deterministic.
    #[must_use]
    pub fn enrolled(&self, session_id: &str) -> bool {
        if self.force_off {
            return false;
        }
        if self.force_on {
            return true;
        }
        let h = fnv1a_64(session_id.as_bytes());
        // Spec : `session-id-hash mod 10 == 0` for 10%. Generalize to
        // `hash mod 100 < enroll_pct` for arbitrary percentages.
        let bucket = (h % 100) as u8;
        bucket < self.enroll_pct
    }

    /// § Classify disagreement between two `IntentClass` outputs.
    #[must_use]
    pub fn intent_disagreement(stage0: &IntentClass, stage1: &IntentClass) -> DisagreementKind {
        if stage0.kind != stage1.kind {
            return DisagreementKind::IntentKind {
                stage0_kind: stage0.kind.clone(),
                stage1_kind: stage1.kind.clone(),
            };
        }
        let delta = (stage0.confidence - stage1.confidence).abs();
        if delta > THRESH_CONF {
            return DisagreementKind::Confidence {
                delta_e2: (delta * 10000.0).round() as u32,
            };
        }
        if stage0.args != stage1.args {
            return DisagreementKind::Args;
        }
        DisagreementKind::Agree
    }

    /// § Classify disagreement between two seed-cell vectors.
    #[must_use]
    pub fn seed_disagreement(stage0: &[SeedCell], stage1: &[SeedCell]) -> DisagreementKind {
        if stage0.len() != stage1.len() {
            return DisagreementKind::Cardinality {
                stage0: stage0.len(),
                stage1: stage1.len(),
            };
        }
        for (a, b) in stage0.iter().zip(stage1.iter()) {
            if a != b {
                return DisagreementKind::Cells;
            }
        }
        DisagreementKind::Agree
    }

    /// § Classify disagreement between two cocreative scores.
    #[must_use]
    pub fn score_disagreement(stage0: f32, stage1: f32) -> DisagreementKind {
        let s0 = if stage0.is_finite() { stage0 } else { 0.0 };
        let s1 = if stage1.is_finite() { stage1 } else { 0.0 };
        let delta = (s0 - s1).abs();
        if delta > THRESH_SCORE {
            DisagreementKind::Score {
                delta_e4: (delta * 10000.0).round() as u32,
            }
        } else {
            DisagreementKind::Agree
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_on_always_enrolled() {
        let g = CanaryGate::force_on();
        assert!(g.enrolled("session-A"));
        assert!(g.enrolled("session-B"));
    }

    #[test]
    fn force_off_never_enrolled() {
        let g = CanaryGate::force_off();
        assert!(!g.enrolled("session-A"));
        assert!(!g.enrolled("session-B"));
    }

    #[test]
    fn enrollment_deterministic() {
        let g = CanaryGate::with_pct(50);
        let a = g.enrolled("session-X");
        let b = g.enrolled("session-X");
        assert_eq!(a, b);
    }

    #[test]
    fn pct_zero_excludes_all() {
        let g = CanaryGate::with_pct(0);
        assert!(!g.enrolled("session-A"));
    }

    #[test]
    fn pct_one_hundred_includes_all() {
        let g = CanaryGate::with_pct(100);
        assert!(g.enrolled("session-A"));
        assert!(g.enrolled("session-B"));
    }
}
