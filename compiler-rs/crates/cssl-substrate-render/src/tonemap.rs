//! § tonemap — Reinhard + ACES + custom-LUT tonemap pipeline.
//!
//! ## Role
//! Per spec § 36 § ALGORITHM step 3 the per-pixel decompressed light value is
//! tonemapped before compositing into the final image. This crate provides
//! three canonical curves :
//!
//!   - Reinhard : `out = x / (1 + x)` ; classic exposure-compressed.
//!   - ACES (approximation) : Krzysztof Narkowicz fit ; cinematic look.
//!   - Custom-LUT : 1D table interpolation for hand-authored curves.

/// Tonemap curve selector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToneCurve {
    /// Reinhard : x / (1 + x).
    Reinhard,
    /// ACES approximation (Narkowicz fit).
    AcesApprox,
    /// Linear (no-op).
    Linear,
}

impl Default for ToneCurve {
    fn default() -> Self {
        ToneCurve::AcesApprox
    }
}

/// 1D LUT for custom tonemap curves.
#[derive(Debug, Clone, PartialEq)]
pub struct ToneLut {
    /// LUT table mapping input ∈ [0,1] → output ∈ [0,1].
    pub table: Vec<f32>,
}

impl ToneLut {
    /// Identity LUT.
    pub fn identity(n: usize) -> Self {
        let mut t = Vec::with_capacity(n);
        for i in 0..n {
            t.push(i as f32 / ((n - 1).max(1)) as f32);
        }
        Self { table: t }
    }

    /// Reinhard-shaped LUT.
    pub fn reinhard(n: usize) -> Self {
        let mut t = Vec::with_capacity(n);
        for i in 0..n {
            let x = i as f32 / ((n - 1).max(1)) as f32;
            // Map x ∈ [0,1] to HDR-range [0, 8] then Reinhard.
            let hdr = x * 8.0;
            let mapped = hdr / (1.0 + hdr);
            t.push(mapped);
        }
        Self { table: t }
    }

    /// Sample the LUT at `t ∈ [0,1]`.
    pub fn sample(&self, t: f32) -> f32 {
        let n = self.table.len();
        if n == 0 {
            return t;
        }
        if n == 1 {
            return self.table[0];
        }
        let tc = t.clamp(0.0, 1.0);
        let pos = tc * ((n - 1) as f32);
        let lo = pos.floor() as usize;
        let hi = (lo + 1).min(n - 1);
        let frac = pos - (lo as f32);
        self.table[lo] * (1.0 - frac) + self.table[hi] * frac
    }
}

/// Apply Reinhard : `x / (1 + x)`.
#[inline]
pub fn tonemap_reinhard(x: f32) -> f32 {
    x / (1.0 + x.max(0.0))
}

/// Apply ACES approximation (Narkowicz).
#[inline]
pub fn tonemap_aces(x: f32) -> f32 {
    let a = 2.51_f32;
    let b = 0.03_f32;
    let c = 2.43_f32;
    let d = 0.59_f32;
    let e = 0.14_f32;
    ((x * (a * x + b)) / (x * (c * x + d) + e)).clamp(0.0, 1.0)
}

/// Tonemap a single pixel with a curve choice + exposure.
pub fn tonemap_pixel(rgb: [f32; 3], exposure: f32, curve: ToneCurve) -> [f32; 3] {
    let exposed = [rgb[0] * exposure, rgb[1] * exposure, rgb[2] * exposure];
    match curve {
        ToneCurve::Reinhard => [
            tonemap_reinhard(exposed[0]),
            tonemap_reinhard(exposed[1]),
            tonemap_reinhard(exposed[2]),
        ],
        ToneCurve::AcesApprox => [
            tonemap_aces(exposed[0]),
            tonemap_aces(exposed[1]),
            tonemap_aces(exposed[2]),
        ],
        ToneCurve::Linear => [
            exposed[0].clamp(0.0, 1.0),
            exposed[1].clamp(0.0, 1.0),
            exposed[2].clamp(0.0, 1.0),
        ],
    }
}

/// Tonemap entry point with optional LUT.
#[derive(Debug, Clone)]
pub struct ToneMapper {
    pub curve: ToneCurve,
    pub exposure: f32,
    pub custom_lut: Option<ToneLut>,
}

impl Default for ToneMapper {
    fn default() -> Self {
        Self {
            curve: ToneCurve::AcesApprox,
            exposure: 1.0,
            custom_lut: None,
        }
    }
}

impl ToneMapper {
    /// Tonemap one pixel.
    pub fn map(&self, rgb: [f32; 3]) -> [f32; 3] {
        if let Some(lut) = &self.custom_lut {
            let exposed = [rgb[0] * self.exposure, rgb[1] * self.exposure, rgb[2] * self.exposure];
            return [
                lut.sample(exposed[0].clamp(0.0, 1.0)),
                lut.sample(exposed[1].clamp(0.0, 1.0)),
                lut.sample(exposed[2].clamp(0.0, 1.0)),
            ];
        }
        tonemap_pixel(rgb, self.exposure, self.curve)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reinhard_zero_is_zero() {
        assert_eq!(tonemap_reinhard(0.0), 0.0);
    }

    #[test]
    fn reinhard_clamps_high_values_below_one() {
        for v in [1.0, 4.0, 16.0, 1024.0] {
            assert!(tonemap_reinhard(v) < 1.0);
        }
    }

    #[test]
    fn aces_clamps_to_unit_interval() {
        for v in [0.0, 0.5, 1.0, 4.0, 100.0] {
            let r = tonemap_aces(v);
            assert!((0.0..=1.0).contains(&r), "ACES out-of-range for v={v}: r={r}");
        }
    }

    #[test]
    fn linear_clamps_negative() {
        let p = tonemap_pixel([-0.5, 0.5, 1.5], 1.0, ToneCurve::Linear);
        assert_eq!(p[0], 0.0);
        assert_eq!(p[1], 0.5);
        assert_eq!(p[2], 1.0);
    }

    #[test]
    fn lut_identity_passes_through() {
        let lut = ToneLut::identity(256);
        for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!((lut.sample(x) - x).abs() < 1e-3);
        }
    }

    #[test]
    fn lut_reinhard_below_one_at_unity_input() {
        // Reinhard LUT compresses the [0,1] HDR range up to 8.0 ;
        // the LUT ends at output = 8/(1+8) = 0.888..., so unity input
        // never reaches unity output.
        let lut = ToneLut::reinhard(256);
        assert!(lut.sample(1.0) < 1.0);
        // The curve is monotonic-increasing.
        let a = lut.sample(0.1);
        let b = lut.sample(0.5);
        let c = lut.sample(0.9);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn tonemapper_default_is_aces() {
        let t = ToneMapper::default();
        assert_eq!(t.curve, ToneCurve::AcesApprox);
        assert_eq!(t.exposure, 1.0);
    }

    #[test]
    fn tonemapper_uses_lut_when_present() {
        let t = ToneMapper {
            curve: ToneCurve::Linear,
            exposure: 1.0,
            custom_lut: Some(ToneLut::identity(8)),
        };
        let p = t.map([0.5, 0.5, 0.5]);
        for c in 0..3 {
            assert!((p[c] - 0.5).abs() < 1e-2);
        }
    }

    #[test]
    fn exposure_scales_input() {
        let p1 = tonemap_pixel([0.25, 0.25, 0.25], 1.0, ToneCurve::Linear);
        let p2 = tonemap_pixel([0.25, 0.25, 0.25], 2.0, ToneCurve::Linear);
        assert!(p2[0] > p1[0]);
    }
}
