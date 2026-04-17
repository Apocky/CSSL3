//! Rust-hybrid parser — hand-rolled recursive-descent + Pratt precedence-climbing.
//!
//! § SPEC : `specs/09_SYNTAX.csl` § RUST-HYBRID SURFACE + § OPERATOR-PRECEDENCE.
//! § DECISION : `DECISIONS.md` T3-D1 (no combinator lib).
//!
//! § ENTRY
//!   `parse_module(source, tokens, &mut bag) -> Module` builds a full `cssl_ast::Module`
//!   from a token-slice produced by `cssl_lex::rust_hybrid::lex`.
//!
//! § MODULE LAYOUT
//!   * `attr`      — `@name(args)` outer + `#![name = …]` inner.
//!   * `generics`  — `<T : Bound>` + `where` clauses.
//!   * `ty`        — type expressions (Path / Tuple / Ref / Fn / Capability / Refined / Array / Slice / Infer).
//!   * `pat`       — patterns (Wildcard / Literal / Binding / Tuple / Struct / Variant / Or / Range / Ref).
//!   * `expr`      — Pratt precedence-climb for binary + prefix unary + postfix chain.
//!   * `stmt`      — `let` / expression-statement / nested item + block.
//!   * `item`      — top-level item dispatch (fn / struct / enum / interface / impl / effect /
//!                   handler / type alias / use / const / module).

pub mod attr;
pub mod expr;
pub mod generics;
pub mod item;
pub mod pat;
pub mod stmt;
pub mod ty;

use cssl_ast::{DiagnosticBag, Module, SourceFile, Span};
use cssl_lex::{Token, TokenKind};

use crate::cursor::TokenCursor;

/// Top-level entry : parse a module from the token slice.
#[must_use]
pub fn parse_module(source: &SourceFile, tokens: &[Token], bag: &mut DiagnosticBag) -> Module {
    let mut cursor = TokenCursor::new(tokens);
    // Module span starts at first token (or zero if empty) and ends at EOF.
    let span_start = tokens.first().map_or(0, |t| t.span.start);
    let span_end = cursor.eof_span().end;
    let module_span = Span::new(source.id, span_start, span_end);

    // Parse optional inner attributes `#![…]` at file head.
    let mut inner_attrs = Vec::new();
    while cursor.check(TokenKind::Hash) && cursor.peek2().kind == TokenKind::Bang {
        if let Some(attr) = attr::parse_inner(&mut cursor, bag) {
            inner_attrs.push(attr);
        } else {
            break;
        }
    }

    // Parse optional module-path declaration `module foo.bar.baz`.
    let path = item::parse_optional_module_path(&mut cursor, bag);

    // Parse items until EOF, recovering at item boundaries on error.
    let mut items = Vec::new();
    while !cursor.is_eof() {
        let pos_before = cursor.effective_pos();
        if let Some(it) = item::parse_item(&mut cursor, bag) {
            items.push(it);
        } else if cursor.effective_pos() == pos_before {
            // No progress — avoid infinite loop. The item parser normally bumps at
            // least one token on error ; if it didn't, bail out.
            break;
        }
        // else : error was emitted and at least one token consumed — continue looking
        // for the next item boundary.
    }

    Module {
        span: module_span,
        inner_attrs,
        path,
        items,
    }
}

#[cfg(test)]
mod mod_tests {
    use super::parse_module;
    use cssl_ast::{DiagnosticBag, SourceFile, SourceId, Surface};

    #[test]
    fn empty_module_has_no_items_and_no_errors() {
        let src = SourceFile::new(SourceId::first(), "<t>", "", Surface::RustHybrid);
        let tokens = cssl_lex::lex(&src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&src, &tokens, &mut bag);
        assert!(m.items.is_empty());
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn module_span_covers_source() {
        let text = "\n\n\n";
        let src = SourceFile::new(SourceId::first(), "<t>", text, Surface::RustHybrid);
        let tokens = cssl_lex::lex(&src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&src, &tokens, &mut bag);
        assert_eq!(m.span.source, src.id);
    }
}
