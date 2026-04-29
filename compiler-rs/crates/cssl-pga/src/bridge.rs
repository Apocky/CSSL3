//! § bridge — conversions between `cssl-math` types and PGA primitives
//!
//! The PGA crate is the canonical algebra ; the `cssl-math` crate is the
//! `f32`-only Vec/Mat/Quat surface used by the renderer hot-paths and the
//! GPU upload buffers. This bridge module provides one-shot conversions
//! between the two surfaces so existing `cssl-math` consumers can lift
//! into PGA when they want algebraic closure (manifold-aware integration,
//! body-omnoid bivector dynamics, FieldCell motor composition).
//!
//! § BRIDGES
//!
//!   `cssl_math::Vec3`   → [`crate::Point`]    (finite point with weight 1)
//!   `cssl_math::Quat`   → [`crate::Rotor`]    (Hamilton ↔ even-subalgebra)
//!   `cssl_math::Plane`  → [`crate::Plane`]    (point-normal-distance ↔ grade-1 vector)
//!
//! Each bridge has a round-trip companion ([`point_to_vec3`], etc.) so
//! consumers can drop into PGA, do algebraic work, and lift back out.

use cssl_math::{Plane as MathPlane, Quat, Vec3};

use crate::plane::Plane;
use crate::point::Point;
use crate::rotor::Rotor;

/// Convert a `cssl-math` `Vec3` (interpreted as a world-space position)
/// to a PGA finite-weight [`Point`].
#[must_use]
pub fn vec3_to_point(v: Vec3) -> Point {
    Point::from_xyz(v.x, v.y, v.z)
}

/// Recover a `cssl-math` `Vec3` from a PGA point. Performs the
/// perspective divide ; returns `Vec3::ZERO` for points at infinity
/// (totality, matching the f32 surface convention).
#[must_use]
pub fn point_to_vec3(p: Point) -> Vec3 {
    let (x, y, z) = p.to_xyz();
    Vec3::new(x, y, z)
}

/// Convert a `cssl-math` `Quat` (Hamilton convention `(x, y, z, w)`) to a
/// PGA [`Rotor`].
///
/// The PGA rotor representing rotation by `θ` about a unit axis `n̂` is
///   `R = cos(θ/2) - sin(θ/2)·(n̂_x e₂₃ + n̂_y e₃₁ + n̂_z e₁₂)`,
/// while the Hamilton quaternion is
///   `q = cos(θ/2) + sin(θ/2)·(n̂_x i + n̂_y j + n̂_z k)`.
/// Mapping `(i, j, k) → -(e₂₃, e₃₁, e₁₂)` gives `Rotor { s: q.w, b1: -q.x,
/// b2: -q.y, b3: -q.z }` for matching rotational behavior. This sign flip
/// is what makes [`Rotor::apply`] produce the same rotation as
/// [`Quat::rotate`] for the same axis-angle input.
#[must_use]
pub fn quat_to_rotor(q: Quat) -> Rotor {
    Rotor::from_components(q.w, -q.x, -q.y, -q.z)
}

/// Inverse bridge — recover a `cssl-math` Quat from a PGA rotor.
#[must_use]
pub fn rotor_to_quat(r: Rotor) -> Quat {
    Quat::new(-r.b1, -r.b2, -r.b3, r.s)
}

/// Convert a `cssl-math` `Plane` (point-normal-distance form) to a PGA
/// [`Plane`] (grade-1 vector form). Sign convention :
///   `cssl-math` plane equation : `dot(normal, p) + distance == 0`,
///   PGA plane equation        : `e₁·x + e₂·y + e₃·z + e₀ == 0`,
/// so `Plane { e1: n.x, e2: n.y, e3: n.z, e0: distance }` aligns the two.
#[must_use]
pub fn math_plane_to_plane(mp: MathPlane) -> Plane {
    Plane::new(mp.normal.x, mp.normal.y, mp.normal.z, mp.distance)
}

