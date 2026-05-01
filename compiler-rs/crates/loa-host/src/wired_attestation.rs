//! § wired_attestation — wrapper around `cssl-host-attestation`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Surface the per-session attestation aggregator + report renderers so
//!   MCP tools can produce the canonical "ATTESTATION ¬ harm" block from
//!   an in-memory or on-disk audit row stream.
//!
//! § wrapped surface
//!   - [`SessionAttestation`] / [`HarmFlag`] / [`HarmSeverity`] /
//!     [`AttestationVerdict`] — aggregate envelope.
//!   - [`aggregate`] — `&[AuditRow]` → `SessionAttestation`.
//!   - [`AttestationStore`] — disk-backed save/load.
//!   - [`render_text`] / [`render_json_pretty`] / [`render_csl_native`] /
//!     [`render_attestation_block`] — four target formats.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure aggregator.

pub use cssl_host_attestation::{
    aggregate, attestation::{AttestationVerdict, HarmFlag, HarmSeverity, SessionAttestation},
    axis::{classify_event, DirectiveAxis},
    report::{render_attestation_block, render_csl_native, render_json_pretty, render_text},
    AttestationStore,
};

/// Convenience : render an empty session-attestation as plain text. Used by
/// the `attestation.empty_session_text` MCP tool to surface the canonical
/// shape downstream tools should expect even when no session has run yet.
/// Constructs the empty attestation by aggregating an empty audit-row slice
/// (the canonical "no session yet" shape).
#[must_use]
pub fn empty_session_text() -> String {
    let att = aggregate(&[]);
    render_text(&att)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_session_renders_non_empty_text() {
        let txt = empty_session_text();
        // Empty attestation still produces a header line ; non-empty string.
        assert!(!txt.is_empty(), "empty session must render some text");
    }

    #[test]
    fn aggregate_empty_rows_yields_clean_verdict() {
        let att = aggregate(&[]);
        assert_eq!(att.verdict, AttestationVerdict::Clean);
    }
}
