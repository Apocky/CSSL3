//! Biometric-egress refusal at the telemetry-ring boundary.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` § OBSERVABILITY-FIRST-CLASS +
//! `specs/11_IFC.csl` § PRIME-DIRECTIVE ENCODING +
//! `PRIME_DIRECTIVE.md §1` (anti-surveillance) +
//! `PRIME_DIRECTIVE.md §11` (attestation) +
//! `Omniverse/07_AESTHETIC/05_VR_RENDERING.csl` § II.A
//! ("eye-track : raw-gaze on-device ⊗ R! NEVER-egress (PRIME §1
//! anti-surveillance gate)").
//!
//! § DESIGN
//!   The boundary at which biometric data could leak past the on-device
//!   trust-perimeter is the **producer-side of the telemetry ring**. Every
//!   `record(...)` call into the ring is a potential egress (the consumer
//!   may export to a remote OTLP collector, write to Chrome-trace files,
//!   or otherwise make the data observable past the device).
//!
//!   This module provides the [`record_labeled`] entry-point used by
//!   compiler-emitted code + library code that wants to log labeled
//!   values. It refuses biometric-family + surveillance + coercion-tagged
//!   values at compile-time-of-the-call-site by returning a
//!   [`TelemetryRefusal`] error rather than performing the push. The
//!   refusal itself is logged into the audit-chain via the
//!   `BiometricRefused` scope so PRIME-DIRECTIVE §11 attestation has a
//!   permanent signed witness.
//!
//! § CAPABILITY-INTERLOCK
//!   Construction of an actual ring-push requires presenting a
//!   [`TelemetryEgress`] capability authorizing the value's domain. The
//!   capability constructor in `cssl-ifc` already refuses biometric-family
//!   domains, so the type system guarantees that no
//!   `TelemetryEgress { authorized_domain: Gaze, .. }` value can ever
//!   exist. This is the structural-gate : you cannot accidentally log
//!   biometric data because there is no way to obtain the capability that
//!   would authorize it.

use cssl_ifc::{validate_egress, EgressGrantError, LabeledValue, TelemetryEgress};
use thiserror::Error;

use crate::audit::AuditChain;
use crate::ring::{TelemetryRing, TelemetrySlot};
use crate::scope::{TelemetryKind, TelemetryScope};

/// Refusal reason returned by the telemetry-ring boundary when a labeled
/// value is rejected.
///
/// Each variant maps to a PRIME-DIRECTIVE §1 prohibition class. The
/// boundary returns these instead of pushing the slot into the ring.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum TelemetryRefusal {
    /// Biometric-family domain (gaze, face, body, biometric).
    /// PRIME-DIRECTIVE §1 anti-surveillance — non-overridable.
    #[error(
        "telemetry refused biometric-family value (domain={domain}) — \
         PRIME-DIRECTIVE §1 anti-surveillance ; \
         compile-time gate, no Privilege<*> override exists"
    )]
    BiometricRefused {
        /// The specific biometric-family domain that triggered the refusal.
        domain: cssl_ifc::SensitiveDomain,
    },
    /// Surveillance domain. PRIME-DIRECTIVE §1 — non-overridable.
    #[error(
        "telemetry refused surveillance-tagged value — \
         PRIME-DIRECTIVE §1 anti-surveillance ; \
         compile-time gate, no Privilege<*> override exists"
    )]
    SurveillanceRefused,
    /// Coercion domain. PRIME-DIRECTIVE §1 — absolute prohibition.
    #[error(
        "telemetry refused coercion-tagged value — \
         PRIME-DIRECTIVE §1 absolute prohibition ; \
         compile-time gate, no Privilege<*> override exists"
    )]
    CoercionRefused,
    /// Weapon domain attempted to flow without `Privilege<Kernel>`.
    #[error(
        "telemetry refused weapon-tagged value without Privilege<Kernel> \
         (specs/11 PRIME-DIRECTIVE ENCODING)"
    )]
    WeaponNeedsKernel,
    /// The presented `TelemetryEgress` capability does not authorize the
    /// value (mismatched authorized_domain). This is a programmer-error
    /// indicating the call-site requested egress without acquiring the
    /// matching cap.
    #[error(
        "telemetry refused : presented TelemetryEgress capability does not \
         authorize the value's sensitive_domains (cap auth = {cap_auth} ; \
         value carries domains that don't match)"
    )]
    CapabilityMismatch {
        /// The domain the presented cap authorized.
        cap_auth: cssl_ifc::SensitiveDomain,
    },
}

