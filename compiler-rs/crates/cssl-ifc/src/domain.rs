//! `SensitiveDomain` enumeration : domain-tags carried on `Sensitive<dom>`
//! effects + on `LabeledValue::sensitive_domains`.
//!
//! § SPEC : `specs/11_IFC.csl` § PRIME-DIRECTIVE ENCODING + `specs/04_EFFECTS.csl`
//! § PRIME-DIRECTIVE EFFECTS.
//!
//! § T11-D132 BIOMETRIC EXTENSION
//!   The biometric-family domains `Biometric`, `Gaze`, `Face`, `Body` are
//!   treated as **absolutely-egress-banned at the telemetry-ring boundary**
//!   per PRIME-DIRECTIVE §1 (anti-surveillance). No `Privilege<*>` capability
//!   can authorize their flow to `TelemetryEgress` — the structural gate is
//!   non-overridable.
//!
//!   This is the *complement* to `cssl-effects::banned::SensitiveDomain` —
//!   that enum operates at the effect-row layer + reasons about
//!   composition with `IO` / `Net`. This enum operates at the IFC layer +
//!   reasons about labels carried on values.
//!
//! § COMPATIBILITY WITH cssl-effects
//!   The biometric-family domains here will, in `W3β-04`, also become first-
//!   class `cssl-effects::SensitiveDomain` variants. Until that lands, the
//!   biometric-family is mocked at the IFC layer here + at the telemetry
//!   boundary in `cssl-telemetry` — see [`SensitiveDomain::is_biometric_family`].
//!   The cross-walk is structural : both layers reject the same set of
//!   compositions, so the order of `W3β-04` vs `W3β-07` integration is
//!   irrelevant.

use core::fmt;

/// Domain-tag attached to `Sensitive<dom>` effects + `LabeledValue::sensitive_domains`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SensitiveDomain {
    /// Privacy / personal data (gated by Subject confidentiality).
    Privacy,
    /// Weapon-systems (compile-error unless `Privilege<Kernel>`).
    Weapon,
    /// Surveillance (absolute compile-error, no override).
    Surveillance,
    /// Coercion / behavior-modification (absolute compile-error, no override).
    Coercion,
    /// Manipulation (requires `Audit<"manipulation-review">` +
    /// `Privilege<Anthropic-Audit>`).
    Manipulation,
    // § Biometric-family (T11-D132 — PRIME §1 anti-surveillance)
    /// Biometric (heart-rate, body-temp, breath, pulse-ox, …).
    /// Absolute-egress-banned at the telemetry-ring boundary.
    Biometric,
    /// Gaze (eye-tracking : pupil-position, fixation, saccade).
    /// Absolute-egress-banned at the telemetry-ring boundary.
    Gaze,
    /// Face (facial-tracking : expression, identity, emotion).
    /// Absolute-egress-banned at the telemetry-ring boundary.
    Face,
    /// Body (body-pose, hand-position, full-body-mocap).
    /// Absolute-egress-banned at the telemetry-ring boundary.
    Body,
}

impl SensitiveDomain {
    /// Canonical short-name (stable diagnostic + serialization form).
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

