//! Comfort-floor enforcement + judder-detector + quality-degrade ladder.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § XII.
//!
//! § XII.A : 90 Hz minimum ⊗ R! held-not-targeted ⊗ p99 frame-time ≤ display-period
//! § XII.A quality-degrade ladder :
//!   1. tighten foveation profile (HIGH → AGGRESSIVE)
//!   2. drop XeSS2 internal-resolution
//!   3. cut radiance-cascade bands
//!   4. reduce explicit-bounces
//!   5. AppSW engaged-permanent
//!   6. ‼ never-drop below 90Hz output ⊗ rather-degrade visual-fidelity

use crate::error::XRFailure;
use crate::foveation::FFRProfile;
use crate::space_warp::AppSwMode;

/// Quality-degrade level. Walks the § XII.A ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QualityLevel {
    /// Full quality (no degrade).
    Full,
    /// Step 1 : tighter foveation.
    DegradeFoveation,
    /// Step 2 : lower upscale internal-resolution.
    DegradeUpscale,
    /// Step 3 : radiance-cascade bands cut to static.
    DegradeCascadeBands,
    /// Step 4 : reduce explicit-bounces.
    DegradeBounces,
    /// Step 5 : AppSW engaged permanently.
    DegradeAppSw,
    /// Worst-case : all degrades + AppSW + minimum-bounces. Visual
    /// fidelity reduced ; 90 Hz output preserved.
    DegradeMax,
}

impl QualityLevel {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::DegradeFoveation => "degrade-foveation",
            Self::DegradeUpscale => "degrade-upscale",
            Self::DegradeCascadeBands => "degrade-cascade-bands",
            Self::DegradeBounces => "degrade-bounces",
            Self::DegradeAppSw => "degrade-appsw",
            Self::DegradeMax => "degrade-max",
        }
    }

    /// Walk one step down the ladder.
    #[must_use]
    pub const fn next_degrade(self) -> Self {
        match self {
            Self::Full => Self::DegradeFoveation,
            Self::DegradeFoveation => Self::DegradeUpscale,
            Self::DegradeUpscale => Self::DegradeCascadeBands,
            Self::DegradeCascadeBands => Self::DegradeBounces,
            Self::DegradeBounces => Self::DegradeAppSw,
            Self::DegradeAppSw | Self::DegradeMax => Self::DegradeMax,
        }
    }

    /// Walk one step up (recovery).
    #[must_use]
    pub const fn prev_degrade(self) -> Self {
        match self {
            Self::Full | Self::DegradeFoveation => Self::Full,
            Self::DegradeUpscale => Self::DegradeFoveation,
            Self::DegradeCascadeBands => Self::DegradeUpscale,
            Self::DegradeBounces => Self::DegradeCascadeBands,
            Self::DegradeAppSw => Self::DegradeBounces,
            Self::DegradeMax => Self::DegradeAppSw,
        }
    }

    /// FFR profile to apply at this quality-level.
    #[must_use]
    pub const fn ffr_profile(self) -> FFRProfile {
        match self {
            Self::Full => FFRProfile::High,
            Self::DegradeFoveation
            | Self::DegradeUpscale
            | Self::DegradeCascadeBands
            | Self::DegradeBounces
            | Self::DegradeAppSw
            | Self::DegradeMax => FFRProfile::Aggressive,
        }
    }

    /// Internal-resolution multiplier (XeSS2/DLSS render-resolution).
    /// 1.0 = native ; 0.75 = 900p-equiv on 1080p ; etc.
    #[must_use]
    pub fn upscale_multiplier(self) -> f32 {
        match self {
            Self::Full | Self::DegradeFoveation => 1.0,
            Self::DegradeUpscale => 0.83,
            Self::DegradeCascadeBands => 0.75,
            Self::DegradeBounces => 0.67,
            Self::DegradeAppSw | Self::DegradeMax => 0.50,
        }
    }

    /// Number of explicit bounces to render this frame.
    /// (Cascade indirect always-on ; explicit-bounce reduction is
    /// per § XII.A step 4.)
    #[must_use]
    pub const fn explicit_bounces(self) -> u32 {
        match self {
            Self::Full
            | Self::DegradeFoveation
            | Self::DegradeUpscale
            | Self::DegradeCascadeBands => 4,
            Self::DegradeBounces => 2,
            Self::DegradeAppSw => 1,
            Self::DegradeMax => 0,
        }
    }

    /// `true` iff AppSW must be permanently-engaged at this level.
    #[must_use]
    pub const fn appsw_required(self) -> bool {
        matches!(self, Self::DegradeAppSw | Self::DegradeMax)
    }

    /// AppSW mode forced-by this quality-level.
    #[must_use]
    pub const fn appsw_mode_forced(self) -> Option<AppSwMode> {
        if self.appsw_required() {
            Some(AppSwMode::EveryOtherFrame)
        } else {
            None
        }
    }
}