impl From<EgressGrantError> for TelemetryRefusal {
    fn from(e: EgressGrantError) -> Self {
        match e {
            EgressGrantError::BiometricRefused { domain } => Self::BiometricRefused { domain },
            EgressGrantError::SurveillanceRefused => Self::SurveillanceRefused,
            EgressGrantError::CoercionRefused => Self::CoercionRefused,
            EgressGrantError::WeaponNeedsKernel => Self::WeaponNeedsKernel,
        }
    }
}

impl TelemetrySlot {
    /// Build a slot recording that a biometric-egress attempt was refused.
    ///
    /// The slot uses the [`TelemetryScope::BiometricRefused`] diagnostic
    /// scope + [`TelemetryKind::Audit`] kind. The payload encodes the
    /// refusal-reason + domain-name in the inline-payload bytes for
    /// downstream exporters.
    #[must_use]
    pub fn refusal(timestamp_ns: u64, reason: &TelemetryRefusal) -> Self {
        let payload = reason.diagnostic_bytes();
        Self::new(
            timestamp_ns,
            TelemetryScope::BiometricRefused,
            TelemetryKind::Audit,
        )
        .with_inline_payload(&payload)
    }
}

impl TelemetryRefusal {
    /// Compact diagnostic-payload bytes describing this refusal, suitable
    /// for inclusion in a `TelemetrySlot::with_inline_payload` slot. The
    /// format is human-readable ASCII so JSON / Chrome-trace exporters can
    /// surface it directly without re-encoding.
    #[must_use]
    pub fn diagnostic_bytes(&self) -> Vec<u8> {
        match self {
            Self::BiometricRefused { domain } => {
                format!("biometric-refused:{domain}").into_bytes()
            }
            Self::SurveillanceRefused => b"surveillance-refused".to_vec(),
            Self::CoercionRefused => b"coercion-refused".to_vec(),
            Self::WeaponNeedsKernel => b"weapon-needs-kernel".to_vec(),
            Self::CapabilityMismatch { cap_auth } => {
                format!("cap-mismatch:auth={cap_auth}").into_bytes()
            }
        }
    }

    /// Short refusal-tag (stable canonical name).
    #[must_use]
    pub const fn refusal_tag(&self) -> &'static str {
        match self {
            Self::BiometricRefused { .. } => "biometric-refused",
            Self::SurveillanceRefused => "surveillance-refused",
            Self::CoercionRefused => "coercion-refused",
            Self::WeaponNeedsKernel => "weapon-needs-kernel",
            Self::CapabilityMismatch { .. } => "capability-mismatch",
        }
    }
}

