//! § cssl-anim — Animation runtime for the Substrate
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Skeletal animation runtime : skeletons + bone hierarchies, animation
//!   clips with keyframe channels, blend-tree compositing for layered
//!   animations, and IK solvers (two-bone analytic + FABRIK iterative)
//!   for runtime-driven pose adjustment. Output is a `Pose` that pairs
//!   bone-local transforms with the cumulative model-space matrices ready
//!   for skinning upload.
//!
//! § PLACE IN THE STACK
//!   - Consumes `cssl-substrate-projections` for `Vec3`, `Quat`, and
//!     `Mat4`. Stage-0 keeps the math-precedent unified : the same
//!     `Quat::compose` / `Mat4::compose` rules apply here.
//!   - Implements [`cssl_substrate_omega_step::OmegaSystem`] via
//!     [`world::AnimationWorld`] so animation evaluation becomes a regular
//!     omega-step phase. The default effect-row is `{Sim}` ; downstream
//!     consumers may union with `{Render}` or `{Audio}` for skinning /
//!     animation-driven sound emission as needed.
//!   - Future MATH (M1) + ASSET (N1) sibling slices will widen the math
//!     surface (curves / dual-quaternions) and add GLTF-canonical loading.
//!     Stage-0 expresses the runtime contract independently so those
//!     slices can land without re-design.
//!
//! § SURFACE SUMMARY
//!   - [`Transform`] : bone-local transform `(translation, rotation, scale)`.
//!     Pairs with [`Transform::to_mat4`] for matrix conversion and
//!     [`Transform::interpolate`] for keyframe blending.
//!   - [`Skeleton`] / [`Bone`] : flat-array bone hierarchy with parent
//!     indexing. Construction sorts bones into topological (parent-first)
//!     order so cumulative-transform passes are a single forward sweep.
//!   - [`AnimationClip`] / [`AnimChannel`] / [`Interpolation`] /
//!     [`KeyframeT`] / [`KeyframeR`] / [`KeyframeS`] : the canonical
//!     authored animation form. Channels target individual bones by index.
//!     Three interpolation modes : `Linear`, `CubicSpline` (GLTF-canonical
//!     tangent form), `Step`.
//!   - [`AnimSampler`] : evaluates an `AnimationClip` at time `t`, writing
//!     bone-local transforms into a target `Pose`. Slerp for rotations,
//!     linear for translation/scale, cubic-spline for high-fidelity work.
//!   - [`BlendTree`] / [`BlendNode`] : compositing graph over clips.
//!     Variants : `Clip`, `Blend2(weight)`, `AdditiveBlend`,
//!     `BlendN(weights)`. Produces a final `Pose` from any number of
//!     authored clips weighted at evaluation time.
//!   - [`TwoBoneIk`] : analytic law-of-cosines two-bone IK suitable for
//!     arms, legs, and any 2-segment chain.
//!   - [`FabrikChain`] : iterative FABRIK (Forward-And-Backward Reaching
//!     Inverse Kinematics) for chains of arbitrary length — spines, tails.
//!   - [`Pose`] : output of animation evaluation. Pairs the bone-local
//!     `Transform` array with the cumulative model-space `Mat4` matrices.
//!   - [`AnimationWorld`] : aggregates skeletons + active clips +
//!     blend-trees and ticks per omega_step.
//!
//! § DETERMINISM
//!   All math here is total + deterministic. Sampling at the same `(clip,
//!   t)` produces bit-identical pose output across runs. Quaternion slerp
//!   uses the canonical `acos`/`sin` form ; `nlerp` is offered as a fast-
//!   path for short keyframe deltas where the angular error is below
//!   the visible threshold (configurable via [`SamplerConfig::nlerp_threshold`]).
//!
//! § PRIME-DIRECTIVE
//!   Animation systems carry the same {Sim} effect-row discipline as every
//!   other Substrate system. Replay-determinism is preserved : sampling at
//!   the same time + the same blend-weights yields identical poses across
//!   machines. No clock reads, no entropy, no global state. Consent for
//!   omega-step participation is gated through the sibling
//!   `cssl-substrate-omega-step` crate's `caps_grant(omega_register)`.
//!
//! § STAGE-0 LIMITATIONS / DEFERRED
//!   - **GLTF parsing** : the on-disk GLTF format is the eventual ASSET
//!     (N1) sibling's responsibility. Stage-0 surfaces an in-memory
//!     constructor only — `Skeleton::from_bones` and `AnimationClip::new`.
//!   - **Dual-quaternion skinning** : stage-0 produces linear-blend-skin
//!     (LBS) ready model matrices. Dual-quaternion skinning to avoid
//!     volume-loss artifacts on heavily-twisted joints lands later.
//!   - **Animation events** : "fire footstep at frame 14" event channels
//!     are deferred ; stage-0 surfaces only T/R/S transform channels.
//!   - **Morph-target / shape-key animation** : skeletal-only at stage-0.
//!   - **Retargeting** : transferring a clip authored on one skeleton to
//!     a different rig is deferred to a follow-up slice.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — match sibling-crate stage-0 stance ; tighten at
// post-T1 stabilization.
#![allow(clippy::similar_names)] // ax/ay/az + tx/ty/tz coordinate-component naming reads better short.
#![allow(clippy::many_single_char_names)] // i/j/k/m/n match the math literature for IK derivations.
#![allow(clippy::float_cmp)] // exact equality intended where keyframes coincide with sample time.
#![allow(clippy::suboptimal_flops)] // mul_add used in hot paths ; literal arithmetic kept readable.
#![allow(clippy::cast_precision_loss)] // bone-index → f32 in test fixtures is bounded + well-defined.
#![allow(clippy::needless_range_loop)] // for col in 0..N reads more naturally for matrix walks.
#![allow(clippy::trivially_copy_pass_by_ref)] // OmegaSystem trait fn-signatures take refs uniformly across crates.
#![allow(clippy::unnecessary_literal_bound)]
// OmegaSystem trait declares fn name(&self) -> &str ; impl widens lifetime by ref-method discipline.
#![allow(clippy::single_char_pattern)] // diagnostic-string contains-checks read clearer with " " over ' '.
#![allow(clippy::explicit_iter_loop)] // BTreeMap.iter_mut() reads clearer than &mut self.foo for newcomers walking the code.

pub mod blend;
pub mod clip;
pub mod error;
pub mod ik;
pub mod pose;
pub mod sampler;
pub mod skeleton;
pub mod transform;
pub mod world;

pub use blend::{BlendNode, BlendTree, BlendTreeError, ClipHandle};
pub use clip::{
    AnimChannel, AnimChannelKind, AnimationClip, ChannelTarget, Interpolation, KeyframeR,
    KeyframeS, KeyframeT,
};
pub use error::AnimError;
pub use ik::{FabrikChain, FabrikOutcome, IkResult, TwoBoneIk};
pub use pose::Pose;
pub use sampler::{AnimSampler, SamplerConfig};
pub use skeleton::{Bone, Skeleton, ROOT_PARENT};
pub use transform::Transform;
pub use world::{AnimationWorld, ClipInstance, ClipInstanceId, SkeletonId};

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
}
