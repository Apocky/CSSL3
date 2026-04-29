//! Shape primitives + AABB + bounding-sphere.
//!
//! § SHAPES
//!   - **Sphere(r)** : radius about body-center. Cheapest narrow-phase test.
//!   - **Box(half_extents)** : axis-aligned (in body-local) ; rotated by
//!     body's quaternion to world-space.
//!   - **Capsule(r, h)** : radius `r`, segment-length `h` along body-local Y.
//!     Pill shape. Common for character controllers.
//!   - **ConvexHull(points)** : convex polytope from a point cloud. Body-local.
//!     Narrow-phase uses GJK/EPA-style intersection (we ship a simple
//!     SAT-with-fallback for stage-0).
//!   - **Plane(normal, d)** : infinite plane `dot(normal, p) == d`. World-space.
//!     Static-only (we don't move planes).
//!
//! § AABB
//!   Axis-aligned bounding-box in world-space. Used by broadphase to prune
//!   narrow-phase work. Each `RigidBody` carries a cached AABB updated each
//!   step ; the broadphase consumes the AABBs.

use crate::math::{Mat3, Quat, Vec3};

// ────────────────────────────────────────────────────────────────────────
// § Shape — body-local geometry
// ────────────────────────────────────────────────────────────────────────

/// A geometric shape attached to a `RigidBody`. Stored body-local ; the
/// world-space form is derived per step from the body's `position` + `orientation`.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Sphere with given radius about the body's local origin.
    Sphere { radius: f64 },
    /// Box with given half-extents along the body's local axes.
    Box { half_extents: Vec3 },
    /// Capsule (pill) — radius `r` cylinder of length `2 * half_height`
    /// extending along the body-local Y axis (top cap at `+half_height`,
    /// bottom cap at `-half_height`).
    Capsule { radius: f64, half_height: f64 },
    /// Convex polytope from a body-local point set. The points define
    /// the hull vertices ; we derive the support-function from these
    /// for narrow-phase intersection.
    ConvexHull { points: Vec<Vec3> },
    /// Infinite static plane in world-space. Stored as `(normal, d)`
    /// where the plane equation is `dot(normal, p) == d`.
    Plane { normal: Vec3, d: f64 },
}

impl Shape {
    /// World-space AABB of the shape, given the body's position + orientation.
    /// For `Plane`, returns an infinite-extent AABB approximation
    /// (bounded by f64::MAX/2 to avoid overflow in BVH math).
    #[must_use]
    pub fn world_aabb(&self, position: Vec3, orientation: Quat) -> Aabb {
        match self {
            Shape::Sphere { radius } => Aabb {
                min: position - Vec3::splat(*radius),
                max: position + Vec3::splat(*radius),
            },
            Shape::Box { half_extents } => {
                // World-space AABB of an oriented box is the AABB of the
                // 8 rotated corners. Optimization : use abs-of-rotation-matrix.
                let r = orientation.to_mat3();
                let abs_r = Mat3 {
                    r0: r.r0.abs(),
                    r1: r.r1.abs(),
                    r2: r.r2.abs(),
                };
                let world_extents = abs_r.mul_vec3(*half_extents);
                Aabb {
                    min: position - world_extents,
                    max: position + world_extents,
                }
            }
            Shape::Capsule {
                radius,
                half_height,
            } => {
                // Capsule extends along body-local Y. World-space tip is
                // body_y_axis * half_height. Bound by abs-Y-axis * half_height
                // + radius in all directions.
                let y_axis = orientation.rotate_vec3(Vec3::Y);
                let half_seg = y_axis.abs() * *half_height;
                Aabb {
                    min: position - half_seg - Vec3::splat(*radius),
                    max: position + half_seg + Vec3::splat(*radius),
                }
            }
            Shape::ConvexHull { points } => {
                // World-transform each point, take min/max.
                if points.is_empty() {
                    return Aabb {
                        min: position,
                        max: position,
                    };
                }
                let mut min = Vec3::splat(f64::INFINITY);
                let mut max = Vec3::splat(f64::NEG_INFINITY);
                for &p in points {
                    let world = position + orientation.rotate_vec3(p);
                    min = min.min(world);
                    max = max.max(world);
                }
                Aabb { min, max }
            }
            Shape::Plane { .. } => {
                // Infinite plane. Use a large sentinel AABB so the broadphase
                // pairs everything with it. The narrow-phase handles plane-vs-X.
                let big = 1e30_f64;
                Aabb {
                    min: Vec3::splat(-big),
                    max: Vec3::splat(big),
                }
            }
        }
    }

