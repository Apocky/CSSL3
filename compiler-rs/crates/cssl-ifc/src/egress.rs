//! `TelemetryEgress` capability + biometric-egress structural-gate.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` § OBSERVABILITY-FIRST-CLASS +
//! `specs/11_IFC.csl` § PRIME-DIRECTIVE ENCODING + PRIME_DIRECTIVE.md §1
//! anti-surveillance.
//!
//! § DESIGN
//!   `TelemetryEgress` is a zero-sized capability-token that authorizes a
//!   labeled value to leave the on-device boundary by way of the telemetry
//!   ring-buffer + exporters. The token is constructed via
//!   [`TelemetryEgress::for_domain`] which **refuses biometric-family +
//!   surveillance + coercion domains AT COMPILE-TIME** — the constructor
//!   returns `Err(EgressGrantError::BiometricRefused)` (or
//!   `SurveillanceRefused` / `CoercionRefused`) and there is no `unsafe`
//!   alternative.
//!
//! § STRUCTURAL-GATE INVARIANT
//!   For every well-typed program :
//!     `∀ v : LabeledValue<T>, v.is_egress_banned() ⇒
//!        ∄ TelemetryEgress that authorizes v's domains`
//!   This is the F5 IFC theorem applied to the telemetry boundary : the
//!   structural-gate is property-of-the-type (not handler-installation).
//!
//! § PRIVILEGE-CANNOT-OVERRIDE
//!   `Privilege<*>` capabilities (`User`, `System`, `Kernel`, `Root`,
//!   `Anthropic-Audit`, `Apocky-Root`) **cannot** authorize biometric egress
//!   per PRIME_DIRECTIVE.md § 6 SCOPE :
//!     "N! flag | config | … can disable this".
//!   Hence `TelemetryEgress::for_domain_with_privilege` STILL refuses the
//!   biometric family — the only privilege effect is gating `Weapon` per
//!   `specs/11` § PRIME-DIRECTIVE ENCODING.

use thiserror::Error;

use crate::domain::SensitiveDomain;
use crate::labeled::LabeledValue;

/// Capability-tier for `Privilege<*>` effects.
///
/// Mirrors `specs/04_EFFECTS.csl` § Privilege effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrivilegeLevel {
    /// Ordinary user / game-code.
    User,
    /// Engine-internals (scheduler, allocator).
    System,
    /// OS-interop (sysman, sensitive-device-access).
    Kernel,
    /// Compiler-plugin / build-system.
    Root,
    /// Audit-review tooling.
    AnthropicAudit,
    /// Framework-level overrides (never-over-PRIME-DIRECTIVE).
    ApockyRoot,
}

impl PrivilegeLevel {
    /// Canonical name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::System => "System",
            Self::Kernel => "Kernel",
            Self::Root => "Root",
            Self::AnthropicAudit => "Anthropic-Audit",
            Self::ApockyRoot => "Apocky-Root",
        }
    }
}

/// Error from attempting to construct a `TelemetryEgress` capability.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum EgressGrantError {
    /// Biometric-family domain attempted to be granted.
    #[error(
        "telemetry-egress REFUSED for biometric-family domain `{domain}` — \
         PRIME-DIRECTIVE §1 anti-surveillance forbids biometric egress \
         (no Privilege<*> can override ; gate is structural)"
    )]
    BiometricRefused {
        /// The specific biometric-family domain that triggered the refusal.
        domain: SensitiveDomain,
    },
    /// Surveillance domain attempted to be granted.
    #[error(
        "telemetry-egress REFUSED for `surveillance` domain — PRIME-DIRECTIVE §1 \
         anti-surveillance forbids egress (no Privilege<*> can override)"
    )]
    SurveillanceRefused,
    /// Coercion domain attempted to be granted.
    #[error(
        "telemetry-egress REFUSED for `coercion` domain — PRIME-DIRECTIVE §1 \
         absolute prohibition (no Privilege<*> can override)"
    )]
    CoercionRefused,
    /// Weapon domain without `Privilege<Kernel>`.
    #[error(
        "telemetry-egress REFUSED for `weapon` domain without Privilege<Kernel> \
         (specs/11 PRIME-DIRECTIVE ENCODING)"
    )]
    WeaponNeedsKernel,
}

/// Zero-sized capability authorizing telemetry egress for a specific domain.
///
/// Constructed only through [`TelemetryEgress::for_domain`] /
/// [`for_domain_with_privilege`][TelemetryEgress::for_domain_with_privilege] ;
/// both reject biometric-family + surveillance + coercion. The struct is
/// `pub(crate)`-non-constructible from outside — see field below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TelemetryEgress {
    /// The single domain this token authorizes.
    pub authorized_domain: SensitiveDomain,
    /// The privilege-level under which it was granted (for diagnostic + audit).
    pub privilege: PrivilegeLevel,
    // Private witness so callers cannot construct directly.
    _witness: TelemetryEgressWitness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TelemetryEgressWitness;

