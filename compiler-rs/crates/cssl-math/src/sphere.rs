//! § Sphere — bounding sphere
//!
//! Bounding sphere defined by a center + radius. Used as a faster
//! alternative to AABB for broadphase culling when shapes are roughly
//! isotropic.

use crate::aabb::Aabb;
use crate::ray::Ray;
use crate::scalar::EPSILON_F32;
use crate::vec3::Vec3;

/// Bounding sphere.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Sphere {
    /// Sphere center.
    pub center: Vec3,
    /// Sphere radius. Always non-negative ; constructors clamp to `0`
    /// for negative input.
    pub radius: f32,
}

impl Default for Sphere {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Sphere {
    /// Empty sphere — zero-radius at the origin. The merge identity.
    pub const EMPTY: Self = Self {
        center: Vec3::ZERO,
        radius: 0.0,
    };

    /// Unit sphere — radius 1 at the origin.
    pub const UNIT: Self = Self {
        center: Vec3::ZERO,
        radius: 1.0,
    };

    /// Construct from explicit center + radius. Negative radii are
    /// clamped to zero.
    #[must_use]
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self {
            center,
            radius: radius.max(0.0),
        }
    }

    /// Construct the smallest sphere bounding an AABB. Center is the
    /// AABB center ; radius is the half-diagonal.
    #[must_use]
    pub fn from_aabb(aabb: Aabb) -> Self {
        if aabb.is_empty() {
            return Self::EMPTY;
        }
        Self {
            center: aabb.center(),
            radius: aabb.half_extents().length(),
        }
    }

    /// Construct from a slice of points. Naive algorithm : center is
    /// the centroid ; radius is the max distance from the centroid to
    /// any input point. NOT the minimum-bounding-sphere — for that use
    /// Welzl's algorithm in a future slice. Adequate for broadphase
    /// culling.
    #[must_use]
    pub fn from_points(points: &[Vec3]) -> Self {
        if points.is_empty() {
            return Self::EMPTY;
        }
        let mut centroid = Vec3::ZERO;
        for &p in points {
            centroid += p;
        }
        centroid *= (points.len() as f32).recip();
        let mut max_d2 = 0.0_f32;
        for &p in points {
            let d2 = (p - centroid).length_squared();
            if d2 > max_d2 {
                max_d2 = d2;
            }
        }
        Self {
            center: centroid,
            radius: max_d2.sqrt(),
        }
    }

    /// Volume of the sphere — `4/3 * π * r^3`.
    #[must_use]
    pub fn volume(self) -> f32 {
        (4.0 / 3.0) * core::f32::consts::PI * self.radius.powi(3)
    }

    /// Surface area — `4 * π * r^2`.
    #[must_use]
    pub fn surface_area(self) -> f32 {
        4.0 * core::f32::consts::PI * self.radius * self.radius
    }

    /// Bounding-AABB of this sphere.
    #[must_use]
    pub fn to_aabb(self) -> Aabb {
        let r = Vec3::splat(self.radius);
        Aabb::new(self.center - r, self.center + r)
    }

    /// True if the sphere contains `point` (inclusive boundary).
    #[must_use]
    pub fn contains_point(self, point: Vec3) -> bool {
        (point - self.center).length_squared() <= self.radius * self.radius
    }

