//! § cap — the 5 author cap-classes that gate `.ccpkg` publishing.
//!
//! § PROGRESSIVE PRIVILEGE MODEL (Σ-mask audience-class)
//!   cap-X-creator         — any verified author · default-self-publish · full-name visible
//!   cap-X-curator         — community-promoted · (creator + ≥ N upvotes from cap-X-creator)
//!   cap-X-moderator       — appointed community-mod · can co-sign curator-promotions
//!   cap-X-substrate-team  — Apocky / core team · can re-sign as hotfix-bundle
//!   cap-X-anonymous       — k-anon-≥-5 cohort signature (privacy by construction)
//!
//! § DISTINCTION (vs. cssl-hotfix `CapRole`)
//!   That enum (cap-A..cap-E) is the **operator-release** key-role split.
//!   This enum (cap-X-*) is the **content audience-class** Σ-mask split.
//!   Bundles signed at cap-X-creator class CANNOT propagate to cap-X-substrate-team
//!   audience without an explicit substrate-team co-signature : cross-class is
//!   default-deny (see `verify::verify_bundle`).
//!
//! § K-ANONYMITY
//!   `K_ANON_MIN = 5` is the minimum cohort size for `cap-X-anonymous` publish.
//!   The publish-pipeline (W12-5) is responsible for proving the k-anon
//!   property ; this crate carries the constant + a guard predicate
//!   `verify_anon_cohort` for completeness.

use serde::{Deserialize, Serialize};

/// § The 5 author cap-classes. `repr(u8)` = stable wire byte.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum AuthorCapClass {
    /// (1) verified-author self-publish.
    Creator = 1,
    /// (2) community-promoted (creator + ≥ N upvotes).
    Curator = 2,
    /// (3) appointed community-moderator.
    Moderator = 3,
    /// (4) Apocky / core substrate-team.
    SubstrateTeam = 4,
    /// (5) k-anon-≥-5 cohort signature.
    Anonymous = 5,
}

/// All 5 cap-classes in stable order, for tests + audience-class checks.
pub const AUTHOR_CAP_CLASSES: [AuthorCapClass; 5] = [
    AuthorCapClass::Creator,
    AuthorCapClass::Curator,
    AuthorCapClass::Moderator,
    AuthorCapClass::SubstrateTeam,
    AuthorCapClass::Anonymous,
];

/// § Minimum k-anonymity cohort size for `cap-X-anonymous` publish.
/// Privacy-by-construction default ; W12-5 publish-pipeline enforces.
pub const K_ANON_MIN: usize = 5;

impl AuthorCapClass {
    /// Stable name for serde / discovery / UI.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Creator => "cap-X-creator",
            Self::Curator => "cap-X-curator",
            Self::Moderator => "cap-X-moderator",
            Self::SubstrateTeam => "cap-X-substrate-team",
            Self::Anonymous => "cap-X-anonymous",
        }
    }

    /// Parse from canonical name.
    #[must_use]
    pub fn parse_canonical(s: &str) -> Option<Self> {
        match s {
            "cap-X-creator" => Some(Self::Creator),
            "cap-X-curator" => Some(Self::Curator),
            "cap-X-moderator" => Some(Self::Moderator),
            "cap-X-substrate-team" => Some(Self::SubstrateTeam),
            "cap-X-anonymous" => Some(Self::Anonymous),
            _ => None,
        }
    }

    /// Audience-class propagation rule : a bundle signed at `self` may
    /// propagate to a viewer with audience `target` IFF the discriminants
    /// are equal OR the target is a STRICT-superclass (defined by enum
    /// ordering : Creator ≤ Curator ≤ Moderator ≤ SubstrateTeam, with
    /// Anonymous in its own quarantined band).
    ///
    /// This is the cross-class default-deny rule : a Creator bundle does
    /// NOT auto-propagate to SubstrateTeam audience without re-signature.
    #[must_use]
    pub fn can_propagate_to(self, target: Self) -> bool {
        // Anonymous is its own quarantined band : only Anonymous → Anonymous.
        if matches!(self, Self::Anonymous) || matches!(target, Self::Anonymous) {
            return self == target;
        }
        // For the non-anon classes we use the ordering as the propagation
        // lattice : a SubstrateTeam-signed bundle can flow DOWN to Creator
        // viewers (broader audience), but a Creator-signed bundle cannot
        // flow UP to SubstrateTeam audience without re-signature.
        (self as u8) >= (target as u8)
    }

    /// What `ContentKind`s is this cap-class authorised to sign ?
    /// All cap-classes can sign all content-kinds — the gate is propagation
    /// (cross-class deny), not authoring. Returns `true` for all kinds.
    #[must_use]
    pub const fn can_sign_kind(self, _kind: crate::kind::ContentKind) -> bool {
        // Authoring is universal ; auditing is via Σ-mask propagation.
        // (No cap-class is exempted from authorship to honor sovereign-creator axiom.)
        true
    }
}

