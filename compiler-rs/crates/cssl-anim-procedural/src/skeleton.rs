//! § ProceduralSkeleton — flat-array bone hierarchy for procedural rigs.
//!
//! § THESIS
//!   Layout-compatible with cssl-anim's `Skeleton`. The procedural
//!   surface evaluates poses on the same hierarchy ; the difference is
//!   that the procedural runtime drives bone-local transforms from a
//!   KAN-pose network rather than from authored keyframes.
//!
//! § DETERMINISM
//!   Construction sorts into topological (parent-before-child) order via
//!   stable Kahn's algorithm. Identical input bones produce identical
//!   skeletons across runs.

use cssl_substrate_projections::Mat4;

use crate::error::ProceduralAnimError;
use crate::transform::Transform;

/// Sentinel for "no parent" (root bone).
pub const ROOT_PARENT: usize = usize::MAX;

/// One bone in the procedural skeleton hierarchy.
#[derive(Debug, Clone)]
pub struct Bone {
    /// Human-readable name. Used for diagnostics + KAN-channel binding.
    pub name: String,
    /// Parent index ; [`ROOT_PARENT`] for roots.
    pub parent_idx: usize,
    /// Local-to-parent bind-pose transform.
    pub local_bind_transform: Transform,
    /// Inverse of the bind-pose model-space matrix. Pre-multiplied into
    /// the runtime model-space matrix to produce the skinning matrix.
    pub inverse_bind_matrix: Mat4,
    /// Bone-segment length (parent → tip). Used by the deformation
    /// surface to compute soft-body discretization. Defaults to `1.0` ;
    /// authoring tools typically set this from the bind-pose distance to
    /// the first child.
    pub segment_length: f32,
    /// Procedural-rig "stiffness" : how much wave-field forces deform
    /// this bone segment. `1.0` = fully rigid, `0.0` = pure soft-body.
    /// Defaults to `0.85` (mild deformation under typical wave-field
    /// pressure).
    pub stiffness: f32,
}

impl Bone {
    /// Construct a bone with explicit name + parent + bind transform. The
    /// inverse-bind-matrix is set to identity ; call
    /// [`ProceduralSkeleton::compute_inverse_binds`] after construction
    /// to populate it from the bind transforms.
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
            segment_length: 1.0,
            stiffness: 0.85,
        }
    }

    /// Builder : set the segment length explicitly.
    #[must_use]
    pub fn with_segment_length(mut self, length: f32) -> Self {
        self.segment_length = length.max(0.0);
        self
    }

    /// Builder : set the stiffness in `[0, 1]`.
    #[must_use]
    pub fn with_stiffness(mut self, stiffness: f32) -> Self {
        self.stiffness = stiffness.clamp(0.0, 1.0);
        self
    }

    /// Builder : pre-set the inverse-bind matrix.
    #[must_use]
    pub fn with_inverse_bind(mut self, ibm: Mat4) -> Self {
        self.inverse_bind_matrix = ibm;
        self
    }

    /// Whether this bone is a root.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.parent_idx == ROOT_PARENT
    }
}

/// A topologically-sorted bone hierarchy.
#[derive(Debug, Clone)]
pub struct ProceduralSkeleton {
    bones: Vec<Bone>,
}

