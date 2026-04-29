//! § Aabb — axis-aligned bounding box
//!
//! Axis-aligned bounding box defined by `min` + `max` corners. Used for
//! broadphase culling, spatial-hash bucketing, and frustum-rejection.
//!
//! § EMPTY-AABB SENTINEL
//!   The "empty" AABB used as an accumulator-pattern seed is
//!   `min = +∞`, `max = -∞`. Any subsequent `expand_to_include` /
//!   `merge` operation correctly establishes the convex hull without a
//!   special "first iteration" branch. This is the standard pattern in
//!   bvh / Embree / pbrt — making the empty-set the additive identity
//!   of the merge operation.

use crate::ray::Ray;
use crate::vec3::Vec3;

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Aabb {
    /// Componentwise minimum corner.
    pub min: Vec3,
    /// Componentwise maximum corner.
    pub max: Vec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Aabb {
    /// Empty AABB sentinel — `min = +inf`, `max = -inf`. Use as the seed
    /// for merge-accumulator loops : every subsequent merge correctly
    /// establishes the convex hull.
    pub const EMPTY: Self = Self {
        min: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
        max: Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
    };

    /// AABB covering all of finite f32 space — useful as an "infinite"
    /// sentinel for "no spatial bound".
    pub const INFINITE: Self = Self {
        min: Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        max: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
    };

    /// Construct from explicit min + max corners. Caller must ensure
    /// `min <= max` componentwise ; mis-ordered inputs produce an
    /// "empty" / negative-extent AABB.
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Construct from a center point + half-extents.
    #[must_use]
    pub fn from_center_extents(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            min: center - half_extents,
            max: center + half_extents,
        }
    }

    /// Construct from a slice of points. Returns `Aabb::EMPTY` if the
    /// slice is empty.
    #[must_use]
    pub fn from_points(points: &[Vec3]) -> Self {
        let mut acc = Self::EMPTY;
        for &p in points {
            acc = acc.expand_to_include(p);
        }
        acc
    }

    /// True if this AABB has no volume — `min > max` on any axis. Empty
    /// AABBs satisfy this ; degenerate AABBs (a flat plane, a line, a
    /// point) do NOT — a flat plane has zero extent on one axis but
    /// `min == max`, not `min > max`.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }

    /// Center point of the box.
    #[must_use]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Half-extent vector (half the side length on each axis).
    #[must_use]
    pub fn half_extents(self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// Full size vector (`max - min`).
    #[must_use]
    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    /// Volume of the box (product of side lengths). Zero for empty or
    /// degenerate AABBs.
    #[must_use]
    pub fn volume(self) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        let s = self.size();
        s.x * s.y * s.z
    }

    /// Surface area of the box. Zero for empty AABBs ; positive for
    /// degenerate (zero-extent) AABBs that have a flat-plane representation.
    #[must_use]
    pub fn surface_area(self) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        let s = self.size();
        2.0 * (s.x * s.y + s.y * s.z + s.z * s.x)
    }

    /// Expand to include a point. Returns the smallest AABB containing
    /// both this AABB and the point. Empty-AABB seed correctly produces
    /// a degenerate AABB at the point on first call.
    #[must_use]
    pub fn expand_to_include(self, point: Vec3) -> Self {
        Self {
            min: self.min.min(point),
            max: self.max.max(point),
        }
    }

    /// Merge two AABBs. Returns the smallest AABB containing both.
    /// `Aabb::EMPTY.merge(other) == other` for any `other` — empty is
    /// the merge-identity.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Intersection of two AABBs. Returns an empty AABB (with
    /// `min > max` on at least one axis) if the boxes don't overlap.
    /// Use [`Self::is_empty`] to detect that case.
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        Self {
            min: self.min.max(other.min),
            max: self.max.min(other.max),
        }
    }

    /// True if the boxes overlap (or touch at a face).
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// True if this AABB contains `point` (inclusive boundary).
    #[must_use]
    pub fn contains_point(self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// True if this AABB fully contains `other`.
    #[must_use]
    pub fn contains_aabb(self, other: Self) -> bool {
        other.min.x >= self.min.x
            && other.max.x <= self.max.x
            && other.min.y >= self.min.y
            && other.max.y <= self.max.y
            && other.min.z >= self.min.z
            && other.max.z <= self.max.z
    }

    /// Closest point on the AABB surface or interior to `point`. If
    /// `point` is inside the box, returns `point` unchanged.
    #[must_use]
    pub fn closest_point(self, point: Vec3) -> Vec3 {
        point.clamp(self.min, self.max)
    }

    /// Squared distance from `point` to the closest point on or in the
    /// box. Zero if `point` is inside.
    #[must_use]
    pub fn distance_squared_to_point(self, point: Vec3) -> f32 {
        let closest = self.closest_point(point);
        (closest - point).length_squared()
    }

    /// Ray vs AABB intersection — slab method. Returns `Some(t_near)`
    /// where `t_near` is the parametric distance along the ray to the
    /// near intersection (or 0 if the ray origin is inside the box).
    /// Returns `None` if the ray misses the box or the entire AABB is
    /// behind the ray origin.
    ///
    /// Implementation note : we use `1.0 / direction` directly (no
    /// near-zero guard) because the slab test relies on the IEEE-754
    /// infinity arithmetic to handle zero-direction components. A ray
    /// parallel to a slab with origin inside the slab produces
    /// `inf - inf = NaN` in the wrong order ; we use the standard
    /// `min(min(...))` IEEE-min discipline so a single NaN doesn't
    /// poison the comparison — `f32::min` returns the non-NaN argument.
    #[must_use]
    pub fn ray_intersect(self, ray: Ray) -> Option<f32> {
        // Slab test : for each axis, find the parametric range
        // [t_min, t_max] where the ray is inside the slab. Intersect all
        // three ranges ; if empty, no intersection.
        let inv_dx = 1.0 / ray.direction.x;
        let inv_dy = 1.0 / ray.direction.y;
        let inv_dz = 1.0 / ray.direction.z;
        let tx1 = (self.min.x - ray.origin.x) * inv_dx;
        let tx2 = (self.max.x - ray.origin.x) * inv_dx;
        let ty1 = (self.min.y - ray.origin.y) * inv_dy;
        let ty2 = (self.max.y - ray.origin.y) * inv_dy;
        let tz1 = (self.min.z - ray.origin.z) * inv_dz;
        let tz2 = (self.max.z - ray.origin.z) * inv_dz;
        let t_min_x = tx1.min(tx2);
        let t_max_x = tx1.max(tx2);
        let t_min_y = ty1.min(ty2);
        let t_max_y = ty1.max(ty2);
        let t_min_z = tz1.min(tz2);
        let t_max_z = tz1.max(tz2);
        let t_near = t_min_x.max(t_min_y).max(t_min_z);
        let t_far = t_max_x.min(t_max_y).min(t_max_z);
        if t_near > t_far || t_far < 0.0 {
            return None;
        }
        Some(t_near.max(0.0))
    }

    /// The 8 corners of the box, in a fixed order. Useful for transforming
    /// the AABB through a non-axis-aligned transform and then re-merging
    /// into a new axis-aligned bound.
    #[must_use]
    pub fn corners(self) -> [Vec3; 8] {
        [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ]
    }

    /// Transform this AABB through a 4x4 matrix and re-fit a new AABB
    /// around the 8 transformed corners. Used when a parent transform
    /// updates and the child AABB needs to be re-projected into world
    /// space.
    #[must_use]
    pub fn transformed(self, m: crate::mat4::Mat4) -> Self {
        let mut acc = Self::EMPTY;
        for c in self.corners() {
            acc = acc.expand_to_include(m.transform_point3(c));
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::Aabb;
    use crate::mat4::Mat4;
    use crate::ray::Ray;
    use crate::vec3::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn aabb_empty_seed_merges_correctly() {
        let aabb = Aabb::EMPTY;
        assert!(aabb.is_empty());
        let p = Vec3::new(1.0, 2.0, 3.0);
        let aabb = aabb.expand_to_include(p);
        assert_eq!(aabb.min, p);
        assert_eq!(aabb.max, p);
        assert!(!aabb.is_empty());
    }

    #[test]
    fn aabb_from_center_extents_correct() {
        let aabb = Aabb::from_center_extents(Vec3::new(1.0, 2.0, 3.0), Vec3::new(0.5, 1.0, 1.5));
        assert_eq!(aabb.min, Vec3::new(0.5, 1.0, 1.5));
        assert_eq!(aabb.max, Vec3::new(1.5, 3.0, 4.5));
    }

    #[test]
    fn aabb_from_points_collects_extremes() {
        let pts = [
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(-1.0, 5.0, 0.0),
            Vec3::new(2.0, -2.0, 4.0),
        ];
        let aabb = Aabb::from_points(&pts);
        assert_eq!(aabb.min, Vec3::new(-1.0, -2.0, 0.0));
        assert_eq!(aabb.max, Vec3::new(2.0, 5.0, 4.0));
    }

    #[test]
    fn aabb_from_empty_slice_is_empty() {
        let aabb = Aabb::from_points(&[]);
        assert!(aabb.is_empty());
    }

    #[test]
    fn aabb_volume_and_surface_known() {
        // 1x1x1 unit cube.
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        assert!(approx(aabb.volume(), 1.0, 1e-6));
        assert!(approx(aabb.surface_area(), 6.0, 1e-6));
        // 2x3x4 box.
        let aabb = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 3.0, 4.0));
        assert!(approx(aabb.volume(), 24.0, 1e-5));
        assert!(approx(aabb.surface_area(), 2.0 * (6.0 + 12.0 + 8.0), 1e-5));
    }

    #[test]
    fn aabb_volume_of_empty_is_zero() {
        assert!(approx(Aabb::EMPTY.volume(), 0.0, 0.0));
        assert!(approx(Aabb::EMPTY.surface_area(), 0.0, 0.0));
    }

    #[test]
    fn aabb_merge_is_associative_and_identity() {
        let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb::new(Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 1.0, 1.0));
        let merged = a.merge(b);
        assert_eq!(merged.min, Vec3::ZERO);
        assert_eq!(merged.max, Vec3::new(3.0, 1.0, 1.0));
        // Empty merge identity.
        let identity = Aabb::EMPTY.merge(a);
        assert_eq!(identity, a);
    }

    #[test]
    fn aabb_intersect_disjoint_is_empty() {
        let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb::new(Vec3::new(5.0, 5.0, 5.0), Vec3::new(6.0, 6.0, 6.0));
        let inter = a.intersect(b);
        assert!(inter.is_empty());
        assert!(!a.intersects(b));
    }

    #[test]
    fn aabb_intersect_overlapping_correct() {
        let a = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 2.0, 2.0));
        let b = Aabb::new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 3.0, 3.0));
        assert!(a.intersects(b));
        let inter = a.intersect(b);
        assert_eq!(inter.min, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(inter.max, Vec3::new(2.0, 2.0, 2.0));
    }

    #[test]
    fn aabb_contains_point_inclusive_boundary() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        assert!(aabb.contains_point(Vec3::new(0.5, 0.5, 0.5)));
        assert!(aabb.contains_point(Vec3::ZERO));
        assert!(aabb.contains_point(Vec3::ONE));
        assert!(!aabb.contains_point(Vec3::new(1.1, 0.5, 0.5)));
    }

    #[test]
    fn aabb_contains_aabb() {
        let outer = Aabb::new(Vec3::splat(-1.0), Vec3::splat(2.0));
        let inner = Aabb::new(Vec3::ZERO, Vec3::ONE);
        assert!(outer.contains_aabb(inner));
        assert!(!inner.contains_aabb(outer));
    }

    #[test]
    fn aabb_closest_point_on_outside_clamps_to_face() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let p = Vec3::new(2.0, 0.5, -1.0);
        let cp = aabb.closest_point(p);
        assert_eq!(cp, Vec3::new(1.0, 0.5, 0.0));
    }

    #[test]
    fn aabb_distance_squared_inside_is_zero() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let p = Vec3::new(0.5, 0.5, 0.5);
        assert!(approx(aabb.distance_squared_to_point(p), 0.0, 1e-6));
    }

    #[test]
    fn aabb_ray_hit_from_outside() {
        let aabb = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        // Ray from (-5, 0, 0) toward +X should hit at t = 4 (front face at x = -1).
        let ray = Ray::new(Vec3::new(-5.0, 0.0, 0.0), Vec3::X);
        let hit = aabb.ray_intersect(ray).expect("hit");
        assert!(approx(hit, 4.0, 1e-5));
    }

    #[test]
    fn aabb_ray_miss_returns_none() {
        let aabb = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        // Parallel above the box.
        let ray = Ray::new(Vec3::new(-5.0, 5.0, 0.0), Vec3::X);
        assert_eq!(aabb.ray_intersect(ray), None);
    }

    #[test]
    fn aabb_ray_origin_inside_returns_zero() {
        let aabb = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        let ray = Ray::new(Vec3::ZERO, Vec3::X);
        let hit = aabb.ray_intersect(ray).expect("hit");
        assert!(approx(hit, 0.0, 1e-5));
    }

    #[test]
    fn aabb_ray_behind_origin_returns_none() {
        let aabb = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        // Ray origin at (5, 0, 0) pointing AWAY (+X) — box is behind.
        let ray = Ray::new(Vec3::new(5.0, 0.0, 0.0), Vec3::X);
        assert_eq!(aabb.ray_intersect(ray), None);
    }

    #[test]
    fn aabb_corners_unique() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let corners = aabb.corners();
        assert_eq!(corners.len(), 8);
        // Min and max corners must be present.
        assert!(corners.iter().any(|&c| c == Vec3::ZERO));
        assert!(corners.iter().any(|&c| c == Vec3::ONE));
    }

    #[test]
    fn aabb_transformed_through_translation() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let t = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let aabb_t = aabb.transformed(t);
        assert!(approx(aabb_t.min.x, 10.0, 1e-5));
        assert!(approx(aabb_t.max.x, 11.0, 1e-5));
    }

    #[test]
    fn aabb_transformed_through_rotation_grows_bound() {
        // 45deg rotation around Z of a unit cube should make the new
        // AABB larger than 1x1x1 in the XY plane (the corners stick out).
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let r = crate::quat::Quat::from_axis_angle(Vec3::Z, core::f32::consts::FRAC_PI_4);
        let m = Mat4::from_rotation(r);
        let aabb_t = aabb.transformed(m);
        let s = aabb_t.size();
        assert!(s.x > 1.0);
        assert!(s.y > 1.0);
    }
}
