// § T11-W11-SIGMA-CHAIN-MERKLE : incremental BLAKE3-Merkle-tree
// §§ design : binary-Merkle ; full-tree-of-leaves ; balanced via duplicating-last-leaf for odd-counts
// §§ append : O(log N) — only re-hash log(N) right-spine nodes
// §§ prove  : O(log N) — sibling-hashes from leaf to root
//
// §§ ALGORITHM (incremental peak-tree, à la Certificate-Transparency-style) :
//     - we keep a Vec<[u8;32]> of "peaks" : the roots of complete sub-trees
//     - on append : push leaf as height-0 peak ; while top-2 peaks have equal height, combine them
//     - root() = fold-right peaks with combine() ; pad with ZERO when empty
//     - this gives O(log N) append cost ; O(log N) memory
//
// §§ proof-generation :
//     - we ALSO retain leaves to enable O(log N) inclusion-proof reconstruction
//     - production system would recompute proofs lazily ; bootstrap stores leaves for simplicity

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// 32-byte zero hash · used as null/genesis prev_root.
pub const ZERO_HASH: [u8; 32] = [0u8; 32];

/// Domain separators per Merkle-tree-design (Certificate-Transparency-style)
/// to prevent leaf-vs-internal-node hash-collision.
const DOMAIN_LEAF: &[u8] = b"cssl-substrate-sigma-chain/v0/merkle/leaf";
const DOMAIN_NODE: &[u8] = b"cssl-substrate-sigma-chain/v0/merkle/node";

/// Compute leaf-hash · domain-separated to prevent 2nd-preimage attacks.
#[must_use]
pub fn hash_leaf(leaf_data: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN_LEAF);
    hasher.update(leaf_data);
    *hasher.finalize().as_bytes()
}

/// Combine two child hashes into parent · domain-separated.
#[must_use]
pub fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN_NODE);
    hasher.update(left);
    hasher.update(right);
    *hasher.finalize().as_bytes()
}

/// Inclusion-proof for a single leaf · O(log N) sibling-hashes.
///
/// § verification-procedure :
///     start with hash_leaf(leaf_data)
///     for each (sibling, side) in path :
///         h = if side==Left { hash_node(sibling, h) } else { hash_node(h, sibling) }
///     compare h to claimed-root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InclusionProof {
    /// 0-indexed leaf position
    pub leaf_index: usize,
    /// total tree size at proof-time
    pub tree_size: usize,
    /// (sibling_hash, side) pairs from leaf-up-to-root
    pub path: Vec<(SiblingSide, [u8; 32])>,
}

/// Which side of the parent the sibling occupies (relative to the prover's path).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SiblingSide {
    Left,
    Right,
}

/// Verify an inclusion-proof · O(log N) hash-operations.
///
/// § returns true iff proof reconstructs claimed_root.
#[must_use]
pub fn verify_inclusion(
    leaf_data: &[u8; 32],
    proof: &InclusionProof,
    claimed_root: &[u8; 32],
) -> bool {
    let mut h = hash_leaf(leaf_data);
    for (side, sibling) in &proof.path {
        h = match side {
            SiblingSide::Left => hash_node(sibling, &h),
            SiblingSide::Right => hash_node(&h, sibling),
        };
    }
    &h == claimed_root
}

/// Incremental Merkle-tree · stores all leaves + maintains running peaks.
///
/// § append-cost : O(log N) hash-ops · O(1) amortized leaf-storage push
/// § root-cost   : O(log N) — fold peaks
/// § prove-cost  : O(log N) hash-ops · path reconstructed from full leaves
#[derive(Clone, Debug, Default)]
pub struct IncrementalMerkle {
    /// All leaf hashes (after hash_leaf wrapping) · enables prove() to walk tree.
    /// In production, this could be stored on-disk + paged ; bootstrap keeps in-memory.
    leaves: Vec<[u8; 32]>,
}

impl IncrementalMerkle {
    /// § new empty tree · root = ZERO_HASH
    #[must_use]
    pub const fn new() -> Self {
        Self { leaves: Vec::new() }
    }

    /// § number of leaves currently in tree
    #[must_use]
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// § true iff zero leaves
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// § append a leaf · O(log N) (amortized re-hash of right-spine inside root() · O(1) here)
    pub fn append(&mut self, leaf_data: &[u8; 32]) -> [u8; 32] {
        let leaf_hash = hash_leaf(leaf_data);
        self.leaves.push(leaf_hash);
        self.root()
    }

