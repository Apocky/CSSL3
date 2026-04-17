//! CSSLv3 stage0 — concrete syntax tree + source-preserving forms.
//!
//! Authoritative design : `specs/02_IR.csl` + `specs/09_SYNTAX.csl` + `specs/16_DUAL_SURFACE.csl`.
//! Cross-crate decisions : `DECISIONS.md` (T1-D2 : Rust-native CSLv3 port).
//!
//! § STATUS : T2 in-progress — source / span / diagnostic / surface primitives landing first;
//!            CST + AST node taxonomy arrives alongside `cssl-hir` elaboration at T3.
//!
//! § SCOPE
//!   - `source`     : `SourceFile`, `SourceId`, `SourceLocation`, `Surface`
//!   - `span`       : byte-offset `Span` for slicing source + diagnostic pointing
//!   - `diagnostic` : `Severity`, `Diagnostic`, `DiagnosticBag` for frontend passes
//!
//! § SURFACE-AGNOSTIC
//!   Both Rust-hybrid and CSLv3-native lexers operate on the same `SourceFile` and
//!   emit `Span`s into the same byte-offset space. The `Surface` enum captures which
//!   grammar was used; downstream passes elaborate to a unified HIR regardless.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod diagnostic;
pub mod source;
pub mod span;

pub use diagnostic::{Diagnostic, DiagnosticBag, Severity};
pub use source::{SourceFile, SourceId, SourceLocation, Surface};
pub use span::Span;

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
