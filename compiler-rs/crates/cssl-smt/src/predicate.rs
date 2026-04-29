//! Predicate-text → SMT [`Term`] translator.
//!
//! § SPEC : `specs/20_SMT.csl` § REFINEMENT-OBLIGATION DISCHARGE.
//!
//! § SCOPE (T9-phase-2a / this commit)
//!   `RefinementObligation::kind == ObligationKind::Predicate { binder, predicate_text }`
//!   carries a textual predicate like `"v > 0"` or `"v >= 0 && v < max"` that must
//!   be translated to an SMT-LIB [`Term`] for solver dispatch. This module :
//!
//!   * Tokenizes the predicate-text into a `Token` stream.
//!   * Parses a minimal comparison-expression grammar (covers 80% of real-world
//!     refinements per `specs/03_TYPES.csl` § REFINEMENT-SUGAR).
//!   * Builds a [`Term`] tree that renders to valid SMT-LIB 2.6.
//!   * Wraps the result in a [`Query`] that declares the binder-var + asserts
//!     `(not P(v))` so an `unsat` verdict proves the refinement holds.
//!
//! § GRAMMAR (minimal stage-0 subset)
//!
//! ```text
//! predicate  := disjunction
//! disjunction := conjunction ( ("||" | "or") conjunction )*
//! conjunction := comparison  ( ("&&" | "and") comparison )*
//! comparison := primary   ( ("==" | "!=" | "<=" | ">=" | "<" | ">") primary )?
//!             | primary "in" "{" primary ("," primary)* "}"
//!             | primary "∈" "{" primary ("," primary)* "}"
//! primary    := int-literal | ident | "(" predicate ")" | "-" primary
//! ```
//!
//! § T9-phase-2b DEFERRED
//!   * Real HIR-expression → Term translation (bypasses predicate-text re-parsing).
//!   * `Lipschitz` obligation translation (needs arithmetic-interval encoding).
//!   * Multi-binder predicates (currently single-binder `{v : T | P(v)}` only).
//!   * Tag-dictionary resolution (currently stub-asserts `true`).
//!   * Float-arithmetic predicates (stage-0 assumes integer `Int` sort).
//!   * Array / tuple / struct access in predicates.
//!   * User-defined fn calls in predicates (needs monomorphized SMT-uninterpreted-fn decls).

use cssl_hir::{ObligationBag, ObligationId, ObligationKind, RefinementObligation};
use thiserror::Error;

use crate::query::{FnDecl, Query};
use crate::term::{Sort, Term, Theory};

/// Translation failure modes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TranslationError {
    /// Predicate-text was syntactically malformed.
    #[error("predicate-text `{text}` did not parse : {reason}")]
    ParseFailure { text: String, reason: String },
    /// Obligation-kind is not yet translatable at stage-0.
    #[error(
        "obligation-kind `{kind}` is not yet translatable to SMT at T9-phase-2a \
         (gated : Lipschitz arithmetic, Tag-dict resolution)"
    )]
    UnsupportedKind { kind: &'static str },
}

