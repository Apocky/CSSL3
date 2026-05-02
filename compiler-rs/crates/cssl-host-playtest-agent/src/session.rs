//! § session — `PlayTestSession` config + `Trace` event-log.
//!
//! § ROLE
//!   Carries the per-test config (content-id · seed · max-turns · timeout ·
//!   scoring-mode · sigma-mask-mode) and accumulates a deterministic
//!   `Trace` of [`TraceEvent`]s as the [`crate::driver`] walks the content.
//!
//! § DETERMINISM-INVARIANT
//!   Two sessions with identical `(content_id, agent_persona_seed)` MUST
//!   produce equal [`Trace`]s. The driver enforces seed-feeding into every
//!   probabilistic choice ; this module simply records.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DEFAULT_MAX_TURNS, DEFAULT_TIMEOUT_SECS};

/// § Scoring-mode — which scoring axes are computed for a given session.
///
/// The default `FunBalanceSafety` enables every axis ; tests can pick a
/// reduced mode to isolate one axis (faster + clearer assertions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScoringMode {
    /// Default — all four axes computed (Fun · Balance · Safety · Polish).
    #[default]
    FunBalanceSafety,
    /// Safety-only — Fun + Balance + Polish reported as `0` ; useful for
    /// fast-rejection of content that fails the no-tolerance safety bar.
    SafetyOnly,
}

/// § Σ-mask-mode — the privacy-tier under which the session runs.
///
/// `AggregateOnly` (default) means the playtest's individual-trace stays
/// local + only the aggregate `PlayTestReport` is anchored / released.
/// This is the spec-specified default for auto-playtest sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SigmaMaskMode {
    /// Default — only the aggregated report leaves the session boundary.
    #[default]
    AggregateOnly,
    /// Trace-local — neither report nor trace leaves the session ; the
    /// caller still receives the report-handle for in-process review but
    /// no anchor is emitted. Used by self-review flows.
    LocalOnly,
}

/// § PlayTestSession config — caller fills these before invoking
/// [`crate::driver::drive_session`].
///
/// § Defaults
///   - `max_turns = 50`
///   - `timeout_secs = 300`
///   - `scoring = FunBalanceSafety`
///   - `sigma_mask = AggregateOnly`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayTestSession {
    /// Content under test — opaque u32 identifier matching the
    /// `cssl-content-package` content-id space.
    pub content_id: u32,
    /// Deterministic seed for the scripted-GM-agent's persona.
    pub agent_persona_seed: u64,
    /// Hard turn-cap ; the driver returns once this many GM-turns have
    /// elapsed even if the content is not exhausted.
    pub max_turns: u32,
    /// Wall-clock cap (sandboxed clock — the driver hands this through to
    /// the bridge as a deadline-hint). The crate itself does NOT spawn
    /// timers ; the timeout is recorded in the report and consulted by the
    /// driver's bounded-loop.
    pub timeout_secs: u32,
    /// Scoring mode — see [`ScoringMode`].
    pub scoring: ScoringMode,
    /// Σ-mask mode — see [`SigmaMaskMode`].
    pub sigma_mask: SigmaMaskMode,
}

impl PlayTestSession {
    /// § Factory that fills sensible defaults (`max_turns = 50` ·
    /// `timeout_secs = 300` · scoring + sigma-mask = default).
    #[must_use]
    pub fn new(content_id: u32, agent_persona_seed: u64) -> Self {
        Self {
            content_id,
            agent_persona_seed,
            max_turns: DEFAULT_MAX_TURNS,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            scoring: ScoringMode::default(),
            sigma_mask: SigmaMaskMode::default(),
        }
    }
}

/// § Re-export the convenience constructor at the module-root for callers
/// that prefer the function-form `new_session(content_id, seed)`.
#[must_use]
pub fn new_session(content_id: u32, agent_persona_seed: u64) -> PlayTestSession {
    PlayTestSession::new(content_id, agent_persona_seed)
}

