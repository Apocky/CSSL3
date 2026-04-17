//! Token cursor with 2-token lookahead.
//!
//! § DESIGN
//!   The parser consumes tokens strictly left-to-right. `TokenCursor` provides :
//!     - `peek()`      — look at the current token without consuming
//!     - `peek2()`     — look at the next-next token (for LL(2) disambiguation)
//!     - `bump()`      — consume the current token and return it
//!     - `check(kind)` / `eat(kind)` — test + conditional-consume helpers
//!     - `expect(kind, …)` — consume with an error-on-mismatch path
//!
//! § TRIVIA
//!   The lexer does not emit whitespace or comment tokens by default (see
//!   `cssl_lex::TokenKind::Whitespace / LineComment / BlockComment` — emitted only in
//!   trivia-preserving mode). The cursor however also skips `Whitespace`, `LineComment`,
//!   and `BlockComment` defensively in case a trivia-preserving lexer feed is used.
//!
//! § NEWLINES
//!   Both surfaces emit `Newline` for significant line-breaks. The Rust-hybrid parser
//!   generally treats them as whitespace (semicolons terminate). The CSLv3-native parser
//!   uses them as block terminators paired with `Indent` / `Dedent`. The cursor exposes
//!   both a "newline-skipping" and a "newline-aware" view via the `skip_newlines` toggle.

use cssl_ast::Span;
use cssl_lex::{Token, TokenKind};

/// A cursor over a token-slice with 2-token lookahead.
///
/// Cloning is O(1) — the cursor only holds `&'a [Token]` plus an index.
#[derive(Debug, Clone)]
pub struct TokenCursor<'a> {
    tokens: &'a [Token],
    pos: usize,
    /// When `true`, `peek` / `peek2` / `bump` skip `Newline` tokens as trivia.
    /// The Rust-hybrid parser enables this by default; the CSLv3-native parser toggles it
    /// off around structural positions (after `§` markers, at block boundaries).
    skip_newlines: bool,
    /// Sentinel span anchored at the end of the source (used for EOF diagnostics).
    eof_span: Span,
}

impl<'a> TokenCursor<'a> {
    /// Build a cursor over the given token slice.
    ///
    /// The `eof_span` is synthesized to point at end-of-source for diagnostic labels.
    /// If the input slice ends with a `TokenKind::Eof`, its span is used; otherwise
    /// a zero-width span at the source-end (or `Span::DUMMY`) is used.
    #[must_use]
    pub fn new(tokens: &'a [Token]) -> Self {
        let eof_span = tokens
            .iter()
            .rev()
            .find(|t| t.kind == TokenKind::Eof)
            .map(|t| t.span)
            .or_else(|| {
                tokens
                    .last()
                    .map(|t| Span::new(t.span.source, t.span.end, t.span.end))
            })
            .unwrap_or(Span::DUMMY);
        Self {
            tokens,
            pos: 0,
            skip_newlines: true,
            eof_span,
        }
    }

    /// Create a cursor that does **not** skip `Newline` tokens (CSLv3-native uses this).
    #[must_use]
    pub fn newline_aware(tokens: &'a [Token]) -> Self {
        let mut c = Self::new(tokens);
        c.skip_newlines = false;
        c
    }

    /// Toggle newline skipping.
    pub fn set_skip_newlines(&mut self, skip: bool) {
        self.skip_newlines = skip;
    }

    /// Current effective position after trivia-skip. Returns the raw index into `tokens`.
    #[must_use]
    pub fn effective_pos(&self) -> usize {
        let mut i = self.pos;
        while let Some(t) = self.tokens.get(i) {
            if self.is_trivia(t) {
                i += 1;
            } else {
                break;
            }
        }
        i
    }

    fn is_trivia(&self, t: &Token) -> bool {
        matches!(
            t.kind,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment
        ) || (self.skip_newlines && t.kind == TokenKind::Newline)
    }

    /// Peek at the current token, skipping trivia. Returns a synthetic EOF token on
    /// end-of-stream (never panics, never returns `None`).
    #[must_use]
    pub fn peek(&self) -> Token {
        let idx = self.effective_pos();
        self.tokens
            .get(idx)
            .copied()
            .unwrap_or_else(|| Token::new(TokenKind::Eof, self.eof_span))
    }

    /// Look one token past `peek` — skipping trivia on both slots.
    /// Returns a synthetic EOF on end-of-stream.
    #[must_use]
    pub fn peek2(&self) -> Token {
        let mut c = self.clone();
        c.bump();
        c.peek()
    }

