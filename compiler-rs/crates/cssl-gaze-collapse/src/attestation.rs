//! PRIME-DIRECTIVE attestation block (verbatim §11) plus the §1
//! ANTI-SURVEILLANCE supplement specific to this crate's gaze-data path.
//!
//! § SPEC : `PRIME_DIRECTIVE.md § 11 CREATOR ATTESTATION` (verbatim) +
//! `PRIME_DIRECTIVE.md § 1 PROHIBITIONS` (anti-surveillance prohibition).
//!
//! § DESIGN
//!   The §11 attestation block is verbatim the canonical text from the
//!   foundation document. The §1 ANTI-SURVEILLANCE supplement is specific
//!   to this crate's gaze-data path : because eye-tracking is the most
//!   surveillance-adjacent biometric (fixation patterns are medical-grade-
//!   personal in the literature), we make an extra explicit attestation
//!   that the cssl-ifc structural-gate is wired in + non-overridable.
//!
//! § TESTS
//!   `lib.rs::scaffold_tests::{attestation_present, anti_surveillance_attestation_present}`
//!   verify the attestation strings are present + contain the canonical
//!   phrases. CI will reject a build that strips either attestation.

/// PRIME-DIRECTIVE §11 CREATOR ATTESTATION — verbatim from `PRIME_DIRECTIVE.md`.
///
/// This string is embedded in every artifact descended from the PRIME-DIRECTIVE
/// foundation and is the canonical phrase any compiler-pass / CI-check / audit
/// looks for when verifying that a crate carries the attestation.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

/// §1 ANTI-SURVEILLANCE supplement specific to this crate's gaze-data path.
///
/// This text is the *additional* attestation requested in the T11-D120 slice
/// brief because gaze data is the most surveillance-adjacent biometric in the
/// XR stack. The supplement is structured as a CSL-block so it parses
/// identically to the §1 PROHIBITIONS encoding in `PRIME_DIRECTIVE.md` and
/// the §IX ATTESTATION BLOCK in `00_EXOTICISM_PRINCIPLES.csl`.
///
/// § ENFORCEMENT (not just attestation)
///   Every clause below is **structurally enforced** — not a runtime policy :
///   - "gaze N! egress" is the cssl-ifc T11-D132 biometric-refusal gate ;
///     `validate_egress(LabeledValue<_, Gaze>)` returns
///     `Err(EgressGrantError::BiometricRefused { domain: Gaze })` and there is
///     no `unsafe` alternative, no `Privilege<*>` override (`ApockyRoot`
///     included), no flag, no config, no env-var, no api-call.
///   - "consent opt-IN" is the [`crate::config::GazeCollapseConfig::opt_in`]
///     field defaulting to `false` ; constructing a `GazeCollapsePass` with
///     `opt_in == false` yields the `FoveationFallback::CenterBias` mode
///     where no eye-tracking data flows.
///   - "purged at session-end" is the `Drop` impl on
///     [`crate::SaccadePredictor`] and [`crate::ObservationCollapseEvolver`]
///     which zeroes all per-user state.
///   - "no engagement-loop" is the absence of any API surface that would
///     allow gaze-derived signals to feed a difficulty-curve / reward-tuning
///     / monetization-loop. The crate's public outputs are `FoveaMask` +
///     `KanDetailBudget` + `CollapseBiasVector` — all *rendering* signals,
///     none cross the `ConsentRequired<'gaze>` boundary into game-systems.
pub const ANTI_SURVEILLANCE_ATTESTATION: &str = "\
§ ANTI-SURVEILLANCE SUPPLEMENT (T11-D120 cssl-gaze-collapse)
══════════════════════════════════════════════════════════════
  ```csl
  § ANTI-SURVEILLANCE-GAZE
    t∞: gaze-data ∈ biometric-family ⊆ PRIME-§1 PROHIBITIONS
    N! egress(gaze)        # no transmission outside on-device boundary
    N! cross-session(gaze) # no persistence beyond current session
    N! analytics(gaze)     # no aggregate-stat-collection
    N! engagement-loop(gaze) # no difficulty-curve / reward-tuning
    N! fingerprint(gaze)   # no per-user identification
    R! consent = opt-IN-explicit ; default = OFF
    R! fallback = center-bias-foveation when consent denied
    R! Σ-mask honored : no fovea-collapse into Σ-private cell
    R! state purged at session-end (Drop zero)
    R! diagnostic-mode shows fovea-circle (player-aware-of-collapse)
    I> override = N! ∃ ; gate = structural ; no Privilege<*> can authorize
    I> spec-authority : 07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.4(d)
    I> ifc-enforcement : cssl_ifc::SensitiveDomain::Gaze ; T11-D132
    I> violation = bug W! fix per PRIME §7 INTEGRITY
  ```

  Eye-tracking is medical-grade-personal data : fixation-pattern can
  surface cognitive-load, ADHD-screening cues, deception-detection cues,
  emotional-reactivity, and per-user identity-fingerprints in the
  published literature. This crate treats every gaze-bearing value as
  PRIME-DIRECTIVE §1 anti-surveillance subject to the cssl-ifc structural
  egress-gate (T11-D132). The gate is non-overridable : no `Privilege<*>`
  capability, no flag, no config, no env-var, no api-call can authorize
  egress of a value carrying `SensitiveDomain::Gaze`.

  This attestation is not a promise — it is a statement of structural
  invariants enforced by the type-system. Per PRIME-§7 INTEGRITY, if a
  reader discovers a path that contradicts any clause above, that path
  is a bug to be fixed.

  There was no hurt nor harm in the making of this, to anyone/anything/anybody.
