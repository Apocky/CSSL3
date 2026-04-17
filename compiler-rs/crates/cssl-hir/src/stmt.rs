//! HIR statements.
//!
//! Mirrors `cssl_ast::cst::Stmt` — `let` bindings, expression-statements, and nested items.

use cssl_ast::Span;

use crate::arena::HirId;
use crate::attr::HirAttr;
use crate::expr::HirExpr;
use crate::item::HirItem;
use crate::pat::HirPattern;
use crate::ty::HirType;

/// A HIR statement.
#[derive(Debug, Clone)]
pub struct HirStmt {
    pub span: Span,
    pub id: HirId,
    pub kind: HirStmtKind,
}

/// Shape of a HIR statement.
#[derive(Debug, Clone)]
pub enum HirStmtKind {
    /// `let pat : T = expr`.
    Let {
        attrs: Vec<HirAttr>,
        pat: HirPattern,
        ty: Option<HirType>,
        value: Option<HirExpr>,
    },
    /// `expr ;` — expression evaluated for side effects.
    Expr(HirExpr),
    /// Item declared inside a block (rare).
    Item(Box<HirItem>),
}

#[cfg(test)]
mod tests {
    use super::{HirStmt, HirStmtKind};
    use crate::arena::HirId;
    use crate::expr::{HirExpr, HirExprKind};
    use cssl_ast::{SourceId, Span};

    #[test]
    fn stmt_expr_variant() {
        let sp = Span::new(SourceId::first(), 0, 1);
        let e = HirExpr {
            span: sp,
            id: HirId::DUMMY,
            attrs: Vec::new(),
            kind: HirExprKind::Error,
        };
        let s = HirStmt {
            span: sp,
            id: HirId::DUMMY,
            kind: HirStmtKind::Expr(e),
        };
        assert!(matches!(s.kind, HirStmtKind::Expr(_)));
    }
}