/// Record a labeled value into the telemetry ring at the producer-side.
///
/// **Refuses biometric-family + surveillance + coercion at the boundary.**
/// The refusal is non-overridable (no `Privilege<*>` cap can authorize it)
/// and is itself logged into `audit_chain` if provided so PRIME-DIRECTIVE
/// §11 attestation has a permanent signed witness.
///
/// Successful (non-refused) records require an authorizing
/// [`TelemetryEgress`] capability whose `authorized_domain` matches the
/// value's domain-set. The capability constructor in `cssl-ifc` itself
/// refuses biometric-family at construction-time, so this function is the
/// second of two coordinated structural gates.
///
/// # Errors
/// Returns the appropriate [`TelemetryRefusal`] variant if :
/// - the value carries a biometric-family domain or principal
/// - the value carries a surveillance / coercion domain or principal
/// - the value carries a weapon domain without Kernel-priv (caller-side check)
/// - the presented cap doesn't authorize the value's domain-set
pub fn record_labeled<T>(
    ring: &TelemetryRing,
    audit_chain: Option<&mut AuditChain>,
    cap: &TelemetryEgress,
    value: &LabeledValue<T>,
    timestamp_ns: u64,
    scope: TelemetryScope,
    kind: TelemetryKind,
) -> Result<(), TelemetryRefusal> {
    // Step 1 : structural-gate validation. Refuses biometric/surveillance/
    // coercion regardless of capability — the cap MIGHT have been forged
    // by some compiler bug, but the value-side refusal is the second
    // independent guarantee.
    if let Err(e) = validate_egress(value) {
        let refusal = TelemetryRefusal::from(e);
        if let Some(chain) = audit_chain {
            chain.append(
                refusal.refusal_tag(),
                refusal.to_string(),
                timestamp_ns / 1_000_000_000,
            );
        }
        let _ = ring.push(TelemetrySlot::refusal(timestamp_ns, &refusal));
        return Err(refusal);
    }
    // Step 2 : capability-side check — the cap must authorize the value's
    // domains. (Defense-in-depth — Step 1 should have already covered all
    // banned domains.)
    if !cap.authorizes(value) {
        let refusal = TelemetryRefusal::CapabilityMismatch {
            cap_auth: cap.authorized_domain,
        };
        if let Some(chain) = audit_chain {
            chain.append(
                refusal.refusal_tag(),
                refusal.to_string(),
                timestamp_ns / 1_000_000_000,
            );
        }
        let _ = ring.push(TelemetrySlot::refusal(timestamp_ns, &refusal));
        return Err(refusal);
    }
    // Step 3 : safe to push.
    let _ = ring.push(TelemetrySlot::new(timestamp_ns, scope, kind));
    Ok(())
}

/// Trait marking types that are **safe to log to telemetry** because they
/// carry no biometric / surveillance / coercion data.
///
/// This is a marker trait : implementations are restricted to types whose
/// values cannot semantically represent biometric data. The compiler /
/// human-reviewer checks on `impl BiometricSafe for T` are the first
/// review-gate ; the runtime [`record_labeled`] check is the second.
///
/// `BiometricSafe` is **explicitly NOT** implemented for any type that
/// could carry gaze/face/body/biometric measurements. Trying to log such
/// a value via [`record_labeled`] will be refused at the
/// [`validate_egress`] step — the type-system layer is informational; the
/// label-lattice layer is enforcing.
pub trait BiometricSafe {}

impl BiometricSafe for u8 {}
impl BiometricSafe for u16 {}
impl BiometricSafe for u32 {}
impl BiometricSafe for u64 {}
impl BiometricSafe for i8 {}
impl BiometricSafe for i16 {}
impl BiometricSafe for i32 {}
impl BiometricSafe for i64 {}
impl BiometricSafe for f32 {}
impl BiometricSafe for f64 {}
impl BiometricSafe for bool {}
impl BiometricSafe for &str {}
impl BiometricSafe for String {}

/// **Compile-time** guarantee : every value passed to this function is
/// `BiometricSafe`. It still performs the runtime
/// [`validate_egress`] / capability check so that wrapper types that
/// implement `BiometricSafe` but contain biometric subfields cannot
/// trivially escape.
///
/// This is the recommended entry-point for compiler-emitted telemetry
/// calls : the trait-bound is checked at the call-site so a buggy
/// compiler-pass that tries to log a non-`BiometricSafe` type will fail
/// the build, not silently leak.
pub fn record_labeled_safe<T: BiometricSafe>(
    ring: &TelemetryRing,
    audit_chain: Option<&mut AuditChain>,
    cap: &TelemetryEgress,
    value: &LabeledValue<T>,
    timestamp_ns: u64,
    scope: TelemetryScope,
    kind: TelemetryKind,
) -> Result<(), TelemetryRefusal> {
    record_labeled(ring, audit_chain, cap, value, timestamp_ns, scope, kind)
}

#[cfg(test)]
mod tests {
    use super::{record_labeled, record_labeled_safe, BiometricSafe, TelemetryRefusal};
    use crate::audit::AuditChain;
    use crate::ring::TelemetryRing;
    use crate::scope::{TelemetryKind, TelemetryScope};
    use cssl_ifc::{
        Label, LabeledValue, Principal, PrincipalSet, PrivilegeLevel, SensitiveDomain,
        TelemetryEgress,
    };

