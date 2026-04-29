//! § SalienceVisualization — salience-tensor → glow-edges + fade + warmth
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Maps the raw [`SalienceScore`] (5-axis cognitive projection) into a
//!   set of rendering parameters the host backend uses to drive the
//!   companion-view's pixel-shader. The mapping is INTENTIONALLY UNFAITHFUL
//!   to visible-light : the companion's perspective is NOT supposed to
//!   look like the geometry-render with extra annotations.
//!
//!     SalienceScore ──→ {glow_edge, fade_factor, palette_warmth, tint_color}
//!
//! § SPEC ANCHORS
//!   - `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.5(a)` :
//!     "color-mapping : KAN @ companion-emotion → palette-shift
//!      {curious=violet-blue, anxious=red-tinted, content=warm-gold}"
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-8` :
//!     "render-as subtle-saturation-shift (NOT obtrusive ⊗ companion-led)"
//!     "N! AI-vision-mode-with-radar-hud cliché"
//!
//! § DIEGETIC DISCIPLINE
//!   - NO HUD overlays. NO icons-on-creatures. NO crosshair.
//!   - Saturation-shifts ONLY ; warmth-tints ONLY ; glow-edges that are
//!     "of the world", not "ontop of the world".
//!   - The companion KNOWS the player is watching ; this is in-fiction
//!     diegetic, so the visualization style is "companion-led" not
//!     "instrumental".
//!
//! § PALETTE-MAP TABLE
//!
//!   ```text
//!   axis-dominant       | tint               | rationale
//!   --------------------+--------------------+--------------------------
//!   Salience            | neutral-warm       | "this matters"
//!   Threat              | red-tinted         | spec § V.5(a)
//!   FoodAffinity        | golden             | spec § V.5(a) (content=warm-gold)
//!   SocialTrust         | warm-amber         | "trustworthy presence"
//!   LambdaTokenDensity  | violet-indigo      | spec § V.5(a) (curious=violet-blue)
//!   ```
//!
//! § GLOW-EDGE THRESHOLD
//!   Cells whose magnitude falls below [`GLOW_EDGE_THRESHOLD`] do not emit
//!   a glow-edge — they fade. This is what gives the companion-view its
//!   characteristic "high-information sparseness" : most of the world
//!   fades, only attended regions glow.
//!
//! § PRIME-DIRECTIVE
//!   The visualization layer NEVER emits a "telemetry pixel" or hidden
//!   marker. Every glow + every tint is in the visible RGB buffer ; the
//!   player can see exactly what the companion is sharing. The audit
//!   chain records the PALETTE choice + the threshold parameters per
//!   frame so a third-party auditor can replay the visualization.

use crate::companion_context::CompanionEmotion;
use crate::salience_evaluator::{SalienceAxis, SalienceScore};

/// Below this magnitude, a cell does NOT emit a glow-edge — it fades.
/// Tunable per-companion-archetype ; default is conservative.
pub const GLOW_EDGE_THRESHOLD: f32 = 0.18;

/// Default fade-floor : cells below threshold fade-to-black (or fade-to-
/// neutral-grey in the production path) with floor `FADE_FLOOR`. Setting
/// this to 0 = full black ; to 0.05 = slight grey-tint to retain spatial
/// orientation.
pub const FADE_FLOOR: f32 = 0.05;

/// Palette-warmth scalar : derived from the companion's emotion + the
/// dominant salience-axis. Range is [-1.0 = cool-blue, +1.0 = warm-gold].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PaletteWarmth(pub f32);

impl PaletteWarmth {
    /// Neutral baseline (no warmth-shift).
    pub const NEUTRAL: PaletteWarmth = PaletteWarmth(0.0);
    /// Maximum warmth (fully gold).
    pub const MAX_WARM: PaletteWarmth = PaletteWarmth(1.0);
    /// Maximum coolness (fully cyan-blue).
    pub const MAX_COOL: PaletteWarmth = PaletteWarmth(-1.0);

    /// Saturate-clamp to [-1, 1].
    #[must_use]
    pub fn saturated(self) -> Self {
        PaletteWarmth(self.0.clamp(-1.0, 1.0))
    }

    /// True iff finite + within [-1, 1] (within 1e-4 slack).
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.0.is_finite() && (-1.0 - 1e-4..=1.0 + 1e-4).contains(&self.0)
    }
}

