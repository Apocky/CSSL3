//! `LabeledValue<T>` : an opaque carrier `(value, Label, sensitive_domains)`.
//!
//! § SPEC : `specs/11_IFC.csl` § TYPE-LEVEL LABELS.
//!
//! § DESIGN
//!   `LabeledValue<T>` is the host-side encoding of CSSLv3's `secret<T, L>`
//!   primitive. It pairs an inner value with :
//!     - the IFC `Label` (confidentiality + integrity)
//!     - any `SensitiveDomain` tags carried via `Sensitive<dom>` effects
//!
//!   Operators in compiler-passes propagate these via `LabeledValue::join`
//!   (label-join + domain-set-union) so the propagation rule from `specs/11`
//!   `output-label = ⊔ of input-labels` is preserved structurally.
//!
//!   The biometric-family predicates `is_biometric` + `is_egress_banned`
//!   answer the structural-egress-question in O(small-constant) — they are
//!   the entry-points the telemetry-ring boundary calls to refuse logging
//!   AT COMPILE-TIME.

use std::collections::BTreeSet;

use crate::domain::SensitiveDomain;
use crate::label::Label;

/// A value carrying an IFC label + sensitive-domain tags.
///
/// `T` is the underlying value-type ; the label + domain-set ride alongside.
/// All consumers that wish to perform IFC-aware operations should accept
/// `&LabeledValue<T>` (or `LabeledValue<T>` for moving) instead of bare `T`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LabeledValue<T> {
    /// The wrapped inner value.
    pub value: T,
    /// IFC label (confidentiality + integrity).
    pub label: Label,
    /// Sensitive-domain tags carried via `Sensitive<dom>` effects.
    pub sensitive_domains: BTreeSet<SensitiveDomain>,
}

impl<T> LabeledValue<T> {
    /// Wrap `value` with `label` + an empty domain-set.
    #[must_use]
    pub fn new(value: T, label: Label) -> Self {
        Self {
            value,
            label,
            sensitive_domains: BTreeSet::new(),
        }
    }

    /// Wrap `value` with `label` + a single `SensitiveDomain` tag.
    #[must_use]
    pub fn with_domain(value: T, label: Label, domain: SensitiveDomain) -> Self {
        let mut domains = BTreeSet::new();
        domains.insert(domain);
        Self {
            value,
            label,
            sensitive_domains: domains,
        }
    }

    /// Wrap `value` with `label` + the given domain-set.
    #[must_use]
    pub fn with_domains(value: T, label: Label, domains: BTreeSet<SensitiveDomain>) -> Self {
        Self {
            value,
            label,
            sensitive_domains: domains,
        }
    }

    /// Add a `SensitiveDomain` tag (idempotent).
    pub fn add_domain(&mut self, domain: SensitiveDomain) {
        self.sensitive_domains.insert(domain);
    }

    /// `true` iff any tag is in the biometric-family.
    #[must_use]
    pub fn is_biometric(&self) -> bool {
        self.sensitive_domains
            .iter()
            .any(|d| d.is_biometric_family())
            || self.label.has_biometric_confidentiality()
    }

    /// `true` iff this value is **absolutely-egress-banned** at the
    /// telemetry-ring boundary :
    ///   - any sensitive-domain tag is `is_telemetry_egress_absolutely_banned`
    ///   - OR the label's confidentiality contains an absolute-egress-banned
    ///     principal (biometric-family ∪ SurveillanceTarget ∪ CoercionTarget)
    ///
    /// **Critically**, this predicate is non-overridable : no `Privilege<*>`
    /// capability can change its return-value. The telemetry-ring uses this
    /// to refuse `record_labeled` at compile-time.
    #[must_use]
    pub fn is_egress_banned(&self) -> bool {
        self.sensitive_domains
            .iter()
            .any(|d| d.is_telemetry_egress_absolutely_banned())
            || self.label.has_absolutely_banned_confidentiality()
    }

    /// First biometric-family domain found, if any.
    /// Used by the telemetry-ring boundary to populate the
    /// `BiometricRefused` diagnostic with the specific kind.
    #[must_use]
    pub fn first_biometric_domain(&self) -> Option<SensitiveDomain> {
        self.sensitive_domains
            .iter()
            .copied()
            .find(|d| d.is_biometric_family())
    }

    /// First absolutely-banned domain found, if any.
    #[must_use]
    pub fn first_egress_banned_domain(&self) -> Option<SensitiveDomain> {
        self.sensitive_domains
            .iter()
            .copied()
            .find(|d| d.is_telemetry_egress_absolutely_banned())
    }

    /// Map the inner value through `f`, preserving label + domain-set.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> LabeledValue<U> {
        LabeledValue {
            value: f(self.value),
            label: self.label,
            sensitive_domains: self.sensitive_domains,
        }
    }
}

