//! `Pose` — the output of animation evaluation.
//!
//! § THESIS
//!   A pose pairs the per-bone local-space `Transform` with the per-bone
//!   model-space `Mat4`. The local-space half is what the sampler /
//!   blend-tree write into ; the model-space half is computed from it
//!   via a single forward pass over the topologically-sorted skeleton.
//!
//! § SKINNING MATRICES
//!   The downstream consumer (a renderer / GPU skinning pass) typically
//!   needs the per-bone "skinning matrix" `M_skin = M_model * M_bind^-1`.
//!   `Pose::skinning_matrix(bone_idx)` produces that on demand without
//!   modifying the pose itself ; alternatively `Pose::compute_skinning_matrices`
//!   bulk-emits a matching `Vec<Mat4>`.

use cssl_substrate_projections::Mat4;

use crate::error::AnimError;
use crate::skeleton::{Skeleton, ROOT_PARENT};
use crate::transform::Transform;

/// Per-bone pose state.
///
/// § FIELDS
/// - `local_transforms` : bone-local `Transform` (relative to parent).
/// - `model_transforms` : cumulative model-space `Mat4` per bone, computed
///   from the local transforms by [`Self::recompute_model_transforms`].
#[derive(Debug, Clone)]
pub struct Pose {
    /// Bone-local transforms — one per skeleton bone, in skeleton order.
    pub local_transforms: Vec<Transform>,
    /// Cumulative model-space matrices — one per skeleton bone, in
    /// skeleton order. Stale until [`Self::recompute_model_transforms`]
    /// is called after the local-transforms array changes.
    pub model_transforms: Vec<Mat4>,
}

impl Pose {
    /// Construct a fresh pose from a skeleton, initialised to the bind
    /// pose. After this constructor returns, both the local-transform
    /// array and model-transform array are valid.
    #[must_use]
    pub fn from_bind_pose(skeleton: &Skeleton) -> Self {
        let local_transforms: Vec<Transform> = skeleton
            .bones()
            .iter()
            .map(|b| b.local_bind_transform)
            .collect();
        let mut pose = Self {
            local_transforms,
            model_transforms: vec![Mat4::IDENTITY; skeleton.bone_count()],
        };
        pose.recompute_model_transforms(skeleton);
        pose
    }

    /// Construct a pose with all-identity local transforms. Useful when
    /// the caller plans to overwrite every entry before reading.
    #[must_use]
    pub fn identity(bone_count: usize) -> Self {
        Self {
            local_transforms: vec![Transform::IDENTITY; bone_count],
            model_transforms: vec![Mat4::IDENTITY; bone_count],
        }
    }

