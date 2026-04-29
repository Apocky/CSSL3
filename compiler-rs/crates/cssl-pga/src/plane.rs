//! § Plane — grade-1 element of G(3,0,1) (Klein-PGA)
//!
//! In Klein-style PGA, **planes are grade-1 vectors** — 4 components on the
//! basis `e₀ e₁ e₂ e₃`. The plane equation `ax + by + cz + d = 0` maps to
//! the multivector `a·e₁ + b·e₂ + c·e₃ + d·e₀`.
//!
//! § REFLECTION
//!   The single most important property of plane-based PGA : reflection of
//!   a geometric primitive `g` in a unit plane `π` is the sandwich
//!   `π g π̃`. Composing two reflections gives a rotation, three give a
//!   reflection-rotation, and so on — the algebraically-closed motor
//!   structure flows out of this fact.

use crate::basis::BLADE_COUNT;
use crate::multivector::Multivector;

/// A plane in 3D space, stored as a grade-1 multivector with components
/// `(e₁, e₂, e₃, e₀) = (a, b, c, d)` such that `a·x + b·y + c·z + d = 0`
/// is the plane equation.
///
/// `(a, b, c)` is the (un-normalized) normal direction ; `d` is the signed
/// offset. A unit plane has `a² + b² + c² = 1`, in which case `d` is the
/// signed distance from the origin (negated relative to the `cssl-math`
/// `Plane::distance` convention — see [`crate::bridge`] for the
/// interconversion).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Plane {
    /// `e₁` coefficient — x-component of the plane normal.
    pub e1: f32,
    /// `e₂` coefficient — y-component of the plane normal.
    pub e2: f32,
    /// `e₃` coefficient — z-component of the plane normal.
    pub e3: f32,
    /// `e₀` coefficient — signed offset.
    pub e0: f32,
}

impl Plane {
    /// Construct from explicit `(a, b, c, d)` — the plane equation
    /// `a·x + b·y + c·z + d = 0`.
    #[must_use]
    pub const fn new(a: f32, b: f32, c: f32, d: f32) -> Self {
        Self {
            e1: a,
            e2: b,
            e3: c,
            e0: d,
        }
    }

    /// Construct from a normal direction `(nx, ny, nz)` and a signed
    /// distance `d` from the origin along that normal. Equivalent to
    /// `Plane::new(nx, ny, nz, -d)` because the plane equation is
    /// `n · p - d = 0`.
    #[must_use]
    pub const fn from_normal_and_distance(nx: f32, ny: f32, nz: f32, d: f32) -> Self {
        Self::new(nx, ny, nz, -d)
    }

    /// Embed this plane as a full 16-component multivector.
    #[must_use]
    pub fn to_multivector(self) -> Multivector {
        let mut a = [0.0_f32; BLADE_COUNT];
        a[1] = self.e1;
        a[2] = self.e2;
        a[3] = self.e3;
        a[4] = self.e0;
        Multivector::from_array(a)
    }

    /// Try to extract a plane from a general multivector — the grade-1
    /// projection. Components on other grades are silently dropped.
    #[must_use]
    pub fn from_multivector(mv: &Multivector) -> Self {
        Self {
            e1: mv.e1(),
            e2: mv.e2(),
            e3: mv.e3(),
            e0: mv.e0(),
        }
    }

    /// Squared norm of the plane's normal — `a² + b² + c²`. The `e₀`
    /// coefficient does NOT contribute (the null-direction signature).
    #[must_use]
    pub fn normal_norm_squared(self) -> f32 {
        self.e1 * self.e1 + self.e2 * self.e2 + self.e3 * self.e3
    }

    /// Renormalize so the plane normal is unit length. Returns the original
    /// plane if the normal is degenerate (zero).
    #[must_use]
    pub fn normalize(self) -> Self {
        let n2 = self.normal_norm_squared();
        if n2 > 1e-12 {
            let inv = n2.sqrt().recip();
            Self::new(self.e1 * inv, self.e2 * inv, self.e3 * inv, self.e0 * inv)
        } else {
            self
        }
    }

