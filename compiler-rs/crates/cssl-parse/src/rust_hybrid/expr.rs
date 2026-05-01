//! Expression parser — Pratt precedence climbing.
//!
//! § SPEC : `specs/09_SYNTAX.csl` § OPERATOR-PRECEDENCE (14 levels) + § expressions.
//!
//! § PRATT TABLE (lower level-number = tighter binding)
//!     1 : . field    :: path    () call    [] index      (postfix — highest)
//!     2 : unary -  !  ~  &  &mut  *  as
//!     3 : * / %
//!     4 : + -
//!     5 : << >>
//!     6 : &
//!     7 : ^
//!     8 : |
//!     9 : == != < <= > >= ≠
//!    10 : && ∧
//!    11 : || ∨
//!    12 : .. ..=
//!    13 : |> ▷                        pipeline (left-assoc)
//!    14 : = compound-assign           (right-assoc)
//!    15 : ??                          null-coalesce / early-return-default
//!
//! § BP ENCODING
//!   bp = `2 * (MAX_LEVEL - level) + associativity_bias`
//!   MAX_LEVEL = 20 (head-room for future glyph-extensions).
//!   Left-assoc infix : `left_bp < right_bp` → `(x + 1)` pair.
//!   Right-assoc infix : `left_bp > right_bp` → reversed pair.

use cssl_ast::{
    ArrayExpr, BinOp, Block, CallArg, DiagnosticBag, Expr, ExprKind, Ident, Literal, LiteralKind,
    MatchArm, ModulePath, Param, Pattern, PatternKind, Span, Stmt, StmtKind, StructFieldInit, Type,
    TypeKind, UnOp,
};
use cssl_lex::{BracketKind, BracketSide, Keyword, TokenKind};

use crate::common::{expect, parse_colon_path, parse_ident};
use crate::cursor::TokenCursor;
use crate::error::custom;
use crate::rust_hybrid::{attr, pat, ty};

// ─ Binding-power helpers ─────────────────────────────────────────────────────

const fn infix_bp(kind: TokenKind) -> Option<(u8, u8, InfixOp)> {
    let (level, left_assoc, op) = match kind {
        // level 3 : * / %
        TokenKind::Star => (3, true, InfixOp::Bin(BinOp::Mul)),
        TokenKind::Slash => (3, true, InfixOp::Bin(BinOp::Div)),
        TokenKind::Percent => (3, true, InfixOp::Bin(BinOp::Rem)),
        // level 4 : + -
        TokenKind::Plus => (4, true, InfixOp::Bin(BinOp::Add)),
        TokenKind::Minus => (4, true, InfixOp::Bin(BinOp::Sub)),
        // level 5 : << >>
        TokenKind::LShift => (5, true, InfixOp::Bin(BinOp::Shl)),
        TokenKind::RShift => (5, true, InfixOp::Bin(BinOp::Shr)),
        // level 6 : &
        TokenKind::Amp => (6, true, InfixOp::Bin(BinOp::BitAnd)),
        // level 7 : ^
        TokenKind::Caret => (7, true, InfixOp::Bin(BinOp::BitXor)),
        // level 8 : |
        TokenKind::Pipe => (8, true, InfixOp::Bin(BinOp::BitOr)),
        // level 9 : == != < <= > >=
        TokenKind::EqEq => (9, true, InfixOp::Bin(BinOp::Eq)),
        TokenKind::Ne => (9, true, InfixOp::Bin(BinOp::Ne)),
        TokenKind::Lt => (9, true, InfixOp::Bin(BinOp::Lt)),
        TokenKind::Le => (9, true, InfixOp::Bin(BinOp::Le)),
        TokenKind::Gt => (9, true, InfixOp::Bin(BinOp::Gt)),
        TokenKind::Ge => (9, true, InfixOp::Bin(BinOp::Ge)),
        // level 10 : &&
        TokenKind::AmpAmp => (10, true, InfixOp::Bin(BinOp::And)),
        // level 11 : ||
        TokenKind::PipePipe => (11, true, InfixOp::Bin(BinOp::Or)),
        // level 12 : range
        TokenKind::DotDot => (12, true, InfixOp::Range { inclusive: false }),
        TokenKind::DotDotEq => (12, true, InfixOp::Range { inclusive: true }),
        // level 13 : pipeline
        TokenKind::PipeArrow => (13, true, InfixOp::Pipeline),
        // level 14 : assignment (right-assoc)
        TokenKind::Eq => (14, false, InfixOp::Assign(None)),
        // level 15 : ?? (right-assoc)
        TokenKind::QuestionQuestion => (15, false, InfixOp::TryDefault),
        // implies / entails (context-dependent) — binding at level 9
        TokenKind::FatArrow => (9, true, InfixOp::Bin(BinOp::Implies)),
        TokenKind::Entails => (9, true, InfixOp::Bin(BinOp::Entails)),
        _ => return None,
    };
    let base = (20 - level) * 2;
    let (l, r) = if left_assoc {
        (base, base + 1)
    } else {
        (base + 1, base)
    };
    Some((l, r, op))
}

const fn postfix_bp(kind: TokenKind) -> Option<(u8, PostfixOp)> {
    let (level, op) = match kind {
        TokenKind::Dot => (1, PostfixOp::Field),
        TokenKind::ColonColon => (1, PostfixOp::PathCont),
        TokenKind::Bracket(BracketKind::Paren, BracketSide::Open) => (1, PostfixOp::Call),
        TokenKind::Bracket(BracketKind::Square, BracketSide::Open) => (1, PostfixOp::Index),
        TokenKind::Question => (1, PostfixOp::Try),
        TokenKind::Keyword(Keyword::As) => (2, PostfixOp::Cast),
        _ => return None,
    };
    Some(((20 - level) * 2, op))
}

const fn unary_prefix_bp() -> u8 {
    (20 - 2) * 2
}

#[derive(Debug, Clone, Copy)]
enum InfixOp {
    Bin(BinOp),
    Assign(Option<BinOp>),
    Pipeline,
    Range { inclusive: bool },
    TryDefault,
}

#[derive(Debug, Clone, Copy)]
enum PostfixOp {
    Field,
    PathCont,
    Call,
    Index,
    Try,
    Cast,
}

// ─ Entry points ──────────────────────────────────────────────────────────────

