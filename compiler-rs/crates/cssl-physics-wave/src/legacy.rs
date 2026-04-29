//! § Legacy — backward-compat shim for the cssl-physics rigid-body API.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Behind the `cssl-physics-legacy` feature-flag, expose a thin shim that
//!   replays the legacy `cssl-physics` public-API names so downstream
//!   crates (`loa-game`, integration tests, anything that pinned to the
//!   old crate at session-9) can keep building during the deprecation
//!   window.
//!
//!   With the flag OFF (default), this module is empty — the wave-physics
//!   surface is the only path. With the flag ON, the legacy names alias
//!   to the wave-physics types so consumers can migrate file-by-file.
//!
//! § DEPRECATION DISCIPLINE
//!   - Every legacy symbol is marked `#[deprecated]` with a migration-
//!     note pointing at the wave-physics replacement.
//!   - The legacy names are **NOT** re-exported at the crate root by
//!     default. Consumers must opt-in via `use cssl_physics_wave::legacy::*`.
//!   - The deprecation window is the T11-D117 → T11-D158 (~10-week)
//!     range ; after that the `cssl-physics-legacy` feature is dropped.
//!
//! § CITATION
//!   Mirror of `cssl-physics::lib.rs` § SURFACE block.

#[cfg(feature = "cssl-physics-legacy")]
pub use legacy_impl::*;

#[cfg(feature = "cssl-physics-legacy")]
mod legacy_impl {
    use crate::world::{BodyId, BodyKind, RigidBody as WaveRigidBody, WavePhysicsWorld, WorldConfig};

    /// § Legacy alias for [`crate::world::BodyId`].
    #[deprecated(
        since = "T11-D117",
        note = "use cssl_physics_wave::world::BodyId — wave-physics is the canonical surface post-T11-D117"
    )]
    pub type LegacyBodyId = BodyId;

    /// § Legacy alias for [`crate::world::BodyHandle`].
    #[deprecated(
        since = "T11-D117",
        note = "use cssl_physics_wave::world::BodyHandle — wave-physics is the canonical surface post-T11-D117"
    )]
    pub type LegacyBodyHandle = crate::world::BodyHandle;

    /// § Legacy alias for [`crate::world::BodyKind`].
    ///
    ///   Note : the legacy crate had a `Sleeping` variant which is
    ///   absent in the wave-physics V0. Consumers that branch on the
    ///   `Sleeping` discriminant must migrate to the wave-physics
    ///   active-only model (which is a strict superset of the legacy
    ///   `Static + Kinematic + Dynamic` triad).
    #[deprecated(
        since = "T11-D117",
        note = "use cssl_physics_wave::world::BodyKind — Sleeping variant deferred (every body is active in wave-physics V0)"
    )]
    pub type LegacyBodyKind = BodyKind;

    /// § Legacy alias for [`crate::world::RigidBody`].
    #[deprecated(
        since = "T11-D117",
        note = "use cssl_physics_wave::world::RigidBody — same shape, additional inverse_mass + AABB methods"
    )]
    pub type LegacyRigidBody = WaveRigidBody;

    /// § Legacy alias for [`crate::world::WavePhysicsWorld`].
    ///
    ///   The legacy crate had `PhysicsWorld` ; the wave-physics rename
    ///   is `WavePhysicsWorld`. Same per-frame entry-point signature,
    ///   modulo the optional SDF collider.
    #[deprecated(
        since = "T11-D117",
        note = "use cssl_physics_wave::world::WavePhysicsWorld — call physics_step(world, dt, None) for the legacy body-only path"
    )]
    pub type LegacyPhysicsWorld = WavePhysicsWorld;

    /// § Legacy alias for [`crate::world::WorldConfig`].
    #[deprecated(
        since = "T11-D117",
        note = "use cssl_physics_wave::world::WorldConfig — additional spatial-hash + xpbd config knobs"
    )]
    pub type LegacyWorldConfig = WorldConfig;

    /// § Legacy `BroadPhase` trait — preserved as an empty marker for
    ///   consumers that imported the trait but never implemented it.
    ///
    ///   The wave-physics broadphase is `MortonSpatialHash` directly ;
    ///   the trait abstraction is dropped.
    #[deprecated(
        since = "T11-D117",
        note = "MortonSpatialHash is the canonical broadphase — no trait abstraction exists in wave-physics"
    )]
    pub trait BroadPhase {}

    /// § Legacy `NarrowPhase` trait — preserved as an empty marker.
    ///
    ///   The wave-physics narrowphase IS the SDF query (`SdfCollider`).
    ///   There is no per-shape-pair dispatch ; everything's an SDF.
    #[deprecated(
        since = "T11-D117",
        note = "SdfCollider is the canonical narrowphase — query the SDF directly"
    )]
    pub trait NarrowPhase {}

    /// § Legacy `ConstraintSolver` trait — preserved as an empty marker.
    #[deprecated(
        since = "T11-D117",
        note = "XpbdSolver is the canonical constraint solver — sequential-impulse PGS is replaced by XPBD"
    )]
    pub trait ConstraintSolver {}

    /// § Legacy `Joint` trait — preserved as an empty marker.
    ///
    ///   The wave-physics surface uses `xpbd::Constraint::distance` /
    ///   `xpbd::Constraint::hinge` / `xpbd::Constraint::contact` etc.
    ///   directly. The trait abstraction is dropped.
    #[deprecated(
        since = "T11-D117",
        note = "use xpbd::Constraint::{distance, hinge, contact, ground_plane} — joint trait dropped"
    )]
    pub trait Joint {}

    /// § Sentinel attestation literal preserved verbatim from
    ///   `cssl-physics::ATTESTATION` for audit-walker compatibility.
    pub const LEGACY_ATTESTATION: &str =
        "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn legacy_attestation_present() {
            assert!(LEGACY_ATTESTATION.contains("no hurt nor harm"));
        }

        #[test]
        fn legacy_aliases_compile() {
            // Compile-only verification — instantiating the aliases
            // exercises the type-aliases.
            let _id: LegacyBodyId = LegacyBodyId(42);
            let _kind: LegacyBodyKind = LegacyBodyKind::Dynamic;
        }

        #[test]
        fn legacy_body_id_raw() {
            #[allow(deprecated)]
            let id: LegacyBodyId = LegacyBodyId(7);
            assert_eq!(id.raw(), 7);
        }
    }
}

// § With the feature OFF, expose at-least one constant so the module
//   compiles non-empty + downstream `use cssl_physics_wave::legacy::*`
//   doesn't break with a "module is empty" parse-error in stage-0
//   integrations that haven't enabled the feature.
#[cfg(not(feature = "cssl-physics-legacy"))]
pub const LEGACY_FEATURE_DISABLED: &str =
    "cssl-physics-legacy feature is disabled ; enable it for backward-compat aliases";

#[cfg(test)]
mod always_compiled_tests {
    #[test]
    fn module_present() {
        // Smoke-test : the module compiles regardless of feature-flag.
        let _ = ();
    }

    #[cfg(not(feature = "cssl-physics-legacy"))]
    #[test]
    fn legacy_disabled_constant_present() {
        assert!(super::LEGACY_FEATURE_DISABLED.contains("disabled"));
    }
}
