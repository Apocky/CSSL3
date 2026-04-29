//! Log macro family : `trace! / debug! / info! / warn! / error! / fatal! /
//! log!`.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2.2 + § 2.3.
//!
//! § COST-MODEL :
//!   - `enabled(severity, subsystem)` is checked FIRST in every macro
//!     expansion ⟵ disabled-call cost ≈ 2ns (single AtomicU64 load + bit-test).
//!   - format_args! is NOT evaluated when disabled — the compiler folds
//!     the `if !enabled { return ... }` short-circuit through the macro.
//!
//! § INVOCATION FORMS :
//! ```ignore
//! info!(SubsystemTag::Render, "frame {n} took {ms}ms", n = 42, ms = 16);
//! warn!(SubsystemTag::Telemetry, "ring overflow count={}", count);
//! error!(SubsystemTag::Render, "render stage failed");
//! log!(Severity::Info, SubsystemTag::Engine, "decision tree depth {d}", d = 5);
//! ```
//!
//! § PATH-HASH FOR SOURCE-LOC :
//!   The macro captures `file!()` + `line!()` + `column!()` and looks up
//!   the path-hash via the engine's installed [`crate::source_hasher`]
//!   thread-local. If no engine-installed hasher is present, the path-
//!   hash is `PathHashField::zero()` (a sentinel) ; production code
//!   installs the hasher at engine-init.

// ───────────────────────────────────────────────────────────────────────
// § Public macros
// ───────────────────────────────────────────────────────────────────────

/// Generic emission macro. Lower-level form ; prefer the level-specific
/// macros (`info!`, etc.) when available.
///
/// # Forms
/// * `log!(Severity::X, SubsystemTag::Y, "format {a}", a = b)`
/// * `log!(Severity::X, SubsystemTag::Y, "literal-format-no-args")`
#[macro_export]
macro_rules! log {
    ($severity:expr, $subsystem:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        let __severity = $severity;
        let __subsystem = $subsystem;
        if $crate::enabled::enabled(__severity, __subsystem) {
            let __source = $crate::macros::source_location_here(file!(), line!(), column!());
            let __ctx = $crate::Context::at_now(__severity, __subsystem, __source);
            let __msg = format!($fmt $(, $($arg)*)?);
            $crate::emit::emit_structured(&__ctx, __msg, Vec::new());
        }
    }};
}

/// Trace-level emission. OFF by default ; enable via cap-token.
#[macro_export]
macro_rules! trace {
    ($subsystem:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        $crate::log!($crate::Severity::Trace, $subsystem, $fmt $(, $($arg)*)?);
    }};
}

/// Debug-level emission. OFF in release builds.
#[macro_export]
macro_rules! debug {
    ($subsystem:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        $crate::log!($crate::Severity::Debug, $subsystem, $fmt $(, $($arg)*)?);
    }};
}

/// Info-level emission. ON by default.
#[macro_export]
macro_rules! info {
    ($subsystem:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        $crate::log!($crate::Severity::Info, $subsystem, $fmt $(, $($arg)*)?);
    }};
}

/// Warning-level emission. ON by default ; recoverable issue.
#[macro_export]
macro_rules! warn {
    ($subsystem:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        $crate::log!($crate::Severity::Warning, $subsystem, $fmt $(, $($arg)*)?);
    }};
}

/// Error-level emission. ON by default ; unrecoverable but engine continues.
/// EXEMPT from rate-limits.
#[macro_export]
macro_rules! error {
    ($subsystem:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        $crate::log!($crate::Severity::Error, $subsystem, $fmt $(, $($arg)*)?);
    }};
}

/// Fatal-level emission. ON by default ; engine cannot continue ; halt-trigger.
/// EXEMPT from rate-limits.
#[macro_export]
macro_rules! fatal {
    ($subsystem:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        $crate::log!($crate::Severity::Fatal, $subsystem, $fmt $(, $($arg)*)?);
    }};
}

// ───────────────────────────────────────────────────────────────────────
// § Helper exposed for macro-expansion (re-exported into crate root)
// ───────────────────────────────────────────────────────────────────────

