//! Local IFC-shim : `Label` + `LabeledValue<T>` + `SensitiveDomain` +
//! `validate_egress` + `EgressGrantError`.
//!
//! § RATIONALE
//!   The `cssl-ifc` crate at this worktree's snapshot is a stage-0 scaffold
//!   that does not yet expose `Label` + `LabeledValue` + `SensitiveDomain`
//!   in its public API. The full IFC machinery lands in T11-D132 (W3β-07).
//!
//!   This shim is a **faithful structural mirror** of the post-D132
//!   `cssl-ifc` API : same domain enum, same labeling semantics, same
//!   `validate_egress` non-overridable refusal for biometric values.
//!   When this worktree merges + the full `cssl-ifc` lands, the shim is
//!   replaced with `pub use cssl_ifc::*;` re-exports — the consumer-side
//!   API contract is identical.
//!
//! § PRIME-DIRECTIVE §1 (anti-surveillance) STRUCTURAL ENFORCEMENT
//!   `validate_egress` returns `Err(BiometricRefused)` for any value
//!   carrying `SensitiveDomain::{Gaze, Face, Body, Biometric}` — no
//!   `Privilege<*>` capability changes the return-value, no unsafe
//!   alternative exists, no flag/config knob enables egress.

use std::collections::BTreeSet;

/// Sensitive-domain tag carried on `LabeledValue::sensitive_domains`.
/// Mirror of `cssl_ifc::SensitiveDomain` (post-D132).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SensitiveDomain {
    /// Privacy / personal data (gated by Subject confidentiality).
    Privacy,
    /// Weapon-systems (privilege-gated).
    Weapon,
    /// Surveillance (absolute compile-error).
    Surveillance,
    /// Coercion (absolute compile-error).
    Coercion,
    /// Manipulation (audit-required).
    Manipulation,
    /// Biometric (heart-rate, body-temp, breath, …). Absolute-egress-banned.
    Biometric,
    /// Gaze (eye-tracking : pupil-position, fixation, saccade). Absolute-egress-banned.
    Gaze,
    /// Face (facial-tracking : expression, identity, emotion). Absolute-egress-banned.
    Face,
    /// Body (body-pose, hand-position, full-body-mocap). Absolute-egress-banned.
    Body,
}

impl SensitiveDomain {
    /// `true` iff this domain is in the biometric-family.
    #[must_use]
    pub const fn is_biometric_family(self) -> bool {
        matches!(self, Self::Biometric | Self::Gaze | Self::Face | Self::Body)
    }

    /// `true` iff this domain is absolutely-egress-banned at the
    /// telemetry-ring boundary regardless of any `Privilege<*>` capability.
    #[must_use]
    pub const fn is_telemetry_egress_absolutely_banned(self) -> bool {
        matches!(
            self,
            Self::Biometric
                | Self::Gaze
                | Self::Face
                | Self::Body
                | Self::Surveillance
                | Self::Coercion
        )
    }

    /// All four biometric-family domains.
    pub const BIOMETRIC_FAMILY: [Self; 4] = [Self::Biometric, Self::Gaze, Self::Face, Self::Body];

    /// Canonical short-name for diagnostics + serialization.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Privacy => "privacy",
            Self::Weapon => "weapon",
            Self::Surveillance => "surveillance",
            Self::Coercion => "coercion",
            Self::Manipulation => "manipulation",
            Self::Biometric => "biometric",
            Self::Gaze => "gaze",
            Self::Face => "face",
            Self::Body => "body",
        }
    }
}

impl std::fmt::Display for SensitiveDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// IFC label (confidentiality + integrity). Stage-0 minimal : just a
/// Top/Bottom marker. Real lattice lands when `cssl-ifc` IFC-machinery
/// merges into this worktree.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Label {
    /// Bottom = least-confidential.
    Bottom,
    /// Top = most-confidential.
    Top,
}

impl Label {
    /// Bottom label.
    #[must_use]
    pub const fn bottom() -> Self {
        Self::Bottom
    }

    /// Top label.
    #[must_use]
    pub const fn top() -> Self {
        Self::Top
    }
}

/// Labeled value carrier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabeledValue<T> {
    /// The wrapped value.
    pub value: T,
    /// IFC label.
    pub label: Label,
    /// Sensitive-domain tags.
    pub sensitive_domains: BTreeSet<SensitiveDomain>,
}

impl<T> LabeledValue<T> {
    /// Wrap with a single domain tag.
    pub fn with_domain(value: T, label: Label, domain: SensitiveDomain) -> Self {
        let mut domains = BTreeSet::new();
        domains.insert(domain);
        Self {
            value,
            label,
            sensitive_domains: domains,
        }
    }

