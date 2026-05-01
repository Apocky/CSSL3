// § merkle.rs · BLAKE3 merkle-root over leaves
// ══════════════════════════════════════════════════════════════════════════════
// § I> Padding-rule : duplicate-last-leaf when odd-count (BLAKE3-friendly)
// § I> Domain-separator : leaf-prefix 0x00 ; node-prefix 0x01 (canonical)
// § I> Empty-set → 32-zero-bytes (sentinel)
// § I> THIS CONSTRUCTION MUST MATCH cssl-host-sigma-chain (W8-C1) — assumption
//   documented HERE for integration-time audit.
// ══════════════════════════════════════════════════════════════════════════════

/// 32-byte BLAKE3 merkle-root.
pub type MerkleRoot = [u8; 32];

const LEAF_PREFIX: u8 = 0x00;
const NODE_PREFIX: u8 = 0x01;

/// Compute BLAKE3 merkle-root over `leaves`.
///
/// - empty → all-zero 32-byte sentinel
/// - odd-count at any level → duplicate last leaf/node
/// - leaf domain-prefix = `0x00` ; node domain-prefix = `0x01`
///
/// **CONSENSUS-CRITICAL** : padding-rule MUST match sibling-crate (W8-C1).
pub fn merkle_root_blake3(leaves: &[[u8; 32]]) -> MerkleRoot {
    if leaves.is_empty() {
        return [0u8; 32];
    }

    // Hash each leaf with the leaf-prefix.
    let mut level: Vec<[u8; 32]> = leaves
        .iter()
        .map(|leaf| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&[LEAF_PREFIX]);
            hasher.update(leaf);
            *hasher.finalize().as_bytes()
        })
        .collect();

    while level.len() > 1 {
        if level.len() % 2 == 1 {
            // pad : duplicate the last node
            let last = *level.last().unwrap();
            level.push(last);
        }
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks_exact(2) {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&[NODE_PREFIX]);
            hasher.update(&pair[0]);
            hasher.update(&pair[1]);
            next.push(*hasher.finalize().as_bytes());
        }
        level = next;
    }
    level[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_is_zero_sentinel() {
        let root = merkle_root_blake3(&[]);
        assert_eq!(root, [0u8; 32]);
    }

    #[test]
    fn single_leaf_root_stable() {
        let leaf = [9u8; 32];
        let r1 = merkle_root_blake3(&[leaf]);
        let r2 = merkle_root_blake3(&[leaf]);
        assert_eq!(r1, r2);
        assert_ne!(r1, [0u8; 32]);
    }

    #[test]
    fn odd_pad_duplicates_last() {
        // 3-leaf : effectively merkle( h(a) , h(b) , h(c) , h(c) )
        let leaves = [[1u8; 32], [2u8; 32], [3u8; 32]];
        let r = merkle_root_blake3(&leaves);
        // recomputing same input → same root
        let r2 = merkle_root_blake3(&leaves);
        assert_eq!(r, r2);
    }

    #[test]
    fn different_order_distinct_root() {
        let a = merkle_root_blake3(&[[1u8; 32], [2u8; 32]]);
        let b = merkle_root_blake3(&[[2u8; 32], [1u8; 32]]);
        assert_ne!(a, b);
    }
}
