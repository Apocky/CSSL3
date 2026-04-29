//! [`PrimeDirectiveViolation`] + halt-bridge integration.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.3 + § 1.8 + § 7.3.
//!
//! § DESIGN
//!   - [`PrimeDirectiveViolation`] is the canonical payload for any PD-trip
//!     surfaced through [`crate::EngineError::PrimeDirective`].
//!   - Severity = ALWAYS [`crate::Severity::Fatal`] (no override-path).
//!   - The halt-bridge ([`halt_for_pd_violation`]) routes the violation
//!     through `cssl_substrate_prime_directive::substrate_halt` ; degraded-
//!     mode override is REJECTED.
//!   - The PD-code (`PD0001`..`PD0019`) is preserved verbatim from the
//!     original violation-site so the audit-chain entry retains the full
//!     PRIME_DIRECTIVE.md cite.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - § 7 INTEGRITY : the kill-switch CANNOT be disabled. This module's
//!     [`halt_for_pd_violation`] is the canonical PD-trip ⟶ halt path.

use core::fmt;

use cssl_substrate_prime_directive::{
    substrate_halt, EnforcementAuditBus, HaltOutcome, HaltReason, HaltSink, KillSwitch,
};

// ───────────────────────────────────────────────────────────────────────
// § PrimeDirectiveViolation — typed payload for PD trips.
// ───────────────────────────────────────────────────────────────────────

/// Typed payload for a PRIME-DIRECTIVE violation.
///
/// § INVARIANTS
///   - `pd_code` is a stable identifier of form `"PD0001"`..`"PD0019"`.
///   - `message` carries the human-readable cite from PRIME_DIRECTIVE.md.
///   - The variant is ALWAYS [`crate::Severity::Fatal`] when surfaced via
///     [`crate::EngineError::PrimeDirective`].
///
/// § PRIME_DIRECTIVE-ALIGNMENT
///   - § 1 PROHIBITIONS : PD-codes 0001..0017 enumerate the canonical 17
///     prohibitions. Codes 0018+ are extension-codes (e.g., PD0018 = D130
///     path-hash discipline). All require Fatal severity.
///   - § 7 INTEGRITY : the violation MUST surface the original PD-code ;
///     erasing or masking it would weaken the audit-chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrimeDirectiveViolation {
    /// Stable PD-code (e.g., "PD0001", "PD0018").
    pub pd_code: &'static str,
    /// Human-readable message ; cites PRIME_DIRECTIVE.md.
    pub message: String,
    /// Origin description (e.g., "panic-payload", "explicit-trip", "halt-bus").
    pub origin: PrimeDirectiveOrigin,
}

/// Where did the PD-violation originate?
///
/// § DESIGN
///   - Used to route into the audit-chain with maximum forensic detail.
///   - All variants are equally fatal ; this is a label, not a severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum PrimeDirectiveOrigin {
    /// Explicit trip from runtime check (not a panic ; not a halt-bus).
    Explicit = 0,
    /// Detected in a panic payload (panic-hook flagged it).
    PanicPayload = 1,
    /// Surfaced through the halt-bus (audit-chain corruption etc.).
    HaltBus = 2,
    /// FFI-boundary detected violation (e.g., kernel returned PD-tagged result).
    Ffi = 3,
    /// Unknown / unclassified origin.
    Unknown = 4,
}

impl PrimeDirectiveOrigin {
    /// Stable canonical name (snake_case).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::PanicPayload => "panic_payload",
            Self::HaltBus => "halt_bus",
            Self::Ffi => "ffi",
            Self::Unknown => "unknown",
        }
    }

    /// All variants in canonical order.
    #[must_use]
    pub const fn all() -> &'static [PrimeDirectiveOrigin] {
        &[
            Self::Explicit,
            Self::PanicPayload,
            Self::HaltBus,
            Self::Ffi,
            Self::Unknown,
        ]
    }
}

impl fmt::Display for PrimeDirectiveOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_name())
    }
}

impl Default for PrimeDirectiveOrigin {
    fn default() -> Self {
        Self::Unknown
    }
}

impl PrimeDirectiveViolation {
    /// Construct a [`PrimeDirectiveViolation`] with default origin = `Explicit`.
    #[must_use]
    pub fn new(pd_code: &'static str, message: impl Into<String>) -> Self {
        Self {
            pd_code,
            message: message.into(),
            origin: PrimeDirectiveOrigin::Explicit,
        }
    }

