//! § companion_semantic_pass — Stage 8 : Companion-perspective semantic render.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 8 of the pipeline. Drives `cssl-render-companion-perspective::
//!   CompanionPerspectivePass` over a small canonical companion context.
//!   For M8 vertical-slice the pass mostly takes the `skip()` zero-cost
//!   path because the full execute path requires a companion-consent token
//!   plus the full Σ-mask wiring that lands in M11 / Stage-9 polish.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use cssl_render_companion_perspective::{CompanionPerspectivePass, RenderCostReport};

use super::omega_field_update::OmegaFieldOutputs;

/// Outputs of Stage 8 — companion-view summary.
#[derive(Debug, Clone)]
pub struct CompanionSemanticOutputs {
    /// Frame this snapshot covers.
    pub frame_idx: u64,
    /// Whether the pass took the zero-cost skip path.
    pub skipped: bool,
    /// Whether the companion explicitly declined.
    pub companion_declined: bool,
    /// Number of cells evaluated (zero on skip path).
    pub cells_evaluated: u32,
    /// Number of cells refused at per-cell consent gate.
    pub cells_consent_refused: u32,
}

impl CompanionSemanticOutputs {
    /// Construct a "skipped" output.
    #[must_use]
    pub fn skipped(frame_idx: u64) -> Self {
        Self {
            frame_idx,
            skipped: true,
            companion_declined: false,
            cells_evaluated: 0,
            cells_consent_refused: 0,
        }
    }

    /// Hash for determinism comparison.
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        self.skipped.hash(&mut h);
        self.companion_declined.hash(&mut h);
        self.cells_evaluated.hash(&mut h);
        self.cells_consent_refused.hash(&mut h);
        h.finish()
    }
}

/// Stage 8 driver.
pub struct CompanionSemanticPass {
    inner: CompanionPerspectivePass,
}

impl std::fmt::Debug for CompanionSemanticPass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompanionSemanticPass")
            .finish_non_exhaustive()
    }
}

impl CompanionSemanticPass {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: CompanionPerspectivePass::canonical(),
        }
    }

    /// Run Stage 8 — for M8 we always take the zero-cost skip path. The
    /// full execute path requires companion-consent + per-cell Σ-mask
    /// integration that lands in subsequent slices.
    pub fn run(&mut self, omega: &OmegaFieldOutputs, frame_idx: u64) -> CompanionSemanticOutputs {
        let (_view, report) = self.inner.skip(frame_idx, false);
        let _ = omega; // upstream coupling (no-op in skip path).
        Self::summarize(frame_idx, &report)
    }

    /// Summarize a `RenderCostReport` into the lite output.
    fn summarize(frame_idx: u64, report: &RenderCostReport) -> CompanionSemanticOutputs {
        CompanionSemanticOutputs {
            frame_idx,
            skipped: report.skipped,
            companion_declined: report.companion_declined,
            cells_evaluated: report.cells_evaluated,
            cells_consent_refused: report.cells_consent_refused,
        }
    }
}

impl Default for CompanionSemanticPass {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn omega() -> OmegaFieldOutputs {
        OmegaFieldOutputs {
            frame_idx: 0,
            epoch: 0,
            dense_cell_count: 0,
            cells_collapsed: 0,
            cells_propagated: 0,
            phase_epochs: [0; 6],
        }
    }

    #[test]
    fn companion_constructs() {
        let _ = CompanionSemanticPass::new();
    }

    #[test]
    fn companion_skip_path_zero_cost() {
        let mut p = CompanionSemanticPass::new();
        let o = p.run(&omega(), 0);
        assert!(o.skipped);
        assert_eq!(o.cells_evaluated, 0);
    }

    #[test]
    fn companion_replay_bit_equal() {
        let mut p1 = CompanionSemanticPass::new();
        let mut p2 = CompanionSemanticPass::new();
        let a = p1.run(&omega(), 7);
        let b = p2.run(&omega(), 7);
        assert_eq!(a.determinism_hash(), b.determinism_hash());
    }

    #[test]
    fn companion_skipped_constructor() {
        let s = CompanionSemanticOutputs::skipped(11);
        assert!(s.skipped);
        assert_eq!(s.frame_idx, 11);
    }
}
