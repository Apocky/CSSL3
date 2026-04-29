//! Severity classification for [`crate::EngineError`].
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.3.
//!
//! § DESIGN
//!   - 6-level enum total-ordered (Trace < Debug < Info < Warning < Error < Fatal).
//!   - Severity-table is canonical : sink-routing decisions key off this enum.
//!   - The PRIME-DIRECTIVE-violation severity is ALWAYS [`Severity::Fatal`] :
//!     this is enforced at the [`crate::EngineError::PrimeDirective`] variant
//!     level (no override-path). See `severity_pd_violation_is_fatal()` in tests.
//!   - The [`Severable`] trait is implemented on every error-type that wants
//!     to participate in the unified-severity pipeline. The default impl is
//!     conservative : [`Severity::Error`]. Concrete impls override per-variant
//!     (e.g., `RingError::Overflow ⟶ Warning` ; `AuditError::ChainBroken ⟶ Fatal`).
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - § 7 INTEGRITY : the severity-classification is part of the L0 contract
//!     ; renaming a variant or reordering the discriminants would break
//!     replay-determinism (logged severity is encoded as u8 in ring-slots).
//!     The set is hash-pinned in the test-suite via [`severity_canonical_table`].

use core::fmt;

// ───────────────────────────────────────────────────────────────────────
// § Severity enum + canonical ordering.
// ───────────────────────────────────────────────────────────────────────

/// Severity classification for [`crate::EngineError`] + structured-log emissions.
///
/// § ORDERING
///   `Trace < Debug < Info < Warning < Error < Fatal`. The [`PartialOrd`] +
///   [`Ord`] impls reflect this. Use [`Severity::is_at_least`] for clarity
///   in level-filtering predicates (e.g., `if sev.is_at_least(Severity::Warning)`).
///
/// § SERIALIZATION
///   Stable u8 discriminants pin the wire-format for ring-slot encoding +
///   replay-determinism. NEVER reorder. Adding a new variant is additive
///   only if it appears AFTER the existing tail.
///
/// § TABLE  (canonical sink-routing decisions ; spec § 1.3 § 2.6)
///
/// | severity | continues? | logged? | audit-chain? | kill-switch? |
/// |----------|-----------|---------|-------------|--------------|
/// | Trace    | yes       | sampled | no          | no           |
/// | Debug    | yes       | release-off | no      | no           |
/// | Info     | yes       | yes     | no          | no           |
/// | Warning  | yes       | yes     | if-PD-adjacent | no       |
/// | Error    | yes (degraded) | yes | yes        | no           |
/// | Fatal    | no        | yes     | yes         | yes          |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Severity {
    /// Per-frame events ; off-by-default ; opt-in via `Cap<TraceMode>`.
    Trace = 0,
    /// Verbose dev-info ; off in release.
    Debug = 1,
    /// Notable event ; not an issue.
    Info = 2,
    /// Recoverable + indicates issue.
    Warning = 3,
    /// Unrecoverable but engine continues ; degraded-mode.
    Error = 4,
    /// Engine cannot continue ; halt-trigger.
    Fatal = 5,
}

