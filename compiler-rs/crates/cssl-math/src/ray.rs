//! § Ray — parametric ray
//!
//! A ray defined by `origin + t * direction` for `t >= 0`. The
//! `direction` is conventionally unit-length but is NOT required to
//! be — the intersection routines correct for non-unit directions
//! where the parametric `t` returned needs to be in physical-distance
//! units.

use crate::vec3::Vec3;

/// Parametric ray.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Ray {
    /// Origin point of the ray.
    pub origin: Vec3,
    /// Direction vector. Conventionally unit-length.
    pub direction: Vec3,
}

impl Ray {
    /// Construct from origin + direction.
    #[must_use]
    pub const fn new(origin: Vec3, direction: Vec3) -> Self {
        Self { origin, direction }
    }

    /// Evaluate the ray at parameter `t`. Returns `origin + t * direction`.
    #[must_use]
    pub fn point_at(self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }

    /// Return a copy of this ray with a unit-length direction.
    /// Returns the original ray if `direction` is zero (totality).
    #[must_use]
    pub fn normalized(self) -> Self {
        let dir = self.direction.normalize();
        if dir == Vec3::ZERO {
            self
        } else {
            Self {
                origin: self.origin,
                direction: dir,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Ray;
    use crate::vec3::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec_approx(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps) && approx(a.z, b.z, eps)
    }

    #[test]
    fn ray_point_at_t() {
        let r = Ray::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(r.point_at(0.0), Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(r.point_at(2.0), Vec3::new(1.0, 2.0, 0.0));
    }

    #[test]
    fn ray_normalized_makes_unit_direction() {
        let r = Ray::new(Vec3::ZERO, Vec3::new(0.0, 5.0, 0.0));
        let n = r.normalized();
        assert!(approx(n.direction.length(), 1.0, 1e-6));
        assert!(vec_approx(n.direction, Vec3::Y, 1e-6));
    }

    #[test]
    fn ray_normalized_zero_direction_is_no_op() {
        let r = Ray::new(Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO);
        let n = r.normalized();
        // No NaN ; original direction preserved.
        assert_eq!(n.direction, Vec3::ZERO);
        assert_eq!(n.origin, r.origin);
    }
}
