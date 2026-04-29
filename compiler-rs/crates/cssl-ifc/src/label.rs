//! IFC label-lattice with biometric anti-surveillance encoding.
//!
//! § SPEC : `specs/11_IFC.csl` § LABEL-LATTICE + `PRIME_DIRECTIVE.md` § 1
//!   N! surveillance.
//!
//! § DESIGN
//!   The label-lattice is a Cartesian product of two bounded chains:
//!     - [`Confidentiality`] : Public ≤ Internal ≤ Confidential ≤ TopSecret
//!     - [`Integrity`]      : Untrusted ≤ Trusted ≤ Root
//!   The lattice supremum (⊔) is the component-wise max ; infimum (⊓) is
//!   component-wise min. The bottom is `(Public, Untrusted)` ; the top is
//!   `(TopSecret, Root)`.
//!
//! § BIOMETRIC LABELS (T11-D129)
//!   Biometric / gaze / face-tracking / body-tracking data is BORN at the label
//!   `(Confidential, Root)` with an additional [`BiometricKind`] tag. The tag is
//!   what makes declassification REFUSED regardless of `Privilege<L>` :
//!     - [`Privilege<L>::declassify_allowed`] returns `false` for any
//!       biometric-tagged label.
//!     - [`Secret<T,L>`] declassification through `Privilege<Kernel>` works for
//!       non-biometric Confidential data (with audit) but is structurally
//!       impossible for biometric data — the only way to materialize a non-
//!       Secret-wrapped biometric value is to consume it on-device via an
//!       `OnDeviceOnly` handler that NEVER returns the raw bits to the caller.

use core::fmt;

use thiserror::Error;

// ─ Confidentiality chain ────────────────────────────────────────────────────

/// Confidentiality axis of the IFC label lattice. Lower is less-restrictive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Confidentiality {
    /// Public — anyone may read.
    Public = 0,
    /// Internal — within the device-OS boundary.
    Internal = 1,
    /// Confidential — biometric / personal-data tier.
    Confidential = 2,
    /// Top-secret — root-of-trust keys, identity-token material.
    TopSecret = 3,
}

impl Confidentiality {
    /// All variants in canonical (low-to-high) order.
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [
            Self::Public,
            Self::Internal,
            Self::Confidential,
            Self::TopSecret,
        ]
    }

    /// The canonical short name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Public => "Public",
            Self::Internal => "Internal",
            Self::Confidential => "Confidential",
            Self::TopSecret => "TopSecret",
        }
    }

    /// Lattice join (⊔) — pick the more-restrictive of the two.
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        if (self as u8) >= (other as u8) {
            self
        } else {
            other
        }
    }

    /// Lattice meet (⊓) — pick the less-restrictive of the two.
    #[must_use]
    pub const fn meet(self, other: Self) -> Self {
        if (self as u8) <= (other as u8) {
            self
        } else {
            other
        }
    }

    /// `true` iff `self ⊑ other` (i.e., self is no more restrictive than other).
    #[must_use]
    pub const fn flows_to(self, other: Self) -> bool {
        (self as u8) <= (other as u8)
    }
}

impl fmt::Display for Confidentiality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ─ Integrity chain ──────────────────────────────────────────────────────────

/// Integrity axis of the IFC label lattice. Higher is more-trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Integrity {
    /// Untrusted — externally-supplied data, network input, user-typed text.
    Untrusted = 0,
    /// Trusted — vetted by the runtime ; e.g., output of a verified handler.
    Trusted = 1,
    /// Root — root-of-trust ; e.g., the PRIME-DIRECTIVE attestation channel,
    /// biometric-sensor-driver bytes that have not yet been redacted.
    Root = 2,
}

impl Integrity {
    /// All variants in canonical order.
    #[must_use]
    pub const fn all() -> [Self; 3] {
        [Self::Untrusted, Self::Trusted, Self::Root]
    }

