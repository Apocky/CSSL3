//! § Attestation — recorded verbatim per `PRIME_DIRECTIVE §11`.

/// Attestation block recording authorship + integration intent.
pub const ATTESTATION: &str = r#"
§ ATTESTATION (T11-D303 / W-S-CORE-4)

  W-S-CORE-4 : adjoint-method kernel · ADCS full-differentiability infrastructure.

  Authored under the Apocky CSL3 mandate :
    - density = sovereignty (CSL3 reasoning + design)
    - English-prose only at user-facing boundaries (rustdoc + error-msgs)
    - PRIME-DIRECTIVE structurally encoded :
        * Σ-mask gates parameter-mutation (Sovereign-claimed cells refuse
          gradient-updates absent explicit consent ; surfaced as "frozen"
          parameters rather than silently overridden)
        * Audit-chain : every optimizer-step bumps a step-counter that
          downstream telemetry can correlate with FieldCell.epoch
        * Determinism : checkpoint-recompute is bit-identical to original
          forward pass given same RNG-seed (none used here ; pure linear-
          algebra + FMA-ordering preserved)

  Specs cited (CSL-MANDATE) :
    - specs/30_SUBSTRATE_v3.csl § FULL-DIFFERENTIABILITY
    - specs/36_CFER_RENDERER.csl § DIFFERENTIABILITY
        - § Forward pass
        - § Backward pass (adjoint)
        - § Checkpointing
        - § Use-cases
    - specs/30_SUBSTRATE_v2.csl § DEFERRED D-1 (KAN foundation)

  Race-discipline : NEW crate · single commit · NO --amend ; workspace
  Cargo.toml glob ("members = [\"crates/*\"]") auto-discovers cssl-substrate-
  adjoint without explicit member-list edit.

  Co-Authored-By : Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("T11-D303"));
        assert!(ATTESTATION.contains("W-S-CORE-4"));
        assert!(ATTESTATION.contains("FULL-DIFFERENTIABILITY"));
        assert!(ATTESTATION.contains("DIFFERENTIABILITY"));
    }

    #[test]
    fn attestation_cites_mandatory_specs() {
        assert!(ATTESTATION.contains("30_SUBSTRATE_v3"));
        assert!(ATTESTATION.contains("36_CFER_RENDERER"));
    }
}
