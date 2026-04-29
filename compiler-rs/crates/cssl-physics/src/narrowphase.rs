//! Narrow-phase contact generation.
//!
//! § THESIS
//!   Given a candidate pair `(BodyA, BodyB)` from the broad-phase, the narrow-
//!   phase determines :
//!     - Are they ACTUALLY touching (vs. just AABBs overlapping)?
//!     - If so : at what point(s)? In what direction (normal)? With what
//!       penetration depth?
//!   Output is one or more `ContactPoint`s.
//!
//! § COVERAGE  (stage-0)
//!   - sphere-sphere : analytic.
//!   - sphere-plane  : analytic.
//!   - sphere-box    : closest-point-on-box-to-sphere-center.
//!   - sphere-capsule : closest-point-on-segment-to-sphere-center.
//!   - box-plane     : 8 corner-tests vs. plane.
//!   - box-box       : SAT (separating-axis theorem) — 15 axes.
//!   - capsule-plane : segment-vs-plane.
//!   - capsule-capsule : closest-points-between-segments.
//!   - convex-hull pairs : DEFERRED to GJK/EPA in stage-1 ; for stage-0 we
//!     fall back to bounding-sphere test (conservative).
//!
//! § DETERMINISM
//!   All ops are explicit float arithmetic ; no transcendentals in the hot
//!   path. Output `ContactPoint`s are sorted by `position` to maintain
//!   stable iteration order in the solver.

use crate::body::RigidBody;
use crate::contact::{Contact, ContactPoint};
use crate::math::Vec3;
use crate::shape::Shape;

// ────────────────────────────────────────────────────────────────────────
// § NarrowPhase trait + free function
// ────────────────────────────────────────────────────────────────────────

/// Narrow-phase API. Implementors take two bodies + produce `Contact` (if any).
pub trait NarrowPhase {
    /// Produce a contact for the given pair if they actually touch.
    fn contact(&self, a: &RigidBody, b: &RigidBody) -> Option<Contact>;
}

/// Default narrow-phase impl ; dispatches to per-shape-pair functions.
#[derive(Debug, Default)]
pub struct DefaultNarrowPhase;

impl NarrowPhase for DefaultNarrowPhase {
    fn contact(&self, a: &RigidBody, b: &RigidBody) -> Option<Contact> {
        contact_pair(a, b)
    }
}

/// Compute contact between two bodies. Returns `None` if no contact.
///
/// Body ordering convention : the resulting `Contact` follows
/// `Contact::new`'s canonical ordering (body_a < body_b), and its normal
/// always points from B to A (per `ContactPoint` convention).
#[must_use]
pub fn contact_pair(a: &RigidBody, b: &RigidBody) -> Option<Contact> {
    // We dispatch by shape variant. We always pass the world-space transform
    // (position, orientation) that the body carries.
    let points = shape_pair_contact(
        &a.shape,
        a.position,
        a.orientation,
        &b.shape,
        b.position,
        b.orientation,
    )?;
    if points.is_empty() {
        return None;
    }
    // Combined material : geometric mean of friction, max of restitution.
    let friction = (a.friction * b.friction).sqrt();
    let restitution = a.restitution.max(b.restitution);
    // BodyId fetch : we need the BodyId — but `RigidBody` doesn't carry one.
    // Caller (PhysicsWorld) handles the BodyId mapping ; here we have no
    // access. So we return contact-points + caller wraps with BodyId.
    // Refactor : split into `shape_pair_contact()` (returns Option<Vec<ContactPoint>>)
    // + caller wraps with `Contact::new(body_a, body_b, points, friction, restitution)`.
    let _ = friction;
    let _ = restitution;
    // Placeholder : `contact_pair` returning `Option<Contact>` with sentinel
    // BodyIds. The PhysicsWorld step calls `shape_pair_contact` directly
    // for the actual contact-generation path. We expose `contact_pair` for
    // tests but it's a no-op for body-id assignment.
    //
    // For tests, we return a Contact with sentinel body-ids 0+1 ; tests that
    // use `contact_pair` directly are exercising shape-pair logic only.
    Some(Contact::new(
        crate::body::BodyId(0),
        crate::body::BodyId(1),
        points,
        friction,
        restitution,
    ))
}