/// Parse one expression at the lowest binding-power (consumes the full expression).
#[must_use]
pub fn parse_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Expr {
    parse_expr_bp(cursor, bag, 0)
}

/// Parse an expression with a minimum binding power (Pratt entry).
fn parse_expr_bp(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag, min_bp: u8) -> Expr {
    let mut lhs = parse_prefix(cursor, bag);
    loop {
        let t = cursor.peek();
        if let Some((bp, op)) = postfix_bp(t.kind) {
            if bp < min_bp {
                break;
            }
            lhs = apply_postfix(cursor, bag, lhs, op);
            continue;
        }
        if let Some((lbp, rbp, op)) = infix_bp(t.kind) {
            if lbp < min_bp {
                break;
            }
            cursor.bump(); // consume operator
            let rhs = parse_expr_bp(cursor, bag, rbp);
            lhs = combine_infix(op, lhs, rhs);
            continue;
        }
        break;
    }
    lhs
}

// ─ Prefix atoms + prefix unary ──────────────────────────────────────────────

fn parse_prefix(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Expr {
    // Outer @attrs attached to this expression.
    let attrs = attr::parse_outer_attrs(cursor, bag);

    let t = cursor.peek();
    let kind = match t.kind {
        // ── Prefix unary operators ───────────────────────────────────────────
        TokenKind::Minus => unary(cursor, bag, UnOp::Neg),
        TokenKind::Bang => unary(cursor, bag, UnOp::Not),
        TokenKind::Tilde => unary(cursor, bag, UnOp::BitNot),
        TokenKind::Star => unary(cursor, bag, UnOp::Deref),
        TokenKind::Amp => parse_reference_prefix(cursor, bag),

        // ── Literals ──────────────────────────────────────────────────────────
        TokenKind::IntLiteral => {
            cursor.bump();
            ExprKind::Literal(Literal {
                span: t.span,
                kind: LiteralKind::Int,
            })
        }
        TokenKind::FloatLiteral => {
            cursor.bump();
            ExprKind::Literal(Literal {
                span: t.span,
                kind: LiteralKind::Float,
            })
        }
        TokenKind::StringLiteral(_) => {
            cursor.bump();
            ExprKind::Literal(Literal {
                span: t.span,
                kind: LiteralKind::Str,
            })
        }
        TokenKind::CharLiteral => {
            cursor.bump();
            ExprKind::Literal(Literal {
                span: t.span,
                kind: LiteralKind::Char,
            })
        }
        TokenKind::Keyword(Keyword::True) => {
            cursor.bump();
            ExprKind::Literal(Literal {
                span: t.span,
                kind: LiteralKind::Bool(true),
            })
        }
        TokenKind::Keyword(Keyword::False) => {
            cursor.bump();
            ExprKind::Literal(Literal {
                span: t.span,
                kind: LiteralKind::Bool(false),
            })
        }

        // ── Paths + self ──────────────────────────────────────────────────────
        TokenKind::Ident | TokenKind::Keyword(Keyword::SelfValue | Keyword::SelfType) => {
            let path = if matches!(
                t.kind,
                TokenKind::Keyword(Keyword::SelfValue | Keyword::SelfType)
            ) {
                let self_tok = cursor.bump();
                ModulePath {
                    span: self_tok.span,
                    segments: vec![Ident {
                        span: self_tok.span,
                    }],
                }
            } else {
                parse_colon_path(cursor, bag, "path expression")
            };
            // Struct constructor form `Point { x : 1 }` — only if followed by `{` AND
            // the interior looks like struct-field shape (guards against `if x { ... }` /
            // `match x { 0 => ... }` / etc).
            if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open))
                && looks_like_struct_body(cursor)
                && !in_context_forbidding_struct_brace(cursor)
            {
                parse_struct_constructor(cursor, bag, path)
            } else {
                ExprKind::Path(path)
            }
        }

        // ── Block / parenthesized / tuple / array ─────────────────────────────
        TokenKind::Bracket(BracketKind::Brace, BracketSide::Open) => {
            ExprKind::Block(parse_block(cursor, bag))
        }
        TokenKind::Bracket(BracketKind::Paren, BracketSide::Open) => {
            parse_paren_or_tuple(cursor, bag)
        }
        TokenKind::Bracket(BracketKind::Square, BracketSide::Open) => parse_array_expr(cursor, bag),

        // ── Control-flow expressions ──────────────────────────────────────────
        TokenKind::Keyword(Keyword::If) => parse_if_expr(cursor, bag),
        TokenKind::Keyword(Keyword::Match) => parse_match_expr(cursor, bag),
        TokenKind::Keyword(Keyword::For) => parse_for_expr(cursor, bag),
        TokenKind::Keyword(Keyword::While) => parse_while_expr(cursor, bag),
        TokenKind::Keyword(Keyword::Loop) => parse_loop_expr(cursor, bag),
        TokenKind::Keyword(Keyword::Return) => parse_return_expr(cursor, bag),
        TokenKind::Keyword(Keyword::Break) => parse_break_expr(cursor, bag),
        TokenKind::Keyword(Keyword::Continue) => parse_continue_expr(cursor, bag),
        TokenKind::Keyword(Keyword::Perform) => parse_perform_expr(cursor, bag),
        TokenKind::Keyword(Keyword::With) => parse_with_expr(cursor, bag),
        TokenKind::Keyword(Keyword::Region) => parse_region_expr(cursor, bag),

        // ── Lambda `|…| body` (also `||` for zero-arg) ─────────────────────────
        TokenKind::Pipe | TokenKind::PipePipe => parse_lambda_expr(cursor, bag),

        // ── #run comptime-eval marker ─────────────────────────────────────────
        TokenKind::Hash if cursor.peek2().kind == TokenKind::Keyword(Keyword::Run) => {
            cursor.bump(); // #
            cursor.bump(); // run
            let e = parse_expr_bp(cursor, bag, unary_prefix_bp());
            ExprKind::Run { expr: Box::new(e) }
        }

        // ── §§ path reference (CSL-embedded) ─────────────────────────────────
        TokenKind::SectionRef => {
            cursor.bump();
            let path = parse_colon_path(cursor, bag, "section reference");
            ExprKind::SectionRef { path }
        }

        _ => {
            bag.push(custom("expected an expression", t.span));
            cursor.bump();
            ExprKind::Error
        }
    };

    let end = cursor.peek().span.start.max(t.span.end);
    Expr {
        span: Span::new(t.span.source, t.span.start, end),
        attrs,
        kind,
    }
}

