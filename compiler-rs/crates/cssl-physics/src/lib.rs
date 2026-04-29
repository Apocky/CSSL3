//! § cssl-physics — CSSLv3 Substrate rigid-body physics simulation
//! ════════════════════════════════════════════════════════════════════════
//!
//! Authoritative specs : `specs/30_SUBSTRATE.csl § OMEGA-STEP` (PHASES § sim-substep),
//!                       PRIME_DIRECTIVE.md (consent-as-OS, no-weaponization,
//!                       deterministic-replay protections).
//!
//! § ROLE
//!   Rigid-body physics simulation for the CSSLv3 Substrate. Provides :
//!     - **BroadPhase** : BVH (bounding-volume-hierarchy) for non-clustered scenes
//!     - **NarrowPhase** : per-shape-pair contact-generation (sphere / box / capsule
//!       / convex-hull / plane)
//!     - **ConstraintSolver** : sequential-impulse PGS (Erin Catto-style) for
//!       contact + joint constraints, with warm-starting
//!     - **Integrator** : symplectic-Euler (energy-stable, deterministic)
//!     - **PhysicsWorld** : aggregates bodies + broadphase + solver + gravity ;
//!       implements `OmegaSystem` so it slots into H2's scheduler
//!
//! § SURFACE  (stage-0 stable)
//!   ```text
//!   pub use body::{RigidBody, BodyId, BodyHandle, BodyKind};
//!   pub use shape::{Shape, Aabb, BoundingSphere};
//!   pub use math::{Vec3, Mat3, Quat};
//!   pub use broadphase::{BroadPhase, BvhBroadPhase};
//!   pub use narrowphase::{NarrowPhase, contact_pair};
//!   pub use contact::{Contact, ContactPoint, ContactManifold};
//!   pub use joint::{Joint, JointKind, HingeJoint, BallSocketJoint, DistanceJoint};
//!   pub use solver::{ConstraintSolver, SolverConfig};
//!   pub use integrator::{integrate_symplectic, IntegratorConfig};
//!   pub use world::{PhysicsWorld, WorldConfig};
//!   pub use determinism::{flush_denormals_to_zero, fmadd_disabled};
//!   ```
//!
//! § DETERMINISM CONTRACT  ‼ load-bearing
//!   Same as `cssl-substrate-omega-step` : two `PhysicsWorld` instances seeded
//!   identically + ticked with the same fixed-dt sequence MUST produce
//!   bit-identical body states after N steps. Achieved via :
//!     - Fixed-dt integration (caller supplies dt ; we never read clocks)
//!     - No FMA (fused-multiply-add) — explicit `(a*b)+c` two-step ops
//!     - Denormal flush probe at `PhysicsWorld::new()`
//!     - No `thread_rng()`, no parallel iteration order dependency
//!     - Stable iteration : bodies + contacts indexed by `u64` ids ;
//!       the solver iterates in id-sorted order
//!     - Sequential-impulse PGS uses fixed iteration count (`SolverConfig::iterations`)
//!       not a residual-tolerance loop (which would diverge under float-noise)
//!
//! § PRIME-DIRECTIVE alignment
//!   - **No-weaponization (§1)** : Physics is a body-simulation kernel. It
//!     does NOT include : ballistics targeting solvers, projectile-trajectory
//!     optimizers tagged "weapon", or kinematic-control APIs that bind to a
//!     "weapon" sensitivity-token. Such uses are forbidden by effect-row
//!     composition rules at the omega-step layer (§ OMEGA-STEP § FORBIDDEN-
//!     COMPOSITIONS) ; this crate offers no escape hatch around those rules.
//!   - **Consent-OS (§0)** : `PhysicsWorld::register(scheduler)` flows through
//!     `caps_grant(omega_register)` like any other `OmegaSystem`. There is
//!     no privileged registration path.
//!   - **Substrate-sovereignty (§3)** : AI-collaborator-implemented physics
//!     (custom `Joint` impls, custom `Shape` impls in future) are first-class —
//!     the trait surfaces never test `is_human_authored`. They never can.
//!
//! § ARCHITECTURAL CHOICES
//!   - **BVH over SAP** : Sweep-and-Prune assumes scene-coherence (objects
//!     mostly stationary or moving slowly). For game scenes with fast-moving
//!     bodies (projectiles, characters, debris), BVH wins. We rebuild the
//!     BVH each frame for simplicity ; refit-instead-of-rebuild is a future
//!     optimization once profiling shows the rebuild cost matters.
//!   - **Discrete CCD only** : Continuous collision detection (TOI < 1)
//!     is DEFERRED per dispatch landmines. Fast-moving bodies will tunnel
//!     through thin geometry ; we accept this trade for stage-0.
//!   - **Sequential-impulse PGS** : Erin Catto's method ; iterates over
//!     constraints + applies impulses to satisfy each. Warm-starts with
//!     previous-frame impulse for stability. Deterministic with fixed iter-count.
//!   - **Symplectic-Euler over Verlet/RK4** : Symplectic-Euler is energy-stable
//!     (drift-free over long sims), deterministic, and matches Catto's
//!     constraint-solver expectations. RK4 is more accurate per-step but
//!     allocates intermediate state and breaks the
//!     `position += linvel*dt ; linvel += force/mass*dt` ordering the solver
//!     relies on.
//!   - **Sleeping bodies** : Bodies whose linear+angular velocity stays below
//!     a threshold for N consecutive frames are marked `BodyKind::Sleeping` ;
//!     they're skipped by the integrator + are inactive in the broadphase.
//!     Touching a sleeping body wakes it.
//!   - **Joint types covered** : Hinge (1-DOF rotation about an axis),
//!     BallSocket (3-DOF rotation about a point), Distance (rigid stick).
//!     Slider / 6-DOF / motor-driven joints are deferred.
//!
//! § ABI STABILITY
//!   The public surface above is stage-0 STABLE. Renaming any item is a
//!   major-version-bump per the T11-D76 ABI lock precedent.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
// PHYSICS-DETERMINISM ALLOWS  (load-bearing per § DETERMINISM CONTRACT)
//  - `float_cmp` : tests use exact equality where the math is exact (basis vectors,
//     identity rotations, etc.). Tolerance-based asserts use approx-eq helpers.
//  - `suboptimal_flops` : clippy suggests `mul_add` (FMA) for `a*b+c`. FMA is
//     EXPLICITLY FORBIDDEN here — it changes rounding behavior + breaks the
//     bit-equal-replay invariant. We override clippy's suggestion.
#![allow(clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]
// PHYSICS-MATH-IDIOMS  (stage-0)
//  - `many_single_char_names` : math idioms use x/y/z, w, a/b, p/q routinely.
//     Renaming to "x_coord" etc. would obscure the math.
//  - `similar_names` : ra/rb, va/vb, idx_a/idx_b, body_a/body_b are paired and
//     deliberate — physics-text convention.
//  - `manual_let_else` : let-else is a 1.65 idiom ; we keep the match form for
//     readability when the binding is paired (two values out of one expression).
//  - `too_many_arguments` : narrowphase + solver helpers thread state through
//     functions ; refactoring to bundles would obscure call-sites.
//  - `match_same_arms` : per-shape-pair narrow-phase has duplicate Some(_) /
//     None arms by design — splitting via OR-pat would lose pair-specific dispatch.
//  - `suspicious_arithmetic_impl` : rolling our own Mul/Sub on Mat3 ; idiomatic.
//  - `unused_self` : solver helper-fns on `&self` allow future stateful extensions.
//  - `option_if_let_else` / `if_let_else` : pattern-matching reads clearer here.
#![allow(clippy::many_single_char_names)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::suspicious_arithmetic_impl)]
#![allow(clippy::suspicious_op_assign_impl)]
#![allow(clippy::unused_self)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::needless_collect)]
#![allow(clippy::redundant_closure_for_method_calls)]
// Closest-points-on-segments uses `a_len_sq * b_len_sq - b*b` (Geometric formulation
// from Real-Time Collision Detection §5.1.9) ; clippy mis-identifies this as a
// transposition bug because variable `b` is named identically to the dot-product
// `b = da·db`. The code is correct.
#![allow(clippy::suspicious_operation_groupings)]

