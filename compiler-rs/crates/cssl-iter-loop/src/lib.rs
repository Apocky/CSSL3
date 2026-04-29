// § cssl-iter-loop — LLM bug-fix iteration-loop foundation.
//
// § T11-D233 (W-Jι) : implements the iteration-loop fixtures + protocols
// documented in `_drafts/phase_j/wave_ji_iteration_loop_docs.md`. Provides
// the state-machine + fixture-extraction + spec-coverage gap-pick + perf-
// regression detection + live-debug orchestration that Wave-Jθ MCP tools
// will call into when an LLM (e.g. Claude-Code) attaches to a running
// engine and iterates on a bug.
//
// § Phase-J spec : `_drafts/phase_j/wave_ji_iteration_loop_docs.md`.
// § Predecessor : `_drafts/phase_j/08_l5_mcp_llm_spec.md` § 10 (iteration-loop protocol).
// § Companion : `_drafts/phase_j/03_pod_composition_iteration_escalation.md`
//   (4-agent-pod definition).
//
// § Module layout
//
// ```text
// protocol.rs            — 9-step bug-fix iteration-loop state-machine
// fixture.rs             — runtime test-fixture extract → serialize → load → regression-test
// spec_coverage_driven.rs — gap-prioritization driven by cssl-spec-coverage report
// perf_regression.rs     — metric-history baseline-vs-current detection
// live_debug.rs          — pause / step / inspect / tweak orchestration
// ```
//
// § Σ-discipline
//
// This crate is a STATE-MACHINE + FIXTURE layer. It does not directly touch
// Σ-mask cells. Cell-touching always routes through cssl-inspect (which
// already enforces Σ-mask refusal) ; this crate's job is to coordinate the
// iteration-loop SHAPE — when to pause, when to extract a fixture, when to
// rank gaps, when to flag a perf-regression. The privacy-discipline that
// applies at this layer :
//
//   - RuntimeFixture serialization NEVER includes raw paths (BLAKE3 hashes
//     only, per D130) ; biometric-marked snapshots are REFUSED at extract.
//   - LiveDebugSession.tweak_value goes through `cssl-tweak::TunableRegistry`
//     which already routes through Cap<Tweak> + range-check + replay-log.
//   - Perf-regression `MetricHistory` is consumed at the report-shape level ;
//     biometric-metrics never reach this layer because cssl-metrics filters
//     them at registry-construction (cap-filtered list_metrics).
//   - All FailureReason / IssueReport text that flows into IterationLoopState
//     carries no biometric / sovereign-private content.
//
// § Integration points (swap when upstream lands)
//
//   - § INTEGRATION-POINT D233/01 : `McpSessionStub` ⇒ swap to real
//     `cssl_mcp_server::Session` when S2-A2 D229 lands.
//   - § INTEGRATION-POINT D233/02 : `EngineState` ⇒ swap to real
//     `cssl_substrate_omega_field::EngineStateSnapshot` when that crate lands.
//   - § INTEGRATION-POINT D233/03 : `OmegaSnapshotStub` ⇒ swap to real
//     `cssl_substrate_omega_field::OmegaFieldSnapshot`.
//   - § INTEGRATION-POINT D233/04 : `CreatureSnapshotStub` ⇒ swap to real
//     `cssl_creature_behavior::CreatureSnapshot` when that crate lands.
//
// § ATTESTATION (PRIME_DIRECTIVE.md § 11)
//   There was no hurt nor harm in the making of this, to anyone, anything,
//   or anybody.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]
#![allow(clippy::float_cmp)]
// Match sibling-crate stage-0 stance — the workspace clippy config below
// is set to deny these by default but we keep our builder-pattern + match-
// over-Option idioms readable rather than chasing every lint.
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::neg_cmp_op_on_partial_ord)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::single_match_else)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::format_push_string)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::needless_collect)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_for_each)]
#![allow(clippy::single_match)]
#![allow(clippy::if_not_else)]
#![allow(clippy::let_and_return)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::iter_without_into_iter)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unused_self)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::single_char_lifetime_names)]
#![allow(clippy::uninlined_format_args)]

pub mod fixture;
pub mod live_debug;
pub mod perf_regression;
pub mod protocol;
pub mod spec_coverage_driven;

pub use fixture::{
    CreatureSnapshotStub, FixtureError, OmegaSnapshotStub, PathHash, RegressionTestCase,
    RuntimeFixture, TriggerEvent,
};
pub use live_debug::{
    EngineInspection, LiveDebugError, LiveDebugSession, LiveDebugStep, TunableValue,
};
pub use perf_regression::{
    compare_against_baseline, MetricHistory, PerfBaseline, PerfRegressionError, RegressionReport,
    RegressionSeverity, Sample,
};
pub use protocol::{
    CommitHash, EngineState, FailureReason, IssueReport, IssueSeverity, IterationLoopState,
    McpSessionStub, ProtocolError, ProtocolStateMachine, ReloadId, SessionId,
};
pub use spec_coverage_driven::{
    pick_largest_gap, rank_gaps, GapCoverageInput, GapPriority, GapRanking, SpecCoverageDrivenError,
};

/// Canonical PRIME-DIRECTIVE § 11 attestation. Embedded as a const so any
/// in-process drift is observable at the next iteration-loop checkpoint.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Slice identifier surfaced for crate-identity probes + audit-trail
/// correlation in any downstream tool that ingests iteration-loop events.
pub const SLICE_ID: &str = "T11-D233 (W-Jι) cssl-iter-loop";

/// Crate-wide convenience result type. Each module exports its own concrete
/// error variant ; the umbrella `IterLoopError` enum aggregates them so callers
/// that orchestrate multiple subsystems can return a single error-type.
pub type Result<T> = std::result::Result<T, IterLoopError>;

/// Aggregate error type covering every iteration-loop subsystem.
///
/// Variants delegate to per-module error enums so the caller retains the
/// finer-grained discriminants when needed (e.g. `if let
/// IterLoopError::Fixture(FixtureError::BiometricRefused) = err { … }`).
#[derive(Debug, thiserror::Error)]
pub enum IterLoopError {
    /// Failure inside the 9-step iteration-loop state-machine.
    #[error("protocol error : {0}")]
    Protocol(#[from] protocol::ProtocolError),

    /// Failure inside the runtime-fixture extract / serialize / load pipeline.
    #[error("fixture error : {0}")]
    Fixture(#[from] fixture::FixtureError),

    /// Failure inside the spec-coverage-driven gap-picker.
    #[error("spec-coverage-driven error : {0}")]
    SpecCoverageDriven(#[from] spec_coverage_driven::SpecCoverageDrivenError),

    /// Failure inside the perf-regression baseline-vs-current detector.
    #[error("perf-regression error : {0}")]
    PerfRegression(#[from] perf_regression::PerfRegressionError),

    /// Failure inside the live-debug pause/step/inspect/tweak orchestrator.
    #[error("live-debug error : {0}")]
    LiveDebug(#[from] live_debug::LiveDebugError),
}

#[cfg(test)]
mod sanity {
    use super::{ATTESTATION, SLICE_ID};

    #[test]
    fn attestation_present() {
        assert!(
            ATTESTATION.starts_with("There was no hurt nor harm"),
            "ATTESTATION drift would be a §11 violation",
        );
    }

    #[test]
    fn slice_id_canonical() {
        assert!(SLICE_ID.contains("T11-D233"));
        assert!(SLICE_ID.contains("W-Jι"));
        assert!(SLICE_ID.contains("cssl-iter-loop"));
    }
}
