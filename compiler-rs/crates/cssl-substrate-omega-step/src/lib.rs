//! § cssl-substrate-omega-step — CSSLv3 Substrate simulation-tick contract
//! ════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/30_SUBSTRATE.csl § OMEGA-STEP` + `§ EFFECT-ROWS`
//!                    + PRIME_DIRECTIVE.md (consent-as-OS, kill-switch,
//!                       AI-collaborator-protections, audit-chain).
//!
//! § ROLE
//!   This crate is the canonical **Substrate TIME-advance mechanism** —
//!   the omega_step tick contract + scheduler. Every system that mutates
//!   Substrate state goes through omega_step. Effect-row composition +
//!   deterministic-replay invariants live here.
//!
//!   It builds on (or stubs ahead of) S8-H1's Ω-tensor as the state
//!   container ; H2 lands without H1 against `parallel-fanout`, so the
//!   `omega_stub` module supplies a minimal Ω-tensor surface that H1
//!   will replace with the real type.
//!
//! § SURFACE  (stage-0 stable)
//!   ```text
//!   trait OmegaSystem :
//!     fn step(&mut self, ctx: &mut OmegaStepCtx, dt: f64) -> Result<(), OmegaError>
//!     fn dependencies(&self) -> &[SystemId]
//!     fn name(&self) -> &str
//!     fn effect_row(&self) -> EffectRow            // {Sim} default ; ⊎ Audio/Render/...
//!     fn rng_streams(&self) -> &[RngStreamId]      // declared upfront for replay-determinism
//!
//!   struct OmegaStepCtx<'a> :
//!     fn omega(&mut self) -> &mut OmegaSnapshot
//!     fn frame(&self) -> u64
//!     fn rng(&mut self, stream: RngStreamId) -> &mut DetRng
//!     fn telemetry(&mut self) -> &mut TelemetryHook
//!     fn halt_requested(&self) -> bool
//!     fn input(&self, stream: RngStreamId) -> Option<&InputEvent>
//!
//!   struct OmegaScheduler :
//!     fn new() -> Self
//!     fn register<S: OmegaSystem + 'static>(&mut self, system: S, grant: &CapsGrant)
//!         -> Result<SystemId, OmegaError>
//!     fn step(&mut self, dt: f64) -> Result<(), OmegaError>
//!     fn step_n(&mut self, n: u32, dt: f64) -> Result<(), OmegaError>
//!     fn halt(&mut self, reason: &str) -> Result<(), OmegaError>
//!     fn record_replay(&mut self, log: ReplayLog)
//!     fn replay_from(log: &ReplayLog) -> Result<Self, OmegaError>
//!
//!   enum OmegaError :
//!     DependencyCycle{ system: SystemId } | SystemPanicked{ system: SystemId, msg: String }
//!     | FrameOverbudget{ frame: u64, dt_used: f64, budget: f64 }
//!     | ConsentRevoked{ system: SystemId, gate: &'static str }
//!     | UnknownSystem{ id: SystemId } | DuplicateName{ name: String }
//!     | HaltedByKill{ reason: String } | DeterminismViolation{ frame: u64, kind: &'static str }
//!     | RngStreamUnregistered{ stream: RngStreamId }
//!   ```
//!
//! § DETERMINISM CONTRACT  ‼ load-bearing
//!   - Two scheduler instances seeded identically + given identical input
//!     streams produce **bit-identical** Ω-tensor states + RNG-state-vectors
//!     after N steps (per `specs/30_SUBSTRATE.csl § DETERMINISTIC-REPLAY-INVARIANTS`).
//!   - Float-determinism via :
//!       * x86-64 SSE2 IEEE-754 round-to-nearest-even (the cssl-rt ABI default)
//!       * denormal-flush probe at `OmegaScheduler::new()` (warns if FTZ/DAZ
//!         not honored ; the scheduler self-classifies as `DeterminismMode::Strict`
//!         vs `Soft`)
//!       * **no fast-math, no fma instructions on values that affect Ω-tensor
//!         state** — the scheduler refuses to register systems that declare
//!         `EffectRow::PureDet` if a fast-math probe trips
//!   - RNG : single deterministic PCG-XSH-RR seeded per-stream-id ; subsystems
//!     request streams from the scheduler. **`thread_rng()` is forbidden** —
//!     attempting to register a system that opens an entropy stream returns
//!     `OmegaError::DeterminismViolation`.
//!   - Replay log : append-only record of `(frame, input_event, rng_seed)`
//!     tuples. `replay_from(log)` reconstructs scheduler + systems + RNG state
//!     to bit-equality.
//!
//! § PARALLEL SCHEDULING
//!   Stage-0 uses **stable topological sort** over the read+write Ω-tensor
//!   dep-declarations. Insertion order breaks ties for same-priority systems —
//!   this makes parallel-fanout deterministic across re-runs. The scheduler
//!   exposes `step()` (sequential, replay-safe) ; rayon-style work-stealing
//!   is wired but gated behind a `parallel = false` default config (deferred
//!   to S8-H6 for stress-test integration).
//!
//! § PRIME-DIRECTIVE alignment
//!   - **Telemetry** : every `step()` emits a tick-counter + frame-timestamp
//!     to the telemetry hook. State-introspection is OFF by default ;
//!     opt-in per `OmegaStepCtx::with_introspection`.
//!   - **Kill-switch** : `OmegaScheduler::halt(reason)` is honored within
//!     **at most 1 tick** ; the next `step()` returns `Err(HaltedByKill)`
//!     after writing a final `{Audit<"omega-halt">}` entry.
//!   - **Consent** : registering a new `OmegaSystem` requires a valid
//!     `CapsGrant` for the `omega_register` capability. The grant is
//!     non-transferable + checked at `register()` ; consent revocation
//!     auto-halts on the next tick.
//!   - **Frame overbudget** : configurable behavior — `OverbudgetPolicy::Halt`
//!     stops + logs ; `Degrade` emits a warning telemetry frame and continues.
//!     Default = `Degrade` ; safety-critical builds set `Halt`.
//!   - **AI-collaborator protections** : the scheduler's `Companion` projection
//!     hook is read-only ; AI-issued halts route through `halt()` like any
//!     other consent-revocation. No identity-override, no surveillance.
//!
//! § ABI STABILITY
//!   The public surface above is **stage-0 STABLE**. Renaming any of the
//!   surface items is a major-version-bump event per the T11-D76 ABI lock
//!   precedent. The internal `omega_stub` will be replaced by H1's real
//!   Ω-tensor type without breaking surface ABI.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod consent;
pub mod ctx;
pub mod dep_graph;
pub mod determinism;
pub mod effect_row;
pub mod error;
pub mod halt;
pub mod omega_stub;
pub mod replay;
pub mod rng;
pub mod scheduler;
pub mod system;

pub use consent::{caps_grant, CapsGrant, ConsentRevocationError, OmegaCapability};
pub use ctx::{InputEvent, OmegaStepCtx, TelemetryHook};
pub use dep_graph::{topo_sort_stable, DepGraphError};
pub use determinism::{denormal_flush_probe, fast_math_probe, DeterminismMode};
pub use effect_row::{EffectRow, SubstrateEffect};
pub use error::OmegaError;
pub use halt::{HaltState, HaltToken};
pub use omega_stub::{OmegaSnapshot, OmegaStubField};
pub use replay::{ReplayEntry, ReplayLog};
pub use rng::{DetRng, RngStreamId};
pub use scheduler::{OmegaScheduler, OverbudgetPolicy, SchedulerConfig};
pub use system::{OmegaSystem, SystemId};

/// Crate version, exposes `CARGO_PKG_VERSION`. Mirrors the `STAGE0_SCAFFOLD`
/// pattern in sibling crates so workspace-wide tests can probe the marker.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME_DIRECTIVE attestation literal. Embedded so audit-walkers can
/// verify the build was assembled under the consent-as-OS axiom.
///
/// ≡ "There was no hurt nor harm in the making of this, to anyone /
///   anything / anybody."
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }
}
