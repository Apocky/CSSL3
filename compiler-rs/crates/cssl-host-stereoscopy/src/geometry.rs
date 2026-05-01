//! § geometry — Stereo eye-pair derivation from a monocular camera pose.
//!
//! § PIPELINE
//!   - `right_vec = normalize(cross(forward, up))`
//!     [defended against degenerate up via parallelism check]
//!   - `up_vec   = normalize(cross(right_vec, forward))`
//!   - `left_pos  = mono_pos - right_vec * (ipd / 2)`
//!   - `right_pos = mono_pos + right_vec * (ipd / 2)`
//!   - toe-in angle θ = atan((ipd / 2) / convergence)
//!   - `left_forward  = rotate(forward, up_vec,  +θ)`  // toes right
//!   - `right_forward = rotate(forward, up_vec,  -θ)`  // toes left
//!
//! § INVARIANTS PRESERVED
//!   - Per-eye `forward` and `up` remain unit-length (within f32 ulp tolerance).
//!   - Per-eye `forward · up ≈ 0` (perpendicularity preserved).

use crate::config::{StereoConfig, StereoErr};
use serde::{Deserialize, Serialize};

/// A single eye pose in world-space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EyePose {
    pub pos: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

/// A stereoscopic eye-pair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EyePair {
    pub left: EyePose,
    pub right: EyePose,
}

// ─── linalg helpers ─────────────────────────────────────────────────

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0].mul_add(b[0], a[1].mul_add(b[1], a[2] * b[2]))
}

#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

#[inline]
fn magnitude(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

#[inline]
fn normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let m = magnitude(v);
    if m <= f32::EPSILON || !m.is_finite() {
        None
    } else {
        let inv = 1.0 / m;
        Some([v[0] * inv, v[1] * inv, v[2] * inv])
    }
}

#[inline]
fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

#[inline]
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Rodrigues rotation : rotate `v` around unit-axis `k` by angle `theta` (rad).
#[inline]
fn rotate_axis_angle(v: [f32; 3], k: [f32; 3], theta: f32) -> [f32; 3] {
    let (s, c) = theta.sin_cos();
    let kxv = cross(k, v);
    let kdotv = dot(k, v);
    // v*cosθ + (k×v)*sinθ + k*(k·v)*(1-cosθ)
    let one_minus_c = 1.0 - c;
    [
        v[0].mul_add(c, kxv[0].mul_add(s, k[0] * kdotv * one_minus_c)),
        v[1].mul_add(c, kxv[1].mul_add(s, k[1] * kdotv * one_minus_c)),
        v[2].mul_add(c, kxv[2].mul_add(s, k[2] * kdotv * one_minus_c)),
    ]
}

/// Build the orthonormal basis `(forward, up_perp, right)` from a possibly
/// non-orthogonal `(forward, up)`. Returns `None` if `forward` and `up` are
/// parallel (degenerate) or zero-length.
fn ortho_basis(forward: [f32; 3], up: [f32; 3]) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
    let f = normalize(forward)?;
    let u_in = normalize(up)?;
    let right = normalize(cross(f, u_in))?;
    let up_perp = normalize(cross(right, f))?;
    Some((f, up_perp, right))
}

// ─── public API ─────────────────────────────────────────────────────

/// Left-eye position : `mono - right_vec * (ipd/2)`.
pub fn left_eye_position(mono_pos: [f32; 3], mono_forward: [f32; 3], mono_up: [f32; 3], cfg: &StereoConfig) -> Result<[f32; 3], StereoErr> {
    cfg.validate()?;
    let (_f, _u, right) = ortho_basis(mono_forward, mono_up).ok_or(StereoErr::ZeroSeparation)?;
    Ok(sub(mono_pos, scale(right, cfg.ipd_meters * 0.5)))
}

/// Right-eye position : `mono + right_vec * (ipd/2)`.
pub fn right_eye_position(mono_pos: [f32; 3], mono_forward: [f32; 3], mono_up: [f32; 3], cfg: &StereoConfig) -> Result<[f32; 3], StereoErr> {
    cfg.validate()?;
    let (_f, _u, right) = ortho_basis(mono_forward, mono_up).ok_or(StereoErr::ZeroSeparation)?;
    Ok(add(mono_pos, scale(right, cfg.ipd_meters * 0.5)))
}

/// Left-eye forward : mono-forward toed-IN toward the convergence point.
/// The left eye is offset to the LEFT (along -right_vec), so to look at the
/// convergence-point on the mono-forward midline it must rotate so its forward
/// gains a +right_vec component. With our right-hand basis (up = +Y when
/// forward = -Z), this is a NEGATIVE rotation around the up-axis.
pub fn left_eye_forward(mono_forward: [f32; 3], mono_up: [f32; 3], cfg: &StereoConfig) -> Result<[f32; 3], StereoErr> {
    cfg.validate()?;
    let (f, u, _r) = ortho_basis(mono_forward, mono_up).ok_or(StereoErr::ZeroSeparation)?;
    let theta = (cfg.ipd_meters * 0.5 / cfg.convergence_meters).atan();
    Ok(rotate_axis_angle(f, u, -theta))
}

/// Right-eye forward : mono-forward toed-IN toward the convergence point.
/// The right eye is offset to the RIGHT (along +right_vec), so its forward must
/// gain a -right_vec component — POSITIVE rotation around the up-axis under our
/// right-hand basis.
pub fn right_eye_forward(mono_forward: [f32; 3], mono_up: [f32; 3], cfg: &StereoConfig) -> Result<[f32; 3], StereoErr> {
    cfg.validate()?;
    let (f, u, _r) = ortho_basis(mono_forward, mono_up).ok_or(StereoErr::ZeroSeparation)?;
    let theta = (cfg.ipd_meters * 0.5 / cfg.convergence_meters).atan();
    Ok(rotate_axis_angle(f, u, theta))
}

