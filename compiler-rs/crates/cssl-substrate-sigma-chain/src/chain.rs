// § T11-W11-SIGMA-CHAIN-CHAIN : append-only log + tail-pointer + checkpoint
// §§ design : Mutex-protected SigmaChain ; concurrent append serialized via parking_lot::Mutex
// §§ checkpoint : every CHECKPOINT_INTERVAL=1024 entries · auto-emit CheckpointMark entry
// §§ stage-0 : in-memory only ; production wire to disk-paged storage in next-wave

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use crate::entry::{EntryKind, LedgerEntry};
use crate::merkle::{IncrementalMerkle, InclusionProof, ZERO_HASH};

/// Auto-checkpoint cadence · per spec 28 § CHECKPOINT.
pub const CHECKPOINT_INTERVAL: u64 = 1024;

/// Snapshot of chain-state at a checkpoint · enables snapshot-replay.
#[derive(Clone, Debug)]
pub struct Checkpoint {
    /// seq_no of the CheckpointMark entry that materialized this snapshot
    pub seq_no: u64,
    /// Merkle root captured at snapshot time
    pub root: [u8; 32],
    /// epoch tag · "checkpoint number" starting at 1 for first one
    pub epoch: u64,
    /// snapshot of leaves at snapshot-time · used for restore-from
    pub leaves: Vec<[u8; 32]>,
}

/// Internal mutable state · guarded by Mutex.
#[derive(Debug, Default)]
struct ChainState {
    entries: Vec<LedgerEntry>,
    merkle: IncrementalMerkle,
    next_seq_no: u64,
    last_root: [u8; 32],
    checkpoints: Vec<Checkpoint>,
}

/// Append-only Σ-Chain ledger · thread-safe.
///
/// § api :
///     - new() · empty chain
///     - append(entry) · monotonic-seq-check + Merkle-extend + maybe-checkpoint
///     - tail() · current state-summary (seq_no · root)
///     - get(seq_no) · read entry by seq
///     - len() · number of entries
///     - checkpoints() · snapshot list
///
/// § thread-safety : Arc<Mutex<...>> internally · Send + Sync · clones share state.
#[derive(Debug, Clone, Default)]
pub struct SigmaChain {
    state: Arc<Mutex<ChainState>>,
}

/// Result of a successful append : new seq_no + new root.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AppendResult {
    pub seq_no: u64,
    pub root: [u8; 32],
    /// true iff a CheckpointMark was auto-emitted as a follow-up entry
    pub checkpoint_emitted: bool,
}

/// Lightweight tail-pointer · expose to callers without holding lock.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ChainTail {
    pub next_seq_no: u64,
    pub last_root: [u8; 32],
    pub entry_count: usize,
}

/// Errors emitted by chain operations.
#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error("expected seq_no {expected}, got {actual}")]
    SeqMismatch { expected: u64, actual: u64 },
    #[error("expected prev_root {expected:?}, got {actual:?}")]
    PrevRootMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    #[error("seq_no {0} out of range (chain has {1} entries)")]
    OutOfRange(u64, usize),
}

