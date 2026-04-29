//! § CompanionContext — the companion's belief-state + emotion + attention.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The input contract that connects the companion's active-inference
//!   cognitive engine (lives in `05_INTELLIGENCE/*`, separate slice) to the
//!   Stage-8 render-pass. The companion-perspective render reads its
//!   `salience` axes from this context ; nothing else upstream of Stage-8
//!   reads the cognitive engine directly.
//!
//! § SPEC ANCHORS
//!   - `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.5(c)` :
//!     "companion-Sovereign carries Φ-fingerprint + active-inference belief-
//!     state (Axiom 4 §VI)". The 32-D belief-embedding is the canonical
//!     input to π_companion.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-8` :
//!     CompanionAI.Perception (active-inference belief-state) + .Attention
//!     (where-companion-looks).
//!   - `Omniverse/08_BODY/02_VR_EMBODIMENT.csl § VII` : Companion-archetype
//!     embodiment ; `attention_target: Option<Vec3>`.
//!
//! § DESIGN
//!   The context is INPUT-ONLY from the perspective of Stage-8. The pass
//!   reads it, projects it into salience-space via the KAN evaluator, and
//!   never mutates it. This makes the pass a PURE function of inputs which
//!   makes the toggle-OFF reverse perfect (`§5 reversibility`).
//!
//! § PRIME-DIRECTIVE
//!   The 32-D belief-embedding is the companion's INTERNAL cognitive state.
//!   It is NEVER exfiltrated outside Stage-8 — the only data that leaves
//!   this crate is the `CompanionView<2,16>` render buffer (palette-mapped
//!   salience), not the belief-state itself. The render-target view IS the
//!   companion's invitation to share ; the belief-state proper stays in
//!   05_INTELLIGENCE.

use crate::salience_evaluator::SalienceAxis;
use cssl_substrate_prime_directive::sigma::SigmaMaskPacked;

/// Dimensionality of the companion's belief-state embedding. 32-D matches
/// the `KanNetwork<32, _>` convention used in cssl-substrate-kan and the
/// `EMBEDDING_DIM` constant in the substrate-evolution spec.
pub const BELIEF_DIM: usize = 32;

/// Dimensionality of the emotion-axis embedding. 4-D : (curious, anxious,
/// content, alert). Each axis ∈ [0.0, 1.0] ; the four sum to ≤ 1 (the
/// companion may also be neutral on all axes).
pub const EMOTION_DIM: usize = 4;

/// Stable companion-identity handle. The same companion across sessions
/// has the same `CompanionId`. Sentinel `CompanionId::INVALID` (== 0) is
/// reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompanionId(pub u32);

impl CompanionId {
    /// "No companion is bound to this context." Used as the default before
    /// the cognitive engine has matched a session-companion to a
    /// CompanionContext.
    pub const INVALID: Self = Self(0);

    /// True iff this is the INVALID sentinel.
    #[must_use]
    pub fn is_invalid(self) -> bool {
        self.0 == 0
    }
}

/// The companion's discrete emotion-state, packed as 4 normalized axes.
/// Maps directly to the palette-shift function in `salience_visualization`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CompanionEmotion {
    /// Curiosity axis. 1.0 = strong curiosity ; warm-violet palette tint.
    pub curious: f32,
    /// Anxiety axis. 1.0 = strong anxiety ; red-tinted palette.
    pub anxious: f32,
    /// Contentment axis. 1.0 = relaxed-content ; warm-gold palette.
    pub content: f32,
    /// Alertness axis. 1.0 = high alert (vigilance) ; cool-cyan palette.
    pub alert: f32,
}

impl CompanionEmotion {
    /// Construct with all axes zeroed — neutral baseline.
    #[must_use]
    pub fn neutral() -> Self {
        Self::default()
    }

    /// True iff every axis ∈ [0.0, 1.0] AND the sum is ≤ 1.0 (allowing
    /// numerical slack of 1e-4 for accumulated rounding from the upstream
    /// inference engine).
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        let axes = [self.curious, self.anxious, self.content, self.alert];
        let mut sum = 0.0_f32;
        for a in axes {
            if !a.is_finite() || !(0.0..=1.0).contains(&a) {
                return false;
            }
            sum += a;
        }
        sum <= 1.0 + 1e-4
    }

    /// Pack as a 4-element array — useful for the palette KAN's input row.
    #[must_use]
    pub fn as_array(&self) -> [f32; EMOTION_DIM] {
        [self.curious, self.anxious, self.content, self.alert]
    }

    /// Saturate-clamp every axis to [0, 1]. Used as a defensive normalizer
    /// when ingesting raw outputs from the upstream inference engine.
    #[must_use]
    pub fn saturated(self) -> Self {
        Self {
            curious: self.curious.clamp(0.0, 1.0),
            anxious: self.anxious.clamp(0.0, 1.0),
            content: self.content.clamp(0.0, 1.0),
            alert: self.alert.clamp(0.0, 1.0),
        }
    }
}