    /// Canonical short name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Untrusted => "Untrusted",
            Self::Trusted => "Trusted",
            Self::Root => "Root",
        }
    }

    /// Integrity join — for biometric data we want the JOIN of two integrity
    /// labels to be the LOWER of the two (you can never trust the result of
    /// combining a Root datum with an Untrusted one). This is the dual of
    /// confidentiality. Encode as: `join(a, b) = min(a, b)`.
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        if (self as u8) <= (other as u8) {
            self
        } else {
            other
        }
    }

    /// Integrity meet — pick the higher (more-trusted) of the two.
    #[must_use]
    pub const fn meet(self, other: Self) -> Self {
        if (self as u8) >= (other as u8) {
            self
        } else {
            other
        }
    }

    /// Lattice flow — `self ⊑ other` iff `self` is no MORE-trusted than `other`
    /// (i.e., `self` can flow into a sink that requires at most `other` integrity).
    #[must_use]
    pub const fn flows_to(self, other: Self) -> bool {
        (self as u8) >= (other as u8)
    }
}

impl fmt::Display for Integrity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ─ Biometric kind ───────────────────────────────────────────────────────────

/// Biometric / sensitive-modality tag attached to a label. Presence of any
/// non-`None` variant makes the label NON-DECLASSIFIABLE — `Privilege<L>` for
/// any level CANNOT override (T11-D129).
///
/// § SPEC : `PRIME_DIRECTIVE.md` § 1 N! surveillance + P18 BiometricEgress.
/// § VR-SPEC : `Omniverse/07_AESTHETIC/05_VR_RENDERING.csl` (raw-gaze NEVER-
///   egress) + `Omniverse/08_BODY/02_VR_EMBODIMENT.csl` (Σ-mask body-region).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BiometricKind {
    /// No biometric tag — ordinary IFC label.
    None,
    /// Eye-tracking / gaze data. Per `05_VR_RENDERING` raw-gaze MUST never leave
    /// the device. Foveation routing uses `Sigma-mask` quantized output, never
    /// raw bits.
    Gaze,
    /// General biometric (heart-rate, EDA, breath, blood-flow, gait-DNA).
    Biometric,
    /// Face-tracking — facial-action-coding-system (FACS) coefficients,
    /// expression-shape vectors.
    FaceTracking,
    /// Body-tracking — joint poses, hand-pose, skeletal data. Σ-mask body-
    /// region defaults from `Omniverse/08_BODY/02_VR_EMBODIMENT.csl`.
    BodyTracking,
}

impl BiometricKind {
    /// `true` iff this tag is biometric (anything except `None`).
    #[must_use]
    pub const fn is_biometric(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Canonical short name (matches `Sensitive<"name">` domain literal).
    #[must_use]
    pub const fn domain_name(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Gaze => "gaze",
            Self::Biometric => "biometric",
            Self::FaceTracking => "face-tracking",
            Self::BodyTracking => "body-tracking",
        }
    }

    /// All biometric variants (excluding `None`).
    #[must_use]
    pub const fn all_biometric() -> [Self; 4] {
        [
            Self::Gaze,
            Self::Biometric,
            Self::FaceTracking,
            Self::BodyTracking,
        ]
    }
}

impl fmt::Display for BiometricKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if matches!(self, Self::None) {
            f.write_str("∅")
        } else {
            f.write_str(self.domain_name())
        }
    }
}

// ─ Privilege level ──────────────────────────────────────────────────────────

/// Privilege level for declassification operations. Higher level = more authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrivilegeLevel {
    /// User-space privilege (default).
    User = 0,
    /// Driver / handler privilege.
    Driver = 1,
    /// Kernel privilege — highest non-Root tier.
    Kernel = 2,
    /// Apocky-Root attestation — for spec-internal verification only ; CANNOT
    /// override §1 prohibitions per § 7 INTEGRITY.
    ApockyRoot = 3,
}

impl PrivilegeLevel {
    /// Canonical name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Driver => "Driver",
            Self::Kernel => "Kernel",
            Self::ApockyRoot => "ApockyRoot",
        }
    }

    /// `true` iff `self ≥ other` in privilege ordering.
    #[must_use]
    pub const fn dominates(self, other: Self) -> bool {
        (self as u8) >= (other as u8)
    }

    /// All four levels in low-to-high order.
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [Self::User, Self::Driver, Self::Kernel, Self::ApockyRoot]
    }
}

impl fmt::Display for PrivilegeLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ─ Composite label ──────────────────────────────────────────────────────────

