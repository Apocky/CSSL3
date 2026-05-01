//! Unified token type covering both Rust-hybrid and CSLv3-native surfaces.
//!
//! § DESIGN
//!   A single `TokenKind` enum carries variants from both surfaces. Each surface lexer
//!   emits only the subset that is legal for its grammar; downstream consumers match
//!   on the full set. This keeps the type-system honest (cross-surface ambiguity is
//!   a hard error, not a silent conflation) while letting shared infrastructure
//!   (Span carrying, span-to-location mapping, diagnostic rendering) run over
//!   a single type.
//!
//! § SPEC SOURCES
//!   - Rust-hybrid : `specs/09_SYNTAX.csl` §§ lexical + keywords + operators.
//!   - CSLv3-native :
//!     - `CSLv3/specs/12_TOKENIZER.csl` (74-glyph master alias table + BPE costs)
//!     - `CSLv3/specs/13_GRAMMAR_SELF.csl` token-classes + morpheme + slot-template.

use cssl_ast::Span;

// ════════════════════════════════════════════════════════════════════════════
// § Token (kind + span)
// ════════════════════════════════════════════════════════════════════════════

/// A single lexeme with its owning byte-offset span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// Kind of the token.
    pub kind: TokenKind,
    /// Byte-offset span in the originating `SourceFile`.
    pub span: Span,
}

impl Token {
    /// Build a new `Token` from its parts.
    #[must_use]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § TokenKind
// ════════════════════════════════════════════════════════════════════════════

/// Kind of a lexical token. Variants cover both surface grammars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // ─ literals (shared) ──────────────────────────────────────────────────
    /// Identifier or keyword-like word. Surface lexer decides whether to
    /// promote to `Keyword(_)`.
    Ident,
    /// Integer literal (optional `'<suffix>`).
    IntLiteral,
    /// Float literal (optional `'<suffix>`).
    FloatLiteral,
    /// String literal (normal, raw, or CSLv3-native dense).
    StringLiteral(StringFlavor),
    /// Character literal `'c'`.
    CharLiteral,
    /// Morpheme / type suffix immediately following another lexeme.
    ///
    /// The suffix itself is a short `'<letter>` sequence per §§ 13_GRAMMAR_SELF
    /// `type-suffix` enum : `'d 'f 's 't 'e 'm 'p 'g 'r`.
    Suffix(TypeSuffix),

    // ─ keywords (Rust-hybrid only) ────────────────────────────────────────
    /// A keyword from the Rust-hybrid surface.
    Keyword(Keyword),

    // ─ punctuation (shared) ───────────────────────────────────────────────
    /// Opening or closing bracket of a given kind.
    Bracket(BracketKind, BracketSide),
    /// `,`
    Comma,
    /// `;`
    Semi,
    /// `:`
    Colon,
    /// `::` (also `∷` in Unicode)
    ColonColon,
    /// `.`
    Dot,
    /// `..`
    DotDot,
    /// `..=`
    DotDotEq,
    /// `@` — attribute prefix (Rust-hybrid) or `AV` compound-op (CSLv3-native).
    At,
    /// `#` — used by `#run`, `#[…]` in Rust-hybrid.
    Hash,
    /// `$` — reserved for positional macro args (§§ 13).
    Dollar,
    /// `'` — standalone apostrophe. Introduces refinement tags (`T'tag`), type-suffix
    /// words (`42'i32`, `f32'pos`), Lipschitz-bound markers (`SDF'L<k>`), and lifetime-like
    /// annotations. When followed by a single recognized morpheme letter at word-boundary
    /// the lexer instead emits `Suffix(TypeSuffix)` atomically; any other attachment surfaces
    /// as `Apostrophe` followed by the next token.
    Apostrophe,
    /// `?` — question / try.
    Question,
    /// `??` — null-coalesce / early-return-default (§§ 09 operator table).
    QuestionQuestion,

