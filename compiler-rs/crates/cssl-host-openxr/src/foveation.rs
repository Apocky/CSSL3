//! Foveation : FFR + DFR + ML-foveated trait.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § V.
//!
//! § DESIGN
//!   - `FFRProfile` enum : `Off / Low / Medium / High / Aggressive`
//!     (§ V.A profile-by-default = High).
//!   - `Foveator` trait dispatches `FFRFoveator` (always-on baseline) +
//!     `DFRFoveator` (eye-tracked) + `MLFoveator` (5-yr stub).
//!   - `FoveationConfig` is the per-frame output : center-per-eye +
//!     periphery-rate-map.
//!   - `SaccadeEKF` : Extended-Kalman-Filter for gaze-prediction
//!     (§ V.B §VIII.C). The Conv-LSTM hybrid layer is a stub today ;
//!     the full impl lands when `cssl-substrate-kan` ships.

use crate::error::XRFailure;
use crate::ifc_shim::{Label, LabeledValue, SensitiveDomain};
use crate::view::ViewSet;

/// FFR-profile : how aggressive the periphery-rate-reduction is.
/// § V.A.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FFRProfile {
    /// Disabled — center 1/1 across the full frame.
    Off,
    /// Low — periphery 1/2 ; center-zone wide.
    Low,
    /// Medium — periphery 1/4 ; center-zone medium.
    Medium,
    /// High (default) — periphery 1/8 ; center-zone narrow.
    /// § V.A profile-by-default.
    High,
    /// Aggressive — periphery 1/16 ; center-zone very-narrow.
    /// Used by quality-degrade ladder (§ XII.A).
    Aggressive,
}

impl FFRProfile {
    /// Default profile per spec § V.A.
    #[must_use]
    pub const fn default_profile() -> Self {
        Self::High
    }

    /// Periphery shading-rate denominator (1/N pixels shaded).
    /// § V.A : aggressive @ 1/16 ; default @ 1/8.
    #[must_use]
    pub const fn periphery_rate_denominator(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::Low => 2,
            Self::Medium => 4,
            Self::High => 8,
            Self::Aggressive => 16,
        }
    }

    /// Center-zone normalized-radius (fraction of display half-extent
    /// covered by the full-rate center). Smaller = more aggressive
    /// periphery, less full-rate area.
    #[must_use]
    pub fn center_zone_radius(self) -> f32 {
        match self {
            Self::Off => 1.0,
            Self::Low => 0.65,
            Self::Medium => 0.50,
            Self::High => 0.35,
            Self::Aggressive => 0.20,
        }
    }

    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Aggressive => "aggressive",
        }
    }

    /// Tighten this profile by one step (used by quality-degrade ladder).
    /// `Off → Low → Medium → High → Aggressive → Aggressive` (saturate).
    #[must_use]
    pub const fn tighten(self) -> Self {
        match self {
            Self::Off => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High | Self::Aggressive => Self::Aggressive,
        }
    }
}

/// Per-eye foveation config carried out of `Foveator::config_for_frame`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoveationConfig {
    /// FFR profile to apply this frame.
    pub profile: FFRProfile,
    /// Per-eye fovea-center in NDC ([-1, +1]). Index = view_index.
    /// For DFR : updated per-frame from gaze-prediction.
    /// For FFR : fixed-center @ (0, 0).
    pub fovea_center_ndc: [[f32; 2]; crate::view::MAX_VIEWS],
    /// Number of populated eye-centers.
    pub view_count: u32,
    /// `true` iff DFR (eye-tracked) is engaged this frame.
    /// `false` = FFR-only.
    pub dfr_engaged: bool,
}

impl FoveationConfig {
    /// FFR-only default config : fovea-center at (0, 0) for every eye.
    #[must_use]
    pub fn ffr_only(view_count: u32, profile: FFRProfile) -> Self {
        Self {
            profile,
            fovea_center_ndc: [[0.0, 0.0]; crate::view::MAX_VIEWS],
            view_count,
            dfr_engaged: false,
        }
    }