/// Token stream for the minimal predicate grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Int(i64),
    Ident(String),
    // Comparison ops
    Eq, // ==
    Ne, // !=
    Lt, // <
    Le, // <=
    Gt, // >
    Ge, // >=
    // Logical ops
    AndTok,
    OrTok,
    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    In, // `in` keyword or `∈` glyph
    Minus,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Two-char punctuation first (ASCII-only : safe to slice at byte-boundary
        // only if both bytes are ASCII).
        if i + 1 < bytes.len() && bytes[i] < 0x80 && bytes[i + 1] < 0x80 {
            let two = &input[i..i + 2];
            let tok = match two {
                "==" => Some(Token::Eq),
                "!=" => Some(Token::Ne),
                "<=" => Some(Token::Le),
                ">=" => Some(Token::Ge),
                "&&" => Some(Token::AndTok),
                "||" => Some(Token::OrTok),
                _ => None,
            };
            if let Some(t) = tok {
                tokens.push(t);
                i += 2;
                continue;
            }
        }
        // Single-char punctuation.
        match c {
            b'<' => {
                tokens.push(Token::Lt);
                i += 1;
                continue;
            }
            b'>' => {
                tokens.push(Token::Gt);
                i += 1;
                continue;
            }
            b'(' => {
                tokens.push(Token::LParen);
                i += 1;
                continue;
            }
            b')' => {
                tokens.push(Token::RParen);
                i += 1;
                continue;
            }
            b'{' => {
                tokens.push(Token::LBrace);
                i += 1;
                continue;
            }
            b'}' => {
                tokens.push(Token::RBrace);
                i += 1;
                continue;
            }
            b',' => {
                tokens.push(Token::Comma);
                i += 1;
                continue;
            }
            b'-' => {
                tokens.push(Token::Minus);
                i += 1;
                continue;
            }
            _ => {}
        }
        // Unicode `∈`  (3 bytes : 0xE2 0x88 0x88)
        if c == 0xE2 && i + 2 < bytes.len() && bytes[i + 1] == 0x88 && bytes[i + 2] == 0x88 {
            tokens.push(Token::In);
            i += 3;
            continue;
        }
        // Integer literal.
        if c.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let n: i64 = input[start..i]
                .parse()
                .map_err(|e: std::num::ParseIntError| format!("int-parse : {e}"))?;
            tokens.push(Token::Int(n));
            continue;
        }
        // Identifier / keyword.
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let ident = &input[start..i];
            match ident {
                "and" => tokens.push(Token::AndTok),
                "or" => tokens.push(Token::OrTok),
                "in" => tokens.push(Token::In),
                _ => tokens.push(Token::Ident(ident.to_string())),
            }
            continue;
        }
        return Err(format!("unexpected char `{}` at offset {i}", c as char));
    }
    Ok(tokens)
}

// ─────────────────────────────────────────────────────────────────────────
// § Recursive-descent parser (hand-rolled, ~100 lines)
// ─────────────────────────────────────────────────────────────────────────

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn eat(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }
    fn expect_token(&mut self, want: &Token, ctx: &str) -> Result<(), String> {
        match self.eat() {
            Some(t) if &t == want => Ok(()),
            Some(other) => Err(format!("{ctx} : expected {want:?}, got {other:?}")),
            None => Err(format!("{ctx} : expected {want:?}, got EOF")),
        }
    }

    fn parse_disjunction(&mut self) -> Result<Term, String> {
        let first = self.parse_conjunction()?;
        let mut args = vec![first];
        while matches!(self.peek(), Some(Token::OrTok)) {
            self.eat();
            args.push(self.parse_conjunction()?);
        }
        Ok(if args.len() == 1 {
            args.pop().unwrap()
        } else {
            Term::app("or", args)
        })
    }

    fn parse_conjunction(&mut self) -> Result<Term, String> {
        let first = self.parse_comparison()?;
        let mut args = vec![first];
        while matches!(self.peek(), Some(Token::AndTok)) {
            self.eat();
            args.push(self.parse_comparison()?);
        }
        Ok(if args.len() == 1 {
            args.pop().unwrap()
        } else {
            Term::app("and", args)
        })
    }

    fn parse_comparison(&mut self) -> Result<Term, String> {
        let lhs = self.parse_primary()?;
        match self.peek() {
            Some(Token::Eq) => {
                self.eat();
                let rhs = self.parse_primary()?;
                Ok(Term::app("=", vec![lhs, rhs]))
            }
            Some(Token::Ne) => {
                self.eat();
                let rhs = self.parse_primary()?;
                Ok(Term::app("not", vec![Term::app("=", vec![lhs, rhs])]))
            }
            Some(Token::Lt) => {
                self.eat();
                let rhs = self.parse_primary()?;
                Ok(Term::app("<", vec![lhs, rhs]))
            }
            Some(Token::Le) => {
                self.eat();
                let rhs = self.parse_primary()?;
                Ok(Term::app("<=", vec![lhs, rhs]))
            }
            Some(Token::Gt) => {
                self.eat();
                let rhs = self.parse_primary()?;
                Ok(Term::app(">", vec![lhs, rhs]))
            }
            Some(Token::Ge) => {
                self.eat();
                let rhs = self.parse_primary()?;
                Ok(Term::app(">=", vec![lhs, rhs]))
            }
            Some(Token::In) => {
                self.eat();
                self.expect_token(&Token::LBrace, "in-set")?;
                let mut members = Vec::new();
                members.push(self.parse_primary()?);
                while matches!(self.peek(), Some(Token::Comma)) {
                    self.eat();
                    members.push(self.parse_primary()?);
                }
                self.expect_token(&Token::RBrace, "in-set")?;
                // (or (= lhs m1) (= lhs m2) ...)
                let disjuncts: Vec<Term> = members
                    .into_iter()
                    .map(|m| Term::app("=", vec![lhs.clone(), m]))
                    .collect();
                Ok(if disjuncts.len() == 1 {
                    disjuncts.into_iter().next().unwrap()
                } else {
                    Term::app("or", disjuncts)
                })
            }
            _ => Ok(lhs),
        }
    }

    fn parse_primary(&mut self) -> Result<Term, String> {
        match self.eat() {
            Some(Token::Int(n)) => Ok(Term::int(n)),
            Some(Token::Ident(n)) => {
                // Special-case boolean literals.
                match n.as_str() {
                    "true" => Ok(Term::bool(true)),
                    "false" => Ok(Term::bool(false)),
                    _ => Ok(Term::var(n)),
                }
            }
            Some(Token::LParen) => {
                let t = self.parse_disjunction()?;
                self.expect_token(&Token::RParen, "paren")?;
                Ok(t)
            }
            Some(Token::Minus) => {
                let inner = self.parse_primary()?;
                Ok(Term::app("-", vec![inner]))
            }
            other => Err(format!("unexpected token in primary : {other:?}")),
        }
    }

    fn finished(&self) -> bool {
        self.pos >= self.tokens.len()
    }
}

