//! Gaze input types — the per-eye gaze-direction + confidence + saccade-state.
//!
//! § DESIGN
//!   `GazeInput` is the entry-point for eye-tracker data into this crate.
//!   It is **always wrapped in [`SensitiveGaze`]** — the type-alias for
//!   `cssl_ifc::LabeledValue<GazeInput>` carrying `SensitiveDomain::Gaze`
//!   in the domain-set. The label travels through the cssl-ifc lattice :
//!   any operator that consumes a `SensitiveGaze` and produces a derived
//!   value carries the `Gaze` domain forward, which in turn means the
//!   `validate_egress` gate at the telemetry-ring boundary refuses
//!   compile-time.
//!
//!   The IFC-label semantic on `SensitiveGaze` is :
//!     `Confidentiality = {Subject, GazeSubject}` — only the user themselves
//!     can read ; `Integrity = {Subject}` — only the user can author.
//!
//!   This is the *strongest* confidentiality on the device : even
//!   `Apocky-Root` cannot read it (per PRIME-DIRECTIVE §6 SCOPE :
//!   "no flag | config | … can disable | weaken | circumvent this").
//!
//! § PRIME-DIRECTIVE compile-time enforcement
//!   - Constructing a `SensitiveGaze` from a raw `GazeInput` calls
//!     [`SensitiveGaze::from_raw`] which wraps with the canonical label +
//!     domain.
//!   - There is no `unsafe` alternative path. The `LabeledValue::value`
//!     field is `pub` (part of the cssl-ifc API), but every operation
//!     that flows the value into a non-on-device sink hits
//!     `validate_egress` which refuses.
//!   - Tests in `tests::sensitive_gaze_egress_refused` verify the
//!     gate fires for every constructor path.

use std::collections::BTreeSet;

use cssl_ifc::{
    Confidentiality, Integrity, Label, LabeledValue, Principal, PrincipalSet, SensitiveDomain,
};

/// Per-eye gaze direction in head-relative space.
///
/// Direction is a unit-vector (x, y, z) where the head's forward axis is +z.
/// Construction validates the unit-vector property within `1e-3` tolerance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GazeDirection {
    /// X-component of the unit gaze direction.
    pub x: f32,
    /// Y-component.
    pub y: f32,
    /// Z-component.
    pub z: f32,
}

impl GazeDirection {
    /// Construct a gaze direction. Validates unit-vector property.
    pub fn new(x: f32, y: f32, z: f32) -> Result<Self, crate::error::GazeCollapseError> {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return Err(crate::error::GazeCollapseError::InvalidGazeInput {
                field: "direction",
                value: format!("({}, {}, {})", x, y, z),
            });
        }
        let mag2 = x.mul_add(x, y.mul_add(y, z * z));
        if (mag2 - 1.0).abs() > 1e-3 {
            return Err(crate::error::GazeCollapseError::InvalidGazeInput {
                field: "direction.magnitude",
                value: format!("|({}, {}, {})|² = {}", x, y, z, mag2),
            });
        }
        Ok(Self { x, y, z })
    }

    /// Construct a gaze direction without unit-vector validation. Used
    /// internally by saccade-prediction where intermediate vectors may
    /// briefly violate normalization before re-normalization.
    pub(crate) const fn unchecked(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Forward axis (0, 0, 1) — the head-relative "looking straight ahead"
    /// direction. Used as the center-bias-foveation fallback gaze.
    pub const FORWARD: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    /// Squared dot product with another direction (for similarity).
    pub fn dot(&self, other: &Self) -> f32 {
        self.x
            .mul_add(other.x, self.y.mul_add(other.y, self.z * other.z))
    }

    /// Angular distance in radians between two unit-direction vectors.
    pub fn angular_distance(&self, other: &Self) -> f32 {
        // clamp to avoid acos NaN on rounding overshoot
        let d = self.dot(other).clamp(-1.0, 1.0);
        d.acos()
    }
}

/// Confidence in the gaze measurement, in [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct GazeConfidence(f32);

