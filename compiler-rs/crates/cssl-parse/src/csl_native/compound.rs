//! CSLv3-native compound-formation helper.
//!
//! § SPEC : `CSLv3/specs/13_GRAMMAR_SELF.csl` § COMPOUND-FORMATION.
//!
//! § STAGE-0 SCOPE
//!   The full compound grammar (tatpuruṣa / dvandva / karmadhāraya / bahuvrīhi /
//!   avyayībhāva chains with morpheme-stacking) is elaborated in `cssl-hir`. This module
//!   exposes a thin helper that constructs a compound-expression node when given two
//!   sub-expressions and a compound operator kind. Full chain parsing is scheduled for
//!   T3.3+ when the elaborator needs it.

use cssl_ast::{CompoundOp as AstCompoundOp, Expr, ExprKind, Span};
use cssl_lex::CompoundOp as LexCompoundOp;

/// Combine two expressions under a compound-formation operator.
///
/// The operator is passed in from the lexer-level enum and translated into the CST-level
/// `CompoundOp` by [`translate_compound_op`].
#[must_use]
pub fn make_compound(op: LexCompoundOp, lhs: Expr, rhs: Expr) -> Expr {
    let ast_op = translate_compound_op(op);
    let span = Span::new(lhs.span.source, lhs.span.start, rhs.span.end);
    Expr {
        span,
        attrs: Vec::new(),
        kind: ExprKind::Compound {
            op: ast_op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
    }
}

/// Translate the lexer-level `CompoundOp` into the CST-level `CompoundOp`.
#[must_use]
pub const fn translate_compound_op(op: LexCompoundOp) -> AstCompoundOp {
    match op {
        LexCompoundOp::Tp => AstCompoundOp::Tp,
        LexCompoundOp::Dv => AstCompoundOp::Dv,
        LexCompoundOp::Kd => AstCompoundOp::Kd,
        LexCompoundOp::Bv => AstCompoundOp::Bv,
        LexCompoundOp::Av => AstCompoundOp::Av,
    }
}

#[cfg(test)]
mod tests {
    use super::{make_compound, translate_compound_op};
    use cssl_ast::{
        CompoundOp as AstCompoundOp, Expr, ExprKind, Literal, LiteralKind, SourceId, Span,
    };
    use cssl_lex::CompoundOp as LexCompoundOp;

    fn lit(start: u32, end: u32) -> Expr {
        let span = Span::new(SourceId::first(), start, end);
        Expr {
            span,
            attrs: Vec::new(),
            kind: ExprKind::Literal(Literal {
                span,
                kind: LiteralKind::Int,
            }),
        }
    }

    #[test]
    fn translate_all_variants() {
        assert_eq!(translate_compound_op(LexCompoundOp::Tp), AstCompoundOp::Tp);
        assert_eq!(translate_compound_op(LexCompoundOp::Dv), AstCompoundOp::Dv);
        assert_eq!(translate_compound_op(LexCompoundOp::Kd), AstCompoundOp::Kd);
        assert_eq!(translate_compound_op(LexCompoundOp::Bv), AstCompoundOp::Bv);
        assert_eq!(translate_compound_op(LexCompoundOp::Av), AstCompoundOp::Av);
    }

    #[test]
    fn make_compound_joins_spans() {
        let l = lit(0, 3);
        let r = lit(6, 9);
        let c = make_compound(LexCompoundOp::Tp, l, r);
        assert_eq!(c.span.start, 0);
        assert_eq!(c.span.end, 9);
        assert!(matches!(
            c.kind,
            ExprKind::Compound {
                op: AstCompoundOp::Tp,
                ..
            }
        ));
    }
}