/// § Check that a claimed `cap-X-anonymous` cohort meets `K_ANON_MIN`.
/// Returns `Ok(())` if the cohort is acceptable, otherwise the actual
/// cohort size for diagnostic.
pub fn verify_anon_cohort(cohort_size: usize) -> Result<(), usize> {
    if cohort_size >= K_ANON_MIN {
        Ok(())
    } else {
        Err(cohort_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_const_is_five_distinct() {
        assert_eq!(AUTHOR_CAP_CLASSES.len(), 5);
        for (i, c) in AUTHOR_CAP_CLASSES.iter().enumerate() {
            assert_eq!(*c as u8, (i as u8) + 1);
        }
    }

    #[test]
    fn name_roundtrip() {
        for c in AUTHOR_CAP_CLASSES {
            assert_eq!(AuthorCapClass::parse_canonical(c.as_str()), Some(c));
        }
    }

    #[test]
    fn k_anon_min_is_five() {
        assert_eq!(K_ANON_MIN, 5);
    }

    #[test]
    fn k_anon_cohort_gate_works() {
        assert!(verify_anon_cohort(5).is_ok());
        assert!(verify_anon_cohort(100).is_ok());
        assert!(verify_anon_cohort(4).is_err());
        assert!(verify_anon_cohort(0).is_err());
    }

    #[test]
    fn substrate_team_propagates_down_to_creator() {
        // Substrate-team-signed content can be viewed by lower audiences.
        assert!(AuthorCapClass::SubstrateTeam.can_propagate_to(AuthorCapClass::Creator));
        assert!(AuthorCapClass::SubstrateTeam.can_propagate_to(AuthorCapClass::SubstrateTeam));
        assert!(AuthorCapClass::Moderator.can_propagate_to(AuthorCapClass::Creator));
    }

    #[test]
    fn creator_does_not_propagate_up_to_substrate_team() {
        // Creator-signed content does NOT auto-flow into substrate-team audience.
        assert!(!AuthorCapClass::Creator.can_propagate_to(AuthorCapClass::SubstrateTeam));
        assert!(!AuthorCapClass::Creator.can_propagate_to(AuthorCapClass::Moderator));
        assert!(!AuthorCapClass::Curator.can_propagate_to(AuthorCapClass::SubstrateTeam));
    }

    #[test]
    fn anonymous_is_quarantined() {
        // Anonymous → only Anonymous, no-one-else.
        assert!(AuthorCapClass::Anonymous.can_propagate_to(AuthorCapClass::Anonymous));
        assert!(!AuthorCapClass::Anonymous.can_propagate_to(AuthorCapClass::Creator));
        assert!(!AuthorCapClass::Anonymous.can_propagate_to(AuthorCapClass::SubstrateTeam));
        // And nothing-else flows into the Anonymous bucket.
        assert!(!AuthorCapClass::Creator.can_propagate_to(AuthorCapClass::Anonymous));
        assert!(!AuthorCapClass::SubstrateTeam.can_propagate_to(AuthorCapClass::Anonymous));
    }

    #[test]
    fn all_caps_can_sign_all_kinds() {
        use crate::kind::CONTENT_KINDS;
        for cap in AUTHOR_CAP_CLASSES {
            for kind in CONTENT_KINDS {
                assert!(cap.can_sign_kind(kind));
            }
        }
    }

    #[test]
    fn cap_class_serde_roundtrip() {
        for c in AUTHOR_CAP_CLASSES {
            let s = serde_json::to_string(&c).unwrap();
            let back: AuthorCapClass = serde_json::from_str(&s).unwrap();
            assert_eq!(c, back);
        }
    }
}