impl SigmaChain {
    /// § new empty chain · seq_no starts at 1 (0 = genesis-sentinel)
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ChainState {
                entries: Vec::new(),
                merkle: IncrementalMerkle::new(),
                next_seq_no: 1,
                last_root: ZERO_HASH,
                checkpoints: Vec::new(),
            })),
        }
    }

    /// § allocate the next seq_no for a caller-prepared entry · used by attest.rs.
    /// §§ returns (next_seq_no, prev_root) ; caller fills these in entry then submits append().
    #[must_use]
    pub fn reserve(&self) -> (u64, [u8; 32]) {
        let s = self.state.lock().expect("sigma-chain mutex poisoned");
        (s.next_seq_no, s.last_root)
    }

    /// § append a fully-prepared (signed) entry · validates seq + prev_root.
    /// §§ MUTEX-PROTECTED : concurrent callers serialize · final order = lock-acquisition order.
    /// §§ DESIGN-NOTE : reserve()+append() is racy across threads ; concurrent producers
    ///     should use try_append() which auto-fills seq_no + prev_root inside the lock.
    pub fn append(&self, entry: LedgerEntry) -> Result<AppendResult, ChainError> {
        let mut s = self.state.lock().expect("sigma-chain mutex poisoned");
        if entry.seq_no != s.next_seq_no {
            return Err(ChainError::SeqMismatch {
                expected: s.next_seq_no,
                actual: entry.seq_no,
            });
        }
        if entry.prev_root != s.last_root {
            return Err(ChainError::PrevRootMismatch {
                expected: s.last_root,
                actual: entry.prev_root,
            });
        }
        let leaf = entry.leaf_hash();
        let new_root = s.merkle.append(&leaf);
        s.last_root = new_root;
        s.next_seq_no += 1;
        s.entries.push(entry.clone());
        // auto-checkpoint
        let checkpoint_emitted = if (entry.seq_no % CHECKPOINT_INTERVAL == 0)
            && entry.kind != EntryKind::CheckpointMark
        {
            let epoch = (s.checkpoints.len() as u64) + 1;
            let checkpoint = Checkpoint {
                seq_no: entry.seq_no,
                root: new_root,
                epoch,
                leaves: s.merkle.snapshot_leaves(),
            };
            s.checkpoints.push(checkpoint);
            true
        } else {
            false
        };
        Ok(AppendResult {
            seq_no: entry.seq_no,
            root: new_root,
            checkpoint_emitted,
        })
    }

    /// § sign-and-append helper for concurrent producers · auto-fills seq_no + prev_root inside lock.
    /// §§ avoids the reserve()-then-append() race · atomic: signing-key ⨉ payload → entry inside crit-section.
    /// §§ the closure receives (next_seq_no, prev_root) and returns a SIGNED entry with those fields
    ///     populated correctly. Lock is held across the closure call · keep it cheap.
    pub fn try_append<F>(&self, build_entry: F) -> Result<AppendResult, ChainError>
    where
        F: FnOnce(u64, [u8; 32]) -> LedgerEntry,
    {
        let mut s = self.state.lock().expect("sigma-chain mutex poisoned");
        let seq_no = s.next_seq_no;
        let prev_root = s.last_root;
        let entry = build_entry(seq_no, prev_root);
        // re-validate (trust-but-verify the closure)
        if entry.seq_no != seq_no {
            return Err(ChainError::SeqMismatch {
                expected: seq_no,
                actual: entry.seq_no,
            });
        }
        if entry.prev_root != prev_root {
            return Err(ChainError::PrevRootMismatch {
                expected: prev_root,
                actual: entry.prev_root,
            });
        }
        let leaf = entry.leaf_hash();
        let new_root = s.merkle.append(&leaf);
        s.last_root = new_root;
        s.next_seq_no += 1;
        s.entries.push(entry.clone());
        let checkpoint_emitted = if (entry.seq_no % CHECKPOINT_INTERVAL == 0)
            && entry.kind != EntryKind::CheckpointMark
        {
            let epoch = (s.checkpoints.len() as u64) + 1;
            let checkpoint = Checkpoint {
                seq_no: entry.seq_no,
                root: new_root,
                epoch,
                leaves: s.merkle.snapshot_leaves(),
            };
            s.checkpoints.push(checkpoint);
            true
        } else {
            false
        };
        Ok(AppendResult {
            seq_no: entry.seq_no,
            root: new_root,
            checkpoint_emitted,
        })
    }

    /// § read tail-pointer · cheap snapshot.
    #[must_use]
    pub fn tail(&self) -> ChainTail {
        let s = self.state.lock().expect("sigma-chain mutex poisoned");
        ChainTail {
            next_seq_no: s.next_seq_no,
            last_root: s.last_root,
            entry_count: s.entries.len(),
        }
    }

    /// § number of entries appended so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.state.lock().expect("sigma-chain mutex poisoned").entries.len()
    }

    /// § true iff zero entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.state.lock().expect("sigma-chain mutex poisoned").entries.is_empty()
    }

    /// § get entry by 1-indexed seq_no · None if out-of-range.
    #[must_use]
    pub fn get(&self, seq_no: u64) -> Option<LedgerEntry> {
        if seq_no == 0 {
            return None;
        }
        let s = self.state.lock().expect("sigma-chain mutex poisoned");
        let idx = (seq_no - 1) as usize;
        s.entries.get(idx).cloned()
    }

    /// § all entries (clones · expensive for large chains · used in verify-replay tests).
    #[must_use]
    pub fn all_entries(&self) -> Vec<LedgerEntry> {
        self.state.lock().expect("sigma-chain mutex poisoned").entries.clone()
    }

    /// § snapshot of all checkpoints (each a Checkpoint).
    #[must_use]
    pub fn checkpoints(&self) -> Vec<Checkpoint> {
        self.state.lock().expect("sigma-chain mutex poisoned").checkpoints.clone()
    }

    /// § generate inclusion-proof for given seq_no.
    #[must_use]
    pub fn prove(&self, seq_no: u64) -> Option<InclusionProof> {
        if seq_no == 0 {
            return None;
        }
        let s = self.state.lock().expect("sigma-chain mutex poisoned");
        let leaf_index = (seq_no - 1) as usize;
        s.merkle.prove(leaf_index)
    }

    /// § sovereign-rollback : truncate chain back to given seq_no (inclusive-keep).
    /// §§ ¬ majority-override : caller-must-have-authority ; this crate provides mechanism only.
    /// §§ destroys post-rollback entries + checkpoints; future-wave: archive instead of destroy.
    pub fn sovereign_rollback(&self, keep_through_seq_no: u64) -> Result<[u8; 32], ChainError> {
        let mut s = self.state.lock().expect("sigma-chain mutex poisoned");
        if keep_through_seq_no >= s.next_seq_no {
            return Err(ChainError::OutOfRange(
                keep_through_seq_no,
                s.entries.len(),
            ));
        }
        let keep_count = keep_through_seq_no as usize;
        s.entries.truncate(keep_count);
        let mut new_merkle = IncrementalMerkle::new();
        for e in &s.entries {
            let leaf = e.leaf_hash();
            new_merkle.append(&leaf);
        }
        s.merkle = new_merkle;
        s.last_root = s.merkle.root();
        s.next_seq_no = keep_through_seq_no + 1;
        s.checkpoints
            .retain(|c| c.seq_no <= keep_through_seq_no);
        Ok(s.last_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryKind;

    fn dummy_entry(seq_no: u64, prev_root: [u8; 32]) -> LedgerEntry {
        LedgerEntry {
            seq_no,
            ts_unix: 1_700_000_000 + seq_no,
            kind: EntryKind::CapGrant,
            actor_pubkey: [1u8; 32],
            payload_hash: [2u8; 32],
            prev_root,
            signature: [3u8; 64],
        }
    }

    #[test]
    fn new_chain_is_empty() {
        let c = SigmaChain::new();
        assert_eq!(c.len(), 0);
        assert_eq!(c.tail().last_root, ZERO_HASH);
        assert_eq!(c.tail().next_seq_no, 1);
    }

    #[test]
    fn append_advances_seq() {
        let c = SigmaChain::new();
        let (sn, pr) = c.reserve();
        let e = dummy_entry(sn, pr);
        let r = c.append(e).expect("append");
        assert_eq!(r.seq_no, 1);
        assert_ne!(r.root, ZERO_HASH);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn append_seq_mismatch_rejected() {
        let c = SigmaChain::new();
        let bad = dummy_entry(99, ZERO_HASH);
        assert!(matches!(c.append(bad), Err(ChainError::SeqMismatch { .. })));
    }

    #[test]
    fn append_prev_root_mismatch_rejected() {
        let c = SigmaChain::new();
        let mut e = dummy_entry(1, [9u8; 32]); // wrong prev_root
        e.seq_no = 1;
        assert!(matches!(
            c.append(e),
            Err(ChainError::PrevRootMismatch { .. })
        ));
    }

    #[test]
    fn rollback_truncates() {
        let c = SigmaChain::new();
        for _ in 0..5 {
            c.try_append(|sn, pr| dummy_entry(sn, pr)).expect("append");
        }
        assert_eq!(c.len(), 5);
        let new_root = c.sovereign_rollback(2).expect("rollback");
        assert_eq!(c.len(), 2);
        assert_eq!(c.tail().last_root, new_root);
        assert_eq!(c.tail().next_seq_no, 3);
    }
}