fn unary(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag, op: UnOp) -> ExprKind {
    cursor.bump();
    let operand = parse_expr_bp(cursor, bag, unary_prefix_bp());
    ExprKind::Unary {
        op,
        operand: Box::new(operand),
    }
}

fn parse_reference_prefix(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // &
    let op = if cursor.eat(TokenKind::Keyword(Keyword::Mut)).is_some() {
        UnOp::RefMut
    } else {
        UnOp::Ref
    };
    let operand = parse_expr_bp(cursor, bag, unary_prefix_bp());
    ExprKind::Unary {
        op,
        operand: Box::new(operand),
    }
}

fn in_context_forbidding_struct_brace(_cursor: &TokenCursor<'_>) -> bool {
    // Minimal implementation : we don't attempt to detect `if`-scrutinee context here.
    // The caller of `parse_expr` in `if` / `while` / `for` headers uses a dedicated
    // `parse_expr_no_struct` (not yet extracted). For T3.2 we keep this permissive ;
    // the formatter path compensates by requiring explicit parentheses around struct
    // constructors in control-flow heads (see §§ 09 FORMATTING).
    false
}

/// Peek-ahead check : distinguish `x { ... }` struct-constructor from `x { ... }`
/// match-scrutinee-followed-by-match-body. Requires the token after `{` to match
/// the struct-body shape (`ident :` / `ident ,` / `ident }` / `..` / `}`).
fn looks_like_struct_body(cursor: &TokenCursor<'_>) -> bool {
    let mut lookahead = cursor.clone();
    lookahead.bump(); // {
    match lookahead.peek().kind {
        // empty struct `{}` — unambiguous.
        // `..base` spread-update — struct-field shape.
        TokenKind::Bracket(BracketKind::Brace, BracketSide::Close) | TokenKind::DotDot => true,
        // `ident [:| ,| }]` — struct-field shape.
        TokenKind::Ident => matches!(
            lookahead.peek2().kind,
            TokenKind::Colon
                | TokenKind::Comma
                | TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)
        ),
        _ => false,
    }
}

// ─ Composite forms ──────────────────────────────────────────────────────────

fn parse_struct_constructor(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    path: ModulePath,
) -> ExprKind {
    cursor.bump(); // {
    let mut fields = Vec::new();
    let mut spread: Option<Box<Expr>> = None;
    while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
        && !cursor.is_eof()
    {
        if cursor.eat(TokenKind::DotDot).is_some() {
            let base = parse_expr(cursor, bag);
            spread = Some(Box::new(base));
            break;
        }
        let name = parse_ident(cursor, bag, "field name");
        let value = if cursor.eat(TokenKind::Colon).is_some() {
            Some(parse_expr(cursor, bag))
        } else {
            None
        };
        let end = value.as_ref().map_or(name.span.end, |e| e.span.end);
        fields.push(StructFieldInit {
            span: Span::new(name.span.source, name.span.start, end),
            name,
            value,
        });
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `}` to close struct constructor",
            cursor.peek().span,
        ));
    }
    ExprKind::Struct {
        path,
        fields,
        spread,
    }
}

fn parse_paren_or_tuple(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // (
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
        return ExprKind::Literal(Literal {
            span: cursor.peek().span,
            kind: LiteralKind::Unit,
        });
    }
    let first = parse_expr(cursor, bag);
    if cursor.eat(TokenKind::Comma).is_some() {
        let mut elems = vec![first];
        while !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close))
            && !cursor.is_eof()
        {
            elems.push(parse_expr(cursor, bag));
            if cursor.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
            cursor.bump();
        } else {
            bag.push(custom("expected `)` to close tuple", cursor.peek().span));
        }
        return ExprKind::Tuple(elems);
    }
    // Single-expr → paren-grouping
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom("expected `)` to close paren", cursor.peek().span));
    }
    ExprKind::Paren(Box::new(first))
}

fn parse_array_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // [
    if cursor.check(TokenKind::Bracket(BracketKind::Square, BracketSide::Close)) {
        cursor.bump();
        return ExprKind::Array(ArrayExpr::List(Vec::new()));
    }
    let first = parse_expr(cursor, bag);
    if cursor.eat(TokenKind::Semi).is_some() {
        // [elem ; N]
        let len = parse_expr(cursor, bag);
        if cursor.check(TokenKind::Bracket(BracketKind::Square, BracketSide::Close)) {
            cursor.bump();
        } else {
            bag.push(custom(
                "expected `]` to close array-repeat expression",
                cursor.peek().span,
            ));
        }
        return ExprKind::Array(ArrayExpr::Repeat {
            elem: Box::new(first),
            len: Box::new(len),
        });
    }
    let mut items = vec![first];
    while cursor.eat(TokenKind::Comma).is_some() {
        if cursor.check(TokenKind::Bracket(BracketKind::Square, BracketSide::Close)) {
            break;
        }
        items.push(parse_expr(cursor, bag));
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Square, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom("expected `]` to close array", cursor.peek().span));
    }
    ExprKind::Array(ArrayExpr::List(items))
}

// ─ Control-flow expressions ─────────────────────────────────────────────────

fn parse_if_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // if
    let cond = parse_expr(cursor, bag);
    let then_branch = parse_block(cursor, bag);
    let else_branch = if cursor.eat(TokenKind::Keyword(Keyword::Else)).is_some() {
        // `else if` chains or plain `else { … }`
        let e = if cursor.check(TokenKind::Keyword(Keyword::If)) {
            parse_expr(cursor, bag)
        } else {
            let b = parse_block(cursor, bag);
            let span = b.span;
            Expr {
                span,
                attrs: Vec::new(),
                kind: ExprKind::Block(b),
            }
        };
        Some(Box::new(e))
    } else {
        None
    };
    ExprKind::If {
        cond: Box::new(cond),
        then_branch,
        else_branch,
    }
}

