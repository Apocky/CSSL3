//! T11-D141 mock — evaluate a small subset of HIR literal expressions
//! into a [`Value`].
//!
//! § ROLE
//!   T11-D141 (comptime-eval) is the canonical native evaluator that compiles
//!   `#run`-marked expressions to throwaway-native + executes them at compile
//!   time. While that slice lands, this module provides a deterministic
//!   test-double sufficient to drive the [`crate::SpecializerPass`] tests +
//!   the [`crate::kan_specialize_demo`] integration scenarios.
//!
//!   When D141 lands, [`evaluate_comptime_block_mock`] is wholesale-swapped
//!   with the real implementation : the function-signature stays the same,
//!   only the inner walking changes from "literal-tree" to "native-execute".
//!
//! § SUPPORTED EXPRESSIONS
//!   - Literal Int / Float / Bool / Unit / Str.
//!   - Binary arithmetic on literals : `+`, `-`, `*`, `/`, `%`.
//!   - Comparison ops : `==`, `!=`, `<`, `<=`, `>`, `>=`.
//!   - Logical ops : `&&`, `||`.
//!   - Unary `-x`, `!x`.
//!   - Tuples of supported expressions.
//!   - `if` expressions whose condition resolves to a Bool literal.
//!   - Parenthesized groupings.
//!
//! § DELIBERATELY OUT-OF-SCOPE FOR THE MOCK
//!   - Function calls (no fn-table walk in the mock).
//!   - Pattern matching, loops, mutation.
//!   - Closures, perform/with, region/handle.
//!   - Path lookups (no env / scope mock — every "name" returns Unbound).

use cssl_hir::{HirBinOp, HirBlock, HirExpr, HirExprKind, HirLiteralKind, HirUnOp};

use crate::value::{CompIntWidth, Value};

/// Failure modes for the mock evaluator.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MockEvalError {
    /// The input expression form is outside the mock-evaluator's coverage.
    #[error("mock evaluator does not handle this expression form : {what}")]
    Unsupported { what: String },
    /// Type mismatch in an arithmetic / comparison op.
    #[error("type mismatch in binary op : lhs={lhs_ty}, rhs={rhs_ty}")]
    TypeMismatch { lhs_ty: String, rhs_ty: String },
    /// Division by zero observed during evaluation.
    #[error("division by zero in mock evaluator")]
    DivByZero,
    /// Unbound identifier — the mock has no scope/env so any path-lookup fails.
    #[error("unbound identifier : {name}")]
    Unbound { name: String },
    /// The literal text could not be parsed as the indicated kind.
    #[error("could not parse literal {text:?} as {kind}")]
    LiteralParse { text: String, kind: &'static str },
}

/// Evaluate an HIR expression to a [`Value`]. Source-text is needed so
/// literal numeric tokens can be re-parsed into i64 / f64 — the
/// [`HirLiteralKind`] only tags the kind, not the parsed number.
pub fn evaluate_comptime_expr_mock(expr: &HirExpr, source: &str) -> Result<Value, MockEvalError> {
    match &expr.kind {
        HirExprKind::Literal(lit) => eval_literal(&lit.kind, expr.span, source),
        HirExprKind::Paren(inner) => evaluate_comptime_expr_mock(inner, source),
        HirExprKind::Binary { op, lhs, rhs } => {
            let l = evaluate_comptime_expr_mock(lhs, source)?;
            let r = evaluate_comptime_expr_mock(rhs, source)?;
            eval_binary(*op, &l, &r)
        }
        HirExprKind::Unary { op, operand } => {
            let v = evaluate_comptime_expr_mock(operand, source)?;
            eval_unary(*op, &v)
        }
        HirExprKind::Tuple(elems) => {
            let mut out = Vec::with_capacity(elems.len());
            for e in elems {
                out.push(evaluate_comptime_expr_mock(e, source)?);
            }
            Ok(Value::Tuple(out))
        }
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let c = evaluate_comptime_expr_mock(cond, source)?;
            match c {
                Value::Bool(true) => evaluate_comptime_block_mock(then_branch, source),
                Value::Bool(false) => match else_branch {
                    Some(e) => evaluate_comptime_expr_mock(e, source),
                    None => Ok(Value::Unit),
                },
                _ => Err(MockEvalError::TypeMismatch {
                    lhs_ty: "if-cond".into(),
                    rhs_ty: format!("non-bool {c}"),
                }),
            }
        }
        HirExprKind::Block(b) => evaluate_comptime_block_mock(b, source),
        HirExprKind::Path { segments, .. } => Err(MockEvalError::Unbound {
            // The mock has no scope/env so any path lookup is unbound.
            // Render the symbol-tags via Debug since Symbol opaque-wraps
            // the interner key.
            name: segments
                .iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join("::"),
        }),
        HirExprKind::Run { expr } => evaluate_comptime_expr_mock(expr, source),
        _ => Err(MockEvalError::Unsupported {
            what: hir_expr_kind_name(&expr.kind).to_string(),
        }),
    }
}