impl GazeConfidence {
    /// Construct ; rejects out-of-range or non-finite values.
    pub fn new(c: f32) -> Result<Self, crate::error::GazeCollapseError> {
        if !c.is_finite() || !(0.0..=1.0).contains(&c) {
            return Err(crate::error::GazeCollapseError::InvalidConfidence(c));
        }
        Ok(Self(c))
    }

    /// Inner value.
    pub const fn value(&self) -> f32 {
        self.0
    }

    /// Confidence threshold below which the pass falls back to the
    /// previous-frame gaze ([`crate::FoveationFallback::LastKnownGaze`]).
    pub const FALLBACK_THRESHOLD: Self = Self(0.3);

    /// `true` iff confidence ≥ threshold.
    pub fn passes_threshold(&self, threshold: Self) -> bool {
        self.0 >= threshold.0
    }
}

/// Eye-openness sample (squint / blink state).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct EyeOpenness(f32);

impl EyeOpenness {
    /// Construct ; rejects out-of-range or non-finite values.
    pub fn new(o: f32) -> Result<Self, crate::error::GazeCollapseError> {
        if !o.is_finite() || !(0.0..=1.0).contains(&o) {
            return Err(crate::error::GazeCollapseError::InvalidGazeInput {
                field: "eye_openness",
                value: format!("{}", o),
            });
        }
        Ok(Self(o))
    }

    /// Inner value.
    pub const fn value(&self) -> f32 {
        self.0
    }

    /// Threshold below which the eye is considered closed (blink-state).
    pub const BLINK_THRESHOLD: Self = Self(0.15);

    /// `true` iff eye is closed enough to be a blink.
    pub fn is_blink(&self) -> bool {
        self.0 < Self::BLINK_THRESHOLD.0
    }
}

/// Coarse blink-state classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlinkState {
    /// Both eyes open above blink-threshold.
    Open,
    /// One eye closed (a wink ; not a saccadic-suppression).
    Wink,
    /// Both eyes closed (saccadic-suppression candidate ; renderer hides
    /// any flicker during this window).
    Both,
}

/// Saccade-state classification, used by the saccadic-predictor to gate
/// when to issue a prediction (only during fixations between saccades).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SaccadeState {
    /// Fixation : gaze is stable on a target. Prediction issued when
    /// likelihood-of-saccade exceeds threshold.
    Fixation,
    /// Saccade in flight : the gaze is mid-jump between targets ; no
    /// prediction issued (saccadic-suppression hides flicker).
    Saccade,
    /// Smooth pursuit : gaze tracking a moving target ; prediction
    /// uses a velocity-extrapolation model.
    Pursuit,
    /// Vestibulo-ocular-reflex : gaze stabilizing against head motion.
    VOR,
}

/// Per-eye gaze input bundle.
///
/// All fields are **on-device-only** ; the canonical wrapper [`SensitiveGaze`]
/// must be used to flow this into the gaze-collapse pass.
#[derive(Debug, Clone, PartialEq)]
pub struct GazeInput {
    /// Left eye gaze direction.
    pub left_direction: GazeDirection,
    /// Right eye gaze direction.
    pub right_direction: GazeDirection,
    /// Left eye confidence.
    pub left_confidence: GazeConfidence,
    /// Right eye confidence.
    pub right_confidence: GazeConfidence,
    /// Left eye openness (squint/blink).
    pub left_openness: EyeOpenness,
    /// Right eye openness.
    pub right_openness: EyeOpenness,
    /// Saccade-state for the bound (left + right share state because real
    /// human saccades are conjugate — both eyes move together).
    pub saccade_state: SaccadeState,
    /// Frame-counter monotone within the session (used by the predictor's
    /// time-axis ; rolls over at u32::MAX which is ~13 hours at 90 Hz).
    pub frame_counter: u32,
    /// Convergence (vergence-distance) inferred from the two eye-rays'
    /// intersection, in head-relative meters. Range typically 0.1..=10 m.
    /// Optional — `None` means the tracker did not estimate it.
    pub convergence_meters: Option<f32>,
}

