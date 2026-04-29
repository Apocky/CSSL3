//! Severity classification (mocked from `cssl-error` until T11-D155 lands).
//!
//! § INTEGRATION-POINT (T11-D155 cssl-error) :
//!   When `cssl-error` lands the canonical [`Severity`] enum + [`SourceLocation`]
//!   newtype, this module's local definitions are REPLACED with re-exports :
//!   ```ignore
//!   pub use cssl_error::{Severity, SourceLocation};
//!   ```
//!   The wire-shape is byte-equal to spec § 1.3 — variant order matches. The
//!   cssl-log crate does NOT depend on cssl-error today (per slice-prompt
//!   "MOCK via test-double + trait-impl ; document integration-point"). Once
//!   D155 merges, the swap is purely additive (no API break).
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.3 (severity-table).

use crate::path_hash_field::PathHashField;

/// Severity classification per spec § 1.3.
///
/// Variants ordered low-to-high so `<=` comparison is the natural filter
/// predicate (`level <= Severity::Info` enables Trace/Debug/Info).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Per-frame events ; off-by-default ; opt-in via cap-token.
    Trace,
    /// Verbose dev-info ; off in release.
    Debug,
    /// Notable event ; not an issue.
    Info,
    /// Recoverable + indicates issue.
    Warning,
    /// Unrecoverable but engine continues ; degraded-mode.
    Error,
    /// Engine cannot continue ; halt-trigger.
    Fatal,
}

impl Severity {
    /// Stable short-name for sink encoding.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warn",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }

    /// Stable u8 encoding for binary wire-format.
    ///
    /// W! N! reorder ⟵ binary-format consumers pin discriminants.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Trace => 0,
            Self::Debug => 1,
            Self::Info => 2,
            Self::Warning => 3,
            Self::Error => 4,
            Self::Fatal => 5,
        }
    }

    /// Decode a u8 back into [`Severity`]. Returns `None` on out-of-range.
    #[must_use]
    pub const fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Trace),
            1 => Some(Self::Debug),
            2 => Some(Self::Info),
            3 => Some(Self::Warning),
            4 => Some(Self::Error),
            5 => Some(Self::Fatal),
            _ => None,
        }
    }

    /// Single-char glyph for CSL-glyph human-readable line format.
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

    /// True for Error + Fatal — these severities are EXEMPT from rate-limit
    /// (spec § 2.5 "we do NOT silence errors").
    #[must_use]
    pub const fn is_rate_limit_exempt(self) -> bool {
        matches!(self, Self::Error | Self::Fatal)
    }

    /// Default per-frame emission cap per spec § 2.5. `u32::MAX` = no-cap.
    #[must_use]
    pub const fn default_per_frame_cap(self) -> u32 {
        match self {
            Self::Trace => 64,
            Self::Debug => 256,
            Self::Info => 1024,
            Self::Warning => 4096,
            Self::Error | Self::Fatal => u32::MAX,
        }
    }

    /// Iterate all 6 variants in canonical order — used by tests + bitfield
    /// installation in [`crate::enabled`].
    #[must_use]
    pub const fn all() -> [Self; 6] {
        [
            Self::Trace,
            Self::Debug,
            Self::Info,
            Self::Warning,
            Self::Error,
            Self::Fatal,
        ]
    }
}

/// Source-loc for a log emission. Spec § 1.4 — `file_path_hash` is the
/// PATH-HASH (D130), never raw `&str`/`&Path`.
///
/// § INTEGRATION-POINT : T11-D155 may move this struct into `cssl-error`
/// verbatim ; the field-set is identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    /// 32-byte BLAKE3-salted path-hash via [`cssl_telemetry::PathHasher`].
    pub file_path_hash: PathHashField,
    /// Source line number (1-based).
    pub line: u32,
    /// Source column number (1-based).
    pub column: u32,
}

impl SourceLocation {
    /// Construct from already-hashed path. The constructor accepts ONLY a
    /// `PathHashField` — there is no `&str`/`&Path` overload, structurally
    /// preventing raw-path leakage at the type level (D130).
    #[must_use]
    pub const fn new(file_path_hash: PathHashField, line: u32, column: u32) -> Self {
        Self {
            file_path_hash,
            line,
            column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Severity;

    #[test]
    fn severity_ordering_low_to_high() {
        assert!(Severity::Trace < Severity::Debug);
        assert!(Severity::Debug < Severity::Info);
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        assert!(Severity::Error < Severity::Fatal);
    }

    #[test]
    fn severity_as_str_canonical() {
        assert_eq!(Severity::Trace.as_str(), "trace");
        assert_eq!(Severity::Debug.as_str(), "debug");
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Warning.as_str(), "warn");
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Fatal.as_str(), "fatal");
    }

    #[test]
    fn severity_as_u8_round_trip() {
        for s in Severity::all() {
            assert_eq!(Severity::from_u8(s.as_u8()), Some(s));
        }
    }

    #[test]
    fn severity_from_u8_rejects_out_of_range() {
        assert_eq!(Severity::from_u8(6), None);
        assert_eq!(Severity::from_u8(255), None);
    }

    #[test]
    fn severity_glyph_unique() {
        let glyphs: Vec<_> = Severity::all().iter().map(|s| s.glyph()).collect();
        let mut sorted = glyphs.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), glyphs.len(), "glyphs must be unique");
    }

    #[test]
    fn severity_rate_limit_exempt_only_error_fatal() {
        assert!(!Severity::Trace.is_rate_limit_exempt());
        assert!(!Severity::Debug.is_rate_limit_exempt());
        assert!(!Severity::Info.is_rate_limit_exempt());
        assert!(!Severity::Warning.is_rate_limit_exempt());
        assert!(Severity::Error.is_rate_limit_exempt());
        assert!(Severity::Fatal.is_rate_limit_exempt());
    }

    #[test]
    fn default_per_frame_caps_match_spec() {
        assert_eq!(Severity::Trace.default_per_frame_cap(), 64);
        assert_eq!(Severity::Debug.default_per_frame_cap(), 256);
        assert_eq!(Severity::Info.default_per_frame_cap(), 1024);
        assert_eq!(Severity::Warning.default_per_frame_cap(), 4096);
        assert_eq!(Severity::Error.default_per_frame_cap(), u32::MAX);
        assert_eq!(Severity::Fatal.default_per_frame_cap(), u32::MAX);
    }

    #[test]
    fn severity_all_has_six_variants() {
        assert_eq!(Severity::all().len(), 6);
    }

    #[test]
    fn severity_clone_copy_works() {
        let s = Severity::Info;
        let t = s;
        assert_eq!(s, t);
    }

    #[test]
    fn severity_hash_works() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        for s in Severity::all() {
            set.insert(s);
        }
        assert_eq!(set.len(), 6);
    }
}