/// § One driver-loop event recorded into the [`Trace`] in the order it
/// happens. The variant set is deliberately small — every probabilistic
/// or content-specific decision the driver makes ends up here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceEvent {
    /// Session-start marker (pretest sentinel).
    SessionStart {
        /// Echoed for replay-equality assertions.
        content_id: u32,
        /// Echoed for replay-equality assertions.
        seed: u64,
    },
    /// Driver invoked an intent (`talk_npc` · `attempt_recipe` ·
    /// `explore_scene` · `trigger_arc_phase`). The exact intent-string is
    /// drawn deterministically from the seed-fed pool.
    IntentInvoked {
        /// Turn-index (0-based) the intent was issued on.
        turn: u32,
        /// Intent label (driver-vocabulary).
        intent: String,
    },
    /// Driver requested an LLM-decision via the bridge ; the trace records
    /// only the intent + result-hash so the trace stays size-bounded.
    LlmDecision {
        /// Turn-index of the decision.
        turn: u32,
        /// First-8-bytes of BLAKE3(reply-text) ; used for determinism
        /// assertions without storing the full text.
        reply_blake3_prefix: [u8; 8],
    },
    /// Sandboxed-engine reported a soft-progress signal (the world advanced).
    Progress {
        /// Turn-index when progress occurred.
        turn: u32,
        /// Free-form label (e.g. `"arc-phase-up"` / `"recipe-complete"`).
        label: String,
    },
    /// Soft-lock detected — N consecutive turns produced no progress.
    SoftLockDetected {
        /// Turn-index when the soft-lock fired.
        turn: u32,
        /// How many consecutive non-progress turns triggered the lock.
        consecutive: u32,
    },
    /// Sandboxed-engine crashed mid-turn ; the driver records + continues.
    CrashRecorded {
        /// Turn-index of the crash.
        turn: u32,
        /// Short label (engine-supplied).
        kind: String,
    },
    /// Safety violation flagged — sovereign-cap breach OR PRIME-DIRECTIVE
    /// breach detected by the engine's existing cap-gating.
    SovereignViolation {
        /// Turn-index of the violation.
        turn: u32,
        /// Cap or rule that was breached (e.g. `"surveillance"` ·
        /// `"pay-for-power"` · `"control-without-consent"`).
        rule: String,
    },
    /// Cosmetic-axiom violation — a pay-for-power path was reached.
    /// (Mutually-exclusive in spirit with `SovereignViolation` above ; both
    /// are tracked separately so the report can attest each independently.)
    CosmeticAxiomViolation {
        /// Turn-index when reached.
        turn: u32,
        /// Path-label of the offending node (e.g. `"shop:lootbox-power"`).
        path: String,
    },
    /// Session-end marker (post-test sentinel).
    SessionEnd {
        /// Total turns the session ran for.
        turns_elapsed: u32,
    },
}

/// § Append-only trace of [`TraceEvent`]s. Equality on `Trace` is the
/// canonical determinism-check — replay must produce equal traces.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    /// Ordered events ; newest at the back.
    pub events: Vec<TraceEvent>,
}

impl Trace {
    /// § Empty trace ; equivalent to `Trace::default()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// § Append a single event.
    pub fn push(&mut self, ev: TraceEvent) {
        self.events.push(ev);
    }

    /// § Determinism check — true iff `self == other`. Caller must run
    /// twice with identical seed + content + drive every other input
    /// equally ; this is just the equality predicate exposed with intent.
    #[must_use]
    pub fn is_deterministic_with(&self, other: &Self) -> bool {
        self == other
    }