    /// `true` iff any tag is biometric-family.
    #[must_use]
    pub fn is_biometric(&self) -> bool {
        self.sensitive_domains
            .iter()
            .any(|d| d.is_biometric_family())
    }

    /// `true` iff this value is absolutely-egress-banned.
    #[must_use]
    pub fn is_egress_banned(&self) -> bool {
        self.sensitive_domains
            .iter()
            .any(|d| d.is_telemetry_egress_absolutely_banned())
    }

    /// First biometric-family domain found.
    #[must_use]
    pub fn first_biometric_domain(&self) -> Option<SensitiveDomain> {
        self.sensitive_domains
            .iter()
            .copied()
            .find(|d| d.is_biometric_family())
    }
}

/// Egress-grant error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EgressGrantError {
    /// Biometric value refused. Carries the domain.
    BiometricRefused {
        /// Which domain.
        domain: SensitiveDomain,
    },
    /// Surveillance value refused.
    SurveillanceRefused,
    /// Coercion value refused.
    CoercionRefused,
}

/// **The structural-gate.** Returns `Err(BiometricRefused)` for any
/// labeled value carrying a biometric-family domain. No `Privilege<*>`
/// capability changes the return-value.
///
/// § PRIME-DIRECTIVE §1 (anti-surveillance) compliance.
pub fn validate_egress<T>(value: &LabeledValue<T>) -> Result<(), EgressGrantError> {
    if let Some(d) = value.first_biometric_domain() {
        return Err(EgressGrantError::BiometricRefused { domain: d });
    }
    if value.sensitive_domains.contains(&SensitiveDomain::Surveillance) {
        return Err(EgressGrantError::SurveillanceRefused);
    }
    if value.sensitive_domains.contains(&SensitiveDomain::Coercion) {
        return Err(EgressGrantError::CoercionRefused);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_egress, EgressGrantError, Label, LabeledValue, SensitiveDomain};

    #[test]
    fn biometric_family_predicate_covers_four() {
        for d in SensitiveDomain::BIOMETRIC_FAMILY {
            assert!(d.is_biometric_family(), "{:?}", d);
        }
    }

    #[test]
    fn absolute_egress_ban_covers_biometric_plus_surveillance_plus_coercion() {
        for d in [
            SensitiveDomain::Biometric,
            SensitiveDomain::Gaze,
            SensitiveDomain::Face,
            SensitiveDomain::Body,
            SensitiveDomain::Surveillance,
            SensitiveDomain::Coercion,
        ] {
            assert!(d.is_telemetry_egress_absolutely_banned(), "{:?}", d);
        }
    }

    #[test]
    fn gaze_egress_refused() {
        let v = LabeledValue::with_domain(0u32, Label::bottom(), SensitiveDomain::Gaze);
        let err = validate_egress(&v).unwrap_err();
        assert!(matches!(err, EgressGrantError::BiometricRefused { domain: SensitiveDomain::Gaze }));
    }

    #[test]
    fn face_egress_refused() {
        let v = LabeledValue::with_domain(0u32, Label::bottom(), SensitiveDomain::Face);
        let err = validate_egress(&v).unwrap_err();
        assert!(matches!(err, EgressGrantError::BiometricRefused { domain: SensitiveDomain::Face }));
    }

    #[test]
    fn body_egress_refused() {
        let v = LabeledValue::with_domain(0u32, Label::bottom(), SensitiveDomain::Body);
        let err = validate_egress(&v).unwrap_err();
        assert!(matches!(err, EgressGrantError::BiometricRefused { domain: SensitiveDomain::Body }));
    }

    #[test]
    fn privacy_egress_allowed() {
        let v = LabeledValue::with_domain(0u32, Label::bottom(), SensitiveDomain::Privacy);
        // Privacy isn't absolutely-banned.
        assert!(validate_egress(&v).is_ok());
    }

    #[test]
    fn surveillance_egress_refused() {
        let v = LabeledValue::with_domain(0u32, Label::bottom(), SensitiveDomain::Surveillance);
        assert!(matches!(
            validate_egress(&v),
            Err(EgressGrantError::SurveillanceRefused)
        ));
    }

    #[test]
    fn label_bottom_top_distinct() {
        assert_ne!(Label::bottom(), Label::top());
    }

    #[test]
    fn first_biometric_domain_returns_canonical() {
        let v = LabeledValue::with_domain(0u32, Label::bottom(), SensitiveDomain::Gaze);
        assert_eq!(v.first_biometric_domain(), Some(SensitiveDomain::Gaze));
    }
}