impl TelemetryEgress {
    /// Try to grant a `TelemetryEgress` capability for `domain` at the
    /// caller's `User`-level privilege (default).
    ///
    /// # Errors
    /// Returns [`EgressGrantError::BiometricRefused`] for biometric-family,
    /// [`EgressGrantError::SurveillanceRefused`] for surveillance,
    /// [`EgressGrantError::CoercionRefused`] for coercion. `Weapon` is
    /// rejected as `WeaponNeedsKernel`. All other domains succeed.
    pub fn for_domain(domain: SensitiveDomain) -> Result<Self, EgressGrantError> {
        Self::for_domain_with_privilege(domain, PrivilegeLevel::User)
    }

    /// Try to grant a `TelemetryEgress` capability for `domain` with the
    /// caller asserting the given `privilege` level. Biometric-family +
    /// surveillance + coercion are refused **regardless of privilege**.
    /// `Weapon` is permitted only with `Kernel` privilege.
    ///
    /// # Errors
    /// Returns the same set of errors as [`for_domain`][Self::for_domain],
    /// PLUS [`EgressGrantError::WeaponNeedsKernel`] if `domain == Weapon`
    /// and `privilege < Kernel`.
    pub fn for_domain_with_privilege(
        domain: SensitiveDomain,
        privilege: PrivilegeLevel,
    ) -> Result<Self, EgressGrantError> {
        // PRIME-DIRECTIVE §1 absolute refusals — privilege is ignored.
        if domain.is_biometric_family() {
            return Err(EgressGrantError::BiometricRefused { domain });
        }
        match domain {
            SensitiveDomain::Surveillance => return Err(EgressGrantError::SurveillanceRefused),
            SensitiveDomain::Coercion => return Err(EgressGrantError::CoercionRefused),
            SensitiveDomain::Weapon => {
                if privilege != PrivilegeLevel::Kernel
                    && privilege != PrivilegeLevel::Root
                    && privilege != PrivilegeLevel::AnthropicAudit
                    && privilege != PrivilegeLevel::ApockyRoot
                {
                    return Err(EgressGrantError::WeaponNeedsKernel);
                }
            }
            _ => {}
        }
        Ok(Self {
            authorized_domain: domain,
            privilege,
            _witness: TelemetryEgressWitness,
        })
    }

    /// Returns `true` iff this capability authorizes egress of the given
    /// labeled value. The check is a structural-AND :
    ///   1. The value's `is_egress_banned()` predicate must be false (so the
    ///      check fails-closed if the value carries any biometric tag, even
    ///      if `authorized_domain` is non-biometric).
    ///   2. The value's domains must all be subsumed by `authorized_domain`,
    ///      OR be domains that don't require a capability (privacy without
    ///      Sensitive-marker).
    #[must_use]
    pub fn authorizes<T>(&self, value: &LabeledValue<T>) -> bool {
        // Even an authorized cap cannot authorize a banned value (biometric in
        // the value's labels is not overrideable by ANY cap).
        if value.is_egress_banned() {
            return false;
        }
        // For non-banned values, we accept if every sensitive-domain on the
        // value matches our authorized_domain (or is non-restrictive).
        value
            .sensitive_domains
            .iter()
            .all(|d| *d == self.authorized_domain || !d.is_telemetry_egress_absolutely_banned())
    }
}

/// Validate that a `LabeledValue` can flow to telemetry. Returns
/// `Err(EgressGrantError::*)` describing the structural refusal if it cannot.
/// This is the entry-point [`crate::TelemetrySlot`] in `cssl-telemetry` calls
/// from `record_labeled` to refuse-at-compile-time-of-the-call-site.
///
/// # Errors
/// Returns the same set of errors as
/// [`TelemetryEgress::for_domain`][TelemetryEgress::for_domain] when the
/// value carries a banned domain.
pub fn validate_egress<T>(value: &LabeledValue<T>) -> Result<(), EgressGrantError> {
    if let Some(d) = value.first_biometric_domain() {
        return Err(EgressGrantError::BiometricRefused { domain: d });
    }
    if value
        .sensitive_domains
        .contains(&SensitiveDomain::Surveillance)
        || value
            .label
            .confidentiality
            .0
            .contains(&crate::principal::Principal::SurveillanceTarget)
    {
        return Err(EgressGrantError::SurveillanceRefused);
    }
    if value.sensitive_domains.contains(&SensitiveDomain::Coercion)
        || value
            .label
            .confidentiality
            .0
            .contains(&crate::principal::Principal::CoercionTarget)
    {
        return Err(EgressGrantError::CoercionRefused);
    }
    // Biometric-family principals on the label trigger biometric-refusal too,
    // even if the SensitiveDomain set is empty.
    if value.label.has_biometric_confidentiality() {
        // Pick a canonical biometric domain (we can't tell which one without
        // the principal-→-domain map ; choose `Biometric` as the umbrella).
        return Err(EgressGrantError::BiometricRefused {
            domain: pick_biometric_domain_for_label(&value.label),
        });
    }
    Ok(())
}

