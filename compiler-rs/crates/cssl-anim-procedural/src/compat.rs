//! § compat — backward-compatibility re-exports for the keyframe surface.
//!
//! § THESIS
//!   Under the `cssl-anim-keyframe` cargo feature, this module re-exports
//!   the legacy keyframe-based runtime from `cssl-anim` so callers can
//!   migrate path-by-path. The compatibility shim is **non-default** —
//!   the procedural surface stands on its own at the crate root.
//!
//!   Migration path :
//!     1. New code uses [`crate`] root surfaces directly.
//!     2. Legacy code that depends on cssl-anim symbols imports them via
//!        [`compat`] : `use cssl_anim_procedural::compat::Skeleton;`.
//!     3. Once a migration cohort completes, the corresponding cssl-anim
//!        symbol is dropped from this module.
//!     4. Final substrate-evolution graduation (T11-G* Phase-G) removes
//!        the feature flag entirely + archives cssl-anim.
//!
//! § AVAILABILITY
//!   This module is only present when the `cssl-anim-keyframe` cargo
//!   feature is enabled. Without the feature, the keyframe runtime is
//!   unreachable from this crate and callers that need it must depend on
//!   `cssl-anim` directly.

pub use cssl_anim::{
    AnimChannel, AnimChannelKind, AnimError, AnimSampler, AnimationClip, AnimationWorld, BlendNode,
    BlendTree, BlendTreeError, Bone, ChannelTarget, ClipHandle, ClipInstance, ClipInstanceId,
    FabrikChain, FabrikOutcome, IkResult, Interpolation, KeyframeR, KeyframeS, KeyframeT, Pose,
    SamplerConfig, Skeleton, SkeletonId, Transform, TwoBoneIk, ROOT_PARENT,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframe_transform_is_reachable() {
        let _t = Transform::IDENTITY;
    }

    #[test]
    fn keyframe_skeleton_construct_empty() {
        let s = Skeleton::from_bones(vec![]).expect("empty allowed");
        assert_eq!(s.bone_count(), 0);
    }

    #[test]
    fn keyframe_root_parent_is_max_usize() {
        assert_eq!(ROOT_PARENT, usize::MAX);
    }
}
