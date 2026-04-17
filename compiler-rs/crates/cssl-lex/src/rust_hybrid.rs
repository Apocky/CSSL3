//! Rust-hybrid surface lexer, `logos`-derived.
//!
//! § SPEC : `specs/09_SYNTAX.csl` § RUST-HYBRID SURFACE.
//! § STRATEGY
//!   A private `RawToken` enum carries flat, logos-friendly variants tied to source
//!   regexes. The public [`lex`] function post-processes each `RawToken` — promoting
//!   identifier text to `Keyword(_)` when applicable — and emits the richer public
//!   `TokenKind` from `crate::token`. This preserves the structured public API while
//!   keeping the raw lexer small and fast.
//! § COT-IN-COMMENTS
//!   CSLv3 chain-of-thought lines (`§ I> …`, `§ W! …`, `§ N! …`, and block form
//!   `§{ … §}`) are accepted here as comment-kind tokens (line + block respectively).
//!   Detailed parsing of their bodies is the responsibility of a future `--lint-csl`
//!   dispatch into `crate::csl_native`.

use cssl_ast::{SourceFile, Span};
use logos::Logos;

use crate::token::{BracketKind, BracketSide, Keyword, StringFlavor, Token, TokenKind};

/// Lex a Rust-hybrid source file into a vector of tokens, terminating with `TokenKind::Eof`.
///
/// Unrecognized byte sequences yield a `TokenKind::Error` token carrying the offending span;
/// the caller is responsible for turning these into `Diagnostic` records.
#[must_use]
pub fn lex(source: &SourceFile) -> Vec<Token> {
    let mut lexer = RawToken::lexer(&source.contents);
    let mut out: Vec<Token> = Vec::new();
    while let Some(raw) = lexer.next() {
        let range = lexer.span();
        let start = u32::try_from(range.start).unwrap_or(u32::MAX);
        let end = u32::try_from(range.end).unwrap_or(u32::MAX);
        let span = Span::new(source.id, start, end);
        let kind = match raw {
            Ok(r) => promote(r, &source.contents[range]),
            Err(()) => TokenKind::Error,
        };
        out.push(Token::new(kind, span));
    }
    let eof_offset = source.len_bytes();
    out.push(Token::new(
        TokenKind::Eof,
        Span::new(source.id, eof_offset, eof_offset),
    ));
    out
}