/// The companion's belief-state + emotion + attention bundle. Read-only
/// input to the salience-evaluator.
///
/// § FIELD-RATIONALE
///   - `companion_id` — stable ; lets the audit-chain attribute consent
///     events to a specific companion across frames.
///   - `belief_embedding` — 32-D active-inference belief-state. Internal
///     to 05_INTELLIGENCE ; Stage-8 reads but does not interpret beyond
///     KAN-projection.
///   - `emotion` — 4-axis emotion ; drives palette.
///   - `attention_target` — optional world-position the companion is
///     attending to. None = unfocused / between attention-bouts.
///   - `attention_radius` — meters ; the companion's "focal distance".
///     Cells outside this radius receive a multiplicative falloff.
///   - `companion_sovereign_handle` — the SigmaMaskPacked Sovereign-handle
///     this companion claims. Used for cells where the companion holds
///     Sovereignty (their own body) — the salience-evaluator can read at
///     full resolution there. For cells where another Sovereign holds
///     claim, the consent-bits gate the read.
#[derive(Debug, Clone)]
pub struct CompanionContext {
    /// Stable identity of the companion this context describes.
    pub companion_id: CompanionId,
    /// 32-D active-inference belief-state. Read-only.
    pub belief_embedding: [f32; BELIEF_DIM],
    /// 4-axis emotion-state.
    pub emotion: CompanionEmotion,
    /// Optional focal point in world-coordinates.
    pub attention_target: Option<[f32; 3]>,
    /// Focal-radius in meters ; cells outside fall off.
    pub attention_radius: f32,
    /// The companion's sovereignty-handle — used to gate Σ-mask reads.
    pub companion_sovereign_handle: u16,
}

impl CompanionContext {
    /// Construct a neutral context. Useful as a test fixture and as the
    /// default when no companion is bound (paired with `CompanionId::INVALID`).
    #[must_use]
    pub fn neutral() -> Self {
        Self {
            companion_id: CompanionId::INVALID,
            belief_embedding: [0.0; BELIEF_DIM],
            emotion: CompanionEmotion::neutral(),
            attention_target: None,
            attention_radius: 0.0,
            companion_sovereign_handle: 0,
        }
    }

    /// True iff `companion_id` is a real (non-INVALID) sentinel value.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        !self.companion_id.is_invalid()
    }

    /// True iff `belief_embedding` contains only finite f32 values.
    #[must_use]
    pub fn belief_is_finite(&self) -> bool {
        self.belief_embedding.iter().all(|x| x.is_finite())
    }

    /// True iff `emotion` is well-formed AND `belief_is_finite`.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.emotion.is_well_formed()
            && self.belief_is_finite()
            && self.attention_radius >= 0.0
            && self.attention_radius.is_finite()
    }

    /// Compute the attention-falloff multiplier ∈ [0, 1] for a cell at
    /// `world_pos`. If the companion has no attention-target, returns 1.0
    /// (no falloff — uniform attention). Otherwise uses inverse-quadratic
    /// falloff with floor at the attention-radius.
    #[must_use]
    pub fn attention_falloff(&self, world_pos: &[f32; 3]) -> f32 {
        let target = match self.attention_target {
            Some(t) => t,
            None => return 1.0,
        };
        let dx = world_pos[0] - target[0];
        let dy = world_pos[1] - target[1];
        let dz = world_pos[2] - target[2];
        let dist_sq = dx * dx + dy * dy + dz * dz;
        let r = self.attention_radius.max(1e-3);
        let r_sq = r * r;
        if dist_sq <= r_sq {
            1.0
        } else {
            // § Inverse-quadratic falloff outside the focal radius. At
            //   2r distance, falloff ≈ 0.25 ; at 4r, ≈ 0.0625. This is
            //   gentle enough that peripheral salience is still visible.
            r_sq / dist_sq
        }
    }

    /// Compose a context with an `axis_weights` row (per-axis salience
    /// weights). Returns a length-`SALIENCE_AXES` array of base-weights.
    /// This is consulted by the SemanticSalienceEvaluator when projecting
    /// the belief-embedding through the per-axis KAN heads.
    #[must_use]
    pub fn axis_base_weights(&self) -> [f32; crate::salience_evaluator::SALIENCE_AXES] {
        // § Derive per-axis weights from the emotion-axes. The mapping is
        //   stable + deterministic ; it is the canonical way emotion
        //   modulates which-belief-axis-dominates.
        let mut w = [1.0_f32; crate::salience_evaluator::SALIENCE_AXES];
        // Curiosity boosts SALIENCE itself + LAMBDA_TOKEN_DENSITY (the
        // companion's interest in symbol-rich regions).
        w[SalienceAxis::Salience as usize] += self.emotion.curious;
        w[SalienceAxis::LambdaTokenDensity as usize] += self.emotion.curious;
        // Anxiety boosts THREAT.
        w[SalienceAxis::Threat as usize] += 2.0 * self.emotion.anxious;
        // Contentment boosts SOCIAL_TRUST.
        w[SalienceAxis::SocialTrust as usize] += self.emotion.content;
        // Alertness boosts THREAT (slightly) + SALIENCE.
        w[SalienceAxis::Threat as usize] += 0.5 * self.emotion.alert;
        w[SalienceAxis::Salience as usize] += 0.5 * self.emotion.alert;
        // Hunger / food-affinity is not directly tied to a single emotion
        // axis ; the inference engine is expected to have already weighted
        // food-affinity into the belief-embedding itself.
        w
    }

    /// True iff the companion holds Sovereignty over a cell with mask `mask`.
    /// The Σ-mask's `sovereignty_handle` is checked against the companion's
    /// `companion_sovereign_handle`.
    #[must_use]
    pub fn holds_sovereignty(&self, mask: &SigmaMaskPacked) -> bool {
        if self.companion_sovereign_handle == 0 {
            return false;
        }
        mask.sovereign_handle() == self.companion_sovereign_handle
    }
}

