//! § ops — projection / rejection / reflection helpers
//!
//! Higher-level convenience operators built on the multivector primitives.
//! "Project this point onto that plane", "reflect this line across that
//! plane", "reject this vector from that bivector" — each implemented in
//! terms of the algebraically-fundamental products on [`Multivector`].

use crate::multivector::Multivector;
use crate::plane::Plane;
use crate::point::Point;

/// Reflect a point across a plane via the sandwich `π p π̃`.
#[must_use]
pub fn reflect_point_across_plane(p: Point, plane: Plane) -> Point {
    let pl = plane.normalize();
    let pl_mv = pl.to_multivector();
    let reflected = pl_mv.sandwich(&p.to_multivector());
    Point::from_multivector(&reflected).normalize()
}

/// Project an arbitrary multivector `a` onto another `b` via `(a | b) · b⁻¹`.
/// Returns zero for degenerate `b` (totality).
#[must_use]
pub fn project(a: &Multivector, b: &Multivector) -> Multivector {
    let bn2 = b.norm_squared();
    if bn2 > 1e-12 {
        let inv = bn2.recip();
        a.inner(b).geometric(b).scale(inv)
    } else {
        Multivector::default()
    }
}

/// Reject `a` from `b` — component of `a` orthogonal to `b`.
#[must_use]
pub fn reject(a: &Multivector, b: &Multivector) -> Multivector {
    *a - project(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basis::{BLADE_COUNT, E1, E2};

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn reflect_point_across_xz_plane_negates_y() {
        let plane = Plane::new(0.0, 1.0, 0.0, 0.0);
        let p = Point::from_xyz(1.0, 3.0, 2.0);
        let r = reflect_point_across_plane(p, plane);
        let (x, y, z) = r.to_xyz();
        assert!(approx(x, 1.0));
        assert!(approx(y, -3.0));
        assert!(approx(z, 2.0));
    }

    #[test]
    fn reject_subtracts_projection() {
        let a = E1 * 2.0 + E2 * 3.0;
        let b = E1;
        let proj = project(&a, &b);
        let rej = reject(&a, &b);
        let sum = proj + rej;
        for i in 0..BLADE_COUNT {
            assert!(approx(sum.coefficient(i), a.coefficient(i)));
        }
    }

    #[test]
    fn reject_handles_degenerate_basis() {
        let a = E1 * 2.0;
        let zero = Multivector::default();
        let r = reject(&a, &zero);
        for i in 0..BLADE_COUNT {
            assert!(approx(r.coefficient(i), a.coefficient(i)));
        }
    }
}