/// Per-cell visualization parameters. The host backend reads these and
/// mixes them into the companion-view's pixel-shader output.
///
/// § FIELDS
///   - `glow_edge` : intensity of the glow-edge ∈ [0, 1]. Below the
///     `GLOW_EDGE_THRESHOLD` this is forced to 0 — the cell fades.
///   - `fade_factor` : 1.0 = fully visible, 0.0 = fully faded.
///   - `warmth` : palette-warmth ∈ [-1, 1].
///   - `tint_rgb` : per-axis RGB tint contribution. The host backend
///     sums these contributions per-cell to produce the final output
///     pixel ; the visualization layer DOES NOT itself emit RGB so the
///     downstream tonemap stage can re-balance for HDR.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualizationParams {
    /// Glow-edge intensity ∈ [0, 1].
    pub glow_edge: f32,
    /// Fade factor ∈ [0, 1] : 1 = full presence, 0 = invisible.
    pub fade_factor: f32,
    /// Warmth ∈ [-1, 1].
    pub warmth: PaletteWarmth,
    /// Per-channel RGB tint ∈ [0, 1]³.
    pub tint_rgb: [f32; 3],
}

impl VisualizationParams {
    /// "Faded" : no glow + minimum visibility. Used as the visualization
    /// for cells below the glow threshold.
    #[must_use]
    pub fn faded() -> Self {
        Self {
            glow_edge: 0.0,
            fade_factor: FADE_FLOOR,
            warmth: PaletteWarmth::NEUTRAL,
            tint_rgb: [FADE_FLOOR, FADE_FLOOR, FADE_FLOOR],
        }
    }

    /// True iff every axis is finite + within range.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.glow_edge.is_finite()
            && (0.0..=1.0 + 1e-4).contains(&self.glow_edge)
            && self.fade_factor.is_finite()
            && (0.0..=1.0 + 1e-4).contains(&self.fade_factor)
            && self.warmth.is_well_formed()
            && self
                .tint_rgb
                .iter()
                .all(|c| c.is_finite() && (0.0..=1.0 + 1e-4).contains(c))
    }
}

/// The visualization mapper. Stateless pure transform ;
/// `(SalienceScore, CompanionEmotion) → VisualizationParams`.
#[derive(Debug, Clone, Copy)]
pub struct SalienceVisualization {
    /// Glow-edge threshold (cells with magnitude below this fade).
    pub glow_threshold: f32,
    /// Fade-floor for below-threshold cells.
    pub fade_floor: f32,
}