/// Map a `RawToken` + source-text-slice into a public `TokenKind`.
fn promote(raw: RawToken, text: &str) -> TokenKind {
    match raw {
        RawToken::Ident => Keyword::from_word(text).map_or(TokenKind::Ident, TokenKind::Keyword),
        RawToken::IntLiteral => TokenKind::IntLiteral,
        RawToken::FloatLiteral => TokenKind::FloatLiteral,
        RawToken::StringLiteral => TokenKind::StringLiteral(StringFlavor::Normal),
        RawToken::RawStringLiteral => TokenKind::StringLiteral(StringFlavor::Raw),
        RawToken::CharLiteral => TokenKind::CharLiteral,
        RawToken::LParen => TokenKind::Bracket(BracketKind::Paren, BracketSide::Open),
        RawToken::RParen => TokenKind::Bracket(BracketKind::Paren, BracketSide::Close),
        RawToken::LBrace => TokenKind::Bracket(BracketKind::Brace, BracketSide::Open),
        RawToken::RBrace => TokenKind::Bracket(BracketKind::Brace, BracketSide::Close),
        RawToken::LBracket => TokenKind::Bracket(BracketKind::Square, BracketSide::Open),
        RawToken::RBracket => TokenKind::Bracket(BracketKind::Square, BracketSide::Close),
        RawToken::Comma => TokenKind::Comma,
        RawToken::Semi => TokenKind::Semi,
        RawToken::Colon => TokenKind::Colon,
        RawToken::ColonColon => TokenKind::ColonColon,
        RawToken::DotDotEq => TokenKind::DotDotEq,
        RawToken::DotDot => TokenKind::DotDot,
        RawToken::Dot => TokenKind::Dot,
        RawToken::At => TokenKind::At,
        RawToken::Hash => TokenKind::Hash,
        RawToken::Dollar => TokenKind::Dollar,
        RawToken::QuestionQuestion => TokenKind::QuestionQuestion,
        RawToken::Question => TokenKind::Question,
        RawToken::Apostrophe => TokenKind::Apostrophe,
        RawToken::Plus => TokenKind::Plus,
        RawToken::Minus => TokenKind::Minus,
        RawToken::Star => TokenKind::Star,
        RawToken::Slash => TokenKind::Slash,
        RawToken::Percent => TokenKind::Percent,
        RawToken::EqEq => TokenKind::EqEq,
        RawToken::Ne => TokenKind::Ne,
        RawToken::Le => TokenKind::Le,
        RawToken::Ge => TokenKind::Ge,
        RawToken::Eq => TokenKind::Eq,
        RawToken::Lt => TokenKind::Lt,
        RawToken::Gt => TokenKind::Gt,
        RawToken::AmpAmp => TokenKind::AmpAmp,
        RawToken::PipePipe => TokenKind::PipePipe,
        RawToken::Amp => TokenKind::Amp,
        RawToken::Pipe => TokenKind::Pipe,
        RawToken::Caret => TokenKind::Caret,
        RawToken::Tilde => TokenKind::Tilde,
        RawToken::Bang => TokenKind::Bang,
        RawToken::LShift => TokenKind::LShift,
        RawToken::RShift => TokenKind::RShift,
        RawToken::Arrow => TokenKind::Arrow,
        RawToken::LeftArrow => TokenKind::LeftArrow,
        RawToken::BiArrow => TokenKind::BiArrow,
        RawToken::FatArrow => TokenKind::FatArrow,
        RawToken::PipeArrow => TokenKind::PipeArrow,
        RawToken::Newline => TokenKind::Newline,
        RawToken::LineComment | RawToken::CotLine => TokenKind::LineComment,
        RawToken::BlockComment | RawToken::CotBlock => TokenKind::BlockComment,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § RawToken (logos-derived)
// ════════════════════════════════════════════════════════════════════════════

/// Internal logos-derived enum. Flat variants map 1:1 to regex patterns.
///
/// Whitespace (`space` + `tab` + `\r`) is skipped by the `#[logos(skip …)]` directive.
/// Newlines are emitted explicitly because the parser treats line-breaks as layout hints.
#[derive(Logos, Debug, Clone, Copy, PartialEq)]
#[logos(skip r"[ \t\r]+")]
enum RawToken {
    // ─ literals ────────────────────────────────────────────────────────────
    #[regex(r"[A-Za-z_][A-Za-z0-9_']*", priority = 2)]
    Ident,

    // float first so `3.14` doesn't tokenize as `3` then `.14`
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*(?:'[A-Za-z_][A-Za-z0-9_]*)?")]
    FloatLiteral,

    #[regex(r"[0-9][0-9_]*(?:'[A-Za-z_][A-Za-z0-9_]*)?")]
    IntLiteral,

    #[regex(r#""(?:[^"\\\n]|\\[\\"nrt0'])*""#)]
    StringLiteral,

    #[regex(r##"r#*"[^"]*"#*"##)]
    RawStringLiteral,

    #[regex(r##"'(?:[^'\\\n]|\\[\\'nrt0"])'"##)]
    CharLiteral,

    // ─ brackets ────────────────────────────────────────────────────────────
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,

    // ─ punctuation ─────────────────────────────────────────────────────────
    #[token(",")]
    Comma,
    #[token(";")]
    Semi,
    #[token("::")]
    ColonColon,
    #[token(":")]
    Colon,
    #[token("..=")]
    DotDotEq,
    #[token("..")]
    DotDot,
    #[token(".")]
    Dot,
    #[token("@")]
    At,
    #[token("#")]
    Hash,
    #[token("$")]
    Dollar,
    #[token("??")]
    QuestionQuestion,
    #[token("?")]
    Question,

    /// Standalone `'` used for `T'tag` refinement / `42'i32` type-suffix / lifetime-like
    /// annotations. Lower priority than `CharLiteral` so well-formed `'c'` still wins.
    #[token("'", priority = 0)]
    Apostrophe,

    // ─ arrows (must precede bare operators) ────────────────────────────────
    #[token("->")]
    #[token("→")]
    Arrow,
    #[token("<-")]
    #[token("←")]
    LeftArrow,
    #[token("<->")]
    #[token("↔")]
    BiArrow,
    #[token("=>")]
    #[token("⇒")]
    FatArrow,
    #[token("|>")]
    #[token("▷")]
    PipeArrow,

    // ─ comparison (multi-char precedes single) ─────────────────────────────
    #[token("==")]
    #[token("≡")]
    EqEq,
    #[token("!=")]
    #[token("≠")]
    Ne,
    #[token("<=")]
    #[token("≤")]
    Le,
    #[token(">=")]
    #[token("≥")]
    Ge,
    #[token("<<")]
    LShift,
    #[token(">>")]
    RShift,

    // ─ logical multi-char ──────────────────────────────────────────────────
    #[token("&&")]
    #[token("∧")]
    AmpAmp,
    #[token("||")]
    #[token("∨")]
    PipePipe,

    // ─ single-char operators ───────────────────────────────────────────────
    #[token("=")]
    Eq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("!")]
    Bang,

    // ─ layout ──────────────────────────────────────────────────────────────
    #[token("\n")]
    Newline,

    // ─ comments (§ forms must precede ordinary // and /* for priority) ────
    /// `§{ … §}` CoT block comment. Body = `[^§]` or `§` followed by non-`}` .
    /// Logos regex-automata lacks non-greedy quantifiers, so the body is expressed
    /// as an explicit alternation that excludes the `§}` terminator.
    #[regex(r"§\{(?:[^§]|§[^}])*§\}")]
    CotBlock,
    /// `§ I> …` / `§ W! …` / `§ N! …` / `§ R! …` / `§ M? …` / `§ Q? …` / `§ P> …` / `§ D> …` line form.
    #[regex(r"§[ \t]+(?:I>|W!|R!|M\?|N!|Q\?|P>|D>)[^\n]*")]
    CotLine,

    /// `// … \n` line comment.
    #[regex(r"//[^\n]*")]
    LineComment,
    /// `/* … */` block comment (non-nesting).
    #[regex(r"/\*([^*]|\*[^/])*\*/")]
    BlockComment,
}

#[cfg(test)]
mod tests {
    use super::lex;
    use crate::token::{BracketKind, BracketSide, Keyword, StringFlavor, TokenKind};
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn mk(src: &str) -> SourceFile {
        SourceFile::new(SourceId::first(), "<test>", src, Surface::RustHybrid)
    }

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(&mk(src)).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn empty_input_emits_only_eof() {
        assert_eq!(kinds(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn simple_ident_and_keyword() {
        assert_eq!(kinds("foo"), vec![TokenKind::Ident, TokenKind::Eof]);
        assert_eq!(
            kinds("fn"),
            vec![TokenKind::Keyword(Keyword::Fn), TokenKind::Eof],
        );
        assert_eq!(
            kinds("let mut"),
            vec![
                TokenKind::Keyword(Keyword::Let),
                TokenKind::Keyword(Keyword::Mut),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn integer_vs_float() {
        assert_eq!(kinds("42"), vec![TokenKind::IntLiteral, TokenKind::Eof]);
        assert_eq!(kinds("3.14"), vec![TokenKind::FloatLiteral, TokenKind::Eof]);
        assert_eq!(kinds("42'i32"), vec![TokenKind::IntLiteral, TokenKind::Eof],);
    }

    #[test]
    fn string_literals() {
        assert_eq!(
            kinds(r#""hello""#),
            vec![
                TokenKind::StringLiteral(StringFlavor::Normal),
                TokenKind::Eof,
            ],
        );
        assert_eq!(
            kinds(r##"r#"raw string"#"##),
            vec![TokenKind::StringLiteral(StringFlavor::Raw), TokenKind::Eof,],
        );
    }

    #[test]
    fn arrow_family() {
        assert_eq!(
            kinds("-> => |>"),
            vec![
                TokenKind::Arrow,
                TokenKind::FatArrow,
                TokenKind::PipeArrow,
                TokenKind::Eof,
            ],
        );
        assert_eq!(
            kinds("→ ⇒ ▷"),
            vec![
                TokenKind::Arrow,
                TokenKind::FatArrow,
                TokenKind::PipeArrow,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn comparisons_multi_and_single() {
        assert_eq!(
            kinds("<= >= != == < > ="),
            vec![
                TokenKind::Le,
                TokenKind::Ge,
                TokenKind::Ne,
                TokenKind::EqEq,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::Eq,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn bracket_triples() {
        assert_eq!(
            kinds("( ) { } [ ]"),
            vec![
                TokenKind::Bracket(BracketKind::Paren, BracketSide::Open),
                TokenKind::Bracket(BracketKind::Paren, BracketSide::Close),
                TokenKind::Bracket(BracketKind::Brace, BracketSide::Open),
                TokenKind::Bracket(BracketKind::Brace, BracketSide::Close),
                TokenKind::Bracket(BracketKind::Square, BracketSide::Open),
                TokenKind::Bracket(BracketKind::Square, BracketSide::Close),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn fn_declaration_shape() {
        let src = "fn sphere_sdf(p : vec3, r : f32) -> f32 { length(p) - r }";
        let ks = kinds(src);
        // sanity : starts with `fn` keyword, ends with EOF, contains Arrow
        assert_eq!(ks.first(), Some(&TokenKind::Keyword(Keyword::Fn)));
        assert_eq!(ks.last(), Some(&TokenKind::Eof));
        assert!(ks.contains(&TokenKind::Arrow));
        assert!(ks.contains(&TokenKind::Colon));
    }

    #[test]
    fn dot_family_disambiguation() {
        assert_eq!(
            kinds("0..=10 0..10 a.b"),
            vec![
                TokenKind::IntLiteral,
                TokenKind::DotDotEq,
                TokenKind::IntLiteral,
                TokenKind::IntLiteral,
                TokenKind::DotDot,
                TokenKind::IntLiteral,
                TokenKind::Ident,
                TokenKind::Dot,
                TokenKind::Ident,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn line_comment_skipped_as_token() {
        assert_eq!(
            kinds("// hi\nfoo"),
            vec![
                TokenKind::LineComment,
                TokenKind::Newline,
                TokenKind::Ident,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn cot_line_forms() {
        assert_eq!(
            kinds("§ I> insight text\n"),
            vec![TokenKind::LineComment, TokenKind::Newline, TokenKind::Eof,],
        );
        assert_eq!(
            kinds("§ W! must hold\n"),
            vec![TokenKind::LineComment, TokenKind::Newline, TokenKind::Eof,],
        );
    }

    #[test]
    fn cot_block_multiline() {
        let src = "§{ design note\n  spans lines §}";
        assert_eq!(kinds(src), vec![TokenKind::BlockComment, TokenKind::Eof]);
    }

    #[test]
    fn attribute_prefix_at() {
        assert_eq!(
            kinds("@differentiable"),
            vec![TokenKind::At, TokenKind::Ident, TokenKind::Eof],
        );
    }

    #[test]
    fn effect_row_punctuation() {
        let src = "fn f() / {GPU, NoAlloc}";
        let ks = kinds(src);
        assert!(ks.contains(&TokenKind::Slash));
        assert!(ks.contains(&TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)));
        assert!(ks.contains(&TokenKind::Comma));
    }

    #[test]
    fn span_offsets_are_exact() {
        let file = mk("foo  bar");
        let toks = lex(&file);
        // foo : 0..3, bar : 5..8
        assert_eq!(toks[0].span.start, 0);
        assert_eq!(toks[0].span.end, 3);
        assert_eq!(toks[1].span.start, 5);
        assert_eq!(toks[1].span.end, 8);
        // EOF at file length (8)
        let eof = toks.last().unwrap();
        assert_eq!(eof.kind, TokenKind::Eof);
        assert_eq!(eof.span.start, 8);
        assert_eq!(eof.span.end, 8);
    }

    #[test]
    fn eof_always_appended() {
        assert_eq!(kinds("foo").last(), Some(&TokenKind::Eof));
        assert_eq!(kinds("").last(), Some(&TokenKind::Eof));
        assert_eq!(kinds("fn foo() {}").last(), Some(&TokenKind::Eof));
    }
}
