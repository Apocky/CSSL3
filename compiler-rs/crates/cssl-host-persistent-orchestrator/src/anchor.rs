// § anchor.rs — Σ-Chain anchor chain (rolling BLAKE3-128 commitment).
//
// § thesis
//   Every cycle-completion produces a journal-entry. The anchor-chain rolls
//   a BLAKE3 hash forward over (prev_anchor || journal_entry_bytes) so the
//   complete cycle-history is tamper-evident even if the daemon-binary is
//   compromised. The `cssl-substrate-sigma-chain` crate accepts this rolling
//   anchor as input to a chain-event when Apocky's SigmaAnchor cap is granted.
//
// § why a separate chain not just blake3-of-journal
//   The chain is APPEND-ONLY by design : once an anchor is computed, it cannot
//   be retroactively changed without invalidating every subsequent anchor.
//   This makes journal-tampering detectable even if the journal-store itself
//   is corrupted (e.g. by FS-corruption on hard reboot).

use serde::{Deserialize, Serialize};

/// One anchor record — the rolling-BLAKE3 state plus an absolute sequence
/// counter. Persisted into the journal-store so replay reconstructs the chain.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnchorRecord {
    /// Sequence number — monotonically increasing.
    pub seq: u64,
    /// Rolling BLAKE3 digest after folding this record's input.
    pub digest: [u8; 32],
    /// `now_ms` at which this anchor was emitted.
    pub at_ms: u64,
    /// Reason this anchor was minted (e.g. cycle-name).
    pub reason: AnchorReason,
}

/// Why an anchor was emitted.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnchorReason {
    CycleClose,
    KanThreshold,
    SovereignPause,
    SovereignResume,
    Bootstrap,
    Restore,
}

/// Rolling-BLAKE3 anchor chain.
#[derive(Debug, Clone)]
pub struct AnchorChain {
    state: [u8; 32],
    seq: u64,
}

impl AnchorChain {
    /// Bootstrap with a deterministic genesis-anchor : BLAKE3("loa-orch-v0").
    pub fn genesis() -> Self {
        let digest = blake3::hash(b"cssl-host-persistent-orchestrator/v0/genesis");
        Self {
            state: *digest.as_bytes(),
            seq: 0,
        }
    }

    /// Bootstrap with a custom seed — used by `restore` when replaying journal.
    pub fn from_state(state: [u8; 32], seq: u64) -> Self {
        Self { state, seq }
    }

    pub fn current_digest(&self) -> [u8; 32] {
        self.state
    }

    pub fn current_seq(&self) -> u64 {
        self.seq
    }

    /// Fold a new payload into the chain + emit the next AnchorRecord.
    pub fn fold(&mut self, payload: &[u8], at_ms: u64, reason: AnchorReason) -> AnchorRecord {
        let mut h = blake3::Hasher::new();
        h.update(&self.state);
        h.update(payload);
        h.update(&at_ms.to_le_bytes());
        let next = *h.finalize().as_bytes();
        self.state = next;
        self.seq = self.seq.saturating_add(1);
        AnchorRecord {
            seq: self.seq,
            digest: next,
            at_ms,
            reason,
        }
    }
}

impl Default for AnchorChain {
    fn default() -> Self {
        Self::genesis()
    }
}