fn parse_match_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // match
    let scrutinee = parse_expr(cursor, bag);
    expect(
        cursor,
        bag,
        TokenKind::Bracket(BracketKind::Brace, BracketSide::Open),
        "match body open",
    );
    let mut arms = Vec::new();
    while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
        && !cursor.is_eof()
    {
        let arm_attrs = attr::parse_outer_attrs(cursor, bag);
        let pat_node = pat::parse_pattern(cursor, bag);
        let guard = if cursor.eat(TokenKind::Keyword(Keyword::If)).is_some() {
            Some(parse_expr(cursor, bag))
        } else {
            None
        };
        expect(cursor, bag, TokenKind::FatArrow, "match arm");
        let body = parse_expr(cursor, bag);
        let arm_span = Span::new(pat_node.span.source, pat_node.span.start, body.span.end);
        arms.push(MatchArm {
            span: arm_span,
            attrs: arm_attrs,
            pat: pat_node,
            guard,
            body,
        });
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    expect(
        cursor,
        bag,
        TokenKind::Bracket(BracketKind::Brace, BracketSide::Close),
        "match body close",
    );
    ExprKind::Match {
        scrutinee: Box::new(scrutinee),
        arms,
    }
}

fn parse_for_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // for
    let pat_node = pat::parse_pattern(cursor, bag);
    expect(cursor, bag, TokenKind::Keyword(Keyword::In), "for-in");
    let iter = parse_expr(cursor, bag);
    let body = parse_block(cursor, bag);
    ExprKind::For {
        pat: pat_node,
        iter: Box::new(iter),
        body,
    }
}

fn parse_while_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // while
    let cond = parse_expr(cursor, bag);
    let body = parse_block(cursor, bag);
    ExprKind::While {
        cond: Box::new(cond),
        body,
    }
}

fn parse_loop_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // loop
    let body = parse_block(cursor, bag);
    ExprKind::Loop { body }
}

fn parse_return_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // return
    let value = if can_start_expression(cursor.peek().kind) {
        Some(Box::new(parse_expr(cursor, bag)))
    } else {
        None
    };
    ExprKind::Return { value }
}

fn parse_break_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // break
    let label = if cursor.check(TokenKind::Apostrophe) {
        cursor.bump(); // '
        Some(parse_ident(cursor, bag, "break label"))
    } else {
        None
    };
    let value = if can_start_expression(cursor.peek().kind) {
        Some(Box::new(parse_expr(cursor, bag)))
    } else {
        None
    };
    ExprKind::Break { label, value }
}

fn parse_continue_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // continue
    let label = if cursor.check(TokenKind::Apostrophe) {
        cursor.bump(); // '
        Some(parse_ident(cursor, bag, "continue label"))
    } else {
        None
    };
    ExprKind::Continue { label }
}

fn parse_perform_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // perform
    let path = parse_colon_path(cursor, bag, "effect path");
    let args = if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
        parse_call_args(cursor, bag)
    } else {
        Vec::new()
    };
    ExprKind::Perform { path, args }
}

fn parse_with_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // with
    let handler = parse_expr(cursor, bag);
    let body = parse_block(cursor, bag);
    ExprKind::With {
        handler: Box::new(handler),
        body,
    }
}

fn parse_region_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    cursor.bump(); // region
    let label = if cursor.check(TokenKind::Apostrophe) {
        cursor.bump();
        Some(parse_ident(cursor, bag, "region label"))
    } else {
        None
    };
    let body = parse_block(cursor, bag);
    ExprKind::Region { label, body }
}

fn parse_lambda_expr(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> ExprKind {
    let open = cursor.bump();
    let mut params = Vec::new();
    if open.kind == TokenKind::Pipe {
        while !cursor.check(TokenKind::Pipe) && !cursor.is_eof() {
            let p = parse_lambda_param(cursor, bag);
            params.push(p);
            if cursor.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        expect(cursor, bag, TokenKind::Pipe, "lambda parameter list close");
    } // else `||` → empty param list, already consumed.
    let return_ty = if cursor.eat(TokenKind::Arrow).is_some() {
        Some(ty::parse_type(cursor, bag))
    } else {
        None
    };
    let body = parse_expr(cursor, bag);
    ExprKind::Lambda {
        params,
        return_ty,
        body: Box::new(body),
    }
}

fn parse_lambda_param(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Param {
    let pat_node = pat::parse_pattern(cursor, bag);
    let ty_node = if cursor.eat(TokenKind::Colon).is_some() {
        ty::parse_type(cursor, bag)
    } else {
        Type {
            span: pat_node.span,
            kind: TypeKind::Infer,
        }
    };
    let span = Span::new(
        pat_node.span.source,
        pat_node.span.start,
        ty_node.span.end.max(pat_node.span.end),
    );
    Param {
        span,
        attrs: Vec::new(),
        pat: pat_node,
        ty: ty_node,
        default: None,
    }
}

// ─ Postfix application ──────────────────────────────────────────────────────

fn apply_postfix(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    lhs: Expr,
    op: PostfixOp,
) -> Expr {
    match op {
        PostfixOp::Field => {
            cursor.bump(); // .
            let name = parse_ident(cursor, bag, "field name");
            let span = Span::new(lhs.span.source, lhs.span.start, name.span.end);
            Expr {
                span,
                attrs: Vec::new(),
                kind: ExprKind::Field {
                    obj: Box::new(lhs),
                    name,
                },
            }
        }
        PostfixOp::PathCont => {
            // `expr::ident` — extend the path if the lhs is a `Path` node, else treat as
            // method-path continuation via a Field-like form.
            cursor.bump(); // ::
            if cursor.eat(TokenKind::Lt).is_some() {
                // Turbofish : `::<T, U>` — T11-D39 now captures the type-args and
                // attaches them to the immediately-following Call (if any).
                let mut type_args: Vec<Type> = Vec::new();
                while !cursor.check(TokenKind::Gt) && !cursor.is_eof() {
                    type_args.push(ty::parse_type(cursor, bag));
                    if cursor.eat(TokenKind::Comma).is_none() {
                        break;
                    }
                }
                if cursor.eat(TokenKind::Gt).is_none() {
                    bag.push(custom(
                        "expected `>` to close turbofish",
                        cursor.peek().span,
                    ));
                }
                // If immediately followed by `(` — consume as a Call with type_args.
                // Otherwise return the lhs untouched ; type-args are dropped for non-
                // call uses (e.g., `Vec::<i32>` as a type reference) — stage-0 scope.
                if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
                    let args = parse_call_args(cursor, bag);
                    let end = cursor.peek().span.start.max(lhs.span.end);
                    return Expr {
                        span: Span::new(lhs.span.source, lhs.span.start, end),
                        attrs: Vec::new(),
                        kind: ExprKind::Call {
                            callee: Box::new(lhs),
                            args,
                            type_args,
                        },
                    };
                }
                return lhs;
            }
            let name = parse_ident(cursor, bag, "path continuation");
            let span = Span::new(lhs.span.source, lhs.span.start, name.span.end);
            // Convert into a single extended Path when possible.
            if let ExprKind::Path(mut path) = lhs.kind {
                path.segments.push(name);
                path.span = span;
                Expr {
                    span,
                    attrs: Vec::new(),
                    kind: ExprKind::Path(path),
                }
            } else {
                // Fallback : represent `e::name` as a Field (lossy but parseable).
                Expr {
                    span,
                    attrs: Vec::new(),
                    kind: ExprKind::Field {
                        obj: Box::new(lhs),
                        name,
                    },
                }
            }
        }
        PostfixOp::Call => {
            let args = parse_call_args(cursor, bag);
            let end = cursor.peek().span.start.max(lhs.span.end);
            Expr {
                span: Span::new(lhs.span.source, lhs.span.start, end),
                attrs: Vec::new(),
                kind: ExprKind::Call {
                    callee: Box::new(lhs),
                    args,
                    type_args: Vec::new(),
                },
            }
        }
        PostfixOp::Index => {
            cursor.bump(); // [
            let index = parse_expr(cursor, bag);
            if cursor.check(TokenKind::Bracket(BracketKind::Square, BracketSide::Close)) {
                cursor.bump();
            } else {
                bag.push(custom("expected `]` to close index", cursor.peek().span));
            }
            let end = cursor.peek().span.start.max(lhs.span.end);
            Expr {
                span: Span::new(lhs.span.source, lhs.span.start, end),
                attrs: Vec::new(),
                kind: ExprKind::Index {
                    obj: Box::new(lhs),
                    index: Box::new(index),
                },
            }
        }
        PostfixOp::Try => {
            let q = cursor.bump();
            Expr {
                span: Span::new(lhs.span.source, lhs.span.start, q.span.end),
                attrs: Vec::new(),
                kind: ExprKind::Try {
                    expr: Box::new(lhs),
                },
            }
        }
        PostfixOp::Cast => {
            cursor.bump(); // as
            let target = ty::parse_type(cursor, bag);
            let span = Span::new(lhs.span.source, lhs.span.start, target.span.end);
            Expr {
                span,
                attrs: Vec::new(),
                kind: ExprKind::Cast {
                    expr: Box::new(lhs),
                    ty: target,
                },
            }
        }
    }
}

fn parse_call_args(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Vec<CallArg> {
    cursor.bump(); // (
    let mut args = Vec::new();
    while !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close))
        && !cursor.is_eof()
    {
        if cursor.peek().kind == TokenKind::Ident && cursor.peek2().kind == TokenKind::Eq {
            let name = parse_ident(cursor, bag, "named arg");
            cursor.bump(); // =
            let value = parse_expr(cursor, bag);
            args.push(CallArg::Named { name, value });
        } else {
            args.push(CallArg::Positional(parse_expr(cursor, bag)));
        }
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `)` to close call arguments",
            cursor.peek().span,
        ));
    }
    args
}

