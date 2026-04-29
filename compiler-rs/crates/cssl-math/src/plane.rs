//! § Plane — infinite plane in 3D
//!
//! Plane defined by a unit normal + signed-distance from origin.
//! Equivalently : `dot(normal, point) + d == 0` for points on the plane.
//! Used for clipping, frustum tests, and reflection computations.

use crate::ray::Ray;
use crate::scalar::EPSILON_F32;
use crate::vec3::Vec3;

/// Infinite plane in 3D space. The plane equation is
/// `dot(normal, p) + distance == 0` for `p` on the plane.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Plane {
    /// Unit-length normal vector. Constructors normalize on entry to
    /// guarantee unit length.
    pub normal: Vec3,
    /// Signed distance from the origin along the normal — negative
    /// values place the origin on the positive (front) side.
    pub distance: f32,
}

impl Plane {
    /// Construct from a unit-length normal and a signed distance.
    /// The normal is normalized internally for safety.
    #[must_use]
    pub fn new(normal: Vec3, distance: f32) -> Self {
        let n = normal.normalize();
        // If the input was zero, normalize returns zero — preserve the
        // distance but warn the caller through a degenerate plane.
        Self {
            normal: n,
            distance,
        }
    }

    /// Construct a plane from a point on the plane + a normal.
    /// The plane equation `dot(n, p) + d == 0` ⇒ `d = -dot(n, p)`.
    #[must_use]
    pub fn from_point_and_normal(point: Vec3, normal: Vec3) -> Self {
        let n = normal.normalize();
        Self {
            normal: n,
            distance: -n.dot(point),
        }
    }

    /// Construct a plane from three points (CCW orientation viewed from
    /// the normal-pointing side gives the front face via the right-hand
    /// rule). Returns a degenerate plane (zero normal) if the three
    /// points are collinear.
    ///
    /// Implementation note : the normal is `(c - a) × (b - a)` rather
    /// than `(b - a) × (c - a)`. Reading "CCW from above" : looking DOWN
    /// at the three points along the negative-normal axis, the points
    /// are ordered counter-clockwise. The cross product that produces a
    /// normal pointing UP (toward the viewer) is `(c - a) × (b - a)` for
    /// `a → b → c` CCW.
    #[must_use]
    pub fn from_three_points(a: Vec3, b: Vec3, c: Vec3) -> Self {
        let n = (c - a).cross(b - a);
        Self::from_point_and_normal(a, n)
    }

    /// Signed distance from `point` to the plane. Positive on the
    /// front (normal-pointing) side, negative on the back.
    #[must_use]
    pub fn signed_distance(self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }

    /// True if `point` is on the front (positive-normal) side of the
    /// plane. Plane points themselves return `true` (boundary
    /// inclusive on the front side).
    #[must_use]
    pub fn point_on_front(self, point: Vec3) -> bool {
        self.signed_distance(point) >= 0.0
    }

    /// Project `point` onto the plane.
    #[must_use]
    pub fn project_point(self, point: Vec3) -> Vec3 {
        point - self.normal * self.signed_distance(point)
    }

    /// Reflect `point` across the plane.
    #[must_use]
    pub fn reflect_point(self, point: Vec3) -> Vec3 {
        point - self.normal * (2.0 * self.signed_distance(point))
    }

    /// Reflect a direction vector across the plane.
    #[must_use]
    pub fn reflect_vector(self, v: Vec3) -> Vec3 {
        v.reflect(self.normal)
    }

