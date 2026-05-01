// § license.rs — License-enum + permission-predicates + canonical-URLs
// I> serde tag="kind" content="text" → forward-compat for AssetSpecific(text)
// I> LoA-compatibility = (commercial ∧ modification ∧ redistribution)

use serde::{Deserialize, Serialize};

/// License kinds recognized by the LoA asset-ingest pipeline.
///
/// Tagged with `kind` discriminator + optional `text` payload (used by `AssetSpecific`).
/// This shape means future variants can be added without breaking serialized data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text")]
pub enum License {
    /// Public-domain dedication. No attribution required.
    #[serde(rename = "cc0")]
    CC0,
    /// Creative Commons Attribution 4.0 — requires attribution.
    #[serde(rename = "cc_by_4_0")]
    CCBY40,
    /// Creative Commons Attribution-ShareAlike 4.0 — attribution + share-alike.
    #[serde(rename = "cc_by_sa_4_0")]
    CCBYSA40,
    /// Creative Commons Attribution-NonCommercial 4.0 — non-commercial only (NOT LoA-compatible).
    #[serde(rename = "cc_by_nc_4_0")]
    CCBYNC40,
    /// Creative Commons Attribution-NoDerivatives 4.0 — no modifications (NOT LoA-compatible).
    #[serde(rename = "cc_by_nd_4_0")]
    CCBYND40,
    /// MIT-style permissive license.
    #[serde(rename = "mit_like")]
    MITLike,
    /// Apache-2.0-style permissive license.
    #[serde(rename = "apache_like")]
    ApacheLike,
    /// GPL-style copyleft license.
    #[serde(rename = "gpl_like")]
    GPLLike,
    /// Proprietary / unlicensed asset (NOT LoA-compatible).
    #[serde(rename = "proprietary_unlicensed")]
    ProprietaryUnlicensed,
    /// Asset-specific custom license — payload carries license name/text.
    #[serde(rename = "asset_specific")]
    AssetSpecific(String),
    /// License unknown / not-yet-classified.
    #[serde(rename = "unknown")]
    Unknown,
}

impl License {
    /// True iff the license requires on-screen / in-credits attribution.
    pub fn requires_attribution(&self) -> bool {
        matches!(
            self,
            License::CCBY40
                | License::CCBYSA40
                | License::CCBYNC40
                | License::CCBYND40
                | License::MITLike
                | License::ApacheLike
                | License::GPLLike
        )
    }

    /// True iff commercial use is permitted.
    pub fn permits_commercial(&self) -> bool {
        match self {
            License::CC0
            | License::CCBY40
            | License::CCBYSA40
            | License::CCBYND40
            | License::MITLike
            | License::ApacheLike
            | License::GPLLike => true,
            License::CCBYNC40
            | License::ProprietaryUnlicensed
            | License::AssetSpecific(_)
            | License::Unknown => false,
        }
    }

    /// True iff the license permits modification (derivative works).
    pub fn permits_modification(&self) -> bool {
        match self {
            License::CC0
            | License::CCBY40
            | License::CCBYSA40
            | License::CCBYNC40
            | License::MITLike
            | License::ApacheLike
            | License::GPLLike => true,
            License::CCBYND40
            | License::ProprietaryUnlicensed
            | License::AssetSpecific(_)
            | License::Unknown => false,
        }
    }

    /// True iff redistribution (re-shipping the asset bytes) is permitted.
    pub fn permits_redistribution(&self) -> bool {
        match self {
            License::CC0
            | License::CCBY40
            | License::CCBYSA40
            | License::CCBYNC40
            | License::CCBYND40
            | License::MITLike
            | License::ApacheLike
            | License::GPLLike => true,
            License::ProprietaryUnlicensed | License::AssetSpecific(_) | License::Unknown => false,
        }
    }

    /// True iff the license is compatible with LoA's free + permissive stance:
    /// requires (commercial ∧ modification ∧ redistribution).
    pub fn is_loa_compatible(&self) -> bool {
        self.permits_commercial() && self.permits_modification() && self.permits_redistribution()
    }

