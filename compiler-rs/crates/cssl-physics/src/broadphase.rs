//! Broad-phase collision detection — BVH-based.
//!
//! § THESIS
//!   Naive collision detection tests every body-pair (O(n²)). The broad-phase
//!   prunes the work to "candidate pairs whose AABBs overlap". For game scenes
//!   with non-clustered geometry, a Bounding Volume Hierarchy (BVH) outperforms
//!   Sweep-and-Prune (SAP), which assumes scene-coherence — game scenes have
//!   fast-moving projectiles + rapidly-changing object distributions that
//!   defeat SAP's incremental update strategy.
//!
//! § ALGORITHM — TOP-DOWN BVH WITH SAH
//!   1. Compute world-space AABB for each body.
//!   2. Sort bodies into a binary tree :
//!        * Each leaf holds one body.
//!        * Each internal node has an AABB containing all child AABBs.
//!        * Split axis chosen by Surface-Area-Heuristic (SAH) — pick the
//!          axis that minimizes `lhs.SA * lhs.count + rhs.SA * rhs.count`.
//!   3. Query : for each body, traverse the tree top-down ; at each node,
//!      if AABB-overlap, descend ; if leaf, emit candidate pair.
//!
//!   Stage-0 form rebuilds the tree each frame. Refit-instead-of-rebuild
//!   is a future optimization (factor of 2-5 speedup ; deferred until
//!   profiling data shows it matters).
//!
//! § DETERMINISM
//!   - Sort uses `f64::total_cmp` (no NaN-vs-NaN ambiguity).
//!   - Tree-build is recursive in a single thread ; no parallel iteration.
//!   - Output candidate pairs are emitted in `(body_a, body_b)` sorted order.

use crate::body::BodyId;
use crate::shape::Aabb;

// ────────────────────────────────────────────────────────────────────────
// § BvhNode
// ────────────────────────────────────────────────────────────────────────

/// A node in the BVH. Either an internal node with two children, or a leaf
/// with a `BodyId`.
#[derive(Debug, Clone)]
pub enum BvhNode {
    /// Internal node : combined AABB + two child indices.
    Internal {
        aabb: Aabb,
        left: usize,
        right: usize,
    },
    /// Leaf node : a single body.
    Leaf { aabb: Aabb, body: BodyId },
}

