//! CSSLv3 stage0 — Jif-DLM label lattice + declassification + PRIME-DIRECTIVE encoding.
//!
//! Authoritative design : `specs/11_IFC.csl`.
//!
//! § STATUS : T11-D132 (W3β-07) — label-lattice live ; biometric-egress
//! structural-gate active at telemetry-ring boundary (`cssl-telemetry`).
//! § PRIME-DIRECTIVE (immutable) :
//!     `consent=OS • violation=bug • no-override-exists`
//!   encoded structurally via IFC labels + `{Sensitive<dom>}` + `{Audit<dom>}` +
//!   `{Privilege<level>}` effects — NOT as policy attached at runtime.
//!
//! § MODULE-LAYOUT
//!   - [`principal`] : `Principal` universe + `PrincipalSet` algebra.
//!     Includes the **biometric-family** principals
//!     (`BiometricSubject`, `GazeSubject`, `FaceSubject`, `BodySubject`)
//!     enforced at the telemetry boundary per PRIME-DIRECTIVE §1.
//!   - [`label`]     : `Confidentiality` + `Integrity` + `Label` lattice
//!     (`flows-to`, `join`, `meet`, `top`, `bottom`).
//!   - [`domain`]    : `SensitiveDomain` enum tagged on
//!     `Sensitive<dom>` effects + on labeled values. Includes biometric-
//!     family domains (`Biometric`, `Gaze`, `Face`, `Body`).
//!   - [`labeled`]   : `LabeledValue<T>` host-side carrier for
//!     `secret<T, L>` with biometric-detection predicates.
//!   - [`egress`]    : `TelemetryEgress` capability + `validate_egress` —
//!     the structural-gate that refuses biometric egress AT COMPILE-TIME.
//!
//! § BIOMETRIC-COMPILE-REFUSAL (T11-D132)
//!   The PRIME-DIRECTIVE §1 anti-surveillance prohibition on biometric
//!   egress is encoded structurally through three coordinated mechanisms :
//!
//!   1. Domain-level : [`SensitiveDomain::is_biometric_family`] +
//!      [`SensitiveDomain::is_telemetry_egress_absolutely_banned`]
//!      identify the four biometric domains as absolute-banned.
//!   2. Principal-level : [`Principal::is_biometric_family`] identifies
//!      the four biometric subject-principals ; any value with one of
//!      these as a confidentiality reader cannot egress.
//!   3. Capability-level : [`TelemetryEgress::for_domain`] +
//!      [`validate_egress`] return `Err(BiometricRefused)` and there is
//!      no unsafe alternative, no `Privilege` override, no flag/
//!      config knob (per PRIME_DIRECTIVE.md § 6 SCOPE).
//!
//!   Downstream, `cssl-telemetry`'s `TelemetrySlot::record_labeled` wires
//!   [`validate_egress`] directly into the producer-side : the `record`
//!   call is non-overridable-refused for biometric values. The `cssl-mir`
//!   lowering pass `biometric_egress_check` walks each
//!   `cssl.telemetry.record` op and applies the same gate at MIR-time.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod domain;
pub mod egress;
pub mod label;
pub mod labeled;
pub mod principal;

pub use domain::SensitiveDomain;
pub use egress::{validate_egress, EgressGrantError, PrivilegeLevel, TelemetryEgress};
pub use label::{Confidentiality, Integrity, Label};
pub use labeled::LabeledValue;
pub use principal::{Principal, PrincipalSet};

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    #[test]
    fn scaffold_version_present() {
        assert!(!super::STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn public_api_re_exports_resolve() {
        // Compile-time check that the re-exports are accessible.
        let _: super::Principal = super::Principal::User;
        let _: super::PrincipalSet = super::PrincipalSet::empty();
        let _: super::Label = super::Label::bottom();
        let _: super::SensitiveDomain = super::SensitiveDomain::Privacy;
        let _: super::PrivilegeLevel = super::PrivilegeLevel::User;
    }

    #[test]
    fn end_to_end_biometric_refusal_via_top_level_api() {
        use super::{validate_egress, EgressGrantError, LabeledValue, SensitiveDomain};
        let label = super::Label::bottom();
        for d in SensitiveDomain::BIOMETRIC_FAMILY {
            let v: LabeledValue<u32> = LabeledValue::with_domain(0, label.clone(), d);
            let res = validate_egress(&v);
            assert!(matches!(
                res,
                Err(EgressGrantError::BiometricRefused { domain }) if domain == d
            ), "{:?}", d);
        }
    }
}
