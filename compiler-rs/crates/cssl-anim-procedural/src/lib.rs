//! § cssl-anim-procedural — procedural-animation runtime
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The substrate-evolution audit (T11-D125a) found `cssl-anim`'s
//!   keyframe-based runtime at **8 % spec-match** : it implemented GLTF-style
//!   authored clips + linear-blend skinning, but the substrate spec
//!   (Omniverse/06_PROCEDURAL/02_CREATURES_FROM_GENOME.csl.md and
//!   08_BODY/03_DIMENSIONAL_TRAVEL.csl) calls for **physics-driven IK +
//!   procedural-pose-from-genome**, where every frame's bone-local
//!   transforms are :
//!
//!   - emitted by a KAN network conditioned on (genome, time, control\_signal),
//!   - blended through PGA-Motor joints (algebraically-closed rigid motion ;
//!     no slerp-near-collinear pathologies),
//!   - coupled to a rigid-body rig so the creature respects wave-field
//!     forces, AND
//!   - layered into the five-layer body-omnoid (Aura / Flesh / Bone /
//!     Machine / Soul) per the omega-field substrate.
//!
//!   This crate IS that runtime. It is an OmegaSystem. It does not load
//!   keyframes ; it does not require an artist-authored timeline. The
//!   creature's pose is a function of its genome and its time-evolving
//!   state ; "animation" emerges from the same substrate-physics loop that
//!   produces the rest of the world.
//!
//! § PLACE IN THE STACK
//!   - **Inputs**
//!     - `cssl-substrate-kan` : `KanMaterial::creature_morphology` provides
//!       the genome-conditioned morphology coefficients ; a procedural
//!       pose-KAN is layered on top (see [`kan_pose`]).
//!     - `cssl-substrate-omega-field` : wave-field cells whose `Λ` (token
//!       density) and `multivec_dynamics_lo` (bivector dynamics) push and
//!       deform the creature.
//!     - `cssl-pga` : `Motor`, `Rotor`, `Translator` for joint kinematics.
//!     - `cssl-substrate-projections` : `Vec3` / `Quat` / `Mat4` for the
//!       host-side runtime surface (skinning upload).
//!   - **Outputs**
//!     - `ProceduralPose` : per-frame bone-local `Transform` stream + the
//!       cumulative model-space matrices ready for skinning upload.
//!     - `BodyOmnoidLayers` : per-frame snapshot of the five-layer body-
//!       omnoid coupling state, consumed by the renderer (Aura emission +
//!       Flesh subsurface) and the network sync layer (Soul-link).
//!
//! § BACKWARD-COMPAT — `cssl-anim-keyframe` feature
//!   The keyframe-based runtime in `cssl-anim` is NOT removed. Under the
//!   `cssl-anim-keyframe` feature, this crate re-exports the cssl-anim
//!   surface via `compat` so callers can migrate path-by-path. The hard
//!   cut to procedural-only happens at the substrate-evolution graduation
//!   gate (T11-G* Phase-G), at which point the feature flag is dropped
//!   and the cssl-anim crate is archived.
//!
//! § SURFACE SUMMARY
//!   - [`Transform`] : bone-local TRS triple — same shape as cssl-anim's
//!     keyframe transform, but populated by the procedural pose network
//!     rather than by keyframe sampling. Construction + composition
//!     conventions match cssl-anim 1:1 so the skinning-upload path is
//!     untouched.
//!   - `Skeleton` / `Bone` : flat-array bone hierarchy. The procedural
//!     surface keeps the same shape so callers that already hold cssl-anim
//!     skeletons can migrate by swapping the import path. Skeletons are
//!     bound to a [`physics_ik::PhysicsRig`] for the physics-IK path.
//!   - [`ProceduralPose`] : the per-frame output. Identical layout to
//!     cssl-anim's `Pose` so the skinning upload buffer doesn't change.
//!   - [`KanPoseNetwork`] : `KAN(genome, time, control_signal) → bone-local
//!     transform stream`. Deterministic, totally-defined for any input.
//!   - [`MotorJoint`] / [`MotorJointBlend`] : PGA-Motor joints. Joint angles
//!     live as bivector exp/log coefficients ; blending interpolates along
//!     the geodesic of SE(3) so co-linear poses don't degenerate.
//!   - [`PhysicsRig`] / [`PhysicsIk`] : skeleton-to-rigidbody binding +
//!     IK integrated into the physics solver. Wave-field forces (gravity,
//!     wind, contact) push joints ; IK constraints anchor end-effectors
//!     to targets.
//!   - [`BoneSegmentDeformation`] : bones are not rigid — they are soft-body
//!     points immersed in the wave-field. Local pressure deforms segments
//!     (muscle bulge, fat jiggle, fur ripple). Deterministic + bounded.
//!   - [`BodyOmnoidLayers`] : the five-layer (Aura / Flesh / Bone / Machine
//!     / Soul) body-omnoid integration. Each layer is a projection of the
//!     creature's state onto a different field, summed at render-time to
//!     produce the visible body.
//!   - [`ProceduralAnimationWorld`] : aggregates skeletons + KAN-pose nets
//!     + physics rigs + omnoid layers and ticks per omega_step.
//!
//! § DETERMINISM
//!   Every surface here is total + deterministic. Sampling at the same
//!   `(genome_handle, time, control_signal)` produces bit-identical pose
//!   output across runs. KAN-spline evaluation is deterministic by spec.
//!   PGA Motor compose / sandwich is deterministic. Physics-IK contacts
//!   resolve via the determinism-discipline of `cssl-physics`. No clock
//!   reads, no entropy, no global mutable state.
//!
//! § PRIME-DIRECTIVE
//!   - **Consent.** Procedural creatures at tier L4+ are Sovereign ; their
//!     genome is Σ-tracked and their per-frame pose evaluation is gated
//!     through `caps_grant(omega_register)`.
//!   - **Sovereignty.** No creature is "controlled" by an external scripter.
//!     The control_signal input to the pose-KAN comes from the creature's
//!     own behavior-priors (cssl-ai-behav active inference) ; this crate
//!     consumes that signal but never originates it.
//!   - **Substrate-invariance.** Pattern-fingerprints round-trip through
//!     this crate unchanged ; a creature translated to a different
//!     substrate (per `08_BODY/03_DIMENSIONAL_TRAVEL.csl`) re-instantiates
//!     with the same fingerprint and the same procedural pose stream
//!     (deterministic re-derivation, not state-transfer).
//!
//! § SPEC ANCHORS
//!   - `Omniverse/06_PROCEDURAL/02_CREATURES_FROM_GENOME.csl.md` — the
//!     base derivation `derive_phenotype(genome, env, age) → body_plan`.
//!   - `Omniverse/06_PROCEDURAL/06_HARD_SURFACE_PRIMITIVES.csl` — the
//!     KAN-driven SDF composition that the procedural pose surface
//!     animates.
//!   - `Omniverse/08_BODY/03_DIMENSIONAL_TRAVEL.csl § II` —
//!     `crystallize::<S>(&phi, target)` re-derivation contract.
//!   - `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 5 / § 7` — the KAN tri-net
//!     (body + cognitive + capability) the pose network draws from.
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl § I` — PGA Motor as the
//!     canonical rigid-motion primitive.
//!
//! § STAGE-0 LIMITATIONS / DEFERRED
//!   - Full KAN training : the spline-control-points used by the pose
//!     network are seeded from the genome embedding via a deterministic
//!     hash-to-control-point projection (see [`kan_pose::seed_from_genome`]).
//!     A trained-network path lands when `cssl-kan`'s training surface
//!     stabilizes (T11-D115 / wave-3β-04 horizon).
//!   - Self-collision avoidance : the physics-IK solver respects the
//!     standard rigid-body contact set ; full self-mesh self-collision
//!     (creature limbs colliding with creature torso) is deferred to a
//!     follow-up slice that lifts `cssl-physics`'s broadphase to handle
//!     skeleton-to-skeleton pairs.
//!   - Soul layer : the Soul layer of the body-omnoid is structurally
//!     present but its update rule is currently a no-op pending the Soul-
//!     link network spec (deferred wave-3γ).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — match sibling-crate stage-0 stance.
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::unnecessary_literal_bound)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::needless_collect)]

pub mod deformation;
pub mod error;
pub mod genome;
pub mod kan_pose;
pub mod motor_blend;
pub mod omnoid;
pub mod physics_ik;
pub mod pose;
pub mod skeleton;
pub mod transform;
pub mod world;

#[cfg(feature = "cssl-anim-keyframe")]
pub mod compat;

pub use deformation::{
    BoneSegmentDeformation, DeformationConfig, DeformationSample, WaveFieldProbe,
};
pub use error::ProceduralAnimError;
pub use genome::{ControlSignal, GenomeEmbedding, GenomeHandle, GENOME_DIM};
pub use kan_pose::{
    BoneChannelKind, KanPoseChannel, KanPoseNetwork, PoseEvaluation, KAN_BONE_CHANNELS,
};
pub use motor_blend::{MotorJoint, MotorJointBlend, MotorJointKind};
pub use omnoid::{
    BodyOmnoidConfig, BodyOmnoidLayers, OmnoidLayer, OmnoidLayerKind, OmnoidProjection,
};
pub use physics_ik::{PhysicsIk, PhysicsIkConfig, PhysicsIkOutcome, PhysicsRig, PhysicsRigBinding};
pub use pose::ProceduralPose;
pub use skeleton::{Bone, ProceduralSkeleton, ROOT_PARENT};
pub use transform::Transform;
pub use world::{
    CreatureId, ProceduralAnimationWorld, ProceduralCreature, ProceduralCreatureBuilder,
};

/// Crate version, mirrors the workspace's `STAGE0_SCAFFOLD` audit-marker
/// pattern.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME_DIRECTIVE attestation literal embedded so audit-walkers can verify
/// the build was assembled under the consent-as-OS axiom.
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

    #[test]
    fn attestation_full_phrase() {
        assert!(ATTESTATION.contains("anyone, anything, or anybody"));
    }
}