impl Default for CompanionContext {
    fn default() -> Self {
        Self::neutral()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_emotion_is_well_formed() {
        assert!(CompanionEmotion::neutral().is_well_formed());
    }

    #[test]
    fn emotion_with_negative_axis_is_malformed() {
        let bad = CompanionEmotion {
            curious: -0.1,
            ..CompanionEmotion::default()
        };
        assert!(!bad.is_well_formed());
    }

    #[test]
    fn emotion_with_oversum_is_malformed() {
        let bad = CompanionEmotion {
            curious: 0.6,
            anxious: 0.6,
            content: 0.0,
            alert: 0.0,
        };
        assert!(!bad.is_well_formed());
    }

    #[test]
    fn saturated_clamps_to_unit_range() {
        let raw = CompanionEmotion {
            curious: 2.0,
            anxious: -1.0,
            content: 0.5,
            alert: 0.99,
        };
        let s = raw.saturated();
        assert_eq!(s.curious, 1.0);
        assert_eq!(s.anxious, 0.0);
        assert_eq!(s.content, 0.5);
        assert!((s.alert - 0.99).abs() < 1e-6);
    }

    #[test]
    fn neutral_context_is_unbound() {
        let ctx = CompanionContext::neutral();
        assert!(!ctx.is_bound());
        assert!(ctx.belief_is_finite());
        assert!(ctx.is_well_formed());
    }

    #[test]
    fn bound_context_with_id() {
        let mut ctx = CompanionContext::neutral();
        ctx.companion_id = CompanionId(42);
        assert!(ctx.is_bound());
    }

    #[test]
    fn nan_belief_fails_finite_check() {
        let mut ctx = CompanionContext::neutral();
        ctx.belief_embedding[0] = f32::NAN;
        assert!(!ctx.belief_is_finite());
    }

    #[test]
    fn attention_falloff_no_target_is_unity() {
        let ctx = CompanionContext::neutral();
        let f = ctx.attention_falloff(&[100.0, 200.0, 300.0]);
        assert_eq!(f, 1.0);
    }

    #[test]
    fn attention_falloff_inside_radius_is_unity() {
        let mut ctx = CompanionContext::neutral();
        ctx.attention_target = Some([0.0, 0.0, 0.0]);
        ctx.attention_radius = 5.0;
        let f = ctx.attention_falloff(&[1.0, 1.0, 1.0]);
        assert_eq!(f, 1.0);
    }

    #[test]
    fn attention_falloff_outside_radius_diminishes() {
        let mut ctx = CompanionContext::neutral();
        ctx.attention_target = Some([0.0, 0.0, 0.0]);
        ctx.attention_radius = 1.0;
        let f_2r = ctx.attention_falloff(&[2.0, 0.0, 0.0]);
        let f_4r = ctx.attention_falloff(&[4.0, 0.0, 0.0]);
        // § Inverse-quadratic : at 2r, ≈ 0.25 ; at 4r, ≈ 0.0625
        assert!((f_2r - 0.25).abs() < 1e-3);
        assert!((f_4r - 0.0625).abs() < 1e-3);
    }

    #[test]
    fn anxiety_boosts_threat_weight() {
        let mut ctx = CompanionContext::neutral();
        ctx.emotion.anxious = 1.0;
        let w = ctx.axis_base_weights();
        assert!(w[SalienceAxis::Threat as usize] > 1.5);
    }

    #[test]
    fn curiosity_boosts_salience_and_lambda() {
        let mut ctx = CompanionContext::neutral();
        ctx.emotion.curious = 1.0;
        let w = ctx.axis_base_weights();
        assert!(w[SalienceAxis::Salience as usize] > 1.5);
        assert!(w[SalienceAxis::LambdaTokenDensity as usize] > 1.5);
    }

    #[test]
    fn contentment_boosts_trust() {
        let mut ctx = CompanionContext::neutral();
        ctx.emotion.content = 0.8;
        let w = ctx.axis_base_weights();
        assert!(w[SalienceAxis::SocialTrust as usize] > 1.0);
    }
}
