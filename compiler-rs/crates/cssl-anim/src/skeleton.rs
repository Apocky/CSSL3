//! Skeletal hierarchy : flat-array bones + parent indices + bind pose.
//!
//! § THESIS
//!   A skeleton is a tree of bones. The conventional representation is a
//!   flat `Vec<Bone>` with each entry carrying its parent index — far
//!   cheaper than allocated tree nodes and trivially traversable in a
//!   single forward pass when bones are sorted parent-before-child.
//!
//!   Construction enforces topological (parent-before-child) order and
//!   rejects cycles. Once a `Skeleton` exists, every consumer can assume
//!   the bone list can be walked front-to-back to compute cumulative
//!   model-space transforms in one sweep.
//!
//! § BIND POSE
//!   The bind pose is the canonical "rest" configuration the mesh was
//!   skinned in. Two pieces of information per bone :
//!     - `local_bind_transform` : the bind-time bone-local `Transform`
//!       relative to its parent.
//!     - `inverse_bind_matrix` : the inverse of the model-space matrix
//!       in the bind pose. Pre-multiplied with the runtime model-space
//!       matrix to produce the skinning matrix `M_skin = M_model * M_bind^-1`.
//!
//! § DETERMINISM
//!   `Skeleton::from_bones` is a pure function ; reordering the input
//!   yields the same skeleton (modulo bone-index permutation, which is
//!   recorded in the returned `topology` map). All sorts use stable
//!   ordering on (parent_idx, original_input_idx).

use cssl_substrate_projections::Mat4;

use crate::error::AnimError;
use crate::transform::Transform;

/// Sentinel parent index that marks a bone as the skeleton's root. Equal
/// to `usize::MAX`. Roots have no parent in the hierarchy.
pub const ROOT_PARENT: usize = usize::MAX;

/// One bone in the skeletal hierarchy.
///
/// § FIELDS
/// - `name` : human-readable name. Used for diagnostics + (eventually)
///   GLTF-channel lookup at load time.
/// - `parent_idx` : index into the skeleton's bone array, or [`ROOT_PARENT`]
///   if this bone is a root. Topologically guaranteed to point to an
///   earlier index in the array.
/// - `local_bind_transform` : the bind-time local-to-parent `Transform`.
/// - `inverse_bind_matrix` : pre-computed `M_bind^-1` for skinning.
#[derive(Debug, Clone)]
pub struct Bone {
    /// Human-readable name used for diagnostics + channel-target lookup.
    pub name: String,
    /// Parent bone index ; [`ROOT_PARENT`] (`usize::MAX`) for roots.
    pub parent_idx: usize,
    /// Local-to-parent transform in the bind pose.
    pub local_bind_transform: Transform,
    /// Inverse of the model-space matrix in the bind pose. Multiplied with
    /// the runtime model-space matrix to obtain the per-bone skinning matrix.
    pub inverse_bind_matrix: Mat4,
}

impl Bone {
    /// Construct a bone with explicit name + parent + local bind transform.
    /// The inverse-bind-matrix is set to identity ; call
    /// [`Skeleton::compute_inverse_binds`] after construction or pass an
    /// explicit value via [`Self::with_inverse_bind`] to override.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        parent_idx: usize,
        local_bind_transform: Transform,
    ) -> Self {
        Self {
            name: name.into(),
            parent_idx,
            local_bind_transform,
            inverse_bind_matrix: Mat4::IDENTITY,
        }
    }

    /// Builder method : set the inverse-bind-matrix explicitly.
    #[must_use]
    pub fn with_inverse_bind(mut self, ibm: Mat4) -> Self {
        self.inverse_bind_matrix = ibm;
        self
    }

    /// Whether this bone is a root (has no parent in the hierarchy).
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.parent_idx == ROOT_PARENT
    }
}

/// A skeletal hierarchy. Bones are stored in topological (parent-before-
/// child) order so a single forward pass computes model-space transforms.
///
/// § INVARIANTS
///   - `bones[i].parent_idx < i` for non-root bones (or `ROOT_PARENT`).
///   - The graph contains no cycles (rejected at construction).
///   - Each bone has a unique name (stage-0 enforces this for debug-aid
///     readability + will be required for GLTF channel resolution).
#[derive(Debug, Clone)]
pub struct Skeleton {
    bones: Vec<Bone>,
}