    /// Ray vs plane intersection. Returns `Some(t)` for the parametric
    /// distance to the hit point ; `None` if the ray is parallel or
    /// the hit is behind the origin.
    #[must_use]
    pub fn ray_intersect(self, ray: Ray) -> Option<f32> {
        let denom = self.normal.dot(ray.direction);
        if denom.abs() < EPSILON_F32 {
            // Parallel.
            return None;
        }
        let t = -(self.signed_distance(ray.origin)) / denom;
        if t < 0.0 {
            None
        } else {
            Some(t)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Plane;
    use crate::ray::Ray;
    use crate::vec3::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec_approx(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps) && approx(a.z, b.z, eps)
    }

    #[test]
    fn plane_from_point_and_normal_satisfies_equation() {
        let n = Vec3::Y;
        let p = Vec3::new(1.0, 5.0, 2.0);
        let plane = Plane::from_point_and_normal(p, n);
        // Original point should have signed_distance ≈ 0.
        assert!(approx(plane.signed_distance(p), 0.0, 1e-5));
    }

    #[test]
    fn plane_signed_distance_known_values() {
        // XZ-plane (y = 0).
        let plane = Plane::new(Vec3::Y, 0.0);
        assert!(approx(plane.signed_distance(Vec3::new(0.0, 5.0, 0.0)), 5.0, 1e-5));
        assert!(approx(plane.signed_distance(Vec3::new(0.0, -3.0, 0.0)), -3.0, 1e-5));
        assert!(approx(plane.signed_distance(Vec3::ZERO), 0.0, 1e-5));
    }

    #[test]
    fn plane_point_on_front_for_positive_distance() {
        let plane = Plane::new(Vec3::Y, 0.0);
        assert!(plane.point_on_front(Vec3::new(0.0, 1.0, 0.0)));
        assert!(!plane.point_on_front(Vec3::new(0.0, -1.0, 0.0)));
    }

    #[test]
    fn plane_project_point_lands_on_plane() {
        let plane = Plane::new(Vec3::Y, 0.0);
        let p = Vec3::new(1.0, 5.0, 2.0);
        let proj = plane.project_point(p);
        // Projection should have y = 0.
        assert!(approx(proj.y, 0.0, 1e-5));
        assert!(approx(proj.x, 1.0, 1e-5));
        assert!(approx(proj.z, 2.0, 1e-5));
    }

    #[test]
    fn plane_reflect_point_mirrors_across_plane() {
        let plane = Plane::new(Vec3::Y, 0.0);
        let p = Vec3::new(1.0, 3.0, 2.0);
        let r = plane.reflect_point(p);
        assert!(vec_approx(r, Vec3::new(1.0, -3.0, 2.0), 1e-5));
    }

    #[test]
    fn plane_reflect_vector_flips_normal_component() {
        let plane = Plane::new(Vec3::Y, 0.0);
        let v = Vec3::new(1.0, -2.0, 3.0); // pointing into plane.
        let r = plane.reflect_vector(v);
        // Reflected direction should have y flipped.
        assert!(vec_approx(r, Vec3::new(1.0, 2.0, 3.0), 1e-5));
    }

    #[test]
    fn plane_from_three_points_orients_via_right_hand_rule() {
        // Three points in the XZ-plane CCW from above ⇒ normal = +Y.
        let a = Vec3::ZERO;
        let b = Vec3::X;
        let c = Vec3::Z;
        let plane = Plane::from_three_points(a, b, c);
        // a→b→c CCW from +Y above ⇒ normal points +Y.
        assert!(vec_approx(plane.normal, Vec3::Y, 1e-5));
    }

    #[test]
    fn plane_ray_intersect_known_distance() {
        // Plane at y = 0 with normal +Y. Ray from (0, 5, 0) toward -Y
        // hits at t = 5.
        let plane = Plane::new(Vec3::Y, 0.0);
        let ray = Ray::new(Vec3::new(0.0, 5.0, 0.0), -Vec3::Y);
        let t = plane.ray_intersect(ray).expect("hit");
        assert!(approx(t, 5.0, 1e-5));
    }

    #[test]
    fn plane_ray_parallel_returns_none() {
        let plane = Plane::new(Vec3::Y, 0.0);
        let ray = Ray::new(Vec3::new(0.0, 5.0, 0.0), Vec3::X);
        assert_eq!(plane.ray_intersect(ray), None);
    }

    #[test]
    fn plane_ray_behind_origin_returns_none() {
        let plane = Plane::new(Vec3::Y, 0.0);
        // Ray below the plane pointing further down.
        let ray = Ray::new(Vec3::new(0.0, -5.0, 0.0), -Vec3::Y);
        assert_eq!(plane.ray_intersect(ray), None);
    }
}
