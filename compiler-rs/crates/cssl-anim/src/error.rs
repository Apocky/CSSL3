//! `AnimError` — every failure mode the animation runtime can surface.
//!
//! § Variants are STABLE from S9-T1 forward ; renaming = major-version-
//! bump per the workspace ABI-stability rule. New variants append-only.

use thiserror::Error;

/// All failures the animation runtime can produce.
///
/// § DIAGNOSTIC CODES
/// | variant                | code     | meaning                                                |
/// |------------------------|----------|--------------------------------------------------------|
/// | `BoneIndexOutOfRange`  | ANIM0001 | a `bone_idx` exceeded the skeleton's bone count        |
/// | `SkeletonCycle`        | ANIM0002 | parent indices form a cycle (impossible hierarchy)     |
/// | `EmptyChannel`         | ANIM0003 | a channel was authored with zero keyframes             |
/// | `KeyframesUnsorted`    | ANIM0004 | keyframe times are not monotonic-non-decreasing        |
/// | `CubicMissingTangents` | ANIM0005 | cubic-spline channel missing in/out tangents           |
/// | `IkChainTooShort`      | ANIM0006 | chain has fewer bones than the solver requires         |
/// | `IkUnreachable`        | ANIM0007 | target lies outside the solvable region                |
/// | `BlendTreeMalformed`   | ANIM0008 | blend-tree references a missing clip / weights wrong   |
/// | `UnknownSkeleton`      | ANIM0009 | `AnimationWorld` asked about an unregistered skeleton  |
/// | `UnknownClipInstance`  | ANIM0010 | `AnimationWorld` asked about an unregistered instance  |
/// | `WeightOutOfRange`     | ANIM0011 | a blend weight was outside `[0, 1]` or negative        |
#[derive(Debug, Error, Clone, PartialEq)]
pub enum AnimError {
    /// A bone index in a channel target / IK chain / pose write exceeded
    /// the skeleton's `bone_count()`.
    #[error("ANIM0001 — bone index {bone_idx} out of range (skeleton has {bone_count} bones)")]
    BoneIndexOutOfRange { bone_idx: usize, bone_count: usize },

    /// The parent-index list contains a cycle. A skeleton is a tree, not a
    /// graph ; cycles are rejected at construction.
    #[error("ANIM0002 — skeleton parent indices form a cycle starting at bone {start_idx}")]
    SkeletonCycle { start_idx: usize },

    /// An animation channel was authored with zero keyframes — there is
    /// nothing to sample, so the channel is rejected.
    #[error("ANIM0003 — animation channel for bone {bone_idx} has no keyframes")]
    EmptyChannel { bone_idx: usize },

    /// Keyframe times must be monotonic-non-decreasing. The sampler relies
    /// on this for binary-search lookup.
    #[error(
        "ANIM0004 — keyframe times not sorted at index {at_idx} (time {time} < previous {prev_time})"
    )]
    KeyframesUnsorted {
        at_idx: usize,
        time: f32,
        prev_time: f32,
    },

    /// A cubic-spline channel must carry in-tangent and out-tangent values
    /// alongside each value. GLTF-canonical : the channel array length is
    /// `3 * keyframe_count`.
    #[error(
        "ANIM0005 — cubic-spline channel for bone {bone_idx} expects {expected} values (got {got})"
    )]
    CubicMissingTangents {
        bone_idx: usize,
        expected: usize,
        got: usize,
    },

    /// An IK chain was constructed with fewer bones than the solver needs.
    /// Two-bone IK requires exactly 2 segments (3 joints) ; FABRIK requires
    /// at least 2 bones.
    #[error(
        "ANIM0006 — IK chain too short : got {got} bones, need at least {required} for {solver}"
    )]
    IkChainTooShort {
        got: usize,
        required: usize,
        solver: &'static str,
    },

    /// IK target lies outside the chain's reachable region. Two-bone IK
    /// returns this when `|target - root| > l1 + l2` ; the solver clamps
    /// to a fully-extended pose by default but exposes this variant for
    /// callers that want the explicit signal.
    #[error("ANIM0007 — IK target unreachable : distance {distance} > max reach {max_reach}")]
    IkUnreachable { distance: f32, max_reach: f32 },

    /// A blend tree references a clip handle that was never registered, or
    /// a Blend2 / BlendN weight tuple is malformed.
    #[error("ANIM0008 — blend tree malformed : {reason}")]
    BlendTreeMalformed { reason: String },

    /// `AnimationWorld` was asked about a skeleton id that was never
    /// registered with `register_skeleton`.
    #[error("ANIM0009 — unknown skeleton id {id}")]
    UnknownSkeleton { id: u64 },

    /// `AnimationWorld` was asked about a clip instance id that was never
    /// registered with `spawn_clip_instance`.
    #[error("ANIM0010 — unknown clip instance id {id}")]
    UnknownClipInstance { id: u64 },

    /// A blend weight or normalized clamp parameter was outside the legal
    /// `[0, 1]` range.
    #[error("ANIM0011 — weight {weight} out of range [0, 1]")]
    WeightOutOfRange { weight: f32 },
}