impl<T: Clone> LabeledValue<T> {
    /// Lift a binary operator over labeled inputs : the output value is
    /// `op(self.value, other.value)`, the output label is the lattice-join
    /// of the inputs', and the output domain-set is the union of the inputs'.
    /// This is the **propagation rule** for binary operators per `specs/11`.
    pub fn join_with<U: Clone, V>(
        &self,
        other: &LabeledValue<U>,
        op: impl FnOnce(&T, &U) -> V,
    ) -> LabeledValue<V> {
        let value = op(&self.value, &other.value);
        let label = self.label.join(&other.label);
        let mut sensitive_domains = self.sensitive_domains.clone();
        for d in &other.sensitive_domains {
            sensitive_domains.insert(*d);
        }
        LabeledValue {
            value,
            label,
            sensitive_domains,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LabeledValue;
    use crate::domain::SensitiveDomain;
    use crate::label::Label;
    use crate::principal::{Principal, PrincipalSet};

    fn user_label() -> Label {
        Label::restricted(
            PrincipalSet::singleton(Principal::User),
            PrincipalSet::singleton(Principal::User),
        )
    }

    #[test]
    fn new_carries_value_and_label() {
        let v: LabeledValue<i32> = LabeledValue::new(42, user_label());
        assert_eq!(v.value, 42);
        assert!(v.sensitive_domains.is_empty());
    }

    #[test]
    fn with_domain_adds_single_tag() {
        let v: LabeledValue<i32> =
            LabeledValue::with_domain(7, user_label(), SensitiveDomain::Privacy);
        assert_eq!(v.sensitive_domains.len(), 1);
        assert!(v.sensitive_domains.contains(&SensitiveDomain::Privacy));
    }

    #[test]
    fn add_domain_idempotent() {
        let mut v: LabeledValue<i32> = LabeledValue::new(1, user_label());
        v.add_domain(SensitiveDomain::Privacy);
        v.add_domain(SensitiveDomain::Privacy);
        assert_eq!(v.sensitive_domains.len(), 1);
    }

    #[test]
    fn is_biometric_detects_gaze_tag() {
        let v: LabeledValue<u64> =
            LabeledValue::with_domain(0xDEAD, user_label(), SensitiveDomain::Gaze);
        assert!(v.is_biometric());
    }

    #[test]
    fn is_biometric_detects_biometric_label_principal() {
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::BiometricSubject),
            PrincipalSet::singleton(Principal::User),
        );
        let v: LabeledValue<u64> = LabeledValue::new(0xBEEF, label);
        assert!(v.is_biometric());
    }

    #[test]
    fn is_biometric_false_for_pure_user_value() {
        let v: LabeledValue<i32> = LabeledValue::new(0, user_label());
        assert!(!v.is_biometric());
    }

    #[test]
    fn is_egress_banned_detects_each_biometric_domain() {
        for d in SensitiveDomain::BIOMETRIC_FAMILY {
            let v: LabeledValue<i32> = LabeledValue::with_domain(0, user_label(), d);
            assert!(v.is_egress_banned(), "{:?}", d);
        }
    }

    #[test]
    fn is_egress_banned_detects_surveillance_and_coercion() {
        let v_surv: LabeledValue<i32> =
            LabeledValue::with_domain(0, user_label(), SensitiveDomain::Surveillance);
        assert!(v_surv.is_egress_banned());
        let v_coer: LabeledValue<i32> =
            LabeledValue::with_domain(0, user_label(), SensitiveDomain::Coercion);
        assert!(v_coer.is_egress_banned());
    }

    #[test]
    fn is_egress_banned_false_for_privacy() {
        let v: LabeledValue<i32> =
            LabeledValue::with_domain(0, user_label(), SensitiveDomain::Privacy);
        assert!(!v.is_egress_banned());
    }

    #[test]
    fn first_biometric_domain_returns_canonical() {
        let v: LabeledValue<i32> =
            LabeledValue::with_domain(0, user_label(), SensitiveDomain::Face);
        assert_eq!(v.first_biometric_domain(), Some(SensitiveDomain::Face));
        let benign: LabeledValue<i32> = LabeledValue::new(0, user_label());
        assert_eq!(benign.first_biometric_domain(), None);
    }

    #[test]
    fn first_egress_banned_domain_returns_canonical() {
        let v: LabeledValue<i32> =
            LabeledValue::with_domain(0, user_label(), SensitiveDomain::Surveillance);
        assert_eq!(
            v.first_egress_banned_domain(),
            Some(SensitiveDomain::Surveillance)
        );
    }

    #[test]
    fn join_with_unions_domains_and_joins_labels() {
        let a: LabeledValue<i32> =
            LabeledValue::with_domain(2, user_label(), SensitiveDomain::Privacy);
        let b: LabeledValue<i32> =
            LabeledValue::with_domain(3, user_label(), SensitiveDomain::Manipulation);
        let sum = a.join_with(&b, |x, y| *x + *y);
        assert_eq!(sum.value, 5);
        assert_eq!(sum.sensitive_domains.len(), 2);
        assert!(sum.sensitive_domains.contains(&SensitiveDomain::Privacy));
        assert!(sum
            .sensitive_domains
            .contains(&SensitiveDomain::Manipulation));
    }

    #[test]
    fn join_with_propagates_biometric() {
        let a: LabeledValue<i32> = LabeledValue::new(1, user_label());
        let b: LabeledValue<i32> =
            LabeledValue::with_domain(2, user_label(), SensitiveDomain::Gaze);
        let sum = a.join_with(&b, |x, y| *x + *y);
        // Crucially, the gaze-tag flows from b → output.
        assert!(sum.is_biometric());
        assert!(sum.is_egress_banned());
    }

    #[test]
    fn map_preserves_label_and_domains() {
        let v: LabeledValue<i32> =
            LabeledValue::with_domain(7, user_label(), SensitiveDomain::Gaze);
        let mapped = v.clone().map(|x| x * 10);
        assert_eq!(mapped.value, 70);
        assert!(mapped.is_biometric());
    }
}