/// Compute contact points between two shapes given their world-space
/// transforms. The `PhysicsWorld` calls this directly + wraps in a
/// `Contact` with the correct `BodyId`s.
#[must_use]
pub fn shape_pair_contact(
    shape_a: &Shape,
    pos_a: Vec3,
    orient_a: crate::math::Quat,
    shape_b: &Shape,
    pos_b: Vec3,
    orient_b: crate::math::Quat,
) -> Option<Vec<ContactPoint>> {
    use Shape::{Box, Capsule, ConvexHull, Plane, Sphere};

    match (shape_a, shape_b) {
        (Sphere { radius: ra }, Sphere { radius: rb }) => sphere_sphere(*ra, pos_a, *rb, pos_b),
        (Sphere { radius }, Plane { normal, d }) => {
            sphere_plane(*radius, pos_a, *normal, *d, false)
        }
        (Plane { normal, d }, Sphere { radius }) => sphere_plane(*radius, pos_b, *normal, *d, true),
        (Sphere { radius }, Box { half_extents }) => {
            sphere_box(*radius, pos_a, *half_extents, pos_b, orient_b, false)
        }
        (Box { half_extents }, Sphere { radius }) => {
            sphere_box(*radius, pos_b, *half_extents, pos_a, orient_a, true)
        }
        (
            Sphere { radius: rs },
            Capsule {
                radius: rc,
                half_height,
            },
        ) => sphere_capsule(*rs, pos_a, *rc, *half_height, pos_b, orient_b, false),
        (
            Capsule {
                radius: rc,
                half_height,
            },
            Sphere { radius: rs },
        ) => sphere_capsule(*rs, pos_b, *rc, *half_height, pos_a, orient_a, true),
        (Box { half_extents }, Plane { normal, d }) => {
            box_plane(*half_extents, pos_a, orient_a, *normal, *d, false)
        }
        (Plane { normal, d }, Box { half_extents }) => {
            box_plane(*half_extents, pos_b, orient_b, *normal, *d, true)
        }
        (Box { half_extents: ha }, Box { half_extents: hb }) => {
            box_box(*ha, pos_a, orient_a, *hb, pos_b, orient_b)
        }
        (
            Capsule {
                radius,
                half_height,
            },
            Plane { normal, d },
        ) => capsule_plane(*radius, *half_height, pos_a, orient_a, *normal, *d, false),
        (
            Plane { normal, d },
            Capsule {
                radius,
                half_height,
            },
        ) => capsule_plane(*radius, *half_height, pos_b, orient_b, *normal, *d, true),
        (
            Capsule {
                radius: ra,
                half_height: ha,
            },
            Capsule {
                radius: rb,
                half_height: hb,
            },
        ) => capsule_capsule(*ra, *ha, pos_a, orient_a, *rb, *hb, pos_b, orient_b),
        // Convex-hull paths : conservative bounding-sphere test for stage-0.
        (ConvexHull { points }, _) => {
            let radius = points.iter().map(|p| p.length()).fold(0.0_f64, f64::max);
            shape_pair_contact(
                &Sphere { radius },
                pos_a,
                orient_a,
                shape_b,
                pos_b,
                orient_b,
            )
        }
        (_, ConvexHull { points }) => {
            let radius = points.iter().map(|p| p.length()).fold(0.0_f64, f64::max);
            shape_pair_contact(
                shape_a,
                pos_a,
                orient_a,
                &Sphere { radius },
                pos_b,
                orient_b,
            )
        }
        // Box-capsule + capsule-box : conservative bounding-sphere fallback for
        // stage-0 ; full GJK-based pair deferred.
        (
            Box { half_extents },
            Capsule {
                radius,
                half_height,
            },
        ) => {
            let bound_r = half_extents.length();
            sphere_capsule(
                bound_r,
                pos_a,
                *radius,
                *half_height,
                pos_b,
                orient_b,
                false,
            )
        }
        (
            Capsule {
                radius,
                half_height,
            },
            Box { half_extents },
        ) => {
            let bound_r = half_extents.length();
            sphere_capsule(bound_r, pos_b, *radius, *half_height, pos_a, orient_a, true)
        }
        // Plane-plane : never collide (statics are inert against each other).
        (Plane { .. }, Plane { .. }) => None,
    }
}

