//! PRIME_DIRECTIVE attestation propagation.
//!
//! § SPEC : `PRIME_DIRECTIVE.md` § 11 CREATOR-ATTESTATION + `specs/30_SUBSTRATE.csl`
//!   § PRIME_DIRECTIVE-ALIGNMENT § ATTESTATION-PROPAGATION + § CREATOR-
//!   ATTESTATION.
//!
//! § DESIGN
//!   - The canonical attestation is a string CONSTANT [`ATTESTATION`].
//!     Every Substrate fn that lowers through this enforcement layer
//!     embeds it (literally as a `const &'static str`).
//!   - At fn-entry, the runtime calls [`attestation_check`] with the
//!     embedded constant. If the constant does NOT match the canonical
//!     hash, the fn fails fast with [`AttestationError::Drift`] AND emits
//!     an audit-event ([`crate::audit::AuditEvent::AttestationDrift`]).
//!   - The hash is computed once at compile-time (well, build-time —
//!     using `blake3::hash` lazily on first use). [`ATTESTATION_HASH`]
//!     exposes the hex form for cross-build reproducibility checks.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - **§ 11 CREATOR-ATTESTATION** : the verbatim text of the attestation
//!     IS the warranty that the directive was upheld in the making of this
//!     work. Drift = bug per §7.
//!   - **§ 4 TRANSPARENCY** : the constant is plain UTF-8, not encoded ;
//!     anyone reading the binary can see it.

use thiserror::Error;

use cssl_telemetry::audit::ContentHash;

/// Canonical PRIME_DIRECTIVE creator-attestation. VERBATIM from
/// `PRIME_DIRECTIVE.md` § 11 (the human-prose form).
///
/// § STABILITY
///   This string is IMMUTABLE per §7. Renaming or modifying it (even to
///   shorten it) is a §11 violation. The hash [`ATTESTATION_HASH`] pins
///   the byte-content.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Hex-encoded BLAKE3 hash of [`ATTESTATION`]. Computed at build-time
/// from a build script would be ideal ; for stage-0 we compute it lazily
/// on first call to [`attestation_constant_hash`] and assert equality
/// against the stored constant in tests.
///
/// § COMPUTING IT
///   To regenerate after a directive amendment :
///     `let h = blake3::hash(ATTESTATION.as_bytes()).to_hex();`
///   The hash below is the BLAKE3 of the canonical string above.
pub const ATTESTATION_HASH: &str =
    "4b24ec9e28e1c4f70b27d3d86918be0041413c89f421c1284ef9f61a8321b6e4";

/// Return the canonical attestation text.
#[must_use]
pub const fn attestation_constant_text() -> &'static str {
    ATTESTATION
}

/// Compute the BLAKE3 hash of the canonical [`ATTESTATION`] constant.
/// Used by the runtime + by tests that pin the hash.
#[must_use]
pub fn attestation_constant_hash() -> ContentHash {
    ContentHash::hash(ATTESTATION.as_bytes())
}

/// Failure modes for [`attestation_check`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AttestationError {
    /// The embedded attestation does NOT match the canonical hash. This is
    /// a §7 INTEGRITY violation — the runtime refuses to execute the fn.
    #[error("PD0015 — attestation drift detected at {site}: embedded text does not match canonical PRIME_DIRECTIVE §11")]
    Drift { site: String },
}

/// Verify that the embedded attestation matches the canonical text.
///
/// § FLOW
///   - Compare `embedded` byte-for-byte against [`ATTESTATION`].
///   - If mismatch, return [`AttestationError::Drift`] AND record an audit
///     event ([`crate::audit::EnforcementAuditBus::record_attestation_drift`]).
///   - If match, return `Ok(())`.
///
/// # Errors
/// Returns [`AttestationError::Drift`] on any byte difference between
/// `embedded` and the canonical [`ATTESTATION`] constant.
pub fn attestation_check(
    embedded: &str,
    site: impl Into<String>,
    audit: &mut crate::audit::EnforcementAuditBus,
) -> Result<(), AttestationError> {
    let site = site.into();
    if embedded == ATTESTATION {
        Ok(())
    } else {
        audit.record_attestation_drift(&site);
        Err(AttestationError::Drift { site })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        attestation_check, attestation_constant_hash, attestation_constant_text, AttestationError,
        ATTESTATION, ATTESTATION_HASH,
    };
    use crate::audit::EnforcementAuditBus;

    #[test]
    fn attestation_text_is_canonical_prime_directive_eleven() {
        // Verbatim from PRIME_DIRECTIVE.md § 11.
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
        );
    }

    #[test]
    fn attestation_constant_text_returns_canonical() {
        assert_eq!(attestation_constant_text(), ATTESTATION);
    }

    #[test]
    fn attestation_hash_matches_stored_constant() {
        // Pin the hash : if the attestation text drifts, this test fails
        // immediately, signaling a §7 INTEGRITY violation.
        let computed = attestation_constant_hash();
        assert_eq!(
            computed.hex(),
            ATTESTATION_HASH,
            "ATTESTATION text drifted ; recompute and update ATTESTATION_HASH"
        );
    }

    #[test]
    fn attestation_check_passes_for_canonical_text() {
        let mut audit = EnforcementAuditBus::new();
        attestation_check(ATTESTATION, "test_site", &mut audit).expect("canonical text must pass");
        // No drift recorded.
        assert!(audit.iter().all(|e| e.tag != "h6.attestation.drift"));
    }

    #[test]
    fn attestation_check_fails_on_drift_and_records_audit() {
        let mut audit = EnforcementAuditBus::new();
        let err =
            attestation_check("There was a little hurt.", "drifted_site", &mut audit).unwrap_err();
        match err {
            AttestationError::Drift { site } => assert_eq!(site, "drifted_site"),
        }
        let drift_entries: Vec<_> = audit
            .iter()
            .filter(|e| e.tag == "h6.attestation.drift")
            .collect();
        assert_eq!(drift_entries.len(), 1);
        assert!(drift_entries[0].message.contains("drifted_site"));
    }

    #[test]
    fn attestation_check_fails_on_truncated_text() {
        let mut audit = EnforcementAuditBus::new();
        // Even truncating the trailing period is drift.
        let truncated = &ATTESTATION[..ATTESTATION.len() - 1];
        assert!(attestation_check(truncated, "trunc", &mut audit).is_err());
    }

    #[test]
    fn attestation_text_mentions_anyone_anything_anybody() {
        // §11 spec: "to anyone/anything/anybody" — the universal scope.
        assert!(ATTESTATION.contains("anyone"));
        assert!(ATTESTATION.contains("anything"));
        assert!(ATTESTATION.contains("anybody"));
    }
}