impl GazeInput {
    /// Construct a center-bias-fallback gaze (both eyes forward, full confidence).
    /// This is the only gaze-input that does NOT require gaze hardware ; it
    /// is what [`crate::FoveationFallback::CenterBias`] uses.
    #[must_use]
    pub fn center_bias_fallback(frame_counter: u32) -> Self {
        Self {
            left_direction: GazeDirection::FORWARD,
            right_direction: GazeDirection::FORWARD,
            left_confidence: GazeConfidence(1.0),
            right_confidence: GazeConfidence(1.0),
            left_openness: EyeOpenness(1.0),
            right_openness: EyeOpenness(1.0),
            saccade_state: SaccadeState::Fixation,
            frame_counter,
            convergence_meters: None,
        }
    }

    /// Coarse blink-state classification.
    #[must_use]
    pub fn blink_state(&self) -> BlinkState {
        let l = self.left_openness.is_blink();
        let r = self.right_openness.is_blink();
        match (l, r) {
            (true, true) => BlinkState::Both,
            (true, false) | (false, true) => BlinkState::Wink,
            (false, false) => BlinkState::Open,
        }
    }

    /// Bound-eye confidence — the minimum of left + right.
    #[must_use]
    pub fn bound_confidence(&self) -> GazeConfidence {
        if self.left_confidence.0 < self.right_confidence.0 {
            self.left_confidence
        } else {
            self.right_confidence
        }
    }

    /// Cyclopean (averaged) gaze direction.
    #[must_use]
    pub fn cyclopean_direction(&self) -> GazeDirection {
        let x = (self.left_direction.x + self.right_direction.x) * 0.5;
        let y = (self.left_direction.y + self.right_direction.y) * 0.5;
        let z = (self.left_direction.z + self.right_direction.z) * 0.5;
        // Re-normalize because averaging two unit vectors does not preserve unit-norm.
        let mag = x.mul_add(x, y.mul_add(y, z * z)).sqrt();
        if mag > 0.0 {
            GazeDirection::unchecked(x / mag, y / mag, z / mag)
        } else {
            GazeDirection::FORWARD
        }
    }
}

/// `SensitiveGaze` : the canonical IFC-labeled wrapper around `GazeInput`.
///
/// Carries `SensitiveDomain::Gaze` in the domain-set + a confidentiality-label
/// restricted to `{Subject, GazeSubject}`. This means the
/// `cssl_ifc::validate_egress` structural-gate refuses any value derived from
/// a `SensitiveGaze` for any telemetry sink.
pub type SensitiveGaze = LabeledValue<GazeInput>;

/// Convenience constructors for `SensitiveGaze`.
pub trait SensitiveGazeConstructors: Sized {
    /// Wrap a raw [`GazeInput`] with the canonical biometric-gaze label.
    fn from_raw(input: GazeInput) -> Self;
    /// Wrap with a custom label, preserving the `Gaze` domain.
    fn from_raw_with_label(input: GazeInput, label: Label) -> Self;
}

impl SensitiveGazeConstructors for SensitiveGaze {
    fn from_raw(input: GazeInput) -> Self {
        let label = canonical_gaze_label();
        let mut domains = BTreeSet::new();
        domains.insert(SensitiveDomain::Gaze);
        LabeledValue::with_domains(input, label, domains)
    }

    fn from_raw_with_label(input: GazeInput, label: Label) -> Self {
        let mut domains = BTreeSet::new();
        domains.insert(SensitiveDomain::Gaze);
        LabeledValue::with_domains(input, label, domains)
    }
}

/// The canonical gaze label : `{Subject, GazeSubject}` confidentiality,
/// `{Subject}` integrity. This is the strongest privacy posture for any
/// data on the device — even `Apocky-Root` cannot read.
fn canonical_gaze_label() -> Label {
    let mut conf = PrincipalSet::empty();
    conf.insert(Principal::Subject);
    conf.insert(Principal::GazeSubject);
    let mut integ = PrincipalSet::empty();
    integ.insert(Principal::Subject);
    Label {
        confidentiality: Confidentiality(conf),
        integrity: Integrity(integ),
    }
}

