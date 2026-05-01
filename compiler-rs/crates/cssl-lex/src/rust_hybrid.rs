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

use crate::token::{BracketKind, BracketSide, Keyword, StringFlavor, Token, TokenKind, TypeSuffix};

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
    // Fold `Ident + Apostrophe + Ident(single-letter-morpheme)` into `Ident + Suffix(_)`
    // per T2-D5 — this turns `base'd` (2 tokens) into atomic morpheme emission while
    // preserving `f32'pos` as a 3-token `Ident + Apostrophe + Ident` (refinement-tag
    // shape). The fold is conservative : adjacency required on both sides, and the
    // third token must be exactly one byte long and a recognized morpheme letter.
    fold_morpheme_suffixes(source, &mut out);
    out
}

/// Post-pass : fold `Ident + Apostrophe + Ident(single-morpheme-letter)` (adjacent) into
/// `Ident + Suffix(_)`. Non-fold cases (multi-letter attachment, non-morpheme letter,
/// whitespace between tokens, preceding token not Ident) pass through unchanged.
fn fold_morpheme_suffixes(source: &SourceFile, tokens: &mut Vec<Token>) {
    let mut folded: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let fold_match = i + 2 < tokens.len()
            && tokens[i].kind == TokenKind::Ident
            && tokens[i + 1].kind == TokenKind::Apostrophe
            && tokens[i + 2].kind == TokenKind::Ident
            && tokens[i].span.end == tokens[i + 1].span.start
            && tokens[i + 1].span.end == tokens[i + 2].span.start
            && tokens[i + 2].span.len() == 1;
        if fold_match {
            let suffix_span = tokens[i + 2].span;
            if let Some(letter_str) = source.slice(suffix_span.start, suffix_span.end) {
                if let Some(letter) = letter_str.chars().next() {
                    if let Some(suffix) = TypeSuffix::from_letter(letter) {
                        // Emit base Ident unchanged, then combined Suffix.
                        folded.push(tokens[i]);
                        let combined = Span::new(
                            tokens[i + 1].span.source,
                            tokens[i + 1].span.start,
                            tokens[i + 2].span.end,
                        );
                        folded.push(Token::new(TokenKind::Suffix(suffix), combined));
                        i += 3;
                        continue;
                    }
                }
            }
        }
        folded.push(tokens[i]);
        i += 1;
    }
    *tokens = folded;
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
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", priority = 2)]
    Ident,

    // float first so `3.14` doesn't tokenize as `3` then `.14`
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*(?:'[A-Za-z_][A-Za-z0-9_]*)?")]
    FloatLiteral,

    /// Integer literal — three syntactic forms, all collapsing to a single
    /// `IntLiteral` token kind:
    ///
    /// * decimal       `42` / `1_000_000` (with optional `'i32` style suffix)
    /// * hexadecimal   `0xCAFE_BABE` / `0x53`
    /// * binary        `0b1010_0110`
    /// * octal         `0o755`
    ///
    /// All non-decimal forms accept ASCII case-insensitive hex digits where
    /// applicable, an underscore separator after the first digit, and the
    /// same `'<suffix>` type-tag as decimal literals. Distinguishing the
    /// numeric base is the parser/sema layer's responsibility — at the lexer
    /// level all four spellings emit `TokenKind::IntLiteral` carrying the
    /// raw source slice so downstream stages can re-parse.
    ///
    /// Hex literal MUST appear as a regex *alternative* on this variant
    /// rather than its own variant: with separate variants logos's
    /// longest-match disambiguator would split `0x53` into `0` (decimal) +
    /// `x53` (ident) because the decimal form is also a valid prefix. Listing
    /// hex/bin/oct first as additional `#[regex]` lines keeps `0x...` /
    /// `0b...` / `0o...` matched as a single token before the bare-decimal
    /// fallback even kicks in.
    #[regex(r"0x[0-9A-Fa-f][0-9A-Fa-f_]*(?:'[A-Za-z_][A-Za-z0-9_]*)?")]
    #[regex(r"0b[01][01_]*(?:'[A-Za-z_][A-Za-z0-9_]*)?")]
    #[regex(r"0o[0-7][0-7_]*(?:'[A-Za-z_][A-Za-z0-9_]*)?")]
    #[regex(r"[0-9][0-9_]*(?:'[A-Za-z_][A-Za-z0-9_]*)?")]
    IntLiteral,

    /// Normal string literal `"…"` — supports common escapes (`\n` `\t` `\r` `\0` `\\`
    /// `\"` `\'`), 2-hex-digit byte escapes (`\x41`), and brace-delimited Unicode
    /// codepoint escapes (`\u{1F600}` — 1..=6 hex digits). Body bytes that are not
    /// `"` / `\` / `\n` are accepted as-is, so multibyte UTF-8 (e.g. `鳥居`) flows
    /// through the regex unchanged.
    #[regex(r#""(?:[^"\\\n]|\\[\\"nrt0']|\\x[0-9A-Fa-f][0-9A-Fa-f]|\\u\{[0-9A-Fa-f]{1,6}\})*""#)]
    StringLiteral,

    #[regex(r##"r#*"[^"]*"#*"##)]
    RawStringLiteral,

    /// Char literal `'c'` — single Unicode-scalar character or single escape (same
    /// escape table as `StringLiteral` minus the closing-quote escape rule).
    #[regex(
        r##"'(?:[^'\\\n]|\\[\\'nrt0"]|\\x[0-9A-Fa-f][0-9A-Fa-f]|\\u\{[0-9A-Fa-f]{1,6}\})'"##
    )]
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
    use crate::token::{BracketKind, BracketSide, Keyword, StringFlavor, TokenKind, TypeSuffix};
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

    /// T11-CC-PARSER-9 (W-CC-array-lit) — hex / binary / octal integer
    /// literals must lex as a *single* `IntLiteral` token. The mandelbulb
    /// scene's `[u8 ; 13]` shader-stub array literal uses `0x53, 0x54, …`
    /// element-syntax; before this fix `0x53` split into `0` + `x53`, which
    /// blew up the array-literal parser one byte after the `[`.
    #[test]
    fn integer_non_decimal_bases_atomic() {
        assert_eq!(kinds("0x53"), vec![TokenKind::IntLiteral, TokenKind::Eof]);
        assert_eq!(
            kinds("0xCAFE_BABE"),
            vec![TokenKind::IntLiteral, TokenKind::Eof],
        );
        assert_eq!(
            kinds("0b1010_0110"),
            vec![TokenKind::IntLiteral, TokenKind::Eof],
        );
        assert_eq!(kinds("0o755"), vec![TokenKind::IntLiteral, TokenKind::Eof]);
        // type-tag suffix still works on hex
        assert_eq!(
            kinds("0xFF'u8"),
            vec![TokenKind::IntLiteral, TokenKind::Eof],
        );
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

    // ─── T11-CC-PARSER-4 : extended escape table ────────────────────────────

    #[test]
    fn string_literal_basic_escapes_lex_atomic() {
        // `\n \t \\ \" \r \0 \'` — single token covering the whole quoted body.
        let toks = lex(&mk(r#""line1\nline2\t\"quoted\"""#));
        assert_eq!(toks[0].kind, TokenKind::StringLiteral(StringFlavor::Normal));
        // span starts at byte 0 and ends at the byte after the trailing quote
        assert_eq!(toks[0].span.start, 0);
        assert!(toks[0].span.end as usize == r#""line1\nline2\t\"quoted\"""#.len());
        assert_eq!(toks[1].kind, TokenKind::Eof);
    }

    #[test]
    fn string_literal_hex_escape_lex() {
        // \x## byte escape — three chars `ABC`.
        assert_eq!(
            kinds(r#""\x41\x42\x43""#),
            vec![
                TokenKind::StringLiteral(StringFlavor::Normal),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn string_literal_unicode_escape_lex() {
        // \u{...} brace-delimited Unicode codepoint escape (1..=6 hex digits).
        assert_eq!(
            kinds(r#""\u{1F600}\u{0}\u{42}""#),
            vec![
                TokenKind::StringLiteral(StringFlavor::Normal),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn string_literal_unicode_body_lex() {
        // Multibyte UTF-8 in body — accepted as raw bytes by `[^"\\\n]` regex class.
        assert_eq!(
            kinds("\"鳥居\""),
            vec![
                TokenKind::StringLiteral(StringFlavor::Normal),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn char_literal_with_escape_lex() {
        // `'\n'` — single CharLiteral covering 4 bytes.
        let toks = lex(&mk(r"'\n'"));
        assert_eq!(toks[0].kind, TokenKind::CharLiteral);
        assert_eq!(toks[0].span.end - toks[0].span.start, 4);
        assert_eq!(toks[1].kind, TokenKind::Eof);
    }

    #[test]
    fn char_literal_hex_escape_lex() {
        // `'\x41'`
        assert_eq!(
            kinds(r"'\x41'"),
            vec![TokenKind::CharLiteral, TokenKind::Eof],
        );
    }

    #[test]
    fn char_literal_unicode_escape_lex() {
        // `'\u{1F600}'`
        assert_eq!(
            kinds(r"'\u{1F600}'"),
            vec![TokenKind::CharLiteral, TokenKind::Eof],
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

    // ─── T2-D8 apostrophe decomposition + morpheme-suffix fold ───────────────

    #[test]
    fn morpheme_fold_single_letter() {
        // `base'd` — `d` is a morpheme letter, adjacent to apostrophe → Suffix.
        assert_eq!(
            kinds("base'd"),
            vec![
                TokenKind::Ident,
                TokenKind::Suffix(TypeSuffix::Data),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn morpheme_fold_rule_letter() {
        assert_eq!(
            kinds("entity'r"),
            vec![
                TokenKind::Ident,
                TokenKind::Suffix(TypeSuffix::Rule),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn multi_letter_refinement_tag_emits_three_tokens() {
        // `f32'pos` — `pos` is multi-letter, NOT folded ; emits Ident + Apostrophe + Ident.
        assert_eq!(
            kinds("f32'pos"),
            vec![
                TokenKind::Ident,
                TokenKind::Apostrophe,
                TokenKind::Ident,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn non_morpheme_single_letter_emits_three_tokens() {
        // `T'L` — `L` is a single letter but NOT a morpheme (the 9 are : d f s t e m p g r).
        assert_eq!(
            kinds("T'L"),
            vec![
                TokenKind::Ident,
                TokenKind::Apostrophe,
                TokenKind::Ident,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn whitespace_between_ident_and_apostrophe_does_not_fold() {
        // `foo 'd` — whitespace breaks adjacency ; no fold.
        assert_eq!(
            kinds("foo 'd"),
            vec![
                TokenKind::Ident,
                TokenKind::Apostrophe,
                TokenKind::Ident,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn lifetime_like_not_folded() {
        // `<'r>` — preceding token is `<`, not an Ident ; no fold.
        assert_eq!(
            kinds("<'r>"),
            vec![
                TokenKind::Lt,
                TokenKind::Apostrophe,
                TokenKind::Ident,
                TokenKind::Gt,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn integer_type_suffix_intact() {
        // `42'i32` — handled by int-lexer's own suffix regex, emits one IntLiteral.
        assert_eq!(kinds("42'i32"), vec![TokenKind::IntLiteral, TokenKind::Eof],);
    }

    #[test]
    fn char_literal_still_wins_over_apostrophe() {
        // `'c'` — CharLiteral (longer match) ; no Apostrophe tokens.
        assert_eq!(kinds("'c'"), vec![TokenKind::CharLiteral, TokenKind::Eof],);
    }

    #[test]
    fn ident_after_longer_attachment_not_folded() {
        // `x'do` — `do` is 2 chars, not single-letter ; no fold.
        assert_eq!(
            kinds("x'do"),
            vec![
                TokenKind::Ident,
                TokenKind::Apostrophe,
                TokenKind::Ident,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn morpheme_span_covers_apostrophe_plus_letter() {
        // Suffix token's span should start at the apostrophe and end after the letter.
        let file = mk("x'd");
        let toks = lex(&file);
        // Expect : Ident(0..1) + Suffix(1..3) + Eof
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0].span.start, 0);
        assert_eq!(toks[0].span.end, 1);
        assert_eq!(toks[1].span.start, 1);
        assert_eq!(toks[1].span.end, 3);
        assert_eq!(toks[1].kind, TokenKind::Suffix(TypeSuffix::Data));
    }
}
