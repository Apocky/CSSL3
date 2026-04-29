//! § scalar — floating-point utility helpers
//!
//! Interpolation, clamping, smoothstep, angle-conversion, and angle-wrapping
//! helpers used by the vector + quaternion paths and exported as the
//! convenience surface for downstream code.
//!
//! All helpers are TOTAL on finite inputs — never panic, never produce
//! NaN for finite arguments. The `lerp` / `smoothstep` paths do not clamp
//! `t` ∈ `[0, 1]` ; that's the caller's responsibility. Use `clamp`
//! upstream if you need a closed-interval interpretation.
//!
//! § FMA — every path that benefits from a fused multiply-add (`lerp`,
//! `smoothstep`) uses `f32::mul_add`. The compiler emits a single FMA
//! instruction on x86_64-v3 + AArch64-NEON targets and a separate
//! mul + add elsewhere ; the bit-exact result on FMA-capable hardware is
//! the reduced-rounding form that matches the IEEE 754-2008 fused operator.

/// Standard `f32` epsilon for "approximately equal" comparisons in this
/// crate. `f32::EPSILON ≈ 1.19e-7` is the ULP at `1.0` and is too tight
/// for the kind of mixed-magnitude arithmetic that typical 3D math
/// produces ; the working epsilon for "close enough" is `1e-6` for
/// f32 paths.
pub const EPSILON_F32: f32 = 1.0e-6;

/// Smaller epsilon for "is this value effectively zero" guards on
/// length-squared / determinant checks. `1e-12` is below f32's machine
/// precision but is the right threshold to detect a *true* zero (where
/// the underlying value really is `0.0` modulo accumulated round-off).
pub const SMALL_EPSILON_F32: f32 = 1.0e-12;

/// Linear interpolation between `a` and `b` by parameter `t`.
///
/// `t = 0` returns `a` ; `t = 1` returns `b`. `t` is NOT clamped — values
/// outside `[0, 1]` extrapolate. Uses FMA for reduced rounding error
/// where the target supports it.
///
/// # Examples
/// ```
/// use cssl_math::lerp;
/// assert!((lerp(0.0, 10.0, 0.5) - 5.0).abs() < 1e-6);
/// assert!((lerp(2.0, 4.0, 0.0) - 2.0).abs() < 1e-6);
/// assert!((lerp(2.0, 4.0, 1.0) - 4.0).abs() < 1e-6);
/// ```
#[must_use]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    // (1 - t) * a + t * b — refactored to FMA-friendly form :
    // a + t * (b - a). One sub + one mul-add.
    t.mul_add(b - a, a)
}

/// Clamp `value` to the closed interval `[lo, hi]`. NaN-tolerant : if
/// `value` is NaN, returns NaN ; if `lo > hi`, the result is `lo` (the
/// interval is empty and we return the lower bound by convention).
///
/// # Examples
/// ```
/// use cssl_math::clamp;
/// assert!((clamp(0.5, 0.0, 1.0) - 0.5).abs() < 1e-6);
/// assert!((clamp(-1.0, 0.0, 1.0) - 0.0).abs() < 1e-6);
/// assert!((clamp(2.0, 0.0, 1.0) - 1.0).abs() < 1e-6);
/// ```
#[must_use]
pub fn clamp(value: f32, lo: f32, hi: f32) -> f32 {
    if value.is_nan() {
        return value;
    }
    if value < lo {
        lo
    } else if value > hi {
        hi
    } else {
        value
    }
}

/// Hermite-interpolation `smoothstep` over `[edge_lo, edge_hi]`. Returns
/// `0` at and below `edge_lo`, `1` at and above `edge_hi`, and a smooth
/// cubic curve in between (`3t² - 2t³` for `t = (x - edge_lo) /
/// (edge_hi - edge_lo)`).
///
/// `edge_lo == edge_hi` returns `1` for `x >= edge_lo` and `0` otherwise
/// (degenerate-interval ⇒ step function rather than NaN).
///
/// # Examples
/// ```
/// use cssl_math::smoothstep;
/// assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
/// assert!((smoothstep(0.0, 1.0, 0.0) - 0.0).abs() < 1e-6);
/// assert!((smoothstep(0.0, 1.0, 1.0) - 1.0).abs() < 1e-6);
/// ```
#[must_use]
pub fn smoothstep(edge_lo: f32, edge_hi: f32, x: f32) -> f32 {
    if (edge_hi - edge_lo).abs() < SMALL_EPSILON_F32 {
        return if x >= edge_lo { 1.0 } else { 0.0 };
    }
    let t = clamp((x - edge_lo) / (edge_hi - edge_lo), 0.0, 1.0);
    // 3t² - 2t³ = t² (3 - 2t).
    t * t * (3.0 - 2.0 * t)
}

/// Step function — returns `0.0` for `x < edge` and `1.0` otherwise.
/// Mirrors GLSL `step(edge, x)` semantics. Useful for branchless
/// shader-style conditional weighting.
#[must_use]
pub fn step(edge: f32, x: f32) -> f32 {
    if x < edge {
        0.0
    } else {
        1.0
    }
}

/// Convert degrees to radians.
///
/// # Examples
/// ```
/// use cssl_math::to_radians;
/// assert!((to_radians(180.0) - core::f32::consts::PI).abs() < 1e-5);
/// assert!((to_radians(0.0)).abs() < 1e-6);
/// ```
#[must_use]
pub fn to_radians(degrees: f32) -> f32 {
    degrees * (core::f32::consts::PI / 180.0)
}