impl BvhNode {
    #[must_use]
    pub fn aabb(&self) -> Aabb {
        match self {
            BvhNode::Internal { aabb, .. } => *aabb,
            BvhNode::Leaf { aabb, .. } => *aabb,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// § BroadPhase trait
// ────────────────────────────────────────────────────────────────────────

/// Broad-phase collision detection trait. Implementors take per-body AABBs +
/// produce a list of candidate pairs whose AABBs overlap.
///
/// § STAGE-0
///   We ship one impl (`BvhBroadPhase`) ; the trait exists so future SAP
///   or grid-based variants can drop in.
pub trait BroadPhase {
    /// Build (or rebuild) the broadphase from a sorted list of (body, aabb) pairs.
    /// Bodies MUST be sorted by `BodyId` for replay-determinism.
    fn build(&mut self, bodies: &[(BodyId, Aabb)]);

    /// Query : produce all candidate pairs whose AABBs overlap. Pairs are
    /// emitted in `(body_a, body_b)` sorted order with `body_a < body_b`.
    fn query_pairs(&self) -> Vec<(BodyId, BodyId)>;
}

// ────────────────────────────────────────────────────────────────────────
// § BvhBroadPhase — top-down SAH BVH
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BvhBroadPhase {
    /// Tree nodes. The root is at `nodes[root]` (or empty if no bodies).
    nodes: Vec<BvhNode>,
    root: Option<usize>,
}

impl BvhBroadPhase {
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
        }
    }

    /// Number of nodes in the tree.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Recursive build. Returns the index of the new node.
    fn build_recursive(&mut self, bodies: &mut [(BodyId, Aabb)]) -> usize {
        debug_assert!(
            !bodies.is_empty(),
            "build_recursive called with empty slice"
        );

        if bodies.len() == 1 {
            let (body, aabb) = bodies[0];
            self.nodes.push(BvhNode::Leaf { aabb, body });
            return self.nodes.len() - 1;
        }

        // Compute combined AABB.
        let mut combined = Aabb::EMPTY;
        for &(_, aabb) in bodies.iter() {
            combined = combined.merge(aabb);
        }

        // Choose split axis : longest extent.
        let extents = combined.extents();
        let axis = if extents.x >= extents.y && extents.x >= extents.z {
            0
        } else if extents.y >= extents.z {
            1
        } else {
            2
        };

        // Sort by center on chosen axis.
        bodies.sort_by(|a, b| {
            let ca = match axis {
                0 => a.1.center().x,
                1 => a.1.center().y,
                _ => a.1.center().z,
            };
            let cb = match axis {
                0 => b.1.center().x,
                1 => b.1.center().y,
                _ => b.1.center().z,
            };
            // Primary : center on axis. Secondary : body-id for stable order.
            ca.total_cmp(&cb).then_with(|| a.0.cmp(&b.0))
        });

        // SAH split : try mid-point split for stage-0. Real SAH evaluates
        // multiple bin-positions ; mid-split gives O(n log n) rebuild + good
        // enough quality for game scenes. SAH-binned is a future optimization.
        let mid = bodies.len() / 2;
        let (left_slice, right_slice) = bodies.split_at_mut(mid);
        let left = self.build_recursive(left_slice);
        let right = self.build_recursive(right_slice);

        let aabb = self.nodes[left].aabb().merge(self.nodes[right].aabb());
        self.nodes.push(BvhNode::Internal { aabb, left, right });
        self.nodes.len() - 1
    }

    /// Recursive pair-emitter : descend the tree, emitting candidate pairs.
    /// We test each pair of subtrees ; if their AABBs overlap, descend further.
    fn collect_pairs(&self, a_idx: usize, b_idx: usize, out: &mut Vec<(BodyId, BodyId)>) {
        let a_node = &self.nodes[a_idx];
        let b_node = &self.nodes[b_idx];

        if !a_node.aabb().overlaps(b_node.aabb()) {
            return;
        }

        match (a_node, b_node) {
            (BvhNode::Leaf { body: a, .. }, BvhNode::Leaf { body: b, .. }) => {
                if a != b {
                    let (lo, hi) = if a <= b { (*a, *b) } else { (*b, *a) };
                    out.push((lo, hi));
                }
            }
            (BvhNode::Internal { left, right, .. }, BvhNode::Leaf { .. }) => {
                self.collect_pairs(*left, b_idx, out);
                self.collect_pairs(*right, b_idx, out);
            }
            (BvhNode::Leaf { .. }, BvhNode::Internal { left, right, .. }) => {
                self.collect_pairs(a_idx, *left, out);
                self.collect_pairs(a_idx, *right, out);
            }
            (
                BvhNode::Internal {
                    left: al,
                    right: ar,
                    ..
                },
                BvhNode::Internal {
                    left: bl,
                    right: br,
                    ..
                },
            ) => {
                self.collect_pairs(*al, *bl, out);
                self.collect_pairs(*al, *br, out);
                self.collect_pairs(*ar, *bl, out);
                self.collect_pairs(*ar, *br, out);
            }
        }
    }

    /// Recursive self-pair emitter : within the subtree rooted at `idx`,
    /// emit all leaf-pairs whose AABBs overlap.
    fn collect_self_pairs(&self, idx: usize, out: &mut Vec<(BodyId, BodyId)>) {
        match &self.nodes[idx] {
            BvhNode::Leaf { .. } => {}
            BvhNode::Internal { left, right, .. } => {
                self.collect_self_pairs(*left, out);
                self.collect_self_pairs(*right, out);
                self.collect_pairs(*left, *right, out);
            }
        }
    }
}

impl Default for BvhBroadPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl BroadPhase for BvhBroadPhase {
    fn build(&mut self, bodies: &[(BodyId, Aabb)]) {
        self.nodes.clear();
        self.root = None;
        if bodies.is_empty() {
            return;
        }
        let mut sorted: Vec<(BodyId, Aabb)> = bodies.to_vec();
        // Initial sort by body-id ⇒ stable output order across rebuilds.
        sorted.sort_by_key(|(id, _)| *id);
        let root = self.build_recursive(&mut sorted);
        self.root = Some(root);
    }

