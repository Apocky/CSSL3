//! § evidence_driver — adaptive sampling-budget via evidence-glyph.
//!
//! ## Role
//! Per spec § 36 § Adaptive-sampling via evidence the per-cell evidence-glyph
//! drives the sample-budget. Cells flagged ◐ (uncertain) get extra iterations ;
//! ✓ (trusted) cells skip ; ✗ (rejected) cells null-light. The
//! [`EvidenceDriver`] consults these glyphs and emits a per-cell budget
//! that the CFER iterator multiplies into the convergence-loop.
//!
//! ## Algorithm
//!   for each cell c :
//!     glyph[c] = ◐  if  ‖L^{(k+1)} - L^{(k)}‖ > confidence-threshold
//!              | ✓  if  ‖L^{(k+1)} - L^{(k)}‖ < ε
//!              | ✗  if  cell.refused_consent
//!              | ○  default
//!     budget[c] = base_budget × glyph_weight[glyph[c]]
//!
//! ## Glyph weights (default)
//!   ◐ → 4.0  (uncertain ; needs 4× the budget)
//!   ✓ → 0.0  (trusted ; skip iteration)
//!   ✗ → 0.0  (rejected ; skip + null-light)
//!   ○ → 1.0  (default cadence)

use crate::light_stub::LightState;

/// Evidence glyph : per-cell sampling-priority hint.
///
/// Per CSL canon : ◐ ✓ ✗ ○ are the four classes ; we encode them as the four
/// variants of this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceGlyph {
    /// ◐ uncertain : prioritize KAN-update-iteration ; re-converge.
    Uncertain,
    /// ✓ trusted : skip ; use cached state.
    Trusted,
    /// ✗ rejected : skip + null-light.
    Rejected,
    /// ○ default : standard cadence.
    Default,
}

impl EvidenceGlyph {
    /// Glyph-weight for the budget multiplier. Returns 0 for skip-class.
    #[inline]
    pub fn budget_weight(self) -> f32 {
        match self {
            EvidenceGlyph::Uncertain => 4.0,
            EvidenceGlyph::Trusted => 0.0,
            EvidenceGlyph::Rejected => 0.0,
            EvidenceGlyph::Default => 1.0,
        }
    }

    /// True iff the glyph is in the skip-class (✓ ✗).
    #[inline]
    pub fn is_skip(self) -> bool {
        matches!(self, EvidenceGlyph::Trusted | EvidenceGlyph::Rejected)
    }

    /// Char rendering for tracing (1B unicode glyph).
    #[inline]
    pub fn as_char(self) -> char {
        match self {
            EvidenceGlyph::Uncertain => '◐',
            EvidenceGlyph::Trusted => '✓',
            EvidenceGlyph::Rejected => '✗',
            EvidenceGlyph::Default => '○',
        }
    }
}

impl Default for EvidenceGlyph {
    fn default() -> Self {
        EvidenceGlyph::Default
    }
}

/// Per-frame summary of evidence-driver activity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EvidenceReport {
    /// Cells flagged ◐ this frame.
    pub uncertain_count: u32,
    /// Cells flagged ✓ this frame.
    pub trusted_count: u32,
    /// Cells flagged ✗ this frame.
    pub rejected_count: u32,
    /// Cells defaulted ○ this frame.
    pub default_count: u32,
    /// Total budget allocated (sum over cells).
    pub total_budget: f32,
}

impl EvidenceReport {
    /// Total cells inspected this frame.
    pub fn total_cells(&self) -> u32 {
        self.uncertain_count + self.trusted_count + self.rejected_count + self.default_count
    }

    /// Skip-rate : (✓ + ✗) / total ; high values indicate well-warmed cache.
    pub fn skip_rate(&self) -> f32 {
        let t = self.total_cells();
        if t == 0 {
            0.0
        } else {
            (self.trusted_count + self.rejected_count) as f32 / (t as f32)
        }
    }
}

/// Adaptive evidence-driver : translates per-cell residuals + glyphs into
/// per-cell sampling-budgets.
#[derive(Debug, Clone)]
pub struct EvidenceDriver {
    /// Convergence ε : ‖ΔL‖ below this ↦ ✓ trusted.
    pub epsilon: f32,
    /// Confidence threshold : ‖ΔL‖ above this ↦ ◐ uncertain.
    pub confidence_threshold: f32,
    /// Base budget per cell (1 = standard one-iteration cadence).
    pub base_budget: f32,
    /// Optional priority multiplier (e.g. foveation factor).
    pub priority_scale: f32,
}

impl Default for EvidenceDriver {
    fn default() -> Self {
        Self {
            epsilon: 1e-3,
            confidence_threshold: 1e-1,
            base_budget: 1.0,
            priority_scale: 1.0,
        }
    }
}