    // ─ arithmetic / bitwise / comparison (shared) ─────────────────────────
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `=`
    Eq,
    /// `==` (also `≡`)
    EqEq,
    /// `!=` (also `≠`)
    Ne,
    /// `<`
    Lt,
    /// `<=` (also `≤`)
    Le,
    /// `>`
    Gt,
    /// `>=` (also `≥`)
    Ge,
    /// `&`
    Amp,
    /// `|`
    Pipe,
    /// `^`
    Caret,
    /// `~`
    Tilde,
    /// `!`
    Bang,
    /// `&&` (also `∧`)
    AmpAmp,
    /// `||` (also `∨`)
    PipePipe,
    /// `<<`
    LShift,
    /// `>>`
    RShift,

    // ─ flow arrows (shared ; ASCII + Unicode both accepted) ───────────────
    /// `->` / `→`  fn-return + flow.
    Arrow,
    /// `<-` / `←`  source-of / data-flow.
    LeftArrow,
    /// `<->` / `↔` bi-directional.
    BiArrow,
    /// `=>` / `⇒`  match-arm + implies.
    FatArrow,
    /// `|>` / `▷`  pipeline forward.
    PipeArrow,
    /// `<|`  pipeline backward (CSLv3-native).
    PipeArrowBack,
    /// `~>`  causes / triggers (CSLv3-native).
    SquigglyArrow,

    // ─ CSLv3-native structural (§§ 12_TOKENIZER tier-0) ───────────────────
    /// `§`  section-marker (single).
    Section,
    /// `§§`  cross-reference / section-reference (double).
    SectionRef,

    // ─ CSLv3-native evidence / modal / compound / determinative ───────────
    /// Evidence marker : `✓ ◐ ○ ✗ ⊘ △ ▽ ‼`.
    Evidence(EvidenceMark),
    /// Modal operator : `W! R! M? N! I> Q? P> D>`.
    Modal(ModalOp),
    /// Compound operator : `.` / `+` / `-` / `⊗` / `@` (contextual within CSLv3-native).
    Compound(CompoundOp),
    /// Determinative delimiter : one of the §§ 12 enclosure pairs (`⟨⟩ ⟦⟧ ⌈⌉ ⌊⌋ «» ⟪⟫`).
    Determinative(Determinative, BracketSide),

    // ─ CSLv3-native dense-math / engine glyphs ────────────────────────────
    /// `∀` / ASCII `all`
    ForAll,
    /// `∃` / ASCII `any`
    Exists,
    /// `∈` / ASCII `in`
    ElemOf,
    /// `∉` / ASCII `!in`
    NotElemOf,
    /// `⊂` / ASCII `<:`
    Subset,
    /// `⊃` / ASCII `:>`
    Superset,
    /// `∴` / ASCII `.:.`
    Therefore,
    /// `∵` / ASCII `:..`
    Because,
    /// `⊢` / ASCII `|-`
    Entails,
    /// `∎` / ASCII `QED`
    Qed,
    /// `∅` / ASCII `nil`
    EmptySet,
    /// `∞` / ASCII `inf`
    Infinity,

    // ─ layout (both surfaces) ─────────────────────────────────────────────
    /// Significant newline. Suppressed inside bracket contexts by the lexer.
    Newline,
    /// Indent (CSLv3-native block open).
    Indent,
    /// Dedent (CSLv3-native block close).
    Dedent,
    /// Whitespace run — emitted only in trivia-preserving mode (formatter path).
    Whitespace,
    /// Line comment `// …` or CSLv3-native CoT-line `§ I> …` / `§ W! …`.
    LineComment,
    /// Block comment `/* … */` or CSLv3-native CoT-block `§{ … §}`.
    BlockComment,

    // ─ terminators ────────────────────────────────────────────────────────
    /// End of file.
    Eof,
    /// Unrecognized byte sequence — emitted with a `Span` and a diagnostic.
    ///
    /// Downstream code reads the slice via `SourceFile::slice` and emits a
    /// labelled `Diagnostic`.
    Error,
}

