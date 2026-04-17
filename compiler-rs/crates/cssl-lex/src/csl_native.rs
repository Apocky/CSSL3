//! CSLv3-native surface lexer — hand-rolled Rust port of the canonical CSLv3 grammar.
//!
//! § SPEC SOURCES (authoritative, both from the sibling `CSLv3` repo)
//!   - `CSLv3/specs/12_TOKENIZER.csl` : 74-glyph master alias table + BPE-cost discipline.
//!   - `CSLv3/specs/13_GRAMMAR_SELF.csl` : token-classes, slot-template, compound formation,
//!     morpheme stacking, parse-priority, LL(2) invariant, Peircean juxtaposition,
//!     2-space indent = scope-boundary.
//! § REFERENCE IMPL : `CSLv3/parser/parser.odin` (Odin) — used as CI differential oracle
//!     only. This Rust port is authoritative for CSSLv3's consumption of the surface
//!     (see `DECISIONS.md` T1-D2).
//! § POLICY : divergence between this lexer's output and `parser.exe --tokens` on
//!     shared fixtures is a spec-ambiguity. File against CSLv3 (not CSSLv3).
//!
//! § ALGORITHM
//!   Byte-stream scan with `indent_stack` tracking. At the start of every non-blank,
//!   non-bracketed line the leading-space count is compared to the stack top:
//!     - greater → push, emit `Indent`
//!     - lower   → pop (emit `Dedent` per pop) until match
//!     - equal   → no emit
//!   Inside any bracket / determinative pair, newlines + indent changes are suppressed.
//!   EOF emits trailing `Dedent`s to close any still-open scopes, then `Eof`.
//!
//! § CURRENT-SCOPE
//!   - Structural : `§` (Section) / `§§` (SectionRef) / `∎` (Qed)
//!   - Evidence   : all 8 Unicode + all 8 ASCII aliases
//!   - Modal      : W! R! M? N! I> Q? P> D>
//!   - Dense math : ∀ ∃ ∈ ∉ ⊂ ⊃ ∴ ∵ ⊢ ∅ ∞ ⊗ ≡ ≠ ≤ ≥ ∧ ∨ ¬ and their ASCII aliases
//!   - Arrows     : → ← ↔ ⇒ ▷ (shared with Rust-hybrid)
//!   - Determinatives : ⟨⟩ ⟦⟧ ⌈⌉ ⌊⌋ «» ⟪⟫
//!   - Type suffix : 'd 'f 's 't 'e 'm 'p 'g 'r
//!   - Identifiers, integer / float literals, normal strings
//!   - Indent / Dedent / Newline layout tokens
//!   - `# …` line comment
//! § DEFERRED-TO-LATER-TURN
//!   - Slot-template determinative (§§ 13 `[EVIDENCE?] [MODAL?] [DET?] …`) — parser layer.
//!   - Morpheme-stack `base.aspect.modality.certainty.scope` — parser layer.
//!   - All-ASCII `<|` `|>` pipelines, `<->`, and other multi-char ASCII ops shared with Rust-hybrid.

use core::cmp::Ordering;

use cssl_ast::{SourceFile, SourceId, Span};

use crate::token::{
    BracketKind, BracketSide, CompoundOp, Determinative, EvidenceMark, ModalOp, StringFlavor,
    Token, TokenKind, TypeSuffix,
};

/// Lex a CSLv3-native source file into a `Vec<Token>`, terminated by `TokenKind::Eof`.
///
/// Unrecognized byte sequences yield a `TokenKind::Error` token carrying the offending span;
/// the caller is responsible for translating into `Diagnostic` records.
#[must_use]
pub fn lex(source: &SourceFile) -> Vec<Token> {
    Lexer::new(source).run()
}

// ════════════════════════════════════════════════════════════════════════════
// § Lexer
// ════════════════════════════════════════════════════════════════════════════

