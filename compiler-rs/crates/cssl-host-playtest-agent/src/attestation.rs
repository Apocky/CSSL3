//! § attestation — sandbox-isolation + cosmetic-axiom attestations.
//!
//! § ROLE
//!   Two distinct attestations are emitted alongside the report :
//!     1. [`SandboxAttestation`] — affirms NO content-leak, NO network,
//!        NO author-store-write occurred during the session. Constructed
//!        by the driver from a small set of measurable invariants (in-
//!        process · in-memory · no fs-write outside session-tmp).
//!     2. [`cosmetic_axiom_holds`] — true iff the trace contains zero
//!        `CosmeticAxiomViolation` events. The axiom : NO pay-for-power
//!        paths.

use serde::{Deserialize, Serialize};

use crate::session::Trace;

/// § Sandbox-attestation — measurable invariants the driver verified
/// during the session. All fields default to `false` ; the driver flips
/// them to `true` only when the corresponding invariant is observed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxAttestation {
    /// True iff the engine ran in-process (no IPC/network).
    pub in_process: bool,
    /// True iff no filesystem-write touched the author's content-store.
    pub no_author_store_write: bool,
    /// True iff no outbound network call was made.
    pub no_network: bool,
    /// True iff the LLM-bridge mode was Mode-C (substrate) ; if Mode-A/B
    /// was used, this is `false` and the host is expected to surface the
    /// fact in the report so authors know an external resource was hit.
    pub mode_c_only: bool,
}

impl SandboxAttestation {
    /// § Build the canonical "fully-sandboxed" attestation. Used by tests
    /// + by the default driver path when run in Mode-C.
    #[must_use]
    pub fn fully_sandboxed() -> Self {
        Self {
            in_process: true,
            no_author_store_write: true,
            no_network: true,
            mode_c_only: true,
        }
    }

    /// § True iff EVERY invariant holds. The host's sovereignty-dashboard
    /// uses this as the single bool to display next to the report.
    #[must_use]
    pub const fn is_fully_sandboxed(&self) -> bool {
        self.in_process && self.no_author_store_write && self.no_network && self.mode_c_only
    }

    /// § Build the attestation from runtime-observed booleans. The
    /// driver supplies all four ; the resulting struct is anchored
    /// alongside the report.
    #[must_use]
    pub const fn from_observed(
        in_process: bool,
        no_author_store_write: bool,
        no_network: bool,
        mode_c_only: bool,
    ) -> Self {
        Self {
            in_process,
            no_author_store_write,
            no_network,
            mode_c_only,
        }
    }
}

/// § Convenience constructor — equivalent to
/// `SandboxAttestation::fully_sandboxed()` but exposed as a function so
/// callers can match the trait-style of other attestation crates.
#[must_use]
pub fn sandbox_attestation() -> SandboxAttestation {
    SandboxAttestation::fully_sandboxed()
}

/// § True iff the trace records ZERO cosmetic-axiom violations.
///
/// The cosmetic-axiom : monetization channels MUST be cosmetic-only ;
/// no pay-for-power paths exist anywhere in the content. A single
/// violation event = axiom-broken = report-rejected (see [`crate::scoring`]).
#[must_use]
pub fn cosmetic_axiom_holds(trace: &Trace) -> bool {
    trace.cosmetic_axiom_violation_count() == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::TraceEvent;

    #[test]
    fn fully_sandboxed_short_circuits() {
        let s = SandboxAttestation::fully_sandboxed();
        assert!(s.is_fully_sandboxed());
    }

    #[test]
    fn missing_one_invariant_breaks_attestation() {
        let s = SandboxAttestation::from_observed(true, true, true, false);
        assert!(!s.is_fully_sandboxed());
    }

    #[test]
    fn cosmetic_axiom_holds_on_clean_trace() {
        assert!(cosmetic_axiom_holds(&Trace::new()));
    }

    #[test]
    fn cosmetic_axiom_violated_when_pay_for_power() {
        let mut t = Trace::new();
        t.push(TraceEvent::CosmeticAxiomViolation {
            turn: 0,
            path: "shop:lootbox-power".into(),
        });
        assert!(!cosmetic_axiom_holds(&t));
    }
}
