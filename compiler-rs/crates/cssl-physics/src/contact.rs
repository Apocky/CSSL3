//! `Contact` + `ContactPoint` + `ContactManifold`.
//!
//! § THESIS
//!   Narrow-phase produces `Contact`s — one or more `ContactPoint`s describing
//!   where two bodies are touching, with what normal, and at what penetration
//!   depth. The solver consumes contacts to compute resolution impulses.
//!
//! § STRUCTURE
//!   - `Contact` : a pair of bodies + a list of contact points + a per-pair
//!     friction coefficient (averaged from both bodies).
//!   - `ContactPoint` : world-space position, world-space normal (from b → a),
//!     penetration depth, accumulated normal+tangent impulses for warm-starting.
//!   - `ContactManifold` : alias used in some narrow-phase contexts ; here
//!     equivalent to the contact-points list inside a `Contact`.

use crate::body::BodyId;
use crate::math::Vec3;

/// One point of contact between two bodies.
///
/// § COORDINATE CONVENTIONS
///   - `position` is world-space.
///   - `normal` points FROM body B TOWARD body A (so that resolution
///     impulse on A is `+J * normal`, on B is `-J * normal`).
///   - `penetration` is the overlap depth (positive when bodies penetrate).
///   - `accumulated_normal_impulse` + `accumulated_tangent_impulse_*` are
///     the warm-start values from the previous frame (zero on first contact).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactPoint {
    pub position: Vec3,
    pub normal: Vec3,
    pub penetration: f64,
    pub accumulated_normal_impulse: f64,
    pub accumulated_tangent_impulse_1: f64,
    pub accumulated_tangent_impulse_2: f64,
}

impl ContactPoint {
    #[must_use]
    pub fn new(position: Vec3, normal: Vec3, penetration: f64) -> Self {
        Self {
            position,
            normal: normal.normalize_or_zero(),
            penetration,
            accumulated_normal_impulse: 0.0,
            accumulated_tangent_impulse_1: 0.0,
            accumulated_tangent_impulse_2: 0.0,
        }
    }

    /// Compute two world-space tangent vectors orthogonal to the normal,
    /// for friction impulse computation. Uses Gram-Schmidt against an
    /// arbitrary axis.
    #[must_use]
    pub fn tangent_basis(self) -> (Vec3, Vec3) {
        // Use the world axis least-aligned with `normal` to start.
        let n = self.normal;
        let abs = n.abs();
        let candidate = if abs.x <= abs.y && abs.x <= abs.z {
            Vec3::X
        } else if abs.y <= abs.z {
            Vec3::Y
        } else {
            Vec3::Z
        };
        // t1 = normalize(candidate - (candidate·n) n)
        let t1 = (candidate - n * candidate.dot(n)).normalize_or_zero();
        // t2 = n × t1
        let t2 = n.cross(t1);
        (t1, t2)
    }
}

/// A contact between two bodies, with one or more contact points.
///
/// § BODY ORDER
///   The `body_a` < `body_b` invariant is maintained at construction time
///   for canonical iteration order — important for replay-determinism.
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub body_a: BodyId,
    pub body_b: BodyId,
    pub points: Vec<ContactPoint>,
    /// Combined friction (geometric mean of the two body frictions).
    pub friction: f64,
    /// Combined restitution (max of the two body restitutions per Baraff's convention).
    pub restitution: f64,
}

impl Contact {
    /// Construct a contact, normalizing body ordering.
    #[must_use]
    pub fn new(
        body_a: BodyId,
        body_b: BodyId,
        points: Vec<ContactPoint>,
        friction: f64,
        restitution: f64,
    ) -> Self {
        if body_a <= body_b {
            Self {
                body_a,
                body_b,
                points,
                friction,
                restitution,
            }
        } else {
            // Flip body order ; flip normals to maintain "from b to a" convention.
            let flipped_points = points
                .into_iter()
                .map(|p| ContactPoint {
                    position: p.position,
                    normal: -p.normal,
                    penetration: p.penetration,
                    accumulated_normal_impulse: p.accumulated_normal_impulse,
                    accumulated_tangent_impulse_1: p.accumulated_tangent_impulse_1,
                    accumulated_tangent_impulse_2: p.accumulated_tangent_impulse_2,
                })
                .collect();
            Self {
                body_a: body_b,
                body_b: body_a,
                points: flipped_points,
                friction,
                restitution,
            }
        }
    }