impl AnimError {
    /// Stable diagnostic code prefix. Used by audit-walkers to bucket
    /// failures by category at low overhead.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::BoneIndexOutOfRange { .. } => "ANIM0001",
            Self::SkeletonCycle { .. } => "ANIM0002",
            Self::EmptyChannel { .. } => "ANIM0003",
            Self::KeyframesUnsorted { .. } => "ANIM0004",
            Self::CubicMissingTangents { .. } => "ANIM0005",
            Self::IkChainTooShort { .. } => "ANIM0006",
            Self::IkUnreachable { .. } => "ANIM0007",
            Self::BlendTreeMalformed { .. } => "ANIM0008",
            Self::UnknownSkeleton { .. } => "ANIM0009",
            Self::UnknownClipInstance { .. } => "ANIM0010",
            Self::WeightOutOfRange { .. } => "ANIM0011",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AnimError;

    #[test]
    fn codes_stable() {
        assert_eq!(
            AnimError::BoneIndexOutOfRange {
                bone_idx: 5,
                bone_count: 3
            }
            .code(),
            "ANIM0001"
        );
        assert_eq!(AnimError::SkeletonCycle { start_idx: 0 }.code(), "ANIM0002");
        assert_eq!(AnimError::EmptyChannel { bone_idx: 0 }.code(), "ANIM0003");
        assert_eq!(
            AnimError::KeyframesUnsorted {
                at_idx: 1,
                time: 0.5,
                prev_time: 1.0,
            }
            .code(),
            "ANIM0004"
        );
        assert_eq!(
            AnimError::CubicMissingTangents {
                bone_idx: 0,
                expected: 9,
                got: 3,
            }
            .code(),
            "ANIM0005"
        );
        assert_eq!(
            AnimError::IkChainTooShort {
                got: 1,
                required: 2,
                solver: "TwoBoneIk",
            }
            .code(),
            "ANIM0006"
        );
        assert_eq!(
            AnimError::IkUnreachable {
                distance: 5.0,
                max_reach: 3.0,
            }
            .code(),
            "ANIM0007"
        );
        assert_eq!(
            AnimError::BlendTreeMalformed {
                reason: "missing clip 0".into()
            }
            .code(),
            "ANIM0008"
        );
        assert_eq!(AnimError::UnknownSkeleton { id: 42 }.code(), "ANIM0009");
        assert_eq!(AnimError::UnknownClipInstance { id: 7 }.code(), "ANIM0010");
        assert_eq!(
            AnimError::WeightOutOfRange { weight: 1.5 }.code(),
            "ANIM0011"
        );
    }

    #[test]
    fn display_renders_useful_diagnostics() {
        let e = AnimError::BoneIndexOutOfRange {
            bone_idx: 5,
            bone_count: 3,
        };
        let s = e.to_string();
        assert!(s.contains("ANIM0001"));
        assert!(s.contains("5"));
        assert!(s.contains("3"));
    }
}