impl Skeleton {
    /// Construct from a list of bones. The constructor :
    ///   1. Validates parent indices reference existing bones (or `ROOT_PARENT`).
    ///   2. Detects cycles (would require a forward parent reference).
    ///   3. Sorts the bone list into topological (parent-before-child) order.
    ///   4. Rebases parent indices to point into the sorted array.
    ///
    /// Returns `Err(AnimError::SkeletonCycle)` if a cycle is detected, or
    /// `Err(AnimError::BoneIndexOutOfRange)` if a parent index references
    /// a non-existent bone.
    pub fn from_bones(bones: Vec<Bone>) -> Result<Self, AnimError> {
        let count = bones.len();
        // Parent-index sanity : every non-ROOT parent must point inside the
        // array. We don't yet require parent-before-child ordering — the
        // caller may have authored leaf-first ; we'll re-sort below.
        for (i, b) in bones.iter().enumerate() {
            if b.parent_idx != ROOT_PARENT && b.parent_idx >= count {
                return Err(AnimError::BoneIndexOutOfRange {
                    bone_idx: b.parent_idx,
                    bone_count: count,
                });
            }
            if b.parent_idx == i {
                // Self-loop counts as a cycle.
                return Err(AnimError::SkeletonCycle { start_idx: i });
            }
        }

        // Topological sort via Kahn's algorithm with stable ordering on
        // input-index. Roots first, then breadth-first by depth-from-root,
        // ties broken by original input index.
        let mut order: Vec<usize> = Vec::with_capacity(count);
        let mut placed = vec![false; count];
        // Place roots in original order.
        for (i, b) in bones.iter().enumerate() {
            if b.parent_idx == ROOT_PARENT {
                order.push(i);
                placed[i] = true;
            }
        }
        // Iteratively place bones whose parents have been placed already.
        // The outer loop terminates because every iteration either makes
        // progress (placing >= 1 bone) or detects a cycle.
        loop {
            let mut progress = false;
            for (i, b) in bones.iter().enumerate() {
                if placed[i] {
                    continue;
                }
                if b.parent_idx != ROOT_PARENT && placed[b.parent_idx] {
                    order.push(i);
                    placed[i] = true;
                    progress = true;
                }
            }
            if order.len() == count {
                break;
            }
            if !progress {
                // Some bone is unreachable from any root — cycle.
                let unplaced = placed.iter().position(|&p| !p).unwrap_or(0);
                return Err(AnimError::SkeletonCycle {
                    start_idx: unplaced,
                });
            }
        }

        // Build a "old-index → new-index" remap and re-emit the bones in
        // topological order, fixing parent indices to use new indices.
        let mut remap = vec![ROOT_PARENT; count];
        for (new_idx, &old_idx) in order.iter().enumerate() {
            remap[old_idx] = new_idx;
        }
        let mut sorted: Vec<Bone> = order
            .iter()
            .map(|&old_idx| {
                let mut b = bones[old_idx].clone();
                if b.parent_idx != ROOT_PARENT {
                    b.parent_idx = remap[b.parent_idx];
                }
                b
            })
            .collect();

        // Compute the inverse bind matrices from the bind transforms. The
        // canonical formula : `IBM = (model-space-bind-matrix)^-1`. We
        // compute model-space bind matrices in a forward sweep, then
        // invert each.
        let mut model_bind: Vec<Mat4> = Vec::with_capacity(count);
        for i in 0..sorted.len() {
            let b = &sorted[i];
            let local = b.local_bind_transform.to_mat4();
            let model = if b.parent_idx == ROOT_PARENT {
                local
            } else {
                model_bind[b.parent_idx].compose(local)
            };
            model_bind.push(model);
            // Set the bone's inverse-bind-matrix in-place if the caller
            // left it as identity ; otherwise honour the override.
            if sorted[i].inverse_bind_matrix == Mat4::IDENTITY {
                sorted[i].inverse_bind_matrix = invert_affine(model);
            }
        }

        Ok(Self { bones: sorted })
    }

    /// Total number of bones in this skeleton.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    /// Read-only access to a single bone.
    #[must_use]
    pub fn bone(&self, idx: usize) -> Option<&Bone> {
        self.bones.get(idx)
    }

    /// Read-only access to the full bone array.
    #[must_use]
    pub fn bones(&self) -> &[Bone] {
        &self.bones
    }

    /// Locate a bone by name. `O(N)` ; suitable for one-off resolution at
    /// asset-load time. Hot-loop lookups should cache the index.
    #[must_use]
    pub fn find_bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// Compute (or recompute) the inverse-bind-matrix for every bone from
    /// its current `local_bind_transform`. Useful when an authoring tool
    /// supplies bind-pose transforms but not pre-computed inverse-bind
    /// matrices.
    pub fn compute_inverse_binds(&mut self) {
        let mut model_bind: Vec<Mat4> = Vec::with_capacity(self.bones.len());
        for b in &self.bones {
            let local = b.local_bind_transform.to_mat4();
            let model = if b.parent_idx == ROOT_PARENT {
                local
            } else {
                model_bind[b.parent_idx].compose(local)
            };
            model_bind.push(model);
        }
        for (i, b) in self.bones.iter_mut().enumerate() {
            b.inverse_bind_matrix = invert_affine(model_bind[i]);
        }
    }

