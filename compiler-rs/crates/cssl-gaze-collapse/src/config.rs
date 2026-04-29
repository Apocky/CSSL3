//! Configuration for the gaze-collapse pass.
//!
//! § DESIGN
//!   The configuration is **opt-in by default-OFF** per the V.4 spec :
//!     "‼ R! eye-tracking opt-IN-explicit ⊗ N! opt-out-default"
//!   Constructing a `GazeCollapseConfig` with `opt_in == false` puts the
//!   pass in [`FoveationFallback::CenterBias`] mode where no eye-tracking
//!   data ever flows through this crate's data path. The
//!   `GazeCollapseConfig::default` returns this safe state.
//!
//!   The prediction-horizon governs how-far-ahead the saccadic-prediction
//!   reaches : 3–5 ms is typical at 90 Hz, with hard-cap at 8 ms (anything
//!   longer is unreliable for human-saccade physiology, which has a
//!   typical ballistic-phase of 30–100 ms but the prediction here only
//!   needs to span the inter-frame gap).
//!
//!   The KAN-budget-coefficient bands map FoveaResolution → KAN-evaluation-
//!   depth ; foveal gets full-depth (depth=8) ; mid gets half (depth=4) ;
//!   peripheral gets coarse (depth=2). These are exposed so callers with
//!   tighter frame-budgets (e.g. Quest-3 at 0.3 ms vs Vision-Pro at 0.25 ms)
//!   can throttle the depth without re-architecting the pass.

use crate::error::GazeCollapseError;

/// Top-level configuration for the gaze-collapse pass.
#[derive(Debug, Clone, PartialEq)]
pub struct GazeCollapseConfig {
    /// Per-spec opt-IN flag. `false` = no gaze-data flows. Default: `false`.
    pub opt_in: bool,
    /// What to do when consent is denied (or `opt_in == false`).
    pub fallback: FoveationFallback,
    /// How far ahead saccade-prediction reaches.
    pub prediction_horizon: PredictionHorizon,
    /// Per-region KAN-evaluation-depth coefficients.
    pub budget: BudgetCoefficients,
    /// Strict-mode : reject center-bias fallback when consent revoked
    /// mid-frame ; the caller must handle the error explicitly. Default
    /// `false` because the spec-default is to fall back gracefully.
    pub strict_mode: bool,
    /// Diagnostic-overlay mode : the [`crate::pass::GazeCollapsePass`] emits
    /// fovea-circle visualization to the FoveaMask debug-channel so the
    /// player is aware of the collapse-region. Per spec § V.4(d) :
    /// "transparency : R! UI shows-fovea-circle in debug-mode".
    pub diagnostic_overlay: bool,
    /// Render-target screen-space resolution (per-eye).
    pub render_target_width: u32,
    /// Render-target screen-space resolution (per-eye).
    pub render_target_height: u32,
}

impl Default for GazeCollapseConfig {
    fn default() -> Self {
        Self {
            opt_in: false,
            fallback: FoveationFallback::CenterBias,
            prediction_horizon: PredictionHorizon::default(),
            budget: BudgetCoefficients::default(),
            strict_mode: false,
            diagnostic_overlay: false,
            render_target_width: 1832,  // Quest-3 per-eye native
            render_target_height: 1920, // Quest-3 per-eye native
        }
    }
}

impl GazeCollapseConfig {
    /// Convenience builder : fully-enabled with Quest-3 defaults.
    /// **Caller must verify the user has explicitly opted in** ; this
    /// function is a builder, not a consent-bypass.
    #[must_use]
    pub fn quest3_opted_in() -> Self {
        Self {
            opt_in: true,
            fallback: FoveationFallback::CenterBias,
            prediction_horizon: PredictionHorizon::default(),
            budget: BudgetCoefficients::quest3(),
            strict_mode: false,
            diagnostic_overlay: false,
            render_target_width: 1832,
            render_target_height: 1920,
        }
    }

