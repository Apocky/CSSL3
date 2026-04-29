//! § SemanticSalienceEvaluator — KAN(world_pos, companion_context) → salience
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The cognitive-projection core of Stage-8. Maps a world-position + the
//!   companion's belief-state into a 5-axis salience tuple :
//!
//!     {salience, threat, food-affinity, social-trust, Λ-token-density}
//!
//!   Spec :  Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.5(a)
//!     "semantic-Ω' axes : {salience, threat, food-affinity, social-trust,
//!      Λ-token-density}"
//!
//! § DESIGN
//!   The evaluator is parameterized over a single KAN-network per axis.
//!   The KAN's INPUT is the concatenation of :
//!     (3-D world position normalized to scene bounds, companion-belief-32D)
//!   total = 35-D : but we use the substrate's canonical KanNetwork<32, _>
//!   instantiation by FOLDING the 3-D position into a deterministic 32-D
//!   row-mix with the belief-embedding. This is the "encoding-row" — the
//!   substrate-canonical way to gate-mix a high-D belief with a low-D
//!   spatial coord.
//!
//! § SHAPE-CONTRACT
//!   - input  : `[f32; 32]` (encoded-row : pos⊕belief, folded into 32-D)
//!   - output : `[f32; 1]`  (per-axis salience score in [0, 1])
//!   - one KAN per axis : 5 KANs total. Sharing networks across axes would
//!     conflate the meanings ; the spec is explicit that the axes are
//!     orthogonal projections.
//!
//! § DETERMINISM
//!   - The encoding-row is a deterministic mix : same `(world_pos, belief)`
//!     ⇒ identical encoded row across hosts and runs. No SystemTime, no
//!     thread_rng, no per-host hashing.
//!   - The KAN.eval() is deterministic per cssl-substrate-kan's contract.
//!   - Combined : `evaluate(pos, ctx) ⇒ same SalienceScore` everywhere.
//!
//! § UNTRAINED-NETWORK FALLBACK
//!   The substrate-kan KanNetwork's eval() returns zeros for untrained
//!   networks. The evaluator detects this and falls back to a deterministic
//!   "synthetic-salience" function : a simple distance-modulated belief-
//!   norm that gives non-degenerate test fixtures. The synthetic mode is
//!   the SHIPPING PATH for development builds where no companion-AI engine
//!   has trained the KANs yet ; the production path retrains the KANs
//!   per-companion-archetype during 05_INTELLIGENCE init.
//!
//! § ANTI-SURVEILLANCE
//!   - The evaluator NEVER stores its inputs or outputs. Every `evaluate`
//!     call is a pure function ; the only state in the evaluator is the
//!     KAN-weight tensors themselves.
//!   - The salience scores never leave the render-pipeline. They become
//!     pixels via the visualization layer ; they are NOT exfiltrated as
//!     analytics.

use crate::companion_context::{CompanionContext, BELIEF_DIM};
use cssl_substrate_kan::KanNetwork;

/// The five salience axes. Spec § V.5(a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SalienceAxis {
    /// "How attended is this cell." General perception-relevance.
    Salience = 0,
    /// "How threatening is this cell." Companion's posterior over harm.
    Threat = 1,
    /// "How nourishing is this cell." Companion's posterior over food.
    FoodAffinity = 2,
    /// "How trusted is this cell." Companion's posterior over friendly-agent.
    SocialTrust = 3,
    /// "How symbol-rich is this cell." Companion's posterior over Λ-density.
    LambdaTokenDensity = 4,
}

impl SalienceAxis {
    /// All five axes in canonical order. Used by the evaluator to iterate
    /// across all axes when computing a full SalienceScore.
    pub const ALL: [SalienceAxis; SALIENCE_AXES] = [
        Self::Salience,
        Self::Threat,
        Self::FoodAffinity,
        Self::SocialTrust,
        Self::LambdaTokenDensity,
    ];
}

/// Number of salience axes. Stable per spec.
pub const SALIENCE_AXES: usize = 5;

/// Per-cell salience tuple, indexed by SalienceAxis. Each entry ∈ [0, 1] —
/// scaled by the evaluator at output time.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SalienceScore {
    /// Per-axis scores ∈ [0, 1].
    pub axes: [f32; SALIENCE_AXES],
}