// ════════════════════════════════════════════════════════════════════════════
// § Keyword (Rust-hybrid)
// ════════════════════════════════════════════════════════════════════════════

/// Rust-hybrid keyword. Reserved word recognized by the Rust-hybrid lexer only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keyword {
    // ─ item / binding ─
    /// `fn`
    Fn,
    /// `let`
    Let,
    /// `const`
    Const,
    /// `mut`
    Mut,
    /// `pub`
    Pub,
    /// `use`
    Use,
    /// `module`
    Module,
    /// `type`
    Type,
    /// `struct`
    Struct,
    /// `enum`
    Enum,
    /// `interface`
    Interface,
    /// `impl`
    Impl,
    /// `extern` — used in `extern fn name(params) -> ret` FFI-decl items.
    /// Stage-0 ABI is implicit "C" ; future revisions may accept `extern "abi"`.
    Extern,

    // ─ control flow ─
    /// `if`
    If,
    /// `else`
    Else,
    /// `match`
    Match,
    /// `while`
    While,
    /// `for`
    For,
    /// `in`
    In,
    /// `return`
    Return,
    /// `break`
    Break,
    /// `continue`
    Continue,
    /// `loop`
    Loop,
    /// `where`
    Where,

    // ─ effects ─
    /// `effect`
    Effect,
    /// `handler`
    Handler,
    /// `perform`
    Perform,
    /// `with`
    With,
    /// `region`
    Region,

    // ─ Pony-6 capabilities ─
    /// `iso`
    Iso,
    /// `trn`
    Trn,
    /// `ref`
    Ref,
    /// `val`
    Val,
    /// `box`
    Box,
    /// `tag`
    Tag,

    // ─ staging / comptime ─
    /// `comptime`
    Comptime,
    /// `#run` (the `run` part is tokenized as keyword after a `#`).
    Run,

    // ─ literals ─
    /// `true`
    True,
    /// `false`
    False,

    // ─ casts ─
    /// `as`
    As,
    /// `self`
    SelfValue,
    /// `Self`
    SelfType,
}

impl Keyword {
    /// Map a source-text word to a keyword, if recognized. Case-sensitive.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "fn" => Self::Fn,
            "let" => Self::Let,
            "const" => Self::Const,
            "mut" => Self::Mut,
            "pub" => Self::Pub,
            "use" => Self::Use,
            "module" => Self::Module,
            "type" => Self::Type,
            "struct" => Self::Struct,
            "enum" => Self::Enum,
            "interface" => Self::Interface,
            "impl" => Self::Impl,
            "extern" => Self::Extern,
            "if" => Self::If,
            "else" => Self::Else,
            "match" => Self::Match,
            "while" => Self::While,
            "for" => Self::For,
            "in" => Self::In,
            "return" => Self::Return,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "loop" => Self::Loop,
            "where" => Self::Where,
            "effect" => Self::Effect,
            "handler" => Self::Handler,
            "perform" => Self::Perform,
            "with" => Self::With,
            "region" => Self::Region,
            "iso" => Self::Iso,
            "trn" => Self::Trn,
            "ref" => Self::Ref,
            "val" => Self::Val,
            "box" => Self::Box,
            "tag" => Self::Tag,
            "comptime" => Self::Comptime,
            "run" => Self::Run,
            "true" => Self::True,
            "false" => Self::False,
            "as" => Self::As,
            "self" => Self::SelfValue,
            "Self" => Self::SelfType,
            _ => return None,
        })
    }

    /// Canonical source-form of the keyword.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fn => "fn",
            Self::Let => "let",
            Self::Const => "const",
            Self::Mut => "mut",
            Self::Pub => "pub",
            Self::Use => "use",
            Self::Module => "module",
            Self::Type => "type",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Impl => "impl",
            Self::Extern => "extern",
            Self::If => "if",
            Self::Else => "else",
            Self::Match => "match",
            Self::While => "while",
            Self::For => "for",
            Self::In => "in",
            Self::Return => "return",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::Loop => "loop",
            Self::Where => "where",
            Self::Effect => "effect",
            Self::Handler => "handler",
            Self::Perform => "perform",
            Self::With => "with",
            Self::Region => "region",
            Self::Iso => "iso",
            Self::Trn => "trn",
            Self::Ref => "ref",
            Self::Val => "val",
            Self::Box => "box",
            Self::Tag => "tag",
            Self::Comptime => "comptime",
            Self::Run => "run",
            Self::True => "true",
            Self::False => "false",
            Self::As => "as",
            Self::SelfValue => "self",
            Self::SelfType => "Self",
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Bracket
// ════════════════════════════════════════════════════════════════════════════