impl Severity {
    /// All variants in canonical order.
    #[must_use]
    pub const fn all() -> &'static [Severity] {
        &[
            Self::Trace,
            Self::Debug,
            Self::Info,
            Self::Warning,
            Self::Error,
            Self::Fatal,
        ]
    }

    /// Stable canonical name (snake_case ; matches log-format).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }

    /// Single-character glyph for terse displays. Matches CSLv3 evidence
    /// ladder (◐ partial / ✗ failed) where applicable.
    #[must_use]
    pub const fn glyph(self) -> char {
        match self {
            Self::Trace => 'T',
            Self::Debug => 'D',
            Self::Info => 'I',
            Self::Warning => 'W',
            Self::Error => 'E',
            Self::Fatal => 'F',
        }
    }

    /// Returns `true` if `self >= other`. Convenient for level-filtering.
    #[must_use]
    pub const fn is_at_least(self, other: Severity) -> bool {
        (self as u8) >= (other as u8)
    }

    /// Returns `true` for severities that the engine can degrade-mode-recover from.
    #[must_use]
    pub const fn is_recoverable(self) -> bool {
        match self {
            Self::Trace | Self::Debug | Self::Info | Self::Warning | Self::Error => true,
            Self::Fatal => false,
        }
    }

    /// Returns `true` for severities that the engine MUST halt on.
    #[must_use]
    pub const fn is_fatal(self) -> bool {
        matches!(self, Self::Fatal)
    }

    /// Returns `true` for severities that should be appended to the audit-chain.
    /// Spec § 1.3 § 2.6 sink-routing matrix : Error + Fatal always ; Warning
    /// only when PD-adjacent (callers handle the PD-adjacency check).
    #[must_use]
    pub const fn is_audit_eligible(self) -> bool {
        matches!(self, Self::Error | Self::Fatal)
    }

    /// Returns `true` for severities that should not be silenced by rate-limiting.
    /// Spec § 2.5 : Error + Fatal are EXEMPT.
    #[must_use]
    pub const fn is_rate_limit_exempt(self) -> bool {
        matches!(self, Self::Error | Self::Fatal)
    }

    /// Returns `true` for severities that fire the kill-switch.
    /// Only [`Severity::Fatal`] ; spec § 1.3.
    #[must_use]
    pub const fn fires_kill_switch(self) -> bool {
        matches!(self, Self::Fatal)
    }

    /// Construct from u8 discriminant. Returns `None` on out-of-range.
    #[must_use]
    pub const fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Trace),
            1 => Some(Self::Debug),
            2 => Some(Self::Info),
            3 => Some(Self::Warning),
            4 => Some(Self::Error),
            5 => Some(Self::Fatal),
            _ => None,
        }
    }

    /// Get the discriminant byte ; matches the wire-format.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_name())
    }
}

