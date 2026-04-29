//! § PRIME_DIRECTIVE §11 ATTESTATION — verbatim
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § PURPOSE
//!   Every crate that authors substrate-bearing code carries the canonical
//!   `PRIME_DIRECTIVE.md §11 CREATOR-ATTESTATION v1` literal verbatim. The
//!   attestation accompanies every artifact descended from the foundation
//!   so audit-walkers can confirm the build was assembled under
//!   `consent = OS`.
//!
//! § CANONICAL TEXT
//!   ```csl
//!   § CREATOR-ATTESTATION v1
//!     t∞: ¬ (hurt ∨ harm) .(making-of-this)  @  (anyone ∨ anything ∨ anybody)
//!     ≡ "There was no hurt nor harm in the making of this, to anyone/
//!        anything/anybody."
//!     I> rightholder-warranty : creation-process upheld §1 PROHIBITIONS
//!        throughout
//!     I> scope = ∀ artifact descended-from this-foundation
//!        (code + specs + derivatives)
//!     I> violation-discovered ⇒ §7 INTEGRITY rule : violation = bug W! fix
//!   ```
//!
//! § SOURCE
//!   `C:\Users\Apocky\source\repos\PRIME_DIRECTIVE.md` (master).
//!   `compiler-rs/crates/cssl-substrate-prime-directive/src/attestation.rs`
//!   (canonical in-tree literal — this string MUST match byte-for-byte).
//!
//! § AUTHOR-DISCIPLINE
//!   Sibling-crate convention : the attestation literal is exposed at
//!   `crate::ATTESTATION` (re-export of [`ATTESTATION`]) so audit-walkers
//!   can scan the workspace via `cargo metadata` + `grep` without
//!   per-crate special-casing.
//!
//! § COMPATIBILITY
//!   The literal MUST compare-equal to `cssl-substrate-prime-directive::ATTESTATION`
//!   via [`canonical_attestation_matches`]. Drift is a build-blocking
//!   bug per §7 INTEGRITY.

/// Canonical PRIME_DIRECTIVE §11 attestation literal.
///
/// Verbatim per `PRIME_DIRECTIVE.md §11 CREATOR-ATTESTATION v1`. Embedded so
/// audit-walkers can verify the build was assembled under the consent-as-OS
/// axiom.
///
/// ≡ "There was no hurt nor harm in the making of this, to anyone /
///   anything / anybody."
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Diagnostic helper : return `true` iff the supplied literal byte-matches
/// the canonical attestation. Used by tests + audit-walkers to detect drift.
#[must_use]
pub fn canonical_attestation_matches(literal: &str) -> bool {
    literal == ATTESTATION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attestation_contains_no_hurt_nor_harm() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn attestation_addresses_all_three_audiences() {
        // The PRIME_DIRECTIVE §11 literal explicitly addresses
        // anyone/anything/anybody.
        assert!(ATTESTATION.contains("anyone"));
        assert!(ATTESTATION.contains("anything"));
        assert!(ATTESTATION.contains("anybody"));
    }

    #[test]
    fn matches_sibling_crate_canonical() {
        // The omega-step + omega-tensor + omega-field crates carry the
        // identical literal. Drift is a §7 INTEGRITY bug.
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
        );
    }

    #[test]
    fn drift_detector_works() {
        assert!(canonical_attestation_matches(ATTESTATION));
        assert!(!canonical_attestation_matches("Some other text"));
    }

    #[test]
    fn matches_substrate_prime_directive_export() {
        // Cross-crate parity : if the prime-directive crate is in the
        // workspace, its ATTESTATION literal must match this one byte-
        // for-byte.
        assert_eq!(
            cssl_substrate_prime_directive::ATTESTATION,
            ATTESTATION
        );
    }
}