// ─ Infix combination ────────────────────────────────────────────────────────

fn combine_infix(op: InfixOp, lhs: Expr, rhs: Expr) -> Expr {
    let span = Span::new(lhs.span.source, lhs.span.start, rhs.span.end);
    let kind = match op {
        InfixOp::Bin(b) => ExprKind::Binary {
            op: b,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        InfixOp::Assign(compound) => ExprKind::Assign {
            op: compound,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        InfixOp::Pipeline => ExprKind::Pipeline {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        InfixOp::Range { inclusive } => ExprKind::Range {
            lo: Some(Box::new(lhs)),
            hi: Some(Box::new(rhs)),
            inclusive,
        },
        InfixOp::TryDefault => ExprKind::TryDefault {
            expr: Box::new(lhs),
            default: Box::new(rhs),
        },
    };
    Expr {
        span,
        attrs: Vec::new(),
        kind,
    }
}

// ─ Block ────────────────────────────────────────────────────────────────────

/// Parse a `{ stmts … trailing? }` block.
#[must_use]
pub fn parse_block(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Block {
    let open = expect(
        cursor,
        bag,
        TokenKind::Bracket(BracketKind::Brace, BracketSide::Open),
        "block open",
    );
    let mut stmts: Vec<Stmt> = Vec::new();
    let mut trailing: Option<Box<Expr>> = None;
    while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
        && !cursor.is_eof()
    {
        // `let` binding statement
        if cursor.check(TokenKind::Keyword(Keyword::Let)) {
            let s = parse_let_stmt(cursor, bag);
            stmts.push(s);
            continue;
        }
        // Otherwise : expression-statement ; last expression without `;` becomes trailing.
        let e = parse_expr(cursor, bag);
        if cursor.eat(TokenKind::Semi).is_some() {
            let s = Stmt {
                span: e.span,
                kind: StmtKind::Expr(e),
            };
            stmts.push(s);
        } else if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            || cursor.is_eof()
        {
            trailing = Some(Box::new(e));
            break;
        } else {
            // Treat expression-at-statement-position without `;` between statements as
            // stmt-kind expression and continue (permissive — many block-forms like
            // `if`/`for` don't need trailing `;`).
            let s = Stmt {
                span: e.span,
                kind: StmtKind::Expr(e),
            };
            stmts.push(s);
        }
    }
    let close_span = cursor.peek().span;
    expect(
        cursor,
        bag,
        TokenKind::Bracket(BracketKind::Brace, BracketSide::Close),
        "block close",
    );
    Block {
        span: Span::new(open.source, open.start, close_span.end),
        stmts,
        trailing,
    }
}

fn parse_let_stmt(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Stmt {
    let kw = cursor.bump(); // let
    let mutable = cursor.eat(TokenKind::Keyword(Keyword::Mut)).is_some();
    let pat_node = if mutable {
        // Re-wrap as `Binding { mutable: true, … }` — simplest path : parse an ident.
        let name = parse_ident(cursor, bag, "mutable binding name");
        Pattern {
            span: name.span,
            kind: PatternKind::Binding {
                mutable: true,
                name,
            },
        }
    } else {
        pat::parse_pattern(cursor, bag)
    };
    let ty_node = if cursor.eat(TokenKind::Colon).is_some() {
        Some(ty::parse_type(cursor, bag))
    } else {
        None
    };
    let value = if cursor.eat(TokenKind::Eq).is_some() {
        Some(parse_expr(cursor, bag))
    } else {
        None
    };
    let end = cursor.peek().span.start;
    // Consume trailing `;` if present.
    let trailing_semi = cursor.eat(TokenKind::Semi);
    let span_end = trailing_semi.map_or(end, |s| s.end);
    Stmt {
        span: Span::new(kw.span.source, kw.span.start, span_end),
        kind: StmtKind::Let {
            attrs: Vec::new(),
            pat: pat_node,
            ty: ty_node,
            value,
        },
    }
}

// ─ Heuristics ───────────────────────────────────────────────────────────────

const fn can_start_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::IntLiteral
            | TokenKind::FloatLiteral
            | TokenKind::StringLiteral(_)
            | TokenKind::CharLiteral
            | TokenKind::Ident
            | TokenKind::Keyword(
                Keyword::True
                    | Keyword::False
                    | Keyword::SelfValue
                    | Keyword::If
                    | Keyword::Match
                    | Keyword::For
                    | Keyword::While
                    | Keyword::Loop
                    | Keyword::Return
                    | Keyword::Break
                    | Keyword::Continue
                    | Keyword::Perform
                    | Keyword::With
                    | Keyword::Region
            )
            | TokenKind::Bracket(
                BracketKind::Paren | BracketKind::Square | BracketKind::Brace,
                BracketSide::Open,
            )
            | TokenKind::Minus
            | TokenKind::Bang
            | TokenKind::Amp
            | TokenKind::Star
            | TokenKind::Tilde
            | TokenKind::Pipe
            | TokenKind::PipePipe
            | TokenKind::At
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_block, parse_expr};
    use crate::cursor::TokenCursor;
    use cssl_ast::{BinOp, DiagnosticBag, ExprKind, SourceFile, SourceId, Surface};

    fn prep(src: &str) -> (SourceFile, Vec<cssl_lex::Token>) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        (f, toks)
    }

    #[test]
    fn int_literal() {
        let (_f, toks) = prep("42");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::Literal(_)));
    }

    #[test]
    fn simple_add() {
        let (_f, toks) = prep("1 + 2");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        if let ExprKind::Binary { op, .. } = e.kind {
            assert_eq!(op, BinOp::Add);
        } else {
            panic!("expected Binary");
        }
    }

    #[test]
    fn mul_binds_tighter_than_add() {
        let (_f, toks) = prep("1 + 2 * 3");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        // Root should be `+`; its rhs should be `*`.
        match e.kind {
            ExprKind::Binary { op, rhs, .. } => {
                assert_eq!(op, BinOp::Add);
                if let ExprKind::Binary { op: op2, .. } = rhs.kind {
                    assert_eq!(op2, BinOp::Mul);
                } else {
                    panic!("expected rhs to be Binary");
                }
            }
            _ => panic!("expected Binary"),
        }
    }

    #[test]
    fn left_assoc_subtraction() {
        // `10 - 3 - 2` = `(10 - 3) - 2`
        let (_f, toks) = prep("10 - 3 - 2");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        if let ExprKind::Binary { op, lhs, .. } = e.kind {
            assert_eq!(op, BinOp::Sub);
            // lhs should itself be a Sub
            assert!(matches!(lhs.kind, ExprKind::Binary { op: BinOp::Sub, .. }));
        } else {
            panic!("expected Binary");
        }
    }

    #[test]
    fn unary_negation() {
        let (_f, toks) = prep("-x");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::Unary { .. }));
    }

    #[test]
    fn path_expr() {
        let (_f, toks) = prep("foo::bar");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        if let ExprKind::Path(p) = e.kind {
            assert_eq!(p.segments.len(), 2);
        } else {
            panic!("expected Path, got {:?}", e.kind);
        }
    }

    #[test]
    fn call_expr() {
        let (_f, toks) = prep("f(1, 2)");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::Call { .. }));
    }

    #[test]
    fn field_access() {
        let (_f, toks) = prep("obj.field");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::Field { .. }));
    }

    #[test]
    fn index_expr() {
        let (_f, toks) = prep("arr[0]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::Index { .. }));
    }

    #[test]
    fn tuple_expr() {
        let (_f, toks) = prep("(1, 2, 3)");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::Tuple(_)));
    }

    #[test]
    fn if_expr() {
        let (_f, toks) = prep("if x { 1 } else { 2 }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::If { .. }));
    }

    #[test]
    fn block_with_trailing_expr() {
        let (_f, toks) = prep("{ let x = 1 ; x + 2 }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let b = parse_block(&mut c, &mut bag);
        assert_eq!(b.stmts.len(), 1);
        assert!(b.trailing.is_some());
    }

    #[test]
    fn cast_expr() {
        let (_f, toks) = prep("x as f32");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::Cast { .. }));
    }

    #[test]
    fn pipeline_expr() {
        let (_f, toks) = prep("x |> f |> g");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert!(matches!(e.kind, ExprKind::Pipeline { .. }));
    }

    #[test]
    fn assign_right_assoc() {
        let (_f, toks) = prep("a = b = c");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        if let ExprKind::Assign { rhs, .. } = e.kind {
            // rhs should itself be Assign.
            assert!(matches!(rhs.kind, ExprKind::Assign { .. }));
        } else {
            panic!("expected Assign");
        }
    }

    // ─── T11-CC-PARSER-4 : string + char + array literal parse-paths ────────

    #[test]
    fn parse_string_lit_basic() {
        // `"hello"` → Literal{Str}.
        let (_f, toks) = prep(r#""hello""#);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        match e.kind {
            ExprKind::Literal(lit) => {
                assert_eq!(lit.kind, cssl_ast::LiteralKind::Str);
            }
            other => panic!("expected Literal(Str), got {other:?}"),
        }
    }

    #[test]
    fn parse_string_lit_escapes() {
        // `"a\nb\tc"` — single Literal token, escapes lex atomically.
        let (_f, toks) = prep(r#""a\nb\tc""#);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        assert!(matches!(
            e.kind,
            ExprKind::Literal(cssl_ast::Literal {
                kind: cssl_ast::LiteralKind::Str,
                ..
            })
        ));
    }

    #[test]
    fn parse_string_lit_unicode() {
        // Multibyte UTF-8 body — `"鳥居"` (toriy gate).
        let (_f, toks) = prep("\"鳥居\"");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        assert!(matches!(
            e.kind,
            ExprKind::Literal(cssl_ast::Literal {
                kind: cssl_ast::LiteralKind::Str,
                ..
            })
        ));
    }

    #[test]
    fn parse_string_lit_hex_unicode_escapes() {
        // `"\x41\u{1F600}"` — both byte-hex and brace-unicode escape sequences.
        let (_f, toks) = prep(r#""\x41\u{1F600}""#);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        assert!(matches!(
            e.kind,
            ExprKind::Literal(cssl_ast::Literal {
                kind: cssl_ast::LiteralKind::Str,
                ..
            })
        ));
    }

    #[test]
    fn parse_string_with_quote_escape() {
        // `"a\"b"` — embedded escaped quote.
        let (_f, toks) = prep(r#""a\"b""#);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        assert!(matches!(
            e.kind,
            ExprKind::Literal(cssl_ast::Literal {
                kind: cssl_ast::LiteralKind::Str,
                ..
            })
        ));
    }

    #[test]
    fn parse_char_lit_ascii() {
        // `'a'` → Literal{Char}.
        let (_f, toks) = prep("'a'");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        match e.kind {
            ExprKind::Literal(lit) => {
                assert_eq!(lit.kind, cssl_ast::LiteralKind::Char);
            }
            other => panic!("expected Literal(Char), got {other:?}"),
        }
    }

    #[test]
    fn parse_char_lit_escape() {
        // `'\n'` — escape sequence char literal.
        let (_f, toks) = prep(r"'\n'");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        match e.kind {
            ExprKind::Literal(lit) => {
                assert_eq!(lit.kind, cssl_ast::LiteralKind::Char);
            }
            other => panic!("expected Literal(Char), got {other:?}"),
        }
    }

    #[test]
    fn parse_array_lit_basic() {
        // `[1, 2, 3]` — array literal in `List` form.
        let (_f, toks) = prep("[1, 2, 3]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        match e.kind {
            ExprKind::Array(cssl_ast::ArrayExpr::List(items)) => {
                assert_eq!(items.len(), 3);
            }
            other => panic!("expected Array(List, len=3), got {other:?}"),
        }
    }

    #[test]
    fn parse_nested_array_lit() {
        // `[[1, 2], [3, 4]]` — outer list of two inner lists.
        let (_f, toks) = prep("[[1, 2], [3, 4]]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        match e.kind {
            ExprKind::Array(cssl_ast::ArrayExpr::List(items)) => {
                assert_eq!(items.len(), 2);
                for inner in &items {
                    assert!(matches!(
                        inner.kind,
                        ExprKind::Array(cssl_ast::ArrayExpr::List(_))
                    ));
                }
            }
            other => panic!("expected Array(List, len=2), got {other:?}"),
        }
    }

    #[test]
    fn parse_array_lit_decimal_triple() {
        // `[10, 20, 30]` — guards the comma-separated array-element path with
        // a third element.
        let (_f, toks) = prep("[10, 20, 30]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        match e.kind {
            ExprKind::Array(cssl_ast::ArrayExpr::List(items)) => {
                assert_eq!(items.len(), 3);
            }
            other => panic!("expected Array(List, len=3), got {other:?}"),
        }
    }

    #[test]
    fn parse_const_string_full_pipeline() {
        // End-to-end : `const TOOL_NAME: &str = "engine.state"` parses cleanly
        // through the full module-parser path and registers as a ConstItem.
        let src = r#"const TOOL_NAME: &str = "engine.state""#;
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "expected zero parse errors");
        assert_eq!(m.items.len(), 1);
        assert!(matches!(m.items[0], cssl_ast::Item::Const(_)));
    }

    // ─── T11-CC-PARSER-9 (W-CC-array-lit) ────────────────────────────────────
    //
    // The four tests below pin down const-array literal patterns from real LoA
    // scenes — specifically the mandelbulb shader-stub byte array. The unifying
    // root cause they all guard against is the lexer formerly splitting
    // `0x53` into `IntLit(0) + Ident(x53)`. Each test exercises one axis of the
    // pattern (whitespace layout, trailing comma, embedded comment, full
    // mandelbulb shape) so a regression on any single dimension fails loudly.
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_const_array_lit_multi_line() {
        // Multi-line array literal whose elements are hex bytes — newlines
        // between the `[` and `]` must be transparent to the array parser.
        let src = "const X: [u8; 3] = [\n    0x01,\n    0x02,\n    0x03\n]";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parse errors, got: {:?}",
            bag.iter().collect::<Vec<_>>(),
        );
        assert_eq!(m.items.len(), 1);
        assert!(matches!(m.items[0], cssl_ast::Item::Const(_)));
    }

    #[test]
    fn parse_const_array_lit_trailing_comma() {
        // Trailing comma after the last element — common style for diff-friendly
        // multi-line literals; parser must not double-consume past the `]`.
        let src = "const X: [u8; 3] = [\n    0x01,\n    0x02,\n    0x03,\n]";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parse errors, got: {:?}",
            bag.iter().collect::<Vec<_>>(),
        );
        assert_eq!(m.items.len(), 1);
        assert!(matches!(m.items[0], cssl_ast::Item::Const(_)));
    }

    #[test]
    fn parse_const_array_lit_with_block_comment() {
        // Block comment immediately inside the array body — trivia must be
        // skipped between `[` and the first element, as well as between
        // adjacent elements.
        let src = "const X: [u8; 3] = [\n    /* marker */\n    0x01, 0x02, 0x03,\n]";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parse errors, got: {:?}",
            bag.iter().collect::<Vec<_>>(),
        );
        assert_eq!(m.items.len(), 1);
        assert!(matches!(m.items[0], cssl_ast::Item::Const(_)));
    }

    #[test]
    fn parse_const_array_lit_mandelbulb_style() {
        // Verbatim shape from `Labyrinth of Apocalypse/scenes/mandelbulb.cssl`
        // (T11-CC-PARSER-9 entry-point case). Wraps two physical lines of
        // eight + five hex bytes with whitespace-padded type annotation.
        let src = "const MANDELBULB_SHADER_STUB : [u8 ; 13] = [\n    \
                   0x53, 0x54, 0x55, 0x42, 0x4D, 0x41, 0x4E, 0x44,\n    \
                   0x45, 0x4C, 0x42, 0x4C, 0x42,\n] ;";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parse errors, got: {:?}",
            bag.iter().collect::<Vec<_>>(),
        );
        assert_eq!(m.items.len(), 1);
        assert!(matches!(m.items[0], cssl_ast::Item::Const(_)));
    }

    // ─── T11-CC-PARSER-11 (W-CC-numeric-suffix) ───────────────────────────────
    //
    // Bare-suffix typed-numeric literals (`1u16`, `42i32`, `3.14f32`,
    // `0xCAFE_BABEu64`) parse as a single `Literal{Int|Float}` whose span
    // covers the digits + suffix together. Downstream HIR re-slices the span
    // via `SourceFile::slice` to extract the suffix for type-resolution. The
    // parser itself is unchanged — the lexer regex extension does the work.
    //
    // Critical invariant : `1u16 << 0` MUST parse as a `Binary{Shl}` with two
    // IntLit operands ; the suffix must NOT bleed into the operator/rhs.
    // ──────────────────────────────────────────────────────────────────────────
    #[test]
    fn parse_const_with_suffixed_int() {
        // `const X: u16 = 1u16` end-to-end — full module parse path.
        let src = "const X: u16 = 1u16";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parse errors, got: {:?}",
            bag.iter().collect::<Vec<_>>(),
        );
        assert_eq!(m.items.len(), 1);
        match &m.items[0] {
            cssl_ast::Item::Const(c) => {
                // The const value should be a Literal{Int}.
                assert!(matches!(
                    c.value.kind,
                    ExprKind::Literal(cssl_ast::Literal {
                        kind: cssl_ast::LiteralKind::Int,
                        ..
                    })
                ));
                // The literal's span must cover all 4 bytes "1u16".
                if let ExprKind::Literal(lit) = &c.value.kind {
                    let txt = f.slice(lit.span.start, lit.span.end).unwrap();
                    assert_eq!(txt, "1u16");
                }
            }
            other => panic!("expected ConstItem, got {other:?}"),
        }
    }

    #[test]
    fn parse_suffix_then_shift() {
        // `1u16 << 0` — the suffix MUST stick to the lhs literal ; the `<<`
        // operator must be recognized and the rhs must parse independently.
        let (f, toks) = prep("1u16 << 0");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        match e.kind {
            ExprKind::Binary { op, lhs, rhs } => {
                assert_eq!(op, BinOp::Shl);
                // lhs must be an IntLit ; the inner Literal.span covers exactly
                // "1u16" (4 bytes) — the lexer regex glues suffix to digit run.
                // (The wrapping Expr.span extends to next-token start, which is
                // why we slice via the Literal.span specifically.)
                if let ExprKind::Literal(lit) = &lhs.kind {
                    assert_eq!(lit.kind, cssl_ast::LiteralKind::Int);
                    let lit_txt = f.slice(lit.span.start, lit.span.end).unwrap();
                    assert_eq!(lit_txt, "1u16");
                } else {
                    panic!("expected lhs Literal(Int), got {:?}", lhs.kind);
                }
                // rhs must be IntLit(0).
                assert!(matches!(
                    rhs.kind,
                    ExprKind::Literal(cssl_ast::Literal {
                        kind: cssl_ast::LiteralKind::Int,
                        ..
                    })
                ));
            }
            other => panic!("expected Binary{{Shl}}, got {other:?}"),
        }
    }

    #[test]
    fn parse_underscored_hex_with_suffix() {
        // `0xDEAD_BEEFu64` — bonus : underscored hex literal with bare-suffix
        // parses as a single IntLit whose span covers all 14 bytes.
        let (f, toks) = prep("0xDEAD_BEEFu64");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let e = parse_expr(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        match e.kind {
            ExprKind::Literal(lit) => {
                assert_eq!(lit.kind, cssl_ast::LiteralKind::Int);
                let txt = f.slice(lit.span.start, lit.span.end).unwrap();
                assert_eq!(txt, "0xDEAD_BEEFu64");
            }
            other => panic!("expected Literal(Int), got {other:?}"),
        }
    }

    #[test]
    fn parse_const_with_suffixed_float() {
        // `const TAU: f32 = 6.283185f32` — float-suffix end-to-end.
        let src = "const TAU: f32 = 6.283185f32";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parse errors, got: {:?}",
            bag.iter().collect::<Vec<_>>(),
        );
        assert_eq!(m.items.len(), 1);
        match &m.items[0] {
            cssl_ast::Item::Const(c) => {
                if let ExprKind::Literal(lit) = &c.value.kind {
                    assert_eq!(lit.kind, cssl_ast::LiteralKind::Float);
                    let txt = f.slice(lit.span.start, lit.span.end).unwrap();
                    assert_eq!(txt, "6.283185f32");
                } else {
                    panic!("expected Literal(Float), got {:?}", c.value.kind);
                }
            }
            other => panic!("expected ConstItem, got {other:?}"),
        }
    }

    #[test]
    fn parse_const_bitor_with_suffixed_hex() {
        // `const FLAGS: u32 = 0xFF00u32 | 0x00FFu32` — bit-or composition
        // with suffix on both sides.
        let src = "const FLAGS: u32 = 0xFF00u32 | 0x00FFu32";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parse errors, got: {:?}",
            bag.iter().collect::<Vec<_>>(),
        );
        assert_eq!(m.items.len(), 1);
        match &m.items[0] {
            cssl_ast::Item::Const(c) => {
                assert!(matches!(c.value.kind, ExprKind::Binary { op: BinOp::BitOr, .. }));
            }
            other => panic!("expected ConstItem, got {other:?}"),
        }
    }

    #[test]
    fn parse_acceptance_module_test_suffix() {
        // The full acceptance snippet from the mission spec.
        let src = "module com.apocky.loa.test_suffix\n\n\
                   const KEY_A: u8 = 30u8\n\
                   const FLAGS: u32 = 0xFF00u32 | 0x00FFu32\n\
                   const TAU: f32 = 6.283185f32\n\n\
                   fn main() -> i32 { 42 }";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parse errors, got: {:?}",
            bag.iter().collect::<Vec<_>>(),
        );
        // 3 const items + 1 fn item = 4 total.
        assert_eq!(m.items.len(), 4);
    }
}