    /// World-space bounding sphere of the shape. Used by some BVH variants.
    #[must_use]
    pub fn world_bounding_sphere(&self, position: Vec3, orientation: Quat) -> BoundingSphere {
        match self {
            Shape::Sphere { radius } => BoundingSphere {
                center: position,
                radius: *radius,
            },
            Shape::Box { half_extents } => BoundingSphere {
                center: position,
                radius: half_extents.length(),
            },
            Shape::Capsule {
                radius,
                half_height,
            } => BoundingSphere {
                center: position,
                radius: *radius + *half_height,
            },
            Shape::ConvexHull { points } => {
                // Conservative : max distance from body-center.
                let mut max_d_sq = 0.0_f64;
                for &p in points {
                    let d_sq = p.length_sq();
                    if d_sq > max_d_sq {
                        max_d_sq = d_sq;
                    }
                }
                BoundingSphere {
                    center: position,
                    radius: max_d_sq.sqrt(),
                }
            }
            Shape::Plane { .. } => BoundingSphere {
                center: position,
                radius: 1e30,
            },
        }
        .bound_check(orientation)
    }

    /// Compute body-local inertia tensor for this shape with the given mass.
    /// For a sphere : I = (2/5) m r^2 along all axes.
    /// For a box : I_x = (1/12) m (h^2 + d^2) etc.
    /// For a capsule : combination of sphere caps + cylinder.
    /// For convex hull : approximation via bounding-box.
    /// For plane : infinite (returned as zero — the body is treated as static).
    #[must_use]
    pub fn local_inertia_tensor(&self, mass: f64) -> Mat3 {
        match self {
            Shape::Sphere { radius } => {
                let i = 0.4 * mass * radius * radius;
                Mat3::diagonal(Vec3::new(i, i, i))
            }
            Shape::Box { half_extents } => {
                let h = *half_extents;
                // I_xx = (1/12) m (4 hy^2 + 4 hz^2) = (1/3) m (hy^2 + hz^2)
                let one_third_m = mass / 3.0;
                Mat3::diagonal(Vec3::new(
                    one_third_m * (h.y * h.y + h.z * h.z),
                    one_third_m * (h.x * h.x + h.z * h.z),
                    one_third_m * (h.x * h.x + h.y * h.y),
                ))
            }
            Shape::Capsule {
                radius,
                half_height,
            } => {
                // Approximation : cylinder + 2 hemispheres ; treat as solid
                // capsule with mass distributed by volume ratio. For simplicity
                // we approximate as a box with half_extents (r, half_height, r).
                let h = Vec3::new(*radius, *half_height, *radius);
                let one_third_m = mass / 3.0;
                Mat3::diagonal(Vec3::new(
                    one_third_m * (h.y * h.y + h.z * h.z),
                    one_third_m * (h.x * h.x + h.z * h.z),
                    one_third_m * (h.x * h.x + h.y * h.y),
                ))
            }
            Shape::ConvexHull { points } => {
                // Approximation : AABB of the hull around origin, treat as box.
                if points.is_empty() {
                    return Mat3::ZERO;
                }
                let mut min = Vec3::splat(f64::INFINITY);
                let mut max = Vec3::splat(f64::NEG_INFINITY);
                for &p in points {
                    min = min.min(p);
                    max = max.max(p);
                }
                let half_extents = (max - min) * 0.5;
                Shape::Box { half_extents }.local_inertia_tensor(mass)
            }
            Shape::Plane { .. } => Mat3::ZERO,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// § Aabb — axis-aligned bounding box
// ────────────────────────────────────────────────────────────────────────

/// Axis-aligned bounding box in world-space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Empty AABB (min = +∞, max = -∞). Useful as the seed for a "merge"
    /// reduction.
    pub const EMPTY: Aabb = Aabb {
        min: Vec3 {
            x: f64::INFINITY,
            y: f64::INFINITY,
            z: f64::INFINITY,
        },
        max: Vec3 {
            x: f64::NEG_INFINITY,
            y: f64::NEG_INFINITY,
            z: f64::NEG_INFINITY,
        },
    };

    /// Construct from min + max corners.
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// AABB containing both `self` and `other`.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Whether two AABBs overlap. Inclusive on the boundary (touching = overlap).
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// Whether `other` is fully contained within `self`.
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.min.x <= other.min.x
            && self.max.x >= other.max.x
            && self.min.y <= other.min.y
            && self.max.y >= other.max.y
            && self.min.z <= other.min.z
            && self.max.z >= other.max.z
    }

    /// Surface area (2x sum of pairwise face areas). Used by BVH SAH cost.
    #[must_use]
    pub fn surface_area(self) -> f64 {
        let d = self.max - self.min;
        2.0 * (d.x * d.y + d.y * d.z + d.x * d.z)
    }

    /// Volume.
    #[must_use]
    pub fn volume(self) -> f64 {
        let d = self.max - self.min;
        d.x * d.y * d.z
    }

    /// Center point.
    #[must_use]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Size (max - min).
    #[must_use]
    pub fn extents(self) -> Vec3 {
        self.max - self.min
    }

    /// Expand by a uniform margin.
    #[must_use]
    pub fn expand(self, margin: f64) -> Self {
        Self {
            min: self.min - Vec3::splat(margin),
            max: self.max + Vec3::splat(margin),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// § BoundingSphere — used by some broadphase variants
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingSphere {
    pub center: Vec3,
    pub radius: f64,
}

impl BoundingSphere {
    /// No-op orientation re-bound — placeholder for future-extension where
    /// non-spherical shapes might tighten their bound based on rotation.
    /// Sphere bounds are rotation-invariant ; we just return self.
    #[must_use]
    fn bound_check(self, _orientation: Quat) -> Self {
        self
    }

    /// Whether two bounding spheres overlap.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        let d_sq = (self.center - other.center).length_sq();
        let r_sum = self.radius + other.radius;
        d_sq <= r_sum * r_sum
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn vec3_approx(a: Vec3, b: Vec3) -> bool {
        approx_eq(a.x, b.x) && approx_eq(a.y, b.y) && approx_eq(a.z, b.z)
    }

    // ─── Shape::world_aabb ───

    #[test]
    fn sphere_aabb_at_origin() {
        let s = Shape::Sphere { radius: 1.0 };
        let a = s.world_aabb(Vec3::ZERO, Quat::IDENTITY);
        assert!(vec3_approx(a.min, Vec3::splat(-1.0)));
        assert!(vec3_approx(a.max, Vec3::splat(1.0)));
    }

    #[test]
    fn sphere_aabb_translated() {
        let s = Shape::Sphere { radius: 0.5 };
        let a = s.world_aabb(Vec3::new(2.0, 3.0, 4.0), Quat::IDENTITY);
        assert!(vec3_approx(a.min, Vec3::new(1.5, 2.5, 3.5)));
        assert!(vec3_approx(a.max, Vec3::new(2.5, 3.5, 4.5)));
    }

    #[test]
    fn sphere_aabb_rotation_invariant() {
        let s = Shape::Sphere { radius: 1.0 };
        let q = Quat::from_axis_angle(Vec3::new(1.0, 1.0, 1.0), 0.7);
        let a = s.world_aabb(Vec3::ZERO, q);
        assert!(vec3_approx(a.min, Vec3::splat(-1.0)));
        assert!(vec3_approx(a.max, Vec3::splat(1.0)));
    }

    #[test]
    fn box_aabb_axis_aligned() {
        let s = Shape::Box {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        };
        let a = s.world_aabb(Vec3::ZERO, Quat::IDENTITY);
        assert!(vec3_approx(a.min, Vec3::new(-1.0, -2.0, -3.0)));
        assert!(vec3_approx(a.max, Vec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn box_aabb_45deg_y_rotation_widens_xz() {
        let s = Shape::Box {
            half_extents: Vec3::new(1.0, 1.0, 1.0),
        };
        let q = Quat::from_axis_angle(Vec3::Y, std::f64::consts::FRAC_PI_4);
        let a = s.world_aabb(Vec3::ZERO, q);
        // 45-deg Y rotation of unit cube : X+Z extent = sqrt(2) ≈ 1.414
        assert!(approx_eq(a.max.x, std::f64::consts::SQRT_2));
        assert!(approx_eq(a.max.z, std::f64::consts::SQRT_2));
        assert!(approx_eq(a.max.y, 1.0));
    }

    #[test]
    fn capsule_aabb_y_aligned() {
        let s = Shape::Capsule {
            radius: 0.5,
            half_height: 1.0,
        };
        let a = s.world_aabb(Vec3::ZERO, Quat::IDENTITY);
        // Y-extent = half_height + radius = 1.5
        // X+Z extent = radius = 0.5
        assert!(approx_eq(a.max.y, 1.5));
        assert!(approx_eq(a.max.x, 0.5));
        assert!(approx_eq(a.max.z, 0.5));
    }

    #[test]
    fn convex_hull_aabb_unit_cube_corners() {
        let pts = vec![
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(-1.0, 1.0, -1.0),
            Vec3::new(1.0, 1.0, -1.0),
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(-1.0, 1.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
        ];
        let s = Shape::ConvexHull { points: pts };
        let a = s.world_aabb(Vec3::ZERO, Quat::IDENTITY);
        assert!(vec3_approx(a.min, Vec3::splat(-1.0)));
        assert!(vec3_approx(a.max, Vec3::splat(1.0)));
    }

    #[test]
    fn plane_aabb_is_huge() {
        let s = Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        };
        let a = s.world_aabb(Vec3::ZERO, Quat::IDENTITY);
        assert!(a.max.y > 1e29);
        assert!(a.min.y < -1e29);
    }

    // ─── Shape::local_inertia_tensor ───

    #[test]
    fn sphere_inertia_uniform_diagonal() {
        let s = Shape::Sphere { radius: 1.0 };
        let i = s.local_inertia_tensor(10.0);
        // I = (2/5) m r^2 = 4 along all axes
        assert!(approx_eq(i.r0.x, 4.0));
        assert!(approx_eq(i.r1.y, 4.0));
        assert!(approx_eq(i.r2.z, 4.0));
        assert_eq!(i.r0.y, 0.0);
        assert_eq!(i.r0.z, 0.0);
    }

    #[test]
    fn box_inertia_per_spec() {
        // Box with half-extents (1, 2, 3) → full-extents (2, 4, 6)
        // I_xx = (1/12) * m * (4*h_y^2 + 4*h_z^2) = (1/3) m (4 + 9) = (1/3) m 13
        let s = Shape::Box {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        };
        let i = s.local_inertia_tensor(3.0);
        assert!(approx_eq(i.r0.x, 13.0));
        assert!(approx_eq(i.r1.y, 10.0));
        assert!(approx_eq(i.r2.z, 5.0));
    }

    #[test]
    fn plane_inertia_zero() {
        let s = Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        };
        let i = s.local_inertia_tensor(100.0);
        assert_eq!(i, Mat3::ZERO);
    }

    // ─── Aabb ───

    #[test]
    fn aabb_empty_merge_with_box_yields_box() {
        let a = Aabb::EMPTY;
        let b = Aabb::new(Vec3::ZERO, Vec3::splat(1.0));
        let merged = a.merge(b);
        assert!(vec3_approx(merged.min, Vec3::ZERO));
        assert!(vec3_approx(merged.max, Vec3::splat(1.0)));
    }

    #[test]
    fn aabb_overlap_disjoint() {
        let a = Aabb::new(Vec3::ZERO, Vec3::splat(1.0));
        let b = Aabb::new(Vec3::splat(2.0), Vec3::splat(3.0));
        assert!(!a.overlaps(b));
    }

    #[test]
    fn aabb_overlap_touching() {
        let a = Aabb::new(Vec3::ZERO, Vec3::splat(1.0));
        let b = Aabb::new(Vec3::splat(1.0), Vec3::splat(2.0));
        // Touching boundaries count as overlap (inclusive).
        assert!(a.overlaps(b));
    }

    #[test]
    fn aabb_overlap_intersecting() {
        let a = Aabb::new(Vec3::ZERO, Vec3::splat(2.0));
        let b = Aabb::new(Vec3::splat(1.0), Vec3::splat(3.0));
        assert!(a.overlaps(b));
    }

    #[test]
    fn aabb_contains() {
        let a = Aabb::new(Vec3::splat(-1.0), Vec3::splat(2.0));
        let b = Aabb::new(Vec3::ZERO, Vec3::splat(1.0));
        assert!(a.contains(b));
        assert!(!b.contains(a));
    }

    #[test]
    fn aabb_surface_area_unit_cube() {
        let a = Aabb::new(Vec3::ZERO, Vec3::splat(1.0));
        // 2 * (1+1+1) = 6
        assert!(approx_eq(a.surface_area(), 6.0));
    }

    #[test]
    fn aabb_volume() {
        let a = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 3.0, 4.0));
        assert!(approx_eq(a.volume(), 24.0));
    }

    #[test]
    fn aabb_center_extents() {
        let a = Aabb::new(Vec3::new(-1.0, 0.0, 1.0), Vec3::new(3.0, 4.0, 5.0));
        assert!(vec3_approx(a.center(), Vec3::new(1.0, 2.0, 3.0)));
        assert!(vec3_approx(a.extents(), Vec3::new(4.0, 4.0, 4.0)));
    }

    #[test]
    fn aabb_expand() {
        let a = Aabb::new(Vec3::ZERO, Vec3::splat(1.0));
        let e = a.expand(0.5);
        assert!(vec3_approx(e.min, Vec3::splat(-0.5)));
        assert!(vec3_approx(e.max, Vec3::splat(1.5)));
    }

    // ─── BoundingSphere ───

    #[test]
    fn bounding_sphere_overlap() {
        let a = BoundingSphere {
            center: Vec3::ZERO,
            radius: 1.0,
        };
        let b = BoundingSphere {
            center: Vec3::new(1.5, 0.0, 0.0),
            radius: 1.0,
        };
        assert!(a.overlaps(b));
    }

    #[test]
    fn bounding_sphere_disjoint() {
        let a = BoundingSphere {
            center: Vec3::ZERO,
            radius: 0.5,
        };
        let b = BoundingSphere {
            center: Vec3::new(2.0, 0.0, 0.0),
            radius: 0.5,
        };
        assert!(!a.overlaps(b));
    }

    #[test]
    fn world_bounding_sphere_sphere() {
        let s = Shape::Sphere { radius: 1.5 };
        let bs = s.world_bounding_sphere(Vec3::splat(2.0), Quat::IDENTITY);
        assert!(vec3_approx(bs.center, Vec3::splat(2.0)));
        assert!(approx_eq(bs.radius, 1.5));
    }
}