    /// DFR-engaged config : fovea-center per-eye from gaze-prediction.
    #[must_use]
    pub fn dfr(view_count: u32, profile: FFRProfile, centers: &[[f32; 2]]) -> Self {
        let mut fovea_center_ndc = [[0.0; 2]; crate::view::MAX_VIEWS];
        for (i, c) in centers.iter().enumerate().take(crate::view::MAX_VIEWS) {
            fovea_center_ndc[i] = *c;
        }
        Self {
            profile,
            fovea_center_ndc,
            view_count,
            dfr_engaged: true,
        }
    }
}

/// Foveator-trait : dispatches `FFRFoveator` / `DFRFoveator` / `MLFoveator`.
///
/// § V (spec) :
///   trait Foveator {
///     fn config_for_frame(&mut self, view_set, gaze) -> FoveationConfig;
///   }
pub trait Foveator: Send + Sync + std::fmt::Debug {
    /// Return the per-frame foveation-config for `view_set` given the
    /// (optional) gaze-prediction. If `gaze` is `None`, FFR-only is
    /// returned. If `gaze` is `Some` and the foveator supports DFR,
    /// DFR-engaged is returned.
    ///
    /// § PRIME-DIRECTIVE §1 : the `gaze` argument is `LabeledValue`-typed
    /// at the boundary between this crate + the saccade-predictor. The
    /// foveator only consumes the `value` field on-device — never
    /// telemetry-egresses it.
    fn config_for_frame(
        &mut self,
        view_set: &ViewSet,
        gaze: Option<&LabeledValue<GazePrediction>>,
    ) -> FoveationConfig;

    /// `true` iff this foveator supports eye-tracked DFR.
    fn supports_dfr(&self) -> bool;

    /// Display-name.
    fn name(&self) -> &'static str;
}

/// Baseline always-on FFR foveator. § V.A.
#[derive(Debug, Clone)]
pub struct FFRFoveator {
    /// Profile to apply each frame.
    pub profile: FFRProfile,
}

impl FFRFoveator {
    /// New FFR foveator with the spec-default profile.
    #[must_use]
    pub const fn default_high() -> Self {
        Self {
            profile: FFRProfile::High,
        }
    }

    /// New FFR foveator with a custom profile.
    #[must_use]
    pub const fn with_profile(profile: FFRProfile) -> Self {
        Self { profile }
    }
}

impl Foveator for FFRFoveator {
    fn config_for_frame(
        &mut self,
        view_set: &ViewSet,
        _gaze: Option<&LabeledValue<GazePrediction>>,
    ) -> FoveationConfig {
        FoveationConfig::ffr_only(view_set.view_count, self.profile)
    }

    fn supports_dfr(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "ffr"
    }
}

/// Eye-tracked dynamic-foveation foveator. § V.B.
#[derive(Debug, Clone)]
pub struct DFRFoveator {
    /// FFR-fallback profile (used when gaze is None).
    pub fallback_profile: FFRProfile,
    /// Active profile when DFR engaged.
    pub active_profile: FFRProfile,
}

impl DFRFoveator {
    /// New DFR foveator with default-profiles (Aggressive when DFR
    /// engaged ; High when fallback).
    #[must_use]
    pub const fn aggressive() -> Self {
        Self {
            fallback_profile: FFRProfile::High,
            active_profile: FFRProfile::Aggressive,
        }
    }
}

impl Foveator for DFRFoveator {
    fn config_for_frame(
        &mut self,
        view_set: &ViewSet,
        gaze: Option<&LabeledValue<GazePrediction>>,
    ) -> FoveationConfig {
        match gaze {
            Some(g) => {
                // PRIME §1 : the gaze sample is `LabeledValue<GazePrediction>` ;
                // we read only the on-device `.value` here. The cssl-ifc
                // egress-gate prevents this value reaching telemetry.
                debug_assert!(g.is_biometric(), "gaze must be biometric-tagged");
                let mut centers = Vec::with_capacity(view_set.view_count as usize);
                for _ in 0..view_set.view_count {
                    centers.push(g.value.center_ndc);
                }
                FoveationConfig::dfr(view_set.view_count, self.active_profile, &centers)
            }
            None => FoveationConfig::ffr_only(view_set.view_count, self.fallback_profile),
        }
    }

    fn supports_dfr(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "dfr"
    }
}

