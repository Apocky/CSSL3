//! § ProceduralAnimError — diagnostic surface for the procedural-animation runtime.

use thiserror::Error;

/// Errors surfaced by the procedural-animation crate. Each variant carries
/// enough structured context for higher-level diagnostics to render an
/// actionable message without consulting the original call-site.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ProceduralAnimError {
    /// Caller referenced a bone index that exceeds the skeleton's bone
    /// count. Diagnostic carries both the offending index and the
    /// skeleton's actual bone count for readability.
    #[error("bone index {bone_idx} is out of range (skeleton has {bone_count} bones)")]
    BoneIndexOutOfRange {
        /// The offending index supplied by the caller.
        bone_idx: usize,
        /// The skeleton's actual bone count.
        bone_count: usize,
    },

    /// A skeleton's bone graph contains a cycle. Topological ordering
    /// cannot be computed ; the skeleton is unusable.
    #[error("skeleton bone graph contains a cycle starting at bone {start_idx}")]
    SkeletonCycle {
        /// The bone index where the cycle was detected.
        start_idx: usize,
    },

    /// A KAN pose-network was asked to emit a channel for a bone that has
    /// no channel binding registered. Diagnostic carries the bone index
    /// and the channel-kind for triage.
    #[error("no KAN pose channel registered for bone {bone_idx} kind {kind:?}")]
    MissingPoseChannel {
        /// Bone index without a channel binding.
        bone_idx: usize,
        /// Channel kind (translation / rotation / scale) that was requested.
        kind: crate::kan_pose::BoneChannelKind,
    },

    /// A creature lookup hit a missing handle in the
    /// [`crate::ProceduralAnimationWorld`].
    #[error("creature id {0} not registered in this world")]
    UnknownCreature(u64),

    /// A skeleton lookup hit a missing handle in the
    /// [`crate::ProceduralAnimationWorld`].
    #[error("skeleton id {0} not registered in this world")]
    UnknownSkeleton(u64),

    /// A physics-rig binding was attempted on a bone whose
    /// [`crate::physics_ik::PhysicsRig`] has no body for that bone index.
    #[error(
        "physics-rig binding for bone {bone_idx} missing in rig (rig has {rig_body_count} bodies)"
    )]
    MissingPhysicsBody {
        /// Bone index whose physics body could not be located.
        bone_idx: usize,
        /// Number of bodies registered in the rig.
        rig_body_count: usize,
    },

    /// A genome embedding had the wrong length. The embedding dim is fixed
    /// at construction-time ; mismatches are rejected eagerly.
    #[error("genome embedding length {got} does not match expected {expected}")]
    GenomeEmbeddingShapeMismatch {
        /// Length the caller supplied.
        got: usize,
        /// Length expected by the network.
        expected: usize,
    },

    /// A control-signal vector had the wrong length. The signal dim is
    /// fixed at construction-time ; mismatches are rejected eagerly.
    #[error("control signal length {got} does not match expected {expected}")]
    ControlSignalShapeMismatch {
        /// Length the caller supplied.
        got: usize,
        /// Length expected by the network.
        expected: usize,
    },

    /// An IK target referenced a chain whose end-effector index points
    /// outside the skeleton.
    #[error(
        "IK chain end-effector index {end_effector} out of range (skeleton has {bone_count} bones)"
    )]
    IkEndEffectorOutOfRange {
        /// End-effector bone index supplied by the caller.
        end_effector: usize,
        /// Skeleton bone count.
        bone_count: usize,
    },

    /// An IK chain has zero segments. The solver cannot operate on a
    /// degenerate chain.
    #[error("IK chain must contain at least one bone segment")]
    IkChainEmpty,
}

impl ProceduralAnimError {
    /// Returns `true` if the error is a structural-validation failure (the
    /// caller's input was malformed) versus a runtime-state failure
    /// (something the caller could not have anticipated).
    #[must_use]
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            ProceduralAnimError::BoneIndexOutOfRange { .. }
                | ProceduralAnimError::SkeletonCycle { .. }
                | ProceduralAnimError::GenomeEmbeddingShapeMismatch { .. }
                | ProceduralAnimError::ControlSignalShapeMismatch { .. }
                | ProceduralAnimError::IkEndEffectorOutOfRange { .. }
                | ProceduralAnimError::IkChainEmpty
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bone_index_diagnostic_renders() {
        let e = ProceduralAnimError::BoneIndexOutOfRange {
            bone_idx: 99,
            bone_count: 4,
        };
        assert!(e.to_string().contains("99"));
        assert!(e.to_string().contains("4"));
    }

    #[test]
    fn structural_classification_recognizes_input_errors() {
        let s = ProceduralAnimError::SkeletonCycle { start_idx: 0 };
        assert!(s.is_structural());
        let r = ProceduralAnimError::UnknownCreature(42);
        assert!(!r.is_structural());
    }

    #[test]
    fn missing_pose_channel_diag_contains_bone() {
        let e = ProceduralAnimError::MissingPoseChannel {
            bone_idx: 7,
            kind: crate::kan_pose::BoneChannelKind::Rotation,
        };
        let s = e.to_string();
        assert!(s.contains("7"));
    }

    #[test]
    fn ik_end_effector_diag_contains_index() {
        let e = ProceduralAnimError::IkEndEffectorOutOfRange {
            end_effector: 12,
            bone_count: 5,
        };
        let s = e.to_string();
        assert!(s.contains("12"));
        assert!(s.contains("5"));
    }

    #[test]
    fn shape_mismatch_diag() {
        let e = ProceduralAnimError::GenomeEmbeddingShapeMismatch {
            got: 16,
            expected: 32,
        };
        assert!(e.to_string().contains("16"));
        assert!(e.to_string().contains("32"));
    }

    #[test]
    fn structural_distinguishes_chain_empty() {
        let e = ProceduralAnimError::IkChainEmpty;
        assert!(e.is_structural());
    }

    #[test]
    fn unknown_creature_renders_id() {
        let e = ProceduralAnimError::UnknownCreature(1234);
        assert!(e.to_string().contains("1234"));
    }
}
