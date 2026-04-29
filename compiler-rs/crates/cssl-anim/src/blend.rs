//! `BlendTree` — composite multiple animation clips into a single pose.
//!
//! § THESIS
//!   Blend trees are the standard runtime form for runtime-driven
//!   animation : leaf nodes are clips, interior nodes weight multiple
//!   inputs. The output is a single `Pose` that combines all the
//!   weighted contributions.
//!
//! § NODE TYPES
//!   - `Clip { handle, time }` : evaluate one clip at the given time.
//!     This is the leaf of every blend tree.
//!   - `Blend2 { a, b, weight }` : weighted average between two child
//!     nodes. `weight` in `[0, 1]` controls the blend ; 0 = pure A,
//!     1 = pure B.
//!   - `AdditiveBlend { base, additive, weight }` : add `additive`'s
//!     transform delta on top of `base`'s pose. Used for sub-animations
//!     like "lean left" layered on top of "running".
//!   - `BlendN { children, weights }` : N-way weighted average. Weights
//!     are normalized at evaluation time.
//!
//! § DETERMINISM
//!   Evaluation is a pure function of `(tree, weights, sample_time, clips)`.
//!   No internal state is mutated during evaluation.
//!
//! § STAGE-0 LIMITATIONS
//!   - 1D / 2D blend-spaces (the typical "locomotion" blend grid) are
//!     deferred ; stage-0 surfaces a `BlendN` form that the caller can
//!     drive directly with computed weights.
//!   - Time-warp (per-clip phase offset) is supported per-clip via the
//!     leaf `time` field but the higher-level "synchronized walk/run"
//!     blend pattern lives in a follow-up slice.

use cssl_substrate_projections::{Quat, Vec3};

use crate::clip::AnimationClip;
use crate::error::AnimError;
use crate::pose::Pose;
use crate::sampler::AnimSampler;
use crate::skeleton::Skeleton;
use crate::transform::{slerp, Transform};

/// Stable handle into a blend-tree's clip array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClipHandle(pub u32);

/// One node in a blend tree.
#[derive(Debug, Clone)]
pub enum BlendNode {
    /// Leaf node — sample a single clip at a single time.
    Clip {
        /// Handle into the blend tree's clip array.
        handle: ClipHandle,
        /// Sample time, in seconds. Wraps via `AnimationClip::wrap_time`
        /// at evaluation time.
        time: f32,
    },
    /// Two-way weighted blend.
    Blend2 {
        /// Left child node index in the tree's `nodes` array.
        a: usize,
        /// Right child node index in the tree's `nodes` array.
        b: usize,
        /// Blend weight `[0, 1]` ; 0 = all A, 1 = all B.
        weight: f32,
    },
    /// Additive blend — add an "additive" pose's delta on top of a base.
    /// Useful for sub-animations like "lean" or "head tracking" layered
    /// onto a base locomotion clip.
    AdditiveBlend {
        /// Base pose source.
        base: usize,
        /// Additive pose source. Its delta from bind-pose is added.
        additive: usize,
        /// Strength of the additive layer `[0, 1]`.
        weight: f32,
    },
    /// N-way weighted blend with explicit weights. Weights are
    /// normalized at evaluation time so the caller need not pre-normalize.
    BlendN {
        /// Child node indices in the tree's `nodes` array.
        children: Vec<usize>,
        /// Per-child weights ; must be the same length as `children`.
        weights: Vec<f32>,
    },
}

/// Errors specific to blend tree construction + evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum BlendTreeError {
    /// Generic malformation : wraps an `AnimError::BlendTreeMalformed`.
    Malformed(String),
    /// Wrapped general animation error.
    Anim(AnimError),
}

impl From<AnimError> for BlendTreeError {
    fn from(e: AnimError) -> Self {
        Self::Anim(e)
    }
}