impl SalienceScore {
    /// Construct the all-zeros score. Used as the default when consent
    /// blocks the read.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            axes: [0.0; SALIENCE_AXES],
        }
    }

    /// Construct from axis values directly.
    #[must_use]
    pub fn new(axes: [f32; SALIENCE_AXES]) -> Self {
        Self { axes }
    }

    /// Read a single axis.
    #[must_use]
    pub fn at(&self, axis: SalienceAxis) -> f32 {
        self.axes[axis as usize]
    }

    /// Get a mutable reference to a single axis (used during construction).
    pub fn at_mut(&mut self, axis: SalienceAxis) -> &mut f32 {
        &mut self.axes[axis as usize]
    }

    /// True iff every axis is finite + within [0, 1] (within 1e-4 slack).
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.axes
            .iter()
            .all(|a| a.is_finite() && (-1e-4..=1.0 + 1e-4).contains(a))
    }

    /// Saturate-clamp every axis to [0, 1]. Used as a defensive normalizer
    /// post-evaluation.
    #[must_use]
    pub fn saturated(self) -> Self {
        let mut s = self;
        for a in &mut s.axes {
            *a = a.clamp(0.0, 1.0);
        }
        s
    }

    /// The dominant axis (the one with the highest score). Used by the
    /// visualization-layer to pick the palette tint.
    #[must_use]
    pub fn dominant(&self) -> SalienceAxis {
        let mut best_idx: usize = 0;
        let mut best_val: f32 = self.axes[0];
        for (i, a) in self.axes.iter().enumerate().skip(1) {
            if *a > best_val {
                best_val = *a;
                best_idx = i;
            }
        }
        SalienceAxis::ALL[best_idx]
    }

    /// Scalar magnitude (mean of all axes). Used as the glow-edge intensity.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        let mut s = 0.0_f32;
        for a in &self.axes {
            s += *a;
        }
        s / (SALIENCE_AXES as f32)
    }
}

/// The salience evaluator. Stores one KAN per axis ; evaluates a
/// (world_pos, ctx) pair into a SalienceScore.
///
/// § DESIGN
///   The evaluator is `Send + Sync` and intended to live for the lifetime
///   of a session ; it is constructed once at 05_INTELLIGENCE init time
///   and consulted per-frame. The per-frame allocation pattern is zero —
///   `evaluate` returns a stack-allocated SalienceScore.
#[derive(Debug, Clone)]
pub struct SemanticSalienceEvaluator {
    /// KAN networks, one per axis. Each is a `KanNetwork<32, 1>` :
    /// 32-D encoded-row → single salience scalar.
    nets: [KanNetwork<32, 1>; SALIENCE_AXES],
    /// Soft-clamp range for the synthetic fallback. The synthetic path
    /// keeps outputs in [0, max_synthetic] before the saturate ; default
    /// is 0.9 so the visualization sees non-saturated values.
    max_synthetic: f32,
}

impl SemanticSalienceEvaluator {
    /// Construct an evaluator with the canonical untrained KAN-set. The
    /// untrained KANs trigger the synthetic-fallback path at evaluation
    /// time, which gives deterministic non-degenerate test fixtures.
    #[must_use]
    pub fn new_untrained() -> Self {
        Self {
            nets: [
                KanNetwork::new_untrained(),
                KanNetwork::new_untrained(),
                KanNetwork::new_untrained(),
                KanNetwork::new_untrained(),
                KanNetwork::new_untrained(),
            ],
            max_synthetic: 0.9,
        }
    }

    /// Construct from a custom KAN-set. The five networks are taken in
    /// canonical axis-order : (Salience, Threat, FoodAffinity, SocialTrust,
    /// LambdaTokenDensity).
    #[must_use]
    pub fn with_networks(nets: [KanNetwork<32, 1>; SALIENCE_AXES]) -> Self {
        Self {
            nets,
            max_synthetic: 0.9,
        }
    }

    /// Override the synthetic-fallback max value. Default is 0.9. Setting
    /// to 1.0 produces saturated-only outputs ; setting to 0.5 produces
    /// dim outputs (useful for tests checking that magnitudes are below a
    /// known threshold).
    pub fn set_max_synthetic(&mut self, max: f32) {
        self.max_synthetic = max.clamp(0.0, 1.0);
    }