/// 5-yr ML-foveated foveator (stub). § V.D.
#[derive(Debug, Clone)]
pub struct MLFoveator {
    /// Stub : a real impl wires to `cssl-substrate-kan::SplineNetwork`.
    pub fallback: DFRFoveator,
}

impl MLFoveator {
    /// New ML-foveator stub : delegates to `DFRFoveator::aggressive` until
    /// the neural-network impl lands.
    #[must_use]
    pub const fn stub() -> Self {
        Self {
            fallback: DFRFoveator::aggressive(),
        }
    }
}

impl Foveator for MLFoveator {
    fn config_for_frame(
        &mut self,
        view_set: &ViewSet,
        gaze: Option<&LabeledValue<GazePrediction>>,
    ) -> FoveationConfig {
        // 5-yr placeholder : delegate to DFR.
        self.fallback.config_for_frame(view_set, gaze)
    }

    fn supports_dfr(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "ml-foveated-stub"
    }
}

/// Gaze-prediction output. § V.B + § VIII.C.
///
/// Always wrapped in `LabeledValue<GazePrediction>` with
/// `SensitiveDomain::Gaze` tag so the IFC layer prevents egress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GazePrediction {
    /// Predicted gaze-center in NDC ([-1, +1]) per-eye.
    /// Default-shape : `[left_eye, right_eye]` 2D-NDC ; for the foveation
    /// config we average them.
    pub center_ndc: [f32; 2],
    /// Confidence ∈ [0, 1].
    pub confidence: f32,
    /// `true` iff a saccade is predicted in-progress.
    pub saccade_in_progress: bool,
}

impl GazePrediction {
    /// Identity-prediction : center @ (0, 0), confidence 0.
    /// Used by tests + by FFR-fallback path.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            center_ndc: [0.0, 0.0],
            confidence: 0.0,
            saccade_in_progress: false,
        }
    }

    /// Wrap this prediction in a `LabeledValue` with the canonical
    /// `SensitiveDomain::Gaze` + an empty Label (downstream IFC layers
    /// add the user-specific principals).
    #[must_use]
    pub fn into_labeled(self) -> LabeledValue<Self> {
        LabeledValue::with_domain(self, Label::bottom(), SensitiveDomain::Gaze)
    }
}

/// Saccade-predictor EKF : 6D state-vector (pos, vel, acc) + 6×6 covariance.
/// § VIII.C.
#[derive(Debug, Clone)]
pub struct SaccadeEKF {
    /// State-vector `[pos_x, pos_y, vel_x, vel_y, acc_x, acc_y]` eye-relative.
    pub state: [f32; 6],
    /// 6×6 covariance, row-major.
    pub cov: [f32; 36],
    /// Process-noise diagonal (6 entries).
    pub q_diag: [f32; 6],
    /// Measurement-noise diagonal (2 entries : x + y).
    pub r_diag: [f32; 2],
    /// Last update-time-ns.
    pub last_update_ns: u64,
}

impl SaccadeEKF {
    /// Initialize at origin.
    #[must_use]
    pub fn at_origin() -> Self {
        let mut cov = [0.0; 36];
        // Identity-init for cov.
        for i in 0..6 {
            cov[i * 6 + i] = 1.0;
        }
        Self {
            state: [0.0; 6],
            cov,
            q_diag: [0.001, 0.001, 0.01, 0.01, 0.1, 0.1],
            r_diag: [0.05, 0.05],
            last_update_ns: 0,
        }
    }

    /// Predict the gaze position at `display_time_ns` ahead. Stage-0
    /// implementation : straight-line extrapolation `pos += vel * dt
    /// + 0.5 * acc * dt²`. The full EKF covariance update lives in the
    /// FFI follow-up slice.
    #[must_use]
    pub fn predict(&self, display_time_ns: u64) -> GazePrediction {
        let dt_ns = display_time_ns.saturating_sub(self.last_update_ns) as f64;
        let dt = (dt_ns / 1_000_000_000.0) as f32;
        let pos_x = self.state[0] + self.state[2] * dt + 0.5 * self.state[4] * dt * dt;
        let pos_y = self.state[1] + self.state[3] * dt + 0.5 * self.state[5] * dt * dt;
        let confidence = (1.0 - dt.min(0.1) * 5.0).max(0.0); // confidence drops with prediction-horizon
        GazePrediction {
            center_ndc: [pos_x.clamp(-1.0, 1.0), pos_y.clamp(-1.0, 1.0)],
            confidence,
            saccade_in_progress: self.state[2].abs() + self.state[3].abs() > 5.0,
        }
    }