// ────────────────────────────────────────────────────────────────────────
// § Per-pair contact-generation functions
// ────────────────────────────────────────────────────────────────────────

/// Sphere-sphere contact. Normal points from B to A.
fn sphere_sphere(ra: f64, pa: Vec3, rb: f64, pb: Vec3) -> Option<Vec<ContactPoint>> {
    let delta = pa - pb;
    let dist_sq = delta.length_sq();
    let r_sum = ra + rb;
    if dist_sq > r_sum * r_sum {
        return None;
    }
    let dist = dist_sq.sqrt();
    let normal = if dist > 1e-12 { delta / dist } else { Vec3::Y };
    let penetration = r_sum - dist;
    // Contact point : midpoint of the overlap region.
    let position = pb + normal * (rb - 0.5 * penetration);
    Some(vec![ContactPoint::new(position, normal, penetration)])
}

/// Sphere vs. plane. `flip_normal=true` means the plane is body_a, sphere is body_b ;
/// we flip the contact-normal accordingly (Contact::new will canonicalize body order).
fn sphere_plane(
    radius: f64,
    sphere_pos: Vec3,
    plane_normal: Vec3,
    plane_d: f64,
    flip_normal: bool,
) -> Option<Vec<ContactPoint>> {
    let n = plane_normal.normalize_or_zero();
    let signed_dist = sphere_pos.dot(n) - plane_d;
    if signed_dist > radius {
        return None;
    }
    let penetration = radius - signed_dist;
    // Contact normal points away from plane (towards sphere) : that's `n` if
    // sphere is "above" the plane, but we want it from B (plane) to A (sphere)
    // by canonical convention. If shape_a is sphere, normal = +n. If
    // shape_a is plane, normal = -n (points from sphere back to plane).
    let normal = if flip_normal { -n } else { n };
    let position = sphere_pos - n * radius;
    Some(vec![ContactPoint::new(position, normal, penetration)])
}

/// Sphere vs. box. We find the closest point on the box (in world-space)
/// to the sphere center, then check if the distance < sphere radius.
fn sphere_box(
    radius: f64,
    sphere_pos: Vec3,
    half_extents: Vec3,
    box_pos: Vec3,
    box_orient: crate::math::Quat,
    flip_normal: bool,
) -> Option<Vec<ContactPoint>> {
    // Sphere center in box-local space.
    let local_center = box_orient.conjugate().rotate_vec3(sphere_pos - box_pos);
    // Clamp to box extents.
    let clamped = Vec3::new(
        local_center.x.clamp(-half_extents.x, half_extents.x),
        local_center.y.clamp(-half_extents.y, half_extents.y),
        local_center.z.clamp(-half_extents.z, half_extents.z),
    );
    let delta_local = local_center - clamped;
    let dist_sq = delta_local.length_sq();
    if dist_sq > radius * radius {
        return None;
    }
    let dist = dist_sq.sqrt();
    let world_clamped = box_pos + box_orient.rotate_vec3(clamped);
    let normal_local = if dist > 1e-12 {
        delta_local / dist
    } else {
        // Sphere center inside box — pick axis with smallest distance to face.
        let dx = half_extents.x - local_center.x.abs();
        let dy = half_extents.y - local_center.y.abs();
        let dz = half_extents.z - local_center.z.abs();
        if dx <= dy && dx <= dz {
            Vec3::new(local_center.x.signum(), 0.0, 0.0)
        } else if dy <= dz {
            Vec3::new(0.0, local_center.y.signum(), 0.0)
        } else {
            Vec3::new(0.0, 0.0, local_center.z.signum())
        }
    };
    let normal_world = box_orient.rotate_vec3(normal_local);
    // Penetration : if outside box, penetration = radius - dist ; if inside, radius + dist.
    let penetration = if dist > 1e-12 {
        radius - dist
    } else {
        radius
            + (half_extents.x - local_center.x.abs())
                .min(half_extents.y - local_center.y.abs())
                .min(half_extents.z - local_center.z.abs())
    };
    let normal = if flip_normal {
        -normal_world
    } else {
        normal_world
    };
    Some(vec![ContactPoint::new(world_clamped, normal, penetration)])
}