/// Parse a predicate-text to a [`Term`]. Returns the parsed term on success.
///
/// # Errors
/// Returns a [`TranslationError::ParseFailure`] if the input is malformed.
pub fn parse_predicate(text: &str) -> Result<Term, TranslationError> {
    let tokens = tokenize(text).map_err(|reason| TranslationError::ParseFailure {
        text: text.to_string(),
        reason,
    })?;
    let mut parser = Parser { tokens, pos: 0 };
    let term = parser
        .parse_disjunction()
        .map_err(|reason| TranslationError::ParseFailure {
            text: text.to_string(),
            reason,
        })?;
    if !parser.finished() {
        return Err(TranslationError::ParseFailure {
            text: text.to_string(),
            reason: format!("unexpected trailing tokens starting at pos {}", parser.pos),
        });
    }
    Ok(term)
}

/// Translate a single [`RefinementObligation`] to a [`Query`].
///
/// For `ObligationKind::Predicate { binder, predicate_text }`, emits a query that :
///   1. Sets logic to `QF_LIA` (linear integer arithmetic — stage-0 default).
///   2. Declares the binder as an `Int`-sorted fn.
///   3. Asserts `(not P(binder))`.
///   4. Check-sats. `unsat` verdict proves the refinement holds.
///
/// For `ObligationKind::Tag { name }`, emits a stub query asserting `(! true :named tag_<n>)` —
/// actual tag-dictionary resolution is T9-phase-2b.
///
/// For `ObligationKind::Lipschitz { bound_text }`, returns `UnsupportedKind`.
///
/// # Errors
/// Returns a [`TranslationError`] on parse-failure or unsupported-kind.
pub fn translate_obligation(
    obligation: &RefinementObligation,
    interner: &cssl_hir::Interner,
) -> Result<Query, TranslationError> {
    match &obligation.kind {
        ObligationKind::Predicate {
            binder,
            predicate_text,
        } => {
            let binder_name = interner.resolve(*binder);
            let predicate_term = parse_predicate(predicate_text)?;
            let mut q = Query::new().with_theory(Theory::LIA);
            // declare-fun v () Int
            q.declare_fn(FnDecl::new(binder_name, vec![], Sort::Int));
            // assert (not P(v))
            let negated = Term::app("not", vec![predicate_term]);
            q.assert_named(format!("obl_{}_predicate", obligation.id.0), negated);
            Ok(q)
        }
        ObligationKind::Tag { name } => {
            let tag_name = interner.resolve(*name);
            let mut q = Query::new().with_theory(Theory::ALL);
            q.assert_named(
                format!("obl_{}_tag_{}", obligation.id.0, tag_name),
                Term::bool(true),
            );
            Ok(q)
        }
        ObligationKind::Lipschitz { bound_text } => {
            // § T9-phase-2b : Lipschitz arithmetic-interval encoding.
            //
            // The Lipschitz condition |f(x) - f(y)| ≤ k * |x - y| is encoded as an
            // SMT query over the real numbers (LRA logic). The bound-text typically
            // matches one of the stage-0 forms :
            //   "k"           — integer or real literal (e.g., "1", "1.0", "2.5")
            //   "k = 1.0"     — keyword-form
            //   "<expr>"      — general real-expression (falls back to textual)
            //
            // Stage-0 emits a structural query :
            //   (declare-fun x () Real)
            //   (declare-fun y () Real)
            //   (declare-fun f (Real) Real)          — uninterpreted fn
            //   (assert (! (not (<= (abs (- (f x) (f y))) (* k (abs (- x y)))))
            //            :named obl_<id>_lipschitz))
            // Unsat verdict proves the Lipschitz bound holds.
            //
            // Phase-2c : inline f's MIR-body via per-primitive-Lipschitz-decomposition
            // (Sum rule : Lip(f+g) ≤ Lip(f) + Lip(g), Product rule for bounded, etc.)
            let k = parse_lipschitz_bound(bound_text);
            let mut q = Query::new().with_theory(Theory::LRA);
            // declare-fun x () Real  + y () Real
            q.declare_fn(FnDecl::new("x", vec![], Sort::Real));
            q.declare_fn(FnDecl::new("y", vec![], Sort::Real));
            // declare-fun f (Real) Real — uninterpreted-fn-name derived from enclosing-def text.
            let fn_name = obligation
                .enclosing_def
                .map_or_else(|| "f".to_string(), |d| format!("f_{}", d.0));
            q.declare_fn(FnDecl::new(fn_name.clone(), vec![Sort::Real], Sort::Real));
            // k-bound : (* k (abs (- x y)))
            let x = Term::var("x");
            let y = Term::var("y");
            let f_x = Term::app(fn_name.clone(), vec![x.clone()]);
            let f_y = Term::app(fn_name, vec![y.clone()]);
            let diff_fx = Term::app("-", vec![f_x, f_y]);
            let diff_xy = Term::app("-", vec![x, y]);
            let abs_fx = Term::app("abs", vec![diff_fx]);
            let abs_xy = Term::app("abs", vec![diff_xy]);
            let k_term = Term::app("*", vec![k, abs_xy]);
            let lipschitz_cond = Term::app("<=", vec![abs_fx, k_term]);
            let negated = Term::app("not", vec![lipschitz_cond]);
            q.assert_named(format!("obl_{}_lipschitz", obligation.id.0), negated);
            Ok(q)
        }
        ObligationKind::Layout {
            kind_word,
            expected_size,
            expected_align,
        } => {
            // § T11-D126 : @layout(...) refinement encoded as size+align equality.
            //
            // The layout-validator in cssl-mir::layout_check has already computed
            // the expected size+align from the `@layout(kind)` attribute and the
            // type's structural-layout. We emit a structural unsat-query that the
            // SMT pass uses as a placeholder for layout-discharge :
            //   (declare-fun s () Int)
            //   (declare-fun a () Int)
            //   (assert (! (not (and (= s <expected_size>) (= a <expected_align>)))
            //            :named obl_<id>_layout_<kind>))
            // unsat ⇒ layout matches ; sat ⇒ layout mismatch.
            let mut q = Query::new().with_theory(Theory::LIA);
            q.declare_fn(FnDecl::new("s", vec![], Sort::Int));
            q.declare_fn(FnDecl::new("a", vec![], Sort::Int));
            let s_term = Term::var("s");
            let a_term = Term::var("a");
            let align_eq = Term::app("=", vec![a_term, Term::int(i64::from(*expected_align))]);
            let conj = if let Some(size) = expected_size {
                let size_eq = Term::app("=", vec![s_term, Term::int(i64::from(*size))]);
                Term::app("and", vec![size_eq, align_eq])
            } else {
                align_eq
            };
            let negated = Term::app("not", vec![conj]);
            q.assert_named(
                format!("obl_{}_layout_{}", obligation.id.0, kind_word),
                negated,
            );
            Ok(q)
        }
    }
}