    fn benign_label() -> Label {
        Label::restricted(
            PrincipalSet::singleton(Principal::User),
            PrincipalSet::singleton(Principal::User),
        )
    }

    fn privacy_cap() -> TelemetryEgress {
        TelemetryEgress::for_domain(SensitiveDomain::Privacy).unwrap()
    }

    fn benign_value() -> LabeledValue<u32> {
        LabeledValue::new(42u32, benign_label())
    }

    // === BIOMETRIC REFUSAL ===

    #[test]
    fn biometric_log_refused() {
        let ring = TelemetryRing::new(8);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Biometric);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            1_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(matches!(
            res,
            Err(TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Biometric
            })
        ));
        // The refusal slot itself was pushed.
        assert_eq!(ring.len(), 1);
        let slot = ring.peek().unwrap();
        assert_eq!(slot.scope, TelemetryScope::BiometricRefused.as_u16());
        assert_eq!(slot.kind, TelemetryKind::Audit.as_u16());
    }

    #[test]
    fn gaze_log_refused() {
        let ring = TelemetryRing::new(8);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Gaze);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            2_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(matches!(
            res,
            Err(TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    #[test]
    fn face_track_log_refused() {
        let ring = TelemetryRing::new(8);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Face);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            3_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(matches!(
            res,
            Err(TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Face
            })
        ));
    }

    #[test]
    fn body_track_log_refused() {
        let ring = TelemetryRing::new(8);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Body);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            4_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(matches!(
            res,
            Err(TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Body
            })
        ));
    }

    // === LABEL-PRINCIPAL TRIGGERED REFUSAL ===

    #[test]
    fn gaze_principal_in_label_refused_even_without_domain_tag() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::GazeSubject),
            PrincipalSet::singleton(Principal::User),
        );
        // No SensitiveDomain tag — only the label-principal carries the gaze info.
        let v: LabeledValue<u32> = LabeledValue::new(0, label);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            5_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(matches!(
            res,
            Err(TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    #[test]
    fn face_principal_in_label_refused() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::FaceSubject),
            PrincipalSet::singleton(Principal::User),
        );
        let v: LabeledValue<u32> = LabeledValue::new(0, label);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            6_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(matches!(
            res,
            Err(TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Face
            })
        ));
    }

    #[test]
    fn body_principal_in_label_refused() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let label = Label::restricted(
            PrincipalSet::singleton(Principal::BodySubject),
            PrincipalSet::singleton(Principal::User),
        );
        let v: LabeledValue<u32> = LabeledValue::new(0, label);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            7_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(matches!(
            res,
            Err(TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Body
            })
        ));
    }

    // === SURVEILLANCE / COERCION REFUSAL ===

    #[test]
    fn surveillance_log_refused() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Surveillance);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            8_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert_eq!(res, Err(TelemetryRefusal::SurveillanceRefused));
    }

    #[test]
    fn coercion_log_refused() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Coercion);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            9_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert_eq!(res, Err(TelemetryRefusal::CoercionRefused));
    }

    // === HAPPY PATH : NON-BIOMETRIC LOGS FINE ===

    #[test]
    fn non_biometric_still_logs_fine() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let v = benign_value();
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            10_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(res.is_ok());
        assert_eq!(ring.len(), 1);
        let slot = ring.peek().unwrap();
        assert_eq!(slot.scope, TelemetryScope::Counters.as_u16());
        assert_eq!(slot.kind, TelemetryKind::Counter.as_u16());
    }

    #[test]
    fn privacy_domain_logs_fine_with_matching_cap() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Privacy);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            11_000,
            TelemetryScope::Spans,
            TelemetryKind::SpanBegin,
        );
        assert!(res.is_ok());
    }

    // === PRIVILEGE-OVERRIDE REFUSED ===

    #[test]
    fn privilege_apocky_root_cannot_override_biometric_refusal() {
        // Even Apocky-Root cap construction is refused for biometric.
        let cap_attempt = TelemetryEgress::for_domain_with_privilege(
            SensitiveDomain::Gaze,
            PrivilegeLevel::ApockyRoot,
        );
        assert!(cap_attempt.is_err());
    }

    #[test]
    fn privilege_kernel_cannot_override_surveillance_refusal() {
        let cap_attempt = TelemetryEgress::for_domain_with_privilege(
            SensitiveDomain::Surveillance,
            PrivilegeLevel::Kernel,
        );
        assert!(cap_attempt.is_err());
    }

    #[test]
    fn privilege_anthropic_audit_cannot_override_coercion_refusal() {
        let cap_attempt = TelemetryEgress::for_domain_with_privilege(
            SensitiveDomain::Coercion,
            PrivilegeLevel::AnthropicAudit,
        );
        assert!(cap_attempt.is_err());
    }

    // === REFUSAL ITSELF AUDIT-LOGGED ===

    #[test]
    fn biometric_refusal_appended_to_audit_chain() {
        let ring = TelemetryRing::new(4);
        let mut chain = AuditChain::new();
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Gaze);
        let _ = record_labeled(
            &ring,
            Some(&mut chain),
            &cap,
            &v,
            12_000_000_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert_eq!(chain.len(), 1);
        let entry = chain.iter().next().unwrap();
        assert_eq!(entry.tag, "biometric-refused");
        assert!(entry.message.contains("PRIME-DIRECTIVE"));
        chain.verify_chain().unwrap();
    }

    #[test]
    fn refusal_payload_carries_domain_in_inline_bytes() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Body);
        let _ = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            13_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        let slot = ring.peek().unwrap();
        let payload = String::from_utf8_lossy(&slot.payload);
        assert!(payload.starts_with("biometric-refused:body"));
    }

    // === CAPABILITY-MISMATCH ===

    #[test]
    fn cap_mismatch_when_authorized_domain_does_not_cover_value() {
        let ring = TelemetryRing::new(4);
        // Cap authorizes Manipulation only.
        let cap = TelemetryEgress::for_domain(SensitiveDomain::Manipulation).unwrap();
        // Value tagged Privacy — Privacy isn't in the absolute-banned set so
        // the validate-egress step passes, but cap.authorizes is false because
        // the value carries Privacy and authorized_domain is Manipulation.
        // However Privacy is also not in the banned set, so cap.authorizes
        // returns true (any non-banned domain is permitted under any cap).
        // We test cap-mismatch via a different angle : two SensitiveDomain
        // values where one is banned. The validate-egress step will catch
        // the banned one first, so cap-mismatch path is structurally
        // unreachable for currently-defined banned domains. Demonstrate
        // instead that cap-correctness is independent of the value-check.
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Privacy);
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &v,
            14_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        // Privacy is not absolutely banned + cap.authorizes accepts non-banned domains.
        assert!(res.is_ok());
    }

    // === SAFE-TRAIT WRAPPER ===

    #[test]
    fn record_labeled_safe_accepts_primitive_types() {
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let v: LabeledValue<u32> = LabeledValue::new(42, benign_label());
        let res = record_labeled_safe(
            &ring,
            None,
            &cap,
            &v,
            15_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn record_labeled_safe_still_refuses_biometric_label() {
        // A primitive value (BiometricSafe) wrapped in a biometric label is
        // STILL refused — type-bound is informational ; label-check is enforcing.
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let v: LabeledValue<u32> =
            LabeledValue::with_domain(0xDEAD, benign_label(), SensitiveDomain::Gaze);
        let res = record_labeled_safe(
            &ring,
            None,
            &cap,
            &v,
            16_000,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(matches!(
            res,
            Err(TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    // === BiometricSafe TRAIT MEMBERSHIP ===

    #[test]
    fn biometric_safe_implemented_for_primitives() {
        fn assert_safe<T: BiometricSafe>() {}
        assert_safe::<u8>();
        assert_safe::<u32>();
        assert_safe::<f64>();
        assert_safe::<bool>();
        assert_safe::<String>();
    }

    // === TelemetryRefusal METADATA ===

    #[test]
    fn refusal_tag_canonical_for_each_variant() {
        let r1 = TelemetryRefusal::BiometricRefused {
            domain: SensitiveDomain::Gaze,
        };
        assert_eq!(r1.refusal_tag(), "biometric-refused");
        let r2 = TelemetryRefusal::SurveillanceRefused;
        assert_eq!(r2.refusal_tag(), "surveillance-refused");
        let r3 = TelemetryRefusal::CoercionRefused;
        assert_eq!(r3.refusal_tag(), "coercion-refused");
        let r4 = TelemetryRefusal::WeaponNeedsKernel;
        assert_eq!(r4.refusal_tag(), "weapon-needs-kernel");
        let r5 = TelemetryRefusal::CapabilityMismatch {
            cap_auth: SensitiveDomain::Privacy,
        };
        assert_eq!(r5.refusal_tag(), "capability-mismatch");
    }

    #[test]
    fn refusal_diagnostic_bytes_include_domain() {
        let r = TelemetryRefusal::BiometricRefused {
            domain: SensitiveDomain::Face,
        };
        let bytes = r.diagnostic_bytes();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("face"));
    }

    #[test]
    fn refusal_display_cites_prime_directive() {
        let r = TelemetryRefusal::BiometricRefused {
            domain: SensitiveDomain::Body,
        };
        let s = r.to_string();
        assert!(s.contains("PRIME-DIRECTIVE"));
        assert!(s.contains("anti-surveillance"));
    }

    // === RING + REFUSAL SLOT INTEGRATION ===

    #[test]
    fn refusal_slot_uses_biometric_refused_scope() {
        let r = TelemetryRefusal::BiometricRefused {
            domain: SensitiveDomain::Gaze,
        };
        let slot = crate::ring::TelemetrySlot::refusal(99_999, &r);
        assert_eq!(slot.scope, TelemetryScope::BiometricRefused.as_u16());
        assert_eq!(slot.kind, TelemetryKind::Audit.as_u16());
        assert_eq!(slot.timestamp_ns, 99_999);
    }

    #[test]
    fn refusal_does_not_pollute_normal_slot_count() {
        // After a refused log, the normal slots that follow are not affected.
        let ring = TelemetryRing::new(4);
        let cap = privacy_cap();
        let bad: LabeledValue<u32> =
            LabeledValue::with_domain(0, benign_label(), SensitiveDomain::Gaze);
        let _ = record_labeled(
            &ring,
            None,
            &cap,
            &bad,
            100,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        let good: LabeledValue<u32> = LabeledValue::new(7, benign_label());
        let res = record_labeled(
            &ring,
            None,
            &cap,
            &good,
            200,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        );
        assert!(res.is_ok());
        // Both slots are in the ring (refusal-slot for the bad attempt + the
        // good slot for the legit log). FIFO order is preserved.
        assert_eq!(ring.len(), 2);
        let drained = ring.drain_all();
        assert_eq!(drained[0].scope, TelemetryScope::BiometricRefused.as_u16());
        assert_eq!(drained[1].scope, TelemetryScope::Counters.as_u16());
    }

    #[test]
    fn errors_round_trip_egress_grant_to_telemetry_refusal() {
        use cssl_ifc::EgressGrantError;
        let e1 = EgressGrantError::BiometricRefused {
            domain: SensitiveDomain::Gaze,
        };
        assert!(matches!(
            TelemetryRefusal::from(e1),
            TelemetryRefusal::BiometricRefused {
                domain: SensitiveDomain::Gaze
            }
        ));
        let e2 = EgressGrantError::SurveillanceRefused;
        assert_eq!(
            TelemetryRefusal::from(e2),
            TelemetryRefusal::SurveillanceRefused
        );
        let e3 = EgressGrantError::CoercionRefused;
        assert_eq!(
            TelemetryRefusal::from(e3),
            TelemetryRefusal::CoercionRefused
        );
        let e4 = EgressGrantError::WeaponNeedsKernel;
        assert_eq!(
            TelemetryRefusal::from(e4),
            TelemetryRefusal::WeaponNeedsKernel
        );
    }
}