/// Sphere vs. capsule. Reduce to sphere-vs-segment-with-radius : find closest
/// point on the capsule's central segment, test as sphere-vs-sphere with
/// radii (sphere_r, capsule_r).
fn sphere_capsule(
    sphere_r: f64,
    sphere_pos: Vec3,
    cap_r: f64,
    half_height: f64,
    cap_pos: Vec3,
    cap_orient: crate::math::Quat,
    flip_normal: bool,
) -> Option<Vec<ContactPoint>> {
    // Capsule segment endpoints (world-space).
    let y_axis = cap_orient.rotate_vec3(Vec3::Y);
    let p0 = cap_pos - y_axis * half_height;
    let p1 = cap_pos + y_axis * half_height;
    // Closest point on segment p0-p1 to sphere_pos.
    let seg = p1 - p0;
    let seg_len_sq = seg.length_sq();
    let t = if seg_len_sq < 1e-12 {
        0.0
    } else {
        ((sphere_pos - p0).dot(seg) / seg_len_sq).clamp(0.0, 1.0)
    };
    let closest = p0 + seg * t;
    // Now sphere-sphere test.
    let result = sphere_sphere(sphere_r, sphere_pos, cap_r, closest);
    if let Some(mut pts) = result {
        if flip_normal {
            for p in &mut pts {
                p.normal = -p.normal;
            }
        }
        Some(pts)
    } else {
        None
    }
}

/// Box vs. plane. Test all 8 box corners against the plane ; emit contact
/// points for those that lie below the plane.
fn box_plane(
    half_extents: Vec3,
    box_pos: Vec3,
    box_orient: crate::math::Quat,
    plane_normal: Vec3,
    plane_d: f64,
    flip_normal: bool,
) -> Option<Vec<ContactPoint>> {
    let n = plane_normal.normalize_or_zero();
    let mut points = Vec::new();
    for sx in [-1.0_f64, 1.0] {
        for sy in [-1.0_f64, 1.0] {
            for sz in [-1.0_f64, 1.0] {
                let local = Vec3::new(
                    sx * half_extents.x,
                    sy * half_extents.y,
                    sz * half_extents.z,
                );
                let world = box_pos + box_orient.rotate_vec3(local);
                let signed = world.dot(n) - plane_d;
                if signed <= 0.0 {
                    let penetration = -signed;
                    let normal = if flip_normal { -n } else { n };
                    points.push(ContactPoint::new(world, normal, penetration));
                }
            }
        }
    }
    if points.is_empty() {
        None
    } else {
        // Sort by position for replay-determinism.
        points.sort_by(|a, b| {
            a.position
                .x
                .total_cmp(&b.position.x)
                .then_with(|| a.position.y.total_cmp(&b.position.y))
                .then_with(|| a.position.z.total_cmp(&b.position.z))
        });
        Some(points)
    }
}