    /// Read-only access to a per-axis KAN. Used by tests + by the
    /// fingerprint-stability proofs.
    #[must_use]
    pub fn network(&self, axis: SalienceAxis) -> &KanNetwork<32, 1> {
        &self.nets[axis as usize]
    }

    /// True iff the evaluator's KANs are all marked-trained. When false,
    /// the synthetic-fallback path is used.
    #[must_use]
    pub fn is_trained(&self) -> bool {
        self.nets.iter().all(|n| n.trained)
    }

    /// Evaluate the salience of a single world-position under the
    /// companion's context. Returns a SalienceScore with all five axes
    /// populated.
    ///
    /// § DETERMINISM
    ///   Same (world_pos, ctx) ⇒ same score. No global state.
    ///
    /// § COST
    ///   The evaluator does NOT loop over all cells. It is called per-cell
    ///   by the pass-orchestrator ; per-call cost is dominated by the KAN
    ///   eval (deferred to cssl-substrate-kan) plus the encoding-row
    ///   fold (5 fma-style ops per axis × 32 lanes ≈ 800 flops per eval).
    #[must_use]
    pub fn evaluate(&self, world_pos: &[f32; 3], ctx: &CompanionContext) -> SalienceScore {
        let row = self.encode_row(world_pos, ctx);
        let attention_falloff = ctx.attention_falloff(world_pos);
        let weights = ctx.axis_base_weights();
        let mut score = SalienceScore::zero();
        if self.is_trained() {
            for (axis_idx, axis) in SalienceAxis::ALL.iter().enumerate() {
                let raw = self.nets[axis_idx].eval(&row);
                // The KAN evaluator emits a single-element output per axis.
                let v = raw[0];
                *score.at_mut(*axis) = (v * weights[axis_idx] * attention_falloff).clamp(0.0, 1.0);
            }
        } else {
            // § Synthetic-fallback path. Deterministic + axis-distinct.
            self.evaluate_synthetic(&row, weights, attention_falloff, &mut score);
        }
        score
    }

    /// Synthetic salience function used when the KANs are untrained. Each
    /// axis gets a different deterministic mix of (row, weights) so the
    /// axes do not collapse. The mixes are designed to be :
    ///   - Bounded in [0, max_synthetic] before clamping
    ///   - Distinct enough that `dominant()` is non-trivial
    ///   - A function of (world_pos, belief, emotion) — moving any of these
    ///     changes the output, which gives test-fixtures plenty of variation.
    fn evaluate_synthetic(
        &self,
        row: &[f32; 32],
        weights: [f32; SALIENCE_AXES],
        attention_falloff: f32,
        score: &mut SalienceScore,
    ) {
        // § Per-axis mix : different sub-windows of the encoded-row.
        //   Each axis gets a distinct slice + a distinct nonlinearity, so
        //   the synthetic-salience axes are well-separated even on
        //   "vanilla" untrained-companion fixtures.
        let s_norm = synthetic_norm(&row[0..8]);
        let t_norm = synthetic_norm(&row[4..12]);
        let f_norm = synthetic_norm(&row[10..20]);
        let st_norm = synthetic_norm(&row[18..26]);
        let l_norm = synthetic_norm(&row[24..32]);
        let cap = self.max_synthetic;
        let put = |axis: SalienceAxis, v: f32, score: &mut SalienceScore| {
            let scaled = (v * weights[axis as usize] * attention_falloff).clamp(0.0, cap);
            *score.at_mut(axis) = scaled;
        };
        put(SalienceAxis::Salience, s_norm, score);
        put(SalienceAxis::Threat, t_norm, score);
        put(SalienceAxis::FoodAffinity, f_norm, score);
        put(SalienceAxis::SocialTrust, st_norm, score);
        put(SalienceAxis::LambdaTokenDensity, l_norm, score);
    }