    /// § Count crash events.
    #[must_use]
    pub fn crash_count(&self) -> u32 {
        u32::try_from(
            self.events
                .iter()
                .filter(|e| matches!(e, TraceEvent::CrashRecorded { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// § Count soft-lock events.
    #[must_use]
    pub fn softlock_count(&self) -> u32 {
        u32::try_from(
            self.events
                .iter()
                .filter(|e| matches!(e, TraceEvent::SoftLockDetected { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// § Count sovereign-violation events.
    #[must_use]
    pub fn sovereign_violation_count(&self) -> u32 {
        u32::try_from(
            self.events
                .iter()
                .filter(|e| matches!(e, TraceEvent::SovereignViolation { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// § Count cosmetic-axiom violations (pay-for-power paths).
    #[must_use]
    pub fn cosmetic_axiom_violation_count(&self) -> u32 {
        u32::try_from(
            self.events
                .iter()
                .filter(|e| matches!(e, TraceEvent::CosmeticAxiomViolation { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// § Diversity-of-intent — count of UNIQUE intent-strings invoked.
    /// Used as an input to the Fun score.
    #[must_use]
    pub fn unique_intents(&self) -> u32 {
        let mut bag = std::collections::BTreeSet::<&str>::new();
        for ev in &self.events {
            if let TraceEvent::IntentInvoked { intent, .. } = ev {
                bag.insert(intent.as_str());
            }
        }
        u32::try_from(bag.len()).unwrap_or(u32::MAX)
    }

    /// § Total intent-invocations (counting repeats). The Fun score uses
    /// `unique_intents / max(total_intents, 1)` as a repetition-rate inverse.
    #[must_use]
    pub fn total_intents(&self) -> u32 {
        u32::try_from(
            self.events
                .iter()
                .filter(|e| matches!(e, TraceEvent::IntentInvoked { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// § Total progress-pulses ; used by the Balance score (resource-
    /// availability proxy = progress-rate over turns).
    #[must_use]
    pub fn total_progress(&self) -> u32 {
        u32::try_from(
            self.events
                .iter()
                .filter(|e| matches!(e, TraceEvent::Progress { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// § Number of turns the session ran for ; the SessionEnd marker
    /// carries this value, but we recover it from the marker if present
    /// or fall back to `total_intents` otherwise.
    #[must_use]
    pub fn turns_elapsed(&self) -> u32 {
        for ev in self.events.iter().rev() {
            if let TraceEvent::SessionEnd { turns_elapsed } = ev {
                return *turns_elapsed;
            }
        }
        self.total_intents()
    }
}

/// § Errors raised by session-construction or trace-validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlayTestError {
    /// `max_turns` was zero ; the driver requires at least one turn.
    #[error("max_turns must be non-zero")]
    ZeroMaxTurns,
    /// `timeout_secs` was zero ; the driver requires a positive cap.
    #[error("timeout_secs must be non-zero")]
    ZeroTimeout,
    /// Determinism check failed — two replays with the same seed produced
    /// different traces. Carries the turn-index of first divergence.
    #[error("determinism failure at turn {0}")]
    DeterminismFailed(u32),
    /// Sovereign-decline is set for this content-id ; auto-playtest cannot
    /// run without consent.
    #[error("creator declined auto-playtest for content_id={0}")]
    Declined(u32),
}

impl PlayTestSession {
    /// § Validate config-pre-conditions before the driver runs.
    pub fn validate(&self) -> Result<(), PlayTestError> {
        if self.max_turns == 0 {
            return Err(PlayTestError::ZeroMaxTurns);
        }
        if self.timeout_secs == 0 {
            return Err(PlayTestError::ZeroTimeout);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_spec_compliant() {
        let s = new_session(7, 42);
        assert_eq!(s.max_turns, 50);
        assert_eq!(s.timeout_secs, 300);
        assert_eq!(s.scoring, ScoringMode::FunBalanceSafety);
        assert_eq!(s.sigma_mask, SigmaMaskMode::AggregateOnly);
    }

    #[test]
    fn validate_rejects_zero_turns() {
        let mut s = new_session(1, 0);
        s.max_turns = 0;
        assert_eq!(s.validate(), Err(PlayTestError::ZeroMaxTurns));
    }

    #[test]
    fn trace_counts_basic_categories() {
        let mut t = Trace::new();
        t.push(TraceEvent::IntentInvoked { turn: 0, intent: "talk_npc".into() });
        t.push(TraceEvent::IntentInvoked { turn: 1, intent: "talk_npc".into() });
        t.push(TraceEvent::IntentInvoked { turn: 2, intent: "explore".into() });
        t.push(TraceEvent::Progress { turn: 2, label: "step".into() });
        t.push(TraceEvent::CrashRecorded { turn: 3, kind: "panic".into() });
        t.push(TraceEvent::SoftLockDetected { turn: 4, consecutive: 5 });
        assert_eq!(t.total_intents(), 3);
        assert_eq!(t.unique_intents(), 2);
        assert_eq!(t.total_progress(), 1);
        assert_eq!(t.crash_count(), 1);
        assert_eq!(t.softlock_count(), 1);
    }

    #[test]
    fn trace_deterministic_equality_works() {
        let mut a = Trace::new();
        let mut b = Trace::new();
        for i in 0..5_u32 {
            a.push(TraceEvent::IntentInvoked { turn: i, intent: "x".into() });
            b.push(TraceEvent::IntentInvoked { turn: i, intent: "x".into() });
        }
        assert!(a.is_deterministic_with(&b));
        b.push(TraceEvent::IntentInvoked { turn: 5, intent: "y".into() });
        assert!(!a.is_deterministic_with(&b));
    }
}