/// Evaluate a HIR block : run statements (which the mock skips since it has
/// no env) + evaluate the trailing expression. If no trailing exists →
/// `Value::Unit`.
pub fn evaluate_comptime_block_mock(
    block: &HirBlock,
    source: &str,
) -> Result<Value, MockEvalError> {
    if let Some(trailing) = &block.trailing {
        evaluate_comptime_expr_mock(trailing, source)
    } else {
        Ok(Value::Unit)
    }
}

fn eval_literal(
    kind: &HirLiteralKind,
    span: cssl_ast::Span,
    source: &str,
) -> Result<Value, MockEvalError> {
    match kind {
        HirLiteralKind::Bool(b) => Ok(Value::Bool(*b)),
        HirLiteralKind::Unit => Ok(Value::Unit),
        HirLiteralKind::Int => {
            let text = span_text(source, span);
            let cleaned: String = text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '-')
                .collect();
            cleaned
                .parse::<i64>()
                .map(|n| Value::Int(n, CompIntWidth::I32))
                .map_err(|_| MockEvalError::LiteralParse {
                    text: text.to_string(),
                    kind: "Int",
                })
        }
        HirLiteralKind::Float => {
            let text = span_text(source, span);
            let cleaned: String = text
                .chars()
                .filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E' | '+'))
                .collect();
            cleaned
                .parse::<f64>()
                .map(Value::Float)
                .map_err(|_| MockEvalError::LiteralParse {
                    text: text.to_string(),
                    kind: "Float",
                })
        }
        HirLiteralKind::Str => {
            let text = span_text(source, span);
            // Strip surrounding quotes if present.
            let stripped = text
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(text);
            Ok(Value::Str(stripped.to_string()))
        }
        HirLiteralKind::Char => {
            let text = span_text(source, span);
            let stripped = text
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .unwrap_or(text);
            // Encode the char value as an Int — minimal mock support.
            let c = stripped.chars().next().ok_or(MockEvalError::LiteralParse {
                text: text.to_string(),
                kind: "Char",
            })?;
            Ok(Value::Int(c as i64, CompIntWidth::I32))
        }
    }
}

fn span_text(source: &str, span: cssl_ast::Span) -> &str {
    let lo = span.start as usize;
    let hi = span.end as usize;
    if lo <= source.len() && hi <= source.len() && lo <= hi {
        &source[lo..hi]
    } else {
        ""
    }
}