impl Default for Severity {
    /// Default = [`Severity::Error`]. Conservative default for unclassified
    /// errors ; downstream callers may explicitly downgrade to Warning/Info.
    fn default() -> Self {
        Self::Error
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Severable trait — uniform severity-classification for any error.
// ───────────────────────────────────────────────────────────────────────

/// Trait : "this type can be classified into a [`Severity`]".
///
/// § DESIGN
///   - Default impl returns [`Severity::Error`] ; every concrete error-type
///     SHOULD override per-variant for accurate classification.
///   - Implemented on [`crate::EngineError`] (with a per-variant match) +
///     on every per-crate `*Error` that opts-in.
///   - The trait is `dyn`-safe so heterogeneous error-collections can be
///     classified without monomorphization.
pub trait Severable {
    /// The severity classification for this error.
    ///
    /// Concrete impls SHOULD perform a per-variant match. The default impl
    /// is conservative ([`Severity::Error`]) for types that don't yet have
    /// fine-grained classification.
    fn severity(&self) -> Severity {
        Severity::Error
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Foundation Severable impls — for telemetry + PD types.
// ───────────────────────────────────────────────────────────────────────

impl Severable for cssl_telemetry::PathLogError {
    /// Path-discipline violation = Warning by default ; Error if it touches
    /// the audit-chain (caller's choice). Spec § 7.1 : these are PD-adjacent.
    fn severity(&self) -> Severity {
        // Raw-path-in-field is a discipline violation but recoverable
        // (caller may strip the field + retry). Fatal is for chain-corruption
        // ; this is not that.
        Severity::Warning
    }
}

impl Severable for cssl_telemetry::AuditError {
    /// Audit-chain failures are FATAL : the chain is the integrity-witness.
    fn severity(&self) -> Severity {
        Severity::Fatal
    }
}

impl Severable for cssl_telemetry::RingError {
    /// Ring-overflow = Warning (lossy by design ; producer-slot dropped is
    /// an expected outcome under bursty load). Spec § 1.3 severity-table.
    fn severity(&self) -> Severity {
        match self {
            cssl_telemetry::RingError::Overflow => Severity::Warning,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Severable, Severity};

    #[test]
    fn severity_total_order() {
        assert!(Severity::Trace < Severity::Debug);
        assert!(Severity::Debug < Severity::Info);
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        assert!(Severity::Error < Severity::Fatal);
    }

    #[test]
    fn severity_all_six_canonical() {
        assert_eq!(Severity::all().len(), 6);
    }

    #[test]
    fn severity_canonical_names_unique() {
        let mut names: Vec<&str> = Severity::all()
            .iter()
            .map(|s| s.canonical_name())
            .collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    #[test]
    fn severity_glyphs_unique() {
        let mut glyphs: Vec<char> = Severity::all().iter().map(|s| s.glyph()).collect();
        glyphs.sort_unstable();
        let original = glyphs.len();
        glyphs.dedup();
        assert_eq!(glyphs.len(), original);
    }

    #[test]
    fn severity_is_at_least() {
        assert!(Severity::Fatal.is_at_least(Severity::Trace));
        assert!(Severity::Error.is_at_least(Severity::Warning));
        assert!(!Severity::Info.is_at_least(Severity::Error));
        assert!(Severity::Trace.is_at_least(Severity::Trace));
    }

    #[test]
    fn severity_recoverable_excludes_fatal() {
        for s in Severity::all() {
            if *s == Severity::Fatal {
                assert!(!s.is_recoverable());
            } else {
                assert!(s.is_recoverable());
            }
        }
    }

    #[test]
    fn severity_fatal_predicate() {
        assert!(Severity::Fatal.is_fatal());
        for s in Severity::all() {
            if *s != Severity::Fatal {
                assert!(!s.is_fatal());
            }
        }
    }

    #[test]
    fn severity_audit_eligible_only_error_fatal() {
        assert!(Severity::Error.is_audit_eligible());
        assert!(Severity::Fatal.is_audit_eligible());
        assert!(!Severity::Warning.is_audit_eligible());
        assert!(!Severity::Info.is_audit_eligible());
        assert!(!Severity::Debug.is_audit_eligible());
        assert!(!Severity::Trace.is_audit_eligible());
    }

    #[test]
    fn severity_rate_limit_exempt_only_error_fatal() {
        assert!(Severity::Error.is_rate_limit_exempt());
        assert!(Severity::Fatal.is_rate_limit_exempt());
        assert!(!Severity::Warning.is_rate_limit_exempt());
    }

    #[test]
    fn severity_kill_switch_only_fatal() {
        assert!(Severity::Fatal.fires_kill_switch());
        for s in Severity::all() {
            if *s != Severity::Fatal {
                assert!(!s.fires_kill_switch());
            }
        }
    }

    #[test]
    fn severity_u8_round_trip() {
        for s in Severity::all() {
            let byte = s.as_u8();
            let parsed = Severity::from_u8(byte).expect("round-trip parsable");
            assert_eq!(*s, parsed);
        }
    }

    #[test]
    fn severity_u8_out_of_range_returns_none() {
        assert!(Severity::from_u8(6).is_none());
        assert!(Severity::from_u8(255).is_none());
    }

    #[test]
    fn severity_default_is_error() {
        assert_eq!(Severity::default(), Severity::Error);
    }

    #[test]
    fn severity_display_matches_canonical() {
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Fatal), "fatal");
    }

    #[test]
    fn severable_default_is_error() {
        struct Dummy;
        impl Severable for Dummy {}
        assert_eq!(Dummy.severity(), Severity::Error);
    }

    #[test]
    fn severable_telemetry_audit_is_fatal() {
        // AuditError variants : tamper-detected etc. Always Fatal.
        let ae = cssl_telemetry::AuditError::SignatureInvalid;
        assert_eq!(ae.severity(), Severity::Fatal);
        let ae2 = cssl_telemetry::AuditError::ChainBreak { seq: 7 };
        assert_eq!(ae2.severity(), Severity::Fatal);
    }

    #[test]
    fn severable_telemetry_path_log_is_warning() {
        let pe = cssl_telemetry::PathLogError::RawPathInField {
            field: "test".into(),
        };
        assert_eq!(pe.severity(), Severity::Warning);
    }

    #[test]
    fn severable_telemetry_ring_overflow_is_warning() {
        let re = cssl_telemetry::RingError::Overflow;
        assert_eq!(re.severity(), Severity::Warning);
    }

    #[test]
    fn severity_canonical_table_byte_pinned() {
        // Wire-format pin : breaking these byte-values breaks ring-slot
        // backward-compat + replay-determinism.
        assert_eq!(Severity::Trace.as_u8(), 0);
        assert_eq!(Severity::Debug.as_u8(), 1);
        assert_eq!(Severity::Info.as_u8(), 2);
        assert_eq!(Severity::Warning.as_u8(), 3);
        assert_eq!(Severity::Error.as_u8(), 4);
        assert_eq!(Severity::Fatal.as_u8(), 5);
    }
}