/// Bracket kind — plain parens / braces / brackets. Determinatives have their own enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BracketKind {
    /// `(` `)`
    Paren,
    /// `{` `}`
    Brace,
    /// `[` `]`
    Square,
}

/// Which side of a bracket pair a token represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BracketSide {
    /// Opening bracket.
    Open,
    /// Closing bracket.
    Close,
}

// ════════════════════════════════════════════════════════════════════════════
// § CSLv3-native sub-enums
// ════════════════════════════════════════════════════════════════════════════

/// Evidence marker — per `CSLv3/specs/13_GRAMMAR_SELF.csl` `evidence-mark` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceMark {
    /// `✓` / `[x]` — confirmed.
    Confirmed,
    /// `◐` / `[~]` — partial.
    Partial,
    /// `○` / `[ ]` — pending.
    Pending,
    /// `✗` / `[!]` — failed.
    Failed,
    /// `⊘` / `[?]` — unknown.
    Unknown,
    /// `△` / `[^]` — hypothetical.
    Hypothetical,
    /// `▽` / `[v]` — deprecated.
    Deprecated,
    /// `‼` / `[!!]` — proven.
    Proven,
}

/// Modal operator — per `CSLv3/specs/13_GRAMMAR_SELF.csl` `modal-op` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModalOp {
    /// `W!` — must.
    Must,
    /// `R!` — should.
    Should,
    /// `M?` — may.
    May,
    /// `N!` — must-not.
    MustNot,
    /// `I>` — insight.
    Insight,
    /// `Q?` — question.
    Question,
    /// `P>` — push further.
    PushFurther,
    /// `D>` — decision.
    Decision,
    /// `TODO` — todo (bareword modal).
    Todo,
    /// `FIXME` — fixme (bareword modal).
    Fixme,
}

/// Compound operator — per `CSLv3/specs/13_GRAMMAR_SELF.csl` `compound-op` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompoundOp {
    /// `.` — TP (B-of-A).
    Tp,
    /// `+` — DV ({A,B} co-equal).
    Dv,
    /// `-` — KD (B-that-is-A).
    Kd,
    /// `⊗` / ASCII `x*` — BV (thing-having-A+B).
    Bv,
    /// `@` — AV (at/per/in-scope-of X).
    Av,
}

/// Determinative delimiter — enclosure pair per `CSLv3/specs/12_TOKENIZER.csl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Determinative {
    /// `⟨ ⟩` — tuple / record.
    AngleTuple,
    /// `⟦ ⟧` — formula.
    Formula,
    /// `⌈ ⌉` — constraint.
    Constraint,
    /// `⌊ ⌋` — precondition.
    Precondition,
    /// `« »` — quotation.
    Quotation,
    /// `⟪ ⟫` — temporal.
    Temporal,
}

/// Morpheme / type suffix — per `CSLv3/specs/13_GRAMMAR_SELF.csl` `type-suffix` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeSuffix {
    /// `'d` — data.
    Data,
    /// `'f` — func.
    Func,
    /// `'s` — system.
    System,
    /// `'t` — type.
    Type,
    /// `'e` — entity.
    Entity,
    /// `'m` — material.
    Material,
    /// `'p` — property.
    Property,
    /// `'g` — gate.
    Gate,
    /// `'r` — rule.
    Rule,
}

