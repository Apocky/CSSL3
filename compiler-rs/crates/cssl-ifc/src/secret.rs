//! `Secret<T, L>` — IFC-labeled wrapper + declassification.
//!
//! § SPEC : `specs/11_IFC.csl` § SECRET-WRAPPER + PRIME-DIRECTIVE §1
//!   N! surveillance + P18 BiometricEgress.
//!
//! § DESIGN
//!   `Secret<T, L>` carries a value at static-known label `L` (encoded as a
//!   const). Access to the inner `T` requires either:
//!     - matching-label sink, or
//!     - explicit declassification through [`declassify`] which routes through
//!       [`IfcLabel::declassify_check`].
//!
//!   Biometric labels are NEVER declassifiable — the only way to consume a
//!   biometric `Secret<T, L>` is to pass it to an on-device handler that
//!   itself returns a non-biometric value (e.g., a Σ-mask quantizer that
//!   emits an integer-bin index instead of the raw signal).

use crate::label::{DeclassifyError, IfcLabel, PrivilegeLevel};

/// A value tagged with a static IFC label. Construction is unrestricted
/// (you can ALWAYS classify), but extraction requires matching label or
/// successful declassification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secret<T> {
    value: T,
    label: IfcLabel,
}

impl<T> Secret<T> {
    /// Wrap `value` at `label`.
    #[must_use]
    pub const fn classify(value: T, label: IfcLabel) -> Self {
        Self { value, label }
    }

    /// The carried IFC label.
    #[must_use]
    pub const fn label(&self) -> IfcLabel {
        self.label
    }

    /// Reference the value at its current label. Callers must check the
    /// flow-policy themselves before consuming the reference at a sink.
    #[must_use]
    pub const fn peek(&self) -> &T {
        &self.value
    }

    /// `true` iff the wrapped label is biometric.
    #[must_use]
    pub const fn is_biometric(&self) -> bool {
        self.label.is_biometric()
    }

    /// Map the inner value while keeping the same label. The closure runs in
    /// a same-label context : its output is re-wrapped at `self.label`. This
    /// is the canonical way to do on-device Σ-mask quantization or analogous
    /// in-place transforms — the result inherits biometric tagging if the
    /// input had it.
    pub fn map_same_label<U, F>(self, f: F) -> Secret<U>
    where
        F: FnOnce(T) -> U,
    {
        Secret {
            value: f(self.value),
            label: self.label,
        }
    }
}

/// Attempt to declassify `s` from its static label down to `target` under
/// `priv_lvl`. Routes through [`IfcLabel::declassify_check`] which REFUSES
/// biometric labels for ALL privilege levels.
///
/// # Errors
/// - [`DeclassifyError::BiometricRefused`] for any biometric-tagged source.
/// - [`DeclassifyError::InsufficientPrivilege`] / [`DeclassifyError::NotDownward`]
///   for non-biometric label-violations.
pub fn declassify<T>(
    s: Secret<T>,
    target: IfcLabel,
    priv_lvl: PrivilegeLevel,
) -> Result<Secret<T>, (DeclassifyError, Secret<T>)> {
    match s.label.declassify_check(target, priv_lvl) {
        Ok(()) => Ok(Secret {
            value: s.value,
            label: target,
        }),
        Err(e) => Err((e, s)),
    }
}

#[cfg(test)]
mod tests {
    use super::{declassify, Secret};
    use crate::label::{BiometricKind, Confidentiality, IfcLabel, Integrity, PrivilegeLevel};

    #[test]
    fn classify_then_label_roundtrips() {
        let s = Secret::classify(42_u32, IfcLabel::confidential_trusted());
        assert_eq!(s.label(), IfcLabel::confidential_trusted());
        assert_eq!(*s.peek(), 42);
    }

    #[test]
    fn biometric_secret_recognized() {
        let s = Secret::classify(0.5_f32, IfcLabel::gaze());
        assert!(s.is_biometric());
    }

    #[test]
    fn map_same_label_preserves_biometric_tag() {
        let s = Secret::classify(0.7_f32, IfcLabel::gaze());
        let s2 = s.map_same_label(|f| f * 2.0);
        assert!(s2.is_biometric());
        assert_eq!(s2.label(), IfcLabel::gaze());
    }

    #[test]
    fn declassify_biometric_secret_fails_for_user() {
        let s = Secret::classify(0.0_f32, IfcLabel::gaze());
        let res = declassify(s, IfcLabel::public(), PrivilegeLevel::User);
        let (err, _back) = res.expect_err("must refuse");
        assert!(
            err.to_string().contains("biometric") || err.to_string().contains("BiometricEgress")
        );
    }

    #[test]
    fn declassify_biometric_secret_fails_for_apocky_root() {
        // ApockyRoot is the highest level ; still must refuse for biometric.
        let s = Secret::classify(0.0_f32, IfcLabel::biometric(BiometricKind::FaceTracking));
        let res = declassify(s, IfcLabel::public(), PrivilegeLevel::ApockyRoot);
        assert!(res.is_err());
    }

    #[test]
    fn declassify_returns_secret_intact_on_failure() {
        let s = Secret::classify(123_u32, IfcLabel::gaze());
        let res = declassify(s, IfcLabel::public(), PrivilegeLevel::Kernel);
        let (_err, back) = res.expect_err("must refuse");
        // The original Secret is returned so the caller can route through
        // an on-device handler instead.
        assert!(back.is_biometric());
        assert_eq!(*back.peek(), 123);
    }

    #[test]
    fn declassify_non_biometric_two_tier_succeeds_with_kernel() {
        let s = Secret::classify(1_u32, IfcLabel::confidential_trusted());
        let res = declassify(
            s,
            IfcLabel::new(
                Confidentiality::Public,
                Integrity::Trusted,
                BiometricKind::None,
            ),
            PrivilegeLevel::Kernel,
        );
        let s2 = res.expect("kernel may declassify");
        assert!(!s2.is_biometric());
        assert_eq!(s2.label().confidentiality, Confidentiality::Public);
    }

    #[test]
    fn declassify_non_biometric_low_priv_fails_but_returns_secret() {
        let s = Secret::classify(1_u32, IfcLabel::confidential_trusted());
        let res = declassify(
            s,
            IfcLabel::new(
                Confidentiality::Public,
                Integrity::Trusted,
                BiometricKind::None,
            ),
            PrivilegeLevel::User,
        );
        assert!(res.is_err());
    }

    #[test]
    fn all_biometric_kinds_refused_via_secret() {
        for kind in BiometricKind::all_biometric() {
            let s = Secret::classify(0_u32, IfcLabel::biometric(kind));
            for priv_lvl in PrivilegeLevel::all() {
                let s_clone = s.clone();
                let res = declassify(s_clone, IfcLabel::public(), priv_lvl);
                assert!(res.is_err(), "kind={kind:?} priv={priv_lvl:?} must refuse");
            }
        }
    }
}
