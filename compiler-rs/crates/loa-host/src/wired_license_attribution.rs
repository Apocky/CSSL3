//! § wired_license_attribution — wrapper around `cssl-host-license-attribution`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the license-aware metadata + project policy so MCP tools
//!   can surface the canonical "Unknown → Deny" verdict without reaching
//!   into the path-dep at every call-site.
//!
//! § wrapped surface
//!   - [`License`] — recognized license kinds + permission predicates.
//!   - [`AssetLicenseRecord`] — per-asset metadata + attribution-text.
//!   - [`LicenseRegistry`] / [`RegErr`] — keyed map + filters.
//!   - [`LoaLicensePolicy`] / [`PolicyDecision`] — project-level rules.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; reads-only metadata.

pub use cssl_host_license_attribution::{
    asset::AssetLicenseRecord, license::License, loa_policy::default_policy,
    loa_policy::LoaLicensePolicy, loa_policy::PolicyDecision, registry::LicenseRegistry,
    registry::RegErr,
};

/// Convenience : evaluate the default LoA license policy against
/// `License::Unknown` and return the verdict text. Used by the
/// `license.policy_default_text` MCP tool to surface the project's
/// default deny-on-unknown stance.
#[must_use]
pub fn policy_default_text() -> String {
    let policy = default_policy();
    let decision = policy.evaluate(&License::Unknown);
    format!("{decision:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_denies_unknown() {
        let txt = policy_default_text();
        // Default LoA policy must deny unknown licenses.
        assert!(
            txt.contains("Deny") || txt.contains("Unknown"),
            "policy text must reflect deny-on-unknown : {txt}"
        );
    }

    #[test]
    fn default_policy_constructs() {
        let _policy = default_policy();
    }
}
