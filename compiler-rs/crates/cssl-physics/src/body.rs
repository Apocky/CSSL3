//! `RigidBody` — the simulation primitive.
//!
//! § FIELDS
//!   - `mass` : in kg ; `0.0` means static (immovable).
//!   - `inv_mass` : `1/mass` if dynamic, `0.0` if static. Cached for solver hot-path.
//!   - `inertia_local` : 3x3 inertia tensor in body-local space.
//!   - `inv_inertia_local` : `inertia_local^(-1)`. Cached.
//!   - `position` : world-space center-of-mass.
//!   - `orientation` : unit quaternion world-space.
//!   - `linear_velocity` : world-space m/s.
//!   - `angular_velocity` : world-space rad/s.
//!   - `force_accum` : world-space force accumulator (cleared each step).
//!   - `torque_accum` : world-space torque accumulator (cleared each step).
//!   - `shape` : geometry attached.
//!   - `restitution` : bounce coefficient (0 = inelastic, 1 = perfectly elastic).
//!   - `friction` : coulomb friction coefficient (0 = frictionless, 1 = sticky).
//!   - `kind` : Dynamic | Static | Kinematic | Sleeping.
//!   - `linear_damping` / `angular_damping` : viscous damping factors (per-tick multiplier).
//!   - `sleep_timer` : frames-of-low-velocity counter for sleep-detection.
//!
//! § BODY KINDS
//!   - **Dynamic** : full-physics. Force/torque integrated, contacts resolved,
//!     joints applied.
//!   - **Static** : never moves (mass = ∞ ⇒ inv_mass = 0). Contacts treat
//!     it as immovable. Static bodies don't appear in the integration loop.
//!   - **Kinematic** : controlled externally — velocity is set by the user
//!     each frame, not derived from forces. Contacts push other bodies but
//!     don't push this one. Use for moving platforms / animated geometry.
//!   - **Sleeping** : a Dynamic body whose linvel + angvel stayed below
//!     thresholds for `WorldConfig::sleep_frames` consecutive ticks. Skipped
//!     by integrator. Wakes when an external impulse touches it.
//!
//! § DETERMINISM
//!   `BodyId` is a stable u64 ; iteration order is by sorted id. The
//!   `force_accum` + `torque_accum` are cleared at integrator-end ;
//!   contributions from solver / user are summed in well-defined order.

use crate::math::{Mat3, Quat, Vec3};
use crate::shape::Shape;

// ────────────────────────────────────────────────────────────────────────
// § BodyId — stable u64 handle
// ────────────────────────────────────────────────────────────────────────

/// Stable identifier for a registered body. Issued in monotone-increasing
/// order by `PhysicsWorld::insert()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyId(pub u64);

/// Convenience type for "the index of a body in `world.bodies`". Use
/// `BodyHandle::new(BodyId(0))` to refer to the first inserted body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyHandle(pub BodyId);

impl BodyHandle {
    #[must_use]
    pub const fn new(id: BodyId) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn id(self) -> BodyId {
        self.0
    }
}

// ────────────────────────────────────────────────────────────────────────
// § BodyKind — body simulation discipline
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyKind {
    /// Full-physics body : forces integrated, contacts resolved, joints applied.
    Dynamic,
    /// Static — never moves. Mass = ∞.
    Static,
    /// Kinematic — velocity set externally ; contacts push others but not this.
    Kinematic,
    /// Dynamic body that has gone to sleep due to low velocity. Wakes on contact.
    Sleeping,
}

impl BodyKind {
    /// Whether the body participates in force integration this tick.
    #[must_use]
    pub fn integrates(self) -> bool {
        matches!(self, BodyKind::Dynamic)
    }

    /// Whether the body moves (Dynamic, Kinematic, or freshly-woken).
    #[must_use]
    pub fn is_movable(self) -> bool {
        !matches!(self, BodyKind::Static)
    }