impl SalienceVisualization {
    /// Construct with the canonical default thresholds.
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            glow_threshold: GLOW_EDGE_THRESHOLD,
            fade_floor: FADE_FLOOR,
        }
    }

    /// Custom thresholds. Saturate-clamps inputs to legal ranges.
    #[must_use]
    pub fn with_thresholds(glow_threshold: f32, fade_floor: f32) -> Self {
        Self {
            glow_threshold: glow_threshold.clamp(0.0, 1.0),
            fade_floor: fade_floor.clamp(0.0, 1.0),
        }
    }

    /// Map a (salience, emotion) pair to visualization parameters.
    ///
    /// § DETERMINISM
    ///   Pure function. Same inputs ⇒ identical outputs across hosts.
    #[must_use]
    pub fn map(&self, score: &SalienceScore, emotion: &CompanionEmotion) -> VisualizationParams {
        let magnitude = score.magnitude();
        if magnitude < self.glow_threshold {
            // § Below-threshold : cell fades. Visualization-discipline
            //   says these cells should still be DIMLY visible so the
            //   player retains spatial orientation.
            return VisualizationParams {
                glow_edge: 0.0,
                fade_factor: self.fade_floor,
                warmth: self.warmth_for_emotion(emotion),
                tint_rgb: [self.fade_floor; 3],
            };
        }
        let dominant = score.dominant();
        let tint_rgb = Self::axis_tint(dominant);
        let warmth = self.warmth_for_emotion(emotion);
        VisualizationParams {
            glow_edge: magnitude.min(1.0),
            fade_factor: 1.0,
            warmth,
            tint_rgb,
        }
    }

    /// The canonical per-axis tint table. Spec § V.5(a) constrains the
    /// emotion-tint ; the per-axis salience-tint is derived consistently :
    /// each axis maps to a hue-region that does not collide with the
    /// emotion-tints, so the player's eye can read both layers.
    #[must_use]
    pub fn axis_tint(axis: SalienceAxis) -> [f32; 3] {
        match axis {
            // "this matters" — neutral-warm (cream).
            SalienceAxis::Salience => [0.95, 0.90, 0.78],
            // Threat — red-tinted per spec.
            SalienceAxis::Threat => [0.90, 0.20, 0.20],
            // FoodAffinity — golden (warm-yellow-orange).
            SalienceAxis::FoodAffinity => [0.95, 0.78, 0.30],
            // SocialTrust — warm-amber.
            SalienceAxis::SocialTrust => [0.85, 0.65, 0.45],
            // LambdaTokenDensity — violet-indigo (curious-violet per spec).
            SalienceAxis::LambdaTokenDensity => [0.45, 0.35, 0.85],
        }
    }

    /// Warmth from emotion. Per spec § V.5(a) :
    ///     curious  → violet-blue (cool)
    ///     anxious  → red-tinted (warm but in a stress-direction)
    ///     content  → warm-gold
    ///
    /// The single warmth-scalar collapses all four emotion-axes into a
    /// linear warmth value. The host shader uses this scalar to bias the
    /// chromatic-adaptation matrix in the tonemap stage.
    #[must_use]
    pub fn warmth_for_emotion(&self, emotion: &CompanionEmotion) -> PaletteWarmth {
        // Curiosity = cool (negative warmth). Spec : curious → violet-blue.
        // Anxiety = mild warmth (red-tinted). Content = strong warmth (gold).
        // Alert = neutral.
        let raw = -emotion.curious + 0.5 * emotion.anxious + emotion.content;
        PaletteWarmth(raw.clamp(-1.0, 1.0))
    }
}