    /// Convenience builder : fully-enabled with Vision-Pro defaults.
    /// **Caller must verify the user has explicitly opted in.**
    #[must_use]
    pub fn vision_pro_opted_in() -> Self {
        Self {
            opt_in: true,
            fallback: FoveationFallback::CenterBias,
            prediction_horizon: PredictionHorizon::default(),
            budget: BudgetCoefficients::vision_pro(),
            strict_mode: false,
            diagnostic_overlay: false,
            render_target_width: 3660, // Vision-Pro per-eye native (approx)
            render_target_height: 3142, // Vision-Pro per-eye native (approx)
        }
    }

    /// Validate the configuration ; returns the first detected error.
    pub fn validate(&self) -> Result<(), GazeCollapseError> {
        self.prediction_horizon.validate()?;
        self.budget.validate()?;
        if self.render_target_width == 0 || self.render_target_height == 0 {
            return Err(GazeCollapseError::InvalidGazeInput {
                field: "render_target_dims",
                value: format!("{}×{}", self.render_target_width, self.render_target_height),
            });
        }
        Ok(())
    }
}

/// Foveation behavior when gaze data is unavailable (consent denied,
/// `opt_in == false`, or hardware-failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FoveationFallback {
    /// No-foveation : every pixel is full-detail. Falls back to the
    /// non-foveated render path. Highest quality, lowest performance.
    None,
    /// Center-bias : a static fovea-region anchored at the screen-center
    /// with the same shading-rate-decay as the gaze-tracked variant. The
    /// spec-canonical fallback per V.4(d) : "fallback : center-bias-
    /// foveation".
    CenterBias,
    /// Last-known-gaze : reuse the most-recent confidence-thresholded
    /// gaze position. Used internally when a single-frame confidence
    /// drop occurs ; not selectable as a long-term fallback because it
    /// would require gaze-state retention beyond the current frame.
    LastKnownGaze,
}

/// Prediction-horizon for the saccadic-predictor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PredictionHorizon {
    /// Milliseconds-ahead. Range [1, 8].
    pub millis: u8,
}

impl Default for PredictionHorizon {
    fn default() -> Self {
        Self { millis: 4 }
    }
}

impl PredictionHorizon {
    /// Construct a prediction horizon ; validates the range eagerly.
    pub fn new(millis: u8) -> Result<Self, GazeCollapseError> {
        let h = Self { millis };
        h.validate()?;
        Ok(h)
    }

    /// Validate the range without consuming.
    pub fn validate(&self) -> Result<(), GazeCollapseError> {
        if self.millis == 0 || self.millis > 8 {
            return Err(GazeCollapseError::PredictionHorizonOutOfRange(self.millis));
        }
        Ok(())
    }
}

/// Per-region KAN-evaluation-depth coefficients.
///
/// Each value is in [0.0, 1.0] where 1.0 = full-depth (depth=8 spline-network),
/// 0.5 = half (depth=4), 0.25 = quarter (depth=2). These match the V.4 spec :
/// "foveal-region : detail-amplifier @ FULL-depth-eval (depth=8 KAN-layers)
///  para-foveal  : detail-amplifier @ HALF-depth-eval
///  peripheral   : detail-amplifier @ COARSE-only (depth=2)".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetCoefficients {
    /// Coefficient for the foveal (full-acuity) region.
    pub foveal: f32,
    /// Coefficient for the para-foveal (mid) region.
    pub para_foveal: f32,
    /// Coefficient for the peripheral (coarse) region.
    pub peripheral: f32,
}

impl Default for BudgetCoefficients {
    fn default() -> Self {
        Self::quest3()
    }
}

impl BudgetCoefficients {
    /// Quest-3 default : full / half / coarse ratios per V.4 spec.
    #[must_use]
    pub const fn quest3() -> Self {
        Self {
            foveal: 1.0,
            para_foveal: 0.5,
            peripheral: 0.25,
        }
    }

    /// Vision-Pro default : higher peripheral retention (Vision-Pro has
    /// higher per-eye pixel-density so the peripheral budget can be
    /// pulled-down further without visible quality loss).
    #[must_use]
    pub const fn vision_pro() -> Self {
        Self {
            foveal: 1.0,
            para_foveal: 0.4,
            peripheral: 0.2,
        }
    }

