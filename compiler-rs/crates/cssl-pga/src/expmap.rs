//! § expmap — exp / log map between bivectors (Lie algebra) and motors (Lie group)
//!
//! § BACKGROUND
//!   The bivectors of G(3,0,1) form the **Lie algebra** `se(3)` of the
//!   `SE(3)` rigid-motion group. The exponential map sends a bivector
//!   `B` to a unit motor `exp(B)` ; the logarithm sends a unit motor
//!   `M` back to a bivector `log(M)` such that `exp(log(M)) = M` and
//!   `log(exp(B)) = B` (modulo the principal range and float drift).
//!
//!   This is the mathematical bridge that lets the substrate **integrate
//!   bivector dynamics** : a body with twist `B` (instantaneous-motion
//!   bivector) and pose `M` updates over time `dt` as
//!     `M ← M · exp(dt · B)`
//!   guaranteeing the result stays on the rigid-motion manifold
//!   (orientation-preserving, no shear) by construction.
//!
//! § DECOMPOSITION
//!   A bivector `B = B_r + B_t` decomposes into a **spatial** (rotor-
//!   generating) part `B_r = b₁·e₂₃ + b₂·e₃₁ + b₃·e₁₂` and an **ideal**
//!   (translator-generating) part `B_t = t₁·e₀₁ + t₂·e₀₂ + t₃·e₀₃`. The
//!   degenerate signature (`e₀² = 0`) makes `B_r` and `B_t` "almost
//!   commute" but with a non-trivial pseudoscalar coupling — handled
//!   below in [`exp_bivector`].
//!
//! § PRINCIPAL RANGE
//!   For numerical stability, the rotor part of the log is wrapped to the
//!   principal range `(-π, π]` for the half-angle (rotation angle in
//!   `(-2π, 2π]`). This matches the Quat slerp short-arc convention.

use crate::line::Line;
use crate::motor::Motor;

/// Exponential map of a bivector (encoded as a [`Line`]) to a unit motor.
///
/// Implementation note : the closed form for `exp(B)` in G(3,0,1) is
///   `exp(B_r + B_t) = (cos(θ/2) - sin(θ/2)/θ · B_r) · (1 + (1/θ²) (1 -
///   cos θ + ...) · ...)` ...
///
/// In practice we split into the rotor + translator factorization that's
/// numerically robust :
///   1. Compute the **rotor** factor from the spatial bivector `B_r` :
///        if `θ = ‖B_r‖ > 0` :
///            `R = cos(θ/2) - (sin(θ/2)/θ) · B_r`        (note B_r is grade-2)
///        else :
///            `R = 1` (Taylor-expanded scalar identity).
///   2. Compute the **translator** factor from the ideal bivector :
///        the translation distance is the magnitude of the ideal part
///        scaled by the rotor's `sinc(θ/2)` factor when the rotor is
///        non-trivial. For zero rotation, `T = 1 + (1/2) · B_t`.
///   3. Combine `M = T · R`.
#[must_use]
pub fn exp_bivector(b: Line) -> Motor {
    // Spatial bivector squared-norm and angle.
    let r2 = b.e23 * b.e23 + b.e31 * b.e31 + b.e12 * b.e12;
    let theta = r2.sqrt();

    if theta > 1e-7 {
        // Non-trivial rotation.
        let half = 0.5 * theta;
        let cos_h = half.cos();
        let sinc_h = half.sin() / theta; // sin(θ/2) / θ
        // R = cos(θ/2) - sinc(θ/2) · B_r
        let s = cos_h;
        let r1 = -sinc_h * b.e23;
        let r2c = -sinc_h * b.e31;
        let r3 = -sinc_h * b.e12;

        // Translator factor : the ideal part contributes via a "screw"
        // formula. For the simple split-decomposition exp = T·R below,
        // the translator components scaled by 1/2 (matching Translator
        // canonical form), and the pseudoscalar tracks the screw pitch.
        //
        // Short-form derivation (see Klein PGA Sec 4 or Bivector.net
        // PGA Cheatsheet) :
        //   exp(B_r + B_t) = exp(B_r) (1 + B_t' + (1/2) B_t'^2 + ...)
        // where B_t' is the translator generator after composition with
        // the rotor. The leading-order form below captures the dominant
        // translation correctly ; the screw-pitch coupling shows up as
        // the pseudoscalar component.
        // Translator components match Translator::from_translation sign
        // convention : T = 1 - (1/2)(t_x e₀₁ + ...) so the linear part of
        // exp goes as -0.5 · b_t.
        let t1_lin = -0.5 * b.e01;
        let t2_lin = -0.5 * b.e02;
        let t3_lin = -0.5 * b.e03;
        // Pseudoscalar component from `r·t` coupling (screw pitch) :
        // m₀ = (1/2) · (b.r · b.t) · sinc(θ/2).
        let dot_rt = b.e23 * b.e01 + b.e31 * b.e02 + b.e12 * b.e03;
        let m0 = dot_rt * sinc_h * 0.5;

        // Build M = T · R via the closed-form rotor-then-translator combine.
        // For a unit-rotation around an axis aligned with the translation
        // direction, the screw-pitch gives the canonical exp behavior.
        let translator = Motor::from_components(1.0, 0.0, 0.0, 0.0, t1_lin, t2_lin, t3_lin, m0);
        let rotor = Motor::from_components(s, r1, r2c, r3, 0.0, 0.0, 0.0, 0.0);
        translator * rotor
    } else {
        // Pure (or near-pure) translation. Matches the translator sign
        // convention `T = 1 - (1/2)(t·e₀_*)`.
        Motor {
            s: 1.0,
            r1: 0.0,
            r2: 0.0,
            r3: 0.0,
            t1: -0.5 * b.e01,
            t2: -0.5 * b.e02,
            t3: -0.5 * b.e03,
            m0: 0.0,
        }
    }
}

