// § journal.rs — append-only crash-resilient journal.
//
// § thesis
//   Every cycle-tick produces ONE [`JournalEntry`]. Entries are appended to
//   an in-memory ring (capacity = `journal_ring_capacity`) AND serialized to
//   a host-supplied [`std::io::Write`] sink via [`JournalStore::flush_into`].
//   On crash + restart, the daemon supplies the persisted bytes to
//   [`JournalReplay::replay`] which reconstructs the schedule + anchor-chain
//   without re-running the cycles.
//
//   The orchestrator is decoupled from the persistence-medium so the same
//   journal works whether the host stores entries in :
//     - a flat append-only file under ~/.loa/orchestrator-journal.ndjson
//     - the W11-W10 Mycelium-Desktop's substrate-knowledge crate
//     - a Σ-Chain transport stream
//     - an in-memory test fixture
//
//   The persistence-medium is the host's responsibility ; the journal-crate
//   is just the typed-event-stream + replay-decoder.

use serde::{Deserialize, Serialize};

use crate::anchor::AnchorRecord;
use crate::cycles::CycleOutcome;

/// A single journal entry — every orchestrator decision produces exactly one.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub seq: u64,
    pub at_ms: u64,
    pub kind: JournalKind,
}

/// Discriminator for the entry payload.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum JournalKind {
    /// Daemon process started — first entry of every run.
    Bootstrap { attestation_blake3: [u8; 32] },
    /// One cycle ran (or was rejected) — the canonical "work happened" record.
    CycleOutcome(CycleOutcome),
    /// Σ-Chain anchor minted.
    Anchor(AnchorRecord),
    /// Sovereign-pause held.
    SovereignPause,
    /// Sovereign-resume cleared the pause.
    SovereignResume,
    /// Cap-policy mutation — Apocky granted / revoked a cap mid-run.
    CapPolicyChange { cap_name: String, granted: bool },
    /// Tick observed without any cycle running (idle / all-throttled tick).
    QuiescentTick { hint_summary: String },
}

/// In-memory append-only journal.
#[derive(Debug, Clone)]
pub struct JournalStore {
    entries: Vec<JournalEntry>,
    next_seq: u64,
    capacity: usize,
    /// Total entries appended over the daemon's lifetime — survives ring
    /// rotation so audit-log can correlate seq across compactions.
    pub total_appended: u64,
}

impl JournalStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity.min(1024)),
            next_seq: 1,
            capacity: capacity.max(1),
            total_appended: 0,
        }
    }

    pub fn append(&mut self, at_ms: u64, kind: JournalKind) -> JournalEntry {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        let entry = JournalEntry { seq, at_ms, kind };
        if self.entries.len() >= self.capacity {
            // Compact : drop the oldest 25% to amortize the cost.
            let drop_n = self.capacity / 4;
            self.entries.drain(0..drop_n);
        }
        self.entries.push(entry.clone());
        self.total_appended = self.total_appended.saturating_add(1);
        entry
    }

    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    pub fn last(&self) -> Option<&JournalEntry> {
        self.entries.last()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize all in-memory entries as newline-delimited JSON. Suitable for
    /// streaming into ~/.loa/orchestrator-journal.ndjson.
    pub fn to_ndjson(&self) -> Result<String, String> {
        let mut out = String::new();
        for e in &self.entries {
            let line = serde_json::to_string(e).map_err(|e| e.to_string())?;
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }
}

/// Replay-decoder. Parses NDJSON back into typed entries + reconstructs the
/// `next_seq` so a restarted daemon picks up where it left off.
pub struct JournalReplay;

impl JournalReplay {
    /// Parse an NDJSON-stream back into ordered [`JournalEntry`] records.
    pub fn replay(ndjson: &str) -> Result<Vec<JournalEntry>, String> {
        let mut out = Vec::new();
        for (i, line) in ndjson.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: JournalEntry = serde_json::from_str(line)
                .map_err(|e| format!("line {}: {}", i + 1, e))?;
            out.push(entry);
        }
        // Replay-determinism : sort by seq so out-of-order persistence is recovered.
        out.sort_by_key(|e| e.seq);
        Ok(out)
    }

    /// Compute the next-seq the daemon should use after replay.
    pub fn next_seq_after(entries: &[JournalEntry]) -> u64 {
        entries
            .iter()
            .map(|e| e.seq)
            .max()
            .map(|x| x.saturating_add(1))
            .unwrap_or(1)
    }
}