/// Parse a Lipschitz bound-text to an SMT Real term. Accepts bare integers,
/// decimals, `k = N` forms, and falls back to `1.0` for unrecognized input.
fn parse_lipschitz_bound(text: &str) -> Term {
    // Strip `k = ` prefix if present.
    let raw = text.trim();
    let body = raw.split_once('=').map_or(raw, |(_, rhs)| rhs).trim();
    // Try int-literal first.
    if let Ok(n) = body.parse::<i64>() {
        return Term::int(n);
    }
    // Try real-literal (format : `num.den` or `num/den`).
    if let Some((whole, frac)) = body.split_once('.') {
        if let (Ok(w), Ok(f)) = (whole.parse::<i64>(), frac.parse::<i64>()) {
            // Encode as (/ num 10^|frac|) rational.
            let denom_u32 = u32::try_from(frac.to_string().len()).unwrap_or(0);
            let denom = 10i64.saturating_pow(denom_u32);
            let num = w.saturating_mul(denom).saturating_add(f);
            let denom_u = u64::try_from(denom).unwrap_or(1);
            return Term::Lit(crate::term::Literal::Rational { num, den: denom_u });
        }
    }
    // Fallback : literal Real `1.0`.
    Term::Lit(crate::term::Literal::Rational { num: 1, den: 1 })
}