    /// Whether the body is asleep.
    #[must_use]
    pub fn is_sleeping(self) -> bool {
        matches!(self, BodyKind::Sleeping)
    }
}

// ────────────────────────────────────────────────────────────────────────
// § RigidBody
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RigidBody {
    /// Linear mass in kg. `f64::INFINITY` for static bodies.
    pub mass: f64,
    /// Inverse mass `1/mass`. `0.0` for static bodies.
    pub inv_mass: f64,
    /// Body-local inertia tensor.
    pub inertia_local: Mat3,
    /// Inverse body-local inertia. `Mat3::ZERO` for static bodies.
    pub inv_inertia_local: Mat3,
    /// World-space center-of-mass position.
    pub position: Vec3,
    /// World-space orientation as unit quaternion.
    pub orientation: Quat,
    /// World-space linear velocity (m/s).
    pub linear_velocity: Vec3,
    /// World-space angular velocity (rad/s).
    pub angular_velocity: Vec3,
    /// World-space accumulated force this step.
    pub force_accum: Vec3,
    /// World-space accumulated torque this step.
    pub torque_accum: Vec3,
    /// Geometry attached to the body.
    pub shape: Shape,
    /// Bounce coefficient `[0, 1]`.
    pub restitution: f64,
    /// Coulomb friction coefficient `[0, 1+]`.
    pub friction: f64,
    /// Body simulation discipline.
    pub kind: BodyKind,
    /// Linear velocity damping per-tick multiplier (0.99 = light damping).
    pub linear_damping: f64,
    /// Angular velocity damping per-tick multiplier.
    pub angular_damping: f64,
    /// Frames of below-threshold velocity ; reset on wake.
    pub sleep_timer: u32,
}

impl RigidBody {
    /// Construct a new dynamic body with the given mass + shape, at origin.
    /// Inertia tensor is derived from the shape.
    #[must_use]
    pub fn new_dynamic(mass: f64, shape: Shape) -> Self {
        let inertia = shape.local_inertia_tensor(mass);
        let inv_inertia = inertia.try_inverse().unwrap_or(Mat3::ZERO);
        Self {
            mass,
            inv_mass: if mass > 0.0 { 1.0 / mass } else { 0.0 },
            inertia_local: inertia,
            inv_inertia_local: inv_inertia,
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            force_accum: Vec3::ZERO,
            torque_accum: Vec3::ZERO,
            shape,
            restitution: 0.0,
            friction: 0.5,
            kind: BodyKind::Dynamic,
            linear_damping: 1.0,
            angular_damping: 1.0,
            sleep_timer: 0,
        }
    }

    /// Construct a static body (mass = ∞, inv_mass = 0).
    #[must_use]
    pub fn new_static(shape: Shape) -> Self {
        Self {
            mass: f64::INFINITY,
            inv_mass: 0.0,
            inertia_local: Mat3::ZERO,
            inv_inertia_local: Mat3::ZERO,
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            force_accum: Vec3::ZERO,
            torque_accum: Vec3::ZERO,
            shape,
            restitution: 0.0,
            friction: 0.5,
            kind: BodyKind::Static,
            linear_damping: 1.0,
            angular_damping: 1.0,
            sleep_timer: 0,
        }
    }

    /// Construct a kinematic body — velocity-driven, contacts don't push.
    #[must_use]
    pub fn new_kinematic(shape: Shape) -> Self {
        let mut b = Self::new_static(shape);
        b.kind = BodyKind::Kinematic;
        b
    }

    /// Builder : set position.
    #[must_use]
    pub fn with_position(mut self, p: Vec3) -> Self {
        self.position = p;
        self
    }

    /// Builder : set orientation.
    #[must_use]
    pub fn with_orientation(mut self, q: Quat) -> Self {
        self.orientation = q.normalize();
        self
    }

    /// Builder : set linear velocity.
    #[must_use]
    pub fn with_linear_velocity(mut self, v: Vec3) -> Self {
        self.linear_velocity = v;
        self
    }

