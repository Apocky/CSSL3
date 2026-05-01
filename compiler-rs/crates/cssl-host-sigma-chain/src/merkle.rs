// § merkle.rs — BLAKE3 merkle-tree over event-ids
// §§ landmine resolution : odd-leaf-count → DUPLICATE-LAST-LEAF (stable + simple)
// §§ leaves are sorted-by-event-id ASC (deterministic across machines)

/// 32-byte BLAKE3 digest used as merkle-node value.
pub type Digest = [u8; 32];

/// Domain-separator for leaf hashing (prevents 2nd-preimage between leaves & internal nodes).
const LEAF_DOMAIN: &[u8] = b"sigma_chain/merkle/leaf/v1";
/// Domain-separator for internal-node hashing.
const NODE_DOMAIN: &[u8] = b"sigma_chain/merkle/node/v1";

/// Empty-tree sentinel = BLAKE3("sigma_chain/merkle/empty/v1").
#[must_use]
pub fn empty_root() -> Digest {
    let mut h = blake3::Hasher::new();
    h.update(b"sigma_chain/merkle/empty/v1");
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

/// Leaf hash : H(LEAF_DOMAIN || event_id).
#[must_use]
pub fn leaf_hash(event_id: &Digest) -> Digest {
    let mut h = blake3::Hasher::new();
    h.update(LEAF_DOMAIN);
    h.update(event_id);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

/// Internal-node hash : H(NODE_DOMAIN || left || right).
#[must_use]
pub fn node_hash(left: &Digest, right: &Digest) -> Digest {
    let mut h = blake3::Hasher::new();
    h.update(NODE_DOMAIN);
    h.update(left);
    h.update(right);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

/// Compute the merkle-root of `event_ids`.
///
/// Conventions :
/// - Empty input → [`empty_root`]
/// - Single leaf → H(LEAF_DOMAIN || id)
/// - Odd row → duplicate the last node (stable canonical convention)
///
/// Caller is responsible for sorting `event_ids` before invocation if cross-machine
/// determinism is required (the [`crate::ledger::SigmaLedger`] does so via BTreeMap iteration).
#[must_use]
pub fn merkle_root_of(event_ids: &[Digest]) -> Digest {
    if event_ids.is_empty() {
        return empty_root();
    }
    let mut row: Vec<Digest> = event_ids.iter().map(leaf_hash).collect();
    while row.len() > 1 {
        if row.len() % 2 == 1 {
            // Duplicate-last-leaf convention (locked-in choice — DO NOT change).
            let last = *row.last().expect("non-empty");
            row.push(last);
        }
        let mut next = Vec::with_capacity(row.len() / 2);
        for pair in row.chunks_exact(2) {
            next.push(node_hash(&pair[0], &pair[1]));
        }
        row = next;
    }
    row[0]
}

/// One step in a merkle-path proving a leaf's inclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MerkleStep {
    /// Sibling-node digest at this level.
    pub sibling: Digest,
    /// `true` iff the sibling is on the LEFT (i.e. our node is on the right).
    pub sibling_is_left: bool,
}

/// Compute a merkle-path for `target_id` within `event_ids`.
///
/// Returns `None` if `target_id` is not present.
///
/// The path together with `target_id` reproduces the merkle-root via [`merkle_path_verify`].
#[must_use]
pub fn merkle_path_of(event_ids: &[Digest], target_id: &Digest) -> Option<Vec<MerkleStep>> {
    let target_idx = event_ids.iter().position(|id| id == target_id)?;
    if event_ids.is_empty() {
        return None;
    }
    let mut idx = target_idx;
    let mut row: Vec<Digest> = event_ids.iter().map(leaf_hash).collect();
    let mut path: Vec<MerkleStep> = Vec::new();
    while row.len() > 1 {
        if row.len() % 2 == 1 {
            let last = *row.last().expect("non-empty");
            row.push(last);
        }
        let sibling_idx = idx ^ 1;
        let sibling_is_left = sibling_idx < idx;
        path.push(MerkleStep {
            sibling: row[sibling_idx],
            sibling_is_left,
        });
        let mut next = Vec::with_capacity(row.len() / 2);
        for pair in row.chunks_exact(2) {
            next.push(node_hash(&pair[0], &pair[1]));
        }
        row = next;
        idx /= 2;
    }
    Some(path)
}

/// Verify a merkle-path : reproduce the root from `target_id` and `path`, compare to `expected_root`.
#[must_use]
pub fn merkle_path_verify(
    target_id: &Digest,
    path: &[MerkleStep],
    expected_root: &Digest,
) -> bool {
    let mut acc = leaf_hash(target_id);
    for step in path {
        acc = if step.sibling_is_left {
            node_hash(&step.sibling, &acc)
        } else {
            node_hash(&acc, &step.sibling)
        };
    }
    &acc == expected_root
}
