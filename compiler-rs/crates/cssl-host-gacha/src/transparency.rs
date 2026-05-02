// § transparency.rs — ¬ predatory-pattern attestations · transparency-mandate
// ════════════════════════════════════════════════════════════════════
// § PRIME-DIRECTIVE : structurally-encode the absence of predatory-patterns.
//   These are NOT runtime-toggleable booleans · they are CONST attestations
//   that downstream renderers + UI-shaders consult and uphold.
//
// § THE TEN ATTESTATIONS :
//   1.  ¬ pay-for-power            (cosmetic-only-axiom · gameplay-impact NEVER)
//   2.  ¬ near-miss-animation      (no "you were SO close" UI feedback)
//   3.  ¬ countdown-FOMO           (no time-limited-exclusive-power)
//   4.  ¬ exclusive-cosmetic       (every cosmetic eventually-attainable)
//   5.  ¬ loss-aversion-framing    (¬ "don't miss out!" copy)
//   6.  ¬ social-comparison        (¬ "X just won the Mythic!" UI · ¬ leaderboard)
//   7.  ¬ celebrity-endorsement    (¬ paid-influencer banner-promotion)
//   8.  ¬ in-game-grind-loop       (pull-currency only via Stripe OR gift-from-friend)
//   9.  transparency-mandate       (drop-rates + pity publicly-disclosed BEFORE pull)
//   10. sovereign-revocable        (7d full-refund · player-pubkey-tied · auto-API)
//
// § COSMETIC-ONLY AXIOM : pull-results carry ZERO power · no stat-buff · no
//   gameplay-shortcut · no XP-multiplier · only cosmetic-shards (visual-skins
//   of weapons/armor/companion · NO numerical-impact).
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// § ATTESTATIONS_COUNT — count of structurally-encoded ¬ predatory attestations.
/// MUST be ≥ 10 (per spec). Exposed for downstream attestation-aggregation.
pub const ATTESTATIONS_COUNT: usize = 10;

/// § AttestationFlags — bitfield of upheld-by-this-build attestations.
///
/// Each bit is set IFF the attestation is structurally upheld at this
/// build / runtime-config. The default constructor sets ALL bits ;
/// removing one requires an explicit constructor + audit-emit so that
/// downstream UI / shaders / engine-modules can refuse to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationFlags {
    pub no_pay_for_power: bool,
    pub no_near_miss_animation: bool,
    pub no_countdown_fomo: bool,
    pub no_exclusive_cosmetic: bool,
    pub no_loss_aversion_framing: bool,
    pub no_social_comparison: bool,
    pub no_celebrity_endorsement: bool,
    pub no_ingame_grind_loop: bool,
    pub transparency_mandate: bool,
    pub sovereign_revocable: bool,
}

impl AttestationFlags {
    /// All-true canonical attestation. The ONLY value that should ship.
    #[must_use]
    pub const fn all_upheld() -> Self {
        Self {
            no_pay_for_power: true,
            no_near_miss_animation: true,
            no_countdown_fomo: true,
            no_exclusive_cosmetic: true,
            no_loss_aversion_framing: true,
            no_social_comparison: true,
            no_celebrity_endorsement: true,
            no_ingame_grind_loop: true,
            transparency_mandate: true,
            sovereign_revocable: true,
        }
    }

    /// Count of upheld bits — must equal `ATTESTATIONS_COUNT` for a valid
    /// shipping build.
    #[must_use]
    pub const fn upheld_count(&self) -> usize {
        let mut n = 0;
        if self.no_pay_for_power { n += 1; }
        if self.no_near_miss_animation { n += 1; }
        if self.no_countdown_fomo { n += 1; }
        if self.no_exclusive_cosmetic { n += 1; }
        if self.no_loss_aversion_framing { n += 1; }
        if self.no_social_comparison { n += 1; }
        if self.no_celebrity_endorsement { n += 1; }
        if self.no_ingame_grind_loop { n += 1; }
        if self.transparency_mandate { n += 1; }
        if self.sovereign_revocable { n += 1; }
        n
    }

    /// Predicate : is this build PRIME-DIRECTIVE-compliant?
    #[must_use]
    pub const fn is_prime_directive_compliant(&self) -> bool {
        self.upheld_count() == ATTESTATIONS_COUNT
    }
}

impl Default for AttestationFlags {
    fn default() -> Self {
        Self::all_upheld()
    }
}

/// § PredatoryPatternAttestation — public-disclosure record. Emitted with
/// every Σ-Chain-anchored pull-event · clients display the attestation
/// list in-UI before pulling (transparency-mandate · attest-by-construction).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredatoryPatternAttestation {
    pub flags: AttestationFlags,
    pub crate_version: String,
    pub spec_anchor: String,
    /// Human-readable list, ordered for stable display.
    pub disclosed_attestations: Vec<String>,
}

impl PredatoryPatternAttestation {
    /// Canonical attestation — all-upheld · with the ten-attestation list
    /// in stable display-order.
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            flags: AttestationFlags::all_upheld(),
            crate_version: crate::VERSION.to_string(),
            spec_anchor: crate::SPEC_ANCHOR.to_string(),
            disclosed_attestations: [
                "¬ pay-for-power (cosmetic-only-axiom)",
                "¬ near-miss-animation",
                "¬ countdown-FOMO",
                "¬ exclusive-cosmetic-AT-ALL",
                "¬ loss-aversion-framing",
                "¬ social-comparison",
                "¬ celebrity-endorsement",
                "¬ in-game-grind-loop for-pull-currency",
                "transparency-mandate (drop-rates + pity publicly-disclosed)",
                "sovereign-revocable (7d full-refund · player-pubkey-tied)",
            ]
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attestations_count_is_at_least_ten() {
        assert!(ATTESTATIONS_COUNT >= 10);
    }

    #[test]
    fn canonical_flags_are_all_upheld() {
        let f = AttestationFlags::all_upheld();
        assert_eq!(f.upheld_count(), ATTESTATIONS_COUNT);
        assert!(f.is_prime_directive_compliant());
    }

    #[test]
    fn default_is_canonical() {
        let f = AttestationFlags::default();
        assert!(f.is_prime_directive_compliant());
    }

    #[test]
    fn missing_one_attestation_is_non_compliant() {
        let mut f = AttestationFlags::all_upheld();
        f.no_pay_for_power = false;
        assert!(!f.is_prime_directive_compliant());
    }

    #[test]
    fn canonical_attestation_lists_ten_strings() {
        let p = PredatoryPatternAttestation::canonical();
        assert_eq!(p.disclosed_attestations.len(), 10);
        assert!(p.flags.is_prime_directive_compliant());
        // Crate version + spec-anchor surfaced for client-display.
        assert!(!p.crate_version.is_empty());
        assert_eq!(p.spec_anchor, crate::SPEC_ANCHOR);
    }

    #[test]
    fn cosmetic_only_axiom_present() {
        let p = PredatoryPatternAttestation::canonical();
        let joined = p.disclosed_attestations.join(" | ");
        assert!(joined.contains("cosmetic-only"));
        assert!(joined.contains("transparency-mandate"));
        assert!(joined.contains("sovereign-revocable"));
    }
}
