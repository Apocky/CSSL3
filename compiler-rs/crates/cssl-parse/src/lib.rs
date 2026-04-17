//! CSSLv3 stage0 — CSLv3-native + Rust-hybrid parser dispatch.
//!
//! § SPEC SOURCES
//!   - `specs/09_SYNTAX.csl` § RUST-HYBRID SURFACE + § OPERATOR-PRECEDENCE
//!   - `specs/16_DUAL_SURFACE.csl` § MODE-DETECTION + § PARSER UNIFICATION
//!   - `specs/02_IR.csl` (HIR contract — the CST the parser emits is elaborated here)
//!   - `CSLv3/specs/13_GRAMMAR_SELF.csl` (LL(2) + compound-formation + slot-template)
//!
//! § DECISIONS
//!   - T3-D1 : hand-rolled recursive-descent + Pratt for Rust-hybrid. Zero combinator lib.
//!   - T3-D2 : no interning in CST — identifiers carry `Span` only.
//!   - T3-D3 : morpheme / compound chains surface at CST as `Expr::Compound { op, lhs, rhs }`.
//!   - T3-D4 : CST single-file in `cssl-ast`; HIR modular in `cssl-hir`.
//!
//! § ENTRY
//!   Public `parse(source, tokens)` dispatches on `source.surface` to either
//!   `rust_hybrid::parse_module` or `csl_native::parse_module`, both of which emit
//!   `cssl_ast::Module` plus a shared `DiagnosticBag`.
//!
//! § ERROR RECOVERY
//!   Every parser rule returns its node unconditionally. Unrecoverable positions surface
//!   as `Expr::Error` / synthetic placeholder nodes with a paired `Diagnostic::error` in
//!   the bag. Tooling never sees `Option<Node>` for parser output — partial CSTs are still
//!   walkable (essential for LSP partial-document editing).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod common;
pub mod cursor;
pub mod error;

pub mod csl_native;
pub mod rust_hybrid;

pub use cursor::TokenCursor;
pub use error::ParseError;

use cssl_ast::{source::Surface, DiagnosticBag, Module, SourceFile};
use cssl_lex::Token;

/// Parse a token-stream (from `cssl_lex::lex`) into a `cssl_ast::Module` plus a
/// `DiagnosticBag` of accumulated parser diagnostics.
///
/// Dispatches on `source.surface` to the Rust-hybrid or CSLv3-native surface parser.
/// For `Surface::Auto`, the Rust-hybrid parser is used (callers can run
/// `cssl_lex::mode::detect` before invoking this function when explicit surface selection
/// is desired).
#[must_use]
pub fn parse(source: &SourceFile, tokens: &[Token]) -> (Module, DiagnosticBag) {
    let mut bag = DiagnosticBag::new();
    let module = match source.surface {
        Surface::CslNative => csl_native::parse_module(source, tokens, &mut bag),
        Surface::RustHybrid | Surface::Auto => rust_hybrid::parse_module(source, tokens, &mut bag),
    };
    (module, bag)
}

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::{parse, STAGE0_SCAFFOLD};
    use cssl_ast::{SourceFile, SourceId, Surface};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn empty_source_produces_empty_module_no_errors() {
        let src = SourceFile::new(SourceId::first(), "<test>", "", Surface::RustHybrid);
        let tokens = cssl_lex::lex(&src);
        let (module, bag) = parse(&src, &tokens);
        assert!(module.items.is_empty());
        assert_eq!(bag.error_count(), 0);
    }
}
