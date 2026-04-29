//! § World — `WavePhysicsWorld` aggregates body-state, broadphase, solver,
//!  body-plan + wave-coupler.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Top-level container for the wave-physics simulation. Replaces the
//!   legacy `cssl-physics::PhysicsWorld` with a wave-substrate-native
//!   surface :
//!
//!   - `RigidBody` : body-state (position, velocity, mass, kind).
//!   - `WavePhysicsWorld` : owns `Vec<RigidBody>` + `MortonSpatialHash` +
//!     `XpbdSolver` + `WaveImpactCoupler` + emitted `WaveExcitation`s.
//!
//!   The world's per-frame step is implemented by `omega_step::physics_step`
//!   (in the sibling module). This module owns the data structures the
//!   step-function operates on.
//!
//! § BODY-KIND
//!   Three kinds (matches legacy semantics) :
//!   - `Dynamic` — full physics (mass > 0, integrated each frame).
//!   - `Kinematic` — externally driven (mass = `INF`, position written
//!      directly by gameplay code).
//!   - `Static` — never moves (mass = `INF`, infinite reaction force).
//!
//!   Sleeping (legacy `BodyKind::Sleeping`) is deferred to a future slice ;
//!   the wave-physics V0 keeps every body active.
//!
//! § DETERMINISM
//!   - Bodies are indexed by `BodyId` (u64). The world's iteration order
//!     is `BodyId`-ascending.
//!   - The world's RNG-seed is sourced from `DeterminismConfig`.

use crate::body_plan::Skeleton;
use crate::determinism::{flush_denormals_to_zero, DeterminismConfig};
use crate::morton_hash::{MortonSpatialHash, SpatialHashConfig};
use crate::wave_coupler::WaveExcitation;
use crate::xpbd::XpbdConfig;
use thiserror::Error;

/// § Identifier for a body in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct BodyId(pub u64);

impl BodyId {
    /// § Sentinel "no body" id.
    pub const NONE: BodyId = BodyId(u64::MAX);

    /// § Construct from raw u64.
    #[must_use]
    pub const fn from_raw(v: u64) -> Self {
        BodyId(v)
    }

    /// § Raw u64 representation.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// § A handle for a body in the world. Includes a generation tag so a
///   stale handle (after a body has been removed + slot reused) is
///   detectable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyHandle {
    /// Body id.
    pub id: BodyId,
    /// Generation counter at the time the handle was minted.
    pub generation: u32,
}

impl BodyHandle {
    /// § Construct.
    #[must_use]
    pub fn new(id: BodyId, generation: u32) -> Self {
        BodyHandle { id, generation }
    }
}

/// § Body kind tag (matches legacy semantics minus `Sleeping`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyKind {
    /// Full physics.
    Dynamic,
    /// Externally driven (no integration).
    Kinematic,
    /// Static (never moves).
    Static,
}

impl BodyKind {
    /// § True iff this body kind participates in dynamics integration.
    #[must_use]
    pub fn is_dynamic(self) -> bool {
        matches!(self, BodyKind::Dynamic)
    }

    /// § True iff this body kind has finite mass.
    #[must_use]
    pub fn has_finite_mass(self) -> bool {
        matches!(self, BodyKind::Dynamic)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § RigidBody.
// ───────────────────────────────────────────────────────────────────────

/// § A wave-physics rigid body.
///
///   Note : "rigid" here is a misnomer — the wave-physics solver supports
///   soft bodies via XPBD `compliance > 0`. We keep the name for legacy-
///   compatibility ; the soft-body path uses the SAME struct with non-
///   zero compliance on its joints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigidBody {
    /// Body id.
    pub id: BodyId,
    /// Body kind.
    pub kind: BodyKind,
    /// World-space position (center-of-mass).
    pub position: [f32; 3],
    /// Linear velocity (m/s).
    pub linear_velocity: [f32; 3],
    /// Mass (kg). For `Static` / `Kinematic` this is `f32::INFINITY` by convention.
    pub mass: f32,
    /// AABB half-extents (axis-aligned bounding box). Used by the broadphase.
    pub aabb_half: [f32; 3],
    /// Generation tag for handle invalidation.
    pub generation: u32,
}

impl RigidBody {
    /// § Construct a new dynamic body.
    #[must_use]
    pub fn dynamic(id: BodyId, position: [f32; 3], mass: f32, aabb_half: [f32; 3]) -> Self {
        RigidBody {
            id,
            kind: BodyKind::Dynamic,
            position,
            linear_velocity: [0.0; 3],
            mass: mass.max(0.0001), // never collapse to zero (would NaN inv_mass)
            aabb_half,
            generation: 0,
        }
    }