/// Judder-detector : monitors frame-time histogram and returns the
/// recommended quality-level for the next frame. § XII.A.
#[derive(Debug, Clone)]
pub struct JudderDetector {
    /// Display-period in nanoseconds (e.g. 11_111_111 for 90Hz).
    pub display_period_ns: u64,
    /// 32-sample rolling window of frame-times.
    samples_ns: [u64; 32],
    /// Index of next slot.
    sample_idx: usize,
    /// Current quality-level.
    quality: QualityLevel,
    /// Frames-stable counter (used to recover quality after judder ends).
    stable_frames: u32,
}

/// Stable-frame threshold for quality-recovery.
pub const STABLE_FRAMES_TO_RECOVER: u32 = 60; // 0.67s @ 90Hz

impl JudderDetector {
    /// New detector for a display-rate.
    #[must_use]
    pub fn for_display_hz(display_hz: f32) -> Self {
        Self {
            display_period_ns: (1_000_000_000.0 / display_hz) as u64,
            samples_ns: [0; 32],
            sample_idx: 0,
            quality: QualityLevel::Full,
            stable_frames: 0,
        }
    }

    /// Quest 3 default at 90Hz.
    #[must_use]
    pub fn quest3_default() -> Self {
        Self::for_display_hz(90.0)
    }

    /// Record a frame-time observation. May update internal quality-level.
    pub fn record(&mut self, frame_time_ns: u64) {
        self.samples_ns[self.sample_idx] = frame_time_ns;
        self.sample_idx = (self.sample_idx + 1) % self.samples_ns.len();

        // p95-ish : sort + take 31st-percentile (95% threshold).
        let mut sorted = self.samples_ns;
        sorted.sort_unstable();
        let p95 = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];