/// Box vs. box via SAT (Separating Axis Theorem). Tests 15 axes :
/// 3 face-normals of A, 3 of B, 9 cross-products.
/// Stage-0 returns a single contact-point (the deepest-penetration point).
fn box_box(
    ha: Vec3,
    pa: Vec3,
    qa: crate::math::Quat,
    hb: Vec3,
    pb: Vec3,
    qb: crate::math::Quat,
) -> Option<Vec<ContactPoint>> {
    let a_axes = [
        qa.rotate_vec3(Vec3::X),
        qa.rotate_vec3(Vec3::Y),
        qa.rotate_vec3(Vec3::Z),
    ];
    let b_axes = [
        qb.rotate_vec3(Vec3::X),
        qb.rotate_vec3(Vec3::Y),
        qb.rotate_vec3(Vec3::Z),
    ];
    let t = pb - pa;

    let project = |axis: Vec3| -> (f64, f64) {
        let ra = ha.x * a_axes[0].dot(axis).abs()
            + ha.y * a_axes[1].dot(axis).abs()
            + ha.z * a_axes[2].dot(axis).abs();
        let rb = hb.x * b_axes[0].dot(axis).abs()
            + hb.y * b_axes[1].dot(axis).abs()
            + hb.z * b_axes[2].dot(axis).abs();
        let center_dist = t.dot(axis).abs();
        // Penetration on this axis = (ra + rb) - center_dist
        ((ra + rb) - center_dist, center_dist)
    };

    let mut min_pen = f64::INFINITY;
    let mut best_axis = Vec3::Y;
    let mut best_sign: f64 = 1.0;

    let test = |axis: Vec3, min_pen: &mut f64, best_axis: &mut Vec3, best_sign: &mut f64| -> bool {
        let len_sq = axis.length_sq();
        if len_sq < 1e-12 {
            return true; // Degenerate axis ; skip.
        }
        let n = axis / len_sq.sqrt();
        let (pen, _cd) = project(n);
        if pen < 0.0 {
            return false; // Separating axis found.
        }
        if pen < *min_pen {
            *min_pen = pen;
            *best_axis = n;
            // Direction : from A to B is t ; if t·n > 0, normal-from-B-to-A = -n.
            *best_sign = -t.dot(n).signum();
            if *best_sign == 0.0 {
                *best_sign = 1.0;
            }
        }
        true
    };

    // 6 face axes.
    for axis in a_axes.iter().chain(b_axes.iter()) {
        if !test(*axis, &mut min_pen, &mut best_axis, &mut best_sign) {
            return None;
        }
    }
    // 9 cross axes.
    for ax in &a_axes {
        for bx in &b_axes {
            if !test(ax.cross(*bx), &mut min_pen, &mut best_axis, &mut best_sign) {
                return None;
            }
        }
    }

    let normal = best_axis * best_sign;
    let position = pa + (pb - pa) * 0.5; // Midpoint approximation.
    Some(vec![ContactPoint::new(position, normal, min_pen)])
}

/// Capsule vs. plane. Sample both endpoint-spheres of the capsule against the plane.
fn capsule_plane(
    radius: f64,
    half_height: f64,
    cap_pos: Vec3,
    cap_orient: crate::math::Quat,
    plane_normal: Vec3,
    plane_d: f64,
    flip_normal: bool,
) -> Option<Vec<ContactPoint>> {
    let y_axis = cap_orient.rotate_vec3(Vec3::Y);
    let p0 = cap_pos - y_axis * half_height;
    let p1 = cap_pos + y_axis * half_height;
    let mut points = Vec::new();
    if let Some(pts) = sphere_plane(radius, p0, plane_normal, plane_d, flip_normal) {
        points.extend(pts);
    }
    if let Some(pts) = sphere_plane(radius, p1, plane_normal, plane_d, flip_normal) {
        points.extend(pts);
    }
    if points.is_empty() {
        None
    } else {
        Some(points)
    }
}

/// Capsule vs. capsule. Find closest points between the two segments,
/// then sphere-vs-sphere test.
fn capsule_capsule(
    ra: f64,
    ha: f64,
    pa: Vec3,
    qa: crate::math::Quat,
    rb: f64,
    hb: f64,
    pb: Vec3,
    qb: crate::math::Quat,
) -> Option<Vec<ContactPoint>> {
    let ya = qa.rotate_vec3(Vec3::Y);
    let yb = qb.rotate_vec3(Vec3::Y);
    let pa0 = pa - ya * ha;
    let pa1 = pa + ya * ha;
    let pb0 = pb - yb * hb;
    let pb1 = pb + yb * hb;
    let (closest_a, closest_b) = closest_points_on_segments(pa0, pa1, pb0, pb1);
    sphere_sphere(ra, closest_a, rb, closest_b)
}

