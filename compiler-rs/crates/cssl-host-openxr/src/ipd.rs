//! Inter-pupillary distance (IPD) calibration.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § III bound :
//!   `ipd_mm: f32 { v in 50.0..=80.0 }`.

use crate::error::XRFailure;

/// Mid-IPD default for adult average. § III canonical.
pub const DEFAULT_IPD_MM: f32 = 64.0;

/// IPD bounds. § III.
pub const IPD_MIN_MM: f32 = 50.0;
pub const IPD_MAX_MM: f32 = 80.0;

/// IPD value with bound-checking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ipd {
    mm: f32,
}

impl Ipd {
    /// Construct, validating bounds.
    pub fn new(mm: f32) -> Result<Self, XRFailure> {
        if !(IPD_MIN_MM..=IPD_MAX_MM).contains(&mm) {
            return Err(XRFailure::IpdOutOfRange { got: mm });
        }
        Ok(Self { mm })
    }

    /// Default 64mm.
    #[must_use]
    pub const fn default_64() -> Self {
        Self {
            mm: DEFAULT_IPD_MM,
        }
    }

    /// Saturating constructor : clamps to bounds.
    #[must_use]
    pub fn clamped(mm: f32) -> Self {
        Self {
            mm: mm.clamp(IPD_MIN_MM, IPD_MAX_MM),
        }
    }

    /// Get the value.
    #[must_use]
    pub const fn mm(self) -> f32 {
        self.mm
    }

    /// Convert to meters.
    #[must_use]
    pub fn meters(self) -> f32 {
        self.mm * 0.001
    }

    /// Eye-offset along the X-axis : ±IPD/2.
    #[must_use]
    pub fn eye_offset_x(self, side: EyeSide) -> f32 {
        let half = self.meters() * 0.5;
        match side {
            EyeSide::Left => -half,
            EyeSide::Right => half,
        }
    }
}

/// Which eye.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EyeSide {
    /// Left eye.
    Left,
    /// Right eye.
    Right,
}

impl EyeSide {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EyeSide, Ipd, DEFAULT_IPD_MM, IPD_MAX_MM, IPD_MIN_MM};

    #[test]
    fn default_64() {
        let i = Ipd::default_64();
        assert_eq!(i.mm(), DEFAULT_IPD_MM);
    }

    #[test]
    fn new_in_range() {
        assert!(Ipd::new(56.0).is_ok());
        assert!(Ipd::new(72.0).is_ok());
    }

    #[test]
    fn new_at_bounds() {
        assert!(Ipd::new(IPD_MIN_MM).is_ok());
        assert!(Ipd::new(IPD_MAX_MM).is_ok());
    }

    #[test]
    fn new_out_of_range_low() {
        assert!(Ipd::new(40.0).is_err());
    }

    #[test]
    fn new_out_of_range_high() {
        assert!(Ipd::new(100.0).is_err());
    }

    #[test]
    fn clamped_low_clamps_to_min() {
        let i = Ipd::clamped(20.0);
        assert_eq!(i.mm(), IPD_MIN_MM);
    }

    #[test]
    fn clamped_high_clamps_to_max() {
        let i = Ipd::clamped(200.0);
        assert_eq!(i.mm(), IPD_MAX_MM);
    }

    #[test]
    fn meters_conversion() {
        let i = Ipd::default_64();
        assert!((i.meters() - 0.064).abs() < 1e-7);
    }

    #[test]
    fn eye_offset_left_negative() {
        let i = Ipd::default_64();
        assert!(i.eye_offset_x(EyeSide::Left) < 0.0);
        assert!(i.eye_offset_x(EyeSide::Right) > 0.0);
    }

    #[test]
    fn eye_offset_symmetric() {
        let i = Ipd::default_64();
        let l = i.eye_offset_x(EyeSide::Left);
        let r = i.eye_offset_x(EyeSide::Right);
        assert!((l + r).abs() < 1e-7);
    }

    #[test]
    fn eye_side_as_str() {
        assert_eq!(EyeSide::Left.as_str(), "left");
        assert_eq!(EyeSide::Right.as_str(), "right");
    }
}
