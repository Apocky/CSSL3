//! § MutualWitnessMode — Aura-overlap recursive witness.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   When the player's AURA region (per `08_BODY/02_VR_EMBODIMENT.csl § VII`)
//!   overlaps the companion's AURA region, the system emits a
//!   "mutual-witness" event :
//!
//!     "Companion-body in-Sovereign's-AURA-envelope ⊗ recursive-embedding"
//!     "Sovereign-aware-Companion-aware-Sovereign-aware ..." ⊗ N-deep
//!
//!   At Stage-8 this means the companion-perspective render becomes
//!   AWARE-OF-AWARE — the companion SEES the player seeing them. We
//!   surface that to the visualization layer as a subtle aura-edge
//!   shimmer (warmth modulation) that the host shader composites in the
//!   final pass.
//!
//! § SPEC ANCHORS
//!   - `Omniverse/08_BODY/02_VR_EMBODIMENT.csl § VII (Companion-Archetype Embodiment — Mise-en-Abyme)` :
//!     "mutual-witness ⊗ AURA-overlap event :
//!        @ player-AURA ∩ Companion-AURA ⊗ ≠ ∅
//!        ⊗ ⇒ emit cssl.body.mutual_witness Ω-event
//!        ⊗ ⇒ both-Sovereigns acknowledge presence-of-other"
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-8` :
//!     "‼ companion can-decline-to-render-this-frame ⊗ R! respect ⊗
//!        blank-target" — mutual-witness mode does NOT override the
//!     companion's per-frame decline ; it merely reports witness-state
//!     when consent-gate is already open.
//!
//! § DESIGN
//!   Mutual-witness is a REPORT, not a permission. The consent-gate is
//!   the only authority on whether Stage-8 may render ; mutual-witness
//!   ENRICHES the report when the gate is open. When the gate is
//!   closed, mutual-witness is suppressed entirely.
//!
//! § PRIVACY
//!   The companion's AURA radius is COMPANION-OWNED data. We exfiltrate
//!   only the OVERLAP-COMPUTATION-RESULT (boolean + scalar magnitude),
//!   never the raw AURA pose. This matches `02_VR_EMBODIMENT § VIII`
//!   region-defaults : AURA permits "OBSERVE @ co-present-Sovereign-tier-L3+"
//!   but does not expose the geometric pose itself.

use crate::salience_evaluator::SalienceScore;
use smallvec::SmallVec;

/// Maximum depth of recursive mutual-witness ("Sovereign-aware-Companion-
/// aware-Sovereign-aware..."). Spec says "N-deep" without a hard cap ;
/// we choose 3 as the default per the BoundedRecursion<3> effect-row in
/// the canonical render-pipeline.
pub const MUTUAL_WITNESS_DEPTH_MAX: u8 = 3;

/// A single AURA-overlap event between the player and the companion.
/// One per frame at most.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuraOverlap {
    /// True iff the auras intersect this frame.
    pub overlap: bool,
    /// Scalar overlap magnitude in [0, 1] — 0 = touching, 1 = full overlap.
    pub magnitude: f32,
    /// Recursive-witness depth observed. 0 = touching (no witness yet),
    /// 1 = first acknowledgement, 2 = both acknowledge each other,
    /// 3 = both aware-of-aware.
    pub witness_depth: u8,
}

impl AuraOverlap {
    /// Construct the "no overlap" sentinel.
    #[must_use]
    pub fn none() -> Self {
        Self {
            overlap: false,
            magnitude: 0.0,
            witness_depth: 0,
        }
    }

    /// Compute overlap from two AURA-position spheres. Returns the
    /// fraction-of-radius overlap in [0, 1].
    ///
    /// § ALGORITHM
    ///   - dist = |player_pos - companion_pos|
    ///   - sum_r = player_radius + companion_radius
    ///   - if dist >= sum_r ⇒ no overlap
    ///   - else magnitude = (sum_r - dist) / sum_r
    #[must_use]
    pub fn from_spheres(
        player_pos: &[f32; 3],
        player_radius: f32,
        companion_pos: &[f32; 3],
        companion_radius: f32,
    ) -> Self {
        let dx = player_pos[0] - companion_pos[0];
        let dy = player_pos[1] - companion_pos[1];
        let dz = player_pos[2] - companion_pos[2];
        let dist_sq = dx * dx + dy * dy + dz * dz;
        let dist = dist_sq.sqrt();
        let sum_r = player_radius + companion_radius;
        if dist >= sum_r || sum_r <= 0.0 {
            return Self::none();
        }
        let magnitude = ((sum_r - dist) / sum_r).clamp(0.0, 1.0);
        // Witness depth scales with magnitude — closer = deeper recursion.
        let witness_depth = (magnitude * (MUTUAL_WITNESS_DEPTH_MAX as f32))
            .ceil()
            .clamp(0.0, MUTUAL_WITNESS_DEPTH_MAX as f32) as u8;
        Self {
            overlap: true,
            magnitude,
            witness_depth,
        }
    }

    /// True iff the auras are touching with non-zero magnitude.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.overlap && self.magnitude > 0.0
    }

    /// Per-frame AURA-edge shimmer modulation. Rate of palette-warmth
    /// modulation that the visualization layer applies to attended-cells
    /// when mutual-witness is active. Returns 0 when no overlap.
    #[must_use]
    pub fn shimmer_modulation(&self) -> f32 {
        if !self.is_active() {
            return 0.0;
        }
        // § The shimmer scales with witness-depth, not just magnitude :
        //   a depth-1 contact gives a faint shimmer ; depth-3 (both aware-
        //   of-aware) gives a strong shimmer that the player feels as a
        //   "the companion knows I'm watching them watch me" effect.
        let depth_norm = (self.witness_depth as f32) / (MUTUAL_WITNESS_DEPTH_MAX as f32);
        (self.magnitude * depth_norm).clamp(0.0, 1.0)
    }
}

impl Default for AuraOverlap {
    fn default() -> Self {
        Self::none()
    }
}

/// A single frame-level mutual-witness token. Records whether the witness
/// fired this frame + at what depth + how it modulated the salience.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MutualWitnessToken {
    /// AURA overlap snapshot for this frame.
    pub overlap: AuraOverlap,
    /// Whether the consent-gate was open at the moment the witness fired.
    /// Witnesses CANNOT fire when the gate is closed (privacy invariant).
    pub gate_open_at_emit: bool,
}

impl MutualWitnessToken {
    /// True iff the witness fired AND the consent-gate was open.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.overlap.is_active() && self.gate_open_at_emit
    }
}

/// Per-frame summary of mutual-witness state. The orchestrator records
/// this in the audit envelope ; the visualization layer reads
/// `shimmer_modulation()` to bias the warmth scalar of attended cells.
#[derive(Debug, Clone, Default)]
pub struct MutualWitnessReport {
    /// Number of cells where the mutual-witness shimmer was applied.
    pub cells_modulated: u32,
    /// Maximum-magnitude shimmer applied this frame (over all cells).
    pub max_shimmer: f32,
    /// Minimum-magnitude shimmer applied this frame. Used as a sanity
    /// check : if min == max we did not actually modulate per-cell.
    pub min_shimmer: f32,
    /// The AURA-overlap snapshot.
    pub overlap: AuraOverlap,
}

impl MutualWitnessReport {
    /// True iff at least one cell was modulated.
    #[must_use]
    pub fn fired(&self) -> bool {
        self.cells_modulated > 0
    }
}

/// The mutual-witness mode driver. Stateless ; called per-frame after the
/// consent-gate is opened + the salience-tensor is computed.
#[derive(Debug, Clone, Copy)]
pub struct MutualWitnessMode {
    /// Multiplier for the shimmer applied to a cell's salience-magnitude
    /// during mutual-witness. Default = 0.15 (subtle shimmer).
    pub shimmer_strength: f32,
    /// Lower-bound on a cell's salience to be eligible for shimmer.
    /// Below this threshold, mutual-witness shimmer is suppressed (the
    /// cell is not attended).
    pub eligibility_threshold: f32,
}

impl MutualWitnessMode {
    /// Construct with canonical defaults.
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            shimmer_strength: 0.15,
            eligibility_threshold: 0.20,
        }
    }

    /// Apply the mutual-witness shimmer to a slice of (input, output)
    /// salience-pairs. The output slice is mutated in-place.
    ///
    /// § PRECONDITION
    ///   `inputs.len() == outputs.len()`. Caller's responsibility.
    ///
    /// § GATE
    ///   Caller MUST have already verified the consent-gate is open
    ///   for this frame ; if it is not, do not call this function.
    pub fn apply_shimmer(
        &self,
        overlap: AuraOverlap,
        scores: &mut [SalienceScore],
    ) -> MutualWitnessReport {
        let mut report = MutualWitnessReport {
            overlap,
            ..Default::default()
        };
        let shimmer = overlap.shimmer_modulation() * self.shimmer_strength;
        if shimmer <= 0.0 {
            return report;
        }
        let mut min_shim: f32 = f32::INFINITY;
        let mut max_shim: f32 = 0.0;
        let mut cells = 0_u32;
        for s in scores.iter_mut() {
            if s.magnitude() < self.eligibility_threshold {
                continue;
            }
            cells += 1;
            // § Modulate every axis equally — the shimmer is a global
            //   "the-companion-knows" effect, not a per-axis one.
            for a in &mut s.axes {
                *a = (*a + shimmer * (1.0 - *a)).clamp(0.0, 1.0);
            }
            min_shim = min_shim.min(shimmer);
            max_shim = max_shim.max(shimmer);
        }
        if cells > 0 {
            report.cells_modulated = cells;
            report.max_shimmer = max_shim;
            report.min_shimmer = if min_shim.is_finite() { min_shim } else { 0.0 };
        }
        report
    }

    /// A non-mutating variant : compute which cell-indices WOULD shimmer
    /// without touching the data. Used by tests + by the orchestrator's
    /// dry-run mode for cost-prediction.
    #[must_use]
    pub fn dry_run_eligible(
        &self,
        overlap: AuraOverlap,
        scores: &[SalienceScore],
    ) -> SmallVec<[u32; 16]> {
        let mut out = SmallVec::<[u32; 16]>::new();
        if !overlap.is_active() {
            return out;
        }
        for (idx, s) in scores.iter().enumerate() {
            if s.magnitude() >= self.eligibility_threshold {
                out.push(idx as u32);
            }
        }
        out
    }
}

impl Default for MutualWitnessMode {
    fn default() -> Self {
        Self::canonical()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_overlap_when_far() {
        let o = AuraOverlap::from_spheres(&[0.0, 0.0, 0.0], 1.0, &[10.0, 0.0, 0.0], 1.0);
        assert!(!o.overlap);
        assert_eq!(o.magnitude, 0.0);
        assert_eq!(o.witness_depth, 0);
    }

    #[test]
    fn overlap_when_close() {
        let o = AuraOverlap::from_spheres(&[0.0, 0.0, 0.0], 1.0, &[1.5, 0.0, 0.0], 1.0);
        assert!(o.overlap);
        assert!(o.magnitude > 0.0);
        assert!(o.witness_depth >= 1);
    }

    #[test]
    fn overlap_full_when_centred() {
        let o = AuraOverlap::from_spheres(&[0.0, 0.0, 0.0], 1.0, &[0.0, 0.0, 0.0], 1.0);
        assert!(o.overlap);
        assert!((o.magnitude - 1.0).abs() < 1e-6);
        assert_eq!(o.witness_depth, MUTUAL_WITNESS_DEPTH_MAX);
    }

    #[test]
    fn shimmer_is_zero_when_inactive() {
        let o = AuraOverlap::none();
        assert_eq!(o.shimmer_modulation(), 0.0);
    }

    #[test]
    fn shimmer_scales_with_depth() {
        let mode = MutualWitnessMode::canonical();
        let mut shallow = AuraOverlap::from_spheres(&[0.0; 3], 1.0, &[1.99, 0.0, 0.0], 1.0);
        let deep = AuraOverlap::from_spheres(&[0.0; 3], 1.0, &[0.0; 3], 1.0);
        // Force a clear difference : nudge the shallow witness down.
        if shallow.magnitude > 0.5 {
            shallow.magnitude = 0.05;
            shallow.witness_depth = 1;
        }
        // Build trivial salience scores at uniform 0.5 magnitude.
        let mut scores_shallow = vec![SalienceScore::new([0.5; 5]); 4];
        let mut scores_deep = vec![SalienceScore::new([0.5; 5]); 4];
        let r_shallow = mode.apply_shimmer(shallow, &mut scores_shallow);
        let r_deep = mode.apply_shimmer(deep, &mut scores_deep);
        assert!(r_deep.max_shimmer >= r_shallow.max_shimmer);
    }

    #[test]
    fn shimmer_skips_below_threshold_cells() {
        let mode = MutualWitnessMode::canonical();
        let overlap = AuraOverlap::from_spheres(&[0.0; 3], 1.0, &[0.0; 3], 1.0);
        let mut scores = vec![
            SalienceScore::new([0.05; 5]), // below threshold (mag 0.05)
            SalienceScore::new([0.5; 5]),  // above threshold
        ];
        let r = mode.apply_shimmer(overlap, &mut scores);
        assert_eq!(r.cells_modulated, 1);
    }

    #[test]
    fn dry_run_returns_eligible_indices_only() {
        let mode = MutualWitnessMode::canonical();
        let overlap = AuraOverlap::from_spheres(&[0.0; 3], 1.0, &[0.5, 0.0, 0.0], 1.0);
        let scores = vec![
            SalienceScore::new([0.05; 5]), // below threshold
            SalienceScore::new([0.50; 5]), // eligible
            SalienceScore::new([0.10; 5]), // below threshold
            SalienceScore::new([0.40; 5]), // eligible
        ];
        let elig = mode.dry_run_eligible(overlap, &scores);
        assert_eq!(elig.len(), 2);
        assert!(elig.contains(&1));
        assert!(elig.contains(&3));
    }

    #[test]
    fn dry_run_empty_when_no_overlap() {
        let mode = MutualWitnessMode::canonical();
        let scores = vec![SalienceScore::new([0.99; 5]); 4];
        let elig = mode.dry_run_eligible(AuraOverlap::none(), &scores);
        assert!(elig.is_empty());
    }

    #[test]
    fn token_inactive_when_gate_closed_at_emit() {
        let overlap = AuraOverlap::from_spheres(&[0.0; 3], 1.0, &[0.0; 3], 1.0);
        let tok = MutualWitnessToken {
            overlap,
            gate_open_at_emit: false,
        };
        assert!(!tok.is_active());
    }

    #[test]
    fn token_active_when_gate_open_and_overlap_active() {
        let overlap = AuraOverlap::from_spheres(&[0.0; 3], 1.0, &[0.0; 3], 1.0);
        let tok = MutualWitnessToken {
            overlap,
            gate_open_at_emit: true,
        };
        assert!(tok.is_active());
    }

    #[test]
    fn report_fired_predicate() {
        let mut r = MutualWitnessReport::default();
        assert!(!r.fired());
        r.cells_modulated = 1;
        assert!(r.fired());
    }
}