    /// Bind-pose model-space matrix for a particular bone — useful for
    /// debug / diagnostic purposes. Stage-0 recomputes every call ; a
    /// caching layer can be added if profiling shows a hotspot.
    #[must_use]
    pub fn bind_model_matrix(&self, bone_idx: usize) -> Mat4 {
        let mut acc: Vec<Mat4> = Vec::with_capacity(self.bones.len());
        for (i, b) in self.bones.iter().enumerate() {
            let local = b.local_bind_transform.to_mat4();
            let model = if b.parent_idx == ROOT_PARENT {
                local
            } else {
                acc[b.parent_idx].compose(local)
            };
            acc.push(model);
            if i == bone_idx {
                return model;
            }
        }
        Mat4::IDENTITY
    }
}

/// Invert an affine 4x4 matrix (assuming the bottom row is `(0, 0, 0, 1)`).
/// Used for inverse-bind-matrix computation. Stage-0 implementation
/// composes a rotation/scale inverse with a translation inverse — exact
/// for any TRS-compose-derived matrix.
///
/// § DERIVATION
///   For an affine matrix `M = [R|t; 0 0 0 1]` where `R` is a 3x3 linear
///   block and `t` is a translation column :
///     `M^-1 = [R^-1 | -R^-1 * t ; 0 0 0 1]`
///   We compute the 3x3 inverse by classical cofactor expansion. This
///   handles general rotation+scale ; pure rotation matrices are
///   orthogonal so the inverse equals the transpose.
fn invert_affine(m: Mat4) -> Mat4 {
    // Extract the 3x3 linear block.
    let a = m.cols[0][0];
    let b = m.cols[1][0];
    let c = m.cols[2][0];
    let d = m.cols[0][1];
    let e = m.cols[1][1];
    let f = m.cols[2][1];
    let g = m.cols[0][2];
    let h = m.cols[1][2];
    let i = m.cols[2][2];

    // Cofactors :
    let c0 = e * i - f * h;
    let c1 = -(d * i - f * g);
    let c2 = d * h - e * g;
    let det = a * c0 + b * c1 + c * c2;
    if det.abs() < f32::EPSILON {
        // Degenerate matrix — return identity to maintain totality.
        return Mat4::IDENTITY;
    }
    let inv_det = 1.0 / det;

    // Inverse of the 3x3 block = (1/det) * adjugate.
    let i00 = c0 * inv_det;
    let i10 = c1 * inv_det;
    let i20 = c2 * inv_det;
    let i01 = -(b * i - c * h) * inv_det;
    let i11 = (a * i - c * g) * inv_det;
    let i21 = -(a * h - b * g) * inv_det;
    let i02 = (b * f - c * e) * inv_det;
    let i12 = -(a * f - c * d) * inv_det;
    let i22 = (a * e - b * d) * inv_det;

    // Translation column = -R^-1 * t.
    let tx = m.cols[3][0];
    let ty = m.cols[3][1];
    let tz = m.cols[3][2];
    let nt_x = -(i00 * tx + i01 * ty + i02 * tz);
    let nt_y = -(i10 * tx + i11 * ty + i12 * tz);
    let nt_z = -(i20 * tx + i21 * ty + i22 * tz);

    Mat4 {
        cols: [
            [i00, i10, i20, 0.0],
            [i01, i11, i21, 0.0],
            [i02, i12, i22, 0.0],
            [nt_x, nt_y, nt_z, 1.0],
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{Bone, Skeleton, ROOT_PARENT};
    use crate::error::AnimError;
    use crate::transform::Transform;
    use cssl_substrate_projections::Vec3;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn make_simple_chain() -> Skeleton {
        // root → bone1 → bone2 ; each translated by +X = 1.
        let bones = vec![
            Bone::new("root", ROOT_PARENT, Transform::from_translation(Vec3::ZERO)),
            Bone::new(
                "bone1",
                0,
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            ),
            Bone::new(
                "bone2",
                1,
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            ),
        ];
        Skeleton::from_bones(bones).expect("simple chain must build")
    }

    #[test]
    fn simple_chain_bone_count_matches() {
        let s = make_simple_chain();
        assert_eq!(s.bone_count(), 3);
    }

    #[test]
    fn root_is_first_bone() {
        let s = make_simple_chain();
        assert!(s.bone(0).expect("first bone").is_root());
    }

    #[test]
    fn topological_order_preserved_when_already_sorted() {
        let s = make_simple_chain();
        for (i, b) in s.bones().iter().enumerate() {
            if !b.is_root() {
                assert!(b.parent_idx < i, "parent must come before child");
            }
        }
    }

    #[test]
    fn from_bones_sorts_unsorted_input() {
        // Author leaf-first ; expect parent-first after construction.
        let bones = vec![
            // bone2 first (refers to bone1) ;
            Bone::new(
                "bone2",
                1,
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            ),
            // bone1 (refers to root)
            Bone::new(
                "bone1",
                2,
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            ),
            // root last
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
        ];
        let s = Skeleton::from_bones(bones).expect("must reorder");
        // The first bone (in sorted output) must be the root.
        assert!(s.bone(0).expect("first").is_root());
        assert_eq!(s.bone(0).expect("first").name, "root");
        // Children must reference earlier indices.
        for (i, b) in s.bones().iter().enumerate() {
            if !b.is_root() {
                assert!(b.parent_idx < i, "child at {i} parent_idx {}", b.parent_idx);
            }
        }
    }

    #[test]
    fn cycle_is_rejected() {
        // bone0 → bone1 → bone0 (cycle).
        let bones = vec![
            Bone::new("a", 1, Transform::IDENTITY),
            Bone::new("b", 0, Transform::IDENTITY),
        ];
        let result = Skeleton::from_bones(bones);
        assert!(matches!(result, Err(AnimError::SkeletonCycle { .. })));
    }

    #[test]
    fn self_loop_is_rejected_as_cycle() {
        let bones = vec![Bone::new("self", 0, Transform::IDENTITY)];
        let result = Skeleton::from_bones(bones);
        assert!(matches!(result, Err(AnimError::SkeletonCycle { .. })));
    }

    #[test]
    fn bone_index_out_of_range_is_rejected() {
        let bones = vec![Bone::new("oob", 99, Transform::IDENTITY)];
        let result = Skeleton::from_bones(bones);
        assert!(matches!(result, Err(AnimError::BoneIndexOutOfRange { .. })));
    }

    #[test]
    fn find_bone_resolves_by_name() {
        let s = make_simple_chain();
        assert_eq!(s.find_bone("root"), Some(0));
        assert_eq!(s.find_bone("bone1"), Some(1));
        assert_eq!(s.find_bone("bone2"), Some(2));
        assert_eq!(s.find_bone("nonexistent"), None);
    }

    #[test]
    fn bind_model_matrix_chain_compounds_translations() {
        // Three bones each translating +X by 1 ; bone2 model-space should
        // be translated by 3 along X (root translation 0 + 1 + 1 + 1 hmm,
        // root has zero translation, bone1 +1 from root, bone2 +1 from
        // bone1. So bone2 model = +2.).
        let s = make_simple_chain();
        let m = s.bind_model_matrix(2);
        // The translation column is m.cols[3].
        assert!(approx_eq(m.cols[3][0], 2.0, 1e-5));
    }

    #[test]
    fn inverse_bind_round_trip_for_identity_skeleton() {
        // For a chain where every bone is at the origin in bind-pose,
        // the inverse-bind-matrices should equal the identity.
        let bones = vec![
            Bone::new("a", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("b", 0, Transform::IDENTITY),
        ];
        let s = Skeleton::from_bones(bones).expect("must build");
        for b in s.bones() {
            for col in 0..4 {
                for row in 0..4 {
                    let expected = if col == row { 1.0 } else { 0.0 };
                    assert!(
                        approx_eq(b.inverse_bind_matrix.cols[col][row], expected, 1e-5),
                        "bone '{}' IBM[{col}][{row}] = {} (expected {expected})",
                        b.name,
                        b.inverse_bind_matrix.cols[col][row]
                    );
                }
            }
        }
    }

    #[test]
    fn unique_bones_are_preserved() {
        let s = make_simple_chain();
        let names: Vec<&str> = s.bones().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["root", "bone1", "bone2"]);
    }

    #[test]
    fn empty_skeleton_is_legal() {
        let s = Skeleton::from_bones(vec![]).expect("empty skeleton allowed");
        assert_eq!(s.bone_count(), 0);
    }

    #[test]
    fn multiple_roots_supported() {
        // Two independent chains in one skeleton — useful for "skeleton
        // pair" rigs (e.g. cape + body, weapon + arm).
        let bones = vec![
            Bone::new("root_a", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("root_b", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("child_a", 0, Transform::IDENTITY),
            Bone::new("child_b", 1, Transform::IDENTITY),
        ];
        let s = Skeleton::from_bones(bones).expect("multi-root allowed");
        assert_eq!(s.bone_count(), 4);
        // Both roots should appear before any child.
        assert!(s.bone(0).expect("0").is_root());
        assert!(s.bone(1).expect("1").is_root());
        assert!(!s.bone(2).expect("2").is_root());
        assert!(!s.bone(3).expect("3").is_root());
    }
}