/// A composite IFC label : `(confidentiality, integrity, biometric-kind)`.
///
/// § ORDERING
///   Lattice ordering is component-wise on (confidentiality, integrity). The
///   biometric-kind is NOT lattice-ordered — it is a tag : if either side has
///   a biometric tag, the join carries that tag. Combining two different
///   biometric tags ESCALATES to `Biometric` (the most-general bucket).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IfcLabel {
    /// Confidentiality component.
    pub confidentiality: Confidentiality,
    /// Integrity component.
    pub integrity: Integrity,
    /// Biometric tag ; presence makes the label NON-DECLASSIFIABLE.
    pub biometric: BiometricKind,
}

impl IfcLabel {
    /// The canonical bottom — `(Public, Untrusted, None)`.
    pub const BOTTOM: Self = Self {
        confidentiality: Confidentiality::Public,
        integrity: Integrity::Untrusted,
        biometric: BiometricKind::None,
    };

    /// The canonical top — `(TopSecret, Root, None)`. Note this is the top of
    /// the NON-biometric sublattice ; biometric labels live in a parallel
    /// sub-lattice with their own bottom + top.
    pub const TOP: Self = Self {
        confidentiality: Confidentiality::TopSecret,
        integrity: Integrity::Root,
        biometric: BiometricKind::None,
    };

    /// Construct an `IfcLabel`.
    #[must_use]
    pub const fn new(c: Confidentiality, i: Integrity, b: BiometricKind) -> Self {
        Self {
            confidentiality: c,
            integrity: i,
            biometric: b,
        }
    }

    /// Construct a public-untrusted label.
    #[must_use]
    pub const fn public() -> Self {
        Self::BOTTOM
    }

    /// Construct a confidential-trusted label (e.g., for ordinary user data).
    #[must_use]
    pub const fn confidential_trusted() -> Self {
        Self::new(
            Confidentiality::Confidential,
            Integrity::Trusted,
            BiometricKind::None,
        )
    }

    /// Construct the canonical biometric label : `(Confidential, Root, kind)`.
    /// Per T11-D129 / §1 N! surveillance, this is BORN at the highest label
    /// and has NO declassification path.
    #[must_use]
    pub const fn biometric(kind: BiometricKind) -> Self {
        Self::new(Confidentiality::Confidential, Integrity::Root, kind)
    }

    /// Canonical gaze label per `05_VR_RENDERING.csl` raw-gaze NEVER-egress.
    #[must_use]
    pub const fn gaze() -> Self {
        Self::biometric(BiometricKind::Gaze)
    }

    /// Canonical face-tracking label.
    #[must_use]
    pub const fn face_tracking() -> Self {
        Self::biometric(BiometricKind::FaceTracking)
    }

    /// Canonical body-tracking label.
    #[must_use]
    pub const fn body_tracking() -> Self {
        Self::biometric(BiometricKind::BodyTracking)
    }

    /// `true` iff this label carries a biometric tag.
    #[must_use]
    pub const fn is_biometric(&self) -> bool {
        self.biometric.is_biometric()
    }

