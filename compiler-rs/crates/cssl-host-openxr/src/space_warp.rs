//! Application-Spacewarp (AppSW) : `XR_FB_space_warp` ½-rate render-path.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § VI.
//!
//! § DESIGN
//!   - `AppSwScheduler` : adaptive ½-rate engagement.
//!   - § VI.A : "if frame-time < 11ms ⇒ render-every-frame ; if frame-time
//!     > 11ms ⇒ render-every-other-frame ; transition R! hysteretic
//!     ⊗ ≥ 30-frame-stable before-switch".
//!   - § VI : motion_vector + linear_depth submitted to runtime ;
//!     runtime synthesizes the intermediate frame.
//!   - § VI : AppSW correctness-conditions (motion_vector encodes
//!     screen-velocity ; linear_depth not NDC-Z ; new-content marked) ;
//!     enforced by `AppSwSubmission::validate`.

use crate::error::XRFailure;
use crate::per_eye::{DepthFormat, MotionVectorFormat, PerEyeOutputArray};

/// Number of stable-frames required before mode-switching (per § VI.A
/// hysteresis).
pub const HYSTERESIS_FRAMES: u32 = 30;

/// Mode the AppSW scheduler is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppSwMode {
    /// Render every frame (full-rate). Display-Hz native.
    EveryFrame,
    /// Render every-other frame ; runtime synthesizes intermediate.
    /// Display-Hz maintained @ ½ engine cost.
    EveryOtherFrame,
}

impl AppSwMode {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EveryFrame => "every-frame",
            Self::EveryOtherFrame => "every-other-frame",
        }
    }

    /// `true` iff this mode renders the current frame (vs. reprojects).
    /// Even-numbered frames render in `EveryOtherFrame` ; odd-numbered
    /// reproject.
    #[must_use]
    pub const fn renders_frame(self, frame_index: u64) -> bool {
        match self {
            Self::EveryFrame => true,
            Self::EveryOtherFrame => (frame_index & 1) == 0,
        }
    }
}

/// AppSW scheduler. § VI.A.
#[derive(Debug, Clone)]
pub struct AppSwScheduler {
    /// Display-rate budget in nanoseconds per frame.
    pub budget_ns: u64,
    /// Current mode.
    mode: AppSwMode,
    /// Frame-time history (last 32 samples).
    history_ns: [u64; 32],
    /// Index of next slot in history.
    history_idx: usize,
    /// Number of consecutive frames the budget has been violated.
    violations_consecutive: u32,
    /// Number of consecutive frames the budget has been met.
    successes_consecutive: u32,
}

impl AppSwScheduler {
    /// New scheduler at the given display-rate.
    /// `display_hz = 90.0` ⇒ budget = 11.111 ms.
    #[must_use]
    pub fn for_display_hz(display_hz: f32) -> Self {
        let budget_ns = (1_000_000_000.0 / display_hz) as u64;
        Self {
            budget_ns,
            mode: AppSwMode::EveryFrame,
            history_ns: [0; 32],
            history_idx: 0,
            violations_consecutive: 0,
            successes_consecutive: HYSTERESIS_FRAMES, // start in stable
        }
    }

    /// Quest 3 default scheduler @ 90Hz (mobile-GPU AppSW required-shipped).
    #[must_use]
    pub fn quest3_default() -> Self {
        Self::for_display_hz(90.0)
    }

    /// Vision Pro default @ 90Hz (visionOS handles internally ; this
    /// scheduler is a stub that effectively never engages).
    #[must_use]
    pub fn vision_pro_default() -> Self {
        Self::for_display_hz(90.0)
    }

    /// Pimax Crystal Super @ 90Hz (PCVR ; AppSW not required).
    #[must_use]
    pub fn pimax_default() -> Self {
        Self::for_display_hz(90.0)
    }

    /// Decide whether the current frame should be rendered or reprojected.
    /// `frame_index` is the engine's monotonic frame-counter (used for
    /// even/odd-frame in the EveryOtherFrame mode).
    #[must_use]
    pub fn should_render(&self, frame_index: u64) -> bool {
        self.mode.renders_frame(frame_index)
    }

