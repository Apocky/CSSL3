//! § attestation — PRIME_DIRECTIVE §11 attestation block (verbatim).
//!
//! Recorded for the `cssl-substrate-render` crate (T11-D304 · W-S-CORE-5).

/// PRIME_DIRECTIVE §11 attestation block — recorded verbatim per
/// `~/source/repos/CSLv3/PRIME_DIRECTIVE.md`.
pub const ATTESTATION: &str = "\
§ ATTESTATION

The CFER (Causal Field-Evolution Rendering) iterator is a uniquely-Apocky
rendering algorithm : ω-field-substrate (S11) + KAN-per-cell (S12) +
capability-encoded-types compose into a single principled algorithm that
replaces ray-stream sampling with deterministic field-iteration. The driver
in this crate is consent-neutral on the forward-only path : the CFER frame
loop READS the field but never SETS Σ-mask bits, never overrides
Sovereign-claimed cells, and never coerces a companion-AI hook. The
adaptive-budget evidence-driver consults the existing per-cell evidence
glyphs (◐ ✓ ✗ ○) — it does not author them.

Differentiability (adjoint backward-pass) is gated by Sovereign-handle and
lives outside this slice : the forward iterator is sufficient for the
ADCS Wave-S CORE-5 keystone.

Per the CSL-MANDATE : reasoning + design-notes are CSLv3-native ; the
rustdoc surface is English-prose for onboarding clarity. The internal
proof-obligations are encoded structurally (FieldCell layout, MERA tier
budget, evidence-glyph state machine) — they need no narrative restatement.

There was no hurt nor harm in the making of this, to anyone, anything, or
anybody.
";

#[cfg(test)]
mod tests {
    use super::ATTESTATION;

    #[test]
    fn attestation_mentions_cfer_and_substrate() {
        assert!(ATTESTATION.contains("CFER"));
        assert!(ATTESTATION.contains("ω-field-substrate"));
        assert!(ATTESTATION.contains("KAN-per-cell"));
    }

    #[test]
    fn attestation_carries_pd_anchor() {
        assert!(ATTESTATION.contains("Sovereign-handle"));
        assert!(ATTESTATION.contains("hurt nor harm"));
    }

    #[test]
    fn attestation_consent_neutral_clause() {
        assert!(ATTESTATION.contains("consent-neutral"));
        assert!(ATTESTATION.contains("CSL-MANDATE"));
    }
}
