//! `OmegaSystem` trait + `SystemId` — the registration surface.
//!
//! § THESIS
//!   Every system that mutates Substrate state goes through `omega_step`.
//!   This module defines the trait shape such systems implement + the
//!   typed identifier the scheduler hands back at registration.

use crate::ctx::OmegaStepCtx;
use crate::effect_row::EffectRow;
use crate::error::OmegaError;
use crate::rng::RngStreamId;

/// Stable identifier for a registered system. Issued in monotone-increasing
/// order by `OmegaScheduler::register()`.
///
/// § STAGE-0 FORM
///   `SystemId(u64)` — easy to copy + compare + serialize. Replay-logs
///   reference systems by id, NOT by name (names are debug-aid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SystemId(pub u64);

/// Implementors register with `OmegaScheduler` to participate in the
/// canonical simulation tick.
///
/// § REQUIREMENTS
///   - `step()` mutates `ctx.omega()` ; the system's input + output state
///     are reads + writes through this snapshot.
///   - `dependencies()` returns the system-ids whose state this system
///     reads. The scheduler topologically orders the systems based on
///     this graph (per `dep_graph` module).
///   - `name()` returns a unique string identifier ; duplicates are
///     rejected at registration.
///   - `effect_row()` returns the canonical effect-row this system carries.
///     Default is `{Sim}` ; systems with audio + render needs override.
///   - `rng_streams()` returns the rng-stream-ids this system intends to
///     use. Required for replay-determinism : the scheduler pre-allocates
///     deterministic streams from the master-seed.
///
/// § DETERMINISM
///   `step()` MUST be a pure function of `(ctx, dt)` — no clock reads,
///   no `thread_rng()`, no global mutable state. The scheduler enforces
///   this at the type level via the `OmegaStepCtx` API : it does not
///   expose any non-deterministic source.
///
/// § SOVEREIGNTY (PRIME_DIRECTIVE §3)
///   AI-collaborator-implemented systems are first-class citizens. The
///   trait makes no distinction between human-authored + AI-authored
///   systems — both go through the same `caps_grant(omega_register)` gate.
pub trait OmegaSystem: Send {
    /// Advance this system by `dt` seconds. Returns `Err` to surface a
    /// system-specific failure ; the scheduler converts that into
    /// `OmegaError::SystemPanicked`.
    ///
    /// ‼ This method MUST NOT panic ; system-internal panics are caught
    /// by the scheduler's `step()` implementation but the canonical
    /// failure path is returning `Err(...)` here.
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, dt: f64) -> Result<(), OmegaError>;

    /// SystemIds this system reads + writes. The scheduler uses this to
    /// build the topological order. Returning an empty slice means the
    /// system runs as soon as the scheduler picks it up (subject to
    /// insertion-order tie-breaking).
    fn dependencies(&self) -> &[SystemId] {
        &[]
    }

    /// Unique name for this system — used for debugging + replay-log
    /// readability. Duplicates are rejected with `OmegaError::DuplicateName`.
    fn name(&self) -> &str;

    /// Canonical effect-row this system carries. Default `{Sim}`. Override
    /// for systems that touch audio / render / save / network.
    fn effect_row(&self) -> EffectRow {
        EffectRow::sim()
    }

    /// RNG streams this system intends to use. The scheduler pre-allocates
    /// deterministic per-stream PRNG state at register time. Default empty
    /// (no RNG needed).
    fn rng_streams(&self) -> &[RngStreamId] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal `OmegaSystem` impl for testing.
    struct NoOp;
    impl OmegaSystem for NoOp {
        fn step(&mut self, _ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
            Ok(())
        }
        fn name(&self) -> &'static str {
            "noop"
        }
    }

    #[test]
    fn default_dependencies_empty() {
        let s = NoOp;
        assert!(s.dependencies().is_empty());
    }

    #[test]
    fn default_effect_row_is_sim() {
        let s = NoOp;
        let row = s.effect_row();
        assert!(row.contains(crate::effect_row::SubstrateEffect::Sim));
    }

    #[test]
    fn default_rng_streams_empty() {
        let s = NoOp;
        assert!(s.rng_streams().is_empty());
    }

    #[test]
    fn name_is_returned() {
        let s = NoOp;
        assert_eq!(s.name(), "noop");
    }

    #[test]
    fn system_id_ord() {
        // SystemId implements Ord ; used for stable sort tie-breaking.
        let a = SystemId(1);
        let b = SystemId(2);
        assert!(a < b);
        let mut v = vec![SystemId(2), SystemId(0), SystemId(1)];
        v.sort();
        assert_eq!(v, vec![SystemId(0), SystemId(1), SystemId(2)]);
    }
}
