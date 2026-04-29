//! Principal universe + principal-set algebra.
//!
//! § SPEC : `specs/11_IFC.csl` § LABEL ALGEBRA + § PRIME-DIRECTIVE ENCODING.
//!
//! § DESIGN
//!   A `Principal` identifies a reader OR an influencer. Built-in principals
//!   encode the PRIME-DIRECTIVE domain-roles (Subject, HarmTarget,
//!   SurveillanceTarget, BiometricSubject, GazeSubject, FaceSubject,
//!   BodySubject, …). User-defined principals are carried as `Other(name)`.
//!
//!   `PrincipalSet` is an immutable ordered set (BTreeSet) so set-comparison +
//!   subset-checks needed by the lattice (`⊑`) are O(n log n) deterministic.
//!
//! § PRIME-DIRECTIVE biometric extension (T11-D132)
//!   The principals `BiometricSubject`, `GazeSubject`, `FaceSubject`,
//!   `BodySubject` mark the readers of physiological/gaze/face/body
//!   tracking-data. PRIME-DIRECTIVE §1 forbids egress of these data outside
//!   the on-device boundary, so the telemetry-ring boundary refuses any value
//!   whose Confidentiality set is restricted to one of these principals.

use core::fmt;
use std::collections::BTreeSet;

/// A principal in the DLM principal-universe.
///
/// Built-ins encode PRIME-DIRECTIVE roles ; user-defined principals appear as
/// `Other`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Principal {
    /// Ordinary user / game-code.
    User,
    /// Engine-internal subsystem.
    System,
    /// OS-interop layer (sysman, device-access).
    Kernel,
    /// Compiler-plugin / build-system.
    Root,
    /// Audit-review tooling (Anthropic-Audit reviewer-role).
    AnthropicAudit,
    /// Framework-level overrides (Apocky-Root) — never-over-PRIME-DIRECTIVE.
    ApockyRoot,
    /// The user themselves (subject of `@sensitive(privacy)`).
    Subject,
    /// Anyone-who-might-be-harmed (PRIME §1).
    HarmTarget,
    /// Surveillance-victim (PRIME §1).
    SurveillanceTarget,
    /// Coercion-victim (PRIME §1).
    CoercionTarget,
    /// Weapon-target (PRIME §1).
    WeaponTarget,
    // § Biometric-family (T11-D132 — PRIME §1 anti-surveillance)
    /// Biometric-subject (heart-rate, breath, body-temp, pulse-ox, …).
    BiometricSubject,
    /// Gaze-subject (eye-tracking — pupil-position, fixation, saccade).
    GazeSubject,
    /// Face-subject (facial-tracking — expression, identity, emotion).
    FaceSubject,
    /// Body-subject (body-pose, hand-position, full-body-mocap).
    BodySubject,
    /// User-defined principal, carried by canonical name.
    Other(&'static str),
}

impl Principal {
    /// Canonical short-name (stable diagnostic + serialization form).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::User => "User",
            Self::System => "System",
            Self::Kernel => "Kernel",
            Self::Root => "Root",
            Self::AnthropicAudit => "Anthropic-Audit",
            Self::ApockyRoot => "Apocky-Root",
            Self::Subject => "Subject",
            Self::HarmTarget => "HarmTarget",
            Self::SurveillanceTarget => "SurveillanceTarget",
            Self::CoercionTarget => "CoercionTarget",
            Self::WeaponTarget => "WeaponTarget",
            Self::BiometricSubject => "BiometricSubject",
            Self::GazeSubject => "GazeSubject",
            Self::FaceSubject => "FaceSubject",
            Self::BodySubject => "BodySubject",
            Self::Other(name) => name,
        }
    }

    /// True iff this principal is in the **biometric-family** :
    /// `BiometricSubject` ∪ `GazeSubject` ∪ `FaceSubject` ∪ `BodySubject`.
    ///
    /// Per PRIME-DIRECTIVE §1 (anti-surveillance), values whose
    /// confidentiality-set is restricted to a biometric-family principal
    /// MUST NOT egress past the on-device boundary — including any
    /// telemetry-ring producer. This predicate is used by the
    /// telemetry-ring to refuse egress at compile-time.
    #[must_use]
    pub const fn is_biometric_family(&self) -> bool {
        matches!(
            self,
            Self::BiometricSubject | Self::GazeSubject | Self::FaceSubject | Self::BodySubject,
        )
    }

    /// True iff this principal is a PRIME-DIRECTIVE absolute-egress-banned
    /// reader (biometric-family ∪ SurveillanceTarget ∪ CoercionTarget).
    ///
    /// `WeaponTarget` is **not** absolute-egress-banned here — it is gated by
    /// `Privilege<Kernel>` per `specs/11` § PRIME-DIRECTIVE ENCODING. The
    /// banned-composition checker in `cssl-effects` handles that gate
    /// separately ; this predicate covers only the no-override class.
    #[must_use]
    pub const fn is_egress_absolutely_banned(&self) -> bool {
        matches!(
            self,
            Self::BiometricSubject
                | Self::GazeSubject
                | Self::FaceSubject
                | Self::BodySubject
                | Self::SurveillanceTarget
                | Self::CoercionTarget,
        )
    }
}