use crate::path_hash_field::PathHashField;
use crate::severity::SourceLocation;

/// Build a [`SourceLocation`] for a macro-call-site. Uses the thread-
/// local [`SOURCE_HASHER`] if installed ; otherwise emits a zero-hash
/// sentinel.
///
/// § STABILITY : the `path` argument MUST be a `&'static str` (typically
/// produced by `file!()` macro). The hasher computes the BLAKE3-salt
/// hash via [`cssl_telemetry::PathHasher::hash_str`] — D130 enforced.
#[must_use]
pub fn source_location_here(path: &'static str, line: u32, col: u32) -> SourceLocation {
    let hash = source_hash(path);
    SourceLocation::new(hash, line, col)
}

thread_local! {
    /// Thread-local PathHasher used by macro-expansion to convert
    /// `file!()` into a `PathHashField`. Installed by engine-init via
    /// [`install_source_hasher`].
    static SOURCE_HASHER: std::cell::RefCell<Option<cssl_telemetry::PathHasher>> =
        const { std::cell::RefCell::new(None) };
}

/// Install a [`cssl_telemetry::PathHasher`] for the current thread. Called
/// by engine-init at startup. Each thread that emits logs MUST install
/// its own hasher (or use the process-default once the cssl-error
/// process-global hasher lands at T11-D155).
pub fn install_source_hasher(hasher: cssl_telemetry::PathHasher) {
    SOURCE_HASHER.with(|cell| {
        *cell.borrow_mut() = Some(hasher);
    });
}

/// Read access to the thread-local hasher.
#[must_use]
pub fn current_source_hasher() -> Option<cssl_telemetry::PathHasher> {
    SOURCE_HASHER.with(|cell| cell.borrow().clone())
}

/// Compute the [`PathHashField`] for a given source-path. Used internally
/// by macro-expansion. Returns `PathHashField::zero()` if no hasher is
/// installed (spec § 7.6 "log-before-engine-init uses frame zero" —
/// extends to source-loc as well).
#[must_use]
pub fn source_hash(path: &str) -> PathHashField {
    SOURCE_HASHER.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|h| PathHashField::from_path_hash(h.hash_str(path)))
            .unwrap_or(PathHashField::zero())
    })
}

#[cfg(test)]
pub(crate) fn reset_source_hasher_for_test() {
    SOURCE_HASHER.with(|cell| *cell.borrow_mut() = None);
}

// ───────────────────────────────────────────────────────────────────────
// § Macro tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        current_source_hasher, install_source_hasher, reset_source_hasher_for_test, source_hash,
        source_location_here,
    };
    use crate::path_hash_field::PathHashField;
    use cssl_telemetry::PathHasher;

    #[test]
    fn source_hash_zero_without_installed_hasher() {
        reset_source_hasher_for_test();
        let h = source_hash("/foo");
        assert_eq!(h, PathHashField::zero());
    }

    #[test]
    fn source_hash_uses_installed_hasher() {
        reset_source_hasher_for_test();
        let hasher = PathHasher::from_seed([0u8; 32]);
        install_source_hasher(hasher.clone());
        let h = source_hash("/foo");
        assert_ne!(h, PathHashField::zero());
        // Same path → same hash within thread.
        let h2 = source_hash("/foo");
        assert_eq!(h, h2);
    }

    #[test]
    fn source_hash_distinct_per_path() {
        reset_source_hasher_for_test();
        install_source_hasher(PathHasher::from_seed([0u8; 32]));
        let a = source_hash("/a");
        let b = source_hash("/b");
        assert_ne!(a, b);
    }

    #[test]
    fn source_location_here_combines_path_line_col() {
        reset_source_hasher_for_test();
        let loc = source_location_here("/foo.rs", 42, 7);
        assert_eq!(loc.line, 42);
        assert_eq!(loc.column, 7);
    }

    #[test]
    fn current_source_hasher_visible_after_install() {
        reset_source_hasher_for_test();
        assert!(current_source_hasher().is_none());
        install_source_hasher(PathHasher::from_seed([0u8; 32]));
        assert!(current_source_hasher().is_some());
    }
}