    /// Build a domain from a label-string. Unknown labels return `None` ;
    /// callers should treat unknowns as opaque user-defined labels (handled
    /// elsewhere in `cssl-effects`).
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "privacy" => Some(Self::Privacy),
            "weapon" => Some(Self::Weapon),
            "surveillance" => Some(Self::Surveillance),
            "coercion" => Some(Self::Coercion),
            "manipulation" => Some(Self::Manipulation),
            "biometric" => Some(Self::Biometric),
            "gaze" => Some(Self::Gaze),
            "face" => Some(Self::Face),
            "body" => Some(Self::Body),
            _ => None,
        }
    }

    /// `true` iff this domain is in the **biometric-family** :
    /// `Biometric` ∪ `Gaze` ∪ `Face` ∪ `Body`.
    ///
    /// Per PRIME-DIRECTIVE §1, biometric-family domains are **absolutely-
    /// egress-banned at the telemetry-ring boundary** (`cssl-telemetry`
    /// `TelemetrySlot::record_labeled` refuses any value with this domain).
    /// No `Privilege<*>` capability can override this gate.
    #[must_use]
    pub const fn is_biometric_family(self) -> bool {
        matches!(self, Self::Biometric | Self::Gaze | Self::Face | Self::Body)
    }

    /// `true` iff this domain is absolutely-egress-banned at the telemetry-
    /// ring boundary regardless of any `Privilege<*>` capability :
    /// biometric-family ∪ `Surveillance` ∪ `Coercion`.
    #[must_use]
    pub const fn is_telemetry_egress_absolutely_banned(self) -> bool {
        matches!(
            self,
            Self::Biometric
                | Self::Gaze
                | Self::Face
                | Self::Body
                | Self::Surveillance
                | Self::Coercion,
        )
    }

    /// `true` iff this domain is permitted to flow to telemetry only with a
    /// suitable `Privilege<*>` capability + `Audit<*>` effect, but not
    /// absolutely banned. Currently only `Weapon` matches (gated by
    /// `Privilege<Kernel>`).
    #[must_use]
    pub const fn is_telemetry_egress_privilege_gated(self) -> bool {
        matches!(self, Self::Weapon)
    }

    /// All domains, for table-driven tests + `for_each` walks.
    pub const ALL: [Self; 9] = [
        Self::Privacy,
        Self::Weapon,
        Self::Surveillance,
        Self::Coercion,
        Self::Manipulation,
        Self::Biometric,
        Self::Gaze,
        Self::Face,
        Self::Body,
    ];

    /// All biometric-family domains, for table-driven tests.
    pub const BIOMETRIC_FAMILY: [Self; 4] =
        [Self::Biometric, Self::Gaze, Self::Face, Self::Body];
}

impl fmt::Display for SensitiveDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::SensitiveDomain;

    #[test]
    fn as_str_canonical() {
        assert_eq!(SensitiveDomain::Privacy.as_str(), "privacy");
        assert_eq!(SensitiveDomain::Surveillance.as_str(), "surveillance");
        assert_eq!(SensitiveDomain::Biometric.as_str(), "biometric");
        assert_eq!(SensitiveDomain::Gaze.as_str(), "gaze");
        assert_eq!(SensitiveDomain::Face.as_str(), "face");
        assert_eq!(SensitiveDomain::Body.as_str(), "body");
    }

    #[test]
    fn from_label_roundtrip() {
        for d in SensitiveDomain::ALL {
            let label = d.as_str();
            assert_eq!(SensitiveDomain::from_label(label), Some(d));
        }
    }

    #[test]
    fn from_label_unknown_returns_none() {
        assert!(SensitiveDomain::from_label("unknown-domain").is_none());
        assert!(SensitiveDomain::from_label("").is_none());
    }

    #[test]
    fn biometric_family_predicate_covers_four() {
        for d in SensitiveDomain::BIOMETRIC_FAMILY {
            assert!(d.is_biometric_family(), "{:?}", d);
        }
        assert_eq!(SensitiveDomain::BIOMETRIC_FAMILY.len(), 4);
    }

    #[test]
    fn biometric_family_predicate_false_for_others() {
        assert!(!SensitiveDomain::Privacy.is_biometric_family());
        assert!(!SensitiveDomain::Weapon.is_biometric_family());
        assert!(!SensitiveDomain::Surveillance.is_biometric_family());
        assert!(!SensitiveDomain::Coercion.is_biometric_family());
        assert!(!SensitiveDomain::Manipulation.is_biometric_family());
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
        // Weapon is privilege-gated, NOT absolute.
        assert!(!SensitiveDomain::Weapon.is_telemetry_egress_absolutely_banned());
        assert!(!SensitiveDomain::Privacy.is_telemetry_egress_absolutely_banned());
        assert!(!SensitiveDomain::Manipulation.is_telemetry_egress_absolutely_banned());
    }

    #[test]
    fn privilege_gated_only_weapon() {
        assert!(SensitiveDomain::Weapon.is_telemetry_egress_privilege_gated());
        for d in SensitiveDomain::ALL {
            if d != SensitiveDomain::Weapon {
                assert!(!d.is_telemetry_egress_privilege_gated(), "{:?}", d);
            }
        }
    }

    #[test]
    fn all_has_nine_domains() {
        assert_eq!(SensitiveDomain::ALL.len(), 9);
    }

    #[test]
    fn all_unique() {
        let mut seen = std::collections::HashSet::new();
        for d in SensitiveDomain::ALL {
            assert!(seen.insert(d));
        }
        assert_eq!(seen.len(), 9);
    }

    #[test]
    fn display_matches_as_str() {
        for d in SensitiveDomain::ALL {
            assert_eq!(format!("{}", d), d.as_str());
        }
    }
}
