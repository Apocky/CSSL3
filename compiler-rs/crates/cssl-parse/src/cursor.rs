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
///
/// § T11-CSSLC-RESTRICTIONS (T11-W11) : cursors carry a `restrictions` bitfield
///   so deep recursive-descent positions (struct-vs-block discrimination in
///   `if`/`while`/`for` headers) can answer "am I in a no-struct-literal
///   context?" without threading an extra arg through every parse fn.
///   The flag is RAII-toggled via [`TokenCursor::with_restriction`] which
///   returns a [`RestrictionGuard`] that auto-restores on drop.
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
    /// Active restriction flags. See [`Restriction`] for the bit-meanings.
    /// Cloned-cursors inherit the active set ; this is intentional — the
    /// `looks_like_struct_body` peek-ahead clones the cursor but should
    /// observe the same no-struct-brace context the original is in.
    restrictions: u32,
}

/// Bit-flags for the cursor's `restrictions` field.
///
/// § Why a bitfield ?
///   Rust's parser uses a similar `Restrictions` enum to disambiguate
///   places where struct-literal `Path { … }` is ambiguous with `if x { … }`,
///   `while x { … }`, `for pat in x { … }`, etc. Wrapping the parser in an
///   environment-passed flag keeps deep recursive-descent fns honest while
///   avoiding the alternative (cloning a parse-context arg through every
///   call). The bit-layout leaves room for future restrictions
///   (no-pipe-arrow, no-eq-sign-as-binding, etc.) without an ABI break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restriction {
    /// `Path { … }` MUST NOT be parsed as a struct-literal here. Set inside
    /// `if`/`while`/`for` headers + `match` scrutinee positions where the
    /// trailing `{` must instead start the block / arms / body.
    NoStructLiteral,
}

impl Restriction {
    /// Bit value for this restriction.
    #[must_use]
    pub const fn bit(self) -> u32 {
        match self {
            Self::NoStructLiteral => 1 << 0,
        }
    }
}

/// Snapshot of the cursor's restriction bits that was active before a
/// scoped restriction-set was applied. Pair with [`TokenCursor::push_restriction`]
/// + [`TokenCursor::pop_restrictions`] to scope a restriction-bit across a
/// recursive-descent block while keeping `&mut` available for inner calls.
///
/// § Why no Drop-guard ?
///   The natural `RAII` guard would hold `&mut TokenCursor`, which prevents
///   the borrow-checker from passing the same cursor down to inner parser
///   fns (which also need `&mut`). The push/pop pair is harder to misuse at
///   compile-time but lets all inner calls keep a clean `&mut TokenCursor`
///   reference. Tests below exercise both correct + sloppy use.
#[derive(Debug, Clone, Copy)]
pub struct RestrictionSnapshot(u32);

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
            restrictions: 0,
        }
    }

    /// Add the given restriction-bit to the active set + return a snapshot
    /// of the prior bits so the caller can restore them with
    /// [`Self::pop_restrictions`].
    ///
    /// § Idiom
    ///   ```ignore
    ///   let snap = cursor.push_restriction(Restriction::NoStructLiteral);
    ///   let cond = parse_expr(cursor, bag);
    ///   cursor.pop_restrictions(snap);
    ///   ```
    ///   The push/pop pair must bracket each other in stack order ; nested
    ///   pushes pop bits via the saved snapshot rather than a literal
    ///   bit-toggle (so an inner push that re-asserts an already-set bit is
    ///   still a no-op net of pop).
    pub fn push_restriction(&mut self, r: Restriction) -> RestrictionSnapshot {
        let snap = RestrictionSnapshot(self.restrictions);
        self.restrictions |= r.bit();
        snap
    }

    /// Restore the cursor's restriction-bits from a saved snapshot.
    pub fn pop_restrictions(&mut self, snap: RestrictionSnapshot) {
        self.restrictions = snap.0;
    }

    /// Check whether `r` is currently active for this cursor.
    #[must_use]
    pub const fn restricts(&self, r: Restriction) -> bool {
        (self.restrictions & r.bit()) != 0
    }

    /// Create a cursor that does **not** skip `Newline` tokens (CSLv3-native uses this).
    #[must_use]
    pub fn newline_aware(tokens: &'a [Token]) -> Self {
        let mut c = Self::new(tokens);
        c.skip_newlines = false;
        c
    }

    /// Direct mutator for restriction-bits — exposed for the cssl-native
    /// parser surface that constructs cursors at non-standard entry points.
    /// Prefer [`Self::with_restriction`] when an RAII guard works.
    pub fn set_restrictions(&mut self, r: u32) {
        self.restrictions = r;
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
    use super::{Restriction, TokenCursor};
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

    // ───────────────────────────────────────────────────────────────────────
    // § T11-CSSLC-RESTRICTIONS — restriction push/pop tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn restriction_default_state_is_empty() {
        let toks = vec![mk(TokenKind::Ident, 0, 1)];
        let c = TokenCursor::new(&toks);
        assert!(!c.restricts(Restriction::NoStructLiteral));
    }

    #[test]
    fn restriction_push_sets_flag() {
        let toks = vec![mk(TokenKind::Ident, 0, 1)];
        let mut c = TokenCursor::new(&toks);
        let _snap = c.push_restriction(Restriction::NoStructLiteral);
        assert!(c.restricts(Restriction::NoStructLiteral));
    }

    #[test]
    fn restriction_pop_clears_flag() {
        let toks = vec![mk(TokenKind::Ident, 0, 1)];
        let mut c = TokenCursor::new(&toks);
        let snap = c.push_restriction(Restriction::NoStructLiteral);
        c.pop_restrictions(snap);
        assert!(!c.restricts(Restriction::NoStructLiteral));
    }

    #[test]
    fn restriction_nested_push_pop_preserves_outer() {
        // Outer push asserts the flag ; an inner push observes it ; pop of
        // inner restores to outer-set ; pop of outer clears.
        let toks = vec![mk(TokenKind::Ident, 0, 1)];
        let mut c = TokenCursor::new(&toks);
        let outer = c.push_restriction(Restriction::NoStructLiteral);
        assert!(c.restricts(Restriction::NoStructLiteral));
        let inner = c.push_restriction(Restriction::NoStructLiteral);
        assert!(c.restricts(Restriction::NoStructLiteral));
        c.pop_restrictions(inner);
        assert!(c.restricts(Restriction::NoStructLiteral));
        c.pop_restrictions(outer);
        assert!(!c.restricts(Restriction::NoStructLiteral));
    }

    #[test]
    fn restriction_clone_inherits_active_set() {
        // Cloning the cursor inherits the restrictions ; this is intentional
        // for the looks_like_struct_body peek-ahead which clones to walk forward.
        let toks = vec![mk(TokenKind::Ident, 0, 1)];
        let mut c = TokenCursor::new(&toks);
        let _snap = c.push_restriction(Restriction::NoStructLiteral);
        let clone = c.clone();
        assert!(clone.restricts(Restriction::NoStructLiteral));
    }
}