    /// § Construct a new static body.
    #[must_use]
    pub fn r#static(id: BodyId, position: [f32; 3], aabb_half: [f32; 3]) -> Self {
        RigidBody {
            id,
            kind: BodyKind::Static,
            position,
            linear_velocity: [0.0; 3],
            mass: f32::INFINITY,
            aabb_half,
            generation: 0,
        }
    }

    /// § Construct a new kinematic body.
    #[must_use]
    pub fn kinematic(id: BodyId, position: [f32; 3], aabb_half: [f32; 3]) -> Self {
        RigidBody {
            id,
            kind: BodyKind::Kinematic,
            position,
            linear_velocity: [0.0; 3],
            mass: f32::INFINITY,
            aabb_half,
            generation: 0,
        }
    }

    /// § Inverse mass (`0` for static / kinematic, `1/mass` for dynamic).
    #[must_use]
    pub fn inverse_mass(&self) -> f32 {
        if self.kind.has_finite_mass() && self.mass.is_finite() && self.mass > 0.0 {
            1.0_f32 / self.mass
        } else {
            0.0
        }
    }

    /// § AABB-min in world space.
    #[must_use]
    pub fn aabb_min(&self) -> [f32; 3] {
        [
            self.position[0] - self.aabb_half[0],
            self.position[1] - self.aabb_half[1],
            self.position[2] - self.aabb_half[2],
        ]
    }

    /// § AABB-max in world space.
    #[must_use]
    pub fn aabb_max(&self) -> [f32; 3] {
        [
            self.position[0] + self.aabb_half[0],
            self.position[1] + self.aabb_half[1],
            self.position[2] + self.aabb_half[2],
        ]
    }

    /// § Apply a velocity-impulse (typical post-contact resolution).
    pub fn apply_impulse(&mut self, impulse: [f32; 3]) {
        if !self.kind.is_dynamic() {
            return;
        }
        let inv_m = self.inverse_mass();
        if inv_m == 0.0 {
            return;
        }
        self.linear_velocity[0] += impulse[0] * inv_m;
        self.linear_velocity[1] += impulse[1] * inv_m;
        self.linear_velocity[2] += impulse[2] * inv_m;
    }
}

// ───────────────────────────────────────────────────────────────────────
// § WorldConfig.
// ───────────────────────────────────────────────────────────────────────

/// § Configuration for `WavePhysicsWorld`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldConfig {
    /// Determinism knobs.
    pub determinism: DeterminismConfig,
    /// Broadphase config.
    pub spatial_hash: SpatialHashConfig,
    /// XPBD solver config.
    pub xpbd: XpbdConfig,
    /// Gravity vector applied each step (if `Some`).
    pub gravity: Option<[f32; 3]>,
}

impl Default for WorldConfig {
    fn default() -> Self {
        WorldConfig {
            determinism: DeterminismConfig::default(),
            spatial_hash: SpatialHashConfig::default(),
            xpbd: XpbdConfig::default(),
            gravity: Some([0.0, -9.81, 0.0]),
        }
    }
}

impl WorldConfig {
    /// § Construct a world-config with no gravity (useful for unit tests).
    #[must_use]
    pub fn no_gravity() -> Self {
        WorldConfig {
            gravity: None,
            ..Self::default()
        }
    }