    /// Validate that all coefficients are in [0.0, 1.0] and that the
    /// monotonic ordering foveal ≥ para_foveal ≥ peripheral holds.
    pub fn validate(&self) -> Result<(), GazeCollapseError> {
        for (name, v) in [
            ("foveal", self.foveal),
            ("para_foveal", self.para_foveal),
            ("peripheral", self.peripheral),
        ] {
            if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                return Err(GazeCollapseError::InvalidGazeInput {
                    field: "budget",
                    value: format!("{}={}", name, v),
                });
            }
        }
        if self.foveal < self.para_foveal || self.para_foveal < self.peripheral {
            return Err(GazeCollapseError::InvalidGazeInput {
                field: "budget.monotonicity",
                value: format!(
                    "foveal={}, para_foveal={}, peripheral={}",
                    self.foveal, self.para_foveal, self.peripheral
                ),
            });
        }
        Ok(())
    }

    /// Coefficient for a region given its FoveaResolution class.
    #[must_use]
    pub fn for_resolution(&self, res: crate::fovea_mask::FoveaResolution) -> f32 {
        use crate::fovea_mask::FoveaResolution::{Full, Half, Quarter};
        match res {
            Full => self.foveal,
            Half => self.para_foveal,
            Quarter => self.peripheral,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BudgetCoefficients, FoveationFallback, GazeCollapseConfig, PredictionHorizon};

    #[test]
    fn default_is_safe_opt_out() {
        let cfg = GazeCollapseConfig::default();
        assert!(!cfg.opt_in);
        assert_eq!(cfg.fallback, FoveationFallback::CenterBias);
    }

    #[test]
    fn quest3_opted_in_is_explicit() {
        let cfg = GazeCollapseConfig::quest3_opted_in();
        assert!(cfg.opt_in);
        assert_eq!(cfg.budget, BudgetCoefficients::quest3());
    }

    #[test]
    fn vision_pro_opted_in_uses_vp_budget() {
        let cfg = GazeCollapseConfig::vision_pro_opted_in();
        assert!(cfg.opt_in);
        assert_eq!(cfg.budget, BudgetCoefficients::vision_pro());
    }

    #[test]
    fn validate_passes_default() {
        assert!(GazeCollapseConfig::default().validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_render_dims() {
        let mut cfg = GazeCollapseConfig::default();
        cfg.render_target_width = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn prediction_horizon_default_is_4ms() {
        assert_eq!(PredictionHorizon::default().millis, 4);
    }

    #[test]
    fn prediction_horizon_rejects_zero() {
        assert!(PredictionHorizon::new(0).is_err());
    }

    #[test]
    fn prediction_horizon_rejects_over_8() {
        assert!(PredictionHorizon::new(9).is_err());
    }

    #[test]
    fn prediction_horizon_accepts_4() {
        assert!(PredictionHorizon::new(4).is_ok());
    }

    #[test]
    fn budget_default_quest3() {
        let b = BudgetCoefficients::default();
        assert!((b.foveal - 1.0).abs() < 1e-6);
        assert!((b.para_foveal - 0.5).abs() < 1e-6);
        assert!((b.peripheral - 0.25).abs() < 1e-6);
    }

    #[test]
    fn budget_validate_rejects_non_monotonic() {
        let bad = BudgetCoefficients {
            foveal: 0.3,
            para_foveal: 0.6,
            peripheral: 0.1,
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn budget_validate_rejects_out_of_range() {
        let bad = BudgetCoefficients {
            foveal: 1.5,
            para_foveal: 0.5,
            peripheral: 0.25,
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn budget_validate_rejects_nan() {
        let bad = BudgetCoefficients {
            foveal: f32::NAN,
            para_foveal: 0.5,
            peripheral: 0.25,
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn vision_pro_lower_peripheral_than_quest3() {
        let q = BudgetCoefficients::quest3();
        let v = BudgetCoefficients::vision_pro();
        assert!(v.peripheral < q.peripheral);
    }
}