    /// Lattice join — combine two labels safely.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        let bio = match (self.biometric, other.biometric) {
            (BiometricKind::None, b) | (b, BiometricKind::None) => b,
            (a, b) if a == b => a,
            // mixing two distinct biometric kinds escalates to the umbrella
            (_, _) => BiometricKind::Biometric,
        };
        Self {
            confidentiality: self.confidentiality.join(other.confidentiality),
            integrity: self.integrity.join(other.integrity),
            biometric: bio,
        }
    }

    /// `true` iff `self ⊑ other` in the IFC flow lattice (standard DLM
    /// semantics : data labeled `self` may flow into a sink labeled `other`).
    ///
    /// - Confidentiality flows UPWARD : `self.c ≤ other.c` (low-secret → high-
    ///   secret OK ; never the reverse without declass).
    /// - Integrity flows DOWNWARD : `self.i ≥ other.i` (high-trust → low-trust
    ///   OK ; you can degrade trust by mixing with untrusted input ; never
    ///   raise without endorsement).
    /// - Biometric tag : a biometric label NEVER flows to a non-biometric
    ///   sink (T11-D129).
    #[must_use]
    pub fn flows_to(self, other: Self) -> bool {
        if !self.confidentiality.flows_to(other.confidentiality) {
            return false;
        }
        if !self.integrity.flows_to(other.integrity) {
            return false;
        }
        if self.biometric.is_biometric() && !other.biometric.is_biometric() {
            return false;
        }
        true
    }

    /// Check that a declassification from `self` to `target` under `priv_lvl`
    /// is permitted. T11-D129 invariant : biometric labels REFUSE for ALL
    /// privilege levels.
    ///
    /// § DECLASSIFICATION SEMANTICS
    ///   Declassification is a confidentiality-only downgrade. Integrity is
    ///   PRESERVED across declass (changes to integrity are "endorsement", a
    ///   separate operation not covered by this check). The check therefore
    ///   requires :
    ///     - `target.confidentiality < self.confidentiality` (strict drop)
    ///     - `target.integrity == self.integrity` (no integrity change)
    ///     - `target.biometric == self.biometric` (no biometric tag change ;
    ///       biometric source ALWAYS refused per §1)
    ///     - caller's privilege ≥ required-tier (Driver / Kernel / ApockyRoot
    ///       depending on how many confidentiality tiers are dropped).
    ///
    /// # Errors
    /// - [`DeclassifyError::BiometricRefused`] for any biometric-tagged source.
    /// - [`DeclassifyError::InsufficientPrivilege`] if the caller's privilege
    ///   level is below the required tier.
    /// - [`DeclassifyError::NotDownward`] if `target` is not strictly weaker
    ///   than `self` on confidentiality, or differs on integrity / biometric.
    pub fn declassify_check(
        self,
        target: IfcLabel,
        priv_lvl: PrivilegeLevel,
    ) -> Result<(), DeclassifyError> {
        // T11-D129 absolute refusal : biometric data has NO declassification
        // path regardless of privilege.
        if self.biometric.is_biometric() {
            return Err(DeclassifyError::BiometricRefused {
                kind: self.biometric,
                privilege_attempted: priv_lvl,
            });
        }
        // integrity preserved + biometric preserved + confidentiality strictly
        // dropped.
        let downward = (target.confidentiality as u8) < (self.confidentiality as u8)
            && target.integrity == self.integrity
            && target.biometric == self.biometric;
        if !downward {
            return Err(DeclassifyError::NotDownward {
                from: self,
                to: target,
            });
        }
        let needed = required_privilege(self, target);
        if !priv_lvl.dominates(needed) {
            return Err(DeclassifyError::InsufficientPrivilege {
                required: needed,
                provided: priv_lvl,
            });
        }
        Ok(())
    }
}

/// What privilege is needed to declassify `from → to` (non-biometric only).
fn required_privilege(from: IfcLabel, to: IfcLabel) -> PrivilegeLevel {
    let drop = (from.confidentiality as u8).saturating_sub(to.confidentiality as u8);
    match drop {
        0 => PrivilegeLevel::User,       // shouldn't reach (downward check failed)
        1 => PrivilegeLevel::Driver,     // one tier
        2 => PrivilegeLevel::Kernel,     // two tiers (Confidential → Public)
        _ => PrivilegeLevel::ApockyRoot, // three tiers (TopSecret → Public)
    }
}

impl fmt::Display for IfcLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.biometric.is_biometric() {
            write!(
                f,
                "({}, {}, biometric=<{}>)",
                self.confidentiality, self.integrity, self.biometric
            )
        } else {
            write!(f, "({}, {})", self.confidentiality, self.integrity)
        }
    }
}

// ─ DeclassifyError ──────────────────────────────────────────────────────────

