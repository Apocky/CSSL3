//! CSLv3-native parser — LL(2) recursive-descent per `CSLv3/specs/13_GRAMMAR_SELF.csl`.
//!
//! § SPEC : `specs/16_DUAL_SURFACE.csl` § CSLv3-NATIVE SURFACE + `CSLv3/specs/13_GRAMMAR_SELF.csl`.
//! § DECISION : `DECISIONS.md` T3-D1 (LL(2), no combinator lib).
//!
//! § STAGE-0 SCOPE
//!   T3.2 implements the structural subset sufficient to round-trip the golden fixtures
//!   in `cssl-lex/tests/fixtures/csl_native_basic.cssl-csl` — specifically :
//!     * `§` section opens → each becomes a CST `Item` when interpretable, or an item-
//!       carrying `ModuleItem` wrapper when nested.
//!     * Evidence / modal / compound operators as stand-alone `Expr` forms (morpheme-
//!       stacking flattens to `Expr::Compound { op, lhs, rhs }`, per T3-D3).
//!     * Slot-template `[EVIDENCE?] [MODAL?] [DET?] SUBJECT [RELATION] OBJECT [GATE?] [SCOPE?]`
//!       parses as an expression sequence whose semantic content is filled in at elaboration.
//!   Full morphological decomposition + slot-template typing is elaborated in `cssl-hir`.
//!
//! § ENTRY
//!   `parse_module(source, tokens, &mut bag) -> Module` — feeds the same CST the Rust-hybrid
//!   parser produces, per §§ 16 PARSER UNIFICATION.

pub mod compound;
pub mod section;
pub mod slot;

use cssl_ast::{DiagnosticBag, Module, SourceFile, Span};
use cssl_lex::Token;

use crate::cursor::TokenCursor;

/// Top-level entry : parse a CSLv3-native module from the token slice.
#[must_use]
pub fn parse_module(source: &SourceFile, tokens: &[Token], bag: &mut DiagnosticBag) -> Module {
    let mut cursor = TokenCursor::newline_aware(tokens);
    let span_start = tokens.first().map_or(0, |t| t.span.start);
    let span_end = cursor.eof_span().end;
    let module_span = Span::new(source.id, span_start, span_end);

    let mut items = Vec::new();
    while !cursor.is_eof() {
        if let Some(it) = section::parse_section(&mut cursor, bag) {
            items.push(it);
        } else {
            // Error-recovery: the section parser advanced past malformed input.
            if cursor.is_eof() {
                break;
            }
        }
    }

    Module {
        span: module_span,
        inner_attrs: Vec::new(),
        path: None,
        items,
    }
}

#[cfg(test)]
mod mod_tests {
    use super::parse_module;
    use cssl_ast::{DiagnosticBag, SourceFile, SourceId, Surface};

    #[test]
    fn empty_csl_native_module_has_no_items() {
        let src = SourceFile::new(SourceId::first(), "<t>", "", Surface::CslNative);
        let tokens = cssl_lex::lex(&src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&src, &tokens, &mut bag);
        assert!(m.items.is_empty());
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn single_section_produces_module_item() {
        let src = SourceFile::new(SourceId::first(), "<t>", "§ foo\n", Surface::CslNative);
        let tokens = cssl_lex::lex(&src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&src, &tokens, &mut bag);
        // Expect exactly one top-level item wrapping the `§ foo` section.
        assert_eq!(m.items.len(), 1);
    }
}