    /// Reflect a geometric primitive `g` (any multivector) through this
    /// plane via the sandwich `π g π̃`. The plane is treated as a unit
    /// reflector if it is unit-normalized ; otherwise the result is scaled
    /// by `|π|²`.
    #[must_use]
    pub fn reflect(self, g: &Multivector) -> Multivector {
        let p = self.to_multivector();
        p.sandwich(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::Point;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn plane_constructor_stores_components() {
        let p = Plane::new(1.0, 2.0, 3.0, 4.0);
        assert!(approx(p.e1, 1.0));
        assert!(approx(p.e2, 2.0));
        assert!(approx(p.e3, 3.0));
        assert!(approx(p.e0, 4.0));
    }

    #[test]
    fn plane_from_normal_and_distance_negates_offset() {
        // Plane through point (0, d, 0) with normal Y has equation y - d = 0,
        // i.e. coefficients (0, 1, 0, -d) — `e₀` coefficient = -d.
        let p = Plane::from_normal_and_distance(0.0, 1.0, 0.0, 5.0);
        assert!(approx(p.e0, -5.0));
    }

    #[test]
    fn plane_normalize_yields_unit_normal() {
        let p = Plane::new(2.0, 0.0, 0.0, 3.0).normalize();
        assert!(approx(p.normal_norm_squared(), 1.0));
    }

    #[test]
    fn plane_normalize_handles_degenerate_normal() {
        let p = Plane::new(0.0, 0.0, 0.0, 5.0);
        let n = p.normalize();
        // Pass-through : degenerate input doesn't produce NaN.
        assert!(approx(n.e0, 5.0));
    }

    #[test]
    fn plane_reflects_point_y_eq_0_through_x_axis() {
        // XZ plane (y = 0) : reflect a point (1, 3, 2) → (1, -3, 2).
        let plane = Plane::new(0.0, 1.0, 0.0, 0.0);
        let pt = Point::from_xyz(1.0, 3.0, 2.0).to_multivector();
        let reflected = plane.reflect(&pt);
        let r = Point::from_multivector(&reflected).to_xyz();
        assert!(approx(r.0, 1.0));
        assert!(approx(r.1, -3.0));
        assert!(approx(r.2, 2.0));
    }

    #[test]
    fn plane_round_trips_through_multivector() {
        let p = Plane::new(0.5, -0.3, 0.7, 1.2);
        let mv = p.to_multivector();
        let p2 = Plane::from_multivector(&mv);
        assert_eq!(p, p2);
    }

    #[test]
    fn double_reflection_in_same_plane_is_identity() {
        // Reflecting twice through the same plane returns the original
        // point — fundamental symmetry of the sandwich product.
        let plane = Plane::new(0.5, 1.0, -0.2, 0.3).normalize();
        let original = Point::from_xyz(1.0, 2.0, 3.0);
        let p_mv = original.to_multivector();
        let once = plane.reflect(&p_mv);
        let twice = plane.reflect(&once);
        let p_back = Point::from_multivector(&twice).normalize();
        let (x, y, z) = p_back.to_xyz();
        assert!(approx(x, 1.0));
        assert!(approx(y, 2.0));
        assert!(approx(z, 3.0));
    }

    #[test]
    fn perpendicular_planes_compose_to_180deg_rotation() {
        // Two perpendicular planes through the origin compose under
        // sandwich-of-sandwiches into a 180° rotation about their line of
        // intersection. With the XZ and YZ planes that's a rotation about
        // the Z axis.
        let p_xz = Plane::new(0.0, 1.0, 0.0, 0.0); // y = 0
        let p_yz = Plane::new(1.0, 0.0, 0.0, 0.0); // x = 0
                                                   // Reflecting (1, 0, 0) through y=0 gives (1, 0, 0). Then through
                                                   // x=0 gives (-1, 0, 0). That's a 180-deg rotation about Z.
        let p_in = Point::from_xyz(1.0, 0.0, 0.0).to_multivector();
        let after_first = p_xz.reflect(&p_in);
        let after_second = p_yz.reflect(&after_first);
        let p_out = Point::from_multivector(&after_second).normalize();
        let (x, y, z) = p_out.to_xyz();
        assert!(approx(x, -1.0));
        assert!(approx(y, 0.0));
        assert!(approx(z, 0.0));
    }
}