impl fmt::Display for Principal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Ordered set of principals.
///
/// Used as the carrier for both `Confidentiality` (who-can-read) and
/// `Integrity` (who-can-influence) labels. Ordered to keep set-equality +
/// subset checks deterministic across builds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PrincipalSet {
    inner: BTreeSet<Principal>,
}

impl PrincipalSet {
    /// Empty set.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Set with a single principal.
    #[must_use]
    pub fn singleton(p: Principal) -> Self {
        let mut s = Self::default();
        s.inner.insert(p);
        s
    }

    /// Add a principal (idempotent).
    pub fn insert(&mut self, p: Principal) {
        self.inner.insert(p);
    }

    /// Remove a principal ; returns `true` if it was present.
    pub fn remove(&mut self, p: &Principal) -> bool {
        self.inner.remove(p)
    }

    /// `true` iff `p` is in the set.
    #[must_use]
    pub fn contains(&self, p: &Principal) -> bool {
        self.inner.contains(p)
    }

    /// Cardinality.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Empty check.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate (sorted).
    pub fn iter(&self) -> impl Iterator<Item = &Principal> {
        self.inner.iter()
    }

    /// Set union (`A ∪ B`).
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for p in &other.inner {
            out.inner.insert(p.clone());
        }
        out
    }

    /// Set intersection (`A ∩ B`).
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        let mut out = Self::empty();
        for p in &self.inner {
            if other.inner.contains(p) {
                out.inner.insert(p.clone());
            }
        }
        out
    }

    /// `true` iff `self ⊆ other`.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.inner.is_subset(&other.inner)
    }

    /// `true` iff `self` contains **any** biometric-family principal.
    ///
    /// This is the predicate the telemetry-ring boundary uses to refuse
    /// egress at compile-time. A confidentiality-set restricted to one of
    /// `{BiometricSubject, GazeSubject, FaceSubject, BodySubject}` triggers
    /// `TelemetryRefusal::Biometric*` in `cssl-telemetry`.
    #[must_use]
    pub fn has_biometric_family(&self) -> bool {
        self.inner.iter().any(Principal::is_biometric_family)
    }

    /// `true` iff `self` contains any absolute-egress-banned principal
    /// (biometric-family ∪ SurveillanceTarget ∪ CoercionTarget).
    #[must_use]
    pub fn has_absolutely_banned(&self) -> bool {
        self.inner
            .iter()
            .any(Principal::is_egress_absolutely_banned)
    }
}

impl FromIterator<Principal> for PrincipalSet {
    fn from_iter<I: IntoIterator<Item = Principal>>(iter: I) -> Self {
        let mut s = Self::empty();
        for p in iter {
            s.insert(p);
        }
        s
    }
}

impl fmt::Display for PrincipalSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{")?;
        for (i, p) in self.inner.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            f.write_str(p.as_str())?;
        }
        f.write_str("}")
    }
}

#[cfg(test)]
mod tests {
    use super::{Principal, PrincipalSet};

    #[test]
    fn principal_as_str_canonical() {
        assert_eq!(Principal::User.as_str(), "User");
        assert_eq!(Principal::ApockyRoot.as_str(), "Apocky-Root");
        assert_eq!(Principal::AnthropicAudit.as_str(), "Anthropic-Audit");
        assert_eq!(Principal::BiometricSubject.as_str(), "BiometricSubject");
        assert_eq!(Principal::GazeSubject.as_str(), "GazeSubject");
        assert_eq!(Principal::FaceSubject.as_str(), "FaceSubject");
        assert_eq!(Principal::BodySubject.as_str(), "BodySubject");
        assert_eq!(Principal::Other("Vendor").as_str(), "Vendor");
    }

    #[test]
    fn biometric_family_predicate_is_true_for_four_subjects() {
        assert!(Principal::BiometricSubject.is_biometric_family());
        assert!(Principal::GazeSubject.is_biometric_family());
        assert!(Principal::FaceSubject.is_biometric_family());
        assert!(Principal::BodySubject.is_biometric_family());
    }

