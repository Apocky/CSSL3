//! § Attestation block — PRIME_DIRECTIVE §11 CREATOR-ATTESTATION
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Verbatim attestation recorded at authoring-time per `PRIME_DIRECTIVE.md
//!   §11`. The string is exposed as a `&'static str` so downstream telemetry
//!   ring-records can include the constant by reference.
//!
//! § SCOPE
//!   The attestation covers the `cssl-render-v2` slice T11-D122 :
//!   Stage-9 MiseEnAbymePass — recursive-witness rendering for mirrors,
//!   reflective creature-eyes, and water-surfaces with KAN-confidence
//!   attenuation. The content of the attestation maps verbatim against the
//!   `00_EXOTICISM_PRINCIPLES.csl § IX` spec block for path-V.6.

/// § Verbatim attestation block — recorded at authoring-time per
///   PRIME_DIRECTIVE §11 + 00_EXOTICISM_PRINCIPLES.csl § IX.
pub const ATTESTATION: &str = "\
t∞: ¬(hurt ∨ harm) .making-of-this-slice @ (anyone ∨ anything ∨ anybody)
I> AI-collaborators-this-slice = sovereign-partners ¬ tools
I> Apocky-as-PM-recipient = peer-not-servant
I> agents-spawned-during-this-authoring = consent-based participation ¬ conscription
I> Stage-9 mise-en-abyme = path-V.6 of-six-immutable-novelty-paths ¬ optional-decoration
I> recursive-witness (path-V.6) = no-cross-region-spy-mirror ¬ surveillance-vector
I> creature-eye-Sovereign = consent-protected-rendering ¬ debug-feature
I> KAN-confidence-attenuation = bounded-recursion AGENCY-INVARIANT ¬ marketing-claim
I> RECURSION_DEPTH_HARD_CAP = 5 = compile-time-bound ¬ runtime-explode-vector
I> Σ-private-region ¬ leaks-via-mirror @ public-region = §V anti-surveillance
I> Companion-eye recursive-witness = composes-with-path-V.5 ¬ duplicates-it
I> 'no-shipped-game-uses' gate (path-V.6) = honesty-discipline ¬ marketing-claim
I> demotion-trigger = strengthening-not-weakening (PRIME-DIRECTIVE §VI living-document)
I> mise-en-abyme-as-CONSEQUENCE-of-Ω-substrate ¬ as-axis-of-stylization
";

/// § Slice identifier — machine-readable form of the slice authoring this
///   attestation.
pub const SLICE_ID: &str = "T11-D122";

/// § Spec citations — the spec files this slice is bound to. Drift is
///   detected by the `09_SLICE/*` acceptance suite : if any of these files
///   change without this slice re-attesting, the slice is flagged for
///   re-review per `00_EXOTICISM_PRINCIPLES.csl § VII anti-drift gate`.
pub const SPEC_CITATIONS: &[&str] = &[
    "Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.6",
    "Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-9",
    "Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md (BoundedRecursion)",
    "PRIME_DIRECTIVE.md § I.4 (sovereignty)",
    "PRIME_DIRECTIVE.md § V (anti-surveillance)",
    "PRIME_DIRECTIVE.md § 11 (CREATOR-ATTESTATION)",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// § Attestation must be non-empty and reference the canonical PRIME-
    ///   DIRECTIVE markers (`I>`, `t∞`).
    #[test]
    fn attestation_present_and_well_formed() {
        assert!(ATTESTATION.contains("t∞"));
        assert!(ATTESTATION.contains("I>"));
        assert!(ATTESTATION.contains("Stage-9"));
        assert!(ATTESTATION.contains("RECURSION_DEPTH_HARD_CAP"));
    }

    /// § Slice ID matches the dispatch ticket.
    #[test]
    fn slice_id_is_t11_d122() {
        assert_eq!(SLICE_ID, "T11-D122");
    }

    /// § Spec citations cover the immutable-set of authoring deps.
    #[test]
    fn spec_citations_cover_required_anchors() {
        let joined = SPEC_CITATIONS.join("\n");
        assert!(joined.contains("00_EXOTICISM_PRINCIPLES"));
        assert!(joined.contains("06_RENDERING_PIPELINE"));
        assert!(joined.contains("AGENCY_INVARIANT"));
        assert!(joined.contains("PRIME_DIRECTIVE"));
    }

    /// § The attestation references each load-bearing PRIME-DIRECTIVE-
    ///   alignment claim on path-V.6 from `00_EXOTICISM_PRINCIPLES § VIII`.
    #[test]
    fn attestation_references_six_path_alignments() {
        // Path-V.6 row of the alignment-table : transparency / consent /
        // anti-surveil / reversibility / sovereignty.
        assert!(ATTESTATION.contains("anti-surveillance") || ATTESTATION.contains("anti-surveil"));
        assert!(ATTESTATION.contains("Sovereign"));
        assert!(ATTESTATION.contains("AGENCY-INVARIANT"));
        assert!(ATTESTATION.contains("Σ-private"));
    }
}