    /// Short label suitable for HUD display.
    pub fn short_label(&self) -> &'static str {
        match self {
            License::CC0 => "CC0",
            License::CCBY40 => "CC-BY-4.0",
            License::CCBYSA40 => "CC-BY-SA-4.0",
            License::CCBYNC40 => "CC-BY-NC-4.0",
            License::CCBYND40 => "CC-BY-ND-4.0",
            License::MITLike => "MIT",
            License::ApacheLike => "Apache-2.0",
            License::GPLLike => "GPL",
            License::ProprietaryUnlicensed => "Proprietary",
            License::AssetSpecific(_) => "Asset-Specific",
            License::Unknown => "Unknown",
        }
    }

    /// Canonical URL for the license — used by attribution-HUD when generating links.
    pub fn full_url(&self) -> Option<&'static str> {
        match self {
            License::CC0 => Some("https://creativecommons.org/publicdomain/zero/1.0/"),
            License::CCBY40 => Some("https://creativecommons.org/licenses/by/4.0/"),
            License::CCBYSA40 => Some("https://creativecommons.org/licenses/by-sa/4.0/"),
            License::CCBYNC40 => Some("https://creativecommons.org/licenses/by-nc/4.0/"),
            License::CCBYND40 => Some("https://creativecommons.org/licenses/by-nd/4.0/"),
            License::MITLike => Some("https://opensource.org/license/mit/"),
            License::ApacheLike => Some("https://www.apache.org/licenses/LICENSE-2.0"),
            License::GPLLike => Some("https://www.gnu.org/licenses/gpl-3.0.html"),
            License::ProprietaryUnlicensed | License::AssetSpecific(_) | License::Unknown => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cc0_no_attribution_required() {
        assert!(!License::CC0.requires_attribution());
        assert!(License::CC0.permits_commercial());
        assert!(License::CC0.permits_modification());
        assert!(License::CC0.permits_redistribution());
        assert!(License::CC0.is_loa_compatible());
    }

    #[test]
    fn cc_by_requires_attribution() {
        assert!(License::CCBY40.requires_attribution());
        assert!(License::CCBY40.is_loa_compatible());
        assert!(License::CCBYSA40.requires_attribution());
        assert!(License::CCBYSA40.is_loa_compatible());
    }

    #[test]
    fn cc_nc_not_loa_compatible() {
        assert!(!License::CCBYNC40.permits_commercial());
        assert!(!License::CCBYNC40.is_loa_compatible());
    }

    #[test]
    fn proprietary_not_compatible() {
        assert!(!License::ProprietaryUnlicensed.is_loa_compatible());
        assert!(!License::ProprietaryUnlicensed.permits_redistribution());
        assert!(!License::Unknown.is_loa_compatible());
        assert!(!License::AssetSpecific("custom-eula".into()).is_loa_compatible());
        assert!(!License::CCBYND40.is_loa_compatible());
        assert!(!License::CCBYND40.permits_modification());
    }

    #[test]
    fn short_label_matches_spec() {
        assert_eq!(License::CC0.short_label(), "CC0");
        assert_eq!(License::CCBY40.short_label(), "CC-BY-4.0");
        assert_eq!(License::CCBYSA40.short_label(), "CC-BY-SA-4.0");
        assert_eq!(License::CCBYNC40.short_label(), "CC-BY-NC-4.0");
        assert_eq!(License::CCBYND40.short_label(), "CC-BY-ND-4.0");
        assert_eq!(License::MITLike.short_label(), "MIT");
        assert_eq!(License::ApacheLike.short_label(), "Apache-2.0");
        assert_eq!(License::GPLLike.short_label(), "GPL");
        assert_eq!(License::ProprietaryUnlicensed.short_label(), "Proprietary");
        assert_eq!(License::Unknown.short_label(), "Unknown");
        assert_eq!(
            License::AssetSpecific("zlib".into()).short_label(),
            "Asset-Specific"
        );
    }

    #[test]
    fn full_url_not_empty() {
        // Standard licenses have URLs; Unknown / Proprietary / AssetSpecific do not.
        for lic in [
            License::CC0,
            License::CCBY40,
            License::CCBYSA40,
            License::CCBYNC40,
            License::CCBYND40,
            License::MITLike,
            License::ApacheLike,
            License::GPLLike,
        ] {
            let url = lic.full_url().expect("standard license must have URL");
            assert!(url.starts_with("https://"));
            assert!(!url.is_empty());
        }
        assert!(License::Unknown.full_url().is_none());
        assert!(License::ProprietaryUnlicensed.full_url().is_none());
        assert!(License::AssetSpecific("x".into()).full_url().is_none());
    }

    #[test]
    fn serde_roundtrip() {
        for lic in [
            License::CC0,
            License::CCBY40,
            License::CCBYSA40,
            License::CCBYNC40,
            License::CCBYND40,
            License::MITLike,
            License::ApacheLike,
            License::GPLLike,
            License::ProprietaryUnlicensed,
            License::AssetSpecific("custom-1.0".into()),
            License::Unknown,
        ] {
            let json = serde_json::to_string(&lic).expect("serialize");
            let back: License = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(lic, back, "roundtrip mismatch json={json}");
        }
    }

    #[test]
    fn variant_coverage_all() {
        // Sanity: ensure every variant has well-defined label + LoA-compat answer.
        let variants = [
            License::CC0,
            License::CCBY40,
            License::CCBYSA40,
            License::CCBYNC40,
            License::CCBYND40,
            License::MITLike,
            License::ApacheLike,
            License::GPLLike,
            License::ProprietaryUnlicensed,
            License::AssetSpecific("x".into()),
            License::Unknown,
        ];
        let loa_compat: Vec<bool> = variants.iter().map(License::is_loa_compatible).collect();
        // CC0 + CC-BY + CC-BY-SA + MIT + Apache + GPL → 6 compatible
        let compat_count = loa_compat.iter().filter(|b| **b).count();
        assert_eq!(compat_count, 6, "expected 6 LoA-compatible variants");
        // every variant has a non-empty label
        for v in &variants {
            assert!(!v.short_label().is_empty());
        }
    }
}
