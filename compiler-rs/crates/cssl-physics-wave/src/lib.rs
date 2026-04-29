//! § cssl-physics-wave — wave-substrate physics : SDF + Morton-spatial-hash + XPBD + KAN-body-plan + ψ-coupling
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Replaces `cssl-physics` (legacy rigid-body : BVH + sequential-impulse
//!   PGS) with the wave-substrate physics demanded by the audit (Omniverse
//!   `07_AESTHETIC/01_SDF_NATIVE_RENDER` § I "SDF IS the world ⊗ collision +
//!   render + audio + physics + AI ⊗ ALL-query-the-same-SDF"). The legacy
//!   crate matched the spec at ~10% ; this crate is the `T11-D117`
//!   reconception that closes the gap.
//!
//!   The five canonical pillars per dispatch :
//!
//!   1. **`SdfCollider`** — query distance-field at point ; gradient yields
//!      contact-normal ; analytic continuous collision detection (CCD) via
//!      gradient-descent toward the iso-surface. SDF is the geometry. There
//!      is no triangle-mesh fall-back.
//!
//!   2. **`MortonSpatialHash`** — O(1) broadphase backed by
//!      `cssl-substrate-omega-field`'s `SparseMortonGrid`. Warp-vote-style
//!      "commit-once" insertion (atomic-light) for GPU dispatch ; on CPU we
//!      fall back to a deterministic single-thread insert that produces the
//!      same final state. Target : 1M+ entities @ 60Hz broadphase.
//!
//!   3. **`XpbdSolver`** — Extended Position-Based Dynamics. Constraint-
//!      graph coloring per warp + Jacobi-block solver + 4-iteration constraint-
//!      satisfaction. Replaces sequential-impulse PGS with the position-
//!      based formulation that is the modern physics-engine canon.
//!
//!   4. **`BodyPlanPhysics`** — KAN-derived skeleton + joints from creature
//!      genome. Reads `cssl-substrate-kan::Pattern` + the
//!      `KanMaterialKind::CreatureMorphology` variant ; emits a
//!      `Skeleton { bones, joints }` graph that the XpbdSolver consumes as
//!      distance-constraints + hinge-constraints.
//!
//!   5. **`WaveImpactCoupler`** — at every contact event, compute the impact
//!      energy + spectrum + emit a ψ-field excitation (per Omniverse
//!      `04_OMEGA_FIELD/04_WAVE_UNITY.csl` § IV.3 "every-impact-sounds-like-
//!      its-physics"). The coupler is the bridge between the constraint
//!      solver's discrete impulse events and the continuous wave-field. This
//!      integrates with the D114 wave-solver via the `WaveExcitation` event-
//!      stream emitted by `physics_step`.
//!
//! § OMEGA-STEP INTEGRATION (Phase-2 PROPAGATE)
//!   `physics_step(omega_field, dt) -> StepReport` slots into the canonical
//!   `omega_step` pipeline AFTER `wave_solver_step` and BEFORE the next
//!   `radiance_cascade_step`. The pipeline order is :
//!
//!   ```text
//!   Phase-2 PROPAGATE :
//!     2a. wave_solver_step(omega_field, dt)         // D114 (ψ-PDE substrate)
//!     2b. physics_step(omega_field, dt)             // D117 (this crate)
//!     2c. radiance_cascade_step(omega_field, dt)    // D118
//!     2d. ...
//!   ```
//!
//!   `physics_step` reads ψ-field amplitude at body-cells (for damping +
//!   buoyancy + magic-effects) and writes back contact-impact ψ-excitations
//!   for the next 2a substep to propagate.
//!
//! § BACKWARD COMPATIBILITY
//!   The legacy `cssl-physics` public API (`RigidBody` / `BroadPhase` /
//!   `NarrowPhase` / `ConstraintSolver` / `PhysicsWorld`) is preserved
//!   behind the `cssl-physics-legacy` feature-flag in `legacy::`. This
//!   gives downstream consumers (`loa-game`, integration tests) a
//!   deprecation window during which they can migrate to the wave-physics
//!   surface incrementally. With the flag off (default), only the wave-
//!   physics surface is exposed.
//!
//! § DETERMINISM CONTRACT
//!   Same as the legacy crate : two `WavePhysicsWorld` instances seeded
//!   identically + ticked with the same fixed-`dt` sequence MUST produce
//!   bit-identical body states after N steps. Achieved via :
//!   - Fixed-dt integration (no clock reads).
//!   - No FMA — explicit `(a*b)+c` two-step ops.
//!   - Denormal flush probe at world-construction.
//!   - Stable iteration order (bodies + contacts indexed by `BodyId`).
//!   - Constraint-graph color-bucket order is sorted by color-id ; within
//!     a color, constraints sort by `(body_a_id, body_b_id)`.
//!   - Morton-hash insertion order is sorted by `MortonKey` for the
//!     CPU-deterministic path ; the GPU warp-vote path produces the same
//!     final hash-state by definition (the warp-vote selects exactly one
//!     thread per slot per cycle).
//!
//! § PRIME-DIRECTIVE alignment
//!   - **No-weaponization** : the crate exposes a body-simulation kernel +
//!     a wave-coupling kernel. It does NOT include : ballistics targeting
//!     solvers, projectile-trajectory optimizers tagged "weapon", kinematic-
//!     control APIs that bind to a "weapon" sensitivity-token, or wave-
//!     packet-injection APIs that bypass the Σ-mask consent gate. The
//!     `WaveImpactCoupler` writes ψ-excitations through the `OmegaField`
//!     surface which enforces Σ-check on every cell-write.
//!   - **Consent-OS** : the entry-point `physics_step(omega_field, dt)`
//!     mutates `omega_field` only via the `OmegaField::set_cell` /
//!     `OmegaField::lambda_overlay_mut` / etc. surfaces, which all enforce
//!     Σ-check. There is no privileged write-path.
//!   - **Substrate-sovereignty** : `BodyPlanPhysics` admits AI-collaborator-
//!     authored creature genomes (any `KanGenomeWeights` instance). The
//!     kan-material-kind discriminator never tests `is_human_authored`.
//!
//! § CITATION
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md` § I, § III, § IV
//!   - `Omniverse/06_PROCEDURAL/02_CREATURES_FROM_GENOME.csl.md` § III, § IV
//!   - `Omniverse/06_PROCEDURAL/06_HARD_SURFACE_PRIMITIVES.csl` § II, § VIII
//!   - `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl` § IV.3, § XIII
//!   - `PRIME_DIRECTIVE.md` § I, § II, § XI

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::similar_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::single_match_else)]
#![allow(clippy::iter_without_into_iter)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::unused_self)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::needless_continue)]
#![allow(dead_code)]
// § Spec-stability gates : `assert!(constant > 0)` style checks are
//   compile-time-true under correct config but exist as guard-rails for
//   future spec drift. Clippy flags them as "optimized out" — that's
//   exactly what we want (zero runtime cost, but the line is documentation
//   the constant was checked against the spec at this version).
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::assign_op_pattern)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::let_and_return)]
// § hypot() is more precise but the SDF evals are deterministic +
//   replay-stable in their current `(a*a + b*b).sqrt()` form. Switching
//   to `hypot()` would change the floating-point trace for existing
//   SDF queries — a wire-format break for replay. Hold for a future
//   slice that decides intentionally.
#![allow(clippy::imprecise_flops)]
// § GridIndex::upsert returns Result<u32, GridError> to keep parity with
//   the public `MortonSpatialHash::insert_body` Result-typing ; the
//   internal implementation never errors today but the wrapper is a
//   future-proofing hook for the saturation-probe propagation. Keep.
#![allow(clippy::unnecessary_wraps)]

pub mod attestation;
pub mod body_plan;
pub mod determinism;
pub mod legacy;
pub mod morton_hash;
pub mod omega_step;
pub mod sdf;
pub mod wave_coupler;
pub mod world;
pub mod xpbd;

// § Public re-exports (stage-0 stable surface).
pub use attestation::ATTESTATION;
pub use body_plan::{BodyPlanError, BodyPlanPhysics, Bone, BoneId, BodyJoint, JointKind, Skeleton};
pub use determinism::{
    flush_denormals_to_zero, fmadd_disabled, DeterminismConfig, DET_RNG_SEED_DEFAULT,
};
pub use morton_hash::{
    BroadphaseError, BroadphasePair, MortonSpatialHash, SpatialHashConfig, WarpVoteResult,
};
pub use omega_step::{physics_step, PhysicsStepReport, StepError};
pub use sdf::{
    sdf_box, sdf_capsule, sdf_cylinder, sdf_plane, sdf_sphere, sdf_torus, SdfCollider, SdfHit,
    SdfPrimitive, SdfQueryError, SdfShape, IsoSurfaceCcd,
};
pub use wave_coupler::{
    ContactSpectrum, WaveCouplingError, WaveExcitation, WaveImpactCoupler, IMPACT_ENERGY_FLOOR,
};
pub use world::{BodyHandle, BodyId, BodyKind, RigidBody, WavePhysicsWorld, WorldConfig};
pub use xpbd::{
    ColorId, Constraint, ConstraintFailure, ConstraintKind, GraphColoring, JacobiBlock, XpbdConfig,
    XpbdSolver, XPBD_DEFAULT_ITERATIONS,
};

// ───────────────────────────────────────────────────────────────────────
// § Crate-version sentinel.
// ───────────────────────────────────────────────────────────────────────

/// § Crate-version stamp ; recorded in audit + telemetry.
pub const CSSL_PHYSICS_WAVE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// § Crate-name stamp.
pub const CSSL_PHYSICS_WAVE_CRATE: &str = "cssl-physics-wave";

/// § Stage-0 scaffold marker. Mirrors the `STAGE0_SCAFFOLD` pattern in
///   sibling crates so workspace-wide invariant-tests can probe the marker.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// § Maximum entity-count target for the broadphase. Per Omniverse density
///   budget the engine should sustain 1M+ broadphase entities at 60 Hz on
///   M7-class hardware. The Morton-hash makes this O(1) on the dominant
///   path ; the upper-bound is the SparseMortonGrid's 21-bit-per-axis
///   capacity (2M entities per axis = 8M unique grid cells).
pub const MAX_BROADPHASE_ENTITIES: usize = 8_388_608; // 2^23

#[cfg(test)]
mod crate_invariants {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CSSL_PHYSICS_WAVE_CRATE, "cssl-physics-wave");
    }

    #[test]
    fn version_is_nonempty() {
        assert!(!CSSL_PHYSICS_WAVE_VERSION.is_empty());
    }

    #[test]
    fn stage0_scaffold_is_nonempty() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn max_broadphase_entities_at_least_1m() {
        assert!(MAX_BROADPHASE_ENTITIES >= 1_000_000);
    }

    #[test]
    fn attestation_is_present_and_well_formed() {
        assert!(ATTESTATION.contains("PRIME_DIRECTIVE"));
        assert!(ATTESTATION.contains("T11-D117"));
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }
}
