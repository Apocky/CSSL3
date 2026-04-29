//! § Error type
//!
//! All fallible operations in this crate return [`SpecCoverageError`].
//! Errors are deliberately verbose : extraction is a build-time / dev-
//! time activity, so producing actionable messages outweighs the cost
//! of a slightly fatter error enum.

use thiserror::Error;

/// Failure modes raised by the spec-coverage tracker.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SpecCoverageError {
    /// A doc-comment line declared a `§ Omniverse …` marker that the
    /// extractor couldn't decompose into (path, section).
    #[error("malformed inline § marker on line {line}: {raw:?}")]
    MalformedMarker { line: usize, raw: String },

    /// A `#[spec_anchor(...)]` invocation cited zero anchor families.
    #[error("spec_anchor invocation missing all anchor keys (omniverse / spec / decision / section / citations)")]
    EmptyAnchor,

    /// Two anchors were registered for the same Rust path with
    /// conflicting impl_status. Stage-0 treats this as an extraction
    /// bug rather than a merge target.
    #[error("conflicting impl_status for anchor {path:?}: existing={existing:?}, new={new:?}")]
    ConflictingStatus {
        path: String,
        existing: String,
        new: String,
    },

    /// A test-name regex match decoded a `[crate]_[fn]_per_spec_[anchor]`
    /// shape that we couldn't map back onto a registered SpecAnchor.
    #[error("test name {test:?} cites unknown anchor {anchor:?}")]
    OrphanTestCitation { test: String, anchor: String },

    /// DECISIONS.md `spec-anchors:` block was malformed (missing list,
    /// misindented, or no slice ID) past the recovery threshold.
    #[error("malformed DECISIONS spec-anchors block at line {line}: {detail}")]
    MalformedDecisionsBlock { line: usize, detail: String },

    /// CoverageMatrix serialization failed (writer or formatter error).
    #[error("coverage matrix serialization failure: {0}")]
    SerializeFailed(String),

    /// Caller supplied a spec_root outside the canonical set (Omniverse,
    /// CssLv3, DecisionsLog).
    #[error("unknown spec root: {0:?}")]
    UnknownSpecRoot(String),

    /// Generic catch-all for assertions in the registry.
    #[error("registry invariant violated: {0}")]
    Invariant(String),
}