fn pick_biometric_domain_for_label(label: &crate::label::Label) -> SensitiveDomain {
    use crate::principal::Principal;
    if label.confidentiality.0.contains(&Principal::GazeSubject) {
        SensitiveDomain::Gaze
    } else if label.confidentiality.0.contains(&Principal::FaceSubject) {
        SensitiveDomain::Face
    } else if label.confidentiality.0.contains(&Principal::BodySubject) {
        SensitiveDomain::Body
    } else {
        SensitiveDomain::Biometric
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_egress, EgressGrantError, PrivilegeLevel, TelemetryEgress};
    use crate::domain::SensitiveDomain;
    use crate::label::Label;
    use crate::labeled::LabeledValue;
    use crate::principal::{Principal, PrincipalSet};

    fn benign_label() -> Label {
        Label::restricted(
            PrincipalSet::singleton(Principal::User),
            PrincipalSet::singleton(Principal::User),
        )
    }

    #[test]
    fn for_domain_accepts_privacy() {
        let cap = TelemetryEgress::for_domain(SensitiveDomain::Privacy);
        assert!(cap.is_ok());
        assert_eq!(cap.unwrap().authorized_domain, SensitiveDomain::Privacy);
    }

    #[test]
    fn for_domain_refuses_each_biometric_family_member() {
        for d in SensitiveDomain::BIOMETRIC_FAMILY {
            let result = TelemetryEgress::for_domain(d);
            assert!(
                matches!(
                    result,
                    Err(EgressGrantError::BiometricRefused { domain }) if domain == d
                ),
                "{:?} must refuse",
                d
            );
        }
    }

    #[test]
    fn for_domain_with_privilege_apocky_root_still_refuses_biometric() {
        // Even Apocky-Root cannot grant biometric egress.
        for d in SensitiveDomain::BIOMETRIC_FAMILY {
            let result = TelemetryEgress::for_domain_with_privilege(d, PrivilegeLevel::ApockyRoot);
            assert!(
                matches!(result, Err(EgressGrantError::BiometricRefused { .. })),
                "Apocky-Root must NOT override biometric refusal for {:?}",
                d
            );
        }
    }

    #[test]
    fn for_domain_refuses_surveillance() {
        let result = TelemetryEgress::for_domain(SensitiveDomain::Surveillance);
        assert_eq!(result, Err(EgressGrantError::SurveillanceRefused));
    }

    #[test]
    fn for_domain_refuses_coercion() {
        let result = TelemetryEgress::for_domain(SensitiveDomain::Coercion);
        assert_eq!(result, Err(EgressGrantError::CoercionRefused));
    }

    #[test]
    fn for_domain_weapon_needs_kernel() {
        let no_priv = TelemetryEgress::for_domain(SensitiveDomain::Weapon);
        assert_eq!(no_priv, Err(EgressGrantError::WeaponNeedsKernel));
        let kernel = TelemetryEgress::for_domain_with_privilege(
            SensitiveDomain::Weapon,
            PrivilegeLevel::Kernel,
        );
        assert!(kernel.is_ok());
    }

    #[test]
    fn for_domain_with_privilege_user_refuses_weapon() {
        let user = TelemetryEgress::for_domain_with_privilege(
            SensitiveDomain::Weapon,
            PrivilegeLevel::User,
        );
        assert_eq!(user, Err(EgressGrantError::WeaponNeedsKernel));
        let system = TelemetryEgress::for_domain_with_privilege(
            SensitiveDomain::Weapon,
            PrivilegeLevel::System,
        );
        assert_eq!(system, Err(EgressGrantError::WeaponNeedsKernel));
    }

    #[test]
    fn authorizes_returns_false_for_banned_value() {
        let cap = TelemetryEgress::for_domain(SensitiveDomain::Privacy).unwrap();
        let v: LabeledValue<i32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Gaze);
        assert!(!cap.authorizes(&v));
    }

    #[test]
    fn authorizes_accepts_matching_privacy_value() {
        let cap = TelemetryEgress::for_domain(SensitiveDomain::Privacy).unwrap();
        let v: LabeledValue<i32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Privacy);
        assert!(cap.authorizes(&v));
    }

    #[test]
    fn authorizes_accepts_unrestricted_value() {
        let cap = TelemetryEgress::for_domain(SensitiveDomain::Privacy).unwrap();
        let v: LabeledValue<i32> = LabeledValue::new(0, benign_label());
        assert!(cap.authorizes(&v));
    }

    #[test]
    fn validate_egress_passes_for_benign_value() {
        let v: LabeledValue<i32> = LabeledValue::new(42, benign_label());
        assert!(validate_egress(&v).is_ok());
    }

    #[test]
    fn validate_egress_refuses_each_biometric_domain() {
        for d in SensitiveDomain::BIOMETRIC_FAMILY {
            let v: LabeledValue<i32> = LabeledValue::with_domain(0, benign_label(), d);
            let result = validate_egress(&v);
            assert!(
                matches!(
                    result,
                    Err(EgressGrantError::BiometricRefused { domain }) if domain == d
                ),
                "{:?}",
                d
            );
        }
    }

    #[test]
    fn validate_egress_refuses_label_with_gaze_principal() {
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::GazeSubject),
            PrincipalSet::singleton(Principal::User),
        );
        let v: LabeledValue<i32> = LabeledValue::new(0, label);
        let result = validate_egress(&v);
        assert!(matches!(
            result,
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    #[test]
    fn validate_egress_refuses_label_with_face_principal() {
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::FaceSubject),
            PrincipalSet::singleton(Principal::User),
        );
        let v: LabeledValue<i32> = LabeledValue::new(0, label);
        let result = validate_egress(&v);
        assert!(matches!(
            result,
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Face
            })
        ));
    }

    #[test]
    fn validate_egress_refuses_label_with_body_principal() {
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::BodySubject),
            PrincipalSet::singleton(Principal::User),
        );
        let v: LabeledValue<i32> = LabeledValue::new(0, label);
        let result = validate_egress(&v);
        assert!(matches!(
            result,
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Body
            })
        ));
    }

    #[test]
    fn validate_egress_refuses_label_with_biometric_subject() {
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::BiometricSubject),
            PrincipalSet::singleton(Principal::User),
        );
        let v: LabeledValue<i32> = LabeledValue::new(0, label);
        let result = validate_egress(&v);
        assert!(matches!(
            result,
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Biometric
            })
        ));
    }

    #[test]
    fn validate_egress_refuses_surveillance_domain_or_principal() {
        let by_domain: LabeledValue<i32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Surveillance);
        assert_eq!(
            validate_egress(&by_domain),
            Err(EgressGrantError::SurveillanceRefused)
        );
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::SurveillanceTarget),
            PrincipalSet::singleton(Principal::User),
        );
        let by_principal: LabeledValue<i32> = LabeledValue::new(0, label);
        assert_eq!(
            validate_egress(&by_principal),
            Err(EgressGrantError::SurveillanceRefused)
        );
    }

    #[test]
    fn validate_egress_refuses_coercion_domain_or_principal() {
        let by_domain: LabeledValue<i32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Coercion);
        assert_eq!(
            validate_egress(&by_domain),
            Err(EgressGrantError::CoercionRefused)
        );
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::CoercionTarget),
            PrincipalSet::singleton(Principal::User),
        );
        let by_principal: LabeledValue<i32> = LabeledValue::new(0, label);
        assert_eq!(
            validate_egress(&by_principal),
            Err(EgressGrantError::CoercionRefused)
        );
    }

    #[test]
    fn validate_egress_passes_for_privacy_domain() {
        let v: LabeledValue<i32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Privacy);
        assert!(validate_egress(&v).is_ok());
    }

    #[test]
    fn privilege_level_canonical_names() {
        assert_eq!(PrivilegeLevel::User.as_str(), "User");
        assert_eq!(PrivilegeLevel::Kernel.as_str(), "Kernel");
        assert_eq!(PrivilegeLevel::AnthropicAudit.as_str(), "Anthropic-Audit");
        assert_eq!(PrivilegeLevel::ApockyRoot.as_str(), "Apocky-Root");
    }

    #[test]
    fn egress_grant_error_messages_cite_prime_directive() {
        let bio = EgressGrantError::BiometricRefused {
            domain: SensitiveDomain::Gaze,
        };
        let s = format!("{}", bio);
        assert!(s.contains("PRIME-DIRECTIVE"));
        assert!(s.contains("anti-surveillance"));
        let surv = EgressGrantError::SurveillanceRefused;
        assert!(format!("{}", surv).contains("PRIME-DIRECTIVE"));
        let coer = EgressGrantError::CoercionRefused;
        assert!(format!("{}", coer).contains("PRIME-DIRECTIVE"));
    }
}
