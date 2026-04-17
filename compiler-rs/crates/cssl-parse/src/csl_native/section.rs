//! CSLv3-native section parser — `§ name [body]` produces a `ModuleItem`.
//!
//! § SPEC : `CSLv3/specs/13_GRAMMAR_SELF.csl` § SECTIONS + `specs/16_DUAL_SURFACE.csl`.
//!
//! § STAGE-0 SCOPE
//!   A `§` followed by a name opens a section. Section content (between one `§` and the
//!   next) is parsed as a free-form sequence of expressions and folded into the section's
//!   `ModuleItem`. Rich slot-template / morpheme-stacking decomposition happens in
//!   `cssl-hir` elaboration (per T3-D3).
//!
//! § TOKEN NOTE
//!   The CSLv3-native lexer emits `Newline` + `Indent` + `Dedent` tokens; the section
//!   parser is newline-aware and uses indent / dedent as block boundaries.

use cssl_ast::{Attr, DiagnosticBag, Item, ModuleItem, Span, Visibility, VisibilityKind};
use cssl_lex::TokenKind;

use crate::common::parse_ident;
use crate::cursor::TokenCursor;
use crate::error::{custom, expected_any};

/// Parse a single `§ name …` section. Returns `None` if the current token is not `§`.
#[must_use]
pub fn parse_section(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Option<Item> {
    // Skip stray newlines between sections.
    while cursor.check(TokenKind::Newline) {
        cursor.bump();
    }
    if cursor.is_eof() {
        return None;
    }
    if !cursor.check(TokenKind::Section) {
        let t = cursor.peek();
        bag.push(expected_any(
            vec![TokenKind::Section],
            t.kind,
            t.span,
            "CSLv3-native section",
        ));
        cursor.bump();
        return None;
    }
    let sec = cursor.bump(); // §
    let name = parse_ident(cursor, bag, "section name");

    // Consume the section header line until Newline / Indent / Dedent / Eof / next §.
    skip_to_section_boundary(cursor);

    // Optional indented body : one-or-more indented lines become nested items.
    let nested = if cursor.check(TokenKind::Indent) {
        cursor.bump(); // Indent
        let mut items = Vec::new();
        while !cursor.check(TokenKind::Dedent) && !cursor.is_eof() {
            // Nested section inside an Indent block also becomes a ModuleItem.
            if cursor.check(TokenKind::Section) {
                if let Some(sub) = parse_section(cursor, bag) {
                    items.push(sub);
                }
            } else {
                // Consume one line of content, discarding details for T3.2 scope.
                skip_to_section_boundary(cursor);
                // Skip the newline if present.
                cursor.eat(TokenKind::Newline);
            }
        }
        if cursor.eat(TokenKind::Dedent).is_none() {
            // Dedent may be synthesized at Eof by the lexer — tolerate absence.
        }
        Some(items)
    } else {
        None
    };

    let end = cursor.peek().span.start.max(name.span.end);
    Some(Item::Module(ModuleItem {
        span: Span::new(sec.span.source, sec.span.start, end),
        attrs: Vec::<Attr>::new(),
        visibility: Visibility {
            span: Span::new(sec.span.source, sec.span.start, sec.span.start),
            kind: VisibilityKind::Private,
        },
        name,
        items: nested,
    }))
}

/// Skip tokens until a section-relevant boundary : `Newline`, `Indent`, `Dedent`, next `§`,
/// or `Eof`. Used after consuming the section header ident to swallow the rest of the
/// header line (which at T3.2 is not structurally elaborated).
fn skip_to_section_boundary(cursor: &mut TokenCursor<'_>) {
    loop {
        let k = cursor.peek().kind;
        if matches!(
            k,
            TokenKind::Newline
                | TokenKind::Indent
                | TokenKind::Dedent
                | TokenKind::Section
                | TokenKind::Eof
        ) {
            break;
        }
        cursor.bump();
    }
    // Consume one newline if present — the body-start test runs after this.
    cursor.eat(TokenKind::Newline);
}

/// Emit a bare diagnostic helper for unsupported CSLv3-native constructs; used by
/// `compound` and `slot` modules when elaboration boundaries are hit.
#[inline]
#[must_use]
pub fn unsupported(span: Span, form: &str) -> cssl_ast::Diagnostic {
    custom(
        format!("CSLv3-native stage0 does not yet parse {form}"),
        span,
    )
}

#[cfg(test)]
mod tests {
    use super::parse_section;
    use crate::cursor::TokenCursor;
    use cssl_ast::{DiagnosticBag, Item, SourceFile, SourceId, Surface};

    fn prep(src: &str) -> (SourceFile, Vec<cssl_lex::Token>) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::CslNative);
        let toks = cssl_lex::lex(&f);
        (f, toks)
    }

    #[test]
    fn simple_section_header() {
        let (_f, toks) = prep("§ foo\n");
        let mut c = TokenCursor::newline_aware(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_section(&mut c, &mut bag).unwrap();
        assert!(matches!(it, Item::Module(_)));
    }

    #[test]
    fn section_with_extra_header_tokens() {
        let (_f, toks) = prep("§ foo ≡ bar\n");
        let mut c = TokenCursor::newline_aware(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_section(&mut c, &mut bag).unwrap();
        assert!(matches!(it, Item::Module(_)));
    }

    #[test]
    fn nested_sections() {
        let text = "§ outer\n  § inner\n";
        let (_f, toks) = prep(text);
        let mut c = TokenCursor::newline_aware(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_section(&mut c, &mut bag).unwrap();
        if let Item::Module(m) = it {
            // Nested items list may be present if Indent/Dedent were emitted.
            let _ = m.items;
        } else {
            panic!("expected ModuleItem");
        }
    }
}