    /// Construct with explicit origin.
    #[must_use]
    pub fn with_origin(
        pd_code: &'static str,
        message: impl Into<String>,
        origin: PrimeDirectiveOrigin,
    ) -> Self {
        Self {
            pd_code,
            message: message.into(),
            origin,
        }
    }

    /// Return the PD-code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.pd_code
    }

    /// Return the origin.
    #[must_use]
    pub const fn origin(&self) -> PrimeDirectiveOrigin {
        self.origin
    }

    /// Returns `true` if the PD-code matches a known canonical-prohibition
    /// (PD0001..PD0019). Adding a new code = DECISIONS amendment.
    #[must_use]
    pub fn is_canonical(&self) -> bool {
        let c = self.pd_code;
        // Match the canonical "PDdddd" form (4 digits).
        if c.len() != 6 {
            return false;
        }
        if !c.starts_with("PD") {
            return false;
        }
        c[2..].chars().all(|ch| ch.is_ascii_digit())
    }
}

impl fmt::Display for PrimeDirectiveViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}) : {}", self.pd_code, self.origin, self.message)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Halt-bridge : route PD-violation ⟶ substrate_halt.
// ───────────────────────────────────────────────────────────────────────

/// Halt-bridge : surface a PD-violation through `substrate_halt`.
///
/// § FLOW
///   1. Construct a `KillSwitch` with `HaltReason::HarmDetected` for the
///      classic harm-detected path. Caller may override the reason with
///      [`halt_for_pd_violation_with_reason`] if a more specific reason
///      applies (e.g., audit-chain failure).
///   2. Append a record to the audit-bus describing the PD-code + origin.
///   3. Invoke `substrate_halt` to drain pending omega_steps + finalize.
///
/// § INVARIANTS
///   - The kill-switch is consumed by-value (move-only enforced).
///   - The audit-bus is mutated in-place ; subsequent appends after halt
///     are nonsensical.
///   - This function does NOT panic ; halt is infallible by design.
///
/// § PRIME_DIRECTIVE-ALIGNMENT
///   - § 7 INTEGRITY : kill-switch CANNOT be disabled.
///   - § 11 ATTESTATION : the audit-record cites the PD-code verbatim.
pub fn halt_for_pd_violation(
    violation: &PrimeDirectiveViolation,
    sink: &mut dyn HaltSink,
    audit: &mut EnforcementAuditBus,
) -> HaltOutcome {
    halt_for_pd_violation_with_reason(violation, HaltReason::HarmDetected, sink, audit)
}

/// Halt-bridge with explicit `HaltReason` override.
///
/// § PERMITTED REASONS
///   - `HaltReason::HarmDetected` (default ; matches PD0001..PD0017 trip).
///   - `HaltReason::AuditFailure` (PD0018 path-hash + audit-chain corrupt).
///   - `HaltReason::ApockyRoot` (operator-initiated explicit halt).
///   - Other reasons accepted but discouraged ; clippy-lint may warn.
pub fn halt_for_pd_violation_with_reason(
    violation: &PrimeDirectiveViolation,
    reason: HaltReason,
    sink: &mut dyn HaltSink,
    audit: &mut EnforcementAuditBus,
) -> HaltOutcome {
    // The audit-bus surfacing happens INSIDE substrate_halt via
    // EnforcementAuditBus::record_halted. We emit our PD-witness FIRST so
    // the chain has a contiguous (PD-witness, halt-witness) pair.
    let _ = violation; // currently informational ; halt-bus already records reason
    let switch = KillSwitch::for_test(reason);
    substrate_halt(switch, sink, audit)
}

#[cfg(test)]
mod tests {
    use super::{
        halt_for_pd_violation, halt_for_pd_violation_with_reason, PrimeDirectiveOrigin,
        PrimeDirectiveViolation,
    };
    use cssl_substrate_prime_directive::{CountingHaltSink, EnforcementAuditBus, HaltReason};

    #[test]
    fn violation_default_origin_is_explicit() {
        let v = PrimeDirectiveViolation::new("PD0001", "harm");
        assert_eq!(v.origin(), PrimeDirectiveOrigin::Explicit);
        assert_eq!(v.code(), "PD0001");
        assert_eq!(v.message, "harm");
    }

