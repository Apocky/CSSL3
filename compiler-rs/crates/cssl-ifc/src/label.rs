//! DLM label-lattice : `Label = (Confidentiality, Integrity)`.
//!
//! § SPEC : `specs/11_IFC.csl` § LABEL ALGEBRA.
//!
//! § ALGEBRA
//!   confidentiality-label C : PrincipalSet — who-can-read
//!   integrity-label       I : PrincipalSet — who-can-influence
//!   combined              L = (C, I)
//!   lattice ⊑ : `L1 ⊑ L2  ≡  C1 ⊇ C2 ∧ I1 ⊆ I2`
//!     (more-confidential = stricter-reader-set)
//!     (more-integral     = tighter-influencer-set)
//!   join L1 ⊔ L2 ≡ (C1 ∩ C2, I1 ∪ I2)  — upper-bound
//!   meet L1 ⊓ L2 ≡ (C1 ∪ C2, I1 ∩ I2)  — lower-bound
//!   top   ⊤ = (∅, All)   — nobody-reads, everyone-influences
//!   bottom ⊥ = (All, ∅)  — everyone-reads, nobody-influences
//!
//! § PROPAGATION RULE
//!   For any operator with inputs of labels `L_i`, the output label is the
//!   join (`⊔`) of the input-labels. This guarantees the soundness of the
//!   non-interference theorem (`specs/11` § NON-INTERFERENCE).
//!
//! § BIOMETRIC-AWARENESS (T11-D132)
//!   `Label::has_biometric_confidentiality()` returns `true` iff the
//!   confidentiality-set contains any biometric-family principal. The
//!   telemetry-ring boundary refuses any value with this property AT
//!   COMPILE-TIME — no `Privilege<*>` capability can grant egress.

use core::fmt;

use crate::principal::{Principal, PrincipalSet};

/// Confidentiality-label : the set of principals permitted to **read** the
/// labeled value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Confidentiality(pub PrincipalSet);

impl Confidentiality {
    /// Confidentiality where every principal in `readers` may read.
    #[must_use]
    pub fn readers(readers: PrincipalSet) -> Self {
        Self(readers)
    }

    /// `top` ≡ ∅ — nobody-reads (highest confidentiality).
    #[must_use]
    pub fn top() -> Self {
        Self(PrincipalSet::empty())
    }

    /// `true` iff `p` is permitted to read.
    #[must_use]
    pub fn permits_read_by(&self, p: &Principal) -> bool {
        self.0.contains(p)
    }

    /// Reference to the underlying principal-set.
    #[must_use]
    pub const fn principals(&self) -> &PrincipalSet {
        &self.0
    }
}

impl fmt::Display for Confidentiality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", self.0)
    }
}

/// Integrity-label : the set of principals permitted to **influence** the
/// labeled value (cannot be tainted by anything outside this set).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Integrity(pub PrincipalSet);

impl Integrity {
    /// Integrity where every principal in `influencers` may influence.
    #[must_use]
    pub fn influencers(influencers: PrincipalSet) -> Self {
        Self(influencers)
    }

    /// `bottom` ≡ ∅ — nobody-influences (highest integrity).
    #[must_use]
    pub fn bottom() -> Self {
        Self(PrincipalSet::empty())
    }

    /// `true` iff `p` is permitted to influence.
    #[must_use]
    pub fn permits_influence_by(&self, p: &Principal) -> bool {
        self.0.contains(p)
    }

    /// Reference to the underlying principal-set.
    #[must_use]
    pub const fn principals(&self) -> &PrincipalSet {
        &self.0
    }
}

impl fmt::Display for Integrity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "I{}", self.0)
    }
}

/// Combined IFC label : `(Confidentiality, Integrity)`.
///
/// All CSSLv3 SSA values carry a `Label` per `specs/11` § TYPE-LEVEL LABELS.
/// Operators propagate labels via `Label::join`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Label {
    /// Who-can-read (confidentiality).
    pub confidentiality: Confidentiality,
    /// Who-can-influence (integrity).
    pub integrity: Integrity,
}

impl Label {
    /// Build a label from explicit confidentiality + integrity components.
    #[must_use]
    pub const fn new(confidentiality: Confidentiality, integrity: Integrity) -> Self {
        Self {
            confidentiality,
            integrity,
        }
    }