    /// Encode `(world_pos, ctx.belief_embedding)` into a 32-D row suitable
    /// for the per-axis KANs. The fold is :
    ///
    ///   row[i] = belief[i] + γ * (Σ_d ψ_i(d) · world_pos[d])
    ///
    /// where ψ_i is a deterministic per-lane spatial-mix coefficient and
    /// γ is the spatial-gain. The coefficients ψ are STATIC + COMPILE-TIME
    /// known so this is a pure function of (world_pos, belief).
    fn encode_row(&self, world_pos: &[f32; 3], ctx: &CompanionContext) -> [f32; BELIEF_DIM] {
        const SPATIAL_GAIN: f32 = 0.05;
        let mut row = ctx.belief_embedding;
        // § ψ_i(d) is a deterministic small-magnitude coefficient. Using
        //   a 32×3 fixed table would be more accurate but heavier ; the
        //   sin-fold below is bit-exact across hosts (libm sinf is
        //   IEEE-754 specified).
        for (i, lane) in row.iter_mut().enumerate() {
            let phase = (i as f32) * 0.1234_f32;
            // Each lane gets a distinct linear combination of (x, y, z).
            let psi_x = (phase + 0.0).sin();
            let psi_y = (phase + 1.5708).sin(); // +π/2
            let psi_z = (phase + 3.1415).sin(); // +π
            let mix = psi_x * world_pos[0] + psi_y * world_pos[1] + psi_z * world_pos[2];
            *lane += SPATIAL_GAIN * mix;
        }
        row
    }
}

impl Default for SemanticSalienceEvaluator {
    fn default() -> Self {
        Self::new_untrained()
    }
}