    /// Builder : set angular velocity.
    #[must_use]
    pub fn with_angular_velocity(mut self, w: Vec3) -> Self {
        self.angular_velocity = w;
        self
    }

    /// Builder : set restitution.
    #[must_use]
    pub fn with_restitution(mut self, r: f64) -> Self {
        self.restitution = r.clamp(0.0, 1.0);
        self
    }

    /// Builder : set friction.
    #[must_use]
    pub fn with_friction(mut self, f: f64) -> Self {
        self.friction = f.max(0.0);
        self
    }

    /// Builder : set damping.
    #[must_use]
    pub fn with_damping(mut self, linear: f64, angular: f64) -> Self {
        self.linear_damping = linear.clamp(0.0, 1.0);
        self.angular_damping = angular.clamp(0.0, 1.0);
        self
    }

    /// Apply a world-space force at the center-of-mass for this tick.
    pub fn apply_force(&mut self, force: Vec3) {
        if self.kind == BodyKind::Static {
            return;
        }
        self.force_accum += force;
        self.wake();
    }

    /// Apply a world-space torque for this tick.
    pub fn apply_torque(&mut self, torque: Vec3) {
        if self.kind == BodyKind::Static {
            return;
        }
        self.torque_accum += torque;
        self.wake();
    }

    /// Apply a force at a world-space point. Computes both linear-force and
    /// torque contribution.
    pub fn apply_force_at_point(&mut self, force: Vec3, world_point: Vec3) {
        if self.kind == BodyKind::Static {
            return;
        }
        let r = world_point - self.position;
        self.force_accum += force;
        self.torque_accum += r.cross(force);
        self.wake();
    }

    /// Apply an instantaneous world-space impulse at the center-of-mass.
    pub fn apply_linear_impulse(&mut self, impulse: Vec3) {
        if self.kind == BodyKind::Static {
            return;
        }
        self.linear_velocity += impulse * self.inv_mass;
        self.wake();
    }

    /// Apply an instantaneous world-space angular impulse.
    pub fn apply_angular_impulse(&mut self, impulse: Vec3) {
        if self.kind == BodyKind::Static {
            return;
        }
        let inv_world = self.inv_inertia_world();
        self.angular_velocity += inv_world.mul_vec3(impulse);
        self.wake();
    }

    /// World-space inverse-inertia tensor : `R · I_local^-1 · R^T`.
    #[must_use]
    pub fn inv_inertia_world(&self) -> Mat3 {
        let r = self.orientation.to_mat3();
        r.mul_mat3(self.inv_inertia_local).mul_mat3(r.transpose())
    }

    /// Wake the body if it was asleep.
    pub fn wake(&mut self) {
        if self.kind == BodyKind::Sleeping {
            self.kind = BodyKind::Dynamic;
        }
        self.sleep_timer = 0;
    }

    /// Velocity at a world-space point : `v + ω × (p - x)`.
    #[must_use]
    pub fn velocity_at_point(&self, world_point: Vec3) -> Vec3 {
        self.linear_velocity + self.angular_velocity.cross(world_point - self.position)
    }

    /// Whether this body is fully static (cannot move from any cause).
    #[must_use]
    pub fn is_static(&self) -> bool {
        self.kind == BodyKind::Static
    }