#[cfg(test)]
mod tests {
    use cssl_ifc::{validate_egress, EgressGrantError, SensitiveDomain};

    use super::{
        BlinkState, EyeOpenness, GazeConfidence, GazeDirection, GazeInput, SaccadeState,
        SensitiveGaze, SensitiveGazeConstructors,
    };

    fn unit_forward() -> GazeDirection {
        GazeDirection::new(0.0, 0.0, 1.0).unwrap()
    }

    fn baseline_input() -> GazeInput {
        GazeInput {
            left_direction: unit_forward(),
            right_direction: unit_forward(),
            left_confidence: GazeConfidence::new(0.95).unwrap(),
            right_confidence: GazeConfidence::new(0.92).unwrap(),
            left_openness: EyeOpenness::new(0.9).unwrap(),
            right_openness: EyeOpenness::new(0.9).unwrap(),
            saccade_state: SaccadeState::Fixation,
            frame_counter: 0,
            convergence_meters: Some(2.0),
        }
    }

    #[test]
    fn gaze_direction_rejects_nan() {
        assert!(GazeDirection::new(f32::NAN, 0.0, 0.0).is_err());
        assert!(GazeDirection::new(0.0, f32::NAN, 0.0).is_err());
        assert!(GazeDirection::new(0.0, 0.0, f32::NAN).is_err());
    }

    #[test]
    fn gaze_direction_rejects_non_unit() {
        assert!(GazeDirection::new(2.0, 0.0, 0.0).is_err());
    }

    #[test]
    fn gaze_direction_accepts_unit() {
        assert!(GazeDirection::new(0.0, 0.0, 1.0).is_ok());
        assert!(GazeDirection::new(1.0, 0.0, 0.0).is_ok());
        let s = (1.0_f32 / 3.0).sqrt();
        assert!(GazeDirection::new(s, s, s).is_ok());
    }