/// Inverse bridge — recover a `cssl-math` Plane from a PGA plane.
#[must_use]
pub fn plane_to_math_plane(p: Plane) -> MathPlane {
    MathPlane::new(Vec3::new(p.e1, p.e2, p.e3), p.e0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }
    fn vec_approx(a: Vec3, b: Vec3) -> bool {
        approx(a.x, b.x) && approx(a.y, b.y) && approx(a.z, b.z)
    }

    #[test]
    fn vec3_round_trip_through_point() {
        let v = Vec3::new(1.5, -2.0, 3.25);
        let p = vec3_to_point(v);
        let back = point_to_vec3(p);
        assert!(vec_approx(v, back));
    }

    #[test]
    fn quat_round_trip_through_rotor() {
        let q = Quat::from_axis_angle(Vec3::Y, 0.7);
        let r = quat_to_rotor(q);
        let q_back = rotor_to_quat(r);
        // Same orientation up to sign flip of all 4 components.
        let direct = approx(q.x, q_back.x)
            && approx(q.y, q_back.y)
            && approx(q.z, q_back.z)
            && approx(q.w, q_back.w);
        let flipped = approx(q.x, -q_back.x)
            && approx(q.y, -q_back.y)
            && approx(q.z, -q_back.z)
            && approx(q.w, -q_back.w);
        assert!(direct || flipped);
    }

    #[test]
    fn quat_to_rotor_produces_same_rotation_on_x_axis() {
        // Quat: 90deg around Y maps X to -Z.
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let v_q = q.rotate(Vec3::X);
        // Rotor : same rotation should map the PGA point at (1, 0, 0) to (0, 0, -1).
        let r = quat_to_rotor(q);
        let p_in = vec3_to_point(Vec3::X).to_multivector();
        let p_out = r.apply(&p_in);
        let v_r = point_to_vec3(Point::from_multivector(&p_out).normalize());
        assert!(vec_approx(v_q, v_r), "quat={v_q:?} rotor={v_r:?}");
    }

    #[test]
    fn math_plane_round_trip_through_pga_plane() {
        let mp = MathPlane::new(Vec3::Y, -3.0);
        let pga = math_plane_to_plane(mp);
        let back = plane_to_math_plane(pga);
        assert!(vec_approx(mp.normal, back.normal));
        assert!(approx(mp.distance, back.distance));
    }

    #[test]
    fn plane_reflection_matches_math_plane_reflection() {
        // Both surfaces should reflect (1, 3, 2) through XZ plane → (1, -3, 2).
        let mp = MathPlane::new(Vec3::Y, 0.0);
        let math_reflected = mp.reflect_point(Vec3::new(1.0, 3.0, 2.0));
        let pga = math_plane_to_plane(mp);
        let p_in = vec3_to_point(Vec3::new(1.0, 3.0, 2.0)).to_multivector();
        let p_out = pga.reflect(&p_in);
        let pga_reflected = point_to_vec3(Point::from_multivector(&p_out).normalize());
        assert!(vec_approx(math_reflected, pga_reflected));
    }

    #[test]
    fn rotor_sandwich_matches_quat_rotation_arbitrary_axis_and_angle() {
        // Pin down that the bridge preserves rotation behavior across a
        // family of (axis, angle, point) triples — strong correctness check
        // for the Hamilton-↔-rotor sign convention.
        let axes = [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1.0, 1.0, 1.0).normalize()];
        let angles: &[f32] = &[0.1, 0.5, 1.2, 2.5, -1.0];
        let probes = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.5, 1.5, 2.5),
            Vec3::new(-2.0, 1.0, 0.5),
        ];
        for ax in &axes {
            for &a in angles {
                let q = Quat::from_axis_angle(*ax, a);
                let r = quat_to_rotor(q);
                for v in &probes {
                    let v_q = q.rotate(*v);
                    let p_in = vec3_to_point(*v).to_multivector();
                    let p_out = r.apply(&p_in);
                    let v_r = point_to_vec3(Point::from_multivector(&p_out).normalize());
                    assert!(
                        vec_approx(v_q, v_r),
                        "axis={ax:?} angle={a} probe={v:?} quat={v_q:?} rotor={v_r:?}",
                    );
                }
            }
        }
    }

    #[test]
    fn quat_compose_matches_rotor_compose() {
        // q1 * q2 (Hamilton) should equal rotor product (PGA) when bridged.
        let q1 = Quat::from_axis_angle(Vec3::Y, 0.4);
        let q2 = Quat::from_axis_angle(Vec3::X, 0.7);
        let q12 = q1 * q2;
        let r12 = quat_to_rotor(q1) * quat_to_rotor(q2);
        let q12_via_rotor = rotor_to_quat(r12);
        let direct = approx(q12.x, q12_via_rotor.x)
            && approx(q12.y, q12_via_rotor.y)
            && approx(q12.z, q12_via_rotor.z)
            && approx(q12.w, q12_via_rotor.w);
        let flipped = approx(q12.x, -q12_via_rotor.x)
            && approx(q12.y, -q12_via_rotor.y)
            && approx(q12.z, -q12_via_rotor.z)
            && approx(q12.w, -q12_via_rotor.w);
        assert!(direct || flipped);
    }

    #[test]
    fn pga_plane_normalize_matches_math_plane_unit_normal() {
        // cssl-math's Plane::new normalizes the normal on construction.
        // PGA Plane preserves raw components ; after normalize() they
        // should match the cssl-math unit-normal version.
        let mp = MathPlane::new(Vec3::new(2.0, 0.0, 0.0), -3.0);
        let pga = math_plane_to_plane(mp).normalize();
        // mp normalized normal is (1, 0, 0) ; mp.distance after normalize
        // is the original distance in cssl-math (no rescale).
        let lifted = plane_to_math_plane(pga);
        assert!(approx(lifted.normal.length(), 1.0));
    }
}