impl Default for SalienceVisualization {
    fn default() -> Self {
        Self::canonical()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companion_context::CompanionEmotion;
    use crate::salience_evaluator::SALIENCE_AXES;

    fn emotion(curious: f32, anxious: f32, content: f32, alert: f32) -> CompanionEmotion {
        CompanionEmotion {
            curious,
            anxious,
            content,
            alert,
        }
    }

    #[test]
    fn canonical_thresholds_are_spec_defaults() {
        let v = SalienceVisualization::canonical();
        assert_eq!(v.glow_threshold, GLOW_EDGE_THRESHOLD);
        assert_eq!(v.fade_floor, FADE_FLOOR);
    }

    #[test]
    fn faded_below_threshold() {
        let v = SalienceVisualization::canonical();
        let dim = SalienceScore::new([0.05, 0.05, 0.05, 0.05, 0.05]);
        let p = v.map(&dim, &emotion(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.glow_edge, 0.0);
        assert_eq!(p.fade_factor, FADE_FLOOR);
        assert!(p.is_well_formed());
    }

    /// § Score-mix that gives one axis dominance + magnitude clearly above
    ///   the canonical glow-threshold. Mean-of-axes is the magnitude metric ;
    ///   mean = 0.5 here (single 0.9 + four 0.4 = 2.5 / 5 = 0.5) so this is
    ///   well above the 0.18 threshold while still having axis-0 dominant.
    fn mostly_axis(axis: SalienceAxis, peak: f32, base: f32) -> SalienceScore {
        let mut s = SalienceScore::new([base; SALIENCE_AXES]);
        *s.at_mut(axis) = peak;
        s
    }

    #[test]
    fn glow_above_threshold() {
        let v = SalienceVisualization::canonical();
        let bright = mostly_axis(SalienceAxis::Salience, 0.9, 0.4);
        let p = v.map(&bright, &emotion(0.0, 0.0, 0.0, 0.0));
        assert!(p.glow_edge > 0.0);
        assert_eq!(p.fade_factor, 1.0);
    }

    #[test]
    fn dominant_threat_picks_red_tint() {
        let v = SalienceVisualization::canonical();
        let threat = mostly_axis(SalienceAxis::Threat, 0.9, 0.4);
        let p = v.map(&threat, &emotion(0.0, 0.0, 0.0, 0.0));
        // Red-tint : R is dominant.
        assert!(p.tint_rgb[0] > p.tint_rgb[1]);
        assert!(p.tint_rgb[0] > p.tint_rgb[2]);
    }

    #[test]
    fn dominant_lambda_picks_violet_tint() {
        let v = SalienceVisualization::canonical();
        let lam = mostly_axis(SalienceAxis::LambdaTokenDensity, 0.9, 0.4);
        let p = v.map(&lam, &emotion(0.0, 0.0, 0.0, 0.0));
        // Violet : B dominates ; G < R.
        assert!(p.tint_rgb[2] > p.tint_rgb[0]);
        assert!(p.tint_rgb[2] > p.tint_rgb[1]);
    }

    #[test]
    fn dominant_food_picks_golden_tint() {
        let v = SalienceVisualization::canonical();
        let food = mostly_axis(SalienceAxis::FoodAffinity, 0.9, 0.4);
        let p = v.map(&food, &emotion(0.0, 0.0, 0.0, 0.0));
        // Golden : R + G > B.
        assert!(p.tint_rgb[0] > p.tint_rgb[2]);
        assert!(p.tint_rgb[1] > p.tint_rgb[2]);
    }

    #[test]
    fn curious_emotion_yields_cool_warmth() {
        let v = SalienceVisualization::canonical();
        let bright = mostly_axis(SalienceAxis::Salience, 0.6, 0.4);
        let p = v.map(&bright, &emotion(1.0, 0.0, 0.0, 0.0));
        assert!(p.warmth.0 < 0.0);
    }

    #[test]
    fn content_emotion_yields_warm_warmth() {
        let v = SalienceVisualization::canonical();
        let bright = mostly_axis(SalienceAxis::Salience, 0.6, 0.4);
        let p = v.map(&bright, &emotion(0.0, 0.0, 1.0, 0.0));
        assert!(p.warmth.0 > 0.0);
    }

    #[test]
    fn neutral_emotion_yields_neutral_warmth() {
        let v = SalienceVisualization::canonical();
        let bright = mostly_axis(SalienceAxis::Salience, 0.6, 0.4);
        let p = v.map(&bright, &emotion(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.warmth.0, 0.0);
    }

    #[test]
    fn warmth_is_saturated_to_unit() {
        let v = SalienceVisualization::canonical();
        // Construct a deliberately oversum emotion (would otherwise
        // produce a warmth > 1.0 if unclamped).
        let pathological = CompanionEmotion {
            curious: 0.0,
            anxious: 1.0,
            content: 1.0,
            alert: 0.0,
        };
        let bright = mostly_axis(SalienceAxis::Salience, 0.6, 0.4);
        let p = v.map(&bright, &pathological);
        assert!(p.warmth.0 <= 1.0);
        assert!(p.warmth.is_well_formed());
    }

    #[test]
    fn faded_visualization_is_well_formed() {
        assert!(VisualizationParams::faded().is_well_formed());
    }

    #[test]
    fn custom_thresholds_round_trip() {
        let v = SalienceVisualization::with_thresholds(0.3, 0.1);
        assert_eq!(v.glow_threshold, 0.3);
        assert_eq!(v.fade_floor, 0.1);
    }

    #[test]
    fn custom_thresholds_are_clamped() {
        let v = SalienceVisualization::with_thresholds(2.0, -0.5);
        assert_eq!(v.glow_threshold, 1.0);
        assert_eq!(v.fade_floor, 0.0);
    }

    #[test]
    fn axis_tint_is_distinct_per_axis() {
        let mut tints = std::collections::HashSet::new();
        for axis in SalienceAxis::ALL {
            // Quantize so we are robust to small float differences.
            let t = SalienceVisualization::axis_tint(axis);
            let key = (
                (t[0] * 100.0) as i32,
                (t[1] * 100.0) as i32,
                (t[2] * 100.0) as i32,
            );
            assert!(tints.insert(key), "axis {axis:?} tint collides");
        }
        assert_eq!(tints.len(), SALIENCE_AXES);
    }

    #[test]
    fn faded_below_threshold_is_well_formed() {
        let v = SalienceVisualization::canonical();
        let zero = SalienceScore::zero();
        let p = v.map(&zero, &emotion(0.5, 0.0, 0.0, 0.0));
        assert!(p.is_well_formed());
    }
}