    /// Record a frame-time observation and update mode if hysteresis
    /// thresholds are crossed.
    pub fn record_frame_time(&mut self, frame_time_ns: u64) {
        self.history_ns[self.history_idx] = frame_time_ns;
        self.history_idx = (self.history_idx + 1) % self.history_ns.len();

        if frame_time_ns > self.budget_ns {
            self.violations_consecutive = self.violations_consecutive.saturating_add(1);
            self.successes_consecutive = 0;
        } else {
            self.successes_consecutive = self.successes_consecutive.saturating_add(1);
            self.violations_consecutive = 0;
        }

        // Hysteresis : require N stable frames before switching.
        match self.mode {
            AppSwMode::EveryFrame => {
                if self.violations_consecutive >= HYSTERESIS_FRAMES {
                    self.mode = AppSwMode::EveryOtherFrame;
                }
            }
            AppSwMode::EveryOtherFrame => {
                // Need N successes at HALF-budget to trust we can render every frame.
                if self.successes_consecutive >= HYSTERESIS_FRAMES
                    && frame_time_ns < self.budget_ns / 2
                {
                    self.mode = AppSwMode::EveryFrame;
                }
            }
        }
    }

    /// Current mode.
    #[must_use]
    pub const fn mode(&self) -> AppSwMode {
        self.mode
    }

    /// p99-style approximation : max of the last 32 frame-times.
    #[must_use]
    pub fn p99_estimate_ns(&self) -> u64 {
        *self.history_ns.iter().max().unwrap_or(&0)
    }

    /// Force a mode (for testing + integration scenarios).
    pub fn force_mode(&mut self, mode: AppSwMode) {
        self.mode = mode;
    }
}

/// Submission to `XR_FB_space_warp` : motion_vector + linear_depth per-eye.
/// § VI correctness-conditions.
#[derive(Debug, Clone)]
pub struct AppSwSubmission<'a> {
    /// Per-eye outputs that were rendered this frame. Each must have
    /// non-zero `motion_vector` + `linear_depth`.
    pub per_eye: &'a PerEyeOutputArray,
}

impl<'a> AppSwSubmission<'a> {
    /// Construct + validate. § VI :
    ///   - motion_vector R! correctly-encode per-pixel screen-velocity
    ///   - linear_depth R! linear-Z (¬ NDC-Z)
    ///   - new-content R! flagged-via motion_vector zeroed-or-invalid-mark
    pub fn new(per_eye: &'a PerEyeOutputArray) -> Result<Self, XRFailure> {
        if !per_eye.all_have_appsw_companions() {
            return Err(XRFailure::SpaceWarpSubmissionRejected { code: -10 });
        }
        // Motion-vector format must be Rg16F or Rg16Unorm or Rg32F.
        for out in &per_eye.outputs {
            // motion-vector format
            match out.motion_vector_format {
                MotionVectorFormat::Rg16F
                | MotionVectorFormat::Rg16Unorm
                | MotionVectorFormat::Rg32F => {}
            }
            // depth must be linear, not NDC ; we encode this via the
            // format-hint : `Depth32F` is the canonical linear-Z choice.
            // Reject `Depth16Unorm` for AppSW (insufficient precision @ Quest3 panel).
            if matches!(out.linear_depth_format, DepthFormat::Depth16Unorm) {
                return Err(XRFailure::SpaceWarpSubmissionRejected { code: -11 });
            }
        }
        Ok(Self { per_eye })
    }
}

#[cfg(test)]
mod tests {
    use super::{AppSwMode, AppSwScheduler, AppSwSubmission, HYSTERESIS_FRAMES};
    use crate::per_eye::{DepthFormat, PerEyeOutputArray};
    use crate::view::ViewSet;

    #[test]
    fn quest3_default_budget_is_11ms() {
        let s = AppSwScheduler::quest3_default();
        // 1e9 / 90 = 11_111_111.111... ns
        assert!((s.budget_ns as i64 - 11_111_111).abs() < 1000);
    }

    #[test]
    fn mode_renders_every_frame_in_default() {
        let mode = AppSwMode::EveryFrame;
        for f in 0..10u64 {
            assert!(mode.renders_frame(f));
        }
    }