    /// True if two spheres overlap (or touch).
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        let d = (self.center - other.center).length();
        d <= self.radius + other.radius
    }

    /// Merge two spheres into the smallest sphere containing both.
    /// `Sphere::EMPTY.merge(other)` returns `other`.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        if self.radius <= EPSILON_F32 {
            return other;
        }
        if other.radius <= EPSILON_F32 {
            return self;
        }
        let dir = other.center - self.center;
        let dist = dir.length();
        // One inside the other ?
        if dist + other.radius <= self.radius {
            return self;
        }
        if dist + self.radius <= other.radius {
            return other;
        }
        // Otherwise span both.
        let new_radius = (dist + self.radius + other.radius) * 0.5;
        let new_center = self.center + dir * ((new_radius - self.radius) / dist);
        Self {
            center: new_center,
            radius: new_radius,
        }
    }

    /// Ray vs sphere intersection. Returns `Some(t)` for the smallest
    /// `t >= 0` where the ray hits the sphere surface, or `None` for
    /// a miss / behind-origin.
    ///
    /// The `direction` vector is NOT assumed to be unit-length ; the
    /// returned `t` is in `direction`-units. Pass [`Ray::normalized`]
    /// upstream if you want `t` in world-space distance units.
    #[must_use]
    pub fn ray_intersect(self, ray: Ray) -> Option<f32> {
        // Solve |o + t*d - c|² = r² for t.
        // ⇒ t² (d·d) + 2t (d·(o-c)) + (o-c)·(o-c) - r² = 0
        let oc = ray.origin - self.center;
        let a = ray.direction.dot(ray.direction);
        let b = 2.0 * ray.direction.dot(oc);
        let c = oc.dot(oc) - self.radius * self.radius;
        if a.abs() < EPSILON_F32 {
            // Zero-length direction. If origin is inside the sphere, t=0 hit.
            return if c <= 0.0 { Some(0.0) } else { None };
        }
        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            return None;
        }
        let sqrt_disc = disc.sqrt();
        let inv_2a = (2.0 * a).recip();
        let t1 = (-b - sqrt_disc) * inv_2a;
        let t2 = (-b + sqrt_disc) * inv_2a;
        if t1 >= 0.0 {
            Some(t1)
        } else if t2 >= 0.0 {
            // Origin inside sphere.
            Some(0.0)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Sphere;
    use crate::aabb::Aabb;
    use crate::ray::Ray;
    use crate::vec3::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn sphere_negative_radius_clamped_to_zero() {
        let s = Sphere::new(Vec3::ZERO, -1.0);
        assert_eq!(s.radius, 0.0);
    }

    #[test]
    fn sphere_volume_and_surface_area() {
        // Unit sphere : volume 4π/3 ≈ 4.18879 ; surface 4π ≈ 12.566.
        let s = Sphere::UNIT;
        assert!(approx(
            s.volume(),
            (4.0 / 3.0) * core::f32::consts::PI,
            1e-5
        ));
        assert!(approx(s.surface_area(), 4.0 * core::f32::consts::PI, 1e-5));
    }

    #[test]
    fn sphere_contains_point() {
        let s = Sphere::new(Vec3::ZERO, 2.0);
        assert!(s.contains_point(Vec3::ZERO));
        assert!(s.contains_point(Vec3::new(1.0, 1.0, 1.0))); // sqrt(3) < 2.
        assert!(!s.contains_point(Vec3::new(3.0, 0.0, 0.0)));
        assert!(s.contains_point(Vec3::new(2.0, 0.0, 0.0))); // boundary inclusive.
    }

    #[test]
    fn sphere_intersects_two_spheres() {
        let a = Sphere::new(Vec3::ZERO, 1.0);
        let b = Sphere::new(Vec3::new(1.5, 0.0, 0.0), 1.0); // overlaps.
        assert!(a.intersects(b));
        let c = Sphere::new(Vec3::new(5.0, 0.0, 0.0), 1.0); // disjoint.
        assert!(!a.intersects(c));
    }

    #[test]
    fn sphere_merge_identity() {
        let s = Sphere::new(Vec3::new(1.0, 2.0, 3.0), 4.0);
        assert_eq!(Sphere::EMPTY.merge(s), s);
        assert_eq!(s.merge(Sphere::EMPTY), s);
    }

    #[test]
    fn sphere_merge_disjoint_grows_to_span_both() {
        let a = Sphere::new(Vec3::ZERO, 1.0);
        let b = Sphere::new(Vec3::new(10.0, 0.0, 0.0), 1.0);
        let m = a.merge(b);
        // Both spheres should be inside the merged one.
        assert!(m.contains_point(Vec3::ZERO));
        assert!(m.contains_point(Vec3::new(10.0, 0.0, 0.0)));
        // Merged radius spans the gap : distance 10 + radii 1+1 ⇒ diameter 12 ⇒ radius 6.
        assert!(approx(m.radius, 6.0, 1e-4));
    }

    #[test]
    fn sphere_merge_one_inside_other_returns_outer() {
        let outer = Sphere::new(Vec3::ZERO, 10.0);
        let inner = Sphere::new(Vec3::new(1.0, 0.0, 0.0), 1.0);
        assert_eq!(outer.merge(inner), outer);
        assert_eq!(inner.merge(outer), outer);
    }

    #[test]
    fn sphere_to_aabb_correct() {
        let s = Sphere::new(Vec3::new(1.0, 2.0, 3.0), 4.0);
        let aabb = s.to_aabb();
        assert_eq!(aabb.min, Vec3::new(-3.0, -2.0, -1.0));
        assert_eq!(aabb.max, Vec3::new(5.0, 6.0, 7.0));
    }

    #[test]
    fn sphere_from_aabb_inscribes_corners() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 2.0, 2.0));
        let s = Sphere::from_aabb(aabb);
        // Sphere should contain all 8 corners.
        for c in aabb.corners() {
            assert!(s.contains_point(c));
        }
    }

    #[test]
    fn sphere_from_points_naive_centroid() {
        let pts = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        ];
        let s = Sphere::from_points(&pts);
        assert!(approx(s.center.x, 0.0, 1e-5));
        assert!(approx(s.center.y, 0.0, 1e-5));
        assert!(approx(s.radius, 1.0, 1e-5));
    }

    #[test]
    fn sphere_ray_hit_from_outside() {
        let s = Sphere::new(Vec3::ZERO, 1.0);
        // Ray from -Z at the origin hits the sphere at z = -1.
        let ray = Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::Z);
        let t = s.ray_intersect(ray).expect("hit");
        assert!(approx(t, 4.0, 1e-5));
    }

    #[test]
    fn sphere_ray_miss_returns_none() {
        let s = Sphere::new(Vec3::ZERO, 1.0);
        // Ray parallel above the sphere.
        let ray = Ray::new(Vec3::new(0.0, 5.0, -5.0), Vec3::Z);
        assert_eq!(s.ray_intersect(ray), None);
    }

    #[test]
    fn sphere_ray_origin_inside_returns_zero() {
        let s = Sphere::new(Vec3::ZERO, 1.0);
        let ray = Ray::new(Vec3::ZERO, Vec3::X);
        let t = s.ray_intersect(ray).expect("hit");
        assert!(approx(t, 0.0, 1e-5));
    }

    #[test]
    fn sphere_ray_behind_origin_returns_none() {
        let s = Sphere::new(Vec3::new(0.0, 0.0, -10.0), 1.0);
        // Ray pointing +Z with origin at +Z 5 — sphere is behind.
        let ray = Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::Z);
        assert_eq!(s.ray_intersect(ray), None);
    }
}
