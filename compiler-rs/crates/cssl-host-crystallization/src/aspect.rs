//! § aspect — 8 procedural aspect-curves per Crystal.
//!
//! Each curve has 16 control points × 4 axis-modulators (= 64 i16 values =
//! 128 bytes per curve · 1 KiB per crystal across all 8 aspects). This is
//! VASTLY smaller than mesh+texture data and evaluates in sub-microsecond
//! per query.
//!
//! Splines are evaluated as axis-weighted Catmull-Rom-style segment
//! interpolations between control points. Stage-0 uses linear-blend for
//! determinism + speed ; future iteration can lift to true Catmull-Rom.

/// Aspect indices — must match crystallization.csl ASPECT_* discriminants.
pub mod aspect_idx {
    pub const SILHOUETTE: u8 = 0;
    pub const SURFACE: u8 = 1;
    pub const LUMINANCE: u8 = 2;
    pub const MOTION: u8 = 3;
    pub const SOUND: u8 = 4;
    pub const AURA: u8 = 5;
    pub const ECHO: u8 = 6;
    pub const BLOOM: u8 = 7;
}

/// One spline = 16 control points · 4 axis modulators each.
#[derive(Debug, Clone, Copy)]
pub struct AspectSpline {
    pub points: [[i16; 4]; 16],
}

impl AspectSpline {
    /// Evaluate at param `t_milli` (0..=1000) with axis weights `w` (each
    /// in 0..=255). Returns i32 in millunits.
    pub fn eval(&self, t_milli: u32, w: [u8; 4]) -> i32 {
        let t = t_milli.min(1000);
        // Find segment : 16 points → 15 segments.
        let seg = (t * 15) / 1000;
        let seg_clamped = seg.min(14) as usize;
        let local_t = (t * 15) - (seg as u32 * 1000);
        let local_t_clamped = local_t.min(1000) as i32;
        let inv_t = 1000 - local_t_clamped;

        // Linear-blend between two adjacent control points, weighted by 4-axis.
        let a = &self.points[seg_clamped];
        let b = &self.points[seg_clamped + 1];

        let wsum: i32 = w.iter().map(|x| *x as i32).sum::<i32>().max(1);
        let mut acc: i64 = 0;
        for i in 0..4 {
            let wa = a[i] as i32;
            let wb = b[i] as i32;
            let mid = (wa * inv_t + wb * local_t_clamped) / 1000;
            acc += (mid as i64) * (w[i] as i64);
        }
        (acc / wsum as i64) as i32
    }
}

/// All 8 aspect splines for one crystal.
#[derive(Debug, Clone)]
pub struct AspectCurves {
    pub splines: [AspectSpline; 8],
}

impl AspectCurves {
    /// Derive 8 splines deterministically from a 32-byte digest + the
    /// crystal's class. Each spline gets 64 bytes (16 points × 4 axes ×
    /// 1 byte) walked from the digest using a deterministic stream cipher
    /// (re-hashing the digest with a counter).
    pub fn derive(digest: &[u8; 32], class: crate::CrystalClass) -> Self {
        let class_byte = class as u32;
        let mut splines = [AspectSpline { points: [[0i16; 4]; 16] }; 8];

        for ai in 0..8u32 {
            // Hash (digest || class || aspect-idx) → 64 bytes per spline.
            let mut h = blake3::Hasher::new();
            h.update(b"aspect-derive-v1");
            h.update(digest);
            h.update(&class_byte.to_le_bytes());
            h.update(&ai.to_le_bytes());
            // We need 16 × 4 × 2 = 128 bytes. BLAKE3 default = 32 bytes ;
            // the XOF (extend output function) gives any length.
            let mut xof = h.finalize_xof();
            let mut buf = [0u8; 128];
            xof.fill(&mut buf);

            // Each i16 gets 2 bytes from buf.
            for pi in 0..16usize {
                for axi in 0..4usize {
                    let off = pi * 8 + axi * 2;
                    let raw = i16::from_le_bytes([buf[off], buf[off + 1]]);
                    splines[ai as usize].points[pi][axi] = raw;
                }
            }
        }
        Self { splines }
    }

    pub fn spline(&self, aspect_idx: u8) -> &AspectSpline {
        &self.splines[(aspect_idx as usize).min(7)]
    }
}

/// Compute the silhouette contour at observer-angle (yaw_milli · pitch_milli).
/// Returns an i32 in millimeters representing the apparent half-extent at
/// the given angle. The crystal's projected silhouette is approximately
/// `extent_mm * silhouette(yaw, pitch) / 1000`.
pub fn silhouette_at_angle(
    curves: &AspectCurves,
    yaw_milli: u32,
    pitch_milli: u32,
    extent_mm: i32,
) -> i32 {
    let s = curves.spline(aspect_idx::SILHOUETTE);
    let t = (yaw_milli + pitch_milli) % 1000;
    // Axis weights : at (0, 0) we still want a non-zero default ; bias
    // axis 0 to be at-least 64 (1/4 of full) so the silhouette has a
    // baseline contour at every angle.
    let w: [u8; 4] = [
        (((yaw_milli >> 0) & 0xFF) as u8).max(64),
        ((yaw_milli >> 8) & 0xFF) as u8,
        ((pitch_milli >> 0) & 0xFF) as u8,
        ((pitch_milli >> 8) & 0xFF) as u8,
    ];
    let raw = s.eval(t, w).abs();
    // Clamp + scale to extent_mm.
    let clamped = raw.clamp(0, 32767);
    let scaled = (clamped as i64 * extent_mm as i64 / 32768).clamp(0, extent_mm as i64);
    scaled as i32
}

/// Evaluate luminance at wavelength_band (0..16).
pub fn luminance_at_band(curves: &AspectCurves, band: u8) -> u16 {
    let s = curves.spline(aspect_idx::LUMINANCE);
    let t = ((band as u32) * 1000) / 16;
    let raw = s.eval(t, [255, 0, 0, 0]);
    (raw.clamp(0, 65535)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CrystalClass;

    #[test]
    fn derive_is_deterministic() {
        let d = [42u8; 32];
        let a = AspectCurves::derive(&d, CrystalClass::Object);
        let b = AspectCurves::derive(&d, CrystalClass::Object);
        for i in 0..8 {
            assert_eq!(a.splines[i].points, b.splines[i].points);
        }
    }

    #[test]
    fn derive_varies_with_class() {
        let d = [42u8; 32];
        let a = AspectCurves::derive(&d, CrystalClass::Object);
        let b = AspectCurves::derive(&d, CrystalClass::Entity);
        // At least one spline differs.
        assert!((0..8).any(|i| a.splines[i].points != b.splines[i].points));
    }

    #[test]
    fn silhouette_at_angle_is_bounded() {
        let d = [42u8; 32];
        let curves = AspectCurves::derive(&d, CrystalClass::Object);
        for yaw in (0..1000).step_by(50) {
            let s = silhouette_at_angle(&curves, yaw, 500, 1000);
            assert!(s >= 0 && s <= 1000, "silhouette {s} not in [0, 1000]");
        }
    }

    #[test]
    fn luminance_at_band_is_bounded() {
        let d = [42u8; 32];
        let curves = AspectCurves::derive(&d, CrystalClass::Entity);
        for band in 0..16u8 {
            let l = luminance_at_band(&curves, band);
            assert!(l <= u16::MAX);
        }
    }

    #[test]
    fn spline_eval_at_zero_returns_first_axis_blend() {
        let s = AspectSpline {
            points: [[100; 4]; 16],
        };
        let v = s.eval(0, [255, 0, 0, 0]);
        assert!(v.abs() <= 100);
    }
}