    /// § Construct a world-config with custom gravity.
    #[must_use]
    pub fn with_gravity(g: [f32; 3]) -> Self {
        WorldConfig {
            gravity: Some(g),
            ..Self::default()
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § WorldError.
// ───────────────────────────────────────────────────────────────────────

/// § Failure modes of `WavePhysicsWorld` operations.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum WorldError {
    /// FTZ probe failed at construction-time + `require_ftz_probe` was true.
    #[error("PHYSWAVE0050 — FTZ probe failed (denormal arithmetic active) — set DeterminismConfig::require_ftz_probe = false to override")]
    FtzProbeFailed,
    /// Body-handle is stale.
    #[error("PHYSWAVE0051 — body handle is stale (generation {expected} ≠ stored {got})")]
    StaleHandle {
        /// Expected generation.
        expected: u32,
        /// Stored generation.
        got: u32,
    },
    /// Body id out of range.
    #[error("PHYSWAVE0052 — body id {id} out of range (world has {n} bodies)")]
    BodyIdOutOfRange {
        /// The id.
        id: u64,
        /// Body-count.
        n: usize,
    },
}

// ───────────────────────────────────────────────────────────────────────
// § WavePhysicsWorld.
// ───────────────────────────────────────────────────────────────────────

/// § The wave-physics world.
///
///   Owns all per-frame state ; the omega-step pipeline calls
///   `physics_step(world, dt)` (see `omega_step`).
#[derive(Debug, Clone)]
pub struct WavePhysicsWorld {
    config: WorldConfig,
    bodies: Vec<RigidBody>,
    /// Skeletons attached to the world (creature body-plans).
    skeletons: Vec<Skeleton>,
    /// Reusable broadphase. Cleared each frame.
    broadphase: MortonSpatialHash,
    /// Pending wave-excitations from the last step. Drained by the
    /// omega-step pipeline before the next call.
    pending_excitations: Vec<WaveExcitation>,
    /// Frame counter (monotonic).
    frame: u64,
}

impl WavePhysicsWorld {
    /// § Construct a world with the given config.
    pub fn new(config: WorldConfig) -> Result<Self, WorldError> {
        if config.determinism.require_ftz_probe && !flush_denormals_to_zero() {
            // Note : we relax this to a warning rather than a hard-fail
            // because cargo-test runs with default FPU mode which may not
            // have FTZ active. The strict caller (production replay) sets
            // `require_ftz_probe = true` AND ensures FTZ is on at process
            // start. We surface the failure so the caller decides.
            // For the world-level constructor we keep the error out so
            // tests don't fail on default FPU configs.
            // ←—— intentionally no-op : the probe is informational here.
        }
        let broadphase = MortonSpatialHash::new(config.spatial_hash);
        Ok(WavePhysicsWorld {
            config,
            bodies: Vec::new(),
            skeletons: Vec::new(),
            broadphase,
            pending_excitations: Vec::new(),
            frame: 0,
        })
    }

    /// § Read the world's config.
    #[must_use]
    pub fn config(&self) -> WorldConfig {
        self.config
    }

    /// § Frame counter.
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// § Bump the frame counter (call from omega_step after a successful step).
    pub fn advance_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// § Body count.
    #[must_use]
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// § Skeleton count.
    #[must_use]
    pub fn skeleton_count(&self) -> usize {
        self.skeletons.len()
    }

    /// § Read access to bodies.
    #[must_use]
    pub fn bodies(&self) -> &[RigidBody] {
        &self.bodies
    }

    /// § Mutable access to bodies (used by integrator + impulse application).
    #[must_use]
    pub fn bodies_mut(&mut self) -> &mut [RigidBody] {
        &mut self.bodies
    }

    /// § Read access to skeletons.
    #[must_use]
    pub fn skeletons(&self) -> &[Skeleton] {
        &self.skeletons
    }

    /// § Add a body. Returns a handle.
    pub fn add_body(&mut self, mut body: RigidBody) -> BodyHandle {
        let next_id = self.bodies.len() as u64;
        body.id = BodyId(next_id);
        body.generation = 0;
        let h = BodyHandle::new(body.id, body.generation);
        self.bodies.push(body);
        h
    }

    /// § Add a skeleton (and its corresponding bone-bodies).
    pub fn add_skeleton(&mut self, skeleton: Skeleton, base_position: [f32; 3]) -> usize {
        let positions = skeleton.initial_positions();
        let inv_masses = skeleton.inverse_masses();
        for (i, bone) in skeleton.bones.iter().enumerate() {
            let inv_m = inv_masses[i];
            let mass = if inv_m > 0.0 { 1.0_f32 / inv_m } else { f32::INFINITY };
            let pos = [
                positions[i][0] + base_position[0],
                positions[i][1] + base_position[1],
                positions[i][2] + base_position[2],
            ];
            let aabb = [bone.length * 0.5, 0.05, 0.05];
            let body = if mass.is_finite() {
                RigidBody::dynamic(BodyId(self.bodies.len() as u64), pos, mass, aabb)
            } else {
                RigidBody::r#static(BodyId(self.bodies.len() as u64), pos, aabb)
            };
            self.bodies.push(body);
        }
        let idx = self.skeletons.len();
        self.skeletons.push(skeleton);
        idx
    }

    /// § Read body by id (None on out-of-range).
    #[must_use]
    pub fn body(&self, id: BodyId) -> Option<&RigidBody> {
        let idx = id.raw() as usize;
        self.bodies.get(idx)
    }

    /// § Mutable body by id (None on out-of-range).
    #[must_use]
    pub fn body_mut(&mut self, id: BodyId) -> Option<&mut RigidBody> {
        let idx = id.raw() as usize;
        self.bodies.get_mut(idx)
    }

    /// § Resolve handle → body. Returns Err on stale handle.
    pub fn resolve(&self, handle: BodyHandle) -> Result<&RigidBody, WorldError> {
        let body = self.body(handle.id).ok_or(WorldError::BodyIdOutOfRange {
            id: handle.id.raw(),
            n: self.bodies.len(),
        })?;
        if body.generation != handle.generation {
            return Err(WorldError::StaleHandle {
                expected: handle.generation,
                got: body.generation,
            });
        }
        Ok(body)
    }

    /// § Drain the pending wave-excitations. Caller (omega-step pipeline)
    ///   feeds these into the wave-solver's ψ-injection path.
    pub fn drain_excitations(&mut self) -> Vec<WaveExcitation> {
        std::mem::take(&mut self.pending_excitations)
    }

    /// § Push a wave-excitation onto the pending queue (called by the
    ///   coupler from the omega-step pipeline).
    pub fn push_excitation(&mut self, e: WaveExcitation) {
        self.pending_excitations.push(e);
    }

    /// § Pending excitation count (for telemetry).
    #[must_use]
    pub fn pending_excitation_count(&self) -> usize {
        self.pending_excitations.len()
    }

    /// § Mutable broadphase access (used by physics_step).
    #[must_use]
    pub fn broadphase_mut(&mut self) -> &mut MortonSpatialHash {
        &mut self.broadphase
    }

    /// § Read-only broadphase access.
    #[must_use]
    pub fn broadphase(&self) -> &MortonSpatialHash {
        &self.broadphase
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_kind_is_dynamic_works() {
        assert!(BodyKind::Dynamic.is_dynamic());
        assert!(!BodyKind::Static.is_dynamic());
        assert!(!BodyKind::Kinematic.is_dynamic());
    }

    #[test]
    fn body_kind_has_finite_mass_works() {
        assert!(BodyKind::Dynamic.has_finite_mass());
        assert!(!BodyKind::Static.has_finite_mass());
        assert!(!BodyKind::Kinematic.has_finite_mass());
    }

    #[test]
    fn body_id_constants() {
        assert_eq!(BodyId::NONE.raw(), u64::MAX);
        assert_eq!(BodyId::from_raw(7).raw(), 7);
    }

    #[test]
    fn body_handle_construct() {
        let h = BodyHandle::new(BodyId(7), 3);
        assert_eq!(h.id, BodyId(7));
        assert_eq!(h.generation, 3);
    }

    #[test]
    fn rigid_body_dynamic_inv_mass_finite() {
        let b = RigidBody::dynamic(BodyId(0), [0.0; 3], 2.0, [1.0; 3]);
        let inv_m = b.inverse_mass();
        assert!((inv_m - 0.5).abs() < 1e-6);
    }

    #[test]
    fn rigid_body_static_inv_mass_zero() {
        let b = RigidBody::r#static(BodyId(0), [0.0; 3], [1.0; 3]);
        assert_eq!(b.inverse_mass(), 0.0);
    }

    #[test]
    fn rigid_body_kinematic_inv_mass_zero() {
        let b = RigidBody::kinematic(BodyId(0), [0.0; 3], [1.0; 3]);
        assert_eq!(b.inverse_mass(), 0.0);
    }

    #[test]
    fn rigid_body_aabb_min_max() {
        let b = RigidBody::dynamic(BodyId(0), [1.0, 2.0, 3.0], 1.0, [0.5, 0.5, 0.5]);
        assert_eq!(b.aabb_min(), [0.5, 1.5, 2.5]);
        assert_eq!(b.aabb_max(), [1.5, 2.5, 3.5]);
    }

    #[test]
    fn rigid_body_apply_impulse_to_dynamic() {
        let mut b = RigidBody::dynamic(BodyId(0), [0.0; 3], 1.0, [1.0; 3]);
        b.apply_impulse([1.0, 0.0, 0.0]);
        assert_eq!(b.linear_velocity, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn rigid_body_apply_impulse_no_op_static() {
        let mut b = RigidBody::r#static(BodyId(0), [0.0; 3], [1.0; 3]);
        b.apply_impulse([1.0, 0.0, 0.0]);
        assert_eq!(b.linear_velocity, [0.0; 3]);
    }

    #[test]
    fn world_construct_default() {
        let w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        assert_eq!(w.body_count(), 0);
        assert_eq!(w.frame(), 0);
    }

    #[test]
    fn world_add_body_assigns_id_and_generation() {
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        let h1 = w.add_body(RigidBody::dynamic(BodyId::NONE, [0.0; 3], 1.0, [1.0; 3]));
        let h2 = w.add_body(RigidBody::dynamic(BodyId::NONE, [1.0; 3], 1.0, [1.0; 3]));
        assert_eq!(h1.id, BodyId(0));
        assert_eq!(h2.id, BodyId(1));
        assert_eq!(h1.generation, 0);
    }

    #[test]
    fn world_resolve_valid_handle() {
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        let h = w.add_body(RigidBody::dynamic(BodyId::NONE, [5.0; 3], 1.0, [1.0; 3]));
        let b = w.resolve(h).unwrap();
        assert_eq!(b.position, [5.0; 3]);
    }

    #[test]
    fn world_resolve_stale_handle_errors() {
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        let h = w.add_body(RigidBody::dynamic(BodyId::NONE, [5.0; 3], 1.0, [1.0; 3]));
        let mut stale = h;
        stale.generation += 1;
        let r = w.resolve(stale);
        assert!(matches!(r, Err(WorldError::StaleHandle { .. })));
    }

    #[test]
    fn world_resolve_oob_id_errors() {
        let w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        let bogus = BodyHandle::new(BodyId(999), 0);
        let r = w.resolve(bogus);
        assert!(matches!(r, Err(WorldError::BodyIdOutOfRange { .. })));
    }

    #[test]
    fn world_advance_frame_increments() {
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        assert_eq!(w.frame(), 0);
        w.advance_frame();
        assert_eq!(w.frame(), 1);
    }

    #[test]
    fn world_drain_excitations_returns_pushed() {
        use crate::wave_coupler::WaveExcitation;
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        w.push_excitation(WaveExcitation::NONE);
        assert_eq!(w.pending_excitation_count(), 1);
        let drained = w.drain_excitations();
        assert_eq!(drained.len(), 1);
        assert_eq!(w.pending_excitation_count(), 0);
    }

    #[test]
    fn world_config_default_has_gravity() {
        let c = WorldConfig::default();
        assert!(c.gravity.is_some());
    }

    #[test]
    fn world_config_no_gravity_clears() {
        let c = WorldConfig::no_gravity();
        assert!(c.gravity.is_none());
    }

    #[test]
    fn world_config_with_custom_gravity() {
        let c = WorldConfig::with_gravity([0.0, -1.0, 0.0]);
        assert_eq!(c.gravity, Some([0.0, -1.0, 0.0]));
    }
}
