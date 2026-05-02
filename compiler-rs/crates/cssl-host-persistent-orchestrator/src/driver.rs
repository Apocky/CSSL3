// § driver.rs — trait-bound drivers (composition seam to sibling crates).
//
// § why traits not direct deps
//   `cssl-host-self-author` + `cssl-host-playtest-agent` pull heavy LLM-bridge
//   transitive deps. The orchestrator must build cleanly + stay test-deterministic
//   in the workspace default. Traits give us a thin seam : a future
//   `cssl-host-persistent-orchestrator-wireup` crate can implement these traits
//   against the real driver crates while this crate stays lean.

use serde::{Deserialize, Serialize};

/// One quality-signal sample produced by playtest / GM-accept / sandbox-pass /
/// player-rating ingest paths. Mirrors `cssl-self-authoring-kan::QualitySignal`
/// in shape so adapter-crate wire-up is trivial.
///
/// `score` is `0..=10_000` Q14-fixed-point ; `template_id` + `archetype_id`
/// identify the (template × archetype) cell whose bias is being updated.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct QualitySignal {
    pub template_id: u32,
    pub archetype_id: u16,
    pub score_q14: i16,
    pub source_kind: u8,
    pub seq: u64,
}

/// SelfAuthorDriver — the orchestrator's seam to the self-authoring pipeline.
/// Default impl is `NoopDriver` (records-only). A future wireup crate
/// implements this trait against `cssl-host-self-author::SelfAuthorOrchestrator`.
pub trait SelfAuthorDriver {
    /// Run one self-author cycle. Returns a short-stat tag for journaling.
    /// Implementations MUST NOT panic. Errors are returned as `Err(String)`.
    fn run_self_author_cycle(&mut self, now_ms: u64, prompt_seed: u64) -> Result<String, String>;
}

/// PlaytestDriver — the orchestrator's seam to the playtest pipeline.
pub trait PlaytestDriver {
    /// Run one auto-playtest cycle. Returns a short-stat tag for journaling
    /// + a vector of QualitySignal samples to feed the KAN reservoir.
    fn run_playtest_cycle(
        &mut self,
        now_ms: u64,
        playtest_seed: u64,
    ) -> Result<(String, Vec<QualitySignal>), String>;
}

/// KanTickSink — drains the reservoir + applies bias updates.
pub trait KanTickSink {
    /// Submit one quality-signal to the KAN reservoir.
    fn submit(&mut self, signal: QualitySignal);

    /// Run one KAN tick. Returns the number of bias-updates emitted on this
    /// tick — used by the orchestrator to know when an Σ-Chain anchor is due
    /// (every `anchor_every_n_kan_updates` updates).
    fn tick(&mut self, now_ms: u64) -> u64;

    /// Snapshot a deterministic 32-byte digest of the current bias-state.
    /// The orchestrator anchors this digest on Σ-Chain at every cycle close.
    fn bias_digest(&self) -> [u8; 32];
}

/// MyceliumSyncDriver — federates chat-pattern shapes to peers.
pub trait MyceliumSyncDriver {
    /// Federate one mycelium-sync tick. Returns the count of pattern-deltas
    /// that were broadcast on this tick.
    fn tick(&mut self, now_ms: u64) -> Result<u64, String>;
}

/// NoopDriver — the default record-only driver used when no real driver
/// has been registered. Crucially, the orchestrator REMAINS USEFUL with
/// noop drivers : the journal still tracks scheduling, cap-decisions, +
/// throttle-state, so Apocky can audit the daemon-lifecycle even before
/// real drivers are wired in.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct NoopDriver {
    pub call_count: u64,
}

impl SelfAuthorDriver for NoopDriver {
    fn run_self_author_cycle(&mut self, now_ms: u64, prompt_seed: u64) -> Result<String, String> {
        self.call_count += 1;
        Ok(format!(
            "noop_self_author@{now_ms}_seed={prompt_seed}_n={}",
            self.call_count
        ))
    }
}

impl PlaytestDriver for NoopDriver {
    fn run_playtest_cycle(
        &mut self,
        now_ms: u64,
        seed: u64,
    ) -> Result<(String, Vec<QualitySignal>), String> {
        self.call_count += 1;
        // Emit one synthetic quality-signal so KAN-tick has work to drain.
        // Score is mid-range so it neither dominates nor zeros the reservoir.
        let signal = QualitySignal {
            template_id: (seed & 0xFFFF_FFFF) as u32,
            archetype_id: ((seed >> 32) & 0xFFFF) as u16,
            score_q14: 4096, // 0.25 in Q14 — a deliberately "meh" baseline
            source_kind: 0xFF, // 0xFF = noop synthetic source
            seq: self.call_count,
        };
        Ok((
            format!("noop_playtest@{now_ms}_n={}", self.call_count),
            vec![signal],
        ))
    }
}

impl KanTickSink for NoopDriver {
    fn submit(&mut self, _signal: QualitySignal) {
        self.call_count += 1;
    }

    fn tick(&mut self, _now_ms: u64) -> u64 {
        self.call_count += 1;
        // Pretend exactly one bias-update happens per tick. Production driver
        // returns the actual reservoir-drain count.
        1
    }

    fn bias_digest(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"noop-kan-digest");
        h.update(&self.call_count.to_le_bytes());
        *h.finalize().as_bytes()
    }
}

impl MyceliumSyncDriver for NoopDriver {
    fn tick(&mut self, _now_ms: u64) -> Result<u64, String> {
        self.call_count += 1;
        // Pretend one pattern-delta was broadcast per tick.
        Ok(1)
    }
}