impl EvidenceDriver {
    /// Classify a residual into a glyph.
    pub fn classify(&self, residual: f32) -> EvidenceGlyph {
        if residual.is_nan() || residual.is_infinite() {
            EvidenceGlyph::Uncertain
        } else if residual < self.epsilon {
            EvidenceGlyph::Trusted
        } else if residual > self.confidence_threshold {
            EvidenceGlyph::Uncertain
        } else {
            EvidenceGlyph::Default
        }
    }

    /// Classify based on two consecutive light-states (computes the residual).
    pub fn classify_states(&self, prev: LightState, next: LightState) -> EvidenceGlyph {
        self.classify(prev.norm_diff_l1(next))
    }

    /// Per-cell budget for a given glyph. Returns 0 for skip-class glyphs.
    pub fn budget_for(&self, glyph: EvidenceGlyph) -> f32 {
        self.base_budget * self.priority_scale * glyph.budget_weight()
    }

    /// Record one cell into a running [`EvidenceReport`] ; convenient for
    /// streaming-aggregation.
    pub fn tally(&self, report: &mut EvidenceReport, glyph: EvidenceGlyph) {
        match glyph {
            EvidenceGlyph::Uncertain => report.uncertain_count += 1,
            EvidenceGlyph::Trusted => report.trusted_count += 1,
            EvidenceGlyph::Rejected => report.rejected_count += 1,
            EvidenceGlyph::Default => report.default_count += 1,
        }
        report.total_budget += self.budget_for(glyph);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::light_stub::LightState;

    #[test]
    fn glyph_budget_weights_canonical() {
        assert_eq!(EvidenceGlyph::Uncertain.budget_weight(), 4.0);
        assert_eq!(EvidenceGlyph::Trusted.budget_weight(), 0.0);
        assert_eq!(EvidenceGlyph::Rejected.budget_weight(), 0.0);
        assert_eq!(EvidenceGlyph::Default.budget_weight(), 1.0);
    }

    #[test]
    fn glyph_skip_class_correct() {
        assert!(EvidenceGlyph::Trusted.is_skip());
        assert!(EvidenceGlyph::Rejected.is_skip());
        assert!(!EvidenceGlyph::Uncertain.is_skip());
        assert!(!EvidenceGlyph::Default.is_skip());
    }

    #[test]
    fn classify_zero_residual_is_trusted() {
        let d = EvidenceDriver::default();
        assert_eq!(d.classify(0.0), EvidenceGlyph::Trusted);
    }

    #[test]
    fn classify_high_residual_is_uncertain() {
        let d = EvidenceDriver::default();
        assert_eq!(d.classify(1.0), EvidenceGlyph::Uncertain);
    }

    #[test]
    fn classify_mid_residual_is_default() {
        let d = EvidenceDriver::default();
        // ε = 1e-3, threshold = 1e-1 ; mid is in (ε, threshold).
        assert_eq!(d.classify(0.05), EvidenceGlyph::Default);
    }

    #[test]
    fn classify_states_uses_norm_diff_l1() {
        let d = EvidenceDriver::default();
        let a = LightState::zero();
        let b = LightState::from_coefs([0.5; 8]);
        assert_eq!(d.classify_states(a, b), EvidenceGlyph::Uncertain);
        assert_eq!(d.classify_states(a, a), EvidenceGlyph::Trusted);
    }

    #[test]
    fn report_skip_rate_reflects_warm_cache() {
        let d = EvidenceDriver::default();
        let mut r = EvidenceReport::default();
        for _ in 0..90 {
            d.tally(&mut r, EvidenceGlyph::Trusted);
        }
        for _ in 0..10 {
            d.tally(&mut r, EvidenceGlyph::Uncertain);
        }
        assert_eq!(r.total_cells(), 100);
        assert!(r.skip_rate() > 0.85);
    }

    #[test]
    fn budget_skip_class_is_zero() {
        let d = EvidenceDriver::default();
        assert_eq!(d.budget_for(EvidenceGlyph::Trusted), 0.0);
        assert_eq!(d.budget_for(EvidenceGlyph::Rejected), 0.0);
    }

    #[test]
    fn nan_residual_treated_as_uncertain() {
        let d = EvidenceDriver::default();
        assert_eq!(d.classify(f32::NAN), EvidenceGlyph::Uncertain);
        assert_eq!(d.classify(f32::INFINITY), EvidenceGlyph::Uncertain);
    }

    #[test]
    fn glyph_chars_canonical() {
        assert_eq!(EvidenceGlyph::Uncertain.as_char(), '◐');
        assert_eq!(EvidenceGlyph::Trusted.as_char(), '✓');
        assert_eq!(EvidenceGlyph::Rejected.as_char(), '✗');
        assert_eq!(EvidenceGlyph::Default.as_char(), '○');
    }
}