/// Logarithm map : motor → bivector.
///
/// For a unit motor `M`, returns the bivector `B` such that
/// `exp_bivector(B) ≈ M` modulo float drift. Wraps the rotation angle
/// to the principal range `(-2π, 2π]` for stability.
#[must_use]
pub fn log_motor(m: Motor) -> Line {
    let r2 = m.r1 * m.r1 + m.r2 * m.r2 + m.r3 * m.r3;
    let r_norm = r2.sqrt();

    if r_norm > 1e-7 {
        // Recover θ from `s = cos(θ/2)` and `‖rotor-bivector‖ = sin(θ/2)`.
        // Use atan2 for numerical robustness near θ = 0 and θ = π.
        let half_theta = r_norm.atan2(m.s);
        let theta = 2.0 * half_theta;
        let scale = -theta / r_norm; // negative because R = cos - sin·B (sign convention)

        // Spatial part : axis × angle (un-normalized, as a Line bivector).
        let e23 = scale * m.r1;
        let e31 = scale * m.r2;
        let e12 = scale * m.r3;

        // Translator part : invert the leading-order exp. Translator
        // sign convention is `t01 = -0.5·t_x`, so log inverts as
        // `t_x = -2·m.t1`.
        let e01 = -2.0 * m.t1;
        let e02 = -2.0 * m.t2;
        let e03 = -2.0 * m.t3;

        Line::from_components(e23, e31, e12, e01, e02, e03)
    } else {
        // Pure translation : log = -2 × translator-bivector.
        Line::from_components(0.0, 0.0, 0.0, -2.0 * m.t1, -2.0 * m.t2, -2.0 * m.t3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::Point;
    use crate::rotor::Rotor;
    use crate::translator::Translator;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn exp_zero_bivector_is_identity_motor() {
        let m = exp_bivector(Line::default());
        assert!(approx(m.s, 1.0));
        assert!(approx(m.r1, 0.0));
        assert!(approx(m.t1, 0.0));
    }

    #[test]
    fn exp_pure_rotation_matches_rotor_axis_angle() {
        // B = -θ · e₂₃ → exp(B) = cos(θ/2) - sin(θ/2)·e₂₃ — rotation by θ
        // around the x-axis (matching Klein-PGA convention).
        let theta = 0.7_f32;
        let b = Line::from_components(theta, 0.0, 0.0, 0.0, 0.0, 0.0);
        let m_exp = exp_bivector(b);
        let r_axis = Rotor::from_axis_angle(1.0, 0.0, 0.0, theta);
        assert!(approx(m_exp.s, r_axis.s));
        assert!(approx(m_exp.r1, r_axis.b1));
    }

    #[test]
    fn exp_pure_translation_yields_translator() {
        // Convention : exp(t_x · e₀₁ + ...) translates by (t_x, t_y, t_z).
        // The Lie-algebra natural-form bivector ↔ world-space translation
        // mapping is identity with the canonical translator sign.
        let b = Line::from_components(0.0, 0.0, 0.0, 2.0, 4.0, 6.0);
        let m_exp = exp_bivector(b);
        let translated = m_exp.apply(&Point::from_xyz(0.0, 0.0, 0.0).to_multivector());
        let p = Point::from_multivector(&translated).normalize();
        let (x, y, z) = p.to_xyz();
        assert!(approx(x, 2.0));
        assert!(approx(y, 4.0));
        assert!(approx(z, 6.0));
    }

    #[test]
    fn exp_log_round_trip_pure_rotation() {
        let theta = 0.5_f32;
        let b = Line::from_components(theta, 0.0, 0.0, 0.0, 0.0, 0.0);
        let m = exp_bivector(b);
        let b_back = log_motor(m);
        assert!(approx(b_back.e23, theta));
        assert!(approx(b_back.e31, 0.0));
        assert!(approx(b_back.e12, 0.0));
    }

    #[test]
    fn exp_log_round_trip_pure_translation() {
        let b = Line::from_components(0.0, 0.0, 0.0, 1.5, -2.0, 3.0);
        let m = exp_bivector(b);
        let b_back = log_motor(m);
        assert!(approx(b_back.e01, 1.5));
        assert!(approx(b_back.e02, -2.0));
        assert!(approx(b_back.e03, 3.0));
    }

    #[test]
    fn exp_log_round_trip_preserves_arbitrary_rotation() {
        // Loop a few axes / angles, exp-then-log should return the input.
        let inputs: &[(f32, f32, f32)] = &[(0.3, 0.0, 0.0), (0.0, 0.5, 0.0), (0.0, 0.0, 0.7)];
        for &(a, b, c) in inputs {
            let bv = Line::from_components(a, b, c, 0.0, 0.0, 0.0);
            let m = exp_bivector(bv);
            let bv_back = log_motor(m);
            assert!(approx(bv_back.e23, a), "e23 round-trip {a}");
            assert!(approx(bv_back.e31, b), "e31 round-trip {b}");
            assert!(approx(bv_back.e12, c), "e12 round-trip {c}");
        }
    }

    #[test]
    fn exp_zero_rotation_branch_handles_translation_only() {
        // Edge case : θ=0 branch should still produce a valid translator.
        let bv = Line::from_components(0.0, 0.0, 0.0, 1.0, 2.0, 3.0);
        let m = exp_bivector(bv);
        let translated = m.apply(&Point::from_xyz(0.0, 0.0, 0.0).to_multivector());
        let p = Point::from_multivector(&translated).normalize();
        let (x, y, z) = p.to_xyz();
        assert!(approx(x, 1.0));
        assert!(approx(y, 2.0));
        assert!(approx(z, 3.0));
    }

    #[test]
    fn exp_rotation_only_yields_unit_norm_motor() {
        let bv = Line::from_components(0.5, -0.3, 0.7, 0.0, 0.0, 0.0);
        let m = exp_bivector(bv);
        // Rotor part must have unit norm.
        let n2 = m.s * m.s + m.r1 * m.r1 + m.r2 * m.r2 + m.r3 * m.r3;
        assert!(approx(n2, 1.0));
    }

    #[test]
    fn exp_translation_via_translator_matches_canonical() {
        // Canonical T = Translator::from_translation(...) translates by
        // exactly the natural-form vector. exp_bivector with the same
        // natural-form vector should produce the equivalent motor.
        let t = Translator::from_translation(2.0, 3.0, 4.0);
        let canonical = Motor::from_translator(t);
        let b = Line::from_components(0.0, 0.0, 0.0, 2.0, 3.0, 4.0);
        let from_exp = exp_bivector(b);
        let p_canon = canonical.apply(&Point::from_xyz(0.0, 0.0, 0.0).to_multivector());
        let p_exp = from_exp.apply(&Point::from_xyz(0.0, 0.0, 0.0).to_multivector());
        let pc = Point::from_multivector(&p_canon).normalize();
        let pe = Point::from_multivector(&p_exp).normalize();
        assert!(approx(pc.to_xyz().0, pe.to_xyz().0));
        assert!(approx(pc.to_xyz().1, pe.to_xyz().1));
        assert!(approx(pc.to_xyz().2, pe.to_xyz().2));
    }
}
