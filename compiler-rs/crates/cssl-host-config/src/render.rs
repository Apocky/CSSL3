//! § cssl-host-config :: render parameters
//!
//! § I> RenderConfig holds all runtime-tweakable rendering parameters for the
//! LoA host : output resolution, MSAA sample count, HDR exposure scalar,
//! ACES tonemap strength, CFER (Cognitive-Field Eccentricity Ratio) alpha,
//! target FPS, and vsync flag. Defaults match LoA-v13 GDD §§ render-budget.
//!
//! § validate-rules
//!   resolution   : both axes > 0
//!   msaa_samples : ∈ {1, 2, 4, 8}
//!   hdr_exposure : > 0
//!   aces_strength: > 0
//!   cfer_alpha   : > 0
//!   target_fps   : ∈ [10, 240]
//!   vsync        : bool — no validation
//!
//! § interaction-with-loader
//! `LoaConfig::validate()` aggregates per-section errors ; this module's
//! `validate()` returns `Result<(), ConfigErr>` so that the loader can
//! collect a `Vec<ConfigErr>` (one error per failing section) for
//! multi-error display.

use serde::{Deserialize, Serialize};

use crate::loader::ConfigErr;

/// § RenderConfig — typed render parameters loaded from `loa.config.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderConfig {
    /// § window resolution (width, height) in physical pixels.
    pub resolution: (u32, u32),
    /// § MSAA sample-count for the swap-chain attachment.
    pub msaa_samples: u8,
    /// § HDR exposure scalar applied pre-tonemap.
    pub hdr_exposure: f32,
    /// § ACES tonemap strength (0 = off, 1 = canonical, > 1 = pushed).
    pub aces_strength: f32,
    /// § CFER (Cognitive-Field Eccentricity Ratio) alpha — controls
    /// foveation falloff in the spectral-renderer pipeline.
    pub cfer_alpha: f32,
    /// § frame-rate target ; renderer pacing aims for this when vsync = false.
    pub target_fps: u32,
    /// § vertical-sync gate ; when true, present is sync'd to display refresh.
    pub vsync: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            resolution: (1920, 1080),
            msaa_samples: 4,
            hdr_exposure: 1.0,
            aces_strength: 1.0,
            cfer_alpha: 0.002,
            target_fps: 60,
            vsync: true,
        }
    }
}

impl RenderConfig {
    /// § validate — returns `ConfigErr::Render(reason)` on first failing rule.
    ///
    /// § rules (in order)
    ///   - both resolution axes non-zero
    ///   - msaa_samples ∈ {1, 2, 4, 8}
    ///   - hdr_exposure finite + > 0
    ///   - aces_strength finite + > 0
    ///   - cfer_alpha finite + > 0
    ///   - target_fps ∈ [10, 240]
    pub fn validate(&self) -> Result<(), ConfigErr> {
        let (w, h) = self.resolution;
        if w == 0 || h == 0 {
            return Err(ConfigErr::Render(format!(
                "resolution must be non-zero ; got {w}x{h}"
            )));
        }
        if !matches!(self.msaa_samples, 1 | 2 | 4 | 8) {
            return Err(ConfigErr::Render(format!(
                "msaa_samples must be one of 1/2/4/8 ; got {}",
                self.msaa_samples
            )));
        }
        if !self.hdr_exposure.is_finite() || self.hdr_exposure <= 0.0 {
            return Err(ConfigErr::Render(format!(
                "hdr_exposure must be positive + finite ; got {}",
                self.hdr_exposure
            )));
        }
        if !self.aces_strength.is_finite() || self.aces_strength <= 0.0 {
            return Err(ConfigErr::Render(format!(
                "aces_strength must be positive + finite ; got {}",
                self.aces_strength
            )));
        }
        if !self.cfer_alpha.is_finite() || self.cfer_alpha <= 0.0 {
            return Err(ConfigErr::Render(format!(
                "cfer_alpha must be positive + finite ; got {}",
                self.cfer_alpha
            )));
        }
        if self.target_fps < 10 || self.target_fps > 240 {
            return Err(ConfigErr::Render(format!(
                "target_fps must be in [10, 240] ; got {}",
                self.target_fps
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // tests intentionally mutate
                                              // a default + re-validate to
                                              // exercise per-field rules.
mod tests {
    use super::*;

    #[test]
    fn default_validates() {
        let cfg = RenderConfig::default();
        cfg.validate().expect("default RenderConfig must validate");
        assert_eq!(cfg.resolution, (1920, 1080));
        assert_eq!(cfg.msaa_samples, 4);
        assert!((cfg.hdr_exposure - 1.0).abs() < f32::EPSILON);
        assert!((cfg.aces_strength - 1.0).abs() < f32::EPSILON);
        assert!((cfg.cfer_alpha - 0.002).abs() < f32::EPSILON);
        assert_eq!(cfg.target_fps, 60);
        assert!(cfg.vsync);
    }

    #[test]
    fn zero_resolution_rejected() {
        let mut cfg = RenderConfig::default();
        cfg.resolution = (0, 1080);
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
        cfg.resolution = (1920, 0);
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
    }

    #[test]
    fn msaa_three_rejected() {
        let mut cfg = RenderConfig::default();
        cfg.msaa_samples = 3;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
        // boundary : 16 also rejected
        cfg.msaa_samples = 16;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
        // valid alternatives
        for s in [1u8, 2, 4, 8] {
            cfg.msaa_samples = s;
            assert!(cfg.validate().is_ok(), "msaa={s} must be accepted");
        }
    }

    #[test]
    fn negative_exposure_rejected() {
        let mut cfg = RenderConfig::default();
        cfg.hdr_exposure = -0.1;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
        cfg.hdr_exposure = 0.0;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
        cfg.hdr_exposure = f32::NAN;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
    }

    #[test]
    fn oversized_fps_rejected() {
        let mut cfg = RenderConfig::default();
        cfg.target_fps = 9;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
        cfg.target_fps = 241;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Render(_))));
        cfg.target_fps = 10;
        assert!(cfg.validate().is_ok());
        cfg.target_fps = 240;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = RenderConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: RenderConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, back);
    }
}