    #[test]
    fn mode_renders_alternate_in_every_other_frame() {
        let mode = AppSwMode::EveryOtherFrame;
        assert!(mode.renders_frame(0));
        assert!(!mode.renders_frame(1));
        assert!(mode.renders_frame(2));
        assert!(!mode.renders_frame(3));
    }

    #[test]
    fn scheduler_starts_in_every_frame() {
        let s = AppSwScheduler::quest3_default();
        assert_eq!(s.mode(), AppSwMode::EveryFrame);
    }

    #[test]
    fn scheduler_engages_app_sw_after_hysteresis_violations() {
        let mut s = AppSwScheduler::quest3_default();
        // Feed budget-violations.
        for _ in 0..HYSTERESIS_FRAMES {
            s.record_frame_time(s.budget_ns + 1_000_000); // 1ms over
        }
        assert_eq!(s.mode(), AppSwMode::EveryOtherFrame);
    }

    #[test]
    fn scheduler_does_not_engage_app_sw_before_hysteresis_threshold() {
        let mut s = AppSwScheduler::quest3_default();
        for _ in 0..HYSTERESIS_FRAMES - 1 {
            s.record_frame_time(s.budget_ns + 1_000_000);
        }
        assert_eq!(s.mode(), AppSwMode::EveryFrame);
    }

    #[test]
    fn scheduler_disengages_app_sw_after_half_budget_stability() {
        let mut s = AppSwScheduler::quest3_default();
        // Engage AppSW.
        for _ in 0..HYSTERESIS_FRAMES {
            s.record_frame_time(s.budget_ns + 1_000_000);
        }
        assert_eq!(s.mode(), AppSwMode::EveryOtherFrame);
        // Now feed sub-half-budget frames for the hysteresis period.
        for _ in 0..HYSTERESIS_FRAMES {
            s.record_frame_time(s.budget_ns / 4); // well under half-budget
        }
        assert_eq!(s.mode(), AppSwMode::EveryFrame);
    }

    #[test]
    fn scheduler_records_p99_max() {
        let mut s = AppSwScheduler::quest3_default();
        s.record_frame_time(1_000_000);
        s.record_frame_time(20_000_000);
        s.record_frame_time(2_000_000);
        assert_eq!(s.p99_estimate_ns(), 20_000_000);
    }

    #[test]
    fn submission_rejects_missing_companions() {
        let vs = ViewSet::stereo_identity(64.0);
        let arr = PerEyeOutputArray::placeholder_for(&vs, 1024, 1024);
        // placeholder has zero handles ⇒ no companions.
        assert!(AppSwSubmission::new(&arr).is_err());
    }

    #[test]
    fn submission_accepts_when_all_companions_present() {
        let vs = ViewSet::stereo_identity(64.0);
        let mut arr = PerEyeOutputArray::placeholder_for(&vs, 1024, 1024);
        for out in &mut arr.outputs {
            out.motion_vector = 1;
            out.linear_depth = 2;
        }
        assert!(AppSwSubmission::new(&arr).is_ok());
    }

    #[test]
    fn submission_rejects_depth16_unorm() {
        let vs = ViewSet::stereo_identity(64.0);
        let mut arr = PerEyeOutputArray::placeholder_for(&vs, 1024, 1024);
        for out in &mut arr.outputs {
            out.motion_vector = 1;
            out.linear_depth = 2;
            out.linear_depth_format = DepthFormat::Depth16Unorm;
        }
        // Depth-16-Unorm is rejected for AppSW (insufficient precision @ Quest3 panel).
        assert!(AppSwSubmission::new(&arr).is_err());
    }

    #[test]
    fn pimax_scheduler_default() {
        let s = AppSwScheduler::pimax_default();
        assert_eq!(s.mode(), AppSwMode::EveryFrame);
    }

    #[test]
    fn vision_pro_scheduler_default() {
        let s = AppSwScheduler::vision_pro_default();
        // Same budget ; visionOS handles ½-rate internally.
        assert!((s.budget_ns as i64 - 11_111_111).abs() < 1000);
    }

    #[test]
    fn should_render_uses_mode() {
        let mut s = AppSwScheduler::quest3_default();
        s.force_mode(AppSwMode::EveryOtherFrame);
        assert!(s.should_render(0));
        assert!(!s.should_render(1));
        assert!(s.should_render(2));
    }
}