impl ProceduralSkeleton {
    /// Construct from a list of bones. Validates parent indices, detects
    /// cycles, sorts into topological order, and pre-computes inverse-
    /// bind matrices.
    pub fn from_bones(bones: Vec<Bone>) -> Result<Self, ProceduralAnimError> {
        let count = bones.len();
        for (i, b) in bones.iter().enumerate() {
            if b.parent_idx != ROOT_PARENT && b.parent_idx >= count {
                return Err(ProceduralAnimError::BoneIndexOutOfRange {
                    bone_idx: b.parent_idx,
                    bone_count: count,
                });
            }
            if b.parent_idx == i {
                return Err(ProceduralAnimError::SkeletonCycle { start_idx: i });
            }
        }

        let mut order: Vec<usize> = Vec::with_capacity(count);
        let mut placed = vec![false; count];
        for (i, b) in bones.iter().enumerate() {
            if b.parent_idx == ROOT_PARENT {
                order.push(i);
                placed[i] = true;
            }
        }
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
                let unplaced = placed.iter().position(|&p| !p).unwrap_or(0);
                return Err(ProceduralAnimError::SkeletonCycle {
                    start_idx: unplaced,
                });
            }
        }

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

        // Compute inverse-bind matrices in a forward sweep.
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
            if sorted[i].inverse_bind_matrix == Mat4::IDENTITY {
                sorted[i].inverse_bind_matrix = invert_affine(model);
            }
        }

        Ok(Self { bones: sorted })
    }

    /// Total bone count.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    /// Single-bone read.
    #[must_use]
    pub fn bone(&self, idx: usize) -> Option<&Bone> {
        self.bones.get(idx)
    }

    /// Mutable single-bone read.
    pub fn bone_mut(&mut self, idx: usize) -> Option<&mut Bone> {
        self.bones.get_mut(idx)
    }

    /// Whole bone slice.
    #[must_use]
    pub fn bones(&self) -> &[Bone] {
        &self.bones
    }

    /// Locate a bone by name in `O(N)`.
    #[must_use]
    pub fn find_bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// Recompute the inverse-bind matrices from current
    /// `local_bind_transform` values.
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

    /// Walk parent → child to produce a vector of (idx, parent_idx) pairs
    /// in pose-evaluation order. Roots come first.
    pub fn iter_topo(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.bones
            .iter()
            .enumerate()
            .map(|(i, b)| (i, b.parent_idx))
    }

    /// Expand a chain of bones from `start` (root-side) to `end` (leaf-side)
    /// inclusive. Used by IK chain construction. Returns
    /// `Err(BoneIndexOutOfRange)` if either index is out of range.
    pub fn chain_from_to(
        &self,
        start: usize,
        end: usize,
    ) -> Result<Vec<usize>, ProceduralAnimError> {
        let count = self.bones.len();
        if start >= count {
            return Err(ProceduralAnimError::BoneIndexOutOfRange {
                bone_idx: start,
                bone_count: count,
            });
        }
        if end >= count {
            return Err(ProceduralAnimError::BoneIndexOutOfRange {
                bone_idx: end,
                bone_count: count,
            });
        }
        // Walk parent-pointers from end back to start ; reverse to produce
        // start-to-end ordering.
        let mut chain: Vec<usize> = Vec::new();
        let mut cur = end;
        loop {
            chain.push(cur);
            if cur == start {
                break;
            }
            let parent = self.bones[cur].parent_idx;
            if parent == ROOT_PARENT {
                // Walked past start without finding it ; the caller asked
                // for a chain that doesn't exist along the parent path.
                return Err(ProceduralAnimError::BoneIndexOutOfRange {
                    bone_idx: start,
                    bone_count: count,
                });
            }
            cur = parent;
        }
        chain.reverse();
        Ok(chain)
    }
}

