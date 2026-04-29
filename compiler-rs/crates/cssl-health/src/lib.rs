// § T11-D159 (W-Jζ-3) : cssl-health crate root
// ══════════════════════════════════════════════════════════════════════════
//
// § Overview
//
// `cssl-health` exposes the **`HealthProbe`** trait + a worst-case-monoid
// **`engine_health()`** aggregator + 12 **mock** per-subsystem probe-impls
// for the substrate-evolution + signature-rendering crates landed in
// Wave-4 (T11-D113..D125). Spec-source : `_drafts/phase_j/06_l2_telemetry_spec.md`
// § V (PILLAR-3.5 health-registry).
//
// § Mock-impl rationale
//
// The 12 subsystem-crates are landing in parallel-fanout and may not all
// be merged when this crate compiles. To avoid cyclic-dep-risk + fanout
// ordering-friction, every probe in `probes::*` is a **toy** returning
// deterministic Ok/Degraded based on a small in-process `MockState`.
//
// Real integration (per spec § V.4) is a follow-up slice : each
// subsystem-crate impls `HealthProbe` itself and uses `#[ctor]` to
// register into a global `HEALTH_REGISTRY`. This crate ships the
// **trait + aggregator + reference toy-impls** so that downstream
// consumers (MCP `cssl_engine_health` tool @ Wave-Jθ-4) can wire-up
// against a stable surface today.
//
// § Effect-row note
//
// `health()` per spec is annotated `{ Telemetry<Counters>, Pure }` and
// `degrade()` is `{ Telemetry<Counters>, Audit<"subsystem-degrade"> }`.
// Effect-row enforcement lives in `cssl-effects` (not in this leaf-crate).
//
// § ATTESTATION (PRIME_DIRECTIVE.md § 11)
// There was no hurt nor harm in the making of this, to anyone, anything,
// or anybody.

#![forbid(unsafe_code)]

pub mod aggregator;
pub mod probes;
pub mod status;

pub use aggregator::{engine_health, HealthAggregate, HealthRegistry};
pub use probes::{HealthError, HealthProbe};
pub use status::{HealthFailureKind, HealthStatus};

/// Build-attestation per `PRIME_DIRECTIVE.md` § 11.
///
/// Embedded as a `pub const` so consumers can verify the attestation
/// surfaces in the binary at link-time (see `cssl-cli --attestation`
/// for the orchestrator-side surface).
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Slice-ID : ties this crate to the originating dispatch-slice in the
/// session-12 plan. Surfaces in audit-bundles + DECISIONS.md cross-refs.
pub const SLICE_ID: &str = "T11-D159";

/// Wave-tag : ties this crate to its Phase-J wave-position.
pub const WAVE_TAG: &str = "W-Jζ-3";

/// The canonical list of subsystem-names this crate ships toy-probes for.
/// Order matches the registration order in [`probes::register_all_mock`].
pub const SUBSYSTEMS: [&str; 12] = [
    "cssl-render-v2",
    "cssl-physics-wave",
    "cssl-wave-solver",
    "cssl-spectral-render",
    "cssl-fractal-amp",
    "cssl-gaze-collapse",
    "cssl-render-companion-perspective",
    "cssl-host-openxr",
    "cssl-anim-procedural",
    "cssl-wave-audio",
    "cssl-substrate-omega-field",
    "cssl-substrate-kan",
];

#[cfg(test)]
mod smoke {
    use super::{ATTESTATION, SLICE_ID, SUBSYSTEMS, WAVE_TAG};

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn slice_metadata() {
        assert_eq!(SLICE_ID, "T11-D159");
        assert_eq!(WAVE_TAG, "W-Jζ-3");
    }

    #[test]
    fn twelve_subsystems_listed() {
        assert_eq!(SUBSYSTEMS.len(), 12);
        let mut sorted = SUBSYSTEMS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 12, "duplicate subsystem-name in SUBSYSTEMS");
    }
}