    /// Clear force + torque accumulators. Called by integrator at end-of-step.
    pub fn clear_forces(&mut self) {
        self.force_accum = Vec3::ZERO;
        self.torque_accum = Vec3::ZERO;
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

    // ─── BodyKind ───

    #[test]
    fn dynamic_kind_integrates_and_movable() {
        assert!(BodyKind::Dynamic.integrates());
        assert!(BodyKind::Dynamic.is_movable());
    }

    #[test]
    fn static_kind_does_not_integrate_or_move() {
        assert!(!BodyKind::Static.integrates());
        assert!(!BodyKind::Static.is_movable());
    }

    #[test]
    fn kinematic_kind_movable_but_no_integrate() {
        assert!(!BodyKind::Kinematic.integrates());
        assert!(BodyKind::Kinematic.is_movable());
    }

    #[test]
    fn sleeping_kind_does_not_integrate() {
        assert!(!BodyKind::Sleeping.integrates());
        assert!(BodyKind::Sleeping.is_sleeping());
    }

    // ─── RigidBody constructors ───

    #[test]
    fn new_dynamic_sphere_inv_mass_correct() {
        let b = RigidBody::new_dynamic(2.0, Shape::Sphere { radius: 1.0 });
        assert!(approx_eq(b.inv_mass, 0.5));
        assert_eq!(b.kind, BodyKind::Dynamic);
    }

    #[test]
    fn new_dynamic_inertia_invertible() {
        let b = RigidBody::new_dynamic(5.0, Shape::Sphere { radius: 1.0 });
        // Sphere I = 0.4 * 5 * 1 = 2 ; inverse = 0.5
        assert!(approx_eq(b.inv_inertia_local.r0.x, 0.5));
        assert!(approx_eq(b.inv_inertia_local.r1.y, 0.5));
        assert!(approx_eq(b.inv_inertia_local.r2.z, 0.5));
    }

    #[test]
    fn new_static_inv_mass_zero() {
        let b = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        assert_eq!(b.inv_mass, 0.0);
        assert_eq!(b.kind, BodyKind::Static);
    }

    #[test]
    fn new_kinematic_kind_set() {
        let b = RigidBody::new_kinematic(Shape::Sphere { radius: 1.0 });
        assert_eq!(b.kind, BodyKind::Kinematic);
        assert_eq!(b.inv_mass, 0.0);
    }

    // ─── Builder ───

    #[test]
    fn with_position_sets_position() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(1.0, 2.0, 3.0));
        assert!(vec3_approx(b.position, Vec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn with_orientation_normalizes() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_orientation(Quat::new(2.0, 0.0, 0.0, 0.0));
        assert!(approx_eq(b.orientation.length(), 1.0));
    }

    #[test]
    fn with_linear_velocity_sets() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_linear_velocity(Vec3::new(5.0, 0.0, 0.0));
        assert!(vec3_approx(b.linear_velocity, Vec3::new(5.0, 0.0, 0.0)));
    }