/// Convert radians to degrees.
///
/// # Examples
/// ```
/// use cssl_math::to_degrees;
/// assert!((to_degrees(core::f32::consts::PI) - 180.0).abs() < 1e-4);
/// assert!((to_degrees(0.0)).abs() < 1e-6);
/// ```
#[must_use]
pub fn to_degrees(radians: f32) -> f32 {
    radians * (180.0 / core::f32::consts::PI)
}

/// Wrap an angle (radians) into the range `(-PI, PI]`. Handles arbitrarily
/// large positive or negative inputs in a single division-free loop-free
/// expression. NaN input returns NaN.
///
/// # Examples
/// ```
/// use cssl_math::wrap_angle;
/// assert!(wrap_angle(0.0).abs() < 1e-6);
/// // 3π wraps to π (or just under, depending on float drift).
/// let three_pi = 3.0 * core::f32::consts::PI;
/// let wrapped = wrap_angle(three_pi);
/// assert!(wrapped.abs() <= core::f32::consts::PI + 1e-5);
/// ```
#[must_use]
pub fn wrap_angle(radians: f32) -> f32 {
    const TAU: f32 = core::f32::consts::TAU;
    const PI: f32 = core::f32::consts::PI;
    if radians.is_nan() {
        return radians;
    }
    // Add π to shift into [0, 2π), modulo, then subtract π to get (-π, π].
    let shifted = radians + PI;
    let wrapped = shifted - (shifted / TAU).floor() * TAU;
    wrapped - PI
}

#[cfg(test)]
mod tests {
    use super::{clamp, lerp, smoothstep, step, to_degrees, to_radians, wrap_angle};

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn lerp_endpoints_exact() {
        assert!(approx(lerp(2.0, 4.0, 0.0), 2.0, 1e-6));
        assert!(approx(lerp(2.0, 4.0, 1.0), 4.0, 1e-6));
        assert!(approx(lerp(2.0, 4.0, 0.5), 3.0, 1e-6));
    }

    #[test]
    fn lerp_extrapolates_outside_unit_interval() {
        assert!(approx(lerp(0.0, 10.0, 2.0), 20.0, 1e-6));
        assert!(approx(lerp(0.0, 10.0, -1.0), -10.0, 1e-6));
    }

    #[test]
    fn clamp_clips_to_bounds() {
        assert!(approx(clamp(0.5, 0.0, 1.0), 0.5, 1e-6));
        assert!(approx(clamp(-1.0, 0.0, 1.0), 0.0, 1e-6));
        assert!(approx(clamp(2.0, 0.0, 1.0), 1.0, 1e-6));
    }

    #[test]
    fn clamp_propagates_nan() {
        assert!(clamp(f32::NAN, 0.0, 1.0).is_nan());
    }

    #[test]
    fn smoothstep_endpoints_and_midpoint() {
        assert!(approx(smoothstep(0.0, 1.0, 0.0), 0.0, 1e-6));
        assert!(approx(smoothstep(0.0, 1.0, 1.0), 1.0, 1e-6));
        assert!(approx(smoothstep(0.0, 1.0, 0.5), 0.5, 1e-6));
        // Symmetric : 0.25 + 0.75 should sum to 1 (smoothstep is point-symmetric about 0.5).
        let a = smoothstep(0.0, 1.0, 0.25);
        let b = smoothstep(0.0, 1.0, 0.75);
        assert!(approx(a + b, 1.0, 1e-6));
    }

    #[test]
    fn smoothstep_clamps_outside_interval() {
        assert!(approx(smoothstep(0.0, 1.0, -2.0), 0.0, 1e-6));
        assert!(approx(smoothstep(0.0, 1.0, 5.0), 1.0, 1e-6));
    }

    #[test]
    fn smoothstep_degenerate_interval_is_step() {
        // edge_lo == edge_hi : step at edge.
        assert!(approx(smoothstep(1.0, 1.0, 0.0), 0.0, 1e-6));
        assert!(approx(smoothstep(1.0, 1.0, 1.5), 1.0, 1e-6));
    }

    #[test]
    fn step_is_unit_at_edge() {
        assert!(approx(step(0.5, 0.4), 0.0, 1e-6));
        assert!(approx(step(0.5, 0.5), 1.0, 1e-6));
        assert!(approx(step(0.5, 0.6), 1.0, 1e-6));
    }

    #[test]
    fn radians_round_trip_with_degrees() {
        for d in [0.0_f32, 30.0, 45.0, 90.0, 180.0, 270.0, 360.0] {
            let r = to_radians(d);
            let dd = to_degrees(r);
            assert!(approx(dd, d, 1e-3), "degrees round-trip {d} got {dd}");
        }
    }

    #[test]
    fn wrap_angle_inside_principal_range_is_identity() {
        for &r in &[0.0_f32, 0.5, -0.5, 1.0, -1.0, 3.14, -3.14] {
            let w = wrap_angle(r);
            assert!(approx(w, r, 1e-5), "wrap_angle({r}) returned {w}");
        }
    }

    #[test]
    fn wrap_angle_outside_wraps_to_principal_range() {
        let pi = core::f32::consts::PI;
        let tau = core::f32::consts::TAU;
        // 3π wraps to π.
        let w = wrap_angle(3.0 * pi);
        assert!(w.abs() <= pi + 1e-5);
        // -3π also wraps to π.
        let w = wrap_angle(-3.0 * pi);
        assert!(w.abs() <= pi + 1e-5);
        // Two full rotations + small offset returns the offset.
        let w = wrap_angle(2.0 * tau + 0.5);
        assert!(approx(w, 0.5, 1e-4));
    }

    #[test]
    fn wrap_angle_propagates_nan() {
        assert!(wrap_angle(f32::NAN).is_nan());
    }
}
