// § loa_policy.rs — LoaLicensePolicy : project-level acceptance rules
// I> default = CC0 ✓ · CC-BY-x ✓ (with-attribution) · CC-NC ✗ · CC-ND ✗ · proprietary ✗ · unknown ✗
// I> custom-policy can opt-in to risky tiers if Apocky-decision

use crate::license::License;
use serde::{Deserialize, Serialize};

/// Outcome of evaluating an asset's license against the policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// License is acceptable, no special handling required.
    Allow,
    /// License is acceptable but the engine MUST display attribution at runtime.
    AllowWithAttribution,
    /// License is rejected; payload carries the reason.
    Deny(String),
}

/// Project-level license policy for LoA.
///
/// Defaults are conservative: only commercial-allowed + modification-allowed +
/// redistribution-allowed licenses pass; attribution-required licenses are
/// allowed only when the policy opts in (`allow_attribution_required`).
/// Unknown / proprietary licenses are denied unless explicitly enabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaLicensePolicy {
    /// Permit licenses that require attribution (CC-BY family, MIT, Apache, GPL).
    pub allow_attribution_required: bool,
    /// Permit `License::Unknown` (NOT recommended).
    pub allow_unknown: bool,
    /// Permit `License::ProprietaryUnlicensed` and `License::AssetSpecific(_)` (NOT recommended).
    pub allow_proprietary: bool,
}

impl Default for LoaLicensePolicy {
    fn default() -> Self {
        default_policy()
    }
}

/// LoA's default policy:
///   allow_attribution_required = true   (we ship attribution-HUD)
///   allow_unknown              = false  (refuse to ship anything we cannot classify)
///   allow_proprietary          = false  (refuse to redistribute non-free assets)
pub fn default_policy() -> LoaLicensePolicy {
    LoaLicensePolicy {
        allow_attribution_required: true,
        allow_unknown: false,
        allow_proprietary: false,
    }
}

impl LoaLicensePolicy {
    /// Evaluate a license under this policy.
    pub fn evaluate(&self, license: &License) -> PolicyDecision {
        // 1. proprietary / asset-specific gate
        if matches!(
            license,
            License::ProprietaryUnlicensed | License::AssetSpecific(_)
        ) {
            return if self.allow_proprietary {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Deny(format!(
                    "proprietary/asset-specific license '{}' not permitted under policy",
                    license.short_label()
                ))
            };
        }

        // 2. unknown gate
        if matches!(license, License::Unknown) {
            return if self.allow_unknown {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Deny(
                    "unknown license — refusing to ship asset of unclassified provenance".into(),
                )
            };
        }

        // 3. LoA-compatibility gate (commercial ∧ modification ∧ redistribution)
        if !license.is_loa_compatible() {
            return PolicyDecision::Deny(format!(
                "license '{}' is not LoA-compatible (missing commercial/modification/redistribution)",
                license.short_label()
            ));
        }

        // 4. attribution-required gate
        if license.requires_attribution() {
            return if self.allow_attribution_required {
                PolicyDecision::AllowWithAttribution
            } else {
                PolicyDecision::Deny(format!(
                    "license '{}' requires attribution but policy disabled it",
                    license.short_label()
                ))
            };
        }

        // 5. plain-allow path
        PolicyDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_allows_cc0() {
        let p = default_policy();
        assert_eq!(p.evaluate(&License::CC0), PolicyDecision::Allow);
    }

    #[test]
    fn default_allows_cc_by_with_attribution() {
        let p = default_policy();
        assert_eq!(
            p.evaluate(&License::CCBY40),
            PolicyDecision::AllowWithAttribution
        );
        assert_eq!(
            p.evaluate(&License::MITLike),
            PolicyDecision::AllowWithAttribution
        );
    }

    #[test]
    fn default_denies_proprietary() {
        let p = default_policy();
        match p.evaluate(&License::ProprietaryUnlicensed) {
            PolicyDecision::Deny(r) => assert!(r.contains("proprietary")),
            other => panic!("expected Deny, got {other:?}"),
        }
        match p.evaluate(&License::AssetSpecific("custom".into())) {
            PolicyDecision::Deny(_) => {}
            other => panic!("expected Deny, got {other:?}"),
        }
        // CC-NC also denied (commercial blocked → not LoA-compatible)
        match p.evaluate(&License::CCBYNC40) {
            PolicyDecision::Deny(r) => assert!(r.contains("not LoA-compatible")),
            other => panic!("expected Deny for CC-NC, got {other:?}"),
        }
        // CC-ND also denied (modification blocked)
        match p.evaluate(&License::CCBYND40) {
            PolicyDecision::Deny(_) => {}
            other => panic!("expected Deny for CC-ND, got {other:?}"),
        }
    }

    #[test]
    fn default_denies_unknown() {
        let p = default_policy();
        match p.evaluate(&License::Unknown) {
            PolicyDecision::Deny(r) => assert!(r.contains("unknown")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn custom_policy_allows_unknown() {
        let p = LoaLicensePolicy {
            allow_attribution_required: true,
            allow_unknown: true,
            allow_proprietary: false,
        };
        assert_eq!(p.evaluate(&License::Unknown), PolicyDecision::Allow);
        // proprietary still denied
        match p.evaluate(&License::ProprietaryUnlicensed) {
            PolicyDecision::Deny(_) => {}
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn custom_policy_no_attribution_denies_cc_by() {
        let p = LoaLicensePolicy {
            allow_attribution_required: false,
            allow_unknown: false,
            allow_proprietary: false,
        };
        match p.evaluate(&License::CCBY40) {
            PolicyDecision::Deny(r) => assert!(r.contains("attribution")),
            other => panic!("expected Deny, got {other:?}"),
        }
        // CC0 still ok — does not require attribution.
        assert_eq!(p.evaluate(&License::CC0), PolicyDecision::Allow);
    }
}
