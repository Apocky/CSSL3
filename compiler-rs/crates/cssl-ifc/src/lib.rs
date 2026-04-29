//! CSSLv3 stage0 — Jif-DLM label lattice + declassification + PRIME-DIRECTIVE encoding.
//!
//! Authoritative design : `specs/11_IFC.csl`.
//!
//! § STATUS : T11-D129 — biometric / gaze / face / body label-lattice + declass
//!   refusal authored. Earlier prose-only scaffold replaced.
//! § PRIME-DIRECTIVE (immutable) :
//!     `consent=OS • violation=bug • no-override-exists`
//!   encoded structurally via IFC labels + `{Sensitive<dom>}` + `{Audit<dom>}` +
//!   `{Privilege<level>}` effects — NOT as policy attached at runtime.
//!
//! § BIOMETRIC ANTI-SURVEILLANCE (T11-D129, F5)
//!   Per `PRIME_DIRECTIVE.md` §1 N! surveillance + the new P18 `BiometricEgress`
//!   prohibition, biometric / gaze / face-tracking / body-tracking data is born
//!   at `Confidentiality::Confidential` ⊓ `Integrity::Root` (highest label) and
//!   has NO declassification path — `Privilege<L>` regardless of level CANNOT
//!   override. Composition with `{Net}` or `{Telemetry<*>}` is an absolute ban
//!   detected at the effect-row level (`cssl-effects::banned_composition`).
//!
//! § ATTESTATION
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."  Per §1 anti-surveillance : raw biometric data NEVER egresses
//!   the device boundary on which the user resides.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod label;
pub mod secret;

pub use label::{
    BiometricKind, Confidentiality, DeclassifyError, IfcLabel, Integrity, PrivilegeLevel,
};
pub use secret::{declassify, Secret};

/// The canonical PRIME-DIRECTIVE attestation.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    #[test]
    fn scaffold_version_present() {
        assert!(!super::STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_text_pinned() {
        assert!(super::ATTESTATION.contains("no hurt nor harm"));
        assert!(super::ATTESTATION.contains("anyone"));
        assert!(super::ATTESTATION.contains("anything"));
        assert!(super::ATTESTATION.contains("anybody"));
    }
}