    /// Stable hash for replay-determinism : `(body_a, body_b)` pair.
    #[must_use]
    pub fn pair_key(&self) -> (BodyId, BodyId) {
        (self.body_a, self.body_b)
    }
}

/// Alias for "the points list of a contact". Used in narrow-phase output.
pub type ContactManifold = Vec<ContactPoint>;

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

    // ─── ContactPoint ───

    #[test]
    fn contact_point_new_normalizes() {
        let p = ContactPoint::new(Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), 0.1);
        assert!(approx_eq(p.normal.length(), 1.0));
    }

    #[test]
    fn contact_point_zero_initial_impulses() {
        let p = ContactPoint::new(Vec3::ZERO, Vec3::Y, 0.0);
        assert_eq!(p.accumulated_normal_impulse, 0.0);
        assert_eq!(p.accumulated_tangent_impulse_1, 0.0);
        assert_eq!(p.accumulated_tangent_impulse_2, 0.0);
    }

    #[test]
    fn tangent_basis_orthogonal_to_normal() {
        let p = ContactPoint::new(Vec3::ZERO, Vec3::Y, 0.0);
        let (t1, t2) = p.tangent_basis();
        assert!(approx_eq(p.normal.dot(t1), 0.0));
        assert!(approx_eq(p.normal.dot(t2), 0.0));
    }

    #[test]
    fn tangent_basis_orthogonal_to_each_other() {
        let p = ContactPoint::new(
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 1.0).normalize_or_zero(),
            0.0,
        );
        let (t1, t2) = p.tangent_basis();
        assert!(approx_eq(t1.dot(t2), 0.0));
    }

    #[test]
    fn tangent_basis_unit_length() {
        let p = ContactPoint::new(Vec3::ZERO, Vec3::Z, 0.0);
        let (t1, t2) = p.tangent_basis();
        assert!(approx_eq(t1.length(), 1.0));
        assert!(approx_eq(t2.length(), 1.0));
    }

    // ─── Contact ───

    #[test]
    fn contact_canonical_body_order() {
        let pts = vec![ContactPoint::new(Vec3::ZERO, Vec3::Y, 0.1)];
        let c = Contact::new(BodyId(5), BodyId(2), pts, 0.5, 0.0);
        assert_eq!(c.body_a, BodyId(2));
        assert_eq!(c.body_b, BodyId(5));
    }

    #[test]
    fn contact_normal_flips_when_body_order_flips() {
        let n = Vec3::Y;
        let pts = vec![ContactPoint::new(Vec3::ZERO, n, 0.1)];
        let c = Contact::new(BodyId(5), BodyId(2), pts, 0.5, 0.0);
        // After flip, normal points FROM body_b (5) TO body_a (2) ⇒ -Y
        assert!(vec3_approx(c.points[0].normal, -n));
    }

    #[test]
    fn contact_normal_preserved_when_already_canonical() {
        let n = Vec3::Y;
        let pts = vec![ContactPoint::new(Vec3::ZERO, n, 0.1)];
        let c = Contact::new(BodyId(2), BodyId(5), pts, 0.5, 0.0);
        assert_eq!(c.body_a, BodyId(2));
        assert_eq!(c.body_b, BodyId(5));
        assert!(vec3_approx(c.points[0].normal, n));
    }

    #[test]
    fn contact_pair_key() {
        let pts = vec![ContactPoint::new(Vec3::ZERO, Vec3::Y, 0.1)];
        let c = Contact::new(BodyId(2), BodyId(5), pts, 0.5, 0.0);
        assert_eq!(c.pair_key(), (BodyId(2), BodyId(5)));
    }

    #[test]
    fn contact_friction_restitution_stored() {
        let pts = vec![ContactPoint::new(Vec3::ZERO, Vec3::Y, 0.1)];
        let c = Contact::new(BodyId(0), BodyId(1), pts, 0.7, 0.3);
        assert!(approx_eq(c.friction, 0.7));
        assert!(approx_eq(c.restitution, 0.3));
    }
}