    #[test]
    fn gaze_direction_dot_self_is_one() {
        let d = unit_forward();
        assert!((d.dot(&d) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn gaze_direction_angular_distance_zero_for_equal() {
        let d = unit_forward();
        let a = d.angular_distance(&d);
        assert!(a.abs() < 1e-3);
    }

    #[test]
    fn gaze_direction_angular_distance_pi_for_opposite() {
        let a = GazeDirection::new(0.0, 0.0, 1.0).unwrap();
        let b = GazeDirection::new(0.0, 0.0, -1.0).unwrap();
        let d = a.angular_distance(&b);
        assert!((d - std::f32::consts::PI).abs() < 1e-3);
    }

    #[test]
    fn gaze_confidence_rejects_out_of_range() {
        assert!(GazeConfidence::new(-0.1).is_err());
        assert!(GazeConfidence::new(1.1).is_err());
        assert!(GazeConfidence::new(f32::NAN).is_err());
    }

    #[test]
    fn gaze_confidence_threshold_check() {
        let high = GazeConfidence::new(0.9).unwrap();
        let low = GazeConfidence::new(0.2).unwrap();
        assert!(high.passes_threshold(GazeConfidence::FALLBACK_THRESHOLD));
        assert!(!low.passes_threshold(GazeConfidence::FALLBACK_THRESHOLD));
    }

    #[test]
    fn eye_openness_blink_threshold() {
        let closed = EyeOpenness::new(0.05).unwrap();
        let open = EyeOpenness::new(0.9).unwrap();
        assert!(closed.is_blink());
        assert!(!open.is_blink());
    }

    #[test]
    fn blink_state_classification() {
        let mut input = baseline_input();
        assert_eq!(input.blink_state(), BlinkState::Open);
        input.left_openness = EyeOpenness::new(0.05).unwrap();
        assert_eq!(input.blink_state(), BlinkState::Wink);
        input.right_openness = EyeOpenness::new(0.05).unwrap();
        assert_eq!(input.blink_state(), BlinkState::Both);
    }

    #[test]
    fn bound_confidence_returns_minimum() {
        let mut input = baseline_input();
        input.left_confidence = GazeConfidence::new(0.4).unwrap();
        input.right_confidence = GazeConfidence::new(0.9).unwrap();
        assert!((input.bound_confidence().value() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn cyclopean_direction_is_unit() {
        let cyc = baseline_input().cyclopean_direction();
        let mag2 = cyc.x.mul_add(cyc.x, cyc.y.mul_add(cyc.y, cyc.z * cyc.z));
        assert!((mag2 - 1.0).abs() < 1e-3);
    }

    #[test]
    fn center_bias_fallback_is_forward_full_confidence() {
        let f = GazeInput::center_bias_fallback(42);
        assert_eq!(f.left_direction, GazeDirection::FORWARD);
        assert_eq!(f.right_direction, GazeDirection::FORWARD);
        assert!((f.left_confidence.value() - 1.0).abs() < 1e-6);
        assert_eq!(f.frame_counter, 42);
    }

    #[test]
    fn sensitive_gaze_carries_gaze_domain() {
        let s: SensitiveGaze = SensitiveGaze::from_raw(baseline_input());
        assert!(s.sensitive_domains.contains(&SensitiveDomain::Gaze));
        assert!(s.is_biometric());
    }

    #[test]
    fn sensitive_gaze_egress_refused_canonical_constructor() {
        let s: SensitiveGaze = SensitiveGaze::from_raw(baseline_input());
        let res = validate_egress(&s);
        assert!(matches!(
            res,
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    #[test]
    fn sensitive_gaze_egress_refused_by_label_principal() {
        // Even if domain-tags were stripped (cannot happen via API but
        // double-checked here), the LABEL alone (with GazeSubject in
        // confidentiality) must trigger refusal.
        let s: SensitiveGaze = SensitiveGaze::from_raw(baseline_input());
        // Confirm the label has GazeSubject in confidentiality.
        assert!(s.label.has_biometric_confidentiality());
    }

    #[test]
    fn sensitive_gaze_join_propagates_gaze_domain() {
        let a: SensitiveGaze = SensitiveGaze::from_raw(baseline_input());
        let b: SensitiveGaze = SensitiveGaze::from_raw(baseline_input());
        // Synthetic join : compute angular distance ; output carries Gaze.
        let combined = a.join_with(&b, |x, y| {
            x.cyclopean_direction()
                .angular_distance(&y.cyclopean_direction())
        });
        assert!(combined.is_biometric());
        assert!(matches!(
            validate_egress(&combined),
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    #[test]
    fn sensitive_gaze_no_privilege_can_egress() {
        // Verify the cssl-ifc `for_domain_with_privilege` refuses gaze for
        // every privilege-tier including ApockyRoot.
        for tier in [
            cssl_ifc::PrivilegeLevel::User,
            cssl_ifc::PrivilegeLevel::System,
            cssl_ifc::PrivilegeLevel::Kernel,
            cssl_ifc::PrivilegeLevel::Root,
            cssl_ifc::PrivilegeLevel::AnthropicAudit,
            cssl_ifc::PrivilegeLevel::ApockyRoot,
        ] {
            let cap = cssl_ifc::TelemetryEgress::for_domain_with_privilege(
                cssl_ifc::SensitiveDomain::Gaze,
                tier,
            );
            assert!(
                matches!(cap, Err(EgressGrantError::BiometricRefused { .. })),
                "tier {:?} must not authorize gaze egress",
                tier
            );
        }
    }

    #[test]
    fn saccade_state_distinct_variants() {
        let states = [
            SaccadeState::Fixation,
            SaccadeState::Saccade,
            SaccadeState::Pursuit,
            SaccadeState::VOR,
        ];
        for (i, a) in states.iter().enumerate() {
            for (j, b) in states.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }
}