fn eval_binary(op: HirBinOp, l: &Value, r: &Value) -> Result<Value, MockEvalError> {
    match (op, l, r) {
        (HirBinOp::Add, Value::Int(a, w), Value::Int(b, _)) => {
            Ok(Value::Int(a.wrapping_add(*b), *w))
        }
        (HirBinOp::Sub, Value::Int(a, w), Value::Int(b, _)) => {
            Ok(Value::Int(a.wrapping_sub(*b), *w))
        }
        (HirBinOp::Mul, Value::Int(a, w), Value::Int(b, _)) => {
            Ok(Value::Int(a.wrapping_mul(*b), *w))
        }
        (HirBinOp::Div, Value::Int(_, _), Value::Int(0, _)) => Err(MockEvalError::DivByZero),
        (HirBinOp::Div, Value::Int(a, w), Value::Int(b, _)) => {
            Ok(Value::Int(a.wrapping_div(*b), *w))
        }
        (HirBinOp::Rem, Value::Int(_, _), Value::Int(0, _)) => Err(MockEvalError::DivByZero),
        (HirBinOp::Rem, Value::Int(a, w), Value::Int(b, _)) => {
            Ok(Value::Int(a.wrapping_rem(*b), *w))
        }
        (HirBinOp::Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (HirBinOp::Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (HirBinOp::Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (HirBinOp::Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        (HirBinOp::Eq, _, _) => Ok(Value::Bool(l == r)),
        (HirBinOp::Ne, _, _) => Ok(Value::Bool(l != r)),
        (HirBinOp::Lt, Value::Int(a, _), Value::Int(b, _)) => Ok(Value::Bool(a < b)),
        (HirBinOp::Le, Value::Int(a, _), Value::Int(b, _)) => Ok(Value::Bool(a <= b)),
        (HirBinOp::Gt, Value::Int(a, _), Value::Int(b, _)) => Ok(Value::Bool(a > b)),
        (HirBinOp::Ge, Value::Int(a, _), Value::Int(b, _)) => Ok(Value::Bool(a >= b)),
        (HirBinOp::Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
        (HirBinOp::Le, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (HirBinOp::Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
        (HirBinOp::Ge, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
        (HirBinOp::And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
        (HirBinOp::Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
        _ => Err(MockEvalError::TypeMismatch {
            lhs_ty: short_type(l),
            rhs_ty: short_type(r),
        }),
    }
}

fn eval_unary(op: HirUnOp, v: &Value) -> Result<Value, MockEvalError> {
    match (op, v) {
        (HirUnOp::Neg, Value::Int(n, w)) => Ok(Value::Int(n.wrapping_neg(), *w)),
        (HirUnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (HirUnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        _ => Err(MockEvalError::Unsupported {
            what: format!("unary {op:?} on {}", short_type(v)),
        }),
    }
}

fn short_type(v: &Value) -> String {
    match v {
        Value::Int(_, w) => w.as_str().to_string(),
        Value::Float(_) => "f64".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Str(_) => "str".to_string(),
        Value::Sym(_) => "sym".to_string(),
        Value::Unit => "unit".to_string(),
        Value::Tuple(_) => "tuple".to_string(),
    }
}

fn hir_expr_kind_name(kind: &HirExprKind) -> &'static str {
    match kind {
        HirExprKind::Literal(_) => "Literal",
        HirExprKind::Path { .. } => "Path",
        HirExprKind::Call { .. } => "Call",
        HirExprKind::Field { .. } => "Field",
        HirExprKind::Index { .. } => "Index",
        HirExprKind::Binary { .. } => "Binary",
        HirExprKind::Unary { .. } => "Unary",
        HirExprKind::Block(_) => "Block",
        HirExprKind::If { .. } => "If",
        HirExprKind::Match { .. } => "Match",
        HirExprKind::For { .. } => "For",
        HirExprKind::While { .. } => "While",
        HirExprKind::Loop { .. } => "Loop",
        HirExprKind::Return { .. } => "Return",
        HirExprKind::Break { .. } => "Break",
        HirExprKind::Continue { .. } => "Continue",
        HirExprKind::Lambda { .. } => "Lambda",
        HirExprKind::Assign { .. } => "Assign",
        HirExprKind::Cast { .. } => "Cast",
        HirExprKind::Range { .. } => "Range",
        HirExprKind::Pipeline { .. } => "Pipeline",
        HirExprKind::TryDefault { .. } => "TryDefault",
        HirExprKind::Try { .. } => "Try",
        HirExprKind::Perform { .. } => "Perform",
        HirExprKind::With { .. } => "With",
        HirExprKind::Region { .. } => "Region",
        HirExprKind::Tuple(_) => "Tuple",
        HirExprKind::Array(_) => "Array",
        HirExprKind::Struct { .. } => "Struct",
        HirExprKind::Run { .. } => "Run",
        HirExprKind::Compound { .. } => "Compound",
        HirExprKind::SectionRef { .. } => "SectionRef",
        HirExprKind::Paren(_) => "Paren",
        HirExprKind::Error => "Error",
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate_comptime_block_mock, evaluate_comptime_expr_mock, MockEvalError};
    use crate::value::{CompIntWidth, Value};
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn parse_expr(src: &str) -> (cssl_hir::HirExpr, String) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, _interner, _bag2) = cssl_hir::lower_module(&f, &cst);
        // The first item should be a fn ; pull its body's trailing expr.
        for item in &hir.items {
            if let cssl_hir::HirItem::Fn(func) = item {
                if let Some(body) = &func.body {
                    if let Some(trailing) = &body.trailing {
                        return ((**trailing).clone(), src.to_string());
                    }
                }
            }
        }
        panic!("could not extract trailing expr from src : {src}");
    }

    #[test]
    fn literal_int_evaluates() {
        let (e, src) = parse_expr("fn t() -> i32 { 42 }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(42, CompIntWidth::I32));
    }

    #[test]
    fn literal_bool_evaluates() {
        let (e, src) = parse_expr("fn t() -> bool { true }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Bool(true));
        let (e, src) = parse_expr("fn t() -> bool { false }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Bool(false));
    }

    #[test]
    fn binary_addition_int() {
        let (e, src) = parse_expr("fn t() -> i32 { 3 + 4 }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(7, CompIntWidth::I32));
    }

    #[test]
    fn binary_subtraction_int() {
        let (e, src) = parse_expr("fn t() -> i32 { 10 - 3 }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(7, CompIntWidth::I32));
    }

    #[test]
    fn binary_multiplication_int() {
        let (e, src) = parse_expr("fn t() -> i32 { 6 * 7 }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(42, CompIntWidth::I32));
    }

    #[test]
    fn binary_division_by_zero_errors() {
        let (e, src) = parse_expr("fn t() -> i32 { 1 / 0 }");
        let err = evaluate_comptime_expr_mock(&e, &src).unwrap_err();
        assert!(matches!(err, MockEvalError::DivByZero));
    }

    #[test]
    fn binary_comparison_lt() {
        let (e, src) = parse_expr("fn t() -> bool { 3 < 4 }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn binary_logical_and() {
        let (e, src) = parse_expr("fn t() -> bool { true && false }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Bool(false));
    }

    #[test]
    fn binary_logical_or_short_circuit_does_not_apply_in_mock() {
        // The mock evaluates BOTH operands ; it does not short-circuit
        // (unlike a real interpreter). For pure literal expressions this
        // is observationally equivalent.
        let (e, src) = parse_expr("fn t() -> bool { true || false }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn unary_neg() {
        let (e, src) = parse_expr("fn t() -> i32 { -5 }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(-5, CompIntWidth::I32));
    }

    #[test]
    fn unary_not() {
        let (e, src) = parse_expr("fn t() -> bool { !true }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Bool(false));
    }

    #[test]
    fn paren_grouping() {
        let (e, src) = parse_expr("fn t() -> i32 { (3 + 4) * 2 }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(14, CompIntWidth::I32));
    }

    #[test]
    fn if_expr_true_branch() {
        let (e, src) = parse_expr("fn t() -> i32 { if true { 100 } else { 200 } }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(100, CompIntWidth::I32));
    }

    #[test]
    fn if_expr_false_branch() {
        let (e, src) = parse_expr("fn t() -> i32 { if false { 100 } else { 200 } }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(200, CompIntWidth::I32));
    }

    #[test]
    fn block_evaluates_to_trailing() {
        let (e, src) = parse_expr("fn t() -> i32 { { 7 } }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(7, CompIntWidth::I32));
    }

    #[test]
    fn empty_block_yields_unit() {
        // Build a synthetic empty block bypassing the parser : we don't
        // need a real source-file since the trailing-expression slot is
        // None ⇒ no span resolution happens.
        let block = cssl_hir::HirBlock {
            span: cssl_ast::Span::DUMMY,
            id: cssl_hir::HirId(0),
            stmts: Vec::new(),
            trailing: None,
        };
        let v = evaluate_comptime_block_mock(&block, "").unwrap();
        assert_eq!(v, Value::Unit);
    }

    #[test]
    fn run_marker_passes_through() {
        let (e, src) = parse_expr("fn t() -> i32 { #run 42 }");
        let v = evaluate_comptime_expr_mock(&e, &src).unwrap();
        assert_eq!(v, Value::Int(42, CompIntWidth::I32));
    }

    #[test]
    fn unsupported_form_returns_unsupported_error() {
        let (e, src) = parse_expr("fn t() -> i32 { let x = 1; x }");
        // The trailing expr is a Path lookup — mock returns Unbound.
        let err = evaluate_comptime_expr_mock(&e, &src).unwrap_err();
        assert!(matches!(err, MockEvalError::Unbound { .. }));
    }
}