/// Derive the full stereoscopic [`EyePair`] from a monocular pose.
pub fn eye_pair_from_mono(mono_pos: [f32; 3], mono_forward: [f32; 3], mono_up: [f32; 3], cfg: &StereoConfig) -> Result<EyePair, StereoErr> {
    cfg.validate()?;
    let (f, u_perp, right) = ortho_basis(mono_forward, mono_up).ok_or(StereoErr::ZeroSeparation)?;
    let half_ipd = cfg.ipd_meters * 0.5;
    let theta = (half_ipd / cfg.convergence_meters).atan();

    let left_pos = sub(mono_pos, scale(right, half_ipd));
    let right_pos = add(mono_pos, scale(right, half_ipd));
    // Left-eye toes toward +right_vec (sign : -theta in right-hand basis).
    // Right-eye toes toward -right_vec (sign : +theta in right-hand basis).
    let left_forward = rotate_axis_angle(f, u_perp, -theta);
    let right_forward = rotate_axis_angle(f, u_perp, theta);

    Ok(EyePair {
        left: EyePose { pos: left_pos, forward: left_forward, up: u_perp },
        right: EyePose { pos: right_pos, forward: right_forward, up: u_perp },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONO_POS: [f32; 3] = [0.0, 1.65, 0.0];
    const FWD: [f32; 3] = [0.0, 0.0, -1.0];
    const UP: [f32; 3] = [0.0, 1.0, 0.0];

    #[test]
    fn default_ipd_shifts_symmetric() {
        let cfg = StereoConfig::default();
        let pair = eye_pair_from_mono(MONO_POS, FWD, UP, &cfg).unwrap();
        let half = cfg.ipd_meters * 0.5;
        // right-vector here is +X (cross(forward=-Z, up=+Y) = +X)
        assert!((pair.left.pos[0] - (MONO_POS[0] - half)).abs() < 1e-6);
        assert!((pair.right.pos[0] - (MONO_POS[0] + half)).abs() < 1e-6);
        // Symmetric around mono on x-axis :
        assert!((pair.left.pos[0] + pair.right.pos[0]).mul_add(0.5, -MONO_POS[0]).abs() < 1e-6);
        assert!((pair.left.pos[1] - MONO_POS[1]).abs() < 1e-6);
        assert!((pair.right.pos[1] - MONO_POS[1]).abs() < 1e-6);
    }

    #[test]
    fn zero_ipd_collapses_to_mono() {
        let cfg = StereoConfig { ipd_meters: 1e-9, ..StereoConfig::default() };
        // 1e-9 is positive so passes validate ; geometry should degenerate to mono.
        let pair = eye_pair_from_mono(MONO_POS, FWD, UP, &cfg).unwrap();
        for i in 0..3 {
            assert!((pair.left.pos[i] - MONO_POS[i]).abs() < 1e-6);
            assert!((pair.right.pos[i] - MONO_POS[i]).abs() < 1e-6);
            assert!((pair.left.forward[i] - FWD[i]).abs() < 1e-6);
            assert!((pair.right.forward[i] - FWD[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn convergence_toes_eyes_in() {
        let cfg = StereoConfig::default();
        let pair = eye_pair_from_mono(MONO_POS, FWD, UP, &cfg).unwrap();
        // Left-eye toes right (+X component) ; right-eye toes left (-X).
        assert!(pair.left.forward[0] > 0.0, "left-eye should have +X toe-in");
        assert!(pair.right.forward[0] < 0.0, "right-eye should have -X toe-in");
        // Magnitudes should match (symmetric toe).
        assert!((pair.left.forward[0] + pair.right.forward[0]).abs() < 1e-6);
        // Angle should equal atan(ipd/2 / convergence).
        let expected_theta = (cfg.ipd_meters * 0.5 / cfg.convergence_meters).atan();
        let actual_theta = pair.left.forward[0].atan2(-pair.left.forward[2]);
        assert!((actual_theta - expected_theta).abs() < 1e-5);
    }

    #[test]
    fn degenerate_up_vector_rejected() {
        let cfg = StereoConfig::default();
        // up parallel to forward (both -Z) → degenerate basis.
        let res = eye_pair_from_mono(MONO_POS, FWD, FWD, &cfg);
        assert_eq!(res, Err(StereoErr::ZeroSeparation));
        // zero-length up.
        let res = eye_pair_from_mono(MONO_POS, FWD, [0.0, 0.0, 0.0], &cfg);
        assert_eq!(res, Err(StereoErr::ZeroSeparation));
    }

    #[test]
    fn perpendicularity_preserved() {
        let cfg = StereoConfig::default();
        let pair = eye_pair_from_mono(MONO_POS, FWD, UP, &cfg).unwrap();
        // forward · up ≈ 0 for both eyes.
        assert!(dot(pair.left.forward, pair.left.up).abs() < 1e-5);
        assert!(dot(pair.right.forward, pair.right.up).abs() < 1e-5);
        // unit length.
        assert!((magnitude(pair.left.forward) - 1.0).abs() < 1e-5);
        assert!((magnitude(pair.right.forward) - 1.0).abs() < 1e-5);
        assert!((magnitude(pair.left.up) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn serde_round_trip() {
        let cfg = StereoConfig::default();
        let pair = eye_pair_from_mono(MONO_POS, FWD, UP, &cfg).unwrap();
        let json = serde_json::to_string(&pair).unwrap();
        let back: EyePair = serde_json::from_str(&json).unwrap();
        assert_eq!(pair, back);
    }
}
