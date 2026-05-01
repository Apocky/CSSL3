//! § config — `StereoConfig` + `StereoErr` validation.
//!
//! § INVARIANTS
//!   - `ipd_meters > 0.0` (zero IPD collapses to mono — caller should pick a
//!     mono path explicitly rather than passing zero through here).
//!   - `eye_separation_dir` has non-zero magnitude.
//!   - No NaN / no infinity in any field.
//!
//! § DEFAULTS
//!   - IPD = 0.063 m (63 mm) — adult-human median per anthropometric tables.
//!   - eye-height = 1.65 m  — adult-human standing eye-height median.
//!   - convergence = 2.0 m  — comfortable-focus mid-distance for stereoscopic
//!                            content; toes the eye-pair in by atan(IPD/2 / 2.0).
//!   - separation-dir = +X (right-vector in world-space).

use serde::{Deserialize, Serialize};

/// Validation errors for [`StereoConfig`].
///
/// § discriminator : the field that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StereoErr {
    /// `ipd_meters <= 0.0`.
    ZeroIpd,
    /// `eye_separation_dir` magnitude is zero.
    ZeroSeparation,
    /// Some f32 field is NaN or infinity.
    NaN,
}

impl core::fmt::Display for StereoErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZeroIpd => write!(f, "stereo : ipd_meters must be > 0.0"),
            Self::ZeroSeparation => write!(f, "stereo : eye_separation_dir must have non-zero magnitude"),
            Self::NaN => write!(f, "stereo : NaN or infinity in config"),
        }
    }
}

impl std::error::Error for StereoErr {}

/// Stereoscopic camera configuration.
///
/// § FIELDS
///   - `ipd_meters` : inter-pupillary distance in meters (default 0.063).
///   - `eye_height_meters` : standing eye-height in meters (default 1.65) —
///     informational ; not used by [`crate::geometry::eye_pair_from_mono`]
///     directly (the caller already encodes y-position into the mono pose).
///   - `convergence_meters` : focal-convergence distance in meters
///     (default 2.0) ; eyes are toed-in by `atan((ipd/2) / convergence)`.
///   - `eye_separation_dir` : world-space right-vector, NORMALIZED on
///     `validate()`-call sites (default `[1.0, 0.0, 0.0]`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StereoConfig {
    pub ipd_meters: f32,
    pub eye_height_meters: f32,
    pub convergence_meters: f32,
    pub eye_separation_dir: [f32; 3],
}

impl Default for StereoConfig {
    fn default() -> Self {
        Self {
            ipd_meters: 0.063,
            eye_height_meters: 1.65,
            convergence_meters: 2.0,
            eye_separation_dir: [1.0, 0.0, 0.0],
        }
    }
}

impl StereoConfig {
    /// Validate field invariants. Returns the first error encountered ; field
    /// order : NaN-check → IPD → separation-magnitude.
    pub fn validate(&self) -> Result<(), StereoErr> {
        if !self.ipd_meters.is_finite()
            || !self.eye_height_meters.is_finite()
            || !self.convergence_meters.is_finite()
            || !self.eye_separation_dir[0].is_finite()
            || !self.eye_separation_dir[1].is_finite()
            || !self.eye_separation_dir[2].is_finite()
        {
            return Err(StereoErr::NaN);
        }
        if self.ipd_meters <= 0.0 {
            return Err(StereoErr::ZeroIpd);
        }
        let mag2 = self.eye_separation_dir[0].mul_add(
            self.eye_separation_dir[0],
            self.eye_separation_dir[1].mul_add(
                self.eye_separation_dir[1],
                self.eye_separation_dir[2] * self.eye_separation_dir[2],
            ),
        );
        if mag2 <= f32::EPSILON {
            return Err(StereoErr::ZeroSeparation);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_validates() {
        let cfg = StereoConfig::default();
        assert!(cfg.validate().is_ok());
        assert!((cfg.ipd_meters - 0.063).abs() < 1e-6);
        assert!((cfg.convergence_meters - 2.0).abs() < 1e-6);
    }

    #[test]
    fn zero_ipd_rejected() {
        let cfg = StereoConfig { ipd_meters: 0.0, ..StereoConfig::default() };
        assert_eq!(cfg.validate(), Err(StereoErr::ZeroIpd));
        let cfg = StereoConfig { ipd_meters: -0.01, ..StereoConfig::default() };
        assert_eq!(cfg.validate(), Err(StereoErr::ZeroIpd));
    }

    #[test]
    fn zero_sep_rejected() {
        let cfg = StereoConfig { eye_separation_dir: [0.0, 0.0, 0.0], ..StereoConfig::default() };
        assert_eq!(cfg.validate(), Err(StereoErr::ZeroSeparation));
    }

    #[test]
    fn nan_rejected() {
        let cfg = StereoConfig { ipd_meters: f32::NAN, ..StereoConfig::default() };
        assert_eq!(cfg.validate(), Err(StereoErr::NaN));
        let cfg = StereoConfig { eye_separation_dir: [0.0, f32::INFINITY, 0.0], ..StereoConfig::default() };
        assert_eq!(cfg.validate(), Err(StereoErr::NaN));
    }
}