    #[test]
    fn with_angular_velocity_sets() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_angular_velocity(Vec3::new(0.0, 1.0, 0.0));
        assert!(vec3_approx(b.angular_velocity, Vec3::new(0.0, 1.0, 0.0)));
    }

    #[test]
    fn with_restitution_clamps_to_one() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }).with_restitution(2.0);
        assert_eq!(b.restitution, 1.0);
    }

    #[test]
    fn with_restitution_clamps_negative() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }).with_restitution(-0.5);
        assert_eq!(b.restitution, 0.0);
    }

    #[test]
    fn with_friction_clamps_negative() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }).with_friction(-1.0);
        assert_eq!(b.friction, 0.0);
    }

    #[test]
    fn with_damping_clamps_to_unit() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }).with_damping(2.0, -0.5);
        assert_eq!(b.linear_damping, 1.0);
        assert_eq!(b.angular_damping, 0.0);
    }

    // ─── Force application ───

    #[test]
    fn apply_force_accumulates() {
        let mut b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        b.apply_force(Vec3::new(1.0, 0.0, 0.0));
        b.apply_force(Vec3::new(2.0, 0.0, 0.0));
        assert!(vec3_approx(b.force_accum, Vec3::new(3.0, 0.0, 0.0)));
    }

    #[test]
    fn apply_force_static_no_op() {
        let mut b = RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        });
        b.apply_force(Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(b.force_accum, Vec3::ZERO);
    }

    #[test]
    fn apply_torque_accumulates() {
        let mut b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        b.apply_torque(Vec3::Y);
        b.apply_torque(Vec3::Y);
        assert!(vec3_approx(b.torque_accum, Vec3::new(0.0, 2.0, 0.0)));
    }

    #[test]
    fn apply_force_at_point_creates_torque() {
        let mut b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        // Force in +Y at point on +X-side ⇒ torque in +Z
        b.apply_force_at_point(Vec3::Y, Vec3::X);
        assert!(vec3_approx(b.force_accum, Vec3::Y));
        assert!(vec3_approx(b.torque_accum, Vec3::Z));
    }

    #[test]
    fn apply_linear_impulse_changes_velocity() {
        let mut b = RigidBody::new_dynamic(2.0, Shape::Sphere { radius: 1.0 });
        b.apply_linear_impulse(Vec3::new(4.0, 0.0, 0.0));
        // Δv = J / m = 4 / 2 = 2
        assert!(vec3_approx(b.linear_velocity, Vec3::new(2.0, 0.0, 0.0)));
    }

    #[test]
    fn apply_angular_impulse_changes_angular_velocity() {
        // Sphere I = 0.4 * 1 * 1 = 0.4 ; angular impulse 0.4 about Y → Δω.y = 1
        let mut b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        b.apply_angular_impulse(Vec3::new(0.0, 0.4, 0.0));
        assert!(vec3_approx(b.angular_velocity, Vec3::new(0.0, 1.0, 0.0)));
    }

    // ─── Velocity at point ───

    #[test]
    fn velocity_at_center_equals_linear_velocity() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::ZERO)
            .with_linear_velocity(Vec3::X)
            .with_angular_velocity(Vec3::Y);
        // At center : v = linear_v + angular_v × 0 = linear_v
        assert!(vec3_approx(b.velocity_at_point(Vec3::ZERO), Vec3::X));
    }

    #[test]
    fn velocity_at_offset_includes_angular_contribution() {
        let b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::ZERO)
            .with_linear_velocity(Vec3::ZERO)
            .with_angular_velocity(Vec3::Y); // 1 rad/s about Y
                                             // At point +Z, ω×r = Y×Z = X, so velocity = X
        let v = b.velocity_at_point(Vec3::Z);
        assert!(vec3_approx(v, Vec3::X));
    }

    // ─── inv_inertia_world ───

    #[test]
    fn inv_inertia_world_identity_orientation_equals_local() {
        let b = RigidBody::new_dynamic(5.0, Shape::Sphere { radius: 1.0 });
        let inv = b.inv_inertia_world();
        // For sphere, world == local (rotation-invariant inertia)
        assert!(approx_eq(inv.r0.x, b.inv_inertia_local.r0.x));
    }

    // ─── Wake / sleep ───

    #[test]
    fn wake_sleeping_body_to_dynamic() {
        let mut b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        b.kind = BodyKind::Sleeping;
        b.sleep_timer = 60;
        b.wake();
        assert_eq!(b.kind, BodyKind::Dynamic);
        assert_eq!(b.sleep_timer, 0);
    }

    #[test]
    fn apply_force_wakes_sleeping_body() {
        let mut b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        b.kind = BodyKind::Sleeping;
        b.apply_force(Vec3::X);
        assert_eq!(b.kind, BodyKind::Dynamic);
    }

    // ─── clear_forces ───

    #[test]
    fn clear_forces_resets_accumulators() {
        let mut b = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        b.apply_force(Vec3::X);
        b.apply_torque(Vec3::Y);
        b.clear_forces();
        assert_eq!(b.force_accum, Vec3::ZERO);
        assert_eq!(b.torque_accum, Vec3::ZERO);
    }

    // ─── BodyId / BodyHandle ───

    #[test]
    fn body_id_ord() {
        let a = BodyId(1);
        let b = BodyId(2);
        assert!(a < b);
    }

    #[test]
    fn body_handle_id_round_trip() {
        let h = BodyHandle::new(BodyId(7));
        assert_eq!(h.id(), BodyId(7));
    }
}