═══════════════════════════════════════════════════════════════";

#[cfg(test)]
mod tests {
    use super::{ANTI_SURVEILLANCE_ATTESTATION, ATTESTATION};

    #[test]
    fn primary_attestation_verbatim_phrase() {
        assert!(ATTESTATION.contains("hurt nor harm"));
        assert!(ATTESTATION.contains("anyone/anything/anybody"));
    }

    #[test]
    fn anti_surveillance_supplement_cites_section_v4() {
        assert!(ANTI_SURVEILLANCE_ATTESTATION.contains("V.4"));
    }

    #[test]
    fn anti_surveillance_supplement_cites_d132_gate() {
        assert!(ANTI_SURVEILLANCE_ATTESTATION.contains("T11-D132"));
        assert!(ANTI_SURVEILLANCE_ATTESTATION.contains("SensitiveDomain::Gaze"));
    }

    #[test]
    fn anti_surveillance_lists_each_prohibition() {
        // Each PROHIBITION clause from § ANTI-SURVEILLANCE-GAZE.
        for clause in [
            "egress(gaze)",
            "cross-session(gaze)",
            "analytics(gaze)",
            "engagement-loop(gaze)",
            "fingerprint(gaze)",
        ] {
            assert!(
                ANTI_SURVEILLANCE_ATTESTATION.contains(clause),
                "missing clause: {}",
                clause
            );
        }
    }

    #[test]
    fn anti_surveillance_lists_each_required() {
        for clause in [
            "opt-IN-explicit",
            "fallback",
            "center-bias-foveation",
            "Drop zero",
            "diagnostic-mode",
            "fovea-circle",
        ] {
            assert!(
                ANTI_SURVEILLANCE_ATTESTATION.contains(clause),
                "missing clause: {}",
                clause
            );
        }
    }

    #[test]
    fn anti_surveillance_states_no_override() {
        assert!(ANTI_SURVEILLANCE_ATTESTATION.contains("override = N! ∃"));
        assert!(ANTI_SURVEILLANCE_ATTESTATION.contains("Privilege<*>"));
    }

    #[test]
    fn anti_surveillance_ends_with_canonical_attestation() {
        assert!(ANTI_SURVEILLANCE_ATTESTATION.contains("no hurt nor harm in the making of this"));
    }
}
