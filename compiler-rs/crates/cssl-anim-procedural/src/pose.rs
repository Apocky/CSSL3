//! § ProceduralPose — per-frame bone-local Transforms + cumulative model
//!   matrices.
//!
//! § THESIS
//!   The pose data structure pairs the bone-local `Transform` array with
//!   the cumulative model-space `Mat4` matrices ready for skinning upload.
//!   Identical layout to cssl-anim's `Pose` so the skinning pipeline does
//!   not need to know whether the pose came from keyframes or from
//!   procedural KAN evaluation.
//!
//!   The procedural runtime additionally produces deformation samples
//!   (per-bone wave-field-driven displacement) which are layered on top
//!   of the local transforms during the model-matrix sweep. See
//!   [`crate::deformation`].

use cssl_substrate_projections::Mat4;

use crate::skeleton::{ProceduralSkeleton, ROOT_PARENT};
use crate::transform::Transform;

/// Per-frame pose : bone-local transforms + cumulative model-space matrices.
#[derive(Debug, Clone, Default)]
pub struct ProceduralPose {
    /// Bone-local transforms ; written by the pose-network and the IK
    /// solver.
    locals: Vec<Transform>,
    /// Cumulative model-space matrices ; computed by
    /// [`ProceduralPose::compute_model_matrices`].
    model_matrices: Vec<Mat4>,
}

impl ProceduralPose {
    /// New empty pose.
    #[must_use]
    pub fn new() -> Self {
        Self {
            locals: Vec::new(),
            model_matrices: Vec::new(),
        }
    }

    /// Construct with `n` identity local transforms.
    #[must_use]
    pub fn with_bone_count(n: usize) -> Self {
        Self {
            locals: vec![Transform::IDENTITY; n],
            model_matrices: vec![Mat4::IDENTITY; n],
        }
    }

    /// Resize the pose to match the skeleton's bone count. Existing
    /// entries are preserved when growing ; new slots default to
    /// `Transform::IDENTITY`. Shrinking truncates.
    pub fn resize_to_skeleton(&mut self, skeleton: &ProceduralSkeleton) {
        let n = skeleton.bone_count();
        self.locals.resize(n, Transform::IDENTITY);
        self.model_matrices.resize(n, Mat4::IDENTITY);
    }