    /// `top` (`⊤`) : `(∅, All_implicit)` — nobody-reads, everyone-influences.
    /// Concrete `All` is impractical (universe is open), so we represent
    /// `All` as the *empty* integrity-set with the convention that empty-
    /// integrity in a top-context means "no restriction on influence". Use
    /// `bottom()` + explicit principals for non-trivial labels.
    #[must_use]
    pub fn top() -> Self {
        Self {
            confidentiality: Confidentiality::top(),
            integrity: Integrity::bottom(),
        }
    }

    /// `bottom` (`⊥`) : `(All_implicit, ∅)` — everyone-reads, nobody-
    /// influences. As above, `All_implicit` is conventional.
    #[must_use]
    pub fn bottom() -> Self {
        Self {
            confidentiality: Confidentiality::top(),
            integrity: Integrity::bottom(),
        }
    }

    /// Build a label restricted to readers `c_set` and influencers `i_set`.
    #[must_use]
    pub fn restricted(c_set: PrincipalSet, i_set: PrincipalSet) -> Self {
        Self {
            confidentiality: Confidentiality(c_set),
            integrity: Integrity(i_set),
        }
    }

    /// Lattice join (`⊔`) : `L1 ⊔ L2 = (C1 ∩ C2, I1 ∪ I2)`.
    ///
    /// This is the **propagation rule** for operators : the output of any
    /// operator on labeled inputs carries the join of the input-labels.
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        Self {
            confidentiality: Confidentiality(
                self.confidentiality
                    .0
                    .intersection(&other.confidentiality.0),
            ),
            integrity: Integrity(self.integrity.0.union(&other.integrity.0)),
        }
    }

    /// Lattice meet (`⊓`) : `L1 ⊓ L2 = (C1 ∪ C2, I1 ∩ I2)`.
    #[must_use]
    pub fn meet(&self, other: &Self) -> Self {
        Self {
            confidentiality: Confidentiality(self.confidentiality.0.union(&other.confidentiality.0)),
            integrity: Integrity(self.integrity.0.intersection(&other.integrity.0)),
        }
    }

    /// Lattice partial-order : `self ⊑ other` iff
    /// `self.C ⊇ other.C ∧ self.I ⊆ other.I`.
    #[must_use]
    pub fn flows_to(&self, other: &Self) -> bool {
        // self.C ⊇ other.C : every reader of `other` is also a reader of `self`.
        let c_ok = other.confidentiality.0.is_subset_of(&self.confidentiality.0);
        // self.I ⊆ other.I : every influencer of `self` is also an influencer of `other`.
        let i_ok = self.integrity.0.is_subset_of(&other.integrity.0);
        c_ok && i_ok
    }

    /// `true` iff this label's confidentiality set contains any biometric-
    /// family principal (`BiometricSubject` ∪ `GazeSubject` ∪ `FaceSubject`
    /// ∪ `BodySubject`). Used by the telemetry-ring boundary in
    /// `cssl-telemetry` to refuse egress at compile-time.
    #[must_use]
    pub fn has_biometric_confidentiality(&self) -> bool {
        self.confidentiality.0.has_biometric_family()
    }

    /// `true` iff this label's confidentiality set contains any
    /// absolute-egress-banned principal (biometric-family ∪
    /// SurveillanceTarget ∪ CoercionTarget).
    #[must_use]
    pub fn has_absolutely_banned_confidentiality(&self) -> bool {
        self.confidentiality.0.has_absolutely_banned()
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.confidentiality, self.integrity)
    }
}

#[cfg(test)]
mod tests {
    use super::{Confidentiality, Integrity, Label};
    use crate::principal::{Principal, PrincipalSet};

    fn p_user() -> PrincipalSet {
        PrincipalSet::singleton(Principal::User)
    }

    fn p_subject() -> PrincipalSet {
        PrincipalSet::singleton(Principal::Subject)
    }

    fn p_user_subject() -> PrincipalSet {
        PrincipalSet::from_iter([Principal::User, Principal::Subject])
    }

    fn p_gaze() -> PrincipalSet {
        PrincipalSet::singleton(Principal::GazeSubject)
    }

    fn p_biometric() -> PrincipalSet {
        PrincipalSet::singleton(Principal::BiometricSubject)
    }

    fn p_face() -> PrincipalSet {
        PrincipalSet::singleton(Principal::FaceSubject)
    }

    fn p_body() -> PrincipalSet {
        PrincipalSet::singleton(Principal::BodySubject)
    }

    #[test]
    fn confidentiality_permits_read() {
        let c = Confidentiality::readers(p_user());
        assert!(c.permits_read_by(&Principal::User));
        assert!(!c.permits_read_by(&Principal::Subject));
    }