    /// Number of bones this pose is sized for.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.local_transforms.len()
    }

    /// Recompute the model-space matrix for every bone from the current
    /// local transforms + the skeleton's parent indices. Single forward
    /// sweep ; relies on the skeleton being topologically sorted.
    pub fn recompute_model_transforms(&mut self, skeleton: &Skeleton) {
        // Defensive resize : if local_transforms has been altered in a way
        // that diverges from skeleton.bone_count(), we still produce a
        // valid model-transforms array for the skeleton.
        if self.model_transforms.len() != skeleton.bone_count() {
            self.model_transforms
                .resize(skeleton.bone_count(), Mat4::IDENTITY);
        }
        if self.local_transforms.len() != skeleton.bone_count() {
            self.local_transforms
                .resize(skeleton.bone_count(), Transform::IDENTITY);
        }

        for (i, bone) in skeleton.bones().iter().enumerate() {
            let local = self.local_transforms[i].to_mat4();
            self.model_transforms[i] = if bone.parent_idx == ROOT_PARENT {
                local
            } else {
                self.model_transforms[bone.parent_idx].compose(local)
            };
        }
    }

    /// Compute the skinning matrix for one bone.
    /// `M_skin = M_model * M_bind^-1`.
    pub fn skinning_matrix(&self, skeleton: &Skeleton, bone_idx: usize) -> Result<Mat4, AnimError> {
        if bone_idx >= self.bone_count() {
            return Err(AnimError::BoneIndexOutOfRange {
                bone_idx,
                bone_count: self.bone_count(),
            });
        }
        let model = self.model_transforms[bone_idx];
        let ibm = skeleton
            .bone(bone_idx)
            .map_or(Mat4::IDENTITY, |b| b.inverse_bind_matrix);
        Ok(model.compose(ibm))
    }

    /// Bulk-emit the per-bone skinning matrices (one per skeleton bone,
    /// in skeleton order). Allocation-on-call ; the caller may reuse the
    /// buffer between frames by writing into [`Self::write_skinning_matrices`].
    #[must_use]
    pub fn compute_skinning_matrices(&self, skeleton: &Skeleton) -> Vec<Mat4> {
        let mut out = Vec::with_capacity(self.bone_count());
        for (i, b) in skeleton.bones().iter().enumerate() {
            let model = self
                .model_transforms
                .get(i)
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            out.push(model.compose(b.inverse_bind_matrix));
        }
        out
    }

    /// Allocation-free variant of [`Self::compute_skinning_matrices`] —
    /// writes into a caller-supplied buffer that the caller resizes (or
    /// that must already be `>= bone_count` long).
    pub fn write_skinning_matrices(&self, skeleton: &Skeleton, out: &mut [Mat4]) {
        let n = self.bone_count().min(out.len());
        for i in 0..n {
            let model = self.model_transforms[i];
            let ibm = skeleton
                .bone(i)
                .map_or(Mat4::IDENTITY, |b| b.inverse_bind_matrix);
            out[i] = model.compose(ibm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Pose;
    use crate::skeleton::{Bone, Skeleton, ROOT_PARENT};
    use crate::transform::Transform;
    use cssl_substrate_projections::{Mat4, Vec3};

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn make_chain() -> Skeleton {
        let bones = vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new(
                "b1",
                0,
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            ),
            Bone::new(
                "b2",
                1,
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            ),
        ];
        Skeleton::from_bones(bones).expect("ok")
    }

    #[test]
    fn pose_from_bind_initializes_local_to_bind() {
        let s = make_chain();
        let pose = Pose::from_bind_pose(&s);
        assert_eq!(pose.local_transforms.len(), 3);
        assert_eq!(pose.local_transforms[0], Transform::IDENTITY);
        // bone1 + bone2 each translate by +X.
        assert!(approx_eq(pose.local_transforms[1].translation.x, 1.0, 1e-6));
        assert!(approx_eq(pose.local_transforms[2].translation.x, 1.0, 1e-6));
    }

    #[test]
    fn pose_model_transforms_sum_translations_in_chain() {
        let s = make_chain();
        let pose = Pose::from_bind_pose(&s);
        // bone2 model-space translation must be +2 along X (root 0 + b1 1 + b2 1).
        let m2 = pose.model_transforms[2];
        assert!(approx_eq(m2.cols[3][0], 2.0, 1e-5));
    }

    #[test]
    fn recompute_after_local_change_updates_model() {
        let s = make_chain();
        let mut pose = Pose::from_bind_pose(&s);
        // Move bone1 to +Y instead of +X.
        pose.local_transforms[1] = Transform::from_translation(Vec3::new(0.0, 5.0, 0.0));
        pose.recompute_model_transforms(&s);
        let m2 = pose.model_transforms[2];
        // bone2 model-space should reflect bone1 at +5Y plus bone2 at +X relative.
        assert!(approx_eq(m2.cols[3][0], 1.0, 1e-5));
        assert!(approx_eq(m2.cols[3][1], 5.0, 1e-5));
    }

    #[test]
    fn skinning_matrix_for_identity_bind_is_model_matrix() {
        // For an identity-bind skeleton at the origin, the skinning matrix
        // equals the model matrix (since inverse-bind is identity).
        let bones = vec![
            Bone::new("a", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("b", 0, Transform::IDENTITY),
        ];
        let s = Skeleton::from_bones(bones).expect("ok");
        let pose = Pose::from_bind_pose(&s);
        let skin = pose.skinning_matrix(&s, 1).expect("bone 1 skinning matrix");
        assert_eq!(skin, Mat4::IDENTITY);
    }

    #[test]
    fn skinning_matrix_oob_errors() {
        let s = make_chain();
        let pose = Pose::from_bind_pose(&s);
        assert!(pose.skinning_matrix(&s, 99).is_err());
    }

    #[test]
    fn compute_skinning_matrices_returns_per_bone_array() {
        let s = make_chain();
        let pose = Pose::from_bind_pose(&s);
        let mats = pose.compute_skinning_matrices(&s);
        assert_eq!(mats.len(), s.bone_count());
    }

    #[test]
    fn write_skinning_matrices_fills_buffer() {
        let s = make_chain();
        let pose = Pose::from_bind_pose(&s);
        let mut buf = vec![Mat4::ZERO; s.bone_count()];
        pose.write_skinning_matrices(&s, &mut buf);
        for m in &buf {
            // Every entry should be non-zero (filled by the call).
            assert_ne!(*m, Mat4::ZERO);
        }
    }

    #[test]
    fn identity_pose_has_all_identity_locals() {
        let p = Pose::identity(5);
        assert_eq!(p.bone_count(), 5);
        for t in &p.local_transforms {
            assert_eq!(*t, Transform::IDENTITY);
        }
    }

    #[test]
    fn recompute_resizes_to_skeleton() {
        // Pose constructed with the wrong bone count : recompute must
        // resize to match the skeleton.
        let s = make_chain();
        let mut pose = Pose::identity(1); // wrong size
        pose.recompute_model_transforms(&s);
        assert_eq!(pose.local_transforms.len(), s.bone_count());
        assert_eq!(pose.model_transforms.len(), s.bone_count());
    }
}