/// § Deterministic synthetic-norm : sum-of-abs over a slice, clamped to
///   [0, 1] via a sigmoid-like saturation.
fn synthetic_norm(slice: &[f32]) -> f32 {
    let mut acc = 0.0_f32;
    for v in slice {
        acc += v.abs();
    }
    // § soft-saturation : x / (1 + x) maps [0, ∞) → [0, 1) bijectively.
    acc / (1.0 + acc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companion_context::CompanionEmotion;

    #[test]
    fn salience_axes_count_is_five() {
        assert_eq!(SALIENCE_AXES, 5);
        assert_eq!(SalienceAxis::ALL.len(), SALIENCE_AXES);
    }

    #[test]
    fn salience_axes_are_distinct_indices() {
        let mut idxs: Vec<u8> = SalienceAxis::ALL.iter().map(|a| *a as u8).collect();
        idxs.sort_unstable();
        assert_eq!(idxs, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn zero_score_is_well_formed() {
        let s = SalienceScore::zero();
        assert!(s.is_well_formed());
        assert_eq!(s.magnitude(), 0.0);
    }

    #[test]
    fn dominant_picks_largest_axis() {
        let s = SalienceScore::new([0.1, 0.5, 0.2, 0.9, 0.3]);
        assert_eq!(s.dominant(), SalienceAxis::SocialTrust);
    }

    #[test]
    fn dominant_breaks_ties_to_first() {
        let s = SalienceScore::new([0.5, 0.5, 0.0, 0.0, 0.0]);
        assert_eq!(s.dominant(), SalienceAxis::Salience);
    }

    #[test]
    fn saturated_clamps_axes() {
        let s = SalienceScore::new([2.0, -0.1, 0.5, 1.5, 0.0]).saturated();
        assert_eq!(s.at(SalienceAxis::Salience), 1.0);
        assert_eq!(s.at(SalienceAxis::Threat), 0.0);
        assert_eq!(s.at(SalienceAxis::FoodAffinity), 0.5);
        assert_eq!(s.at(SalienceAxis::SocialTrust), 1.0);
    }

    #[test]
    fn magnitude_is_axis_mean() {
        let s = SalienceScore::new([0.2, 0.4, 0.6, 0.8, 0.0]);
        let expected = (0.2 + 0.4 + 0.6 + 0.8 + 0.0) / 5.0;
        assert!((s.magnitude() - expected).abs() < 1e-6);
    }

    #[test]
    fn evaluator_starts_untrained() {
        let e = SemanticSalienceEvaluator::new_untrained();
        assert!(!e.is_trained());
    }

    #[test]
    fn evaluate_neutral_context_at_origin_is_well_formed() {
        let e = SemanticSalienceEvaluator::new_untrained();
        let ctx = CompanionContext::neutral();
        let s = e.evaluate(&[0.0, 0.0, 0.0], &ctx);
        assert!(s.is_well_formed());
    }

    #[test]
    fn evaluator_is_deterministic_across_calls() {
        let e = SemanticSalienceEvaluator::new_untrained();
        let mut ctx = CompanionContext::neutral();
        ctx.belief_embedding[3] = 0.5;
        ctx.belief_embedding[7] = -0.3;
        let s1 = e.evaluate(&[1.0, 2.0, 3.0], &ctx);
        let s2 = e.evaluate(&[1.0, 2.0, 3.0], &ctx);
        assert_eq!(s1.axes, s2.axes);
    }

    #[test]
    fn evaluator_changes_with_position() {
        let e = SemanticSalienceEvaluator::new_untrained();
        let mut ctx = CompanionContext::neutral();
        ctx.belief_embedding[1] = 0.7;
        let s_a = e.evaluate(&[0.0, 0.0, 0.0], &ctx);
        let s_b = e.evaluate(&[10.0, 0.0, 0.0], &ctx);
        assert_ne!(s_a.axes, s_b.axes);
    }

    #[test]
    fn evaluator_changes_with_belief() {
        let e = SemanticSalienceEvaluator::new_untrained();
        let mut a = CompanionContext::neutral();
        let mut b = CompanionContext::neutral();
        a.belief_embedding[5] = 0.0;
        b.belief_embedding[5] = 0.7;
        let s_a = e.evaluate(&[1.0, 1.0, 1.0], &a);
        let s_b = e.evaluate(&[1.0, 1.0, 1.0], &b);
        assert_ne!(s_a.axes, s_b.axes);
    }

    #[test]
    fn evaluator_attention_falloff_zeros_far_cells() {
        let e = SemanticSalienceEvaluator::new_untrained();
        let mut ctx = CompanionContext::neutral();
        ctx.attention_target = Some([0.0, 0.0, 0.0]);
        ctx.attention_radius = 0.5;
        ctx.belief_embedding[0] = 1.0;
        let s_near = e.evaluate(&[0.1, 0.1, 0.1], &ctx);
        let s_far = e.evaluate(&[100.0, 100.0, 100.0], &ctx);
        // § Far cell should have substantially lower salience than the near
        //   cell once the inverse-quadratic falloff applies.
        assert!(s_far.magnitude() < s_near.magnitude());
    }

    #[test]
    fn anxious_companion_emphasises_threat_axis() {
        let e = SemanticSalienceEvaluator::new_untrained();
        let mut neutral = CompanionContext::neutral();
        // Avoid null-row : non-zero belief so synthetic-norms are non-zero.
        for i in 0..BELIEF_DIM {
            neutral.belief_embedding[i] = 0.1 * (i as f32 + 1.0);
        }
        let mut anxious = neutral.clone();
        anxious.emotion = CompanionEmotion {
            curious: 0.0,
            anxious: 1.0,
            content: 0.0,
            alert: 0.0,
        };
        let s_neutral = e.evaluate(&[1.0, 2.0, 3.0], &neutral);
        let s_anxious = e.evaluate(&[1.0, 2.0, 3.0], &anxious);
        // Anxious should produce a strictly-higher Threat axis (subject to
        // saturation cap).
        assert!(
            s_anxious.at(SalienceAxis::Threat) > s_neutral.at(SalienceAxis::Threat)
                || s_anxious.at(SalienceAxis::Threat) >= 0.89,
            "anxiety did not boost threat axis : neutral={}, anxious={}",
            s_neutral.at(SalienceAxis::Threat),
            s_anxious.at(SalienceAxis::Threat)
        );
    }

    #[test]
    fn synthetic_norm_is_bounded_unit() {
        // Empty slice ⇒ 0 / 1 = 0
        assert_eq!(synthetic_norm(&[]), 0.0);
        // Large input ⇒ saturates < 1
        let big = [1e6_f32; 8];
        let s = synthetic_norm(&big);
        assert!(s < 1.0 && s > 0.99);
    }

    #[test]
    fn salience_score_at_mut_round_trips() {
        let mut s = SalienceScore::zero();
        *s.at_mut(SalienceAxis::SocialTrust) = 0.42;
        assert_eq!(s.at(SalienceAxis::SocialTrust), 0.42);
    }

    #[test]
    fn nan_score_is_not_well_formed() {
        let s = SalienceScore::new([f32::NAN, 0.0, 0.0, 0.0, 0.0]);
        assert!(!s.is_well_formed());
    }
}