/// Translate an entire [`ObligationBag`] to a sequence of `(id, result)` pairs.
pub fn translate_bag(
    bag: &ObligationBag,
    interner: &cssl_hir::Interner,
) -> Vec<(ObligationId, Result<Query, TranslationError>)> {
    bag.iter()
        .map(|o| (o.id, translate_obligation(o, interner)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_predicate, translate_bag, translate_obligation};
    use cssl_hir::{
        HirId, Interner, ObligationBag, ObligationId, ObligationKind, RefinementObligation,
    };

    fn mk_obligation(
        interner: &Interner,
        binder_name: &str,
        predicate: &str,
        id: u32,
    ) -> RefinementObligation {
        let binder = interner.intern(binder_name);
        RefinementObligation {
            id: ObligationId(id),
            origin: HirId::DUMMY,
            span: cssl_ast::Span::DUMMY,
            enclosing_def: None,
            kind: ObligationKind::Predicate {
                binder,
                predicate_text: predicate.to_string(),
            },
            base_type_text: "i32".into(),
        }
    }

    #[test]
    fn parse_integer_comparison() {
        let t = parse_predicate("v > 0").unwrap();
        assert_eq!(t.render(), "(> v 0)");
    }

    #[test]
    fn parse_ge_le_eq_ne() {
        assert_eq!(parse_predicate("v >= 0").unwrap().render(), "(>= v 0)");
        assert_eq!(parse_predicate("v <= 10").unwrap().render(), "(<= v 10)");
        assert_eq!(parse_predicate("v == 5").unwrap().render(), "(= v 5)");
        assert_eq!(parse_predicate("v != 7").unwrap().render(), "(not (= v 7))");
    }

    #[test]
    fn parse_conjunction() {
        let t = parse_predicate("v >= 0 && v < 100").unwrap();
        assert_eq!(t.render(), "(and (>= v 0) (< v 100))");
    }

    #[test]
    fn parse_disjunction() {
        let t = parse_predicate("v == 1 || v == 2").unwrap();
        assert_eq!(t.render(), "(or (= v 1) (= v 2))");
    }

    #[test]
    fn parse_set_membership() {
        let t = parse_predicate("v in {44100, 48000, 96000, 192000}").unwrap();
        // Should expand to (or (= v 44100) (= v 48000) (= v 96000) (= v 192000)).
        let r = t.render();
        assert!(r.starts_with("(or"));
        assert!(r.contains("(= v 44100)"));
        assert!(r.contains("(= v 48000)"));
        assert!(r.contains("(= v 96000)"));
        assert!(r.contains("(= v 192000)"));
    }

    #[test]
    fn parse_unicode_in_glyph() {
        let t = parse_predicate("v ∈ {0, 1}").unwrap();
        assert!(t.render().contains("(= v 0)"));
        assert!(t.render().contains("(= v 1)"));
    }

    #[test]
    fn parse_parenthesized() {
        let t = parse_predicate("(v > 0) && (v < 100)").unwrap();
        assert_eq!(t.render(), "(and (> v 0) (< v 100))");
    }

    #[test]
    fn parse_negative_literal() {
        let t = parse_predicate("v > -5").unwrap();
        assert_eq!(t.render(), "(> v (- 5))");
    }

    #[test]
    fn parse_rejects_malformed() {
        // Missing rhs of comparison.
        assert!(parse_predicate("v >=").is_err());
        assert!(parse_predicate("&& v").is_err());
        assert!(parse_predicate("").is_err());
    }

    #[test]
    fn parse_plain_variable_is_term() {
        // A bare identifier is a valid boolean-term at stage-0.
        let t = parse_predicate("is_valid").unwrap();
        assert_eq!(t.render(), "is_valid");
    }

    #[test]
    fn translate_predicate_emits_declare_fn_and_assert() {
        let interner = Interner::new();
        let o = mk_obligation(&interner, "v", "v > 0", 7);
        let q = translate_obligation(&o, &interner).unwrap();
        let smt = crate::emit::emit_smtlib(&q);
        assert!(smt.contains("(set-logic QF_LIA)"));
        assert!(smt.contains("(declare-fun v () Int)"));
        // assertion is negated : (not (> v 0))
        assert!(smt.contains("(not (> v 0))"));
        assert!(smt.contains("obl_7_predicate"));
    }

    #[test]
    fn translate_tag_emits_stub_query() {
        let interner = Interner::new();
        let tag = interner.intern("pos");
        let o = RefinementObligation {
            id: ObligationId(3),
            origin: HirId::DUMMY,
            span: cssl_ast::Span::DUMMY,
            enclosing_def: None,
            kind: ObligationKind::Tag { name: tag },
            base_type_text: "f32".into(),
        };
        let q = translate_obligation(&o, &interner).unwrap();
        let smt = crate::emit::emit_smtlib(&q);
        assert!(smt.contains("obl_3_tag_pos"));
    }

    #[test]
    fn translate_lipschitz_emits_lra_query() {
        // T9-phase-2b : Lipschitz is now a supported kind.
        let interner = Interner::new();
        let o = RefinementObligation {
            id: ObligationId(5),
            origin: HirId::DUMMY,
            span: cssl_ast::Span::DUMMY,
            enclosing_def: None,
            kind: ObligationKind::Lipschitz {
                bound_text: "1.0".into(),
            },
            base_type_text: "f32".into(),
        };
        let q = translate_obligation(&o, &interner).unwrap();
        let smt = crate::emit::emit_smtlib(&q);
        assert!(smt.contains("(set-logic QF_LRA)"));
        assert!(smt.contains("(declare-fun x () Real)"));
        assert!(smt.contains("(declare-fun y () Real)"));
        assert!(smt.contains("obl_5_lipschitz"));
        assert!(smt.contains("abs"));
    }

    #[test]
    fn lipschitz_bound_k_equals_1_parses() {
        use super::parse_lipschitz_bound;
        let t = parse_lipschitz_bound("k = 1.0");
        assert!(matches!(
            t,
            super::Term::Lit(crate::term::Literal::Rational { num: 10, den: 10 })
        ));
    }

    #[test]
    fn lipschitz_bound_bare_int_parses() {
        use super::parse_lipschitz_bound;
        let t = parse_lipschitz_bound("2");
        assert_eq!(t.render(), "2");
    }

    #[test]
    fn lipschitz_bound_unrecognized_falls_back_to_1() {
        use super::parse_lipschitz_bound;
        let t = parse_lipschitz_bound("unknown_form");
        assert!(matches!(
            t,
            super::Term::Lit(crate::term::Literal::Rational { num: 1, den: 1 })
        ));
    }

    #[test]
    fn translate_bag_processes_all_obligations() {
        let interner = Interner::new();
        let mut bag = ObligationBag::new();
        bag.push(mk_obligation(&interner, "v", "v > 0", 0));
        bag.push(mk_obligation(&interner, "w", "w >= 0 && w < 100", 0));
        let results = translate_bag(&bag, &interner);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, r)| r.is_ok()));
    }

    #[test]
    fn translate_bag_records_parse_failure() {
        let interner = Interner::new();
        let mut bag = ObligationBag::new();
        bag.push(mk_obligation(&interner, "v", "v >=", 0)); // malformed
        let results = translate_bag(&bag, &interner);
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_err());
    }

    #[test]
    fn predicate_with_audio_callback_refinement_form() {
        // Models the refinement in audio_callback.cssl :
        //   u32 { v : u32 | v ∈ {44100, 48000, 96000, 192000} }
        let interner = Interner::new();
        let o = mk_obligation(&interner, "v", "v in {44100, 48000, 96000, 192000}", 42);
        let q = translate_obligation(&o, &interner).unwrap();
        let smt = crate::emit::emit_smtlib(&q);
        assert!(smt.contains("(= v 44100)"));
        assert!(smt.contains("(= v 192000)"));
        assert!(smt.contains("obl_42_predicate"));
    }
}