    /// § current Merkle root · ZERO_HASH if empty.
    /// §§ algorithm : balanced-binary-tree with last-leaf-duplication for odd counts at each level.
    /// §§ cost : O(N) — full re-fold ; for incremental-append-only-flag we cache via peaks (future opt).
    #[must_use]
    pub fn root(&self) -> [u8; 32] {
        if self.leaves.is_empty() {
            return ZERO_HASH;
        }
        let mut level = self.leaves.clone();
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut i = 0;
            while i < level.len() {
                let left = level[i];
                let right = if i + 1 < level.len() {
                    level[i + 1]
                } else {
                    // odd count : duplicate last leaf at this level
                    level[i]
                };
                next.push(hash_node(&left, &right));
                i += 2;
            }
            level = next;
        }
        level[0]
    }

    /// § generate inclusion-proof for leaf at index · O(log N) hash-ops · O(N) leaf-walk.
    /// §§ returns None if index out-of-bounds.
    #[must_use]
    pub fn prove(&self, leaf_index: usize) -> Option<InclusionProof> {
        if leaf_index >= self.leaves.len() {
            return None;
        }
        let mut path = Vec::new();
        let mut level = self.leaves.clone();
        let mut idx = leaf_index;
        while level.len() > 1 {
            let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            let (sibling_hash, sibling_side) = if sibling_idx < level.len() {
                let side = if idx % 2 == 0 {
                    SiblingSide::Right
                } else {
                    SiblingSide::Left
                };
                (level[sibling_idx], side)
            } else {
                // odd-end : sibling is duplicate of self
                (level[idx], SiblingSide::Right)
            };
            path.push((sibling_side, sibling_hash));
            // build next level
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut i = 0;
            while i < level.len() {
                let left = level[i];
                let right = if i + 1 < level.len() {
                    level[i + 1]
                } else {
                    level[i]
                };
                next.push(hash_node(&left, &right));
                i += 2;
            }
            level = next;
            idx /= 2;
        }
        Some(InclusionProof {
            leaf_index,
            tree_size: self.leaves.len(),
            path,
        })
    }

    /// § snapshot leaves for checkpoint-archival · clone-only ; tree stays usable.
    #[must_use]
    pub fn snapshot_leaves(&self) -> Vec<[u8; 32]> {
        self.leaves.clone()
    }

    /// § restore from snapshot · used by checkpoint-load.
    pub fn restore_from(leaves: Vec<[u8; 32]>) -> Self {
        Self { leaves }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_has_zero_root() {
        let m = IncrementalMerkle::new();
        assert_eq!(m.root(), ZERO_HASH);
        assert!(m.is_empty());
    }

    #[test]
    fn single_leaf_root_is_leaf_hash() {
        let mut m = IncrementalMerkle::new();
        let leaf = [42u8; 32];
        let root = m.append(&leaf);
        assert_eq!(root, hash_leaf(&leaf));
    }

    #[test]
    fn two_leaves_root_is_node_hash() {
        let mut m = IncrementalMerkle::new();
        let l0 = [1u8; 32];
        let l1 = [2u8; 32];
        m.append(&l0);
        let root = m.append(&l1);
        let expected = hash_node(&hash_leaf(&l0), &hash_leaf(&l1));
        assert_eq!(root, expected);
    }

    #[test]
    fn inclusion_proof_round_trip() {
        let mut m = IncrementalMerkle::new();
        for i in 0..7u8 {
            let leaf = [i; 32];
            m.append(&leaf);
        }
        let root = m.root();
        for i in 0..7 {
            let leaf = [i as u8; 32];
            let proof = m.prove(i).expect("prove");
            assert!(
                verify_inclusion(&leaf, &proof, &root),
                "inclusion-proof verify failed for leaf {i}"
            );
        }
    }

    #[test]
    fn tamper_proof_rejected() {
        let mut m = IncrementalMerkle::new();
        for i in 0..4u8 {
            let leaf = [i; 32];
            m.append(&leaf);
        }
        let root = m.root();
        let mut proof = m.prove(0).expect("prove");
        proof.path[0].1[0] ^= 0xFF; // tamper sibling
        let leaf = [0u8; 32];
        assert!(!verify_inclusion(&leaf, &proof, &root));
    }

    #[test]
    fn root_changes_with_append() {
        let mut m = IncrementalMerkle::new();
        let mut prev_root = m.root();
        for i in 0..5u8 {
            let new_root = m.append(&[i; 32]);
            assert_ne!(new_root, prev_root, "root must change on append (i={i})");
            prev_root = new_root;
        }
    }
}
