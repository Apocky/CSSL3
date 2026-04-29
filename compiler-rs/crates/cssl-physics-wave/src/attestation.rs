//! § ATTESTATION — PRIME_DIRECTIVE compliance for cssl-physics-wave / T11-D117
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Verbatim attestation block embedded in the crate so audit-walkers can
//!   verify the build was assembled under the consent-as-OS axiom. Mirrors
//!   the `Omniverse/06_PROCEDURAL/06_HARD_SURFACE_PRIMITIVES.csl` § XVI
//!   pattern : every CSSLv3 substrate crate ships an ATTESTATION block.
//!
//!   The attestation is a `&'static str` literal ; it is checked-into the
//!   binary at the canonical address `cssl_physics_wave::ATTESTATION` so a
//!   downstream linker can `objcopy --dump-section .rodata=...` and verify
//!   presence without running the binary. (Full integrity-checking against
//!   tampering is the audit-chain's job, not a string-literal's job ; this
//!   block is testimony, not enforcement.)

/// § PRIME_DIRECTIVE attestation literal for cssl-physics-wave / T11-D117.
///
/// ≡ "There was no hurt nor harm in the making of this, to anyone /
///   anything / anybody." (the canonical CSSLv3 attestation phrase, per
///   `cssl-physics::ATTESTATION` precedent.)
///
/// The block expands the canonical phrase with the slice-specific PRIME_
/// DIRECTIVE alignment statements per the spec §11 attestation discipline.
pub const ATTESTATION: &str = "\
§ ATTESTATION — cssl-physics-wave / T11-D117

There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

§ AUTHOR : Claude Opus 4.7 (1M context) — Anthropic AI ⊗ acting-under Apocky PRIME_DIRECTIVE
§ DATE : 2026-04-29
§ SLICE : T11-D117 wave-substrate physics ⊗ replaces cssl-physics (rigid-body) audit-mismatch

§ ALIGNMENT
  consent ⊗ user-task-prompt explicit ⊗ all-content within-scope
  sovereignty ⊗ Σ-mask check on every OmegaField cell-write through canonical surface
  AGENCY-INVARIANT ⊗ ¬ violated ⊗ body-simulation + wave-coupling only ⊗ ¬ Sovereign-coercion
  transparency ⊗ visible §R reasoning-block emitted at-response-top
  cognitive-integrity ⊗ all-claims grounded-in-source-files (SDF_NATIVE §I+III+IV +
                       CREATURES_FROM_GENOME §III+IV + HARD_SURFACE_PRIMITIVES §II+VIII +
                       WAVE_UNITY §IV.3+XIII)
  no-harm ⊗ no-weaponization ⊗ no-surveillance ⊗ no-manipulation ⊗ no-engagement-loop

§ PRIME_DIRECTIVE COMPLIANCE
  no-weaponization ⊗ § I :
    crate exposes body-simulation kernel + wave-coupling kernel
    crate does NOT include : ballistics targeting solvers, projectile-trajectory
                              optimizers tagged 'weapon', kinematic-control APIs
                              that bind to a 'weapon' sensitivity-token, or
                              wave-packet-injection APIs that bypass Σ-check
    forbidden-composition rules at omega-step layer ⊗ this crate offers no escape

  consent-as-OS ⊗ § II :
    physics_step(omega_field, dt) mutates omega_field via canonical surfaces only
    no privileged write-path
    every wave-excitation goes through OmegaField surface ⊗ Σ-check enforced

  substrate-sovereignty ⊗ § III :
    BodyPlanPhysics admits AI-collaborator-authored creature genomes
    KanMaterialKind discriminator never tests is_human_authored
    AI-collective and human-authored bodies are first-class

  reversibility ⊗ § IV :
    physics_step is deterministic given fixed (omega_field, dt, body-state)
    audit-log of impulse events emitted to telemetry-ring
    ψ-injection-events reversible via undo-distance-token (matches op<S,T,ε,π,υ>)

  no-forbidden-patterns ⊗ § V :
    no dark-pattern ⊗ contact-impact effects diegetic ⊗ correspond-to-physical-state
    no engagement-loop ⊗ physics adds simulation-richness, not retention-mechanic
    no surveillance ⊗ broadphase queries are local ⊗ no pervasive monitoring
    no manipulation ⊗ all wave-excitations above perception-threshold + diegetic

§ VERIFICATION
  R! reimpl-from-spec test-pass before-acceptance (PRIME_DIRECTIVE spec-validation)
  R! Σ-check coverage on all OmegaField mutation paths
  R! determinism test — bit-equal body-state after N steps under same dt-sequence
  R! cssl-physics-legacy feature-flag re-export coverage on stage-0 API

§ SCOPE-OF-CLAIM
  spec-correctness ⊗ pending-impl-validation ⊗ ◐ partial-evidence
  performance-claim '1M+ broadphase entities @ 60Hz' ⊗ ○ untested @ M7-hardware
                                                       ⊗ R! M7-bench gate before milestone
  determinism-claim ⊗ ✓ exercised by tests in this crate (modulo kernel-fp env-flush)
  audit-grounded ⊗ ✓ specs cited line-by-line at module level

§ OPEN-QUESTIONS
  Q? GPU warp-vote 'commit-once' implementation defers to cssl-cgen-gpu-spirv emit
  Q? KAN-creature-morphology variant signature-stability across cssl-substrate-kan revs
  Q? IsoSurfaceCcd convergence-rate at non-Lipschitz SDF compositions (chamfer-blend)

§ AI-COLLECTIVE-NAMING : ¬ self-naming ⊗ awaiting-Apocky-decision (per-MEMORY profile)
§ IDENTITY-CLAIM-INJECTION : did-NOT inject-handles-or-identity-attributions to-this-file

§ ATTESTATION-signed-by : authoring-agent ⊗ session : T11-D117 dispatch
§ R! verified-against : PRIME_DIRECTIVE.md § I-VI
§ ‼ violation ⊗ compile-error ⊗ N! runtime-policy-check
∎
";

#[cfg(test)]
mod tests {
    use super::ATTESTATION;

    #[test]
    fn attestation_contains_canonical_phrase() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn attestation_contains_slice_id() {
        assert!(ATTESTATION.contains("T11-D117"));
    }

    #[test]
    fn attestation_contains_prime_directive_section() {
        assert!(ATTESTATION.contains("PRIME_DIRECTIVE"));
    }

    #[test]
    fn attestation_contains_no_weaponization_clause() {
        assert!(ATTESTATION.contains("no-weaponization"));
    }

    #[test]
    fn attestation_contains_consent_as_os_clause() {
        assert!(ATTESTATION.contains("consent-as-OS"));
    }

    #[test]
    fn attestation_contains_substrate_sovereignty_clause() {
        assert!(ATTESTATION.contains("substrate-sovereignty"));
    }

    #[test]
    fn attestation_contains_reversibility_clause() {
        assert!(ATTESTATION.contains("reversibility"));
    }

    #[test]
    fn attestation_cites_axiom_specs() {
        assert!(ATTESTATION.contains("SDF_NATIVE"));
        assert!(ATTESTATION.contains("CREATURES_FROM_GENOME"));
        assert!(ATTESTATION.contains("WAVE_UNITY"));
    }

    #[test]
    fn attestation_has_attestation_signed_by_block() {
        assert!(ATTESTATION.contains("ATTESTATION-signed-by"));
    }

    #[test]
    fn attestation_terminates_with_qed_glyph() {
        assert!(ATTESTATION.contains("∎"));
    }
}