        if p95 > self.display_period_ns {
            // Judder : tighten quality.
            self.quality = self.quality.next_degrade();
            self.stable_frames = 0;
        } else {
            // Stable.
            self.stable_frames = self.stable_frames.saturating_add(1);
            if self.stable_frames >= STABLE_FRAMES_TO_RECOVER && self.quality != QualityLevel::Full
            {
                self.quality = self.quality.prev_degrade();
                self.stable_frames = 0;
            }
        }
    }

    /// Current quality-level.
    #[must_use]
    pub const fn quality(&self) -> QualityLevel {
        self.quality
    }

    /// Current p95 estimate.
    #[must_use]
    pub fn p95_estimate_ns(&self) -> u64 {
        let mut sorted = self.samples_ns;
        sorted.sort_unstable();
        sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)]
    }

    /// Force a quality (test + integration scenarios).
    pub fn force_quality(&mut self, q: QualityLevel) {
        self.quality = q;
    }

    /// Validate the comfort-floor : the current p99 estimate should
    /// be ≤ display-period at the current quality. Returns error if
    /// even at DegradeMax the floor is violated.
    pub fn validate_floor(&self) -> Result<(), XRFailure> {
        let p99_est = self.p95_estimate_ns(); // proxy
        if p99_est > self.display_period_ns && self.quality == QualityLevel::DegradeMax {
            return Err(XRFailure::ComfortFloorViolated {
                ns: p99_est,
                budget_ns: self.display_period_ns,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{JudderDetector, QualityLevel, STABLE_FRAMES_TO_RECOVER};
    use crate::space_warp::AppSwMode;

    #[test]
    fn quality_level_ladder_walks_down() {
        let q0 = QualityLevel::Full;
        let q1 = q0.next_degrade();
        let q2 = q1.next_degrade();
        let q3 = q2.next_degrade();
        let q4 = q3.next_degrade();
        let q5 = q4.next_degrade();
        let q6 = q5.next_degrade();
        assert_eq!(q1, QualityLevel::DegradeFoveation);
        assert_eq!(q2, QualityLevel::DegradeUpscale);
        assert_eq!(q3, QualityLevel::DegradeCascadeBands);
        assert_eq!(q4, QualityLevel::DegradeBounces);
        assert_eq!(q5, QualityLevel::DegradeAppSw);
        assert_eq!(q6, QualityLevel::DegradeMax);
    }

    #[test]
    fn quality_level_ladder_saturates_at_max() {
        let q = QualityLevel::DegradeMax.next_degrade();
        assert_eq!(q, QualityLevel::DegradeMax);
    }

    #[test]
    fn quality_level_ladder_walks_up() {
        let q = QualityLevel::DegradeMax;
        let q = q.prev_degrade();
        assert_eq!(q, QualityLevel::DegradeAppSw);
        let q = q.prev_degrade();
        assert_eq!(q, QualityLevel::DegradeBounces);
    }

    #[test]
    fn quality_level_ffr_tightens_at_first_degrade() {
        assert_eq!(
            QualityLevel::Full.ffr_profile(),
            crate::foveation::FFRProfile::High
        );
        assert_eq!(
            QualityLevel::DegradeFoveation.ffr_profile(),
            crate::foveation::FFRProfile::Aggressive
        );
    }

    #[test]
    fn quality_level_upscale_multiplier_drops() {
        assert!((QualityLevel::Full.upscale_multiplier() - 1.0).abs() < 1e-6);
        assert!(QualityLevel::DegradeUpscale.upscale_multiplier() < 1.0);
        assert!(
            QualityLevel::DegradeMax.upscale_multiplier()
                < QualityLevel::DegradeUpscale.upscale_multiplier()
        );
    }

    #[test]
    fn quality_level_bounces_drop() {
        assert_eq!(QualityLevel::Full.explicit_bounces(), 4);
        assert_eq!(QualityLevel::DegradeBounces.explicit_bounces(), 2);
        assert_eq!(QualityLevel::DegradeMax.explicit_bounces(), 0);
    }

    #[test]
    fn quality_level_appsw_engaged_at_step5() {
        assert!(!QualityLevel::Full.appsw_required());
        assert!(!QualityLevel::DegradeBounces.appsw_required());
        assert!(QualityLevel::DegradeAppSw.appsw_required());
        assert!(QualityLevel::DegradeMax.appsw_required());
    }

    #[test]
    fn quality_level_appsw_mode_forced_when_required() {
        assert_eq!(QualityLevel::Full.appsw_mode_forced(), None);
        assert_eq!(
            QualityLevel::DegradeAppSw.appsw_mode_forced(),
            Some(AppSwMode::EveryOtherFrame)
        );
    }

    #[test]
    fn judder_detector_default_quest3() {
        let d = JudderDetector::quest3_default();
        assert!((d.display_period_ns as i64 - 11_111_111).abs() < 1000);
        assert_eq!(d.quality(), QualityLevel::Full);
    }

    #[test]
    fn judder_detector_starts_at_full_quality() {
        let d = JudderDetector::quest3_default();
        assert_eq!(d.quality(), QualityLevel::Full);
    }

    #[test]
    fn judder_detector_degrades_on_violations() {
        let mut d = JudderDetector::quest3_default();
        // Feed > 1 budget per sample.
        for _ in 0..32 {
            d.record(d.display_period_ns + 5_000_000);
        }
        // After enough violation samples, quality should have walked down.
        assert_ne!(d.quality(), QualityLevel::Full);
    }

    #[test]
    fn judder_detector_recovers_on_stable_frames() {
        let mut d = JudderDetector::quest3_default();
        d.force_quality(QualityLevel::DegradeFoveation);
        for _ in 0..STABLE_FRAMES_TO_RECOVER {
            d.record(d.display_period_ns / 2);
        }
        assert_eq!(d.quality(), QualityLevel::Full);
    }

    #[test]
    fn judder_detector_validate_floor_passes_at_full() {
        let d = JudderDetector::quest3_default();
        assert!(d.validate_floor().is_ok());
    }

    #[test]
    fn judder_detector_validate_floor_fails_only_at_max_with_violations() {
        let mut d = JudderDetector::quest3_default();
        d.force_quality(QualityLevel::DegradeMax);
        for _ in 0..32 {
            d.record(d.display_period_ns * 2);
        }
        assert!(d.validate_floor().is_err());
    }

    #[test]
    fn judder_detector_p95_estimate_responds_to_samples() {
        let mut d = JudderDetector::quest3_default();
        d.record(20_000_000);
        d.record(30_000_000);
        let p95 = d.p95_estimate_ns();
        // Sorted [0; 30 zero-filled, 20m, 30m] : p95 = 30m typical.
        assert!(p95 >= 20_000_000);
    }

    #[test]
    fn quality_level_as_str() {
        assert_eq!(QualityLevel::Full.as_str(), "full");
        assert_eq!(QualityLevel::DegradeMax.as_str(), "degrade-max");
    }
}