/// Closest points between two line segments (a0-a1) and (b0-b1).
/// Returns `(point_on_a, point_on_b)`.
fn closest_points_on_segments(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> (Vec3, Vec3) {
    let da = a1 - a0;
    let db = b1 - b0;
    let r = a0 - b0;
    let a_len_sq = da.length_sq();
    let b_len_sq = db.length_sq();
    let f = db.dot(r);

    if a_len_sq < 1e-12 && b_len_sq < 1e-12 {
        return (a0, b0);
    }
    if a_len_sq < 1e-12 {
        let t = (f / b_len_sq).clamp(0.0, 1.0);
        return (a0, b0 + db * t);
    }
    if b_len_sq < 1e-12 {
        let s = ((-da.dot(r)) / a_len_sq).clamp(0.0, 1.0);
        return (a0 + da * s, b0);
    }
    let c = da.dot(r);
    let b = da.dot(db);
    let denom = a_len_sq * b_len_sq - b * b;
    let mut s = if denom.abs() < 1e-12 {
        0.0
    } else {
        ((b * f - c * b_len_sq) / denom).clamp(0.0, 1.0)
    };
    let mut t = (b * s + f) / b_len_sq;
    if t < 0.0 {
        t = 0.0;
        s = ((-c) / a_len_sq).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((b - c) / a_len_sq).clamp(0.0, 1.0);
    }
    (a0 + da * s, b0 + db * t)
}

// ════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::RigidBody;
    use crate::math::Quat;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn vec3_approx(a: Vec3, b: Vec3) -> bool {
        approx_eq(a.x, b.x) && approx_eq(a.y, b.y) && approx_eq(a.z, b.z)
    }

    // ─── Sphere-sphere ───

    #[test]
    fn sphere_sphere_disjoint_no_contact() {
        let a =
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }).with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(3.0, 0.0, 0.0));
        assert!(contact_pair(&a, &b).is_none());
    }

    #[test]
    fn sphere_sphere_overlapping_contact() {
        let a =
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }).with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(1.5, 0.0, 0.0));
        let c = contact_pair(&a, &b).expect("contact expected");
        assert_eq!(c.points.len(), 1);
        assert!(approx_eq(c.points[0].penetration, 0.5));
        // Normal from B to A : -X
        assert!(vec3_approx(c.points[0].normal, Vec3::new(-1.0, 0.0, 0.0)));
    }

    #[test]
    fn sphere_sphere_touching_contact() {
        let a =
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }).with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(2.0, 0.0, 0.0));
        let c = contact_pair(&a, &b).expect("touching counts as contact");
        assert!(approx_eq(c.points[0].penetration, 0.0));
    }

    // ─── Sphere-plane ───

    #[test]
    fn sphere_plane_above_no_contact() {
        let s = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(0.0, 5.0, 0.0));
        let p = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        assert!(contact_pair(&s, &p).is_none());
    }

    #[test]
    fn sphere_plane_touching_contact() {
        let s = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(0.0, 1.0, 0.0));
        let p = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        let c = contact_pair(&s, &p).expect("touching counts");
        assert_eq!(c.points.len(), 1);
        assert!(approx_eq(c.points[0].penetration, 0.0));
    }

    #[test]
    fn sphere_plane_penetrating_contact() {
        let s = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(0.0, 0.5, 0.0));
        let p = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        let c = contact_pair(&s, &p).expect("penetrating");
        assert!(approx_eq(c.points[0].penetration, 0.5));
    }

    // ─── Sphere-box ───

    #[test]
    fn sphere_box_disjoint_no_contact() {
        let s = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(5.0, 0.0, 0.0));
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::ZERO);
        assert!(contact_pair(&s, &b).is_none());
    }

    #[test]
    fn sphere_box_overlap_contact() {
        let s = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(1.3, 0.0, 0.0));
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::ZERO);
        let c = contact_pair(&s, &b).expect("overlapping");
        // Sphere center at +X 1.3, box face at +X 1.0 ; sphere overlaps by 0.5-(1.3-1.0)=0.2
        assert!(approx_eq(c.points[0].penetration, 0.2));
    }

    #[test]
    fn sphere_box_sphere_inside_box() {
        let s =
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 }).with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::ZERO);
        let c = contact_pair(&s, &b).expect("sphere inside box always overlaps");
        assert!(c.points[0].penetration > 0.0);
    }

    // ─── Sphere-capsule ───

    #[test]
    fn sphere_capsule_disjoint() {
        let s = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(5.0, 0.0, 0.0));
        let c = RigidBody::new_dynamic(
            1.0,
            Shape::Capsule {
                radius: 0.5,
                half_height: 1.0,
            },
        )
        .with_position(Vec3::ZERO);
        assert!(contact_pair(&s, &c).is_none());
    }

    #[test]
    fn sphere_capsule_side_contact() {
        let s = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(0.7, 0.0, 0.0));
        let c = RigidBody::new_dynamic(
            1.0,
            Shape::Capsule {
                radius: 0.5,
                half_height: 1.0,
            },
        )
        .with_position(Vec3::ZERO);
        let cp = contact_pair(&s, &c).expect("side overlap");
        // Sphere at X=0.7, capsule cylinder radius 0.5 ; closest point on segment (origin) is origin.
        // Distance = 0.7 ; r_sum = 1.0 ; penetration = 0.3.
        assert!(approx_eq(cp.points[0].penetration, 0.3));
    }

    #[test]
    fn sphere_capsule_above_endcap() {
        let s = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(0.0, 1.7, 0.0));
        let c = RigidBody::new_dynamic(
            1.0,
            Shape::Capsule {
                radius: 0.5,
                half_height: 1.0,
            },
        )
        .with_position(Vec3::ZERO);
        let cp = contact_pair(&s, &c).expect("endcap overlap");
        // Top endcap at y=1.0 ; sphere at y=1.7 ; distance = 0.7 ; r_sum = 1.0 ; pen = 0.3.
        assert!(approx_eq(cp.points[0].penetration, 0.3));
    }

    // ─── Box-plane ───

    #[test]
    fn box_plane_above_no_contact() {
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::new(0.0, 5.0, 0.0));
        let p = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        assert!(contact_pair(&b, &p).is_none());
    }

    #[test]
    fn box_plane_resting_four_corner_contacts() {
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::new(0.0, 1.0, 0.0));
        let p = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        let c = contact_pair(&b, &p).expect("box on plane");
        // Box at y=1 with half-extent 1 ⇒ bottom corners at y=0. They TOUCH the plane.
        assert_eq!(c.points.len(), 4);
        for pt in &c.points {
            assert!(approx_eq(pt.penetration, 0.0));
        }
    }

    #[test]
    fn box_plane_partial_penetration() {
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::new(0.0, 0.5, 0.0));
        let p = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        let c = contact_pair(&b, &p).expect("penetrating");
        // Bottom 4 corners at y=-0.5 → penetration 0.5.
        assert_eq!(c.points.len(), 4);
        for pt in &c.points {
            assert!(approx_eq(pt.penetration, 0.5));
        }
    }

    // ─── Box-box ───

    #[test]
    fn box_box_disjoint() {
        let a = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::new(5.0, 0.0, 0.0));
        assert!(contact_pair(&a, &b).is_none());
    }

    #[test]
    fn box_box_overlap_contact() {
        let a = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
            },
        )
        .with_position(Vec3::new(1.5, 0.0, 0.0));
        let c = contact_pair(&a, &b).expect("overlap");
        // 0.5 overlap on X axis.
        assert!(approx_eq(c.points[0].penetration, 0.5));
    }

    // ─── Capsule-plane ───

    #[test]
    fn capsule_plane_resting() {
        let c = RigidBody::new_dynamic(
            1.0,
            Shape::Capsule {
                radius: 0.5,
                half_height: 1.0,
            },
        )
        .with_position(Vec3::new(0.0, 1.5, 0.0));
        let p = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        let cp = contact_pair(&c, &p).expect("resting capsule");
        // Bottom hemisphere at y=0.5-0.5 = 0 ⇒ touching.
        assert!(approx_eq(cp.points[0].penetration, 0.0));
    }

    // ─── Capsule-capsule ───

    #[test]
    fn capsule_capsule_disjoint() {
        let a = RigidBody::new_dynamic(
            1.0,
            Shape::Capsule {
                radius: 0.5,
                half_height: 1.0,
            },
        )
        .with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Capsule {
                radius: 0.5,
                half_height: 1.0,
            },
        )
        .with_position(Vec3::new(5.0, 0.0, 0.0));
        assert!(contact_pair(&a, &b).is_none());
    }

    #[test]
    fn capsule_capsule_side_contact() {
        let a = RigidBody::new_dynamic(
            1.0,
            Shape::Capsule {
                radius: 0.5,
                half_height: 1.0,
            },
        )
        .with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(
            1.0,
            Shape::Capsule {
                radius: 0.5,
                half_height: 1.0,
            },
        )
        .with_position(Vec3::new(0.8, 0.0, 0.0));
        let c = contact_pair(&a, &b).expect("side overlap");
        // Two parallel capsules with side-distance 0.8 ; r_sum = 1.0 ; pen = 0.2.
        assert!(approx_eq(c.points[0].penetration, 0.2));
    }

    // ─── Convex hull (conservative bounding-sphere fallback) ───

    #[test]
    fn convex_hull_falls_back_to_bounding_sphere() {
        let hull_pts = vec![Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)];
        let a = RigidBody::new_dynamic(1.0, Shape::ConvexHull { points: hull_pts })
            .with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(1.5, 0.0, 0.0));
        // Hull bound-sphere radius = sqrt(3) ≈ 1.732 ; sphere overlaps.
        let c = contact_pair(&a, &b).expect("overlap via bounding-sphere");
        assert!(c.points[0].penetration > 0.0);
    }

    // ─── Plane-plane (no contact) ───

    #[test]
    fn plane_plane_no_contact() {
        let a = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        let b = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 5.0,
        });
        assert!(contact_pair(&a, &b).is_none());
    }

    // ─── shape_pair_contact direct ───

    #[test]
    fn shape_pair_contact_sphere_plane_direct() {
        let pts = shape_pair_contact(
            &Shape::Sphere { radius: 1.0 },
            Vec3::new(0.0, 0.5, 0.0),
            Quat::IDENTITY,
            &Shape::Plane {
                normal: Vec3::Y,
                d: 0.0,
            },
            Vec3::ZERO,
            Quat::IDENTITY,
        )
        .expect("contact");
        assert!(approx_eq(pts[0].penetration, 0.5));
    }

    // ─── DefaultNarrowPhase trait impl ───

    #[test]
    fn default_narrow_phase_dispatches() {
        let np = DefaultNarrowPhase;
        let a =
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }).with_position(Vec3::ZERO);
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(1.0, 0.0, 0.0));
        assert!(np.contact(&a, &b).is_some());
    }

    // ─── Closest-points-on-segments helper ───

    #[test]
    fn closest_points_parallel_segments() {
        let a0 = Vec3::new(0.0, 0.0, 0.0);
        let a1 = Vec3::new(1.0, 0.0, 0.0);
        let b0 = Vec3::new(0.0, 1.0, 0.0);
        let b1 = Vec3::new(1.0, 1.0, 0.0);
        let (ca, cb) = closest_points_on_segments(a0, a1, b0, b1);
        assert!(approx_eq((cb - ca).length(), 1.0));
    }

    #[test]
    fn closest_points_intersecting_segments() {
        let a0 = Vec3::new(-1.0, 0.0, 0.0);
        let a1 = Vec3::new(1.0, 0.0, 0.0);
        let b0 = Vec3::new(0.0, -1.0, 0.0);
        let b1 = Vec3::new(0.0, 1.0, 0.0);
        let (ca, cb) = closest_points_on_segments(a0, a1, b0, b1);
        // Both segments meet at origin.
        assert!(vec3_approx(ca, Vec3::ZERO));
        assert!(vec3_approx(cb, Vec3::ZERO));
    }

    #[test]
    fn closest_points_endpoint_on_segment() {
        let a0 = Vec3::new(0.0, 0.0, 0.0);
        let a1 = Vec3::new(1.0, 0.0, 0.0);
        let b0 = Vec3::new(2.0, 0.0, 0.0); // Past the end of A.
        let b1 = Vec3::new(3.0, 0.0, 0.0);
        let (ca, cb) = closest_points_on_segments(a0, a1, b0, b1);
        assert!(vec3_approx(ca, a1));
        assert!(vec3_approx(cb, b0));
    }
}