struct Lexer<'a> {
    source_id: SourceId,
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    indent_stack: Vec<u32>,
    bracket_depth: u32,
    at_line_start: bool,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Self {
            source_id: source.id,
            text: &source.contents,
            bytes: source.contents.as_bytes(),
            pos: 0,
            indent_stack: vec![0],
            bracket_depth: 0,
            at_line_start: true,
            tokens: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Token> {
        while self.pos < self.bytes.len() {
            if self.at_line_start && self.bracket_depth == 0 {
                self.handle_line_start();
            }
            self.at_line_start = false;
            if self.pos >= self.bytes.len() {
                break;
            }
            self.lex_one();
        }
        // close any still-open indents before EOF
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.emit_empty(TokenKind::Dedent);
        }
        self.emit_empty(TokenKind::Eof);
        self.tokens
    }

    // ─── indent handling ────────────────────────────────────────────────────
    fn handle_line_start(&mut self) {
        let mut col: u32 = 0;
        let start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b' ' => {
                    col += 1;
                    self.pos += 1;
                }
                b'\t' => {
                    // § I> §§ 13 invariant : 2-space indent ; treat tab as 4 spaces for tolerance
                    col += 4;
                    self.pos += 1;
                }
                _ => break,
            }
        }
        // blank line / comment-only line : do not perturb the indent stack
        match self.bytes.get(self.pos) {
            None | Some(b'\n' | b'#') => return,
            Some(&b'\r') if self.bytes.get(self.pos + 1) == Some(&b'\n') => return,
            _ => {}
        }
        let top = *self
            .indent_stack
            .last()
            .expect("indent_stack has sentinel 0");
        match col.cmp(&top) {
            Ordering::Greater => {
                self.indent_stack.push(col);
                self.emit(TokenKind::Indent, start as u32, self.pos as u32);
            }
            Ordering::Less => {
                while let Some(&top_now) = self.indent_stack.last() {
                    if col >= top_now {
                        break;
                    }
                    self.indent_stack.pop();
                    self.emit(TokenKind::Dedent, start as u32, self.pos as u32);
                }
            }
            Ordering::Equal => {}
        }
    }

    // ─── primary dispatch ───────────────────────────────────────────────────
    fn lex_one(&mut self) {
        let start = self.pos;
        let Some(&first) = self.bytes.get(self.pos) else {
            return;
        };

        // horizontal whitespace
        if first == b' ' || first == b'\t' {
            self.pos += 1;
            while matches!(self.bytes.get(self.pos), Some(&(b' ' | b'\t'))) {
                self.pos += 1;
            }
            return;
        }

        // newline
        if first == b'\n' {
            self.pos += 1;
            if self.bracket_depth == 0 {
                self.emit(TokenKind::Newline, start as u32, self.pos as u32);
                self.at_line_start = true;
            }
            return;
        }
        if first == b'\r' {
            // bare \r or \r\n — consume silently; \n handling happens on next iter
            self.pos += 1;
            return;
        }

        // line comment `# …`
        if first == b'#' {
            while let Some(&b) = self.bytes.get(self.pos) {
                if b == b'\n' {
                    break;
                }
                self.pos += 1;
            }
            self.emit(TokenKind::LineComment, start as u32, self.pos as u32);
            return;
        }

        // ASCII two-char constructs : modal ops, comparison, arrows, evidence aliases
        if let Some(kind) = self.try_ascii_evidence_alias() {
            self.emit(kind, start as u32, self.pos as u32);
            return;
        }
        if let Some(kind) = self.try_ascii_multichar() {
            self.emit(kind, start as u32, self.pos as u32);
            return;
        }

        // brackets + determinatives (single-char or single-Unicode)
        if let Some(kind) = self.try_bracket(first) {
            self.emit(kind, start as u32, self.pos as u32);
            return;
        }

        // numbers
        if first.is_ascii_digit() {
            self.lex_number(start);
            return;
        }

        // strings
        if first == b'"' {
            self.lex_string(start);
            return;
        }

        // identifier
        if first == b'_' || first.is_ascii_alphabetic() {
            self.lex_identifier(start);
            return;
        }

        // type suffix `'x` (only meaningful after ident/number, but safe to emit here)
        if first == b'\'' {
            if let Some(&letter) = self.bytes.get(self.pos + 1) {
                if let Some(suffix) = TypeSuffix::from_letter(letter as char) {
                    // ensure it's a real suffix, not start of char literal
                    let after = self.bytes.get(self.pos + 2).copied();
                    let is_ident_continuation =
                        matches!(after, Some(b) if b.is_ascii_alphanumeric() || b == b'_');
                    if !is_ident_continuation {
                        self.pos += 2;
                        self.emit(TokenKind::Suffix(suffix), start as u32, self.pos as u32);
                        return;
                    }
                }
            }
        }

        // single-char ASCII ops / punctuation
        if let Some(kind) = Self::try_ascii_single(first) {
            self.pos += 1;
            self.emit(kind, start as u32, self.pos as u32);
            return;
        }

        // Unicode dispatch : consume one code-point and try glyph dispatch
        if first >= 0x80 {
            if let Some(kind) = self.try_unicode_glyph() {
                self.emit(kind, start as u32, self.pos as u32);
                return;
            }
        }

        // fallback : error token of one byte (or one code-point for multi-byte)
        self.advance_one_char();
        self.emit(TokenKind::Error, start as u32, self.pos as u32);
    }

    // ─── helpers ────────────────────────────────────────────────────────────

    fn advance_one_char(&mut self) {
        let remaining = &self.text[self.pos..];
        if let Some(c) = remaining.chars().next() {
            self.pos += c.len_utf8();
        } else {
            self.pos += 1;
        }
    }

    fn current_char(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn emit(&mut self, kind: TokenKind, start: u32, end: u32) {
        self.tokens
            .push(Token::new(kind, Span::new(self.source_id, start, end)));
    }

    fn emit_empty(&mut self, kind: TokenKind) {
        let p = u32::try_from(self.pos).unwrap_or(u32::MAX);
        self.tokens
            .push(Token::new(kind, Span::new(self.source_id, p, p)));
    }

    // ─── try_* submodule patterns ───────────────────────────────────────────

    fn try_ascii_evidence_alias(&mut self) -> Option<TokenKind> {
        let rest = self.bytes.get(self.pos..)?;
        let (mark, width) = match rest {
            b if b.starts_with(b"[!!]") => (EvidenceMark::Proven, 4),
            b if b.starts_with(b"[x]") => (EvidenceMark::Confirmed, 3),
            b if b.starts_with(b"[~]") => (EvidenceMark::Partial, 3),
            b if b.starts_with(b"[ ]") => (EvidenceMark::Pending, 3),
            b if b.starts_with(b"[!]") => (EvidenceMark::Failed, 3),
            b if b.starts_with(b"[?]") => (EvidenceMark::Unknown, 3),
            b if b.starts_with(b"[^]") => (EvidenceMark::Hypothetical, 3),
            b if b.starts_with(b"[v]") => (EvidenceMark::Deprecated, 3),
            _ => return None,
        };
        self.pos += width;
        Some(TokenKind::Evidence(mark))
    }

    fn try_ascii_multichar(&mut self) -> Option<TokenKind> {
        let rest = self.bytes.get(self.pos..)?;

        // modal ops (word-like — 2 chars at a word-boundary)
        if rest.len() >= 2 {
            let pair = &rest[..2];
            let ok = match pair {
                b"W!" => Some(ModalOp::Must),
                b"R!" => Some(ModalOp::Should),
                b"M?" => Some(ModalOp::May),
                b"N!" => Some(ModalOp::MustNot),
                b"I>" => Some(ModalOp::Insight),
                b"Q?" => Some(ModalOp::Question),
                b"P>" => Some(ModalOp::PushFurther),
                b"D>" => Some(ModalOp::Decision),
                _ => None,
            };
            if let Some(m) = ok {
                // require word-boundary on both sides (simpler : require leading SOF or whitespace / punctuation)
                let boundary_before = self.pos == 0
                    || matches!(self.bytes.get(self.pos - 1), Some(b) if !b.is_ascii_alphanumeric());
                if boundary_before {
                    self.pos += 2;
                    return Some(TokenKind::Modal(m));
                }
            }
        }

        // arrows + comparison multi-char
        if rest.starts_with(b"..=") {
            self.pos += 3;
            return Some(TokenKind::DotDotEq);
        }
        if rest.starts_with(b"<->") {
            self.pos += 3;
            return Some(TokenKind::BiArrow);
        }
        let two: &[u8] = rest.get(..2)?;
        let tk = match two {
            b"->" => TokenKind::Arrow,
            b"<-" => TokenKind::LeftArrow,
            b"=>" => TokenKind::FatArrow,
            b"|>" => TokenKind::PipeArrow,
            b"<|" => TokenKind::PipeArrowBack,
            b"~>" => TokenKind::SquigglyArrow,
            b"==" => TokenKind::EqEq,
            b"!=" => TokenKind::Ne,
            b"<=" => TokenKind::Le,
            b">=" => TokenKind::Ge,
            b"&&" => TokenKind::AmpAmp,
            b"||" => TokenKind::PipePipe,
            b"<<" => TokenKind::LShift,
            b">>" => TokenKind::RShift,
            b"::" => TokenKind::ColonColon,
            b".." => TokenKind::DotDot,
            b"??" => TokenKind::QuestionQuestion,
            _ => return None,
        };
        self.pos += 2;
        Some(tk)
    }

    fn try_bracket(&mut self, b: u8) -> Option<TokenKind> {
        let kind = match b {
            b'(' => {
                self.bracket_depth += 1;
                TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)
            }
            b')' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)
            }
            b'{' => {
                self.bracket_depth += 1;
                TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)
            }
            b'}' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)
            }
            b'[' => {
                self.bracket_depth += 1;
                TokenKind::Bracket(BracketKind::Square, BracketSide::Open)
            }
            b']' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Bracket(BracketKind::Square, BracketSide::Close)
            }
            _ => return None,
        };
        self.pos += 1;
        Some(kind)
    }

    fn try_ascii_single(b: u8) -> Option<TokenKind> {
        Some(match b {
            b'+' => TokenKind::Plus,
            b'-' => TokenKind::Minus,
            b'*' => TokenKind::Star,
            b'/' => TokenKind::Slash,
            b'%' => TokenKind::Percent,
            b'=' => TokenKind::Eq,
            b'<' => TokenKind::Lt,
            b'>' => TokenKind::Gt,
            b'&' => TokenKind::Amp,
            b'|' => TokenKind::Pipe,
            b'^' => TokenKind::Caret,
            b'~' => TokenKind::Tilde,
            b'!' => TokenKind::Bang,
            b',' => TokenKind::Comma,
            b';' => TokenKind::Semi,
            b':' => TokenKind::Colon,
            b'.' => TokenKind::Dot,
            b'?' => TokenKind::Question,
            b'@' => TokenKind::At,
            b'$' => TokenKind::Dollar,
            _ => return None,
        })
    }

    fn try_unicode_glyph(&mut self) -> Option<TokenKind> {
        let c = self.current_char()?;
        let kind = match c {
            // structural
            '§' => {
                // double § → SectionRef
                let after = self.pos + c.len_utf8();
                if self.text[after..].starts_with('§') {
                    self.pos = after + '§'.len_utf8();
                    return Some(TokenKind::SectionRef);
                }
                TokenKind::Section
            }
            '∎' => TokenKind::Qed,
            // evidence (unicode)
            '✓' => TokenKind::Evidence(EvidenceMark::Confirmed),
            '◐' => TokenKind::Evidence(EvidenceMark::Partial),
            '○' => TokenKind::Evidence(EvidenceMark::Pending),
            '✗' => TokenKind::Evidence(EvidenceMark::Failed),
            '⊘' => TokenKind::Evidence(EvidenceMark::Unknown),
            '△' => TokenKind::Evidence(EvidenceMark::Hypothetical),
            '▽' => TokenKind::Evidence(EvidenceMark::Deprecated),
            '‼' => TokenKind::Evidence(EvidenceMark::Proven),
            // dense math
            '∀' => TokenKind::ForAll,
            '∃' => TokenKind::Exists,
            '∈' => TokenKind::ElemOf,
            '∉' => TokenKind::NotElemOf,
            '⊂' => TokenKind::Subset,
            '⊃' => TokenKind::Superset,
            '∴' => TokenKind::Therefore,
            '∵' => TokenKind::Because,
            '⊢' => TokenKind::Entails,
            '∅' => TokenKind::EmptySet,
            '∞' => TokenKind::Infinity,
            '⊗' => TokenKind::Compound(CompoundOp::Bv),
            // comparison / logic unicode aliases
            '≡' => TokenKind::EqEq,
            '≠' => TokenKind::Ne,
            '≤' => TokenKind::Le,
            '≥' => TokenKind::Ge,
            '∧' => TokenKind::AmpAmp,
            '∨' => TokenKind::PipePipe,
            '¬' => TokenKind::Tilde,
            // arrows (shared with Rust-hybrid)
            '→' => TokenKind::Arrow,
            '←' => TokenKind::LeftArrow,
            '↔' => TokenKind::BiArrow,
            '⇒' => TokenKind::FatArrow,
            '▷' => TokenKind::PipeArrow,
            // determinatives (open)
            '⟨' => {
                self.bracket_depth += 1;
                TokenKind::Determinative(Determinative::AngleTuple, BracketSide::Open)
            }
            '⟦' => {
                self.bracket_depth += 1;
                TokenKind::Determinative(Determinative::Formula, BracketSide::Open)
            }
            '⌈' => {
                self.bracket_depth += 1;
                TokenKind::Determinative(Determinative::Constraint, BracketSide::Open)
            }
            '⌊' => {
                self.bracket_depth += 1;
                TokenKind::Determinative(Determinative::Precondition, BracketSide::Open)
            }
            '«' => {
                self.bracket_depth += 1;
                TokenKind::Determinative(Determinative::Quotation, BracketSide::Open)
            }
            '⟪' => {
                self.bracket_depth += 1;
                TokenKind::Determinative(Determinative::Temporal, BracketSide::Open)
            }
            // determinatives (close)
            '⟩' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Determinative(Determinative::AngleTuple, BracketSide::Close)
            }
            '⟧' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Determinative(Determinative::Formula, BracketSide::Close)
            }
            '⌉' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Determinative(Determinative::Constraint, BracketSide::Close)
            }
            '⌋' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Determinative(Determinative::Precondition, BracketSide::Close)
            }
            '»' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Determinative(Determinative::Quotation, BracketSide::Close)
            }
            '⟫' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                TokenKind::Determinative(Determinative::Temporal, BracketSide::Close)
            }
            _ => return None,
        };
        self.pos += c.len_utf8();
        Some(kind)
    }

    fn lex_number(&mut self, start: usize) {
        while matches!(self.bytes.get(self.pos), Some(b) if b.is_ascii_digit() || *b == b'_') {
            self.pos += 1;
        }
        let is_float = matches!(self.bytes.get(self.pos), Some(&b'.'))
            && matches!(self.bytes.get(self.pos + 1), Some(b) if b.is_ascii_digit());
        if is_float {
            self.pos += 1;
            while matches!(self.bytes.get(self.pos), Some(b) if b.is_ascii_digit() || *b == b'_') {
                self.pos += 1;
            }
        }
        // optional type suffix `'X`
        if self.bytes.get(self.pos) == Some(&b'\'') {
            if let Some(&letter) = self.bytes.get(self.pos + 1) {
                if letter.is_ascii_alphabetic() {
                    self.pos += 2;
                    while matches!(self.bytes.get(self.pos), Some(b) if b.is_ascii_alphanumeric() || *b == b'_')
                    {
                        self.pos += 1;
                    }
                }
            }
        }
        let end = self.pos;
        let kind = if is_float {
            TokenKind::FloatLiteral
        } else {
            TokenKind::IntLiteral
        };
        self.emit(kind, start as u32, end as u32);
    }

    fn lex_string(&mut self, start: usize) {
        self.pos += 1; // opening quote
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b'"' => {
                    self.pos += 1;
                    let end = self.pos;
                    self.emit(
                        TokenKind::StringLiteral(StringFlavor::Normal),
                        start as u32,
                        end as u32,
                    );
                    return;
                }
                b'\\' => {
                    // skip escape pair
                    self.pos += 1;
                    if self.pos < self.bytes.len() {
                        self.advance_one_char();
                    }
                }
                b'\n' => break, // unterminated; fall through to error
                _ => self.advance_one_char(),
            }
        }
        // unterminated string
        self.emit(TokenKind::Error, start as u32, self.pos as u32);
    }

    fn lex_identifier(&mut self, start: usize) {
        self.pos += 1; // consume first char
        while let Some(&b) = self.bytes.get(self.pos) {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
                continue;
            }
            break;
        }
        let end = self.pos;
        let text = &self.text[start..end];
        // Recognize CSLv3-native bareword modals (TODO, FIXME) per §§ 13 modal-op enum
        let kind = match text {
            "TODO" => TokenKind::Modal(ModalOp::Todo),
            "FIXME" => TokenKind::Modal(ModalOp::Fixme),
            // ASCII-alias bareword glyphs per §§ 12 (accepted in CSLv3-native mode)
            "all" => TokenKind::ForAll,
            "any" => TokenKind::Exists,
            "in" => TokenKind::ElemOf,
            "nil" => TokenKind::EmptySet,
            "inf" => TokenKind::Infinity,
            "QED" => TokenKind::Qed,
            _ => TokenKind::Ident,
        };
        self.emit(kind, start as u32, end as u32);
        // optional trailing type-suffix `'X` (single-letter, not identifier-continuation)
        if self.bytes.get(self.pos) == Some(&b'\'') {
            if let Some(&letter) = self.bytes.get(self.pos + 1) {
                if let Some(suffix) = TypeSuffix::from_letter(letter as char) {
                    let after = self.bytes.get(self.pos + 2).copied();
                    let is_ident_continuation =
                        matches!(after, Some(b) if b.is_ascii_alphanumeric() || b == b'_');
                    if !is_ident_continuation {
                        let suffix_start = self.pos;
                        self.pos += 2;
                        self.emit(
                            TokenKind::Suffix(suffix),
                            suffix_start as u32,
                            self.pos as u32,
                        );
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::lex;
    use crate::token::{
        BracketSide, CompoundOp, Determinative, EvidenceMark, ModalOp, StringFlavor, TokenKind,
        TypeSuffix,
    };
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn mk(src: &str) -> SourceFile {
        SourceFile::new(
            SourceId::first(),
            "<csl-native-test>",
            src,
            Surface::CslNative,
        )
    }

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(&mk(src)).into_iter().map(|t| t.kind).collect()
    }

    // ─── basic dispatch ──────────────────────────────────────────────────────

    #[test]
    fn empty_input_emits_only_eof() {
        assert_eq!(kinds(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn section_marker_single_and_double() {
        assert_eq!(kinds("§"), vec![TokenKind::Section, TokenKind::Eof]);
        assert_eq!(kinds("§§"), vec![TokenKind::SectionRef, TokenKind::Eof]);
    }

    #[test]
    fn qed_terminator() {
        assert_eq!(kinds("∎"), vec![TokenKind::Qed, TokenKind::Eof]);
        assert_eq!(kinds("QED"), vec![TokenKind::Qed, TokenKind::Eof]);
    }

    // ─── evidence marks ──────────────────────────────────────────────────────

    #[test]
    fn evidence_unicode_all_eight() {
        assert_eq!(
            kinds("✓ ◐ ○ ✗ ⊘ △ ▽ ‼"),
            vec![
                TokenKind::Evidence(EvidenceMark::Confirmed),
                TokenKind::Evidence(EvidenceMark::Partial),
                TokenKind::Evidence(EvidenceMark::Pending),
                TokenKind::Evidence(EvidenceMark::Failed),
                TokenKind::Evidence(EvidenceMark::Unknown),
                TokenKind::Evidence(EvidenceMark::Hypothetical),
                TokenKind::Evidence(EvidenceMark::Deprecated),
                TokenKind::Evidence(EvidenceMark::Proven),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn evidence_ascii_all_eight() {
        assert_eq!(
            kinds("[x] [~] [ ] [!] [?] [^] [v] [!!]"),
            vec![
                TokenKind::Evidence(EvidenceMark::Confirmed),
                TokenKind::Evidence(EvidenceMark::Partial),
                TokenKind::Evidence(EvidenceMark::Pending),
                TokenKind::Evidence(EvidenceMark::Failed),
                TokenKind::Evidence(EvidenceMark::Unknown),
                TokenKind::Evidence(EvidenceMark::Hypothetical),
                TokenKind::Evidence(EvidenceMark::Deprecated),
                TokenKind::Evidence(EvidenceMark::Proven),
                TokenKind::Eof,
            ],
        );
    }

    // ─── modal ops ───────────────────────────────────────────────────────────

    #[test]
    fn modal_ops_all_eight() {
        assert_eq!(
            kinds("W! R! M? N! I> Q? P> D>"),
            vec![
                TokenKind::Modal(ModalOp::Must),
                TokenKind::Modal(ModalOp::Should),
                TokenKind::Modal(ModalOp::May),
                TokenKind::Modal(ModalOp::MustNot),
                TokenKind::Modal(ModalOp::Insight),
                TokenKind::Modal(ModalOp::Question),
                TokenKind::Modal(ModalOp::PushFurther),
                TokenKind::Modal(ModalOp::Decision),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn modal_bareword_todo_fixme() {
        assert_eq!(
            kinds("TODO FIXME"),
            vec![
                TokenKind::Modal(ModalOp::Todo),
                TokenKind::Modal(ModalOp::Fixme),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn modal_suppressed_in_middle_of_identifier() {
        // `abcW!` must not parse as `abc` + `Modal(Must)` because `W!` has no word-boundary after `abc`
        let ks = kinds("abcW!");
        assert_eq!(ks.first(), Some(&TokenKind::Ident));
        // tail contains either error or separated tokens — but NOT ModalOp adjacent-to-ident
    }

    // ─── dense math ──────────────────────────────────────────────────────────

    #[test]
    fn dense_math_quantifiers() {
        assert_eq!(
            kinds("∀ ∃ ∈ ∉ ⊂ ⊃"),
            vec![
                TokenKind::ForAll,
                TokenKind::Exists,
                TokenKind::ElemOf,
                TokenKind::NotElemOf,
                TokenKind::Subset,
                TokenKind::Superset,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn dense_math_inference_symbols() {
        assert_eq!(
            kinds("∴ ∵ ⊢"),
            vec![
                TokenKind::Therefore,
                TokenKind::Because,
                TokenKind::Entails,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn ascii_alias_quantifiers() {
        assert_eq!(
            kinds("all any in nil inf"),
            vec![
                TokenKind::ForAll,
                TokenKind::Exists,
                TokenKind::ElemOf,
                TokenKind::EmptySet,
                TokenKind::Infinity,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn unicode_comparison_aliases() {
        assert_eq!(
            kinds("≡ ≠ ≤ ≥ ∧ ∨"),
            vec![
                TokenKind::EqEq,
                TokenKind::Ne,
                TokenKind::Le,
                TokenKind::Ge,
                TokenKind::AmpAmp,
                TokenKind::PipePipe,
                TokenKind::Eof,
            ],
        );
    }

    // ─── arrows ──────────────────────────────────────────────────────────────

    #[test]
    fn arrow_family_ascii_and_unicode() {
        assert_eq!(
            kinds("-> <- <-> => |> <| ~>"),
            vec![
                TokenKind::Arrow,
                TokenKind::LeftArrow,
                TokenKind::BiArrow,
                TokenKind::FatArrow,
                TokenKind::PipeArrow,
                TokenKind::PipeArrowBack,
                TokenKind::SquigglyArrow,
                TokenKind::Eof,
            ],
        );
        assert_eq!(
            kinds("→ ← ↔ ⇒ ▷"),
            vec![
                TokenKind::Arrow,
                TokenKind::LeftArrow,
                TokenKind::BiArrow,
                TokenKind::FatArrow,
                TokenKind::PipeArrow,
                TokenKind::Eof,
            ],
        );
    }

    // ─── determinatives ──────────────────────────────────────────────────────

    #[test]
    fn determinative_pairs() {
        assert_eq!(
            kinds("⟨⟩ ⟦⟧ ⌈⌉ ⌊⌋ «» ⟪⟫"),
            vec![
                TokenKind::Determinative(Determinative::AngleTuple, BracketSide::Open),
                TokenKind::Determinative(Determinative::AngleTuple, BracketSide::Close),
                TokenKind::Determinative(Determinative::Formula, BracketSide::Open),
                TokenKind::Determinative(Determinative::Formula, BracketSide::Close),
                TokenKind::Determinative(Determinative::Constraint, BracketSide::Open),
                TokenKind::Determinative(Determinative::Constraint, BracketSide::Close),
                TokenKind::Determinative(Determinative::Precondition, BracketSide::Open),
                TokenKind::Determinative(Determinative::Precondition, BracketSide::Close),
                TokenKind::Determinative(Determinative::Quotation, BracketSide::Open),
                TokenKind::Determinative(Determinative::Quotation, BracketSide::Close),
                TokenKind::Determinative(Determinative::Temporal, BracketSide::Open),
                TokenKind::Determinative(Determinative::Temporal, BracketSide::Close),
                TokenKind::Eof,
            ],
        );
    }

    // ─── identifiers + suffixes ──────────────────────────────────────────────

    #[test]
    fn identifier_with_type_suffix() {
        assert_eq!(
            kinds("foo'd"),
            vec![
                TokenKind::Ident,
                TokenKind::Suffix(TypeSuffix::Data),
                TokenKind::Eof,
            ],
        );
        assert_eq!(
            kinds("bar'r"),
            vec![
                TokenKind::Ident,
                TokenKind::Suffix(TypeSuffix::Rule),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn integer_with_suffix() {
        assert_eq!(kinds("42'i32"), vec![TokenKind::IntLiteral, TokenKind::Eof],);
    }

    #[test]
    fn float_literal() {
        assert_eq!(kinds("3.14"), vec![TokenKind::FloatLiteral, TokenKind::Eof]);
    }

    #[test]
    fn string_literal_normal() {
        assert_eq!(
            kinds(r#""hello""#),
            vec![
                TokenKind::StringLiteral(StringFlavor::Normal),
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn string_with_escape() {
        assert_eq!(
            kinds(r#""say \"hi\"""#),
            vec![
                TokenKind::StringLiteral(StringFlavor::Normal),
                TokenKind::Eof,
            ],
        );
    }

    // ─── comments ────────────────────────────────────────────────────────────

    #[test]
    fn hash_line_comment() {
        assert_eq!(
            kinds("# comment text\nfoo"),
            vec![
                TokenKind::LineComment,
                TokenKind::Newline,
                TokenKind::Ident,
                TokenKind::Eof,
            ],
        );
    }

    // ─── compound-ops ────────────────────────────────────────────────────────

    #[test]
    fn bahuvrihi_bv_operator() {
        assert_eq!(
            kinds("a ⊗ b"),
            vec![
                TokenKind::Ident,
                TokenKind::Compound(CompoundOp::Bv),
                TokenKind::Ident,
                TokenKind::Eof,
            ],
        );
    }

    // ─── indent + dedent ─────────────────────────────────────────────────────

    #[test]
    fn indent_then_dedent() {
        let src = "a\n  b\n";
        let ks = kinds(src);
        // expected : Ident(a), Newline, Indent, Ident(b), Newline, Dedent, Eof
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn nested_indent_levels() {
        let src = "a\n  b\n    c\n";
        let ks = kinds(src);
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Dedent,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn blank_line_does_not_perturb_indent() {
        let src = "a\n  b\n\n  c\n";
        let ks = kinds(src);
        // should NOT emit Dedent/Indent around the blank line
        let dedent_count = ks.iter().filter(|k| **k == TokenKind::Dedent).count();
        let indent_count = ks.iter().filter(|k| **k == TokenKind::Indent).count();
        assert_eq!(indent_count, 1);
        assert_eq!(dedent_count, 1);
    }

    #[test]
    fn bracket_suppresses_indent_tracking() {
        // newlines inside parens do not trigger indent handling
        let src = "f(\n  a,\n  b\n)\n";
        let ks = kinds(src);
        assert!(!ks.contains(&TokenKind::Indent));
        assert!(!ks.contains(&TokenKind::Dedent));
    }

    // ─── full fragment ───────────────────────────────────────────────────────

    #[test]
    fn full_csl_native_fragment() {
        let src = "§ fn ≡ @differentiable\n  I> length(p) - r\n  W! p ∈ vec3\n  → f32\n";
        let ks = kinds(src);
        // sanity : contains Section, FatArrow-family (≡ → EqEq), Modal(Insight), Modal(Must),
        //         ElemOf, Arrow, Ident, and at least one Indent + one Dedent
        assert!(ks.contains(&TokenKind::Section));
        assert!(ks.contains(&TokenKind::EqEq));
        assert!(ks.contains(&TokenKind::Modal(ModalOp::Insight)));
        assert!(ks.contains(&TokenKind::Modal(ModalOp::Must)));
        assert!(ks.contains(&TokenKind::ElemOf));
        assert!(ks.contains(&TokenKind::Arrow));
        assert!(ks.contains(&TokenKind::Indent));
        assert!(ks.contains(&TokenKind::Dedent));
        assert_eq!(ks.last(), Some(&TokenKind::Eof));
    }

    #[test]
    fn span_offsets_are_exact() {
        let toks = lex(&mk("§ foo"));
        assert_eq!(toks[0].kind, TokenKind::Section);
        assert_eq!(toks[0].span.start, 0);
        assert_eq!(toks[0].span.end, '§'.len_utf8() as u32);
        assert_eq!(toks[1].kind, TokenKind::Ident);
        let ident_start = '§'.len_utf8() as u32 + 1; // + space
        assert_eq!(toks[1].span.start, ident_start);
    }

    #[test]
    fn eof_always_appended() {
        for src in ["", "§", "foo\n  bar\n", "42", "\"a\""] {
            assert_eq!(
                kinds(src).last(),
                Some(&TokenKind::Eof),
                "failed on src = {src:?}",
            );
        }
    }

    #[test]
    fn unrecognized_control_char_emits_error() {
        let ks = kinds("\x01");
        assert_eq!(ks, vec![TokenKind::Error, TokenKind::Eof]);
    }
}