    fn query_pairs(&self) -> Vec<(BodyId, BodyId)> {
        let mut out = Vec::new();
        if let Some(root) = self.root {
            self.collect_self_pairs(root, &mut out);
        }
        // Final sort to guarantee canonical order.
        out.sort();
        out.dedup();
        out
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    fn aabb_at(c: Vec3, half: f64) -> Aabb {
        Aabb::new(c - Vec3::splat(half), c + Vec3::splat(half))
    }

    // ─── BvhBroadPhase ───

    #[test]
    fn empty_broadphase_no_pairs() {
        let mut bp = BvhBroadPhase::new();
        bp.build(&[]);
        assert!(bp.query_pairs().is_empty());
        assert!(bp.is_empty());
    }

    #[test]
    fn single_body_no_pairs() {
        let mut bp = BvhBroadPhase::new();
        bp.build(&[(BodyId(0), aabb_at(Vec3::ZERO, 0.5))]);
        assert!(bp.query_pairs().is_empty());
    }

    #[test]
    fn two_overlapping_aabbs_yield_pair() {
        let mut bp = BvhBroadPhase::new();
        bp.build(&[
            (BodyId(0), aabb_at(Vec3::ZERO, 0.5)),
            (BodyId(1), aabb_at(Vec3::new(0.4, 0.0, 0.0), 0.5)),
        ]);
        let pairs = bp.query_pairs();
        assert_eq!(pairs, vec![(BodyId(0), BodyId(1))]);
    }

    #[test]
    fn two_disjoint_aabbs_no_pair() {
        let mut bp = BvhBroadPhase::new();
        bp.build(&[
            (BodyId(0), aabb_at(Vec3::ZERO, 0.5)),
            (BodyId(1), aabb_at(Vec3::new(5.0, 0.0, 0.0), 0.5)),
        ]);
        assert!(bp.query_pairs().is_empty());
    }

    #[test]
    fn three_bodies_two_pairs() {
        // Bodies 0, 1, 2 in a row : 0-1 overlap, 1-2 overlap, 0-2 disjoint.
        let mut bp = BvhBroadPhase::new();
        bp.build(&[
            (BodyId(0), aabb_at(Vec3::new(0.0, 0.0, 0.0), 0.6)),
            (BodyId(1), aabb_at(Vec3::new(1.0, 0.0, 0.0), 0.6)),
            (BodyId(2), aabb_at(Vec3::new(2.0, 0.0, 0.0), 0.6)),
        ]);
        let pairs = bp.query_pairs();
        // 0-1 and 1-2 overlap, but not 0-2 (gap of 0.8 > 0)
        assert_eq!(pairs, vec![(BodyId(0), BodyId(1)), (BodyId(1), BodyId(2))]);
    }

    #[test]
    fn five_bodies_clustered_canonical_pair_order() {
        // Five bodies all at origin with varying AABB sizes ; should produce
        // all 10 pairs.
        let mut bp = BvhBroadPhase::new();
        bp.build(&[
            (BodyId(10), aabb_at(Vec3::ZERO, 1.0)),
            (BodyId(20), aabb_at(Vec3::ZERO, 1.0)),
            (BodyId(30), aabb_at(Vec3::ZERO, 1.0)),
            (BodyId(40), aabb_at(Vec3::ZERO, 1.0)),
            (BodyId(50), aabb_at(Vec3::ZERO, 1.0)),
        ]);
        let pairs = bp.query_pairs();
        assert_eq!(pairs.len(), 10); // C(5,2) = 10
                                     // Verify canonical order : a < b in every pair.
        for (a, b) in &pairs {
            assert!(a < b, "pair ({a:?}, {b:?}) not canonically ordered");
        }
        // Verify pair order is sorted globally.
        let mut sorted = pairs.clone();
        sorted.sort();
        assert_eq!(pairs, sorted);
    }

    #[test]
    fn rebuild_clears_old_tree() {
        let mut bp = BvhBroadPhase::new();
        bp.build(&[
            (BodyId(0), aabb_at(Vec3::ZERO, 0.5)),
            (BodyId(1), aabb_at(Vec3::ZERO, 0.5)),
        ]);
        let _ = bp.query_pairs();
        bp.build(&[(BodyId(2), aabb_at(Vec3::ZERO, 0.5))]);
        // After rebuild, only 1 body ⇒ no pairs ⇒ tree has 1 node only.
        assert!(bp.query_pairs().is_empty());
        assert_eq!(bp.node_count(), 1);
    }

    #[test]
    fn determinism_same_input_same_pairs() {
        let inputs = vec![
            (BodyId(0), aabb_at(Vec3::ZERO, 0.5)),
            (BodyId(1), aabb_at(Vec3::splat(0.3), 0.5)),
            (BodyId(2), aabb_at(Vec3::splat(0.6), 0.5)),
            (BodyId(3), aabb_at(Vec3::splat(0.9), 0.5)),
        ];
        let mut bp1 = BvhBroadPhase::new();
        let mut bp2 = BvhBroadPhase::new();
        bp1.build(&inputs);
        bp2.build(&inputs);
        assert_eq!(bp1.query_pairs(), bp2.query_pairs());
    }

    #[test]
    fn determinism_input_order_independent() {
        let mut a = vec![
            (BodyId(0), aabb_at(Vec3::ZERO, 0.5)),
            (BodyId(1), aabb_at(Vec3::splat(0.3), 0.5)),
            (BodyId(2), aabb_at(Vec3::splat(0.6), 0.5)),
        ];
        let b = a.clone();
        a.reverse();
        let mut bp1 = BvhBroadPhase::new();
        let mut bp2 = BvhBroadPhase::new();
        bp1.build(&a);
        bp2.build(&b);
        // Output should be identical regardless of input order ; build sorts by id.
        assert_eq!(bp1.query_pairs(), bp2.query_pairs());
    }

    #[test]
    fn touching_pairs_count_as_overlap() {
        let mut bp = BvhBroadPhase::new();
        // Two AABBs sharing a face exactly.
        bp.build(&[
            (BodyId(0), Aabb::new(Vec3::ZERO, Vec3::splat(1.0))),
            (BodyId(1), Aabb::new(Vec3::splat(1.0), Vec3::splat(2.0))),
        ]);
        assert_eq!(bp.query_pairs(), vec![(BodyId(0), BodyId(1))]);
    }

    #[test]
    fn no_self_pairs_emitted() {
        let mut bp = BvhBroadPhase::new();
        bp.build(&[(BodyId(7), aabb_at(Vec3::ZERO, 1.0))]);
        let pairs = bp.query_pairs();
        // Single body must not pair with itself.
        for (a, b) in &pairs {
            assert_ne!(a, b);
        }
    }

    #[test]
    fn bvh_node_aabb_dispatches_correctly() {
        let leaf = BvhNode::Leaf {
            aabb: aabb_at(Vec3::ZERO, 1.0),
            body: BodyId(0),
        };
        assert_eq!(leaf.aabb(), aabb_at(Vec3::ZERO, 1.0));
        let internal = BvhNode::Internal {
            aabb: aabb_at(Vec3::splat(1.0), 2.0),
            left: 0,
            right: 0,
        };
        assert_eq!(internal.aabb(), aabb_at(Vec3::splat(1.0), 2.0));
    }
}