impl TypeSuffix {
    /// Parse the single-letter suffix following an apostrophe. Returns `None`
    /// for unrecognized letters (lexer emits `TokenKind::Error` in that case).
    #[must_use]
    pub const fn from_letter(letter: char) -> Option<Self> {
        Some(match letter {
            'd' => Self::Data,
            'f' => Self::Func,
            's' => Self::System,
            't' => Self::Type,
            'e' => Self::Entity,
            'm' => Self::Material,
            'p' => Self::Property,
            'g' => Self::Gate,
            'r' => Self::Rule,
            _ => return None,
        })
    }

    /// The suffix body letter (without apostrophe).
    #[must_use]
    pub const fn letter(self) -> char {
        match self {
            Self::Data => 'd',
            Self::Func => 'f',
            Self::System => 's',
            Self::Type => 't',
            Self::Entity => 'e',
            Self::Material => 'm',
            Self::Property => 'p',
            Self::Gate => 'g',
            Self::Rule => 'r',
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § String flavor
// ════════════════════════════════════════════════════════════════════════════

/// Flavor of a string literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringFlavor {
    /// `"…"` with `\n` `\t` `\\` `\"` escape processing.
    Normal,
    /// `r"…"` or `r#"…"#` — no escape processing.
    Raw,
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::{Keyword, Token, TokenKind, TypeSuffix};
    use cssl_ast::{SourceId, Span};

    #[test]
    fn token_is_copyable() {
        let span = Span::new(SourceId::first(), 0, 2);
        let t = Token::new(TokenKind::Ident, span);
        let _ = t;
        let t2 = t;
        assert_eq!(t.span, t2.span);
    }

    #[test]
    fn keyword_roundtrip_through_from_word() {
        for kw in [
            Keyword::Fn,
            Keyword::Let,
            Keyword::Pub,
            Keyword::Iso,
            Keyword::Handler,
            Keyword::SelfValue,
            Keyword::SelfType,
        ] {
            assert_eq!(Keyword::from_word(kw.as_str()), Some(kw));
        }
    }

    #[test]
    fn unknown_keyword_returns_none() {
        assert_eq!(Keyword::from_word("not_a_keyword"), None);
        assert_eq!(Keyword::from_word(""), None);
    }

    #[test]
    fn type_suffix_letters_roundtrip() {
        for s in [
            TypeSuffix::Data,
            TypeSuffix::Func,
            TypeSuffix::System,
            TypeSuffix::Type,
            TypeSuffix::Entity,
            TypeSuffix::Material,
            TypeSuffix::Property,
            TypeSuffix::Gate,
            TypeSuffix::Rule,
        ] {
            assert_eq!(TypeSuffix::from_letter(s.letter()), Some(s));
        }
        assert_eq!(TypeSuffix::from_letter('x'), None);
    }

    #[test]
    fn keyword_count_matches_expectation() {
        // § I> if this drops when a keyword is removed, update both counters
        //      (from_word + as_str) and this test.
        let known = [
            "fn",
            "let",
            "const",
            "mut",
            "pub",
            "use",
            "module",
            "type",
            "struct",
            "enum",
            "interface",
            "impl",
            "if",
            "else",
            "match",
            "while",
            "for",
            "in",
            "return",
            "break",
            "continue",
            "loop",
            "where",
            "effect",
            "handler",
            "perform",
            "with",
            "region",
            "iso",
            "trn",
            "ref",
            "val",
            "box",
            "tag",
            "comptime",
            "run",
            "true",
            "false",
            "as",
            "self",
            "Self",
        ];
        assert_eq!(known.len(), 41);
        for w in known {
            assert!(Keyword::from_word(w).is_some(), "missing keyword: {w}");
        }
    }
}