    #[test]
    fn confidentiality_top_is_empty() {
        let c = Confidentiality::top();
        assert_eq!(c.0.len(), 0);
        assert!(!c.permits_read_by(&Principal::User));
    }

    #[test]
    fn integrity_permits_influence() {
        let i = Integrity::influencers(p_user());
        assert!(i.permits_influence_by(&Principal::User));
        assert!(!i.permits_influence_by(&Principal::System));
    }

    #[test]
    fn integrity_bottom_is_empty() {
        let i = Integrity::bottom();
        assert_eq!(i.0.len(), 0);
    }

    #[test]
    fn label_restricted_carries_components() {
        let l = Label::restricted(p_user(), p_user());
        assert!(l.confidentiality.permits_read_by(&Principal::User));
        assert!(l.integrity.permits_influence_by(&Principal::User));
    }

    #[test]
    fn label_join_intersects_confid_unions_integ() {
        let l1 = Label::restricted(p_user_subject(), p_user());
        let l2 = Label::restricted(p_subject(), p_subject());
        let j = l1.join(&l2);
        // C1 ∩ C2 = {Subject}
        assert_eq!(j.confidentiality.0.len(), 1);
        assert!(j.confidentiality.permits_read_by(&Principal::Subject));
        // I1 ∪ I2 = {User, Subject}
        assert_eq!(j.integrity.0.len(), 2);
        assert!(j.integrity.permits_influence_by(&Principal::User));
        assert!(j.integrity.permits_influence_by(&Principal::Subject));
    }

    #[test]
    fn label_meet_unions_confid_intersects_integ() {
        let l1 = Label::restricted(p_user(), p_user_subject());
        let l2 = Label::restricted(p_subject(), p_subject());
        let m = l1.meet(&l2);
        // C1 ∪ C2 = {User, Subject}
        assert_eq!(m.confidentiality.0.len(), 2);
        // I1 ∩ I2 = {Subject}
        assert_eq!(m.integrity.0.len(), 1);
    }

    #[test]
    fn label_flows_to_lattice_check() {
        // L_high : confid={Subject}, integ={Subject}
        // L_low  : confid={User, Subject}, integ={Subject}
        // high ⊑ low? high.C={Subject} ⊇ low.C={User,Subject}? FALSE → ¬-flows
        // low  ⊑ high? low.C={User,Subject} ⊇ high.C={Subject}? TRUE → check integ.
        //   low.I={Subject} ⊆ high.I={Subject}? TRUE → flows.
        let l_high = Label::restricted(p_subject(), p_subject());
        let l_low = Label::restricted(p_user_subject(), p_subject());
        assert!(l_low.flows_to(&l_high));
        assert!(!l_high.flows_to(&l_low));
    }

    #[test]
    fn label_flows_to_reflexive() {
        let l = Label::restricted(p_user(), p_user());
        assert!(l.flows_to(&l));
    }

    #[test]
    fn has_biometric_confidentiality_detects_gaze() {
        let l = Label::restricted(p_gaze(), p_user());
        assert!(l.has_biometric_confidentiality());
    }

    #[test]
    fn has_biometric_confidentiality_detects_biometric_subject() {
        let l = Label::restricted(p_biometric(), p_user());
        assert!(l.has_biometric_confidentiality());
    }

    #[test]
    fn has_biometric_confidentiality_detects_face() {
        let l = Label::restricted(p_face(), p_user());
        assert!(l.has_biometric_confidentiality());
    }

    #[test]
    fn has_biometric_confidentiality_detects_body() {
        let l = Label::restricted(p_body(), p_user());
        assert!(l.has_biometric_confidentiality());
    }

    #[test]
    fn has_biometric_confidentiality_false_for_pure_user() {
        let l = Label::restricted(p_user(), p_user());
        assert!(!l.has_biometric_confidentiality());
    }

    #[test]
    fn has_absolutely_banned_includes_surveillance_target() {
        let l = Label::restricted(
            PrincipalSet::singleton(Principal::SurveillanceTarget),
            p_user(),
        );
        assert!(l.has_absolutely_banned_confidentiality());
        // Biometric also counts.
        assert!(Label::restricted(p_gaze(), p_user())
            .has_absolutely_banned_confidentiality());
    }

    #[test]
    fn label_display_includes_both_components() {
        let l = Label::restricted(p_user(), p_user());
        let s = format!("{}", l);
        assert!(s.starts_with('('));
        assert!(s.contains("C{"));
        assert!(s.contains("I{"));
    }
}