    /// Update with a new measurement `z = (pos_x, pos_y)` at `t_ns`.
    /// Stage-0 : simple low-pass-update on state + zero-out cov.
    /// Real EKF math lands in the FFI follow-up slice.
    pub fn update(&mut self, z_x: f32, z_y: f32, t_ns: u64) -> Result<(), XRFailure> {
        let dt_ns = t_ns.saturating_sub(self.last_update_ns) as f64;
        let dt = (dt_ns / 1_000_000_000.0) as f32;
        if dt > 0.0 && self.last_update_ns != 0 {
            let new_vel_x = (z_x - self.state[0]) / dt;
            let new_vel_y = (z_y - self.state[1]) / dt;
            self.state[4] = (new_vel_x - self.state[2]) / dt; // acc estimate
            self.state[5] = (new_vel_y - self.state[3]) / dt;
            self.state[2] = new_vel_x;
            self.state[3] = new_vel_y;
        }
        self.state[0] = z_x;
        self.state[1] = z_y;
        self.last_update_ns = t_ns;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DFRFoveator, FFRFoveator, FFRProfile, FoveationConfig, Foveator, GazePrediction,
        MLFoveator, SaccadeEKF,
    };
    use crate::view::ViewSet;

    #[test]
    fn ffr_default_is_high() {
        assert_eq!(FFRProfile::default_profile(), FFRProfile::High);
    }

    #[test]
    fn ffr_periphery_rate_aggressive_is_16() {
        assert_eq!(FFRProfile::Aggressive.periphery_rate_denominator(), 16);
        assert_eq!(FFRProfile::High.periphery_rate_denominator(), 8);
        assert_eq!(FFRProfile::Off.periphery_rate_denominator(), 1);
    }

    #[test]
    fn ffr_center_zone_shrinks_with_aggression() {
        assert!(FFRProfile::Off.center_zone_radius() > FFRProfile::Low.center_zone_radius());
        assert!(FFRProfile::Low.center_zone_radius() > FFRProfile::Medium.center_zone_radius());
        assert!(FFRProfile::Medium.center_zone_radius() > FFRProfile::High.center_zone_radius());
        assert!(FFRProfile::High.center_zone_radius() > FFRProfile::Aggressive.center_zone_radius());
    }

    #[test]
    fn ffr_tighten_ladder() {
        assert_eq!(FFRProfile::Off.tighten(), FFRProfile::Low);
        assert_eq!(FFRProfile::Low.tighten(), FFRProfile::Medium);
        assert_eq!(FFRProfile::Medium.tighten(), FFRProfile::High);
        assert_eq!(FFRProfile::High.tighten(), FFRProfile::Aggressive);
        // saturate
        assert_eq!(FFRProfile::Aggressive.tighten(), FFRProfile::Aggressive);
    }

    #[test]
    fn ffr_foveator_returns_ffr_only_no_dfr() {
        let mut f = FFRFoveator::default_high();
        let vs = ViewSet::stereo_identity(64.0);
        let cfg = f.config_for_frame(&vs, None);
        assert!(!cfg.dfr_engaged);
        assert_eq!(cfg.profile, FFRProfile::High);
        assert!(!f.supports_dfr());
        assert_eq!(f.name(), "ffr");
    }

    #[test]
    fn dfr_foveator_falls_back_when_no_gaze() {
        let mut f = DFRFoveator::aggressive();
        let vs = ViewSet::stereo_identity(64.0);
        let cfg = f.config_for_frame(&vs, None);
        assert!(!cfg.dfr_engaged);
        assert_eq!(cfg.profile, FFRProfile::High);
        assert!(f.supports_dfr());
        assert_eq!(f.name(), "dfr");
    }