    /// Bone count.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.locals.len()
    }

    /// Read a single bone-local transform.
    #[must_use]
    pub fn local_transform(&self, idx: usize) -> Option<Transform> {
        self.locals.get(idx).copied()
    }

    /// Set a single bone-local transform.
    pub fn set_local_transform(&mut self, idx: usize, t: Transform) {
        if idx >= self.locals.len() {
            self.locals.resize(idx + 1, Transform::IDENTITY);
            self.model_matrices.resize(idx + 1, Mat4::IDENTITY);
        }
        self.locals[idx] = t;
    }

    /// Read-only access to all locals.
    #[must_use]
    pub fn locals(&self) -> &[Transform] {
        &self.locals
    }

    /// Read-only access to all model matrices.
    #[must_use]
    pub fn model_matrices(&self) -> &[Mat4] {
        &self.model_matrices
    }

    /// Read a single model-space matrix.
    #[must_use]
    pub fn model_matrix(&self, idx: usize) -> Option<Mat4> {
        self.model_matrices.get(idx).copied()
    }

    /// Compute the cumulative model-space matrices in a single forward
    /// sweep. `skeleton` must have the same bone count as the pose.
    pub fn compute_model_matrices(&mut self, skeleton: &ProceduralSkeleton) {
        if self.locals.len() != skeleton.bone_count() {
            self.resize_to_skeleton(skeleton);
        }
        for (i, b) in skeleton.bones().iter().enumerate() {
            let local = self.locals[i].to_mat4();
            self.model_matrices[i] = if b.parent_idx == ROOT_PARENT {
                local
            } else {
                self.model_matrices[b.parent_idx].compose(local)
            };
        }
    }

    /// Compute the skinning matrices `M_skin = M_model * M_inverseBind`.
    /// The result is written to the supplied buffer ; the buffer is
    /// resized to match the bone count.
    pub fn compute_skinning_matrices(
        &mut self,
        skeleton: &ProceduralSkeleton,
        out: &mut Vec<Mat4>,
    ) {
        self.compute_model_matrices(skeleton);
        out.resize(skeleton.bone_count(), Mat4::IDENTITY);
        for (i, b) in skeleton.bones().iter().enumerate() {
            out[i] = self.model_matrices[i].compose(b.inverse_bind_matrix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{Bone, ROOT_PARENT};
    use cssl_substrate_projections::Vec3;

    fn make_skel() -> ProceduralSkeleton {
        ProceduralSkeleton::from_bones(vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new(
                "a",
                0,
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            ),
            Bone::new(
                "b",
                1,
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            ),
        ])
        .unwrap()
    }

    #[test]
    fn new_pose_is_empty() {
        let p = ProceduralPose::new();
        assert_eq!(p.bone_count(), 0);
    }

    #[test]
    fn resize_to_skeleton_grows() {
        let s = make_skel();
        let mut p = ProceduralPose::new();
        p.resize_to_skeleton(&s);
        assert_eq!(p.bone_count(), 3);
    }

    #[test]
    fn set_local_transform_grows_pose() {
        let mut p = ProceduralPose::new();
        p.set_local_transform(5, Transform::IDENTITY);
        assert_eq!(p.bone_count(), 6);
    }

    #[test]
    fn local_transform_round_trip() {
        let mut p = ProceduralPose::with_bone_count(2);
        let t = Transform::from_translation(Vec3::new(7.0, 8.0, 9.0));
        p.set_local_transform(1, t);
        assert_eq!(p.local_transform(1).unwrap(), t);
    }

    #[test]
    fn compute_model_matrices_chains_translation() {
        let s = make_skel();
        let mut p = ProceduralPose::new();
        p.resize_to_skeleton(&s);
        // Pose locals match bind-pose.
        for (i, b) in s.bones().iter().enumerate() {
            p.set_local_transform(i, b.local_bind_transform);
        }
        p.compute_model_matrices(&s);
        // Bone "b" should be at +2 along X (root + a + b each adding +1 except root which is 0).
        let mb = p.model_matrix(2).unwrap();
        assert!((mb.cols[3][0] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn compute_skinning_matrices_writes_buffer() {
        let s = make_skel();
        let mut p = ProceduralPose::new();
        p.resize_to_skeleton(&s);
        for (i, b) in s.bones().iter().enumerate() {
            p.set_local_transform(i, b.local_bind_transform);
        }
        let mut buf = Vec::new();
        p.compute_skinning_matrices(&s, &mut buf);
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn locals_returns_full_slice() {
        let p = ProceduralPose::with_bone_count(4);
        assert_eq!(p.locals().len(), 4);
    }

    #[test]
    fn local_transform_out_of_range_returns_none() {
        let p = ProceduralPose::with_bone_count(2);
        assert!(p.local_transform(99).is_none());
    }

    #[test]
    fn skinning_matrices_for_bind_pose_equal_identity() {
        // For a chain in bind pose, M_skin = M_model * M_inverseBind = I.
        let s = make_skel();
        let mut p = ProceduralPose::new();
        p.resize_to_skeleton(&s);
        for (i, b) in s.bones().iter().enumerate() {
            p.set_local_transform(i, b.local_bind_transform);
        }
        let mut buf = Vec::new();
        p.compute_skinning_matrices(&s, &mut buf);
        for m in &buf {
            for col in 0..4 {
                for row in 0..4 {
                    let expected = if col == row { 1.0 } else { 0.0 };
                    assert!(
                        (m.cols[col][row] - expected).abs() < 1e-4,
                        "col {} row {} = {} expected {}",
                        col,
                        row,
                        m.cols[col][row],
                        expected
                    );
                }
            }
        }
    }
}