impl std::fmt::Display for BlendTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(s) => write!(f, "BlendTree malformed: {s}"),
            Self::Anim(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BlendTreeError {}

/// A full blend tree : node-graph + clip-handle table + root index.
#[derive(Debug, Clone)]
pub struct BlendTree {
    /// All nodes in the tree, in arbitrary order. Children reference
    /// other nodes by index.
    pub nodes: Vec<BlendNode>,
    /// The root node index (the entry point for evaluation). Defaults to 0.
    pub root: usize,
    /// Clip table, indexed by `ClipHandle`. Stage-0 stores clips
    /// directly ; production builds may want a handle into a global
    /// `ClipPool` instead.
    pub clips: Vec<AnimationClip>,
}

impl BlendTree {
    /// Construct an empty blend tree.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: 0,
            clips: Vec::new(),
        }
    }

    /// Register a clip and return its handle.
    pub fn add_clip(&mut self, clip: AnimationClip) -> ClipHandle {
        let h = ClipHandle(self.clips.len() as u32);
        self.clips.push(clip);
        h
    }

    /// Append a node and return its index.
    pub fn add_node(&mut self, node: BlendNode) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        idx
    }

    /// Set the root node index.
    pub fn set_root(&mut self, idx: usize) {
        self.root = idx;
    }

    /// Validate the tree structure : every child reference points inside
    /// the array, every clip handle resolves, every blend weight is in
    /// `[0, 1]` (for 2-way / additive ; the N-way path normalizes).
    pub fn validate(&self) -> Result<(), BlendTreeError> {
        let n = self.nodes.len();
        if n == 0 {
            return Err(BlendTreeError::Malformed("empty tree".into()));
        }
        if self.root >= n {
            return Err(BlendTreeError::Malformed(format!(
                "root {} out of range (have {n} nodes)",
                self.root
            )));
        }
        for (i, node) in self.nodes.iter().enumerate() {
            match node {
                BlendNode::Clip { handle, .. } => {
                    if handle.0 as usize >= self.clips.len() {
                        return Err(BlendTreeError::Malformed(format!(
                            "node {i} references clip handle {} ; only {} clips registered",
                            handle.0,
                            self.clips.len()
                        )));
                    }
                }
                BlendNode::Blend2 { a, b, weight } => {
                    if *a >= n || *b >= n {
                        return Err(BlendTreeError::Malformed(format!(
                            "node {i} blend2 children ({a}, {b}) out of range"
                        )));
                    }
                    if !(0.0..=1.0).contains(weight) {
                        return Err(BlendTreeError::Anim(AnimError::WeightOutOfRange {
                            weight: *weight,
                        }));
                    }
                }
                BlendNode::AdditiveBlend {
                    base,
                    additive,
                    weight,
                } => {
                    if *base >= n || *additive >= n {
                        return Err(BlendTreeError::Malformed(format!(
                            "node {i} additive children ({base}, {additive}) out of range"
                        )));
                    }
                    if !(0.0..=1.0).contains(weight) {
                        return Err(BlendTreeError::Anim(AnimError::WeightOutOfRange {
                            weight: *weight,
                        }));
                    }
                }
                BlendNode::BlendN { children, weights } => {
                    if children.len() != weights.len() {
                        return Err(BlendTreeError::Malformed(format!(
                            "node {i} BlendN child/weight length mismatch ({} vs {})",
                            children.len(),
                            weights.len()
                        )));
                    }
                    for c in children {
                        if *c >= n {
                            return Err(BlendTreeError::Malformed(format!(
                                "node {i} BlendN child {c} out of range"
                            )));
                        }
                    }
                    for w in weights {
                        if !w.is_finite() || *w < 0.0 {
                            return Err(BlendTreeError::Anim(AnimError::WeightOutOfRange {
                                weight: *w,
                            }));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Evaluate the tree into the target pose. The pose is overwritten
    /// with the result. The provided sampler determines how each leaf
    /// clip is sampled.
    pub fn evaluate(
        &self,
        skeleton: &Skeleton,
        sampler: &AnimSampler,
        out_pose: &mut Pose,
    ) -> Result<(), BlendTreeError> {
        self.validate()?;
        let result = self.eval_node(self.root, skeleton, sampler)?;
        *out_pose = result;
        out_pose.recompute_model_transforms(skeleton);
        Ok(())
    }

    /// Recursive evaluation helper. Returns a fresh pose ; the public
    /// `evaluate` wraps the recursion + writes into the caller's buffer.
    fn eval_node(
        &self,
        idx: usize,
        skeleton: &Skeleton,
        sampler: &AnimSampler,
    ) -> Result<Pose, BlendTreeError> {
        match &self.nodes[idx] {
            BlendNode::Clip { handle, time } => {
                let clip = &self.clips[handle.0 as usize];
                let mut p = Pose::from_bind_pose(skeleton);
                let wrapped = clip.wrap_time(*time);
                sampler.sample(clip, wrapped, skeleton, &mut p)?;
                Ok(p)
            }
            BlendNode::Blend2 { a, b, weight } => {
                let pa = self.eval_node(*a, skeleton, sampler)?;
                let pb = self.eval_node(*b, skeleton, sampler)?;
                Ok(blend_two_poses(&pa, &pb, *weight, skeleton))
            }
            BlendNode::AdditiveBlend {
                base,
                additive,
                weight,
            } => {
                let pb = self.eval_node(*base, skeleton, sampler)?;
                let pa = self.eval_node(*additive, skeleton, sampler)?;
                Ok(blend_additive(&pb, &pa, *weight, skeleton))
            }
            BlendNode::BlendN { children, weights } => {
                if children.is_empty() {
                    return Ok(Pose::from_bind_pose(skeleton));
                }
                // Normalize weights ; if all zero, fall back to uniform.
                let sum: f32 = weights.iter().copied().sum();
                let normalized: Vec<f32> = if sum.abs() < f32::EPSILON {
                    let n = weights.len() as f32;
                    weights.iter().map(|_| 1.0 / n).collect()
                } else {
                    weights.iter().map(|w| w / sum).collect()
                };
                // Evaluate children and blend N ways.
                let mut child_poses: Vec<Pose> = Vec::with_capacity(children.len());
                for c in children {
                    child_poses.push(self.eval_node(*c, skeleton, sampler)?);
                }
                Ok(blend_n_poses(&child_poses, &normalized, skeleton))
            }
        }
    }
}

impl Default for BlendTree {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Blending primitives ────────────────────────────────────────────────

/// Blend two poses : `(1 - w) * a + w * b`.
fn blend_two_poses(a: &Pose, b: &Pose, w: f32, skeleton: &Skeleton) -> Pose {
    let n = skeleton.bone_count();
    let mut out = Pose::identity(n);
    for i in 0..n {
        let ta = *a.local_transforms.get(i).unwrap_or(&Transform::IDENTITY);
        let tb = *b.local_transforms.get(i).unwrap_or(&Transform::IDENTITY);
        out.local_transforms[i] = ta.interpolate(tb, w);
    }
    out
}

/// Additive blend : compute the delta of `additive` from bind-pose, scale
/// it by `weight`, apply it on top of `base`. The "delta" is :
/// translation difference, rotation `bind^-1 * additive`, scale ratio.
fn blend_additive(base: &Pose, additive: &Pose, weight: f32, skeleton: &Skeleton) -> Pose {
    let n = skeleton.bone_count();
    let mut out = Pose::identity(n);
    for i in 0..n {
        let bind = skeleton
            .bone(i)
            .map_or(Transform::IDENTITY, |b| b.local_bind_transform);
        let tb = *base.local_transforms.get(i).unwrap_or(&Transform::IDENTITY);
        let ta = *additive
            .local_transforms
            .get(i)
            .unwrap_or(&Transform::IDENTITY);
        // Compute the additive delta (relative to bind).
        let delta_t = Vec3::new(
            ta.translation.x - bind.translation.x,
            ta.translation.y - bind.translation.y,
            ta.translation.z - bind.translation.z,
        );
        // Rotation delta = bind^-1 * additive.
        let bind_inv_rot = bind.rotation.conjugate();
        let delta_r = bind_inv_rot.compose(ta.rotation);
        // Scale delta = additive_scale / bind_scale.
        let safe_div = |a: f32, b: f32| if b.abs() < f32::EPSILON { 1.0 } else { a / b };
        let delta_s = Vec3::new(
            safe_div(ta.scale.x, bind.scale.x),
            safe_div(ta.scale.y, bind.scale.y),
            safe_div(ta.scale.z, bind.scale.z),
        );
        // Apply the delta on top of the base.
        let scaled_delta_t = Vec3::new(delta_t.x * weight, delta_t.y * weight, delta_t.z * weight);
        // Slerp identity → delta_r by `weight`.
        let scaled_delta_r = slerp(Quat::IDENTITY, delta_r, weight);
        // Blend scale toward 1 by `(1-weight)` and toward delta by `weight`.
        let one_minus = 1.0 - weight;
        let scaled_delta_s = Vec3::new(
            one_minus + delta_s.x * weight,
            one_minus + delta_s.y * weight,
            one_minus + delta_s.z * weight,
        );
        out.local_transforms[i] = Transform::new(
            tb.translation + scaled_delta_t,
            tb.rotation.compose(scaled_delta_r).normalize(),
            Vec3::new(
                tb.scale.x * scaled_delta_s.x,
                tb.scale.y * scaled_delta_s.y,
                tb.scale.z * scaled_delta_s.z,
            ),
        );
    }
    out
}

/// Blend N poses with normalized weights. Translation + scale are linear
/// averaged ; rotation uses iterated nlerp ("blend in the first one,
/// then nlerp toward the next, weighted by accumulated weight").
fn blend_n_poses(poses: &[Pose], weights: &[f32], skeleton: &Skeleton) -> Pose {
    let n = skeleton.bone_count();
    let mut out = Pose::identity(n);
    if poses.is_empty() {
        return out;
    }
    for i in 0..n {
        // Translation + scale : weighted sum.
        let mut t = Vec3::ZERO;
        let mut s = Vec3::ZERO;
        for (p, w) in poses.iter().zip(weights.iter()) {
            let tr = *p.local_transforms.get(i).unwrap_or(&Transform::IDENTITY);
            t = t + Vec3::new(
                tr.translation.x * w,
                tr.translation.y * w,
                tr.translation.z * w,
            );
            s = s + Vec3::new(tr.scale.x * w, tr.scale.y * w, tr.scale.z * w);
        }
        // Rotation : iterated nlerp. Start with the first pose's rotation
        // weighted by w0, then for each subsequent pose, slerp the running
        // result toward that pose's rotation by an "accumulated-weight"
        // factor — specifically, w_k / (w_0 + ... + w_k).
        let mut r = poses[0]
            .local_transforms
            .get(i)
            .copied()
            .unwrap_or(Transform::IDENTITY)
            .rotation;
        let mut running_w = weights[0];
        for k in 1..poses.len() {
            running_w += weights[k];
            if running_w <= 0.0 {
                continue;
            }
            let alpha = (weights[k] / running_w).clamp(0.0, 1.0);
            let q_k = poses[k]
                .local_transforms
                .get(i)
                .copied()
                .unwrap_or(Transform::IDENTITY)
                .rotation;
            r = slerp(r, q_k, alpha);
        }
        out.local_transforms[i] = Transform::new(t, r, s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{BlendNode, BlendTree, BlendTreeError, ClipHandle};
    use crate::clip::{AnimChannel, AnimationClip, Interpolation, KeyframeT};
    use crate::pose::Pose;
    use crate::sampler::AnimSampler;
    use crate::skeleton::{Bone, Skeleton, ROOT_PARENT};
    use crate::transform::Transform;
    use cssl_substrate_projections::Vec3;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn vec3_approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx_eq(a.x, b.x, eps) && approx_eq(a.y, b.y, eps) && approx_eq(a.z, b.z, eps)
    }

    fn make_skel() -> Skeleton {
        let bones = vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("b1", 0, Transform::IDENTITY),
        ];
        Skeleton::from_bones(bones).expect("ok")
    }

    fn make_clip(target_x: f32) -> AnimationClip {
        let ch = AnimChannel::translation(
            1,
            Interpolation::Linear,
            vec![
                KeyframeT {
                    time: 0.0,
                    value: Vec3::new(target_x, 0.0, 0.0),
                },
                KeyframeT {
                    time: 1.0,
                    value: Vec3::new(target_x, 0.0, 0.0),
                },
            ],
        )
        .expect("ok");
        AnimationClip::new("test", vec![ch])
    }

    #[test]
    fn empty_tree_fails_validation() {
        let t = BlendTree::new();
        assert!(matches!(t.validate(), Err(BlendTreeError::Malformed(_))));
    }

    #[test]
    fn single_clip_node_evaluates() {
        let mut tree = BlendTree::new();
        let h = tree.add_clip(make_clip(5.0));
        tree.add_node(BlendNode::Clip {
            handle: h,
            time: 0.5,
        });
        let s = make_skel();
        let mut pose = Pose::from_bind_pose(&s);
        tree.evaluate(&s, &AnimSampler::new(), &mut pose)
            .expect("eval");
        assert!(approx_eq(pose.local_transforms[1].translation.x, 5.0, 1e-5));
    }

    #[test]
    fn blend2_at_zero_returns_a() {
        let mut tree = BlendTree::new();
        let h_a = tree.add_clip(make_clip(1.0));
        let h_b = tree.add_clip(make_clip(9.0));
        let n_a = tree.add_node(BlendNode::Clip {
            handle: h_a,
            time: 0.0,
        });
        let n_b = tree.add_node(BlendNode::Clip {
            handle: h_b,
            time: 0.0,
        });
        let n_blend = tree.add_node(BlendNode::Blend2 {
            a: n_a,
            b: n_b,
            weight: 0.0,
        });
        tree.set_root(n_blend);
        let s = make_skel();
        let mut pose = Pose::from_bind_pose(&s);
        tree.evaluate(&s, &AnimSampler::new(), &mut pose)
            .expect("eval");
        assert!(approx_eq(pose.local_transforms[1].translation.x, 1.0, 1e-5));
    }

    #[test]
    fn blend2_at_one_returns_b() {
        let mut tree = BlendTree::new();
        let h_a = tree.add_clip(make_clip(1.0));
        let h_b = tree.add_clip(make_clip(9.0));
        let n_a = tree.add_node(BlendNode::Clip {
            handle: h_a,
            time: 0.0,
        });
        let n_b = tree.add_node(BlendNode::Clip {
            handle: h_b,
            time: 0.0,
        });
        let n_blend = tree.add_node(BlendNode::Blend2 {
            a: n_a,
            b: n_b,
            weight: 1.0,
        });
        tree.set_root(n_blend);
        let s = make_skel();
        let mut pose = Pose::from_bind_pose(&s);
        tree.evaluate(&s, &AnimSampler::new(), &mut pose)
            .expect("eval");
        assert!(approx_eq(pose.local_transforms[1].translation.x, 9.0, 1e-5));
    }

    #[test]
    fn blend2_at_half_is_midpoint() {
        let mut tree = BlendTree::new();
        let h_a = tree.add_clip(make_clip(0.0));
        let h_b = tree.add_clip(make_clip(10.0));
        let n_a = tree.add_node(BlendNode::Clip {
            handle: h_a,
            time: 0.0,
        });
        let n_b = tree.add_node(BlendNode::Clip {
            handle: h_b,
            time: 0.0,
        });
        let n_blend = tree.add_node(BlendNode::Blend2 {
            a: n_a,
            b: n_b,
            weight: 0.5,
        });
        tree.set_root(n_blend);
        let s = make_skel();
        let mut pose = Pose::from_bind_pose(&s);
        tree.evaluate(&s, &AnimSampler::new(), &mut pose)
            .expect("eval");
        assert!(approx_eq(pose.local_transforms[1].translation.x, 5.0, 1e-5));
    }

    #[test]
    fn blend_n_uniform_weights() {
        // Three clips with X = 0, 5, 10 ; uniform weights ⇒ result X = 5.
        let mut tree = BlendTree::new();
        let h0 = tree.add_clip(make_clip(0.0));
        let h1 = tree.add_clip(make_clip(5.0));
        let h2 = tree.add_clip(make_clip(10.0));
        let n0 = tree.add_node(BlendNode::Clip {
            handle: h0,
            time: 0.0,
        });
        let n1 = tree.add_node(BlendNode::Clip {
            handle: h1,
            time: 0.0,
        });
        let n2 = tree.add_node(BlendNode::Clip {
            handle: h2,
            time: 0.0,
        });
        let n_blend = tree.add_node(BlendNode::BlendN {
            children: vec![n0, n1, n2],
            weights: vec![1.0, 1.0, 1.0],
        });
        tree.set_root(n_blend);
        let s = make_skel();
        let mut pose = Pose::from_bind_pose(&s);
        tree.evaluate(&s, &AnimSampler::new(), &mut pose)
            .expect("eval");
        assert!(approx_eq(pose.local_transforms[1].translation.x, 5.0, 1e-4));
    }

    #[test]
    fn blend_n_normalizes_weights() {
        // Same clips, same uniform-result-expected, but with weights that
        // sum to 6.0 (not 1.0). Normalization should produce identical output.
        let mut tree = BlendTree::new();
        let h0 = tree.add_clip(make_clip(0.0));
        let h1 = tree.add_clip(make_clip(10.0));
        let n0 = tree.add_node(BlendNode::Clip {
            handle: h0,
            time: 0.0,
        });
        let n1 = tree.add_node(BlendNode::Clip {
            handle: h1,
            time: 0.0,
        });
        let n_blend = tree.add_node(BlendNode::BlendN {
            children: vec![n0, n1],
            weights: vec![3.0, 3.0],
        });
        tree.set_root(n_blend);
        let s = make_skel();
        let mut pose = Pose::from_bind_pose(&s);
        tree.evaluate(&s, &AnimSampler::new(), &mut pose)
            .expect("eval");
        assert!(approx_eq(pose.local_transforms[1].translation.x, 5.0, 1e-4));
    }

    #[test]
    fn blend_n_zero_weights_uses_uniform() {
        let mut tree = BlendTree::new();
        let h0 = tree.add_clip(make_clip(0.0));
        let h1 = tree.add_clip(make_clip(10.0));
        let n0 = tree.add_node(BlendNode::Clip {
            handle: h0,
            time: 0.0,
        });
        let n1 = tree.add_node(BlendNode::Clip {
            handle: h1,
            time: 0.0,
        });
        let n_blend = tree.add_node(BlendNode::BlendN {
            children: vec![n0, n1],
            weights: vec![0.0, 0.0],
        });
        tree.set_root(n_blend);
        let s = make_skel();
        let mut pose = Pose::from_bind_pose(&s);
        tree.evaluate(&s, &AnimSampler::new(), &mut pose)
            .expect("eval");
        // Fall-through to uniform 0.5 ⇒ x = 5.0.
        assert!(approx_eq(pose.local_transforms[1].translation.x, 5.0, 1e-4));
    }

    #[test]
    fn additive_at_zero_is_base() {
        // Additive at weight 0 should leave the base unchanged.
        let mut tree = BlendTree::new();
        let h_base = tree.add_clip(make_clip(7.0));
        let h_add = tree.add_clip(make_clip(100.0));
        let n_base = tree.add_node(BlendNode::Clip {
            handle: h_base,
            time: 0.0,
        });
        let n_add = tree.add_node(BlendNode::Clip {
            handle: h_add,
            time: 0.0,
        });
        let n_additive = tree.add_node(BlendNode::AdditiveBlend {
            base: n_base,
            additive: n_add,
            weight: 0.0,
        });
        tree.set_root(n_additive);
        let s = make_skel();
        let mut pose = Pose::from_bind_pose(&s);
        tree.evaluate(&s, &AnimSampler::new(), &mut pose)
            .expect("eval");
        assert!(approx_eq(pose.local_transforms[1].translation.x, 7.0, 1e-4));
    }

    #[test]
    fn validate_rejects_oob_node_reference() {
        let mut tree = BlendTree::new();
        tree.add_node(BlendNode::Blend2 {
            a: 99,
            b: 100,
            weight: 0.5,
        });
        assert!(matches!(tree.validate(), Err(BlendTreeError::Malformed(_))));
    }

    #[test]
    fn validate_rejects_oob_clip_handle() {
        let mut tree = BlendTree::new();
        tree.add_node(BlendNode::Clip {
            handle: ClipHandle(99),
            time: 0.0,
        });
        assert!(matches!(tree.validate(), Err(BlendTreeError::Malformed(_))));
    }

    #[test]
    fn validate_rejects_negative_blend2_weight() {
        let mut tree = BlendTree::new();
        let h = tree.add_clip(make_clip(0.0));
        let n = tree.add_node(BlendNode::Clip {
            handle: h,
            time: 0.0,
        });
        tree.add_node(BlendNode::Blend2 {
            a: n,
            b: n,
            weight: -0.5,
        });
        tree.set_root(1);
        assert!(matches!(tree.validate(), Err(BlendTreeError::Anim(_))));
    }

    #[test]
    fn validate_rejects_blend_n_length_mismatch() {
        let mut tree = BlendTree::new();
        let h = tree.add_clip(make_clip(0.0));
        let n = tree.add_node(BlendNode::Clip {
            handle: h,
            time: 0.0,
        });
        tree.add_node(BlendNode::BlendN {
            children: vec![n, n],
            weights: vec![1.0],
        });
        tree.set_root(1);
        assert!(matches!(tree.validate(), Err(BlendTreeError::Malformed(_))));
    }

    #[test]
    fn unused_value_check_blend_two() {
        // Confirm the helper-style dead-code paths are exercised : empty
        // child poses fall back to bind-pose ; this test doesn't need
        // assertion, just ensures the code path is reachable.
        let s = make_skel();
        let p = Pose::from_bind_pose(&s);
        let q = Pose::from_bind_pose(&s);
        let blended = super::blend_two_poses(&p, &q, 0.5, &s);
        assert_eq!(blended.bone_count(), s.bone_count());
        assert!(vec3_approx_eq(
            blended.local_transforms[1].translation,
            Vec3::ZERO,
            1e-5
        ));
    }
}