/// Failure modes for declassification. Variants align with diagnostic codes
/// `IFC0001` (NotDownward), `IFC0002` (InsufficientPrivilege),
/// `IFC0003` (BiometricRefused).
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DeclassifyError {
    /// `[IFC0003]` Biometric label declassification refused regardless of
    /// privilege. PRIME-DIRECTIVE §1 N! surveillance + P18 BiometricEgress.
    #[error(
        "[IFC0003] declassification of biometric label `{kind}` REFUSED regardless of \
         privilege ({privilege_attempted}) — PRIME-DIRECTIVE §1 N! surveillance + P18 \
         BiometricEgress (no override exists)"
    )]
    BiometricRefused {
        /// Which biometric kind was on the source label.
        kind: BiometricKind,
        /// Privilege level the caller attempted (recorded for the audit-chain).
        privilege_attempted: PrivilegeLevel,
    },
    /// `[IFC0002]` Caller's privilege level is insufficient for the requested
    /// declassification.
    #[error(
        "[IFC0002] insufficient privilege for declassification : required {required}, \
         provided {provided}"
    )]
    InsufficientPrivilege {
        /// Required minimum privilege level.
        required: PrivilegeLevel,
        /// Privilege level the caller actually held.
        provided: PrivilegeLevel,
    },
    /// `[IFC0001]` Target label is not strictly weaker than the source — the
    /// proposed declassification would not be downward.
    #[error("[IFC0001] declassification target {to} is not strictly weaker than {from}")]
    NotDownward {
        /// Source label.
        from: IfcLabel,
        /// Proposed (non-downward) target.
        to: IfcLabel,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        BiometricKind, Confidentiality, DeclassifyError, IfcLabel, Integrity, PrivilegeLevel,
    };

    // ─ confidentiality chain ──────────────────────────────────────────────

    #[test]
    fn confidentiality_chain_ordering() {
        assert!(Confidentiality::Public < Confidentiality::Internal);
        assert!(Confidentiality::Internal < Confidentiality::Confidential);
        assert!(Confidentiality::Confidential < Confidentiality::TopSecret);
    }

    #[test]
    fn confidentiality_join_picks_max() {
        assert_eq!(
            Confidentiality::Public.join(Confidentiality::Confidential),
            Confidentiality::Confidential
        );
        assert_eq!(
            Confidentiality::TopSecret.join(Confidentiality::Internal),
            Confidentiality::TopSecret
        );
    }

    #[test]
    fn confidentiality_meet_picks_min() {
        assert_eq!(
            Confidentiality::Public.meet(Confidentiality::Confidential),
            Confidentiality::Public
        );
    }

    #[test]
    fn confidentiality_flows_to_holds_for_lower_levels() {
        assert!(Confidentiality::Public.flows_to(Confidentiality::Confidential));
        assert!(!Confidentiality::Confidential.flows_to(Confidentiality::Public));
    }

    // ─ integrity chain ────────────────────────────────────────────────────

    #[test]
    fn integrity_join_picks_lower_trust() {
        // Untrusted joined with Root yields Untrusted (more conservative)
        assert_eq!(
            Integrity::Untrusted.join(Integrity::Root),
            Integrity::Untrusted
        );
    }

    #[test]
    fn integrity_meet_picks_higher_trust() {
        assert_eq!(Integrity::Untrusted.meet(Integrity::Root), Integrity::Root);
    }

    #[test]
    fn integrity_root_flows_to_trusted_but_not_reverse() {
        assert!(Integrity::Root.flows_to(Integrity::Trusted));
        assert!(!Integrity::Untrusted.flows_to(Integrity::Trusted));
    }

    // ─ biometric kinds ────────────────────────────────────────────────────

    #[test]
    fn biometric_kinds_are_distinct() {
        let all = BiometricKind::all_biometric();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn biometric_none_is_not_biometric() {
        assert!(!BiometricKind::None.is_biometric());
    }

    #[test]
    fn all_four_biometric_kinds_recognized_as_biometric() {
        for k in BiometricKind::all_biometric() {
            assert!(k.is_biometric(), "{k:?} should be biometric");
        }
    }

    #[test]
    fn biometric_domain_names_match_spec() {
        assert_eq!(BiometricKind::Gaze.domain_name(), "gaze");
        assert_eq!(BiometricKind::Biometric.domain_name(), "biometric");
        assert_eq!(BiometricKind::FaceTracking.domain_name(), "face-tracking");
        assert_eq!(BiometricKind::BodyTracking.domain_name(), "body-tracking");
    }

    // ─ privilege levels ───────────────────────────────────────────────────

    #[test]
    fn privilege_dominance_is_reflexive_and_monotone() {
        for p in PrivilegeLevel::all() {
            assert!(p.dominates(p));
        }
        assert!(PrivilegeLevel::Kernel.dominates(PrivilegeLevel::Driver));
        assert!(PrivilegeLevel::ApockyRoot.dominates(PrivilegeLevel::Kernel));
        assert!(!PrivilegeLevel::User.dominates(PrivilegeLevel::Driver));
    }

    // ─ IfcLabel constructors ──────────────────────────────────────────────

    #[test]
    fn bottom_is_public_untrusted_none() {
        let b = IfcLabel::BOTTOM;
        assert_eq!(b.confidentiality, Confidentiality::Public);
        assert_eq!(b.integrity, Integrity::Untrusted);
        assert!(!b.is_biometric());
    }

    #[test]
    fn biometric_label_is_confidential_root_with_kind() {
        let l = IfcLabel::biometric(BiometricKind::Gaze);
        assert_eq!(l.confidentiality, Confidentiality::Confidential);
        assert_eq!(l.integrity, Integrity::Root);
        assert!(l.is_biometric());
    }

    #[test]
    fn gaze_label_constructor_matches_spec() {
        assert_eq!(IfcLabel::gaze(), IfcLabel::biometric(BiometricKind::Gaze));
    }

    #[test]
    fn face_and_body_label_constructors_match_spec() {
        assert_eq!(
            IfcLabel::face_tracking(),
            IfcLabel::biometric(BiometricKind::FaceTracking)
        );
        assert_eq!(
            IfcLabel::body_tracking(),
            IfcLabel::biometric(BiometricKind::BodyTracking)
        );
    }

    // ─ flow / join ────────────────────────────────────────────────────────

    #[test]
    fn join_propagates_biometric_kind() {
        let l1 = IfcLabel::public();
        let l2 = IfcLabel::biometric(BiometricKind::Gaze);
        let j = l1.join(l2);
        assert_eq!(j.biometric, BiometricKind::Gaze);
    }

    #[test]
    fn join_two_distinct_biometrics_escalates_to_biometric_umbrella() {
        let l1 = IfcLabel::biometric(BiometricKind::Gaze);
        let l2 = IfcLabel::biometric(BiometricKind::FaceTracking);
        let j = l1.join(l2);
        assert_eq!(j.biometric, BiometricKind::Biometric);
    }

    #[test]
    fn join_same_biometric_kept() {
        let l1 = IfcLabel::biometric(BiometricKind::Gaze);
        let l2 = IfcLabel::biometric(BiometricKind::Gaze);
        assert_eq!(l1.join(l2).biometric, BiometricKind::Gaze);
    }

    #[test]
    fn biometric_does_not_flow_to_non_biometric() {
        let bio = IfcLabel::biometric(BiometricKind::Biometric);
        let nonbio = IfcLabel::confidential_trusted();
        assert!(!bio.flows_to(nonbio));
    }

    #[test]
    fn same_integrity_public_flows_to_confidential() {
        // Public-Untrusted should flow into Confidential-Untrusted (lower
        // confidentiality + same integrity).
        let from = IfcLabel::new(
            Confidentiality::Public,
            Integrity::Untrusted,
            BiometricKind::None,
        );
        let to = IfcLabel::new(
            Confidentiality::Confidential,
            Integrity::Untrusted,
            BiometricKind::None,
        );
        assert!(from.flows_to(to));
    }

    #[test]
    fn untrusted_does_not_flow_to_trusted_sink() {
        // Standard DLM : integrity flows downward (high-trust → low-trust OK).
        // An Untrusted source CANNOT flow into a Trusted sink without
        // endorsement.
        assert!(!IfcLabel::public().flows_to(IfcLabel::confidential_trusted()));
    }

    // ─ declassify_check ───────────────────────────────────────────────────

    #[test]
    fn declassify_biometric_refused_for_user() {
        let from = IfcLabel::gaze();
        let to = IfcLabel::public();
        let err = from
            .declassify_check(to, PrivilegeLevel::User)
            .expect_err("must refuse");
        assert!(matches!(err, DeclassifyError::BiometricRefused { .. }));
    }

    #[test]
    fn declassify_biometric_refused_for_driver() {
        let from = IfcLabel::biometric(BiometricKind::FaceTracking);
        let to = IfcLabel::public();
        assert!(matches!(
            from.declassify_check(to, PrivilegeLevel::Driver),
            Err(DeclassifyError::BiometricRefused { .. })
        ));
    }

    #[test]
    fn declassify_biometric_refused_for_kernel() {
        let from = IfcLabel::biometric(BiometricKind::BodyTracking);
        let to = IfcLabel::public();
        assert!(matches!(
            from.declassify_check(to, PrivilegeLevel::Kernel),
            Err(DeclassifyError::BiometricRefused { .. })
        ));
    }

    #[test]
    fn declassify_biometric_refused_for_apocky_root() {
        // Even the highest privilege level cannot declassify biometric data.
        let from = IfcLabel::biometric(BiometricKind::Biometric);
        let to = IfcLabel::confidential_trusted();
        assert!(matches!(
            from.declassify_check(to, PrivilegeLevel::ApockyRoot),
            Err(DeclassifyError::BiometricRefused { .. })
        ));
    }

    #[test]
    fn declassify_non_biometric_one_tier_drop_needs_driver() {
        let from = IfcLabel::confidential_trusted();
        let to = IfcLabel::new(
            Confidentiality::Internal,
            Integrity::Trusted,
            BiometricKind::None,
        );
        assert!(from.declassify_check(to, PrivilegeLevel::Driver).is_ok());
        assert!(matches!(
            from.declassify_check(to, PrivilegeLevel::User),
            Err(DeclassifyError::InsufficientPrivilege { .. })
        ));
    }

    #[test]
    fn declassify_non_biometric_two_tier_drop_needs_kernel() {
        let from = IfcLabel::confidential_trusted();
        let to = IfcLabel::new(
            Confidentiality::Public,
            Integrity::Trusted,
            BiometricKind::None,
        );
        assert!(from.declassify_check(to, PrivilegeLevel::Kernel).is_ok());
        assert!(matches!(
            from.declassify_check(to, PrivilegeLevel::Driver),
            Err(DeclassifyError::InsufficientPrivilege { .. })
        ));
    }

    #[test]
    fn declassify_top_secret_to_public_needs_apocky_root() {
        // Declass preserves integrity ; both labels carry Root.
        let from = IfcLabel::TOP;
        let to = IfcLabel::new(
            Confidentiality::Public,
            Integrity::Root,
            BiometricKind::None,
        );
        assert!(from
            .declassify_check(to, PrivilegeLevel::ApockyRoot)
            .is_ok());
        assert!(matches!(
            from.declassify_check(to, PrivilegeLevel::Kernel),
            Err(DeclassifyError::InsufficientPrivilege { .. })
        ));
    }

    #[test]
    fn declassify_not_downward_rejected() {
        let from = IfcLabel::public();
        let to = IfcLabel::confidential_trusted();
        assert!(matches!(
            from.declassify_check(to, PrivilegeLevel::ApockyRoot),
            Err(DeclassifyError::NotDownward { .. })
        ));
    }

    #[test]
    fn declassify_self_to_self_rejected() {
        let l = IfcLabel::confidential_trusted();
        assert!(matches!(
            l.declassify_check(l, PrivilegeLevel::ApockyRoot),
            Err(DeclassifyError::NotDownward { .. })
        ));
    }

    #[test]
    fn declassify_error_text_carries_diagnostic_code() {
        let from = IfcLabel::gaze();
        let err = from
            .declassify_check(IfcLabel::public(), PrivilegeLevel::Kernel)
            .unwrap_err();
        let s = err.to_string();
        assert!(s.contains("IFC0003"));
        assert!(s.contains("BiometricEgress") || s.contains("biometric"));
    }

    #[test]
    fn all_four_biometric_kinds_refused_for_all_privileges() {
        for kind in BiometricKind::all_biometric() {
            for priv_lvl in PrivilegeLevel::all() {
                let from = IfcLabel::biometric(kind);
                let res = from.declassify_check(IfcLabel::public(), priv_lvl);
                assert!(
                    matches!(res, Err(DeclassifyError::BiometricRefused { .. })),
                    "kind={kind:?} priv={priv_lvl:?} must refuse"
                );
            }
        }
    }
}