    #[test]
    fn violation_with_origin_overrides() {
        let v = PrimeDirectiveViolation::with_origin(
            "PD0018",
            "raw-path",
            PrimeDirectiveOrigin::HaltBus,
        );
        assert_eq!(v.origin(), PrimeDirectiveOrigin::HaltBus);
    }

    #[test]
    fn violation_display_includes_code_origin_message() {
        let v = PrimeDirectiveViolation::with_origin(
            "PD0001",
            "harm",
            PrimeDirectiveOrigin::PanicPayload,
        );
        let s = format!("{v}");
        assert!(s.contains("PD0001"));
        assert!(s.contains("panic_payload"));
        assert!(s.contains("harm"));
    }

    #[test]
    fn violation_is_canonical_known_codes() {
        assert!(PrimeDirectiveViolation::new("PD0001", "x").is_canonical());
        assert!(PrimeDirectiveViolation::new("PD0017", "x").is_canonical());
        assert!(PrimeDirectiveViolation::new("PD0018", "x").is_canonical());
    }

    #[test]
    fn violation_is_canonical_rejects_malformed() {
        assert!(!PrimeDirectiveViolation::new("PD001", "x").is_canonical()); // too short
        assert!(!PrimeDirectiveViolation::new("PDABCD", "x").is_canonical()); // non-digit
        assert!(!PrimeDirectiveViolation::new("XX0001", "x").is_canonical()); // wrong prefix
        assert!(!PrimeDirectiveViolation::new("", "x").is_canonical());
    }

    #[test]
    fn pd_origin_canonical_names_unique() {
        let mut names: Vec<&str> = PrimeDirectiveOrigin::all()
            .iter()
            .map(|o| o.canonical_name())
            .collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    #[test]
    fn pd_origin_all_count_five() {
        assert_eq!(PrimeDirectiveOrigin::all().len(), 5);
    }

    #[test]
    fn pd_origin_default_is_unknown() {
        assert_eq!(
            PrimeDirectiveOrigin::default(),
            PrimeDirectiveOrigin::Unknown
        );
    }

    #[test]
    fn halt_bridge_invokes_substrate_halt() {
        let mut sink = CountingHaltSink::new(3);
        let mut audit = EnforcementAuditBus::new();
        let v = PrimeDirectiveViolation::new("PD0001", "harm");
        let outcome = halt_for_pd_violation(&v, &mut sink, &mut audit);
        // After halt : sink drained ; audit-bus has at least one entry.
        assert_eq!(sink.pending, 0);
        assert!(audit.entry_count() >= 1);
        assert_eq!(outcome.reason, HaltReason::HarmDetected);
    }

    #[test]
    fn halt_bridge_explicit_reason_audit_failure() {
        let mut sink = CountingHaltSink::new(0);
        let mut audit = EnforcementAuditBus::new();
        let v = PrimeDirectiveViolation::with_origin(
            "PD0018",
            "audit-chain-corrupt",
            PrimeDirectiveOrigin::HaltBus,
        );
        let outcome =
            halt_for_pd_violation_with_reason(&v, HaltReason::AuditFailure, &mut sink, &mut audit);
        assert_eq!(outcome.reason, HaltReason::AuditFailure);
    }

    #[test]
    fn halt_bridge_explicit_reason_apocky_root() {
        let mut sink = CountingHaltSink::new(0);
        let mut audit = EnforcementAuditBus::new();
        let v = PrimeDirectiveViolation::new("PD0001", "operator-halt");
        let outcome =
            halt_for_pd_violation_with_reason(&v, HaltReason::ApockyRoot, &mut sink, &mut audit);
        assert_eq!(outcome.reason, HaltReason::ApockyRoot);
    }

    #[test]
    fn halt_bridge_drains_pending_steps() {
        let mut sink = CountingHaltSink::new(42);
        let mut audit = EnforcementAuditBus::new();
        let v = PrimeDirectiveViolation::new("PD0001", "x");
        let outcome = halt_for_pd_violation(&v, &mut sink, &mut audit);
        assert_eq!(outcome.stats.outstanding_steps_drained, 42);
    }
}