    #[test]
    fn dfr_foveator_engages_with_gaze() {
        let mut f = DFRFoveator::aggressive();
        let vs = ViewSet::stereo_identity(64.0);
        let gaze = GazePrediction {
            center_ndc: [0.3, -0.2],
            confidence: 0.8,
            saccade_in_progress: false,
        }
        .into_labeled();
        let cfg = f.config_for_frame(&vs, Some(&gaze));
        assert!(cfg.dfr_engaged);
        assert_eq!(cfg.profile, FFRProfile::Aggressive);
        assert!((cfg.fovea_center_ndc[0][0] - 0.3).abs() < 1e-6);
        assert!((cfg.fovea_center_ndc[0][1] + 0.2).abs() < 1e-6);
    }

    #[test]
    fn ml_foveator_delegates_to_dfr_today() {
        let mut f = MLFoveator::stub();
        let vs = ViewSet::stereo_identity(64.0);
        let cfg = f.config_for_frame(&vs, None);
        assert!(!cfg.dfr_engaged); // no gaze ⇒ falls back via DFR's own fallback
        assert!(f.supports_dfr());
        assert_eq!(f.name(), "ml-foveated-stub");
    }

    #[test]
    fn foveation_config_ffr_only_zero_centers() {
        let cfg = FoveationConfig::ffr_only(2, FFRProfile::High);
        assert!(!cfg.dfr_engaged);
        assert_eq!(cfg.fovea_center_ndc[0], [0.0, 0.0]);
        assert_eq!(cfg.fovea_center_ndc[1], [0.0, 0.0]);
    }

    #[test]
    fn foveation_config_dfr_populates_centers() {
        let centers = [[0.1, 0.2], [-0.1, -0.2]];
        let cfg = FoveationConfig::dfr(2, FFRProfile::Aggressive, &centers);
        assert!(cfg.dfr_engaged);
        assert_eq!(cfg.fovea_center_ndc[0], [0.1, 0.2]);
        assert_eq!(cfg.fovea_center_ndc[1], [-0.1, -0.2]);
        assert_eq!(cfg.fovea_center_ndc[2], [0.0, 0.0]); // unused slot zero
    }

    #[test]
    fn gaze_prediction_into_labeled_carries_gaze_domain() {
        let lv = GazePrediction::identity().into_labeled();
        assert!(lv.is_biometric());
        assert!(lv.is_egress_banned());
    }

    #[test]
    fn saccade_ekf_initial_state_origin() {
        let ekf = SaccadeEKF::at_origin();
        assert_eq!(ekf.state, [0.0; 6]);
    }

    #[test]
    fn saccade_ekf_predict_zero_velocity_returns_origin() {
        let ekf = SaccadeEKF::at_origin();
        let p = ekf.predict(1_000_000); // 1ms
        assert!(p.center_ndc[0].abs() < 1e-6);
        assert!(p.center_ndc[1].abs() < 1e-6);
    }

    #[test]
    fn saccade_ekf_update_records_position() {
        let mut ekf = SaccadeEKF::at_origin();
        ekf.update(0.3, -0.2, 1_000_000).unwrap();
        assert!((ekf.state[0] - 0.3).abs() < 1e-6);
        assert!((ekf.state[1] + 0.2).abs() < 1e-6);
    }

    #[test]
    fn saccade_ekf_predict_after_update_extrapolates() {
        let mut ekf = SaccadeEKF::at_origin();
        // First update : seed at t=1ms (non-zero so the second update's
        // velocity-computation branch engages).
        ekf.update(0.0, 0.0, 1_000_000).unwrap();
        ekf.update(0.1, 0.0, 17_666_666).unwrap(); // 16.67ms later
        // Now velocity is set ; predict 16ms ahead.
        let p = ekf.predict(33_333_333);
        // We've extrapolated forward in x ; pos_x should be > 0.1.
        assert!(p.center_ndc[0] > 0.1, "got {}", p.center_ndc[0]);
    }

    #[test]
    fn saccade_ekf_predict_bounds_to_ndc() {
        let mut ekf = SaccadeEKF::at_origin();
        ekf.state[0] = 5.0; // out-of-bounds
        let p = ekf.predict(0);
        assert!(p.center_ndc[0] <= 1.0);
        assert!(p.center_ndc[0] >= -1.0);
    }
}
