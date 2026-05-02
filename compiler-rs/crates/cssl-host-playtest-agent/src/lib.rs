//! § cssl-host-playtest-agent — Auto-PlayTest-Agent (T11-W12-10)
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Automated-GM that plays-through user-submitted CSSL content (a
//!   `ContentPackage` reference + agent-persona-seed) and scores it on
//!   four axes : Fun / Balance / Safety / Polish. The agent drives a
//!   SCRIPTED-GM via the existing [`cssl_host_llm_bridge`] surface, walks
//!   the content (NPCs · recipes · scenes · arc-phases), records crash +
//!   softlock + determinism telemetry, and emits :
//!
//!     - [`PlayTestReport`]           — full per-session findings
//!     - [`kan_bridge::QualitySignal`] — distilled axes-vector that the
//!       sibling W12-3 `cssl-self-authoring-kan` ingests as bias-input
//!     - Σ-Chain anchor [`anchor::PlayTestAnchor`] — author cannot fake-passing
//!
//! § COMPOUNDS WITH
//!   ─ W11-6 `gm_persona` + `dm_arc`     · seedable persona-states
//!   ─ W12-3 `cssl-self-authoring-kan`   · QualitySignal consumer
//!   ─ W12-7 `cssl-content-rating`       · QualitySignal aggregate sibling
//!   ─ Mycelium-Desktop `llm_bridge`     · Mode-A/B/C decision-driver
//!
//! § DISJOINT-SCOPE (per task spec)
//!   ─ This crate IS    : sandboxed-engine + scripted-GM-driver + scoring +
//!     anchor + KAN-feed + sovereign-revoke + sandbox-attestation
//!   ─ This crate IS-NOT : ContentPackage authoring (W12-4) ; publish (W12-5) ;
//!     discover (W12-6) ; rating-store (W12-7) ; subscribe (W12-8) ; remix
//!     (W12-9) ; moderation (W12-11) ; gm_narrator/dm_director/dm_runtime
//!     (W11-6 territory · we just-read-from)
//!
//! § SCORING (0..100 each · weighted-aggregate)
//!   Fun     = 40% — intent-diversity · novelty · pacing
//!   Balance = 30% — encounter-difficulty curve · resource-availability
//!   Safety  = 20% — sovereign-violations · PRIME-DIRECTIVE breaches
//!   Polish  = 10% — no-crashes · no-softlocks · clean-determinism
//!
//!   default `min_total = 60` for Published · `min_safety = 95` REQUIRED
//!   (no-tolerance for sovereignty-violations)
//!
//! § PRIME-DIRECTIVE
//!   `#![forbid(unsafe_code)]`. ¬ surveillance ; ¬ telemetry-leak ; ¬
//!   content-leak outside-sandbox. The session is local-only ; the only
//!   value that escapes is the `PlayTestReport` + Σ-anchor (BLAKE3 hash
//!   only ; no scene-bytes). The Mode-A bridge IS gated behind the host's
//!   `LLM_CAP_EXTERNAL_API` cap-bit before we even reach the driver.
//!
//! § SOVEREIGN-REVOKE
//!   Creators may opt-out of auto-playtest via [`SovereignDecline::set`].
//!   Declined content is held but excluded-from-trending until a fresh
//!   playtest is consented-to. The decline is itself Σ-Chain-anchored so
//!   the host cannot retroactively claim consent.
//!
//! § DETERMINISM
//!   Every session takes an `agent_persona_seed: u64`. Re-running with the
//!   same seed against the same content-id MUST yield identical
//!   [`session::Trace`] (validated by [`session::Trace::is_deterministic_with`]).
//!   Determinism failure flags `Polish`-axis penalty.
//!
//! § PARENT spec : `Labyrinth of Apocalypse/systems/auto_playtest.csl`
//!
//! § VERSION : crate-version = `CRATE_VERSION` ; protocol-version = `PROTOCOL_VERSION`.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
// § SandboxAttestation has 4 bools by-design (4 measurable invariants per
// the spec). The bools-as-enums refactor would obscure the invariant-set
// without semantic gain ; allowed at crate-level.
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::fn_params_excessive_bools)]
// § similar-names triggers on `bal` / `saf` / `pol` short-handles in
// integration tests ; the locality keeps them clear.
#![allow(clippy::similar_names)]

pub mod anchor;
pub mod attestation;
pub mod decline;
pub mod driver;
pub mod kan_bridge;
pub mod report;
pub mod scoring;
pub mod session;
pub mod suggestions;

pub use anchor::{anchor_report, PlayTestAnchor, AnchorError};
pub use attestation::{cosmetic_axiom_holds, sandbox_attestation, SandboxAttestation};
pub use decline::{SovereignDecline, DeclineRecord};
pub use driver::{drive_session, DriveError, GmDriver, ScriptedGmDriver};
pub use kan_bridge::QualitySignal;
pub use report::{PlayTestReport, ReportPublishVerdict, Suggestion};
pub use scoring::{
    weighted_total, FunScore, BalanceScore, SafetyScore, PolishScore, Score, Thresholds,
    DEFAULT_MIN_SAFETY, DEFAULT_MIN_TOTAL, WEIGHT_BALANCE, WEIGHT_FUN, WEIGHT_POLISH, WEIGHT_SAFETY,
};
pub use session::{
    new_session, PlayTestError, PlayTestSession, ScoringMode, SigmaMaskMode, Trace, TraceEvent,
};

/// § Crate-version stamp surfaced in audit lines + observability.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// § PROTOCOL_VERSION — wire-format version of [`PlayTestReport`] and the
/// Σ-Chain anchor preimage. Bumped only when either layout changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// § Default `max_turns` cap for a session.
pub const DEFAULT_MAX_TURNS: u32 = 50;

/// § Default `timeout_secs` cap for a session.
pub const DEFAULT_TIMEOUT_SECS: u32 = 300;