    #[test]
    fn biometric_family_predicate_is_false_for_others() {
        assert!(!Principal::User.is_biometric_family());
        assert!(!Principal::Subject.is_biometric_family());
        assert!(!Principal::SurveillanceTarget.is_biometric_family());
        assert!(!Principal::WeaponTarget.is_biometric_family());
        assert!(!Principal::ApockyRoot.is_biometric_family());
    }

    #[test]
    fn egress_absolutely_banned_covers_biometric_plus_surveillance_plus_coercion() {
        for p in [
            Principal::BiometricSubject,
            Principal::GazeSubject,
            Principal::FaceSubject,
            Principal::BodySubject,
            Principal::SurveillanceTarget,
            Principal::CoercionTarget,
        ] {
            assert!(p.is_egress_absolutely_banned(), "{:?} must be banned", p);
        }
        // WeaponTarget is gated by Privilege<Kernel>, NOT absolutely banned.
        assert!(!Principal::WeaponTarget.is_egress_absolutely_banned());
        assert!(!Principal::User.is_egress_absolutely_banned());
        assert!(!Principal::HarmTarget.is_egress_absolutely_banned());
    }

    #[test]
    fn principal_set_empty_and_singleton() {
        let e = PrincipalSet::empty();
        assert!(e.is_empty());
        assert_eq!(e.len(), 0);
        let s = PrincipalSet::singleton(Principal::User);
        assert_eq!(s.len(), 1);
        assert!(s.contains(&Principal::User));
    }

    #[test]
    fn principal_set_insert_remove_idempotent() {
        let mut s = PrincipalSet::empty();
        s.insert(Principal::User);
        s.insert(Principal::User);
        assert_eq!(s.len(), 1);
        assert!(s.remove(&Principal::User));
        assert!(!s.remove(&Principal::User));
        assert!(s.is_empty());
    }

    #[test]
    fn principal_set_union_and_intersection() {
        let a = PrincipalSet::from_iter([Principal::User, Principal::System]);
        let b = PrincipalSet::from_iter([Principal::System, Principal::Kernel]);
        let u = a.union(&b);
        assert_eq!(u.len(), 3);
        assert!(u.contains(&Principal::User));
        assert!(u.contains(&Principal::System));
        assert!(u.contains(&Principal::Kernel));
        let i = a.intersection(&b);
        assert_eq!(i.len(), 1);
        assert!(i.contains(&Principal::System));
    }

    #[test]
    fn principal_set_subset_check() {
        let a = PrincipalSet::from_iter([Principal::User]);
        let b = PrincipalSet::from_iter([Principal::User, Principal::System]);
        assert!(a.is_subset_of(&b));
        assert!(!b.is_subset_of(&a));
        assert!(a.is_subset_of(&a));
    }

    #[test]
    fn principal_set_has_biometric_family_detects() {
        let pure = PrincipalSet::from_iter([Principal::User, Principal::Subject]);
        assert!(!pure.has_biometric_family());
        let with_gaze = PrincipalSet::from_iter([Principal::User, Principal::GazeSubject]);
        assert!(with_gaze.has_biometric_family());
        let with_bio = PrincipalSet::singleton(Principal::BiometricSubject);
        assert!(with_bio.has_biometric_family());
        let with_face = PrincipalSet::singleton(Principal::FaceSubject);
        assert!(with_face.has_biometric_family());
        let with_body = PrincipalSet::singleton(Principal::BodySubject);
        assert!(with_body.has_biometric_family());
    }

    #[test]
    fn principal_set_has_absolutely_banned_includes_biometric_plus_surveillance() {
        let s = PrincipalSet::singleton(Principal::SurveillanceTarget);
        assert!(s.has_absolutely_banned());
        let s2 = PrincipalSet::singleton(Principal::BiometricSubject);
        assert!(s2.has_absolutely_banned());
        let benign = PrincipalSet::from_iter([Principal::User, Principal::Subject]);
        assert!(!benign.has_absolutely_banned());
    }

    #[test]
    fn principal_set_display_is_braced_csv() {
        let s = PrincipalSet::from_iter([Principal::User, Principal::Subject]);
        let disp = format!("{}", s);
        // BTreeSet-order : Subject < User alphabetically? Actually ord is enum-discriminant order.
        // We just check that braces + at-least-one-name + comma exists.
        assert!(disp.starts_with('{') && disp.ends_with('}'));
        assert!(disp.contains(','));
    }
}