/// Invert an affine 4x4 matrix (assumes bottom-row `(0, 0, 0, 1)`). Returns
/// identity for singular matrices to maintain totality.
fn invert_affine(m: Mat4) -> Mat4 {
    let a = m.cols[0][0];
    let b = m.cols[1][0];
    let c = m.cols[2][0];
    let d = m.cols[0][1];
    let e = m.cols[1][1];
    let f = m.cols[2][1];
    let g = m.cols[0][2];
    let h = m.cols[1][2];
    let i = m.cols[2][2];

    let c0 = e * i - f * h;
    let c1 = -(d * i - f * g);
    let c2 = d * h - e * g;
    let det = a * c0 + b * c1 + c * c2;
    if det.abs() < f32::EPSILON {
        return Mat4::IDENTITY;
    }
    let inv_det = det.recip();

    let i00 = c0 * inv_det;
    let i10 = c1 * inv_det;
    let i20 = c2 * inv_det;
    let i01 = -(b * i - c * h) * inv_det;
    let i11 = (a * i - c * g) * inv_det;
    let i21 = -(a * h - b * g) * inv_det;
    let i02 = (b * f - c * e) * inv_det;
    let i12 = -(a * f - c * d) * inv_det;
    let i22 = (a * e - b * d) * inv_det;

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
    use super::*;
    use cssl_substrate_projections::Vec3;

    fn make_chain() -> ProceduralSkeleton {
        let bones = vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new(
                "spine",
                0,
                Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
            ),
            Bone::new(
                "head",
                1,
                Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
            ),
        ];
        ProceduralSkeleton::from_bones(bones).expect("chain must build")
    }

    #[test]
    fn chain_bone_count() {
        let s = make_chain();
        assert_eq!(s.bone_count(), 3);
    }

    #[test]
    fn topological_order_holds() {
        let s = make_chain();
        for (i, b) in s.bones().iter().enumerate() {
            if !b.is_root() {
                assert!(b.parent_idx < i);
            }
        }
    }

    #[test]
    fn cycle_rejected() {
        let bones = vec![
            Bone::new("a", 1, Transform::IDENTITY),
            Bone::new("b", 0, Transform::IDENTITY),
        ];
        assert!(matches!(
            ProceduralSkeleton::from_bones(bones),
            Err(ProceduralAnimError::SkeletonCycle { .. })
        ));
    }

    #[test]
    fn self_loop_rejected_as_cycle() {
        let bones = vec![Bone::new("self", 0, Transform::IDENTITY)];
        assert!(matches!(
            ProceduralSkeleton::from_bones(bones),
            Err(ProceduralAnimError::SkeletonCycle { .. })
        ));
    }

    #[test]
    fn out_of_range_parent_rejected() {
        let bones = vec![Bone::new("oob", 99, Transform::IDENTITY)];
        assert!(matches!(
            ProceduralSkeleton::from_bones(bones),
            Err(ProceduralAnimError::BoneIndexOutOfRange { .. })
        ));
    }

    #[test]
    fn empty_skeleton_legal() {
        let s = ProceduralSkeleton::from_bones(vec![]).expect("empty allowed");
        assert_eq!(s.bone_count(), 0);
    }

    #[test]
    fn find_bone_by_name() {
        let s = make_chain();
        assert_eq!(s.find_bone("spine"), Some(1));
        assert_eq!(s.find_bone("nonexistent"), None);
    }

    #[test]
    fn chain_from_to_walks_parent_pointers() {
        let s = make_chain();
        let chain = s.chain_from_to(0, 2).expect("chain valid");
        assert_eq!(chain, vec![0, 1, 2]);
    }

    #[test]
    fn chain_from_to_oob_rejected() {
        let s = make_chain();
        let err = s.chain_from_to(0, 99).unwrap_err();
        assert!(matches!(
            err,
            ProceduralAnimError::BoneIndexOutOfRange { .. }
        ));
    }

    #[test]
    fn segment_length_default() {
        let b = Bone::new("a", ROOT_PARENT, Transform::IDENTITY);
        assert_eq!(b.segment_length, 1.0);
    }

    #[test]
    fn stiffness_default_is_partial() {
        let b = Bone::new("a", ROOT_PARENT, Transform::IDENTITY);
        assert!(b.stiffness > 0.0);
        assert!(b.stiffness < 1.0);
    }

    #[test]
    fn stiffness_clamps_above_one() {
        let b = Bone::new("a", ROOT_PARENT, Transform::IDENTITY).with_stiffness(2.0);
        assert_eq!(b.stiffness, 1.0);
    }

    #[test]
    fn stiffness_clamps_below_zero() {
        let b = Bone::new("a", ROOT_PARENT, Transform::IDENTITY).with_stiffness(-1.0);
        assert_eq!(b.stiffness, 0.0);
    }

    #[test]
    fn segment_length_clamps_negative_to_zero() {
        let b = Bone::new("a", ROOT_PARENT, Transform::IDENTITY).with_segment_length(-5.0);
        assert_eq!(b.segment_length, 0.0);
    }

    #[test]
    fn iter_topo_yields_in_order() {
        let s = make_chain();
        let collected: Vec<(usize, usize)> = s.iter_topo().collect();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0].1, ROOT_PARENT);
    }
}