    /// Consume and return the current token (after trivia-skip).
    /// Returns a synthetic EOF if already past-end.
    pub fn bump(&mut self) -> Token {
        // advance pos past trivia then consume
        self.pos = self.effective_pos();
        let t = self
            .tokens
            .get(self.pos)
            .copied()
            .unwrap_or_else(|| Token::new(TokenKind::Eof, self.eof_span));
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

    /// `true` iff the current token kind equals `expected`.
    #[must_use]
    pub fn check(&self, expected: TokenKind) -> bool {
        self.peek().kind == expected
    }

    /// If the current token kind matches `expected`, consume it and return `Some(span)`;
    /// otherwise return `None`.
    pub fn eat(&mut self, expected: TokenKind) -> Option<Span> {
        if self.check(expected) {
            Some(self.bump().span)
        } else {
            None
        }
    }

    /// `true` iff we are at end-of-stream (either past the slice or at `Eof`).
    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    /// Return the span used for EOF diagnostics.
    #[must_use]
    pub const fn eof_span(&self) -> Span {
        self.eof_span
    }
}

#[cfg(test)]
mod tests {
    use super::TokenCursor;
    use cssl_ast::{SourceId, Span};
    use cssl_lex::{Token, TokenKind};

    fn mk(kind: TokenKind, start: u32, end: u32) -> Token {
        Token::new(kind, Span::new(SourceId::first(), start, end))
    }

    #[test]
    fn empty_cursor_is_eof() {
        let toks: Vec<Token> = vec![];
        let c = TokenCursor::new(&toks);
        assert!(c.is_eof());
        assert_eq!(c.peek().kind, TokenKind::Eof);
    }

    #[test]
    fn bump_advances_and_returns_tokens() {
        let toks = vec![
            mk(TokenKind::Ident, 0, 3),
            mk(TokenKind::Comma, 3, 4),
            mk(TokenKind::Ident, 5, 8),
        ];
        let mut c = TokenCursor::new(&toks);
        assert_eq!(c.bump().kind, TokenKind::Ident);
        assert_eq!(c.bump().kind, TokenKind::Comma);
        assert_eq!(c.bump().kind, TokenKind::Ident);
        assert!(c.is_eof());
    }

    #[test]
    fn trivia_skipped_by_default() {
        let toks = vec![
            mk(TokenKind::Whitespace, 0, 1),
            mk(TokenKind::Ident, 1, 4),
            mk(TokenKind::LineComment, 4, 10),
            mk(TokenKind::Newline, 10, 11),
            mk(TokenKind::Ident, 11, 14),
        ];
        let mut c = TokenCursor::new(&toks);
        assert_eq!(c.peek().kind, TokenKind::Ident);
        assert_eq!(c.bump().span.start, 1);
        assert_eq!(c.bump().span.start, 11);
        assert!(c.is_eof());
    }

    #[test]
    fn newline_aware_mode_preserves_newlines() {
        let toks = vec![
            mk(TokenKind::Ident, 0, 1),
            mk(TokenKind::Newline, 1, 2),
            mk(TokenKind::Ident, 2, 3),
        ];
        let mut c = TokenCursor::newline_aware(&toks);
        assert_eq!(c.bump().kind, TokenKind::Ident);
        assert_eq!(c.peek().kind, TokenKind::Newline);
        assert_eq!(c.bump().kind, TokenKind::Newline);
        assert_eq!(c.bump().kind, TokenKind::Ident);
    }

    #[test]
    fn peek2_looks_ahead_one() {
        let toks = vec![
            mk(TokenKind::Ident, 0, 1),
            mk(TokenKind::Comma, 1, 2),
            mk(TokenKind::Ident, 2, 3),
        ];
        let c = TokenCursor::new(&toks);
        assert_eq!(c.peek().kind, TokenKind::Ident);
        assert_eq!(c.peek2().kind, TokenKind::Comma);
    }

    #[test]
    fn peek_does_not_advance() {
        let toks = vec![mk(TokenKind::Ident, 0, 1)];
        let c = TokenCursor::new(&toks);
        assert_eq!(c.peek().kind, TokenKind::Ident);
        assert_eq!(c.peek().kind, TokenKind::Ident);
    }

    #[test]
    fn eat_matches_and_consumes() {
        let toks = vec![mk(TokenKind::Comma, 0, 1), mk(TokenKind::Ident, 1, 2)];
        let mut c = TokenCursor::new(&toks);
        assert!(c.eat(TokenKind::Comma).is_some());
        assert!(c.eat(TokenKind::Comma).is_none());
        assert_eq!(c.peek().kind, TokenKind::Ident);
    }

    #[test]
    fn check_does_not_consume() {
        let toks = vec![mk(TokenKind::Ident, 0, 1)];
        let c = TokenCursor::new(&toks);
        assert!(c.check(TokenKind::Ident));
        assert!(c.check(TokenKind::Ident));
    }

    #[test]
    fn eof_span_preserved_from_source() {
        let toks = vec![mk(TokenKind::Ident, 0, 3), mk(TokenKind::Eof, 3, 3)];
        let c = TokenCursor::new(&toks);
        assert_eq!(c.eof_span().start, 3);
    }
}