pub mod body;
pub mod broadphase;
pub mod contact;
pub mod determinism;
pub mod integrator;
pub mod joint;
pub mod math;
pub mod narrowphase;
pub mod shape;
pub mod solver;
pub mod world;

pub use body::{BodyHandle, BodyId, BodyKind, RigidBody};
pub use broadphase::{BroadPhase, BvhBroadPhase, BvhNode};
pub use contact::{Contact, ContactManifold, ContactPoint};
pub use determinism::{flush_denormals_to_zero, fmadd_disabled};
pub use integrator::{integrate_symplectic, IntegratorConfig};
pub use joint::{BallSocketJoint, DistanceJoint, HingeJoint, Joint, JointId, JointKind};
pub use math::{Mat3, Quat, Vec3};
pub use narrowphase::{contact_pair, NarrowPhase};
pub use shape::{Aabb, BoundingSphere, Shape};
pub use solver::{ConstraintSolver, SolverConfig};
pub use world::{PhysicsWorld, WorldConfig};

/// Crate version, exposes `CARGO_PKG_VERSION`. Mirrors the `STAGE0_SCAFFOLD`
/// pattern in sibling crates so workspace-wide tests can probe the marker.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME_DIRECTIVE attestation literal. Embedded so audit-walkers can
/// verify the build was assembled under the consent-as-OS axiom.
///
/// ≡ "There was no hurt nor harm in the making of this, to anyone /
///   anything / anybody."
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }
}
